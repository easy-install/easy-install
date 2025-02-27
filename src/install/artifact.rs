use crate::download::download_extract;
use crate::env::get_install_dir;
use crate::install::file::install_from_single_file;
use crate::tool::{
    display_output, get_common_prefix_len, install_output_files, is_archive_file, path_to_str,
};
use crate::ty::{Output, OutputFile, OutputItem};
use tracing::trace;

pub async fn install_from_download_file(url: &str, name: &str, dir: Option<String>) -> Output {
    trace!("install_from_download_file");
    let mut install_dir = get_install_dir();
    let mut v: OutputItem = Default::default();
    let mut files: Vec<OutputFile> = vec![];
    let mut output = Output::new();
    if let Some(target_dir) = dir.or(Some(path_to_str(&install_dir).to_string())) {
        if target_dir.contains("/") || target_dir.contains("\\") {
            install_dir = target_dir.into();
        } else {
            install_dir.push(target_dir);
        }

        let install_dir_str = path_to_str(&install_dir);
        v.install_dir = install_dir_str;

        if let Some(download_files) = download_extract(url).await {
            let file_list: Vec<_> = download_files.into_iter().filter(|i| !i.is_dir).collect();
            if file_list.len() > 1 {
                install_dir.push(name);
            }

            let prefix_len = get_common_prefix_len(
                file_list
                    .iter()
                    .map(|i| i.path.as_str())
                    .collect::<Vec<_>>()
                    .as_slice(),
            );

            for entry in file_list {
                let size = entry.buffer.len() as u32;
                let is_dir = entry.is_dir;
                if is_dir {
                    continue;
                }
                let mut dst = install_dir.clone();
                dst.push(&entry.path[prefix_len..]);
                files.push(OutputFile {
                    install_path: path_to_str(&dst),
                    mode: entry.mode,
                    size,
                    origin_path: entry.path,
                    is_dir,
                    buffer: entry.buffer,
                });
            }

            v.files = files;
            if !v.files.is_empty() {
                install_output_files(&v.files);
                println!("Installation Successful");
                output.insert(url.to_string(), v);
                println!("{}", display_output(&output));
            }
        }
    } else {
        println!("Maybe you should use -d to set the folder");
    }

    output
}

pub async fn install_from_artifact_url(
    art_url: &str,
    name: &str,
    dir: Option<String>,
) -> Output {
    trace!("install_from_artifact_url {}", art_url);
    let mut v = Output::new();
    println!("download {}", art_url);
    if !is_archive_file(art_url) {
        let output = install_from_single_file(art_url, name, dir.clone()).await;
        return output;
    }
    let output = install_from_download_file(art_url, name, dir.clone()).await;
    v.extend(output);
    v
}
