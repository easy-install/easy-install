use crate::InstallConfig;
use crate::install::artifact::install_from_artifact_url;
use crate::tool::not_found_asset_message;
use crate::types::{Nightly, Output};
use anyhow::Result;
use tracing::trace;

pub(crate) async fn install_from_nightly(repo: &Nightly, config: &InstallConfig) -> Result<Output> {
    trace!("install_from_nightly {}", repo);

    let artifact_url = repo.get_artifact_url(config).await?;
    let mut v = Output::new();
    if !artifact_url.is_empty() {
        // When --alias is specified and matches an artifact name, only install that one
        let artifact_url = if let Some(alias) = &config.alias {
            let matching: Vec<_> = artifact_url
                .iter()
                .filter(|(name, _)| name == alias)
                .cloned()
                .collect();
            if matching.is_empty() {
                artifact_url
            } else {
                matching
            }
        } else {
            artifact_url
        };
        for (name, i) in artifact_url {
            trace!("install_from_git artifact_url {}", i);
            if !config.name.is_empty() && !config.name.contains(&name) {
                continue;
            }
            v.extend(install_from_artifact_url(&i, &name, config).await?);
        }
    } else {
        not_found_asset_message(&repo.url, config);
    }
    Ok(v)
}
