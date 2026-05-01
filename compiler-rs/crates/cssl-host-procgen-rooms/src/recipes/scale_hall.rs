// § T11-WAVE3-PROCGEN-ROOMS : ScaleHall recipe
// ══════════════════════════════════════════════════════════════════
//! Long corridor with receding-tile-size effect.
//!
//! § Use-case : depth-perception calibration. Corridor length aligned to
//! z-axis, walls converge subtly via per-row pattern_idx that the renderer
//! can interpret as scale-bias. Two doorways at narrow ends only.

use crate::constraints::RoomConstraints;
use crate::recipe::{
    Doorway, LightHint, RoomDims, RoomKind, RoomRecipe, TileLayer, TilePlacement, WallSide,
};
use crate::rng::Pcg32;

#[must_use]
pub fn generate(seed: u64, dims: RoomDims, _c: &RoomConstraints) -> RoomRecipe {
    let mut rng = Pcg32::new(seed.wrapping_mul(0xC1) ^ 0x_5CA1_E000);
    let mut r = RoomRecipe::empty(RoomKind::ScaleHall, dims, seed);

    let nx = r.tile_count_x();
    let nz = r.tile_count_z();

    // Floor : per-row pattern_idx scaled by z-position (0..nz).
    for x in 0..nx {
        for z in 0..nz {
            // Quadratic recede so back-of-corridor patterns shrink faster.
            let recede = ((z as u32 * z as u32) / nz.max(1) as u32) as u8;
            let pattern = recede.min(15);
            r.tiles.push(TilePlacement {
                x, z,
                layer: TileLayer::Floor,
                material_idx: 1,
                pattern_idx: pattern,
            });
        }
    }

    // Ceiling : flat with a single material.
    for x in 0..nx {
        for z in 0..nz {
            r.tiles.push(TilePlacement { x, z, layer: TileLayer::Ceiling, material_idx: 2, pattern_idx: 0 });
        }
    }

    // Long-side walls (E + W) : pattern_idx mirrors floor recede.
    for z in 0..nz {
        let recede = ((z as u32 * z as u32) / nz.max(1) as u32) as u8;
        let pattern = recede.min(15);
        r.tiles.push(TilePlacement { x: 0,      z, layer: TileLayer::Wall(WallSide::W), material_idx: 4, pattern_idx: pattern });
        r.tiles.push(TilePlacement { x: nx - 1, z, layer: TileLayer::Wall(WallSide::E), material_idx: 4, pattern_idx: pattern });
    }
    // End walls (N + S).
    for x in 0..nx {
        r.tiles.push(TilePlacement { x, z: 0,      layer: TileLayer::Wall(WallSide::S), material_idx: 5, pattern_idx: 0 });
        r.tiles.push(TilePlacement { x, z: nz - 1, layer: TileLayer::Wall(WallSide::N), material_idx: 5, pattern_idx: 0 });
    }

    // 2 doorways at narrow ends (N + S).
    r.doorways.push(Doorway { side: WallSide::S, position_m: dims.width_m * 0.5, width_m: 1.0, height_m: 2.1 });
    r.doorways.push(Doorway { side: WallSide::N, position_m: dims.width_m * 0.5, width_m: 1.0, height_m: 2.1 });

    // Strip lights along the centreline at quarter intervals.
    for k in 1..=3 {
        let z_pos = dims.length_m * (k as f32 / 4.0);
        // Slight rng-driven flicker in intensity.
        let flicker = 0.8 + rng.next_f32() * 0.4;
        r.lights.push(LightHint {
            pos_m:     [dims.width_m * 0.5, dims.height_m * 0.92, z_pos],
            color:     [1.0, 0.97, 0.92],
            intensity: 50.0 * flicker,
        });
    }

    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_hall_determinism() {
        let dims = RoomDims { width_m: 4.0, length_m: 12.0, height_m: 3.0, tile_size_m: 0.5 };
        let c = RoomConstraints::default();
        assert_eq!(generate(11, dims, &c), generate(11, dims, &c));
    }

    #[test]
    fn scale_hall_doorway_count() {
        let r = generate(0, RoomDims { width_m: 4.0, length_m: 12.0, height_m: 3.0, tile_size_m: 0.5 }, &RoomConstraints::default());
        assert_eq!(r.doorways.len(), 2);
    }

    #[test]
    fn scale_hall_pattern_index_in_bounds() {
        let r = generate(0, RoomDims { width_m: 4.0, length_m: 12.0, height_m: 3.0, tile_size_m: 0.5 }, &RoomConstraints::default());
        for t in &r.tiles {
            assert!(t.pattern_idx <= 15, "pattern_idx {} out of [0,15]", t.pattern_idx);
        }
    }
}
