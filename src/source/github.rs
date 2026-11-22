//! GitHub repository sources.
//!
//! A repository (or release page / release asset URL) is resolved to its
//! release assets. When the project publishes a `dist-manifest.json`, that
//! manifest is preferred because it describes multi-file bundles precisely.

use crate::InstallConfig;
use crate::artifact::Artifact;
use crate::download::{download_json, download_text};
use crate::error::{Context, Result, err};
use crate::manifest::DistManifest;
use crate::source::{Input, Source, manifest, select_assets};
use async_trait::async_trait;
use github_proxy::artifacts::{GhArtifact, GhArtifacts};
use github_proxy::{Proxy, Resource};
use regex::Regex;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::fmt::Display;
use std::str::FromStr;
use std::sync::LazyLock;
use tracing::trace;

static RE_GH_TAG: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^https?://github\.com/(?P<owner>[^/]+)/(?P<repo>[^/]+)/releases/tag/(?P<tag>[^/]+)$",
    )
    .unwrap()
});

static RE_GH_DOWNLOAD_TAG: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^https?://github\.com/(?P<owner>[^/]+)/(?P<repo>[^/]+)/releases/download/(?P<tag>[^/]+)/(?P<filename>.+)$",
    )
    .unwrap()
});

static RE_GH_RELEASES: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^https?://github\.com/(?P<owner>[^/]+)/(?P<repo>[^/]+)").unwrap()
});

static RE_SHORT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?P<owner>[\w.-]+)/(?P<repo>[\w.-]+)(?:@(?P<tag>[\w.-]+))?$").unwrap()
});

static RE_LATEST_TAG: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"href="/[^/]+/[^/]+/releases/tag/([^"]+)""#).unwrap());

static RE_RELEASE_HTML: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"<a\s+href="(/[^/]+/[^/]+/releases/download/[^/]+/([^"]+))"\s+rel="nofollow""#)
        .unwrap()
});

#[derive(Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Clone)]
pub(crate) struct Repo {
    pub(crate) owner: String,
    pub(crate) name: String,
    pub(crate) tag: Option<String>,
}

#[derive(Debug, Default, PartialEq, Eq, Clone, Deserialize)]
struct JsdelivrPackage {
    tags: HashMap<String, String>,
    versions: Vec<String>,
}

/// A GitHub repository, release page or release asset URL.
pub(crate) struct Github;

#[async_trait]
impl Source for Github {
    fn name(&self) -> &'static str {
        "github"
    }

    fn matches(&self, input: &Input<'_>) -> bool {
        input.raw.parse::<Repo>().is_ok()
    }

    async fn resolve(&self, input: &Input<'_>, config: &InstallConfig) -> Result<Vec<Artifact>> {
        let repo = input.raw.parse::<Repo>()?;
        resolve(&repo, config).await
    }
}

/// Resolve a repository into its installable artifacts.
pub(crate) async fn resolve(repo: &Repo, config: &InstallConfig) -> Result<Vec<Artifact>> {
    trace!("resolve repo {repo}");

    if let Ok(artifact_manifest) = repo
        .get_manifest(config.retry, config.proxy, config.timeout)
        .await
    {
        let url = repo
            .get_manifest_url(config.proxy, config.retry, config.timeout)
            .await?;
        return manifest::resolve(&artifact_manifest, &url, config);
    }

    let assets = repo.get_raw_artifacts(config.retry, config.timeout).await?;
    select_assets(assets, config, &repo.get_gh_url())
}

impl FromStr for Repo {
    type Err = crate::error::Error;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        trace!("parse repo {value}");
        let value = value.strip_suffix(".git").unwrap_or(value);

        for regex in [
            &*RE_GH_TAG,
            &*RE_GH_DOWNLOAD_TAG,
            &*RE_GH_RELEASES,
            &*RE_SHORT,
        ] {
            let Some(captures) = regex.captures(value) else {
                continue;
            };
            return Ok(Repo {
                owner: captures["owner"].to_string(),
                name: captures["repo"].to_string(),
                tag: captures.name("tag").map(|tag| tag.as_str().to_string()),
            });
        }

        Err(err!("Invalid repo string: {value}"))
    }
}

impl Repo {
    pub(crate) fn get_gh_url(&self) -> String {
        format!("https://github.com/{}/{}", self.owner, self.name)
    }

    fn get_artifact_api(&self) -> String {
        trace!("get_artifact_api {}/{}", self.owner, self.name);
        match &self.tag {
            Some(tag) => format!(
                "https://api.github.com/repos/{}/{}/releases/tags/{}",
                self.owner, self.name, tag
            ),
            None => format!(
                "https://api.github.com/repos/{}/{}/releases/latest",
                self.owner, self.name
            ),
        }
    }

    fn build_release_url(&self, filename: &str, tag: &str, proxy: Proxy) -> Option<String> {
        proxy.url(&Resource::Release {
            owner: self.owner.clone(),
            repo: self.name.clone(),
            tag: tag.to_string(),
            name: filename.to_string(),
        })
    }

    async fn get_manifest_url(&self, proxy: Proxy, retry: usize, timeout: u64) -> Result<String> {
        match &self.tag {
            Some(tag) => self
                .build_release_url("dist-manifest.json", tag, proxy)
                .context("Failed to build manifest URL from tag"),
            None => {
                let tag = self.get_latest_tag(retry, timeout).await?;
                self.build_release_url("dist-manifest.json", &tag, proxy)
                    .context("Failed to get manifest URL from latest tag")
            }
        }
    }

    fn parse_latest_tag(html: &str) -> Result<String> {
        if let Some(captures) = RE_LATEST_TAG.captures(html) {
            let tag = captures[1].to_string();
            trace!("Found latest tag: {}", tag);
            return Ok(tag);
        }
        Err(err!("No release tag found in HTML"))
    }

    async fn get_latest_tag(&self, retry: usize, timeout: u64) -> Result<String> {
        let releases_url = format!("https://github.com/{}/{}/releases", self.owner, self.name);
        trace!("Fetching releases page to get latest tag: {}", releases_url);

        if let Ok(html) = download_text(&releases_url, retry, timeout).await {
            return Self::parse_latest_tag(&html);
        }

        let package_url = format!(
            "https://data.jsdelivr.com/v1/package/gh/{}/{}",
            self.owner, self.name
        );
        let package: JsdelivrPackage = download_json(&package_url, retry, timeout).await?;

        if let Some(latest) = package.tags.get("latest") {
            return Ok(latest.to_string());
        }
        if let Some(latest) = package.versions.first() {
            return Ok(latest.to_string());
        }

        Err(err!("No latest tag found from GitHub or jsDelivr"))
    }

    async fn get_release_page_url(&self, retry: usize, timeout: u64) -> Result<String> {
        let tag = match &self.tag {
            Some(tag) => tag.clone(),
            None => self.get_latest_tag(retry, timeout).await?,
        };
        Ok(format!(
            "https://github.com/{}/{}/releases/expanded_assets/{}",
            self.owner, self.name, tag
        ))
    }

    fn parse_release_html(html: &str) -> Result<GhArtifacts> {
        let mut assets = HashSet::new();
        for captures in RE_RELEASE_HTML.captures_iter(html) {
            let path = &captures[1];
            if !path.starts_with('/') || !path.contains("/releases/download/") {
                continue;
            }
            assets.insert(GhArtifact {
                name: captures[2].to_string(),
                browser_download_url: format!("https://github.com{path}"),
                size: 0,
                url: None,
            });
        }

        if assets.is_empty() {
            return Err(err!("No assets found in release page HTML"));
        }
        Ok(GhArtifacts { assets })
    }

    pub(crate) async fn get_manifest(
        &self,
        retry: usize,
        proxy: Proxy,
        timeout: u64,
    ) -> Result<DistManifest> {
        download_json(
            &self.get_manifest_url(proxy, retry, timeout).await?,
            retry,
            timeout,
        )
        .await
    }

    /// Fetch raw (unfiltered) release assets from the GitHub API, falling back
    /// to the release page HTML when the API is unavailable.
    pub(crate) async fn get_raw_artifacts(
        &self,
        retry: usize,
        timeout: u64,
    ) -> Result<GhArtifacts> {
        let api = self.get_artifact_api();
        trace!("get_raw_artifacts api {}", api);

        match download_json::<GhArtifacts>(&api, retry, timeout).await {
            Ok(artifacts) => {
                trace!(
                    "Successfully retrieved artifacts from API for {}/{}",
                    self.owner, self.name
                );
                Ok(artifacts)
            }
            Err(api_error) => {
                trace!(
                    "API request failed for {}/{}: {}, attempting HTML fallback",
                    self.owner, self.name, api_error
                );

                let page_url = self.get_release_page_url(retry, timeout).await?;
                let html = download_text(&page_url, retry, timeout).await?;
                Self::parse_release_html(&html).map_err(|html_error| {
                    err!(
                        "Failed to retrieve artifacts for {}/{}. API error: {}. HTML parsing error: {}",
                        self.owner,
                        self.name,
                        api_error,
                        html_error
                    )
                })
            }
        }
    }

    #[cfg(test)]
    async fn select(&self, config: &InstallConfig) -> Result<Vec<Artifact>> {
        let assets = self.get_raw_artifacts(config.retry, config.timeout).await?;
        crate::select::from_assets(assets, config)
    }
}

impl Display for Repo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.tag {
            Some(tag) => write!(f, "{}/{}@{}", self.owner, self.name, tag),
            None => write!(f, "{}/{}", self.owner, self.name),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Context;

    #[test]
    fn test_is_github() {
        let repo = Repo {
            owner: "ahaoboy".to_string(),
            name: "ansi2".to_string(),
            tag: None,
        };
        for input in [
            "https://github.com/ahaoboy/ansi2",
            "ahaoboy/ansi2",
            "https://github.com/ahaoboy/ansi2.git",
        ] {
            assert_eq!(
                input.parse::<Repo>().context(input).unwrap(),
                repo,
                "{input}"
            );
        }

        assert!(
            "https://api.github.com/repos/ahaoboy/ansi2/releases/latest"
                .parse::<Repo>()
                .context("api url should not be a repo")
                .is_err()
        );

        let repo = Repo {
            owner: "ahaoboy".to_string(),
            name: "ansi2".to_string(),
            tag: Some("v0.2.11".to_string()),
        };
        for input in [
            "ahaoboy/ansi2@v0.2.11",
            "https://github.com/ahaoboy/ansi2/releases/tag/v0.2.11",
            "https://github.com/ahaoboy/ansi2/releases/download/v0.2.11/ansi2-x86_64-unknown-linux-musl.tar.gz",
            "https://github.com/ahaoboy/ansi2/releases/download/v0.2.11/ansi2-x86_64-pc-windows-msvc.zip",
        ] {
            assert_eq!(
                input.parse::<Repo>().context(input).unwrap(),
                repo,
                "{input}"
            );
        }
    }

    #[test]
    fn test_get_artifact_api() {
        let repo = "https://github.com/axodotdev/cargo-dist"
            .parse::<Repo>()
            .unwrap();
        assert_eq!(
            repo.get_artifact_api(),
            "https://api.github.com/repos/axodotdev/cargo-dist/releases/latest"
        );
    }

    #[tokio::test]
    async fn test_get_manifest() {
        let repo = "https://github.com/axodotdev/cargo-dist/releases/tag/v0.25.1"
            .parse::<Repo>()
            .unwrap();
        let url = repo.get_manifest_url(Proxy::Github, 3, 600).await.unwrap();
        assert_eq!(
            url,
            "https://github.com/axodotdev/cargo-dist/releases/download/v0.25.1/dist-manifest.json"
        );

        let manifest = repo.get_manifest(3, Proxy::Github, 30).await.unwrap();
        assert!(!manifest.artifacts.is_empty());

        let repo = "https://github.com/ahaoboy/mujs-build/releases/tag/v0.0.2"
            .parse::<Repo>()
            .unwrap();
        let url = repo.get_manifest_url(Proxy::Github, 3, 600).await.unwrap();
        assert_eq!(
            url,
            "https://github.com/ahaoboy/mujs-build/releases/download/v0.0.2/dist-manifest.json"
        );

        let manifest = repo.get_manifest(3, Proxy::Github, 30).await.unwrap();
        assert!(!manifest.artifacts.is_empty());
    }

    #[tokio::test]
    async fn test_select() {
        for url in [
            "https://github.com/AlistGo/alist",
            "https://github.com/ahaoboy/bloaty-metafile.git",
        ] {
            let repo = url.parse::<Repo>().unwrap();
            let artifacts = repo.select(&InstallConfig::default()).await.unwrap();
            assert_eq!(artifacts.len(), 1, "{url}");
        }
    }

    #[tokio::test]
    async fn test_release_page_html() {
        let repo = "ahaoboy/neofetch".parse::<Repo>().unwrap();
        let page_url = repo.get_release_page_url(3, 30).await.unwrap();
        let html = download_text(&page_url, 3, 30).await.unwrap();
        let assets = Repo::parse_release_html(&html).unwrap();
        assert!(!assets.assets.is_empty());
    }

    #[tokio::test]
    async fn test_deno() {
        let repo = "https://github.com/denoland/deno".parse::<Repo>().unwrap();
        let artifacts = repo.select(&InstallConfig::default()).await.unwrap();
        assert_eq!(artifacts.len(), 3);
    }

    #[tokio::test]
    async fn test_starship() {
        let repo = "https://github.com/starship/starship"
            .parse::<Repo>()
            .unwrap();
        let artifacts = repo.select(&InstallConfig::default()).await.unwrap();
        assert_eq!(artifacts.len(), 1);
    }

    /// `--regex` matches directly against the original file name (including
    /// extension) and selects the matching asset, bypassing `guess_target`.
    #[tokio::test]
    async fn test_mpv_regex() {
        let repo = "https://github.com/mpv-player/mpv/releases/tag/git-release"
            .parse::<Repo>()
            .unwrap();
        let config = InstallConfig {
            regex: Some(r"mpv-v.+x86_64-pc-windows-msvc\.zip$".to_string()),
            ..Default::default()
        };
        let artifacts = repo.select(&config).await.unwrap();
        assert_eq!(artifacts.len(), 1);
        assert!(!artifacts[0].name.is_empty());
        assert!(artifacts[0].url.contains("mpv-v"));
        assert!(artifacts[0].url.ends_with("x86_64-pc-windows-msvc.zip"));
    }
}
