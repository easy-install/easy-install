use serde::Deserialize;
use std::hash::Hash;
use std::{collections::HashSet, hash::Hasher};
#[derive(Eq, Deserialize, Debug)]
pub(crate) struct GhArtifact {
    pub(crate) name: String,
    pub(crate) browser_download_url: String,
}

// Manually implement PartialEq and Hash to ensure it will always produce the
// same hash as a str with the same content, and that the comparison will be
// the same to coparing a string.

impl PartialEq for GhArtifact {
    fn eq(&self, other: &Self) -> bool {
        self.name.eq(&other.name)
    }
}

impl Hash for GhArtifact {
    fn hash<H>(&self, state: &mut H)
    where
        H: Hasher,
    {
        let s: &str = self.name.as_str();
        s.hash(state)
    }
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct GhArtifacts {
    pub(crate) assets: HashSet<GhArtifact>,
}
