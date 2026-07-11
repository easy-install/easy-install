use crate::InstallConfig;
use crate::install::install_artifacts;
use crate::install::manfiest::install_from_manfiest;
use crate::tool::{filter_artifacts, get_artifact_url, not_found_asset_message};
use crate::types::{Output, Repo};
use anyhow::Result;
use tracing::trace;

pub(crate) async fn install_from_github(repo: &Repo, config: &InstallConfig) -> Result<Output> {
    trace!("install_from_git {}", repo);
    if let Ok(man) = repo
        .get_manfiest(config.retry, config.proxy, config.timeout)
        .await
    {
        return install_from_manfiest(
            man,
            &repo
                .get_manfiest_url(config.proxy, config.retry, config.timeout)
                .await?,
            config,
        )
        .await;
    }

    let artifacts = repo.get_raw_artifacts(config.retry, config.timeout).await?;
    let available: Vec<String> = artifacts.assets.iter().map(|a| a.name.clone()).collect();
    let artifact_url = get_artifact_url(artifacts, config)?;
    if artifact_url.is_empty() {
        not_found_asset_message(&repo.get_gh_url(), config, Some(&available));
        return Ok(Output::new());
    }

    let artifact_url = filter_artifacts(artifact_url, config);
    install_artifacts(artifact_url, config).await
}
