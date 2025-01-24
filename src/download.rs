use binstalk_downloader::{bytes::Bytes, download::Download, remote::Client};
use reqwest::header::{HeaderMap, HeaderValue};
use std::num::NonZeroU16;
use tracing::trace;
use url::Url;

use crate::manfiest::DistManifest;

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

pub async fn download_files(url: &str) -> Download<'static> {
    trace!("download {}", url);
    let client = create_client().await;
    Download::new(client, Url::parse(url).unwrap())
}

pub async fn download(url: &str) -> Option<reqwest::Response> {
    trace!("download {}", url);
    let client = reqwest::Client::new();
    let mut headers = HeaderMap::new();
    headers.append("User-Agent", HeaderValue::from_static("reqwest"));
    if let Ok(token) = std::env::var("GITHUB_TOKEN") {
        headers.append(
            "Authorization",
            HeaderValue::from_str(&format!("token {token}")).expect("Authorization token error"),
        );
    };
    client.get(url).headers(headers).send().await.ok()
}

pub async fn download_dist_manfiest(url: &str) -> Option<DistManifest> {
    trace!("download_dist_manfiest {}", url);
    let response = download(url).await?;
    response.json().await.ok()
}

pub async fn download_binary(url: &str) -> Option<Bytes> {
    trace!("download_dist_manfiest {}", url);
    let response = download(url).await?;
    response.bytes().await.ok()
}

pub fn read_dist_manfiest(url: &str) -> Option<DistManifest> {
    trace!("read_dist_manfiest {}", url);
    let s = std::fs::read_to_string(url).unwrap_or_else(|_| panic!("read file error: {url}"));
    serde_json::from_str(&s).ok()
}

#[cfg(test)]
mod test {
    use crate::download::download_files;
    use binstalk_downloader::download::PkgFmt;
    use std::path::Path;
    use tempfile::tempdir;
    #[tokio::test]
    async fn test_download() {
        let url = "https://github.com/ahaoboy/mujs-build/releases/download/v0.0.1/mujs-x86_64-unknown-linux-gnu.tar.gz";
        let files = download_files(url).await;
        let fmt = PkgFmt::guess_pkg_format(url).unwrap();
        let out_dir = tempdir().unwrap();
        let files = files.and_extract(fmt, out_dir.path()).await.unwrap();
        assert!(files.has_file(Path::new("mujs")));
        assert!(files.has_file(Path::new("mujs-pp")));
        assert!(files.has_file(Path::new("libmujs.a")));
    }
}
