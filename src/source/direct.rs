//! Direct URLs, local files and the catch-all fallback.

use crate::InstallConfig;
use crate::artifact::{Artifact, Kind};
use crate::error::Result;
use crate::path::{is_archive_file, is_exe_file, is_known_format};
use crate::source::{Input, Source};
use async_trait::async_trait;
use github_proxy::fetch::Url;

/// A URL or local path that already names an installable file.
///
/// Inputs of unknown format (a repository URL, for example) are left to the
/// later sources.
pub(crate) struct Direct;

/// Accepts whatever no other source claimed and installs it as a single file.
pub(crate) struct Fallback;

#[async_trait]
impl Source for Direct {
    fn name(&self) -> &'static str {
        "direct"
    }

    fn matches(&self, input: &Input<'_>) -> bool {
        let known = Url::is_valid(input.raw)
            && (is_archive_file(&input.proxied)
                || is_exe_file(&input.proxied)
                || is_known_format(&input.proxied));

        known || std::fs::exists(input.raw).unwrap_or(false)
    }

    async fn resolve(&self, input: &Input<'_>, _config: &InstallConfig) -> Result<Vec<Artifact>> {
        // Proxies only apply to remote URLs.
        let url = if Url::is_valid(input.raw) {
            input.proxied.as_str()
        } else {
            input.raw
        };

        // Archives are extracted at the destination; everything else is
        // written as a single file under its own file name.
        Ok(vec![if is_archive_file(url) {
            Artifact::new(input.name.clone(), url, Kind::Auto)
        } else {
            Artifact::new(input.filename.clone(), url, Kind::Single)
        }])
    }
}

#[async_trait]
impl Source for Fallback {
    fn name(&self) -> &'static str {
        "fallback"
    }

    fn matches(&self, _input: &Input<'_>) -> bool {
        true
    }

    async fn resolve(&self, input: &Input<'_>, _config: &InstallConfig) -> Result<Vec<Artifact>> {
        Ok(vec![Artifact::new(
            input.name.clone(),
            input.raw.to_string(),
            Kind::Single,
        )])
    }
}
