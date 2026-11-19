//! Install targets.
//!
//! Everything an install does to the "other side" — writing files, creating
//! directories, checking free space, running `strip`/`upx`, updating `$PATH` —
//! goes through the [`Host`] trait:
//!
//! * [`LocalHost`] — write to the local disk.
//! * [`SshHost`] — stream to a remote host over SSH and run `strip`/`upx`
//!   there (requires the `ssh` feature).
//!
//! [`Backend`] is the runtime-selected combination of both. The rest of the
//! pipeline only ever sees `&Backend`, so it never has to care where the files
//! actually land.

use crate::InstallConfig;
use crate::error::{Context, Result, err};
use crate::tool::path_to_str;
use guess_target::Target;
use std::path::PathBuf;

#[cfg(feature = "ssh")]
use crate::remote::{RemoteConfig, RemotePlatform, detect_platform};
#[cfg(feature = "ssh")]
use sssh::{Client, Fs, Output, shell_quote};

/// Result of a command executed on the install target.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommandResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

impl CommandResult {
    pub fn success(&self) -> bool {
        self.exit_code == 0
    }
}

#[cfg(feature = "ssh")]
impl From<Output> for CommandResult {
    fn from(out: Output) -> Self {
        let exit_code = if out.success() {
            out.code().unwrap_or(0)
        } else {
            out.code().unwrap_or(-1)
        };
        Self {
            stdout: out.stdout_lossy(),
            stderr: out.stderr_lossy(),
            exit_code,
        }
    }
}

/// The machine an install writes to.
///
/// Methods take `&self` and are async so an implementation can hold a
/// connection (SSH) for the whole install.
#[allow(async_fn_in_trait)]
pub trait Host {
    /// Whether this host is remote.
    fn is_remote(&self) -> bool;

    /// Short label used in logs (`localhost` / `user@host`).
    fn label(&self) -> String;

    /// Expand `~` and relative paths for this host.
    fn expand(&self, path: &str) -> String;

    /// Default install directory (`~/.ei` for both hosts).
    fn home_dir(&self) -> String;

    /// Human readable platform description, when detectable.
    ///
    /// `None` means "not probed": the caller keeps its own default.
    async fn platform(&self) -> Option<String> {
        None
    }

    /// Target triples supported by this host.
    ///
    /// `None` means "no opinion"; callers fall back to `--target` or the
    /// compile-time local target.
    async fn targets(&self) -> Option<Vec<Target>> {
        None
    }

    /// Create a directory and its parents.
    async fn mkdir(&self, dir: &str) -> Result<()>;

    /// Write `data` to `path`, applying `mode` when provided.
    async fn write(&self, path: &str, data: &[u8], mode: Option<u32>) -> Result<()>;

    /// Whether `path` exists.
    #[allow(dead_code)] // part of the public host API
    async fn exists(&self, path: &str) -> bool;

    /// Size of the regular file at `path` in bytes, or `None` when it does not
    /// exist (or is not a regular file).
    async fn file_size(&self, path: &str) -> Option<u64>;

    /// Remove a file, ignoring "not found".
    #[allow(dead_code)] // part of the public host API
    async fn remove(&self, path: &str) -> Result<()>;

    /// Mark a file as executable.
    async fn chmod(&self, path: &str) -> Result<()>;

    /// Free space in bytes on the filesystem holding `dir`.
    /// Free space in bytes on the filesystem holding `dir`.
    ///
    /// `None` means the value is unknown (no `stat`/`df`, unparsable
    /// output); a genuine `Some(0)` means the filesystem is full.
    async fn free_space(&self, dir: &str) -> Result<Option<u64>>;

    /// Run a program on the host (used for `strip` / `upx`).
    async fn run(&self, program: &str, args: &[&str]) -> Result<CommandResult>;

    /// Make `dir` part of the login `$PATH`.
    async fn add_to_path(&self, dir: &str, quiet: bool) -> Result<()>;
}

// ── Local host ──────────────────────────────────────────────────────────

/// Writes to the local filesystem.
#[derive(Debug, Clone, Copy, Default)]
pub struct LocalHost;

impl Host for LocalHost {
    fn is_remote(&self) -> bool {
        false
    }

    fn label(&self) -> String {
        "localhost".to_string()
    }

    fn expand(&self, path: &str) -> String {
        crate::tool::expand_path(path)
    }

    fn home_dir(&self) -> String {
        crate::env::get_install_dir()
            .map(|p| path_to_str(&p))
            .unwrap_or_else(|_| "~/.ei".to_string())
    }

    async fn mkdir(&self, dir: &str) -> Result<()> {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("failed to create directory '{dir}'"))?;
        Ok(())
    }

    async fn write(&self, path: &str, data: &[u8], mode: Option<u32>) -> Result<()> {
        crate::tool::write_to_file(path, data, &mode)
    }

    async fn exists(&self, path: &str) -> bool {
        std::fs::exists(path).unwrap_or(false)
    }

    async fn file_size(&self, path: &str) -> Option<u64> {
        let meta = std::fs::metadata(path).ok()?;
        meta.is_file().then_some(meta.len())
    }

    async fn remove(&self, path: &str) -> Result<()> {
        std::fs::remove_file(path).with_context(|| format!("failed to remove '{path}'"))?;
        Ok(())
    }

    async fn chmod(&self, path: &str) -> Result<()> {
        #[cfg(not(windows))]
        crate::tool::add_execute_permission(path)?;
        #[cfg(windows)]
        let _ = path;
        Ok(())
    }

    async fn free_space(&self, dir: &str) -> Result<Option<u64>> {
        let path = PathBuf::from(dir);
        // The directory may not exist yet — measure its parent instead.
        let probe = if path.exists() {
            path
        } else {
            path.parent()
                .map(|p| p.to_path_buf())
                .ok_or_else(|| err!("not found: {dir}"))?
        };
        fs4::available_space(&probe)
            .map(Some)
            .map_err(|e| err!("failed to check disk space: {e}"))
    }

    async fn run(&self, program: &str, args: &[&str]) -> Result<CommandResult> {
        let output = tokio::process::Command::new(program)
            .args(args)
            .output()
            .await
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    err!("{program} command not found")
                } else {
                    err!("failed to execute {program}: {e}")
                }
            })?;
        Ok(CommandResult {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            exit_code: output.status.code().unwrap_or(-1),
        })
    }

    async fn add_to_path(&self, dir: &str, quiet: bool) -> Result<()> {
        crate::env::add_to_path(dir, quiet);
        Ok(())
    }
}

// ── SSH host ────────────────────────────────────────────────────────────

/// Writes to a remote host over SSH.
#[cfg(feature = "ssh")]
pub struct SshHost {
    client: Client,
    fs: Fs,
    /// Memoised platform probe (one SSH round-trip per install).
    platform: tokio::sync::OnceCell<RemotePlatform>,
}

#[cfg(feature = "ssh")]
impl SshHost {
    pub async fn connect(target: RemoteConfig, timeout_secs: u64) -> Result<Self> {
        let client = target.connect(timeout_secs).await?;
        let fs = client.fs();
        Ok(Self {
            client,
            fs,
            platform: tokio::sync::OnceCell::new(),
        })
    }

    /// Detect the remote platform once per install.
    async fn detect_platform(&self) -> Result<RemotePlatform> {
        self.platform
            .get_or_try_init(|| detect_platform(&self.client, &self.fs))
            .await
            .cloned()
    }
}

#[cfg(feature = "ssh")]
impl Host for SshHost {
    fn is_remote(&self) -> bool {
        true
    }

    fn label(&self) -> String {
        self.client.label().to_string()
    }

    fn expand(&self, path: &str) -> String {
        self.client.expand_path(path).to_string_lossy().into_owned()
    }

    fn home_dir(&self) -> String {
        match self.client.home() {
            "" => "~/.ei".to_string(),
            home => format!("{}/.ei", home.trim_end_matches('/')),
        }
    }

    async fn platform(&self) -> Option<String> {
        self.detect_platform()
            .await
            .ok()
            .map(|platform| platform.describe())
    }

    async fn targets(&self) -> Option<Vec<Target>> {
        Some(vec![self.detect_platform().await.ok()?.target_triple()?])
    }

    async fn mkdir(&self, dir: &str) -> Result<()> {
        self.fs
            .create_dir_all(dir)
            .await
            .map_err(|e| err!("failed to create '{dir}' on {}: {e}", self.label()))
    }

    async fn write(&self, path: &str, data: &[u8], mode: Option<u32>) -> Result<()> {
        // `Fs::write_with_mode` matches `std::fs::write` and does not create
        // parents; installs may target a nested directory, so do it here.
        self.fs.create_parent_dirs(path).await.map_err(|e| {
            err!(
                "failed to create the parent of '{path}' on {}: {e}",
                self.label()
            )
        })?;
        self.fs
            .write_with_mode(path, data, mode)
            .await
            .map_err(|e| err!("failed to write '{path}' on {}: {e}", self.label()))
    }

    async fn exists(&self, path: &str) -> bool {
        self.fs.try_exists(path).await.unwrap_or(false)
    }

    async fn file_size(&self, path: &str) -> Option<u64> {
        let meta = self.fs.metadata(path).await.ok()?;
        meta.is_file().then_some(meta.len())
    }

    async fn remove(&self, path: &str) -> Result<()> {
        match self.fs.remove_file(path).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(err!("failed to remove '{path}' on {}: {e}", self.label())),
        }
    }

    async fn chmod(&self, path: &str) -> Result<()> {
        // `chmod +x` semantics: keep the existing mode, add execute bits.
        let meta = self
            .fs
            .metadata(path)
            .await
            .map_err(|e| err!("failed to stat '{path}' on {}: {e}", self.label()))?;
        self.fs
            .set_mode(path, meta.permissions().mode() | 0o111)
            .await
            .map_err(|e| err!("failed to chmod '{path}' on {}: {e}", self.label()))
    }

    async fn free_space(&self, dir: &str) -> Result<Option<u64>> {
        self.fs
            .available_space(dir)
            .await
            .map_err(|e| err!("failed to check disk space on {}: {e}", self.label()))
    }

    async fn run(&self, program: &str, args: &[&str]) -> Result<CommandResult> {
        let mut command = shell_quote(program);
        for arg in args {
            command.push(' ');
            command.push_str(&shell_quote(arg));
        }
        let output = self
            .client
            .exec(&command)
            .await
            .map_err(|e| err!("failed to run {program} on {}: {e}", self.label()))?;
        Ok(output.into())
    }

    async fn add_to_path(&self, dir: &str, quiet: bool) -> Result<()> {
        // Never edit the remote shell config behind the user's back.
        if !quiet {
            println!("Add {dir} to the remote PATH with:\n  export PATH=\"$PATH:{dir}\"");
        }
        Ok(())
    }
}

// ── Runtime selection ───────────────────────────────────────────────────

/// Forward a method call to whichever host was selected.
///
/// Each arm binds `$host` to a different concrete type, so the same
/// expression (including `.await`) is type-checked per arm.
macro_rules! forward {
    ($self:ident, $host:ident => $call:expr) => {
        match $self {
            Backend::Local($host) => $call,
            #[cfg(feature = "ssh")]
            Backend::Ssh($host) => $call,
        }
    };
}

/// The host an install writes to, chosen at runtime.
pub enum Backend {
    Local(LocalHost),
    #[cfg(feature = "ssh")]
    Ssh(Box<SshHost>),
}

impl Backend {
    /// Local disk.
    pub fn local() -> Self {
        Self::Local(LocalHost)
    }

    /// Connect over SSH when `config.remote` is set, otherwise use the local disk.
    pub async fn connect(config: &InstallConfig) -> Result<Self> {
        match &config.remote {
            None => Ok(Self::local()),
            #[cfg(feature = "ssh")]
            Some(remote) => Ok(Self::Ssh(Box::new(
                SshHost::connect(remote.clone(), config.timeout).await?,
            ))),
            #[cfg(not(feature = "ssh"))]
            Some(remote) => Err(err!(
                "cannot install to {}: this build of ei was compiled without the `ssh` feature",
                remote.label()
            )),
        }
    }
}

impl Host for Backend {
    fn is_remote(&self) -> bool {
        forward!(self, h => h.is_remote())
    }

    fn label(&self) -> String {
        forward!(self, h => h.label())
    }

    fn expand(&self, path: &str) -> String {
        forward!(self, h => h.expand(path))
    }

    fn home_dir(&self) -> String {
        forward!(self, h => h.home_dir())
    }

    async fn platform(&self) -> Option<String> {
        forward!(self, h => h.platform().await)
    }

    async fn targets(&self) -> Option<Vec<Target>> {
        forward!(self, h => h.targets().await)
    }

    async fn mkdir(&self, dir: &str) -> Result<()> {
        forward!(self, h => h.mkdir(dir).await)
    }

    async fn write(&self, path: &str, data: &[u8], mode: Option<u32>) -> Result<()> {
        forward!(self, h => h.write(path, data, mode).await)
    }

    async fn exists(&self, path: &str) -> bool {
        forward!(self, h => h.exists(path).await)
    }

    async fn file_size(&self, path: &str) -> Option<u64> {
        forward!(self, h => h.file_size(path).await)
    }

    async fn remove(&self, path: &str) -> Result<()> {
        forward!(self, h => h.remove(path).await)
    }

    async fn chmod(&self, path: &str) -> Result<()> {
        forward!(self, h => h.chmod(path).await)
    }

    async fn free_space(&self, dir: &str) -> Result<Option<u64>> {
        forward!(self, h => h.free_space(dir).await)
    }

    async fn run(&self, program: &str, args: &[&str]) -> Result<CommandResult> {
        forward!(self, h => h.run(program, args).await)
    }

    async fn add_to_path(&self, dir: &str, quiet: bool) -> Result<()> {
        forward!(self, h => h.add_to_path(dir, quiet).await)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn connect_without_remote_uses_local_disk() {
        let config = InstallConfig::default();
        let backend = Backend::connect(&config).await.expect("local connect");
        assert!(!backend.is_remote());
        assert_eq!(backend.label(), "localhost");
        // No opinion: callers fall back to `--target` / guess-target.
        assert!(backend.targets().await.is_none());
    }

    #[test]
    fn local_paths_are_expanded() {
        let host = LocalHost;
        assert!(!host.expand("./relative").contains('~'));
        assert!(host.home_dir().ends_with(".ei"));
    }

    /// `file_size` drives the "replacing an existing file frees its space"
    /// discount. It must report real sizes and `None` for anything that is not
    /// an existing regular file — otherwise the disk check would be wrong.
    ///
    /// `SshHost` implements the same trait method on top of `sssh::Fs`, so the
    /// remote install gets the identical behaviour.
    #[tokio::test]
    async fn file_size_reports_regular_files_only() {
        let dir = std::env::temp_dir().join(format!("ei-file-size-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let host = LocalHost;

        let file = dir.join("binary");
        std::fs::write(&file, b"1234567890").unwrap();
        assert_eq!(host.file_size(file.to_str().unwrap()).await, Some(10));

        let missing = dir.join("missing");
        assert_eq!(host.file_size(missing.to_str().unwrap()).await, None);

        // A directory is not a file: its size must not be discounted.
        assert_eq!(host.file_size(dir.to_str().unwrap()).await, None);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `sssh::Output` is adapted to the string-based `CommandResult` used by
    /// the rest of the install pipeline.
    #[cfg(feature = "ssh")]
    #[test]
    fn converts_ssh_output() {
        let result: CommandResult = sssh::Output::default().into();
        assert!(result.success());
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "");
        assert_eq!(result.stderr, "");
    }
}
