use std::{
    borrow::Borrow,
    cmp::{max, min},
    fmt::Display,
    ops::Bound,
};

use pubgrub::{range::Range, version_set::VersionSet};
use semver::{BuildMetadata, Comparator, Op, Prerelease, Version, VersionReq};

mod bump_helpers;
mod semver_compatibility;

pub use semver_compatibility::SemverCompatibility;

use bump_helpers::{between, bump_major, bump_minor, bump_patch, bump_pre};

use crate::bump_helpers::simplified_bounds_to_normal;

#[cfg(feature = "serde")]
fn range_is_empty(r: &Range<Version>) -> bool {
    r == &Range::empty()
}

/// This needs to be bug-for-bug compatible with https://github.com/dtolnay/semver/blob/master/src/eval.rs
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct SemverPubgrub {
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "range_is_empty"))]
    #[cfg_attr(feature = "serde", serde(default = "Range::empty"))]
    normal: Range<Version>,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "range_is_empty"))]
    #[cfg_attr(feature = "serde", serde(default = "Range::empty"))]
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
        let Some((ns, ne)) = self.normal.bounding_range() else {
            return self.pre.bounding_range();
        };
        let Some((ps, pe)) = self.pre.bounding_range() else {
            return Some((ns, ne));
        };
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

    /// Whether cargo would require that only one package matche this range.
    ///
    /// While this crate matches the semantics of `semver`
    /// and implements the traits from `pubgrub`, there is an important difference in semantics.
    /// `pubgrub` assumes that only one version of each package can be selected.
    /// Whereas cargo allows one version per compatibility range to be selected.
    /// In general to lower cargo semantics to `pubgrub`
    /// you need to add synthetic packages to allow this multiplicity.
    /// (Currently look at the `pubgrub` guide for how to do this.
    /// Eventually there will be a crate for this.)
    /// But that's only "in general", in specific most requirements used in the rust ecosystem
    /// can skip these synthetic packages because they
    /// can only match one compatibility range anyway.
    /// This function returns the compatibility range if self can only one.
    pub fn only_one_compatibility_range(&self) -> Option<SemverCompatibility> {
        use Bound::*;
        let normal_bound = self.normal.bounding_range();
        let pre_bound = self.pre.bounding_range();
        if normal_bound.is_none() && pre_bound.is_none() {
            return Some(SemverCompatibility::Patch(0));
        }
        let normal_start = normal_bound.map(|(s, _)| match s {
            Included(v) | Excluded(v) => v.into(),
            Unbounded => SemverCompatibility::Patch(0),
        });
        let pre_start = pre_bound.map(|(s, _)| match s {
            Included(v) | Excluded(v) => v.into(),
            Unbounded => SemverCompatibility::Patch(0),
        });
        if normal_start.is_some() && pre_start.is_some() && normal_start != pre_start {
            return None;
        }
        let start = normal_start.or(pre_start).unwrap();
        if let Some(next) = start.next() {
            if let Some((_, pe)) = pre_bound {
                match (pe, next.minimum()) {
                    (Unbounded, _) => return None,
                    (Included(e) | Excluded(e), m) => {
                        if e >= &m {
                            return None;
                        }
                    }
                }
            }
            if let Some((_, ne)) = normal_bound {
                match (ne, next.canonical()) {
                    (Unbounded, _) => return None,
                    (Included(e) | Excluded(e), m) => {
                        if e >= &m {
                            return None;
                        }
                    }
                }
            }
        }

        Some(start)
    }

    /// Returns true if the this Range contains the specified values.
    ///
    /// The `versions` iterator must be sorted.
    /// Functionally equivalent to `versions.map(|v| self.contains(v))`.
    /// Except it runs in `O(size_of_range + len_of_versions)` not `O(size_of_range * len_of_versions)`
    pub fn contains_many<'s, I, BV>(&'s self, versions: I) -> impl Iterator<Item = bool> + 's
    where
        I: Iterator<Item = BV> + Clone + 's,
        BV: Borrow<Version> + 's,
    {
        let mut n_iter = self
            .normal
            .contains_many(versions.clone().filter(|v| v.borrow().pre.is_empty()));
        let mut p_iter = self
            .pre
            .contains_many(versions.clone().filter(|v| !v.borrow().pre.is_empty()));
        versions.filter_map(move |v| {
            if v.borrow().pre.is_empty() {
                n_iter.next()
            } else {
                p_iter.next()
            }
        })
    }

    /// Returns a simpler Range that contains the same versions
    ///
    /// For every one of the Versions provided in versions the existing range and
    /// the simplified range will agree on whether it is contained.
    /// The simplified version may include or exclude versions that are not in versions as the implementation wishes.
    /// For example:
    ///  - If all the versions are contained in the original than the range will be simplified to `full`.
    ///  - If none of the versions are contained in the original than the range will be simplified to `empty`.
    ///
    /// If versions are not sorted the correctness of this function is not guaranteed.
    pub fn simplify<'v, I, BV>(&self, versions: I) -> Self
    where
        I: Iterator<Item = BV> + Clone + 'v,
        BV: Borrow<Version> + 'v,
    {
        Self {
            normal: self
                .normal
                .simplify(versions.clone().filter(|v| v.borrow().pre.is_empty())),
            pre: self
                .pre
                .simplify(versions.filter(|v| !v.borrow().pre.is_empty())),
        }
    }

    /// Iterate over the parts of the range that can match normal releases.
    pub fn iter_normal(&self) -> impl Iterator<Item = (&Bound<Version>, &Bound<Version>)> {
        self.normal.iter()
    }

    /// Iterate over the parts of the range that can match pre-releases.
    pub fn iter_pre(&self) -> impl Iterator<Item = (&Bound<Version>, &Bound<Version>)> {
        self.pre.iter()
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

impl From<&SemverCompatibility> for SemverPubgrub {
    fn from(compat: &SemverCompatibility) -> Self {
        let r = Range::from(compat);
        Self {
            normal: r.clone(),
            pre: r,
        }
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

    fn union(&self, other: &Self) -> Self {
        SemverPubgrub {
            normal: self.normal.union(&other.normal),
            pre: self.pre.union(&other.pre),
        }
    }

    fn is_disjoint(&self, other: &Self) -> bool {
        self.normal.is_disjoint(&other.normal) && self.pre.is_disjoint(&other.pre)
    }

    fn subset_of(&self, other: &Self) -> bool {
        self.normal.subset_of(&other.normal) && self.pre.subset_of(&other.pre)
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
        normal: simplified_to_normal(&normal),
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
        normal: simplified_to_normal(&out),
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
        normal: simplified_to_normal(&out),
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
            normal: simplified_to_normal(&out),
            pre: out,
        };
    }
    let normal = if cmp.minor.is_some() {
        between(low, bump_minor)
    } else {
        between(low, bump_major)
    };
    SemverPubgrub {
        normal: simplified_to_normal(&normal),
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
            normal: simplified_to_normal(&out),
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
            normal: simplified_to_normal(&out),
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
        normal: simplified_to_normal(&out),
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

fn simplified_to_normal(input: &Range<Version>) -> Range<Version> {
    input
        .iter()
        .map(|(from, to)| simplified_bounds_to_normal((from.clone(), to.clone())))
        .map(|bounds| Range::from_range_bounds(bounds))
        .reduce(|a, b| a.union(&b))
        .unwrap_or_else(Range::empty)
}

#[cfg(test)]
mod test {
    use super::*;
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
