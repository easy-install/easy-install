use crate::InstallConfig;
use crate::install::artifact::install_from_artifact_url;
use crate::install::manfiest::install_from_manfiest;
use crate::ty::{Output, Repo};
use anyhow::Result;
use tracing::trace;

pub(crate) async fn install_from_github(repo: &Repo, config: &InstallConfig) -> Result<Output> {
    trace!("install_from_git {}", repo);
    if let Ok(man) = repo.get_manfiest().await {
        return install_from_manfiest(man, &repo.get_manfiest_url(), config).await;
    }

    let artifact_url = repo.get_artifact_url().await?;
    let mut v = Output::new();
    if !artifact_url.is_empty() {
        for (name, i) in artifact_url {
            trace!("install_from_git artifact_url {}", i);
            if !config.name.is_empty() && !config.name.contains(&name) {
                continue;
            }
            v.extend(install_from_artifact_url(&i, &name, config).await?);
        }
    } else {
        println!(
            "not found asset for os:{} arch:{} on {}",
            std::env::consts::OS,
            std::env::consts::ARCH,
            repo.get_gh_url()
        );
    }
    Ok(v)
}
