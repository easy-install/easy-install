use crate::download::create_client;
use crate::install::repo::install_from_github;
use crate::ty::{Output, Repo};
use binstalk_registry::Registry;
use semver::VersionReq;
use tracing::trace;

pub async fn install_from_crate_name(crate_name: &str, dir: Option<String>) -> Output {
    trace!("install_from_crate_name {}", crate_name);
    let client = create_client().await;
    let version_req = &VersionReq::STAR;
    let sparse_registry: Registry = Registry::crates_io_sparse_registry();
    let mut v = Output::new();
    if let Ok(manifest_from_sparse) = sparse_registry
        .fetch_crate_matched(client, crate_name, version_req)
        .await
    {
        if let Some(pkg) = manifest_from_sparse.package {
            if let Some(repository) = pkg.repository() {
                if let Ok(repo) = Repo::try_from(repository) {
                    v.extend(install_from_github(&repo, dir).await);
                }
            }
        }
    }
    v
}
