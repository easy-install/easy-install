use crate::InstallConfig;
use crate::artifact::{GhArtifact, GhArtifacts};
use crate::download::download_json;
use crate::tool::get_artifact_url;
use anyhow::{Context, Result};
use regex::Regex;
use serde::Deserialize;
use std::collections::HashSet;
use std::fmt::Display;
use std::sync::LazyLock;

// URL patterns:
//   https://github.com/{owner}/{repo}/actions/runs/{run_id}
//   https://github.com/{owner}/{repo}/actions/workflows/{workflow_file}
static RE_CI_RUN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^https?://github\.com/(?P<owner>[^/]+)/(?P<repo>[^/]+)/actions/runs/(?P<run_id>\d+)$",
    )
    .unwrap()
});

pub(crate) static RE_CI_WORKFLOW: LazyLock<Regex> = LazyLock::new(|| {
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

#[derive(Debug, Clone)]
pub(crate) struct CiRun {
    pub(crate) owner: String,
    pub(crate) repo: String,
    pub(crate) run_id: u64,
}

impl CiRun {
    /// Resolve the run ID from either a direct run URL or a workflow file
    /// URL (fetches the latest completed run).
    async fn resolve(
        owner: &str,
        repo: &str,
        run_str: &str,
        retry: usize,
        timeout: u64,
    ) -> Result<u64> {
        // Direct run ID
        if let Ok(id) = run_str.parse::<u64>() {
            return Ok(id);
        }

        // Workflow file — find the latest successful run
        let url = format!(
            "https://api.github.com/repos/{owner}/{repo}/actions/workflows/{run_str}/runs?per_page=1&status=completed&conclusion=success"
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

    #[allow(dead_code)]
    pub(crate) async fn get_artifact_url(
        &self,
        config: &InstallConfig,
    ) -> Result<Vec<(String, String)>> {
        let artifacts = self.get_artifacts(config.retry, config.timeout).await?;
        get_artifact_url(artifacts, config)
    }

    pub(crate) async fn get_artifacts(&self, retry: usize, timeout: u64) -> Result<GhArtifacts> {
        let url = format!(
            "https://api.github.com/repos/{}/{}/actions/runs/{}/artifacts",
            self.owner, self.repo, self.run_id
        );

        let response: ActionsArtifacts = download_json(&url, retry, timeout)
            .await
            .context("Failed to fetch CI artifacts. The GitHub Actions API requires authentication — set GITHUB_TOKEN or run `gh auth login`.")?;

        let mut assets = HashSet::new();
        for a in response.artifacts {
            if a.expired {
                continue;
            }
            assets.insert(GhArtifact {
                name: format!("{}.zip", a.name),
                browser_download_url: a.archive_download_url,
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

impl TryFrom<&str> for CiRun {
    type Error = anyhow::Error;

    fn try_from(url: &str) -> Result<Self> {
        if let Some(cap) = RE_CI_RUN.captures(url) {
            return Ok(Self {
                owner: cap["owner"].to_string(),
                repo: cap["repo"].to_string(),
                run_id: cap["run_id"].parse()?,
            });
        }

        // Workflow URL — we can't resolve the run_id synchronously, so we
        // store the workflow_file as a special marker. The caller must call
        // `resolve_workflow` before using.
        if let Some(_cap) = RE_CI_WORKFLOW.captures(url) {
            return Err(anyhow::anyhow!(
                "Workflow URLs require async resolution. Use the workflow file as a CI reference."
            ));
        }

        Err(anyhow::anyhow!("Invalid CI URL: {url}"))
    }
}

/// Parse a CI workflow URL and resolve to the latest completed run.
pub(crate) async fn resolve_ci_workflow(url: &str, retry: usize, timeout: u64) -> Result<CiRun> {
    let cap = RE_CI_WORKFLOW
        .captures(url)
        .context("Not a valid CI workflow URL")?;

    let owner = cap["owner"].to_string();
    let repo = cap["repo"].to_string();
    let workflow = cap["workflow"].to_string();

    let run_id = CiRun::resolve(&owner, &repo, &workflow, retry, timeout).await?;

    Ok(CiRun {
        owner,
        repo,
        run_id,
    })
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_ci_run_url() {
        let url = "https://github.com/theMackabu/ant/actions/runs/28723919642";
        let ci = CiRun::try_from(url).unwrap();
        assert_eq!(ci.owner, "theMackabu");
        assert_eq!(ci.repo, "ant");
        assert_eq!(ci.run_id, 28723919642);
    }

    #[test]
    fn test_ci_workflow_url() {
        let url = "https://github.com/lucid-softworks/lumen/actions/workflows/release.yml";
        let cap = RE_CI_WORKFLOW.captures(url).unwrap();
        assert_eq!(&cap["owner"], "lucid-softworks");
        assert_eq!(&cap["repo"], "lumen");
        assert_eq!(&cap["workflow"], "release.yml");
    }

    #[test]
    fn test_invalid_url() {
        assert!(CiRun::try_from("https://github.com/owner/repo/releases/tag/v1.0").is_err());
    }
}
