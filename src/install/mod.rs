mod artifact;
mod builtin;
mod crateio;
mod file;
mod manfiest;
mod repo;

use crate::download::{download_dist_manfiest, read_dist_manfiest};
use crate::install::artifact::install_from_artifact_url;
use crate::install::crateio::install_from_crate_name;
use crate::install::file::install_from_single_file;
use crate::install::manfiest::install_from_manfiest;
use crate::install::repo::install_from_github;
use crate::tool::{is_archive_file, is_dist_manfiest, is_exe_file, is_url};
use crate::ty::{Output, Repo};
use builtin::{builtin_install, get_builtin_name};
use tracing::trace;

pub async fn install(url: &str, dir: Option<String>) -> Output {
    trace!("install {}", url);
    let repo = Repo::try_from(url);

    if let Ok(repo) = &repo {
        if let Some(name) = get_builtin_name(repo).await {
            return builtin_install(repo, name, dir).await;
        }
    }

    if is_dist_manfiest(url) {
        if let Some(manfiest) = if is_url(url) {
            download_dist_manfiest(url).await
        } else {
            read_dist_manfiest(url)
        } {
            return install_from_manfiest(manfiest, dir, url).await;
        }
    }
    if is_url(url) {
        if is_archive_file(url) {
            return install_from_artifact_url(url, None, dir).await;
        }

        if is_exe_file(url) {
            return install_from_single_file(url, None, dir).await;
        }
    }

    if let Ok(repo) = repo {
        return install_from_github(&repo, dir).await;
    }
    install_from_crate_name(url, dir).await
}
