use crate::InstallConfig;
use crate::download::{extract_bytes, get_bytes};
use crate::env::get_install_dir;
use crate::install::file::install_from_single_file;
use crate::tool::{
    check_disk_space, display_output, expand_path, get_common_prefix_len, get_filename,
    install_output_files, is_archive_file, name_no_ext, path_to_str,
};
use crate::types::{Output, OutputFile, OutputItem};
use anyhow::{Context, Result};
use easy_archive::Fmt;
use guess_target::guess_target;
use tracing::trace;

pub(crate) fn install_from_download_file(
    bytes: Vec<u8>,
    fmt: Fmt,
    url: &str,
    name: &str,
    config: &InstallConfig,
) -> Result<Output> {
    trace!("install_from_download_file name={}", name);
    let mut install_dir = get_install_dir()?;
    let mut v: OutputItem = Default::default();
    let mut files: Vec<OutputFile> = vec![];
    let mut output = Output::new();
    if let Some(target_dir) = config
        .dir
        .clone()
        .or(Some(path_to_str(&install_dir).to_string()))
    {
        if target_dir.contains("/") || target_dir.contains("\\") {
            install_dir = expand_path(&target_dir).into();
        } else {
            install_dir.push(target_dir);
        }

        if let Ok(download_files) = extract_bytes(bytes, fmt) {
            // Handle nested archive: if there's only one file and it's an archive,
            // extract it recursively and use the inner archive name for platform/name inference
            if let &[first] = &download_files.as_slice()
                && let Some(inner_fmt) = Fmt::guess(&first.path)
            {
                let inner_filename = get_filename(&first.path);
                // Extract tool name from inner archive (e.g., "bloaty" from "bloaty-x86_64-pc-windows-gnu.tar.gz")
                let inner_name_no_ext = name_no_ext(&inner_filename);
                let inner_name = guess_target(&inner_name_no_ext)
                    .pop()
                    .map_or(inner_name_no_ext.clone(), |i| i.name);
                println!(
                    "detected nested archive: outer={}, inner={}, tool_name={}",
                    name, inner_filename, inner_name
                );
                trace!(
                    "detected nested archive: outer={}, inner={}, tool_name={}",
                    name, inner_filename, inner_name
                );
                return install_from_download_file(
                    first.buffer.clone(),
                    inner_fmt,
                    url,
                    &inner_name,
                    config,
                );
            }
            let file_list: Vec<_> = download_files.into_iter().filter(|i| !i.is_dir).collect();
            if file_list.len() > 1 {
                if let Some(alias) = &config.alias {
                    install_dir.push(alias);
                } else {
                    install_dir.push(name);
                }
            }
            let install_dir_str = path_to_str(&install_dir);
            v.install_dir = install_dir_str;

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
                check_disk_space(&v.files, &install_dir)?;
                install_output_files(&mut v.files, config)?;
                output.insert(url.to_string(), v);
                if !config.quiet {
                    println!("{}", display_output(&output, config));
                }
            }
        }
    } else if !config.quiet {
        println!("Maybe you should use -d to set the folder");
    }

    Ok(output)
}

pub(crate) async fn install_from_artifact_url(
    art_url: &str,
    name: &str,
    config: &InstallConfig,
) -> Result<Output> {
    trace!("install_from_artifact_url {}", art_url);
    let mut v = Output::new();
    if !config.quiet {
        println!("download {art_url}");
    }
    if !is_archive_file(art_url) {
        let output = install_from_single_file(art_url, name, config).await?;
        return Ok(output);
    }

    let bytes = get_bytes(art_url, config.retry, config.timeout).await?;
    let fmt = Fmt::guess(art_url).context("fmt guess error")?;
    let output = install_from_download_file(bytes, fmt, art_url, name, config)?;
    v.extend(output);
    Ok(v)
}
