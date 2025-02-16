#[cfg(windows)]
use std::os::windows::fs::MetadataExt;

use easy_archive::tool::{human_size, mode_to_string};
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use crate::{
    env::add_to_path,
    install::{Output, OutputFile},
};

pub fn get_bin_name(s: &str) -> String {
    if cfg!(windows) && !s.ends_with(".exe") && !s.contains(".") {
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
                        mode_to_string(k.mode, k.is_dir),
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

pub fn add_output_to_path(output: &Output) {
    for v in output.values() {
        for f in &v.files {
            if check(f, &v.install_dir, &v.bin_dir) {
                println!("Warning: file exists at {}", f.install_path);
            }
        }
        add_to_path(&v.install_dir);
        if v.install_dir != v.bin_dir {
            add_to_path(&v.bin_dir);
        }

        #[cfg(unix)]
        if v.files.len() == 1 {
            let i = &v.files[0];
            crate::install::add_execute_permission(&i.install_path)
                .expect("failed to add_execute_permission");
        }
    }
}

pub fn get_filename(s: &str) -> Option<String> {
    s.split("/").last().map(|i| i.to_string())
}

#[cfg(windows)]
fn which(name: &str) -> Option<String> {
    let cmd = std::process::Command::new("powershell")
        .args(["-c", &format!("(get-command {name}).Source")])
        .output()
        .ok()?;
    String::from_utf8(cmd.stdout)
        .ok()
        .map(|i| i.trim().replace("\\", "/"))
}

#[cfg(unix)]
fn which(name: &str) -> Option<String> {
    let cmd = std::process::Command::new("which")
        .arg(name)
        .output()
        .ok()?;
    String::from_utf8(cmd.stdout).ok()
}

const EXEC_MASK: u32 = 0o111;
fn executable(file: &OutputFile) -> bool {
    file.install_path.ends_with(".exe") || file.mode & EXEC_MASK != 0
}

pub fn check(file: &OutputFile, install_dir: &str, binstall_dir: &str) -> bool {
    let file_path = &file.install_path;
    if !file_path.starts_with(install_dir)
        || !file_path.starts_with(binstall_dir)
        || !executable(file)
    {
        return false;
    }

    let name = get_filename(file_path).unwrap();
    if let Some(p) = which(&name) {
        if file_path == &p {
            return true;
        }
    }
    false
}
