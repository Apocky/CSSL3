// § T11-WAVE3-PROCGEN-ROOMS : NoiseField recipe
// ══════════════════════════════════════════════════════════════════
//! Value-noise indexed material assignment.
//!
//! § Use-case : organic-pattern showcase. Each floor tile samples a
//! deterministic value-noise function (hash of (x,z,seed)) and bins to
//! material_idx 0..7. Walls + ceiling solid.

use crate::constraints::RoomConstraints;
use crate::recipe::{
    Doorway, LightHint, RoomDims, RoomKind, RoomRecipe, TileLayer, TilePlacement, WallSide,
};
use crate::rng::Pcg32;

const BINS: u8 = 8;

#[must_use]
pub fn generate(seed: u64, dims: RoomDims, _c: &RoomConstraints) -> RoomRecipe {
    let mut rng = Pcg32::new(seed.wrapping_mul(0xF1) ^ 0x_F0F0_FA11);
    let mut r = RoomRecipe::empty(RoomKind::NoiseField, dims, seed);

    let nx = r.tile_count_x();
    let nz = r.tile_count_z();

    // Generate a 2D value-noise grid via PCG hash of (x,z).
    // Each cell is independently determined by seed+(x,z).
    for x in 0..nx {
        for z in 0..nz {
            let v = hash_2d(seed, x, z);
            let bin = ((v as f32 / u32::MAX as f32) * f32::from(BINS)).floor() as u8 % BINS;
            r.tiles.push(TilePlacement {
                x, z,
                layer: TileLayer::Floor,
                material_idx: bin,
                pattern_idx: 0,
            });
        }
    }

    // Solid ceiling.
    for x in 0..nx {
        for z in 0..nz {
            r.tiles.push(TilePlacement { x, z, layer: TileLayer::Ceiling, material_idx: 2, pattern_idx: 0 });
        }
    }

    // Walls.
    for x in 0..nx {
        r.tiles.push(TilePlacement { x, z: 0,      layer: TileLayer::Wall(WallSide::S), material_idx: 3, pattern_idx: 0 });
        r.tiles.push(TilePlacement { x, z: nz - 1, layer: TileLayer::Wall(WallSide::N), material_idx: 3, pattern_idx: 0 });
    }
    for z in 0..nz {
        r.tiles.push(TilePlacement { x: 0,      z, layer: TileLayer::Wall(WallSide::W), material_idx: 3, pattern_idx: 0 });
        r.tiles.push(TilePlacement { x: nx - 1, z, layer: TileLayer::Wall(WallSide::E), material_idx: 3, pattern_idx: 0 });
    }

    // 3 doorways : N + S + E (asymmetric).
    r.doorways.push(Doorway { side: WallSide::N, position_m: dims.width_m  * 0.5, width_m: 1.0, height_m: 2.1 });
    r.doorways.push(Doorway { side: WallSide::S, position_m: dims.width_m  * 0.5, width_m: 1.0, height_m: 2.1 });
    r.doorways.push(Doorway { side: WallSide::E, position_m: dims.length_m * 0.5, width_m: 1.0, height_m: 2.1 });

    // 5 lights at random positions (rng draws make placement seed-dependent).
    for _ in 0..5 {
        let lx = rng.next_f32() * dims.width_m;
        let lz = rng.next_f32() * dims.length_m;
        let h  = 0.4 + rng.next_f32() * 0.5;
        r.lights.push(LightHint {
            pos_m:     [lx, dims.height_m * h, lz],
            color:     [0.9 + rng.next_f32() * 0.1, 0.9 + rng.next_f32() * 0.1, 0.9 + rng.next_f32() * 0.1],
            intensity: 20.0 + rng.next_f32() * 30.0,
        });
    }

    r
}

/// § 2D-hash → u32 ; pure function, deterministic across rustc.
fn hash_2d(seed: u64, x: u16, z: u16) -> u32 {
    let mut s = seed;
    s ^= u64::from(x).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    s ^= u64::from(z).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    s = s.wrapping_mul(0x94D0_49BB_1331_11EB);
    ((s >> 32) ^ s) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noise_field_determinism() {
        let dims = RoomDims::default();
        let c = RoomConstraints::default();
        assert_eq!(generate(19, dims, &c), generate(19, dims, &c));
    }

    #[test]
    fn noise_field_doorway_count() {
        let r = generate(0, RoomDims::default(), &RoomConstraints::default());
        assert_eq!(r.doorways.len(), 3);
    }

    #[test]
    fn noise_field_uses_multiple_bins() {
        let r = generate(0, RoomDims::default(), &RoomConstraints::default());
        let mut bins = std::collections::HashSet::new();
        for t in &r.tiles {
            if matches!(t.layer, TileLayer::Floor) {
                bins.insert(t.material_idx);
            }
        }
        assert!(bins.len() >= 4, "noise field should fill ≥4 bins, got {}", bins.len());
        for b in &bins {
            assert!(*b < BINS, "bin {b} out of [0,{BINS})");
        }
    }
}
