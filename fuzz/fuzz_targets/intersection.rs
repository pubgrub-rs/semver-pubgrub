#![no_main]
use std::ops::RangeBounds;

use libfuzzer_sys::fuzz_target;
use proptest::strategy::Strategy;
use proptest::test_runner::{Config, RngAlgorithm, TestRng, TestRunner};
use proptest::test_runner::{TestCaseError, TestError};
use proptest::{prop_assert, prop_assert_eq};
use pubgrub::version_set::VersionSet;
use semver_pubgrub::SemverPubgrub;
use semver_pubgrub_fuzz::{req_strategy, version_strategy};

// cargo fuzz run intersection

fn intersection(
    req: &semver::VersionReq,
    req2: &semver::VersionReq,
    ver: &semver::Version,
) -> Result<(), TestCaseError> {
    let pver: SemverPubgrub = req.into();
    let pver2: SemverPubgrub = req2.into();

    let inter: SemverPubgrub = pver2.intersection(&pver);
    let mat = req.matches(&ver) && req2.matches(&ver);
    prop_assert_eq!(mat, inter.contains(&ver));

    let bounding_range = pver.bounding_range();
    if bounding_range.is_some_and(|b| !b.contains(&ver)) {
        prop_assert!(!mat);
    }
    if mat {
        prop_assert!(bounding_range.unwrap().contains(&ver));
    }
    Ok(())
}

fn case(seed: &[u8]) {
    let mut test_runner = TestRunner::new_with_rng(
        Config {
            cases: 1,
            failure_persistence: None,
            ..Config::default()
        },
        TestRng::from_seed(RngAlgorithm::PassThrough, seed),
    );
    let strategy = &(req_strategy(), req_strategy(), version_strategy());
    let new_tree = strategy.new_tree(&mut test_runner).unwrap();
    let result = test_runner.run_one(new_tree, |v| intersection(&v.0, &v.1, &v.2));

    if let Err(TestError::Fail(_, (req, req2, ver))) = result {
        panic!("Found minimal failing case: {req} U {req2} |=> {ver}");
    }
}

fuzz_target!(|seed: &[u8]| case(seed));
