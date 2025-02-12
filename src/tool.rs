#[cfg(windows)]
use std::os::windows::fs::MetadataExt;

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use crud_path::{add_github_path, add_path, has_path, is_github};

use crate::install::Output;

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

fn mode_to_string(mode: u32, is_dir: bool) -> String {
    let rwx_mapping = ["---", "--x", "-w-", "-wx", "r--", "r-x", "rw-", "rwx"];

    let owner = rwx_mapping[((mode >> 6) & 0b111) as usize]; // Owner permissions
    let group = rwx_mapping[((mode >> 3) & 0b111) as usize]; // Group permissions
    let others = rwx_mapping[(mode & 0b111) as usize]; // Others permissions
    let d = if is_dir { "d" } else { "-" };
    format!("{}{}{}{}", d, owner, group, others)
}

fn round(value: f64) -> String {
    let mut s = format!("{:.1}", value);
    if s.contains('.') {
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
    }
    s
}

pub fn human_size(bytes: u32) -> String {
    if bytes == 0 {
        return "0".to_string();
    }
    let units = ["", "K", "M", "G", "T", "P", "E", "Z", "Y"];
    let b = bytes as f64;
    let exponent = (b.log(1024.0)).floor() as usize;
    let value = b / 1024f64.powi(exponent as i32);
    let rounded = round(value);
    format!("{}{}", rounded, units[exponent])
}

pub fn display_output(output: &Output) -> String {
    let mut v = vec![];
    for i in output.values() {
        let max_size_len = i
            .iter()
            .fold(0, |pre, cur| pre.max(human_size(cur.size).len()));

        for k in i {
            let s = human_size(k.size);
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
    v.join("\n")
}

pub fn add_output_to_path(output: &Output) {
    for v in output.values() {
        for i in v {
            if !has_path(&i.install_dir) {
                add_path(&i.install_dir);
                if is_github() {
                    add_github_path(&i.install_dir);
                }
            }
        }
        #[cfg(unix)]
        if v.len() == 1 {
            let i = &v[0];
            crate::install::add_execute_permission(&i.install_path)
                .expect("failed to add_execute_permission");
        }
    }
}
#[cfg(test)]
mod test {
    use crate::tool::round;

    #[test]
    fn test_round() {
        for (a, b) in [(1., "1"), (1.0, "1"), (1.5, "1.5"), (1.23, "1.2")] {
            assert_eq!(round(a), b);
        }
    }
}
