use std::path::PathBuf;

use crud_path::{add_github_path, is_github};

pub const IS_WINDOWS: bool = cfg!(target_os = "windows");

pub fn add_to_path(dir: &str) {
    if crud_path::has_path(dir) {
        return;
    }

    if is_github() {
        add_github_path(dir);
        println!("Successfully added {dir} to github's $PATH");
    }

    if let Some(sh) = crud_path::add_path(dir) {
        println!("Successfully added {dir} to {sh}'s $PATH");
    } else {
        println!("You need to add {dir} to your $PATH");
    }
}

pub fn get_install_dir() -> PathBuf {
    let mut home = dirs::home_dir().expect("Failed to get home_dir");
    home.push(".easy-install");

    if !home.exists() {
        std::fs::create_dir_all(&home).expect("Failed to create_dir home_dir");
    }
    // let home_str = home.to_str().expect("Failed to get home_dir string");
    // add_to_path(home_str);
    home
}
