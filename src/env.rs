use std::path::PathBuf;

pub const IS_WINDOWS: bool = cfg!(target_os = "windows");

#[cfg(not(target_os = "windows"))]
pub fn add_to_path(dir: &str) {
    let bash = format!(r#"echo 'export PATH="$PATH:{dir}"' >> ~/.bashrc"#);
    let zsh = format!(r#"echo 'export PATH="$PATH:{dir}"' >> ~/.zshrc"#);
    let fish = format!(r#"set -U fish_user_paths {dir} $fish_user_paths"#);
    println!("run cmd to add {dir} to your $PATH:");
    if let Some(which_shell::ShellVersion { shell, version: _ }) = which_shell::which_shell() {
        match shell {
            which_shell::Shell::Bash => println!("{}", bash),
            which_shell::Shell::Zsh => println!("{}", zsh),
            which_shell::Shell::Fish => println!("{}", fish),
            sh => {
                println!("not support shell: {}", sh)
            }
        }
    }
}

#[cfg(target_os = "windows")]
pub fn add_to_path(dir: &str) {
    let path = std::env::var_os("PATH")
        .expect("Failed to get PATH")
        .to_string_lossy()
        .to_string();
    let paths: Vec<&str> = path.split(';').collect();

    if paths.contains(&dir) {
        return;
    }

    let mode = if is_admin() { "Machine" } else { "User" };
    let shell = format!(
        r#"$currentPath = [Environment]::GetEnvironmentVariable("Path", "{mode}");$newPath = "$currentPath;{dir}"; [Environment]::SetEnvironmentVariable("Path", $newPath, "{mode}")"#,
    );
    std::process::Command::new("powershell")
        .args(["-c", &shell])
        .output()
        .expect("Failed to execute powershell command");
    println!("Successfully added {dir} to $PATH");
}

#[cfg(not(target_os = "windows"))]
pub fn is_admin() -> bool {
    use libc::{geteuid, getuid};
    unsafe { getuid() == 0 || geteuid() == 0 }
}

#[cfg(not(target_os = "windows"))]
pub fn get_install_dir() -> PathBuf {
    use std::str::FromStr;
    let home = PathBuf::from_str(if is_admin() {
        "/usr/bin"
    } else {
        "/usr/local/bin"
    })
    .unwrap();

    if !home.exists() {
        std::fs::create_dir(&home).expect("Failed to create_dir home_dir");
    }
    home
}

#[cfg(target_os = "windows")]
pub fn is_admin() -> bool {
    let shell = "[bool]([System.Security.Principal.WindowsPrincipal][System.Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([System.Security.Principal.WindowsBuiltInRole]::Administrator)";
    let output = std::process::Command::new("powershell")
        .args(["-c", shell])
        .output()
        .expect("Failed to execute powershell command");

    let s = String::from_utf8(output.stdout).unwrap_or_default();
    &s == "True"
}

#[cfg(target_os = "windows")]
pub fn get_install_dir() -> PathBuf {
    let mut home = dirs::home_dir().expect("Failed to get home_dir");
    home.push("easy-install");

    if !home.exists() {
        std::fs::create_dir_all(&home).expect("Failed to create_dir home_dir");
    }
    let home_str = home.to_str().expect("Failed to get home_dir string");
    add_to_path(home_str);
    home
}
