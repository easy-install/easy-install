use crate::manfiest::DistManifest;
use easy_archive::{File, Fmt};
use reqwest::header::{HeaderMap, HeaderValue};
use serde::de::DeserializeOwned;
use tracing::trace;

fn get_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.append("User-Agent", HeaderValue::from_static("reqwest"));
    if let Ok(token) = std::env::var("GITHUB_TOKEN") {
        headers.append(
            "Authorization",
            HeaderValue::from_str(&format!("token {token}")).expect("Authorization token error"),
        );
    };
    headers
}

pub async fn download_json<T: DeserializeOwned>(url: &str) -> Option<T> {
    let client = reqwest::Client::new();
    let response = client.get(url).headers(get_headers()).send().await.ok()?;
    response.json::<T>().await.ok()
}

pub async fn download_extract(url: &str) -> Option<Vec<File>> {
    let fmt = Fmt::guess(url)?;
    let buffer = download_binary(url).await?;
    let files = fmt.decode(buffer)?;
    Some(files)
}

pub async fn download(url: &str) -> Option<reqwest::Response> {
    trace!("download {}", url);
    let client = reqwest::Client::new();
    let headers = get_headers();
    client.get(url).headers(headers).send().await.ok()
}

pub async fn download_dist_manfiest(url: &str) -> Option<DistManifest> {
    trace!("download_dist_manfiest {}", url);
    let response = download(url).await?;
    response.json().await.ok()
}

pub async fn download_binary(url: &str) -> Option<Vec<u8>> {
    trace!("download_dist_manfiest {}", url);
    let response = download(url).await?;
    let bytes = response.bytes().await.ok()?;
    Some(bytes.to_vec())
}

pub fn read_dist_manfiest(url: &str) -> Option<DistManifest> {
    trace!("read_dist_manfiest {}", url);
    let s = std::fs::read_to_string(url).unwrap_or_else(|_| panic!("read file error: {url}"));
    serde_json::from_str(&s).ok()
}

#[cfg(test)]
mod test {
    use crate::download::download_extract;
    #[tokio::test]
    async fn test_download() {
        let url = "https://github.com/ahaoboy/mujs-build/releases/download/v0.0.1/mujs-x86_64-unknown-linux-gnu.tar.gz";
        let files = download_extract(url).await.unwrap();
        assert!(files.iter().any(|i| i.path == "mujs"));
        assert!(files.iter().any(|i| i.path == "mujs-pp"));
        assert!(files.iter().any(|i| i.path == "libmujs.a"));
    }
}
