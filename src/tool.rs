use crate::InstallConfig;
use crate::artifact::GhArtifacts;
use crate::env::add_to_path;
use crate::manfiest::DistManifest;
use crate::types::{Output, OutputFile};
use anyhow::{Context, Result};
use easy_archive::{Fmt, clean};
use easy_archive::{human_size, mode_to_string, types::IntoEnumIterator};
use guess_target::{Abi, Arch, Os, guess_target};
use regex::Regex;
use std::collections::HashSet;
#[cfg(unix)]
use std::os::unix::prelude::PermissionsExt;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::LazyLock;

pub(crate) const DEEP: usize = 3;
pub(crate) const WINDOWS_EXE_EXTS: [&str; 6] = [".exe", ".ps1", ".bat", ".cmd", ".com", ".vbs"];
pub(crate) const INSTALLER_EXTS: [&str; 11] = [
    ".msi",
    ".msix",
    ".appx",
    ".deb",
    ".rpm",
    ".dmg",
    ".pkg",
    ".app",
    ".apk",
    ".ipa",
    ".appimage",
];
pub(crate) const TEXT_FILE_EXTS: [&str; 19] = [
    ".txt", ".md", ".json", ".xml", ".csv", ".log", ".ini", ".cfg", ".conf", ".yaml", ".yml",
    ".rsa", ".pub", ".ed25519", ".jsonl", ".json5", ".md5", ".sha256", ".sha512",
];
pub(crate) const MAYBE_EXECUTABLE_EXTS: [&str; 13] = [
    ".out", ".sh", ".bash", ".zsh", ".py", ".pl", ".js", ".ts", ".jsx", ".tsx", ".wasm", ".fish",
    ".nu",
];

pub(crate) const SKIP_FMT_LIST: [&str; 20] = [
    ".sha256sum",
    ".sha256",
    ".sha1",
    ".md5",
    ".sum",
    ".msi",
    ".msix",
    ".appx",
    ".app",
    ".appimage",
    ".json",
    ".txt",
    ".md",
    ".log",
    ".sig",
    ".asc",
    ".intoto.jsonl",
    ".jsonl",
    ".sha256",
    ".sha512",
];

pub(crate) fn is_known_format(s: &str) -> bool {
    let all: &[&[&str]] = &[
        &WINDOWS_EXE_EXTS[..],
        &INSTALLER_EXTS[..],
        &TEXT_FILE_EXTS[..],
        &MAYBE_EXECUTABLE_EXTS[..],
        &SKIP_FMT_LIST[..],
    ];

    for i in all {
        for ext in i.iter() {
            if s.ends_with(ext) {
                return true;
            }
        }
    }

    false
}
const LICENSE_PREFIXES: &[&str] = &["license", "licence", "copying", "unlicense", "copyright"];
const LICENSE_EXTS: &[&str] = &["", "md", "txt", "rst"];
pub fn is_license_file(p: &str) -> bool {
    let name = get_filename(p).to_lowercase();
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) => (s, e),
        None => (name.as_str(), ""),
    };
    if !LICENSE_EXTS.contains(&ext) {
        return false;
    }
    LICENSE_PREFIXES
        .iter()
        .any(|prefix| stem.starts_with(prefix))
}

pub(crate) fn is_skip(s: &str) -> bool {
    s.rsplit('/').next().unwrap_or_default().starts_with('.')
        || INSTALLER_EXTS
            .iter()
            .chain(TEXT_FILE_EXTS.iter())
            .chain(SKIP_FMT_LIST.iter())
            .any(|&ext| s.to_ascii_lowercase().ends_with(&ext.to_ascii_lowercase()))
}

pub(crate) fn get_bin_name(s: &str) -> String {
    if cfg!(windows) && !WINDOWS_EXE_EXTS.iter().any(|i| s.ends_with(i)) && !s.contains(".") {
        return s.to_string() + ".exe";
    }
    s.to_string()
}

fn abs_path(p: &str) -> PathBuf {
    std::path::absolute(p).unwrap_or(p.into())
}

const MAX_FILE_COUNT: usize = 16;
pub(crate) fn display_output(output: &Output, config: &InstallConfig) -> String {
    let s: u64 = output
        .values()
        .flat_map(|v| v.files.iter().map(|f| f.size))
        .sum();

    let mut v = vec![format!(
        "Installation Successful ({})",
        human_size(s as usize)
    )];

    for i in output.values() {
        if i.files.len() > MAX_FILE_COUNT {
            let sum_size = i.files.iter().fold(0, |pre, cur| pre + cur.size);
            v.push(
                [
                    human_size(sum_size as usize).as_str(),
                    format!("(total {})", i.files.len()).as_str(),
                    &path_to_str(abs_path(&i.install_dir).as_path()),
                ]
                .join(" "),
            );
        } else {
            let max_size_len = i
                .files
                .iter()
                .fold(0, |pre, cur| pre.max(human_size(cur.size as usize).len()));

            let is_single = i.files.len() == 1;

            for k in &i.files {
                let s = human_size(k.size as usize);
                let install_info = if is_single && config.strip | config.upx {
                    let size = human_size(file_size(&k.install_path));
                    format!("{} {}", size, k.install_path)
                } else {
                    k.install_path.to_string()
                };
                v.push(
                    [
                        mode_to_string(k.mode.unwrap_or(0), k.is_dir),
                        " ".repeat(max_size_len - s.len()) + &s,
                        [k.origin_path.as_str(), &install_info].join(" -> "),
                    ]
                    .join(" "),
                );
            }
        }
    }
    v.join("\n")
}

fn dirname(s: &str) -> String {
    let i = s.rfind('/').map_or(s.len(), |i| i + 1);
    s[0..i].to_string()
}

fn file_size(p: &str) -> usize {
    std::fs::metadata(p).map(|i| i.len()).unwrap_or(0) as usize
}

pub(crate) fn add_output_to_path(output: &Output, config: &InstallConfig) {
    // Collect candidate executable files (non-skipped, non-license).
    // If exactly one candidate exists, it is treated as the executable
    // even without an exec bit or known extension.
    let mut maybe_exe = HashSet::new();
    for v in output.values() {
        for f in &v.files {
            let is_installable = !is_skip(&f.install_path) && !is_license_file(&f.install_path);
            if is_installable {
                maybe_exe.insert(f.install_path.clone());
            }
            let deep = f.origin_path.split("/").count();
            if deep <= DEEP
                && is_installable
                && let Some(p) = check(f)
                && !config.quiet
            {
                let msg = if p != f.install_path {
                    format!("Warning: file exists at {p}")
                } else {
                    format!("Warning: file updated at {p}")
                };
                println!("{msg}");
            }
        }
    }

    let mut filter = HashSet::new();
    for v in output.values() {
        add_to_path(&v.install_dir, config.quiet);

        for f in &v.files {
            let deep = f.origin_path.split("/").count();
            let is_exe = (maybe_exe.len() == 1 && maybe_exe.contains(&f.install_path))
                || ends_with_exe(&f.origin_path)
                || (f.mode.unwrap_or(0) & EXEC_MASK != 0);
            let dir = dirname(&f.install_path);
            if deep <= DEEP && is_exe && !filter.contains(&dir) {
                add_to_path(&dir, config.quiet);
                filter.insert(dir);
            }
        }
    }
}

pub(crate) fn get_filename(s: &str) -> String {
    let s = s.replace("\\\\", "/");
    let s = s.replace("\\", "/");
    let i = s.rfind("/").map_or(0, |i| i + 1);
    s[i..].to_string()
}

const EXEC_MASK: u32 = 0o111;
pub(crate) fn executable(name: &str, mode: &Option<u32>) -> bool {
    ends_with_exe(name) || (!name.contains(".") && mode.unwrap_or(0) & EXEC_MASK != 0)
}

pub(crate) fn check(file: &OutputFile) -> Option<String> {
    let file_path = &file.install_path;
    let name = get_filename(file_path);
    if !executable(&name, &file.mode) {
        return None;
    }
    if let Ok(p) = which::which(&name)
        && !p.as_os_str().is_empty()
        && file_path != &p
    {
        return Some(p.to_string_lossy().to_string());
    }
    None
}

pub(crate) fn write_to_file(src: &str, buffer: &[u8], mode: &Option<u32>) -> Result<()> {
    let d = std::path::PathBuf::from_str(src).context("invalid path for write_to_file")?;
    if let Some(p) = d.parent()
        && !std::fs::exists(p).unwrap_or(false)
    {
        std::fs::create_dir_all(p).context("failed to create_dir_all")?;
    }

    if std::fs::exists(src).unwrap_or(false)
        && let Ok(meta) = std::fs::metadata(src)
    {
        if meta.is_file() {
            std::fs::remove_file(src).context("failed to remove file")?;
        } else {
            anyhow::bail!("target path is a directory, refusing to overwrite: {src}");
        }
    }

    if !buffer.is_empty() {
        std::fs::write(src, buffer).context("failed to write file")?;
    }

    #[cfg(unix)]
    if let Some(mode) = mode
        && *mode > 0
        && !buffer.is_empty()
    {
        std::fs::set_permissions(src, PermissionsExt::from_mode(*mode))
            .context("failed to set_permissions")?;
    }

    #[cfg(windows)]
    {
        _ = mode;
    }
    Ok(())
}

fn has_common_elements(arr1: &[String], arr2: &[String]) -> bool {
    arr1.iter().any(|x| arr2.contains(x))
}

pub(crate) fn get_artifact_url_from_manfiest(
    url: &str,
    manfiest: &DistManifest,
    config: &InstallConfig,
) -> Vec<(String, String)> {
    let mut v = vec![];
    let local_target = config.get_local_target();

    for (key, art) in manfiest.artifacts.iter() {
        let filename = get_filename(key);
        if is_skip(&filename) {
            continue;
        }

        if ends_with_exe(key) && local_target.iter().any(|t| t.os() != Os::Windows) {
            continue;
        }

        if has_common_elements(
            &art.target_triples,
            local_target
                .iter()
                .map(|i| i.to_str().to_string())
                .collect::<Vec<_>>()
                .as_slice(),
        ) {
            if let Some(kind) = &art.kind
                && !["executable-zip"].contains(&kind.as_str())
            {
                continue;
            }
            let name = name_no_ext(&filename);
            let name = guess_target(&name).pop().map_or(name, |i| i.name);
            if !is_url(key) {
                v.push((name, replace_filename(url, key)));
            } else {
                v.push((name, key.to_string()));
            }
            continue;
        }
    }
    v
}

pub(crate) fn get_common_prefix_len(list: &[&str]) -> usize {
    if list.is_empty() {
        return 0;
    }

    if list.len() == 1 {
        match list[0].rfind('/') {
            Some(i) => return i + 1,
            None => return 0,
        }
    }

    let parts: Vec<Vec<&str>> = list.iter().map(|i| i.split('/').collect()).collect();
    let max_len = parts.iter().map(|p| p.len()).max().unwrap_or(0);

    let mut p = 0;
    while p < max_len {
        let head: Vec<_> = parts.iter().map(|k| k.get(p).unwrap_or(&"")).collect();
        let first = head[0];
        if head.iter().any(|&i| i != first) {
            break;
        }
        p += 1;
    }

    if p == 0 {
        return 0;
    }
    parts[0][..p].join("/").len() + 1
}

pub(crate) fn is_executable(mode: u32) -> bool {
    const S_IXUSR: u32 = 0o100; // owner execute
    const S_IXGRP: u32 = 0o010; // group execute
    const S_IXOTH: u32 = 0o001; // others execute

    mode & (S_IXUSR | S_IXGRP | S_IXOTH) != 0
}

pub(crate) fn maybe_executable(name: &str) -> bool {
    MAYBE_EXECUTABLE_EXTS.iter().any(|i| name.ends_with(i))
}

// if no executable file is found, then the only possible executable program is set to executable
pub(crate) fn guess_executable(files: &mut [OutputFile]) {
    let exe_files: Vec<_> = files
        .iter()
        .filter(|i| is_executable(i.mode.unwrap_or(0)))
        .collect();

    if !exe_files.is_empty() {
        return;
    }

    let mut no_ext_files: Vec<_> = files
        .iter_mut()
        .filter(|i| !get_filename(&i.origin_path).contains("."))
        .collect();

    if let &mut [first] = &mut no_ext_files.as_mut_slice() {
        first.mode = Some(0o755);
        return;
    }

    let mut maybe_executable: Vec<_> = files
        .iter_mut()
        .filter(|i| maybe_executable(&i.origin_path))
        .collect();
    if let &mut [first] = &mut maybe_executable.as_mut_slice() {
        first.mode = Some(0o755);
    }
}

fn rename_alias(files: &mut [OutputFile], alias: &str) {
    let file = if files.len() == 1 {
        Some(&mut files[0])
    } else {
        let mut iter = files.iter_mut().filter(|i| {
            let name = get_filename(&i.origin_path);
            executable(&name, &i.mode)
        });

        let first = iter.next();
        if first.is_some() && iter.next().is_none() {
            first
        } else {
            None
        }
    };

    let Some(first) = file else { return };

    let filename = get_filename(&first.install_path);
    let bin = name_no_ext(&filename);
    let alias_name = filename.replace(&bin, alias);
    let d = dirname(&first.install_path);

    first.install_path = clean(&(d + "/" + &alias_name));
}

fn format_size(n: u64) -> String {
    humansize::format_size(
        n,
        if cfg!(windows) {
            humansize::WINDOWS
        } else {
            humansize::DECIMAL
        },
    )
}

pub(crate) fn check_disk_space(files: &[OutputFile], dir: &PathBuf) -> Result<()> {
    let sum: u64 = files.iter().map(|i| i.size).sum();
    let disk = if dir.exists() {
        fs4::available_space(dir).map_err(|e| e.to_string())
    } else if let Some(p) = dir.parent() {
        fs4::available_space(p).map_err(|e| e.to_string())
    } else {
        Err(format!("Not found: {:?}", dir))
    };

    match disk {
        Ok(disk) => {
            if disk < sum {
                return Err(anyhow::anyhow!(
                    r#"Insufficient disk space for installation
  Installation directory: {}
  Available space: {}
  Required space: {}"#,
                    dir.to_string_lossy(),
                    format_size(disk),
                    human_size(sum as usize),
                ));
            }
        }
        Err(e) => {
            eprintln!("Failed to check disk space: {}", e);
        }
    }

    Ok(())
}

pub(crate) fn install_output_files(files: &mut [OutputFile], config: &InstallConfig) -> Result<()> {
    if let Some(alias) = config.alias.clone() {
        rename_alias(files, &alias);
    }

    guess_executable(files);
    for OutputFile {
        install_path,
        buffer,
        mode,
        origin_path,
        ..
    } in files.iter()
    {
        // FIXME: skip __MACOSX
        if origin_path.starts_with("__MACOSX") {
            continue;
        }
        write_to_file(install_path, buffer, mode)?;
    }

    #[cfg(not(windows))]
    {
        let maybe_exe = files
            .iter()
            .filter(|i| !is_skip(&i.origin_path))
            .collect::<Vec<_>>();

        if let [single_exe] = maybe_exe.as_slice() {
            add_execute_permission(&single_exe.install_path)?;
        }
    }

    // Optimize single executable if strip or upx flags are enabled
    if config.strip || config.upx {
        let executables: Vec<_> = files
            .iter()
            .filter(|f| executable(&get_filename(&f.install_path), &f.mode))
            .collect();

        if let [single_exe] = executables.as_slice() {
            use crate::optimize::optimize_executable;
            let _ = optimize_executable(
                &single_exe.install_path,
                config.strip,
                config.upx,
                config.quiet,
            );
        } else if !executables.is_empty() && !config.quiet {
            eprintln!("Warning: --strip and --upx only work with single executable installations");
            eprintln!(
                "  Found {} executables, skipping optimization",
                executables.len()
            );
        }
    }

    Ok(())
}

pub(crate) fn name_no_ext(s: &str) -> String {
    let exts = known_extensions();
    for ext in exts.iter() {
        if s.ends_with(ext.as_str()) {
            return s[0..s.len() - ext.len()].to_string();
        }
    }
    s.to_string()
}

/// Cached, length-sorted list of all known file extensions (archive formats
/// plus executable/installer/text/skip extensions). Built once and reused.
fn known_extensions() -> &'static Vec<String> {
    static CACHE: LazyLock<Vec<String>> = LazyLock::new(|| {
        let mut v: Vec<String> = Fmt::iter()
            .flat_map(|i| i.extensions().to_vec())
            .map(|e| e.to_string())
            .chain(
                WINDOWS_EXE_EXTS
                    .iter()
                    .chain(INSTALLER_EXTS.iter())
                    .chain(TEXT_FILE_EXTS.iter())
                    .chain(MAYBE_EXECUTABLE_EXTS.iter())
                    .chain(SKIP_FMT_LIST.iter())
                    .map(|e| e.to_string()),
            )
            .collect();
        v.sort_by_key(|b| std::cmp::Reverse(b.len()));
        v
    });
    &CACHE
}

#[cfg(not(windows))]
pub(crate) fn add_execute_permission(file_path: &str) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(metadata) = std::fs::metadata(file_path).context("metadata failed") {
        if metadata.is_dir() {
            return Ok(());
        }

        let mut permissions = metadata.permissions();
        let current_mode = permissions.mode();

        let new_mode = current_mode | EXEC_MASK;
        permissions.set_mode(new_mode);

        std::fs::set_permissions(file_path, permissions)?
    }
    Ok(())
}

pub(crate) fn expand_path(path: &str) -> String {
    if path.starts_with("~") {
        let expanded = shellexpand::tilde(path);
        path_clean::PathClean::clean(Path::new(&*expanded))
            .to_string_lossy()
            .to_string()
    } else {
        abs_path(path).to_string_lossy().to_string()
    }
}

pub(crate) fn is_archive_file(s: &str) -> bool {
    Fmt::guess(s).is_some()
}

pub(crate) fn ends_with_exe(s: &str) -> bool {
    WINDOWS_EXE_EXTS.iter().any(|i| s.ends_with(i))
}
pub(crate) fn is_exe_file(s: &str) -> Result<bool> {
    if ends_with_exe(s) {
        return Ok(true);
    }
    static RE_LATEST: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^https://github\.com/([^/]+)/([^/]+)/releases/latest/download/([^/]+)$")
            .unwrap()
    });
    static RE_TAG: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^https://github\.com/([^/]+)/([^/]+)/releases/download/([^/]+)/([^/]+)$")
            .unwrap()
    });
    static RE_TAG2: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"^https://github\.com/([^/]+)/([^/]+)/releases/download/([^/]+)/([^/]+)/([^/]+)$",
        )
        .unwrap()
    });
    for (re, n) in [(&*RE_TAG2, 5), (&*RE_TAG, 4), (&*RE_LATEST, 3)] {
        if let Some(cap) = re.captures(s)
            && let Some(name) = cap.get(n)
        {
            if is_archive_file(name.as_str()) {
                return Ok(false);
            }
            if !name.as_str().contains(".") {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// Parse and validate URL before downloading
/// Returns Ok(Url) if valid, Err with description if invalid
pub(crate) fn parse_and_validate_url(url: &str) -> Result<reqwest::Url> {
    // Check if URL is empty
    if url.trim().is_empty() {
        return Err(anyhow::anyhow!("URL cannot be empty"));
    }

    // Parse URL
    let parsed = reqwest::Url::parse(url).context(format!("Invalid URL format: {}", url))?;

    // Check scheme (only allow http/https)
    let scheme = parsed.scheme();
    if scheme != "http" && scheme != "https" {
        return Err(anyhow::anyhow!(
            "Invalid URL scheme '{}': only http and https are allowed",
            scheme
        ));
    }

    Ok(parsed)
}

pub(crate) fn is_url(s: &str) -> bool {
    parse_and_validate_url(s).is_ok()
}

pub(crate) fn is_dist_manfiest(s: &str) -> bool {
    s.ends_with(".json")
}

pub(crate) fn path_to_str(p: &Path) -> String {
    p.to_str().unwrap().replace("\\", "/")
}

pub(crate) fn replace_filename(base_url: &str, name: &str) -> String {
    if let Some(pos) = base_url.rfind('/') {
        format!("{}{}", &base_url[..pos + 1], name)
    } else {
        name.to_string()
    }
}

pub(crate) fn get_artifact_url(
    artifacts: GhArtifacts,
    config: &InstallConfig,
) -> Result<Vec<(String, String)>> {
    use crate::types::Repo;

    let mut v = vec![];
    let local_target = config.get_local_target();

    for i in artifacts.assets {
        if is_skip(&i.browser_download_url) {
            continue;
        }
        if ends_with_exe(&i.browser_download_url)
            && local_target.iter().any(|t| t.os() != Os::Windows)
        {
            continue;
        }
        let filename = get_filename(&i.browser_download_url);
        let name = name_no_ext(&filename);
        let guess = guess_target(&name);

        // Match priority:
        // 1. Exact target match (including abi).
        // 2. Fuzzy match by (arch, os), ignoring abi (only when --fuzzy is
        //    set). This catches assets whose filename omits the abi (e.g.
        //    "mihomo-linux-amd64.tar.gz" parsed as gnu) when the user
        //    requested musl, since the binary is typically abi-agnostic or
        //    the provider just didn't tag it. Fuzzy matches are penalized
        //    by RANK_PENALTY so exact matches win when both exist.
        const RANK_PENALTY: u32 = 1;
        let mut penalized = false;
        let item = if let Some(t) = config.target {
            let exact = guess.iter().find(|i| i.target == t);
            if let Some(m) = exact {
                Some(m)
            } else if config.fuzzy {
                let fuzzy = guess
                    .iter()
                    .find(|i| i.target.arch() == t.arch() && i.target.os() == t.os());
                if fuzzy.is_some() {
                    penalized = true;
                }
                fuzzy
            } else {
                None
            }
        } else {
            let exact = guess.iter().find(|i| local_target.contains(&i.target));
            if let Some(m) = exact {
                Some(m)
            } else if config.fuzzy {
                let fuzzy = guess.iter().find(|i| {
                    local_target
                        .iter()
                        .any(|t| t.arch() == i.target.arch() && t.os() == i.target.os())
                });
                if fuzzy.is_some() {
                    penalized = true;
                }
                fuzzy
            } else {
                None
            }
        };

        if let Some(item) = item {
            // HACK: Prioritize using musl on the arm platform
            let hack_musl = match (item.target.arch(), item.target.abi()) {
                (Arch::Aarch64, Some(Abi::Musl)) => 10,
                _ => 0,
            };
            let rank = item.rank + hack_musl;
            let rank = if penalized {
                rank.saturating_sub(RANK_PENALTY)
            } else {
                rank
            };
            v.push((rank, item.name.clone(), i.browser_download_url.clone()));
        }
    }

    // we should still apply rank-based deduplication (keep only highest-rank per name).
    let max_rank = v.iter().fold(0, |pre, cur| pre.max(cur.0));
    let mut filter = vec![];
    let mut list = vec![];
    // FIXME: Need user to select eg: llrt-no-sdk llrt-full-sdk
    for (rank, name, url) in v {
        if rank < max_rank {
            continue;
        }
        if filter.contains(&name) {
            continue;
        }

        filter.push(name.clone());
        let proxied_url = Repo::convert_github_url_to_proxy(&url, config.proxy);
        list.push((name, proxied_url));
    }
    Ok(list)
}

/// Apply `--alias` and `--name` filters to a list of (name, url) artifacts.
///
/// - When `--alias` is set and matches at least one artifact name, only those
///   matching artifacts are kept (rename happens later in `install_output_files`).
/// - When `--name` is set, only artifacts whose name is in the list are kept.
pub(crate) fn filter_artifacts(
    artifact_url: Vec<(String, String)>,
    config: &InstallConfig,
) -> Vec<(String, String)> {
    let filtered = if let Some(alias) = &config.alias {
        let matching: Vec<_> = artifact_url
            .iter()
            .filter(|(name, _)| name == alias)
            .cloned()
            .collect();
        if matching.is_empty() {
            artifact_url
        } else {
            matching
        }
    } else {
        artifact_url
    };

    if config.name.is_empty() {
        filtered
    } else {
        filtered
            .into_iter()
            .filter(|(name, _)| config.name.contains(name))
            .collect()
    }
}

pub(crate) fn not_found_asset_message(url: &str, config: &InstallConfig) {
    if !config.quiet {
        println!(
            "Not found asset for os:{} arch:{} target:{} on {}",
            std::env::consts::OS,
            std::env::consts::ARCH,
            config
                .get_local_target()
                .iter()
                .map(|i| i.to_str().to_string())
                .collect::<Vec<_>>()
                .join(", "),
            url
        );
    }
}
#[cfg(test)]
mod test {
    use anyhow::Context;

    use crate::{
        InstallConfig,
        download::download_dist_manfiest,
        tool::{dirname, get_artifact_url_from_manfiest, is_archive_file, is_exe_file, is_url},
        types::Repo,
    };
    use github_proxy::Proxy;

    use super::{get_bin_name, get_common_prefix_len};

    #[test]
    fn test_is_file() {
        assert!(!is_archive_file("https://github.com/ahaoboy/ansi2"));
        assert!(!is_archive_file(
            "https://api.github.com/repos/ahaoboy/ansi2/releases/latest"
        ));
        assert!(!is_archive_file(
            "https://github.com/ahaoboy/ansi2/releases/tag/v0.2.11"
        ));
        assert!(is_archive_file(
            "https://github.com/ahaoboy/ansi2/releases/download/v0.2.11/ansi2-x86_64-unknown-linux-musl.tar.gz"
        ));
        assert!(is_archive_file(
            "https://github.com/ahaoboy/ansi2/releases/download/v0.2.11/ansi2-x86_64-pc-windows-msvc.zip"
        ));
    }

    #[test]
    fn test_is_github() {
        let repo = Repo {
            owner: "ahaoboy".to_string(),
            name: "ansi2".to_string(),
            tag: None,
        };
        assert_eq!(
            Repo::try_from("https://github.com/ahaoboy/ansi2")
                .context("failed to try_from")
                .unwrap(),
            repo
        );

        assert!(
            Repo::try_from("https://api.github.com/repos/ahaoboy/ansi2/releases/latest")
                .context("failed to try_from")
                .is_err()
        );
        assert_eq!(
            Repo::try_from("ahaoboy/ansi2")
                .context("failed to try_from")
                .unwrap(),
            repo
        );
        let repo = Repo {
            owner: "ahaoboy".to_string(),
            name: "ansi2".to_string(),
            tag: Some("v0.2.11".to_string()),
        };
        assert_eq!(
            Repo::try_from("ahaoboy/ansi2@v0.2.11")
                .context("failed to try_from")
                .unwrap(),
            repo
        );
        assert_eq!(
            Repo::try_from("https://github.com/ahaoboy/ansi2/releases/tag/v0.2.11")
                .context("failed to try_from")
                .unwrap(),
            repo
        );

        assert_eq!(
          Repo::try_from("https://github.com/ahaoboy/ansi2/releases/download/v0.2.11/ansi2-x86_64-unknown-linux-musl.tar.gz").context("failed to try_from").unwrap(),
          repo
        );

        assert_eq!(
          Repo::try_from("https://github.com/ahaoboy/ansi2/releases/download/v0.2.11/ansi2-x86_64-pc-windows-msvc.zip").context("failed to try_from").unwrap(),
          repo
        );

        // let repo = Repo {
        //     owner: "Ryubing".to_string(),
        //     name: "Ryujinx".to_string(),
        //     tag: Some("1.2.78".to_string()),
        // };
        // assert_eq!(
        //   Repo::try_from("https://github.com/Ryubing/Ryujinx/releases/download/1.2.78/ryujinx-*.*.*-win_x64.zip").context("failed to try_from").unwrap(),
        //   repo
        // );
    }

    #[test]
    fn test_is_url() {
        assert!(is_url("https://github.com/ahaoboy/ansi2"));
        assert!(!is_url("ansi2"));
    }

    #[tokio::test]
    async fn test_get_artifact_api() {
        let repo = Repo::try_from("https://github.com/axodotdev/cargo-dist").unwrap();
        let url = repo.get_artifact_api();
        assert_eq!(
            url,
            "https://api.github.com/repos/axodotdev/cargo-dist/releases/latest"
        );
    }
    #[tokio::test]
    async fn test_get_manfiest() {
        // TODO: support latest tag
        // let repo = Repo::try_from("https://github.com/axodotdev/cargo-dist/releases").unwrap();
        // let url = repo.get_manfiest_url(Proxy::Github, 3, 600).await.unwrap();
        // assert_eq!(
        //     url,
        //     "https://github.com/axodotdev/cargo-dist/releases/latest/download/dist-manifest.json"
        // );
        // assert!(repo.get_manfiest(3, Proxy::Github, 30).await.is_ok());

        let repo =
            Repo::try_from("https://github.com/axodotdev/cargo-dist/releases/tag/v0.25.1").unwrap();
        let url = repo.get_manfiest_url(Proxy::Github, 3, 600).await.unwrap();
        assert_eq!(
            url,
            "https://github.com/axodotdev/cargo-dist/releases/download/v0.25.1/dist-manifest.json"
        );

        let manfiest = repo.get_manfiest(3, Proxy::Github, 30).await.unwrap();
        assert!(!manfiest.artifacts.is_empty());

        let repo =
            Repo::try_from("https://github.com/ahaoboy/mujs-build/releases/tag/v0.0.2").unwrap();
        let url = repo.get_manfiest_url(Proxy::Github, 3, 600).await.unwrap();
        assert_eq!(
            url,
            "https://github.com/ahaoboy/mujs-build/releases/download/v0.0.2/dist-manifest.json"
        );

        let manfiest = repo.get_manfiest(3, Proxy::Github, 30).await.unwrap();
        assert!(!manfiest.artifacts.is_empty())
    }

    // #[tokio::test]
    // async fn test_install_from_manfiest() {
    //     let url =
    //         "https://github.com/ahaoboy/mujs-build/releases/latest/download/dist-manifest.json";
    //     let manfiest = download_dist_manfiest(url)
    //         .await
    //         .context("failed to download_dist_manfiest")
    //         .unwrap();
    //     let art_url = get_artifact_url_from_manfiest(url, &manfiest);
    //     assert!(!art_url.is_empty())
    // }

    #[tokio::test]
    async fn test_cargo_dist() {
        let url = "https://github.com/axodotdev/cargo-dist/releases/download/v1.0.0-rc.1/dist-manifest.json";
        let manfiest = download_dist_manfiest(url, 3, 30).await.unwrap();
        let art_url = get_artifact_url_from_manfiest(url, &manfiest, &InstallConfig::default());
        assert!(!art_url.is_empty())
    }

    #[tokio::test]
    async fn test_deno() {
        let url = "https://github.com/denoland/deno";
        let repo = Repo::try_from(url).unwrap();
        let artifact_url = repo.get_artifact_url(&Default::default()).await.unwrap();
        println!("artifact_url{artifact_url:?}");
        assert_eq!(artifact_url.len(), 3);
    }

    #[tokio::test]
    async fn test_starship() {
        let repo = Repo::try_from("https://github.com/starship/starship").unwrap();
        let artifact_url = repo.get_artifact_url(&Default::default()).await.unwrap();
        println!("{artifact_url:?}");
        assert_eq!(artifact_url.len(), 1);
    }

    #[test]
    fn test_is_exe_file() {
        for (a, b) in [
            (
                "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe",
                true,
            ),
            (
                "https://github.com/pnpm/pnpm/releases/latest/download/pnpm-win-x64.exe",
                true,
            ),
            (
                "https://github.com/pnpm/pnpm/releases/latest/download/pnpm-win-x64",
                true,
            ),
            (
                "https://github.com/easy-install/easy-install/releases/download/v0.1.5/ei-x86_64-apple-darwin.tar.gz",
                false,
            ),
            ("https://github.com/easy-install/easy-install", false),
            (
                "https://github.com/easy-install/easy-install/releases/tag/v0.1.5",
                false,
            ),
            (
                "https://github.com/biomejs/biome/releases/download/cli/v1.9.4/biome-darwin-arm64",
                true,
            ),
            (
                "https://github.com/biomejs/biome/releases/download/cli/v1.9.4/biome-darwin-arm64.zip",
                false,
            ),
            (
                "https://github.com/biomejs/biome/releases/download/cli/v1.9.4/biome-darwin-arm64.msi",
                false,
            ),
        ] {
            assert_eq!(
                is_exe_file(a)
                    .context("failed to check is_exe_file")
                    .unwrap(),
                b
            );
        }
    }

    #[test]
    fn test_get_common_prefix() {
        assert_eq!(get_common_prefix_len(&["/a/ab/c", "/a/ad", "/a/ab/d",]), 3);
        assert_eq!(get_common_prefix_len(&["a",]), 0);
        assert_eq!(get_common_prefix_len(&["/a",]), 1);
        assert_eq!(get_common_prefix_len(&["/a/b"]), 3);
    }

    #[test]
    fn test_get_bin_name() {
        for (a, b) in [
            ("a", if cfg!(windows) { "a.exe" } else { "a" }),
            ("a.bat", "a.bat"),
            ("a.ps1", "a.ps1"),
            ("a.msi", "a.msi"),
        ] {
            let s = get_bin_name(a);
            assert_eq!(b, s)
        }
    }

    #[test]
    fn test_dirname() {
        for (a, b) in [("a", "a"), ("/a", "/"), ("/a/b", "/a/"), ("a/b/c", "a/b/")] {
            assert_eq!(dirname(a), b);
        }
    }
}
