//! § cssl-rt allocator + tracker (T11-D52, S6-A1).
//!
//! § ROLE
//!   Stage-0 hosted allocator surface for CSSLv3-emitted code. Two layers:
//!   1. [`AllocTracker`] — atomic counters (alloc-count / free-count / bytes-in-use)
//!                       observable from the host without unsafe.
//!   2. [`BumpArena`] — Rust-side amortizing bump-allocator for batch
//!                       short-lived allocations (no per-block free ; the entire
//!                       arena releases on `Drop`).
//!
//! § FFI bridge ←→ [`crate::ffi`]
//!   [`raw_alloc`]  /  [`raw_free`]  /  [`raw_realloc`]  are the internal
//!   `unsafe fn` shims invoked by the `__cssl_alloc` / `__cssl_free` /
//!   `__cssl_realloc` FFI symbols. They delegate to [`std::alloc`] for the
//!   hosted target. Phase-B will swap in `mmap` / `VirtualAlloc` for the
//!   freestanding target while keeping these signatures.
//!
//! § INVARIANTS
//!   - All counters are `AtomicU64` ⇒ thread-safe from day-1.
//!   - `raw_alloc` returns null on `Layout` rejection or OOM ; never panics.
//!   - `raw_free(null, …)` is a no-op (tracker untouched).
//!   - `bytes_in_use` is monotone-bounded ; freeing more than allocated
//!     saturates at zero (defensive : never wrap-around).
//!
//! § SAFETY
//!   The unsafe surface is the three `raw_*` fns + the `BumpArena::alloc`
//!   raw-pointer return. Each is annotated with a `# Safety` paragraph.
//!   Tests exercise both the safe API ([`alloc_count`] / [`reset_for_tests`])
//!   and the unsafe FFI shims via dedicated `unsafe { … }` blocks.

// § T11-D52 (S6-A1) : the allocator + bump-arena fundamentally need
// raw-pointer ops + std::alloc dispatch ; file-level `unsafe_code` allow
// per cssl-cgen-cpu-cranelift/src/jit.rs convention. Each unsafe block
// carries an inline SAFETY paragraph.
#![allow(unsafe_code)]
#![allow(clippy::cast_possible_wrap)]

use core::cell::Cell;
use core::sync::atomic::{AtomicU64, Ordering};

// ───────────────────────────────────────────────────────────────────────
// § Tracker — global atomic counters
// ───────────────────────────────────────────────────────────────────────

/// Global allocation counters, updated by every [`raw_alloc`] / [`raw_free`].
///
/// Counters are `AtomicU64`. On a 32-bit host they still hold ≥ 4 GiB of
/// summed allocations before wrapping ; stage-0 has no realistic risk.
#[allow(clippy::module_name_repetitions)]
pub struct AllocTracker {
    alloc_count: AtomicU64,
    free_count: AtomicU64,
    bytes_in_use: AtomicU64,
    bytes_alloc_total: AtomicU64,
    bytes_free_total: AtomicU64,
}

impl AllocTracker {
    /// Construct an `AllocTracker` with all counters at zero.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            alloc_count: AtomicU64::new(0),
            free_count: AtomicU64::new(0),
            bytes_in_use: AtomicU64::new(0),
            bytes_alloc_total: AtomicU64::new(0),
            bytes_free_total: AtomicU64::new(0),
        }
    }

    fn record_alloc(&self, bytes: u64) {
        self.alloc_count.fetch_add(1, Ordering::Relaxed);
        self.bytes_in_use.fetch_add(bytes, Ordering::Relaxed);
        self.bytes_alloc_total.fetch_add(bytes, Ordering::Relaxed);
    }

    fn record_free(&self, bytes: u64) {
        self.free_count.fetch_add(1, Ordering::Relaxed);
        // saturating subtract ← defensive ; never wrap-around
        let prev = self
            .bytes_in_use
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |b| {
                Some(b.saturating_sub(bytes))
            })
            .unwrap_or(0);
        let _ = prev;
        self.bytes_free_total.fetch_add(bytes, Ordering::Relaxed);
    }

    /// Reset all counters to zero. Test-only.
    pub fn reset(&self) {
        self.alloc_count.store(0, Ordering::Relaxed);
        self.free_count.store(0, Ordering::Relaxed);
        self.bytes_in_use.store(0, Ordering::Relaxed);
        self.bytes_alloc_total.store(0, Ordering::Relaxed);
        self.bytes_free_total.store(0, Ordering::Relaxed);
    }
}

impl Default for AllocTracker {
    fn default() -> Self {
        Self::new()
    }
}

// § The single global tracker instance ; observable via the public readers.
static TRACKER: AllocTracker = AllocTracker::new();

/// Number of completed `__cssl_alloc` calls observed since process start.
#[must_use]
pub fn alloc_count() -> u64 {
    TRACKER.alloc_count.load(Ordering::Relaxed)
}

/// Number of completed `__cssl_free` calls observed since process start.
#[must_use]
pub fn free_count() -> u64 {
    TRACKER.free_count.load(Ordering::Relaxed)
}

/// Bytes currently outstanding (alloc'd minus freed). Saturates at zero.
#[must_use]
pub fn bytes_in_use() -> u64 {
    TRACKER.bytes_in_use.load(Ordering::Relaxed)
}

/// Total bytes ever allocated through `__cssl_alloc`.
#[must_use]
pub fn bytes_allocated_total() -> u64 {
    TRACKER.bytes_alloc_total.load(Ordering::Relaxed)
}

/// Total bytes ever freed through `__cssl_free`.
#[must_use]
pub fn bytes_freed_total() -> u64 {
    TRACKER.bytes_free_total.load(Ordering::Relaxed)
}

/// Reset all counters. Intended for test isolation only ; do not call in
/// production code paths.
pub fn reset_for_tests() {
    TRACKER.reset();
}

// ───────────────────────────────────────────────────────────────────────
// § raw_alloc / raw_free / raw_realloc — FFI bridge implementations
// ───────────────────────────────────────────────────────────────────────

/// Allocate `size` bytes with `align` alignment.
///
/// # Returns
/// Non-null pointer on success ; null pointer on:
/// - `Layout::from_size_align` rejection (size 0, align 0, align not power-of-two,
///   or rounded-size exceeds `isize::MAX`),
/// - System allocator OOM.
///
/// # Safety
/// Caller must:
/// - eventually pair with [`raw_free`] using the same `size` and `align`,
/// - never read or write past `size` bytes,
/// - never assume any initial value of returned memory (uninit bytes).
#[allow(unsafe_code)]
#[must_use]
pub unsafe fn raw_alloc(size: usize, align: usize) -> *mut u8 {
    let layout = match core::alloc::Layout::from_size_align(size, align) {
        Ok(l) if l.size() > 0 => l,
        _ => return core::ptr::null_mut(),
    };
    // SAFETY : layout has non-zero size + valid align (checked above) ⇒
    // std::alloc::alloc preconditions satisfied.
    let ptr = unsafe { std::alloc::alloc(layout) };
    if !ptr.is_null() {
        TRACKER.record_alloc(size as u64);
    }
    ptr
}

/// Free a `(ptr, size, align)` allocation produced by [`raw_alloc`].
///
/// `ptr == null` is a no-op (counters untouched).
///
/// # Safety
/// Caller must:
/// - have obtained `ptr` from a prior [`raw_alloc`] (or compatible
///   `__cssl_alloc` FFI call) with the SAME `size` and `align`,
/// - not double-free,
/// - not use `ptr` after this call.
#[allow(unsafe_code)]
pub unsafe fn raw_free(ptr: *mut u8, size: usize, align: usize) {
    if ptr.is_null() {
        return;
    }
    let layout = match core::alloc::Layout::from_size_align(size, align) {
        Ok(l) if l.size() > 0 => l,
        _ => return,
    };
    // SAFETY : ptr is non-null + layout matches the original alloc per
    // documented caller contract.
    unsafe { std::alloc::dealloc(ptr, layout) };
    TRACKER.record_free(size as u64);
}

/// Reallocate `(ptr, old_size)` to `new_size` keeping `align`.
///
/// On `null_mut` return : the original allocation is untouched.
/// On non-null return : the old pointer is invalidated.
///
/// # Safety
/// Caller must:
/// - have obtained `ptr` from a prior [`raw_alloc`] with the SAME `align`
///   and `old_size`,
/// - not use `ptr` after a successful (non-null) return.
#[allow(unsafe_code)]
#[must_use]
pub unsafe fn raw_realloc(ptr: *mut u8, old_size: usize, new_size: usize, align: usize) -> *mut u8 {
    if ptr.is_null() {
        // SAFETY : raw_alloc preconditions satisfied (size+align checked inside).
        return unsafe { raw_alloc(new_size, align) };
    }
    if new_size == 0 {
        // SAFETY : ptr+old_size+align match per caller contract.
        unsafe { raw_free(ptr, old_size, align) };
        return core::ptr::null_mut();
    }
    let old_layout = match core::alloc::Layout::from_size_align(old_size, align) {
        Ok(l) if l.size() > 0 => l,
        _ => return core::ptr::null_mut(),
    };
    // verify the new layout would be valid before calling realloc
    if core::alloc::Layout::from_size_align(new_size, align).is_err() {
        return core::ptr::null_mut();
    }
    // SAFETY : old_layout matches caller-asserted original alloc ; new_size
    // produces a valid Layout (checked above) ; std::alloc::realloc preconds met.
    let new_ptr = unsafe { std::alloc::realloc(ptr, old_layout, new_size) };
    if !new_ptr.is_null() {
        // tracker book-keeping : free old, alloc new
        TRACKER.record_free(old_size as u64);
        TRACKER.record_alloc(new_size as u64);
    }
    new_ptr
}

// ───────────────────────────────────────────────────────────────────────
// § BumpArena — Rust-side amortizing allocator
// ───────────────────────────────────────────────────────────────────────

/// A simple bump allocator over a single contiguous chunk.
///
/// § DESIGN
///   Stage-0 deliberately scope-limits this to one chunk (no chunk-list).
///   Allocations beyond the initial capacity return null. Phase-B will
///   evolve to a chunk-list with mmap/VirtualAlloc backing.
///
/// § INVARIANTS
///   - `cursor <= capacity` always.
///   - All bytes in `[base, base + capacity)` are owned by the arena
///     for its lifetime ; dropping the arena releases the chunk.
///
/// § THREAD-SAFETY
///   `BumpArena` is `!Sync` by virtue of `Cell<usize>`. Use one arena
///   per thread or wrap in a Mutex.
pub struct BumpArena {
    base: *mut u8,
    capacity: usize,
    cursor: Cell<usize>,
}

// SAFETY : BumpArena owns its chunk + uses Cell ⇒ not Sync, but Send is fine
// because the contained pointer is exclusively owned.
#[allow(unsafe_code)]
unsafe impl Send for BumpArena {}

impl BumpArena {
    /// Construct an arena with a backing chunk of `capacity` bytes.
    ///
    /// Returns `None` on:
    /// - `capacity == 0`,
    /// - underlying [`raw_alloc`] failure (OOM).
    #[allow(unsafe_code)]
    #[must_use]
    pub fn new(capacity: usize) -> Option<Self> {
        if capacity == 0 {
            return None;
        }
        // SAFETY : raw_alloc returns either non-null with `capacity` valid
        // bytes or null on failure ; we check for null below.
        let base = unsafe { raw_alloc(capacity, ALIGN_MAX) };
        if base.is_null() {
            return None;
        }
        Some(Self {
            base,
            capacity,
            cursor: Cell::new(0),
        })
    }

    /// Capacity of the underlying chunk in bytes.
    #[must_use]
    pub const fn capacity(&self) -> usize {
        self.capacity
    }

    /// Bytes currently consumed by the arena's served allocations.
    #[must_use]
    pub fn used(&self) -> usize {
        self.cursor.get()
    }

    /// Bytes still available in the arena.
    #[must_use]
    pub fn remaining(&self) -> usize {
        self.capacity.saturating_sub(self.cursor.get())
    }

    /// Allocate `size` bytes with `align` alignment from the arena.
    ///
    /// Returns null if:
    /// - `align` is not a power of two,
    /// - aligned cursor would exceed capacity.
    ///
    /// # Safety
    /// Returned pointer is valid for the arena's lifetime, then invalidated
    /// when the arena drops. Caller must not retain the pointer beyond.
    #[allow(unsafe_code)]
    #[must_use]
    pub unsafe fn alloc(&self, size: usize, align: usize) -> *mut u8 {
        if !align.is_power_of_two() {
            return core::ptr::null_mut();
        }
        let cur = self.cursor.get();
        let aligned = (cur + align - 1) & !(align - 1);
        let Some(end) = aligned.checked_add(size) else {
            return core::ptr::null_mut();
        };
        if end > self.capacity {
            return core::ptr::null_mut();
        }
        self.cursor.set(end);
        // SAFETY : aligned is in [0, capacity-size] ⇒ pointer is in-bounds
        // of the originally-allocated chunk.
        unsafe { self.base.add(aligned) }
    }

    /// Reset the arena cursor to zero ; freeing all served allocations
    /// at once. Existing pointers are immediately invalidated.
    pub fn reset(&self) {
        self.cursor.set(0);
    }
}

#[allow(unsafe_code)]
impl Drop for BumpArena {
    fn drop(&mut self) {
        // SAFETY : self.base + self.capacity match the original raw_alloc
        // call in `Self::new` ; the arena owns the chunk exclusively.
        unsafe {
            raw_free(self.base, self.capacity, ALIGN_MAX);
        }
    }
}

/// Default arena alignment ; matches `std::alloc::Layout::new::<u128>()`-style
/// over-alignment. Sufficient for any primitive scalar type CSSLv3 emits.
pub const ALIGN_MAX: usize = 16;

// ───────────────────────────────────────────────────────────────────────
// § tests
// ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::lock_and_reset_all as lock_and_reset;

    #[test]
    fn tracker_starts_zero_after_reset() {
        let _g = lock_and_reset();
        assert_eq!(alloc_count(), 0);
        assert_eq!(free_count(), 0);
        assert_eq!(bytes_in_use(), 0);
        assert_eq!(bytes_allocated_total(), 0);
        assert_eq!(bytes_freed_total(), 0);
    }

    #[test]
    fn raw_alloc_64bytes_increments_counters() {
        let _g = lock_and_reset();
        let p = unsafe { raw_alloc(64, 8) };
        assert!(!p.is_null(), "expected non-null pointer");
        assert_eq!(alloc_count(), 1);
        assert_eq!(bytes_in_use(), 64);
        unsafe { raw_free(p, 64, 8) };
        assert_eq!(free_count(), 1);
        assert_eq!(bytes_in_use(), 0);
    }

    #[test]
    fn raw_alloc_zero_size_returns_null() {
        let _g = lock_and_reset();
        let p = unsafe { raw_alloc(0, 8) };
        assert!(p.is_null());
        assert_eq!(alloc_count(), 0); // zero-size never counted
    }

    #[test]
    fn raw_alloc_zero_align_returns_null() {
        let _g = lock_and_reset();
        let p = unsafe { raw_alloc(64, 0) };
        assert!(p.is_null());
        assert_eq!(alloc_count(), 0);
    }

    #[test]
    fn raw_alloc_non_power_of_two_align_returns_null() {
        let _g = lock_and_reset();
        let p = unsafe { raw_alloc(64, 3) };
        assert!(p.is_null());
        assert_eq!(alloc_count(), 0);
    }

    #[test]
    fn raw_free_null_is_noop() {
        let _g = lock_and_reset();
        unsafe { raw_free(core::ptr::null_mut(), 64, 8) };
        assert_eq!(free_count(), 0);
    }

    #[test]
    fn raw_alloc_writeable_memory_round_trip() {
        let _g = lock_and_reset();
        let size = 32usize;
        let p = unsafe { raw_alloc(size, 8) };
        assert!(!p.is_null());
        // write a recognizable byte-pattern then read it back
        for i in 0..size {
            unsafe { p.add(i).write(0xA5_u8) };
        }
        for i in 0..size {
            assert_eq!(unsafe { p.add(i).read() }, 0xA5_u8);
        }
        unsafe { raw_free(p, size, 8) };
    }

    #[test]
    fn raw_realloc_grows_in_place_preserves_bytes() {
        let _g = lock_and_reset();
        let p = unsafe { raw_alloc(8, 8) };
        assert!(!p.is_null());
        for i in 0..8 {
            unsafe { p.add(i).write(i as u8) };
        }
        let p2 = unsafe { raw_realloc(p, 8, 64, 8) };
        assert!(!p2.is_null());
        for i in 0..8 {
            assert_eq!(unsafe { p2.add(i).read() }, i as u8);
        }
        unsafe { raw_free(p2, 64, 8) };
    }

    #[test]
    fn raw_realloc_shrink_preserves_kept_prefix() {
        let _g = lock_and_reset();
        let p = unsafe { raw_alloc(64, 8) };
        for i in 0..64 {
            unsafe { p.add(i).write((i & 0xFF) as u8) };
        }
        let p2 = unsafe { raw_realloc(p, 64, 16, 8) };
        assert!(!p2.is_null());
        for i in 0..16 {
            assert_eq!(unsafe { p2.add(i).read() }, (i & 0xFF) as u8);
        }
        unsafe { raw_free(p2, 16, 8) };
    }

    #[test]
    fn raw_realloc_null_old_acts_as_alloc() {
        let _g = lock_and_reset();
        let p = unsafe { raw_realloc(core::ptr::null_mut(), 0, 32, 8) };
        assert!(!p.is_null());
        assert_eq!(alloc_count(), 1);
        unsafe { raw_free(p, 32, 8) };
    }

    #[test]
    fn raw_realloc_to_zero_acts_as_free() {
        let _g = lock_and_reset();
        let p = unsafe { raw_alloc(32, 8) };
        let p2 = unsafe { raw_realloc(p, 32, 0, 8) };
        assert!(p2.is_null());
        assert_eq!(free_count(), 1);
    }

    #[test]
    fn freeing_more_than_alloc_saturates_at_zero() {
        let _g = lock_and_reset();
        // Direct fiddling : record_free without prior record_alloc.
        TRACKER.record_free(1024);
        assert_eq!(bytes_in_use(), 0); // saturated, did not wrap
        assert_eq!(bytes_freed_total(), 1024);
    }

    #[test]
    fn many_alloc_free_pairs_keep_bytes_in_use_at_zero() {
        let _g = lock_and_reset();
        for _ in 0..50 {
            let p = unsafe { raw_alloc(128, 8) };
            unsafe { raw_free(p, 128, 8) };
        }
        assert_eq!(alloc_count(), 50);
        assert_eq!(free_count(), 50);
        assert_eq!(bytes_in_use(), 0);
        assert_eq!(bytes_allocated_total(), 50 * 128);
        assert_eq!(bytes_freed_total(), 50 * 128);
    }

    #[test]
    fn arena_zero_capacity_is_none() {
        assert!(BumpArena::new(0).is_none());
    }

    #[test]
    fn arena_basic_alloc_returns_non_null_within_capacity() {
        let arena = BumpArena::new(1024).expect("arena");
        let p = unsafe { arena.alloc(64, 8) };
        assert!(!p.is_null());
        assert!(arena.used() >= 64);
        assert!(arena.used() <= arena.capacity());
    }

    #[test]
    fn arena_alignment_is_respected() {
        let arena = BumpArena::new(1024).expect("arena");
        // burn 1 byte ; next alloc with align=16 must be 16-byte-aligned
        let _b = unsafe { arena.alloc(1, 1) };
        let p = unsafe { arena.alloc(8, 16) };
        assert!(!p.is_null());
        assert_eq!((p as usize) & 15, 0, "expected 16-byte alignment");
    }

    #[test]
    fn arena_alloc_beyond_capacity_returns_null() {
        let arena = BumpArena::new(64).expect("arena");
        let p = unsafe { arena.alloc(128, 1) };
        assert!(p.is_null());
        assert_eq!(arena.used(), 0); // cursor not advanced on failure
    }

    #[test]
    fn arena_non_power_of_two_align_returns_null() {
        let arena = BumpArena::new(64).expect("arena");
        let p = unsafe { arena.alloc(8, 3) };
        assert!(p.is_null());
    }

    #[test]
    fn arena_sequential_allocs_advance_cursor() {
        let arena = BumpArena::new(1024).expect("arena");
        let _a = unsafe { arena.alloc(64, 8) };
        let used_after_first = arena.used();
        let _b = unsafe { arena.alloc(64, 8) };
        let used_after_second = arena.used();
        assert!(used_after_second >= used_after_first + 64);
    }

    #[test]
    fn arena_reset_returns_all_capacity() {
        let arena = BumpArena::new(1024).expect("arena");
        let _ = unsafe { arena.alloc(64, 8) };
        assert!(arena.used() >= 64);
        arena.reset();
        assert_eq!(arena.used(), 0);
        assert_eq!(arena.remaining(), 1024);
    }

    #[test]
    fn arena_drop_releases_chunk_via_tracker() {
        let _g = lock_and_reset();
        let pre_alloc = alloc_count();
        {
            let _arena = BumpArena::new(256).expect("arena");
            assert_eq!(alloc_count(), pre_alloc + 1);
        }
        // drop runs raw_free which increments free_count
        assert_eq!(free_count(), 1);
        assert_eq!(bytes_in_use(), 0);
    }

    #[test]
    fn many_arenas_stress_tracker() {
        let _g = lock_and_reset();
        let mut keep = Vec::new();
        for _ in 0..16 {
            keep.push(BumpArena::new(512).expect("arena"));
        }
        assert_eq!(alloc_count(), 16);
        assert_eq!(bytes_in_use(), 16 * 512);
        drop(keep);
        assert_eq!(free_count(), 16);
        assert_eq!(bytes_in_use(), 0);
    }

    #[test]
    fn unaligned_size_request_still_succeeds() {
        let _g = lock_and_reset();
        // size=23 with align=8 ⇒ valid Layout (size doesn't have to be align-multiple)
        let p = unsafe { raw_alloc(23, 8) };
        assert!(!p.is_null());
        assert_eq!(alloc_count(), 1);
        assert_eq!(bytes_in_use(), 23);
        unsafe { raw_free(p, 23, 8) };
    }

    #[test]
    fn realloc_keeps_alignment() {
        let _g = lock_and_reset();
        let p = unsafe { raw_alloc(8, 16) };
        assert!(!p.is_null());
        assert_eq!((p as usize) & 15, 0);
        let p2 = unsafe { raw_realloc(p, 8, 32, 16) };
        assert!(!p2.is_null());
        assert_eq!((p2 as usize) & 15, 0);
        unsafe { raw_free(p2, 32, 16) };
    }

    #[test]
    fn alloc_count_total_matches_history() {
        let _g = lock_and_reset();
        for n in 1..=10u64 {
            let p = unsafe { raw_alloc((n * 8) as usize, 8) };
            assert_eq!(alloc_count(), n);
            unsafe { raw_free(p, (n * 8) as usize, 8) };
        }
        assert_eq!(alloc_count(), 10);
        assert_eq!(free_count(), 10);
    }
}
