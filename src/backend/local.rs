//! The local filesystem target.

use super::{CommandResult, Host};
use crate::error::{Context, Result, bail, err};
use crate::path::path_to_str;
#[cfg(unix)]
use std::os::unix::prelude::PermissionsExt;
use std::path::PathBuf;
use std::str::FromStr;
use which_shell::Shell;

// ── Local filesystem helpers ────────────────────────────────────────────

/// Write a file, creating its parent directory and replacing an existing file.
fn write_to_file(path: &str, buffer: &[u8], mode: &Option<u32>) -> Result<()> {
    let path_buf = PathBuf::from_str(path).context("invalid path for write_to_file")?;
    if let Some(parent) = path_buf.parent() {
        if std::fs::exists(parent).unwrap_or(false) {
            // A directory is required here. If a regular file occupies the
            // same path (e.g. a leftover single-file install named like the
            // package), `create_dir_all` is skipped and the write fails with
            // a confusing "path not found" OS error.
            if !std::fs::metadata(parent)
                .map(|meta| meta.is_dir())
                .unwrap_or(false)
            {
                bail!(
                    "cannot install to '{}': a file with the same name already exists.\nRemove it first, or choose a different install directory with --dir or --alias.",
                    path_to_str(parent)
                );
            }
        } else {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create directory '{}'", path_to_str(parent)))?;
        }
    }

    if std::fs::exists(path).unwrap_or(false)
        && let Ok(meta) = std::fs::metadata(path)
    {
        if meta.is_file() {
            std::fs::remove_file(path)
                .with_context(|| format!("failed to remove existing file '{path}'"))?;
        } else {
            bail!("target path is a directory, refusing to overwrite: {path}");
        }
    }

    if !buffer.is_empty()
        && let Err(e) = std::fs::write(path, buffer)
    {
        // The up-front check passed, so the disk filled in between (or the
        // quota is per-user). Report the real cause.
        if crate::error::is_out_of_space(&e) {
            bail!("{}", crate::error::disk_full_message(path, &e));
        }
        return Err(e).with_context(|| format!("failed to write file '{path}'"));
    }

    #[cfg(unix)]
    if let Some(mode) = mode
        && *mode > 0
        && !buffer.is_empty()
    {
        std::fs::set_permissions(path, PermissionsExt::from_mode(*mode))
            .with_context(|| format!("failed to set permissions on '{path}'"))?;
    }

    #[cfg(windows)]
    {
        _ = mode;
    }
    Ok(())
}

/// Create a symbolic link, replacing any existing entry at `link`.
fn write_link(target: &str, link: &str) -> Result<()> {
    let link_path = PathBuf::from_str(link).context("invalid path for symlink")?;

    if let Some(parent) = link_path.parent()
        && !std::fs::exists(parent).unwrap_or(false)
    {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory '{}'", path_to_str(parent)))?;
    }

    // `symlink` refuses to overwrite, and a leftover file would make the
    // install fail on the second run.
    if std::fs::symlink_metadata(&link_path).is_ok() {
        std::fs::remove_file(&link_path)
            .with_context(|| format!("failed to replace existing '{link}'"))?;
    }

    make_link(target, &link_path)
        .with_context(|| format!("failed to create symlink '{link}' -> '{target}'"))
}

#[cfg(unix)]
fn make_link(target: &str, link: &std::path::Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

/// Windows decides between file and directory links by looking at the target,
/// and generally needs developer mode or the symlink privilege.
#[cfg(windows)]
fn make_link(target: &str, link: &std::path::Path) -> std::io::Result<()> {
    let resolved = link
        .parent()
        .unwrap_or_else(|| std::path::Path::new(""))
        .join(target);
    if resolved.is_dir() {
        std::os::windows::fs::symlink_dir(target, link)
    } else {
        std::os::windows::fs::symlink_file(target, link)
    }
}

#[cfg(not(any(unix, windows)))]
fn make_link(_target: &str, _link: &std::path::Path) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "symbolic links are not supported on this platform",
    ))
}

/// Add the execute bits to an existing file, keeping the rest of its mode.
#[cfg(not(windows))]
fn add_execute_permission(path: &str) -> Result<()> {
    if let Ok(metadata) = std::fs::metadata(path).context("metadata failed") {
        if metadata.is_dir() {
            return Ok(());
        }

        let mut permissions = metadata.permissions();
        permissions.set_mode(permissions.mode() | crate::path::EXEC_MASK);
        std::fs::set_permissions(path, permissions)?;
    }
    Ok(())
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
        crate::path::expand_path(path)
    }

    fn home_dir(&self) -> String {
        crate::env::get_install_dir()
            .map(|p| path_to_str(&p))
            .unwrap_or_else(|_| "~/.ei".to_string())
    }

    async fn mkdir(&self, dir: &str) -> Result<()> {
        std::fs::create_dir_all(dir).map_err(|e| {
            if crate::error::is_out_of_space(&e) {
                return err!("{}", crate::error::disk_full_message(dir, &e));
            }
            err!("failed to create directory '{dir}': {e}")
        })
    }

    async fn write(&self, path: &str, data: &[u8], mode: Option<u32>) -> Result<()> {
        write_to_file(path, data, &mode)
    }

    async fn symlink(&self, target: &str, link: &str) -> Result<()> {
        write_link(target, link)
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
        add_execute_permission(path)?;
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

    async fn add_to_path(&self, dir: &str, shell: Option<Shell>, quiet: bool) -> Result<()> {
        crate::env::add_to_path(dir, shell, quiet);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
