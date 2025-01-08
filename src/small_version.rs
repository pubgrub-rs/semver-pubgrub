use std::sync::Arc;

use zerocopy::{IntoBytes, TryFromBytes};

use crate::VersionLike;

/// A module boundary to emulate unsafe fields.
mod def {
    use std::{ptr::without_provenance, sync::Arc};

    use super::PackedVersion;

    /// A one pointer wide representation of common `semver::Version`s or a `Arc<semver::Version>`
    ///
    /// A `semver::Version` is quite large (5 ptr) to support all kinds of uncommon use cases.
    /// A `Arc<semver::Version>` is 1 aligned ptr, but always allocates and has a cash miss when read.
    /// In practice most versions could be accurately represented by `[u8; 3]`, which is smaller than 1 ptr.
    /// So this type represents common versions as a usize and uses `Arc` for full generality.
    /// The discriminant is hidden in the unused alignment bits of the `Arc`.
    ///
    /// The exact set of versions that are common enough to get a small representation depends on the size of a pointer
    /// and is subject to change between releases.
    #[derive(Debug, Eq)]
    #[repr(packed)]
    pub struct SmallVersion {
        /// The version, either packed into a pointer, or allocated on the heap.
        ///
        /// # Safety
        ///
        /// If and only if the least significant is `0`, `raw` is derived from
        /// [`Arc::into_raw`].
        ///
        /// # Invariants
        ///
        /// If and only if the least significant bit is `1`, the value of `raw`
        /// should be interpreted as having the layout of [`PackedVersion`].
        raw: *const semver::Version,
    }

    impl SmallVersion {
        pub(super) fn from_arc(arc: Arc<semver::Version>) -> Self {
            // Safety: Came from a [`Arc::into_raw`] and the least significant bit is `0` as required.
            let raw = Arc::into_raw(arc);
            assert!(core::mem::align_of::<semver::Version>() > 1);
            assert!(
                raw.addr() & 1 == 0,
                "A valid pointer should have 0 four it's alignment bits"
            );
            Self { raw }
        }
        pub(super) fn from_packed(packed: PackedVersion) -> Self {
            // Safety: Came from a `PackedVersion` and the  least significant is bit `1` as required.
            // With it tagged as coming froma a `PackedVersion` this pointer will never be dereferenced.
            let raw: *const semver::Version = without_provenance(packed.into_raw());
            assert!(
                raw.addr() & 1 == 1,
                "Incorrectly tagged pointer, which will brake safety invariants of `SmallVersion`"
            );
            Self { raw }
        }

        pub(super) fn addr(&self) -> usize {
            self.raw.addr()
        }

        pub(super) fn as_ref<'a>(&'a self) -> Option<&'a semver::Version> {
            self.is_full().then(|| {
                let ptr = self.raw;
                // Safety: The ptr is valid until the last `SmallVersion` referencing it is dropped.
                // We know that this `SmallVersion` cannot be dropped in the lifetime `'a` because we have a `&'a self`.
                // Therefore we know that it is valid to use this ptr as a reference for the lifetime `'a`.
                unsafe { &*ptr }
            })
        }
    }

    impl Clone for SmallVersion {
        fn clone(&self) -> Self {
            if self.is_full() {
                // Safety: We are a `Full` and so where constructed with `Arc::into_raw`,
                // and we are being cloned, so we must increment the strong count to match.
                // We know that the ptr is still valid, because we have a reference to self.
                unsafe { Arc::increment_strong_count(self.raw) };
            }
            Self { raw: self.raw }
        }
    }

    impl Drop for SmallVersion {
        fn drop(&mut self) {
            if self.is_full() {
                // Safety: We are a `Full` and so where constructed with `Arc::into_raw`,
                // and we are being droped, so we must `Arc::from_raw` to drop it.
                // All notes on `Arc::from_raw` are trivially satisfied because we are not doing any type punning.
                unsafe { Arc::from_raw(self.raw) };
            }
        }
    }

    impl SmallVersion {
        pub(super) fn is_full(&self) -> bool {
            !self.is_small()
        }

        pub(super) fn is_small(&self) -> bool {
            // Safety: Given the alignment, a real pointer will have its smallest bit not set.
            // So if that is set then we must not be a pointer we must be a `Packed`.
            assert!(core::mem::align_of::<semver::Version>() > 1);
            self.raw.addr() & 1 == 1
        }
    }
}

pub use def::*;

// Safety: We are a `Arc` in disguise. `Arc` is `Send + Sync` so we are too.
unsafe impl Send for SmallVersion {}
unsafe impl Sync for SmallVersion {}

// A type small enough that we can put four of them in a pointer.
#[cfg(target_pointer_width = "64")]
type Elem = u16;
#[cfg(target_pointer_width = "32")]
type Elem = u8;

/// Is this a pre-release version?
///
/// # Safety
///
/// Unsafe code may expect that the least significant bit of `Pre` is `1`.
#[derive(Debug, PartialEq, Eq, Copy, Clone, Hash, PartialOrd, Ord, TryFromBytes, IntoBytes)]
#[cfg_attr(target_pointer_width = "32", repr(u8))]
#[cfg_attr(target_pointer_width = "64", repr(u16))]
enum Pre {
    /// The pre-release string is "0".
    Smallest = 1,
    /// Not a pre-release.
    Empty = 3,
}

#[test]
fn lsd_is_one() {
    for i in 0..=(Elem::MAX as Elem) {
        let tans: Result<Pre, _> = zerocopy::try_transmute!(i);
        if i & 1 == 0 {
            // for Safty there must not be a reper with a 0 for the least significant bit
            assert!(tans.is_err(), "{i:#x} should not be a valid Pre");
        }
    }
}

#[derive(Debug, Eq, Copy, Clone, IntoBytes, TryFromBytes)]
#[repr(C)]
struct PackedVersion {
    pre: Pre,
    patch: Elem,
    minor: Elem,
    major: Elem,
}

impl std::hash::Hash for PackedVersion {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.into_raw().hash(state);
    }
}

impl PartialEq for PackedVersion {
    fn eq(&self, other: &Self) -> bool {
        self.into_raw() == other.into_raw()
    }
}

impl PartialOrd for PackedVersion {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PackedVersion {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.into_raw().cmp(&other.into_raw())
    }
}

impl PackedVersion {
    fn into_raw(self) -> usize {
        let out = zerocopy::transmute!(self);
        assert_eq!(out & 1, 1);
        out
    }

    fn from_raw(raw: usize) -> Option<Self> {
        let out = zerocopy::try_transmute!(raw).ok()?;
        assert_eq!(raw & 1, 1);
        Some(out)
    }
}

impl From<semver::Version> for SmallVersion {
    fn from(v: semver::Version) -> Self {
        match PackedVersion::try_from(&v) {
            Ok(packed) => Self::from_packed(packed),
            Err(()) => Self::from_arc(Arc::new(v)),
        }
    }
}

impl From<&semver::Version> for SmallVersion {
    fn from(v: &semver::Version) -> Self {
        match PackedVersion::try_from(v) {
            Ok(packed) => Self::from_packed(packed),
            Err(()) => Self::from_arc(Arc::new(v.clone())),
        }
    }
}

#[derive(Debug, Hash)]
enum RefIner<'a> {
    Full(&'a semver::Version),
    Packed(PackedVersion),
}

impl<'a> From<&'a SmallVersion> for RefIner<'a> {
    fn from(v: &'a SmallVersion) -> Self {
        if let Some(v) = v.as_ref() {
            Self::Full(v)
        } else {
            Self::Packed(PackedVersion::from_raw(v.addr()).unwrap())
        }
    }
}

impl SmallVersion {
    pub fn into_version(&self) -> semver::Version {
        match RefIner::from(self) {
            RefIner::Full(v) => v.clone(),
            RefIner::Packed(s) => semver::Version {
                major: s.major(),
                minor: s.minor(),
                patch: s.patch(),
                pre: semver::Prerelease::new(s.pre()).unwrap(),
                build: semver::BuildMetadata::EMPTY,
            },
        }
    }
}

impl std::hash::Hash for SmallVersion {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        RefIner::from(self).hash(state)
    }
}

impl PartialEq for SmallVersion {
    fn eq(&self, other: &Self) -> bool {
        if self.addr() == other.addr() {
            return true;
        }
        let s_ref = RefIner::from(self);
        let o_ref = RefIner::from(other);
        match (s_ref, o_ref) {
            (RefIner::Full(s), RefIner::Full(o)) => s == o,
            _ => false,
        }
    }
}

impl TryFrom<&semver::Version> for PackedVersion {
    type Error = ();
    fn try_from(v: &semver::Version) -> Result<Self, Self::Error> {
        if !v.build.is_empty() {
            return Err(());
        }
        Ok(Self {
            major: v.major.try_into().map_err(|_| ())?,
            minor: v.minor.try_into().map_err(|_| ())?,
            patch: v.patch.try_into().map_err(|_| ())?,
            pre: if v.pre.is_empty() {
                Pre::Empty
            } else if v.pre.as_str() == "0" {
                Pre::Smallest
            } else {
                return Err(());
            },
        })
    }
}

impl PackedVersion {
    fn major(&self) -> u64 {
        self.major as _
    }

    fn minor(&self) -> u64 {
        self.minor as _
    }

    fn patch(&self) -> u64 {
        self.patch as _
    }

    fn pre_is_empty(&self) -> bool {
        self.pre == Pre::Empty
    }

    fn pre(&self) -> &'static str {
        match self.pre {
            Pre::Empty => "",
            Pre::Smallest => "0",
        }
    }
}

impl VersionLike for SmallVersion {
    fn major(&self) -> u64 {
        match RefIner::from(self) {
            RefIner::Full(v) => v.major,
            RefIner::Packed(s) => s.major(),
        }
    }

    fn minor(&self) -> u64 {
        match RefIner::from(self) {
            RefIner::Full(v) => v.minor,
            RefIner::Packed(s) => s.minor(),
        }
    }

    fn patch(&self) -> u64 {
        match RefIner::from(self) {
            RefIner::Full(v) => v.patch,
            RefIner::Packed(s) => s.patch(),
        }
    }

    fn pre(&self) -> &str {
        match RefIner::from(self) {
            RefIner::Full(v) => v.pre.as_str(),
            RefIner::Packed(s) => s.pre(),
        }
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for SmallVersion {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.into_version().serialize(s)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for SmallVersion {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let v = semver::Version::deserialize(deserializer)?;
        Ok(SmallVersion::from(v))
    }
}

impl Ord for SmallVersion {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        if self.is_small() && other.is_small() {
            return self.addr().cmp(&other.addr());
        }
        if self.addr() == other.addr() {
            return core::cmp::Ordering::Equal;
        }
        match self.major().cmp(&other.major()) {
            core::cmp::Ordering::Equal => {}
            ord => return ord,
        }
        match self.minor().cmp(&other.minor()) {
            core::cmp::Ordering::Equal => {}
            ord => return ord,
        }
        match self.patch().cmp(&other.patch()) {
            core::cmp::Ordering::Equal => {}
            ord => return ord,
        }
        match (RefIner::from(self), RefIner::from(other)) {
            (RefIner::Full(s), RefIner::Full(o)) => {
                match s.pre.cmp(&o.pre) {
                    core::cmp::Ordering::Equal => {}
                    ord => {
                        return ord;
                    }
                }
                s.build.cmp(&o.build)
            }
            (RefIner::Full(s), RefIner::Packed(o)) => {
                if o.pre_is_empty() && !s.pre.is_empty() {
                    return core::cmp::Ordering::Less;
                }
                core::cmp::Ordering::Greater
            }
            (RefIner::Packed(s), RefIner::Full(o)) => {
                if s.pre_is_empty() && !o.pre.is_empty() {
                    return core::cmp::Ordering::Greater;
                }
                core::cmp::Ordering::Less
            }
            (RefIner::Packed(_), RefIner::Packed(_)) => unreachable!(),
        }
    }
}

impl PartialOrd for SmallVersion {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl std::fmt::Display for SmallVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.into_version().fmt(f)
    }
}
