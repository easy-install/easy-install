use crate::error::{Context, Result};
use crate::remote::RemoteConfig;
use github_proxy::Proxy;
use guess_target::Target;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Everything one install needs: where to install, what to select, and how to
/// fetch it.
///
/// This is the resolved form of the CLI arguments, with the persistent
/// configuration file already merged in.
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
    /// Whether adding the install directory to `PATH` is skipped.
    pub skip_path: bool,
    pub fuzzy: bool,
    pub regex: Option<String>,
    /// When set, install to this SSH host instead of the local filesystem.
    pub remote: Option<RemoteConfig>,
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
            skip_path: false,
            fuzzy: false,
            regex: None,
            remote: None,
        }
    }
}

impl InstallConfig {
    /// Load configuration from the persistent config file, falling back to
    /// defaults for everything it does not set.
    pub fn load() -> Self {
        let persistent = PersistentConfig::load();

        Self {
            dir: persistent.dir,
            name: Vec::new(),
            alias: None,
            target: persistent.target,
            retry: persistent.retry.unwrap_or(3) as usize,
            proxy: persistent.proxy.unwrap_or(Proxy::Github),
            timeout: persistent.timeout.unwrap_or(600),
            strip: persistent.strip.unwrap_or(false),
            upx: persistent.upx.unwrap_or(false),
            quiet: false,
            skip_path: false,
            fuzzy: false,
            regex: None,
            remote: None,
        }
    }

    /// Target triples to match artifacts against.
    ///
    /// An explicit [`Self::target`] wins; otherwise the compile-time target of
    /// this binary. Remote installs fill [`Self::target`] with the probed
    /// remote platform before matching.
    pub fn targets(&self) -> Vec<Target> {
        match self.target {
            Some(target) => vec![target],
            None => guess_target::get_local_target().to_vec(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PersistentConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proxy: Option<Proxy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dir: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<Target>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upx: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strip: Option<bool>,
}

pub const DEFAULT_CONFIG_NAME: &str = "ei_config.json";
pub const DEFAULT_CONFIG_DIR: &str = ".ei";

fn get_config_path() -> Result<PathBuf> {
    let exe_path = std::env::current_exe().context("Failed to get executable path")?;
    let exe_dir = exe_path
        .parent()
        .context("Failed to get executable directory")?;
    Ok(exe_dir.join(DEFAULT_CONFIG_NAME))
}

fn get_default_config_path() -> Result<PathBuf> {
    let mut home = dirs::home_dir().context("Failed to get home dir")?;
    home.push(DEFAULT_CONFIG_DIR);
    home.push(DEFAULT_CONFIG_NAME);
    Ok(home)
}

fn read_config(config_path: &PathBuf) -> Result<PersistentConfig> {
    let content = std::fs::read_to_string(config_path)?;
    let c = serde_json::from_str::<PersistentConfig>(&content)?;
    Ok(c)
}

impl PersistentConfig {
    pub fn load() -> Self {
        for get_path in [get_config_path, get_default_config_path] {
            if let Ok(config_path) = get_path()
                && config_path.exists()
                && let Ok(c) = read_config(&config_path)
            {
                return c;
            };
        }

        let default_config = Self::default();
        let _ = default_config.save_quiet(true);
        default_config
    }

    pub fn save_quiet(&self, quiet: bool) -> Result<()> {
        let config_path = get_config_path()?;
        let content =
            serde_json::to_string_pretty(self).context("Failed to serialize configuration")?;
        std::fs::write(&config_path, content).context("Failed to write configuration file")?;
        if !quiet {
            println!("Configuration saved to: {}", config_path.display());
        }
        Ok(())
    }
}

/// Edits driven by `ei config`; embedders build [`PersistentConfig`] directly.
#[cfg(feature = "cli")]
impl PersistentConfig {
    pub fn set_proxy(&mut self, proxy: Proxy) {
        self.proxy = Some(proxy);
    }

    pub fn set_dir(&mut self, dir: String) {
        self.dir = Some(dir);
    }

    pub fn set_target(&mut self, target: Target) {
        self.target = Some(target);
    }

    pub fn set_timeout(&mut self, timeout: u64) {
        self.timeout = Some(timeout);
    }
    pub fn set_retry(&mut self, retry: u64) {
        self.retry = Some(retry);
    }
    pub fn set_upx(&mut self, upx: bool) {
        self.upx = Some(upx);
    }
    pub fn set_strip(&mut self, strip: bool) {
        self.strip = Some(strip);
    }
    pub fn display(&self) {
        println!("{}", serde_json::to_string_pretty(self).unwrap_or_default())
    }
}
