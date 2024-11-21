use crate::{gh_token, git_credentials::try_from_home};
use binstalk::helpers::lazy_gh_api_client::LazyGhApiClient;
use binstalk_downloader::{download::Download, remote::Client};
use std::num::NonZeroU16;
use tracing::trace;
use url::Url;

async fn get_gh_token() -> Option<zeroize::Zeroizing<Box<str>>> {
    try_from_home().or(gh_token::get().await.ok())
}

pub async fn create_client() -> Client {
    trace!("create_client");
    Client::new(
        concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION")),
        None,
        NonZeroU16::new(10).unwrap(),
        1.try_into().unwrap(),
        [],
    )
    .unwrap()
}

pub async fn create_gh_client() -> LazyGhApiClient {
    trace!("create_gh_client");
    let client = Client::new(
        concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION")),
        None,
        NonZeroU16::new(10).unwrap(),
        1.try_into().unwrap(),
        [],
    )
    .unwrap();
    match get_gh_token().await {
        Some(token) => LazyGhApiClient::new(client.clone(), Some(token)),
        None => LazyGhApiClient::new(client.clone(), None),
    }
}

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
    Download::new(client, Url::parse(url).unwrap())
}

// pub async fn download_artifact(url: &str) -> Download<'static> {
//     trace!("download_artifact {}", url);
//     let client = create_client().await;
//     let gh = GhApiClient::new(client.clone(), get_gh_token().await);
//     let gh_release_artifact =
//         GhReleaseArtifact::try_extract_from_url(&url::Url::parse(url).unwrap()).unwrap();
//     let artifact_url = gh
//         .has_release_artifact(gh_release_artifact)
//         .await
//         .unwrap()
//         .unwrap();

//     // gh.download_artifact(artifact_url).await.unwrap()
//     Download::new(client.clone(), Url::parse(artifact_url.0).unwrap())
// }

#[cfg(test)]
mod test {
    use crate::{download::download, env::IS_WINDOWS};
    use binstalk_downloader::download::PkgFmt;
    use std::path::Path;
    use tempfile::tempdir;

    // #[tokio::test]
    // async fn test_get_gh_token() {
    //     let tk = get_gh_token().await;
    //     assert!(tk.is_some());
    // }

    // #[tokio::test]
    // async fn test_download_artifact() {
    //     let url = "https://github.com/ahaoboy/ansi2/releases/download/v0.2.11/ansi2-aarch64-apple-darwin.tar.gz";
    //     let files = download_artifact(url).await;
    //     let fmt = PkgFmt::guess_pkg_format(url).unwrap();
    //     let out_dir = tempdir().unwrap();
    //     let files = files.and_extract(fmt, out_dir.path()).await.unwrap();
    //     assert!(files.has_file(Path::new("ansi2")));
    // }

    #[tokio::test]
    async fn test_download() {
        let url = "https://github.com/ahaoboy/mujs-build/releases/download/v0.0.1/mujs-x86_64-unknown-linux-gnu.tar.gz";
        let files = download(url).await;
        let fmt = PkgFmt::guess_pkg_format(url).unwrap();
        let out_dir = tempdir().unwrap();
        let files = files.and_extract(fmt, out_dir.path()).await.unwrap();
        assert!(files.has_file(Path::new(if IS_WINDOWS { "mujs.exe" } else { "mujs" })));
        assert!(files.has_file(Path::new(if IS_WINDOWS {
            "mujs-pp.exe"
        } else {
            "mujs-pp"
        })));
        assert!(files.has_file(Path::new("libmujs.a")));
    }
}
