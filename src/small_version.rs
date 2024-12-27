use std::sync::Arc;

use crate::VersionLike;

#[derive(Debug, PartialEq, Eq, Clone, Hash)]
pub struct SmallVersion(Iner);

#[derive(Debug, PartialEq, Eq, Clone, Hash)]
enum Iner {
    Full(Arc<semver::Version>),
    Small(Small),
}

#[derive(Debug, PartialEq, Eq, Clone, Hash)]
struct Small {
    major: u16,
    minor: u16,
    patch: u16,
    pre: bool,
}

impl Small {
    fn new(v: &semver::Version) -> Option<Small> {
        if !v.build.is_empty() {
            return None;
        }
        Some(Small {
            major: v.major.try_into().ok()?,
            minor: v.minor.try_into().ok()?,
            patch: v.patch.try_into().ok()?,
            pre: if v.pre.is_empty() {
                false
            } else if v.pre.as_str() == "0" {
                true
            } else {
                return None;
            },
        })
    }

    fn pre(&self) -> &str {
        if self.pre {
            "0"
        } else {
            ""
        }
    }

    pub fn into_version(&self) -> semver::Version {
        semver::Version {
            major: self.major as _,
            minor: self.minor as _,
            patch: self.patch as _,
            pre: semver::Prerelease::new(self.pre()).unwrap(),
            build: semver::BuildMetadata::EMPTY,
        }
    }
}

impl VersionLike for SmallVersion {
    fn major(&self) -> u64 {
        match &self.0 {
            Iner::Full(v) => v.major as _,
            Iner::Small(s) => s.major as _,
        }
    }

    fn minor(&self) -> u64 {
        match &self.0 {
            Iner::Full(v) => v.minor as _,
            Iner::Small(s) => s.minor as _,
        }
    }

    fn patch(&self) -> u64 {
        match &self.0 {
            Iner::Full(v) => v.patch as _,
            Iner::Small(s) => s.patch as _,
        }
    }

    fn pre(&self) -> &str {
        match &self.0 {
            Iner::Full(v) => v.pre.as_str(),
            Iner::Small(s) => s.pre(),
        }
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for SmallVersion {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.into_version().serialize(s)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for SmallVersion {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let v = semver::Version::deserialize(deserializer)?;
        Ok(SmallVersion::from(v))
    }
}

impl Ord for SmallVersion {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match self.major().cmp(&other.major()) {
            core::cmp::Ordering::Equal => {}
            ord => return ord,
        }
        match self.minor().cmp(&other.minor()) {
            core::cmp::Ordering::Equal => {}
            ord => return ord,
        }
        match self.patch().cmp(&other.patch()) {
            core::cmp::Ordering::Equal => {}
            ord => return ord,
        }
        match (&self.0, &other.0) {
            (Iner::Full(s), Iner::Full(o)) => {
                match s.pre.cmp(&o.pre) {
                    core::cmp::Ordering::Equal => {}
                    ord => return ord,
                }
                s.build.cmp(&o.build)
            }
            (Iner::Full(s), Iner::Small(o)) => {
                if !o.pre && !s.pre.is_empty() {
                    return core::cmp::Ordering::Less;
                }
                core::cmp::Ordering::Greater
            }
            (Iner::Small(s), Iner::Full(o)) => {
                if !s.pre && !o.pre.is_empty() {
                    return core::cmp::Ordering::Greater;
                }
                core::cmp::Ordering::Less
            }
            (Iner::Small(s), Iner::Small(o)) => {
                // Having a pre-release makes it smaller
                o.pre.cmp(&s.pre)
            }
        }
    }
}

impl PartialOrd for SmallVersion {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl std::fmt::Display for SmallVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.into_version().fmt(f)
    }
}

impl SmallVersion {
    pub fn into_version(&self) -> semver::Version {
        match &self.0 {
            Iner::Full(v) => v.as_ref().clone(),
            Iner::Small(s) => s.into_version(),
        }
    }
}

impl From<semver::Version> for SmallVersion {
    fn from(v: semver::Version) -> Self {
        Self(
            Small::new(&v)
                .map(|s| Iner::Small(s))
                .unwrap_or_else(|| Iner::Full(Arc::new(v))),
        )
    }
}

impl From<&semver::Version> for SmallVersion {
    fn from(v: &semver::Version) -> Self {
        Self(
            Small::new(&v)
                .map(|s| Iner::Small(s))
                .unwrap_or_else(|| Iner::Full(Arc::new(v.clone()))),
        )
    }
}
