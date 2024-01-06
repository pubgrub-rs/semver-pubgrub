This provides compatibility between `VersionReq` from the [semver crate](https://docs.rs/semver/latest/semver) and the `VersionSet` trait from [pubgrub crate](https://docs.rs/pubgrub/latest/pubgrub/) (well the unreleased v3/dev branch).

The semver crate defines for the Cargo ecosystem when a version like `1.2.3-alpha` matches a requirement like `^1.x`. It defines a `matches` method for comparing a `Version` with a `VersionReq`. The PubGrub dependency resolution algorithm requires more operations on its version requirements. The fundamental ones are a negation and intersection. This library provides the `SemverPubgrub` struct witch is a representation of a `VersionReq` that supports these additional operations.

We work very hard to make sure that `SemverPubgrub` `contains` a `Version` if and only if the `VersionReq` `matches` the `Version`. Where possible, the logic in this library matches (and links back to) the structure of the code in the semver crate. The goal is to be exactly compatible.

We have code that [checks this](examples/crates-vers/src/main.rs) for all versions currently on crates.io. But crates.io is not particularly creative with its versions or their requirements, so we also have [fuzz testing](fuzz/fuzz_targets). Unfortunately this fuzz testing is not available on Windows, so we also have proptest and snapshot testing (still to be written).


## To the maintainers of the semver crate, thank you!
This project is built on the wonderful work in the semver crate. Full credit and appreciation to the [semver contributors](https://github.com/dtolnay/semver/graphs/contributors).
All parsing as well as the semantics of `matches`/`contains` are developed and maintained in that semver crate. That crate is available under "MIT OR Apache-2.0" licensing.

## Contributing

Discussion and development happens here on GitHub and on our
[Zulip stream](https://rust-lang.zulipchat.com/#narrow/stream/260232-t-cargo.2FPubGrub). This is an important project with a lot of work still to do. It is also a small self-contained project with a clear definition of correct behavior.
Please join in!

Remember to always be considerate of others,
who may have different native languages, cultures and experiences.
We want everyone to feel welcomed,
let us know with a private message on Zulip if you don't feel that way.