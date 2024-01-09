use std::fmt::Display;

use pubgrub::{range::Range, version_set::VersionSet};
use semver::{BuildMetadata, Comparator, Op, Prerelease, Version, VersionReq};

/// This needs to be bug-for-bug compatible with https://github.com/dtolnay/semver/blob/master/src/eval.rs

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct SemverPubgrub {
    normal: Range<Version>,
    pre: Range<Version>,
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
        if v.build.is_empty() {
            if v.pre.is_empty() {
                self.normal.contains(v)
            } else {
                self.pre.contains(v)
            }
        } else {
            self.contains(&Version {
                major: v.major,
                minor: v.minor,
                patch: v.patch,
                pre: v.pre.clone(),
                build: BuildMetadata::EMPTY,
            })
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
    if !cmp.pre.is_empty() {
        return SemverPubgrub::singleton(Version {
            major: cmp.major,
            minor: cmp.minor.expect("pre without minor"),
            patch: cmp.patch.expect("pre without patch"),
            pre: cmp.pre.clone(),
            build: BuildMetadata::EMPTY,
        });
    }
    let low = Version {
        major: cmp.major,
        minor: cmp.minor.unwrap_or(0),
        patch: cmp.patch.unwrap_or(0),
        pre: Prerelease::EMPTY,
        build: BuildMetadata::EMPTY,
    };
    if let Some(patch) = cmp.patch {
        let minor = cmp.minor.expect("patch without minor");
        let normal = match patch.checked_add(1) {
            Some(new) => Range::between(low, Version::new(cmp.major, minor, new)),
            None => match minor.checked_add(1) {
                Some(new) => Range::between(low, Version::new(cmp.major, new, 0)),
                None => match cmp.major.checked_add(1) {
                    Some(new) => Range::between(low, Version::new(new, 0, 0)),
                    None => Range::higher_than(low),
                },
            },
        };
        return SemverPubgrub {
            normal,
            pre: Range::empty(),
        };
    }
    if let Some(minor) = cmp.minor {
        let normal = match minor.checked_add(1) {
            Some(new) => Range::between(low, Version::new(cmp.major, new, 0)),
            None => match cmp.major.checked_add(1) {
                Some(new) => Range::between(low, Version::new(new, 0, 0)),
                None => Range::higher_than(low),
            },
        };
        return SemverPubgrub {
            normal,
            pre: Range::empty(),
        };
    }
    let normal = match cmp.major.checked_add(1) {
        Some(new) => Range::between(low, Version::new(new, 0, 0)),
        None => Range::higher_than(low),
    };
    SemverPubgrub {
        normal,
        pre: Range::empty(),
    }
}

fn matches_greater(cmp: &Comparator) -> SemverPubgrub {
    // https://github.com/dtolnay/semver/blob/master/src/eval.rs#L64
    let out = Range::strictly_higher_than(Version {
        major: cmp.major,
        minor: cmp.minor.unwrap_or(!0),
        patch: cmp.patch.unwrap_or(!0),
        pre: cmp.pre.clone(),
        build: BuildMetadata::EMPTY,
    });
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
        let minor = cmp.minor.expect("patch without minor");
        let out = match minor.checked_add(1) {
            Some(new) => Range::between(low, Version::new(cmp.major, new, 0)),
            None => match cmp.major.checked_add(1) {
                Some(new) => Range::between(low, Version::new(new, 0, 0)),
                None => Range::higher_than(low),
            },
        };
        return SemverPubgrub {
            normal: out.clone(),
            pre: out,
        };
    }
    if let Some(minor) = cmp.minor {
        let normal = match minor.checked_add(1) {
            Some(new) => Range::between(low, Version::new(cmp.major, new, 0)),
            None => match cmp.major.checked_add(1) {
                Some(new) => Range::between(low, Version::new(new, 0, 0)),
                None => Range::higher_than(low),
            },
        };
        return SemverPubgrub {
            normal,
            pre: Range::empty(),
        };
    }
    let normal = match cmp.major.checked_add(1) {
        Some(new) => Range::between(low, Version::new(new, 0, 0)),
        None => Range::higher_than(low),
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
        pre: cmp.pre.clone(),
        build: BuildMetadata::EMPTY,
    };
    let Some(minor) = cmp.minor else {
        let out = match cmp.major.checked_add(1) {
            Some(new) => Range::between(low, Version::new(new, 0, 0)),
            None => Range::higher_than(low),
        };
        return SemverPubgrub {
            normal: out.clone(),
            pre: out,
        };
    };

    let Some(patch) = cmp.patch else {
        let out = if cmp.major > 0 {
            match cmp.major.checked_add(1) {
                Some(new) => Range::between(low, Version::new(new, 0, 0)),
                None => Range::higher_than(low),
            }
        } else {
            match minor.checked_add(1) {
                Some(new) => Range::between(low, Version::new(0, new, 0)),
                None => Range::between(low, Version::new(1, 0, 0)),
            }
        };
        return SemverPubgrub {
            normal: out.clone(),
            pre: out,
        };
    };
    let out = if cmp.major > 0 {
        match cmp.major.checked_add(1) {
            Some(new) => Range::between(low, Version::new(new, 0, 0)),
            None => Range::higher_than(low),
        }
    } else if minor > 0 {
        match minor.checked_add(1) {
            Some(new) => Range::between(low, Version::new(0, new, 0)),
            None => Range::between(low, Version::new(1, 0, 0)),
        }
    } else {
        match patch.checked_add(1) {
            Some(new) => Range::between(low, Version::new(0, 0, new)),
            None => Range::between(low, Version::new(0, 1, 0)),
        }
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

#[test]
fn test_into_overflow() {
    for pre in ["^", "~", "=", "<", ">", "<=", ">="] {
        for numbs in [
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
            let req = semver::VersionReq::parse(&format!("{pre}{numbs}")).unwrap();
            println!("{req}");
            let _: SemverPubgrub = (&req).into();
        }
    }
}

#[test]
fn test_contains_pre() {
    for pre in ["^", "~", "<", "<=", ">", ">=", "="] {
        for psot in [
            "0, <=0.0.1-z0",
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
        ] {
            let raw_req = format!("{pre}{psot}");
            let req = semver::VersionReq::parse(&raw_req).unwrap();
            let pver: SemverPubgrub = (&req).into();
            for raw_ver in ["0.0.1-z0", "0.9.8-z", "1.0.1-z0"] {
                let ver = semver::Version::parse(raw_ver).unwrap();
                if req.matches(&ver) != pver.contains(&ver) {
                    eprintln!("{}", ver);
                    eprintln!("{}", req);
                    dbg!(&pver);
                    assert_eq!(req.matches(&ver), pver.contains(&ver));
                }
            }
        }
    }
}
