use crate::artifact::GhArtifacts;
use crate::download::{download_dist_manfiest, download_json};
use crate::manfiest::DistManifest;
use crate::tool::{ends_with_exe, get_filename, is_skip, name_no_ext};
use anyhow::Result;
use guess_target::{Os, get_local_target, guess_target};
use regex::Regex;
use std::collections::HashMap;
use std::fmt::Display;
use tracing::trace;

#[derive(Debug, Clone, PartialEq, PartialOrd, Default)]
pub struct OutputFile {
    pub install_path: String,
    pub mode: Option<u32>,
    pub size: u32,
    pub origin_path: String,
    pub is_dir: bool,
    pub buffer: Vec<u8>,
}
#[derive(Debug, Clone, PartialEq, Default)]
pub struct OutputItem {
    pub install_dir: String,
    pub files: Vec<OutputFile>,
}

pub type Output = HashMap<String, OutputItem>;

#[derive(Debug, Default, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct Repo {
    pub owner: String,
    pub name: String,
    pub tag: Option<String>,
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
    pub fn get_gh_url(&self) -> String {
        format!("https://github.com/{}/{}", self.owner, self.name)
    }

    pub fn get_artifact_api(&self) -> String {
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

    pub fn get_manfiest_url(&self) -> String {
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

    pub async fn get_manfiest(&self) -> Result<DistManifest> {
        download_dist_manfiest(&self.get_manfiest_url()).await
    }
    pub async fn get_artifact_url(&self) -> Result<Vec<(String, String)>> {
        trace!("get_artifact_url {}/{}", self.owner, self.name);
        let api = self.get_artifact_api();
        trace!("get_artifact_url api {}", api);
        let mut v = vec![];
        let local_target = get_local_target();
        if let Ok(artifacts) = download_json::<GhArtifacts>(&api).await {
            for i in artifacts.assets {
                if is_skip(&i.browser_download_url) {
                    continue;
                }
                if ends_with_exe(&i.browser_download_url)
                    && local_target.iter().any(|t| t.os() != Os::Windows)
                {
                    continue;
                }
                let filename = get_filename(&i.browser_download_url);
                let name = name_no_ext(&filename);
                let guess = guess_target(&name);
                if let Some(item) = guess.iter().find(|i| local_target.contains(&i.target)) {
                    v.push((item.rank, item.name.clone(), i.browser_download_url.clone()));
                }
            }
        }
        let max_rank = v.iter().fold(0, |pre, cur| pre.max(cur.0));
        let mut filter = vec![];
        let mut list = vec![];
        // FIXME: Need user to select eg: llrt-no-sdk llrt-full-sdk
        for (rank, name, url) in v {
            if rank < max_rank {
                continue;
            }
            if filter.contains(&name) {
                continue;
            }

            filter.push(name.clone());
            list.push((name, url));
        }
        Ok(list)
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

#[cfg(test)]
mod test {
    use crate::ty::Repo;
    #[tokio::test]
    async fn test() {
        for i in [
            "https://github.com/AlistGo/alist",
            "https://github.com/ahaoboy/bloaty-metafile.git",
        ] {
            let repo = Repo::try_from(i).unwrap();
            let v = repo.get_artifact_url().await.unwrap();
            assert_eq!(v.len(), 1);
        }
    }
}
