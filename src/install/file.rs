use crate::download::download_binary;
use crate::env::get_install_dir;
use crate::manfiest::DistManifest;
use crate::tool::{display_output, get_bin_name, get_meta, path_to_str};
use crate::ty::{Output, OutputFile, OutputItem};

pub async fn install_from_single_file(
    url: &str,
    manfiest: Option<DistManifest>,
    dir: Option<String>,
) -> Output {
    let mut install_dir = get_install_dir();
    let mut output = Output::new();
    if let Some(target_dir) = dir {
        if target_dir.contains("/") || target_dir.contains("\\") {
            install_dir = target_dir.into();
        } else {
            install_dir.push(target_dir);
        }
    }

    if let Some(bin) = download_binary(url).await {
        let artifact = manfiest.and_then(|i| i.get_artifact_by_key(url));

        let art_name = url
            .split("/")
            .last()
            .map(|i| i.to_string())
            .expect("can't get artifact name");
        let name = artifact.and_then(|i| i.name).unwrap_or(art_name);
        let mut install_path = install_dir.clone();
        install_path.push(get_bin_name(&name));

        if let Some(dir) = install_path.parent() {
            std::fs::create_dir_all(dir).expect("Failed to create_dir dir");
        }
        std::fs::write(&install_path, &bin).expect("write file failed");
        let (mode, size, is_dir) = get_meta(&install_path);
        let install_path = path_to_str(&install_path);
        println!("Installation Successful");
        let origin_path = url.split("/").last().unwrap_or(name.as_str()).to_string();

        let files = vec![OutputFile {
            mode,
            size,
            origin_path,
            is_dir,
            install_path,
        }];

        let bin_dir_str = path_to_str(&install_dir);
        let item = OutputItem {
            install_dir: bin_dir_str.clone(),
            bin_dir: bin_dir_str.clone(),
            files,
        };

        output.insert(url.to_string(), item);
        println!("{}", display_output(&output));
    } else {
        println!("not found/download artifact for {url}")
    }
    output
}
