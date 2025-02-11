pub mod artifact;
pub mod download;
pub mod env;
pub mod install;
pub mod manfiest;
pub mod tool;

#[derive(Parser, Debug, Clone)]
#[command(version, about, long_about = None)]
pub struct Args {
    #[arg(short, long)]
    dir: Option<String>,

    #[arg()]
    pub name_or_url: String,
}

use clap::Parser;
use crud_path::{add_github_path, add_path, has_path, is_github};

pub async fn run_main(args: Args) {
    let Args { name_or_url, dir } = args;
    let output = install::install(&name_or_url, dir).await;
    for (_, v) in output {
        for i in &v {
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
            install::add_execute_permission(&i.install_path).expect("failed to add_execute_permission");
        }
    }
}
