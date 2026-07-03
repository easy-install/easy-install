use crate::InstallConfig;
use crate::install::install_artifacts;
use crate::tool::{filter_artifacts, not_found_asset_message};
use crate::types::{Nightly, Output};
use anyhow::Result;
use tracing::trace;

pub(crate) async fn install_from_nightly(repo: &Nightly, config: &InstallConfig) -> Result<Output> {
    trace!("install_from_nightly {}", repo);

    let artifact_url = repo.get_artifact_url(config).await?;
    if artifact_url.is_empty() {
        not_found_asset_message(&repo.url, config);
        return Ok(Output::new());
    }

    let artifact_url = filter_artifacts(artifact_url, config);
    install_artifacts(artifact_url, config).await
}
