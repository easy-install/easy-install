use crate::download::download_extract;
use crate::env::get_install_dir;
use crate::install::file::install_from_single_file;
use crate::manfiest::DistManifest;
use crate::tool::{
    display_output, get_artifact_download_url, get_bin_name, get_filename, get_meta,
    is_archive_file, path_to_str, write_to_file,
};
use crate::ty::{Output, OutputFile, OutputItem};
use detect_targets::detect_targets;
use tracing::trace;

pub async fn install_from_download_file(
    url: &str,
    manfiest: Option<DistManifest>,
    dir: Option<String>,
) -> Output {
    trace!("install_from_download_file");
    let mut install_dir = get_install_dir();
    let mut v: OutputItem = Default::default();
    let mut files: Vec<OutputFile> = vec![];
    let targets = detect_targets().await;
    let artifact = manfiest.and_then(|i| i.get_artifact(&targets));
    let mut output = Output::new();
    if let Some(asset) = artifact.clone().and_then(|a| a.get_assets_executable_dir()) {
        if let Some(target_dir) = dir.clone().or(asset.name) {
            if target_dir.contains("/") || target_dir.contains("\\") {
                install_dir = target_dir.into();
            } else {
                install_dir.push(target_dir);
            }

            let prefix = asset.path.unwrap_or("".to_string());

            let install_dir_str = path_to_str(&install_dir);

            let mut bin_dir = install_dir.clone();
            if let Some(ref dir) = asset.executable_dir {
                bin_dir.push(dir);
            }
            let bin_dir_str = path_to_str(&bin_dir);
            v.bin_dir = bin_dir_str;
            v.install_dir = install_dir_str;

            if let Some(download_files) = download_extract(url).await {
                for entry in download_files {
                    let size = entry.buffer.len() as u32;
                    let is_dir = entry.is_dir;
                    if is_dir {
                        continue;
                    }
                    let mut dst = install_dir.clone();
                    dst.push(entry.path.replace(&(prefix.clone() + "/"), ""));

                    // FIXME: remove same name file
                    // if let Some(dst_dir) = dst.parent() {
                    //     if dst_dir.exists() && dst_dir.is_file() {
                    //         std::fs::remove_file(dst_dir).unwrap_or_else(|_| {
                    //             panic!("failed to remove file : {:?}", dst_dir)
                    //         });
                    //         println!("remove {:?}", dst_dir);
                    //     }
                    //     if !dst_dir.exists() {
                    //         std::fs::create_dir_all(dst_dir)
                    //             .expect("Failed to create_dir install_dir");
                    //     }
                    // }

                    // atomic_install(&src, dst.as_path()).unwrap_or_else(|_| {
                    //     panic!("failed to atomic_install from {:?} to {:?}", src, dst)
                    // });
                    write_to_file(dst.to_string_lossy().as_ref(), &entry.buffer, entry.mode);
                    let mode = entry.mode.unwrap_or(get_meta(&dst).0);

                    files.push(OutputFile {
                        install_path: path_to_str(&dst),
                        mode,
                        size,
                        origin_path: entry.path,
                        is_dir,
                    });
                }

                v.files = files;
                if !v.files.is_empty() {
                    println!("Installation Successful");
                    output.insert(url.to_string(), v);
                    println!("{}", display_output(&output));
                }
            }
        } else {
            println!("Maybe you should use -d to set the folder");
        }
    } else {
        if let Some(ref target_dir) = dir {
            if target_dir.contains("/") || target_dir.contains("\\") {
                install_dir = target_dir.into();
            } else {
                install_dir.push(target_dir);
            }
        }
        let install_dir_str = path_to_str(&install_dir);

        v.bin_dir = install_dir_str.clone();
        v.install_dir = install_dir_str;

        let allow = |p: &str| -> bool {
            match artifact.clone() {
                None => true,
                Some(art) => art.has_file(p),
            }
        };
        if let Some(download_files) = download_extract(url).await {
            for entry in download_files {
                let size = entry.buffer.len() as u32;
                let is_dir = entry.is_dir;
                if is_dir || !allow(&entry.path) {
                    continue;
                }

                let mut dst = install_dir.clone();

                let file_name = get_filename(&entry.path).expect("failed to get filename");
                let name = artifact
                    .clone()
                    .and_then(|a| a.get_asset(&entry.path).and_then(|i| i.executable_name))
                    .unwrap_or(file_name.clone());

                dst.push(get_bin_name(&name));
                write_to_file(dst.to_string_lossy().as_ref(), &entry.buffer, entry.mode);
                let mode = entry.mode.unwrap_or(get_meta(&dst).0);
                files.push(OutputFile {
                    install_path: path_to_str(&dst),
                    mode,
                    size,
                    origin_path: entry.path,
                    is_dir,
                });
            }
            v.files = files;
            if !v.files.is_empty() {
                println!("Installation Successful");
                output.insert(url.to_string(), v);
                println!("{}", display_output(&output));
            }
        }
    }

    output
}

pub async fn install_from_artifact_url(
    art_url: &str,
    manfiest: Option<DistManifest>,
    dir: Option<String>,
) -> Output {
    trace!("install_from_artifact_url {}", art_url);
    let urls = get_artifact_download_url(art_url).await;
    let mut v = Output::new();
    if urls.is_empty() {
        println!("not found download_url for {art_url}");
        return v;
    }
    if urls.len() == 1 && !is_archive_file(&urls[0]) {
        println!("download {}", urls[0]);
        let output = install_from_single_file(&urls[0], manfiest.clone(), dir.clone()).await;
        return output;
    }
    for url in urls {
        println!("download {}", url);
        let output = install_from_download_file(&url, manfiest.clone(), dir.clone()).await;
        // println!("{}", display_output(&output));
        v.extend(output);
    }
    v
}
