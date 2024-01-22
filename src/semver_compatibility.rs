use std::{num::NonZeroU64, ops::Bound};

use pubgrub::range::Range;
use semver::{BuildMetadata, Prerelease, Version};

use crate::bump_helpers::{bump_major, bump_minor, bump_patch};

/// A type that represents when cargo treats two Versions as compatible.
/// Versions `a` and `b` are compatible if their left-most nonzero digit is the
/// same.
#[derive(Clone, Copy, Eq, PartialEq, Hash, Debug, PartialOrd, Ord)]
pub enum SemverCompatibility {
    Major(NonZeroU64),
    Minor(NonZeroU64),
    Patch(u64),
}

impl SemverCompatibility {
    pub fn minimum(&self) -> Version {
        match self {
            SemverCompatibility::Major(new) => Version {
                major: (*new).into(),
                minor: 0,
                patch: 0,
                pre: Prerelease::new("0").unwrap(),
                build: BuildMetadata::EMPTY,
            },
            SemverCompatibility::Minor(new) => Version {
                major: 0,
                minor: (*new).into(),
                patch: 0,
                pre: Prerelease::new("0").unwrap(),
                build: BuildMetadata::EMPTY,
            },
            SemverCompatibility::Patch(new) => Version {
                major: 0,
                minor: 0,
                patch: *new,
                pre: Prerelease::new("0").unwrap(),
                build: BuildMetadata::EMPTY,
            },
        }
    }

    pub fn maximum(&self) -> Bound<Version> {
        let min = self.minimum();
        match self {
            SemverCompatibility::Major(_) => bump_major(&min),
            SemverCompatibility::Minor(_) => bump_minor(&min),
            SemverCompatibility::Patch(_) => bump_patch(&min),
        }
    }
}

impl From<&Version> for SemverCompatibility {
    fn from(ver: &Version) -> Self {
        if let Some(m) = NonZeroU64::new(ver.major) {
            return SemverCompatibility::Major(m);
        }
        if let Some(m) = NonZeroU64::new(ver.minor) {
            return SemverCompatibility::Minor(m);
        }
        SemverCompatibility::Patch(ver.patch)
    }
}

impl From<&SemverCompatibility> for Range<Version> {
    fn from(compat: &SemverCompatibility) -> Self {
        let low = compat.minimum();
        let hight = {
            match compat {
                SemverCompatibility::Major(_) => bump_major(&low),
                SemverCompatibility::Minor(_) => bump_minor(&low),
                SemverCompatibility::Patch(_) => bump_patch(&low),
            }
        };
        Range::from_range_bounds((Bound::Included(low), hight))
    }
}
