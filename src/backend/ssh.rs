//! The SSH target.

use super::{CommandResult, Host};
use crate::error::{Result, err};
use crate::remote::{RemoteConfig, RemotePlatform, detect_platform};
use guess_target::Target;
use sssh::{Client, Fs, Output, Shell, shell_quote};

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

/// Writes to a remote host over SSH.
pub struct SshHost {
    client: Client,
    fs: Fs,
    /// Memoised platform probe (one SSH round-trip per install).
    platform: tokio::sync::OnceCell<RemotePlatform>,
}

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
        self.fs.create_dir_all(dir).await.map_err(|e| {
            if crate::error::is_out_of_space(&e) {
                return err!("{}", crate::error::disk_full_message(dir, &e));
            }
            err!("failed to create '{dir}' on {}: {e}", self.label())
        })
    }

    async fn write(&self, path: &str, data: &[u8], mode: Option<u32>) -> Result<()> {
        // `Fs::write_with_mode` matches `std::fs::write` and does not create
        // parents; installs may target a nested directory, so do it here.
        self.fs.create_parent_dirs(path).await.map_err(|e| {
            if crate::error::is_out_of_space(&e) {
                return err!("{}", crate::error::disk_full_message(path, &e));
            }
            err!(
                "failed to create the parent of '{path}' on {}: {e}",
                self.label()
            )
        })?;
        self.fs
            .write_with_mode(path, data, mode)
            .await
            .map_err(|e| {
                // Remote `cat`/`dd` report ENOSPC through stderr; map it so
                // the user sees the same message as for a local install.
                if crate::error::is_out_of_space(&e) {
                    return err!("{}", crate::error::disk_full_message(path, &e));
                }
                err!("failed to write '{path}' on {}: {e}", self.label())
            })
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

    async fn add_to_path(&self, dir: &str, shell: Option<Shell>, quiet: bool) -> Result<()> {
        // `sssh` owns the remote edit (which config file, how to append it);
        // `bash` is the default because the remote login shell cannot be
        // guessed reliably.
        let shell = shell.unwrap_or(Shell::Bash);
        let file = self.client.add_to_path(shell, dir).await.map_err(|e| {
            err!(
                "could not add '{dir}' to {shell}'s PATH on {}: {e}",
                self.label()
            )
        })?;
        if !quiet {
            println!(
                "Added {dir} to {shell}'s PATH on {} ({})",
                self.label(),
                crate::path::path_to_str(&file)
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `sssh::Output` is adapted to the string-based `CommandResult` used by
    /// the rest of the install pipeline.
    #[test]
    fn converts_ssh_output() {
        let result: CommandResult = Output::default().into();
        assert!(result.success());
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "");
        assert_eq!(result.stderr, "");
    }
}
