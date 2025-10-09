mod artifact;
mod download;
mod env;
mod install;
mod manfiest;
mod tool;
mod ty;

use anyhow::Result;
use clap::Parser;
use tool::add_output_to_path;

#[derive(Parser, Debug, Clone)]
#[command(version, about, long_about = None)]
pub struct Args {
    #[arg()]
    pub(crate) url: String,

    #[arg(short, long)]
    dir: Option<String>,

    #[arg(long, default_value_t = false)]
    install_only: bool,

    #[arg(long, value_delimiter = ',')]
    name: Vec<String>,

    // #[arg(long)]
    // bin: String,

    #[arg(long)]
    alias: Option<String>,
}

pub async fn run_main(args: Args) -> Result<()> {
    let Args {
        url,
        dir,
        install_only,
        name,
        alias,
        // bin
    } = args;
    let output = install::install(&url, &name, dir, alias).await?;
    if !install_only {
        add_output_to_path(&output);
    }
    if output.is_empty() {
        println!("No file installed from {url}");
    }
    Ok(())
}
