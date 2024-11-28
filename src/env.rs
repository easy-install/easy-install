use std::path::PathBuf;

pub const IS_WINDOWS: bool = cfg!(target_os = "windows");

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
    let shell = "([System.Security.Principal.WindowsPrincipal][System.Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([System.Security.Principal.WindowsBuiltInRole]::Administrator)";
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
        std::fs::create_dir(&home).expect("Failed to create_dir home_dir");
    }
    let path = std::env::var_os("PATH")
        .expect("Failed to get PATH")
        .to_string_lossy()
        .to_string();
    let paths: Vec<&str> = path.split(';').collect();

    let home_str = home.to_str().expect("Failed to get home_dir string");
    if !paths.contains(&home_str) {
        let mode = if is_admin() { "Machine" } else { "User" };
        let shell = format!(
            r#"$currentPath = [Environment]::GetEnvironmentVariable("Path", "{mode}");$newPath = "$currentPath;{}"; [Environment]::SetEnvironmentVariable("Path", $newPath, "{mode}")"#,
            home_str
        );
        std::process::Command::new("powershell")
            .args(["-c", &shell])
            .output()
            .expect("Failed to execute powershell command");
    }

    home
}
