mod artifact;
mod file;
mod manfiest;
mod repo;

use crate::download::{download_dist_manfiest, read_dist_manfiest};
use crate::install::artifact::install_from_artifact_url;
use crate::install::file::install_from_single_file;
use crate::install::manfiest::install_from_manfiest;
use crate::install::repo::install_from_github;
use crate::tool::{
    get_filename, is_archive_file, is_dist_manfiest, is_exe_file, is_url, name_no_ext,
};
use crate::ty::{Output, Repo};
use artifact::install_from_download_file;
use guess_target::{get_local_target, guess_target};
use tracing::trace;

pub async fn install(url: &str, bin: &[String], dir: Option<String>) -> Output {
    trace!("install {}", url);
    let repo = Repo::try_from(url);

    if is_dist_manfiest(url) {
        if let Some(manfiest) = if is_url(url) {
            download_dist_manfiest(url).await
        } else {
            read_dist_manfiest(url)
        } {
            return install_from_manfiest(manfiest, dir, url, bin).await;
        }
    }
    let filename = get_filename(url);
    let name = name_no_ext(&filename);
    let guess = guess_target(&name);
    let local = get_local_target();
    let item = guess.iter().find(|i| local.contains(&i.target));
    let name = item.map_or(name, |i| i.name.clone());

    if is_url(url) {
        if is_archive_file(url) {
            return install_from_artifact_url(url, &name, dir).await;
        }

        if is_exe_file(url) {
            return install_from_single_file(url, &name, dir).await;
        }
    }

    if std::fs::exists(url).unwrap_or(false) && is_archive_file(url) {
        return install_from_download_file(url, &name, dir).await;
    }

    if let Ok(repo) = repo {
        return install_from_github(&repo, dir, bin).await;
    }

    Output::new()
}
