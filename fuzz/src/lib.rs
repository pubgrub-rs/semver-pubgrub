use proptest::{strategy::{Strategy, Just}, arbitrary::any, prop_oneof};



pub fn prerelease_strategy() -> impl Strategy<Value = semver::Prerelease> {
    proptest::collection::vec(r"(?-u:0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*)", 0..=4)
        .prop_map(|s| s.join(".").parse().unwrap())
}

pub fn prerelease_build() -> impl Strategy<Value = semver::BuildMetadata> {
    proptest::collection::vec(r"[0-9a-zA-Z-]+", 0..=2).prop_map(|s| s.join(".").parse().unwrap())
}

pub fn version_strategy() -> impl Strategy<Value = semver::Version> {
    // r"(?-u:(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(-((0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*)(\.(0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*))*))?(\+([0-9a-zA-Z-]+(\.[0-9a-zA-Z-]+)*))?)"

    (any::<[u64; 3]>(), prerelease_strategy(), prerelease_build()).prop_map(|(ver, pre, build)| {
        semver::Version {
            major: ver[0],
            minor: ver[1],
            patch: ver[2],
            pre,
            build: build.parse().unwrap(),
        }
    })
}

pub fn op_strategy() -> impl Strategy<Value = semver::Op> {
    prop_oneof![
        Just(semver::Op::Caret),
        Just(semver::Op::Tilde),
        Just(semver::Op::Greater),
        Just(semver::Op::GreaterEq),
        Just(semver::Op::Less),
        Just(semver::Op::LessEq),
        Just(semver::Op::Exact),
    ]
}

pub fn req_strategy() -> impl Strategy<Value = semver::VersionReq> {
    proptest::collection::vec(
        (
            op_strategy(),
            prop_oneof![
                any::<[u64; 1]>().prop_map(|[major]| (
                    major,
                    None,
                    None,
                    semver::Prerelease::EMPTY
                )),
                any::<[u64; 2]>().prop_map(|[major, minor]| (
                    major,
                    Some(minor),
                    None,
                    semver::Prerelease::EMPTY
                )),
                any::<[u64; 3]>().prop_map(|[major, minor, patch]| (
                    major,
                    Some(minor),
                    Some(patch),
                    semver::Prerelease::EMPTY
                )),
                (any::<[u64; 3]>(), prerelease_strategy()).prop_map(
                    |([major, minor, patch], pre)| (
                        major,
                        Some(minor),
                        Some(patch),
                        semver::Prerelease::new(&pre).unwrap()
                    )
                ),
            ],
        )
            .prop_map(|(op, ver)| semver::Comparator {
                op,
                major: ver.0,
                minor: ver.1,
                patch: ver.2,
                pre: ver.3,
            }),
        1..=3,
    )
    .prop_map(|v| v.into_iter().collect::<semver::VersionReq>())
}