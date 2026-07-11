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
    let local_strs: Vec<String> = local_target
        .iter()
        .map(|i| i.to_str().to_string())
        .collect();

    // Pass 1: exact target match
    for (key, art) in manfiest.artifacts.iter() {
        let filename = get_filename(key);
        if is_skip(&filename) {
            continue;
        }
        if ends_with_exe(key) && local_target.iter().any(|t| t.os() != Os::Windows) {
            continue;
        }
        if has_common_elements(&art.target_triples, &local_strs) {
            push_manifest_artifact(&mut v, url, key, art);
        }
    }

    // Pass 2: ABI fallback (msvc↔gnu, musl↔gnu) — only when pass 1 found nothing
    if v.is_empty() {
        for (key, art) in manfiest.artifacts.iter() {
            let filename = get_filename(key);
            if is_skip(&filename) {
                continue;
            }
            if ends_with_exe(key) && local_target.iter().any(|t| t.os() != Os::Windows) {
                continue;
            }
            let abi_match = local_target.iter().any(|local_t| {
                art.target_triples.iter().any(|art_t| {
                    guess_target::Target::from_str(art_t).is_ok_and(|art_target| {
                        art_target.arch() == local_t.arch()
                            && art_target.os() == local_t.os()
                            && is_compatible_abi(art_target.abi(), local_t.abi())
                    })
                })
            });
            if abi_match {
                push_manifest_artifact(&mut v, url, key, art);
            }
        }
    }

    v
}

fn push_manifest_artifact(
    v: &mut Vec<(String, String)>,
    url: &str,
    key: &str,
    art: &crate::manfiest::Artifact,
) {
    if let Some(kind) = &art.kind
        && !["executable-zip"].contains(&kind.as_str())
    {
        return;
    }
    let filename = get_filename(key);
    let name = name_no_ext(&filename);
    let name = guess_target(&name).pop().map_or(name, |i| i.name);
    if !is_url(key) {
        v.push((name, replace_filename(url, key)));
    } else {
        v.push((name, key.to_string()));
    }
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

/// Check whether `name` matches `stem` at a word boundary.
///
/// Returns true when `stem` starts with `name` followed by a separator
/// (`-`, `_`, `.`) or end-of-string. This prevents `--name qjs` from
/// accidentally matching `qjsc-linux-x86` while still matching
/// `qjs-linux-x86` and bare `qjs`.
fn name_boundary_match(stem: &str, name: &str) -> bool {
    stem == name
        || (stem.starts_with(name)
            && matches!(stem.as_bytes().get(name.len()), Some(b'-' | b'_' | b'.')))
}

/// Check whether two ABIs are binary-compatible enough to serve as a
/// fallback when the exact ABI is unavailable.
///
/// Fallback pairs:
/// - Windows: `msvc` ↔ `gnu`
/// - Linux:   `musl` ↔ `gnu`
fn is_compatible_abi(a: Option<Abi>, b: Option<Abi>) -> bool {
    matches!(
        (a, b),
        (Some(Abi::Msvc), Some(Abi::Gnu))
            | (Some(Abi::Gnu), Some(Abi::Msvc))
            | (Some(Abi::Musl), Some(Abi::Gnu))
            | (Some(Abi::Gnu), Some(Abi::Musl))
    )
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

    // When --regex is supplied, pre-compute the compiled regex and verify
    // it matches exactly one asset. The regex is matched against the
    // original filename (not the stem), and the matching asset is selected
    // directly — no guess_target, no target-triple inference.
    let regex_compiled: Option<regex::Regex> = match &config.regex {
        Some(re_str) => Some(regex::Regex::new(re_str).context("invalid --regex pattern")?),
        None => None,
    };
    if let Some(re) = &regex_compiled {
        let matched: Vec<String> = artifacts
            .assets
            .iter()
            .filter(|a| {
                let f = get_filename(&a.browser_download_url);
                re.is_match(&f)
            })
            .map(|a| a.name.clone())
            .collect();
        if matched.is_empty() {
            anyhow::bail!("--regex did not match any assets. Check the pattern and try again.");
        }
        if matched.len() > 1 {
            anyhow::bail!(
                "--regex matched {} assets, expected exactly 1. Pattern is too permissive.\n  Matched: {:#?}\n  Tighten the regex (e.g. anchor it to the platform triple) so only one asset remains.",
                matched.len(),
                matched
            );
        }
    }

    for i in artifacts.assets {
        let filename = get_filename(&i.browser_download_url);
        // When the download URL is an API endpoint (e.g. GitHub Actions
        // artifact download URL ending in .../zip), get_filename returns
        // a path segment like "zip" which is useless. Fall back to the
        // artifact's display name (which was set by the caller, e.g.
        // "ant-windows-x64.zip" for CI artifacts).
        let filename = if filename.contains('.') {
            filename
        } else {
            i.name.clone()
        };

        // --regex mode: match directly against the original filename.
        // When matched, the asset is selected immediately — no guess_target,
        // no target-triple matching. The regex is the sole authority.
        if let Some(re) = &regex_compiled {
            if re.is_match(&filename) {
                let rank = u32::MAX;
                let name = name_no_ext(&filename);
                v.push((rank, name, i.browser_download_url.clone()));
            }
            continue;
        }

        if is_skip(&i.browser_download_url) {
            continue;
        }
        if ends_with_exe(&i.browser_download_url)
            && local_target.iter().any(|t| t.os() != Os::Windows)
        {
            continue;
        }
        let name_no_ext_str = name_no_ext(&filename);
        let guess = guess_target(&name_no_ext_str);

        // --name filter: match against the guess_target-inferred tool name
        // when available (e.g. "lumen" from "lumen-x86_64-unknown-linux-gnu",
        // "lumen-cli" from "lumen-cli-x86_64-unknown-linux-gnu"). Falls back
        // to raw stem boundary-match when guess_target can't parse.
        if !config.name.is_empty() {
            let matched = if guess.is_empty() {
                config
                    .name
                    .iter()
                    .any(|n| name_boundary_match(&name_no_ext_str, n))
            } else {
                config
                    .name
                    .iter()
                    .any(|n| guess.iter().any(|g| g.name == n.as_str()))
            };
            if !matched {
                continue;
            }
        }

        // Match priority:
        // 1. Exact target match (including abi).
        // 2. ABI fallback: same (arch, os) with a compatible abi
        //    (msvc↔gnu on Windows, musl↔gnu on Linux). Always enabled.
        // 3. Fuzzy match by (arch, os), ignoring abi entirely (only when
        //    --fuzzy is set). Fuzzy matches have the same penalty as abi
        //    fallback, so exact matches win over both.
        const RANK_PENALTY: u32 = 1;
        let mut penalized = false;
        let item = if let Some(t) = config.target {
            let exact = guess.iter().find(|i| i.target == t);
            if let Some(m) = exact {
                Some(m)
            } else {
                // ABI fallback: msvc↔gnu, musl↔gnu
                let abi_fallback = guess.iter().find(|i| {
                    i.target.arch() == t.arch()
                        && i.target.os() == t.os()
                        && is_compatible_abi(i.target.abi(), t.abi())
                });
                if let Some(m) = abi_fallback {
                    penalized = true;
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
            }
        } else {
            let exact = guess.iter().find(|i| local_target.contains(&i.target));
            if let Some(m) = exact {
                Some(m)
            } else {
                // ABI fallback: msvc↔gnu, musl↔gnu
                let abi_fallback = guess.iter().find(|i| {
                    local_target.iter().any(|t| {
                        i.target.arch() == t.arch()
                            && i.target.os() == t.os()
                            && is_compatible_abi(i.target.abi(), t.abi())
                    })
                });
                if let Some(m) = abi_fallback {
                    penalized = true;
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
///
/// Note: `--name` filtering is applied earlier, inside `get_artifact_url`
/// (against the raw asset filename), so it does not need to be re-applied here.
pub(crate) fn filter_artifacts(
    artifact_url: Vec<(String, String)>,
    config: &InstallConfig,
) -> Vec<(String, String)> {
    if let Some(alias) = &config.alias {
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
    }
}

/// Print "not found" message. When `available` is `Some`, also prints
/// available artifacts sorted by relevance (same-arch first) with their
/// `guess_target` results.
pub(crate) fn not_found_asset_message(
    url: &str,
    config: &InstallConfig,
    available: Option<&[String]>,
) {
    if config.quiet {
        return;
    }

    let target_str = config
        .get_local_target()
        .iter()
        .map(|t| t.to_str().to_string())
        .collect::<Vec<_>>()
        .join(", ");

    let active_filters: Vec<String> = [
        (!config.name.is_empty()).then(|| format!("--name {}", config.name.join(","))),
        config.alias.as_ref().map(|a| format!("--alias {a}")),
        config.regex.as_ref().map(|r| format!("--regex {r}")),
    ]
    .into_iter()
    .flatten()
    .collect();
    let filter_hint = if active_filters.is_empty() {
        String::new()
    } else {
        format!(" (with {})", active_filters.join(", "))
    };

    println!("No {target_str} asset found in {url}{filter_hint}");

    let Some(names) = available else { return };
    if names.is_empty() {
        return;
    }

    let local_targets = config.get_local_target();

    // Build (stem, tool_name, target) rows.
    #[derive(Clone)]
    struct Row {
        stem: String,
        tool: String,
        target: String,
    }
    let mut rows: Vec<Row> = Vec::new();
    for name in names {
        let stem = name_no_ext(name);
        let guesses = guess_target(&stem);
        if guesses.is_empty() {
            rows.push(Row {
                stem: stem.clone(),
                tool: stem,
                target: String::new(),
            });
        } else {
            for g in &guesses {
                rows.push(Row {
                    stem: stem.clone(),
                    tool: g.name.clone(),
                    target: g.target.to_str().to_string(),
                });
            }
        }
    }

    // Sort: same-arch targets first, then by target, then by tool name.
    rows.sort_by(|a, b| {
        let a_same = !a.target.is_empty()
            && local_targets.iter().any(|lt| {
                guess_target::Target::from_str(&a.target).is_ok_and(|p| p.arch() == lt.arch())
            });
        let b_same = !b.target.is_empty()
            && local_targets.iter().any(|lt| {
                guess_target::Target::from_str(&b.target).is_ok_and(|p| p.arch() == lt.arch())
            });
        b_same
            .cmp(&a_same)
            .then_with(|| a.target.cmp(&b.target))
            .then_with(|| a.tool.cmp(&b.tool))
    });

    println!();
    let w_stem = rows.iter().map(|r| r.stem.len()).max().unwrap_or(6);
    let w_tool = rows.iter().map(|r| r.tool.len()).max().unwrap_or(4);
    let w_target = rows.iter().map(|r| r.target.len()).max().unwrap_or(6);
    println!(
        "  {:<w_stem$}  {:<w_tool$}  {:<w_target$}",
        "ORIGINAL", "NAME", "TARGET",
    );
    println!("  {:-<w_stem$}  {:-<w_tool$}  {:-<w_target$}", "", "", "",);
    for row in &rows {
        let target = if row.target.is_empty() {
            "(unknown)"
        } else {
            &row.target
        };
        println!(
            "  {:<w_stem$}  {:<w_tool$}  {:<w_target$}",
            row.stem, row.tool, target,
        );
    }
}

#[cfg(test)]
mod test {
    use anyhow::Context;

    use crate::{
        InstallConfig,
        download::download_dist_manfiest,
        tool::{
            dirname, get_artifact_url_from_manfiest, is_archive_file, is_compatible_abi,
            is_exe_file, is_url, name_boundary_match,
        },
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

    /// --regex matches directly against the original filename (including
    /// extension) and selects the matching asset bypassing guess_target.
    /// For complex filenames like
    /// `mpv-v0.41.0-dev-g4c220ffd9-28826186115-x86_64-pc-windows-msvc.zip`,
    /// the regex acts as the sole filter — no target-triple inference.
    #[tokio::test]
    async fn test_mpv_regex() {
        let repo =
            Repo::try_from("https://github.com/mpv-player/mpv/releases/tag/git-release").unwrap();
        // Match the specific x86_64 windows-msvc asset (non-pdb variant).
        // The regex is anchored at the end to avoid matching the -pdb.zip
        // companion, and includes the platform triple to narrow to one asset.
        let config = InstallConfig {
            regex: Some(r"mpv-v.+x86_64-pc-windows-msvc\.zip$".to_string()),
            ..Default::default()
        };
        let artifact_url = repo.get_artifact_url(&config).await.unwrap();
        println!("mpv artifact_url: {artifact_url:?}");
        assert_eq!(artifact_url.len(), 1);
        let (name, url) = &artifact_url[0];
        assert!(!name.is_empty());
        assert!(url.contains("mpv-v"));
        assert!(url.ends_with("x86_64-pc-windows-msvc.zip"));
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

    #[test]
    fn test_name_boundary_match() {
        // Exact match
        assert!(name_boundary_match("qjs", "qjs"));
        // Starts with name followed by separator
        assert!(name_boundary_match("qjs-linux-x86", "qjs"));
        assert!(name_boundary_match("qjs_linux-x86", "qjs"));
        assert!(name_boundary_match("qjs.linux-x86", "qjs"));
        // Should NOT match: next char is not a separator
        assert!(!name_boundary_match("qjsc-linux-x86", "qjs"));
        assert!(!name_boundary_match("qjsc", "qjs"));
        // Name longer than stem
        assert!(!name_boundary_match("qjs", "qjsc"));
    }

    #[test]
    fn test_is_compatible_abi() {
        use guess_target::Abi;
        // Windows: msvc ↔ gnu
        assert!(is_compatible_abi(Some(Abi::Msvc), Some(Abi::Gnu)));
        assert!(is_compatible_abi(Some(Abi::Gnu), Some(Abi::Msvc)));
        // Linux: musl ↔ gnu
        assert!(is_compatible_abi(Some(Abi::Musl), Some(Abi::Gnu)));
        assert!(is_compatible_abi(Some(Abi::Gnu), Some(Abi::Musl)));
        // NOT compatible
        assert!(!is_compatible_abi(Some(Abi::Msvc), Some(Abi::Musl)));
        assert!(!is_compatible_abi(Some(Abi::Musl), Some(Abi::Msvc)));
        assert!(!is_compatible_abi(Some(Abi::Gnu), Some(Abi::Gnu))); // same abi is exact match, not fallback
        assert!(!is_compatible_abi(Some(Abi::Msvc), Some(Abi::Msvc)));
        assert!(!is_compatible_abi(None, Some(Abi::Gnu)));
        assert!(!is_compatible_abi(Some(Abi::Gnu), None));
    }
}
