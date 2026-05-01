// § T11-WAVE3-PROCGEN-ROOMS : MaterialShowcase recipe
// ══════════════════════════════════════════════════════════════════
//! 4×4 = 16 floor patches with cycling material indices.
//!
//! § Use-case : material-library reference room. Each floor patch is a
//! square block of tiles with a distinct material_idx 0..16. Walls solid,
//! ceiling solid, single ambient + 4 corner lights.

use crate::constraints::RoomConstraints;
use crate::recipe::{
    Doorway, LightHint, RoomDims, RoomKind, RoomRecipe, TileLayer, TilePlacement, WallSide,
};
use crate::rng::Pcg32;

#[must_use]
pub fn generate(seed: u64, dims: RoomDims, _c: &RoomConstraints) -> RoomRecipe {
    let mut rng = Pcg32::new(seed.wrapping_mul(0xB1) ^ 0x_BAD_F00D);
    let mut r = RoomRecipe::empty(RoomKind::MaterialShowcase, dims, seed);

    let nx = r.tile_count_x();
    let nz = r.tile_count_z();
    // 4×4 patches.
    let patch_x = nx / 4;
    let patch_z = nz / 4;

    for px in 0..4u16 {
        for pz in 0..4u16 {
            let mat = (px * 4 + pz) as u8;
            let pattern = (rng.next_u32() & 0x07) as u8;
            for ix in 0..patch_x {
                for iz in 0..patch_z {
                    r.tiles.push(TilePlacement {
                        x: px * patch_x + ix,
                        z: pz * patch_z + iz,
                        layer: TileLayer::Floor,
                        material_idx: mat,
                        pattern_idx: pattern,
                    });
                }
            }
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

    // 2 doorways : N + S.
    r.doorways.push(Doorway { side: WallSide::N, position_m: dims.width_m * 0.5, width_m: 1.2, height_m: 2.1 });
    r.doorways.push(Doorway { side: WallSide::S, position_m: dims.width_m * 0.5, width_m: 1.2, height_m: 2.1 });

    // 4 corner lights + 1 ambient zenith.
    for (ax, az) in [(0.15, 0.15), (0.85, 0.15), (0.15, 0.85), (0.85, 0.85)] {
        r.lights.push(LightHint {
            pos_m:     [dims.width_m * ax, dims.height_m * 0.9, dims.length_m * az],
            color:     [1.0, 0.95, 0.9],
            intensity: 40.0,
        });
    }
    r.lights.push(LightHint {
        pos_m:     [dims.width_m * 0.5, dims.height_m * 0.99, dims.length_m * 0.5],
        color:     [1.0, 1.0, 1.0],
        intensity: 30.0,
    });

    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn material_showcase_determinism() {
        let dims = RoomDims::default();
        let c = RoomConstraints::default();
        assert_eq!(generate(7, dims, &c), generate(7, dims, &c));
    }

    #[test]
    fn material_showcase_has_16_distinct_materials_on_floor() {
        let r = generate(0, RoomDims::default(), &RoomConstraints::default());
        let mut mats = std::collections::HashSet::new();
        for t in &r.tiles {
            if matches!(t.layer, TileLayer::Floor) {
                mats.insert(t.material_idx);
            }
        }
        assert_eq!(mats.len(), 16, "should have 16 distinct floor-material patches, got {}", mats.len());
    }

    #[test]
    fn material_showcase_doorway_and_light_counts() {
        let r = generate(0, RoomDims::default(), &RoomConstraints::default());
        assert_eq!(r.doorways.len(), 2);
        assert_eq!(r.lights.len(), 5);
    }
}
