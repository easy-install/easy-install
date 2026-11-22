//! A selected, installable artifact.
//!
//! Source resolution (`source`) turns a user-supplied URL into a list of
//! [`Artifact`]s; the install pipeline consumes them. Keeping this tiny model
//! separate lets every source (release assets, dist-manifests, CI artifacts,
//! nightly links, plain URLs) converge on one install path.

/// How the payload of an [`Artifact`] is installed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    /// Infer from the URL: archives are extracted, anything else is installed
    /// as a single file.
    Auto,
    /// Always a ZIP archive (GitHub Actions artifacts).
    Zip,
    /// Always a single file, even when the name looks like an archive.
    Single,
}

/// One file or archive to install.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Artifact {
    /// Tool name, used to name the install directory of multi-file archives.
    pub(crate) name: String,
    /// Download URL or local path.
    pub(crate) url: String,
    pub(crate) kind: Kind,
}

impl Artifact {
    pub(crate) fn new(name: impl Into<String>, url: impl Into<String>, kind: Kind) -> Self {
        Self {
            name: name.into(),
            url: url.into(),
            kind,
        }
    }
}
