use crate::InstallConfig;
use crate::install::artifact::install_from_artifact_url;
use crate::manfiest::DistManifest;
use crate::tool::get_artifact_url_from_manfiest;
use crate::types::Output;
use anyhow::Result;
use tracing::trace;

pub(crate) async fn install_from_manfiest(
    manfiest: DistManifest,
    url: &str,
    config: &InstallConfig,
) -> Result<Output> {
    trace!("install_from_manfiest {}", url);
    let mut v: std::collections::HashMap<String, crate::types::OutputItem> = Output::new();
    let art_url_list = get_artifact_url_from_manfiest(url, &manfiest);
    if art_url_list.is_empty() {
        println!("install_from_manfiest {url} failed");
        return Ok(v);
    }
    for (name, art_url) in art_url_list {
        trace!("install_from_manfiest art_url {}", art_url);
        if !config.name.is_empty() && !config.name.contains(&name) {
            continue;
        }
        v.extend(install_from_artifact_url(&art_url, &name, config).await?);
    }
    Ok(v)
}
