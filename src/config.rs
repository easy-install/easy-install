use anyhow::{Context, Result};
use github_proxy::Proxy;
use guess_target::Target;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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

impl PersistentConfig {
    pub fn get_config_path() -> Result<PathBuf> {
        let exe_path = std::env::current_exe().context("Failed to get executable path")?;
        let exe_dir = exe_path
            .parent()
            .context("Failed to get executable directory")?;
        Ok(exe_dir.join("ei_config.json"))
    }

    pub fn load() -> Self {
        let config_path = match Self::get_config_path() {
            Ok(path) => path,
            Err(_) => return Self::default(),
        };

        if !config_path.exists() {
            return Self::default();
        }

        match std::fs::read_to_string(&config_path) {
            Ok(content) => match serde_json::from_str::<PersistentConfig>(&content) {
                Ok(config) => config,
                Err(e) => {
                    eprintln!("Warning: Failed to parse config file: {}", e);
                    eprintln!("Using default configuration and overwriting config file");
                    let default_config = Self::default();
                    let _ = default_config.save();
                    default_config
                }
            },
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) -> Result<()> {
        let config_path = Self::get_config_path()?;
        let content =
            serde_json::to_string_pretty(self).context("Failed to serialize configuration")?;
        std::fs::write(&config_path, content).context("Failed to write configuration file")?;
        println!("Configuration saved to: {}", config_path.display());
        Ok(())
    }

    pub fn save_quiet(&self, quiet: bool) -> Result<()> {
        let config_path = Self::get_config_path()?;
        let content =
            serde_json::to_string_pretty(self).context("Failed to serialize configuration")?;
        std::fs::write(&config_path, content).context("Failed to write configuration file")?;
        if !quiet {
            println!("Configuration saved to: {}", config_path.display());
        }
        Ok(())
    }

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
