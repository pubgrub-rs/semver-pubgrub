# semver-pubgrub

`semver-pubgrub` converts `VersionReq` requirements from the [semver crate](https://docs.rs/semver/latest/semver) into version sets implementing [PubGrub's `VersionSet` trait](https://docs.rs/pubgrub/latest/pubgrub/trait.VersionSet.html). It currently targets PubGrub's `main` branch.

The semver crate defines which versions match a requirement through `VersionReq::matches`. PubGrub also needs set operations such as complement and intersection. This library provides `SemverPubgrub`, a representation of a `VersionReq` that supports these additional operations.

We work very hard to make sure that `SemverPubgrub` `contains` a `Version` if and only if the `VersionReq` `matches` the `Version`. Where possible, the logic in this library matches (and links back to) the structure of the code in the semver crate. The goal is to be exactly compatible.

Convert a requirement and check release and prerelease membership:

```rust
use semver::{Version, VersionReq};
use semver_pubgrub::SemverPubgrub;

let req = VersionReq::parse("^1.2.3").unwrap();
let set: SemverPubgrub<Version> = (&req).into();

let release = Version::parse("1.3.0").unwrap();
let prerelease = Version::parse("1.3.0-alpha").unwrap();

assert!(set.contains(&release));
assert!(!set.contains(&prerelease));
```

## Other capabilities

- `SemverPubgrub` works with both `semver::Version` and `SmallVersion`, a compact version representation that stores common versions without a heap allocation.
- The optional `serde` feature enables serialization and deserialization of version sets and `SmallVersion`.
- `contains_many()` checks a sorted sequence of versions in a single pass through the versions and the set's intervals.
- `widen_versions()` and `narrow_versions()` preserve membership for supplied versions in sorted order. Widening may add other versions; narrowing may remove them. `simplify()` also preserves membership for supplied versions, but may both add and remove other versions.

## Cargo-style resolution

PubGrub selects one version per package key. Cargo can use multiple semver-incompatible versions of the same crate, so a Cargo-style dependency provider needs to account for that multiplicity in its package keys and dependency constraints. `SemverCompatibility` identifies a version's compatibility group, and `SemverPubgrub::only_one_compatibility_range()` can identify constraints confined to a single group.

## Testing

The [corpus checker](examples/crates-vers/src/main.rs) checks membership and set operations against the checked-in versions and requirements collected from crates.io. Run it from `examples/crates-vers`:

```sh
cargo run --release -- contains
cargo run --release -- intersection difference relation
```

The first command checks version representations and membership. The second also checks the named set operations; these modes can be run individually.

To refresh `data/versions.csv` and `data/requirements.csv` from the crates.io index, run from the same directory:

```sh
cargo run --release -- get-from-index
```

Running the checker without arguments refreshes the corpus and enables all checks.

The [fuzz targets](fuzz/fuzz_targets) exercise generated requirements and versions. With nightly Rust and `cargo-fuzz` installed, following the [cargo-fuzz setup instructions](https://rust-fuzz.github.io/book/cargo-fuzz/setup.html), run from the crate root:

```sh
cargo +nightly fuzz run contains -- -max_total_time=60
```

The time limit bounds the fuzzing run. Other targets are `versions`, `intersection`, `difference`, and `relation`.

Run the existing tests from the crate root:

```sh
cargo test
cargo test --features serde
```

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
