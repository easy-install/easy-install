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
use anyhow::{Context, Result};
use clap::{ArgAction, CommandFactory, Parser, Subcommand};
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
    pub no_path: bool,
    pub fuzzy: bool,
    pub regex: Option<String>,
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
            no_path: false,
            fuzzy: false,
            regex: None,
        }
    }
}

impl InstallConfig {
    /// Load configuration from persistent config file
    /// Returns InstallConfig with values from config file, or defaults if not set
    pub fn load() -> Self {
        let persistent_config = PersistentConfig::load();

        Self {
            dir: persistent_config.dir,
            name: Vec::new(),
            alias: None,
            target: persistent_config.target,
            retry: persistent_config.retry.unwrap_or(3) as usize,
            proxy: persistent_config.proxy.unwrap_or(Proxy::Github),
            timeout: persistent_config.timeout.unwrap_or(600),
            strip: persistent_config.strip.unwrap_or(false),
            upx: persistent_config.upx.unwrap_or(false),
            quiet: false,
            no_path: false,
            fuzzy: false,
            regex: None,
        }
    }

    pub fn get_local_target(&self) -> Vec<Target> {
        if let Some(t) = self.target {
            return vec![t];
        }
        guess_target::get_local_target().to_vec()
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
    /// Generate shell completion scripts
    Completions {
        /// Shell type to generate completions for
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
    /// Upgrade crash to the latest version
    Upgrade,
}

const CARGO_PKG_VERSION: &str = env!("CARGO_PKG_VERSION");
const GIT_HASH: &str = git_version::git_version!();
const VERSION: &str = const_str::concat!(CARGO_PKG_VERSION, " ", GIT_HASH);

#[derive(Parser, Debug, Clone)]
#[command(name="ei", version=VERSION, about, long_about)]
pub struct Args {
    #[command(subcommand)]
    pub cmd: Option<Command>,

    /// GitHub repo (owner/repo), release URL, or artifact URL
    #[arg(default_value_t = String::new())]
    pub url: String,

    /// Installation directory for downloaded binaries
    #[arg(short, long, help = "Installation directory")]
    pub dir: Option<String>,

    /// Skip adding installed binaries to PATH
    #[arg(long, default_value_t = false, action = ArgAction::SetTrue, help = "Skip adding installed binaries to PATH")]
    pub no_path: bool,

    /// Filter artifacts by name (comma-separated, word-boundary match)
    ///
    /// Matches filenames whose stem starts with the given name followed
    /// by a separator (`-`, `_`, `.`) or end-of-string. This prevents
    /// e.g. `--name qjs` from matching `qjsc-linux-x86`.
    #[arg(
        long,
        value_delimiter = ',',
        help = "Filter artifacts by name (comma-separated, word-boundary match)"
    )]
    pub name: Vec<String>,

    /// Rename the installed binary
    #[arg(long, help = "Rename the installed binary")]
    pub alias: Option<String>,

    /// Target platform (e.g., x86_64-unknown-linux-gnu)
    #[arg(long, help = "Target platform (auto-detected if not specified)")]
    pub target: Option<Target>,

    /// Number of retry attempts for failed downloads
    #[arg(long, default_value_t = 3, help = "Number of retry attempts")]
    pub retry: usize,

    /// GitHub proxy to use (github, ghproxy, etc.)
    #[arg(long, help = "GitHub proxy to use")]
    pub proxy: Option<Proxy>,

    /// Network request timeout in seconds
    #[arg(long, help = "Network request timeout in seconds")]
    pub timeout: Option<u64>,

    /// Strip debug symbols from executable
    #[arg(
        long,
        help = "Strip debug symbols from executable",
        default_missing_value = "true",
        num_args = 0..=1,
    )]
    pub strip: Option<bool>,

    /// Compress executable with UPX
    #[arg(
        long,
        help = "Compress executable with UPX",
        default_missing_value = "true",
        num_args = 0..=1,
    )]
    pub upx: Option<bool>,

    /// Suppress all output messages
    #[arg(
        long,
        short,
        help = "Suppress all output messages",
        default_missing_value = "true",
        num_args = 0..=1,
        default_value_t = false
    )]
    pub quiet: bool,

    /// Use fuzzy target matching (match arch+os, ignoring abi)
    ///
    /// By default ei requires an exact target triple match (including the
    /// abi, e.g. gnu vs musl). Enable this to fall back to a (arch, os)
    /// match when no exact match is found, so assets whose filenames omit
    /// the abi (e.g. "mihomo-linux-amd64.tar.gz" parsed as gnu) can still
    /// be selected when you requested musl.
    #[arg(
        long,
        help = "Use fuzzy target matching (match arch+os, ignoring abi)",
        default_missing_value = "true",
        num_args = 0..=1,
        default_value_t = false
    )]
    pub fuzzy: bool,

    /// Regex pattern to match against the original GitHub asset filenames
    ///
    /// When supplied, the regex is matched directly against each asset's
    /// original filename (e.g. "mpv-v0.41.0-dev-g4c220ffd9-x86_64-pc-windows-msvc.zip"),
    /// bypassing `guess_target` entirely. This gives you full control over
    /// which asset to select when filenames use non-standard naming that
    /// confuses automatic target detection.
    ///
    /// The regex must match exactly one asset; matching zero or multiple
    /// is an error. Examples:
    ///   --regex "x86_64-pc-windows-msvc"        (match a specific triple)
    ///   --regex "macos-15-arm"                   (match a macOS variant)
    ///   --regex "aarch64-pc-windows-msvc\\.zip$" (anchor to file extension)
    #[arg(
        long,
        help = "Regex to match asset filenames directly (bypasses guess_target)"
    )]
    pub regex: Option<String>,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            cmd: None,
            url: "".to_string(),
            dir: None,
            no_path: false,
            name: vec![],
            alias: None,
            target: None,
            retry: 3,
            proxy: None,
            timeout: None,
            strip: None,
            upx: None,
            quiet: false,
            fuzzy: false,
            regex: None,
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
            no_path: value.no_path,
            fuzzy: value.fuzzy,
            regex: value.regex,
        }
    }
}

pub async fn run_main(args: Args) -> Result<()> {
    // Handle completions subcommand
    if let Some(Command::Completions { shell }) = args.cmd {
        return handle_completions_command(shell);
    }

    if let Some(Command::Upgrade) = args.cmd {
        return handle_upgrade().await;
    }

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
    let config = args.into();
    ei(&url, &config).await?;
    Ok(())
}

pub async fn ei(url: &str, config: &InstallConfig) -> Result<()> {
    let output = install::install(url, config).await?;
    if !config.no_path {
        add_output_to_path(&output, config);
    }
    if output.is_empty() && !config.quiet {
        println!("No file installed from {url}");
    }
    Ok(())
}

fn handle_completions_command(shell: clap_complete::Shell) -> Result<()> {
    use clap_complete::generate;
    use std::io;

    let mut cmd = Args::command();
    let bin_name = cmd.get_name().to_string();

    generate(shell, &mut cmd, bin_name, &mut io::stdout());
    Ok(())
}

async fn handle_upgrade() -> Result<()> {
    let exe = std::env::current_exe()?;
    let dir = exe
        .parent()
        .ok_or_else(|| anyhow::anyhow!("ei dir not found"))?;

    // On Windows the running exe cannot be overwritten, but it *can* be
    // renamed. Rename the old binary to free the original filename for the
    // new version, then install. Clean up previous stale .old files first.
    let old_exe = exe.with_extension("exe.old");
    let _ = std::fs::remove_file(&old_exe);
    std::fs::rename(&exe, &old_exe).context("Failed to rename running ei.exe for upgrade")?;

    let config = InstallConfig {
        dir: Some(dir.to_string_lossy().to_string()),
        alias: Some("ei".to_string()),
        ..InstallConfig::load()
    };

    match ei("easy-install/easy-install", &config).await {
        Ok(()) => {
            // Upgrade succeeded — try to remove the old binary (best-effort;
            // it may still be locked and will be cleaned up on next upgrade).
            let _ = std::fs::remove_file(&old_exe);
            Ok(())
        }
        Err(e) => {
            // Rollback: restore the old binary so the user isn't left
            // without a working ei.
            let _ = std::fs::remove_file(&exe);
            std::fs::rename(&old_exe, &exe)?;
            Err(e)
        }
    }
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
            let current = config
                .proxy
                .map_or("not set (default: Github)".to_string(), |p| {
                    format!("{p:?}")
                });
            apply_config(
                &mut config,
                quiet,
                value,
                PersistentConfig::set_proxy,
                |v| format!("{v:?}"),
                "Proxy",
                current,
            )?
        }
        ConfigSubcommand::Dir { value } => {
            let current = config.dir.as_deref().unwrap_or("not set").to_string();
            apply_config(
                &mut config,
                quiet,
                value,
                |c, v| c.set_dir(expand_path(&v)),
                |v: &String| v.to_string(),
                "Directory",
                current,
            )?
        }
        ConfigSubcommand::Target { value } => {
            let current = config
                .target
                .map_or("not set (auto-detect)".to_string(), |t| {
                    t.to_str().to_string()
                });
            apply_config(
                &mut config,
                quiet,
                value,
                PersistentConfig::set_target,
                |v| v.to_str().to_string(),
                "Target",
                current,
            )?
        }
        ConfigSubcommand::Timeout { value } => {
            let current = config
                .timeout
                .map_or("not set (default: 600 seconds)".to_string(), |t| {
                    format!("{t} seconds")
                });
            apply_config(
                &mut config,
                quiet,
                value,
                PersistentConfig::set_timeout,
                |v: &u64| v.to_string(),
                "Timeout",
                current,
            )?
        }
        ConfigSubcommand::Retry { value } => {
            let current = config
                .retry
                .map_or("not set (default: 3)".to_string(), |t| format!("{t}"));
            apply_config(
                &mut config,
                quiet,
                value,
                PersistentConfig::set_retry,
                |v: &u64| v.to_string(),
                "Retry",
                current,
            )?
        }
        ConfigSubcommand::Upx { value } => {
            let current = config
                .upx
                .map_or("not set (default: false)".to_string(), |t| format!("{t}"));
            apply_config(
                &mut config,
                quiet,
                value,
                PersistentConfig::set_upx,
                |v: &bool| v.to_string(),
                "Upx",
                current,
            )?
        }
        ConfigSubcommand::Strip { value } => {
            let current = config
                .strip
                .map_or("not set (default: false)".to_string(), |t| format!("{t}"));
            apply_config(
                &mut config,
                quiet,
                value,
                PersistentConfig::set_strip,
                |v: &bool| v.to_string(),
                "Strip",
                current,
            )?
        }
    }

    Ok(())
}

/// Generic set-or-show helper for `config` subcommands: if a value is
/// provided, apply it, persist, and confirm; otherwise print the current
/// value.
fn apply_config<T>(
    config: &mut PersistentConfig,
    quiet: bool,
    value: Option<T>,
    setter: impl FnOnce(&mut PersistentConfig, T),
    fmt_val: impl Fn(&T) -> String,
    label: &str,
    current: String,
) -> Result<()> {
    if let Some(v) = value {
        let msg = format!("{label} set to: {}", fmt_val(&v));
        setter(config, v);
        config.save_quiet(quiet)?;
        if !quiet {
            println!("{msg}");
        }
    } else if !quiet {
        println!("Current {label}: {current}");
    }
    Ok(())
}
