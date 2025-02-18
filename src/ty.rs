use crate::artifact::Artifacts;
use crate::download::{download_dist_manfiest, download_json};
use crate::manfiest::{self, Artifact, Asset, DistManifest};
use crate::tool::{is_archive_file, is_hash_file, is_msi_file, remove_postfix};
use regex::Regex;
use std::collections::HashMap;
use std::fmt::Display;
use std::path::PathBuf;
use std::str::FromStr;
use tracing::trace;

#[derive(Debug, Clone, PartialEq, PartialOrd, Default)]
pub struct OutputFile {
    pub install_path: String,
    pub mode: u32,
    pub size: u32,
    pub origin_path: String,
    pub is_dir: bool,
}
#[derive(Debug, Clone, PartialEq, Default)]
pub struct OutputItem {
    pub install_dir: String,
    pub bin_dir: String,
    pub files: Vec<OutputFile>,
}

pub type Output = HashMap<String, OutputItem>;

impl Artifact {
    pub fn has_file(&self, p: &str) -> bool {
        let mut p = p.to_string().replace("\\", "/");
        // FIXME: The full path should be used
        // but the cargo-dist path has a prefix
        if let Some(name) = &(self.name) {
            let prefix = remove_postfix(name) + "/";
            if p.starts_with(&prefix) {
                p = p[prefix.len()..].to_string();
            }
        }

        for i in &self.assets {
            let name = PathBuf::from_str(&p).unwrap().to_str().unwrap().to_string();
            if i.path.clone().unwrap_or_default() == "*" {
                return true;
            }
            if Some(name.as_str()) == i.path.as_deref() {
                return match &i.kind {
                    manfiest::AssetKind::Executable(_) => true,
                    manfiest::AssetKind::ExecutableDir(_) => false,
                    manfiest::AssetKind::CDynamicLibrary(_) => true,
                    manfiest::AssetKind::CStaticLibrary(_) => true,
                    manfiest::AssetKind::Readme => false,
                    manfiest::AssetKind::License => false,
                    manfiest::AssetKind::Changelog => false,
                    manfiest::AssetKind::Unknown => false,
                };
            }
        }
        false
    }

    pub fn match_targets(&self, targets: &Vec<String>) -> bool {
        for i in targets {
            if self.target_triples.contains(i) {
                return true;
            }
        }
        false
    }

    pub fn get_assets_executable_dir(&self) -> Option<Asset> {
        for i in self.assets.clone() {
            if let manfiest::AssetKind::ExecutableDir(_) = i.kind {
                return Some(i);
            }
        }
        None
    }

    pub fn get_asset(&self, path: &str) -> Option<Asset> {
        self.assets.clone().into_iter().find_map(|i| {
            if i.path == Some(path.to_owned()) {
                return Some(i);
            }
            None
        })
    }
}

impl DistManifest {
    pub fn get_artifact(&self, targets: &Vec<String>) -> Option<Artifact> {
        self.artifacts.clone().into_iter().find_map(|(_, art)| {
            if art.match_targets(targets)
              // && is_archive_file(&name)
              && art.kind.clone().unwrap_or("executable-zip".to_owned()) == "executable-zip"
            {
                return Some(art);
            }
            None
        })
    }

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
    pub async fn get_artifact_url(&self, targets: Vec<String>) -> Vec<String> {
        trace!("get_artifact_url {}/{}", self.owner, self.name);
        let api = self.get_artifact_api();
        trace!("get_artifact_url api {}", api);
        let mut v = vec![];
        if let Some(artifacts) = download_json::<Artifacts>(&api).await {
            let mut filter = vec![];
            for i in artifacts.assets {
                for pat in &targets {
                    let remove_target = i.name.replace(pat, "");
                    if i.name.contains(pat)
                        && is_archive_file(&i.name)
                        && !filter.contains(&remove_target)
                    {
                        v.push(i.browser_download_url.clone());
                        filter.push(remove_target)
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
        if let Some(artifacts) = download_json::<Artifacts>(&api).await {
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

#[cfg(test)]
mod test {
    use crate::ty::Repo;

    #[tokio::test]
    async fn test_api() {
        let repo = Repo::try_from("https://github.com/ahaoboy/jsc-build").unwrap();
        let artifact_url = repo
            .get_artifact_url(vec![
                "x86_64-unknown-linux-gnu".to_string(),
                "x86_64-unknown-linux-musl".to_string(),
            ])
            .await;
        assert_eq!(artifact_url.len(), 1);
    }
}
