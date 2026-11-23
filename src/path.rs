//! File-name classification, path helpers and format detection.
//!
//! Everything in this module is pure (no I/O), so the download, selection and
//! install stages can all share one definition of "this is an archive", "this
//! is an executable" or "this is release metadata we must not install".

use easy_archive::{Fmt, types::IntoEnumIterator};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

/// Maximum path depth for warnings and auto-registered `$PATH` entries.
pub(crate) const DEEP: usize = 3;

/// Execute bits of a Unix permission mode.
pub(crate) const EXEC_MASK: u32 = 0o111;

/// Extensions that denote an executable on Windows.
pub(crate) const WINDOWS_EXE_EXTS: [&str; 6] = [".exe", ".ps1", ".bat", ".cmd", ".com", ".vbs"];

/// Package formats `ei` does not install (it never runs a platform installer).
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

/// Plain-text and metadata formats that are not installable executables.
pub(crate) const TEXT_FILE_EXTS: [&str; 19] = [
    ".txt", ".md", ".json", ".xml", ".csv", ".log", ".ini", ".cfg", ".conf", ".yaml", ".yml",
    ".rsa", ".pub", ".ed25519", ".jsonl", ".json5", ".md5", ".sha256", ".sha512",
];

/// Extensions that may be executable even without an execute bit.
pub(crate) const MAYBE_EXECUTABLE_EXTS: [&str; 13] = [
    ".out", ".sh", ".bash", ".zsh", ".py", ".pl", ".js", ".ts", ".jsx", ".tsx", ".wasm", ".fish",
    ".nu",
];

/// Disk images are installed as a single file: they are never unpacked, even
/// if an archive backend ever learns the format.
pub(crate) const DISK_IMAGE_EXTS: [&str; 2] = [".iso", ".img"];

/// File names that are checksum manifests rather than installable files.
///
/// Release pages commonly publish `sha256`, `SHA256SUMS` or `checksums` next to
/// the binaries. They carry no platform in their name and no file extension,
/// so without this they would be offered as `(unknown)` candidates.
const CHECKSUM_NAMES: [&str; 15] = [
    "md5",
    "md5sum",
    "md5sums",
    "sha1",
    "sha1sum",
    "sha1sums",
    "sha256",
    "sha256sum",
    "sha256sums",
    "sha512",
    "sha512sum",
    "sha512sums",
    "checksum",
    "checksums",
    "hashes",
];

/// Checksums, signatures, patches and archives of metadata: never installed.
/// Checksums, signatures and patches: release metadata that is never
/// installed.
///
/// Extensions already listed in [`INSTALLER_EXTS`] or [`TEXT_FILE_EXTS`] are
/// not repeated here; [`is_skip`] and [`is_known_format`] check all tables.
pub(crate) const SKIP_FMT_LIST: [&str; 7] = [
    ".sha256sum",
    ".sha1",
    ".sum",
    ".sig",
    ".asc",
    ".intoto.jsonl",
    ".bsdiff",
];

/// Whether `s` ends with one of the extensions `ei` knows how to handle as a
/// directly installable file (as opposed to an archive or a repository).
pub(crate) fn is_known_format(s: &str) -> bool {
    [
        &WINDOWS_EXE_EXTS[..],
        &INSTALLER_EXTS[..],
        &TEXT_FILE_EXTS[..],
        &MAYBE_EXECUTABLE_EXTS[..],
        &SKIP_FMT_LIST[..],
        &DISK_IMAGE_EXTS[..],
    ]
    .iter()
    .any(|exts| exts.iter().any(|ext| s.ends_with(ext)))
}

/// Whether `name` looks like a checksum manifest (`sha256`, `SHA256SUMS.txt`,
/// `checksums`, …).
pub(crate) fn is_checksum_file(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let stem = lower.split('.').next().unwrap_or_default();
    CHECKSUM_NAMES.contains(&stem)
}

const LICENSE_PREFIXES: &[&str] = &["license", "licence", "copying", "unlicense", "copyright"];
const LICENSE_EXTS: &[&str] = &["", "md", "txt", "rst"];

/// Whether `p` looks like a license/notice file.
pub(crate) fn is_license_file(p: &str) -> bool {
    let name = get_filename(p).to_lowercase();
    let (stem, ext) = match name.rsplit_once('.') {
        Some((stem, ext)) => (stem, ext),
        None => (name.as_str(), ""),
    };
    LICENSE_EXTS.contains(&ext)
        && LICENSE_PREFIXES
            .iter()
            .any(|prefix| stem.starts_with(prefix))
}

/// Whether `s` is a hidden file or release metadata that must not be installed.
pub(crate) fn is_skip(s: &str) -> bool {
    let name = get_filename(s);
    if name.starts_with('.') || is_checksum_file(&name) {
        return true;
    }
    let lower = s.to_ascii_lowercase();
    INSTALLER_EXTS
        .iter()
        .chain(TEXT_FILE_EXTS.iter())
        .chain(SKIP_FMT_LIST.iter())
        .any(|ext| lower.ends_with(ext))
}

/// Installed file name for `s`.
///
/// `.exe` is appended for bare names only when the *install target* is
/// Windows. The decision cannot use `cfg!(windows)`: a Windows build of `ei`
/// routinely installs onto a Linux host over SSH, and the compiled-in platform
/// says nothing about where the file lands.
pub(crate) fn get_bin_name(s: &str, windows: bool) -> String {
    if windows && !WINDOWS_EXE_EXTS.iter().any(|i| s.ends_with(i)) && !s.contains(".") {
        return s.to_string() + ".exe";
    }
    s.to_string()
}

fn abs_path(p: &str) -> PathBuf {
    std::path::absolute(p).unwrap_or(p.into())
}

/// Directory part of `s`, including the trailing separator (`""` when there is
/// none).
pub(crate) fn dirname(s: &str) -> String {
    match s.rfind('/') {
        Some(i) => s[..i + 1].to_string(),
        None => String::new(),
    }
}

/// Whether `name` is executable, given its optional Unix mode.
pub(crate) fn executable(name: &str, mode: &Option<u32>) -> bool {
    ends_with_exe(name) || (!name.contains('.') && mode.unwrap_or(0) & EXEC_MASK != 0)
}

/// Last path segment of `s`, treating both `/` and `\` as separators.
pub(crate) fn get_filename(s: &str) -> String {
    let s = s.replace("\\\\", "/");
    let s = s.replace("\\", "/");
    let i = s.rfind('/').map_or(0, |i| i + 1);
    s[i..].to_string()
}

/// Whether any execute bit is set in `mode`.
pub(crate) fn is_executable(mode: u32) -> bool {
    mode & EXEC_MASK != 0
}

/// Whether `name` has an extension that suggests it is executable.
pub(crate) fn maybe_executable(name: &str) -> bool {
    MAYBE_EXECUTABLE_EXTS.iter().any(|ext| name.ends_with(ext))
}

/// Strip a known file extension (including multi-part ones such as
/// `.tar.gz`) from `s`.
pub(crate) fn name_no_ext(s: &str) -> String {
    for ext in known_extensions().iter() {
        if s.ends_with(ext.as_str()) {
            return s[0..s.len() - ext.len()].to_string();
        }
    }
    s.to_string()
}

/// Every extension `ei` recognises, longest first so `.tar.gz` wins over `.gz`.
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

/// Whether `s` points at an archive format `ei` can decode.
pub(crate) fn is_archive_file(s: &str) -> bool {
    !is_disk_image(s) && Fmt::guess(s).is_some()
}

/// Whether `s` is a disk image, installed as a single file rather than
/// unpacked.
pub(crate) fn is_disk_image(s: &str) -> bool {
    let name = get_filename(s).to_ascii_lowercase();
    DISK_IMAGE_EXTS.iter().any(|ext| name.ends_with(ext))
}

/// Whether `s` ends with a Windows executable extension.
pub(crate) fn ends_with_exe(s: &str) -> bool {
    WINDOWS_EXE_EXTS.iter().any(|ext| s.ends_with(ext))
}

/// Whether a GitHub download URL names a single executable rather than an
/// archive. URLs without a dot in the file name (e.g. `.../linux-x64`) are
/// treated as executables.
pub(crate) fn is_exe_file(s: &str) -> bool {
    if ends_with_exe(s) {
        return true;
    }
    static RE_LATEST: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r"^https://github\.com/([^/]+)/([^/]+)/releases/latest/download/([^/]+)$")
            .unwrap()
    });
    static RE_TAG: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(
            r"^https://github\.com/([^/]+)/([^/]+)/releases/download/([^/]+)/([^/]+)$",
        )
        .unwrap()
    });
    static RE_TAG2: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(
            r"^https://github\.com/([^/]+)/([^/]+)/releases/download/([^/]+)/([^/]+)/([^/]+)$",
        )
        .unwrap()
    });
    for (re, n) in [(&*RE_TAG2, 5), (&*RE_TAG, 4), (&*RE_LATEST, 3)] {
        if let Some(cap) = re.captures(s)
            && let Some(name) = cap.get(n)
        {
            let name = name.as_str();
            if is_archive_file(name) {
                return false;
            }
            if !name.contains('.') {
                return true;
            }
        }
    }
    false
}

/// Whether `s` names a dist-manifest JSON document.
pub(crate) fn is_dist_manifest(s: &str) -> bool {
    s.ends_with(".json")
}

/// `/`-separated string form of `p`.
pub(crate) fn path_to_str(p: &Path) -> String {
    p.to_str().unwrap().replace("\\", "/")
}

/// Replace the file name of `base_url` with `name`.
pub(crate) fn replace_filename(base_url: &str, name: &str) -> String {
    match base_url.rfind('/') {
        Some(pos) => format!("{}{}", &base_url[..pos + 1], name),
        None => name.to_string(),
    }
}

/// Join two path segments with a single `/`.
pub(crate) fn join_path(dir: &str, name: &str) -> String {
    let dir = dir.trim_end_matches(['/', '\\']);
    if name.is_empty() {
        dir.to_string()
    } else if dir.is_empty() {
        name.to_string()
    } else {
        format!("{dir}/{name}")
    }
}

/// Expand `~` and turn `path` into an absolute, cleaned path.
pub(crate) fn expand_path(path: &str) -> String {
    if path.starts_with('~') {
        let expanded = shellexpand::tilde(path);
        path_clean::PathClean::clean(Path::new(&*expanded))
            .to_string_lossy()
            .to_string()
    } else {
        abs_path(path).to_string_lossy().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_bin_name() {
        // Bare names get `.exe` on a Windows target only.
        assert_eq!(get_bin_name("a", true), "a.exe");
        assert_eq!(get_bin_name("a", false), "a");

        // Existing extensions are never rewritten.
        for name in ["a.bat", "a.ps1", "a.msi", "a.exe", "tool-1.0"] {
            assert_eq!(get_bin_name(name, true), name);
            assert_eq!(get_bin_name(name, false), name);
        }
    }

    #[test]
    fn test_dirname() {
        for (a, b) in [("a", ""), ("/a", "/"), ("/a/b", "/a/"), ("a/b/c", "a/b/")] {
            assert_eq!(dirname(a), b);
        }
    }

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
    fn test_is_exe_file() {
        for (url, expected) in [
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
            assert_eq!(is_exe_file(url), expected, "{url}");
        }
    }

    #[test]
    fn test_is_skip() {
        assert!(is_skip(".hidden"));
        assert!(is_skip("checksums.sha256"));
        assert!(is_skip("notes.txt"));
        assert!(!is_skip("tool"));
        assert!(!is_skip("tool.exe"));
    }

    /// The extension tables are disjoint: a duplicate would drift the first
    /// time only one of them is updated.
    #[test]
    fn extension_tables_do_not_overlap() {
        let mut seen = std::collections::HashSet::new();
        for ext in WINDOWS_EXE_EXTS
            .iter()
            .chain(&INSTALLER_EXTS)
            .chain(&TEXT_FILE_EXTS)
            .chain(&MAYBE_EXECUTABLE_EXTS)
            .chain(&SKIP_FMT_LIST)
        {
            assert!(seen.insert(*ext), "{ext} is listed twice");
        }
    }

    /// Checksum manifests are published next to the binaries, often without an
    /// extension (Ventoy ships a bare `sha256`).
    #[test]
    fn checksum_manifests_are_skipped() {
        for name in [
            "sha256",
            "SHA256SUMS",
            "sha256sums.txt",
            "checksums",
            "md5sum",
            "https://github.com/ventoy/Ventoy/releases/download/v1.1.18/sha256",
        ] {
            assert!(is_skip(name), "{name} must be skipped");
        }

        assert!(!is_skip("sha256sum-tool"));
        assert!(!is_skip("tool"));
    }

    #[test]
    fn disk_images_are_single_files() {
        assert!(!is_archive_file("ventoy-1.1.18-livecd.iso"));
        assert!(!is_archive_file("https://example.com/disk.img"));
        assert!(is_known_format("ventoy-1.1.18-livecd.iso"));
        assert_eq!(
            name_no_ext("ventoy-1.1.18-livecd.iso"),
            "ventoy-1.1.18-livecd.iso"
        );
        assert!(is_archive_file("tool.tar.gz"));
        assert!(is_archive_file("tool.zip"));
    }

    #[test]
    fn test_name_no_ext() {
        assert_eq!(name_no_ext("tool-linux.tar.gz"), "tool-linux");
        assert_eq!(name_no_ext("tool.zip"), "tool");
        assert_eq!(name_no_ext("tool"), "tool");
    }

    #[test]
    fn test_join_path() {
        assert_eq!(join_path("/a/", "b"), "/a/b");
        assert_eq!(join_path("/a", "b"), "/a/b");
        assert_eq!(join_path("", "b"), "b");
        assert_eq!(join_path("/a/", ""), "/a");
    }
}
