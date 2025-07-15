use crate::{manfiest::DistManifest, tool::is_url};
use easy_archive::{File, Fmt};
use reqwest::header::{HeaderMap, HeaderValue};
use serde::de::DeserializeOwned;
use tracing::trace;
use anyhow::{Result, Context};

fn get_headers() -> Result<HeaderMap> {
    let mut headers = HeaderMap::new();
    headers.append("User-Agent", HeaderValue::from_static("reqwest"));
    if let Ok(token) = std::env::var("GITHUB_TOKEN") {
        headers.append(
            "Authorization",
            HeaderValue::from_str(&format!("token {token}")).context("Authorization token error")?,
        );
    };
    Ok(headers)
}

pub async fn download_json<T: DeserializeOwned>(url: &str) -> Result<T> {
    let client = reqwest::Client::new();
    let response = client.get(url).headers(get_headers()?).send().await.context("send failed")?;
    Ok(response.json::<T>().await.context("json parse failed")?)
}

pub async fn download_extract(url: &str) -> Result<Vec<File>> {
    let fmt = Fmt::guess(url).context("Fmt guess failed")?;
    let buffer = if is_url(url) {
        download_binary(url).await?
    } else {
        std::fs::read(url).context("read file failed")?.to_vec()
    };
    let files = fmt
        .decode(buffer).context("decode failed")?
        .into_iter()
        // FIXME: remove __MACOSX
        .filter(|i| !i.path.starts_with("__MACOSX"))
        .collect();
    Ok(files)
}

pub async fn download(url: &str) -> Result<reqwest::Response> {
    trace!("download {}", url);
    let client = reqwest::Client::new();
    let headers = get_headers()?;
    Ok(client.get(url).headers(headers).send().await.context("send failed")?)
}

pub async fn download_dist_manfiest(url: &str) -> Result<DistManifest> {
    trace!("download_dist_manfiest {}", url);
    let response = download(url).await?;
    Ok(response.json().await.context("json parse failed")?)
}

pub async fn download_binary(url: &str) -> Result<Vec<u8>> {
    trace!("download_dist_manfiest {}", url);
    let response = download(url).await?;
    let bytes = response.bytes().await.context("bytes failed")?;
    Ok(bytes.to_vec())
}

pub fn read_dist_manfiest(url: &str) -> Result<DistManifest> {
    trace!("read_dist_manfiest {}", url);
    let s = std::fs::read_to_string(url).context(format!("read file error: {url}"))?;
    Ok(serde_json::from_str(&s).context("json parse failed")?)
}

#[cfg(test)]
mod test {
    use anyhow::Context;

    use crate::download::download_extract;
    #[tokio::test]
    async fn test_download() {
        let url = "https://github.com/ahaoboy/mujs-build/releases/download/v0.0.1/mujs-x86_64-unknown-linux-gnu.tar.gz";
        let files = download_extract(url).await.context("download_extract failed").unwrap();
        assert!(files.iter().any(|i| i.path == "mujs"));
        assert!(files.iter().any(|i| i.path == "mujs-pp"));
        assert!(files.iter().any(|i| i.path == "libmujs.a"));
    }
}
