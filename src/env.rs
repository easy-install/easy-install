use std::path::PathBuf;

use anyhow::{Context, Result};
use crud_path::{add_github_path, is_github};

pub(crate) fn add_to_path(dir: &str, quiet: bool) {
    let dir = dir.trim_end_matches('/');
    if crud_path::has_path(dir) {
        return;
    }

    if is_github() {
        add_github_path(dir);
        if !quiet {
            println!("Successfully added {dir} to github's $PATH");
        }
    }

    if let Some(sh) = crud_path::add_path(dir) {
        if !quiet {
            println!("Successfully added {dir} to {sh}'s $PATH");
        }
    } else if !quiet {
        println!("You need to add {dir} to your $PATH");
    }
}

pub(crate) fn get_install_dir() -> Result<PathBuf> {
    let mut home = dirs::home_dir().context("Failed to get home_dir")?;
    home.push(".ei");

    if !home.exists() {
        std::fs::create_dir_all(&home).context("Failed to create_dir home_dir")?;
    }
    Ok(home)
}
