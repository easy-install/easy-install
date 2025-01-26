use std::path::PathBuf;

pub const IS_WINDOWS: bool = cfg!(target_os = "windows");

fn is_github_action() -> bool {
    std::env::var("GITHUB_ACTIONS") == Ok("true".to_string())
}
#[cfg(not(target_os = "windows"))]
fn add_github_path(path: &str) {
    std::process::Command::new("bash")
        .args(["-c", &format!(r#"echo "PATH=$PATH:{path}" >> $GITHUB_ENV"#)])
        .output()
        .expect("add_github_path error");
}

#[cfg(target_os = "windows")]
fn add_github_path(path: &str) {
    std::process::Command::new("powershell")
        .args([
            "-c",
            &format!(
                r#"echo "{path}" | Out-File -Append -FilePath $env:GITHUB_PATH -Encoding utf8"#
            ),
        ])
        .output()
        .expect("add_github_path error");
}

pub fn add_to_path(dir: &str) {
    if is_github_action() {
        add_github_path(dir);
    }

    if crud_path::has_path(dir) {
        return;
    }
    if crud_path::add_path(dir) {
        println!("Successfully added {dir} to $PATH");
    } else {
        println!("You need to add {dir} to your PATH");
    }
}

#[cfg(not(target_os = "windows"))]
pub fn get_install_dir() -> PathBuf {
    use std::str::FromStr;
    let home = PathBuf::from_str(if is_admin::is_admin() {
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
