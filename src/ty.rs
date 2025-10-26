use crate::InstallConfig;
use crate::artifact::{GhArtifact, GhArtifacts};
use crate::download::{download, download_dist_manfiest, download_json};
use crate::manfiest::DistManifest;
use crate::tool::get_artifact_url;
use anyhow::Result;
use regex::Regex;
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

#[derive(Debug, Default, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Repo {
    pub(crate) owner: String,
    pub(crate) name: String,
    pub(crate) tag: Option<String>,
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

    pub(crate) fn get_manfiest_url(&self) -> String {
        match &self.tag {
            Some(t) => format!(
                "https://github.com/{}/{}/releases/download/{}/dist-manifest.json",
                self.owner, self.name, t
            ),
            None => format!(
                "https://github.com/{}/{}/releases/latest/download/dist-manifest.json",
                self.owner, self.name
            ),
        }
    }

    pub(crate) async fn get_manfiest(&self, retry: usize) -> Result<DistManifest> {
        download_dist_manfiest(&self.get_manfiest_url(), retry).await
    }
    pub(crate) async fn get_artifact_url(
        &self,
        config: &InstallConfig,
    ) -> Result<Vec<(String, String)>> {
        trace!("get_artifact_url {}/{}", self.owner, self.name);
        let api = self.get_artifact_api();
        trace!("get_artifact_url api {}", api);

        let artifacts = download_json::<GhArtifacts>(&api, config.retry).await?;
        get_artifact_url(artifacts, config)
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
    pub(crate) async fn get_artifact(&self, retry: usize) -> Result<GhArtifacts> {
        let html = download(&self.url, retry).await?.text().await?;
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
        let artifacts = self.get_artifact(config.retry).await?;
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
    use crate::ty::{Nightly, Repo};
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
}
