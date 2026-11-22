//! Artifact selection.
//!
//! Every source eventually produces a set of available assets. This module
//! applies the user's filters and picks the assets for the requested platform:
//!
//! 1. `--regex` matches the original file name directly and is the sole
//!    authority (exactly one match required).
//! 2. `--name` keeps only assets whose inferred tool name matches.
//! 3. The target triple is matched exactly first, then through a compatible
//!    ABI (msvc↔gnu, musl↔gnu) and finally, with `--fuzzy`, by arch+os.
//! 4. The best ranked asset per tool name wins.

use crate::InstallConfig;
use crate::artifact::{Artifact, Kind};
use crate::error::{Context, Result, bail};
use crate::manifest::{Artifact as ManifestArtifact, DistManifest};
use crate::path::{
    ends_with_exe, get_filename, is_archive_file, is_skip, name_no_ext, replace_filename,
};
use github_proxy::artifacts::GhArtifacts;
use github_proxy::{Proxy, Resource};
use guess_target::{Abi, Arch, Os, Target, guess_target};
use regex::Regex;
use std::str::FromStr;
use std::sync::LazyLock;

/// Rank penalty for a non-exact match, so exact matches always win.
const RANK_PENALTY: u32 = 1;

/// `(tool name, url)` pair produced by the selection passes.
type Candidate = (String, String);

/// Select the artifacts to install from a list of release assets.
pub(crate) fn from_assets(artifacts: GhArtifacts, config: &InstallConfig) -> Result<Vec<Artifact>> {
    let ranked = rank_assets(artifacts, config)?;
    Ok(filter_alias(ranked, config)
        .into_iter()
        .map(|(name, url)| {
            // Archives are unpacked; everything else is written as-is. Deciding
            // here keeps the original name of a single file (an ISO must not
            // lose its extension, or gain `.exe` on Windows).
            let kind = if is_archive_file(&url) {
                Kind::Auto
            } else {
                Kind::Single
            };
            Artifact::new(name, url, kind)
        })
        .collect())
}

/// Select the artifacts declared by a dist-manifest.
///
/// Exact target matches win over ABI-compatible ones across the whole
/// manifest, independent of `BTreeMap` iteration order.
pub(crate) fn from_manifest(
    url: &str,
    manifest: &DistManifest,
    config: &InstallConfig,
) -> Vec<Artifact> {
    let targets = config.targets();
    let mut exact = Vec::new();
    let mut compatible = Vec::new();

    for (key, artifact) in &manifest.artifacts {
        if !manifest_eligible(key, artifact, &targets) {
            continue;
        }
        if artifact
            .target_triples
            .iter()
            .any(|triple| targets.iter().any(|target| target.to_str() == triple))
        {
            exact.push(manifest_candidate(url, key, artifact));
        } else if artifact.target_triples.iter().any(|triple| {
            Target::from_str(triple)
                .is_ok_and(|asset| targets.iter().any(|t| compatible_abi(&asset, t)))
        }) {
            compatible.push(manifest_candidate(url, key, artifact));
        }
    }

    let selected = if exact.is_empty() { compatible } else { exact };
    filter_alias(selected, config)
        .into_iter()
        .map(|(name, url)| Artifact::new(name, url, Kind::Auto))
        .collect()
}

/// Print the "no asset found" message, listing available assets when known.
pub(crate) fn not_found(url: &str, config: &InstallConfig, available: Option<&[String]>) {
    if config.quiet {
        return;
    }

    let targets = config.targets();
    let target_str = targets
        .iter()
        .map(|target| target.to_str())
        .collect::<Vec<_>>()
        .join(", ");

    let active_filters: Vec<String> = [
        (!config.name.is_empty()).then(|| format!("--name {}", config.name.join(","))),
        config
            .alias
            .as_ref()
            .map(|alias| format!("--alias {alias}")),
        config
            .regex
            .as_ref()
            .map(|regex| format!("--regex {regex}")),
    ]
    .into_iter()
    .flatten()
    .collect();
    let filter_hint = if active_filters.is_empty() {
        String::new()
    } else {
        format!(" (with {})", active_filters.join(", "))
    };

    println!("No {target_str} asset found in {url}{filter_hint}");

    let Some(names) = available else { return };
    if names.is_empty() {
        return;
    }

    // Checksum manifests and other release metadata cannot be installed;
    // listing them next to the real assets only adds noise.
    let listed: Vec<&str> = names
        .iter()
        .map(String::as_str)
        .filter(|name| !is_skip(name))
        .collect();
    if listed.is_empty() {
        return;
    }

    // (stem, tool name, target)
    let mut rows: Vec<(String, String, String)> = Vec::new();
    for name in listed {
        let stem = name_no_ext(name);
        let guesses = guess_target(&stem);
        if guesses.is_empty() {
            rows.push((stem.clone(), stem, String::new()));
        } else {
            for guess in guesses {
                rows.push((stem.clone(), guess.name, guess.target.to_str().to_string()));
            }
        }
    }

    // Same-arch targets first, then by target, then by tool name.
    rows.sort_by(|a, b| {
        let same_arch = |target: &str| {
            !target.is_empty()
                && targets.iter().any(|local| {
                    Target::from_str(target).is_ok_and(|parsed| parsed.arch() == local.arch())
                })
        };
        same_arch(&b.2)
            .cmp(&same_arch(&a.2))
            .then_with(|| a.2.cmp(&b.2))
            .then_with(|| a.1.cmp(&b.1))
    });

    println!();
    let stem_width = rows.iter().map(|row| row.0.len()).max().unwrap_or(6);
    let name_width = rows.iter().map(|row| row.1.len()).max().unwrap_or(4);
    let target_width = rows.iter().map(|row| row.2.len()).max().unwrap_or(6);
    println!(
        "  {:<stem_width$}  {:<name_width$}  {:<target_width$}",
        "ORIGINAL", "NAME", "TARGET",
    );
    println!(
        "  {:-<stem_width$}  {:-<name_width$}  {:-<target_width$}",
        "", "", "",
    );
    for row in &rows {
        let target = if row.2.is_empty() {
            "(unknown)"
        } else {
            &row.2
        };
        println!(
            "  {:<stem_width$}  {:<name_width$}  {:<target_width$}",
            row.0, row.1, target,
        );
    }
}

/// Whether `name` matches `stem` at a word boundary: `stem` starts with `name`
/// followed by `-`, `_`, `.` or the end of the string, so `qjs` does not match
/// `qjsc-linux-x86`.
fn name_boundary_match(stem: &str, name: &str) -> bool {
    stem == name
        || (stem.starts_with(name)
            && matches!(stem.as_bytes().get(name.len()), Some(b'-' | b'_' | b'.')))
}

/// Whether two ABIs are close enough to serve as a fallback when the exact ABI
/// is unavailable (msvc↔gnu on Windows, musl↔gnu on Linux).
fn is_compatible_abi(a: Option<Abi>, b: Option<Abi>) -> bool {
    matches!(
        (a, b),
        (Some(Abi::Msvc), Some(Abi::Gnu))
            | (Some(Abi::Gnu), Some(Abi::Msvc))
            | (Some(Abi::Musl), Some(Abi::Gnu))
            | (Some(Abi::Gnu), Some(Abi::Musl))
    )
}

/// Whether `asset` and `target` describe the same arch and OS with a
/// compatible ABI.
fn compatible_abi(asset: &Target, target: &Target) -> bool {
    asset.arch() == target.arch()
        && asset.os() == target.os()
        && is_compatible_abi(asset.abi(), target.abi())
}

/// Rank release assets against the requested targets, keeping the best
/// candidate per tool name.
fn rank_assets(artifacts: GhArtifacts, config: &InstallConfig) -> Result<Vec<Candidate>> {
    let targets = config.targets();

    // `--regex` matches the original file name and must select exactly one
    // asset; it bypasses `guess_target` entirely.
    let regex = match &config.regex {
        Some(pattern) => {
            let regex = Regex::new(pattern).context("invalid --regex pattern")?;
            let matched: Vec<String> = artifacts
                .assets
                .iter()
                .filter(|asset| regex.is_match(&get_filename(&asset.browser_download_url)))
                .map(|asset| asset.name.clone())
                .collect();
            if matched.is_empty() {
                bail!("--regex did not match any assets. Check the pattern and try again.");
            }
            if matched.len() > 1 {
                bail!(
                    "--regex matched {} assets, expected exactly 1. Pattern is too permissive.\n  Matched: {:#?}\n  Tighten the regex (e.g. anchor it to the platform triple) so only one asset remains.",
                    matched.len(),
                    matched
                );
            }
            Some(regex)
        }
        None => None,
    };

    // (penalized, rank, name, url)
    let mut candidates = Vec::new();

    for asset in artifacts.assets {
        // API download URLs (e.g. `.../artifacts/{id}/zip`) end in a path
        // segment without a dot; fall back to the artifact's display name.
        let filename = get_filename(&asset.browser_download_url);
        let filename = if filename.contains('.') {
            filename
        } else {
            asset.name.clone()
        };

        if let Some(regex) = &regex {
            if regex.is_match(&filename) {
                candidates.push((
                    false,
                    u32::MAX,
                    name_no_ext(&filename),
                    asset.browser_download_url,
                ));
            }
            continue;
        }

        if is_skip(&asset.browser_download_url) {
            continue;
        }
        if ends_with_exe(&asset.browser_download_url)
            && targets.iter().any(|target| target.os() != Os::Windows)
        {
            continue;
        }

        let stem = name_no_ext(&filename);
        let guesses = guess_target(&stem);

        // `--name` matches the inferred tool name when available, and falls
        // back to a word-boundary match on the raw stem.
        if !config.name.is_empty() {
            let matched = if guesses.is_empty() {
                config
                    .name
                    .iter()
                    .any(|name| name_boundary_match(&stem, name))
            } else {
                config
                    .name
                    .iter()
                    .any(|name| guesses.iter().any(|guess| guess.name == name.as_str()))
            };
            if !matched {
                continue;
            }
        }

        // Exact target → compatible ABI → fuzzy arch+os.
        let mut penalized = false;
        let matched = if let Some(guess) = guesses.iter().find(|g| targets.contains(&g.target)) {
            Some(guess)
        } else if let Some(guess) = guesses.iter().find(|g| {
            targets
                .iter()
                .any(|target| compatible_abi(&g.target, target))
        }) {
            penalized = true;
            Some(guess)
        } else if config.fuzzy {
            let guess = guesses.iter().find(|g| {
                targets
                    .iter()
                    .any(|target| g.target.arch() == target.arch() && g.target.os() == target.os())
            });
            penalized = guess.is_some();
            guess
        } else {
            None
        };

        // A single file with no platform in its name (an ISO, a shell script)
        // cannot be matched to a target. Install it only when the user asked
        // for it by name: silently preferring it over a platform binary would
        // be surprising.
        let Some(guess) = matched else {
            if !config.name.is_empty() && !is_archive_file(&asset.browser_download_url) {
                candidates.push((
                    false,
                    0,
                    filename.clone(),
                    asset.browser_download_url.clone(),
                ));
            }
            continue;
        };

        // ARM platforms prefer musl: glibc builds often do not run there.
        let musl_bonus = match (guess.target.arch(), guess.target.abi()) {
            (Arch::Aarch64, Some(Abi::Musl)) => 10,
            _ => 0,
        };
        let rank = guess.rank + musl_bonus;
        let rank = if penalized {
            rank.saturating_sub(RANK_PENALTY)
        } else {
            rank
        };
        candidates.push((
            penalized,
            rank,
            guess.name.clone(),
            asset.browser_download_url.clone(),
        ));
    }

    // With an explicit `--target`, an exact match must beat an ABI fallback:
    // `rank` measures how specifically the *file name* matched, not how well it
    // matches the requested target.
    let candidates: Vec<_> = if config.target.is_some() {
        let has_exact = candidates.iter().any(|(penalized, ..)| !*penalized);
        candidates
            .into_iter()
            .filter(|(penalized, ..)| !has_exact || !*penalized)
            .collect()
    } else {
        candidates
    };

    // Keep only the highest-ranked asset per tool name.
    // FIXME: variants such as `llrt-no-sdk` / `llrt-full-sdk` need user choice.
    let max_rank = candidates
        .iter()
        .map(|(_, rank, ..)| *rank)
        .max()
        .unwrap_or(0);
    let mut seen = Vec::new();
    let mut list = Vec::new();
    for (_, rank, name, url) in candidates {
        if rank < max_rank || seen.contains(&name) {
            continue;
        }
        seen.push(name.clone());
        list.push((name, to_proxy_url(&url, config.proxy)));
    }
    Ok(list)
}

/// Keep only the artifacts matching `--alias`.
///
/// The rename itself happens in [`crate::tree`]; this only narrows the
/// selection when an alias names one of several artifacts.
fn filter_alias(artifacts: Vec<Candidate>, config: &InstallConfig) -> Vec<Candidate> {
    match &config.alias {
        Some(alias) => {
            let matching: Vec<_> = artifacts
                .iter()
                .filter(|(name, _)| name == alias)
                .cloned()
                .collect();
            if matching.is_empty() {
                artifacts
            } else {
                matching
            }
        }
        None => artifacts,
    }
}

/// Whether a manifest entry is an installable bundle for one of `targets`'
/// operating systems.
fn manifest_eligible(key: &str, artifact: &ManifestArtifact, targets: &[Target]) -> bool {
    if is_skip(&get_filename(key)) {
        return false;
    }
    if ends_with_exe(key) && targets.iter().any(|target| target.os() != Os::Windows) {
        return false;
    }
    // Only executable bundles are installable; other kinds (e.g. sources) are
    // deliberately ignored.
    match &artifact.kind {
        Some(kind) => kind == "executable-zip",
        None => true,
    }
}

/// `(name, url)` of a matched manifest artifact.
fn manifest_candidate(url: &str, key: &str, artifact: &ManifestArtifact) -> Candidate {
    // Prefer the manifest's explicit `name`: the artifact URL may only carry
    // the platform (e.g. `.../ant/windows-x64`).
    let name = match &artifact.name {
        Some(name) if !name.is_empty() => name.clone(),
        _ => {
            let raw = name_no_ext(&get_filename(key));
            guess_target(&raw).pop().map_or(raw, |guess| guess.name)
        }
    };
    let url = if github_proxy::fetch::Url::is_valid(key) {
        key.to_string()
    } else {
        replace_filename(url, key)
    };
    (name, url)
}

static RE_RELEASE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^https://github\.com/(?P<owner>[^/]+)/(?P<repo>[^/]+)/releases/download/(?P<tag>[^/]+)/(?P<filename>.+)$",
    )
    .unwrap()
});

/// Route a release asset URL through the configured GitHub proxy.
fn to_proxy_url(url: &str, proxy: Proxy) -> String {
    if proxy == Proxy::Github {
        return url.to_string();
    }
    let Some(captures) = RE_RELEASE.captures(url) else {
        return url.to_string();
    };
    let release = Resource::Release {
        owner: captures["owner"].to_string(),
        repo: captures["repo"].to_string(),
        tag: captures["tag"].to_string(),
        name: captures["filename"].to_string(),
    };
    proxy.url(&release).unwrap_or_else(|| url.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::DistManifest;
    use github_proxy::artifacts::GhArtifact;

    #[test]
    fn test_name_boundary_match() {
        assert!(name_boundary_match("qjs", "qjs"));
        assert!(name_boundary_match("qjs-linux-x86", "qjs"));
        assert!(name_boundary_match("qjs_linux-x86", "qjs"));
        assert!(name_boundary_match("qjs.linux-x86", "qjs"));
        assert!(!name_boundary_match("qjsc-linux-x86", "qjs"));
        assert!(!name_boundary_match("qjsc", "qjs"));
        assert!(!name_boundary_match("qjs", "qjsc"));
    }

    #[test]
    fn test_is_compatible_abi() {
        assert!(is_compatible_abi(Some(Abi::Msvc), Some(Abi::Gnu)));
        assert!(is_compatible_abi(Some(Abi::Gnu), Some(Abi::Msvc)));
        assert!(is_compatible_abi(Some(Abi::Musl), Some(Abi::Gnu)));
        assert!(is_compatible_abi(Some(Abi::Gnu), Some(Abi::Musl)));
        assert!(!is_compatible_abi(Some(Abi::Msvc), Some(Abi::Musl)));
        assert!(!is_compatible_abi(Some(Abi::Musl), Some(Abi::Msvc)));
        assert!(!is_compatible_abi(Some(Abi::Gnu), Some(Abi::Gnu)));
        assert!(!is_compatible_abi(Some(Abi::Msvc), Some(Abi::Msvc)));
        assert!(!is_compatible_abi(None, Some(Abi::Gnu)));
        assert!(!is_compatible_abi(Some(Abi::Gnu), None));
    }

    /// With an explicit `--target`, an exact target match must win over an ABI
    /// fallback. Regression: requesting `x86_64-unknown-linux-gnu` must select
    /// `lo-linux-x64.gz`, not the higher-ranked `lo-linux-musl-x64.gz`.
    #[test]
    fn test_target_exact_beats_abi_fallback() {
        let assets = ["lo-linux-x64.gz", "lo-linux-musl-x64.gz"]
            .into_iter()
            .map(|name| GhArtifact {
                name: name.to_string(),
                browser_download_url: format!(
                    "https://github.com/just-js/lo/releases/download/0.0.33-pre/{name}"
                ),
                size: 0,
                url: None,
            })
            .collect();

        let config = InstallConfig {
            target: Some(Target::from_str("x86_64-unknown-linux-gnu").unwrap()),
            ..Default::default()
        };
        let list = from_assets(GhArtifacts { assets }, &config).unwrap();
        assert_eq!(list.len(), 1, "expected exactly one artifact: {list:?}");
        assert!(
            list[0].url.ends_with("lo-linux-x64.gz"),
            "gnu target must select the gnu asset, got: {list:?}"
        );
    }

    /// Ventoy publishes platform installers plus a bare `sha256` file and a
    /// platform-independent live ISO. The checksum must never be a candidate,
    /// and the ISO is installable as one file once the user names it.
    #[test]
    fn test_named_single_file_without_target() {
        let assets: std::collections::HashSet<_> = [
            "ventoy-1.1.18-windows.zip",
            "ventoy-1.1.18-linux.tar.gz",
            "sha256",
            "ventoy-1.1.18-livecd.iso",
        ]
        .into_iter()
        .map(|name| GhArtifact {
            name: name.to_string(),
            browser_download_url: format!(
                "https://github.com/ventoy/Ventoy/releases/download/v1.1.18/{name}"
            ),
            size: 0,
            url: None,
        })
        .collect();

        let target = Some(Target::from_str("aarch64-apple-darwin").unwrap());

        // No platform binary and no explicit name: nothing is selected.
        let config = InstallConfig {
            target,
            ..Default::default()
        };
        let list = from_assets(
            GhArtifacts {
                assets: assets.clone(),
            },
            &config,
        )
        .unwrap();
        assert!(list.is_empty(), "{list:?}");

        // Naming the ISO explicitly installs it as a single file under its own
        // name instead of unpacking it.
        let config = InstallConfig {
            target,
            name: vec!["ventoy-1.1.18-livecd.iso".to_string()],
            ..Default::default()
        };
        let list = from_assets(GhArtifacts { assets }, &config).unwrap();
        assert_eq!(list.len(), 1, "{list:?}");
        assert_eq!(list[0].name, "ventoy-1.1.18-livecd.iso");
        assert_eq!(list[0].kind, Kind::Single);
        assert!(list[0].url.ends_with("ventoy-1.1.18-livecd.iso"));
    }

    fn manifest(name: &str) -> DistManifest {
        let path = format!("{}/dist-manifest/{name}.json", env!("CARGO_MANIFEST_DIR"));
        let text = std::fs::read_to_string(path).unwrap();
        serde_json::from_str(&text).unwrap()
    }

    /// The antjs manifest downloads bare binaries whose URL path ends in the
    /// platform name (e.g. `.../ant/windows-x64`) rather than the tool name.
    /// The installed binary name must come from the artifact's `name` field.
    #[test]
    fn test_antjs_manifest() {
        let manifest = manifest("ant");
        for (triple, suffix) in [
            ("x86_64-pc-windows-gnu", "/windows-x64"),
            ("x86_64-unknown-linux-gnu", "/linux-x64"),
            ("aarch64-unknown-linux-musl", "/linux-aarch64-musl"),
            ("aarch64-apple-darwin", "/darwin-aarch64"),
        ] {
            let config = InstallConfig {
                target: Some(Target::from_str(triple).unwrap()),
                ..Default::default()
            };
            let urls = from_manifest("manifest.json", &manifest, &config);
            assert_eq!(
                urls.len(),
                1,
                "expected one artifact for {triple}: {urls:?}"
            );
            assert_eq!(urls[0].name, "ant", "binary name for {triple}");
            assert!(
                urls[0].url.ends_with(suffix),
                "url {} should end with {suffix}",
                urls[0].url
            );
        }
    }

    /// The bare manifest points at npm registry tarballs. The artifact URL
    /// filename is `bare-runtime-<platform>-<version>.tgz`, so the installed
    /// binary name must come from the artifact's `name` field (`"bare"`).
    #[test]
    fn test_bare_manifest() {
        let manifest = manifest("bare");
        for (triple, package) in [
            ("x86_64-pc-windows-msvc", "bare-runtime-win32-x64"),
            ("x86_64-unknown-linux-gnu", "bare-runtime-linux-x64"),
            ("aarch64-apple-darwin", "bare-runtime-darwin-arm64"),
            ("aarch64-linux-android", "bare-runtime-android-arm64"),
        ] {
            let config = InstallConfig {
                target: Some(Target::from_str(triple).unwrap()),
                ..Default::default()
            };
            let urls = from_manifest("manifest.json", &manifest, &config);
            assert_eq!(
                urls.len(),
                1,
                "expected one artifact for {triple}: {urls:?}"
            );
            assert_eq!(urls[0].name, "bare", "binary name for {triple}");
            assert!(
                urls[0].url.contains(package),
                "url {} should contain {package}",
                urls[0].url
            );
        }
    }

    #[test]
    fn proxy_is_only_applied_to_release_assets() {
        let url = "https://github.com/o/r/releases/download/v1/tool.zip";
        assert_eq!(to_proxy_url(url, Proxy::Github), url);
        assert_eq!(
            to_proxy_url("https://example.com/tool.zip", Proxy::Github),
            "https://example.com/tool.zip"
        );
    }
}
