use crate::git_credentials::try_from_home;
use binstalk_downloader::{download::Download, remote::Client};
use binstalk_git_repo_api::gh_api_client::{GhApiClient, GhReleaseArtifact};
use std::num::NonZeroU16;
use tracing::trace;
use url::Url;

pub async fn download(url: &str) -> Download<'static> {
    trace!("download {}", url);
    let client = Client::new(
        concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION")),
        None,
        NonZeroU16::new(10).unwrap(),
        1.try_into().unwrap(),
        [],
    )
    .unwrap();
    Download::new(client.clone(), Url::parse(url).unwrap())
}

pub async fn download_artifact(url: &str) -> Download<'static> {
    trace!("download_artifact {}", url);
    let client = Client::new(
        concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION")),
        None,
        NonZeroU16::new(10).unwrap(),
        1.try_into().unwrap(),
        [],
    )
    .unwrap();
    let gh = GhApiClient::new(client, try_from_home());
    let gh_release_artifact =
        GhReleaseArtifact::try_extract_from_url(&url::Url::parse(url).unwrap()).unwrap();
    let artifact_url = gh
        .has_release_artifact(gh_release_artifact)
        .await
        .unwrap()
        .unwrap();
    
    gh.download_artifact(artifact_url).await.unwrap()
}

#[cfg(test)]
mod test {
    use std::path::Path;

    use binstalk_downloader::download::PkgFmt;
    use tempfile::tempdir;

    use crate::download::download_artifact;

    #[tokio::test]
    async fn test_download_artifact() {
        let url = "https://github.com/ahaoboy/ansi2/releases/download/v0.2.11/ansi2-aarch64-apple-darwin.tar.gz";
        let files = download_artifact(url).await;
        let fmt = PkgFmt::guess_pkg_format(url).unwrap();
        let out_dir = tempdir().unwrap();
        let files = files.and_extract(fmt, out_dir.path()).await.unwrap();
        assert!(files.has_file(Path::new("ansi2")));
    }

    #[tokio::test]
    async fn test_download() {
        let url = "https://github.com/ahaoboy/mujs-build/releases/download/v0.0.1/mujs-x86_64-unknown-linux-gnu.tar.gz";
        let files = download_artifact(url).await;
        let fmt = PkgFmt::guess_pkg_format(url).unwrap();
        let out_dir = tempdir().unwrap();
        let files = files.and_extract(fmt, out_dir.path()).await.unwrap();
        assert!(files.has_file(Path::new("mujs")));
        assert!(files.has_file(Path::new("libmujs.so")));
        assert!(files.has_file(Path::new("libmujs.a")));
    }
}
