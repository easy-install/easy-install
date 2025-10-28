use crate::{manfiest::DistManifest, tool::is_url};
use anyhow::{Context, Result};
use easy_archive::{File, Fmt};
use reqwest::header::{HeaderMap, HeaderValue};
use serde::de::DeserializeOwned;
use std::time::Duration;
use tokio::time::sleep;
use tracing::trace;

async fn retry_request<F, Fut, T>(
    max_retries: usize,
    operation: F,
    operation_name: &str,
) -> Result<T>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    let mut last_error = None;

    for attempt in 0..=max_retries {
        match operation().await {
            Ok(result) => {
                if attempt > 0 {
                    trace!(
                        "{} succeeded on attempt {}/{}",
                        operation_name,
                        attempt + 1,
                        max_retries + 1
                    );
                }
                return Ok(result);
            }
            Err(e) => {
                last_error = Some(e);
                if attempt < max_retries {
                    let delay = Duration::from_millis(100 * 2_u64.pow(attempt as u32));
                    trace!(
                        "{} failed on attempt {}/{}, retrying in {:?}...",
                        operation_name,
                        attempt + 1,
                        max_retries + 1,
                        delay
                    );
                    sleep(delay).await;
                } else {
                    trace!(
                        "{} failed after {} attempts",
                        operation_name,
                        max_retries + 1
                    );
                }
            }
        }
    }

    Err(last_error.unwrap())
}

fn get_headers() -> Result<HeaderMap> {
    let mut headers = HeaderMap::new();
    headers.append("User-Agent", HeaderValue::from_static("reqwest"));
    if let Ok(token) = std::env::var("GITHUB_TOKEN") {
        headers.append(
            "Authorization",
            HeaderValue::from_str(&format!("token {token}"))
                .context("Authorization token error")?,
        );
    };
    Ok(headers)
}

fn create_client(timeout_secs: u64) -> Result<reqwest::Client> {
    let timeout = Duration::from_secs(timeout_secs);
    reqwest::Client::builder()
        .timeout(timeout)
        .connect_timeout(timeout)
        .build()
        .context("Failed to create HTTP client")
}

pub(crate) async fn download_json<T: DeserializeOwned>(
    url: &str,
    retry: usize,
    timeout: u64,
) -> Result<T> {
    let url_clone = url.to_string();
    retry_request(
        retry,
        || async {
            let client = create_client(timeout)?;
            let response = match client.get(&url_clone).headers(get_headers()?).send().await {
                Ok(resp) => resp,
                Err(e) => {
                    if e.is_timeout() {
                        return Err(anyhow::anyhow!(
                            "Request timed out after {} seconds: {}",
                            timeout,
                            url_clone
                        ));
                    }
                    return Err(e).context("send failed");
                }
            };
            if response.status() != reqwest::StatusCode::OK {
                return Err(anyhow::anyhow!(
                    "request failed with status: {}",
                    response.status()
                ));
            }
            response.json::<T>().await.context("json parse failed")
        },
        &format!("download_json({})", url),
    )
    .await
}

pub(crate) async fn get_bytes(url: &str, retry: usize, timeout: u64) -> Result<Vec<u8>> {
    let bin = if is_url(url) {
        download_binary(url, retry, timeout).await?
    } else {
        std::fs::read(url).context("read file failed")?.to_vec()
    };
    Ok(bin)
}

pub(crate) fn extract_bytes(bytes: Vec<u8>, fmt: Fmt) -> Result<Vec<File>> {
    let files = fmt
        .decode(bytes)
        .context("decode failed")?
        .into_iter()
        // FIXME: remove __MACOSX
        .filter(|i| !i.path.starts_with("__MACOSX"))
        .collect();

    Ok(files)
}

pub(crate) async fn download(url: &str, retry: usize, timeout: u64) -> Result<reqwest::Response> {
    let url_clone = url.to_string();
    retry_request(
        retry,
        || async {
            trace!("download {}", url_clone);
            let client = create_client(timeout)?;
            let headers = get_headers()?;
            let response = match client.get(&url_clone).headers(headers).send().await {
                Ok(resp) => resp,
                Err(e) => {
                    if e.is_timeout() {
                        return Err(anyhow::anyhow!(
                            "Request timed out after {} seconds: {}",
                            timeout,
                            url_clone
                        ));
                    }
                    return Err(e).context("send failed");
                }
            };
            if response.status() != reqwest::StatusCode::OK {
                return Err(anyhow::anyhow!(
                    "request failed with status: {}",
                    response.status()
                ));
            }
            Ok(response)
        },
        &format!("download({})", url),
    )
    .await
}

pub(crate) async fn download_dist_manfiest(
    url: &str,
    retry: usize,
    timeout: u64,
) -> Result<DistManifest> {
    let url_clone = url.to_string();
    retry_request(
        retry,
        || async {
            trace!("download_dist_manfiest {}", url_clone);
            let response = download(&url_clone, 0, timeout).await?;
            if response.status() != reqwest::StatusCode::OK {
                return Err(anyhow::anyhow!(
                    "request failed with status: {}",
                    response.status()
                ));
            }
            response.json().await.context("json parse failed")
        },
        &format!("download_dist_manfiest({})", url),
    )
    .await
}

pub(crate) async fn download_binary(url: &str, retry: usize, timeout: u64) -> Result<Vec<u8>> {
    let url_clone = url.to_string();
    retry_request(
        retry,
        || async {
            trace!("download_binary {}", url_clone);
            let response = download(&url_clone, 0, timeout).await?;
            let bytes = response.bytes().await.context("bytes failed")?;
            Ok(bytes.to_vec())
        },
        &format!("download_binary({})", url),
    )
    .await
}

pub(crate) fn read_dist_manfiest(url: &str) -> Result<DistManifest> {
    trace!("read_dist_manfiest {}", url);
    let s = std::fs::read_to_string(url).context(format!("read file error: {url}"))?;
    serde_json::from_str(&s).context("json parse failed")
}

#[cfg(test)]
mod test {
    use easy_archive::Fmt;

    use crate::download::{extract_bytes, get_bytes};

    #[tokio::test]
    async fn test_download() {
        let url = "https://github.com/ahaoboy/mujs-build/releases/download/v0.0.1/mujs-x86_64-unknown-linux-gnu.tar.gz";
        let bytes = get_bytes(url, 3, 30).await.expect("donwload error");
        let fmt = Fmt::guess(url).expect("fmt error");
        let files = extract_bytes(bytes, fmt).expect("extract_bytes failed");
        assert!(files.iter().any(|i| i.path == "mujs"));
        assert!(files.iter().any(|i| i.path == "mujs-pp"));
        assert!(files.iter().any(|i| i.path == "libmujs.a"));
    }
}
