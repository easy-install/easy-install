pub mod artifact;
pub mod download;
pub mod env;
pub mod install;
pub mod manfiest;

#[derive(Parser, Debug, Clone)]
#[command(version, about, long_about = None)]
pub struct Args {
    #[arg()]
    pub name_or_url: String,
}

use clap::Parser;

pub async fn run_main(args: Args) {
    let Args { name_or_url } = args;
    install::install(&name_or_url).await;
}
