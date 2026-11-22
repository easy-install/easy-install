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

use crate::InstallConfig;
use crate::backend::{Backend, Host};
use crate::error::{Result, err};
// The probe helpers are only used by the `ssh` transport, but the platform
// tests exercise them in every build, so the import stays unconditional.
#[cfg_attr(not(feature = "ssh"), allow(unused_imports))]
use guess_target::{Libc, SystemInfo, Target, guess_system_target};
use std::str::FromStr;

#[cfg(feature = "ssh")]
use crate::error::Context;
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
    /// SSH port (`22`, a `host:port:path` destination, or `-P`).
    pub port: u16,
    /// Password authentication (mutually exclusive with the key fields).
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

    /// Connect over SSH using the configured authentication method.
    ///
    /// Crate-internal: the returned client type belongs to the optional
    /// `sssh` dependency and is not part of `ei`'s public API.
    #[cfg(feature = "ssh")]
    pub(crate) async fn connect(&self, timeout_secs: u64) -> Result<Client> {
        let target = format!("{}@{}", self.user, self.host);
        Client::connect_target(&target, self.port, self.auth()?, Some(timeout_secs.max(1)))
            .await
            .map_err(|e| err!("SSH connection to {} failed: {e}", self.label()))
    }

    /// Build the [`Auth`] to use.
    ///
    /// Exactly one method must be configured: a private key (in-memory `key`
    /// preferred over the `identity` file) or a password. Mixing them is an
    /// error instead of silently preferring one, so a mistyped flag never
    /// connects with credentials the user did not intend.
    #[cfg(feature = "ssh")]
    fn auth(&self) -> Result<Auth> {
        let key = self.key.as_deref().filter(|key| !key.trim().is_empty());
        let identity = self.identity.as_deref().filter(|path| !path.is_empty());
        let password = self.password.as_deref().filter(|pass| !pass.is_empty());

        if password.is_some() && (key.is_some() || identity.is_some()) {
            return Err(err!(
                "{} received both a password and a private key; use exactly one authentication method (-p or -i)",
                self.label()
            ));
        }

        if let Some(key) = key {
            return Ok(Auth::key(key));
        }
        if let Some(identity) = identity {
            let path = crate::path::expand_path(identity);
            let key = std::fs::read_to_string(&path)
                .with_context(|| format!("failed to read SSH key '{path}'"))?;
            return Ok(Auth::key(key));
        }
        if let Some(password) = password {
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
// Only the CLI turns `--dir` strings into remote specs; embedders set
// `InstallConfig::remote` directly.
#[cfg_attr(not(feature = "cli"), allow(dead_code))]
pub fn parse_remote(value: &str) -> Option<(RemoteConfig, String)> {
    let value = value.trim();

    // Windows drive letters are local paths, not `host:path`.
    let bytes = value.as_bytes();
    if bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic() {
        return None;
    }

    let (authority, rest) = value.split_once(':')?;
    if authority.is_empty() || authority.contains('/') || authority.contains('\\') {
        return None;
    }

    // `host:2222:/path` — a numeric segment right after the host is a port;
    // anything else keeps the whole remainder as the path, so paths that
    // themselves contain a colon (`host:/tmp/a:b`) still work.
    let (port, path) = match rest.split_once(':') {
        Some((digits, path)) => match digits.parse::<u16>() {
            Ok(port) if port != 0 && digits.bytes().all(|b| b.is_ascii_digit()) => (port, path),
            _ => (22, rest),
        },
        None => (22, rest),
    };

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
        port,
        password: None,
        identity: None,
        key: None,
    };

    Some((remote, path.to_string()))
}

/// Best-effort local username, used when the destination omits `user@`.
#[cfg_attr(not(feature = "cli"), allow(dead_code))]
fn default_user() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_default()
}

/// Pick the artifact target for a remote install.
///
/// The local platform says nothing about the remote host, so it is never used
/// as a fallback: the remote platform is probed instead, and the install fails
/// with instructions when that is impossible. An explicit `target` always wins
/// and skips the probe.
pub(crate) async fn resolve_target(
    url: &str,
    config: &mut InstallConfig,
    backend: &Backend,
) -> Result<()> {
    let Some(remote) = &config.remote else {
        return Ok(());
    };
    if config.target.is_some() {
        return Ok(());
    }

    if let Some(targets) = backend
        .targets()
        .await
        .filter(|targets| !targets.is_empty())
    {
        if !config.quiet {
            let platform = backend.platform().await.unwrap_or_else(|| "unknown".into());
            println!(
                "Detected remote platform: {platform} → {}",
                join_targets(&targets)
            );
        }
        // The SSH probe reports exactly one usable triple. Store it in
        // `target` on this (per-invocation) clone so every artifact-matching
        // helper sees the remote platform instead of the local one.
        config.target = targets.first().copied();
        return Ok(());
    }

    let local = join_targets(guess_target::get_local_target());
    let path = config.dir.as_deref().unwrap_or_default();
    let dest = format!("{}:{path}", remote.label());
    let port = if remote.port == 22 {
        String::new()
    } else {
        format!(" -P {}", remote.port)
    };
    Err(err!(
        "could not detect the target platform of {}

  Refusing to fall back to the LOCAL platform ({local}), which is almost
  never the remote host's platform.

  Pass the remote platform explicitly, e.g.:
    ei {url} -d {dest}{port} --target aarch64-unknown-linux-gnu
    ei {url} -d {dest}{port} --target x86_64-unknown-linux-musl",
        remote.label()
    ))
}

/// Comma-separated target triples, for log and error messages.
fn join_targets(targets: &[Target]) -> String {
    targets
        .iter()
        .map(|target| target.to_str())
        .collect::<Vec<_>>()
        .join(", ")
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
        assert_eq!(cfg.port, 22);
    }

    #[test]
    fn parses_port_in_the_destination() {
        let (cfg, path) = parse_remote("ace@192.168.50.1:9421:/tmp/ei-ssh").unwrap();
        assert_eq!(cfg.user, "ace");
        assert_eq!(cfg.host, "192.168.50.1");
        assert_eq!(cfg.port, 9421);
        assert_eq!(path, "/tmp/ei-ssh");

        let (cfg, path) = parse_remote("host:2222:~/.ei").unwrap();
        assert_eq!(cfg.port, 2222);
        assert_eq!(path, "~/.ei");

        // A non-numeric segment is part of the path, not a port.
        let (cfg, path) = parse_remote("host:/tmp/a:b").unwrap();
        assert_eq!(cfg.port, 22);
        assert_eq!(path, "/tmp/a:b");

        // Out-of-range numbers are not ports either.
        let (cfg, path) = parse_remote("host:99999:/x").unwrap();
        assert_eq!(cfg.port, 22);
        assert_eq!(path, "99999:/x");
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
    fn auth_accepts_exactly_one_method() {
        let base = RemoteConfig {
            user: "u".into(),
            host: "h".into(),
            ..Default::default()
        };

        // In-memory key wins over the key file, and never needs the file.
        let cfg = RemoteConfig {
            key: Some("MEMORY".into()),
            identity: Some("/does/not/exist".into()),
            ..base.clone()
        };
        match cfg.auth().unwrap() {
            Auth::PrivateKey(key) => assert_eq!(key, "MEMORY"),
            _ => panic!("expected the in-memory key"),
        }

        let path = std::env::temp_dir().join("ei-ssh-auth-test-key");
        std::fs::write(&path, "FILE").unwrap();
        let cfg = RemoteConfig {
            identity: Some(path.to_string_lossy().into_owned()),
            ..base.clone()
        };
        match cfg.auth().unwrap() {
            Auth::PrivateKey(key) => assert_eq!(key, "FILE"),
            _ => panic!("expected the key file"),
        }
        let _ = std::fs::remove_file(path);

        let cfg = RemoteConfig {
            password: Some("PW".into()),
            ..base.clone()
        };
        match cfg.auth().unwrap() {
            Auth::Password(password) => assert_eq!(password, "PW"),
            _ => panic!("expected the password"),
        }

        // Mixing methods is rejected instead of silently preferring one.
        let cfg = RemoteConfig {
            password: Some("PW".into()),
            identity: Some("/tmp/key".into()),
            ..base.clone()
        };
        assert!(cfg.auth().is_err());

        assert!(base.auth().is_err(), "no credentials at all");
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
