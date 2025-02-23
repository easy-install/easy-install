// https://github.com/axodotdev/cargo-dist/blob/main/cargo-dist-schema/src/lib.rs

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
pub type RelPath = String;
pub type ArtifactId = String;
pub type TripleName = String;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistManifest {
    #[serde(default)]
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub artifacts: BTreeMap<ArtifactId, Artifact>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub kind: Option<ArtifactId>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub name: Option<ArtifactId>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[serde(default)]
    pub target_triples: Vec<TripleName>,
    /// Assets included in the bundle (like executables and READMEs)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[serde(default)]
    pub assets: Vec<Asset>,
}
/// An asset contained in an artifact (executable, license, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Asset {
    /// The executable_name name of the asset
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executable_name: Option<String>,

    /// The high-level name of the asset
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The path of the asset relative to the root of the artifact
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<RelPath>,
    /// The kind of asset this is
    #[serde(flatten)]
    pub kind: AssetKind,
}

/// An artifact included in a Distributable
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
#[non_exhaustive]
pub enum AssetKind {
    /// An executable artifact
    #[serde(rename = "executable")]
    Executable(ExecutableAsset),
    /// A C dynamic library
    #[serde(rename = "c_dynamic_library")]
    CDynamicLibrary(DynamicLibraryAsset),
    /// A C static library
    #[serde(rename = "c_static_library")]
    CStaticLibrary(StaticLibraryAsset),
    /// A README file
    #[serde(rename = "readme")]
    Readme,
    /// A LICENSE file
    #[serde(rename = "license")]
    License,
    /// A CHANGELOG or RELEASES file
    #[serde(rename = "changelog")]
    Changelog,
    /// Unknown to this version of cargo-dist-schema
    ///
    /// This is a fallback for forward/backward-compat
    #[serde(other)]
    #[serde(rename = "unknown")]
    Unknown,
}

/// An executable artifact (exe/binary)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutableAsset {
    /// The name of the Artifact containing symbols for this executable
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub symbols_artifact: Option<ArtifactId>,
}

/// A C dynamic library artifact (so/dylib/dll)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DynamicLibraryAsset {
    /// The name of the Artifact containing symbols for this library
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub symbols_artifact: Option<ArtifactId>,
}

/// A C static library artifact (a/lib)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaticLibraryAsset {
    /// The name of the Artifact containing symbols for this library
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub symbols_artifact: Option<ArtifactId>,
}

/// A kind of Artifact
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
#[non_exhaustive]
pub enum ArtifactKind {
    /// A zip or a tarball
    #[serde(rename = "executable-zip")]
    ExecutableZip,
    /// Standalone Symbols/Debuginfo for a build
    #[serde(rename = "symbols")]
    Symbols,
    /// Installer
    #[serde(rename = "installer")]
    Installer,
    /// A checksum of another artifact
    #[serde(rename = "checksum")]
    Checksum,
    /// The checksums of many artifacts
    #[serde(rename = "unified-checksum")]
    UnifiedChecksum,
    /// A tarball containing the source code
    #[serde(rename = "source-tarball")]
    SourceTarball,
    /// Some form of extra artifact produced by a sidecar build
    #[serde(rename = "extra-artifact")]
    ExtraArtifact,
    /// An updater executable
    #[serde(rename = "updater")]
    Updater,
    /// A file that already exists
    // #[serde(rename = "sbom")]
    // SBOM,
    /// Unknown to this version of cargo-dist-schema
    ///
    /// This is a fallback for forward/backward-compat
    #[serde(other)]
    #[serde(rename = "unknown")]
    Unknown,
}
