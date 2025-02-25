use crate::download::download_binary;
use crate::env::get_install_dir;
use crate::manfiest::DistManifest;
use crate::tool::{
    display_output, get_bin_name, get_filename, get_meta, install_output_files, path_to_str,
};
use crate::ty::{Output, OutputFile, OutputItem};
use guess_target::{get_local_target, guess_target};

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

    let local_target = get_local_target();

    if let Some(bin) = download_binary(url).await {
        let artifact = manfiest.and_then(|i| i.get_artifact_by_key(url));
        let filename = get_filename(url);

        let guess = guess_target(&filename);

        let art_name = guess
            .iter()
            .find(|i| local_target.contains(&i.target))
            .map_or(filename.clone(), |i| i.name.clone());

        let name = artifact.and_then(|i| i.name).unwrap_or(art_name);
        let mut install_path = install_dir.clone();
        install_path.push(get_bin_name(&name));
        // println!("install_dir {:?} {}", install_dir, name);
        let (mode, size, is_dir) = get_meta(&install_path);
        let install_path = path_to_str(&install_path);
        let files = vec![OutputFile {
            mode: Some(mode),
            size,
            origin_path: filename,
            is_dir,
            install_path,
            buffer: bin,
        }];
        install_output_files(&files);
        println!("Installation Successful");
        let bin_dir_str = path_to_str(&install_dir);
        let item = OutputItem {
            install_dir: bin_dir_str.clone(),
            files,
        };

        output.insert(url.to_string(), item);
        println!("{}", display_output(&output));
    } else {
        println!("not found/download artifact for {url}")
    }
    output
}
