use crate::tool::parse_and_validate_url;
use crate::{manfiest::DistManifest, tool::is_url};
use anyhow::{Context, Result};
use easy_archive::{File, Fmt};
use reqwest::Client;
use reqwest::header::{HeaderMap, HeaderValue};
use serde::de::DeserializeOwned;
use std::sync::OnceLock;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::time::{sleep, timeout};
use tracing::{trace, warn};

// Credential cache for GitHub token detection
static GITHUB_TOKEN_CACHE: OnceLock<Option<String>> = OnceLock::new();

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

async fn detect_github_token() -> Option<String> {
    // Check cache first
    if let Some(cached) = GITHUB_TOKEN_CACHE.get() {
        return cached.clone();
    }

    // Try detection methods in order
    let token = if let Some(t) = try_github_cli_token().await {
        Some(t)
    } else if let Some(t) = try_git_credential_manager().await {
        Some(t)
    } else {
        std::env::var("GITHUB_TOKEN").ok()
    };

    // Cache the result (even if None)
    let _ = GITHUB_TOKEN_CACHE.set(token.clone());

    token
}

async fn try_github_cli_token() -> Option<String> {
    trace!("Attempting to detect GitHub CLI token");

    let timeout_duration = Duration::from_secs(5);

    // Platform-specific command execution
    #[cfg(target_os = "windows")]
    let mut cmd = {
        let mut c = Command::new("powershell");
        c.args(["-c", "gh auth token"]);
        c
    };

    #[cfg(not(target_os = "windows"))]
    let mut cmd = {
        let mut c = Command::new("gh");
        c.arg("auth").arg("token");
        c
    };

    // Execute command with timeout
    let result = timeout(timeout_duration, cmd.output()).await;

    match result {
        Ok(Ok(output)) if output.status.success() => {
            let token = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !token.is_empty() {
                trace!("Successfully detected GitHub CLI token");
                return Some(token);
            }
            trace!("GitHub CLI returned empty token");
            None
        }
        Ok(Ok(output)) => {
            trace!("GitHub CLI command failed with status: {}", output.status);
            None
        }
        Ok(Err(e)) => {
            trace!("Failed to execute GitHub CLI command: {}", e);
            None
        }
        Err(_) => {
            warn!("GitHub CLI token detection timed out after 5 seconds");
            None
        }
    }
}

struct GitCredentialOutput {
    protocol: Option<String>,
    host: Option<String>,
    username: Option<String>,
    password: Option<String>,
}

impl GitCredentialOutput {
    fn parse(output: &str) -> Self {
        let mut result = GitCredentialOutput {
            protocol: None,
            host: None,
            username: None,
            password: None,
        };

        for line in output.lines() {
            if let Some((key, value)) = line.split_once('=') {
                match key {
                    "protocol" => result.protocol = Some(value.to_string()),
                    "host" => result.host = Some(value.to_string()),
                    "username" => result.username = Some(value.to_string()),
                    "password" => result.password = Some(value.to_string()),
                    _ => {}
                }
            }
        }

        result
    }

    fn get_token(&self) -> Option<String> {
        self.password.clone()
    }
}

async fn try_git_credential_manager() -> Option<String> {
    trace!("Attempting to detect Git Credential Manager token");

    let timeout_duration = Duration::from_secs(5);

    let mut cmd = Command::new("git");
    cmd.args(["credential", "fill"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .env("GIT_TERMINAL_PROMPT", "0");

    // Spawn the process
    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(e) => {
            trace!("Failed to spawn git credential fill command: {}", e);
            return None;
        }
    };

    // Write input to stdin
    if let Some(mut stdin) = child.stdin.take() {
        let input = "protocol=https\nhost=github.com\n\n";
        if let Err(e) = timeout(timeout_duration, stdin.write_all(input.as_bytes())).await {
            trace!("Failed to write to git credential stdin: {}", e);
            let _ = child.kill().await;
            return None;
        }
    } else {
        trace!("Failed to get stdin for git credential command");
        let _ = child.kill().await;
        return None;
    }

    // Wait for output with timeout
    let result = timeout(timeout_duration, child.wait_with_output()).await;

    match result {
        Ok(Ok(output)) if output.status.success() => {
            let output_str = String::from_utf8_lossy(&output.stdout);
            let parsed = GitCredentialOutput::parse(&output_str);

            if let Some(token) = parsed.get_token()
                && !token.is_empty()
            {
                trace!("Successfully detected Git Credential Manager token");
                return Some(token);
            }
            trace!("Git Credential Manager returned no password");
            None
        }
        Ok(Ok(output)) => {
            trace!(
                "Git credential command failed with status: {}",
                output.status
            );
            None
        }
        Ok(Err(e)) => {
            trace!("Failed to execute git credential command: {}", e);
            None
        }
        Err(_) => {
            warn!("Git Credential Manager detection timed out after 5 seconds");
            None
        }
    }
}

fn is_github_url(parsed: &reqwest::Url) -> bool {
    if let Some(host) = parsed.host_str() {
        return host == "github.com"
            || host.ends_with(".github.com")
            || host == "githubusercontent.com"
            || host.ends_with(".githubusercontent.com");
    }
    false
}

async fn get_headers(parsed: &reqwest::Url) -> Result<HeaderMap> {
    let mut headers = HeaderMap::new();
    headers.append("User-Agent", HeaderValue::from_static("reqwest"));

    // Only add GitHub token for GitHub URLs to prevent token leakage
    if is_github_url(parsed) {
        if let Some(token) = detect_github_token().await {
            headers.append(
                "Authorization",
                HeaderValue::from_str(&format!("Bearer {}", token))
                    .context("Authorization token error")?,
            );
            trace!("Using detected GitHub token for authentication");
        } else {
            trace!("No GitHub token detected, proceeding without authentication");
        }
    }

    Ok(headers)
}

fn create_client(timeout_secs: u64) -> &'static Client {
    static CLIENT: OnceLock<Client> = OnceLock::new();
    let timeout = Duration::from_secs(timeout_secs);
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(timeout)
            .connect_timeout(timeout)
            .tcp_keepalive(timeout)
            .cookie_store(true)
            .build()
            .expect("Failed to create HTTP client")
    })
}

pub(crate) async fn download_json<T: DeserializeOwned>(
    url: &str,
    retry: usize,
    timeout: u64,
) -> Result<T> {
    let parsed = parse_and_validate_url(url)?;

    retry_request(
        retry,
        || async {
            let client = create_client(timeout);
            let response = match client
                .get(parsed.clone())
                .headers(get_headers(&parsed).await?)
                .send()
                .await
            {
                Ok(resp) => resp,
                Err(e) => {
                    if e.is_timeout() {
                        return Err(anyhow::anyhow!(
                            "Request timed out after {} seconds: {}",
                            timeout,
                            url
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
        .decode(bytes)?
        .into_iter()
        // FIXME: remove __MACOSX
        .filter(|i| !i.path.starts_with("__MACOSX"))
        .collect();

    Ok(files)
}

pub(crate) async fn download(url: &str, retry: usize, timeout: u64) -> Result<reqwest::Response> {
    let parsed = parse_and_validate_url(url)?;

    retry_request(
        retry,
        || async {
            trace!("download {}", url);
            let client = create_client(timeout);
            let headers = get_headers(&parsed).await?;
            let response = match client.get(parsed.clone()).headers(headers).send().await {
                Ok(resp) => resp,
                Err(e) => {
                    if e.is_timeout() {
                        return Err(anyhow::anyhow!(
                            "Request timed out after {} seconds: {}",
                            timeout,
                            url
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
