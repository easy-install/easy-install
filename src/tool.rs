#[cfg(windows)]
use std::os::windows::fs::MetadataExt;

use easy_archive::tool::{human_size, mode_to_string};
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use crate::{env::add_to_path, install::Output};

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
