#![no_main]
use std::ops::RangeBounds;

use libfuzzer_sys::fuzz_target;

use semver::{Version, VersionReq};
use semver_pubgrub::SemverPubgrub;
use semver_pubgrub_fuzz::{ArbitraryComparator, ArbitraryVersion};

// cargo fuzz run contains

fn contains(req: &semver::VersionReq, ver: &semver::Version) {
    // println!("{req} |=> {ver}");
    let pver: SemverPubgrub<Version> = req.into();
    let neg = pver.complement();
    let mat = req.matches(&ver);
    assert_eq!(mat, pver.contains(&ver), "matches {} |=> {}", req, ver);
    assert_eq!(!mat, neg.contains(&ver), "!matches {} |=> {}", req, ver);

    if mat {
        let bounding_range = pver.bounding_range();
        assert!(bounding_range.unwrap().contains(&ver));
    }
}

fn contains_many(pver: &SemverPubgrub<Version>, versions: &[Version]) {
    let expected: Vec<_> = versions.iter().map(|ver| pver.contains(ver)).collect();
    assert_eq!(
        pver.contains_many(versions.iter()).collect::<Vec<_>>(),
        expected
    );
}

fn case(req: Vec<ArbitraryComparator>, versions: Vec<ArbitraryVersion>) {
    let req: VersionReq = req.into_iter().map(|r| r.to_comparator()).collect();
    let mut versions: Vec<_> = versions.into_iter().map(|v| v.to_version()).collect();
    versions.sort();
    for ver in &versions {
        contains(&req, ver);
    }

    let pver: SemverPubgrub<Version> = (&req).into();
    contains_many(&pver, &versions);
    contains_many(&pver.complement(), &versions);
    contains_many(&SemverPubgrub::empty(), &versions);
    contains_many(&SemverPubgrub::full(), &versions);
    for ver in &versions {
        contains_many(&SemverPubgrub::singleton(ver.clone()), &versions);
    }
}

fuzz_target!(|seed: (Vec<ArbitraryComparator>, Vec<ArbitraryVersion>)| case(seed.0, seed.1));
