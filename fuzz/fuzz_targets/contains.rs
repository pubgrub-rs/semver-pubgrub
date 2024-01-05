#![no_main]
use libfuzzer_sys::fuzz_target;
use pubgrub::version_set::VersionSet;
use semver_pubgrub::SemverPubgrub;

// cargo fuzz run contains

fuzz_target!(|v: [&str; 2]| {
    let Ok(req) = semver::VersionReq::parse(&v[0]) else {
        return;
    };
    let pver: SemverPubgrub = (&req).into();
    let neg = pver.complement();
    let Ok(ver) = semver::Version::parse(&v[1]) else {
        return;
    };
    assert_eq!(req.matches(&ver), pver.contains(&ver));
    assert_eq!(!req.matches(&ver), neg.contains(&ver));
});
