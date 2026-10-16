#![no_main]
use std::ops::RangeBounds;

use libfuzzer_sys::fuzz_target;
use semver::{Version, VersionReq};
use semver_pubgrub::SemverPubgrub;
use semver_pubgrub_fuzz::{ArbitraryComparator, ArbitraryVersion};

// cargo fuzz run difference

fn difference(req: &VersionReq, req2: &VersionReq, ver: &Version) {
    let pver: SemverPubgrub<Version> = req.into();
    let pver2: SemverPubgrub<Version> = req2.into();
    let matches = req.matches(ver);
    let matches2 = req2.matches(ver);

    for (left, right, mat) in [
        (&pver, &pver2, matches && !matches2),
        (&pver2, &pver, matches2 && !matches),
    ] {
        let diff = left.difference(right);
        assert_eq!(diff, left.intersection(&right.complement()));
        assert_eq!(mat, diff.contains(ver));
        if mat {
            assert!(diff.bounding_range().unwrap().contains(ver));
        }
    }
}

fn case(req: Vec<ArbitraryComparator>, req2: Vec<ArbitraryComparator>, ver: ArbitraryVersion) {
    let req: VersionReq = req.into_iter().map(|r| r.to_comparator()).collect();
    let req2: VersionReq = req2.into_iter().map(|r| r.to_comparator()).collect();
    difference(&req, &req2, &ver.to_version());
}

fuzz_target!(|seed: (
    Vec<ArbitraryComparator>,
    Vec<ArbitraryComparator>,
    ArbitraryVersion
)| case(seed.0, seed.1, seed.2));
