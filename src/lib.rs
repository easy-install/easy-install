mod artifact;
mod download;
mod env;
mod install;
mod manfiest;
mod tool;
mod ty;

use anyhow::Result;
use clap::Parser;
use guess_target::Target;
use tool::add_output_to_path;

#[derive(Debug, Clone, Default)]
pub struct InstallConfig {
    pub dir: Option<String>,
    pub name: Vec<String>,
    pub alias: Option<String>,
    pub target: Option<Target>,
}

impl InstallConfig {
    pub fn new(
        dir: Option<String>,
        name: Vec<String>,
        alias: Option<String>,
        target: Option<Target>,
    ) -> Self {
        Self {
            dir,
            name,
            alias,
            target,
        }
    }
}

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

    #[arg(long)]
    alias: Option<String>,

    #[arg(long)]
    target: Option<Target>,
}

impl Args {
    pub fn to_install_config(&self) -> InstallConfig {
        InstallConfig::new(
            self.dir.clone(),
            self.name.clone(),
            self.alias.clone(),
            self.target,
        )
    }
}

pub async fn run_main(args: Args) -> Result<()> {
    let url = args.url.clone();
    let install_only = args.install_only;
    let config = args.to_install_config();

    let output = install::install(&url, &config).await?;
    if !install_only {
        add_output_to_path(&output);
    }
    if output.is_empty() {
        println!("No file installed from {url}");
    }
    Ok(())
}
