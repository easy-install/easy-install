#[cfg(windows)]
use std::os::windows::fs::MetadataExt;

use easy_archive::tool::{human_size, mode_to_string};
use easy_archive::ty::Fmt;
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
use std::path::Path;

#[cfg(unix)]
use std::os::unix::prelude::PermissionsExt;

use crate::env::add_to_path;
use crate::manfiest::DistManifest;
use crate::ty::{Output, OutputFile, Repo};
use detect_targets::detect_targets;
use regex::Regex;
use std::str::FromStr;

pub fn get_bin_name(s: &str) -> String {
    if cfg!(windows) && !WINDOWS_EXE_EXTS.iter().any(|i| s.ends_with(i)) && !s.contains(".") {
        return s.to_string() + ".exe";
    }
    s.to_string()
}

pub fn get_meta<P: AsRef<Path>>(s: P) -> (u32, u32, bool) {
    let mut mode = 0;
    let mut size = 0;
    let mut is_dir = false;
    if let Ok(meta) = std::fs::metadata(s) {
        #[cfg(windows)]
        {
            mode = 0;
            size = meta.file_size() as u32;
        }

        #[cfg(unix)]
        {
            mode = meta.mode();
            size = meta.size() as u32;
        }

        is_dir = meta.is_dir()
    }

    (mode, size, is_dir)
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

const DEEP: usize = 3;
const WINDOWS_EXE_EXTS: [&str; 3] = [".exe", ".ps1", ".bat"];

fn dirname(s: &str) -> String {
    let i = s.rfind('/').map_or(s.len(), |i| i + 1);
    s[0..i].to_string()
}

pub fn add_output_to_path(output: &Output) {
    for v in output.values() {
        for f in &v.files {
            let deep = f.origin_path.split("/").count();
            if deep <= DEEP && check(f) {
                println!("Warning: file exists at {}", f.install_path);
            }
        }
    }
    for v in output.values() {
        add_to_path(&v.install_dir);

        for f in &v.files {
            let deep = f.origin_path.split("/").count();
            if deep <= DEEP
                && WINDOWS_EXE_EXTS
                    .iter()
                    .any(|i| f.origin_path.ends_with(i) || (f.mode.unwrap_or(0) & 0o111 != 0))
            {
                let dir = dirname(&f.install_path);
                add_to_path(&dir);
            }
        }

        #[cfg(unix)]
        if v.files.len() == 1 {
            let i = &v.files[0];
            crate::tool::add_execute_permission(&i.install_path)
                .expect("failed to add_execute_permission");
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
        .map(|i| i.trim().replace("\\", "/"))
}

#[cfg(unix)]
pub fn which(name: &str) -> Option<String> {
    let cmd = std::process::Command::new("which")
        .arg(name)
        .output()
        .ok()?;
    String::from_utf8(cmd.stdout)
        .ok()
        .map(|i| i.trim().to_string())
}

const EXEC_MASK: u32 = 0o111;
pub fn executable(name: &str, mode: &Option<u32>) -> bool {
    name.ends_with(".exe") || (!name.contains(".") && mode.unwrap_or(0) & EXEC_MASK != 0)
}

pub fn check(file: &OutputFile) -> bool {
    let file_path = &file.install_path;
    let name = get_filename(file_path);
    if !executable(&name, &file.mode) {
        return false;
    }
    if let Some(p) = which(&name) {
        if !p.is_empty() && file_path != &p {
            return true;
        }
    }
    false
}

// pub fn atomic_install(src: &Path, dst: &Path) -> std::io::Result<u64> {
//     std::fs::copy(src, dst)
// }

pub fn write_to_file(src: &str, buffer: &[u8], mode: &Option<u32>) {
    let Ok(d) = std::path::PathBuf::from_str(src);
    if let Some(p) = d.parent() {
        std::fs::create_dir_all(p).expect("failed to create_dir_all");
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

pub async fn get_artifact_url_from_manfiest(url: &str, manfiest: &DistManifest) -> Vec<String> {
    let targets = detect_targets().await;
    let mut v = vec![];
    for (name, art) in manfiest.artifacts.iter() {
        if art.match_targets(&targets)
          // && is_archive_file(name)
          && art.kind.clone().unwrap_or("executable-zip".to_owned()) == "executable-zip"
        {
            if !is_url(name) {
                v.push(replace_filename(url, name));
            } else {
                v.push(name.clone());
            }
        }
    }
    v
}

// pub fn remove_postfix(s: &str) -> String {
//     use Fmt::*;
//     for i in [Tar, TarBz, TarGz, TarXz, TarZstd, Zip] {
//         for ext in i.extensions() {
//             if !ext.is_empty() && s.ends_with(&ext) {
//                 return s[0..s.len() - ext.len()].to_string();
//             }
//         }
//     }
//     s.to_string()
// }

pub fn get_common_prefix_len(list: &[&str]) -> usize {
    if list.is_empty() {
        return 0;
    }

    if list.len() == 1 {
        match list[0].rfind('/') {
            Some(i) => return i+1,
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
        ..
    } in files
    {
        write_to_file(install_path, buffer, mode);
    }
}

pub fn name_no_ext(s: &str) -> String {
    let i = s.find(".").unwrap_or(s.len());
    s[0..i].to_string()
}

#[cfg(unix)]
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

pub fn is_exe_file(s: &str) -> bool {
    if WINDOWS_EXE_EXTS.iter().any(|i| s.ends_with(i)) {
        return true;
    }
    let re_latest =
        Regex::new(r"^https://github\.com/([^/]+)/([^/]+)/releases/latest/download/([^/]+)$")
            .expect("failed to build github latest release regex");
    let re_tag =
        Regex::new(r"^https://github\.com/([^/]+)/([^/]+)/releases/download/([^/]+)/([^/]+)$")
            .expect("failed to build github release regex");

    for (re, n) in [(re_latest, 3), (re_tag, 4)] {
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

pub fn is_hash_file(s: &str) -> bool {
    s.ends_with(".sha256")
}

pub fn is_msi_file(s: &str) -> bool {
    s.ends_with(".msi")
}

pub async fn get_artifact_download_url(art_url: &str) -> Vec<String> {
    if !art_url.contains("*") {
        return vec![art_url.to_string()];
    }

    if let Ok(repo) = Repo::try_from(art_url) {
        return repo.match_artifact_url(art_url).await;
    }
    vec![]
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
        download::{download_dist_manfiest, read_dist_manfiest},
        tool::{
            dirname, get_artifact_download_url, get_artifact_url_from_manfiest, is_archive_file,
            is_exe_file, is_url,
        },
        ty::Repo,
    };
    use detect_targets::detect_targets;

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
        assert!(is_archive_file("https://github.com/ahaoboy/ansi2/releases/download/v0.2.11/ansi2-x86_64-unknown-linux-musl.tar.gz"));
        assert!(is_archive_file("https://github.com/ahaoboy/ansi2/releases/download/v0.2.11/ansi2-x86_64-pc-windows-msvc.zip"));
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

        let repo = Repo {
            owner: "ahaoboy".to_string(),
            name: "ansi2".to_string(),
            tag: Some("v0.2.11".to_string()),
        };

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

    // #[tokio::test]
    // async fn test_get_artifact_url() {
    //     let repo = Repo::try_from("https://github.com/ahaoboy/mujs-build").unwrap();
    //     let url = repo.get_artifact_url(detect_targets().await).await[0].clone();
    //     let files = download_extract(&url).await.unwrap();
    //     assert!(files
    //         .iter()
    //         .find(|i| i.path == if IS_WINDOWS { "mujs.exe" } else { "mujs" })
    //         .is_some());
    // }

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

    // #[tokio::test]
    // async fn test_manifest_jsc() {
    //     let repo = Repo {
    //         owner: "ahaoboy".to_string(),
    //         name: "jsc-build".to_string(),
    //         tag: None,
    //     };

    //     let manifest = repo.get_manfiest().await.unwrap();
    //     let art = manifest
    //         .get_artifact(&vec!["x86_64-unknown-linux-gnu".to_string()])
    //         .unwrap();

    //     assert!(art.has_file("bin/jsc"));
    //     assert!(art.has_file("lib/libJavaScriptCore.a"));
    //     assert!(!art.has_file("lib/jsc"));
    // }

    // #[tokio::test]
    // async fn test_manifest_mujs() {
    //     let repo = Repo {
    //         owner: "ahaoboy".to_string(),
    //         name: "mujs-build".to_string(),
    //         tag: None,
    //     };

    //     let manifest = repo.get_manfiest().await.unwrap();
    //     let art = manifest
    //         .get_artifact(&vec!["x86_64-unknown-linux-gnu".to_string()])
    //         .unwrap();

    //     assert!(art.has_file("mujs"));
    //     assert!(!art.has_file("mujs.exe"));

    //     let manifest = repo.get_manfiest().await.unwrap();
    //     let art = manifest
    //         .get_artifact(&vec!["x86_64-pc-windows-gnu".to_string()])
    //         .unwrap();

    //     assert!(!art.has_file("mujs"));
    //     assert!(art.has_file("mujs.exe"));
    // }

    #[tokio::test]
    async fn test_install_from_manfiest() {
        let url =
            "https://github.com/ahaoboy/mujs-build/releases/latest/download/dist-manifest.json";
        let manfiest = download_dist_manfiest(url).await.unwrap();
        let art_url = get_artifact_url_from_manfiest(url, &manfiest).await;
        assert!(!art_url.is_empty())
    }

    #[tokio::test]
    async fn test_cargo_dist() {
        let url =
            "https://github.com/axodotdev/cargo-dist/releases/download/v1.0.0-rc.1/dist-manifest.json";
        let manfiest = download_dist_manfiest(url).await.unwrap();
        let art_url = get_artifact_url_from_manfiest(url, &manfiest).await;
        assert!(!art_url.is_empty())
    }

    #[tokio::test]
    async fn test_deno() {
        let url = "https://github.com/denoland/deno";
        let repo = Repo::try_from(url).unwrap();
        let artifact_url = repo.get_artifact_url(detect_targets().await).await;
        assert_eq!(artifact_url.len(), 2);
    }

    #[tokio::test]
    async fn test_get_artifact_download_url() {
        for url in [
        "https://github.com/Ryubing/Ryujinx/releases/latest/download/^ryujinx-*.*.*-win_x64.zip",
        "https://github.com/Ryubing/Ryujinx/releases/download/1.2.80/ryujinx-*.*.*-win_x64.zip",
        "https://github.com/Ryubing/Ryujinx/releases/download/1.2.78/ryujinx-*.*.*-win_x64.zip",
        "https://github.com/shinchiro/mpv-winbuild-cmake/releases/latest/download/^mpv-x86_64-v3-.*?-git-.*?",
        "https://github.com/NickeManarin/ScreenToGif/releases/latest/download/ScreenToGif.[0-9]*.[0-9]*.[0-9]*.Portable.x64.zip",
        "https://github.com/ip7z/7zip/releases/latest/download/7z.*?-linux-x64.tar.xz",
        "https://github.com/mpv-easy/mpv-winbuild/releases/latest/download/mpv-x86_64-v3-.*?-git-.*?.zip",
      ]{
          let art_url = get_artifact_download_url(url).await;
          assert_eq!(art_url.len(), 1);
      }
    }

    #[tokio::test]
    async fn test_starship() {
        let repo = Repo::try_from("https://github.com/starship/starship").unwrap();
        let artifact_url = repo.get_artifact_url(detect_targets().await).await;
        assert_eq!(artifact_url.len(), 1);
    }

    #[tokio::test]
    async fn test_quickjs_ng() {
        let json = "./dist-manifest/quickjs-ng.json";
        let manifest = read_dist_manfiest(json).unwrap();
        let urls = get_artifact_url_from_manfiest(json, &manifest).await;
        assert_eq!(urls.len(), 2);

        for i in urls {
            let download_urls = get_artifact_download_url(&i).await;
            assert_eq!(download_urls.len(), 1);
        }
    }

    // #[tokio::test]
    // async fn test_graaljs() {
    //     let json = "./dist-manifest/graaljs.json";
    //     let manifest = read_dist_manfiest(json).unwrap();
    //     let urls = get_artifact_url_from_manfiest(json, &manifest).await;
    //     assert_eq!(urls.len(), 1);

    //     for i in urls {
    //         let download_urls = get_artifact_download_url(&i).await;
    //         assert_eq!(download_urls.len(), 1);
    //     }
    // }

    #[test]
    fn test_is_exe_file() {
        for (a,b) in [
          ("https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe", true),
        ("https://github.com/pnpm/pnpm/releases/latest/download/pnpm-win-x64.exe", true),
        ("https://github.com/pnpm/pnpm/releases/latest/download/pnpm-win-x64", true),
        ("https://github.com/easy-install/easy-install/releases/download/v0.1.5/ei-x86_64-apple-darwin.tar.gz", false),
        ("https://github.com/easy-install/easy-install", false),
        ("https://github.com/easy-install/easy-install/releases/tag/v0.1.5", false)
      ]{
        assert_eq!(is_exe_file(a),b);
      }
    }

    #[test]
    fn test_get_common_prefix() {
        assert_eq!(
            get_common_prefix_len(&["/a/ab/c", "/a/ad", "/a/ab/d",]),
           3
        );
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
