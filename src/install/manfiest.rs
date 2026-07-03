use crate::InstallConfig;
use crate::install::install_artifacts;
use crate::manfiest::DistManifest;
use crate::tool::{filter_artifacts, get_artifact_url_from_manfiest};
use crate::types::Output;
use anyhow::Result;
use tracing::trace;

pub(crate) async fn install_from_manfiest(
    manfiest: DistManifest,
    url: &str,
    config: &InstallConfig,
) -> Result<Output> {
    trace!("install_from_manfiest {}", url);
    let art_url_list = get_artifact_url_from_manfiest(url, &manfiest, config);
    if art_url_list.is_empty() {
        if !config.quiet {
            println!("install_from_manfiest {url} failed");
        }
        return Ok(Output::new());
    }
    let art_url_list = filter_artifacts(art_url_list, config);
    install_artifacts(art_url_list, config).await
}
