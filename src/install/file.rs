use crate::InstallConfig;
use crate::download::download_binary;
use crate::env::get_install_dir;
use crate::tool::{
    display_output, ends_with_exe, expand_path, get_bin_name, get_filename, install_output_files, path_to_str
};
use crate::types::{Output, OutputFile, OutputItem};
use anyhow::Result;
use guess_target::{Os, get_local_target};

pub(crate) async fn install_from_single_file(
    url: &str,
    name: &str,
    config: &InstallConfig,
) -> Result<Output> {
    let mut install_dir = get_install_dir()?;
    let mut output = Output::new();

    if let Some(target_dir) = &config.dir {
        if target_dir.contains("/") || target_dir.contains("\\") {
            install_dir = expand_path(&target_dir).into();
        } else {
            install_dir.push(target_dir);
        }
    }

    let local_target = get_local_target();
    if ends_with_exe(url) && local_target.iter().any(|t| t.os() != Os::Windows) {
        return Ok(output);
    }
    let filename = get_filename(url);
    let bin = if std::fs::exists(url).unwrap_or(false) {
        Some(std::fs::read(url)?)
    } else {
        Some(download_binary(url, config.retry, config.timeout).await?)
    };
    if let Some(bin) = bin {
        let mut install_path = install_dir.clone();
        let target_name = get_bin_name(name);
        install_path.push(target_name);
        let install_path = path_to_str(&install_path);
        let mut files = vec![OutputFile {
            mode: None,
            size: bin.len() as u32,
            origin_path: filename,
            is_dir: false,
            install_path,
            buffer: bin,
        }];
        install_output_files(&mut files, config.alias.clone())?;
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
    Ok(output)
}
