use crate::env::add_to_path;
use crate::manfiest::DistManifest;
use crate::ty::{Output, OutputFile};
use easy_archive::{Fmt, IntoEnumIterator};
use easy_archive::{human_size, mode_to_string};
use guess_target::{Os, get_local_target, guess_target};
use regex::Regex;
use std::collections::HashSet;
#[cfg(unix)]
use std::os::unix::prelude::PermissionsExt;
use std::path::Path;
use std::str::FromStr;

const DEEP: usize = 3;
const WINDOWS_EXE_EXTS: [&str; 6] = [".exe", ".ps1", ".bat", ".cmd", ".com", ".vbs"];
const INSTALLER_EXTS: [&str; 11] = [
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
const TEXT_FILE_EXTS: [&str; 11] = [
    ".txt", ".md", ".json", ".xml", ".csv", ".log", ".ini", ".cfg", ".conf", ".yaml", ".yml",
];

const SKIP_FMT_LIST: [&str; 16] = [
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
];

pub fn is_skip(s: &str) -> bool {
    s.rsplit('/').next().unwrap_or_default().starts_with('.')
        || INSTALLER_EXTS
            .iter()
            .chain(TEXT_FILE_EXTS.iter())
            .chain(SKIP_FMT_LIST.iter())
            .any(|&ext| s.to_ascii_lowercase().ends_with(&ext.to_ascii_lowercase()))
}

pub fn get_bin_name(s: &str) -> String {
    if cfg!(windows) && !WINDOWS_EXE_EXTS.iter().any(|i| s.ends_with(i)) && !s.contains(".") {
        return s.to_string() + ".exe";
    }
    s.to_string()
}

const MAX_FILE_COUNT: usize = 16;
pub fn display_output(output: &Output) -> String {
    let mut v = vec![];
    for i in output.values() {
        if i.files.len() > MAX_FILE_COUNT {
            let sum_size = i.files.iter().fold(0, |pre, cur| pre + cur.size);
            v.push(
                [
                    human_size(sum_size as usize).as_str(),
                    format!("(total {})", i.files.len()).as_str(),
                    i.install_dir.as_str(),
                ]
                .join(" "),
            );
        } else {
            let max_size_len = i
                .files
                .iter()
                .fold(0, |pre, cur| pre.max(human_size(cur.size as usize).len()));

            for k in &i.files {
                let s = human_size(k.size as usize);
                v.push(
                    [
                        mode_to_string(k.mode.unwrap_or(0), k.is_dir),
                        " ".repeat(max_size_len - s.len()) + &s,
                        [k.origin_path.as_str(), k.install_path.as_str()].join(" -> "),
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

pub fn add_output_to_path(output: &Output) {
    let mut maybe_exe = HashSet::new();
    for v in output.values() {
        for f in &v.files {
            if !is_skip(&f.install_path) {
                maybe_exe.insert(f.install_path.clone());
            }
            let deep = f.origin_path.split("/").count();
            if deep <= DEEP {
                if let Some(p) = check(f) {
                    if p != f.install_path {
                        println!("Warning: file exists at {p}");
                    }
                }
            }
        }
    }

    let mut filter = HashSet::new();
    for v in output.values() {
        add_to_path(&v.install_dir);

        for f in &v.files {
            let deep = f.origin_path.split("/").count();
            let is_exe = (maybe_exe.len() == 1 && maybe_exe.contains(&f.install_path))
                || ends_with_exe(&f.origin_path)
                || (f.mode.unwrap_or(0) & 0o111 != 0);
            let dir = dirname(&f.install_path);
            if deep <= DEEP && is_exe && !filter.contains(&dir) {
                add_to_path(&dir);
                filter.insert(dir);
            }
        }
    }
}

pub fn get_filename(s: &str) -> String {
    let i = s.rfind("/").map_or(0, |i| i + 1);
    s[i..].to_string()
}

#[cfg(windows)]
pub fn which(name: &str) -> Option<String> {
    let cmd = std::process::Command::new("powershell")
        .args(["-c", &format!("(get-command {name}).Source")])
        .output()
        .ok()?;
    String::from_utf8(cmd.stdout)
        .ok()
        .map(|i| i.trim().replace("\\", "/").replace("//", "/"))
}

#[cfg(unix)]
pub fn which(name: &str) -> Option<String> {
    let cmd = std::process::Command::new("which")
        .arg(name)
        .output()
        .ok()?;
    String::from_utf8(cmd.stdout)
        .ok()
        .map(|i| i.trim().to_string().replace("\\", "/").replace("//", "/"))
}

const EXEC_MASK: u32 = 0o111;
pub fn executable(name: &str, mode: &Option<u32>) -> bool {
    ends_with_exe(name) || (!name.contains(".") && mode.unwrap_or(0) & EXEC_MASK != 0)
}

pub fn check(file: &OutputFile) -> Option<String> {
    let file_path = &file.install_path;
    let name = get_filename(file_path);
    if !executable(&name, &file.mode) {
        return None;
    }
    if let Some(p) = which(&name) {
        if !p.is_empty() && file_path != &p {
            return Some(p);
        }
    }
    None
}

pub fn write_to_file(src: &str, buffer: &[u8], mode: &Option<u32>) {
    let Ok(d) = std::path::PathBuf::from_str(src);
    if let Some(p) = d.parent() {
        if !std::fs::exists(p).unwrap_or(false) {
            std::fs::create_dir_all(p).expect("failed to create_dir_all");
        }
    }

    if std::fs::exists(src).unwrap_or(false) {
        if let Ok(meta) = std::fs::metadata(src) {
            if meta.is_file() {
                std::fs::remove_file(src).expect("failed to remove file");
            } else {
                std::fs::remove_dir_all(src).expect("failed to remove dir");
            }
        }
    }

    std::fs::write(src, buffer).expect("failed to write file");

    #[cfg(unix)]
    if let Some(mode) = mode {
        if *mode > 0 {
            std::fs::set_permissions(src, PermissionsExt::from_mode(*mode))
                .expect("failed to set_permissions");
        }
    }

    #[cfg(windows)]
    {
        _ = mode;
    }
}

fn has_common_elements(arr1: &[String], arr2: &[String]) -> bool {
    arr1.iter().any(|x| arr2.contains(x))
}

pub fn get_artifact_url_from_manfiest(url: &str, manfiest: &DistManifest) -> Vec<(String, String)> {
    let mut v = vec![];
    // let mut filter = vec![];
    let local_target = get_local_target();

    for (key, art) in manfiest.artifacts.iter() {
        let filename = get_filename(key);
        if is_skip(&filename) {
            continue;
        }

        if ends_with_exe(key) && local_target.iter().any(|t| t.os() != Os::Windows) {
            continue;
        }

        // let guess = guess_target(&filename);

        // if let Some(item) = guess.iter().find(|i| local_target.contains(&i.target)) {
        //     if filter.contains(&item.name) {
        //         continue;
        //     }
        //     if !is_url(key) {
        //         v.push((item.rank, item.name.clone(), replace_filename(url, key)));
        //     } else {
        //         v.push((item.rank, item.name.clone(), key.clone()));
        //     }
        //     filter.push(item.name.clone());
        //     continue;
        // }

        if has_common_elements(
            &art.target_triples,
            local_target
                .iter()
                .map(|i| i.to_str().to_string())
                .collect::<Vec<_>>()
                .as_slice(),
        ) {
            if let Some(kind) = &art.kind {
                if !["executable-zip"].contains(&kind.as_str()) {
                    continue;
                }
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
    // let max_rank = v.iter().fold(0, |pre, cur| pre.max(cur.0));
    // v.into_iter()
    //     .filter_map(|i| {
    //         if i.0 < max_rank {
    //             None
    //         } else {
    //             Some((i.1, i.2))
    //         }
    //     })
    //     .collect()
    v
}

pub fn get_common_prefix_len(list: &[&str]) -> usize {
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

pub fn install_output_files(files: &Vec<OutputFile>) {
    for OutputFile {
        install_path,
        buffer,
        mode,
        origin_path,
        ..
    } in files
    {
        // FIXME: skip __MACOSX
        if origin_path.starts_with("__MACOSX") {
            continue;
        }
        write_to_file(install_path, buffer, mode);
    }

    #[cfg(not(windows))]
    {
        let maybe_exe = files
            .iter()
            .filter(|i| !is_skip(&i.origin_path))
            .collect::<Vec<_>>();

        if let [single_exe] = maybe_exe.as_slice() {
            crate::tool::add_execute_permission(&single_exe.install_path)
                .expect("failed to add_execute_permission");
        }
    }
}

pub fn name_no_ext(s: &str) -> String {
    let mut exts: Vec<_> = Fmt::iter().flat_map(|i| i.extensions().to_vec()).collect();
    exts.sort_by_key(|b| std::cmp::Reverse(b.len()));
    for ext in exts {
        if s.ends_with(ext) {
            return s[0..s.len() - ext.len()].to_string();
        }
    }
    let i = s.find(".").unwrap_or(s.len());
    s[0..i].to_string()
}

#[cfg(not(windows))]
pub fn add_execute_permission(file_path: &str) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let metadata = std::fs::metadata(file_path)?;
    if metadata.is_dir() {
        return Ok(());
    }

    let mut permissions = metadata.permissions();
    let current_mode = permissions.mode();

    let new_mode = current_mode | 0o111;
    permissions.set_mode(new_mode);

    std::fs::set_permissions(file_path, permissions)?;

    Ok(())
}

pub fn is_archive_file(s: &str) -> bool {
    Fmt::guess(s).is_some()
}

pub fn ends_with_exe(s: &str) -> bool {
    WINDOWS_EXE_EXTS.iter().any(|i| s.ends_with(i))
}
pub fn is_exe_file(s: &str) -> bool {
    if ends_with_exe(s) {
        return true;
    }
    let re_latest =
        Regex::new(r"^https://github\.com/([^/]+)/([^/]+)/releases/latest/download/([^/]+)$")
            .expect("failed to build github latest release regex");
    let re_tag =
        Regex::new(r"^https://github\.com/([^/]+)/([^/]+)/releases/download/([^/]+)/([^/]+)$")
            .expect("failed to build github release regex");
    let re_tag2 = Regex::new(
        r"^https://github\.com/([^/]+)/([^/]+)/releases/download/([^/]+)/([^/]+)/([^/]+)$",
    )
    .expect("failed to build github release regex");
    for (re, n) in [(re_tag2, 5), (re_tag, 4), (re_latest, 3)] {
        if let Some(cap) = re.captures(s) {
            if let Some(name) = cap.get(n) {
                if is_archive_file(name.as_str()) {
                    return false;
                }
                if !name.as_str().contains(".") {
                    return true;
                }
            }
        }
    }

    false
}

pub fn is_url(s: &str) -> bool {
    s.starts_with("http://") || s.starts_with("https://")
}

pub fn is_dist_manfiest(s: &str) -> bool {
    s.ends_with(".json")
}

pub fn path_to_str(p: &Path) -> String {
    p.to_str().unwrap().replace("\\", "/")
}

pub fn replace_filename(base_url: &str, name: &str) -> String {
    if let Some(pos) = base_url.rfind('/') {
        format!("{}{}", &base_url[..pos + 1], name)
    } else {
        name.to_string()
    }
}

#[cfg(test)]
mod test {
    use crate::{
        download::download_dist_manfiest,
        tool::{dirname, get_artifact_url_from_manfiest, is_archive_file, is_exe_file, is_url},
        ty::Repo,
    };

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
            Repo::try_from("https://github.com/ahaoboy/ansi2").unwrap(),
            repo
        );

        assert!(
            Repo::try_from("https://api.github.com/repos/ahaoboy/ansi2/releases/latest").is_err()
        );
        assert_eq!(Repo::try_from("ahaoboy/ansi2").unwrap(), repo);
        let repo = Repo {
            owner: "ahaoboy".to_string(),
            name: "ansi2".to_string(),
            tag: Some("v0.2.11".to_string()),
        };
        assert_eq!(Repo::try_from("ahaoboy/ansi2@v0.2.11").unwrap(), repo);
        assert_eq!(
            Repo::try_from("https://github.com/ahaoboy/ansi2/releases/tag/v0.2.11").unwrap(),
            repo
        );

        assert_eq!(
          Repo::try_from("https://github.com/ahaoboy/ansi2/releases/download/v0.2.11/ansi2-x86_64-unknown-linux-musl.tar.gz").unwrap(),
          repo
        );

        assert_eq!(
          Repo::try_from("https://github.com/ahaoboy/ansi2/releases/download/v0.2.11/ansi2-x86_64-pc-windows-msvc.zip").unwrap(),
          repo
        );

        let repo = Repo {
            owner: "Ryubing".to_string(),
            name: "Ryujinx".to_string(),
            tag: Some("1.2.78".to_string()),
        };
        assert_eq!(
          Repo::try_from("https://github.com/Ryubing/Ryujinx/releases/download/1.2.78/ryujinx-*.*.*-win_x64.zip").unwrap(),
          repo
        );
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
        )
    }
    #[tokio::test]
    async fn test_get_manfiest() {
        let repo = Repo::try_from("https://github.com/axodotdev/cargo-dist/releases").unwrap();
        let url = repo.get_manfiest_url();
        assert_eq!(
            url,
            "https://github.com/axodotdev/cargo-dist/releases/latest/download/dist-manifest.json"
        );
        assert!(repo.get_manfiest().await.is_some());

        let repo =
            Repo::try_from("https://github.com/axodotdev/cargo-dist/releases/tag/v0.25.1").unwrap();
        let url = repo.get_manfiest_url();
        assert_eq!(
            url,
            "https://github.com/axodotdev/cargo-dist/releases/download/v0.25.1/dist-manifest.json"
        );

        let manfiest = repo.get_manfiest().await.unwrap();
        assert!(!manfiest.artifacts.is_empty());

        let repo =
            Repo::try_from("https://github.com/ahaoboy/mujs-build/releases/tag/v0.0.2").unwrap();
        let url = repo.get_manfiest_url();
        assert_eq!(
            url,
            "https://github.com/ahaoboy/mujs-build/releases/download/v0.0.2/dist-manifest.json"
        );

        let manfiest = repo.get_manfiest().await.unwrap();
        assert!(!manfiest.artifacts.is_empty())
    }

    #[tokio::test]
    async fn test_install_from_manfiest() {
        let url =
            "https://github.com/ahaoboy/mujs-build/releases/latest/download/dist-manifest.json";
        let manfiest = download_dist_manfiest(url).await.unwrap();
        let art_url = get_artifact_url_from_manfiest(url, &manfiest);
        assert!(!art_url.is_empty())
    }

    #[tokio::test]
    async fn test_cargo_dist() {
        let url = "https://github.com/axodotdev/cargo-dist/releases/download/v1.0.0-rc.1/dist-manifest.json";
        let manfiest = download_dist_manfiest(url).await.unwrap();
        let art_url = get_artifact_url_from_manfiest(url, &manfiest);
        assert!(!art_url.is_empty())
    }

    #[tokio::test]
    async fn test_deno() {
        let url = "https://github.com/denoland/deno";
        let repo = Repo::try_from(url).unwrap();
        let artifact_url = repo.get_artifact_url().await;
        println!("artifact_url{artifact_url:?}");
        assert_eq!(artifact_url.len(), 2);
    }

    #[tokio::test]
    async fn test_starship() {
        let repo = Repo::try_from("https://github.com/starship/starship").unwrap();
        let artifact_url = repo.get_artifact_url().await;
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
            assert_eq!(is_exe_file(a), b);
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
