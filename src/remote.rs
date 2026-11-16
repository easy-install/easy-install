//! Remote installation over SSH.
//!
//! `ei` normally writes files to the local filesystem. Supplying a
//! destination like `user@host:/path` (or `-d user@host:/path`) switches the
//! install to a remote host: the artifact is still downloaded locally, then
//! streamed over SSH and written there, and `strip`/`upx` are executed on the
//! remote machine.
//!
//! Destination parsing and target mapping are always compiled. The SSH
//! transport (connect/exec/filesystem), `uname` probing and platform
//! detection come from the [`sssh`] crate behind the `ssh` feature (enabled
//! by default).

use guess_target::Target;
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
/// Uses [`Client::uname`] plus a few filesystem/exec probes, then maps the
/// fields to a Rust target triple with the same rules the old shell probe
/// used. The result is cached by [`SshHost`](crate::backend::SshHost).
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
        "gnu".to_string()
    };
    let target = target_triple(&os, &arch, &libc, android);

    Ok(RemotePlatform {
        os,
        arch,
        libc,
        android,
        target,
    })
}

/// Distinguish musl from glibc on Linux.
#[cfg(feature = "ssh")]
async fn detect_libc(client: &Client, fs: &Fs) -> String {
    // Alpine names its libc explicitly in os-release.
    if let Ok(release) = fs.read_to_string("/etc/os-release").await
        && release.to_ascii_lowercase().contains("alpine")
    {
        return "musl".to_string();
    }

    for loader in [
        "/lib/ld-musl-x86_64.so.1",
        "/lib/ld-musl-aarch64.so.1",
        "/lib/ld-musl-armhf.so.1",
    ] {
        if fs.try_exists(loader).await.unwrap_or(false) {
            return "musl".to_string();
        }
    }

    if let Ok(output) = client.exec("ldd --version").await {
        let text = format!("{}\n{}", output.stdout_lossy(), output.stderr_lossy());
        if text.to_ascii_lowercase().contains("musl") {
            return "musl".to_string();
        }
    }

    "gnu".to_string()
}

/// Map `uname` fields to a Rust target triple.
///
/// Mirrors the rules the previous shell probe implemented; ARM prefers musl
/// on Linux because glibc builds often fail on those systems.
fn target_triple(os: &str, arch: &str, libc: &str, android: bool) -> String {
    let arch = arch.to_ascii_lowercase();
    let libc = libc.to_ascii_lowercase();
    let armv7 = arch.starts_with("armv7") || arch == "armv8l";
    let armv6 = arch.starts_with("armv6") || arch == "arm";

    if os == "Linux" {
        if android {
            return match arch.as_str() {
                "aarch64" | "arm64" => "aarch64-linux-android",
                "i686" | "x86" => "i686-linux-android",
                "x86_64" => "x86_64-linux-android",
                _ if armv7 => "armv7-linux-androideabi",
                _ => "",
            }
            .to_string();
        }

        return match arch.as_str() {
            "x86_64" | "amd64" => format!("x86_64-unknown-linux-{libc}"),
            "aarch64" | "arm64" if libc == "gnu" => "aarch64-unknown-linux-musl".to_string(),
            "aarch64" | "arm64" => format!("aarch64-unknown-linux-{libc}"),
            "i686" | "i386" => format!("i686-unknown-linux-{libc}"),
            "riscv64" => format!("riscv64gc-unknown-linux-{libc}"),
            "loongarch64" => format!("loongarch64-unknown-linux-{libc}"),
            "s390x" => "s390x-unknown-linux-gnu".to_string(),
            "ppc64le" | "powerpc64le" => format!("powerpc64le-unknown-linux-{libc}"),
            _ if armv7 && libc == "gnu" => "armv7-unknown-linux-musleabihf".to_string(),
            _ if armv7 => format!("armv7-unknown-linux-{libc}eabihf"),
            _ if armv6 && libc == "gnu" => "arm-unknown-linux-musleabihf".to_string(),
            _ if armv6 => format!("arm-unknown-linux-{libc}eabihf"),
            _ => String::new(),
        };
    }

    if os == "Darwin" {
        return if arch == "arm64" || arch == "aarch64" {
            "aarch64-apple-darwin"
        } else {
            "x86_64-apple-darwin"
        }
        .to_string();
    }

    if os.starts_with("MINGW")
        || os.starts_with("MSYS")
        || os.starts_with("CYGWIN")
        || os.starts_with("Windows")
    {
        return match arch.as_str() {
            "aarch64" | "arm64" => "aarch64-pc-windows-msvc",
            "i686" | "i386" => "i686-pc-windows-msvc",
            _ => "x86_64-pc-windows-msvc",
        }
        .to_string();
    }

    if os == "FreeBSD" {
        return "x86_64-unknown-freebsd".to_string();
    }

    String::new()
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

    fn platform(os: &str, arch: &str, libc: &str, android: bool) -> RemotePlatform {
        RemotePlatform {
            os: os.to_string(),
            arch: arch.to_string(),
            libc: libc.to_string(),
            android,
            target: target_triple(os, arch, libc, android),
        }
    }

    #[test]
    fn maps_linux_targets() {
        assert_eq!(
            target_triple("Linux", "x86_64", "gnu", false),
            "x86_64-unknown-linux-gnu"
        );
        assert_eq!(
            target_triple("Linux", "x86_64", "musl", false),
            "x86_64-unknown-linux-musl"
        );
        // ARM prefers musl when the detected libc is glibc.
        assert_eq!(
            target_triple("Linux", "aarch64", "gnu", false),
            "aarch64-unknown-linux-musl"
        );
        assert_eq!(
            target_triple("Linux", "aarch64", "musl", false),
            "aarch64-unknown-linux-musl"
        );
        assert_eq!(
            target_triple("Linux", "armv7l", "gnu", false),
            "armv7-unknown-linux-musleabihf"
        );
        assert_eq!(
            target_triple("Linux", "armv8l", "musl", false),
            "armv7-unknown-linux-musleabihf"
        );
        assert_eq!(
            target_triple("Linux", "armv6l", "gnu", false),
            "arm-unknown-linux-musleabihf"
        );
        assert_eq!(
            target_triple("Linux", "i686", "gnu", false),
            "i686-unknown-linux-gnu"
        );
        assert_eq!(
            target_triple("Linux", "riscv64", "musl", false),
            "riscv64gc-unknown-linux-musl"
        );
        assert_eq!(
            target_triple("Linux", "loongarch64", "gnu", false),
            "loongarch64-unknown-linux-gnu"
        );
        assert_eq!(
            target_triple("Linux", "s390x", "musl", false),
            "s390x-unknown-linux-gnu"
        );
        assert_eq!(
            target_triple("Linux", "ppc64le", "musl", false),
            "powerpc64le-unknown-linux-musl"
        );
    }

    #[test]
    fn maps_android_and_other_targets() {
        assert_eq!(
            target_triple("Linux", "x86_64", "gnu", true),
            "x86_64-linux-android"
        );
        assert_eq!(
            target_triple("Linux", "aarch64", "gnu", true),
            "aarch64-linux-android"
        );
        assert_eq!(
            target_triple("Linux", "armv7l", "gnu", true),
            "armv7-linux-androideabi"
        );
        assert_eq!(
            target_triple("Darwin", "arm64", "gnu", false),
            "aarch64-apple-darwin"
        );
        assert_eq!(
            target_triple("Darwin", "x86_64", "gnu", false),
            "x86_64-apple-darwin"
        );
        assert_eq!(
            target_triple("MINGW64_NT-10.0", "x86_64", "gnu", false),
            "x86_64-pc-windows-msvc"
        );
        assert_eq!(
            target_triple("MINGW64_NT-10.0", "aarch64", "gnu", false),
            "aarch64-pc-windows-msvc"
        );
        assert_eq!(
            target_triple("MSYS_NT-10.0", "i686", "gnu", false),
            "i686-pc-windows-msvc"
        );
        assert_eq!(
            target_triple("FreeBSD", "x86_64", "gnu", false),
            "x86_64-unknown-freebsd"
        );
        assert_eq!(target_triple("Plan9", "mips", "gnu", false), "");
    }

    #[test]
    fn describes_platforms() {
        let linux = platform("Linux", "aarch64", "musl", false);
        assert_eq!(linux.describe(), "linux/aarch64 (musl)");
        assert_eq!(
            linux.target_triple().map(|target| target.to_str()),
            Some("aarch64-unknown-linux-musl")
        );

        let android = platform("Linux", "armv7l", "gnu", true);
        assert!(android.android);
        assert_eq!(android.describe(), "linux/armv7l android");
        assert_eq!(
            android.target_triple().map(|target| target.to_str()),
            Some("armv7-linux-androideabi")
        );

        let unsupported = platform("Plan9", "mips", "gnu", false);
        assert_eq!(unsupported.target, "");
        assert!(unsupported.target_triple().is_none());
    }
}
