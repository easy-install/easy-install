//! The installable file tree.
//!
//! Decoding an archive produces a [`FileTree`]: file paths relative to the
//! tree root, independent of the machine that will receive them. `--alias`
//! and executable-bit inference are applied here, while everything is still in
//! memory, so the local and SSH writers share a single code path.

use crate::error::{Result, bail};
use crate::path::{executable, get_filename, is_executable, maybe_executable, name_no_ext};
use easy_archive::{Fmt, clean};
use guess_target::guess_target;
use tracing::trace;

/// One file inside a [`FileTree`].
#[derive(Debug, Clone)]
pub(crate) struct TreeFile {
    /// Path relative to the tree root (`/`-separated, never absolute).
    pub(crate) path: String,
    /// Original path inside the archive; used to classify the entry.
    pub(crate) origin_path: String,
    /// Unix mode recorded by the archive, when it has one.
    pub(crate) mode: Option<u32>,
    pub(crate) data: Vec<u8>,
}

impl TreeFile {
    pub(crate) fn size(&self) -> u64 {
        self.data.len() as u64
    }
}

/// A decoded archive, ready to be written.
#[derive(Debug)]
pub(crate) struct FileTree {
    pub(crate) files: Vec<TreeFile>,
    /// Tool name used to name the install directory of multi-file trees.
    pub(crate) name: String,
}

/// Decode `bytes` into a relative [`FileTree`].
///
/// An archive whose only entry is another archive (for example
/// `tool.tar.gz` holding `tool.tar.zst`) is unwrapped recursively; the inner
/// name then names the install directory.
pub(crate) fn decode(bytes: Vec<u8>, fmt: Fmt, name: &str) -> Result<FileTree> {
    let mut entries = extract(bytes, fmt)?;
    check_paths(entries.iter().map(|entry| entry.path.as_str()))?;

    // Single-file formats such as plain gzip may omit the original file name
    // from their header, leaving an empty path. Use the caller-supplied name
    // (derived from the URL) instead.
    for entry in &mut entries {
        if entry.path.is_empty() && !entry.is_dir {
            entry.path = name.to_string();
        }
    }

    if let [first] = entries.as_slice()
        && let Some(inner_fmt) = Fmt::guess(&first.path)
    {
        let inner = name_no_ext(&get_filename(&first.path));
        let inner_name = guess_target(&inner).pop().map_or(inner, |i| i.name);
        trace!(
            "nested archive: outer={name}, inner={}, tool={inner_name}",
            first.path
        );
        return decode(first.buffer.clone(), inner_fmt, &inner_name);
    }

    let files: Vec<_> = entries.into_iter().filter(|entry| !entry.is_dir).collect();
    let paths: Vec<&str> = files.iter().map(|file| file.path.as_str()).collect();
    let prefix_len = get_common_prefix_len(&paths);

    Ok(FileTree {
        files: files
            .into_iter()
            .map(|entry| TreeFile {
                path: entry.path[prefix_len..].to_string(),
                origin_path: entry.path,
                mode: entry.mode,
                data: entry.buffer,
            })
            .collect(),
        name: name.to_string(),
    })
}

/// Decode an archive and drop the macOS resource-fork mirror, which must never
/// be installed.
fn extract(bytes: Vec<u8>, fmt: Fmt) -> Result<Vec<easy_archive::File>> {
    Ok(fmt
        .decode(bytes)?
        .into_iter()
        .filter(|file| !file.path.starts_with("__MACOSX"))
        .collect())
}

/// Reject archive entries that escape the install directory.
///
/// Archives are untrusted input: a `..` segment would make the install write
/// outside the target directory, so the whole archive is refused before
/// anything is written.
fn check_paths<'a>(paths: impl IntoIterator<Item = &'a str>) -> Result<()> {
    for path in paths {
        if path.split(['/', '\\']).any(|segment| segment == "..") {
            bail!(
                "refusing to install '{path}': the archive contains a path that escapes the install directory"
            );
        }
    }
    Ok(())
}

/// Byte length of the directory prefix shared by every path in `list`,
/// including the trailing separator.
fn get_common_prefix_len(list: &[&str]) -> usize {
    if list.is_empty() {
        return 0;
    }

    if list.len() == 1 {
        return match list[0].rfind('/') {
            Some(i) => i + 1,
            None => 0,
        };
    }

    let parts: Vec<Vec<&str>> = list.iter().map(|i| i.split('/').collect()).collect();
    let max_len = parts.iter().map(|p| p.len()).max().unwrap_or(0);

    let mut prefix = 0;
    while prefix < max_len {
        let first = parts[0].get(prefix).unwrap_or(&"");
        if parts.iter().any(|p| p.get(prefix).unwrap_or(&"") != first) {
            break;
        }
        prefix += 1;
    }

    if prefix == 0 {
        return 0;
    }
    parts[0][..prefix].join("/").len() + 1
}

/// Rename the installed file (or the single executable of a multi-file tree)
/// to `alias`, keeping its directory and extension.
pub(crate) fn rename_alias(files: &mut [TreeFile], alias: &str) {
    let target = if files.len() == 1 {
        Some(&mut files[0])
    } else {
        let mut executables = files
            .iter_mut()
            .filter(|file| executable(&get_filename(&file.origin_path), &file.mode));
        let first = executables.next();
        if first.is_some() && executables.next().is_none() {
            first
        } else {
            None
        }
    };

    let Some(file) = target else { return };

    let (dir, filename) = split_dir(&file.path);
    let bin = name_no_ext(filename);
    let renamed = filename.replace(&bin, alias);
    file.path = match dir {
        "" => clean(&renamed),
        dir => clean(&format!("{dir}{renamed}")),
    };
}

/// Split `path` into a directory part (with trailing `/`) and a file name.
fn split_dir(path: &str) -> (&str, &str) {
    match path.rfind('/') {
        Some(i) => (&path[..=i], &path[i + 1..]),
        None => ("", path),
    }
}

/// If the archive recorded no execute bit anywhere, mark the single plausible
/// executable candidate as executable.
pub(crate) fn guess_executable(files: &mut [TreeFile]) {
    if files
        .iter()
        .any(|file| is_executable(file.mode.unwrap_or(0)))
    {
        return;
    }

    let without_ext: Vec<usize> = files
        .iter()
        .enumerate()
        .filter(|(_, file)| !get_filename(&file.origin_path).contains('.'))
        .map(|(index, _)| index)
        .collect();

    if let [index] = without_ext.as_slice() {
        files[*index].mode = Some(0o755);
        return;
    }

    let maybe: Vec<usize> = files
        .iter()
        .enumerate()
        .filter(|(_, file)| maybe_executable(&file.origin_path))
        .map(|(index, _)| index)
        .collect();

    if let [index] = maybe.as_slice() {
        files[*index].mode = Some(0o755);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, mode: Option<u32>) -> TreeFile {
        TreeFile {
            path: path.to_string(),
            origin_path: path.to_string(),
            mode,
            data: Vec::new(),
        }
    }

    #[test]
    fn strips_the_common_directory() {
        assert_eq!(get_common_prefix_len(&["/a/ab/c", "/a/ad", "/a/ab/d"]), 3);
        assert_eq!(get_common_prefix_len(&["a"]), 0);
        assert_eq!(get_common_prefix_len(&["/a"]), 1);
        assert_eq!(get_common_prefix_len(&["/a/b"]), 3);
        assert_eq!(get_common_prefix_len(&[]), 0);
    }

    /// A `..` segment is the only thing rejected; ordinary relative paths,
    /// including names that merely contain dots, are accepted.
    #[test]
    fn rejects_escaping_paths() {
        check_paths(["tool", "bin/tool", "./tool", "a..b/c"]).expect("relative paths are fine");

        for path in ["../evil", "bin/../../evil", "..\\evil", "bin\\..\\evil"] {
            let error = check_paths([path]).expect_err(path);
            assert!(
                error.to_string().contains("escapes the install directory"),
                "{path}: {error}"
            );
        }
    }

    #[test]
    fn alias_keeps_directory_and_extension() {
        let mut files = vec![file("bin/tool-x86_64.deb", Some(0o755))];
        rename_alias(&mut files, "renamed");
        assert_eq!(files[0].path, "bin/renamed.deb");

        // A root-level file keeps its root.
        let mut files = vec![file("tool", Some(0o755))];
        rename_alias(&mut files, "renamed");
        assert_eq!(files[0].path, "renamed");
    }

    #[test]
    fn alias_only_renames_the_single_executable() {
        let mut files = vec![
            file("readme.txt", None),
            file("tool", Some(0o755)),
            file("lib.so", None),
        ];
        rename_alias(&mut files, "renamed");
        assert_eq!(files[1].path, "renamed");
        assert_eq!(files[0].path, "readme.txt");
    }

    #[test]
    fn guesses_the_executable_when_no_mode_is_set() {
        let mut files = vec![file("tool", None), file("README.md", None)];
        guess_executable(&mut files);
        assert_eq!(files[0].mode, Some(0o755));

        // An existing execute bit is never overridden.
        let mut files = vec![file("tool", Some(0o644)), file("other", None)];
        guess_executable(&mut files);
        assert_eq!(files[0].mode, Some(0o644));
        assert_eq!(files[1].mode, None);
    }
}
