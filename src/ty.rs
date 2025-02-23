use crate::artifact::GhArtifacts;
use crate::download::{download_dist_manfiest, download_json};
use crate::manfiest::{Artifact, DistManifest};
use crate::rule::match_name;
use crate::tool::{is_hash_file, is_msi_file};
use is_musl::is_musl;
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

// impl Artifact {
//     pub fn match_targets(&self, targets: &Vec<String>) -> bool {
//         for i in targets {
//             if self.target_triples.contains(i) {
//                 return true;
//             }
//         }
//         false
//     }
// }

impl DistManifest {
    pub fn get_artifact_by_key(&self, key: &str) -> Option<Artifact> {
        self.artifacts.get(key).cloned()
    }
}

#[derive(Debug, Default, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct Repo {
    pub owner: String,
    pub name: String,
    pub tag: Option<String>,
}

impl TryFrom<&str> for Repo {
    type Error = ();

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        trace!("get_artifact_api {}", value);
        let re_gh_tag = Regex::new(
            r"https?://github\.com/(?P<owner>[^/]+)/(?P<repo>[^/]+)/releases/tag/(?P<tag>[^/]+)",
        )
        .unwrap();

        let re_gh_download_tag = Regex::new(r"https?://github\.com/(?P<owner>[^/]+)/(?P<repo>[^/]+)/releases/download/(?P<tag>[^/]+)/(?P<filename>.+)").unwrap();

        let re_gh_releases =
            Regex::new(r"http?s://github\.com/(?P<owner>[^/]+)/(?P<repo>[^/]+)").unwrap();

        if let Some(captures) = re_gh_tag.captures(value) {
            if let (Some(owner), Some(name), Some(tag)) = (
                captures.name("owner"),
                captures.name("repo"),
                captures.name("tag"),
            ) {
                return Ok(Repo {
                    owner: owner.as_str().to_string(),
                    name: name.as_str().to_string(),
                    tag: Some(tag.as_str().to_string()),
                });
            }
        }

        if let Some(captures) = re_gh_download_tag.captures(value) {
            if let (Some(owner), Some(name), Some(tag)) = (
                captures.name("owner"),
                captures.name("repo"),
                captures.name("tag"),
            ) {
                return Ok(Repo {
                    owner: owner.as_str().to_string(),
                    name: name.as_str().to_string(),
                    tag: Some(tag.as_str().to_string()),
                });
            }
        }

        if let Some(captures) = re_gh_releases.captures(value) {
            if let (Some(owner), Some(name)) = (captures.name("owner"), captures.name("repo")) {
                return Ok(Repo {
                    owner: owner.as_str().to_string(),
                    name: name.as_str().to_string(),
                    tag: None,
                });
            }
        }
        Err(())
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

    pub async fn get_manfiest(&self) -> Option<DistManifest> {
        download_dist_manfiest(&self.get_manfiest_url()).await
    }
    pub async fn get_artifact_url(&self) -> Vec<String> {
        trace!("get_artifact_url {}/{}", self.owner, self.name);
        let api = self.get_artifact_api();
        trace!("get_artifact_url api {}", api);
        let mut v = vec![];
        let os = std::env::consts::OS;
        let arch = std::env::consts::ARCH;
        let musl = is_musl();
        if let Some(artifacts) = download_json::<GhArtifacts>(&api).await {
            let mut filter = vec![];
            for i in artifacts.assets {
                if let Some(name) = match_name(&i.name, None, os, arch, musl) {
                    if !filter.contains(&name) {
                        v.push(i.browser_download_url.clone());
                        filter.push(name)
                    }
                }
            }
        }
        v
    }

    pub async fn match_artifact_url(&self, pattern: &str) -> Vec<String> {
        trace!("get_artifact_url {}/{}", self.owner, self.name);
        let api = self.get_artifact_api();
        trace!("get_artifact_url api {}", api);

        let mut v = vec![];
        let re = Regex::new(pattern).unwrap();
        let pattern_name = pattern.split("/").last();
        let name_re = pattern_name.map(|i| Regex::new(i).unwrap());
        if let Some(artifacts) = download_json::<GhArtifacts>(&api).await {
            for art in artifacts.assets {
                if !is_hash_file(&art.browser_download_url)
                    && !is_msi_file(&art.browser_download_url)
                    && (re.is_match(&art.browser_download_url)
                        || name_re.clone().map(|r| r.is_match(&art.name)) == Some(true))
                {
                    v.push(art.browser_download_url);
                }
            }
        }
        v
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
