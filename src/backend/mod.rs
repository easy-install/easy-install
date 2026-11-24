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

mod local;
#[cfg(feature = "ssh")]
mod ssh;

pub use local::LocalHost;
#[cfg(feature = "ssh")]
pub use ssh::SshHost;

use crate::InstallConfig;
use crate::error::Result;
#[cfg(not(feature = "ssh"))]
use crate::error::err;
use guess_target::Target;
use which_shell::Shell;

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

    /// Create a symbolic link at `link` pointing to `target`.
    ///
    /// Replaces an existing entry at `link`. Fails when the platform cannot
    /// create links (Windows without the symlink privilege); callers fall back
    /// to writing the content instead.
    async fn symlink(&self, target: &str, link: &str) -> Result<()>;

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
    ///
    /// `None` means the value is unknown (no `stat`/`df`, unparsable
    /// output); a genuine `Some(0)` means the filesystem is full.
    async fn free_space(&self, dir: &str) -> Result<Option<u64>>;

    /// Run a program on the host (used for `strip` / `upx`).
    async fn run(&self, program: &str, args: &[&str]) -> Result<CommandResult>;

    /// Make `dir` part of the login `$PATH` of `shell`.
    ///
    /// `None` lets a local host detect the current shell; a remote host falls
    /// back to `bash`.
    async fn add_to_path(&self, dir: &str, shell: Option<Shell>, quiet: bool) -> Result<()>;
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

    async fn symlink(&self, target: &str, link: &str) -> Result<()> {
        forward!(self, h => h.symlink(target, link).await)
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

    async fn add_to_path(&self, dir: &str, shell: Option<Shell>, quiet: bool) -> Result<()> {
        forward!(self, h => h.add_to_path(dir, shell, quiet).await)
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
}
