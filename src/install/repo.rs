use crate::install::artifact::install_from_artifact_url;
use crate::ty::{Output, Repo};
use detect_targets::detect_targets;
use tracing::trace;

pub async fn install_from_github(repo: &Repo, dir: Option<String>) -> Output {
    trace!("install_from_git {}", repo);
    let artifact_url = repo.get_artifact_url(detect_targets().await).await;
    let mut v = Output::new();
    if !artifact_url.is_empty() {
        for i in artifact_url {
            trace!("install_from_git artifact_url {}", i);
            let manfiest = repo.get_manfiest().await;
            v.extend(install_from_artifact_url(&i, manfiest, dir.clone()).await);
        }
    } else {
        println!(
            "not found asset for {} on {}",
            detect_targets().await.join(","),
            repo.get_gh_url()
        );
    }
    v
}
