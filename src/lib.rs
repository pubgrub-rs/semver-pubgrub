use std::{
    cmp::{max, min},
    fmt::Display,
    ops::Bound,
};

use pubgrub::{range::Range, version_set::VersionSet};
use semver::{BuildMetadata, Comparator, Op, Prerelease, Version, VersionReq};

/// This needs to be bug-for-bug compatible with https://github.com/dtolnay/semver/blob/master/src/eval.rs

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct SemverPubgrub {
    normal: Range<Version>,
    pre: Range<Version>,
}

impl SemverPubgrub {
    /// Convert to something that can be used with
    /// [BTreeMap::range](std::collections::BTreeMap::range).
    /// All versions contained in self, will be in the output,
    /// but there may be versions in the output that are not contained in self.
    /// Returns None if the range is empty.
    pub fn bounding_range(&self) -> Option<(Bound<&Version>, Bound<&Version>)> {
        use Bound::*;
        match (self.normal.bounding_range(), self.pre.bounding_range()) {
            (None, None) => None,
            (None, Some(s)) | (Some(s), None) => Some(s),
            (Some((ns, ne)), Some((ps, pe))) => {
                let start = match (ns, ps) {
                    (Included(n), Included(p)) => Included(min(n, p)),
                    (Included(i), Excluded(e)) | (Excluded(e), Included(i)) => {
                        if e < i {
                            Excluded(e)
                        } else {
                            Included(i)
                        }
                    }
                    (Excluded(n), Excluded(p)) => Excluded(min(n, p)),
                    (Unbounded, _) | (_, Unbounded) => Unbounded,
                };
                let end = match (ne, pe) {
                    (Included(n), Included(p)) => Included(max(n, p)),
                    (Included(i), Excluded(e)) | (Excluded(e), Included(i)) => {
                        if i < e {
                            Excluded(e)
                        } else {
                            Included(i)
                        }
                    }
                    (Excluded(n), Excluded(p)) => Excluded(max(n, p)),
                    (Unbounded, _) | (_, Unbounded) => Unbounded,
                };
                Some((start, end))
            }
        }
    }
}

impl Display for SemverPubgrub {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        "SemverPubgrub { norml: ".fmt(f)?;
        self.normal.fmt(f)?;
        ", pre: ".fmt(f)?;
        self.pre.fmt(f)?;
        " } ".fmt(f)
    }
}

impl VersionSet for SemverPubgrub {
    type V = Version;

    fn empty() -> Self {
        SemverPubgrub {
            normal: Range::empty(),
            pre: Range::empty(),
        }
    }

    fn singleton(v: Self::V) -> Self {
        let is_pre = !v.pre.is_empty();
        let singleton = Range::singleton(v);
        if !is_pre {
            SemverPubgrub {
                normal: singleton,
                pre: Range::empty(),
            }
        } else {
            SemverPubgrub {
                normal: Range::empty(),
                pre: singleton,
            }
        }
    }

    fn complement(&self) -> Self {
        SemverPubgrub {
            normal: self.normal.complement(),
            pre: self.pre.complement(),
        }
    }

    fn intersection(&self, other: &Self) -> Self {
        SemverPubgrub {
            normal: self.normal.intersection(&other.normal),
            pre: self.pre.intersection(&other.pre),
        }
    }

    fn contains(&self, v: &Self::V) -> bool {
        // This needs to be bug-for-bug compatible with matches_req https://github.com/dtolnay/semver/blob/master/src/eval.rs#L3
        if v.pre.is_empty() {
            self.normal.contains(v)
        } else {
            self.pre.contains(v)
        }
    }
}

impl From<&VersionReq> for SemverPubgrub {
    fn from(req: &VersionReq) -> Self {
        let mut out = SemverPubgrub::full();
        // add to normal the intersection of cmps in req
        for cmp in &req.comparators {
            out = out.intersection(&matches_impl(cmp));
        }
        let mut pre = Range::empty();
        // add to pre the union of cmps in req
        for cmp in &req.comparators {
            pre = pre.union(&pre_is_compatible(cmp));
        }
        out.pre = pre.intersection(&out.pre);
        out
    }
}

fn bump_major(v: &Version) -> Bound<Version> {
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

fn bump_minor(v: &Version) -> Bound<Version> {
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

fn bump_patch(v: &Version) -> Bound<Version> {
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

fn bump_pre(v: &Version) -> Bound<Version> {
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

fn between(low: Version, into: impl Fn(&Version) -> Bound<Version>) -> Range<Version> {
    let hight = into(&low);
    Range::from_range_bounds((Bound::Included(low), hight))
}

fn matches_impl(cmp: &Comparator) -> SemverPubgrub {
    // https://github.com/dtolnay/semver/blob/master/src/eval.rs#L30
    match cmp.op {
        Op::Exact | Op::Wildcard => matches_exact(cmp),
        Op::Greater => matches_greater(cmp),
        Op::GreaterEq => matches_exact(cmp).union(&matches_greater(cmp)),
        Op::Less => matches_less(cmp),
        Op::LessEq => matches_exact(cmp).union(&matches_less(cmp)),
        Op::Tilde => matches_tilde(cmp),
        Op::Caret => matches_caret(cmp),
        _ => unreachable!("update to a version that supports this Op"),
    }
}

fn matches_exact(cmp: &Comparator) -> SemverPubgrub {
    // https://github.com/dtolnay/semver/blob/master/src/eval.rs#L44
    let low = Version {
        major: cmp.major,
        minor: cmp.minor.unwrap_or(0),
        patch: cmp.patch.unwrap_or(0),
        pre: cmp.pre.clone(),
        build: BuildMetadata::EMPTY,
    };
    if !cmp.pre.is_empty() {
        return SemverPubgrub {
            normal: Range::empty(),
            pre: between(low, bump_pre),
        };
    }
    let normal = if cmp.patch.is_some() {
        between(low, bump_patch)
    } else if cmp.minor.is_some() {
        between(low, bump_minor)
    } else {
        between(low, bump_major)
    };

    SemverPubgrub {
        normal,
        pre: Range::empty(),
    }
}

fn matches_greater(cmp: &Comparator) -> SemverPubgrub {
    // https://github.com/dtolnay/semver/blob/master/src/eval.rs#L64
    let low = Version {
        major: cmp.major,
        minor: cmp.minor.unwrap_or(0),
        patch: cmp.patch.unwrap_or(0),
        pre: cmp.pre.clone(),
        build: BuildMetadata::EMPTY,
    };
    let bump = if cmp.patch.is_some() {
        bump_pre(&low)
    } else if cmp.minor.is_some() {
        bump_minor(&low)
    } else {
        bump_major(&low)
    };
    let low_bound = match bump {
        Bound::Included(_) => unreachable!(),
        Bound::Excluded(v) => Bound::Included(v),
        Bound::Unbounded => return SemverPubgrub::empty(),
    };
    let out = Range::from_range_bounds((low_bound, Bound::Unbounded));
    SemverPubgrub {
        normal: out.clone(),
        pre: out,
    }
}

fn matches_less(cmp: &Comparator) -> SemverPubgrub {
    // https://github.com/dtolnay/semver/blob/master/src/eval.rs#L90
    let out = Range::strictly_lower_than(Version {
        major: cmp.major,
        minor: cmp.minor.unwrap_or(0),
        patch: cmp.patch.unwrap_or(0),
        pre: if cmp.patch.is_some() {
            cmp.pre.clone()
        } else {
            Prerelease::new("0").unwrap()
        },
        build: BuildMetadata::EMPTY,
    });
    SemverPubgrub {
        normal: out.clone(),
        pre: out,
    }
}

fn matches_tilde(cmp: &Comparator) -> SemverPubgrub {
    // https://github.com/dtolnay/semver/blob/master/src/eval.rs#L116
    let low = Version {
        major: cmp.major,
        minor: cmp.minor.unwrap_or(0),
        patch: cmp.patch.unwrap_or(0),
        pre: cmp.pre.clone(),
        build: BuildMetadata::EMPTY,
    };
    if cmp.patch.is_some() {
        let out = between(low, bump_minor);
        return SemverPubgrub {
            normal: out.clone(),
            pre: out,
        };
    }
    let normal = if cmp.minor.is_some() {
        between(low, bump_minor)
    } else {
        between(low, bump_major)
    };
    SemverPubgrub {
        normal,
        pre: Range::empty(),
    }
}

fn matches_caret(cmp: &Comparator) -> SemverPubgrub {
    // https://github.com/dtolnay/semver/blob/master/src/eval.rs#L136
    let low = Version {
        major: cmp.major,
        minor: cmp.minor.unwrap_or(0),
        patch: cmp.patch.unwrap_or(0),
        pre: if cmp.patch.is_some() {
            cmp.pre.clone()
        } else {
            Prerelease::new("0").unwrap()
        },
        build: BuildMetadata::EMPTY,
    };
    let Some(minor) = cmp.minor else {
        let out = between(low, bump_major);
        return SemverPubgrub {
            normal: out.clone(),
            pre: out,
        };
    };

    if cmp.patch.is_none() {
        let out = if cmp.major > 0 {
            between(low, bump_major)
        } else {
            between(low, bump_minor)
        };
        return SemverPubgrub {
            normal: out.clone(),
            pre: out,
        };
    };

    let out = if cmp.major > 0 {
        between(low, bump_major)
    } else if minor > 0 {
        between(low, bump_minor)
    } else {
        between(low, bump_patch)
    };
    SemverPubgrub {
        normal: out.clone(),
        pre: out,
    }
}

fn pre_is_compatible(cmp: &Comparator) -> Range<Version> {
    // https://github.com/dtolnay/semver/blob/master/src/eval.rs#L176
    if cmp.pre.is_empty() {
        return Range::empty();
    }
    let (Some(minor), Some(patch)) = (cmp.minor, cmp.patch) else {
        return Range::empty();
    };

    Range::between(
        Version {
            major: cmp.major,
            minor,
            patch,
            pre: Prerelease::new("0").unwrap(),
            build: BuildMetadata::EMPTY,
        },
        Version::new(cmp.major, minor, patch),
    )
}

#[cfg(test)]
mod test {
    use super::*;
    use pubgrub::version_set::VersionSet;
    use std::ops::RangeBounds;

    const OPS: &[&str] = &["^", "~", "=", "<", ">", "<=", ">="];

    #[test]
    fn test_contains_overflow() {
        for op in OPS {
            for psot in [
                "0.0.18446744073709551615",
                "0.18446744073709551615.0",
                "0.18446744073709551615.1",
                "0.18446744073709551615.18446744073709551615",
                "0.18446744073709551615",
                "18446744073709551615",
                "18446744073709551615.0",
                "18446744073709551615.1",
                "18446744073709551615.18446744073709551615",
                "18446744073709551615.18446744073709551615.0",
                "18446744073709551615.18446744073709551615.1",
                "18446744073709551615.18446744073709551615.18446744073709551615",
            ] {
                let raw_req = format!("{op}{psot}");
                let req = semver::VersionReq::parse(&raw_req).unwrap();
                let pver: SemverPubgrub = (&req).into();
                let bounding_range = pver.bounding_range();
                for raw_ver in ["18446744073709551615.1.0"] {
                    let ver = semver::Version::parse(raw_ver).unwrap();
                    let mat = req.matches(&ver);
                    if mat != pver.contains(&ver) {
                        eprintln!("{}", ver);
                        eprintln!("{}", req);
                        dbg!(&pver);
                        assert_eq!(mat, pver.contains(&ver));
                    }

                    if mat {
                        assert!(bounding_range.unwrap().contains(&ver));
                    }
                }
            }
        }
    }

    #[test]
    fn test_contains_pre() {
        for op in OPS {
            for psot in [
                "0, <=0.0.1-z0",
                "0, ^0.0.0-0",
                "0.0, <=0.0.1-z0",
                "0.0.1, <=0.0.1-z0",
                "0.9.8-r",
                "0.9.8-r, >0.8",
                "0.9.8-r, ~0.9.1",
                "1, <=0.0.1-z0",
                "1, <=1.0.1-z0",
                "1.0, <=1.0.1-z0",
                "1.0.1, <=1.0.1-z0",
                "1.1, <=1.0.1-z0",
                "0.0.1-r",
                "0.0.2-r",
                "0.0.2-r, ^0.0.1",
            ] {
                let raw_req = format!("{op}{psot}");
                let req = semver::VersionReq::parse(&raw_req).unwrap();
                let pver: SemverPubgrub = (&req).into();
                let bounding_range = pver.bounding_range();
                for raw_ver in ["0.0.0-0", "0.0.1-z0", "0.0.2-z0", "0.9.8-z", "1.0.1-z0"] {
                    let ver = semver::Version::parse(raw_ver).unwrap();
                    let mat = req.matches(&ver);
                    if mat != pver.contains(&ver) {
                        eprintln!("{}", ver);
                        eprintln!("{}", req);
                        dbg!(&pver);
                        assert_eq!(mat, pver.contains(&ver));
                    }

                    if mat {
                        assert!(bounding_range.unwrap().contains(&ver));
                    }
                }
            }
        }
    }
}
