use std::{num::NonZeroU64, ops::Bound};

use pubgrub::Range;
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
    /// The smallest version that is in this compatibility range.
    pub fn minimum(&self) -> Version {
        match *self {
            Self::Major(new) => Version {
                major: new.into(),
                minor: 0,
                patch: 0,
                pre: Prerelease::new("0").unwrap(),
                build: BuildMetadata::EMPTY,
            },
            Self::Minor(new) => Version {
                major: 0,
                minor: new.into(),
                patch: 0,
                pre: Prerelease::new("0").unwrap(),
                build: BuildMetadata::EMPTY,
            },
            Self::Patch(new) => Version {
                major: 0,
                minor: 0,
                patch: new,
                pre: Prerelease::new("0").unwrap(),
                build: BuildMetadata::EMPTY,
            },
        }
    }

    /// The smallest non pre-release version that is in this compatibility range.
    pub fn canonical(&self) -> Version {
        match *self {
            Self::Major(new) => Version {
                major: new.into(),
                minor: 0,
                patch: 0,
                pre: Prerelease::EMPTY,
                build: BuildMetadata::EMPTY,
            },
            Self::Minor(new) => Version {
                major: 0,
                minor: new.into(),
                patch: 0,
                pre: Prerelease::EMPTY,
                build: BuildMetadata::EMPTY,
            },
            Self::Patch(new) => Version {
                major: 0,
                minor: 0,
                patch: new,
                pre: Prerelease::EMPTY,
                build: BuildMetadata::EMPTY,
            },
        }
    }

    pub fn next(&self) -> Option<SemverCompatibility> {
        let one = NonZeroU64::new(1).unwrap();
        match *self {
            Self::Patch(s) => Some(
                s.checked_add(1)
                    .map(Self::Patch)
                    .unwrap_or_else(|| Self::Minor(one)),
            ),
            Self::Minor(s) => Some(
                s.checked_add(1)
                    .map(Self::Minor)
                    .unwrap_or_else(|| Self::Major(one)),
            ),
            Self::Major(s) => s.checked_add(1).map(Self::Major),
        }
    }

    pub fn maximum_bound(&self) -> Bound<Version> {
        if let Some(next) = self.next() {
            Bound::Excluded(next.minimum())
        } else {
            Bound::Unbounded
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
