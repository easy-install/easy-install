use std::collections::HashMap;

use tokio::sync::OnceCell;

use crate::{
    download::{download_json, download_text},
    manfiest::DistManifest,
    ty::{Output, Repo},
};

use super::manfiest::install_from_manfiest;

const API: &str = "https://github.com/ahaoboy/easy-install/raw/refs/heads/main/builtin.json";

static ONCE: OnceCell<Option<HashMap<String, String>>> = OnceCell::const_new();

async fn download_builtin_json() -> Option<HashMap<String, String>> {
    download_json(API).await
}

async fn get_builtin() -> Option<HashMap<String, String>> {
    ONCE.get_or_init(download_builtin_json).await.clone()
}

pub async fn get_builtin_name(repo: &Repo) -> Option<String> {
    let builtin = get_builtin().await?;
    for (url, name) in builtin {
        if let Ok(item) = Repo::try_from(url.as_str()) {
            if item.name == repo.name && item.owner == repo.owner {
                return Some(name);
            }
        }
    }
    None
}

fn get_dist_url(name: &str) -> String {
    format!("https://github.com/ahaoboy/easy-install/raw/refs/heads/main/dist-manifest/{name}.json")
}

async fn get_dist(dist_url: &str, tag: Option<String>) -> Option<DistManifest> {
    let json = download_text(dist_url).await?;
    let tag = tag.unwrap_or("latest".to_string());
    if tag == "latest" {
        return serde_json::from_str(&json).ok();
    }
    let tag_json = json.replace(
        "/releases/latest/download/",
        &format!("/releases/download/{tag}/"),
    );
    serde_json::from_str(&tag_json).ok()
}

pub async fn builtin_install(repo: &Repo, name: String, install_dir: Option<String>) -> Output {
    let dist_url = get_dist_url(&name);
    if let Some(dist) = get_dist(&dist_url, repo.tag.clone()).await {
        return install_from_manfiest(dist, install_dir, &dist_url).await;
    }
    Default::default()
}

#[cfg(test)]
mod test {
    use crate::{
        install::builtin::{get_builtin_name, get_dist, get_dist_url},
        ty::Repo,
    };

    #[tokio::test]
    async fn test_name() {
        for (a, b) in [
            ("https://github.com/pnpm/pnpm", "pnpm"),
            ("https://github.com/pnpm/pnpm/releases/tag/v10.4.1", "pnpm"),
            ("https://github.com/pnpm/pnpm/releases", "pnpm"),
        ] {
            assert_eq!(
                get_builtin_name(&Repo::try_from(a).unwrap()).await.unwrap(),
                b
            );
        }
    }

    #[tokio::test]
    async fn test_get_dist() {
        let dist_url = get_dist_url("pnpm");
        let dist = get_dist(&dist_url, None).await.unwrap();
        assert!(dist
            .artifacts
            .contains_key("https://github.com/pnpm/pnpm/releases/latest/download/pnpm-linux-x64"));

        let repo = Repo::try_from("https://github.com/pnpm/pnpm/releases/tag/v9.15.3").unwrap();
        let dist9 = get_dist(&dist_url, repo.tag).await.unwrap();
        assert!(dist9
            .artifacts
            .contains_key("https://github.com/pnpm/pnpm/releases/download/v9.15.3/pnpm-linux-x64"));
    }
}
