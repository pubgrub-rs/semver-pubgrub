#![no_main]
use std::ops::RangeBounds;

use libfuzzer_sys::fuzz_target;

use pubgrub::VersionSet as _;
use semver::VersionReq;
use semver_pubgrub::SemverPubgrub;
use semver_pubgrub_fuzz::{ArbitraryComparator, ArbitraryVersion};

// cargo fuzz run contains

fn contains(req: &semver::VersionReq, ver: &semver::Version) {
    // println!("{req} |=> {ver}");
    let pver: SemverPubgrub = req.into();
    let neg = pver.complement();
    let mat = req.matches(&ver);
    assert_eq!(mat, pver.contains(&ver), "matches {} |=> {}", req, ver);
    assert_eq!(!mat, neg.contains(&ver), "!matches {} |=> {}", req, ver);

    if mat {
        let bounding_range = pver.bounding_range();
        assert!(bounding_range.unwrap().contains(&ver));
    }
}

fn case(req: Vec<ArbitraryComparator>, ver: ArbitraryVersion) {
    let req: VersionReq = req.into_iter().map(|r| r.to_comparator()).collect();
    let ver = ver.to_version();
    contains(&req, &ver);
}

fuzz_target!(|seed: (Vec<ArbitraryComparator>, ArbitraryVersion)| case(seed.0, seed.1));
