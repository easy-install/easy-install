//! GitHub Actions artifact sources.
//!
//! Supports both a direct run URL (`…/actions/runs/<id>`) and a workflow URL
//! (`…/actions/workflows/<file>.yml`), which resolves to the latest successful
//! run. The artifacts themselves are ZIP archives served through the API.

use crate::InstallConfig;
use crate::artifact::{Artifact, Kind};
use crate::download::download_json;
use crate::error::{Context, Result, err};
use crate::source::{Input, Source, select_assets};
use async_trait::async_trait;
use github_proxy::artifacts::{GhArtifact, GhArtifacts};
use regex::Regex;
use serde::Deserialize;
use std::collections::HashSet;
use std::fmt::Display;
use std::sync::LazyLock;

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
    #[allow(dead_code)]
    status: String,
    #[allow(dead_code)]
    conclusion: Option<String>,
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
            Err(_) => resolve_workflow(input.raw, config.retry, config.timeout).await?,
        };

        let artifacts = ci.get_artifacts(config.retry, config.timeout).await?;
        let mut selected = select_assets(artifacts, config, &ci.to_string())?;
        // CI artifacts are always ZIP archives served by the GitHub API.
        for artifact in &mut selected {
            artifact.kind = Kind::Zip;
        }
        Ok(selected)
    }
}

impl CiRun {
    /// Resolve a run ID from either a run number or a workflow file name.
    async fn resolve_run(
        owner: &str,
        repo: &str,
        run: &str,
        retry: usize,
        timeout: u64,
    ) -> Result<u64> {
        if let Ok(id) = run.parse::<u64>() {
            return Ok(id);
        }

        let url = format!(
            "https://api.github.com/repos/{owner}/{repo}/actions/workflows/{run}/runs?per_page=1&status=completed&conclusion=success"
        );
        let runs: WorkflowRuns = download_json(&url, retry, timeout)
            .await
            .context("Failed to fetch workflow runs. GitHub Actions API requires authentication — set GITHUB_TOKEN or run `gh auth login`.")?;

        let run = runs
            .workflow_runs
            .into_iter()
            .next()
            .context("No completed workflow runs found")?;
        Ok(run.id)
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

/// Parse a CI workflow URL and resolve it to the latest completed run.
async fn resolve_workflow(url: &str, retry: usize, timeout: u64) -> Result<CiRun> {
    let captures = RE_CI_WORKFLOW
        .captures(url)
        .context("Not a valid CI workflow URL")?;

    let owner = captures["owner"].to_string();
    let repo = captures["repo"].to_string();
    let workflow = captures["workflow"].to_string();
    let run_id = CiRun::resolve_run(&owner, &repo, &workflow, retry, timeout).await?;

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
}
