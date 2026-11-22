//! Source resolution.
//!
//! A user-supplied input can mean many things: a GitHub repository, a release
//! page, a direct artifact URL, a local archive, a dist-manifest, a GitHub
//! Actions artifact or a nightly.link build. Every kind of input is a
//! [`Source`]; [`SOURCES`] lists them in dispatch order and [`resolve`] hands
//! the input to the first one that recognizes it.
//!
//! A source only *identifies* its input and produces [`Artifact`]s. Selection
//! (`--regex` / `--name` / `--target`), extraction, writing and
//! post-processing are shared by the install pipeline, so adding a source
//! means one new module plus one entry in `SOURCES`.

pub(crate) mod ci;
pub(crate) mod direct;
pub(crate) mod github;
pub(crate) mod manifest;
pub(crate) mod nightly;

use crate::InstallConfig;
use crate::artifact::{Artifact, Kind};
use crate::error::Result;
use crate::path::{get_filename, name_no_ext};
use crate::select;
use async_trait::async_trait;
use github_proxy::artifacts::GhArtifacts;
use guess_target::guess_target;
use tracing::trace;

/// One input plus the values every source derives from it.
pub(crate) struct Input<'a> {
    /// Exactly what the user passed.
    pub(crate) raw: &'a str,
    /// File name of [`Self::raw`]; works for URLs and local paths alike.
    pub(crate) filename: String,
    /// Tool name derived from [`Self::filename`], refined by the target guess.
    pub(crate) name: String,
    /// [`Self::raw`] routed through the configured GitHub proxy when it is a
    /// GitHub resource, otherwise unchanged.
    pub(crate) proxied: String,
}

impl<'a> Input<'a> {
    pub(crate) fn new(raw: &'a str, config: &InstallConfig) -> Self {
        let filename = get_filename(raw);
        let stem = name_no_ext(&filename);
        let name = guess_target(&stem)
            .into_iter()
            .find(|guess| config.targets().contains(&guess.target))
            .map_or(stem, |guess| guess.name);

        Self {
            raw,
            filename,
            name,
            proxied: apply_proxy(raw, config.proxy),
        }
    }
}

/// A place installable artifacts can come from.
#[async_trait]
pub(crate) trait Source: Send + Sync {
    /// Short identifier used in traces.
    fn name(&self) -> &'static str;

    /// Whether this source handles `input`.
    ///
    /// Must stay cheap: it runs for every source up to the matching one.
    fn matches(&self, input: &Input<'_>) -> bool;

    /// Resolve `input` into the artifacts to install.
    async fn resolve(&self, input: &Input<'_>, config: &InstallConfig) -> Result<Vec<Artifact>>;
}

/// Sources in dispatch order; the first match wins.
///
/// [`direct::Direct`] claims any URL or file whose format it recognizes, so
/// the more specific sources must stay ahead of it, and
/// [`direct::Fallback`] must stay last because it accepts everything.
static SOURCES: &[&dyn Source] = &[
    &manifest::Manifest,
    &ci::Ci,
    &direct::Direct,
    &nightly::Link,
    &github::Github,
    &direct::Fallback,
];

/// Resolve `url` into the artifacts to install.
pub(crate) async fn resolve(url: &str, config: &InstallConfig) -> Result<Vec<Artifact>> {
    trace!("resolve {url}");
    let input = Input::new(url, config);

    match SOURCES.iter().find(|source| source.matches(&input)) {
        Some(source) => {
            trace!("source {} handles {url}", source.name());
            source.resolve(&input, config).await
        }
        // `Fallback` accepts everything, so this is only reachable if it is
        // removed from `SOURCES`.
        None => Ok(vec![Artifact::new(
            input.name.clone(),
            input.raw.to_string(),
            Kind::Single,
        )]),
    }
}

/// Route a URL through the configured GitHub proxy when it is one of its
/// resources.
pub(crate) fn apply_proxy(url: &str, proxy: github_proxy::Proxy) -> String {
    match github_proxy::Resource::try_from(url) {
        Ok(resource) => resource.url(&proxy).unwrap_or_else(|| url.to_string()),
        Err(_) => url.to_string(),
    }
}

/// Select installable artifacts from a set of release assets, reporting when
/// nothing matched.
pub(crate) fn select_assets(
    assets: GhArtifacts,
    config: &InstallConfig,
    source: &str,
) -> Result<Vec<Artifact>> {
    let available: Vec<String> = assets
        .assets
        .iter()
        .map(|asset| asset.name.clone())
        .collect();
    let selected = select::from_assets(assets, config)?;
    if selected.is_empty() {
        select::not_found(source, config, Some(&available));
    }
    Ok(selected)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Name of the source that would handle `input`.
    fn source_for(input: &str) -> &'static str {
        let config = InstallConfig::default();
        let input = Input::new(input, &config);
        SOURCES
            .iter()
            .find(|source| source.matches(&input))
            .map(|source| source.name())
            .expect("the fallback source accepts everything")
    }

    /// More specific sources win over the direct/fallback ones, so a
    /// repository URL is never downloaded as a single HTML file.
    #[test]
    fn sources_are_tried_in_priority_order() {
        assert_eq!(source_for("https://github.com/owner/repo"), "github");
        assert_eq!(source_for("owner/repo@v1"), "github");
        assert_eq!(
            source_for("https://github.com/owner/repo/actions/runs/123"),
            "github-actions"
        );
        assert_eq!(
            source_for("https://nightly.link/o/r/workflows/release/main"),
            "nightly.link"
        );
        assert_eq!(
            source_for("https://example.com/dist-manifest.json"),
            "dist-manifest"
        );
        assert_eq!(source_for("https://example.com/tool.tar.gz"), "direct");
        assert_eq!(source_for("not-a-real-input"), "fallback");
    }
}
