use compact_str::CompactString;
use serde::Deserialize;
use std::hash::Hash;
use std::{borrow::Borrow, collections::HashSet, hash::Hasher};
use url::Url;

#[derive(Eq, Deserialize, Debug)]
pub struct Artifact {
    pub name: CompactString,
    pub url: Url,
    pub browser_download_url: String,
}

// Manually implement PartialEq and Hash to ensure it will always produce the
// same hash as a str with the same content, and that the comparison will be
// the same to coparing a string.

impl PartialEq for Artifact {
    fn eq(&self, other: &Self) -> bool {
        self.name.eq(&other.name)
    }
}

impl Hash for Artifact {
    fn hash<H>(&self, state: &mut H)
    where
        H: Hasher,
    {
        let s: &str = self.name.as_str();
        s.hash(state)
    }
}

// Implement Borrow so that we can use call
// `HashSet::contains::<str>`

impl Borrow<str> for Artifact {
    fn borrow(&self) -> &str {
        &self.name
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct Artifacts {
    pub assets: HashSet<Artifact>,
}
