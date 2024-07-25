use std::{
    borrow::Borrow,
    cmp::{max, min},
    fmt::Display,
    ops::Bound,
};

use pubgrub::{Range, VersionSet};
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
#[derive(Debug, PartialEq, Eq, Clone, Hash)]
pub struct SemverPubgrub {
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "range_is_empty"))]
    #[cfg_attr(feature = "serde", serde(default = "Range::empty"))]
    range: Range<Version>,
}

impl SemverPubgrub {
    /// Convert to something that can be used with
    /// [BTreeMap::range](std::collections::BTreeMap::range).
    /// All versions contained in self, will be in the output,
    /// but there may be versions in the output that are not contained in self.
    /// Returns None if the range is empty.
    pub fn bounding_range(&self) -> Option<(Bound<&Version>, Bound<&Version>)> {
        self.range.bounding_range()
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
        let bound = self.range.bounding_range();
        if bound.is_none() {
            return Some(SemverCompatibility::Patch(0));
        }
        let start = bound
            .map(|(s, _)| match s {
                Included(v) | Excluded(v) => v.into(),
                Unbounded => SemverCompatibility::Patch(0),
            })
            .unwrap();
        if let Some(next) = start.next() {
            if let Some((_, pe)) = bound {
                match (pe, next.minimum()) {
                    (Unbounded, _) => return None,
                    (Included(e), m) => {
                        if e >= &m {
                            return None;
                        }
                    }
                    (Excluded(e), m) => {
                        if e > &m {
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
        I: Iterator<Item = BV> + 's,
        BV: Borrow<Version> + 's,
    {
        self.range.contains_many(versions)
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
            range: self.range.simplify(versions),
        }
    }

    /// If the range was constructed using Singleton, return the version from the constructor.
    /// Otherwise, returns [None].
    pub fn as_singleton(&self) -> Option<&Version> {
        self.range.as_singleton()
    }

    /// Iterate over the parts of the range.
    pub fn iter_bounds(&self) -> impl Iterator<Item = (&Bound<Version>, &Bound<Version>)> {
        self.range.iter()
    }
}

impl Display for SemverPubgrub {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        "SemverPubgrub { range: ".fmt(f)?;
        self.range.fmt(f)?;
        " } ".fmt(f)
    }
}

impl From<&SemverCompatibility> for SemverPubgrub {
    fn from(compat: &SemverCompatibility) -> Self {
        let r = Range::from(compat);
        Self { range: r }
    }
}

impl VersionSet for SemverPubgrub {
    type V = Version;

    fn empty() -> Self {
        SemverPubgrub {
            range: Range::empty(),
        }
    }

    fn singleton(v: Self::V) -> Self {
        let singleton = Range::singleton(v);
        SemverPubgrub { range: singleton }
    }

    fn complement(&self) -> Self {
        SemverPubgrub {
            range: self.range.complement(),
        }
    }

    fn intersection(&self, other: &Self) -> Self {
        SemverPubgrub {
            range: self.range.intersection(&other.range),
        }
    }

    fn contains(&self, v: &Self::V) -> bool {
        // This needs to be bug-for-bug compatible with matches_prerelease https://github.com/dtolnay/semver/blob/master/src/eval.rs#L3
        self.range.contains(v)
    }

    fn union(&self, other: &Self) -> Self {
        SemverPubgrub {
            range: self.range.union(&other.range),
        }
    }

    fn is_disjoint(&self, other: &Self) -> bool {
        self.range.is_disjoint(&other.range)
    }

    fn subset_of(&self, other: &Self) -> bool {
        self.range.subset_of(&other.range)
    }
}

impl From<&VersionReq> for SemverPubgrub {
    fn from(req: &VersionReq) -> Self {
        let mut out = SemverPubgrub::full();
        // add to normal the intersection of cmps in req
        for cmp in &req.comparators {
            out = out.intersection(&matches_impl(cmp));
        }
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
            range: between(low, bump_pre),
        };
    }
    let range = if cmp.patch.is_some() {
        between(low, bump_patch)
    } else if cmp.minor.is_some() {
        between(low, bump_minor)
    } else {
        between(low, bump_major)
    };

    SemverPubgrub { range }
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
    let range = Range::from_range_bounds((low_bound, Bound::Unbounded));
    SemverPubgrub { range }
}

fn matches_less(cmp: &Comparator) -> SemverPubgrub {
    // https://github.com/dtolnay/semver/blob/master/src/eval.rs#L90
    let range = Range::strictly_lower_than(Version {
        major: cmp.major,
        minor: cmp.minor.unwrap_or(0),
        patch: cmp.patch.unwrap_or(0),
        pre: if cmp.patch.is_some() {
            cmp.pre.clone()
        } else {
            Prerelease::EMPTY
        },
        build: BuildMetadata::EMPTY,
    });
    SemverPubgrub { range }
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
        let range = between(low, bump_minor);
        return SemverPubgrub { range };
    }
    let range = if cmp.minor.is_some() {
        between(low, bump_minor)
    } else {
        between(low, bump_major)
    };
    SemverPubgrub { range }
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
            Prerelease::EMPTY
        },
        build: BuildMetadata::EMPTY,
    };
    let Some(minor) = cmp.minor else {
        let range = between(low, bump_major);
        return SemverPubgrub { range };
    };

    if cmp.patch.is_none() {
        let range = if cmp.major > 0 {
            between(low, bump_major)
        } else {
            between(low, bump_minor)
        };
        return SemverPubgrub { range };
    };

    let range = if cmp.major > 0 {
        between(low, bump_major)
    } else if minor > 0 {
        between(low, bump_minor)
    } else {
        between(low, bump_patch)
    };
    SemverPubgrub { range }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::{collections::HashSet, ops::RangeBounds};

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
                    let mat = req.matches_prerelease(&ver);
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
    fn test_only_one_compatibility_range() {
        for op in OPS {
            for psot in [
                "0.0.0-r",
                "0.0.0",
                "0.0.1-r",
                "0.0.1",
                "0.1.0-r",
                "0.1.0",
                "1.0.0-r",
                "1.0.0",
                "0.0.0, <=0.0.1",
                "0.0.0-r, <=0.0.1-0",
                "0.0.1, <=0.0.2",
                "0.0.1-r, <=0.0.2-0",
                "0.1.0, <=0.2.0",
                "0.1.0-r, <=0.2.0-0",
                "1.0.0, <=2.0.0",
                "1.0.0-r, <=2.0.0-0",
            ] {
                let raw_req = format!("{op}{psot}");
                let req = semver::VersionReq::parse(&raw_req).unwrap();
                let pver: SemverPubgrub = (&req).into();
                dbg!(raw_req);

                let set: HashSet<_> = [
                    "0.0.0-0", "0.0.0-r", "0.0.0", "0.0.1-0", "0.0.1-r", "0.0.1", "0.0.2-0",
                    "0.0.2-r", "0.0.2", "0.1.0-0", "0.1.0-r", "0.1.0", "0.1.1", "0.2.0-0",
                    "0.2.0-r", "0.2.0", "1.0.0-0", "1.0.0-r", "1.0.0", "1.1.0", "2.0.0-0",
                    "2.0.0-r", "2.0.0", "3.0.0",
                ]
                .into_iter()
                .filter_map(|raw_ver| {
                    let ver = semver::Version::parse(raw_ver).unwrap();
                    let mat = req.matches_prerelease(&ver);
                    if mat != pver.contains(&ver) {
                        eprintln!("{}", ver);
                        eprintln!("{}", req);
                        dbg!(&pver);
                        assert_eq!(mat, pver.contains(&ver));
                    }
                    let cap: SemverCompatibility = (&ver).into();
                    mat.then_some(cap)
                })
                .collect();

                let bounding_range = pver.only_one_compatibility_range();
                assert_eq!(set.len() <= 1, bounding_range.is_some());
            }
        }
    }

    #[test]
    fn test_only_one_compatibility_range_singletons() {
        let raw_vers = [
            "0.0.0-0", "0.0.0-r", "0.0.0", "0.0.1-0", "0.0.1-r", "0.0.1", "0.0.2-0", "0.0.2-r",
            "0.0.2", "0.1.0-0", "0.1.0-r", "0.1.0", "0.1.1", "0.2.0-0", "0.2.0-r", "0.2.0",
            "1.0.0-0", "1.0.0-r", "1.0.0", "1.1.0", "2.0.0-0", "2.0.0-r", "2.0.0", "3.0.0",
        ];
        let vers = raw_vers.map(|raw_ver| semver::Version::parse(raw_ver).unwrap());
        let reqs = vers.clone().map(|v| SemverPubgrub::singleton(v.clone()));
        for pver in &reqs {
            pver.as_singleton().unwrap();
            // Singletons can only match one thing so they definitely only match one compatibility range.
            pver.only_one_compatibility_range().unwrap();
        }

        let req_unions = reqs
            .iter()
            .flat_map(|req1| reqs.iter().map(|req2: &SemverPubgrub| req1.union(req2)));

        for preq in req_unions {
            let set: HashSet<SemverCompatibility> = vers
                .iter()
                .filter(|ver| preq.contains(&ver))
                .map(|ver| ver.into())
                .collect();
            let only_one_comp = preq.only_one_compatibility_range();
            dbg!(&preq, &set, only_one_comp);
            assert_eq!(set.len() <= 1, only_one_comp.is_some());
            if only_one_comp.is_none() {
                assert!(preq.as_singleton().is_none());
            }
            if preq.as_singleton().is_some() {
                assert_eq!(set.len(), 1);
            }
        }
    }
}
