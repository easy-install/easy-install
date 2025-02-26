use crate::download::download_binary;
use crate::env::get_install_dir;
use crate::tool::{
    display_output, ends_with_exe, get_bin_name, get_filename, install_output_files, path_to_str,
};
use crate::ty::{Output, OutputFile, OutputItem};
use guess_target::{get_local_target, Os};

pub async fn install_from_single_file(url: &str, name: &str, dir: Option<String>) -> Output {
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
    if ends_with_exe(url) && local_target.iter().any(|t| t.os() != Os::Windows) {
        return output;
    }
    let filename = get_filename(url);
    if let Some(bin) = download_binary(url).await {
        let mut install_path = install_dir.clone();
        install_path.push(get_bin_name(name));
        let install_path = path_to_str(&install_path);
        let files = vec![OutputFile {
            mode: None,
            size: bin.len() as u32,
            origin_path: filename,
            is_dir: false,
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
