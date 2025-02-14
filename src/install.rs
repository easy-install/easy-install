use crate::download::{
    create_client, download_binary, download_dist_manfiest, download_extract, download_json,
    read_dist_manfiest,
};
use crate::manfiest::{self, Artifact, Asset, DistManifest};
use crate::tool::{display_output, get_bin_name, get_filename, get_meta};
use crate::{artifact::Artifacts, env::get_install_dir};
use binstalk::manifests::cargo_toml_binstall::PkgFmt;
use binstalk_registry::Registry;
use detect_targets::detect_targets;
use regex::Regex;
use semver::VersionReq;
use std::collections::HashMap;
#[cfg(unix)]
use std::os::unix::prelude::PermissionsExt;
use std::path::PathBuf;
use std::str::FromStr;
use std::{fmt::Display, path::Path};
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

pub fn atomic_install(src: &Path, dst: &Path) -> std::io::Result<u64> {
    std::fs::copy(src, dst)
}

pub fn write_to_file(src: &str, buffer: &[u8], mode: Option<u32>) {
    let Ok(d) = std::path::PathBuf::from_str(src);
    if let Some(p) = d.parent() {
        std::fs::create_dir_all(p).expect("failed to create_dir_all");
    }

    std::fs::write(src, buffer).expect("failed to write file");

    #[cfg(unix)]
    if let Some(mode) = mode {
        std::fs::set_permissions(src, PermissionsExt::from_mode(mode)).expect("failed to set_permissions");
    }

    #[cfg(windows)]
    {
        _ = mode;
    }
}

pub async fn install(url: &str, dir: Option<String>) -> Output {
    trace!("install {}", url);
    if is_dist_manfiest(url) {
        return install_from_manfiest(url, dir).await;
    }
    if is_url(url) {
        if is_archive_file(url) {
            return install_from_artifact_url(url, None, dir).await;
        }

        if is_exe_file(url) {
            return install_from_single_file(url, None, dir).await;
        }
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

fn path_to_str(p: &Path) -> String {
    p.to_str().unwrap().replace("\\", "/")
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

    if let Some(bin) = download_binary(url).await {
        let artifact = manfiest.and_then(|i| i.get_artifact_by_key(url));

        let art_name = url
            .split("/")
            .last()
            .map(|i| i.to_string())
            .expect("can't get artifact name");
        let name = artifact.and_then(|i| i.name).unwrap_or(art_name);
        let mut install_path = install_dir.clone();
        install_path.push(get_bin_name(&name));

        if let Some(dir) = install_path.parent() {
            std::fs::create_dir_all(dir).expect("Failed to create_dir dir");
        }
        std::fs::write(&install_path, &bin).expect("write file failed");
        let (mode, size, is_dir) = get_meta(&install_path);
        let install_path = path_to_str(&install_path);
        println!("Installation Successful");
        let origin_path = url.split("/").last().unwrap_or(name.as_str()).to_string();

        let files = vec![OutputFile {
            mode,
            size,
            origin_path,
            is_dir,
            install_path,
        }];

        let bin_dir_str = path_to_str(&install_dir);
        let item = OutputItem {
            install_dir: bin_dir_str.clone(),
            bin_dir: bin_dir_str.clone(),
            files,
        };

        output.insert(url.to_string(), item);
        println!("{}", display_output(&output));
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
        return output;
    }
    for url in urls {
        println!("download {}", url);
        let output = install_from_download_file(&url, manfiest.clone(), dir.clone()).await;
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
    manfiest: Option<DistManifest>,
    dir: Option<String>,
) -> Output {
    trace!("install_from_download_file");
    let mut install_dir = get_install_dir();
    let mut v: OutputItem = Default::default();
    let mut files: Vec<OutputFile> = vec![];
    let targets = detect_targets().await;
    let artifact = manfiest.and_then(|i| i.get_artifact(&targets));
    let mut output = Output::new();
    if let Some(asset) = artifact.clone().and_then(|a| a.get_assets_executable_dir()) {
        if let Some(target_dir) = dir.clone().or(asset.name) {
            if target_dir.contains("/") || target_dir.contains("\\") {
                install_dir = target_dir.into();
            } else {
                install_dir.push(target_dir);
            }

            let prefix = asset.path.unwrap_or("".to_string());

            let install_dir_str = path_to_str(&install_dir);

            let mut bin_dir = install_dir.clone();
            if let Some(ref dir) = asset.executable_dir {
                bin_dir.push(dir);
            }
            let bin_dir_str = path_to_str(&bin_dir);
            v.bin_dir = bin_dir_str;
            v.install_dir = install_dir_str;

            if let Some(download_files) = download_extract(url).await {
                for (entry_path, entry) in download_files {
                    let size = entry.buffer.len() as u32;
                    let is_dir = entry.is_dir;
                    if is_dir {
                        continue;
                    }
                    let mut dst = install_dir.clone();
                    dst.push(entry_path.replace(&(prefix.clone() + "/"), ""));

                    // FIXME: remove same name file
                    // if let Some(dst_dir) = dst.parent() {
                    //     if dst_dir.exists() && dst_dir.is_file() {
                    //         std::fs::remove_file(dst_dir).unwrap_or_else(|_| {
                    //             panic!("failed to remove file : {:?}", dst_dir)
                    //         });
                    //         println!("remove {:?}", dst_dir);
                    //     }
                    //     if !dst_dir.exists() {
                    //         std::fs::create_dir_all(dst_dir)
                    //             .expect("Failed to create_dir install_dir");
                    //     }
                    // }

                    // atomic_install(&src, dst.as_path()).unwrap_or_else(|_| {
                    //     panic!("failed to atomic_install from {:?} to {:?}", src, dst)
                    // });
                    write_to_file(dst.to_string_lossy().as_ref(), &entry.buffer, entry.mode);
                    let mode = entry.mode.unwrap_or(get_meta(&dst).0);

                    files.push(OutputFile {
                        install_path: path_to_str(&dst),
                        mode,
                        size,
                        origin_path: entry_path,
                        is_dir,
                    });
                }

                v.files = files;
                if !v.files.is_empty() {
                    println!("Installation Successful");
                    output.insert(url.to_string(), v);
                    println!("{}", display_output(&output));
                }
            }
        } else {
            println!("Maybe you should use -d to set the folder");
        }
    } else {
        if let Some(ref target_dir) = dir {
            if target_dir.contains("/") || target_dir.contains("\\") {
                install_dir = target_dir.into();
            } else {
                install_dir.push(target_dir);
            }
        }
        let install_dir_str = path_to_str(&install_dir);

        v.bin_dir = install_dir_str.clone();
        v.install_dir = install_dir_str;

        let allow = |p: &str| -> bool {
            match artifact.clone() {
                None => true,
                Some(art) => art.has_file(p),
            }
        };
        if let Some(download_files) = download_extract(url).await {
            for (entry_path, entry) in download_files {
                let size = entry.buffer.len() as u32;
                let is_dir = entry.is_dir;
                if is_dir || !allow(&entry_path) {
                    continue;
                }

                let mut dst = install_dir.clone();

                let file_name = get_filename(&entry_path).expect("failed to get filename");
                let name = artifact
                    .clone()
                    .and_then(|a| a.get_asset(&entry_path).and_then(|i| i.executable_name))
                    .unwrap_or(file_name.clone());

                dst.push(get_bin_name(&name));
                write_to_file(dst.to_string_lossy().as_ref(), &entry.buffer, entry.mode);
                let mode = entry.mode.unwrap_or(get_meta(&dst).0);
                files.push(OutputFile {
                    install_path: path_to_str(&dst),
                    mode,
                    size,
                    origin_path: entry_path,
                    is_dir,
                });
            }
            v.files = files;
            if !v.files.is_empty() {
                println!("Installation Successful");
                output.insert(url.to_string(), v);
                println!("{}", display_output(&output));
            }
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
        let mut v = vec![];
        if let Some(artifacts) = download_json::<Artifacts>(&api).await {
            let targets = detect_targets().await;
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

    async fn match_artifact_url(&self, pattern: &str) -> Vec<String> {
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

pub fn is_exe_file(s: &str) -> bool {
    if s.ends_with(".exe") {
        return true;
    }
    let re_latest =
        Regex::new(r"^https://github\.com/([^/]+)/([^/]+)/releases/latest/download/([^/]+)$")
            .expect("failed to build github latest release regex");
    let re_tag =
        Regex::new(r"^https://github\.com/([^/]+)/([^/]+)/releases/download/([^/]+)/([^/]+)$")
            .expect("failed to build github release regex");

    for (re, n) in [(re_latest, 3), (re_tag, 4)] {
        if let Some(cap) = re.captures(s) {
            if let Some(name) = cap.get(n) {
                if is_archive_file(name.as_str()) {
                    return false;
                }
                if !name.as_str().contains(".") {
                    return true;
                }
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
    use crate::{
        download::{download_dist_manfiest, download_extract, read_dist_manfiest},
        env::IS_WINDOWS,
        install::{
            get_artifact_download_url, get_artifact_url_from_manfiest, is_archive_file,
            is_exe_file, is_url, Repo,
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
        let files = download_extract(&url).await.unwrap();
        assert!(files
            .get(if IS_WINDOWS { "mujs.exe" } else { "mujs" })
            .is_some());
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

    #[test]
    fn test_is_exe_file() {
        for (a,b) in [
          ("https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe", true),
        ("https://github.com/pnpm/pnpm/releases/latest/download/pnpm-win-x64.exe", true),
        ("https://github.com/pnpm/pnpm/releases/latest/download/pnpm-win-x64", true),
        ("https://github.com/easy-install/easy-install/releases/download/v0.1.5/ei-x86_64-apple-darwin.tar.gz", false),
        ("https://github.com/easy-install/easy-install", false),
        ("https://github.com/easy-install/easy-install/releases/tag/v0.1.5", false)
      ]{
        assert_eq!(is_exe_file(a),b);
      }
    }
}
