#![no_main]

use libfuzzer_sys::fuzz_target;
use semver::Version;
use semver_pubgrub::{SmallVersion, VersionLike};
use semver_pubgrub_fuzz::ArbitraryVersion;

// cargo fuzz run versions

fn versions(v1: &semver::Version, v2: &semver::Version) {
    let s1: SmallVersion = v1.into();
    assert_eq!(v1.major, s1.major());
    assert_eq!(v1.minor, s1.minor());
    assert_eq!(v1.patch, s1.patch());
    assert_eq!(v1.pre.as_str(), s1.pre());
    // small version round trips
    let v: Version = s1.into_version();
    assert_eq!(v1, &v);
    let s2: SmallVersion = v2.into();
    assert_eq!(s1.cmp(&s2), v1.cmp(v2));
    assert_eq!(s1 == s2, v1 == v2);
}

fn case(v1: ArbitraryVersion, v2: ArbitraryVersion) {
    let v1 = v1.to_version();
    let v2 = v2.to_version();
    versions(&v1, &v2);
}

fuzz_target!(|seed: (ArbitraryVersion, ArbitraryVersion)| case(seed.0, seed.1));
