//! § Handle — packed (generation u32, index u32) = u64
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   The handle type that `FieldCell.pattern_handle` stores. Spec literal :
//!   `pattern_handle: Handle<Phi'Pattern>` — a packed u64 with NULL-pattern
//!   detection (`02_CSSL/06_SUBSTRATE_EVOLUTION.csl § 1`). The packing
//!   discipline makes a `Handle<T>` :
//!
//!   - 8 bytes — fits in the FieldCell's 8-byte Φ-facet slot exactly.
//!   - Comparable / hashable via the underlying u64.
//!   - NULL-detectable : the all-zeros `u64` IS the NULL handle by design.
//!   - Generation-tagged : a handle that survives across a hypothetical
//!     pool-recycle has stale generation and refuses resolve.
//!
//! § WHY THIS PACKING (rather than e.g. (idx + tag) or (idx + version-counter))
//!   The spec at `§ 1` declares the Φ-facet slot is exactly 8 bytes
//!   (`Handle<Phi'Pattern>` literal). The 32-bit index gives `~4.2e9` patterns
//!   per pool, which is more than the entire active substrate's pattern
//!   density would ever require. The 32-bit generation gives `~4.2e9` cycles
//!   of pool-recycle protection — way more than any realistic runtime.
//!
//!   The alternative (smaller index + larger tag) was rejected because the
//!   substrate spec is unambiguous : "stable-handle-indexed pool" means
//!   indices ARE the primary key, not the generation. Generation is the
//!   integrity check.

use core::fmt;
use core::marker::PhantomData;

/// § Handle<T> — opaque generation-tagged index into an [`AppendOnlyPool<T>`](crate::pool::AppendOnlyPool).
///
/// Layout : packed (generation : u32, index : u32) = u64. The high 32 bits
/// hold the generation tag ; the low 32 bits hold the slot index. The
/// all-zeros u64 IS the NULL handle (= [`NULL_HANDLE`]).
#[repr(transparent)]
pub struct Handle<T> {
    packed: u64,
    _marker: PhantomData<fn() -> T>,
}

// § Manual Clone / Copy impls. A derive-Clone-Copy with the
//   `PhantomData<fn() -> T>` marker would propagate `T: Clone + Copy`
//   bounds onto every use site, which is wrong : `Handle<T>` is purely
//   an integer-and-tag handle that can be copied freely regardless of
//   `T`'s own bounds.
impl<T> Clone for Handle<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Handle<T> {}

impl<T> Handle<T> {
    /// § The NULL handle — packed value `0`. Used to mark a `FieldCell`
    ///   slot as "unclaimed" per `§ 1` rustdoc literal :
    ///   `Handle<Phi'Pattern>: NULL ≡ unclaimed`.
    pub const NULL: Self = Self {
        packed: 0,
        _marker: PhantomData,
    };

    /// § Construct a handle from an explicit (generation, index) pair.
    ///   Generation `0` is RESERVED for the NULL handle and any caller
    ///   that mints a non-NULL handle starts at generation `1`. The pool
    ///   guarantees this discipline ; this constructor is the unsafe-op
    ///   for tests and serialization round-trip paths.
    #[must_use]
    pub const fn from_parts(generation: u32, index: u32) -> Self {
        Self {
            packed: ((generation as u64) << 32) | (index as u64),
            _marker: PhantomData,
        }
    }

    /// § Construct from the raw packed u64. Used when deserializing a
    ///   `FieldCell` from disk where the `pattern_handle` field came in
    ///   as a raw u64.
    #[must_use]
    pub const fn from_raw(packed: u64) -> Self {
        Self {
            packed,
            _marker: PhantomData,
        }
    }

    /// § Extract the raw packed u64. Used when serializing a `FieldCell`
    ///   to disk or uploading the `pattern_handle` field to a GPU buffer.
    #[must_use]
    pub const fn to_raw(self) -> u64 {
        self.packed
    }

    /// § Generation tag (high 32 bits). For NULL handle this is `0`.
    #[must_use]
    pub const fn generation(self) -> u32 {
        (self.packed >> 32) as u32
    }

    /// § Slot index (low 32 bits). For NULL handle this is `0`.
    #[must_use]
    pub const fn index(self) -> u32 {
        (self.packed & 0xFFFF_FFFF) as u32
    }

    /// § True if this handle is the NULL handle.
    #[must_use]
    pub const fn is_null(self) -> bool {
        self.packed == 0
    }

    /// § True if this handle is NOT the NULL handle.
    #[must_use]
    pub const fn is_some(self) -> bool {
        self.packed != 0
    }

    /// § Erase the type parameter — produce a type-untagged handle.
    ///   Used by audit-chain extension paths that store handles to
    ///   heterogeneous types in a single ledger.
    #[must_use]
    pub const fn erase(self) -> Handle<()> {
        Handle {
            packed: self.packed,
            _marker: PhantomData,
        }
    }
}

impl<T> Default for Handle<T> {
    fn default() -> Self {
        Self::NULL
    }
}

impl<T> PartialEq for Handle<T> {
    fn eq(&self, other: &Self) -> bool {
        self.packed == other.packed
    }
}

impl<T> Eq for Handle<T> {}

impl<T> core::hash::Hash for Handle<T> {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.packed.hash(state);
    }
}

impl<T> PartialOrd for Handle<T> {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<T> Ord for Handle<T> {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.packed.cmp(&other.packed)
    }
}

impl<T> fmt::Debug for Handle<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_null() {
            write!(f, "Handle::NULL")
        } else {
            write!(
                f,
                "Handle<{}>(g={}, i={})",
                core::any::type_name::<T>(),
                self.generation(),
                self.index()
            )
        }
    }
}

/// § Type-erased NULL handle — exported as a constant for callers who want
///   to reference "the NULL handle" without naming a specific `T`. Equal
///   to `Handle::<T>::NULL` under `eq` for any `T` after `erase()`.
pub const NULL_HANDLE: Handle<()> = Handle::<()>::NULL;

/// § Errors returned by `Handle::resolve` paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandleResolveError {
    /// § The handle was the NULL sentinel — caller must check `.is_some()`
    ///   before resolving.
    Null,
    /// § The handle's index is past the pool's end. Indicates either a
    ///   handle from a different pool, or a corrupted handle.
    OutOfBounds {
        /// § Index in the offending handle.
        index: u32,
        /// § Number of slots currently in the pool.
        pool_len: u32,
    },
    /// § The handle's generation tag does not match the pool slot. Indicates
    ///   either a recycled-pool handle (future de-dup path) or corruption.
    GenerationMismatch {
        /// § Generation in the offending handle.
        handle_generation: u32,
        /// § Generation currently stored at the slot.
        slot_generation: u32,
    },
}

impl fmt::Display for HandleResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Null => write!(f, "handle is NULL ; cannot resolve"),
            Self::OutOfBounds { index, pool_len } => write!(
                f,
                "handle index {index} >= pool len {pool_len} ; foreign or corrupted handle"
            ),
            Self::GenerationMismatch {
                handle_generation,
                slot_generation,
            } => write!(
                f,
                "handle generation {handle_generation} != slot generation {slot_generation} ; recycled or corrupted handle"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// § NULL handle is all-zeros.
    #[test]
    fn null_handle_is_zero() {
        let n: Handle<u32> = Handle::NULL;
        assert_eq!(n.to_raw(), 0);
        assert!(n.is_null());
        assert!(!n.is_some());
    }

    /// § from_parts / generation / index round-trip.
    #[test]
    fn from_parts_roundtrip() {
        let h: Handle<u32> = Handle::from_parts(0xDEAD_BEEF, 0xCAFE_F00D);
        assert_eq!(h.generation(), 0xDEAD_BEEF);
        assert_eq!(h.index(), 0xCAFE_F00D);
    }

    /// § from_raw / to_raw round-trip.
    #[test]
    fn from_raw_to_raw_roundtrip() {
        let r: u64 = 0x1234_5678_9ABC_DEF0;
        let h: Handle<u32> = Handle::from_raw(r);
        assert_eq!(h.to_raw(), r);
    }

    /// § Default is NULL.
    #[test]
    fn default_is_null() {
        let h: Handle<u32> = Handle::default();
        assert!(h.is_null());
    }

    /// § Equality of identical handles.
    #[test]
    fn equality() {
        let a: Handle<u32> = Handle::from_parts(7, 11);
        let b: Handle<u32> = Handle::from_parts(7, 11);
        let c: Handle<u32> = Handle::from_parts(7, 12);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    /// § Hash agrees with raw u64.
    #[test]
    fn hash_agrees_with_raw() {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        fn hash_of<T: Hash>(t: &T) -> u64 {
            let mut h = DefaultHasher::new();
            t.hash(&mut h);
            h.finish()
        }

        let h: Handle<u32> = Handle::from_parts(3, 5);
        assert_eq!(hash_of(&h), hash_of(&h.to_raw()));
    }

    /// § Ord matches raw u64.
    #[test]
    fn ord_matches_raw() {
        let a: Handle<u32> = Handle::from_parts(0, 0);
        let b: Handle<u32> = Handle::from_parts(0, 1);
        let c: Handle<u32> = Handle::from_parts(1, 0);
        assert!(a < b);
        assert!(b < c);
    }

    /// § Erase preserves raw value.
    #[test]
    fn erase_preserves_raw() {
        let h: Handle<u32> = Handle::from_parts(42, 17);
        let e = h.erase();
        assert_eq!(e.to_raw(), h.to_raw());
    }

    /// § HandleResolveError Display formatting.
    #[test]
    fn resolve_error_display() {
        let e = HandleResolveError::Null;
        assert!(format!("{e}").contains("NULL"));
        let e = HandleResolveError::OutOfBounds {
            index: 5,
            pool_len: 3,
        };
        assert!(format!("{e}").contains('5'));
        let e = HandleResolveError::GenerationMismatch {
            handle_generation: 1,
            slot_generation: 2,
        };
        assert!(format!("{e}").contains('1'));
    }

    /// § Debug for NULL.
    #[test]
    fn debug_null() {
        let h: Handle<u32> = Handle::NULL;
        assert!(format!("{h:?}").contains("NULL"));
    }

    /// § Debug for non-NULL includes type name + parts.
    #[test]
    fn debug_non_null() {
        let h: Handle<u32> = Handle::from_parts(3, 5);
        let s = format!("{h:?}");
        assert!(s.contains("g=3"));
        assert!(s.contains("i=5"));
    }

    /// § sizeof Handle<T> = 8 bytes (matches FieldCell.pattern_handle slot).
    #[test]
    fn size_is_eight() {
        assert_eq!(core::mem::size_of::<Handle<u32>>(), 8);
        assert_eq!(core::mem::align_of::<Handle<u32>>(), 8);
    }
}
