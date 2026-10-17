#![no_main]
use libfuzzer_sys::fuzz_target;
use pubgrub::{SetRelation, VersionSet};
use semver::{Version, VersionReq};
use semver_pubgrub::SemverPubgrub;
use semver_pubgrub_fuzz::{ArbitraryComparator, ArbitraryVersion};

// cargo fuzz run relation

fn relation(req: &VersionReq, req2: &VersionReq, ver: &Version) {
    let pver: SemverPubgrub<Version> = req.into();
    let pver2: SemverPubgrub<Version> = req2.into();
    let matches = req.matches(ver);
    let matches2 = req2.matches(ver);
    let sets = [
        (pver.complement(), !matches),
        (pver2.complement(), !matches2),
        (pver, matches),
        (pver2, matches2),
        (SemverPubgrub::empty(), false),
        (SemverPubgrub::full(), true),
        (SemverPubgrub::singleton(ver.clone()), true),
    ];

    for (left, left_matches) in &sets {
        for (right, right_matches) in &sets {
            let expected = if left.subset_of(right) {
                SetRelation::Subset
            } else if left.is_disjoint(right) {
                SetRelation::Disjoint
            } else {
                SetRelation::Overlapping
            };
            let relation = left.relation(right);
            assert_eq!(relation, expected, "{left:?} relative to {right:?}");
            assert_eq!(VersionSet::relation(left, right), expected);
            match relation {
                SetRelation::Subset => assert!(!left_matches || *right_matches),
                SetRelation::Disjoint => assert!(!left_matches || !right_matches),
                SetRelation::Overlapping => {}
            }
        }
    }
}

fn case(req: Vec<ArbitraryComparator>, req2: Vec<ArbitraryComparator>, ver: ArbitraryVersion) {
    let req: VersionReq = req.into_iter().map(|r| r.to_comparator()).collect();
    let req2: VersionReq = req2.into_iter().map(|r| r.to_comparator()).collect();
    relation(&req, &req2, &ver.to_version());
}

fuzz_target!(|seed: (
    Vec<ArbitraryComparator>,
    Vec<ArbitraryComparator>,
    ArbitraryVersion
)| case(seed.0, seed.1, seed.2));
