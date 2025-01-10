use crate::download::{create_client, download_dist_manfiest};
use crate::manfiest::{self, Artifact, DistManifest};
use crate::{artifact::Artifacts, download::download, env::get_install_dir};
use atomic_file_install::atomic_install;
use binstalk_downloader::download::{Download, ExtractedFilesEntry, PkgFmt};
use binstalk_registry::Registry;
use detect_targets::detect_targets;
use regex::Regex;
use semver::VersionReq;
use std::path::PathBuf;
use std::str::FromStr;
use std::{collections::VecDeque, fmt::Display, path::Path};
use tempfile::tempdir;
use tracing::trace;

pub async fn install(url: &str) {
    trace!("install {}", url);
    if is_dist_manfiest(url) {
        install_from_manfiest(url).await;
        return;
    }
    if is_url(url) && is_file(url) {
        install_from_artifact_url(url, None).await;
        return;
    }

    if let Ok(repo) = Repo::try_from(url) {
        install_from_github(&repo).await;
        return;
    }

    install_from_crate_name(url).await;
}

async fn install_from_crate_name(crate_name: &str) {
    trace!("install_from_crate_name {}", crate_name);
    let client = create_client().await;
    let version_req = &VersionReq::STAR;
    let sparse_registry: Registry = Registry::crates_io_sparse_registry();
    let manifest_from_sparse = sparse_registry
        .fetch_crate_matched(client, crate_name, version_req)
        .await
        .unwrap();
    if let Some(pkg) = manifest_from_sparse.package {
        if let Some(repository) = pkg.repository() {
            if let Ok(repo) = Repo::try_from(repository) {
                install_from_github(&repo).await;
            }
        }
    }
}

async fn install_from_artifact_url(url: &str, manfiest: Option<DistManifest>) {
    trace!("install_from_artifact_url {}", url);
    let fmt = PkgFmt::guess_pkg_format(url).unwrap();

    println!("download {}", url);
    let files = download(url).await;

    install_from_download_file(fmt, files, manfiest).await;
}

fn replace_filename(base_url: &str, name: &str) -> String {
    if let Some(pos) = base_url.rfind('/') {
        format!("{}{}", &base_url[..pos + 1], name)
    } else {
        name.to_string()
    }
}

async fn get_artifact_url_from_manfiest(url: &str, manfiest: &DistManifest) -> Option<String> {
    let targets = detect_targets().await;
    for (name, art) in manfiest.artifacts.iter() {
        if art.match_targets(&targets)
            && is_file(name)
            && art.kind.clone().unwrap_or("executable-zip".to_owned()) == "executable-zip"
        {
            if !is_url(name) {
                return Some(replace_filename(url, name));
            }
            return Some(name.clone());
        }
    }
    None
}

async fn install_from_manfiest(url: &str) {
    trace!("install_from_manfiest {}", url);
    let manfiest = download_dist_manfiest(url).await;
    if let Some(manfiest) = manfiest {
        if let Some(art_url) = get_artifact_url_from_manfiest(url, &manfiest).await {
            trace!("install_from_manfiest art_url {}", art_url);
            install_from_artifact_url(&art_url, Some(manfiest)).await;
            return;
        }
    }
    println!("install_from_manfiest {} failed", url);
}

fn remove_postfix(s: &str) -> String {
    use PkgFmt::*;
    for i in [Tar, Tbz2, Tgz, Txz, Tzstd, Zip, Bin] {
        for ext in i.extensions(IS_WINDOWS) {
            if !ext.is_empty() && s.ends_with(ext) {
                return s[0..s.len() - ext.len()].to_string();
            }
        }
    }
    s.to_string()
}

impl Artifact {
    fn has_file(&self, p: &str) -> bool {
        let mut p = p.to_string();
        // FIXME: The full path should be used
        // but the cargo-dist path has a prefix
        if let Some(name) = &(self.name) {
            let prefix = remove_postfix(name);
            if p.starts_with(&prefix) {
                p = p[prefix.len()..].to_string();
            }
        }

        if p.starts_with("/") || p.starts_with("\\") {
            p = p[1..].to_string();
        }

        for i in &self.assets {
            let name = PathBuf::from_str(&p).unwrap().to_str().unwrap().to_string();
            if Some(name.as_str()) == i.path.as_deref() {
                return match &i.kind {
                    manfiest::AssetKind::Executable(_) => true,
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

    fn match_targets(&self, targets: &Vec<String>) -> bool {
        for i in targets {
            if self.target_triples.contains(i) {
                return true;
            }
        }
        false
    }
}

impl DistManifest {
    fn get_artifact(self, targets: &Vec<String>) -> Option<Artifact> {
        self.artifacts.into_iter().find_map(|(name, art)| {
            if art.match_targets(targets)
                && is_file(&name)
                && art.kind.clone().unwrap_or("executable-zip".to_owned()) == "executable-zip"
            {
                return Some(art);
            }
            None
        })
    }
}

#[cfg(not(target_os = "windows"))]
fn add_execute_permission(file_path: &str) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let metadata = std::fs::metadata(file_path)?;

    let mut permissions = metadata.permissions();
    let current_mode = permissions.mode();

    let new_mode = current_mode | 0o111;
    permissions.set_mode(new_mode);

    std::fs::set_permissions(file_path, permissions)?;

    Ok(())
}

async fn install_from_download_file(
    fmt: PkgFmt,
    download: Download<'static>,
    manfiest: Option<DistManifest>,
) {
    trace!("install_from_download_file");
    let out_dir = tempdir().unwrap();
    let files = download.and_extract(fmt, &out_dir).await.unwrap();
    let install_dir = get_install_dir();
    let src_dir = out_dir.path().to_path_buf();
    let mut v = vec![];
    let mut q = VecDeque::new();
    let targets = detect_targets().await;
    q.push_back(".".to_string());
    let artifact = manfiest.and_then(|i| i.get_artifact(&targets));

    let allow = move |p: &str| -> bool {
        match &artifact {
            None => true,
            Some(art) => art.has_file(p),
        }
    };

    while let Some(top) = q.pop_front() {
        let p = Path::new(&top);
        let entry = files.get_entry(p);
        match entry {
            Some(ExtractedFilesEntry::Dir(dir)) => {
                for i in dir.iter() {
                    let p = p.join(i.to_str().unwrap());
                    let next = path_clean::clean(p.to_str().unwrap())
                        .to_str()
                        .unwrap()
                        .to_string()
                        .replace("\\", "/");
                    q.push_back(next);
                }
            }
            Some(ExtractedFilesEntry::File) => {
                if !allow(&top) {
                    continue;
                }
                let mut src = src_dir.clone();
                let mut dst = install_dir.clone();
                let name = p.file_name().unwrap().to_str().unwrap().to_string();
                src.push(&top);
                dst.push(&name);
                atomic_install(&src, dst.as_path()).unwrap();

                #[cfg(not(target_os = "windows"))]
                add_execute_permission(dst.as_path().to_str().unwrap())
                    .expect("Failed to add_execute_permission");

                v.push(format!("{} -> {}", name, dst.to_str().unwrap()));
            }
            None => {}
        }
    }
    if v.is_empty() {
        println!("No files installed");
    } else {
        println!("Installation Successful");
        println!("{}", v.join("\n"));
    }
}

#[derive(Debug, Default, Hash, PartialEq, Eq, PartialOrd, Ord)]
struct Repo {
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
    fn get_gh_url(&self) -> String {
        format!("https://github.com/{}/{}", self.owner, self.name)
    }

    fn get_artifact_api(&self) -> String {
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

    fn get_manfiest_url(&self) -> String {
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

    async fn get_manfiest(&self) -> Option<DistManifest> {
        download_dist_manfiest(&self.get_manfiest_url()).await
    }

    async fn get_artifact_url(&self) -> Vec<String> {
        trace!("get_artifact_url {}/{}", self.owner, self.name);
        let api = self.get_artifact_api();
        trace!("get_artifact_url api {}", api);

        let client = reqwest::Client::new();
        let response = client
            .get(&api)
            .header("User-Agent", "reqwest")
            .send()
            .await
            .unwrap();

        let artifacts: Artifacts = response.json().await.unwrap();

        let targets = detect_targets().await;

        let mut v = vec![];
        for i in artifacts.assets {
            for pat in &targets {
                if i.name.contains(pat) && is_file(&i.name) {
                    v.push(i.browser_download_url.clone());
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

async fn install_from_github(repo: &Repo) {
    trace!("install_from_git {}", repo);
    let artifact_url = repo.get_artifact_url().await;
    if !artifact_url.is_empty() {
        for i in artifact_url {
            trace!("install_from_git artifact_url {}", i);
            let manfiest = repo.get_manfiest().await;
            install_from_artifact_url(&i, manfiest).await;
        }
    } else {
        println!(
            "not found asset for {} on {}",
            detect_targets().await.join(","),
            repo.get_gh_url()
        );
    }
}

const IS_WINDOWS: bool = cfg!(target_os = "windows");

fn is_file(s: &str) -> bool {
    use PkgFmt::*;

    for i in [Tar, Tbz2, Tgz, Txz, Tzstd, Zip, Bin] {
        for ext in i.extensions(IS_WINDOWS) {
            if !ext.is_empty() && s.ends_with(ext) {
                return true;
            }
        }
    }

    false
}

fn is_url(s: &str) -> bool {
    s.starts_with("http://") || s.starts_with("https://")
}

fn is_dist_manfiest(s: &str) -> bool {
    s.ends_with(".json")
}

#[cfg(test)]
mod test {
    use std::path::Path;

    use binstalk_downloader::download::PkgFmt;
    use tempfile::tempdir;

    use crate::{
        download::{download, download_dist_manfiest},
        env::IS_WINDOWS,
        install::{get_artifact_url_from_manfiest, is_file, is_url, Repo},
    };

    #[test]
    fn test_is_file() {
        assert!(!is_file("https://github.com/ahaoboy/ansi2"));

        assert!(!is_file(
            "https://api.github.com/repos/ahaoboy/ansi2/releases/latest"
        ));
        assert!(!is_file(
            "https://github.com/ahaoboy/ansi2/releases/tag/v0.2.11"
        ));
        assert!(is_file("https://github.com/ahaoboy/ansi2/releases/download/v0.2.11/ansi2-x86_64-unknown-linux-musl.tar.gz"));
        assert!(is_file("https://github.com/ahaoboy/ansi2/releases/download/v0.2.11/ansi2-x86_64-pc-windows-msvc.zip"));
    }

    #[test]
    fn test_is_github() {
        let repo = Repo {
            owner: "ahaoboy".to_string(),
            name: "ansi2".to_string(),
            tag: None,
        };
        assert_eq!(
            Repo::try_from("https://github.com/ahaoboy/ansi2").unwrap(),
            repo
        );

        assert!(
            Repo::try_from("https://api.github.com/repos/ahaoboy/ansi2/releases/latest").is_err()
        );

        let repo = Repo {
            owner: "ahaoboy".to_string(),
            name: "ansi2".to_string(),
            tag: Some("v0.2.11".to_string()),
        };

        assert_eq!(
            Repo::try_from("https://github.com/ahaoboy/ansi2/releases/tag/v0.2.11").unwrap(),
            repo
        );

        // assert_eq!(
        //   Repo::try_from("https://github.com/ahaoboy/ansi2/releases/download/v0.2.11/ansi2-x86_64-unknown-linux-musl.tar.gz").unwrap(),
        //   repo
        // );

        // assert_eq!(
        //   Repo::try_from("https://github.com/ahaoboy/ansi2/releases/download/v0.2.11/ansi2-x86_64-pc-windows-msvc.zip").unwrap(),
        //   repo
        // );
    }

    #[test]
    fn test_is_url() {
        assert!(is_url("https://github.com/ahaoboy/ansi2"));
        assert!(!is_url("ansi2"));
    }

    #[tokio::test]
    async fn test_get_artifact_url() {
        let repo = Repo::try_from("https://github.com/ahaoboy/mujs-build").unwrap();
        let url = repo.get_artifact_url().await[0].clone();
        let fmt = PkgFmt::guess_pkg_format(&url).unwrap();
        let files = download(&url).await;
        let out_dir = tempdir().unwrap();
        let files = files.and_extract(fmt, out_dir.path()).await.unwrap();
        assert!(files.has_file(Path::new(if IS_WINDOWS { "mujs.exe" } else { "mujs" })));
    }

    #[tokio::test]
    async fn test_get_artifact_api() {
        let repo = Repo::try_from("https://github.com/axodotdev/cargo-dist").unwrap();
        let url = repo.get_artifact_api();
        assert_eq!(
            url,
            "https://api.github.com/repos/axodotdev/cargo-dist/releases/latest"
        )
    }
    #[tokio::test]
    async fn test_get_manfiest() {
        let repo = Repo::try_from("https://github.com/axodotdev/cargo-dist/releases").unwrap();
        let url = repo.get_manfiest_url();
        assert_eq!(
            url,
            "https://github.com/axodotdev/cargo-dist/releases/latest/download/dist-manifest.json"
        );
        assert!(repo.get_manfiest().await.is_some());

        let repo =
            Repo::try_from("https://github.com/axodotdev/cargo-dist/releases/tag/v0.25.1").unwrap();
        let url = repo.get_manfiest_url();
        assert_eq!(
            url,
            "https://github.com/axodotdev/cargo-dist/releases/download/v0.25.1/dist-manifest.json"
        );

        let manfiest = repo.get_manfiest().await.unwrap();
        assert!(!manfiest.artifacts.is_empty());

        let repo =
            Repo::try_from("https://github.com/ahaoboy/mujs-build/releases/tag/v0.0.2").unwrap();
        let url = repo.get_manfiest_url();
        assert_eq!(
            url,
            "https://github.com/ahaoboy/mujs-build/releases/download/v0.0.2/dist-manifest.json"
        );

        let manfiest = repo.get_manfiest().await.unwrap();
        assert!(!manfiest.artifacts.is_empty())
    }

    #[tokio::test]
    async fn test_manifest_jsc() {
        let repo = Repo {
            owner: "ahaoboy".to_string(),
            name: "jsc-build".to_string(),
            tag: None,
        };

        let manifest = repo.get_manfiest().await.unwrap();
        let art = manifest
            .get_artifact(&vec!["x86_64-unknown-linux-gnu".to_string()])
            .unwrap();

        assert!(art.has_file("bin/jsc"));
        assert!(art.has_file("lib/libJavaScriptCore.a"));
        assert!(!art.has_file("lib/jsc"));
    }

    #[tokio::test]
    async fn test_manifest_mujs() {
        let repo = Repo {
            owner: "ahaoboy".to_string(),
            name: "mujs-build".to_string(),
            tag: None,
        };

        let manifest = repo.get_manfiest().await.unwrap();
        let art = manifest
            .get_artifact(&vec!["x86_64-unknown-linux-gnu".to_string()])
            .unwrap();

        assert!(art.has_file("mujs"));
        assert!(!art.has_file("mujs.exe"));

        let manifest = repo.get_manfiest().await.unwrap();
        let art = manifest
            .get_artifact(&vec!["x86_64-pc-windows-gnu".to_string()])
            .unwrap();

        assert!(!art.has_file("mujs"));
        assert!(art.has_file("mujs.exe"));
    }

    #[tokio::test]
    async fn test_install_from_manfiest() {
        let url =
            "https://github.com/ahaoboy/mujs-build/releases/latest/download/dist-manifest.json";
        let manfiest = download_dist_manfiest(url).await.unwrap();
        let art_url = get_artifact_url_from_manfiest(url, &manfiest).await;
        assert!(art_url.is_some())
    }

    #[tokio::test]
    async fn test_cargo_dist() {
        let url =
            "https://github.com/axodotdev/cargo-dist/releases/download/v1.0.0-rc.1/dist-manifest.json";
        let manfiest = download_dist_manfiest(url).await.unwrap();
        let art_url = get_artifact_url_from_manfiest(url, &manfiest).await;
        assert!(art_url.is_some());
    }

    #[tokio::test]
    async fn test_deno() {
        let url = "https://github.com/denoland/deno";
        let repo = Repo::try_from(url).unwrap();
        let artifact_url = repo.get_artifact_url().await;
        assert_eq!(artifact_url.len(), 2);
    }
}
