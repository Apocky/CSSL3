//! § morton — 21-bit-per-axis Morton-key encoding for sparse Ω-field storage
//! ═══════════════════════════════════════════════════════════════════════
//!
//! Authoritative spec :
//!   `Omniverse/04_OMEGA_FIELD/02_STORAGE.csl.md` § II
//!   `Omniverse/02_CSSL/06_SUBSTRATE_EVOLUTION.csl` § IV.2
//!
//! § ROLE
//!   Convert 3-D voxel-coordinates (i_x, i_y, i_z) ∈ [0, 2²¹)³ into a
//!   single 64-bit Morton-Z-curve key + back. The curve is the
//!   space-filling permutation of {0,1}³² that interleaves the bits of
//!   the three axes ; consecutive cells along any axis differ by O(log
//!   bound) bits, which keeps the sparse-hash-grid cache-line-coherent
//!   under raster-scan traversal.
//!
//! § DETERMINISM CONTRACT (T11-D113 LANDMINE)
//!   The encoding MUST be bit-identical across hosts. The implementation
//!   uses the canonical "magic-mask spread" algorithm (`bit_spread_3` /
//!   `bit_compact_3`) with all constants spelled out — there is no
//!   `intrinsics::pdep` fast path because PDEP/PEXT availability differs
//!   by CPU and would break replay-determinism (Axiom-3 §V).
//!
//! § COORDINATE → KEY
//!   `morton3_encode(ix, iy, iz) = spread(ix) | spread(iy)<<1 | spread(iz)<<2`
//!   where `spread(b)` interleaves zeros between every bit of `b`.
//!   Each axis carries 21 significant bits ⇒ 3·21 = 63 ≤ 64 ; the high
//!   bit is always zero (acts as a sanity-check tag for `morton_valid`).
//!
//! § PHYSICAL BOUNDS
//!   At canonical-cell-size 0.125 m (Axiom-3 §V), 2²¹ cells per axis ⇒
//!   2²¹ · 0.125 m ≈ 262 km per axis ; 16 EB·m³ implicit-volume.
//!   At fovea-tier 0.01 m (T0), 2²¹ ⇒ 20.97 km ; comfortably exceeds
//!   any realistic cross-section M7-budget needs.
//!
//! § NEIGHBOR ITERATION
//!   `morton_neighbors_27` enumerates the 3×3×3 Moore neighborhood
//!   (excluding self when `include_center == false`). Result-order is
//!   deterministic : (dz, dy, dx) lexicographic with d ∈ {-1, 0, +1}.
//!
//! § VALIDATION
//!   `morton_valid(k)` : k & MORTON_HIGH_BIT_MASK == 0 (i.e. bit-63 is
//!   zero). Any other bit-pattern is rejected as a sentinel.
//!
//! § ATTESTATION
//!   PRIME_DIRECTIVE.md § 11 attestation : "There was no hurt nor harm
//!   in the making of this, to anyone / anything / anybody."

// 21 bits per axis ⇒ 2²¹ cells per axis.
/// Maximum coordinate component value (exclusive). Equals `1 << 21 = 2_097_152`.
pub const MORTON_AXIS_BITS: u32 = 21;

/// Maximum coordinate component (exclusive).
pub const MORTON_AXIS_MAX: u64 = 1 << MORTON_AXIS_BITS;

/// Mask of bit-63 — must be zero on every valid Morton key.
pub const MORTON_HIGH_BIT_MASK: u64 = 1 << 63;

/// Spread the low 21 bits of `v` into bit-positions 0, 3, 6, …, 60.
///
/// `bit_spread_3(0bABCDE) = 0b...A0_0B0_0C0_0D0_0E` (interleaved with
/// two-zero gaps). The result has no bits set above position 60.
///
/// Algorithm : "magic-mask" parallel bit-deal (Hacker's Delight + GPU-
/// gems variant), unrolled for u64. Constants spelled to keep
/// determinism reviewable.
#[must_use]
#[inline]
pub const fn bit_spread_3(v: u64) -> u64 {
    let mut x = v & 0x0000_0000_001F_FFFF; // 21 bits
    x = (x | (x << 32)) & 0x001F_0000_0000_FFFF;
    x = (x | (x << 16)) & 0x001F_0000_FF00_00FF;
    x = (x | (x << 8)) & 0x100F_00F0_0F00_F00F;
    x = (x | (x << 4)) & 0x10C3_0C30_C30C_30C3;
    x = (x | (x << 2)) & 0x1249_2492_4924_9249;
    x
}

/// Inverse of [`bit_spread_3`]. Compacts every third bit into the low
/// 21 bits ; the upper 43 bits of the result are always zero.
///
/// `bit_compact_3(spread(v)) == v` for any `v` with `v < 2²¹`.
#[must_use]
#[inline]
pub const fn bit_compact_3(v: u64) -> u64 {
    let mut x = v & 0x1249_2492_4924_9249;
    x = (x | (x >> 2)) & 0x10C3_0C30_C30C_30C3;
    x = (x | (x >> 4)) & 0x100F_00F0_0F00_F00F;
    x = (x | (x >> 8)) & 0x001F_0000_FF00_00FF;
    x = (x | (x >> 16)) & 0x001F_0000_0000_FFFF;
    x = (x | (x >> 32)) & 0x0000_0000_001F_FFFF;
    x
}

/// Encode a 3-D voxel-coordinate into a Morton-key.
///
/// Coordinates are clamped (saturated) at `MORTON_AXIS_MAX - 1` ; this
/// mirrors a sparse-hash grid that drops out-of-implicit-bounds inputs
/// rather than panicking on boundary-overflow.
#[must_use]
#[inline]
pub const fn morton3_encode(ix: u64, iy: u64, iz: u64) -> u64 {
    // Saturate. Out-of-range coords get folded to the extremum cell ;
    // at the SparseMortonGrid layer the analytic-SDF fallback handles
    // those cells deterministically (Axiom-5 base-distribution).
    let cx = if ix >= MORTON_AXIS_MAX {
        MORTON_AXIS_MAX - 1
    } else {
        ix
    };
    let cy = if iy >= MORTON_AXIS_MAX {
        MORTON_AXIS_MAX - 1
    } else {
        iy
    };
    let cz = if iz >= MORTON_AXIS_MAX {
        MORTON_AXIS_MAX - 1
    } else {
        iz
    };
    bit_spread_3(cx) | (bit_spread_3(cy) << 1) | (bit_spread_3(cz) << 2)
}

/// Decode a Morton-key back into its 3-D voxel-coordinate.
#[must_use]
#[inline]
pub const fn morton3_decode(k: u64) -> (u64, u64, u64) {
    let ix = bit_compact_3(k);
    let iy = bit_compact_3(k >> 1);
    let iz = bit_compact_3(k >> 2);
    (ix, iy, iz)
}

/// True iff `k` is a syntactically-valid Morton-key (bit-63 zero).
#[must_use]
#[inline]
pub const fn morton_valid(k: u64) -> bool {
    (k & MORTON_HIGH_BIT_MASK) == 0
}

/// Morton-key newtype — `u64` with bit-63 as the validity sentinel.
///
/// Public construction is `MortonKey::new(ix, iy, iz)` ; the raw u64
/// constructor is intentionally `pub(crate)` so callers can't smuggle
/// bit-63-set values into a SparseMortonGrid.
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MortonKey(pub(crate) u64);

impl MortonKey {
    /// Encode 3-D coordinates into a key.
    #[must_use]
    pub const fn new(ix: u64, iy: u64, iz: u64) -> Self {
        Self(morton3_encode(ix, iy, iz))
    }

    /// Wrap a raw u64 as a key. Returns `None` if bit-63 is set.
    #[must_use]
    pub const fn from_raw(raw: u64) -> Option<Self> {
        if morton_valid(raw) {
            Some(Self(raw))
        } else {
            None
        }
    }

    /// Raw 64-bit representation.
    #[must_use]
    pub const fn raw(&self) -> u64 {
        self.0
    }

    /// Decode to 3-D coordinates.
    #[must_use]
    pub const fn coords(&self) -> (u64, u64, u64) {
        morton3_decode(self.0)
    }

    /// Sentinel "never-occupied" key. Bit-63 set ⇒ never collides with a
    /// real key (which has bit-63 zero by construction).
    #[must_use]
    pub const fn tombstone() -> u64 {
        u64::MAX
    }
}

impl core::fmt::Debug for MortonKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let (x, y, z) = self.coords();
        f.debug_struct("MortonKey")
            .field("raw", &format_args!("0x{:016X}", self.0))
            .field("ix", &x)
            .field("iy", &y)
            .field("iz", &z)
            .finish()
    }
}

/// Enumerate the 3×3×3 (or 3³−1 = 26 if `include_center == false`)
/// Moore-neighborhood of `k`. Out-of-bounds neighbors (any axis < 0 or
/// ≥ 2²¹) are skipped so the returned vec has length ≤ 27.
///
/// Iteration-order is deterministic : `dz ∈ {-1, 0, +1}` outermost,
/// then `dy`, then `dx` ; replay-stable.
#[must_use]
pub fn morton_neighbors_27(k: MortonKey, include_center: bool) -> smallvec::SmallVec<[u64; 27]> {
    let mut out = smallvec::SmallVec::<[u64; 27]>::new();
    let (cx, cy, cz) = k.coords();
    for dz in -1_i64..=1 {
        for dy in -1_i64..=1 {
            for dx in -1_i64..=1 {
                if !include_center && dx == 0 && dy == 0 && dz == 0 {
                    continue;
                }
                let nx = cx as i64 + dx;
                let ny = cy as i64 + dy;
                let nz = cz as i64 + dz;
                if nx < 0
                    || ny < 0
                    || nz < 0
                    || nx >= MORTON_AXIS_MAX as i64
                    || ny >= MORTON_AXIS_MAX as i64
                    || nz >= MORTON_AXIS_MAX as i64
                {
                    continue;
                }
                out.push(morton3_encode(nx as u64, ny as u64, nz as u64));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn morton_origin_is_zero() {
        assert_eq!(morton3_encode(0, 0, 0), 0);
    }

    #[test]
    fn morton_round_trip_small_coords() {
        for x in 0..16u64 {
            for y in 0..16u64 {
                for z in 0..16u64 {
                    let k = morton3_encode(x, y, z);
                    let (rx, ry, rz) = morton3_decode(k);
                    assert_eq!((rx, ry, rz), (x, y, z), "round-trip failed for {x},{y},{z}");
                }
            }
        }
    }

    #[test]
    fn morton_round_trip_extremum() {
        let max = MORTON_AXIS_MAX - 1;
        let k = morton3_encode(max, max, max);
        assert_eq!(morton3_decode(k), (max, max, max));
        // bit-63 must remain zero for all valid coords.
        assert!(morton_valid(k));
    }

    #[test]
    fn morton_independence_axes() {
        // Pure-X: only bits 0, 3, 6, ...
        let kx = morton3_encode(0b1111, 0, 0);
        // Pure-Y: only bits 1, 4, 7, ...
        let ky = morton3_encode(0, 0b1111, 0);
        // Pure-Z: only bits 2, 5, 8, ...
        let kz = morton3_encode(0, 0, 0b1111);
        // No overlap among axes.
        assert_eq!(kx & ky, 0);
        assert_eq!(kx & kz, 0);
        assert_eq!(ky & kz, 0);
        assert_eq!(
            kx | ky | kz,
            morton3_encode(0b1111, 0b1111, 0b1111),
            "OR of axis-keys equals the joint-key"
        );
    }

    #[test]
    fn morton_valid_rejects_high_bit() {
        assert!(morton_valid(0));
        assert!(morton_valid(0x7FFF_FFFF_FFFF_FFFF));
        assert!(!morton_valid(0x8000_0000_0000_0000));
        assert!(!morton_valid(u64::MAX));
    }

    #[test]
    fn morton_key_newtype_basic() {
        let k = MortonKey::new(7, 13, 21);
        assert_eq!(k.coords(), (7, 13, 21));
        let raw = k.raw();
        assert!(morton_valid(raw));
        let k2 = MortonKey::from_raw(raw).expect("valid raw → key");
        assert_eq!(k, k2);
        assert!(MortonKey::from_raw(MortonKey::tombstone()).is_none());
    }

    #[test]
    fn morton_neighbors_full_interior() {
        let k = MortonKey::new(100, 100, 100);
        let neighbors = morton_neighbors_27(k, false);
        assert_eq!(neighbors.len(), 26);
        // Center NOT included.
        assert!(!neighbors.iter().any(|&n| n == k.raw()));
        // Including center : 27.
        let neighbors_with_self = morton_neighbors_27(k, true);
        assert_eq!(neighbors_with_self.len(), 27);
    }

    #[test]
    fn morton_neighbors_origin_skips_negatives() {
        let k = MortonKey::new(0, 0, 0);
        let neighbors = morton_neighbors_27(k, false);
        // At origin: only +1-side neighbors valid ; positive octant has
        // 8 cells minus self = 7 ; plus the +x/+y/+z faces = 7 + 6
        // (faces) wait, let me just check length is correct (3³-1 minus
        // the negative ones).
        // dx,dy,dz each ∈ {0,+1}, exclude (0,0,0) ⇒ 2³-1 = 7.
        assert_eq!(neighbors.len(), 7);
        for &n in neighbors.iter() {
            assert!(morton_valid(n));
        }
    }

    #[test]
    fn morton_saturates_oob_coords() {
        let oob = MORTON_AXIS_MAX + 100;
        let k = morton3_encode(oob, oob, oob);
        let max = MORTON_AXIS_MAX - 1;
        assert_eq!(morton3_decode(k), (max, max, max));
    }

    #[test]
    fn morton_neighbors_corner_max_skips_oob() {
        let k = MortonKey::new(MORTON_AXIS_MAX - 1, MORTON_AXIS_MAX - 1, MORTON_AXIS_MAX - 1);
        let neighbors = morton_neighbors_27(k, false);
        // At max-corner: only -1-side neighbors valid ⇒ 2³-1 = 7.
        assert_eq!(neighbors.len(), 7);
    }

    #[test]
    fn morton_determinism_replay() {
        // Same inputs, two separate machines (simulated by two separate
        // calls) must yield bit-equal keys. Crucial for replay-determinism.
        let inputs: [(u64, u64, u64); 5] = [
            (0, 0, 0),
            (1, 2, 3),
            (1023, 1024, 1025),
            (MORTON_AXIS_MAX - 1, 0, 0),
            (123_456, 789_012, 1_234_567 % MORTON_AXIS_MAX),
        ];
        for (x, y, z) in inputs {
            let k1 = morton3_encode(x, y, z);
            let k2 = morton3_encode(x, y, z);
            assert_eq!(k1, k2, "encode is not deterministic");
            // Round-trip
            assert_eq!(morton3_decode(k1), (x, y, z));
        }
    }
}
