pub mod artifact;
pub mod download;
pub mod env;
pub mod install;
pub mod manfiest;
pub mod tool;
pub mod ty;
pub mod rule;

#[derive(Parser, Debug, Clone)]
#[command(version, about, long_about = None)]
pub struct Args {
    #[arg(short, long)]
    dir: Option<String>,

    #[arg()]
    pub url: String,
}

use clap::Parser;
use tool::add_output_to_path;

pub async fn run_main(args: Args) {
    let Args { url, dir } = args;
    let output = install::install(&url, dir).await;
    add_output_to_path(&output);
    if output.is_empty() {
        println!("No file installed from {}", url);
    }
}
