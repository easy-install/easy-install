use crate::InstallConfig;
use crate::install::manfiest::install_from_manfiest;
use crate::install::install_artifacts;
use crate::tool::{filter_artifacts, not_found_asset_message};
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

    let artifact_url = repo.get_artifact_url(config).await?;
    if artifact_url.is_empty() {
        not_found_asset_message(&repo.get_gh_url(), config);
        return Ok(Output::new());
    }

    let artifact_url = filter_artifacts(artifact_url, config);
    install_artifacts(artifact_url, config).await
}
