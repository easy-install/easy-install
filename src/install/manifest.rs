use crate::InstallConfig;
use crate::backend::Backend;
use crate::error::Result;
use crate::install::{Kind, install_artifacts};
use crate::manifest::DistManifest;
use crate::tool::{filter_artifacts, get_artifact_url_from_manifest};
use crate::types::Output;
use tracing::trace;

pub(crate) async fn install_from_manifest(
    backend: &Backend,
    manifest: DistManifest,
    url: &str,
    config: &InstallConfig,
) -> Result<Output> {
    trace!("install_from_manifest {}", url);
    let art_url_list = get_artifact_url_from_manifest(url, &manifest, config);
    if art_url_list.is_empty() {
        if !config.quiet {
            println!("install_from_manifest {url} failed");
        }
        return Ok(Output::new());
    }
    let art_url_list = filter_artifacts(art_url_list, config);
    install_artifacts(backend, Kind::Auto, art_url_list, config).await
}
