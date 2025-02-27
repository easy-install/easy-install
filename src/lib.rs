mod artifact;
mod download;
mod env;
mod install;
mod manfiest;
mod tool;
mod ty;

use clap::Parser;
use tool::add_output_to_path;

#[derive(Parser, Debug, Clone)]
#[command(version, about, long_about = None)]
pub struct Args {
    #[arg(short, long)]
    dir: Option<String>,

    #[arg(long, default_value_t = false)]
    install_only: bool,

    #[arg()]
    pub url: String,
}

pub async fn run_main(args: Args) {
    let Args {
        url,
        dir,
        install_only,
    } = args;
    let output = install::install(&url, dir).await;
    if !install_only {
        add_output_to_path(&output);
    }
    if output.is_empty() {
        println!("No file installed from {}", url);
    }
}
