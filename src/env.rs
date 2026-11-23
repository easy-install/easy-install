use std::path::PathBuf;

use crate::error::{Context, Result};
use crud_path::{add_github_path, is_github};
use which_shell::Shell;

use crate::config::DEFAULT_CONFIG_DIR;

/// Register `dir` in the login `$PATH` of the local machine.
///
/// `shell` selects the configuration file; when it is `None` (or a shell
/// `crud-path` cannot write) the current shell is detected instead, which is
/// what a user running `ei` interactively expects.
pub(crate) fn add_to_path(dir: &str, shell: Option<Shell>, quiet: bool) {
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

    // An explicit `--shell` means *that* shell, on every platform: writing a
    // different shell's config (or the global registry PATH) is surprising.
    if let Some(shell) = shell {
        match crud_path::add_path_to_shell(shell, dir) {
            Some(file) => {
                if !quiet {
                    println!(
                        "Successfully added {dir} to {shell}'s $PATH ({})",
                        crate::path::path_to_str(&file)
                    );
                }
            }
            None => {
                if !quiet {
                    eprintln!("Warning: could not add {dir} to {shell}'s $PATH");
                    eprintln!("  Add it manually, or rerun without --shell to use the system PATH");
                }
            }
        }
        return;
    }

    if let Some(shell) = crud_path::add_path(dir, true) {
        if !quiet {
            // `Shell::config_file` is the location the entry went to, so the
            // message can name it without the writer returning anything extra.
            match shell.config_file() {
                Some(file) => println!(
                    "Successfully added {dir} to {shell}'s $PATH ({})",
                    crate::path::path_to_str(&file)
                ),
                None => println!("Successfully added {dir} to {shell}'s $PATH"),
            }
        }
    } else if !quiet {
        println!("You need to add {dir} to your $PATH");
    }
}

pub(crate) fn get_install_dir() -> Result<PathBuf> {
    let mut home = dirs::home_dir().context("Failed to get home_dir")?;
    home.push(DEFAULT_CONFIG_DIR);

    if !home.exists() {
        std::fs::create_dir_all(&home).context("Failed to create_dir home_dir")?;
    }
    Ok(home)
}
