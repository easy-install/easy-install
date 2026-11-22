//! `ei` — install a binary from a GitHub release, a URL or a local archive.
//!
//! The pipeline is split into small modules:
//!
//! * `source` normalizes every accepted input (repository, release page,
//!   direct URL, local file, dist-manifest, CI artifact, nightly.link) into a
//!   list of installable artifacts. Each input kind is a `source::Source`
//!   implementation listed in `source::SOURCES`, so a new source is one module
//!   plus one registry entry.
//! * `select` applies `--regex` / `--name` / `--target` and picks the assets
//!   for this platform.
//! * `tree` decodes archives (including nested ones) into a relative file
//!   tree, then `--alias` and executable bits are applied to it.
//! * `install` plans the write (including disk space), writes through the
//!   `backend` (local disk or SSH) and runs `strip` / `upx`.
//! * `report` renders the summary and registers `$PATH` entries.
//!
//! The public surface is [`ei`], [`InstallConfig`], the [`upgrade`] flow, the
//! [`error`] type and the remote helpers. The `cli` feature (enabled by
//! default) adds the `Args` / `run_main` entry points, `archive` selects the
//! supported archive formats and `ssh` the remote backend.

mod artifact;
mod backend;
#[cfg(feature = "cli")]
mod cli;
mod config;
mod download;
mod env;
pub mod error;
mod install;
mod manifest;
mod optimize;
mod path;
mod remote;
mod report;
mod select;
mod source;
mod tree;
pub mod upgrade;

use crate::backend::{Backend, Host};
use crate::error::Result;

#[cfg(feature = "cli")]
pub use cli::{Args, run_main};
pub use config::InstallConfig;
pub use remote::{
    PROXIES, RemoteConfig, RemotePlatform, SUPPORTED_TARGETS, default_target, is_supported_target,
};

/// Target triple this binary was compiled for.
pub(crate) const BUILD_TARGET: &str = env!("BUILD_TARGET");

/// Install whatever `url` refers to.
pub async fn ei(url: &str, config: &InstallConfig) -> Result<()> {
    let mut config = config.clone();
    let backend = Backend::connect(&config).await?;

    if backend.is_remote() && !config.quiet {
        println!("Installing to {} over SSH", backend.label());
    }

    remote::resolve_target(url, &mut config, &backend).await?;

    let report = install::install(url, &config, &backend).await?;
    if !config.skip_path {
        report::add_to_path(&report, &config, &backend).await;
    }
    if report.is_empty() && !config.quiet {
        println!("No file installed from {url}");
    }
    Ok(())
}
