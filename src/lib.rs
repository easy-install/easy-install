pub mod artifact;
pub mod download;
pub mod env;
pub mod install;
pub mod manfiest;

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
    let v = install::install(&name_or_url, dir).await;
    for i in v {
        if has_path(&i.install_dir) {
            add_path(&i.install_dir);
            if is_github() {
                add_github_path(&i.install_dir);
            }
        }
    }
}
