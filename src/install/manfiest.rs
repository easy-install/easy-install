use crate::install::artifact::install_from_artifact_url;
use crate::manfiest::DistManifest;
use crate::tool::get_artifact_url_from_manfiest;
use crate::ty::Output;
use tracing::trace;

pub async fn install_from_manfiest(
    manfiest: DistManifest,
    dir: Option<String>,
    url: &str,
) -> Output {
    trace!("install_from_manfiest {}", url);
    let mut v: std::collections::HashMap<String, crate::ty::OutputItem> = Output::new();
    let art_url_list = get_artifact_url_from_manfiest(url, &manfiest).await;
    if art_url_list.is_empty() {
        println!("install_from_manfiest {} failed", url);
        return v;
    }
    for art_url in art_url_list {
        trace!("install_from_manfiest art_url {}", art_url);
        v.extend(install_from_artifact_url(&art_url, Some(manfiest.clone()), dir.clone()).await);
    }
    v
}
