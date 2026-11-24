//! The install pipeline.
//!
//! [`install`] turns a source URL into written files:
//!
//! ```text
//! resolve (source) → artifacts (select) → bytes
//!   → file tree (decode, nested archives) → alias / exec bits (tree)
//!   → disk-space plan → write (local or SSH) → strip/upx (optimize) → report
//! ```
//!
//! Artifacts are installed concurrently unless there is only one, or the
//! target is remote (a single SSH connection is reused and interleaved writes
//! would only garble the output).

use crate::InstallConfig;
use crate::artifact::{Artifact, Kind};
use crate::backend::{Backend, Host};
use crate::download::{download_binary, get_bytes};
use crate::error::{Context, Result, bail, err};
use crate::optimize::optimize_executable;
use crate::path::{
    ends_with_exe, executable, get_bin_name, get_filename, is_archive_file, is_skip, join_path,
};
use crate::report::{self, Installed, InstalledFile, Report};
use crate::tree::{self, FileTree, TreeFile};
use easy_archive::{Fmt, human_size};
use github_proxy::fetch::Url;
use guess_target::Os;
use std::sync::LazyLock;
use tokio::task::JoinSet;
use tracing::trace;

/// Limits concurrent network downloads to avoid hammering GitHub and
/// tripping rate limits.
static DOWNLOAD_SEM: LazyLock<tokio::sync::Semaphore> =
    LazyLock::new(|| tokio::sync::Semaphore::new(4));

/// Install whatever `url` refers to.
pub(crate) async fn install(
    url: &str,
    config: &InstallConfig,
    backend: &Backend,
) -> Result<Report> {
    trace!("install {url}");
    let artifacts = crate::source::resolve(url, config).await?;
    install_artifacts(backend, artifacts, config).await
}

/// Install every artifact, merging the reports.
async fn install_artifacts(
    backend: &Backend,
    artifacts: Vec<Artifact>,
    config: &InstallConfig,
) -> Result<Report> {
    // `--alias` renames the installed file to a single fixed name. With more
    // than one artifact they would all target the same path, silently
    // overwriting each other and making the result non-deterministic.
    if let Some(alias) = &config.alias
        && artifacts.len() > 1
    {
        let names = artifacts
            .iter()
            .map(|artifact| format!("  - {}", artifact.name))
            .collect::<Vec<_>>()
            .join("\n");
        bail!(
            "--alias '{alias}' is ambiguous: {} installable artifacts matched:\n{names}\n\nUse --name or --regex to select exactly one artifact,\nor drop --alias so each artifact keeps its own name.",
            artifacts.len()
        );
    }

    // Bundles from one source (CI artifacts, manifest entries) usually share a
    // layout such as `bin/` + `lib/`, so writing them side by side would make
    // them overwrite each other. Each bundle gets its own directory instead.
    let multiple = artifacts.len() > 1;

    if artifacts.len() <= 1 || backend.is_remote() {
        let mut report = Report::new();
        for artifact in artifacts {
            report.extend(install_artifact(backend, &artifact, config, multiple).await?);
        }
        return Ok(report);
    }

    let mut tasks: JoinSet<Result<Report>> = JoinSet::new();
    for artifact in artifacts {
        let config = config.clone();
        tasks.spawn(async move {
            let _permit = DOWNLOAD_SEM.acquire().await.expect("semaphore closed");
            // One connection per task keeps the writes independent.
            let backend = Backend::connect(&config).await?;
            install_artifact(&backend, &artifact, &config, multiple).await
        });
    }

    let mut report = Report::new();
    while let Some(result) = tasks.join_next().await {
        report.extend(result??);
    }
    Ok(report)
}

async fn install_artifact(
    backend: &Backend,
    artifact: &Artifact,
    config: &InstallConfig,
    multiple: bool,
) -> Result<Report> {
    match artifact.kind {
        Kind::Single => install_single_file(backend, artifact, config).await,
        Kind::Auto => install_auto(backend, artifact, config, multiple).await,
        Kind::Zip => {
            log_download(artifact, config);
            // The download URL is an API endpoint (…/artifacts/{id}/zip) that
            // `Fmt::guess` may not recognise, hence the explicit format.
            let bytes = get_bytes(&artifact.url, config.retry, config.timeout)
                .await
                .context("Failed to download CI artifact")?;
            install_bytes(backend, bytes, Fmt::Zip, artifact, config, multiple).await
        }
    }
}

/// Install an artifact whose format is inferred from its URL.
async fn install_auto(
    backend: &Backend,
    artifact: &Artifact,
    config: &InstallConfig,
    multiple: bool,
) -> Result<Report> {
    if !is_archive_file(&artifact.file) {
        return install_single_file(backend, artifact, config).await;
    }

    log_download(artifact, config);
    let bytes = get_bytes(&artifact.url, config.retry, config.timeout).await?;
    let fmt = Fmt::guess(&artifact.url).context("fmt guess error")?;
    install_bytes(backend, bytes, fmt, artifact, config, multiple).await
}

/// Install an artifact as a single file.
async fn install_single_file(
    backend: &Backend,
    artifact: &Artifact,
    config: &InstallConfig,
) -> Result<Report> {
    // A Windows executable is never installed on a non-Windows target.
    if ends_with_exe(&artifact.url)
        && config
            .targets()
            .iter()
            .any(|target| target.os() != Os::Windows)
    {
        return Ok(Report::new());
    }

    log_download(artifact, config);
    let data = if std::fs::exists(&artifact.url).unwrap_or(false) {
        std::fs::read(&artifact.url)?
    } else {
        download_binary(&artifact.url, config.retry, config.timeout).await?
    };

    // Naming follows the target, not the machine running `ei`: a Windows
    // build installing onto Linux over SSH must not add `.exe`.
    let windows = config
        .targets()
        .iter()
        .any(|target| target.os() == Os::Windows);

    let file = TreeFile {
        path: get_bin_name(&artifact.name, windows),
        origin_path: get_filename(&artifact.url),
        mode: None,
        link_target: None,
        data,
    };
    let dir = base_install_dir(backend, config);
    write_tree(backend, vec![file], dir, &artifact.url, config).await
}

/// Print what is being fetched, for remote sources only.
///
/// The published file name is included because CI artifact URLs are opaque
/// (`…/artifacts/14886629774/zip`).
fn log_download(artifact: &Artifact, config: &InstallConfig) {
    if !config.quiet && Url::is_valid(&artifact.url) {
        if artifact.file.is_empty() || artifact.file == "zip" {
            println!("download {}", artifact.url);
        } else {
            println!("download {} {}", artifact.file, artifact.url);
        }
    }
}

/// Install an in-memory archive.
async fn install_bytes(
    backend: &Backend,
    bytes: Vec<u8>,
    fmt: Fmt,
    artifact: &Artifact,
    config: &InstallConfig,
    multiple: bool,
) -> Result<Report> {
    // A corrupt archive is a real failure: report which artifact could not be
    // extracted instead of pretending nothing was selected.
    let mut tree = tree::decode(bytes, fmt, &artifact.name)
        .with_context(|| format!("failed to extract the archive from '{}'", artifact.url))?;
    if tree.files.is_empty() {
        return Ok(Report::new());
    }

    // A single file inside an archive is the tool itself, but archive entries
    // carry the platform (`nova-windows-amd64.exe`). Install it under the
    // inferred tool name when the entry names the tool; `--alias` still wins
    // because `write_tree` renames after this.
    if config.alias.is_none() {
        tree::rename_to_tool(&mut tree.files, &artifact.name);
    }

    let dir = install_dir_for(backend, config, &tree, multiple);
    write_tree(backend, tree.files, dir, &artifact.url, config).await
}

/// Directory the files of `tree` are written to.
fn install_dir_for(
    backend: &Backend,
    config: &InstallConfig,
    tree: &FileTree,
    multiple: bool,
) -> String {
    let base = base_install_dir(backend, config);

    // A multi-file tree gets its own directory. Several bundles from one source
    // (`multiple`) always do, even with `-d`: two CI bundles sharing `bin/js`
    // and `lib/*` would otherwise overwrite each other.
    if tree.files.len() > 1 && (multiple || config.dir.is_none()) {
        // Bundles cannot share one alias directory, so they keep their own name.
        match &config.alias {
            Some(alias) if !multiple => join_path(&base, alias),
            _ => join_path(&base, &tree.name),
        }
    } else {
        base
    }
}

/// The install directory before any archive-specific subdirectory is added.
fn base_install_dir(backend: &Backend, config: &InstallConfig) -> String {
    match config.dir.as_deref() {
        // Paths are `/`-separated throughout the pipeline, including on
        // Windows, so that `dirname`/`get_filename` behave uniformly.
        Some(dir) if dir.contains('/') || dir.contains('\\') => {
            backend.expand(dir).replace('\\', "/")
        }
        Some(dir) => join_path(&backend.home_dir(), dir),
        None => backend.home_dir(),
    }
}

/// Apply the final tree transformations and write the files.
async fn write_tree(
    backend: &Backend,
    mut files: Vec<TreeFile>,
    dir: String,
    source: &str,
    config: &InstallConfig,
) -> Result<Report> {
    if let Some(alias) = &config.alias {
        tree::rename_alias(&mut files, alias);
    }
    tree::guess_executable(&mut files);

    let installed: Vec<InstalledFile> = files
        .iter()
        .map(|file| InstalledFile {
            install_path: join_path(&dir, &file.path),
            origin_path: file.origin_path.clone(),
            mode: file.mode,
            link_target: file.link_target.clone(),
            size: file.size(),
        })
        .collect();

    check_disk_space(backend, &installed, &dir).await?;

    let skipped = write_files(backend, &files, &installed, &dir, config).await?;
    // A link that had to be skipped is not part of the install: reporting it
    // would claim a file that does not exist.
    let installed: Vec<InstalledFile> = installed
        .into_iter()
        .enumerate()
        .filter(|(index, _)| !skipped.contains(index))
        .map(|(_, file)| file)
        .collect();

    // If the tree contains exactly one installable file, make sure it is
    // executable. `LocalHost` is a no-op on Windows; the SSH host runs
    // `chmod +x` remotely.
    let candidates: Vec<&InstalledFile> = installed
        .iter()
        .filter(|file| !is_skip(&file.origin_path) && file.link_target.is_none())
        .collect();
    if let [single] = candidates.as_slice() {
        backend.chmod(&single.install_path).await?;
    }

    optimize(backend, &installed, config).await;

    let mut report = Report::new();
    report.insert(
        source.to_string(),
        Installed {
            install_dir: dir,
            files: installed,
        },
    );
    if !config.quiet {
        println!("{}", report::display(&report, config, backend));
    }
    Ok(report)
}

/// Write every entry of `files` to its install path.
///
/// Symbolic links are recreated as links. A link is skipped when it points
/// outside the install directory; when the target cannot create links at all
/// (Windows without the symlink privilege), the content it points at is
/// written instead so the installed tree stays usable.
///
/// Returns the indices that were skipped.
async fn write_files(
    backend: &Backend,
    files: &[TreeFile],
    installed: &[InstalledFile],
    dir: &str,
    config: &InstallConfig,
) -> Result<Vec<usize>> {
    let mut skipped = Vec::new();
    let mut warned = false;

    for (index, file) in files.iter().enumerate() {
        let path = &installed[index].install_path;
        let Some(target) = &file.link_target else {
            backend.write(path, &file.data, file.mode).await?;
            continue;
        };

        if !tree::link_stays_inside(dir, path, target) {
            if !config.quiet {
                eprintln!(
                    "Warning: skipped symlink '{path}' -> '{target}': it points outside the install directory"
                );
            }
            skipped.push(index);
            continue;
        }

        match backend.symlink(target, path).await {
            Ok(()) => trace!("link {path} -> {target}"),
            Err(e) => {
                let Some(donor) = tree::link_donor(files, file) else {
                    if !config.quiet {
                        eprintln!("Warning: skipped symlink '{path}' -> '{target}': {e}");
                    }
                    skipped.push(index);
                    continue;
                };
                // Say it once: a host that cannot link would otherwise warn for
                // every link in the archive.
                if !warned && !config.quiet {
                    eprintln!(
                        "Warning: could not create symlinks on {} ({e}); installing copies instead",
                        backend.label()
                    );
                    warned = true;
                }
                trace!(
                    "copying '{path}' from '{}' instead of linking",
                    files[donor].path
                );
                backend
                    .write(path, &files[donor].data, files[donor].mode)
                    .await?;
            }
        }
    }

    Ok(skipped)
}

/// Run `strip` / `upx` on the single executable of a tree.
async fn optimize(backend: &Backend, files: &[InstalledFile], config: &InstallConfig) {
    if !config.strip && !config.upx {
        return;
    }

    let executables: Vec<&InstalledFile> = files
        .iter()
        .filter(|file| file.link_target.is_none())
        .filter(|file| executable(&get_filename(&file.install_path), &file.mode))
        .collect();

    match executables.as_slice() {
        [single] => {
            // Optimization is best effort: the file is already installed and
            // usable, so a missing or failing strip/upx only warns.
            if let Err(e) = optimize_executable(
                backend,
                &single.install_path,
                config.strip,
                config.upx,
                config.quiet,
            )
            .await
                && !config.quiet
            {
                eprintln!("Warning: {e}");
            }
        }
        [] => {}
        executables => {
            if !config.quiet {
                eprintln!(
                    "Warning: --strip and --upx only work with single executable installations"
                );
                eprintln!(
                    "  Found {} executables, skipping optimization",
                    executables.len()
                );
            }
        }
    }
}

/// Verify there is room for `files` in `dir`, discounting files that will be
/// replaced.
async fn check_disk_space(backend: &Backend, files: &[InstalledFile], dir: &str) -> Result<()> {
    // Create the install directory before measuring free space. Measuring the
    // nearest existing ancestor can report the wrong filesystem when the
    // target is a mount point. `mkdir -p` is idempotent.
    backend.mkdir(dir).await?;

    // `None` means free space is unknown; `Some(0)` is a full filesystem.
    let Ok(Some(available)) = backend.free_space(dir).await else {
        return Ok(());
    };

    let required: u64 = files.iter().map(|file| file.size).sum();

    // Fast path: everything fits even if nothing is replaced. This costs no
    // extra round trips, which matters for SSH.
    if available >= required {
        return Ok(());
    }

    // Overwriting an existing file frees its blocks, so re-installing a 25 MB
    // package over a 24 MB copy only needs about 1 MB. `write_to_file` removes
    // the old file before writing, so the full old size is available again.
    let mut reclaimed: u64 = 0;
    for file in files {
        if let Some(size) = backend.file_size(&file.install_path).await {
            reclaimed = reclaimed.saturating_add(size);
        }
    }

    let needed = space_needed(required, reclaimed);
    if available < needed {
        let reclaim_note = if reclaimed > 0 {
            format!(
                "\n  Existing files will be replaced: {}\n  Additional space needed: {}",
                human_size(reclaimed as usize),
                human_size(needed as usize)
            )
        } else {
            String::new()
        };
        return Err(err!(
            r#"Insufficient disk space for installation
  Installation directory: {}
  Available space: {}
  Required space: {}{reclaim_note}"#,
            dir,
            format_size(available),
            human_size(required as usize),
        ));
    }

    Ok(())
}

fn format_size(size: u64) -> String {
    humansize::format_size(
        size,
        if cfg!(windows) {
            humansize::WINDOWS
        } else {
            humansize::DECIMAL
        },
    )
}

/// Space that must be free *in addition to* what replacing existing files
/// frees again. Saturating: replacing larger files (a downgrade) can free more
/// than the install writes.
fn space_needed(required: u64, reclaimed: u64) -> u64 {
    required.saturating_sub(reclaimed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(name: &str, files: usize) -> FileTree {
        FileTree {
            name: name.to_string(),
            files: (0..files)
                .map(|i| TreeFile {
                    path: format!("file{i}"),
                    origin_path: format!("file{i}"),
                    mode: None,
                    link_target: None,
                    data: Vec::new(),
                })
                .collect(),
        }
    }

    /// A multi-file tree gets its own directory: `--alias` names it, otherwise
    /// the tool name does. A single file is written straight into the base
    /// directory.
    #[tokio::test]
    async fn multi_file_trees_get_their_own_directory() {
        let backend = Backend::local();
        let home = backend.home_dir();

        let config = InstallConfig::default();
        assert_eq!(
            install_dir_for(&backend, &config, &tree("tool", 2), false),
            format!("{home}/tool")
        );
        assert_eq!(
            install_dir_for(&backend, &config, &tree("tool", 1), false),
            home
        );

        let config = InstallConfig {
            alias: Some("alias".into()),
            ..Default::default()
        };
        assert_eq!(
            install_dir_for(&backend, &config, &tree("tool", 2), false),
            format!("{home}/alias")
        );
        // An alias only renames the file when there is a single one.
        assert_eq!(
            install_dir_for(&backend, &config, &tree("tool", 1), false),
            home
        );
    }

    /// Several bundles from one source must not share a directory: CI artifacts
    /// such as `ladybird-js-*` and `ladybird-wasm-*` both ship `bin/js`, so
    /// writing them side by side loses one of them.
    #[tokio::test]
    async fn concurrent_bundles_are_isolated() {
        let backend = Backend::local();
        let home = backend.home_dir();

        // Even with an explicit `-d`, each multi-file bundle gets a subdirectory.
        let config = InstallConfig {
            dir: Some("C:\\tmp\\ei".into()),
            ..Default::default()
        };
        // The base is platform-dependent (`-d` is expanded by the host), so the
        // expected directories are derived from it instead of hardcoded.
        let base = base_install_dir(&backend, &config);
        let dirs: Vec<String> = ["ladybird-js", "ladybird-wasm"]
            .into_iter()
            .map(|name| install_dir_for(&backend, &config, &tree(name, 3), true))
            .collect();
        assert_eq!(dirs[0], join_path(&base, "ladybird-js"), "{dirs:?}");
        assert_eq!(dirs[1], join_path(&base, "ladybird-wasm"), "{dirs:?}");
        assert_ne!(dirs[0], dirs[1]);

        // A single-file bundle stays flat so its executable lands in PATH.
        assert_eq!(
            install_dir_for(&backend, &config, &tree("tool", 1), true),
            base
        );

        // Without `-d` the isolated directories live under the home directory.
        let config = InstallConfig::default();
        assert_eq!(
            install_dir_for(&backend, &config, &tree("tool", 3), true),
            format!("{home}/tool")
        );
    }

    /// Install paths are `/`-separated on every platform so that the path
    /// helpers (`dirname`, `get_filename`) behave uniformly.
    #[tokio::test]
    async fn install_dir_is_slash_separated() {
        let backend = Backend::local();
        let config = InstallConfig {
            dir: Some("C:\\tmp\\ei".into()),
            ..Default::default()
        };
        let dir = base_install_dir(&backend, &config);
        assert!(!dir.contains('\\'), "{dir}");
        assert!(dir.ends_with("tmp/ei"), "{dir}");
    }

    #[test]
    fn replaced_files_are_discounted() {
        // Fresh install: everything must fit.
        assert_eq!(space_needed(25, 0), 25);
        // Re-installing over a slightly older build: only the delta matters.
        assert_eq!(space_needed(25, 24), 1);
        // Identical size: nothing extra needed.
        assert_eq!(space_needed(25, 25), 0);
        // A downgrade frees more than it writes: never negative.
        assert_eq!(space_needed(25, 40), 0);
        // Multiple files: callers sum both sides first.
        assert_eq!(space_needed(10 + 20, 5 + 22), 3);
    }
}
