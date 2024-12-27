#![no_main]
use std::ops::RangeBounds;

use libfuzzer_sys::fuzz_target;
use semver::VersionReq;
use semver_pubgrub::SemverPubgrub;
use semver_pubgrub_fuzz::{ArbitraryComparator, ArbitraryVersion};

// cargo fuzz run intersection

fn intersection(req: &semver::VersionReq, req2: &semver::VersionReq, ver: &semver::Version) {
    let pver: SemverPubgrub<semver::Version> = req.into();
    let pver2: SemverPubgrub<semver::Version> = req2.into();

    let inter: SemverPubgrub<semver::Version> = pver2.intersection(&pver);
    let mat = req.matches(&ver) && req2.matches(&ver);
    assert_eq!(mat, inter.contains(&ver));
    if mat {
        let bounding_range = pver.bounding_range();
        assert!(bounding_range.unwrap().contains(&ver));
    }
}

fn case(req: Vec<ArbitraryComparator>, req2: Vec<ArbitraryComparator>, ver: ArbitraryVersion) {
    let req: VersionReq = req.into_iter().map(|r| r.to_comparator()).collect();
    let req2: VersionReq = req2.into_iter().map(|r| r.to_comparator()).collect();
    let ver = ver.to_version();
    intersection(&req, &req2, &ver);
}

fuzz_target!(|seed: (
    Vec<ArbitraryComparator>,
    Vec<ArbitraryComparator>,
    ArbitraryVersion
)| case(seed.0, seed.1, seed.2));
