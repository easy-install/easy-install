mod artifact;
mod config;
mod download;
mod env;
mod install;
mod manfiest;
mod tool;
mod types;

use anyhow::Result;
use clap::{CommandFactory, Parser, Subcommand};
use config::PersistentConfig;
use github_proxy::Proxy;
use guess_target::Target;
use tool::add_output_to_path;

#[derive(Debug, Clone)]
pub struct InstallConfig {
    pub dir: Option<String>,
    pub name: Vec<String>,
    pub alias: Option<String>,
    pub target: Option<Target>,
    pub retry: usize,
    pub proxy: Proxy,
    pub timeout: u64,
}

impl Default for InstallConfig {
    fn default() -> Self {
        Self {
            dir: None,
            name: Vec::new(),
            alias: None,
            target: None,
            retry: 3,
            proxy: Proxy::Github,
            timeout: 600,
        }
    }
}

impl InstallConfig {
    pub fn new(
        dir: Option<String>,
        name: Vec<String>,
        alias: Option<String>,
        target: Option<Target>,
        retry: usize,
        proxy: Proxy,
        timeout: u64,
    ) -> Self {
        Self {
            dir,
            name,
            alias,
            target,
            retry,
            proxy,
            timeout,
        }
    }

    pub fn get_local_target(&self) -> Vec<Target> {
        if let Some(t) = self.target {
            return vec![t];
        }
        guess_target::get_local_target()
    }
}

#[derive(Debug, Clone, Subcommand)]
pub enum Command {
    /// Manage configuration settings
    Config {
        /// Configuration key to view or modify (proxy, dir, target, timeout)
        key: String,
        /// Value to set (omit to view current value)
        value: Option<String>,
    },
}

#[derive(Parser, Debug, Clone)]
#[command(version, about, long_about)]
pub struct Args {
    #[command(subcommand)]
    pub cmd: Option<Command>,

    #[arg(default_value_t = String::new())]
    pub url: String,

    #[arg(short, long)]
    pub dir: Option<String>,

    #[arg(long, default_value_t = false)]
    pub install_only: bool,

    #[arg(long, value_delimiter = ',')]
    pub name: Vec<String>,

    #[arg(long)]
    pub alias: Option<String>,

    #[arg(long)]
    pub target: Option<Target>,

    #[arg(long, default_value_t = 3)]
    pub retry: usize,

    #[arg(long)]
    pub proxy: Option<Proxy>,

    #[arg(long, help = "Network request timeout in seconds")]
    pub timeout: Option<u64>,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            cmd: None,
            url: "".to_string(),
            dir: None,
            install_only: false,
            name: vec![],
            alias: None,
            target: None,
            retry: 3,
            proxy: None,
            timeout: None,
        }
    }
}

impl Args {
    pub fn to_install_config(&self) -> InstallConfig {
        let persistent_config = PersistentConfig::load();

        let proxy = self
            .proxy
            .or(persistent_config.proxy)
            .unwrap_or(Proxy::Github);

        let timeout = self.timeout.or(persistent_config.timeout).unwrap_or(600);

        let dir = self.dir.clone().or(persistent_config.dir);

        let target = self.target.or(persistent_config.target);

        InstallConfig::new(
            dir,
            self.name.clone(),
            self.alias.clone(),
            target,
            self.retry,
            proxy,
            timeout,
        )
    }
}

pub async fn run_main(args: Args) -> Result<()> {
    // Handle config subcommand
    if let Some(Command::Config { key, value }) = args.cmd {
        return handle_config_command(&key, value);
    }

    // Regular install command
    let url = args.url.clone();

    if url.is_empty() {
        let s = Args::command().render_usage();
        println!("{s}");
        return Ok(());
    }

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

fn handle_config_command(key: &str, value: Option<String>) -> Result<()> {
    let mut config = PersistentConfig::load();

    match key.to_lowercase().as_str() {
        "proxy" => {
            if let Some(val) = value {
                let proxy =
                    Proxy::from_str(&val).map_err(|e| anyhow::anyhow!("Invalid proxy: {}", e))?;
                config.set_proxy(proxy);
                config.save()?;
                println!("Proxy set to: {:?}", proxy);
            } else {
                println!(
                    "Current proxy: {}",
                    config
                        .proxy
                        .map_or("not set (default: Github)".to_string(), |p| format!(
                            "{:?}",
                            p
                        ))
                );
            }
        }
        "dir" => {
            if let Some(val) = value {
                config.set_dir(val.clone());
                config.save()?;
                println!("Directory set to: {}", val);
            } else {
                println!(
                    "Current directory: {}",
                    config.dir.as_deref().unwrap_or("not set")
                );
            }
        }
        "target" => {
            if let Some(val) = value {
                let target =
                    Target::from_str(&val).map_err(|e| anyhow::anyhow!("Invalid target: {}", e))?;
                config.set_target(target);
                config.save()?;
                println!("Target set to: {}", target.to_str());
            } else {
                println!(
                    "Current target: {}",
                    config
                        .target
                        .map_or("not set (auto-detect)".to_string(), |t| t
                            .to_str()
                            .to_string())
                );
            }
        }
        "timeout" => {
            if let Some(val) = value {
                let timeout: u64 = val
                    .parse()
                    .map_err(|_| anyhow::anyhow!("Invalid timeout value, must be a number"))?;
                config.set_timeout(timeout);
                config.save()?;
                println!("Timeout set to: {} seconds", timeout);
            } else {
                println!(
                    "Current timeout: {}",
                    config.timeout.map_or(
                        "not set (default: 600 seconds)".to_string(),
                        |t| format!("{} seconds", t)
                    )
                );
            }
        }
        "show" | "list" | "all" => {
            config.display();
        }
        _ => {
            return Err(anyhow::anyhow!(
                "Unknown config key: {}. Valid keys are: proxy, dir, target, timeout, show",
                key
            ));
        }
    }

    Ok(())
}

use std::str::FromStr;
