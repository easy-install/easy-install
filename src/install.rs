use crate::download::{create_client, download_binary, download_dist_manfiest, read_dist_manfiest};
use crate::manfiest::{self, Artifact, Asset, DistManifest};
use crate::tool::{display_output, get_meta};
use crate::{artifact::Artifacts, download::download_files, env::get_install_dir};
use binstalk_downloader::download::{Download, ExtractedFilesEntry, PkgFmt};
use binstalk_registry::Registry;
use detect_targets::detect_targets;
use regex::Regex;
use semver::VersionReq;
use std::collections::HashMap;
use std::path::PathBuf;
use std::str::FromStr;
use std::{collections::VecDeque, fmt::Display, path::Path};
use tempfile::tempdir;
use tracing::trace;

pub struct OutputItem {
    pub download_url: String,
    pub install_dir: String,
    pub install_path: String,
    pub mode: u32,
    pub size: u32,
    pub origin_path: String,
    pub is_dir: bool,
}

pub type Output = HashMap<String, Vec<OutputItem>>;

pub fn atomic_install(src: &Path, dst: &Path) -> std::io::Result<u64> {
    std::fs::copy(src, dst)
}

pub async fn install(url: &str, dir: Option<String>) -> Output {
    trace!("install {}", url);
    if is_dist_manfiest(url) {
        return install_from_manfiest(url, dir).await;
    }
    if is_url(url) && is_archive_file(url) {
        return install_from_artifact_url(url, None, dir).await;
    }

    if let Ok(repo) = Repo::try_from(url) {
        return install_from_github(&repo, dir).await;
    }

    install_from_crate_name(url, dir).await
}

async fn install_from_crate_name(crate_name: &str, dir: Option<String>) -> Output {
    trace!("install_from_crate_name {}", crate_name);
    let client = create_client().await;
    let version_req = &VersionReq::STAR;
    let sparse_registry: Registry = Registry::crates_io_sparse_registry();
    let manifest_from_sparse = sparse_registry
        .fetch_crate_matched(client, crate_name, version_req)
        .await
        .unwrap();
    let mut v = Output::new();
    if let Some(pkg) = manifest_from_sparse.package {
        if let Some(repository) = pkg.repository() {
            if let Ok(repo) = Repo::try_from(repository) {
                v.extend(install_from_github(&repo, dir).await);
            }
        }
    }
    v
}
async fn get_artifact_download_url(art_url: &str) -> Vec<String> {
    if !art_url.contains("*") {
        return vec![art_url.to_string()];
    }

    if let Ok(repo) = Repo::try_from(art_url) {
        return repo.match_artifact_url(art_url).await;
    }
    vec![]
}
async fn install_from_single_file(
    url: &str,
    manfiest: Option<DistManifest>,
    dir: Option<String>,
) -> Output {
    // let targets = detect_targets().await;
    let mut install_dir = get_install_dir();
    let mut output = Output::new();
    if let Some(target_dir) = dir {
        if target_dir.contains("/") || target_dir.contains("\\") {
            install_dir = target_dir.into();
        } else {
            install_dir.push(target_dir);
        }
    }

    if let (Some(artifact), Some(bin)) = (
        manfiest.and_then(|i| i.get_artifact_by_key(url)),
        download_binary(url).await,
    ) {
        let art_name = url
            .split("/")
            .last()
            .map(|i| i.to_string())
            .expect("can't get artifact name");
        let name = artifact.name.unwrap_or(art_name);
        let mut install_path = install_dir.clone();
        install_path.push(&name);

        if let Some(dir) = install_path.parent() {
            std::fs::create_dir_all(dir).expect("Failed to create_dir dir");
        }
        std::fs::write(&install_path, &bin).expect("write file failed");
        let (mode, size, is_dir) = get_meta(&install_path);
        let install_path = install_path.to_str().unwrap().replace("\\", "/");
        let install_dir = install_dir.to_str().unwrap().replace("\\", "/");
        println!("Installation Successful");
        let item = vec![OutputItem {
            download_url: url.to_string(),
            install_dir,
            install_path,
            mode,
            size,
            origin_path: name,
            is_dir,
        }];
        output.insert(url.to_string(), item);
    } else {
        println!("not found/download artifact for {url}")
    }
    output
}

async fn install_from_artifact_url(
    art_url: &str,
    manfiest: Option<DistManifest>,
    dir: Option<String>,
) -> Output {
    trace!("install_from_artifact_url {}", art_url);
    let urls = get_artifact_download_url(art_url).await;
    let mut v = Output::new();
    if urls.is_empty() {
        println!("not found download_url for {art_url}");
        return v;
    }
    if urls.len() == 1 && !is_archive_file(&urls[0]) {
        println!("download {}", urls[0]);
        let output = install_from_single_file(&urls[0], manfiest.clone(), dir.clone()).await;
        println!("{}", display_output(&output));
        return output;
    }
    for url in urls {
        println!("download {}", url);
        let files = download_files(&url).await;
        let fmt = PkgFmt::guess_pkg_format(art_url).unwrap();
        let output =
            install_from_download_file(&url, fmt, files, manfiest.clone(), dir.clone()).await;
        // println!("{}", display_output(&output));
        v.extend(output);
    }
    v
}

fn replace_filename(base_url: &str, name: &str) -> String {
    if let Some(pos) = base_url.rfind('/') {
        format!("{}{}", &base_url[..pos + 1], name)
    } else {
        name.to_string()
    }
}

async fn get_artifact_url_from_manfiest(url: &str, manfiest: &DistManifest) -> Vec<String> {
    let targets = detect_targets().await;
    let mut v = vec![];
    for (name, art) in manfiest.artifacts.iter() {
        if art.match_targets(&targets)
            // && is_archive_file(name)
            && art.kind.clone().unwrap_or("executable-zip".to_owned()) == "executable-zip"
        {
            if !is_url(name) {
                v.push(replace_filename(url, name));
            } else {
                v.push(name.clone());
            }
        }
    }
    v
}

async fn install_from_manfiest(url: &str, dir: Option<String>) -> Output {
    trace!("install_from_manfiest {}", url);
    let manfiest = if is_url(url) {
        download_dist_manfiest(url).await
    } else {
        read_dist_manfiest(url)
    };

    let mut v = Output::new();
    if let Some(manfiest) = manfiest {
        let art_url_list = get_artifact_url_from_manfiest(url, &manfiest).await;
        if art_url_list.is_empty() {
            println!("install_from_manfiest {} failed", url);
            return v;
        }
        for art_url in art_url_list {
            trace!("install_from_manfiest art_url {}", art_url);
            v.extend(
                install_from_artifact_url(&art_url, Some(manfiest.clone()), dir.clone()).await,
            );
        }
    }
    v
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

    fn match_targets(&self, targets: &Vec<String>) -> bool {
        for i in targets {
            if self.target_triples.contains(i) {
                return true;
            }
        }
        false
    }

    fn get_assets_executable_dir(&self) -> Option<Asset> {
        for i in self.assets.clone() {
            if let manfiest::AssetKind::ExecutableDir(_) = i.kind {
                return Some(i);
            }
        }
        None
    }

    fn get_asset(&self, path: &str) -> Option<Asset> {
        self.assets.clone().into_iter().find_map(|i| {
            if i.path == Some(path.to_owned()) {
                return Some(i);
            }
            None
        })
    }
}

impl DistManifest {
    fn get_artifact(&self, targets: &Vec<String>) -> Option<Artifact> {
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

    fn get_artifact_by_key(&self, key: &str) -> Option<Artifact> {
        self.artifacts.get(key).cloned()
    }
}

#[cfg(unix)]
pub(crate) fn add_execute_permission(file_path: &str) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let metadata = std::fs::metadata(file_path)?;
    if metadata.is_dir() {
        return Ok(());
    }

    let mut permissions = metadata.permissions();
    let current_mode = permissions.mode();

    let new_mode = current_mode | 0o111;
    permissions.set_mode(new_mode);

    std::fs::set_permissions(file_path, permissions)?;

    Ok(())
}

async fn install_from_download_file(
    url: &str,
    fmt: PkgFmt,
    download: Download<'static>,
    manfiest: Option<DistManifest>,
    dir: Option<String>,
) -> Output {
    trace!("install_from_download_file");
    let out_dir = tempdir().unwrap();
    let mut install_dir = get_install_dir();
    let src_dir = out_dir.path().to_path_buf();
    let mut v = vec![];
    let mut q = VecDeque::new();
    let targets = detect_targets().await;
    let artifact = manfiest.and_then(|i| i.get_artifact(&targets));
    let mut output = Output::new();
    if let Some(asset) = artifact.clone().and_then(|a| a.get_assets_executable_dir()) {
        if let Some(target_dir) = dir.or(asset.name) {
            if target_dir.contains("/") || target_dir.contains("\\") {
                install_dir = target_dir.into();
            } else {
                install_dir.push(target_dir);
            }

            let prefix = asset.path.unwrap_or(".".to_string());
            q.push_back(prefix.clone());
            let files = download.and_extract(fmt, &out_dir).await.unwrap();
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
                        let mut src = src_dir.clone();
                        let mut dst = install_dir.clone();
                        src.push(&top);
                        dst.push(top.replace(&(prefix.clone() + "/"), ""));

                        if let Some(dst_dir) = dst.parent() {
                            if dst_dir.exists() && dst_dir.is_file() {
                                std::fs::remove_file(dst_dir).unwrap_or_else(|_| {
                                    panic!("failed to remove file : {:?}", dst_dir)
                                });
                                println!("remove {:?}", dst_dir);
                            }
                            if !dst_dir.exists() {
                                std::fs::create_dir_all(dst_dir)
                                    .expect("Failed to create_dir install_dir");
                            }
                        }

                        atomic_install(&src, dst.as_path()).unwrap_or_else(|_| {
                            panic!("failed to atomic_install from {:?} to {:?}", src, dst)
                        });

                        let (mode, size, is_dir) = get_meta(&dst);
                        v.push(OutputItem {
                            download_url: url.to_string(),
                            install_dir: install_dir
                                .to_string_lossy()
                                .to_string()
                                .replace("\\", "/"),
                            install_path: dst.to_string_lossy().to_string().replace("\\", "/"),
                            mode,
                            size,
                            origin_path: top,
                            is_dir,
                        });
                    }
                    None => {}
                }
            }
            if v.is_empty() {
                println!("No files installed");
            } else {
                println!("Installation Successful");
                output.insert(url.to_string(), v);
                println!("{}", display_output(&output));
            }
        } else {
            println!("Maybe you should use -d to set the folder");
        }
    } else {
        if let Some(target_dir) = dir {
            if target_dir.contains("/") || target_dir.contains("\\") {
                install_dir = target_dir.into();
            } else {
                install_dir.push(target_dir);
            }
        }

        q.push_back(".".to_string());
        let allow = |p: &str| -> bool {
            match artifact.clone() {
                None => true,
                Some(art) => art.has_file(p),
            }
        };
        let files = download.and_extract(fmt, &out_dir).await.unwrap();
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

                    let file_name = p.file_name().unwrap().to_str().unwrap().to_string();
                    let name = artifact
                        .clone()
                        .and_then(|a| {
                            a.get_asset(p.to_str().unwrap())
                                .and_then(|i| i.executable_name)
                        })
                        .unwrap_or(file_name.clone());

                    src.push(&top);
                    dst.push(&name);
                    atomic_install(&src, dst.as_path()).unwrap();
                    let (mode, size, is_dir) = get_meta(&dst);
                    v.push(OutputItem {
                        download_url: url.to_string(),
                        install_dir: install_dir.to_string_lossy().to_string().replace("\\", "/"),
                        install_path: dst.to_string_lossy().to_string().replace("\\", "/"),
                        mode,
                        size,
                        origin_path: top,
                        is_dir,
                    });
                }
                None => {}
            }
        }
        if v.is_empty() {
            println!("No files installed");
        } else {
            println!("Installation Successful");
            output.insert(url.to_string(), v);
            println!("{}", display_output(&output));
        }
    }

    output
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

        v
    }

    async fn match_artifact_url(&self, pattern: &str) -> Vec<String> {
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

        let mut v = vec![];
        let re = Regex::new(pattern).unwrap();
        let pattern_name = pattern.split("/").last();
        let name_re = pattern_name.map(|i| Regex::new(i).unwrap());

        for art in artifacts.assets {
            if !is_hash_file(&art.browser_download_url)
                && !is_msi_file(&art.browser_download_url)
                && (re.is_match(&art.browser_download_url)
                    || name_re.clone().map(|r| r.is_match(&art.name)) == Some(true))
            {
                v.push(art.browser_download_url);
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

async fn install_from_github(repo: &Repo, dir: Option<String>) -> Output {
    trace!("install_from_git {}", repo);
    let artifact_url = repo.get_artifact_url().await;
    let mut v = Output::new();
    if !artifact_url.is_empty() {
        for i in artifact_url {
            trace!("install_from_git artifact_url {}", i);
            let manfiest = repo.get_manfiest().await;
            v.extend(install_from_artifact_url(&i, manfiest, dir.clone()).await);
        }
    } else {
        println!(
            "not found asset for {} on {}",
            detect_targets().await.join(","),
            repo.get_gh_url()
        );
    }
    v
}

const IS_WINDOWS: bool = cfg!(target_os = "windows");

fn is_archive_file(s: &str) -> bool {
    use PkgFmt::*;

    for i in [
        Tar, Tbz2, Tgz, Txz, Tzstd, Zip,
        // Bin
    ] {
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

fn is_hash_file(s: &str) -> bool {
    s.ends_with(".sha256")
}

fn is_msi_file(s: &str) -> bool {
    s.ends_with(".msi")
}

#[cfg(test)]
mod test {
    use std::path::Path;

    use binstalk_downloader::download::PkgFmt;
    use tempfile::tempdir;

    use crate::{
        download::{download_dist_manfiest, download_files, read_dist_manfiest},
        env::IS_WINDOWS,
        install::{
            get_artifact_download_url, get_artifact_url_from_manfiest, is_archive_file, is_url,
            Repo,
        },
    };

    #[test]
    fn test_is_file() {
        assert!(!is_archive_file("https://github.com/ahaoboy/ansi2"));

        assert!(!is_archive_file(
            "https://api.github.com/repos/ahaoboy/ansi2/releases/latest"
        ));
        assert!(!is_archive_file(
            "https://github.com/ahaoboy/ansi2/releases/tag/v0.2.11"
        ));
        assert!(is_archive_file("https://github.com/ahaoboy/ansi2/releases/download/v0.2.11/ansi2-x86_64-unknown-linux-musl.tar.gz"));
        assert!(is_archive_file("https://github.com/ahaoboy/ansi2/releases/download/v0.2.11/ansi2-x86_64-pc-windows-msvc.zip"));
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

        assert_eq!(
          Repo::try_from("https://github.com/ahaoboy/ansi2/releases/download/v0.2.11/ansi2-x86_64-unknown-linux-musl.tar.gz").unwrap(),
          repo
        );

        assert_eq!(
          Repo::try_from("https://github.com/ahaoboy/ansi2/releases/download/v0.2.11/ansi2-x86_64-pc-windows-msvc.zip").unwrap(),
          repo
        );

        let repo = Repo {
            owner: "Ryubing".to_string(),
            name: "Ryujinx".to_string(),
            tag: Some("1.2.78".to_string()),
        };
        assert_eq!(
          Repo::try_from("https://github.com/Ryubing/Ryujinx/releases/download/1.2.78/ryujinx-*.*.*-win_x64.zip").unwrap(),
          repo
        );
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
        let files = download_files(&url).await;
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
        assert!(!art_url.is_empty())
    }

    #[tokio::test]
    async fn test_cargo_dist() {
        let url =
            "https://github.com/axodotdev/cargo-dist/releases/download/v1.0.0-rc.1/dist-manifest.json";
        let manfiest = download_dist_manfiest(url).await.unwrap();
        let art_url = get_artifact_url_from_manfiest(url, &manfiest).await;
        assert!(!art_url.is_empty())
    }

    #[tokio::test]
    async fn test_deno() {
        let url = "https://github.com/denoland/deno";
        let repo = Repo::try_from(url).unwrap();
        let artifact_url = repo.get_artifact_url().await;
        assert_eq!(artifact_url.len(), 2);
    }

    #[tokio::test]
    async fn test_get_artifact_download_url() {
        for url in [
        "https://github.com/Ryubing/Ryujinx/releases/latest/download/^ryujinx-*.*.*-win_x64.zip",
        "https://github.com/Ryubing/Ryujinx/releases/download/1.2.80/ryujinx-*.*.*-win_x64.zip",
        "https://github.com/Ryubing/Ryujinx/releases/download/1.2.78/ryujinx-*.*.*-win_x64.zip",
        "https://github.com/shinchiro/mpv-winbuild-cmake/releases/latest/download/^mpv-x86_64-v3-.*?-git-.*?",
        "https://github.com/NickeManarin/ScreenToGif/releases/latest/download/ScreenToGif.[0-9]*.[0-9]*.[0-9]*.Portable.x64.zip",
        "https://github.com/ip7z/7zip/releases/latest/download/7z.*?-linux-x64.tar.xz",
        "https://github.com/mpv-easy/mpv-winbuild/releases/latest/download/mpv-x86_64-v3-.*?-git-.*?.zip",
      ]{
          let art_url = get_artifact_download_url(url).await;
          assert_eq!(art_url.len(), 1);
      }
    }

    #[tokio::test]
    async fn test_starship() {
        let repo = Repo::try_from("https://github.com/starship/starship").unwrap();
        let artifact_url = repo.get_artifact_url().await;
        assert_eq!(artifact_url.len(), 1);
    }

    #[tokio::test]
    async fn test_quickjs_ng() {
        let json = "./dist-manifest/quickjs-ng.json";
        let manifest = read_dist_manfiest(json).unwrap();
        let urls = get_artifact_url_from_manfiest(json, &manifest).await;
        assert_eq!(urls.len(), 2);

        for i in urls {
            let download_urls = get_artifact_download_url(&i).await;
            assert_eq!(download_urls.len(), 1);
        }
    }

    #[tokio::test]
    async fn test_graaljs() {
        let json = "./dist-manifest/graaljs.json";
        let manifest = read_dist_manfiest(json).unwrap();
        let urls = get_artifact_url_from_manfiest(json, &manifest).await;
        assert_eq!(urls.len(), 1);

        for i in urls {
            let download_urls = get_artifact_download_url(&i).await;
            assert_eq!(download_urls.len(), 1);
        }
    }
}
