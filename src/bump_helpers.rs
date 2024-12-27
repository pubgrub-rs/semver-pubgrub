use std::ops::Bound;

use pubgrub::Range;

use semver::BuildMetadata;
use semver::Prerelease;
use semver::Version;

use crate::VersionLike;

pub(crate) fn bump_major<V: VersionLike>(v: &V) -> Bound<V> {
    match v.major().checked_add(1) {
        Some(new) => Bound::Excluded(V::from(Version {
            major: new,
            minor: 0,
            patch: 0,
            pre: Prerelease::new("0").unwrap(),
            build: BuildMetadata::EMPTY,
        })),
        None => Bound::Unbounded,
    }
}

pub(crate) fn bump_minor<V: VersionLike>(v: &V) -> Bound<V> {
    match v.minor().checked_add(1) {
        Some(new) => Bound::Excluded(V::from(Version {
            major: v.major(),
            minor: new,
            patch: 0,
            pre: Prerelease::new("0").unwrap(),
            build: BuildMetadata::EMPTY,
        })),
        None => bump_major(v),
    }
}

pub(crate) fn bump_patch<V: VersionLike>(v: &V) -> Bound<V> {
    match v.patch().checked_add(1) {
        Some(new) => Bound::Excluded(V::from(Version {
            major: v.major(),
            minor: v.minor(),
            patch: new,
            pre: Prerelease::new("0").unwrap(),
            build: BuildMetadata::EMPTY,
        })),
        None => bump_minor(v),
    }
}

pub(crate) fn bump_pre<V: VersionLike>(v: &V) -> Bound<V> {
    if !v.pre().is_empty() {
        Bound::Excluded(V::from(Version {
            major: v.major(),
            minor: v.minor(),
            patch: v.patch(),
            pre: Prerelease::new(&format!("{}.0", v.pre())).unwrap(),
            build: BuildMetadata::EMPTY,
        }))
    } else {
        bump_patch(v)
    }
}

pub(crate) fn between<V: Clone + Ord>(low: V, into: impl Fn(&V) -> Bound<V>) -> Range<V> {
    let hight = into(&low);
    Range::from_range_bounds((Bound::Included(low), hight))
}

fn bump_up_to_normal<V: VersionLike>(v: &V) -> Option<V> {
    if v.pre().is_empty() {
        return None;
    } else {
        Some(V::from(Version {
            major: v.major(),
            minor: v.minor(),
            patch: v.patch(),
            pre: Prerelease::EMPTY,
            build: BuildMetadata::EMPTY,
        }))
    }
}

pub(crate) fn simplified_bounds_to_normal<V: VersionLike>(
    bounds: (Bound<V>, Bound<V>),
) -> (Bound<V>, Bound<V>) {
    let (mut from, mut to) = bounds;
    if let Bound::Included(f) | Bound::Excluded(f) = &from {
        if let Some(n) = bump_up_to_normal(f) {
            from = Bound::Included(n)
        }
    };
    if let Bound::Included(f) | Bound::Excluded(f) = &to {
        if let Some(n) = bump_up_to_normal(f) {
            to = Bound::Excluded(n)
        }
    };
    (from, to)
}
