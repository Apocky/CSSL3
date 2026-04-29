//! § AppendOnlyPool<T> — append-only with stable handles
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   The substrate-spec primitive `02_CSSL/06_SUBSTRATE_EVOLUTION.csl § 3` :
//!   `type PhiTable = AppendOnlyPool<Phi'Pattern>` (stable-handle-indexed).
//!   The same shape is reusable for any `T` that wants append-only stable-handle
//!   semantics ; this crate uses it for the Φ-pattern-pool and exposes it
//!   generically so the upcoming `cssl-substrate-omega-field` crate can share
//!   the type with cohomology-class-pool, throat-record-pool, and any other
//!   append-only-with-handle storage the spec calls for.
//!
//! § INVARIANTS
//!   - **Append-only.** Once a slot is written, its data is immutable.
//!     `AppendOnlyPool::push(t)` is the only mutation path that adds a slot ;
//!     `AppendOnlyPool::resolve(handle)` is the only read path. There is no
//!     `set` / `remove` / `swap` API by design.
//!
//!   - **Stable handles.** A `Handle<T>` minted by `push` stays valid for the
//!     pool's entire lifetime. The pool never reallocates the backing storage
//!     in a way that invalidates handles ; on growth, only the trailing
//!     unused capacity is reallocated, and inserted slots stay at the same
//!     index forever.
//!
//!   - **Generation-tagged collision-free indices.** Every slot stores a
//!     per-slot generation `u32`. On `push`, the slot's generation is set to
//!     `1` (initial mint). The handle returned has matching generation.
//!     Future de-duplication paths that recycle a slot will advance the
//!     generation ; stale handles (with an older generation) refuse resolve.
//!     For this slice, no recycling occurs ; the generation field is reserved
//!     for that future path and is documented as such.
//!
//!   - **Bounded.** The pool refuses `push` past
//!     `crate::MAX_PATTERNS_PER_POOL = 2^30`. This leaves headroom for the
//!     future generation-recycle path to advance generations without
//!     overflowing the packed `Handle` representation.
//!
//! § STORAGE LAYOUT
//!   The pool is a `Vec<Slot<T>>` where `Slot<T> = (generation: u32, value: T)`.
//!   Append-only with `Vec::push` gives amortized O(1) push and O(1) random
//!   access by index (= `Handle::index()`). The generation field is
//!   `repr(C)`-laid-out to be stable across save/load ; the `Pattern` type
//!   below has its own canonical wire format on top of this.
//!
//! § THREAD-SAFETY
//!   The pool is `Send + Sync` when `T: Send + Sync`. The append-only
//!   discipline means readers never observe a partially-written slot under
//!   external `Mutex`/`RwLock` synchronization (which is what the upcoming
//!   `OmegaField::clone_cow` boundary will provide). This crate itself does
//!   NOT add interior mutability ; it is a pure `&mut self`-on-mutate type.

use crate::handle::{Handle, HandleResolveError};

/// § Errors returned by [`AppendOnlyPool::push`] and related paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PoolError {
    /// § The pool has reached its bounded capacity
    ///   ([`crate::MAX_PATTERNS_PER_POOL`]).
    AtCapacity {
        /// § Current pool length.
        len: usize,
        /// § Maximum admissible length.
        cap: usize,
    },
}

impl core::fmt::Display for PoolError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::AtCapacity { len, cap } => write!(
                f,
                "AppendOnlyPool at-capacity ; len = {len} ; max = {cap}"
            ),
        }
    }
}

/// § Internal slot record. The generation field is stored alongside the
///   value so resolve-time generation-checks are a single load.
#[derive(Debug, Clone)]
struct Slot<T> {
    generation: u32,
    value: T,
}

/// § AppendOnlyPool<T> — append-only stable-handle-indexed storage.
///
/// See module-level docs for invariants and design rationale.
#[derive(Debug, Clone)]
pub struct AppendOnlyPool<T> {
    slots: Vec<Slot<T>>,
}

impl<T> AppendOnlyPool<T> {
    /// § Construct an empty pool with default capacity.
    #[must_use]
    pub const fn new() -> Self {
        Self { slots: Vec::new() }
    }

    /// § Construct an empty pool with reserved capacity for `n` slots.
    ///   Useful when the caller knows an approximate size (e.g. from a
    ///   save-file header).
    #[must_use]
    pub fn with_capacity(n: usize) -> Self {
        Self {
            slots: Vec::with_capacity(n),
        }
    }

    /// § Number of patterns in the pool.
    #[must_use]
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    /// § True if the pool has no patterns.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// § Currently-allocated capacity.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.slots.capacity()
    }

    /// § Append a value and return a stable handle. The handle's generation
    ///   is `1` (initial mint) and its index is the slot's position in the
    ///   pool. Returns [`PoolError::AtCapacity`] if the pool is full.
    pub fn push(&mut self, value: T) -> Result<Handle<T>, PoolError> {
        let cap = crate::MAX_PATTERNS_PER_POOL;
        if self.slots.len() >= cap {
            return Err(PoolError::AtCapacity {
                len: self.slots.len(),
                cap,
            });
        }
        let index = self.slots.len() as u32;
        self.slots.push(Slot {
            generation: 1,
            value,
        });
        Ok(Handle::from_parts(1, index))
    }

    /// § Append-with-pre-checked-capacity variant : assumes the caller has
    ///   already checked `len() < MAX_PATTERNS_PER_POOL`. Panics if not.
    ///   Useful in hot loops where the capacity check is hoisted.
    ///
    /// # Panics
    /// Panics if the pool is at capacity. Use [`push`] for the checked
    /// variant.
    pub fn push_unchecked(&mut self, value: T) -> Handle<T> {
        assert!(
            self.slots.len() < crate::MAX_PATTERNS_PER_POOL,
            "AppendOnlyPool::push_unchecked at capacity"
        );
        let index = self.slots.len() as u32;
        self.slots.push(Slot {
            generation: 1,
            value,
        });
        Handle::from_parts(1, index)
    }

    /// § Reserved hook for the future de-duplication path. For this slice
    ///   it behaves identically to [`push`] but is kept distinct so the
    ///   call-site can declare its intent. When the de-dup path lands, this
    ///   variant will look up the value-fingerprint first and either return
    ///   an existing handle or stamp a new slot.
    ///
    /// § RESERVED FOR DEDUP — current behavior is identical to [`push`].
    pub fn stamp_with_dedup_hint(&mut self, value: T) -> Result<Handle<T>, PoolError> {
        self.push(value)
    }

    /// § Resolve a handle to a borrowed reference. Returns
    ///   [`HandleResolveError`] for NULL / out-of-bounds / generation
    ///   mismatch. The resolved reference is valid for the lifetime of the
    ///   pool borrow ; appending more patterns (which requires `&mut self`)
    ///   cannot invalidate it within Rust's borrow checker.
    pub fn resolve(&self, handle: Handle<T>) -> Result<&T, HandleResolveError> {
        if handle.is_null() {
            return Err(HandleResolveError::Null);
        }
        let idx = handle.index() as usize;
        let pool_len = self.slots.len() as u32;
        if idx >= self.slots.len() {
            return Err(HandleResolveError::OutOfBounds {
                index: handle.index(),
                pool_len,
            });
        }
        let slot = &self.slots[idx];
        if slot.generation != handle.generation() {
            return Err(HandleResolveError::GenerationMismatch {
                handle_generation: handle.generation(),
                slot_generation: slot.generation,
            });
        }
        Ok(&slot.value)
    }

    /// § Convenience : `resolve` that panics on error. Useful in tests
    ///   where the handle is known-good.
    ///
    /// # Panics
    /// Panics if the handle is NULL, out-of-bounds, or has a stale
    /// generation tag.
    pub fn resolve_or_panic(&self, handle: Handle<T>) -> &T {
        self.resolve(handle).expect("AppendOnlyPool::resolve_or_panic : invalid handle")
    }

    /// § True if the handle resolves to a live slot in this pool.
    #[must_use]
    pub fn contains(&self, handle: Handle<T>) -> bool {
        self.resolve(handle).is_ok()
    }

    /// § Iterate over every (handle, value) pair in stable insertion order.
    ///   Useful for serialization / save-file emission.
    pub fn iter(&self) -> PoolIter<'_, T> {
        PoolIter {
            slots: self.slots.iter(),
            index: 0,
        }
    }
}

impl<T> Default for AppendOnlyPool<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'p, T> IntoIterator for &'p AppendOnlyPool<T> {
    type Item = (Handle<T>, &'p T);
    type IntoIter = PoolIter<'p, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// § Iterator over a pool's `(Handle, &T)` pairs in stable insertion order.
pub struct PoolIter<'p, T> {
    slots: core::slice::Iter<'p, Slot<T>>,
    index: u32,
}

impl<'p, T> Iterator for PoolIter<'p, T> {
    type Item = (Handle<T>, &'p T);

    fn next(&mut self) -> Option<Self::Item> {
        let slot = self.slots.next()?;
        let handle = Handle::from_parts(slot.generation, self.index);
        self.index += 1;
        Some((handle, &slot.value))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.slots.size_hint()
    }
}

impl<T> ExactSizeIterator for PoolIter<'_, T> {}

#[cfg(test)]
mod tests {
    use super::*;

    /// § Empty pool has length zero and is empty.
    #[test]
    fn empty_pool() {
        let p: AppendOnlyPool<u32> = AppendOnlyPool::new();
        assert_eq!(p.len(), 0);
        assert!(p.is_empty());
    }

    /// § Push returns a handle with generation 1 and the right index.
    #[test]
    fn push_returns_handle() {
        let mut p: AppendOnlyPool<u32> = AppendOnlyPool::new();
        let h0 = p.push(10).unwrap();
        let h1 = p.push(20).unwrap();
        assert_eq!(h0.generation(), 1);
        assert_eq!(h0.index(), 0);
        assert_eq!(h1.generation(), 1);
        assert_eq!(h1.index(), 1);
    }

    /// § Resolve returns the stored value.
    #[test]
    fn resolve_returns_value() {
        let mut p: AppendOnlyPool<u32> = AppendOnlyPool::new();
        let h = p.push(42).unwrap();
        assert_eq!(*p.resolve(h).unwrap(), 42);
    }

    /// § Resolve of NULL returns Null error.
    #[test]
    fn resolve_null_errors() {
        let p: AppendOnlyPool<u32> = AppendOnlyPool::new();
        let n = Handle::NULL;
        assert_eq!(p.resolve(n), Err(HandleResolveError::Null));
    }

    /// § Resolve of out-of-bounds returns OutOfBounds error.
    #[test]
    fn resolve_oob_errors() {
        let mut p: AppendOnlyPool<u32> = AppendOnlyPool::new();
        let _ = p.push(1).unwrap();
        let bad = Handle::from_parts(1, 99);
        match p.resolve(bad) {
            Err(HandleResolveError::OutOfBounds {
                index, pool_len,
            }) => {
                assert_eq!(index, 99);
                assert_eq!(pool_len, 1);
            }
            other => panic!("expected OutOfBounds, got {other:?}"),
        }
    }

    /// § Resolve with wrong generation returns GenerationMismatch.
    #[test]
    fn resolve_wrong_generation_errors() {
        let mut p: AppendOnlyPool<u32> = AppendOnlyPool::new();
        let _ = p.push(7).unwrap();
        let stale: Handle<u32> = Handle::from_parts(99, 0);
        match p.resolve(stale) {
            Err(HandleResolveError::GenerationMismatch {
                handle_generation,
                slot_generation,
            }) => {
                assert_eq!(handle_generation, 99);
                assert_eq!(slot_generation, 1);
            }
            other => panic!("expected GenerationMismatch, got {other:?}"),
        }
    }

    /// § Many pushes give monotonically-increasing indices.
    #[test]
    fn many_pushes_give_monotone_indices() {
        let mut p: AppendOnlyPool<u32> = AppendOnlyPool::new();
        for i in 0..1000 {
            let h = p.push(i).unwrap();
            assert_eq!(h.index(), i);
        }
    }

    /// § Handles stay valid after subsequent pushes.
    #[test]
    fn handles_stay_valid_after_pushes() {
        let mut p: AppendOnlyPool<u32> = AppendOnlyPool::new();
        let h0 = p.push(7).unwrap();
        for i in 0..1000 {
            let _ = p.push(i).unwrap();
        }
        assert_eq!(*p.resolve(h0).unwrap(), 7);
    }

    /// § Iter returns every (handle, value) in order.
    #[test]
    fn iter_yields_all() {
        let mut p: AppendOnlyPool<u32> = AppendOnlyPool::new();
        for i in 0..50 {
            let _ = p.push(i * 2).unwrap();
        }
        let collected: Vec<_> = p.iter().collect();
        assert_eq!(collected.len(), 50);
        for (i, (h, v)) in collected.iter().enumerate() {
            assert_eq!(h.index(), i as u32);
            assert_eq!(**v, (i as u32) * 2);
        }
    }

    /// § contains agrees with resolve.
    #[test]
    fn contains_agrees_with_resolve() {
        let mut p: AppendOnlyPool<u32> = AppendOnlyPool::new();
        let h = p.push(1).unwrap();
        assert!(p.contains(h));
        assert!(!p.contains(Handle::NULL));
        assert!(!p.contains(Handle::from_parts(1, 99)));
    }

    /// § with_capacity reserves but does not grow length.
    #[test]
    fn with_capacity() {
        let p: AppendOnlyPool<u32> = AppendOnlyPool::with_capacity(100);
        assert_eq!(p.len(), 0);
        assert!(p.capacity() >= 100);
    }

    /// § stamp_with_dedup_hint behaves identically to push for now.
    #[test]
    fn stamp_with_dedup_hint_works() {
        let mut p: AppendOnlyPool<u32> = AppendOnlyPool::new();
        let h = p.stamp_with_dedup_hint(42).unwrap();
        assert_eq!(*p.resolve(h).unwrap(), 42);
    }

    /// § resolve_or_panic returns value for valid handle.
    #[test]
    fn resolve_or_panic_ok() {
        let mut p: AppendOnlyPool<u32> = AppendOnlyPool::new();
        let h = p.push(5).unwrap();
        assert_eq!(*p.resolve_or_panic(h), 5);
    }

    /// § resolve_or_panic panics on bad handle.
    #[test]
    #[should_panic(expected = "invalid handle")]
    fn resolve_or_panic_panics() {
        let p: AppendOnlyPool<u32> = AppendOnlyPool::new();
        let _ = p.resolve_or_panic(Handle::NULL);
    }
}
