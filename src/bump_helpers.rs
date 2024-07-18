use std::ops::Bound;

use pubgrub::Range;

use semver::BuildMetadata;
use semver::Prerelease;
use semver::Version;

pub(crate) fn bump_major(v: &Version) -> Bound<Version> {
    match v.major.checked_add(1) {
        Some(new) => Bound::Excluded({
            Version {
                major: new,
                minor: 0,
                patch: 0,
                pre: Prerelease::new("0").unwrap(),
                build: BuildMetadata::EMPTY,
            }
        }),
        None => Bound::Unbounded,
    }
}

pub(crate) fn bump_minor(v: &Version) -> Bound<Version> {
    match v.minor.checked_add(1) {
        Some(new) => Bound::Excluded({
            Version {
                major: v.major,
                minor: new,
                patch: 0,
                pre: Prerelease::new("0").unwrap(),
                build: BuildMetadata::EMPTY,
            }
        }),
        None => bump_major(v),
    }
}

pub(crate) fn bump_patch(v: &Version) -> Bound<Version> {
    match v.patch.checked_add(1) {
        Some(new) => Bound::Excluded({
            Version {
                major: v.major,
                minor: v.minor,
                patch: new,
                pre: Prerelease::new("0").unwrap(),
                build: BuildMetadata::EMPTY,
            }
        }),
        None => bump_minor(v),
    }
}

pub(crate) fn bump_pre(v: &Version) -> Bound<Version> {
    if !v.pre.is_empty() {
        Bound::Excluded({
            Version {
                major: v.major,
                minor: v.minor,
                patch: v.patch,
                pre: Prerelease::new(&format!("{}.0", v.pre)).unwrap(),
                build: BuildMetadata::EMPTY,
            }
        })
    } else {
        bump_patch(v)
    }
}

pub(crate) fn between(low: Version, into: impl Fn(&Version) -> Bound<Version>) -> Range<Version> {
    let hight = into(&low);
    Range::from_range_bounds((Bound::Included(low), hight))
}

fn bump_up_to_normal(v: &Version) -> Option<Version> {
    if v.pre.is_empty() {
        return None;
    } else {
        Some(Version {
            major: v.major,
            minor: v.minor,
            patch: v.patch,
            pre: Prerelease::EMPTY,
            build: BuildMetadata::EMPTY,
        })
    }
}

pub(crate) fn simplified_bounds_to_normal(
    bounds: (Bound<Version>, Bound<Version>),
) -> (Bound<Version>, Bound<Version>) {
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
