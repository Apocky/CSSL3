// § T11-WAVE3-PROCGEN-ROOMS : CalibrationGrid recipe
// ══════════════════════════════════════════════════════════════════
//! Grid-aligned reference tiles + 4 NSEW doorways at midpoints.
//!
//! § Use-case : VR scale-validation room. Solid floor + ceiling + walls,
//! with a single bright zenith light, no decorative variation.

use crate::constraints::RoomConstraints;
use crate::recipe::{
    Doorway, LightHint, RoomDims, RoomKind, RoomRecipe, TileLayer, TilePlacement, WallSide,
};
use crate::rng::Pcg32;

#[must_use]
pub fn generate(seed: u64, dims: RoomDims, _c: &RoomConstraints) -> RoomRecipe {
    let mut rng = Pcg32::new(seed.wrapping_mul(0xA1) ^ 0x_C0DE);
    let mut r = RoomRecipe::empty(RoomKind::CalibrationGrid, dims, seed);

    let nx = r.tile_count_x();
    let nz = r.tile_count_z();

    // Floor + ceiling : checker-pattern (alt 0/1) for visual scale ref.
    for x in 0..nx {
        for z in 0..nz {
            let parity = ((x + z) & 1) as u8;
            r.tiles.push(TilePlacement {
                x, z,
                layer: TileLayer::Floor,
                material_idx: parity,
                pattern_idx: 0,
            });
            r.tiles.push(TilePlacement {
                x, z,
                layer: TileLayer::Ceiling,
                material_idx: 2,
                pattern_idx: 0,
            });
        }
    }

    // Walls : single material, full coverage on all 4 sides.
    for x in 0..nx {
        r.tiles.push(TilePlacement { x, z: 0,      layer: TileLayer::Wall(WallSide::S), material_idx: 3, pattern_idx: 0 });
        r.tiles.push(TilePlacement { x, z: nz - 1, layer: TileLayer::Wall(WallSide::N), material_idx: 3, pattern_idx: 0 });
    }
    for z in 0..nz {
        r.tiles.push(TilePlacement { x: 0,      z, layer: TileLayer::Wall(WallSide::W), material_idx: 3, pattern_idx: 0 });
        r.tiles.push(TilePlacement { x: nx - 1, z, layer: TileLayer::Wall(WallSide::E), material_idx: 3, pattern_idx: 0 });
    }

    // 4 NSEW doorways at side midpoints.
    let door_w = 1.0;
    let door_h = 2.1;
    r.doorways.push(Doorway { side: WallSide::N, position_m: dims.width_m  * 0.5, width_m: door_w, height_m: door_h });
    r.doorways.push(Doorway { side: WallSide::S, position_m: dims.width_m  * 0.5, width_m: door_w, height_m: door_h });
    r.doorways.push(Doorway { side: WallSide::E, position_m: dims.length_m * 0.5, width_m: door_w, height_m: door_h });
    r.doorways.push(Doorway { side: WallSide::W, position_m: dims.length_m * 0.5, width_m: door_w, height_m: door_h });

    // Single zenith light, slight rng wiggle in intensity.
    let intensity = 80.0 + rng.next_f32() * 20.0;
    r.lights.push(LightHint {
        pos_m:     [dims.width_m * 0.5, dims.height_m * 0.95, dims.length_m * 0.5],
        color:     [1.0, 1.0, 1.0],
        intensity,
    });

    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calibration_determinism() {
        let dims = RoomDims::default();
        let c = RoomConstraints::default();
        let a = generate(42, dims, &c);
        let b = generate(42, dims, &c);
        assert_eq!(a, b);
    }

    #[test]
    fn calibration_doorway_count() {
        let r = generate(0, RoomDims::default(), &RoomConstraints::default());
        assert_eq!(r.doorways.len(), 4, "calibration should have 4 NSEW doorways");
    }

    #[test]
    fn calibration_tile_density_in_bounds() {
        let r = generate(0, RoomDims::default(), &RoomConstraints::default());
        // Floor + ceiling + 4 walls : at least 2 * nx * nz tiles.
        let nx = r.tile_count_x() as usize;
        let nz = r.tile_count_z() as usize;
        assert!(r.tiles.len() >= 2 * nx * nz);
        // Sanity upper bound : 2 surfaces + 4 walls.
        assert!(r.tiles.len() < 6 * nx * nz + 100);
    }
}
