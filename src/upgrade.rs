//! In-place self-upgrade.
//!
//! Installing a new copy of the **running** executable is subtle:
//!
//! * Windows cannot overwrite (or delete) a running image, so the old file
//!   must be renamed first and restored if the install fails.
//! * The replacement must be the same build flavor as the running binary
//!   (`-android`, `-musl`, `-msvc`, …). Deriving the target from the host
//!   looks right until the host is Termux, a container, or a cross-compiled
//!   setup: a musl binary on Android cannot resolve DNS and simply hangs, and
//!   `--target`/`ei config target` may legitimately point at another ABI for
//!   *normal* installs. `build.rs`'s `BUILD_TARGET` is the only value that is
//!   always correct.
//! * A failed or partial install must never leave the user without a working
//!   binary.
//!
//! This module owns that flow. It only calls the crate's own [`ei`] installer,
//! so no extra crate (and no circular dependency) is involved.
//!
//! Conceptually an upgrade is a regular [`ei`] install with two defaults:
//!
//! * `target` defaults to this binary's compile-time target
//!   ([`build_target`]) instead of the platform of the machine;
//! * `dir` defaults to the directory of the running executable.
//!
//! It is always local: the `remote` field of the base config is set to
//! `None`, because replacing a running process on another machine is not
//! possible.
//!
//! # Example
//!
//! ```no_run
//! use easy_install::upgrade::{self, UpgradeOptions};
//! use easy_install::InstallConfig;
//!
//! # async fn run() -> easy_install::error::Result<()> {
//! upgrade::upgrade(UpgradeOptions::new(
//!     "easy-install/easy-install",
//!     "ei",
//!     InstallConfig::default(),
//! ))
//! .await?;
//! # Ok(())
//! # }
//! ```

use crate::InstallConfig;
use crate::error::{Context, Result, err};
use guess_target::Target;
use std::path::{Path, PathBuf};

/// Suffix of the rollback copy made next to the running executable.
pub const BACKUP_SUFFIX: &str = ".old";

/// What to upgrade, and how.
#[derive(Debug, Clone)]
pub struct UpgradeOptions {
    /// GitHub repository, e.g. `easy-install/easy-install`.
    pub repo: String,
    /// Installed binary name (file stem), e.g. `ei`.
    pub bin: String,
    /// Target triple to download. `None` uses [`build_target`], i.e. the
    /// flavor this binary was compiled for.
    pub target: Option<Target>,
    /// Install directory. `None` uses the running executable's directory.
    pub dir: Option<PathBuf>,
    /// Base install settings (proxy, timeout, retry, quiet, strip, upx, …).
    ///
    /// `dir`, `alias`, `target` and `remote` are pinned by the upgrade: the
    /// running binary's directory, [`Self::bin`], the resolved target and
    /// "local". Every other field is forwarded as-is.
    pub config: InstallConfig,
}

impl UpgradeOptions {
    /// Options for upgrading `bin` from `repo`, keeping `config`'s settings.
    pub fn new(repo: impl Into<String>, bin: impl Into<String>, config: InstallConfig) -> Self {
        Self {
            repo: repo.into(),
            bin: bin.into(),
            target: None,
            dir: None,
            config,
        }
    }

    /// Use an explicit target instead of the build target.
    pub fn with_target(mut self, target: Target) -> Self {
        self.target = Some(target);
        self
    }

    /// Install somewhere other than the running executable's directory.
    pub fn with_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.dir = Some(dir.into());
        self
    }
}

/// What [`upgrade`] did.
#[derive(Debug, Clone)]
pub struct UpgradeOutcome {
    /// Target that was installed.
    pub target: Target,
    /// Path of the new binary.
    pub exe: PathBuf,
    /// Whether the running executable was replaced.
    pub replaced_running: bool,
}

/// Path of the currently running executable.
pub fn running_exe() -> Result<PathBuf> {
    std::env::current_exe().context("could not locate the running executable")
}

/// The target to download when the caller did not pass one.
///
/// Always prefers the compile-time triple: it is the only value that keeps the
/// replacement runnable on the host (see the module docs).
pub fn resolve_target(explicit: Option<Target>) -> Result<Target> {
    explicit.or_else(build_target).ok_or_else(|| {
        err!(
            "could not determine this binary's build target ({}); pass an explicit --target",
            env!("BUILD_TARGET")
        )
    })
}

/// [`env!("BUILD_TARGET")`][crate::BUILD_TARGET] parsed as a [`Target`].
///
/// `BUILD_TARGET` is the `TARGET` Cargo compiled this crate for; it is exact,
/// including the ABI. It is always set (`build.rs` forwards it), so this only
/// returns `None` for triples `guess-target` does not model (custom JSON
/// targets).
pub fn build_target() -> Option<Target> {
    use std::str::FromStr;
    Target::from_str(crate::BUILD_TARGET).ok()
}

/// Path the installer is expected to write for `name` in `dir`.
pub fn installed_path(dir: &Path, name: &str) -> PathBuf {
    let suffix = std::env::consts::EXE_SUFFIX;
    if suffix.is_empty() || name.ends_with(suffix) {
        dir.join(name)
    } else {
        dir.join(format!("{name}{suffix}"))
    }
}

/// Rollback path used during the rename dance (`ei` → `ei.old`).
pub fn backup_path(exe: &Path) -> PathBuf {
    let mut name = exe.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    name.push(BACKUP_SUFFIX);
    exe.with_file_name(name)
}

/// Upgrade the running executable from `options.repo`.
///
/// On failure — including "installer returned success but wrote nothing" — the
/// previous binary is restored.
pub async fn upgrade(options: UpgradeOptions) -> Result<UpgradeOutcome> {
    let exe = running_exe()?;
    upgrade_at(exe, options).await
}

/// [`upgrade`] for an explicit executable path (used by tests and by callers
/// that manage a second copy of the binary).
pub async fn upgrade_at(exe: PathBuf, options: UpgradeOptions) -> Result<UpgradeOutcome> {
    let dir = match &options.dir {
        Some(dir) => dir.clone(),
        None => exe
            .parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| err!("{} has no parent directory", exe.display()))?,
    };

    let target = resolve_target(options.target)?;
    warn_on_cross_platform_upgrade(target, options.config.quiet);

    let expected = installed_path(&dir, &options.bin);
    let replaces_running = paths_equal(&expected, &exe);

    if !options.config.quiet {
        println!(
            "Upgrading {} ({}) from {}",
            options.bin,
            target.to_str(),
            options.repo
        );
    }

    // Prepare a way back before touching anything.
    let backup = if replaces_running {
        Some(prepare_backup(&exe)?)
    } else {
        None
    };

    let config = upgrade_config(&dir, &options.bin, target, options.config);

    let installed = crate::ei(&options.repo, &config).await;

    let result = installed.and_then(|()| verify_binary(&expected));

    match result {
        Ok(()) => {
            if let Some(backup) = &backup {
                // A stale backup is harmless, but it confuses later upgrades
                // and wastes space.
                let _ = std::fs::remove_file(backup);
            }
            Ok(UpgradeOutcome {
                target,
                exe: expected,
                replaced_running: replaces_running,
            })
        }
        Err(e) => {
            restore(&exe, backup.as_deref())?;
            Err(e)
        }
    }
}

/// Build the config handed to [`crate::ei`] for an upgrade.
///
/// This is a regular `ei` config with three fields pinned: the destination
/// directory, the installed name and the target. `remote` is cleared because
/// an upgrade always replaces the local running executable.
fn upgrade_config(
    dir: &Path,
    bin: &str,
    target: Target,
    base: InstallConfig,
) -> InstallConfig {
    InstallConfig {
        dir: Some(dir.to_string_lossy().into_owned()),
        // Pin the installed name: the artifact may be called differently.
        alias: Some(bin.to_string()),
        // Explicit `--target`, else the build flavor (see `resolve_target`).
        target: Some(target),
        // Replacing a running process on another machine is impossible.
        remote: None,
        ..base
    }
}

/// Warn when an explicit target would replace the binary with a different OS
/// build — usually a mistake (e.g. a Windows exe into a Linux container).
fn warn_on_cross_platform_upgrade(target: Target, quiet: bool) {
    let Some(built) = build_target() else {
        return;
    };
    if quiet || built.os() == target.os() {
        return;
    }
    eprintln!(
        "Warning: upgrading a {} binary to {} — the new file may not run on this host",
        built.to_str(),
        target.to_str()
    );
}

/// Create the backup of `exe`.
///
/// Windows cannot delete or overwrite a running image, so the file is renamed
/// away and the installer writes a fresh one.
///
/// On Unix the running image may be unlinked, but the installer could also
/// truncate it in place (`std::fs::write` does). A hard link would share the
/// inode and be emptied along with it, so this makes a real copy — the only
/// version that survives every way an installer may overwrite the file.
fn prepare_backup(exe: &Path) -> Result<PathBuf> {
    let backup = backup_path(exe);
    remove_if_exists(&backup)?;

    #[cfg(windows)]
    std::fs::rename(exe, &backup).with_context(|| {
        format!("failed to rename the running executable '{}'", exe.display())
    })?;

    #[cfg(not(windows))]
    std::fs::copy(exe, &backup).with_context(|| {
        format!("failed to back up the running executable '{}'", exe.display())
    })?;

    Ok(backup)
}

/// Undo a partially applied upgrade.
fn restore(exe: &Path, backup: Option<&Path>) -> Result<()> {
    let Some(backup) = backup else {
        return Ok(());
    };
    remove_if_exists(exe)?;
    std::fs::rename(backup, exe).with_context(|| {
        format!(
            "failed to restore the previous executable '{}' from '{}'",
            exe.display(),
            backup.display()
        )
    })?;
    Ok(())
}

/// Make sure the installed file is usable, and executable on Unix.
fn verify_binary(path: &Path) -> Result<()> {
    let meta = match std::fs::metadata(path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(err!(
                "installation reported success but '{}' was not created",
                path.display()
            ));
        }
        Err(e) => {
            return Err(e).with_context(|| format!("failed to inspect '{}'", path.display()));
        }
    };
    if !meta.is_file() {
        return Err(err!("'{}' is not a regular file", path.display()));
    }
    if meta.len() == 0 {
        return Err(err!(
            "installation produced an empty file at '{}'",
            path.display()
        ));
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // Some archives lose the mode; a binary the user cannot execute is
        // not an upgrade.
        if meta.permissions().mode() & 0o111 == 0 {
            let mut perms = meta.permissions();
            perms.set_mode(perms.mode() | 0o755);
            std::fs::set_permissions(path, perms).with_context(|| {
                format!("failed to mark '{}' executable", path.display())
            })?;
        }
    }

    Ok(())
}

fn remove_if_exists(path: &Path) -> Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e).with_context(|| format!("failed to remove '{}'", path.display())),
    }
}

/// Compare two paths, resolving symlinks when both exist.
fn paths_equal(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    fn temp_dir() -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("ei-upgrade-test-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn installed_path_applies_exe_suffix() {
        let dir = Path::new("/tmp");
        let p = installed_path(dir, "tool");
        if std::env::consts::EXE_SUFFIX.is_empty() {
            assert_eq!(p, PathBuf::from("/tmp/tool"));
        } else {
            assert_eq!(
                p,
                PathBuf::from(format!("/tmp/tool{}", std::env::consts::EXE_SUFFIX))
            );
        }
        // An explicit suffix is not duplicated.
        let with = installed_path(dir, &format!("tool{}", std::env::consts::EXE_SUFFIX));
        assert_eq!(with, installed_path(dir, "tool"));
    }

    #[test]
    fn backup_path_appends_suffix_to_file_name() {
        let p = backup_path(Path::new("/opt/ei.exe"));
        assert_eq!(p, PathBuf::from("/opt/ei.exe.old"));
    }

    #[test]
    fn target_defaults_to_build_target() {
        let t = resolve_target(None).expect("build target must be available in tests");
        assert_eq!(t.to_str(), env!("BUILD_TARGET"));
    }

    #[test]
    fn explicit_target_wins() {
        let t = Target::X86_64UnknownLinuxMusl;
        assert_eq!(resolve_target(Some(t)).unwrap(), t);
    }

    #[test]
    fn verify_rejects_missing_and_empty_files() {
        let dir = temp_dir();
        let missing = installed_path(&dir, "tool");
        assert!(verify_binary(&missing).is_err());

        std::fs::write(&missing, b"").unwrap();
        assert!(verify_binary(&missing).is_err());

        std::fs::write(&missing, b"binary").unwrap();
        verify_binary(&missing).expect("non-empty file should verify");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn backup_and_restore_round_trip() {
        let dir = temp_dir();
        let exe = installed_path(&dir, "tool");
        std::fs::write(&exe, b"old").unwrap();

        let backup = prepare_backup(&exe).unwrap();
        assert_eq!(std::fs::read(&backup).unwrap(), b"old");

        // The installer would write a new file here; simulate a broken result
        // and roll back.
        std::fs::write(&exe, b"").unwrap();
        restore(&exe, Some(&backup)).unwrap();

        assert_eq!(std::fs::read(&exe).unwrap(), b"old");
        assert!(!backup.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn upgrade_config_pins_location_name_and_target() {
        let base = InstallConfig {
            dir: Some("/somewhere".into()),
            alias: Some("other".into()),
            target: Some(Target::X86_64PcWindowsMsvc),
            remote: Some(crate::RemoteConfig {
                user: "u".into(),
                host: "h".into(),
                port: 22,
                ..Default::default()
            }),
            no_path: true,
            ..Default::default()
        };

        let config = upgrade_config(
            Path::new("/opt/ei"),
            "ei",
            Target::Aarch64UnknownLinuxMusl,
            base,
        );

        assert_eq!(config.dir.as_deref(), Some("/opt/ei"));
        assert_eq!(config.alias.as_deref(), Some("ei"));
        assert_eq!(config.target, Some(Target::Aarch64UnknownLinuxMusl));
        // An upgrade is always local, and unrelated settings are kept.
        assert!(config.remote.is_none());
        assert!(config.no_path);
    }
}
