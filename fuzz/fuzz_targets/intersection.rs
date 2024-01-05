#![no_main]
use libfuzzer_sys::fuzz_target;
use pubgrub::version_set::VersionSet;
use semver_pubgrub::SemverPubgrub;

// cargo fuzz run intersection

fuzz_target!(|v: [&str; 3]| {
    let Ok(req) = semver::VersionReq::parse(&v[0]) else {
        return;
    };
    let pver: SemverPubgrub = (&req).into();
    let Ok(req2) = semver::VersionReq::parse(&v[1]) else {
        return;
    };
    let pver2: SemverPubgrub = (&req2).into();
    let Ok(ver) = semver::Version::parse(&v[2]) else {
        return;
    };

    let inter: SemverPubgrub = pver2.intersection(&pver);
    assert_eq!(
        req.matches(&ver) && req2.matches(&ver),
        inter.contains(&ver)
    );
});
