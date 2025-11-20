mod artifact;
mod config;
mod download;
mod env;
mod install;
mod manfiest;
mod optimize;
mod tool;
mod types;

use crate::tool::expand_path;
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
    pub strip: bool,
    pub upx: bool,
    pub quiet: bool,
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
            strip: false,
            upx: false,
            quiet: false,
        }
    }
}

impl InstallConfig {
    pub fn get_local_target(&self) -> Vec<Target> {
        if let Some(t) = self.target {
            return vec![t];
        }
        guess_target::get_local_target()
    }
}

#[derive(Debug, Clone, Subcommand)]
pub enum ConfigSubcommand {
    /// View or set proxy configuration
    Proxy {
        /// Proxy value to set (omit to view current value)
        value: Option<Proxy>,
    },
    /// View or set installation directory
    Dir {
        /// Directory path to set (omit to view current value)
        value: Option<String>,
    },
    /// View or set target platform
    Target {
        /// Target platform to set (omit to view current value)
        value: Option<Target>,
    },
    /// View or set network timeout in seconds
    Timeout {
        /// Timeout in seconds (omit to view current value)
        value: Option<u64>,
    },
    /// View or set retry count
    Retry {
        /// Number of retries (omit to view current value)
        value: Option<u64>,
    },
    /// View or set UPX compression
    Upx {
        /// Enable or disable UPX compression (omit to view current value)
        value: Option<bool>,
    },
    /// View or set strip debug symbols
    Strip {
        /// Enable or disable stripping debug symbols (omit to view current value)
        value: Option<bool>,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum Command {
    /// Manage configuration settings
    Config {
        #[command(subcommand)]
        subcmd: Option<ConfigSubcommand>,
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

    #[arg(long, help = "Strip debug symbols from executable")]
    pub strip: Option<bool>,

    #[arg(long, help = "Compress executable with UPX")]
    pub upx: Option<bool>,

    #[arg(long, short, help = "Suppress all output messages")]
    pub quiet: bool,
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
            strip: None,
            upx: None,
            quiet: false,
        }
    }
}

impl From<Args> for InstallConfig {
    fn from(value: Args) -> Self {
        let persistent_config = PersistentConfig::load();

        let proxy = value
            .proxy
            .or(persistent_config.proxy)
            .unwrap_or(Proxy::Github);

        let timeout = value.timeout.or(persistent_config.timeout).unwrap_or(600);

        let dir = value.dir.clone().or(persistent_config.dir);

        let target = value.target.or(persistent_config.target);
        let strip = value.strip.or(persistent_config.strip).unwrap_or(false);
        let upx = value.upx.or(persistent_config.upx).unwrap_or(false);

        InstallConfig {
            dir,
            name: value.name.clone(),
            alias: value.alias.clone(),
            target,
            retry: value.retry,
            proxy,
            timeout,
            strip,
            upx,
            quiet: value.quiet,
        }
    }
}

pub async fn run_main(args: Args) -> Result<()> {
    // Handle config subcommand
    if let Some(Command::Config { subcmd }) = args.cmd {
        let quiet = args.quiet;
        return handle_config_command(subcmd, quiet);
    }

    // Regular install command
    let url = args.url.clone();

    if url.is_empty() {
        let s = Args::command().render_help();
        println!("{s}");
        return Ok(());
    }

    let install_only = args.install_only;
    let config = args.into();

    let output = install::install(&url, &config).await?;
    if !install_only {
        add_output_to_path(&output, &config);
    }
    if output.is_empty() && !config.quiet {
        println!("No file installed from {url}");
    }
    Ok(())
}

fn handle_config_command(subcmd: Option<ConfigSubcommand>, quiet: bool) -> Result<()> {
    let mut config = PersistentConfig::load();

    let Some(subcmd) = subcmd else {
        if !quiet {
            config.display();
        }
        return Ok(());
    };

    match subcmd {
        ConfigSubcommand::Proxy { value } => {
            if let Some(proxy) = value {
                config.set_proxy(proxy);
                config.save_quiet(quiet)?;
                if !quiet {
                    println!("Proxy set to: {:?}", proxy);
                }
            } else if !quiet {
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
        ConfigSubcommand::Dir { value } => {
            if let Some(val) = value {
                config.set_dir(expand_path(&val));
                config.save_quiet(quiet)?;
                if !quiet {
                    println!("Directory set to: {}", val);
                }
            } else if !quiet {
                println!(
                    "Current directory: {}",
                    config.dir.as_deref().unwrap_or("not set")
                );
            }
        }
        ConfigSubcommand::Target { value } => {
            if let Some(target) = value {
                config.set_target(target);
                config.save_quiet(quiet)?;
                if !quiet {
                    println!("Target set to: {}", target.to_str());
                }
            } else if !quiet {
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
        ConfigSubcommand::Timeout { value } => {
            if let Some(timeout) = value {
                config.set_timeout(timeout);
                config.save_quiet(quiet)?;
                if !quiet {
                    println!("Timeout set to: {} seconds", timeout);
                }
            } else if !quiet {
                println!(
                    "Current timeout: {}",
                    config.timeout.map_or(
                        "not set (default: 600 seconds)".to_string(),
                        |t| format!("{} seconds", t)
                    )
                );
            }
        }
        ConfigSubcommand::Retry { value } => {
            if let Some(retry) = value {
                config.set_retry(retry);
                config.save_quiet(quiet)?;
                if !quiet {
                    println!("Retry set to: {}", retry);
                }
            } else if !quiet {
                println!(
                    "Current retry: {}",
                    config
                        .retry
                        .map_or("not set (default: 3)".to_string(), |t| format!("{}", t))
                );
            }
        }
        ConfigSubcommand::Upx { value } => {
            if let Some(upx) = value {
                config.set_upx(upx);
                config.save_quiet(quiet)?;
                if !quiet {
                    println!("Upx set to: {}", upx);
                }
            } else if !quiet {
                println!(
                    "Current upx: {}",
                    config
                        .upx
                        .map_or("not set (default: false)".to_string(), |t| format!("{}", t))
                );
            }
        }
        ConfigSubcommand::Strip { value } => {
            if let Some(strip) = value {
                config.set_strip(strip);
                config.save_quiet(quiet)?;
                if !quiet {
                    println!("Strip set to: {}", strip);
                }
            } else if !quiet {
                println!(
                    "Current strip: {}",
                    config
                        .strip
                        .map_or("not set (default: false)".to_string(), |t| format!("{}", t))
                );
            }
        }
    }

    Ok(())
}
