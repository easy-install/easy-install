use crate::{artifact::Artifacts, download::download, env::get_install_dir};
use atomic_file_install::atomic_install;
use binstalk_downloader::{
    download::{Download, PkgFmt},
    remote::Client,
};
use binstalk_registry::Registry;
use detect_targets::detect_targets;
use regex::Regex;
use semver::VersionReq;
use std::{num::NonZeroU16, path::Path};
use tempfile::tempdir;
use tracing::trace;

pub async fn install(url: &str) {
    trace!("install {}", url);
    if is_github(url) {
        install_from_github(url).await
    } else if is_url(url) && is_file(url) {
        install_from_url(url).await;
    } else {
        install_from_crate(url).await;
    }
}

async fn install_from_crate(crate_name: &str) {
    trace!("install_from_crate {}", crate_name);
    let client = Client::new(
        concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION")),
        None,
        NonZeroU16::new(10).unwrap(),
        1.try_into().unwrap(),
        [],
    )
    .unwrap();
    let version_req = &VersionReq::STAR;
    let sparse_registry: Registry = Registry::crates_io_sparse_registry();
    let manifest_from_sparse = sparse_registry
        .fetch_crate_matched(client, crate_name, version_req)
        .await
        .unwrap();
    if let Some(pkg) = manifest_from_sparse.package {
        if let Some(repo) = pkg.repository() {
            if is_github(repo) {
                install_from_github(repo).await;
            }
        }
    }
}

async fn install_from_url(url: &str) {
    trace!("install_from_url {}", url);
    let fmt = PkgFmt::guess_pkg_format(url).unwrap();
    let files = download(url).await;
    install_from_download_file(fmt, files).await;
}

async fn install_from_download_file(fmt: PkgFmt, download: Download<'static>) {
    trace!("install_from_download_file");
    let out_dir = tempdir().unwrap();
    let files = download.and_extract(fmt, &out_dir).await.unwrap();
    let dir = files.get_dir(Path::new(".")).unwrap();
    let install_dir = get_install_dir();
    let src_dir = out_dir.path().to_path_buf();

    let mut v = vec![];
    for i in dir {
        let mut dst = install_dir.clone();
        let mut src = src_dir.clone();
        let s = i.to_str().unwrap();

        src.push(s);
        dst.push(s);
        atomic_install(src.as_path(), dst.as_path()).unwrap();

        v.push(format!("{} -> {}", s, dst.to_str().unwrap()));
    }
    println!("Installation Successful");
    println!("{}", v.join("\n"));
}

async fn get_artifact_api(url: &str) -> Option<String> {
    trace!("get_artifact_api {}", url);
    let re_gh_tag = Regex::new(
        r"https?://github\.com/(?P<owner>[^/]+)/(?P<repo>[^/]+)/releases/tag/(?P<tag>[^/]+)",
    )
    .unwrap();

    let re_gh_releases =
        Regex::new(r"http?s://github\.com/(?P<owner>[^/]+)/(?P<repo>[^/]+)").unwrap();

    if let Some(captures) = re_gh_tag.captures(url) {
        if let (Some(owner), Some(repo), Some(tag)) = (
            captures.name("owner"),
            captures.name("repo"),
            captures.name("tag"),
        ) {
            return Some(format!(
                "https://api.github.com/repos/{}/{}/releases/tags/{}",
                owner.as_str(),
                repo.as_str(),
                tag.as_str()
            ));
        }
    }

    if let Some(captures) = re_gh_releases.captures(url) {
        if let (Some(owner), Some(repo)) = (captures.name("owner"), captures.name("repo")) {
            return Some(format!(
                "https://api.github.com/repos/{}/{}/releases/latest",
                owner.as_str(),
                repo.as_str(),
            ));
        }
    }
    None
}

pub async fn get_artifact_url(url: &str) -> Option<String> {
    trace!("get_artifact_url {}", url);
    let api = get_artifact_api(url).await.unwrap();
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

    for i in artifacts.assets {
        for pat in &targets {
            if i.name.contains(pat) {
                return Some(i.browser_download_url);
            }
        }
    }

    None
}

async fn install_from_github(url: &str) {
    trace!("install_from_git {}", url);
    let artifact_url = get_artifact_url(url).await.unwrap();
    install_from_url(&artifact_url).await;
}

fn is_file(s: &str) -> bool {
    use PkgFmt::*;
    let is_windows = cfg!(target_os = "windows");

    for i in [Tar, Tbz2, Tgz, Txz, Tzstd, Zip, Bin] {
        for ext in i.extensions(is_windows) {
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

fn is_github(s: &str) -> bool {
    s.starts_with("http://github.com/") || s.starts_with("https://github.com/")
}

#[cfg(test)]
mod test {
    use std::path::Path;

    use binstalk_downloader::download::PkgFmt;
    use tempfile::tempdir;

    use crate::{
        download::download,
        env::IS_WINDOWS,
        install::{is_file, is_github, is_url},
    };

    use super::{get_artifact_api, get_artifact_url};

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
        assert!(is_github("https://github.com/ahaoboy/ansi2"));

        assert!(!is_github(
            "https://api.github.com/repos/ahaoboy/ansi2/releases/latest"
        ));
        assert!(is_github(
            "https://github.com/ahaoboy/ansi2/releases/tag/v0.2.11"
        ));
        assert!(is_github("https://github.com/ahaoboy/ansi2/releases/download/v0.2.11/ansi2-x86_64-unknown-linux-musl.tar.gz"));
        assert!(is_github("https://github.com/ahaoboy/ansi2/releases/download/v0.2.11/ansi2-x86_64-pc-windows-msvc.zip"));
    }

    #[test]
    fn test_is_url() {
        assert!(is_url("https://github.com/ahaoboy/ansi2"));
        assert!(!is_url("ansi2"));
    }

    #[tokio::test]
    async fn test_get_artifact_url() {
        let url = get_artifact_url("https://github.com/ahaoboy/ansi2/releases/tag/v0.2.11")
            .await
            .unwrap();
        let fmt = PkgFmt::guess_pkg_format(&url).unwrap();
        let files = download(&url).await;
        let out_dir = tempdir().unwrap();
        let files = files.and_extract(fmt, out_dir.path()).await.unwrap();
        assert!(files.has_file(Path::new(if IS_WINDOWS { "ansi2.exe" } else { "ansi2" })));
    }

    #[tokio::test]
    async fn test_get_artifact_api() {
        let url = get_artifact_api("https://github.com/ahaoboy/ansi2/releases/tag/v0.2.11")
            .await
            .unwrap();
        assert_eq!(
            url,
            "https://api.github.com/repos/ahaoboy/ansi2/releases/tags/v0.2.11"
        )
    }
}
