use std::sync::Arc;

use crate::VersionLike;

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
pub struct SmallVersion(*const semver::Version);

/// polyfill for `std::ptr::without_provenance`
pub fn without_provenance(addr: usize) -> *const semver::Version {
    std::ptr::null::<semver::Version>().with_addr(addr)
}

#[derive(Debug, PartialEq, Eq, Clone, Hash, PartialOrd, Ord)]
struct Small(usize);

impl SmallVersion {
    fn new_full(v: semver::Version) -> Self {
        // Safety: Remember that owning a `SmallVersion` that is a `Full` is equivalent to owning an `Arc`.
        let out = Self(Arc::into_raw(Arc::new(v)));
        // Safety: We always check that a newly constructed `SmallVersion` has been tagged correctly
        // before we return it to code outside this module.
        assert!(out.is_full());
        out
    }
}

impl Drop for SmallVersion {
    fn drop(&mut self) {
        if let RefIner::Full(ptr) = RefIner::from(&*self) {
            // Safety: We are a `Full` and so where constructed with `Arc::into_raw`,
            // and we are being droped, so we must `Arc::from_raw` to drop it.
            // All notes on `Arc::from_raw` are trivially satisfied because we are not doing any type punning.
            unsafe { Arc::from_raw(ptr) };
        }
    }
}

impl Clone for SmallVersion {
    fn clone(&self) -> Self {
        if let RefIner::Full(ptr) = RefIner::from(self) {
            // Safety: We are a `Full` and so where constructed with `Arc::into_raw`,
            // and we are being cloned, so we must increment the strong count to match.
            // We know that the ptr is still valid, because we have a reference to self.
            unsafe { Arc::increment_strong_count(ptr) };
        }
        Self(self.0)
    }
}

impl From<semver::Version> for SmallVersion {
    fn from(v: semver::Version) -> Self {
        match (&v).try_into() {
            Ok(Small(s)) => {
                let out = SmallVersion(without_provenance(s));
                // Safety: We always check that a newly constructed `SmallVersion` has been tagged correctly
                // before we return it to code outside this module.
                assert!(out.is_small());
                out
            }
            Err(()) => SmallVersion::new_full(v),
        }
    }
}

impl From<&semver::Version> for SmallVersion {
    fn from(v: &semver::Version) -> Self {
        match v.try_into() {
            Ok(Small(s)) => {
                let out = SmallVersion(without_provenance(s));
                // Safety: We always check that a newly constructed `SmallVersion` has been tagged correctly
                // before we return it to code outside this module.
                assert!(out.is_small());
                out
            }
            Err(()) => SmallVersion::new_full(v.clone()),
        }
    }
}

#[derive(Debug, Hash)]
enum RefIner<'a> {
    Full(&'a semver::Version),
    Small(Small),
}

impl<'a> From<&'a SmallVersion> for RefIner<'a> {
    fn from(v: &'a SmallVersion) -> Self {
        if v.is_full() {
            let ptr = v.0;
            // Safety: The ptr is valid until the last `SmallVersion` referencing it is dropped.
            // We know that this `SmallVersion` cannot be dropped in the lifetime `'a` because we have a `&'a self`.
            // Therefore we know that it is valid to use this ptr as a reference for the lifetime `'a`.
            Self::Full(unsafe { &*ptr })
        } else {
            Self::Small(Small(v.0.addr()))
        }
    }
}

impl SmallVersion {
    pub fn into_version(&self) -> semver::Version {
        match RefIner::from(self) {
            RefIner::Full(v) => v.clone(),
            RefIner::Small(s) => semver::Version {
                major: s.major(),
                minor: s.minor(),
                patch: s.patch(),
                pre: semver::Prerelease::new(s.pre()).unwrap(),
                build: semver::BuildMetadata::EMPTY,
            },
        }
    }

    fn is_full(&self) -> bool {
        !self.is_small()
    }

    fn is_small(&self) -> bool {
        // Safety: Given the alignment, a real pointer will have its smallest bit not set.
        // So if that is set then we must not be a pointer we must be a `Small`.
        assert!(core::mem::align_of::<semver::Version>() > 1);
        self.0.addr() & 1 == 1
    }
}

impl std::hash::Hash for SmallVersion {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        RefIner::from(self).hash(state)
    }
}

impl PartialEq for SmallVersion {
    fn eq(&self, other: &Self) -> bool {
        if std::ptr::eq(self.0, other.0) {
            return true;
        }
        let s_ref = RefIner::from(self);
        let o_ref = RefIner::from(other);
        let (RefIner::Full(s), RefIner::Full(o)) = (s_ref, o_ref) else {
            return false;
        };
        s == o
    }
}

// A type small enough that we can put four of them in a pointer.
#[cfg(target_pointer_width = "64")]
type Elem = u16;
#[cfg(target_pointer_width = "32")]
type Elem = u8;

impl TryFrom<&semver::Version> for Small {
    type Error = ();
    fn try_from(v: &semver::Version) -> Result<Self, Self::Error> {
        if !v.build.is_empty() {
            return Err(());
        }

        let to_be = |n: u64| -> Result<usize, Self::Error> {
            Ok(Elem::try_from(n).map_err(|_| ())? as usize)
        };

        let mut ret = to_be(v.major)?;
        ret <<= Elem::BITS as usize;
        ret |= to_be(v.minor)?;
        ret <<= Elem::BITS as usize;
        ret |= to_be(v.patch)?;
        ret <<= Elem::BITS as usize;

        ret |= if v.pre.is_empty() {
            Elem::MAX as usize
        } else if v.pre.as_str() == "0" {
            (Elem::MAX / 2) as usize
        } else {
            return Err(());
        };
        // Safety: Check that a newly constructed `Small` has been tagged correctly.
        assert_ne!(ret & 1, 0);
        Ok(Self(ret))
    }
}

impl Small {
    fn major(&self) -> u64 {
        (self.0 >> (3 * Elem::BITS)) as _
    }

    fn minor(&self) -> u64 {
        (self.0 >> (2 * Elem::BITS) as usize & Elem::MAX as usize) as _
    }

    fn patch(&self) -> u64 {
        (self.0 >> (1 * Elem::BITS) as usize & Elem::MAX as usize) as _
    }

    fn pre_is_empty(&self) -> bool {
        self.0 & (Elem::MAX as usize) == (Elem::MAX as usize)
    }

    fn pre(&self) -> &'static str {
        if self.pre_is_empty() {
            ""
        } else {
            "0"
        }
    }
}

impl VersionLike for SmallVersion {
    fn major(&self) -> u64 {
        match RefIner::from(self) {
            RefIner::Full(v) => v.major,
            RefIner::Small(s) => s.major(),
        }
    }

    fn minor(&self) -> u64 {
        match RefIner::from(self) {
            RefIner::Full(v) => v.minor,
            RefIner::Small(s) => s.minor(),
        }
    }

    fn patch(&self) -> u64 {
        match RefIner::from(self) {
            RefIner::Full(v) => v.patch,
            RefIner::Small(s) => s.patch(),
        }
    }

    fn pre(&self) -> &str {
        match RefIner::from(self) {
            RefIner::Full(v) => v.pre.as_str(),
            RefIner::Small(s) => s.pre(),
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
            return self.0.addr().cmp(&other.0.addr());
        }
        if std::ptr::eq(self.0, other.0) {
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
            (RefIner::Full(s), RefIner::Small(o)) => {
                if o.pre_is_empty() && !s.pre.is_empty() {
                    return core::cmp::Ordering::Less;
                }
                core::cmp::Ordering::Greater
            }
            (RefIner::Small(s), RefIner::Full(o)) => {
                if s.pre_is_empty() && !o.pre.is_empty() {
                    return core::cmp::Ordering::Greater;
                }
                core::cmp::Ordering::Less
            }
            (RefIner::Small(_), RefIner::Small(_)) => unreachable!(),
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
