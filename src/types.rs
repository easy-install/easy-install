use crate::InstallConfig;
use crate::artifact::{GhArtifact, GhArtifacts};
use crate::download::{download, download_dist_manfiest, download_json};
use crate::manfiest::DistManifest;
use crate::tool::get_artifact_url;
use anyhow::{Context, Result};
use github_proxy::{Proxy, Resource};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fmt::Display;
use tracing::trace;

#[derive(Debug, Clone, PartialEq, PartialOrd, Default)]
pub(crate) struct OutputFile {
    pub(crate) install_path: String,
    pub(crate) mode: Option<u32>,
    pub(crate) size: u32,
    pub(crate) origin_path: String,
    pub(crate) is_dir: bool,
    pub(crate) buffer: Vec<u8>,
}
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct OutputItem {
    pub(crate) install_dir: String,
    pub(crate) files: Vec<OutputFile>,
}

pub(crate) type Output = HashMap<String, OutputItem>;

#[derive(Debug, Default, Hash, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub(crate) struct Repo {
    pub(crate) owner: String,
    pub(crate) name: String,
    pub(crate) tag: Option<String>,
}

#[derive(Debug, Default, PartialEq, Eq, Clone, Serialize, Deserialize)]
struct JsdelivrPackage {
    tags: HashMap<String, String>,
    versions: Vec<String>,
}

impl TryFrom<&str> for Repo {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> Result<Self> {
        trace!("get_artifact_api {}", value);
        let value = if value.ends_with(".git") {
            &value[0..value.len() - 4]
        } else {
            value
        };
        let re_gh_tag = Regex::new(
            r"^https?://github\.com/(?P<owner>[^/]+)/(?P<repo>[^/]+)/releases/tag/(?P<tag>[^/]+)$",
        )?;

        let re_gh_download_tag = Regex::new(
            r"^https?://github\.com/(?P<owner>[^/]+)/(?P<repo>[^/]+)/releases/download/(?P<tag>[^/]+)/(?P<filename>.+)$",
        )?;

        let re_gh_releases = Regex::new(r"^http?s://github\.com/(?P<owner>[^/]+)/(?P<repo>[^/]+)")?;

        let re_short = Regex::new(r"^(?P<owner>[\w.-]+)/(?P<repo>[\w.-]+)(?:@(?P<tag>[\w.-]+))?$")?;
        if let Some(captures) = re_gh_tag.captures(value)
            && let (Some(owner), Some(name), Some(tag)) = (
                captures.name("owner"),
                captures.name("repo"),
                captures.name("tag"),
            )
        {
            return Ok(Repo {
                owner: owner.as_str().to_string(),
                name: name.as_str().to_string(),
                tag: Some(tag.as_str().to_string()),
            });
        }

        if let Some(captures) = re_gh_download_tag.captures(value)
            && let (Some(owner), Some(name), Some(tag)) = (
                captures.name("owner"),
                captures.name("repo"),
                captures.name("tag"),
            )
        {
            return Ok(Repo {
                owner: owner.as_str().to_string(),
                name: name.as_str().to_string(),
                tag: Some(tag.as_str().to_string()),
            });
        }

        if let Some(captures) = re_gh_releases.captures(value)
            && let (Some(owner), Some(name)) = (captures.name("owner"), captures.name("repo"))
        {
            return Ok(Repo {
                owner: owner.as_str().to_string(),
                name: name.as_str().to_string(),
                tag: None,
            });
        }

        if let Some(captures) = re_short.captures(value)
            && let (Some(owner), Some(name), tag) = (
                captures.name("owner"),
                captures.name("repo"),
                captures.name("tag"),
            )
        {
            return Ok(Repo {
                owner: owner.as_str().to_string(),
                name: name.as_str().to_string(),
                tag: tag.map(|i| i.as_str().to_string()),
            });
        }
        Err(anyhow::anyhow!("Invalid repo string: {value}"))
    }
}

impl Repo {
    pub(crate) fn get_gh_url(&self) -> String {
        format!("https://github.com/{}/{}", self.owner, self.name)
    }

    pub(crate) fn convert_github_url_to_proxy(url: &str, proxy: Proxy) -> String {
        if proxy == Proxy::Github {
            return url.to_string();
        }

        let re = Regex::new(
            r"^https://github\.com/(?P<owner>[^/]+)/(?P<repo>[^/]+)/releases/download/(?P<tag>[^/]+)/(?P<filename>.+)$"
        ).unwrap();

        if let Some(captures) = re.captures(url) {
            let owner = captures.name("owner").unwrap().as_str();
            let repo_name = captures.name("repo").unwrap().as_str();
            let tag = captures.name("tag").unwrap().as_str();
            let filename = captures.name("filename").unwrap().as_str();

            let release = Resource::Release {
                owner: owner.to_string(),
                repo: repo_name.to_string(),
                tag: tag.to_string(),
                name: filename.to_string(),
            };
            return proxy.url(release).unwrap_or(url.to_string());
        }

        // If URL doesn't match GitHub release pattern, return as-is
        url.to_string()
    }

    pub(crate) fn get_artifact_api(&self) -> String {
        trace!("get_artifact_api {}/{}", self.owner, self.name);
        if let Some(tag) = &self.tag {
            return format!(
                "https://api.github.com/repos/{}/{}/releases/tags/{}",
                self.owner, self.name, tag
            );
        }

        format!(
            "https://api.github.com/repos/{}/{}/releases/latest",
            self.owner, self.name,
        )
    }

    pub(crate) fn build_release_url(
        &self,
        filename: &str,
        tag: &str,
        proxy: Proxy,
    ) -> Option<String> {
        let release = Resource::Release {
            owner: self.owner.clone(),
            repo: self.name.clone(),
            tag: tag.to_string(),
            name: filename.to_string(),
        };
        proxy.url(release)
    }

    pub(crate) async fn get_manfiest_url(
        &self,
        proxy: Proxy,
        retry: usize,
        timeout: u64,
    ) -> Result<String> {
        let filename = "dist-manifest.json";
        match &self.tag {
            Some(tag) => self
                .build_release_url(filename, tag, proxy)
                .context("Failed to build manifest URL from tag"),
            None => {
                let tag = self.get_latest_tag(retry, timeout).await?;

                let resource = Resource::Release {
                    owner: self.owner.clone(),
                    repo: self.name.clone(),
                    tag,
                    name: filename.to_string(),
                };
                proxy
                    .url(resource)
                    .context("Failed to get manifest URL from latest tag")
            }
        }
    }

    fn parse_latest_tag(html: &str) -> Result<String> {
        let re = Regex::new(r#"href="/[^/]+/[^/]+/releases/tag/([^"]+)""#)?;

        if let Some(cap) = re.captures(html) {
            let tag = cap[1].to_string();
            trace!("Found latest tag: {}", tag);
            return Ok(tag);
        }

        Err(anyhow::anyhow!("No release tag found in HTML"))
    }

    async fn get_latest_tag(&self, retry: usize, timeout: u64) -> Result<String> {
        let releases_url = format!("https://github.com/{}/{}/releases", self.owner, self.name);
        trace!("Fetching releases page to get latest tag: {}", releases_url);

        if let Ok(response) = download(&releases_url, retry, timeout).await
            && let Ok(html) = response.text().await
        {
            return Self::parse_latest_tag(&html);
        }

        let pkg_url = format!(
            "https://data.jsdelivr.com/v1/package/gh/{}/{}",
            self.owner, self.name
        );
        let response = download(&pkg_url, retry, timeout).await?;
        let pkg: JsdelivrPackage = response.json().await?;

        if let Some(v) = pkg.tags.get("latest") {
            return Ok(v.to_string());
        };

        if let Some(v) = pkg.versions.first() {
            return Ok(v.to_string());
        }

        Err(anyhow::anyhow!(
            "No latest tag found from GitHub or jsDelivr"
        ))
    }

    async fn get_release_page_url(&self, retry: usize, timeout: u64) -> Result<String> {
        match &self.tag {
            Some(t) => Ok(format!(
                "https://github.com/{}/{}/releases/expanded_assets/{}",
                self.owner, self.name, t
            )),
            None => {
                let tag = self.get_latest_tag(retry, timeout).await?;
                Ok(format!(
                    "https://github.com/{}/{}/releases/expanded_assets/{}",
                    self.owner, self.name, tag
                ))
            }
        }
    }

    fn parse_release_html(html: &str) -> Result<GhArtifacts> {
        let re = Regex::new(
            r#"<a\s+href="(/[^/]+/[^/]+/releases/download/[^/]+/([^"]+))"\s+rel="nofollow""#,
        )?;
        let mut assets = HashSet::new();

        for cap in re.captures_iter(html) {
            let path = &cap[1];
            let name = cap[2].to_string();

            if !path.starts_with('/') || !path.contains("/releases/download/") {
                continue;
            }

            let browser_download_url = format!("https://github.com{}", path);

            assets.insert(GhArtifact {
                name,
                browser_download_url,
            });
        }

        if assets.is_empty() {
            return Err(anyhow::anyhow!("No assets found in release page HTML"));
        }

        Ok(GhArtifacts { assets })
    }

    pub(crate) async fn get_manfiest(
        &self,
        retry: usize,
        proxy: Proxy,
        timeout: u64,
    ) -> Result<DistManifest> {
        download_dist_manfiest(
            &self.get_manfiest_url(proxy, retry, timeout).await?,
            retry,
            timeout,
        )
        .await
    }

    async fn get_artifact_url_from_html(
        &self,
        config: &InstallConfig,
    ) -> Result<Vec<(String, String)>> {
        let page_url = self
            .get_release_page_url(config.retry, config.timeout)
            .await?;
        trace!("Fetching release page HTML from {}", page_url);

        let response = download(&page_url, config.retry, config.timeout).await?;
        let html = response.text().await?;

        let artifacts = Self::parse_release_html(&html)?;
        get_artifact_url(artifacts, config)
    }

    pub(crate) async fn get_artifact_url(
        &self,
        config: &InstallConfig,
    ) -> Result<Vec<(String, String)>> {
        trace!("get_artifact_url {}/{}", self.owner, self.name);
        let api = self.get_artifact_api();
        trace!("get_artifact_url api {}", api);

        match download_json::<GhArtifacts>(&api, config.retry, config.timeout).await {
            Ok(artifacts) => {
                trace!(
                    "Successfully retrieved artifacts from API for {}/{}",
                    self.owner, self.name
                );
                get_artifact_url(artifacts, config)
            }
            Err(api_error) => {
                trace!(
                    "API request failed for {}/{}: {}, attempting HTML fallback",
                    self.owner, self.name, api_error
                );

                match self.get_artifact_url_from_html(config).await {
                    Ok(result) => {
                        trace!(
                            "Successfully retrieved artifacts from HTML for {}/{}",
                            self.owner, self.name
                        );
                        Ok(result)
                    }
                    Err(html_error) => Err(anyhow::anyhow!(
                        "Failed to retrieve artifacts for {}/{}. API error: {}. HTML parsing error: {}",
                        self.owner,
                        self.name,
                        api_error,
                        html_error
                    )),
                }
            }
        }
    }
}

impl Display for Repo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.tag {
            Some(t) => f.write_str(&format!("{}/{}@{}", self.owner, self.name, t)),
            None => f.write_str(&format!("{}/{}", self.owner, self.name)),
        }
    }
}

pub(crate) struct Nightly {
    pub(crate) url: String,
}

impl Nightly {
    pub(crate) async fn get_artifact(&self, retry: usize, timeout: u64) -> Result<GhArtifacts> {
        let html = download(&self.url, retry, timeout).await?.text().await?;
        if html.contains("class=\"absent\"") {
            return Ok(Default::default());
        }
        let re = Regex::new(r#"<th><a rel="nofollow" href="[^"]+">([^<]+)</a></th>\s*<td><a rel="nofollow" href="([^"]+)">"#).unwrap();
        let mut assets = HashSet::new();

        // Iterate over all matches in the HTML
        for cap in re.captures_iter(&html) {
            let name = cap[1].to_string();
            let browser_download_url = cap[2].to_string();
            assets.insert(GhArtifact {
                name,
                browser_download_url,
            });
        }

        Ok(GhArtifacts { assets })
    }
    pub(crate) async fn get_artifact_url(
        &self,
        config: &InstallConfig,
    ) -> Result<Vec<(String, String)>> {
        let artifacts = self.get_artifact(config.retry, config.timeout).await?;
        get_artifact_url(artifacts, config)
    }
}

impl Display for Nightly {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.url.to_string())
    }
}
impl TryFrom<&str> for Nightly {
    type Error = anyhow::Error;

    fn try_from(url: &str) -> std::result::Result<Self, Self::Error> {
        let re =
            Regex::new(r"^https://nightly\.link/[^/]+/[^/]+/workflows/[^/]+/[^/?]+(\?preview)?$")?;
        let v = re.is_match(url);
        if v {
            Ok(Self {
                url: url.to_string(),
            })
        } else {
            Err(anyhow::anyhow!("Invalid nightly.link string: {url}"))
        }
    }
}

#[cfg(test)]
mod test {
    use crate::types::{Nightly, Repo};
    #[tokio::test]
    async fn test() {
        for i in [
            "https://github.com/AlistGo/alist",
            "https://github.com/ahaoboy/bloaty-metafile.git",
        ] {
            let repo = Repo::try_from(i).unwrap();
            let v = repo.get_artifact_url(&Default::default()).await.unwrap();
            assert_eq!(v.len(), 1);
        }
    }

    #[tokio::test]
    async fn test_nighty() {
        for i in [
            "https://nightly.link/ahaoboy/cross-env/workflows/release/main",
            "https://nightly.link/ahaoboy/cross-env/workflows/release/main?preview",
        ] {
            let nightly = Nightly::try_from(i).unwrap();
            let v = nightly.get_artifact_url(&Default::default()).await.unwrap();
            assert!(!v.is_empty())
        }
    }
    #[tokio::test]
    async fn test_html() {
        let repo = Repo::try_from("ahaoboy/neofetch").unwrap();
        let v = repo
            .get_artifact_url_from_html(&Default::default())
            .await
            .unwrap();
        assert!(!v.is_empty())
    }
}
