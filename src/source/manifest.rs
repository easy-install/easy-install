//! Dist-manifest sources (`cargo-dist` schema).

use crate::InstallConfig;
use crate::artifact::Artifact;
use crate::download::{download_json, read_dist_manifest};
use crate::error::{Result, format_chain};
use crate::manifest::DistManifest;
use crate::path::is_dist_manifest;
use crate::select;
use crate::source::{Input, Source};
use async_trait::async_trait;
use github_proxy::fetch::Url;

/// A local or remote dist-manifest.
pub(crate) struct Manifest;

#[async_trait]
impl Source for Manifest {
    fn name(&self) -> &'static str {
        "dist-manifest"
    }

    fn matches(&self, input: &Input<'_>) -> bool {
        is_dist_manifest(input.raw)
    }

    async fn resolve(&self, input: &Input<'_>, config: &InstallConfig) -> Result<Vec<Artifact>> {
        let manifest = if Url::is_valid(input.raw) {
            download_json(input.raw, config.retry, config.timeout).await
        } else {
            read_dist_manifest(input.raw)
        };

        match manifest {
            Ok(manifest) if !manifest.artifacts.is_empty() => resolve(&manifest, input.raw, config),
            Ok(_) => {
                if !config.quiet {
                    println!("dist-manifest at {} contains no artifacts", input.raw);
                }
                Ok(Vec::new())
            }
            Err(e) => {
                if !config.quiet {
                    println!(
                        "failed to read dist-manifest from {}: {}",
                        input.raw,
                        format_chain(&e)
                    );
                }
                Ok(Vec::new())
            }
        }
    }
}

/// Select the artifacts declared by `manifest`.
pub(crate) fn resolve(
    manifest: &DistManifest,
    url: &str,
    config: &InstallConfig,
) -> Result<Vec<Artifact>> {
    let selected = select::from_manifest(url, manifest, config);
    if selected.is_empty() && !config.quiet {
        println!("install_from_manifest {url} failed");
    }
    Ok(selected)
}
