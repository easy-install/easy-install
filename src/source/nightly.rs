//! nightly.link sources.
//!
//! `nightly.link` exposes the artifacts of a GitHub Actions workflow as a
//! plain HTML page. The page is parsed for `name → download URL` pairs, which
//! are then matched like any other release asset.

use crate::InstallConfig;
use crate::artifact::Artifact;
use crate::download::download_text;
use crate::error::{Result, err};
use crate::source::{Input, Source, select_assets};
use async_trait::async_trait;
use github_proxy::artifacts::{GhArtifact, GhArtifacts};
use regex::Regex;
use std::collections::HashSet;
use std::fmt::Display;
use std::sync::LazyLock;

static RE_NIGHTLY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^https://nightly\.link/[^/]+/[^/]+/workflows/[^/]+/[^/?]+(\?preview)?$").unwrap()
});

static RE_ASSETS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"<th><a rel="nofollow" href="[^"]+">([^<]+)</a></th>\s*<td><a rel="nofollow" href="([^"]+)">"#,
    )
    .unwrap()
});

/// A nightly.link page.
#[derive(Debug, Clone)]
pub(crate) struct Nightly {
    pub(crate) url: String,
}

impl Nightly {
    async fn get_raw_artifacts(&self, retry: usize, timeout: u64) -> Result<GhArtifacts> {
        let html = download_text(&self.url, retry, timeout).await?;
        if html.contains("class=\"absent\"") {
            return Ok(GhArtifacts::default());
        }

        let mut assets = HashSet::new();
        for captures in RE_ASSETS.captures_iter(&html) {
            assets.insert(GhArtifact {
                name: captures[1].to_string(),
                browser_download_url: captures[2].to_string(),
                size: 0,
                url: None,
            });
        }
        Ok(GhArtifacts { assets })
    }
}

impl Display for Nightly {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.url)
    }
}

impl std::str::FromStr for Nightly {
    type Err = crate::error::Error;

    fn from_str(url: &str) -> std::result::Result<Self, Self::Err> {
        if RE_NIGHTLY.is_match(url) {
            Ok(Self {
                url: url.to_string(),
            })
        } else {
            Err(err!("Invalid nightly.link string: {url}"))
        }
    }
}

/// A `nightly.link` build page.
pub(crate) struct Link;

#[async_trait]
impl Source for Link {
    fn name(&self) -> &'static str {
        "nightly.link"
    }

    fn matches(&self, input: &Input<'_>) -> bool {
        input.raw.parse::<Nightly>().is_ok()
    }

    async fn resolve(&self, input: &Input<'_>, config: &InstallConfig) -> Result<Vec<Artifact>> {
        let nightly = input.raw.parse::<Nightly>()?;
        resolve(&nightly, config).await
    }
}

/// Resolve a nightly.link page into its installable artifacts.
pub(crate) async fn resolve(nightly: &Nightly, config: &InstallConfig) -> Result<Vec<Artifact>> {
    let artifacts = nightly
        .get_raw_artifacts(config.retry, config.timeout)
        .await?;
    select_assets(artifacts, config, &nightly.url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_nightly() {
        for url in [
            "https://nightly.link/mpv-easy/mpv-easy/workflows/release/main",
            "https://nightly.link/mpv-easy/mpv-easy/workflows/release/main?preview",
        ] {
            let nightly = url.parse::<Nightly>().unwrap();
            let artifacts = resolve(&nightly, &Default::default()).await.unwrap();
            assert!(!artifacts.is_empty());
        }
    }
}
