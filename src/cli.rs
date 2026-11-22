//! Command-line interface.
//!
//! [`Args`] is the parsed command line; [`run_main`] dispatches subcommands
//! and hands regular installs to [`crate::ei`].

use crate::InstallConfig;
use crate::config::PersistentConfig;
use crate::error::Result;
use crate::path::expand_path;
use crate::remote;
use clap::{ArgAction, CommandFactory, Parser, Subcommand};
use github_proxy::Proxy;
use guess_target::Target;

#[derive(Debug, Clone, Subcommand)]
pub(crate) enum ConfigSubcommand {
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
pub(crate) enum Command {
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
    /// Upgrade ei to the latest version
    ///
    /// Keeps the build flavor this binary was compiled for (an
    /// `aarch64-linux-android` binary stays Android, a musl build stays musl).
    /// Use `--target` to override deliberately.
    Upgrade,
}

const VERSION: &str = const_str::concat!(
    env!("CARGO_PKG_VERSION"),
    " ",
    git_version::git_version!(fallback = "unknown"),
    " ",
    crate::BUILD_TARGET
);

#[derive(Parser, Debug, Clone)]
#[command(name = "ei", version = VERSION, about, long_about)]
pub struct Args {
    #[command(subcommand)]
    cmd: Option<Command>,

    /// GitHub repo (owner/repo), release URL, or artifact URL
    #[arg(default_value_t = String::new())]
    url: String,

    /// Destination (scp style). Either a local directory or
    /// `[user@]host[:port]:/path` to install on a remote machine over SSH.
    ///
    /// Examples:
    ///   ei ahaoboy/fsv user@192.168.1.100:/root/
    ///   ei ahaoboy/fsv ace@192.168.1.100:2222:/root/
    ///   ei ./app.tar.gz me@host:~/.ei
    #[arg(default_value_t = String::new())]
    dest: String,

    /// SSH port for remote installs (default: 22)
    ///
    /// Uppercase `-P`, like `scp`; lowercase `-p` is the password.
    #[arg(
        short = 'P',
        long,
        global = true,
        help = "SSH port for remote installs (note: -p is the password; use -P)"
    )]
    port: Option<u16>,

    /// SSH password for remote installs
    ///
    /// Cannot be combined with `-i`: use exactly one authentication method.
    #[arg(
        short = 'p',
        long,
        global = true,
        conflicts_with = "identity",
        help = "SSH password for remote installs (the port is -P)"
    )]
    password: Option<String>,

    /// SSH private key file for remote installs
    ///
    /// Cannot be combined with `-p`: use exactly one authentication method.
    #[arg(
        short = 'i',
        long,
        global = true,
        value_name = "FILE",
        conflicts_with = "password",
        help = "SSH private key file for remote installs"
    )]
    identity: Option<String>,

    /// Installation directory for downloaded binaries
    #[arg(
        short,
        long,
        help = "Installation directory (local path or [user@]host:/path)"
    )]
    dir: Option<String>,

    /// Skip adding the install directory to PATH
    ///
    /// Installed files stay usable through their full path.
    #[arg(
        long,
        global = true,
        default_value_t = false,
        action = ArgAction::SetTrue,
        help = "Skip adding the install directory to PATH"
    )]
    skip_path: bool,

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
    name: Vec<String>,

    /// Rename the installed binary
    #[arg(long, help = "Rename the installed binary")]
    alias: Option<String>,

    /// Target platform (e.g., x86_64-unknown-linux-gnu)
    ///
    /// Auto-detected from the local machine when omitted. Required for SSH
    /// remote installs (`user@host:/path`): the local platform says nothing
    /// about the remote host, so guessing would download the wrong binary.
    ///
    /// `ei upgrade` defaults to the target this binary was compiled for, and
    /// accepts this flag as an explicit override.
    #[arg(
        long,
        global = true,
        help = "Target platform (auto-detected locally if not specified; required for SSH installs)"
    )]
    target: Option<Target>,

    /// Number of retry attempts for failed downloads
    #[arg(
        long,
        global = true,
        default_value_t = 3,
        help = "Number of retry attempts"
    )]
    retry: usize,

    /// GitHub proxy to use (github, ghproxy, etc.)
    #[arg(long, global = true, help = "GitHub proxy to use")]
    proxy: Option<Proxy>,

    /// Network request timeout in seconds
    #[arg(long, global = true, help = "Network request timeout in seconds")]
    timeout: Option<u64>,

    /// Strip debug symbols from executable
    #[arg(
        long,
        global = true,
        help = "Strip debug symbols from executable",
        default_missing_value = "true",
        num_args = 0..=1,
    )]
    strip: Option<bool>,

    /// Compress executable with UPX
    #[arg(
        long,
        global = true,
        help = "Compress executable with UPX",
        default_missing_value = "true",
        num_args = 0..=1,
    )]
    upx: Option<bool>,

    /// Suppress all output messages
    #[arg(
        long,
        short,
        global = true,
        help = "Suppress all output messages",
        default_missing_value = "true",
        num_args = 0..=1,
        default_value_t = false
    )]
    quiet: bool,

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
    fuzzy: bool,

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
    regex: Option<String>,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            cmd: None,
            url: String::new(),
            dest: String::new(),
            port: None,
            password: None,
            identity: None,
            dir: None,
            skip_path: false,
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
        let persistent = PersistentConfig::load();

        let proxy = value.proxy.or(persistent.proxy).unwrap_or(Proxy::Github);
        let timeout = value.timeout.or(persistent.timeout).unwrap_or(600);

        let dir = value.dir.clone().or(persistent.dir);
        // scp-style destination positional: `ei <src> [user@]host:/path`
        let dir = if value.dest.trim().is_empty() {
            dir
        } else {
            Some(value.dest.clone())
        };

        let target = value.target.or(persistent.target);
        let strip = value.strip.or(persistent.strip).unwrap_or(false);
        let upx = value.upx.or(persistent.upx).unwrap_or(false);

        // A `user@host[:port]:/path` directory switches the install to SSH.
        let (dir, remote) = match dir {
            Some(spec) => match remote::parse_remote(&spec) {
                Some((mut remote, path)) => {
                    // `-P` overrides a port parsed from the destination.
                    remote.port = value.port.unwrap_or(remote.port);
                    remote.password = value.password.clone();
                    remote.identity = value.identity.clone();
                    let dir = if path.is_empty() { None } else { Some(path) };
                    (dir, Some(remote))
                }
                None => (Some(spec), None),
            },
            None => (None, None),
        };

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
            skip_path: value.skip_path,
            fuzzy: value.fuzzy,
            regex: value.regex,
            remote,
        }
    }
}

/// Run the CLI.
pub async fn run_main(args: Args) -> Result<()> {
    if let Some(Command::Completions { shell }) = args.cmd {
        return handle_completions_command(shell);
    }

    if matches!(&args.cmd, Some(Command::Upgrade)) {
        return handle_upgrade(args).await;
    }

    if let Some(Command::Config { subcmd }) = args.cmd {
        let quiet = args.quiet;
        return handle_config_command(subcmd, quiet);
    }

    if args.url.is_empty() {
        println!("{}", Args::command().render_help());
        return Ok(());
    }

    let url = args.url.clone();
    let config = args.into();
    crate::ei(&url, &config).await
}

fn handle_completions_command(shell: clap_complete::Shell) -> Result<()> {
    use clap_complete::generate;
    use std::io;

    let mut cmd = Args::command();
    let bin_name = cmd.get_name().to_string();
    generate(shell, &mut cmd, bin_name, &mut io::stdout());
    Ok(())
}

async fn handle_upgrade(args: Args) -> Result<()> {
    // `--target` given on the command line wins. The *persisted* `config
    // target` must not: it selects artifacts for normal installs and may name
    // a different ABI than this binary (e.g. musl on Termux), which would
    // replace a working binary with one whose DNS resolver hangs.
    let explicit_target = args.target;
    let mut config: InstallConfig = args.into();
    config.target = None;
    // Remote settings are not an upgrade concern: `upgrade` pins
    // `remote: None` (it can only replace the local running executable).

    let options = crate::upgrade::UpgradeOptions {
        target: explicit_target,
        ..crate::upgrade::UpgradeOptions::new("easy-install/easy-install", "ei", config)
    };
    crate::upgrade::upgrade(options).await?;
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
            )
        }
        ConfigSubcommand::Dir { value } => {
            let current = config.dir.as_deref().unwrap_or("not set").to_string();
            apply_config(
                &mut config,
                quiet,
                value,
                |c, v| {
                    // Keep `user@host:/path` specs intact for remote installs.
                    if remote::parse_remote(&v).is_some() {
                        c.set_dir(v);
                    } else {
                        c.set_dir(expand_path(&v));
                    }
                },
                |v: &String| v.to_string(),
                "Directory",
                current,
            )
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
            )
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
            )
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
            )
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
            )
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
            )
        }
    }
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
        let message = format!("{label} set to: {}", fmt_val(&v));
        setter(config, v);
        config.save_quiet(quiet)?;
        if !quiet {
            println!("{message}");
        }
    } else if !quiet {
        println!("Current {label}: {current}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parse args into an install config (every test supplies an explicit
    /// destination, which overrides any persistent config on disk).
    fn resolve(argv: &[&str]) -> InstallConfig {
        let args = Args::try_parse_from(argv).expect("args should parse");
        args.into()
    }

    #[test]
    fn scp_style_destination_is_remote() {
        let config = resolve(&[
            "ei",
            "-P",
            "2222",
            "./app.tar.gz",
            "user@192.168.1.100:/root/",
        ]);
        assert_eq!(config.dir.as_deref(), Some("/root/"));
        let remote = config.remote.expect("remote should be set");
        assert_eq!(remote.user, "user");
        assert_eq!(remote.host, "192.168.1.100");
        assert_eq!(remote.port, 2222);
    }

    #[test]
    fn dir_flag_accepts_remote_spec() {
        let config = resolve(&["ei", "ahaoboy/fsv", "-d", "me@host:~/.ei"]);
        assert_eq!(config.dir.as_deref(), Some("~/.ei"));
        assert_eq!(config.remote.expect("remote").host, "host");
    }

    #[test]
    fn password_and_key_are_forwarded() {
        let config = resolve(&["ei", "ahaoboy/fsv", "-p", "secret", "me@host:/opt"]);
        assert_eq!(
            config.remote.expect("remote").password.as_deref(),
            Some("secret")
        );

        let config = resolve(&[
            "ei",
            "ahaoboy/fsv",
            "-i",
            "~/.ssh/id_ed25519",
            "me@host:/opt",
        ]);
        assert_eq!(
            config.remote.expect("remote").identity.as_deref(),
            Some("~/.ssh/id_ed25519")
        );
    }

    #[test]
    fn local_destination_stays_local() {
        let config = resolve(&["ei", "./app.tar.gz", "/tmp/out"]);
        assert!(config.remote.is_none());
        assert_eq!(config.dir.as_deref(), Some("/tmp/out"));
    }

    /// PATH registration still happens by default; `--skip-path` disables it.
    #[test]
    fn skip_path_can_be_enabled() {
        assert!(!resolve(&["ei", "ahaoboy/fsv"]).skip_path);
        assert!(resolve(&["ei", "ahaoboy/fsv", "--skip-path"]).skip_path);
    }
}
