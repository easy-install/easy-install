use crate::InstallConfig;
use crate::install::install_artifacts;
use crate::tool::{filter_artifacts, get_artifact_url, not_found_asset_message};
use crate::types::{Nightly, Output};
use anyhow::Result;
use tracing::trace;

pub(crate) async fn install_from_nightly(
    nightly: &Nightly,
    config: &InstallConfig,
) -> Result<Output> {
    trace!("install_from_nightly {}", nightly);

    let artifacts = nightly
        .get_raw_artifacts(config.retry, config.timeout)
        .await?;
    let available: Vec<String> = artifacts.assets.iter().map(|a| a.name.clone()).collect();
    let artifact_url = get_artifact_url(artifacts, config)?;
    if artifact_url.is_empty() {
        not_found_asset_message(&nightly.url, config, Some(&available));
        return Ok(Output::new());
    }

    let artifact_url = filter_artifacts(artifact_url, config);
    install_artifacts(artifact_url, config).await
}
