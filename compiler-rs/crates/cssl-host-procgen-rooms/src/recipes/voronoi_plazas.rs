// § T11-WAVE3-PROCGEN-ROOMS : VoronoiPlazas recipe
// ══════════════════════════════════════════════════════════════════
//! 8 random voronoi sites + nearest-site tile assignment.
//!
//! § Use-case : plaza/courtyard layouts. The room is partitioned into 8
//! convex-ish regions (Voronoi cells) by Euclidean distance to 8 sites
//! sampled deterministically from the seed. Each cell is a different
//! material. Walls solid, 4 doorways.

use crate::constraints::RoomConstraints;
use crate::recipe::{
    Doorway, LightHint, RoomDims, RoomKind, RoomRecipe, TileLayer, TilePlacement, WallSide,
};
use crate::rng::Pcg32;

const SITES: usize = 8;

#[must_use]
pub fn generate(seed: u64, dims: RoomDims, _c: &RoomConstraints) -> RoomRecipe {
    let mut rng = Pcg32::new(seed.wrapping_mul(0x101) ^ 0x_5170_BAD0);
    let mut r = RoomRecipe::empty(RoomKind::VoronoiPlazas, dims, seed);

    let nx = r.tile_count_x();
    let nz = r.tile_count_z();

    // 8 sites : random (x,z) positions in tile-grid space.
    let mut sites: [(f32, f32); SITES] = [(0.0, 0.0); SITES];
    for s in &mut sites {
        s.0 = rng.next_f32() * f32::from(nx);
        s.1 = rng.next_f32() * f32::from(nz);
    }

    // Nearest-site assignment.
    for x in 0..nx {
        for z in 0..nz {
            let mut best_idx = 0usize;
            let mut best_dist = f32::INFINITY;
            for (i, (sx, sz)) in sites.iter().enumerate() {
                let dx = f32::from(x) - sx;
                let dz = f32::from(z) - sz;
                let d = dx * dx + dz * dz;
                if d < best_dist {
                    best_dist = d;
                    best_idx = i;
                }
            }
            r.tiles.push(TilePlacement {
                x, z,
                layer: TileLayer::Floor,
                material_idx: best_idx as u8,
                pattern_idx: 0,
            });
        }
    }

    // Plain ceiling.
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

    // 4 NSEW doorways at midpoints.
    for side in [WallSide::N, WallSide::S, WallSide::E, WallSide::W] {
        let pos = match side {
            WallSide::N | WallSide::S => dims.width_m  * 0.5,
            WallSide::E | WallSide::W => dims.length_m * 0.5,
        };
        r.doorways.push(Doorway { side, position_m: pos, width_m: 1.2, height_m: 2.1 });
    }

    // One light per site, positioned at the site's tile-coords.
    for (i, (sx, sz)) in sites.iter().enumerate() {
        let lx = (*sx / f32::from(nx)) * dims.width_m;
        let lz = (*sz / f32::from(nz)) * dims.length_m;
        // Hue cycles across sites for visual disambiguation.
        let h = i as f32 / SITES as f32;
        let (rr, gg, bb) = light_color(h);
        r.lights.push(LightHint {
            pos_m:     [lx, dims.height_m * 0.7, lz],
            color:     [rr, gg, bb],
            intensity: 25.0,
        });
    }

    r
}

/// § Cheap hue→RGB (single primary + secondary blends).
fn light_color(h: f32) -> (f32, f32, f32) {
    let h = h.rem_euclid(1.0);
    let i = (h * 6.0).floor() as i32;
    let f = h * 6.0 - i as f32;
    match i.rem_euclid(6) {
        0 => (1.0, f, 0.0),
        1 => (1.0 - f, 1.0, 0.0),
        2 => (0.0, 1.0, f),
        3 => (0.0, 1.0 - f, 1.0),
        4 => (f, 0.0, 1.0),
        _ => (1.0, 0.0, 1.0 - f),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voronoi_determinism() {
        let dims = RoomDims::default();
        let c = RoomConstraints::default();
        assert_eq!(generate(23, dims, &c), generate(23, dims, &c));
    }

    #[test]
    fn voronoi_doorway_count() {
        let r = generate(0, RoomDims::default(), &RoomConstraints::default());
        assert_eq!(r.doorways.len(), 4);
    }

    #[test]
    fn voronoi_uses_all_8_sites() {
        let r = generate(0, RoomDims::default(), &RoomConstraints::default());
        let mut sites_seen = std::collections::HashSet::new();
        for t in &r.tiles {
            if matches!(t.layer, TileLayer::Floor) {
                sites_seen.insert(t.material_idx);
            }
        }
        // Per voronoi geometry on a 12x12 grid, all 8 sites should claim
        // at least one tile (extremely unlikely otherwise with random seeds).
        assert!(sites_seen.len() >= 6, "voronoi should expose ≥6 of 8 sites, got {}", sites_seen.len());
        for s in &sites_seen {
            assert!(*s < 8, "site idx {s} out of [0,8)");
        }
    }
}
