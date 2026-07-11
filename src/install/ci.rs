use crate::InstallConfig;
use crate::ci::CiRun;
use crate::download::get_bytes;
use crate::install::artifact::install_from_download_file;
use crate::tool::{filter_artifacts, not_found_asset_message};
use crate::types::Output;
use anyhow::{Context, Result};
use easy_archive::Fmt;
use tokio::task::JoinSet;
use tracing::trace;

/// Limits concurrent network downloads to avoid hammering GitHub.
static DOWNLOAD_SEM: std::sync::LazyLock<tokio::sync::Semaphore> =
    std::sync::LazyLock::new(|| tokio::sync::Semaphore::new(4));

pub(crate) async fn install_from_ci(ci: &CiRun, config: &InstallConfig) -> Result<Output> {
    trace!("install_from_ci {}", ci);

    let artifact_url = ci.get_artifact_url(config).await?;
    if artifact_url.is_empty() {
        not_found_asset_message(&ci.to_string(), config);
        return Ok(Output::new());
    }

    let artifact_url = filter_artifacts(artifact_url, config);

    // CI artifacts are always ZIP archives served by the GitHub API.
    // The download URL is an API endpoint (…/artifacts/{id}/zip) that
    // `Fmt::guess` may not recognise, so we force `Fmt::Zip` explicitly.
    if artifact_url.len() <= 1 {
        let mut v = Output::new();
        for (name, url) in artifact_url {
            v.extend(install_ci_artifact(&url, &name, config).await?);
        }
        return Ok(v);
    }

    let mut tasks: JoinSet<Result<Output>> = JoinSet::new();
    for (name, url) in artifact_url {
        let config = config.clone();
        tasks.spawn(async move {
            let _permit = DOWNLOAD_SEM.acquire().await.expect("semaphore closed");
            install_ci_artifact(&url, &name, &config).await
        });
    }

    let mut v = Output::new();
    while let Some(res) = tasks.join_next().await {
        v.extend(res??);
    }
    Ok(v)
}

async fn install_ci_artifact(url: &str, name: &str, config: &InstallConfig) -> Result<Output> {
    if !config.quiet {
        println!("download {url}");
    }
    let bytes = get_bytes(url, config.retry, config.timeout)
        .await
        .context("Failed to download CI artifact")?;
    install_from_download_file(bytes, Fmt::Zip, url, name, config)
}
