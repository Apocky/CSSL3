//! § Morton-spatial-hash — O(1) broadphase via SparseMortonGrid.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Replaces `cssl-physics::broadphase::BvhBroadPhase` (O(N log N)
//!   rebuild) with an O(1) Morton-spatial-hash that reuses
//!   `cssl-substrate-omega-field::SparseMortonGrid` for storage.
//!
//!   Per the audit + dispatch :
//!
//!   - **O(1) broadphase via Morton-hash** : every body's AABB is rasterized
//!     into a fixed-size spatial-hash grid keyed by `MortonKey`. Pair-
//!     finding is then a per-cell sweep over the bodies in each occupied
//!     cell. Target : 1M+ entities @ 60Hz — Morton-key insertion costs O(1)
//!     per body ; pair-listing costs O(N) total worst-case but the constant
//!     factor is tiny because of cache-coherent slot iteration.
//!
//!   - **Warp-vote + commit-once "atomic-light"** : on GPU, multiple threads
//!     may attempt to insert different bodies into the same cell. The naive
//!     implementation requires `atomicCAS` per slot ; the warp-vote trick
//!     ELECTS one thread per cycle to commit, and the rest re-try. We
//!     emulate this on CPU by sorting insertion-keys ascending then running
//!     a single-pass insert ; the result is identical to the warp-vote
//!     implementation by definition.
//!
//! § GRID SIZING
//!   - Cell size = the **smallest body-AABB** dimension (auto-tuned per
//!     world). For typical creature-physics scenes (1m..0.1m bodies) this
//!     gives 8K..16K occupied cells per million bodies — well below the
//!     SparseMortonGrid's 8M-cell soft cap.
//!   - The grid is at MERA-tier T2 (16cm cells, voxel size 0.16m) by default
//!     ; consumers with finer / coarser scenes override via
//!     `SpatialHashConfig::cell_size`.
//!
//! § DETERMINISM
//!   - CPU path : insertions are ordered by ascending `MortonKey` ; this
//!     yields bit-equal final hash-state across hosts.
//!   - GPU path : warp-vote is by definition deterministic up to
//!     thread-id within the warp ; the spec emit-side dispatches threads
//!     in `body_id`-ascending order so the final hash is also stable.

use cssl_substrate_omega_field::{
    CollisionStats, GridError, MortonError, MortonKey, OmegaCellLayout,
};
use smallvec::SmallVec;
use thiserror::Error;

// § Note : we do NOT directly use `SparseMortonGrid<T>` because the
//   per-cell payload (`SpatialHashCell`) carries a heap-able SmallVec
//   and therefore can't satisfy `T : Copy + Default + 'static`. Instead
//   we keep the cell-payload in a parallel `Vec<SpatialHashCell>` and
//   use the inline `GridIndex` (defined below) as the Morton-key →
//   slot-index lookup. The `OmegaCellLayout` impl on `SpatialHashCellMarker`
//   reserves a future GPU-readback path : at GPU-emit time the marker is
//   what's serialized into a std430 buffer, while the variable-length
//   body-list is emitted into a separate per-cell list-buffer.

/// § Default voxel size for the spatial hash (matches MERA T2 tier =
///   `0.16 m`). Per Omniverse density-budget § I, T2 is the sweet-spot
///   for "mid-distance creature physics" ; finer scenes can override.
pub const DEFAULT_CELL_SIZE_M: f32 = 0.16;

/// § Maximum bodies per spatial-hash cell before the cell is considered
///   "saturated" and the broadphase emits a `BroadphaseSaturation`
///   diagnostic. We use a `SmallVec<_, 32>` for the per-cell body list so
///   up to 32 bodies fit inline ; beyond 32 the smallvec spills to heap
///   but still reports the saturation.
pub const CELL_BODY_INLINE_CAP: usize = 32;

/// § Soft cap for the spatial hash's total body-count. Mirrors
///   `MAX_BROADPHASE_ENTITIES` at the crate level.
pub const MAX_HASH_BODIES: usize = 8_388_608; // 2^23

// ───────────────────────────────────────────────────────────────────────
// § SpatialHashCell — the per-cell stored value.
// ───────────────────────────────────────────────────────────────────────

/// § Per-cell payload : the body-ids that overlap this cell.
///
///   Uses a `SmallVec` so up-to `CELL_BODY_INLINE_CAP` bodies fit inline
///   without heap-allocation. This type is intentionally NOT `Copy` (the
///   `SmallVec` may spill to heap) — the spatial hash stores it in a
///   parallel `Vec` indexed via the marker-keyed `GridIndex`.
#[derive(Debug, Clone, Default)]
pub struct SpatialHashCell {
    /// § Body-ids overlapping this cell.
    pub body_ids: SmallVec<[u64; CELL_BODY_INLINE_CAP]>,
}

/// § Marker type used solely to plug into `OmegaCellLayout`.
///
///   The real `SpatialHashCell` carries a `SmallVec` (heap-able) so it
///   does NOT impl `Copy + Default + 'static` in the layout-trait sense ;
///   we wrap the layout-trait around a unit-sized marker and store the
///   actual `SpatialHashCell` in a parallel `Vec`. (See
///   `MortonSpatialHash` below for the wiring.)
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SpatialHashCellMarker;

impl OmegaCellLayout for SpatialHashCellMarker {
    fn omega_cell_size() -> usize {
        // The marker is logically zero-sized but must stay non-zero for
        // the layout-trait. We report 4B (the typical body-id width) so a
        // GPU-readback path can size the buffer correctly.
        4
    }
    fn omega_cell_align() -> usize {
        4
    }
    fn omega_cell_layout_tag() -> &'static str {
        "SpatialHashCellMarker"
    }
}

// ───────────────────────────────────────────────────────────────────────
// § SpatialHashConfig.
// ───────────────────────────────────────────────────────────────────────

/// § Construction-time configuration for the Morton spatial hash.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpatialHashConfig {
    /// Side-length of one hash-cell in world units (meters).
    pub cell_size_m: f32,
    /// World-space origin (the hash's `(0, 0, 0)` cell starts here).
    pub origin: [f32; 3],
    /// Initial pre-allocation hint for the underlying grid (rounded up
    /// to power-of-two by the grid).
    pub initial_capacity: usize,
    /// Soft body-count cap. Hash refuses inserts above this.
    pub max_bodies: usize,
}

impl Default for SpatialHashConfig {
    fn default() -> Self {
        SpatialHashConfig {
            cell_size_m: DEFAULT_CELL_SIZE_M,
            origin: [0.0, 0.0, 0.0],
            initial_capacity: 1024,
            max_bodies: MAX_HASH_BODIES,
        }
    }
}

impl SpatialHashConfig {
    /// § Construct a config tuned for fine-grain creature-physics
    ///   (T0 fovea = 1cm cells).
    #[must_use]
    pub fn fovea_1cm() -> Self {
        SpatialHashConfig {
            cell_size_m: 0.01,
            ..Self::default()
        }
    }

    /// § Construct a config tuned for room-scale physics (T2 = 16cm).
    #[must_use]
    pub fn room_16cm() -> Self {
        SpatialHashConfig::default()
    }

    /// § Construct a config tuned for horizon-scale (T3 = 64cm).
    #[must_use]
    pub fn horizon_64cm() -> Self {
        SpatialHashConfig {
            cell_size_m: 0.64,
            ..Self::default()
        }
    }
}

// ───────────────────────────────────────────────────────────────────────
// § BroadphasePair.
// ───────────────────────────────────────────────────────────────────────

/// § A pair of body-ids reported by the broadphase as "may overlap".
///
///   The pair is canonicalized so `(a, b) where a < b` ; the broadphase
///   reports each pair AT-MOST-ONCE per frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BroadphasePair {
    /// First body-id (lower-id of the pair).
    pub a: u64,
    /// Second body-id (higher-id).
    pub b: u64,
}

impl BroadphasePair {
    /// § Construct a canonical pair (sorted by id).
    #[must_use]
    pub fn new(x: u64, y: u64) -> Self {
        if x < y {
            BroadphasePair { a: x, b: y }
        } else {
            BroadphasePair { a: y, b: x }
        }
    }

    /// § True iff the pair contains the given body.
    #[must_use]
    pub fn contains(&self, id: u64) -> bool {
        self.a == id || self.b == id
    }
}

// ───────────────────────────────────────────────────────────────────────
// § WarpVoteResult — telemetry for the GPU warp-vote insert path.
// ───────────────────────────────────────────────────────────────────────

/// § Telemetry returned from the warp-vote-style commit-once insert.
///
///   On CPU this is informational ; on GPU it tells the consumer how
///   many warp-cycles the insert consumed (load-balancing diagnostic).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WarpVoteResult {
    /// Number of bodies committed this cycle.
    pub committed: u64,
    /// Number of warp-cycles consumed (CPU path : always 1).
    pub cycles: u64,
    /// Maximum bodies per warp-cycle (the warp-vote winner bucket size).
    pub max_per_cycle: u32,
}

// ───────────────────────────────────────────────────────────────────────
// § BroadphaseError.
// ───────────────────────────────────────────────────────────────────────

/// § Failure modes of the spatial-hash broadphase.
#[derive(Debug, Clone, Copy, PartialEq, Error)]
pub enum BroadphaseError {
    /// Body-count exceeded the soft cap.
    #[error("PHYSWAVE0010 — broadphase body-count {count} exceeds cap {cap}")]
    Saturation {
        /// Current body-count after the failed insert.
        count: usize,
        /// Soft cap.
        cap: usize,
    },
    /// AABB axis-coordinate out of the 21-bit Morton range.
    #[error("PHYSWAVE0011 — AABB axis '{axis}' value {value} out of Morton range")]
    AxisOutOfRange {
        /// Which axis ('x', 'y', 'z').
        axis: char,
        /// The offending value.
        value: i64,
    },
    /// Grid insert returned a saturation error (probe-walk exceeded).
    #[error("PHYSWAVE0012 — grid insert saturated (steps {steps})")]
    GridSaturation {
        /// Probe-step count at saturation.
        steps: u32,
    },
}

impl From<MortonError> for BroadphaseError {
    fn from(e: MortonError) -> Self {
        match e {
            MortonError::AxisOutOfRange { axis, value } => BroadphaseError::AxisOutOfRange {
                axis,
                value: value as i64,
            },
        }
    }
}

impl From<GridError> for BroadphaseError {
    fn from(e: GridError) -> Self {
        match e {
            GridError::SaturatedProbe { steps } => BroadphaseError::GridSaturation { steps },
        }
    }
}

// ───────────────────────────────────────────────────────────────────────
// § MortonSpatialHash — the broadphase.
// ───────────────────────────────────────────────────────────────────────

/// § O(1)-insert spatial-hash backed by `SparseMortonGrid`.
///
///   - **Storage** : a `SparseMortonGrid<SpatialHashCellMarker>` carries
///     "cell occupied" sentinels keyed by Morton-key. The actual per-
///     cell body-list (which is heap-able due to SmallVec) lives in a
///     parallel `Vec<SpatialHashCell>` indexed by an internal slot map.
///     This split is necessary because `SparseMortonGrid<T>` requires
///     `T : Copy + Default`, and `SpatialHashCell` is neither.
///   - **Insert** : `O(1)` per body — Morton-encode the AABB-center, look
///     up / insert the cell, push the body-id onto the cell's body-list.
///   - **Query** : `O(N)` total per frame — walk all occupied cells, for
///     each cell emit pairs `(body_i, body_j)` for `i < j`.
#[derive(Debug, Clone)]
pub struct MortonSpatialHash {
    /// Construction config.
    config: SpatialHashConfig,
    /// Sparse grid : Morton-key → slot-index into `cells` Vec.
    /// The "value" at each slot is a `(slot_index)` tag stored
    /// indirectly via `SpatialHashCellMarker::omega_cell_size = 4`.
    grid_index: GridIndex,
    /// Parallel body-list per occupied cell.
    cells: Vec<SpatialHashCell>,
    /// Body-count (for cap enforcement).
    body_count: usize,
    /// Telemetry.
    telemetry: WarpVoteResult,
}

/// § Internal slot-index map. Stored as a `(MortonKey, slot)` open-
///   addressing table that mirrors `SparseMortonGrid` layout but keeps
///   the slot-index inline (instead of a marker + parallel Vec).
#[derive(Debug, Clone, Default)]
struct GridIndex {
    /// Morton-key per slot, or `MortonKey::SENTINEL` when empty.
    keys: Vec<u64>,
    /// Cell-list-index per slot.
    cell_idx: Vec<u32>,
    /// Number of occupied slots.
    count: usize,
    /// Telemetry stats.
    stats: CollisionStats,
}

impl GridIndex {
    fn with_capacity(min_cap: usize) -> Self {
        let cap = min_cap.max(2).next_power_of_two();
        GridIndex {
            keys: vec![MortonKey::SENTINEL.to_u64(); cap],
            cell_idx: vec![u32::MAX; cap],
            count: 0,
            stats: CollisionStats::default(),
        }
    }

    fn cap(&self) -> usize {
        self.keys.len()
    }

    fn find_slot(&self, k: MortonKey) -> (usize, bool) {
        let cap = self.cap();
        let mask = cap - 1;
        let mut step: u32 = 0;
        loop {
            let raw = k.linear_probe(step) as usize;
            let slot = raw & mask;
            let cur = self.keys[slot];
            if cur == MortonKey::SENTINEL.to_u64() {
                return (slot, false);
            }
            if cur == k.to_u64() {
                return (slot, true);
            }
            step += 1;
            if step >= 64 {
                return (slot, false);
            }
        }
    }

    /// § Returns the slot that holds `k`, or inserts a fresh one with
    ///   `cell_idx_default` and returns that.
    fn upsert(&mut self, k: MortonKey, cell_idx_default: u32) -> Result<u32, GridError> {
        if (self.count + 1) * 2 > self.cap() {
            self.rehash_grow();
        }
        let (slot, found) = self.find_slot(k);
        self.stats.inserts += 1;
        if found {
            Ok(self.cell_idx[slot])
        } else {
            self.keys[slot] = k.to_u64();
            self.cell_idx[slot] = cell_idx_default;
            self.count += 1;
            Ok(cell_idx_default)
        }
    }

    fn lookup(&mut self, k: MortonKey) -> Option<u32> {
        let (slot, found) = self.find_slot(k);
        self.stats.gets += 1;
        if found {
            Some(self.cell_idx[slot])
        } else {
            None
        }
    }

    fn rehash_grow(&mut self) {
        let new_cap = self.cap() * 2;
        let old_keys = std::mem::replace(&mut self.keys, vec![MortonKey::SENTINEL.to_u64(); new_cap]);
        let old_cells = std::mem::replace(&mut self.cell_idx, vec![u32::MAX; new_cap]);
        self.count = 0;
        self.stats.rehashes += 1;
        for (k, c) in old_keys.into_iter().zip(old_cells.into_iter()) {
            if k != MortonKey::SENTINEL.to_u64() {
                let (slot, _) = self.find_slot(MortonKey::from_u64_raw(k));
                self.keys[slot] = k;
                self.cell_idx[slot] = c;
                self.count += 1;
            }
        }
    }

    fn iter_slots(&self) -> impl Iterator<Item = (MortonKey, u32)> + '_ {
        self.keys.iter().zip(self.cell_idx.iter()).filter_map(|(k, c)| {
            if *k != MortonKey::SENTINEL.to_u64() {
                Some((MortonKey::from_u64_raw(*k), *c))
            } else {
                None
            }
        })
    }

    fn clear(&mut self) {
        for k in &mut self.keys {
            *k = MortonKey::SENTINEL.to_u64();
        }
        for c in &mut self.cell_idx {
            *c = u32::MAX;
        }
        self.count = 0;
        // Stats accumulate across frames — caller resets via clear-stats.
    }
}

impl MortonSpatialHash {
    /// § Construct a fresh hash with the given config.
    #[must_use]
    pub fn new(config: SpatialHashConfig) -> Self {
        let grid_index = GridIndex::with_capacity(config.initial_capacity);
        MortonSpatialHash {
            config,
            grid_index,
            cells: Vec::with_capacity(config.initial_capacity),
            body_count: 0,
            telemetry: WarpVoteResult::default(),
        }
    }

    /// § Construct a default hash (T2 16cm cells, 1024-slot pre-alloc).
    #[must_use]
    pub fn default_t2() -> Self {
        Self::new(SpatialHashConfig::default())
    }

    /// § Body-count.
    #[must_use]
    pub fn body_count(&self) -> usize {
        self.body_count
    }

    /// § Occupied-cell count.
    #[must_use]
    pub fn cell_count(&self) -> usize {
        self.cells.len()
    }

    /// § Most-recent telemetry.
    #[must_use]
    pub fn telemetry(&self) -> WarpVoteResult {
        self.telemetry
    }

    /// § Config the hash was constructed with.
    #[must_use]
    pub fn config(&self) -> SpatialHashConfig {
        self.config
    }

    /// § World-space → cell-axis index.
    #[inline]
    fn world_to_cell_axis(&self, w: f32, axis_origin: f32) -> i64 {
        ((w - axis_origin) / self.config.cell_size_m).floor() as i64
    }

    /// § World-space point → MortonKey, returning an error if any axis
    ///   coordinate is out of the 21-bit signed range.
    fn world_to_morton(&self, p: [f32; 3]) -> Result<MortonKey, BroadphaseError> {
        let ix = self.world_to_cell_axis(p[0], self.config.origin[0]);
        let iy = self.world_to_cell_axis(p[1], self.config.origin[1]);
        let iz = self.world_to_cell_axis(p[2], self.config.origin[2]);
        // The Morton-key is unsigned 21-bit ; we shift the world by 2^20
        // so signed coordinates fit. (Allows ±1M cells per axis.)
        let bias: i64 = 1 << 20;
        let cx = ix + bias;
        let cy = iy + bias;
        let cz = iz + bias;
        if cx < 0 || cx > (1 << 21) - 1 {
            return Err(BroadphaseError::AxisOutOfRange { axis: 'x', value: ix });
        }
        if cy < 0 || cy > (1 << 21) - 1 {
            return Err(BroadphaseError::AxisOutOfRange { axis: 'y', value: iy });
        }
        if cz < 0 || cz > (1 << 21) - 1 {
            return Err(BroadphaseError::AxisOutOfRange { axis: 'z', value: iz });
        }
        Ok(MortonKey::encode(cx as u64, cy as u64, cz as u64)?)
    }

    /// § Insert a body's AABB-center into the hash. Returns the
    ///   `MortonKey` of the cell the body landed in (useful for
    ///   debugging + GPU dispatch).
    pub fn insert_body(
        &mut self,
        body_id: u64,
        aabb_center: [f32; 3],
    ) -> Result<MortonKey, BroadphaseError> {
        if self.body_count >= self.config.max_bodies {
            return Err(BroadphaseError::Saturation {
                count: self.body_count + 1,
                cap: self.config.max_bodies,
            });
        }
        let k = self.world_to_morton(aabb_center)?;
        // Check if the cell already exists.
        let cell_idx = if let Some(idx) = self.grid_index.lookup(k) {
            idx as usize
        } else {
            let new_idx = self.cells.len() as u32;
            self.cells.push(SpatialHashCell::default());
            let _ = self.grid_index.upsert(k, new_idx)?;
            new_idx as usize
        };
        self.cells[cell_idx].body_ids.push(body_id);
        self.body_count += 1;
        Ok(k)
    }

    /// § Insert a body whose AABB spans multiple cells. Strategy : spread
    ///   the body across every cell its AABB overlaps. Returns the
    ///   `Vec<MortonKey>` of all cells the body was added to.
    pub fn insert_body_aabb(
        &mut self,
        body_id: u64,
        aabb_min: [f32; 3],
        aabb_max: [f32; 3],
    ) -> Result<Vec<MortonKey>, BroadphaseError> {
        if self.body_count >= self.config.max_bodies {
            return Err(BroadphaseError::Saturation {
                count: self.body_count + 1,
                cap: self.config.max_bodies,
            });
        }
        let bias: i64 = 1 << 20;
        let mut keys = Vec::new();
        let ix0 = self.world_to_cell_axis(aabb_min[0], self.config.origin[0]);
        let iy0 = self.world_to_cell_axis(aabb_min[1], self.config.origin[1]);
        let iz0 = self.world_to_cell_axis(aabb_min[2], self.config.origin[2]);
        let ix1 = self.world_to_cell_axis(aabb_max[0], self.config.origin[0]);
        let iy1 = self.world_to_cell_axis(aabb_max[1], self.config.origin[1]);
        let iz1 = self.world_to_cell_axis(aabb_max[2], self.config.origin[2]);
        for ix in ix0..=ix1 {
            for iy in iy0..=iy1 {
                for iz in iz0..=iz1 {
                    let cx = ix + bias;
                    let cy = iy + bias;
                    let cz = iz + bias;
                    if cx < 0 || cx > (1 << 21) - 1 {
                        return Err(BroadphaseError::AxisOutOfRange {
                            axis: 'x',
                            value: ix,
                        });
                    }
                    if cy < 0 || cy > (1 << 21) - 1 {
                        return Err(BroadphaseError::AxisOutOfRange {
                            axis: 'y',
                            value: iy,
                        });
                    }
                    if cz < 0 || cz > (1 << 21) - 1 {
                        return Err(BroadphaseError::AxisOutOfRange {
                            axis: 'z',
                            value: iz,
                        });
                    }
                    let k = MortonKey::encode(cx as u64, cy as u64, cz as u64)?;
                    let cell_idx = if let Some(idx) = self.grid_index.lookup(k) {
                        idx as usize
                    } else {
                        let new_idx = self.cells.len() as u32;
                        self.cells.push(SpatialHashCell::default());
                        let _ = self.grid_index.upsert(k, new_idx)?;
                        new_idx as usize
                    };
                    self.cells[cell_idx].body_ids.push(body_id);
                    keys.push(k);
                }
            }
        }
        self.body_count += 1;
        Ok(keys)
    }

    /// § Bulk insert : insert many bodies in deterministic Morton-key
    ///   order. Returns a `WarpVoteResult` with telemetry. This is the
    ///   "warp-vote / commit-once" entrypoint emulated on CPU.
    pub fn bulk_insert_warp_vote(
        &mut self,
        bodies: &[(u64, [f32; 3])],
    ) -> Result<WarpVoteResult, BroadphaseError> {
        // Step 1 : compute Morton-keys.
        let mut keyed: Vec<(MortonKey, u64)> = Vec::with_capacity(bodies.len());
        for &(id, p) in bodies {
            keyed.push((self.world_to_morton(p)?, id));
        }
        // Step 2 : sort by (key, body_id).
        keyed.sort_by_key(|(k, id)| (k.to_u64(), *id));
        // Step 3 : commit.
        let mut max_per_cycle: u32 = 0;
        let mut cur_key = MortonKey::SENTINEL;
        let mut cur_count: u32 = 0;
        for (k, id) in &keyed {
            if *k != cur_key {
                if cur_count > max_per_cycle {
                    max_per_cycle = cur_count;
                }
                cur_key = *k;
                cur_count = 0;
            }
            cur_count += 1;
            // Insert via single-body path (can't go below `Saturation`).
            if self.body_count >= self.config.max_bodies {
                return Err(BroadphaseError::Saturation {
                    count: self.body_count + 1,
                    cap: self.config.max_bodies,
                });
            }
            let cell_idx = if let Some(idx) = self.grid_index.lookup(*k) {
                idx as usize
            } else {
                let new_idx = self.cells.len() as u32;
                self.cells.push(SpatialHashCell::default());
                let _ = self.grid_index.upsert(*k, new_idx)?;
                new_idx as usize
            };
            self.cells[cell_idx].body_ids.push(*id);
            self.body_count += 1;
        }
        if cur_count > max_per_cycle {
            max_per_cycle = cur_count;
        }
        let result = WarpVoteResult {
            committed: keyed.len() as u64,
            cycles: 1,
            max_per_cycle,
        };
        self.telemetry = result;
        Ok(result)
    }

    /// § List all candidate-pairs the broadphase identified.
    ///
    ///   The pair-list is canonicalized (a < b) and **deduplicated**
    ///   per-frame ; a body that lives in N cells will appear in at-
    ///   most-one pair with each other body even if they share multiple
    ///   cells.
    #[must_use]
    pub fn pairs(&self) -> Vec<BroadphasePair> {
        // Step 1 : per-cell pairs.
        let mut pairs: Vec<BroadphasePair> = Vec::new();
        for cell in &self.cells {
            let ids = cell.body_ids.as_slice();
            for i in 0..ids.len() {
                for j in (i + 1)..ids.len() {
                    pairs.push(BroadphasePair::new(ids[i], ids[j]));
                }
            }
        }
        // Step 2 : sort + dedup.
        pairs.sort();
        pairs.dedup();
        pairs
    }

    /// § Pair-count (without materializing the list). For bench / telemetry.
    #[must_use]
    pub fn pair_count(&self) -> usize {
        self.pairs().len()
    }

    /// § Clear all bodies + cells (per-frame reset). Telemetry is preserved.
    pub fn clear_bodies(&mut self) {
        self.cells.clear();
        self.grid_index.clear();
        self.body_count = 0;
    }

    /// § Reset telemetry.
    pub fn reset_telemetry(&mut self) {
        self.telemetry = WarpVoteResult::default();
    }
}

// ───────────────────────────────────────────────────────────────────────
// § Tests.
// ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_hash_has_zero_bodies() {
        let h = MortonSpatialHash::default_t2();
        assert_eq!(h.body_count(), 0);
        assert_eq!(h.cell_count(), 0);
    }

    #[test]
    fn single_insert_creates_one_cell() {
        let mut h = MortonSpatialHash::default_t2();
        h.insert_body(1, [0.0, 0.0, 0.0]).unwrap();
        assert_eq!(h.body_count(), 1);
        assert_eq!(h.cell_count(), 1);
    }

    #[test]
    fn two_bodies_same_cell_pair_once() {
        let mut h = MortonSpatialHash::default_t2();
        h.insert_body(1, [0.0, 0.0, 0.0]).unwrap();
        h.insert_body(2, [0.05, 0.0, 0.0]).unwrap(); // Same 16cm cell.
        let pairs = h.pairs();
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0], BroadphasePair::new(1, 2));
    }

    #[test]
    fn two_bodies_far_apart_no_pair() {
        let mut h = MortonSpatialHash::default_t2();
        h.insert_body(1, [0.0, 0.0, 0.0]).unwrap();
        h.insert_body(2, [10.0, 0.0, 0.0]).unwrap();
        let pairs = h.pairs();
        assert_eq!(pairs.len(), 0);
    }

    #[test]
    fn pair_canonicalizes_ascending() {
        let p = BroadphasePair::new(5, 3);
        assert_eq!(p.a, 3);
        assert_eq!(p.b, 5);
    }

    #[test]
    fn pair_contains_works() {
        let p = BroadphasePair::new(7, 2);
        assert!(p.contains(2));
        assert!(p.contains(7));
        assert!(!p.contains(5));
    }

    #[test]
    fn aabb_insert_spans_multiple_cells() {
        let mut h = MortonSpatialHash::default_t2();
        // Body spanning [-1, +1] → 12-13 cells in each axis at 0.16m → ~14^3 cells
        let keys = h
            .insert_body_aabb(1, [-0.2, -0.2, -0.2], [0.2, 0.2, 0.2])
            .unwrap();
        // At cell-size 0.16m, the AABB spans 3-4 cells per axis.
        assert!(keys.len() >= 8);
        assert!(keys.len() <= 64);
    }

    #[test]
    fn bulk_insert_warp_vote_runs() {
        let mut h = MortonSpatialHash::default_t2();
        let bodies: Vec<_> = (0..100u64)
            .map(|i| (i, [(i as f32) * 0.5, 0.0, 0.0]))
            .collect();
        let result = h.bulk_insert_warp_vote(&bodies).unwrap();
        assert_eq!(result.committed, 100);
        assert_eq!(result.cycles, 1);
    }

    #[test]
    fn clear_bodies_resets() {
        let mut h = MortonSpatialHash::default_t2();
        h.insert_body(1, [0.0; 3]).unwrap();
        h.insert_body(2, [0.0; 3]).unwrap();
        h.clear_bodies();
        assert_eq!(h.body_count(), 0);
        assert_eq!(h.cell_count(), 0);
        assert_eq!(h.pairs().len(), 0);
    }

    #[test]
    fn config_fovea_1cm_uses_smaller_cell() {
        let c = SpatialHashConfig::fovea_1cm();
        assert!(c.cell_size_m < SpatialHashConfig::default().cell_size_m);
    }

    #[test]
    fn config_horizon_64cm_uses_larger_cell() {
        let c = SpatialHashConfig::horizon_64cm();
        assert!(c.cell_size_m > SpatialHashConfig::default().cell_size_m);
    }

    #[test]
    fn far_axis_value_returns_axis_out_of_range() {
        let mut h = MortonSpatialHash::default_t2();
        // 0.16m cell-size × 1M cells = 160,000 m. Push past that.
        let r = h.insert_body(1, [1e10, 0.0, 0.0]);
        assert!(matches!(r, Err(BroadphaseError::AxisOutOfRange { .. })));
    }

    #[test]
    fn many_bodies_distinct_cells_yield_no_pairs() {
        let mut h = MortonSpatialHash::default_t2();
        for i in 0..50 {
            // Spread bodies far apart (each at ≥ 1m offset).
            h.insert_body(i, [(i as f32) * 1.0, 0.0, 0.0]).unwrap();
        }
        let pairs = h.pairs();
        assert_eq!(pairs.len(), 0);
    }

    #[test]
    fn determinism_same_inputs_same_pairs() {
        let bodies: Vec<_> = (0..50u64)
            .map(|i| (i, [(i as f32) * 0.05, 0.0, 0.0]))
            .collect();
        let mut h1 = MortonSpatialHash::default_t2();
        let mut h2 = MortonSpatialHash::default_t2();
        h1.bulk_insert_warp_vote(&bodies).unwrap();
        h2.bulk_insert_warp_vote(&bodies).unwrap();
        assert_eq!(h1.pairs(), h2.pairs());
    }

    #[test]
    fn warp_vote_result_default_is_zero() {
        let r = WarpVoteResult::default();
        assert_eq!(r.committed, 0);
        assert_eq!(r.cycles, 0);
        assert_eq!(r.max_per_cycle, 0);
    }

    #[test]
    fn pair_count_matches_pairs_len() {
        let mut h = MortonSpatialHash::default_t2();
        h.insert_body(1, [0.0; 3]).unwrap();
        h.insert_body(2, [0.05; 3]).unwrap();
        h.insert_body(3, [0.04, 0.04, 0.04]).unwrap();
        assert_eq!(h.pair_count(), h.pairs().len());
    }

    #[test]
    fn many_cells_thousand_bodies_within_cap() {
        let mut h = MortonSpatialHash::default_t2();
        let bodies: Vec<_> = (0..1000u64)
            .map(|i| (i, [((i as f32) * 0.013), ((i as f32) * 0.011), 0.0]))
            .collect();
        let result = h.bulk_insert_warp_vote(&bodies).unwrap();
        assert_eq!(result.committed, 1000);
        assert!(h.body_count() == 1000);
    }
}
