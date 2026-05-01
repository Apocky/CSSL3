// § T11-WAVE3-PROCGEN-ROOMS : PatternMaze recipe
// ══════════════════════════════════════════════════════════════════
//! Cellular-automata maze (Rule-30 style 1-D rule, replicated row-by-row).
//!
//! § Use-case : decorative-pattern showcase. The CA seeds with a single
//! centre-bit at z=0 ; each successive z-row evolves via Rule-30. The output
//! is mapped to floor pattern_idx (0/1) so the renderer paints alternating
//! tiles. Walls solid, 4 doorways.

use crate::constraints::RoomConstraints;
use crate::recipe::{
    Doorway, LightHint, RoomDims, RoomKind, RoomRecipe, TileLayer, TilePlacement, WallSide,
};
use crate::rng::Pcg32;

const RULE: u8 = 30;

#[must_use]
pub fn generate(seed: u64, dims: RoomDims, _c: &RoomConstraints) -> RoomRecipe {
    let mut rng = Pcg32::new(seed.wrapping_mul(0xE1) ^ 0x_AABB_BEEF_u64);
    let mut r = RoomRecipe::empty(RoomKind::PatternMaze, dims, seed);

    let nx = r.tile_count_x();
    let nz = r.tile_count_z();

    // Initial CA row : bit-pattern derived from seed (so distinct seeds give
    // distinct mazes, but each seed is fully deterministic).
    let mut row: Vec<u8> = (0..nx)
        .map(|x| ((rng.next_u32() >> ((x as u32) & 31)) & 1) as u8)
        .collect();
    // Force a single anchor bit so the maze is non-trivial even if the rng
    // happens to draw all zeros.
    row[(nx / 2) as usize] = 1;

    // Iterate over z-rows applying Rule-30.
    let cap_iters = 100u16; // §-spec : 5x5 → 100 iterations rule-30-ish
    let actual_z = nz.min(cap_iters * 2); // safety cap
    for z in 0..actual_z {
        for (x, &cell) in row.iter().enumerate() {
            r.tiles.push(TilePlacement {
                x: x as u16, z,
                layer: TileLayer::Floor,
                material_idx: 1,
                pattern_idx: cell, // 0 or 1
            });
        }
        // Evolve.
        let mut next = vec![0u8; row.len()];
        for i in 0..row.len() {
            let l = if i == 0 { 0 } else { row[i - 1] };
            let c = row[i];
            let r_n = if i + 1 == row.len() { 0 } else { row[i + 1] };
            let triplet = (l << 2) | (c << 1) | r_n;
            next[i] = (RULE >> triplet) & 1;
        }
        row = next;
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

    // 4 doorways.
    for side in [WallSide::N, WallSide::S, WallSide::E, WallSide::W] {
        let pos = match side {
            WallSide::N | WallSide::S => dims.width_m  * 0.5,
            WallSide::E | WallSide::W => dims.length_m * 0.5,
        };
        r.doorways.push(Doorway { side, position_m: pos, width_m: 1.0, height_m: 2.1 });
    }

    // Two ambient lights at quarter heights.
    r.lights.push(LightHint {
        pos_m:     [dims.width_m * 0.25, dims.height_m * 0.85, dims.length_m * 0.5],
        color:     [0.9, 0.95, 1.0],
        intensity: 35.0,
    });
    r.lights.push(LightHint {
        pos_m:     [dims.width_m * 0.75, dims.height_m * 0.85, dims.length_m * 0.5],
        color:     [1.0, 0.95, 0.9],
        intensity: 35.0,
    });

    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pattern_maze_determinism() {
        let dims = RoomDims::default();
        let c = RoomConstraints::default();
        assert_eq!(generate(17, dims, &c), generate(17, dims, &c));
    }

    #[test]
    fn pattern_maze_doorway_count() {
        let r = generate(0, RoomDims::default(), &RoomConstraints::default());
        assert_eq!(r.doorways.len(), 4);
    }

    #[test]
    fn pattern_maze_has_both_pattern_values() {
        let r = generate(0, RoomDims::default(), &RoomConstraints::default());
        let mut seen0 = false;
        let mut seen1 = false;
        for t in &r.tiles {
            if matches!(t.layer, TileLayer::Floor) {
                if t.pattern_idx == 0 { seen0 = true; }
                if t.pattern_idx == 1 { seen1 = true; }
            }
        }
        assert!(seen0 && seen1, "rule-30 should produce both 0-cells and 1-cells (seen0={seen0} seen1={seen1})");
    }
}
