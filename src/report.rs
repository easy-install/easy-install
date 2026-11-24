//! What an install produced, and how it is reported.
//!
//! [`Report`] is the final result of the pipeline: the files written into each
//! install directory, keyed by the source that produced them. It is used for
//! the human-readable summary and for registering directories in `$PATH`.

use crate::InstallConfig;
use crate::backend::{Backend, Host};
use crate::path::{
    DEEP, EXEC_MASK, dirname, ends_with_exe, get_filename, is_license_file, is_skip,
};
use easy_archive::{human_size, mode_to_string};
use std::collections::{BTreeMap, HashSet};

/// Maximum number of files listed individually before summarizing a directory.
const MAX_FILE_COUNT: usize = 16;

/// One file written to the install target.
#[derive(Debug, Clone)]
pub(crate) struct InstalledFile {
    pub(crate) install_path: String,
    pub(crate) origin_path: String,
    pub(crate) mode: Option<u32>,
    /// Link target when the entry was installed as a symbolic link.
    pub(crate) link_target: Option<String>,
    pub(crate) size: u64,
}

/// Files written from one source into one directory.
#[derive(Debug, Clone, Default)]
pub(crate) struct Installed {
    pub(crate) install_dir: String,
    pub(crate) files: Vec<InstalledFile>,
}

/// Everything one invocation installed, keyed by source (usually the URL).
pub(crate) type Report = BTreeMap<String, Installed>;

/// Render the install summary.
pub(crate) fn display(report: &Report, config: &InstallConfig, backend: &Backend) -> String {
    let total: u64 = report
        .values()
        .flat_map(|installed| installed.files.iter().map(|file| file.size))
        .sum();

    let mut lines = vec![format!(
        "Installation Successful ({})",
        human_size(total as usize)
    )];

    for installed in report.values() {
        if installed.files.len() > MAX_FILE_COUNT {
            let size: u64 = installed.files.iter().map(|file| file.size).sum();
            lines.push(
                [
                    human_size(size as usize).as_str(),
                    format!("(total {})", installed.files.len()).as_str(),
                    &backend.expand(&installed.install_dir),
                ]
                .join(" "),
            );
            continue;
        }

        let max_size_len = installed
            .files
            .iter()
            .fold(0, |max, file| max.max(human_size(file.size as usize).len()));
        let is_single = installed.files.len() == 1;

        for file in &installed.files {
            let size = human_size(file.size as usize);
            // The post-optimisation size is only measurable locally.
            let info = if is_single && (config.strip || config.upx) {
                let new_size = if backend.is_remote() {
                    String::new()
                } else {
                    human_size(file_size(&file.install_path))
                };
                format!("{new_size} {}", file.install_path)
                    .trim()
                    .to_string()
            } else {
                file.install_path.clone()
            };
            lines.push(
                [
                    mode_string(file),
                    " ".repeat(max_size_len - size.len()) + &size,
                    [file.origin_path.as_str(), &info].join(" -> "),
                ]
                .join(" "),
            );
        }
    }

    lines.join("\n")
}

/// Permission column of the summary; links get the `l` prefix they have on
/// disk.
fn mode_string(file: &InstalledFile) -> String {
    match file.link_target {
        Some(_) => "lrwxrwxrwx".to_string(),
        None => mode_to_string(file.mode.unwrap_or(0), false),
    }
}

/// Register installed directories in `$PATH`.
///
/// Remote shells are edited over SSH in the same way as local ones.
pub(crate) async fn add_to_path(report: &Report, config: &InstallConfig, backend: &Backend) {
    if backend.is_remote() {
        let mut seen = HashSet::new();
        for installed in report.values() {
            let dir = backend.expand(&installed.install_dir);
            if seen.insert(dir.clone()) {
                register_path(backend, &dir, config).await;
            }
        }
        return;
    }

    // Every installable (non-skip, non-license) file is a candidate
    // executable; if there is exactly one, it is treated as the executable
    // even without an exec bit or a known extension.
    let mut maybe_exe = HashSet::new();
    for installed in report.values() {
        for file in &installed.files {
            let installable = !is_skip(&file.install_path) && !is_license_file(&file.install_path);
            if installable {
                maybe_exe.insert(file.install_path.clone());
            }
            if depth(&file.origin_path) <= DEEP
                && installable
                && let Some(shadowing) = shadowing_binary(file)
                && !config.quiet
            {
                let message = if shadowing != file.install_path {
                    format!("Warning: file exists at {shadowing}")
                } else {
                    format!("Warning: file updated at {shadowing}")
                };
                println!("{message}");
            }
        }
    }

    let mut added = HashSet::new();
    for installed in report.values() {
        if added.insert(installed.install_dir.clone()) {
            register_path(backend, &installed.install_dir, config).await;
        }

        for file in &installed.files {
            let is_exe = (maybe_exe.len() == 1 && maybe_exe.contains(&file.install_path))
                || ends_with_exe(&file.origin_path)
                || file.mode.unwrap_or(0) & EXEC_MASK != 0;
            // `dirname` keeps the trailing separator; the install directory
            // does not, so compare without it to avoid registering twice.
            let dir = dirname(&file.install_path);
            let dir = dir.trim_end_matches('/');
            if depth(&file.origin_path) <= DEEP && is_exe && added.insert(dir.to_string()) {
                register_path(backend, dir, config).await;
            }
        }
    }
}

/// Register `dir` in `$PATH`.
///
/// The files are installed either way, so a failure only warns: a read-only
/// shell profile must not turn a successful install into an error.
async fn register_path(backend: &Backend, dir: &str, config: &InstallConfig) {
    let quiet = config.quiet;
    if let Err(e) = backend.add_to_path(dir, config.shell, quiet).await
        && !quiet
    {
        eprintln!(
            "Warning: could not add '{dir}' to PATH: {}",
            crate::error::format_chain(&e)
        );
    }
}

fn depth(path: &str) -> usize {
    path.split('/').count()
}

/// Path of an executable with the same name that `$PATH` resolves to, when it
/// is not the file just installed.
fn shadowing_binary(file: &InstalledFile) -> Option<String> {
    let name = get_filename(&file.install_path);
    if !crate::path::executable(&name, &file.mode) {
        return None;
    }
    let found = which::which(&name).ok()?;
    let found = crate::path::path_to_str(&found);
    (found != file.install_path).then_some(found)
}

fn file_size(path: &str) -> usize {
    std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0) as usize
}
