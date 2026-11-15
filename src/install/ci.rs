use crate::InstallConfig;
use crate::backend::Backend;
use crate::ci::CiRun;
use crate::error::Result;
use crate::install::{Kind, install_artifacts};
use crate::tool::{filter_artifacts, get_artifact_url, not_found_asset_message};
use crate::types::Output;
use tracing::trace;

pub(crate) async fn install_from_ci(
    backend: &Backend,
    ci: &CiRun,
    config: &InstallConfig,
) -> Result<Output> {
    trace!("install_from_ci {}", ci);

    let artifacts = ci.get_artifacts(config.retry, config.timeout).await?;

    // Collect available names before `get_artifact_url` consumes the set.
    let available: Vec<String> = artifacts.assets.iter().map(|a| a.name.clone()).collect();

    let artifact_url = get_artifact_url(artifacts, config)?;
    if artifact_url.is_empty() {
        not_found_asset_message(&ci.to_string(), config, Some(&available));
        return Ok(Output::new());
    }

    let artifact_url = filter_artifacts(artifact_url, config);

    // CI artifacts are always ZIP archives served by the GitHub API.
    install_artifacts(backend, Kind::Zip, artifact_url, config).await
}
