//! Downloading and decoding install sources.
//!
//! The HTTP work lives in `github_proxy::fetch` (client configuration, headers,
//! token handling, Android/Termux DNS guard). This module only adds what is
//! specific to `ei`:
//!
//! * the process-wide [`Client`],
//! * the `--retry` backoff policy (the library is single-attempt),
//! * the "private release asset through the API" fallback,
//! * reading local files and decoding archives.

use crate::error::{Context, Result};
use crate::manifest::DistManifest;
use easy_archive::{File, Fmt};
use github_proxy::artifacts;
use github_proxy::fetch::{self, Options, StatusCode, Url};
use github_proxy::token;
use reqwest::Client;
use serde::de::DeserializeOwned;
use std::sync::OnceLock;
use std::time::Duration;
use tokio::time::sleep;

/// The process-wide HTTP client.
///
/// [`fetch::builder`] supplies the platform-sensitive settings (bundled roots,
/// HTTP/1.1, bounded IPv4-first DNS); `ei` only adds a connect timeout and the
/// cookie store some proxies rely on.
fn client() -> &'static Client {
    static CLIENT: OnceLock<Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        fetch::builder()
            .connect_timeout(Duration::from_secs(30))
            .cookie_store(true)
            .build()
            .expect("failed to build the HTTP client")
    })
}

/// Exponential backoff for `--retry`.
///
/// Retry policy is an application concern, so it stays here rather than
/// complicating the library.
async fn with_retry<T, F, Fut>(retry: usize, mut operation: F) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    let mut attempt = 0;
    loop {
        match operation().await {
            Ok(value) => return Ok(value),
            Err(e) if attempt >= retry => return Err(e),
            Err(_) => {
                sleep(Duration::from_millis(100 * 2_u64.pow(attempt as u32))).await;
                attempt += 1;
            }
        }
    }
}

/// Per-request options including the cached token.
fn options(token: Option<&str>, timeout: u64) -> Options<'_> {
    Options {
        token,
        timeout: Some(Duration::from_secs(timeout)),
    }
}

/// One `GET`, falling back to the API for private release assets.
///
/// `browser_download_url` rejects API tokens on private repositories (GitHub
/// answers 404 to hide their existence), so 401/403/404 trigger the asset API
/// route before giving up.
async fn request_bytes(url: &str, timeout: u64) -> Result<Vec<u8>> {
    let token = token::detect().await;
    let opts = options(token.as_deref(), timeout);

    match fetch::bytes_with(client(), url, opts).await {
        Ok(bytes) => Ok(bytes),
        Err(e) => {
            if matches!(
                e.status(),
                Some(
                    StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN | StatusCode::NOT_FOUND
                )
            ) && let Some(bytes) = artifacts::private_asset(client(), url, opts).await?
            {
                return Ok(bytes);
            }
            Err(e.into())
        }
    }
}

/// Download a URL as JSON.
pub(crate) async fn download_json<T: DeserializeOwned>(
    url: &str,
    retry: usize,
    timeout: u64,
) -> Result<T> {
    with_retry(retry, || async {
        let token = token::detect().await;
        fetch::json_with(client(), url, options(token.as_deref(), timeout))
            .await
            .map_err(Into::into)
    })
    .await
}

/// Download a URL as text.
pub(crate) async fn download_text(url: &str, retry: usize, timeout: u64) -> Result<String> {
    with_retry(retry, || async {
        let token = token::detect().await;
        fetch::text_with(client(), url, options(token.as_deref(), timeout))
            .await
            .map_err(Into::into)
    })
    .await
}

/// Download a URL (or read a local path) as bytes.
pub(crate) async fn get_bytes(url: &str, retry: usize, timeout: u64) -> Result<Vec<u8>> {
    if Url::is_valid(url) {
        download_binary(url, retry, timeout).await
    } else {
        std::fs::read(url).with_context(|| format!("failed to read '{url}'"))
    }
}

/// Download a URL as bytes, honouring `--retry`.
pub(crate) async fn download_binary(url: &str, retry: usize, timeout: u64) -> Result<Vec<u8>> {
    with_retry(retry, || request_bytes(url, timeout)).await
}

/// Read and decode a dist-manifest from disk.
pub(crate) fn read_dist_manifest(url: &str) -> Result<DistManifest> {
    let text = std::fs::read_to_string(url).with_context(|| format!("failed to read '{url}'"))?;
    serde_json::from_str(&text).with_context(|| format!("failed to parse '{url}'"))
}

/// Decode an in-memory archive.
pub(crate) fn extract_bytes(bytes: Vec<u8>, fmt: Fmt) -> Result<Vec<File>> {
    Ok(fmt
        .decode(bytes)?
        .into_iter()
        // Some macOS archives carry a metadata mirror that must not be
        // installed.
        .filter(|file| !file.path.starts_with("__MACOSX"))
        .collect())
}

#[cfg(test)]
mod test {
    use easy_archive::Fmt;

    use crate::download::{extract_bytes, get_bytes};

    fn block_on<F: std::future::Future>(future: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(future)
    }

    #[tokio::test]
    async fn downloads_and_decodes_an_archive() {
        let url = "https://github.com/ahaoboy/mujs-build/releases/download/v0.0.1/mujs-x86_64-unknown-linux-gnu.tar.gz";
        let bytes = get_bytes(url, 3, 30).await.expect("download error");
        let fmt = Fmt::guess(url).expect("format error");
        let files = extract_bytes(bytes, fmt).expect("decode error");
        for expected in ["mujs", "mujs-pp", "libmujs.a"] {
            assert!(
                files.iter().any(|file| file.path == expected),
                "missing {expected}"
            );
        }
    }

    #[test]
    fn reads_a_local_file() {
        let dir = std::env::temp_dir().join(format!("ei-get-bytes-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("payload");
        std::fs::write(&path, b"local").unwrap();

        let bytes = block_on(get_bytes(path.to_str().unwrap(), 0, 5));
        assert_eq!(bytes.unwrap(), b"local");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
