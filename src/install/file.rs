use crate::InstallConfig;
use crate::backend::{Backend, Host};
use crate::download::download_binary;
use crate::error::Result;
use crate::tool::{
    check_disk_space, display_output, ends_with_exe, get_bin_name, get_filename,
    install_output_files, path_to_str,
};
use crate::types::{Output, OutputFile, OutputItem};
use guess_target::Os;
use std::path::PathBuf;

pub(crate) async fn install_from_single_file(
    backend: &Backend,
    url: &str,
    name: &str,
    config: &InstallConfig,
) -> Result<Output> {
    let local_target = config.targets();
    if ends_with_exe(url) && local_target.iter().any(|t| t.os() != Os::Windows) {
        return Ok(Output::new());
    }
    let filename = get_filename(url);
    // Callers pass the full source filename (e.g. `cli.ts`, `run.sh`, `bun`),
    // so known extensions are preserved. get_bin_name only appends a platform
    // extension (.exe) to bare names.
    let bin = if std::fs::exists(url).unwrap_or(false) {
        std::fs::read(url)?
    } else {
        download_binary(url, config.retry, config.timeout).await?
    };
    install_from_buffer(backend, bin, &filename, name, url, config).await
}

/// Install an already-downloaded (or decompressed) in-memory file.
///
/// * `origin_path` - the source file name (used for executable detection), e.g. `tool.exe`.
/// * `name` - the base name used for the installed file (platform extension applied).
/// * `key` - the key stored in the returned `Output` map (typically the source URL).
pub(crate) async fn install_from_buffer(
    backend: &Backend,
    buffer: Vec<u8>,
    origin_path: &str,
    name: &str,
    key: &str,
    config: &InstallConfig,
) -> Result<Output> {
    let mut install_dir = PathBuf::from(backend.home_dir());
    let mut output = Output::new();

    if let Some(target_dir) = &config.dir {
        if target_dir.contains("/") || target_dir.contains("\\") {
            install_dir = backend.expand(target_dir).into();
        } else {
            install_dir.push(target_dir);
        }
    }

    let mut install_path = install_dir.clone();
    install_path.push(get_bin_name(name));
    let install_path = path_to_str(&install_path);
    let size = buffer.len() as u64;
    let mut files = vec![OutputFile {
        mode: None,
        size,
        origin_path: origin_path.to_string(),
        is_dir: false,
        install_path,
        buffer,
    }];
    backend.mkdir(&path_to_str(&install_dir)).await?;
    check_disk_space(backend, &files, &install_dir).await?;
    install_output_files(backend, &mut files, config).await?;
    let item = OutputItem {
        install_dir: path_to_str(&install_dir),
        files,
    };
    output.insert(key.to_string(), item);

    if !config.quiet {
        println!("{}", display_output(&output, config, backend));
    }
    Ok(output)
}
