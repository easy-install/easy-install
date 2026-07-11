mod artifact;
mod ci;
mod file;
mod manfiest;
mod nightly;
mod repo;

use crate::InstallConfig;
use crate::ci::{CiRun, RE_CI_WORKFLOW, resolve_ci_workflow};
use crate::download::{download_dist_manfiest, get_bytes, read_dist_manfiest};
use crate::install::artifact::install_from_artifact_url;
use crate::install::ci::install_from_ci;
use crate::install::file::install_from_single_file;
use crate::install::manfiest::install_from_manfiest;
use crate::install::nightly::install_from_nightly;
use crate::install::repo::install_from_github;
use crate::tool::{
    get_filename, is_archive_file, is_dist_manfiest, is_exe_file, is_known_format, is_url,
    name_no_ext,
};
use crate::types::{Nightly, Output, Repo};
use anyhow::Result;
use artifact::install_from_download_file;
use easy_archive::Fmt;
use guess_target::guess_target;
use std::sync::LazyLock;
use tokio::task::JoinSet;
use tracing::trace;

/// Limits concurrent network downloads to avoid hammering GitHub and
/// tripping rate limits.
static DOWNLOAD_SEM: LazyLock<tokio::sync::Semaphore> =
    LazyLock::new(|| tokio::sync::Semaphore::new(4));

pub(crate) async fn install(url: &str, config: &InstallConfig) -> Result<Output> {
    trace!("install {}", url);
    let repo = Repo::try_from(url);

    if is_dist_manfiest(url) {
        let manifest = if is_url(url) {
            download_dist_manfiest(url, config.retry, config.timeout).await
        } else {
            read_dist_manfiest(url)
        };
        match manifest {
            Ok(m) if !m.artifacts.is_empty() => {
                return install_from_manfiest(m, url, config).await;
            }
            Ok(_) => {
                if !config.quiet {
                    println!("dist-manifest at {url} contains no artifacts");
                }
                return Ok(Output::new());
            }
            Err(e) => {
                if !config.quiet {
                    println!("failed to read dist-manifest from {url}: {e}");
                }
                return Ok(Output::new());
            }
        }
    }

    let filename = get_filename(url);
    let name = name_no_ext(&filename);
    let guess = guess_target(&name);
    let local = config.get_local_target();
    let item = guess.iter().find(|i| local.contains(&i.target));
    let name = item.map_or(name, |i| i.name.clone());

    // GitHub Actions CI — must be checked first because CI workflow URLs
    // end with .yml which would otherwise be treated as a known format
    // and downloaded directly. CI run URLs also match Repo patterns, so
    // they must be intercepted here before falling through.
    //
    // Workflow URLs (…/actions/workflows/*.yml) require an async API call
    // to resolve the latest run ID. The API needs authentication — set
    // GITHUB_TOKEN or run `gh auth login` if you see an auth error.
    if CiRun::try_from(url).is_ok() || RE_CI_WORKFLOW.is_match(url) {
        let ci = if let Ok(ci) = CiRun::try_from(url) {
            ci
        } else {
            resolve_ci_workflow(url, config.retry, config.timeout).await?
        };
        return install_from_ci(&ci, config).await;
    }

    if is_url(url) {
        let proxied = apply_proxy(url, config.proxy);

        if is_archive_file(&proxied) {
            return install_from_artifact_url(&proxied, &name, config).await;
        }

        if is_exe_file(&proxied).unwrap_or(false) || is_known_format(&proxied) {
            return install_from_single_file(&proxied, &filename, config).await;
        }
    }

    if std::fs::exists(url).unwrap_or(false) {
        if is_archive_file(url) {
            if let Ok(bytes) = get_bytes(url, config.retry, config.timeout).await
                && let Some(fmt) = Fmt::guess(url)
            {
                return install_from_download_file(bytes, fmt, url, &name, config);
            }
        } else {
            return install_from_single_file(url, &name, config).await;
        }
    }

    if let Ok(nightly) = Nightly::try_from(url) {
        return install_from_nightly(&nightly, config).await;
    }

    if let Ok(repo) = repo {
        return install_from_github(&repo, config).await;
    }

    install_from_single_file(url, &name, config).await
}

/// Apply the configured GitHub proxy to a URL if it is a GitHub resource.
fn apply_proxy(url: &str, proxy: github_proxy::Proxy) -> String {
    match github_proxy::Resource::try_from(url) {
        Ok(r) => r.url(&proxy).unwrap_or_else(|| url.to_string()),
        Err(_) => url.to_string(),
    }
}

/// Install a list of (name, url) artifacts, downloading concurrently when
/// there is more than one. Results are merged into a single `Output`.
pub(crate) async fn install_artifacts(
    artifact_url: Vec<(String, String)>,
    config: &InstallConfig,
) -> Result<Output> {
    // Fast path: zero or one artifact — no need to spawn tasks.
    if artifact_url.len() <= 1 {
        let mut v = Output::new();
        for (name, url) in artifact_url {
            v.extend(install_from_artifact_url(&url, &name, config).await?);
        }
        return Ok(v);
    }

    let mut tasks: JoinSet<Result<Output>> = JoinSet::new();
    for (name, url) in artifact_url {
        let config = config.clone();
        tasks.spawn(async move {
            let _permit = DOWNLOAD_SEM.acquire().await.expect("semaphore closed");
            install_from_artifact_url(&url, &name, &config).await
        });
    }

    let mut v = Output::new();
    while let Some(res) = tasks.join_next().await {
        v.extend(res??);
    }
    Ok(v)
}
