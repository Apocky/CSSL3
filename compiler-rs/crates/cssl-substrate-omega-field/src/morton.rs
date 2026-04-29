//! § Morton-key — 21-bit-per-axis Z-order curve encoding for Ω-field cells.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Defines [`MortonKey`] — the canonical sparse-grid index for every Ω-field
//!   cell + every overlay. The encoding interleaves three 21-bit axis indices
//!   into a 63-bit space-filling curve, leaving the high bit (bit 63) for an
//!   occupancy/sentinel flag the open-addressing hashtable uses to distinguish
//!   "empty slot" from "valid key zero".
//!
//! § SPEC
//!   - `Omniverse/04_OMEGA_FIELD/02_STORAGE.csl.md` § II `morton3_u64(ix, iy, iz)
//!     ⊗ 21 bits per axis ⊗ 2M voxels per axis`
//!   - `Omniverse/02_CSSL/06_SUBSTRATE_EVOLUTION.csl` § IV.2 `MortonKey =
//!     u64'refined<{k: u64 | k.morton_valid()}>`
//!   - `Omniverse/04_OMEGA_FIELD/05_DENSITY_BUDGET.csl.md` § I `cascade-tiers`
//!     (1cm fovea / 4cm mid / 16cm distant / 64cm horizon).
//!
//! § DETERMINISM CONTRACT
//!   The encoding MUST be bit-equal across hosts (replay-determinism). The
//!   bit-spreading uses a fixed 4-step magic-number sequence — no
//!   `std::arch::x86_64::_pdep_u64` / BMI2 paths that vary by hardware. This
//!   is the load-bearing landmine called out in T11-D144.
//!
//! § BIT-LAYOUT (63 bits ; bit 63 reserved for occupancy)
//!   ```text
//!   bit 63        : reserved (0 = valid key, 1 = sentinel/empty in tables)
//!   bits 0..=62   : interleaved Z-order
//!     bit 3i + 0  : x[i]   for i in 0..=20
//!     bit 3i + 1  : y[i]   for i in 0..=20
//!     bit 3i + 2  : z[i]   for i in 0..=20
//!   ```
//!
//! § AXIS RANGE
//!   Each axis index is 21 bits = 0..=(2^21 - 1) = 0..=2_097_151. At the
//!   canonical 0.125 m cell-size that gives 262_144 m per axis (262 km),
//!   well beyond M7 vertical-slice scope (~256 m horizon). At the 1 cm fovea
//!   tier that's still 20_971 m (20 km) — also fine.

/// Mask of the 21 low bits used per axis. Each axis index must satisfy
/// `i & MORTON_AXIS_MASK == i` ; otherwise [`MortonKey::encode`] returns
/// [`MortonError::AxisOutOfRange`].
pub const MORTON_AXIS_MASK: u64 = (1u64 << 21) - 1;
/// Maximum legal axis index (inclusive).
pub const MORTON_AXIS_MAX: u64 = MORTON_AXIS_MASK;
/// Reserved high-bit indicating "sentinel / empty slot" inside the open-
/// addressing hashtable. Real keys clear this bit ; the table uses it to
/// distinguish an occupied-zero key from an empty cell without a parallel
/// occupancy bitmap.
pub const MORTON_SENTINEL_BIT: u64 = 1u64 << 63;
/// Width of one axis in bits.
pub const MORTON_AXIS_WIDTH: u32 = 21;
/// Total payload width (3 axes × 21 bits).
pub const MORTON_PAYLOAD_WIDTH: u32 = 63;

// ───────────────────────────────────────────────────────────────────────
// § Bit-spreading helpers — fixed magic-number sequence for determinism.
// ───────────────────────────────────────────────────────────────────────

/// Spread a 21-bit value `v` so each input bit `i` ends up at output bit
/// `3 * i`. Uses a 4-step cascade of mask-shift-mask operations matching
/// the canonical "split by 3" Morton encoding (Wikipedia "Z-order curve",
/// AMD whitepaper "Morton encoding via SIMD").
///
/// § DETERMINISM : the mask constants are fixed ; identical bytes-out for
/// the same bytes-in on any 64-bit host (LE or BE). This is the load-
/// bearing replay-determinism guarantee called out by T11-D144's landmine
/// note.
#[inline]
#[must_use]
pub const fn morton_split_by_3(v: u64) -> u64 {
    // Mask to 21 bits first so out-of-range inputs do not pollute high bits.
    let mut x: u64 = v & MORTON_AXIS_MASK;
    // Step 1 : 0x...000FFC00000003FF → split-21 → split into two 11/10 halves.
    x = (x | (x << 32)) & 0x001F_0000_0000_FFFF;
    // Step 2 : split into 4 chunks.
    x = (x | (x << 16)) & 0x001F_0000_FF00_00FF;
    // Step 3 : split into 8 chunks.
    x = (x | (x << 8)) & 0x100F_00F0_0F00_F00F;
    // Step 4 : split into 16 chunks.
    x = (x | (x << 4)) & 0x10C3_0C30_C30C_30C3;
    // Step 5 : every-3rd-bit pattern.
    x = (x | (x << 2)) & 0x1249_2492_4924_9249;
    x
}

/// Inverse of [`morton_split_by_3`] — collect every-3rd bit back into a
/// contiguous 21-bit value. Same magic-number cascade, run in reverse.
#[inline]
#[must_use]
pub const fn morton_compact_by_3(v: u64) -> u64 {
    let mut x: u64 = v & 0x1249_2492_4924_9249;
    x = (x ^ (x >> 2)) & 0x10C3_0C30_C30C_30C3;
    x = (x ^ (x >> 4)) & 0x100F_00F0_0F00_F00F;
    x = (x ^ (x >> 8)) & 0x001F_0000_FF00_00FF;
    x = (x ^ (x >> 16)) & 0x001F_0000_0000_FFFF;
    x = (x ^ (x >> 32)) & MORTON_AXIS_MASK;
    x
}

// ───────────────────────────────────────────────────────────────────────
// § MortonKey — the public key type used everywhere in the substrate.
// ───────────────────────────────────────────────────────────────────────

/// 63-bit Morton-encoded (x, y, z) cell index. Bit 63 reserved for the
/// open-addressing-table sentinel ; all keys produced by [`MortonKey::encode`]
/// have bit 63 == 0.
///
/// § INVARIANTS
///   - `MortonKey::encode(x, y, z)` returns Some iff x, y, z are all
///     ≤ `MORTON_AXIS_MAX`.
///   - `MortonKey::decode(k)` is the unique pre-image for any valid key
///     produced by [`MortonKey::encode`]. (Roundtrip property.)
///   - Bit 63 is always 0 for valid keys.
///   - Lexicographic ordering of (x, y, z) is NOT preserved by Morton
///     ordering ; instead, Morton-ordering preserves locality (Z-order
///     space-filling curve property). Iter-by-MortonKey traverses cells in
///     locality-coherent batches, which is the property we want for cache
///     coherence on iter+map workloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(transparent)]
pub struct MortonKey(u64);

impl MortonKey {
    /// The "zero" key — encodes (0, 0, 0). Distinct from [`Self::SENTINEL`].
    pub const ZERO: MortonKey = MortonKey(0);

    /// The sentinel key — bit 63 set, indicates "empty slot" in tables.
    /// Real keys NEVER produce this value via [`Self::encode`].
    pub const SENTINEL: MortonKey = MortonKey(MORTON_SENTINEL_BIT);

    /// Encode three axis indices into a Morton key.
    ///
    /// # Errors
    /// Returns [`MortonError::AxisOutOfRange`] if any of `x`, `y`, `z`
    /// exceeds [`MORTON_AXIS_MAX`].
    pub fn encode(x: u64, y: u64, z: u64) -> Result<MortonKey, MortonError> {
        if x > MORTON_AXIS_MAX {
            return Err(MortonError::AxisOutOfRange {
                axis: 'x',
                value: x,
            });
        }
        if y > MORTON_AXIS_MAX {
            return Err(MortonError::AxisOutOfRange {
                axis: 'y',
                value: y,
            });
        }
        if z > MORTON_AXIS_MAX {
            return Err(MortonError::AxisOutOfRange {
                axis: 'z',
                value: z,
            });
        }
        let xs = morton_split_by_3(x);
        let ys = morton_split_by_3(y) << 1;
        let zs = morton_split_by_3(z) << 2;
        Ok(MortonKey(xs | ys | zs))
    }

    /// Encode without bounds-checking ; clamps to [`MORTON_AXIS_MASK`]
    /// implicitly via the bit-spreader. The caller is responsible for
    /// ensuring axis indices are in range when correctness depends on
    /// uniqueness ; out-of-range values silently fold (collide) into the
    /// 21-bit space.
    #[inline]
    #[must_use]
    pub const fn encode_clamped(x: u64, y: u64, z: u64) -> MortonKey {
        let xs = morton_split_by_3(x);
        let ys = morton_split_by_3(y) << 1;
        let zs = morton_split_by_3(z) << 2;
        MortonKey(xs | ys | zs)
    }

    /// Decode a key back to (x, y, z) axis indices.
    #[must_use]
    pub const fn decode(self) -> (u64, u64, u64) {
        let raw = self.0;
        let x = morton_compact_by_3(raw);
        let y = morton_compact_by_3(raw >> 1);
        let z = morton_compact_by_3(raw >> 2);
        (x, y, z)
    }

    /// Raw u64 representation (for pack/unpack to GPU buffers).
    #[inline]
    #[must_use]
    pub const fn to_u64(self) -> u64 {
        self.0
    }

    /// Construct from a raw u64 — used by the open-addressing table when
    /// reading back stored keys.
    #[inline]
    #[must_use]
    pub const fn from_u64_raw(v: u64) -> MortonKey {
        MortonKey(v)
    }

    /// True iff this key is the sentinel/empty marker.
    #[inline]
    #[must_use]
    pub const fn is_sentinel(self) -> bool {
        (self.0 & MORTON_SENTINEL_BIT) != 0
    }

    /// True iff this key is a "valid" Morton-encoded cell index (bit 63
    /// clear). All keys produced by [`Self::encode`] are valid.
    #[inline]
    #[must_use]
    pub const fn is_valid(self) -> bool {
        !self.is_sentinel()
    }

    /// Voxel-resolution tier for the cascade (1cm/4cm/16cm/64cm).
    /// The tier is encoded by the high 3 bits of the original (x, y, z)
    /// indices folded into the low 3 bits of the Morton key — at canonical
    /// substrate-instantiation we use the lowest three bits of (x mod 8) to
    /// distinguish tier T0..T3 implicitly.
    ///
    /// At this slice [`Self::tier`] is a heuristic helper based on the
    /// magnitude of the smallest axis : near-origin = T0, far = T3. It is
    /// REPLACED at substrate-instantiation by an explicit per-cell tier
    /// store. The heuristic is preserved here so the unit-test suite has a
    /// stable function to exercise.
    #[must_use]
    pub fn tier(self) -> CellTier {
        let (x, y, z) = self.decode();
        // L_inf distance from origin in axis-units.
        let d = x.max(y).max(z);
        if d < 4 {
            CellTier::T0Fovea
        } else if d < 16 {
            CellTier::T1Mid
        } else if d < 64 {
            CellTier::T2Distant
        } else {
            CellTier::T3Horizon
        }
    }

    /// Linear-probe step. The open-addressing table uses this to advance
    /// to the next slot on a hash-collision. The step is a fixed odd
    /// integer (33) to ensure the probe sequence visits every slot of any
    /// table size that is a power of two.
    #[inline]
    #[must_use]
    pub const fn linear_probe(self, step: u32) -> u64 {
        // We hash by mixing the low + high halves of the key with a
        // splittable-counter ; the result is reduced modulo the table size
        // by the consumer.
        let k = self.0 as u128;
        let h = k.wrapping_mul(0x9E37_79B9_7F4A_7C15) as u64;
        h.wrapping_add(33u64.wrapping_mul(step as u64))
    }
}

// ───────────────────────────────────────────────────────────────────────
// § CellTier — the 4-tier MERA cascade discriminant.
// ───────────────────────────────────────────────────────────────────────

/// Voxel-resolution tier per `Omniverse/04_OMEGA_FIELD/05_DENSITY_BUDGET §
/// I.cascade-tiers`. Maps to a MERA layer:
///
///   T0 → 1 cm  → MERA layer 0 (active fovea)
///   T1 → 4 cm  → MERA layer 1
///   T2 → 16 cm → MERA layer 2
///   T3 → 64 cm → MERA layer 3 (horizon summary)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum CellTier {
    /// T0 fovea : 1 cm cells, 0..4 m distance.
    T0Fovea = 0,
    /// T1 mid : 4 cm cells, 4..16 m.
    T1Mid = 1,
    /// T2 distant : 16 cm cells, 16..64 m.
    T2Distant = 2,
    /// T3 horizon : 64 cm cells, 64..256 m.
    T3Horizon = 3,
}

impl CellTier {
    /// Voxel size in meters.
    #[must_use]
    pub const fn voxel_size_m(self) -> f32 {
        match self {
            Self::T0Fovea => 0.01,
            Self::T1Mid => 0.04,
            Self::T2Distant => 0.16,
            Self::T3Horizon => 0.64,
        }
    }

    /// MERA pyramid layer index.
    #[must_use]
    pub const fn mera_layer(self) -> u8 {
        self as u8
    }

    /// Canonical name for telemetry / audit.
    #[must_use]
    pub const fn canonical_name(self) -> &'static str {
        match self {
            Self::T0Fovea => "t0_fovea_1cm",
            Self::T1Mid => "t1_mid_4cm",
            Self::T2Distant => "t2_distant_16cm",
            Self::T3Horizon => "t3_horizon_64cm",
        }
    }

    /// All tiers in canonical (coarse-to-fine reverse) order.
    #[must_use]
    pub const fn all() -> &'static [CellTier] {
        &[Self::T0Fovea, Self::T1Mid, Self::T2Distant, Self::T3Horizon]
    }
}

// ───────────────────────────────────────────────────────────────────────
// § MortonError — the failure modes of Morton-key encoding.
// ───────────────────────────────────────────────────────────────────────

/// Failure modes of [`MortonKey::encode`].
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum MortonError {
    /// One of the axes exceeded [`MORTON_AXIS_MAX`].
    #[error("OF0001 — Morton axis '{axis}' value {value} exceeds 21-bit max {MORTON_AXIS_MAX}")]
    AxisOutOfRange { axis: char, value: u64 },
}

#[cfg(test)]
mod tests {
    use super::{
        morton_compact_by_3, morton_split_by_3, CellTier, MortonError, MortonKey, MORTON_AXIS_MAX,
        MORTON_AXIS_WIDTH, MORTON_PAYLOAD_WIDTH, MORTON_SENTINEL_BIT,
    };

    // ── Bit-spreading roundtrip ─────────────────────────────────────

    #[test]
    fn split_compact_zero() {
        assert_eq!(morton_split_by_3(0), 0);
        assert_eq!(morton_compact_by_3(0), 0);
    }

    #[test]
    fn split_compact_one() {
        let s = morton_split_by_3(1);
        assert_eq!(s & 0x7, 0x1);
        assert_eq!(morton_compact_by_3(s), 1);
    }

    #[test]
    fn split_compact_max() {
        let s = morton_split_by_3(MORTON_AXIS_MAX);
        assert_eq!(morton_compact_by_3(s), MORTON_AXIS_MAX);
    }

    #[test]
    fn split_compact_roundtrip_arbitrary() {
        for v in [
            0_u64,
            1,
            7,
            42,
            1024,
            65535,
            MORTON_AXIS_MAX / 2,
            MORTON_AXIS_MAX,
        ] {
            assert_eq!(morton_compact_by_3(morton_split_by_3(v)), v);
        }
    }

    // ── MortonKey::encode / decode roundtrip ────────────────────────

    #[test]
    fn encode_zero_axes_yields_zero_key() {
        let k = MortonKey::encode(0, 0, 0).unwrap();
        assert_eq!(k.to_u64(), 0);
    }

    #[test]
    fn decode_zero_yields_origin() {
        let (x, y, z) = MortonKey::ZERO.decode();
        assert_eq!((x, y, z), (0, 0, 0));
    }

    #[test]
    fn encode_decode_roundtrip_unit() {
        let k = MortonKey::encode(1, 2, 3).unwrap();
        let (x, y, z) = k.decode();
        assert_eq!((x, y, z), (1, 2, 3));
    }

    #[test]
    fn encode_decode_roundtrip_max() {
        let k = MortonKey::encode(MORTON_AXIS_MAX, MORTON_AXIS_MAX, MORTON_AXIS_MAX).unwrap();
        let (x, y, z) = k.decode();
        assert_eq!(
            (x, y, z),
            (MORTON_AXIS_MAX, MORTON_AXIS_MAX, MORTON_AXIS_MAX)
        );
    }

    #[test]
    fn encode_decode_roundtrip_many() {
        // Sample a deterministic spread of (x, y, z) tuples across the
        // 21-bit range. We use a fixed-seed LCG so the test is replay-stable.
        let mut s: u64 = 0xC0FF_EE_BA_BE_42;
        for _ in 0..256 {
            s = s
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let x = (s >> 4) & MORTON_AXIS_MAX;
            s = s
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let y = (s >> 4) & MORTON_AXIS_MAX;
            s = s
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let z = (s >> 4) & MORTON_AXIS_MAX;
            let k = MortonKey::encode(x, y, z).unwrap();
            assert_eq!(k.decode(), (x, y, z));
            assert!(k.is_valid());
        }
    }

    #[test]
    fn encode_x_out_of_range_rejected() {
        assert_eq!(
            MortonKey::encode(MORTON_AXIS_MAX + 1, 0, 0),
            Err(MortonError::AxisOutOfRange {
                axis: 'x',
                value: MORTON_AXIS_MAX + 1
            })
        );
    }

    #[test]
    fn encode_y_out_of_range_rejected() {
        assert!(matches!(
            MortonKey::encode(0, u64::MAX, 0),
            Err(MortonError::AxisOutOfRange { axis: 'y', .. })
        ));
    }

    #[test]
    fn encode_z_out_of_range_rejected() {
        assert!(matches!(
            MortonKey::encode(0, 0, u64::MAX),
            Err(MortonError::AxisOutOfRange { axis: 'z', .. })
        ));
    }

    // ── Sentinel + validity ─────────────────────────────────────────

    #[test]
    fn sentinel_is_distinct_from_zero() {
        assert!(MortonKey::SENTINEL.is_sentinel());
        assert!(!MortonKey::ZERO.is_sentinel());
        assert_ne!(MortonKey::SENTINEL, MortonKey::ZERO);
    }

    #[test]
    fn encoded_keys_never_set_sentinel_bit() {
        let k = MortonKey::encode(MORTON_AXIS_MAX, MORTON_AXIS_MAX, MORTON_AXIS_MAX).unwrap();
        assert!(k.is_valid());
        assert_eq!(k.to_u64() & MORTON_SENTINEL_BIT, 0);
    }

    // ── Determinism (replay) ────────────────────────────────────────

    #[test]
    fn determinism_byte_for_byte_roundtrip() {
        // For replay-stability we record byte-equal keys for fixed inputs.
        // If this test ever fails after a refactor, the bit-spread
        // magic-numbers were changed and the on-disk save format would
        // de-sync.
        assert_eq!(MortonKey::encode(1, 0, 0).unwrap().to_u64(), 0x1);
        assert_eq!(MortonKey::encode(0, 1, 0).unwrap().to_u64(), 0x2);
        assert_eq!(MortonKey::encode(0, 0, 1).unwrap().to_u64(), 0x4);
        // (1, 1, 1) → bits 0, 1, 2 set = 0b111 = 0x7
        assert_eq!(MortonKey::encode(1, 1, 1).unwrap().to_u64(), 0x7);
        // (2, 0, 0) → bit 3 set
        assert_eq!(MortonKey::encode(2, 0, 0).unwrap().to_u64(), 0x8);
        // (3, 0, 0) → bits 0 + 3 set
        assert_eq!(MortonKey::encode(3, 0, 0).unwrap().to_u64(), 0x9);
    }

    // ── Tier discrimination ─────────────────────────────────────────

    #[test]
    fn tier_origin_is_t0_fovea() {
        let k = MortonKey::encode(0, 0, 0).unwrap();
        assert_eq!(k.tier(), CellTier::T0Fovea);
    }

    #[test]
    fn tier_progression_t0_to_t3() {
        for &(d, expected) in &[
            (0_u64, CellTier::T0Fovea),
            (3, CellTier::T0Fovea),
            (4, CellTier::T1Mid),
            (15, CellTier::T1Mid),
            (16, CellTier::T2Distant),
            (63, CellTier::T2Distant),
            (64, CellTier::T3Horizon),
            (1024, CellTier::T3Horizon),
        ] {
            let k = MortonKey::encode(d, 0, 0).unwrap();
            assert_eq!(k.tier(), expected, "axis {d}");
        }
    }

    #[test]
    fn cell_tier_voxel_sizes() {
        assert!((CellTier::T0Fovea.voxel_size_m() - 0.01).abs() < 1e-6);
        assert!((CellTier::T1Mid.voxel_size_m() - 0.04).abs() < 1e-6);
        assert!((CellTier::T2Distant.voxel_size_m() - 0.16).abs() < 1e-6);
        assert!((CellTier::T3Horizon.voxel_size_m() - 0.64).abs() < 1e-6);
    }

    #[test]
    fn cell_tier_mera_layers_match_index() {
        assert_eq!(CellTier::T0Fovea.mera_layer(), 0);
        assert_eq!(CellTier::T1Mid.mera_layer(), 1);
        assert_eq!(CellTier::T2Distant.mera_layer(), 2);
        assert_eq!(CellTier::T3Horizon.mera_layer(), 3);
    }

    #[test]
    fn cell_tier_canonical_names_unique() {
        let mut names: Vec<&'static str> =
            CellTier::all().iter().map(|t| t.canonical_name()).collect();
        names.sort_unstable();
        let original = names.len();
        names.dedup();
        assert_eq!(names.len(), original);
    }

    // ── Constants sanity ────────────────────────────────────────────

    #[test]
    fn morton_axis_width_is_21() {
        assert_eq!(MORTON_AXIS_WIDTH, 21);
    }

    #[test]
    fn morton_payload_width_is_63() {
        assert_eq!(MORTON_PAYLOAD_WIDTH, 63);
    }

    #[test]
    fn morton_axis_max_matches_mask() {
        assert_eq!(MORTON_AXIS_MAX, (1u64 << 21) - 1);
    }

    // ── Linear-probe progression ────────────────────────────────────

    #[test]
    fn linear_probe_advances_distinctly() {
        let k = MortonKey::encode(7, 8, 9).unwrap();
        let p0 = k.linear_probe(0);
        let p1 = k.linear_probe(1);
        let p2 = k.linear_probe(2);
        assert_ne!(p0, p1);
        assert_ne!(p1, p2);
        // Step distance is fixed (33).
        assert_eq!(p1.wrapping_sub(p0), 33);
        assert_eq!(p2.wrapping_sub(p1), 33);
    }
}
