//! Remote installation over SSH.
//!
//! `ei` normally writes files to the local filesystem. Supplying a
//! destination like `user@host:/path` (or `-d user@host:/path`) switches the
//! install to a remote host: the artifact is still downloaded locally, then
//! streamed over SSH and written there, and `strip`/`upx` are executed on the
//! remote machine.
//!
//! Destination parsing and platform detection need no extra dependencies and
//! are always compiled; the SSH transport itself is gated behind the `ssh`
//! feature (enabled by default).

use guess_target::Target;
use std::str::FromStr;

#[cfg(feature = "ssh")]
use crate::backend::CommandResult;
#[cfg(feature = "ssh")]
use crate::error::{Context, Result, err};
#[cfg(feature = "ssh")]
use russh::client;
#[cfg(feature = "ssh")]
use russh::keys::{HashAlg, PrivateKeyWithHashAlg, PublicKeyOrCertificate, decode_secret_key};
#[cfg(feature = "ssh")]
use std::sync::Arc;
#[cfg(feature = "ssh")]
use std::time::Duration;

/// POSIX shell probe used to infer the remote platform.
///
/// The script lives in `src/shell/platform_probe.sh` (syntax highlighting and
/// standalone testing) and is embedded at compile time. It runs via `sh -c`,
/// so it works no matter which login shell (bash/zsh/fish/…) the user has.
/// It prints `KEY=value` lines consumed by [`RemotePlatform::parse`].
#[cfg_attr(not(feature = "ssh"), allow(dead_code))]
const PLATFORM_PROBE: &str = include_str!("shell/platform_probe.sh");

/// Platform information detected on the remote host.
#[cfg_attr(not(feature = "ssh"), allow(dead_code))]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RemotePlatform {
    pub os: String,
    pub arch: String,
    /// `musl` or `gnu` (Linux only).
    pub libc: String,
    pub android: bool,
    /// Detected Rust target triple (empty when unsupported).
    pub target: String,
}

/// POSIX script that inspects a directory.
///
/// Lives in `src/shell/dir_status.sh` and is embedded at compile time; the
/// `__DIR__` placeholder is replaced with a shell-quoted path before it is
/// sent to the remote host.
#[cfg_attr(not(feature = "ssh"), allow(dead_code))]
const DIR_STATUS_SCRIPT: &str = include_str!("shell/dir_status.sh");

/// State of a directory on a remote host.
#[cfg_attr(not(feature = "ssh"), allow(dead_code))]
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct DirStatus {
    /// Expanded (absolute or `~`-free) path that was inspected.
    pub dir: String,
    pub exists: bool,
    pub writable: bool,
    /// The directory does not exist but its parent is writable.
    pub creatable: bool,
    pub total_bytes: u64,
    pub available_bytes: u64,
    /// Whether [`Self::available_bytes`] is a real measurement.
    ///
    /// `false` means the platform could not report it (no `stat -f`, no `df`,
    /// unparsable output) — which is different from a genuine `0` on a full
    /// filesystem.
    pub space_known: bool,
    /// Path the space figures were taken from.
    ///
    /// Equals [`Self::dir`] when it exists; otherwise it is the nearest
    /// existing ancestor (often `/`), which is the filesystem that would hold
    /// a newly created directory.
    pub space_from: String,
    /// Why the directory is unusable, when it is.
    pub error: Option<String>,
}

#[cfg_attr(not(feature = "ssh"), allow(dead_code))]
impl DirStatus {
    fn parse(dir: String, stdout: &str) -> Self {
        let exists = stdout.contains("EXISTS=yes");
        let mut status = Self {
            writable: stdout.contains("WRITABLE=yes"),
            creatable: stdout.contains("CREATABLE=yes"),
            exists,
            space_from: get_str(stdout, "PROBE").unwrap_or_else(|| dir.clone()),
            dir,
            ..Default::default()
        };

        // `stat -f` markers (preferred, unambiguous), else the `df` table.
        if let (Some(total), Some(avail), Some(size)) = (
            get_num(stdout, "TOTAL_BLOCKS"),
            get_num(stdout, "AVAIL_BLOCKS"),
            get_num(stdout, "BLOCK_SIZE"),
        ) {
            if size > 0 {
                status.total_bytes = total.saturating_mul(size);
                status.available_bytes = avail.saturating_mul(size);
                status.space_known = true;
            }
        } else if let Some((total, available)) = parse_df(stdout) {
            status.total_bytes = total;
            status.available_bytes = available;
            status.space_known = true;
        }

        status.error = if !exists && !status.creatable {
            Some("directory does not exist and cannot be created".to_string())
        } else if exists && !status.writable {
            Some("directory is not writable".to_string())
        } else {
            None
        };

        status
    }

    /// Whether files can be written to the directory (creating it if needed).
    pub fn usable(&self) -> bool {
        self.writable || (!self.exists && self.creatable)
    }
}

/// Read a string `KEY=value` marker from probe output.
#[cfg_attr(not(feature = "ssh"), allow(dead_code))]
fn get_str(stdout: &str, key: &str) -> Option<String> {
    stdout.lines().find_map(|line| {
        let (name, value) = line.split_once('=')?;
        (name.trim() == key).then(|| value.trim().to_string())
    })
}

/// Read a numeric `KEY=value` marker from probe output.
#[cfg_attr(not(feature = "ssh"), allow(dead_code))]
fn get_num(stdout: &str, key: &str) -> Option<u64> {
    stdout.lines().find_map(|line| {
        let (name, value) = line.split_once('=')?;
        (name.trim() == key)
            .then(|| value.trim().parse::<u64>().ok())
            .flatten()
    })
}

/// Extract `(total, available)` bytes from `df -Pk` output.
///
/// Handles every `df` flavour encountered in practice:
///
/// * GNU coreutils prints a header followed by one row per filesystem.
/// * BusyBox (Alpine, embedded routers) prints the same header and then **no
///   rows at all** when the path is missing — `None` is returned.
/// * Long device names may wrap onto a second physical line; only lines whose
///   2nd and 4th fields are numeric are considered, scanning from the bottom.
#[cfg_attr(not(feature = "ssh"), allow(dead_code))]
fn parse_df(stdout: &str) -> Option<(u64, u64)> {
    stdout.lines().rev().find_map(|line| {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 4 {
            return None;
        }
        let kb = |value: &str| value.parse::<u64>().ok();
        Some((
            kb(fields[1])?.saturating_mul(1024),
            kb(fields[3])?.saturating_mul(1024),
        ))
    })
}

// ── Target / proxy catalogue ────────────────────────────────────────────
/// Rust target triples that commonly publish prebuilt release assets.
///
/// UIs can use this to populate a target dropdown; `--target` itself accepts
/// any triple `guess-target` understands.
pub const SUPPORTED_TARGETS: &[&str] = &[
    "x86_64-unknown-linux-musl",
    "x86_64-unknown-linux-gnu",
    "aarch64-unknown-linux-musl",
    "aarch64-unknown-linux-gnu",
    "armv7-unknown-linux-musleabihf",
    "armv7-unknown-linux-gnueabihf",
    "arm-unknown-linux-musleabihf",
    "arm-unknown-linux-gnueabihf",
    "i686-unknown-linux-musl",
    "i686-unknown-linux-gnu",
    "x86_64-apple-darwin",
    "aarch64-apple-darwin",
    "x86_64-pc-windows-msvc",
    "aarch64-pc-windows-msvc",
    "aarch64-linux-android",
    "armv7-linux-androideabi",
];

/// Download proxies accepted by `--proxy`.
pub const PROXIES: &[&str] = &["github", "gh-proxy", "xget", "jsdelivr", "statically"];

/// Target of the machine this binary was built for.
pub fn default_target() -> String {
    Target::default().to_string()
}

/// Whether `target` is one of [`SUPPORTED_TARGETS`].
pub fn is_supported_target(target: &str) -> bool {
    SUPPORTED_TARGETS.contains(&target)
}

#[cfg_attr(not(feature = "ssh"), allow(dead_code))]
impl RemotePlatform {
    /// Parse the `KEY=value` output of [`PLATFORM_PROBE`].
    fn parse(stdout: &str) -> Self {
        let mut platform = Self::default();
        for line in stdout.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim().to_string();
            match key.trim() {
                "OS" => platform.os = value,
                "ARCH" => platform.arch = value,
                "LIBC" => platform.libc = value,
                "ANDROID" => platform.android = value == "yes",
                "TARGET" => platform.target = value,
                _ => {}
            }
        }
        platform
    }

    /// Parsed target triple, when the detected value is a known target.
    pub fn target_triple(&self) -> Option<Target> {
        Target::from_str(&self.target).ok()
    }

    /// Short human readable description, e.g. `linux/aarch64 (musl)`.
    pub fn describe(&self) -> String {
        let mut s = format!("{}/{}", self.os.to_lowercase(), self.arch);
        if self.android {
            s.push_str(" android");
        } else if self.os == "Linux" && !self.libc.is_empty() {
            s.push_str(&format!(" ({})", self.libc));
        }
        s
    }
}

/// Credentials + connection details for the remote host.
///
/// The target directory (`--dir`) is kept on [`InstallConfig`][crate::InstallConfig];
/// this struct only holds what is needed to establish a connection.
#[derive(Debug, Clone, Default)]
pub struct RemoteConfig {
    pub user: String,
    pub host: String,
    pub port: u16,
    /// Password authentication (mutually exclusive with `identity`).
    pub password: Option<String>,
    /// Path to a private key file (local path).
    pub identity: Option<String>,
    /// Private key material, for embedders that store keys in memory
    /// instead of a file. Takes precedence over [`Self::identity`].
    pub key: Option<String>,
}

impl RemoteConfig {
    /// Human readable `user@host` label.
    pub fn label(&self) -> String {
        format!("{}@{}", self.user, self.host)
    }
}

/// Parse an scp-style destination: `[user@]host:/path`.
///
/// Returns `None` for anything that is not a remote spec, notably local
/// Windows paths (`C:\…`, `C:/…`) and plain directories.
pub fn parse_remote(value: &str) -> Option<(RemoteConfig, String)> {
    let value = value.trim();

    // Windows drive letters are local paths, not `host:path`.
    let bytes = value.as_bytes();
    if bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic() {
        return None;
    }

    let (authority, path) = value.split_once(':')?;
    if authority.is_empty() || authority.contains('/') || authority.contains('\\') {
        return None;
    }

    let (user, host) = match authority.rsplit_once('@') {
        Some((user, host)) => (user.to_string(), host.to_string()),
        None => (default_user(), authority.to_string()),
    };
    if host.is_empty() || user.is_empty() {
        return None;
    }

    let remote = RemoteConfig {
        user,
        host,
        port: 22,
        password: None,
        identity: None,
        key: None,
    };

    Some((remote, path.to_string()))
}

/// Best-effort local username, used when the destination omits `user@`.
fn default_user() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_default()
}

/// Quote a value for safe interpolation into a POSIX shell command.
#[cfg_attr(not(feature = "ssh"), allow(dead_code))]
pub fn sh_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

/// Accept every host key (matches `StrictHostKeyChecking=no`).
///
/// This mirrors the rest of `ei`, which only downloads release binaries over
/// TLS and has no host-key store. Callers that need pinning can wrap the
/// client themselves.
#[cfg(feature = "ssh")]
struct AcceptAnyKey;

#[cfg(feature = "ssh")]
impl client::Handler for AcceptAnyKey {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &PublicKeyOrCertificate,
    ) -> std::result::Result<bool, Self::Error> {
        Ok(true)
    }
}

/// A live SSH connection, reused for every file written during one install.
#[cfg(feature = "ssh")]
pub struct SshClient {
    handle: client::Handle<AcceptAnyKey>,
    /// Remote `$HOME`, cached for `~` expansion.
    home: String,
    target: RemoteConfig,
    /// Memoised platform probe (one SSH round-trip per install).
    platform: tokio::sync::OnceCell<RemotePlatform>,
}

#[cfg(feature = "ssh")]
impl SshClient {
    /// Connect and authenticate.
    pub async fn connect(target: &RemoteConfig, timeout_secs: u64) -> Result<Self> {
        let config = client::Config::default();
        let addr = (target.host.as_str(), target.port);

        let timeout = Duration::from_secs(timeout_secs.max(1));
        let mut handle = tokio::time::timeout(
            timeout,
            client::connect(Arc::new(config), addr, AcceptAnyKey),
        )
        .await
        .map_err(|_| err!("SSH connect to {} timed out", target.label()))?
        .with_context(|| format!("SSH connect to {} failed", target.label()))?;

        let auth = if let Some(key_str) = target.key.as_deref().filter(|k| !k.trim().is_empty()) {
            let key = decode_secret_key(key_str, None)
                .map_err(|e| err!("invalid SSH private key: {e}"))?;
            // RSA keys default to SHA-1 in russh, which modern servers reject.
            let key = PrivateKeyWithHashAlg::new(Arc::new(key), Some(HashAlg::Sha256));
            handle
                .authenticate_publickey(&target.user, key)
                .await
                .map_err(|e| err!("SSH authentication with key failed: {e}"))?
        } else if let Some(identity) = &target.identity {
            let path = crate::tool::expand_path(identity);
            let key_str = std::fs::read_to_string(&path)
                .with_context(|| format!("failed to read SSH key '{path}'"))?;
            let key = decode_secret_key(&key_str, None)
                .map_err(|e| err!("invalid SSH private key '{path}': {e}"))?;
            // RSA keys default to SHA-1 in russh, which modern servers reject.
            let key = PrivateKeyWithHashAlg::new(Arc::new(key), Some(HashAlg::Sha256));
            handle
                .authenticate_publickey(&target.user, key)
                .await
                .map_err(|e| err!("SSH authentication with key failed: {e}"))?
        } else if let Some(password) = &target.password {
            handle
                .authenticate_password(&target.user, password)
                .await
                .map_err(|e| err!("SSH password authentication failed: {e}"))?
        } else {
            return Err(err!(
                "no SSH credentials for {}: pass -p <password> or -i <private key>",
                target.label()
            ));
        };

        if !auth.success() {
            return Err(err!("SSH authentication rejected by {}", target.label()));
        }

        let mut client = Self {
            handle,
            home: String::new(),
            target: target.clone(),
            platform: tokio::sync::OnceCell::new(),
        };
        client.home = client
            .exec_sh(r#"printf '%s' "$HOME""#)
            .await
            .map(|o| o.stdout.trim().to_string())
            .unwrap_or_default();
        Ok(client)
    }

    /// Connection label (`user@host`).
    pub fn label(&self) -> String {
        self.target.label()
    }

    /// Remote `$HOME` (empty when the probe failed).
    pub fn home(&self) -> &str {
        &self.home
    }

    /// Detect the remote OS/arch/libc and the matching Rust target triple.
    ///
    /// The probe runs the same checks the mho installer uses, but over the
    /// existing SSH exec channel — no SFTP or agent forwarding required.
    /// The result is cached, so calling this repeatedly costs nothing.
    pub async fn detect_platform(&self) -> Result<RemotePlatform> {
        self.platform
            .get_or_try_init(|| async { self.probe_platform().await })
            .await
            .cloned()
    }

    async fn probe_platform(&self) -> Result<RemotePlatform> {
        let output = self.exec_sh(PLATFORM_PROBE).await?;
        if !output.success() {
            return Err(err!(
                "failed to detect remote platform on {}: {}",
                self.label(),
                output.stderr.trim()
            ));
        }
        Ok(RemotePlatform::parse(&output.stdout))
    }

    /// Resolve `path_or_name` to an executable on the remote host.
    ///
    /// Accepts an absolute or `~`-relative path, or a bare command name that
    /// is looked up in `PATH`. Returns `None` when nothing executable matches.
    pub async fn resolve_executable(&self, path_or_name: &str) -> Option<String> {
        let path = self.expand_path(path_or_name);
        let quoted = sh_quote(&path);
        let script = format!(
            "if [ -x {quoted} ]; then printf %s {quoted}; \
             else command -v {} 2>/dev/null || true; fi",
            sh_quote(path_or_name)
        );
        let found = self.exec_sh(&script).await.ok()?;
        let found = found.stdout.trim();
        (!found.is_empty()).then(|| found.to_string())
    }

    /// Inspect a directory on the remote host.
    ///
    /// Reports whether it exists, whether it can be written to (or created),
    /// and the free/total space of the filesystem that holds it.
    pub async fn dir_status(&self, dir: &str) -> Result<DirStatus> {
        let dir = self.expand_path(dir);
        let script = DIR_STATUS_SCRIPT.replace("__DIR__", &sh_quote(&dir));
        let output = self.exec_sh(&script).await?;
        if !output.success() {
            return Err(err!(
                "failed to inspect '{dir}' on {}: {}",
                self.label(),
                output.stderr.trim()
            ));
        }
        Ok(DirStatus::parse(dir, &output.stdout))
    }

    /// Run a POSIX shell script, regardless of the login shell.
    ///
    /// The remote shell may be fish or zsh, which reject POSIX constructs
    /// such as `d=$(…)` or `while …; do …; done`. Handing the script to
    /// `sh -c` keeps behaviour identical everywhere.
    pub async fn exec_sh(&self, script: &str) -> Result<CommandResult> {
        self.exec(&format!("sh -c {}", sh_quote(script))).await
    }

    /// Run a command with no stdin and capture its output.
    pub async fn exec(&self, command: &str) -> Result<CommandResult> {
        self.exec_with_input(command, None).await
    }

    /// Run a command, optionally streaming `input` to its stdin.
    ///
    /// The write runs **concurrently** with the read loop. Writing first and
    /// reading afterwards deadlocks whenever the remote command exits early
    /// (disk full, `cat: write error`): the command no longer consumes stdin,
    /// the SSH window never reopens, and the pending write only completes
    /// after the client timeout — or never.
    pub async fn exec_with_input(
        &self,
        command: &str,
        input: Option<Vec<u8>>,
    ) -> Result<CommandResult> {
        let channel = self
            .handle
            .channel_open_session()
            .await
            .map_err(|e| err!("failed to open SSH channel: {e}"))?;

        channel
            .exec(true, command.as_bytes())
            .await
            .map_err(|e| err!("failed to execute remote command: {e}"))?;

        // Split into read/write halves so the payload can be streamed while
        // the read loop drains output.
        let (mut rx, tx) = channel.split();

        let writer = tokio::spawn(async move {
            if let Some(data) = input.filter(|data| !data.is_empty()) {
                if tx.data_bytes(data).await.is_err() {
                    // The command stopped reading (early exit, disk full…);
                    // the read loop below still collects its output.
                    return;
                }
            }
            // Always signal EOF so commands that read stdin terminate.
            let _ = tx.eof().await;
        });

        let mut out = CommandResult::default();
        loop {
            match rx.wait().await {
                Some(russh::ChannelMsg::Data { data }) => {
                    out.stdout.push_str(&String::from_utf8_lossy(&data));
                }
                Some(russh::ChannelMsg::ExtendedData { data, .. }) => {
                    out.stderr.push_str(&String::from_utf8_lossy(&data));
                }
                Some(russh::ChannelMsg::ExitStatus { exit_status }) => {
                    out.exit_code = exit_status as i32;
                }
                Some(russh::ChannelMsg::Eof) | Some(russh::ChannelMsg::Close) | None => break,
                _ => {}
            }
        }

        // The command may have exited without draining stdin; stop the writer
        // instead of waiting for window space that will never be granted.
        writer.abort();

        Ok(out)
    }

    /// Run a command and fail with stderr when it exits non-zero.
    pub async fn exec_checked(&self, command: &str) -> Result<CommandResult> {
        let output = self.exec(command).await?;
        if !output.success() {
            return Err(err!(
                "remote command failed (exit {}): {}",
                output.exit_code,
                output.stderr.trim()
            ));
        }
        Ok(output)
    }

    /// Expand `~` and relative paths against the remote home directory.
    pub fn expand_path(&self, path: &str) -> String {
        expand_with_home(path, &self.home)
    }

    /// Create a directory (and parents) on the remote host.
    pub async fn ensure_dir(&self, dir: &str) -> Result<()> {
        self.exec_checked(&format!("mkdir -p {}", sh_quote(dir)))
            .await
            .map(|_| ())
    }

    /// Write `data` to `path` on the remote host.
    ///
    /// The write is verified by size: a full disk makes `cat` exit early, and
    /// without this check the install would silently report success after
    /// writing a truncated binary.
    pub async fn write_file(&self, path: &str, data: &[u8], mode: Option<u32>) -> Result<()> {
        if let Some(parent) = parent_dir(path) {
            self.ensure_dir(&parent).await?;
        }

        let expected = data.len() as u64;
        let quoted = sh_quote(path);
        let mut command = format!("cat > {quoted}");
        if let Some(mode) = mode.filter(|m| *m > 0) {
            command.push_str(&format!(" && chmod {:o} {quoted}", mode));
        }
        // Report the resulting size so a short write is detected below.
        command.push_str(&format!(" && wc -c < {quoted} | tr -d ' \\n'"));

        let output = self.exec_with_input(&command, Some(data.to_vec())).await?;
        let stderr = output.stderr.trim();
        if !output.success() {
            return Err(err!(
                "failed to write '{path}' on {}: {}",
                self.label(),
                if stderr.is_empty() {
                    format!("exit {}", output.exit_code)
                } else {
                    stderr.to_string()
                }
            ));
        }

        let written: u64 = output.stdout.trim().parse().unwrap_or(0);
        if written != expected {
            return Err(err!(
                "wrote {written} of {expected} bytes to '{path}' on {} (disk full?)",
                self.label()
            ));
        }

        Ok(())
    }

    /// Whether a remote path exists.
    #[allow(dead_code)] // part of the public backend API
    pub async fn exists(&self, path: &str) -> bool {
        self.exec(&format!("test -e {}", sh_quote(path)))
            .await
            .map(|o| o.success())
            .unwrap_or(false)
    }

    /// Remove a remote file (ignoring missing files).
    #[allow(dead_code)] // part of the public backend API
    pub async fn remove_file(&self, path: &str) -> Result<()> {
        self.exec_checked(&format!("rm -f {}", sh_quote(path)))
            .await
            .map(|_| ())
    }

    /// Add the executable bit to a remote file.
    pub async fn make_executable(&self, path: &str) -> Result<()> {
        self.exec_checked(&format!("chmod +x {}", sh_quote(path)))
            .await
            .map(|_| ())
    }

    /// Free space (bytes) of the filesystem containing `dir`.
    ///
    /// Returns `None` when the platform cannot report it; a real `0` (full
    /// filesystem) is returned as `Some(0)`.
    pub async fn available_space(&self, dir: &str) -> Result<Option<u64>> {
        // Reuses the same probe as [`Self::dir_status`], so there is one
        // implementation of the platform quirks (BusyBox, 32-bit ash, …).
        let status = self.dir_status(dir).await?;
        Ok(status.space_known.then_some(status.available_bytes))
    }
}

/// Parent directory of a POSIX path.
#[cfg_attr(not(feature = "ssh"), allow(dead_code))]
fn parent_dir(path: &str) -> Option<String> {
    path.rfind('/').map(|i| {
        if i == 0 {
            "/".to_string()
        } else {
            path[..i].to_string()
        }
    })
}

/// Expand `~` / relative paths against `home`.
#[cfg_attr(not(feature = "ssh"), allow(dead_code))]
fn expand_with_home(path: &str, home: &str) -> String {
    let home = home.trim_end_matches('/');
    if path == "~" {
        return home.to_string();
    }
    if let Some(rest) = path.strip_prefix("~/") {
        return format!("{home}/{rest}");
    }
    if path.starts_with('/') {
        return path.to_string();
    }
    format!("{home}/{path}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_remote_specs() {
        let (cfg, path) = parse_remote("root@192.168.1.100:/root/").unwrap();
        assert_eq!(cfg.user, "root");
        assert_eq!(cfg.host, "192.168.1.100");
        assert_eq!(path, "/root/");

        let (cfg, path) = parse_remote("me@example.com:~/.ei").unwrap();
        assert_eq!(cfg.user, "me");
        assert_eq!(path, "~/.ei");
    }

    #[test]
    fn ignores_local_paths() {
        assert!(parse_remote("C:\\Users\\x").is_none());
        assert!(parse_remote("C:/Users/x").is_none());
        assert!(parse_remote("./out").is_none());
        assert!(parse_remote("relative").is_none());
        assert!(parse_remote("/tmp/out").is_none());
    }

    #[test]
    fn quotes_shell_values() {
        assert_eq!(sh_quote("a'b"), r"'a'\''b'");
    }

    #[test]
    fn expands_remote_paths() {
        assert_eq!(expand_with_home("~", "/home/me"), "/home/me");
        assert_eq!(expand_with_home("~/.ei", "/home/me"), "/home/me/.ei");
        assert_eq!(expand_with_home("/opt/bin", "/home/me"), "/opt/bin");
        assert_eq!(expand_with_home(".ei", "/home/me"), "/home/me/.ei");
    }

    #[test]
    fn finds_parent_dirs() {
        assert_eq!(parent_dir("/root/mho").as_deref(), Some("/root"));
        assert_eq!(parent_dir("mho"), None);
    }

    #[test]
    fn parses_platform_probe_output() {
        let out =
            "OS=Linux\nARCH=aarch64\nLIBC=musl\nANDROID=no\nTARGET=aarch64-unknown-linux-musl\n";
        let p = RemotePlatform::parse(out);
        assert_eq!(p.os, "Linux");
        assert_eq!(p.arch, "aarch64");
        assert_eq!(p.libc, "musl");
        assert!(!p.android);
        assert_eq!(p.target, "aarch64-unknown-linux-musl");
        assert_eq!(
            p.target_triple().map(|t| t.to_str()),
            Some("aarch64-unknown-linux-musl")
        );
        assert_eq!(p.describe(), "linux/aarch64 (musl)");
    }

    #[test]
    fn parses_unsupported_platform() {
        let p = RemotePlatform::parse("OS=Plan9\nARCH=mips\nLIBC=gnu\nANDROID=no\nTARGET=\n");
        assert_eq!(p.target, "");
        assert!(p.target_triple().is_none());
    }

    #[test]
    fn parses_android_platform() {
        let p = RemotePlatform::parse(
            "OS=Linux\nARCH=armv7l\nLIBC=gnu\nANDROID=yes\nTARGET=armv7-linux-androideabi\n",
        );
        assert!(p.android);
        assert_eq!(p.describe(), "linux/armv7l android");
        assert_eq!(
            p.target_triple().map(|t| t.to_str()),
            Some("armv7-linux-androideabi")
        );
    }

    /// Run the probe locally (skipped when no POSIX shell is available) to
    /// make sure the script is syntactically valid and emits a target.
    #[test]
    fn probe_script_runs() {
        use std::process::Command;

        let Ok(output) = Command::new("sh").arg("-c").arg(PLATFORM_PROBE).output() else {
            eprintln!("skipping: no `sh` available");
            return;
        };
        assert!(output.status.success(), "probe script failed");
        let platform = RemotePlatform::parse(&String::from_utf8_lossy(&output.stdout));
        assert!(!platform.os.is_empty());
        assert!(!platform.arch.is_empty());
        assert!(
            platform.target_triple().is_some(),
            "unexpected target {:?} on {}",
            platform.target,
            platform.describe()
        );
    }

    #[test]
    fn parses_dir_status_output() {
        let stdout = "EXISTS=yes\nWRITABLE=yes\noverlay 1000 10 900 1% /root\n";
        let status = DirStatus::parse("/root".into(), stdout);
        assert!(status.exists && status.writable && status.usable());
        assert_eq!(status.total_bytes, 1000 * 1024);
        assert_eq!(status.available_bytes, 900 * 1024);
        assert_eq!(status.error, None);
    }

    #[test]
    fn reports_unusable_dir_status() {
        let status = DirStatus::parse("/sys/locked".into(), "overlay 1000 10 900 1% /\n");
        assert!(!status.exists && !status.creatable && !status.usable());
        assert!(status.error.is_some());
    }

    #[test]
    fn parses_df_header_only_as_unknown() {
        // BusyBox prints the header and no rows when the path is missing.
        let out = "Filesystem           1024-blocks    Used Available Capacity Mounted on\n";
        assert_eq!(parse_df(out), None);
        assert_eq!(DirStatus::parse("/x".into(), out).total_bytes, 0);
    }

    #[test]
    fn parses_df_skipping_noise() {
        let out = "df: /root/.mho: No such file or directory\n\
                   Filesystem 1024-blocks Used Available Capacity Mounted on\n\
                   /dev/root 8000000 1000 7000000 12% /\n";
        assert_eq!(parse_df(out), Some((8_000_000 * 1024, 7_000_000 * 1024)));
    }

    #[test]
    fn full_filesystem_is_known_and_zero() {
        // A 100%-full partition reports 0 available; that must not be
        // confused with "unknown" (which would skip the disk-space check).
        let out = "EXISTS=yes\nWRITABLE=yes\nTOTAL_BLOCKS=15264\nAVAIL_BLOCKS=0\nBLOCK_SIZE=4096\n";
        let status = DirStatus::parse("/".into(), out);
        assert!(status.space_known);
        assert_eq!(status.available_bytes, 0);
        assert_eq!(status.total_bytes, 15264 * 4096);
    }

    #[test]
    fn unknown_space_is_not_reported_as_zero() {
        // BusyBox `df` printed only a header and `stat -f` was unavailable.
        let out = "EXISTS=yes\nFilesystem 1024-blocks Used Available Capacity Mounted on\n";
        let status = DirStatus::parse("/x".into(), out);
        assert!(!status.space_known);
    }

    #[test]
    fn tracks_which_path_was_measured() {
        // DIR does not exist: the space figures come from `/`.
        let out = "CREATABLE=yes\nPROBE=/\nTOTAL_BLOCKS=100\nAVAIL_BLOCKS=0\nBLOCK_SIZE=4096\n";
        let status = DirStatus::parse("/mhossh3".into(), out);
        assert_eq!(status.dir, "/mhossh3");
        assert_eq!(status.space_from, "/");
        assert_eq!(status.available_bytes, 0);
    }

    #[test]
    fn measured_path_defaults_to_dir() {
        let out = "EXISTS=yes\nWRITABLE=yes\nTOTAL_BLOCKS=100\nAVAIL_BLOCKS=50\nBLOCK_SIZE=1024\n";
        let status = DirStatus::parse("/root".into(), out);
        assert_eq!(status.space_from, "/root");
    }

    #[test]
    fn parses_stat_space_markers() {
        let out = "EXISTS=yes\nWRITABLE=yes\nTOTAL_BLOCKS=8000000\nAVAIL_BLOCKS=7000000\nBLOCK_SIZE=4096\n";
        let status = DirStatus::parse("/root".into(), out);
        assert_eq!(status.total_bytes, 8_000_000 * 4096);
        assert_eq!(status.available_bytes, 7_000_000 * 4096);
    }

    #[test]
    fn falls_back_to_df_when_stat_is_missing() {
        // Old BusyBox without `stat -f`: only the df table is printed.
        let out = "EXISTS=yes\nWRITABLE=yes\n\
                   Filesystem 1024-blocks Used Available Capacity Mounted on\n\
                   /dev/root 8000000 1000 7000000 12% /\n";
        let status = DirStatus::parse("/root".into(), out);
        assert_eq!(status.available_bytes, 7_000_000 * 1024);
    }

    /// Run the directory probe locally so the script stays POSIX-valid.
    #[test]
    fn dir_status_script_runs() {
        use std::process::Command;

        let script = DIR_STATUS_SCRIPT.replace("__DIR__", "'~/.ei-dir-status-test'");
        let Ok(output) = Command::new("sh").arg("-c").arg(&script).output() else {
            eprintln!("skipping: no `sh` available");
            return;
        };
        assert!(
            output.status.success(),
            "script failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let status = DirStatus::parse(
            "~/.ei-dir-status-test".into(),
            &String::from_utf8_lossy(&output.stdout),
        );
        assert!(status.usable(), "unexpected status: {status:?}");
        assert!(status.available_bytes > 0, "df was not parsed");
    }
}
