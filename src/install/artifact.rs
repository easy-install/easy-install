use crate::download::download_extract;
use crate::env::get_install_dir;
use crate::install::file::install_from_single_file;
use crate::manfiest::DistManifest;
use crate::rule::match_name;
use crate::tool::{
    display_output, get_artifact_download_url, get_common_prefix_len, get_filename,
    install_output_files, is_archive_file, name_no_ext, path_to_str,
};
use crate::ty::{Output, OutputFile, OutputItem};
use is_musl::is_musl;
use tracing::trace;

pub async fn install_from_download_file(url: &str, dir: Option<String>) -> Output {
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
                let os = std::env::consts::OS;
                let arch = std::env::consts::ARCH;
                let musl = is_musl();
                let filename = get_filename(url);
                let name =
                    match_name(&filename, None, os, arch, musl).unwrap_or(name_no_ext(&filename));
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
        let output = install_from_download_file(&url, dir.clone()).await;
        v.extend(output);
    }
    v
}
