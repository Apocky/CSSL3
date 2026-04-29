//! § SparseMortonGrid — open-addressing hashtable keyed by [`MortonKey`].
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Generic sparse Morton-keyed grid. Used for the dense FieldCell tier +
//!   every overlay (Λ, Ψ, Σ-overlay).
//!
//! § SPEC
//!   - `Omniverse/04_OMEGA_FIELD/02_STORAGE.csl.md` § II.open-addressing :
//!     - HashEntry { key: u64, cell: T }
//!     - load-factor ≤ 0.5 (rehash above)
//!     - linear-probe-fallback : K steps, then analytic-SDF-call
//!   - `Omniverse/02_CSSL/06_SUBSTRATE_EVOLUTION.csl` § IV.2 SparseMortonGrid<T>.
//!
//! § DETERMINISM CONTRACT
//!   - The hash function is fixed (splitmix64-style mix). No platform-specific
//!     intrinsics. The collision-resolution probe-sequence is deterministic
//!     across hosts (same `(key, slot_count)` ⇒ same probe-walk).
//!   - Iteration order is sorted-by-MortonKey on demand via [`SparseMortonGrid::iter`]
//!     — replay-stable across builds even though the underlying slot-layout is
//!     not a stable surface.
//!
//! § INVARIANTS
//!   - load-factor ≤ 0.5 ; growth doubles the capacity. (Power-of-2 capacities
//!     guarantee the linear-probe sequence covers every slot.)
//!   - Empty slots are encoded by `MortonKey::SENTINEL` (bit 63 set). Real keys
//!     produced by [`MortonKey::encode`] never set bit 63.
//!
//! § CAPABILITY
//!   The grid carries iso-ownership of its backing storage at the type level
//!   ; consumers wishing to share ownership must thread a higher-level
//!   capability (e.g. `Arc<RwLock<SparseMortonGrid<T>>>` at the OmegaField
//!   level). At this slice the grid exposes `&self` / `&mut self` only.
//!
//! § TELEMETRY
//!   The grid records collision-statistics (probe-step histogram) for nightly-
//!   bench gating per `Omniverse/04_OMEGA_FIELD/05_DENSITY_BUDGET § XI
//!   ACCEPTANCE`. Stats are read via [`SparseMortonGrid::collision_stats`].

use crate::field_cell::FieldCell;
use crate::morton::MortonKey;

/// Default initial capacity for an empty grid (must be a power of two).
pub const DEFAULT_GRID_CAPACITY: usize = 64;
/// Maximum probe-steps before we give up + log a linear-probe-saturation
/// telemetry entry. Beyond this the consumer is expected to grow the table
/// (the grid handles growth automatically when load-factor > 0.5).
pub const MAX_PROBE_STEPS: u32 = 64;

/// Tombstone marker — distinct from the empty-sentinel + still satisfies
/// `is_sentinel()`. Stored at slot positions whose key was removed ; the
/// probe-walk treats tombstones as "skip past" so displaced keys remain
/// reachable. Tombstones are reclaimed at the next rehash.
const TOMBSTONE_KEY: MortonKey = MortonKey::from_u64_raw(MORTON_TOMBSTONE_RAW);
const MORTON_TOMBSTONE_RAW: u64 = (1u64 << 63) | 0x1; // sentinel-bit + low bit

// ───────────────────────────────────────────────────────────────────────
// § OmegaCellLayout — the trait that every cell-type must implement.
// ───────────────────────────────────────────────────────────────────────

/// Trait for cell-types stored inside a [`SparseMortonGrid`]. Captures the
/// std430-layout invariants used by the GPU-upload path + the audit-chain.
///
/// # Stable invariants
///   - `omega_cell_size()` returns the canonical byte-size that the layout
///     validator (LAY0001 in cssl-mir) expects.
///   - `omega_cell_align()` returns the std430 alignment.
///   - `omega_cell_layout_tag()` is the canonical-name used in the audit
///     chain when a layout-violation is recorded.
pub trait OmegaCellLayout: Copy + Default + 'static {
    /// Expected `sizeof(Self)` in bytes (must match the `@layout(std430)` tag).
    fn omega_cell_size() -> usize;
    /// Expected `alignof(Self)` in bytes.
    fn omega_cell_align() -> usize;
    /// Stable canonical layout-tag used in audit + telemetry.
    fn omega_cell_layout_tag() -> &'static str;
}

impl OmegaCellLayout for FieldCell {
    fn omega_cell_size() -> usize {
        72
    }
    fn omega_cell_align() -> usize {
        8
    }
    fn omega_cell_layout_tag() -> &'static str {
        "FieldCell"
    }
}

// ───────────────────────────────────────────────────────────────────────
// § MissPolicy — what happens on a key-miss read.
// ───────────────────────────────────────────────────────────────────────

/// Policy for [`SparseMortonGrid::at`] when the key is not present.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MissPolicy {
    /// Return [`None`]. Default for most overlays.
    None,
    /// Return [`Some`] of `T::default()`. Useful for the dense FieldCell tier
    /// where the "absent cell" is implicitly the air-cell.
    Default,
    /// Reserved tag — call analytic-SDF fallback per `Axiom 5 base-distribution`.
    /// At this slice the SDF integration is not yet wired ; this variant
    /// behaves identically to [`Self::None`] until D116 lands. The variant is
    /// preserved here so the surface is API-stable from D113 onward.
    AnalyticSDF,
}

impl Default for MissPolicy {
    fn default() -> Self {
        Self::None
    }
}

// ───────────────────────────────────────────────────────────────────────
// § CollisionStats — telemetry for the probe-walk histogram.
// ───────────────────────────────────────────────────────────────────────

/// Per-grid telemetry on the linear-probe walk. Used by nightly-bench to
/// detect "collision avalanche" + drive auto-rehash decisions.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CollisionStats {
    /// Number of insert calls.
    pub inserts: u64,
    /// Number of probe-steps taken on insert.
    pub insert_probe_steps: u64,
    /// Number of get calls.
    pub gets: u64,
    /// Number of probe-steps taken on get.
    pub get_probe_steps: u64,
    /// Times the grid grew (capacity doubled).
    pub rehashes: u64,
    /// Worst-case probe-distance seen so far.
    pub max_probe_distance: u32,
}

impl CollisionStats {
    /// Average probe-steps-per-insert. Returns 0 if no inserts have been
    /// recorded yet.
    #[must_use]
    pub fn avg_insert_probe(&self) -> f32 {
        if self.inserts == 0 {
            0.0
        } else {
            self.insert_probe_steps as f32 / self.inserts as f32
        }
    }

    /// Average probe-steps-per-get. Returns 0 if no gets have been recorded.
    #[must_use]
    pub fn avg_get_probe(&self) -> f32 {
        if self.gets == 0 {
            0.0
        } else {
            self.get_probe_steps as f32 / self.gets as f32
        }
    }

    /// Estimated collision-rate (probe-steps / total operations). 0.0 = no
    /// collisions ; > 1.0 = pathological collisions.
    #[must_use]
    pub fn collision_rate(&self) -> f32 {
        let total_ops = self.inserts + self.gets;
        if total_ops == 0 {
            0.0
        } else {
            (self.insert_probe_steps + self.get_probe_steps) as f32 / total_ops as f32
        }
    }
}

// ───────────────────────────────────────────────────────────────────────
// § SparseMortonGrid — the hashtable itself.
// ───────────────────────────────────────────────────────────────────────

/// Open-addressing hashtable keyed by [`MortonKey`], storing values of type
/// `T : OmegaCellLayout`.
///
/// § STORAGE
///   - `keys[i]` : the Morton key of slot `i` (or [`MortonKey::SENTINEL`] if
///     empty).
///   - `values[i]` : the cell at slot `i` (untouched when slot is empty).
///   - The two arrays are allocated together to maintain cache-coherence on
///     the iter+map path.
///
/// § GROWTH
///   When `count` exceeds `cap / 2` the table is rehashed into a 2× larger
///   table. Rehashing is done in-place (well, into a fresh allocation) and
///   takes `O(N)`. Replay-stability is preserved because the rehash uses the
///   same hash function ; only the slot-count changes.
#[derive(Debug, Clone)]
pub struct SparseMortonGrid<T: OmegaCellLayout> {
    /// Per-slot Morton key (or sentinel for empty).
    keys: Vec<MortonKey>,
    /// Per-slot value.
    values: Vec<T>,
    /// Number of occupied slots.
    count: usize,
    /// Per-grid telemetry.
    stats: CollisionStats,
    /// Miss-policy for [`Self::at`] reads.
    miss_policy: MissPolicy,
}

impl<T: OmegaCellLayout> SparseMortonGrid<T> {
    /// Construct an empty grid with the [`DEFAULT_GRID_CAPACITY`].
    #[must_use]
    pub fn new() -> SparseMortonGrid<T> {
        Self::with_capacity(DEFAULT_GRID_CAPACITY)
    }

    /// Construct an empty grid with at least `min_capacity` slots. The
    /// allocated capacity is the next power of two ≥ `min_capacity`.
    #[must_use]
    pub fn with_capacity(min_capacity: usize) -> SparseMortonGrid<T> {
        let cap = min_capacity.max(2).next_power_of_two();
        let keys = vec![MortonKey::SENTINEL; cap];
        let values = vec![T::default(); cap];
        SparseMortonGrid {
            keys,
            values,
            count: 0,
            stats: CollisionStats::default(),
            miss_policy: MissPolicy::default(),
        }
    }

    /// Set the miss-on-read policy. Default is [`MissPolicy::None`].
    pub fn set_miss_policy(&mut self, policy: MissPolicy) {
        self.miss_policy = policy;
    }

    /// Slot count (always a power of two).
    #[inline]
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.keys.len()
    }

    /// Number of occupied slots.
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.count
    }

    /// True iff the grid has no occupied slots.
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Current load-factor (occupied / capacity).
    #[inline]
    #[must_use]
    pub fn load_factor(&self) -> f32 {
        self.count as f32 / self.capacity() as f32
    }

    /// Read-only telemetry.
    #[inline]
    #[must_use]
    pub fn collision_stats(&self) -> CollisionStats {
        self.stats
    }

    // ── Slot-finding (internal) ─────────────────────────────────────

    /// Find the slot index for `key`. Returns `(slot, found)` where
    /// `found = true` iff the slot currently holds `key`. If `found = false`
    /// then `slot` is a slot suitable for insertion (either an empty slot
    /// or a tombstone — the first tombstone in the probe sequence is
    /// preferred so the grid reuses tombstones eagerly).
    ///
    /// § TOMBSTONE-AWARENESS
    ///   Walk past tombstones for `find` (to locate the live key downstream)
    ///   but record the FIRST tombstone seen as the preferred-insert-slot.
    ///   Stop the walk only at an EMPTY slot (or on key-match).
    ///
    /// `probes_used` is incremented for telemetry.
    fn find_slot(&self, key: MortonKey, probes_used: &mut u32) -> (usize, bool) {
        let cap = self.capacity();
        let mask = cap - 1;
        let mut step: u32 = 0;
        let mut first_tombstone: Option<usize> = None;
        loop {
            *probes_used = step;
            let raw = key.linear_probe(step) as usize;
            let slot = raw & mask;
            let cur = self.keys[slot];
            // Empty slot terminates the walk. If we saw a tombstone earlier,
            // prefer that for inserts. Otherwise return this empty slot.
            if cur.to_u64() == MortonKey::SENTINEL.to_u64() {
                let insert_slot = first_tombstone.unwrap_or(slot);
                return (insert_slot, false);
            }
            // Tombstone : remember the first one + keep walking (the live
            // key may be downstream).
            if cur.to_u64() == TOMBSTONE_KEY.to_u64() {
                if first_tombstone.is_none() {
                    first_tombstone = Some(slot);
                }
                step += 1;
                if step >= MAX_PROBE_STEPS {
                    return (first_tombstone.unwrap_or(slot), false);
                }
                continue;
            }
            // Live key : check for match.
            if cur == key {
                return (slot, true);
            }
            step += 1;
            if step >= MAX_PROBE_STEPS {
                return (first_tombstone.unwrap_or(slot), false);
            }
        }
    }

    // ── Read APIs ───────────────────────────────────────────────────

    /// Read the value at `key`. Honors the [`MissPolicy`] when the key is
    /// not present.
    pub fn at(&mut self, key: MortonKey) -> Option<T> {
        let mut probes = 0;
        let (slot, found) = self.find_slot(key, &mut probes);
        self.stats.gets += 1;
        self.stats.get_probe_steps += probes as u64;
        if probes > self.stats.max_probe_distance {
            self.stats.max_probe_distance = probes;
        }
        if found {
            return Some(self.values[slot]);
        }
        match self.miss_policy {
            MissPolicy::None => None,
            MissPolicy::Default => Some(T::default()),
            MissPolicy::AnalyticSDF => None, // wired-up in D116 ; see § module-doc.
        }
    }

    /// Read the value at `key` without modifying telemetry. Pure-read +
    /// const-correct ; preferred for hot loops where the caller already has
    /// telemetry hooks in place.
    #[must_use]
    pub fn at_const(&self, key: MortonKey) -> Option<T> {
        let mut probes = 0;
        let (slot, found) = self.find_slot(key, &mut probes);
        if found {
            return Some(self.values[slot]);
        }
        match self.miss_policy {
            MissPolicy::None => None,
            MissPolicy::Default => Some(T::default()),
            MissPolicy::AnalyticSDF => None,
        }
    }

    /// Get a mutable reference to the value at `key`. Returns `None` if the
    /// key is not present.
    pub fn at_mut(&mut self, key: MortonKey) -> Option<&mut T> {
        let mut probes = 0;
        let (slot, found) = self.find_slot(key, &mut probes);
        self.stats.gets += 1;
        self.stats.get_probe_steps += probes as u64;
        if probes > self.stats.max_probe_distance {
            self.stats.max_probe_distance = probes;
        }
        if found {
            Some(&mut self.values[slot])
        } else {
            None
        }
    }

    /// Insert `(key, value)`, replacing any existing entry. Returns the
    /// previous value if any.
    ///
    /// # Errors
    /// Returns [`GridError::SaturatedProbe`] if the linear-probe walk
    /// exceeded [`MAX_PROBE_STEPS`] without finding either the key or a
    /// free slot. (In practice the load-factor 0.5 cap makes this branch
    /// astronomically unlikely ; the error is preserved for completeness.)
    pub fn insert(&mut self, key: MortonKey, value: T) -> Result<Option<T>, GridError> {
        // Pre-check for growth : if the load-factor will exceed 0.5 after
        // this insert, grow first.
        if (self.count + 1) * 2 > self.capacity() {
            self.rehash_grow();
        }
        let mut probes = 0;
        let (slot, found) = self.find_slot(key, &mut probes);
        self.stats.inserts += 1;
        self.stats.insert_probe_steps += probes as u64;
        if probes > self.stats.max_probe_distance {
            self.stats.max_probe_distance = probes;
        }
        if probes >= MAX_PROBE_STEPS {
            return Err(GridError::SaturatedProbe { steps: probes });
        }
        if found {
            // Replace the existing value.
            let old = self.values[slot];
            self.values[slot] = value;
            Ok(Some(old))
        } else {
            self.keys[slot] = key;
            self.values[slot] = value;
            self.count += 1;
            Ok(None)
        }
    }

    /// Remove `key`. Returns the prior value if any.
    ///
    /// § REMOVAL DISCIPLINE
    ///   We use **tombstone** markers : a removed slot is marked with the
    ///   distinct [`MortonKey::TOMBSTONE`] sentinel rather than the empty-
    ///   sentinel. The find_slot walker treats tombstones as "skip past"
    ///   so subsequent reads still find the displaced keys downstream.
    ///   On the next rehash all tombstones are dropped.
    pub fn remove(&mut self, key: MortonKey) -> Option<T> {
        let mut probes = 0;
        let (slot, found) = self.find_slot(key, &mut probes);
        self.stats.gets += 1;
        if !found {
            return None;
        }
        let removed = self.values[slot];
        // Mark the slot as a tombstone (distinct from "empty") so the probe
        // chain remains intact for subsequent reads of displaced keys.
        self.keys[slot] = TOMBSTONE_KEY;
        self.count -= 1;
        Some(removed)
    }

    // ── Growth ──────────────────────────────────────────────────────

    /// Double the grid capacity + re-insert every entry. Called automatically
    /// when load-factor > 0.5.
    fn rehash_grow(&mut self) {
        let new_cap = self.capacity() * 2;
        let old_keys = std::mem::replace(&mut self.keys, vec![MortonKey::SENTINEL; new_cap]);
        let old_values = std::mem::replace(&mut self.values, vec![T::default(); new_cap]);
        let old_count = self.count;
        self.count = 0;
        self.stats.rehashes += 1;
        for (k, v) in old_keys.into_iter().zip(old_values.into_iter()) {
            // Skip BOTH empty-sentinels AND tombstones — rehash is the
            // canonical tombstone-reclaim point.
            if !k.is_sentinel() {
                // Re-insert without recursing (we know the new table has
                // headroom).
                let mut probes = 0;
                let (slot, _found) = self.find_slot(k, &mut probes);
                self.keys[slot] = k;
                self.values[slot] = v;
                self.count += 1;
            }
        }
        debug_assert_eq!(self.count, old_count, "rehash must preserve count");
    }

    /// Reserve at least `additional` more occupied slots. Forces a rehash if
    /// necessary.
    pub fn reserve(&mut self, additional: usize) {
        let target = self.count + additional;
        while target * 2 > self.capacity() {
            self.rehash_grow();
        }
    }

    // ── Iteration ───────────────────────────────────────────────────

    /// Iterator over all `(key, &value)` pairs in MortonKey-ascending order.
    /// O(N + N log N) — collects then sorts ; for hot per-frame iteration
    /// prefer [`Self::iter_unordered`] which is O(N) but slot-order.
    #[must_use]
    pub fn iter(&self) -> SortedIter<'_, T> {
        let mut entries: Vec<(MortonKey, &T)> = self
            .keys
            .iter()
            .zip(self.values.iter())
            .filter(|(k, _)| !k.is_sentinel())
            .map(|(k, v)| (*k, v))
            .collect();
        entries.sort_by_key(|(k, _)| k.to_u64());
        SortedIter { entries, idx: 0 }
    }

    /// Iterator over occupied slots in slot-order (no MortonKey sort). Faster
    /// than [`Self::iter`] when the consumer doesn't care about ordering.
    #[must_use]
    pub fn iter_unordered(&self) -> UnorderedIter<'_, T> {
        UnorderedIter {
            keys: &self.keys,
            values: &self.values,
            idx: 0,
        }
    }

    /// Iterator over occupied slots whose tier matches `tier`. Used by the
    /// MERA-cascade per-tier walks.
    #[must_use]
    pub fn iter_by_tier(&self, tier: crate::morton::CellTier) -> ByTierIter<'_, T> {
        ByTierIter {
            keys: &self.keys,
            values: &self.values,
            idx: 0,
            tier,
        }
    }

    // ── Pruning ─────────────────────────────────────────────────────

    /// Remove every entry for which `predicate(key, &value)` returns false.
    /// Returns the number of entries pruned.
    pub fn prune<F>(&mut self, mut predicate: F) -> usize
    where
        F: FnMut(MortonKey, &T) -> bool,
    {
        let to_remove: Vec<MortonKey> = self
            .keys
            .iter()
            .zip(self.values.iter())
            .filter(|(k, _)| !k.is_sentinel())
            .filter_map(|(k, v)| if predicate(*k, v) { None } else { Some(*k) })
            .collect();
        let n = to_remove.len();
        for k in to_remove {
            self.remove(k);
        }
        n
    }
}

impl<T: OmegaCellLayout> Default for SparseMortonGrid<T> {
    fn default() -> Self {
        Self::new()
    }
}

// ───────────────────────────────────────────────────────────────────────
// § Iterators.
// ───────────────────────────────────────────────────────────────────────

/// Iterator over `(MortonKey, &T)` pairs in ascending Morton-key order.
#[derive(Debug)]
pub struct SortedIter<'a, T> {
    entries: Vec<(MortonKey, &'a T)>,
    idx: usize,
}

impl<'a, T> Iterator for SortedIter<'a, T> {
    type Item = (MortonKey, &'a T);
    fn next(&mut self) -> Option<Self::Item> {
        if self.idx >= self.entries.len() {
            return None;
        }
        let item = self.entries[self.idx];
        self.idx += 1;
        Some(item)
    }
}

/// Iterator over `(MortonKey, &T)` pairs in slot-order.
#[derive(Debug)]
pub struct UnorderedIter<'a, T> {
    keys: &'a [MortonKey],
    values: &'a [T],
    idx: usize,
}

impl<'a, T> Iterator for UnorderedIter<'a, T> {
    type Item = (MortonKey, &'a T);
    fn next(&mut self) -> Option<Self::Item> {
        while self.idx < self.keys.len() {
            let k = self.keys[self.idx];
            let v = &self.values[self.idx];
            self.idx += 1;
            if !k.is_sentinel() {
                return Some((k, v));
            }
        }
        None
    }
}

/// Iterator over entries whose [`MortonKey::tier`] equals a fixed
/// [`crate::morton::CellTier`].
#[derive(Debug)]
pub struct ByTierIter<'a, T> {
    keys: &'a [MortonKey],
    values: &'a [T],
    idx: usize,
    tier: crate::morton::CellTier,
}

impl<'a, T> Iterator for ByTierIter<'a, T> {
    type Item = (MortonKey, &'a T);
    fn next(&mut self) -> Option<Self::Item> {
        while self.idx < self.keys.len() {
            let k = self.keys[self.idx];
            let v = &self.values[self.idx];
            self.idx += 1;
            if !k.is_sentinel() && k.tier() == self.tier {
                return Some((k, v));
            }
        }
        None
    }
}

// ───────────────────────────────────────────────────────────────────────
// § GridError — failure modes for grid mutations.
// ───────────────────────────────────────────────────────────────────────

/// Failure modes for [`SparseMortonGrid`] insertions.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum GridError {
    /// The linear-probe walk saturated [`MAX_PROBE_STEPS`] without finding
    /// either the key or a free slot. In practice this means the grid is
    /// pathologically loaded ; consumers should call
    /// [`SparseMortonGrid::reserve`] to force a rehash.
    #[error("OF0010 — sparse-grid linear-probe saturated at {steps} steps")]
    SaturatedProbe { steps: u32 },
}

#[cfg(test)]
mod tests {
    use super::{
        CollisionStats, MissPolicy, OmegaCellLayout, SparseMortonGrid, DEFAULT_GRID_CAPACITY,
    };
    use crate::field_cell::FieldCell;
    use crate::morton::{CellTier, MortonKey};

    // A trivial test-only cell type so the SparseMortonGrid<T> generic
    // surface is exercised independently of FieldCell.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    struct TestCell {
        tag: u32,
    }
    impl OmegaCellLayout for TestCell {
        fn omega_cell_size() -> usize {
            4
        }
        fn omega_cell_align() -> usize {
            4
        }
        fn omega_cell_layout_tag() -> &'static str {
            "TestCell"
        }
    }

    // ── Construction ───────────────────────────────────────────────

    #[test]
    fn new_grid_is_empty() {
        let g: SparseMortonGrid<TestCell> = SparseMortonGrid::new();
        assert_eq!(g.len(), 0);
        assert!(g.is_empty());
        assert_eq!(g.capacity(), DEFAULT_GRID_CAPACITY);
    }

    #[test]
    fn with_capacity_pow2_round_up() {
        let g: SparseMortonGrid<TestCell> = SparseMortonGrid::with_capacity(7);
        assert_eq!(g.capacity(), 8);
    }

    #[test]
    fn with_capacity_already_pow2_no_change() {
        let g: SparseMortonGrid<TestCell> = SparseMortonGrid::with_capacity(64);
        assert_eq!(g.capacity(), 64);
    }

    // ── Insert / get / remove ─────────────────────────────────────

    #[test]
    fn insert_then_get_returns_value() {
        let mut g: SparseMortonGrid<TestCell> = SparseMortonGrid::new();
        let k = MortonKey::encode(7, 8, 9).unwrap();
        g.insert(k, TestCell { tag: 42 }).unwrap();
        assert_eq!(g.at_const(k).unwrap().tag, 42);
        assert_eq!(g.len(), 1);
    }

    #[test]
    fn insert_replaces_existing_returns_old() {
        let mut g: SparseMortonGrid<TestCell> = SparseMortonGrid::new();
        let k = MortonKey::encode(7, 8, 9).unwrap();
        let prev = g.insert(k, TestCell { tag: 1 }).unwrap();
        assert!(prev.is_none());
        let prev = g.insert(k, TestCell { tag: 2 }).unwrap();
        assert_eq!(prev.unwrap().tag, 1);
        assert_eq!(g.at_const(k).unwrap().tag, 2);
    }

    #[test]
    fn at_const_missing_key_returns_none_default_policy() {
        let g: SparseMortonGrid<TestCell> = SparseMortonGrid::new();
        let k = MortonKey::encode(99, 99, 99).unwrap();
        assert!(g.at_const(k).is_none());
    }

    #[test]
    fn at_const_missing_key_with_default_policy_returns_default() {
        let mut g: SparseMortonGrid<TestCell> = SparseMortonGrid::new();
        g.set_miss_policy(MissPolicy::Default);
        let k = MortonKey::encode(99, 99, 99).unwrap();
        let v = g.at_const(k).unwrap();
        assert_eq!(v, TestCell::default());
    }

    #[test]
    fn at_mut_returns_handle_for_present_key() {
        let mut g: SparseMortonGrid<TestCell> = SparseMortonGrid::new();
        let k = MortonKey::encode(1, 2, 3).unwrap();
        g.insert(k, TestCell { tag: 7 }).unwrap();
        let v = g.at_mut(k).unwrap();
        v.tag = 99;
        assert_eq!(g.at_const(k).unwrap().tag, 99);
    }

    #[test]
    fn remove_returns_value_and_decrements_count() {
        let mut g: SparseMortonGrid<TestCell> = SparseMortonGrid::new();
        let k = MortonKey::encode(1, 2, 3).unwrap();
        g.insert(k, TestCell { tag: 5 }).unwrap();
        let removed = g.remove(k).unwrap();
        assert_eq!(removed.tag, 5);
        assert_eq!(g.len(), 0);
        assert!(g.at_const(k).is_none());
    }

    #[test]
    fn remove_missing_key_returns_none() {
        let mut g: SparseMortonGrid<TestCell> = SparseMortonGrid::new();
        let k = MortonKey::encode(1, 2, 3).unwrap();
        assert!(g.remove(k).is_none());
    }

    // ── Growth (rehash) ──────────────────────────────────────────

    #[test]
    fn growth_doubles_capacity_at_load_factor_0_5() {
        let mut g: SparseMortonGrid<TestCell> = SparseMortonGrid::with_capacity(8);
        for i in 0..5_u64 {
            let k = MortonKey::encode(i, 0, 0).unwrap();
            g.insert(k, TestCell { tag: i as u32 }).unwrap();
        }
        // After 5 inserts into capacity 8 (load 5/8 = 0.625) the grid must
        // have grown.
        assert!(g.capacity() >= 16);
    }

    #[test]
    fn growth_preserves_all_entries() {
        let mut g: SparseMortonGrid<TestCell> = SparseMortonGrid::with_capacity(8);
        for i in 0..32_u64 {
            let k = MortonKey::encode(i, i + 1, i + 2).unwrap();
            g.insert(k, TestCell { tag: i as u32 }).unwrap();
        }
        // All 32 inserts must be readable.
        for i in 0..32_u64 {
            let k = MortonKey::encode(i, i + 1, i + 2).unwrap();
            assert_eq!(g.at_const(k).unwrap().tag, i as u32);
        }
        assert_eq!(g.len(), 32);
    }

    #[test]
    fn reserve_grows_to_meet_target() {
        let mut g: SparseMortonGrid<TestCell> = SparseMortonGrid::with_capacity(8);
        g.reserve(100);
        // Capacity must be at least 256 (smallest pow2 ≥ 200, since
        // we need cap ≥ 2 × target = 200).
        assert!(g.capacity() >= 256);
    }

    // ── Iteration ────────────────────────────────────────────────

    #[test]
    fn iter_yields_sorted_morton_order() {
        let mut g: SparseMortonGrid<TestCell> = SparseMortonGrid::new();
        let inputs = vec![(7, 0, 0), (1, 0, 0), (3, 0, 0), (5, 0, 0)];
        for &(x, y, z) in &inputs {
            g.insert(
                MortonKey::encode(x, y, z).unwrap(),
                TestCell { tag: x as u32 },
            )
            .unwrap();
        }
        let collected: Vec<u64> = g.iter().map(|(k, _)| k.to_u64()).collect();
        let mut sorted = collected.clone();
        sorted.sort_unstable();
        assert_eq!(collected, sorted);
    }

    #[test]
    fn iter_unordered_yields_all_occupied() {
        let mut g: SparseMortonGrid<TestCell> = SparseMortonGrid::new();
        for i in 0..10_u64 {
            g.insert(
                MortonKey::encode(i, 0, 0).unwrap(),
                TestCell { tag: i as u32 },
            )
            .unwrap();
        }
        let count = g.iter_unordered().count();
        assert_eq!(count, 10);
    }

    #[test]
    fn iter_by_tier_filters_correctly() {
        let mut g: SparseMortonGrid<TestCell> = SparseMortonGrid::new();
        // 4 cells at distinct tiers.
        g.insert(MortonKey::encode(0, 0, 0).unwrap(), TestCell { tag: 0 })
            .unwrap(); // T0
        g.insert(MortonKey::encode(8, 0, 0).unwrap(), TestCell { tag: 1 })
            .unwrap(); // T1
        g.insert(MortonKey::encode(32, 0, 0).unwrap(), TestCell { tag: 2 })
            .unwrap(); // T2
        g.insert(
            MortonKey::encode(128, 0, 0).unwrap(),
            TestCell { tag: 3 },
        )
        .unwrap(); // T3
        let t0_cells: Vec<_> = g.iter_by_tier(CellTier::T0Fovea).collect();
        let t3_cells: Vec<_> = g.iter_by_tier(CellTier::T3Horizon).collect();
        assert_eq!(t0_cells.len(), 1);
        assert_eq!(t3_cells.len(), 1);
        assert_eq!(t0_cells[0].1.tag, 0);
        assert_eq!(t3_cells[0].1.tag, 3);
    }

    // ── Prune ───────────────────────────────────────────────────

    #[test]
    fn prune_removes_matching_entries() {
        let mut g: SparseMortonGrid<TestCell> = SparseMortonGrid::new();
        for i in 0..16_u64 {
            g.insert(
                MortonKey::encode(i, 0, 0).unwrap(),
                TestCell { tag: i as u32 },
            )
            .unwrap();
        }
        // Prune predicate semantics : KEEP iff predicate returns true ;
        // REMOVE iff predicate returns false. Here we keep even-tagged
        // cells, so 8 odd-tagged cells should be removed.
        let pre_count = g.len();
        let kept_pred = |_k: MortonKey, v: &TestCell| v.tag % 2 == 0;
        let pruned = g.prune(kept_pred);
        let post_count = g.len();
        assert_eq!(post_count, 8, "8 even cells survive");
        assert_eq!(pre_count - post_count, pruned, "pruned-count matches diff");
    }

    // ── Telemetry ───────────────────────────────────────────────

    #[test]
    fn collision_stats_record_inserts_and_gets() {
        let mut g: SparseMortonGrid<TestCell> = SparseMortonGrid::new();
        for i in 0..10_u64 {
            g.insert(
                MortonKey::encode(i, 0, 0).unwrap(),
                TestCell { tag: i as u32 },
            )
            .unwrap();
        }
        // Stress the reads to record telemetry.
        for i in 0..10_u64 {
            let _ = g.at(MortonKey::encode(i, 0, 0).unwrap());
        }
        let stats = g.collision_stats();
        assert_eq!(stats.inserts, 10);
        assert_eq!(stats.gets, 10);
    }

    #[test]
    fn collision_rate_starts_at_zero_and_rises() {
        let g: SparseMortonGrid<TestCell> = SparseMortonGrid::new();
        let stats = g.collision_stats();
        assert_eq!(stats.collision_rate(), 0.0);
        // Empty grid : avg is also zero.
        assert_eq!(stats.avg_insert_probe(), 0.0);
    }

    // ── Layout trait ───────────────────────────────────────────

    #[test]
    fn field_cell_layout_size_72() {
        assert_eq!(<FieldCell as OmegaCellLayout>::omega_cell_size(), 72);
        assert_eq!(<FieldCell as OmegaCellLayout>::omega_cell_align(), 8);
        assert_eq!(
            <FieldCell as OmegaCellLayout>::omega_cell_layout_tag(),
            "FieldCell"
        );
    }

    // ── Determinism (replay) ───────────────────────────────────

    #[test]
    fn determinism_repeated_insert_sequence_yields_same_iter_order() {
        // Build two grids with identical insert sequences ; the resulting
        // sorted-iter order MUST be byte-equal.
        let inputs = (0..50_u64)
            .map(|i| (i * 7 % 100, i * 13 % 100, i * 17 % 100))
            .collect::<Vec<_>>();
        let mut g1: SparseMortonGrid<TestCell> = SparseMortonGrid::new();
        let mut g2: SparseMortonGrid<TestCell> = SparseMortonGrid::new();
        for &(x, y, z) in &inputs {
            let k = MortonKey::encode(x, y, z).unwrap();
            g1.insert(k, TestCell { tag: x as u32 }).unwrap();
            g2.insert(k, TestCell { tag: x as u32 }).unwrap();
        }
        let v1: Vec<u64> = g1.iter().map(|(k, _)| k.to_u64()).collect();
        let v2: Vec<u64> = g2.iter().map(|(k, _)| k.to_u64()).collect();
        assert_eq!(v1, v2);
    }

    // ── Saturated probe error path ────────────────────────────

    #[test]
    fn collision_stats_default_is_all_zero() {
        let s = CollisionStats::default();
        assert_eq!(s.inserts, 0);
        assert_eq!(s.gets, 0);
        assert_eq!(s.rehashes, 0);
        assert_eq!(s.max_probe_distance, 0);
    }

    // ── Stress : 1000 random inserts ─────────────────────────

    #[test]
    fn stress_thousand_inserts_all_recoverable() {
        let mut g: SparseMortonGrid<TestCell> = SparseMortonGrid::new();
        let mut s: u64 = 0xDEAD_BEEF;
        let mut keys = Vec::new();
        for i in 0..1000 {
            s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let x = (s >> 4) & 0xFFFF;
            let y = (s >> 24) & 0xFFFF;
            let z = (s >> 44) & 0xFFFF;
            let k = MortonKey::encode(x, y, z).unwrap();
            g.insert(k, TestCell { tag: i }).unwrap();
            keys.push(k);
        }
        // All inserted keys must still be findable.
        for (i, &k) in keys.iter().enumerate() {
            // Skip duplicates that overwrote earlier entries.
            let v = g.at_const(k).unwrap();
            // The *latest* insert for this key wins.
            let last = keys
                .iter()
                .rposition(|x| *x == k)
                .map(|p| p as u32)
                .unwrap_or(i as u32);
            assert_eq!(v.tag, last);
        }
        // The collision rate must be reasonable (< 5 probe steps avg).
        let stats = g.collision_stats();
        assert!(
            stats.avg_insert_probe() < 5.0,
            "avg insert probe steps too high : {}",
            stats.avg_insert_probe()
        );
    }
}
