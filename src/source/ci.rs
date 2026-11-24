//! GitHub Actions artifact sources.
//!
//! Supports both a direct run URL (`…/actions/runs/<id>`) and a workflow URL
//! (`…/actions/workflows/<file>.yml`), which resolves to the latest *successful*
//! run. When a newer run exists but is unfinished or did not succeed, the
//! user is told why that run is not used, instead of silently installing
//! nothing (or an unexpected older build). The artifacts themselves are ZIP
//! archives served through the API.

use crate::InstallConfig;
use crate::artifact::{Artifact, Kind};
use crate::download::download_json;
use crate::error::{Context, Result, bail, err};
use crate::source::{Input, Source, select_assets};
use async_trait::async_trait;
use github_proxy::artifacts::{GhArtifact, GhArtifacts};
use regex::Regex;
use serde::Deserialize;
use std::collections::HashSet;
use std::fmt::Display;
use std::sync::LazyLock;

/// How many recent runs are inspected when picking the latest successful one.
///
/// GitHub's `conclusion` query parameter does not filter reliably (a run that
/// ended with `action_required` is returned for `conclusion=success`), so the
/// selection happens here.
const MAX_RUN_SCAN: usize = 10;

static RE_CI_RUN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^https?://github\.com/(?P<owner>[^/]+)/(?P<repo>[^/]+)/actions/runs/(?P<run_id>\d+)$",
    )
    .unwrap()
});

static RE_CI_WORKFLOW: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^https?://github\.com/(?P<owner>[^/]+)/(?P<repo>[^/]+)/actions/workflows/(?P<workflow>.+)$",
    )
    .unwrap()
});

#[derive(Deserialize)]
struct ActionsArtifacts {
    artifacts: Vec<ActionsArtifact>,
}

#[derive(Deserialize)]
struct ActionsArtifact {
    #[allow(dead_code)]
    id: u64,
    name: String,
    archive_download_url: String,
    expired: bool,
}

#[derive(Deserialize)]
struct WorkflowRuns {
    workflow_runs: Vec<WorkflowRun>,
}

#[derive(Deserialize)]
struct WorkflowRun {
    id: u64,
    /// `queued`, `in_progress`, `completed`, …
    status: String,
    /// Only meaningful once `status` is `completed` (`success`, `failure`,
    /// `action_required`, `cancelled`, `timed_out`, …).
    conclusion: Option<String>,
}

impl WorkflowRun {
    /// Whether this run produced installable artifacts.
    fn is_success(&self) -> bool {
        self.status == "completed" && self.conclusion.as_deref() == Some("success")
    }

    /// State in plain words, e.g. `still running`, `waiting for approval`.
    fn state(&self) -> String {
        if self.status != "completed" {
            return match self.status.as_str() {
                "in_progress" => "still running".to_string(),
                "queued" => "queued, waiting for a runner".to_string(),
                "requested" | "waiting" | "pending" => "not started yet".to_string(),
                other => other.to_string(),
            };
        }
        match self.conclusion.as_deref() {
            Some("action_required") => "waiting for approval".to_string(),
            Some("failure") => "failed".to_string(),
            Some("cancelled") => "cancelled".to_string(),
            Some("timed_out") => "timed out".to_string(),
            Some("skipped") => "skipped".to_string(),
            Some(other) => other.replace('_', " "),
            None => "finished without a conclusion".to_string(),
        }
    }

    /// Browser URL of the run, for the "look at it yourself" hint.
    fn url(&self, owner: &str, repo: &str) -> String {
        format!("https://github.com/{owner}/{repo}/actions/runs/{}", self.id)
    }
}

/// Outcome of picking a run out of the recent ones.
#[derive(Debug, PartialEq, Eq)]
enum Pick {
    /// The newest run is usable.
    Latest,
    /// A newer run is not usable; the run at this index is.
    Fallback(usize),
    /// Nothing is usable yet.
    None,
}

/// Choose which recent run to install from.
///
/// `runs` is newest first, as the API returns it. The newest successful run
/// wins; the index of the unusable newer run is reported when one exists, so
/// the caller can explain the choice.
fn pick(runs: &[WorkflowRun]) -> Pick {
    if runs.first().is_none_or(WorkflowRun::is_success) {
        return Pick::Latest;
    }
    match runs.iter().position(WorkflowRun::is_success) {
        Some(index) => Pick::Fallback(index),
        None => Pick::None,
    }
}

/// A resolved GitHub Actions run.
#[derive(Debug, Clone)]
pub(crate) struct CiRun {
    owner: String,
    repo: String,
    run_id: u64,
}

/// Whether `url` references a CI run or a CI workflow.
pub(crate) fn is_ci_url(url: &str) -> bool {
    url.parse::<CiRun>().is_ok() || RE_CI_WORKFLOW.is_match(url)
}

/// A GitHub Actions run or workflow URL.
pub(crate) struct Ci;

#[async_trait]
impl Source for Ci {
    fn name(&self) -> &'static str {
        "github-actions"
    }

    fn matches(&self, input: &Input<'_>) -> bool {
        is_ci_url(input.raw)
    }

    async fn resolve(&self, input: &Input<'_>, config: &InstallConfig) -> Result<Vec<Artifact>> {
        let ci = match input.raw.parse::<CiRun>() {
            Ok(ci) => ci,
            Err(_) => resolve_workflow(input.raw, config).await?,
        };

        let artifacts = ci.get_artifacts(config.retry, config.timeout).await?;
        // A run that is still going (or was blocked, cancelled, …) has no
        // artifacts at all; without this, that looks like a missing platform
        // asset.
        if artifacts.assets.is_empty() {
            ci.explain_no_artifacts(config).await?;
        }
        let mut selected = select_assets(artifacts, config, &ci.to_string())?;
        // CI artifacts are always ZIP archives served by the GitHub API.
        for artifact in &mut selected {
            artifact.kind = Kind::Zip;
        }
        Ok(selected)
    }
}

impl CiRun {
    /// Resolve the run to install from.
    ///
    /// A numeric run ID is used as-is. For a workflow URL, the newest
    /// successful run is chosen; when a newer run exists but is unfinished or
    /// did not succeed, its state is reported so the user is not left guessing
    /// why an older build was installed.
    async fn resolve_run(
        owner: &str,
        repo: &str,
        workflow: &str,
        config: &InstallConfig,
    ) -> Result<u64> {
        if let Ok(id) = workflow.parse::<u64>() {
            return Ok(id);
        }

        // No `status`/`conclusion` filter: GitHub's `conclusion` parameter does
        // not filter, and the real state is needed for the message below.
        let url = format!(
            "https://api.github.com/repos/{owner}/{repo}/actions/workflows/{workflow}/runs?per_page={MAX_RUN_SCAN}"
        );
        let runs: WorkflowRuns = download_json(&url, config.retry, config.timeout)
            .await
            .context("Failed to fetch workflow runs. GitHub Actions API requires authentication — set GITHUB_TOKEN or run `gh auth login`.")?;

        let Some(newest) = runs.workflow_runs.first() else {
            bail!("No workflow runs found for '{workflow}' in {owner}/{repo}.");
        };

        match pick(&runs.workflow_runs) {
            Pick::Latest => Ok(newest.id),
            Pick::Fallback(index) => {
                let run = &runs.workflow_runs[index];
                if !config.quiet {
                    println!(
                        "The latest run of {workflow} is {}, so the latest successful run {} is used instead.",
                        newest.state(),
                        run.id
                    );
                    println!("  {}", newest.url(owner, repo));
                }
                Ok(run.id)
            }
            Pick::None => bail!(
                "{owner}/{repo}'s {workflow} has no successful run among the {count} most recent ones: the latest is {state}.\n  {url}\nWait for a run to finish, or pass a run URL to install from a specific run.",
                count = runs.workflow_runs.len(),
                state = newest.state(),
                url = newest.url(owner, repo)
            ),
        }
    }

    async fn get_artifacts(&self, retry: usize, timeout: u64) -> Result<GhArtifacts> {
        let url = format!(
            "https://api.github.com/repos/{}/{}/actions/runs/{}/artifacts",
            self.owner, self.repo, self.run_id
        );

        let response: ActionsArtifacts = download_json(&url, retry, timeout)
            .await
            .context("Failed to fetch CI artifacts. The GitHub Actions API requires authentication — set GITHUB_TOKEN or run `gh auth login`.")?;

        let mut assets = HashSet::new();
        for artifact in response.artifacts {
            if artifact.expired {
                continue;
            }
            assets.insert(GhArtifact {
                name: format!("{}.zip", artifact.name),
                browser_download_url: artifact.archive_download_url,
                size: 0,
                url: None,
            });
        }
        Ok(GhArtifacts { assets })
    }

    /// Explain a run that has no artifacts at all.
    ///
    /// Without this, an unfinished run looks exactly like "this platform is not
    /// built". The run's own state is fetched so the message can say which of
    /// the two it is.
    async fn explain_no_artifacts(&self, config: &InstallConfig) -> Result<()> {
        let url = format!(
            "https://api.github.com/repos/{}/{}/actions/runs/{}",
            self.owner, self.repo, self.run_id
        );
        let run: WorkflowRun = download_json(&url, config.retry, config.timeout)
            .await
            .context("Failed to fetch the CI run")?;
        let page = run.url(&self.owner, &self.repo);
        let state = run.state();

        if run.is_success() {
            bail!(
                "Run {} succeeded but has no artifacts left (they may have expired).\n  {page}",
                run.id
            );
        }
        if run.status != "completed" {
            bail!(
                "The artifacts of run {} are not available yet: it is {state}.\n  {page}\nRetry once the run finishes.",
                run.id
            );
        }
        bail!("Run {} produced no artifacts ({state}).\n  {page}", run.id);
    }
}

impl Display for CiRun {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "https://github.com/{}/{}/actions/runs/{}",
            self.owner, self.repo, self.run_id
        )
    }
}

impl std::str::FromStr for CiRun {
    type Err = crate::error::Error;

    fn from_str(url: &str) -> std::result::Result<Self, Self::Err> {
        if let Some(captures) = RE_CI_RUN.captures(url) {
            return Ok(Self {
                owner: captures["owner"].to_string(),
                repo: captures["repo"].to_string(),
                run_id: captures["run_id"].parse()?,
            });
        }

        // Workflow URLs need an async API call to find the run ID; the caller
        // uses `resolve_workflow` for those.
        Err(err!("Invalid CI URL: {url}"))
    }
}

/// Parse a CI workflow URL and resolve it to the latest successful run.
async fn resolve_workflow(url: &str, config: &InstallConfig) -> Result<CiRun> {
    let captures = RE_CI_WORKFLOW
        .captures(url)
        .context("Not a valid CI workflow URL")?;

    let owner = captures["owner"].to_string();
    let repo = captures["repo"].to_string();
    let workflow = captures["workflow"].to_string();
    let run_id = CiRun::resolve_run(&owner, &repo, &workflow, config).await?;

    Ok(CiRun {
        owner,
        repo,
        run_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ci_run_url() {
        let url = "https://github.com/theMackabu/ant/actions/runs/28723919642";
        let ci = url.parse::<CiRun>().unwrap();
        assert_eq!(ci.owner, "theMackabu");
        assert_eq!(ci.repo, "ant");
        assert_eq!(ci.run_id, 28723919642);
    }

    #[test]
    fn test_ci_workflow_url() {
        let url = "https://github.com/lucid-softworks/lumen/actions/workflows/release.yml";
        let captures = RE_CI_WORKFLOW.captures(url).unwrap();
        assert_eq!(&captures["owner"], "lucid-softworks");
        assert_eq!(&captures["repo"], "lumen");
        assert_eq!(&captures["workflow"], "release.yml");
        assert!(is_ci_url(url));
    }

    #[test]
    fn test_invalid_url() {
        assert!(
            "https://github.com/owner/repo/releases/tag/v1.0"
                .parse::<CiRun>()
                .is_err()
        );
        assert!(!is_ci_url("https://github.com/owner/repo"));
    }

    fn run(id: u64, status: &str, conclusion: Option<&str>) -> WorkflowRun {
        WorkflowRun {
            id,
            status: status.to_string(),
            conclusion: conclusion.map(str::to_string),
        }
    }

    /// The newest run wins when it succeeded; otherwise the newest successful
    /// one is used, and the caller learns which newer run was skipped.
    #[test]
    fn picks_the_latest_successful_run() {
        let success = |id| run(id, "completed", Some("success"));

        assert_eq!(pick(&[success(3), success(2)]), Pick::Latest);

        // Newest run still running: fall back to the previous success.
        assert_eq!(
            pick(&[run(4, "in_progress", None), success(3), success(2)]),
            Pick::Fallback(1)
        );
        // Newest run blocked on approval or failed: same.
        assert_eq!(
            pick(&[
                run(5, "completed", Some("action_required")),
                run(4, "completed", Some("failure")),
                success(3),
            ]),
            Pick::Fallback(2)
        );
        // Nothing usable yet: the caller reports the state instead of
        // pretending there is no asset.
        assert_eq!(
            pick(&[run(6, "queued", None), run(5, "completed", Some("failure"))]),
            Pick::None
        );
        assert_eq!(pick(&[]), Pick::Latest);
    }

    /// Run states are reported in plain words, since the raw values are what
    /// confused the user (`action_required` looks like a missing asset).
    #[test]
    fn describes_run_state() {
        for (status, conclusion, expected) in [
            ("in_progress", None, "still running"),
            ("queued", None, "queued, waiting for a runner"),
            ("completed", Some("action_required"), "waiting for approval"),
            ("completed", Some("failure"), "failed"),
            ("completed", Some("cancelled"), "cancelled"),
            ("completed", Some("timed_out"), "timed out"),
            ("completed", Some("success"), "success"),
            ("completed", None, "finished without a conclusion"),
        ] {
            let run = run(1, status, conclusion);
            assert_eq!(run.state(), expected, "{status}/{conclusion:?}");
        }

        assert!(run(1, "completed", Some("success")).is_success());
        assert!(!run(1, "completed", Some("action_required")).is_success());
        assert!(!run(1, "in_progress", None).is_success());
    }
}
