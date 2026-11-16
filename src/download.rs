use crate::artifact::GhArtifacts;
use crate::error::{Context, Result, err};
use crate::tool::parse_and_validate_url;
use crate::{manifest::DistManifest, tool::is_url};
use easy_archive::{File, Fmt};
use regex::Regex;
use reqwest::Client;
use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use reqwest::header::{HeaderMap, HeaderValue};
use serde::de::DeserializeOwned;
use std::net::SocketAddr;
use std::sync::{Arc, LazyLock, OnceLock};
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::time::{sleep, timeout};
use tracing::{trace, warn};

/// Upper bound for a single DNS lookup. Without it a stuck resolver (seen on
/// some Android/Termux networks) blocks the request forever.
const DNS_LOOKUP_TIMEOUT: Duration = Duration::from_secs(10);

// Credential cache for GitHub token detection
static GITHUB_TOKEN_CACHE: OnceLock<Option<String>> = OnceLock::new();

// Matches GitHub release asset download URLs so that private-repo downloads
// can be retried through the API:
// https://github.com/{owner}/{repo}/releases/download/{tag}/{filename}
static RE_GH_RELEASE_DOWNLOAD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^https://github\.com/(?P<owner>[^/]+)/(?P<repo>[^/]+)/releases/download/(?P<tag>[^/]+)/(?P<filename>.+)$",
    )
    .unwrap()
});

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

    Err(last_error.unwrap_or_else(|| err!("retry_request: no error captured")))
}

async fn detect_github_token() -> Option<String> {
    // Check cache first
    if let Some(cached) = GITHUB_TOKEN_CACHE.get() {
        return cached.clone();
    }

    // Check GITHUB_TOKEN env var first — it's the most reliable and
    // fastest method in CI runners (no subprocess overhead).
    let token = if let Ok(t) = std::env::var("GITHUB_TOKEN")
        && !t.is_empty()
    {
        Some(t)
    } else if let Some(t) = try_github_cli_token().await {
        Some(t)
    } else {
        try_git_credential_manager().await
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

fn create_client() -> &'static Client {
    static CLIENT: OnceLock<Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            // Use rustls with the bundled (webpki) Mozilla root store.
            //
            // On Android/Termux the OS trust store is not exposed to native
            // CLI processes, and `rustls-platform-verifier` (what reqwest 0.13
            // uses by default) requires a JNI-initialized Android context and
            // panics with "Expect rustls-platform-verifier to be initialized".
            // Bundled roots avoid both problems and behave identically on
            // every platform.
            .use_rustls_tls()
            .tls_built_in_root_certs(true)
            // HTTP/2 is not needed here and h1-only avoids ALPN negotiation
            // issues on mobile networks.
            .http1_only()
            // Prefer IPv4; see `PreferIpv4Resolver`.
            .dns_resolver(Arc::new(PreferIpv4Resolver))
            .connect_timeout(Duration::from_secs(30))
            .cookie_store(true)
            .build()
            .expect("Failed to create HTTP client")
    })
}

/// Resolver that returns IPv4 addresses before IPv6 ones.
///
/// reqwest/hyper try the resolved addresses **in order** and the whole
/// attempt (including DNS and the TLS handshake) is bounded by a single
/// connect timeout. On Android/mobile networks an advertised but unroutable
/// IPv6 address is common: it silently drops packets, consumes the entire
/// connect timeout, and the request fails even though IPv4 works. That is why
/// `curl` (which implements Happy Eyeballs and falls back to IPv4) succeeds
/// while a reqwest client fails.
///
/// Sorting IPv4 first makes the reachable addresses win immediately while
/// still keeping IPv6 as a fallback. The lookup itself is also bounded so a
/// hung resolver produces a real error instead of an endless hang.
///
/// On Android this additionally refuses to call `getaddrinfo` at all when the
/// C library has no nameserver (see [`android_dns_error`]): musl would block
/// there for a very long time instead of failing.
#[derive(Debug, Default)]
struct PreferIpv4Resolver;

impl Resolve for PreferIpv4Resolver {
    fn resolve(&self, name: Name) -> Resolving {
        let host = name.as_str().to_owned();
        Box::pin(async move {
            // Numeric addresses need no resolver, and are common for CDNs and
            // for users working around a broken DNS setup. Port 0 is replaced
            // by reqwest with the scheme's default port.
            if let Ok(ip) = host.parse::<std::net::IpAddr>() {
                return Ok(Box::new(std::iter::once(SocketAddr::new(ip, 0))) as Addrs);
            }

            if dns_unavailable(&host) {
                return Err::<Addrs, BoxError>(android_dns_error(&host).into());
            }

            let lookup = tokio::net::lookup_host((host.as_str(), 0));
            let collected: Vec<SocketAddr> = timeout(DNS_LOOKUP_TIMEOUT, lookup)
                .await
                .map_err(|_| -> BoxError { format!("DNS lookup timed out for {host}").into() })?
                .map_err(|e| -> BoxError { format!("DNS lookup failed for {host}: {e}").into() })?
                .collect();

            if collected.is_empty() {
                return Err::<Addrs, BoxError>(
                    format!("DNS lookup returned no addresses for {host}").into(),
                );
            }

            let (mut ipv4, ipv6): (Vec<_>, Vec<_>) =
                collected.into_iter().partition(SocketAddr::is_ipv4);
            ipv4.extend(ipv6);

            Ok(Box::new(ipv4.into_iter()) as Addrs)
        })
    }
}

/// True when this process runs on Android, even if it was not compiled for it.
///
/// A `*-unknown-linux-musl` build reports `target_os = "linux"` but may still
/// be running inside Termux (or another Android rootfs).
fn running_on_android() -> bool {
    cfg!(target_os = "android")
        || std::env::var_os("ANDROID_ROOT").is_some()
        || std::path::Path::new("/system/build.prop").exists()
}

/// Parse a `resolv.conf` body the same way musl/glibc do: a `nameserver`
/// directive with a non-empty address.
fn resolv_conf_has_nameserver(content: &str) -> bool {
    content.lines().any(|line| {
        let mut fields = line.split_whitespace();
        matches!(fields.next(), Some("nameserver"))
            && fields
                .next()
                .is_some_and(|ns| !ns.is_empty() && ns != "0.0.0.0" && ns != "::")
    })
}

fn host_in_hosts(content: &str, host: &str) -> bool {
    content
        .lines()
        .map(|line| line.split('#').next().unwrap_or_default())
        .any(|line| {
            line.split_whitespace()
                .skip(1)
                .any(|name| name.eq_ignore_ascii_case(host))
        })
}

/// Whether a DNS lookup for `host` is guaranteed to be unable to succeed.
///
/// Android only exposes DNS through its native resolver service (`netd`).
/// `aarch64-linux-android` builds use bionic, which talks to `netd`. Static
/// musl/glibc builds cannot, and `/etc/resolv.conf` does not exist on Android,
/// so `getaddrinfo()` there does not fail fast — it blocks (musl keeps
/// retrying, and Android gives it no route to the network) which shows up as
/// "the download hangs forever". Detecting the situation up front turns that
/// hang into an actionable error.
fn dns_unavailable(host: &str) -> bool {
    let hosts = std::fs::read_to_string("/etc/hosts").ok();
    let resolv_conf = std::fs::read_to_string("/etc/resolv.conf").ok();
    dns_unavailable_with(
        running_on_android(),
        cfg!(target_os = "android"),
        host,
        hosts.as_deref(),
        resolv_conf.as_deref(),
    )
}

/// Pure decision logic for [`dns_unavailable`], so it can be tested.
fn dns_unavailable_with(
    on_android: bool,
    bionic_build: bool,
    host: &str,
    hosts_file: Option<&str>,
    resolv_conf: Option<&str>,
) -> bool {
    if !on_android || bionic_build {
        // Not Android, or an Android build (bionic → netd): the platform
        // resolver is present.
        return false;
    }
    if hosts_file.is_some_and(|h| host_in_hosts(h, host)) {
        return false; // resolved from /etc/hosts, no nameserver needed
    }
    !resolv_conf.is_some_and(resolv_conf_has_nameserver)
}

/// Actionable message for [`dns_unavailable`].
fn android_dns_error(host: &str) -> String {
    format!(
        "cannot resolve '{host}': this build has no working DNS on Android

  This ei was compiled for {target} (static musl/glibc). Android only exposes
  DNS through its native resolver (`netd`), which such builds cannot reach,
  and /etc/resolv.conf does not exist here. getaddrinfo() does not fail on
  Android in that case — it blocks, so downloads appear to hang.

  Use the Android build instead:
    curl -fsSL https://raw.githubusercontent.com/easy-install/easy-install/main/install.sh | sh

  Or map '{host}' in /etc/hosts / pass a direct artifact URL.",
        target = env!("BUILD_TARGET"),
    )
}

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// `browser_download_url` is a GitHub *web* endpoint: for private repos it
/// does not accept API tokens (GitHub returns 404 to avoid leaking the
/// repo's existence), which is why the asset list can be fetched from the
/// API but the download itself fails. The supported way to download a
/// private release asset is the asset API endpoint with
/// `Accept: application/octet-stream`, which 302s to a pre-signed CDN URL.
async fn download_private_release_asset(
    parsed: &reqwest::Url,
    timeout_dur: Duration,
) -> Result<Option<reqwest::Response>> {
    let Some(captures) = RE_GH_RELEASE_DOWNLOAD.captures(parsed.as_str()) else {
        return Ok(None);
    };
    let (Some(owner), Some(repo), Some(tag), Some(filename)) = (
        captures.name("owner"),
        captures.name("repo"),
        captures.name("tag"),
        captures.name("filename"),
    ) else {
        return Ok(None);
    };
    // Without a token there is no way to authenticate the API request.
    if detect_github_token().await.is_none() {
        return Ok(None);
    }

    trace!("download_private_release_asset {}", parsed.as_str());
    let client = create_client();

    // Resolve the asset's API URL by matching the filename in the release.
    let release_api = format!(
        "https://api.github.com/repos/{}/{}/releases/tags/{}",
        owner.as_str(),
        repo.as_str(),
        tag.as_str()
    );
    let api_parsed = parse_and_validate_url(&release_api)?;
    let response = client
        .get(api_parsed.clone())
        .timeout(timeout_dur)
        .headers(get_headers(&api_parsed).await?)
        .send()
        .await
        .context("release api send failed")?;
    if response.status() != reqwest::StatusCode::OK {
        trace!("release api returned status: {}", response.status());
        return Ok(None);
    }
    let release: GhArtifacts = response.json().await.context("release api json failed")?;
    let Some(asset) = release
        .assets
        .iter()
        .find(|a| a.name == filename.as_str() || a.browser_download_url == parsed.as_str())
    else {
        trace!(
            "asset {} not found in release {}",
            filename.as_str(),
            tag.as_str()
        );
        return Ok(None);
    };

    // The octet-stream Accept header makes the API return the binary
    // (via a 302 to a pre-signed URL) instead of the asset metadata JSON.
    let Some(api_url) = &asset.url else {
        trace!("asset {} has no API url", filename.as_str());
        return Ok(None);
    };
    let asset_parsed = parse_and_validate_url(api_url)?;
    let mut headers = get_headers(&asset_parsed).await?;
    headers.append(
        "Accept",
        HeaderValue::from_static("application/octet-stream"),
    );
    let response = client
        .get(asset_parsed)
        .timeout(timeout_dur)
        .headers(headers)
        .send()
        .await
        .context("asset api send failed")?;
    if response.status() != reqwest::StatusCode::OK {
        trace!("asset api returned status: {}", response.status());
        return Ok(None);
    }
    Ok(Some(response))
}

pub(crate) async fn download_json<T: DeserializeOwned>(
    url: &str,
    retry: usize,
    timeout: u64,
) -> Result<T> {
    let parsed = parse_and_validate_url(url)?;
    let timeout_dur = Duration::from_secs(timeout);

    retry_request(
        retry,
        || async {
            let client = create_client();
            let response = match client
                .get(parsed.clone())
                .timeout(timeout_dur)
                .headers(get_headers(&parsed).await?)
                .send()
                .await
            {
                Ok(resp) => resp,
                Err(e) => {
                    if e.is_timeout() {
                        return Err(err!("Request timed out after {} seconds: {}", timeout, url));
                    }
                    return Err(e).context("send failed");
                }
            };
            if response.status() != reqwest::StatusCode::OK {
                return Err(err!("request failed with status: {}", response.status()));
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
    let timeout_dur = Duration::from_secs(timeout);

    retry_request(
        retry,
        || async {
            trace!("download {}", url);
            let client = create_client();
            let headers = get_headers(&parsed).await?;
            let response = match client
                .get(parsed.clone())
                .timeout(timeout_dur)
                .headers(headers)
                .send()
                .await
            {
                Ok(resp) => resp,
                Err(e) => {
                    if e.is_timeout() {
                        return Err(err!("Request timed out after {} seconds: {}", timeout, url));
                    }
                    return Err(e).context("send failed");
                }
            };
            if response.status() != reqwest::StatusCode::OK {
                let status = response.status();
                // Private release assets: browser_download_url rejects API
                // tokens, so retry through the GitHub asset API instead.
                if matches!(
                    status,
                    reqwest::StatusCode::UNAUTHORIZED
                        | reqwest::StatusCode::FORBIDDEN
                        | reqwest::StatusCode::NOT_FOUND
                ) && let Some(resp) =
                    download_private_release_asset(&parsed, timeout_dur).await?
                {
                    return Ok(resp);
                }
                return Err(err!("request failed with status: {}", status));
            }
            Ok(response)
        },
        &format!("download({})", url),
    )
    .await
}

pub(crate) async fn download_dist_manifest(
    url: &str,
    retry: usize,
    timeout: u64,
) -> Result<DistManifest> {
    let url_clone = url.to_string();
    retry_request(
        retry,
        || async {
            trace!("download_dist_manifest {}", url_clone);
            let response = download(&url_clone, 0, timeout).await?;
            response.json().await.context("json parse failed")
        },
        &format!("download_dist_manifest({})", url),
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

pub(crate) fn read_dist_manifest(url: &str) -> Result<DistManifest> {
    trace!("read_dist_manifest {}", url);
    let s = std::fs::read_to_string(url).context(format!("read file error: {url}"))?;
    serde_json::from_str(&s).context("json parse failed")
}

#[cfg(test)]
mod test {
    use easy_archive::Fmt;

    use crate::download::{extract_bytes, get_bytes};

    #[test]
    fn resolv_conf_nameserver_detection() {
        // The usual case.
        assert!(super::resolv_conf_has_nameserver(
            "search lan\nnameserver 192.168.1.1\n"
        ));
        // Commented out / implicit-only entries do not count.
        assert!(!super::resolv_conf_has_nameserver(
            "#nameserver 8.8.8.8\noptions ndots:1\n"
        ));
        // Missing file content.
        assert!(!super::resolv_conf_has_nameserver(""));
        // Placeholder addresses are what a broken resolver setup leaves behind;
        // musl would block trying to reach them.
        assert!(!super::resolv_conf_has_nameserver("nameserver 0.0.0.0\n"));
        assert!(!super::resolv_conf_has_nameserver("nameserver ::"));
    }

    #[test]
    fn hosts_file_lookup_matches_names_but_not_ips() {
        let hosts = "\
127.0.0.1   localhost
::1         localhost ip6-localhost
10.0.0.5    api.internal  # comment
";
        assert!(super::host_in_hosts(hosts, "api.internal"));
        assert!(super::host_in_hosts(hosts, "LOCALHOST"));
        // The IP column is not a name.
        assert!(!super::host_in_hosts(hosts, "10.0.0.5"));
        assert!(!super::host_in_hosts(hosts, "comment"));
        assert!(!super::host_in_hosts(hosts, "example.com"));
    }

    #[test]
    fn android_without_resolver_fails_fast() {
        // A musl/glibc build on Android: bionic's netd is unreachable and
        // /etc/resolv.conf is missing → must not call getaddrinfo (it blocks).
        assert!(super::dns_unavailable_with(
            true,
            false,
            "github.com",
            Some("127.0.0.1 localhost\n"),
            None,
        ));
        // An Android (bionic) build uses netd; never fail fast for it.
        assert!(!super::dns_unavailable_with(
            true, true, "github.com", None, None
        ));
        // Not on Android at all.
        assert!(!super::dns_unavailable_with(
            false, false, "github.com", None, None
        ));
        // proot-distro and similar setups ship a resolv.conf.
        assert!(!super::dns_unavailable_with(
            true,
            false,
            "github.com",
            None,
            Some("nameserver 8.8.8.8\n"),
        ));
        // /etc/hosts entries resolve without any nameserver.
        assert!(!super::dns_unavailable_with(
            true,
            false,
            "github.com",
            Some("140.82.1.1 github.com\n"),
            None,
        ));
        // A resolv.conf with only a placeholder address is not usable.
        assert!(super::dns_unavailable_with(
            true,
            false,
            "github.com",
            None,
            Some("nameserver 0.0.0.0\n"),
        ));
    }

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
