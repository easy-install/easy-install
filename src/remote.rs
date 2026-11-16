//! Remote installation over SSH.
//!
//! `ei` normally writes files to the local filesystem. Supplying a
//! destination like `user@host:/path` (or `-d user@host:/path`) switches the
//! install to a remote host: the artifact is still downloaded locally, then
//! streamed over SSH and written there, and `strip`/`upx` are executed on the
//! remote machine.
//!
//! Destination parsing is always compiled. The SSH transport
//! (connect/exec/filesystem), `uname` probing and platform detection come
//! from the [`sssh`] crate behind the `ssh` feature (enabled by default);
//! the mapping from those facts to a target triple is
//! [`guess_target::guess_system_target`], shared with other tools.

use guess_target::{Libc, SystemInfo, Target, guess_system_target};
use std::str::FromStr;

#[cfg(feature = "ssh")]
use crate::error::{Context, Result, err};
#[cfg(feature = "ssh")]
use sssh::{Auth, Client, Fs};

/// Platform information detected on the remote host.
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

impl RemotePlatform {
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

/// Detect the remote platform through an authenticated [`Client`].
///
/// Only the *facts* are gathered here ([`Client::uname`] plus libc/Android
/// probes); the mapping to a Rust target triple lives in
/// [`guess_target::guess_system_target`], shared with every other consumer.
/// The result is cached by [`SshHost`](crate::backend::SshHost).
#[cfg(feature = "ssh")]
pub(crate) async fn detect_platform(client: &Client, fs: &Fs) -> Result<RemotePlatform> {
    let uname = client
        .uname()
        .await
        .map_err(|e| err!("failed to run `uname` on {}: {e}", client.label()))?;

    let os = uname.sysname;
    let arch = uname.machine;
    let android = uname
        .os
        .as_deref()
        .is_some_and(|kernel| kernel.eq_ignore_ascii_case("android"))
        || (os == "Linux" && fs.try_exists("/system/build.prop").await.unwrap_or(false));
    let libc = if os == "Linux" && !android {
        detect_libc(client, fs).await
    } else {
        Libc::Gnu
    };

    let info = SystemInfo::new(&os, &arch)
        .with_libc(libc)
        .with_android(android);
    let target = guess_system_target(&info)
        .map(|target| target.to_str().to_string())
        .unwrap_or_default();

    Ok(RemotePlatform {
        os,
        arch,
        libc: libc.to_string(),
        android,
        target,
    })
}

/// Distinguish musl from glibc on Linux.
#[cfg(feature = "ssh")]
async fn detect_libc(client: &Client, fs: &Fs) -> Libc {
    // Alpine names its libc explicitly in os-release.
    if let Ok(release) = fs.read_to_string("/etc/os-release").await
        && release.to_ascii_lowercase().contains("alpine")
    {
        return Libc::Musl;
    }

    for loader in [
        "/lib/ld-musl-x86_64.so.1",
        "/lib/ld-musl-aarch64.so.1",
        "/lib/ld-musl-armhf.so.1",
    ] {
        if fs.try_exists(loader).await.unwrap_or(false) {
            return Libc::Musl;
        }
    }

    if let Ok(output) = client.exec("ldd --version").await {
        let text = format!("{}\n{}", output.stdout_lossy(), output.stderr_lossy());
        if text.to_ascii_lowercase().contains("musl") {
            return Libc::Musl;
        }
    }

    Libc::Gnu
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

    /// Connect over SSH with the first credential available, in order:
    /// in-memory key → key file → password.
    #[cfg(feature = "ssh")]
    pub async fn connect(&self, timeout_secs: u64) -> Result<Client> {
        let target = format!("{}@{}", self.user, self.host);
        Client::connect_target(&target, self.port, self.auth()?, Some(timeout_secs.max(1)))
            .await
            .map_err(|e| err!("SSH connection to {} failed: {e}", self.label()))
    }

    #[cfg(feature = "ssh")]
    fn auth(&self) -> Result<Auth> {
        if let Some(key) = self.key.as_deref().filter(|key| !key.trim().is_empty()) {
            return Ok(Auth::key(key));
        }
        if let Some(identity) = &self.identity {
            let path = crate::tool::expand_path(identity);
            let key = std::fs::read_to_string(&path)
                .with_context(|| format!("failed to read SSH key '{path}'"))?;
            return Ok(Auth::key(key));
        }
        if let Some(password) = &self.password {
            return Ok(Auth::password(password));
        }
        Err(err!(
            "no SSH credentials for {}: pass -p <password> or -i <private key>",
            self.label()
        ))
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

    #[cfg(feature = "ssh")]
    #[test]
    fn auth_prefers_key_then_file_then_password() {
        let mut cfg = RemoteConfig {
            user: "u".into(),
            host: "h".into(),
            key: Some("MEMORY".into()),
            identity: Some("/does/not/exist".into()),
            password: Some("PW".into()),
            ..Default::default()
        };
        match cfg.auth().unwrap() {
            Auth::PrivateKey(key) => assert_eq!(key, "MEMORY"),
            _ => panic!("expected the in-memory key"),
        }

        let path = std::env::temp_dir().join("ei-ssh-auth-test-key");
        std::fs::write(&path, "FILE").unwrap();
        cfg.key = None;
        cfg.identity = Some(path.to_string_lossy().into_owned());
        match cfg.auth().unwrap() {
            Auth::PrivateKey(key) => assert_eq!(key, "FILE"),
            _ => panic!("expected the key file"),
        }
        let _ = std::fs::remove_file(path);

        cfg.identity = None;
        match cfg.auth().unwrap() {
            Auth::Password(password) => assert_eq!(password, "PW"),
            _ => panic!("expected the password"),
        }
    }
    fn platform(os: &str, arch: &str, libc: Libc, android: bool) -> RemotePlatform {
        let info = SystemInfo::new(os, arch)
            .with_libc(libc)
            .with_android(android);
        RemotePlatform {
            os: os.to_string(),
            arch: arch.to_string(),
            libc: libc.to_string(),
            android,
            target: guess_system_target(&info)
                .map(|target| target.to_str().to_string())
                .unwrap_or_default(),
        }
    }

    /// The guessing itself lives in `guess-target`; this only checks that the
    /// detected facts reach it and the triple comes back.
    #[test]
    fn maps_detected_facts_to_targets() {
        assert_eq!(
            platform("Linux", "x86_64", Libc::Musl, false).target,
            "x86_64-unknown-linux-musl"
        );
        assert_eq!(
            platform("Linux", "aarch64", Libc::Gnu, false).target,
            "aarch64-unknown-linux-musl"
        );
        assert_eq!(
            platform("Linux", "armv7l", Libc::Gnu, true).target,
            "armv7-linux-androideabi"
        );
        assert_eq!(
            platform("MINGW64_NT-10.0", "x86_64", Libc::Gnu, false).target,
            "x86_64-pc-windows-msvc"
        );
        assert_eq!(platform("Plan9", "mips", Libc::Gnu, false).target, "");
    }

    #[test]
    fn describes_platforms() {
        let linux = platform("Linux", "aarch64", Libc::Musl, false);
        assert_eq!(linux.describe(), "linux/aarch64 (musl)");
        assert_eq!(
            linux.target_triple().map(|target| target.to_str()),
            Some("aarch64-unknown-linux-musl")
        );

        let android = platform("Linux", "armv7l", Libc::Gnu, true);
        assert!(android.android);
        assert_eq!(android.describe(), "linux/armv7l android");
        assert_eq!(
            android.target_triple().map(|target| target.to_str()),
            Some("armv7-linux-androideabi")
        );

        let unsupported = platform("Plan9", "mips", Libc::Gnu, false);
        assert_eq!(unsupported.target, "");
        assert!(unsupported.target_triple().is_none());
    }
}
