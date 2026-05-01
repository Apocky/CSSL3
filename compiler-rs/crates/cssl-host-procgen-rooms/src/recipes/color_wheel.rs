// § T11-WAVE3-PROCGEN-ROOMS : ColorWheel recipe
// ══════════════════════════════════════════════════════════════════
//! 12-segment circular floor pattern.
//!
//! § Use-case : color-calibration room. Floor tiles are coloured per their
//! polar-angle (0..12 segment index → material_idx 0..11). Segment indices
//! cycle around the centre. Walls solid + 4 doorways.

use crate::constraints::RoomConstraints;
use crate::recipe::{
    Doorway, LightHint, RoomDims, RoomKind, RoomRecipe, TileLayer, TilePlacement, WallSide,
};
use crate::rng::Pcg32;

const SEGMENTS: u8 = 12;

#[must_use]
pub fn generate(seed: u64, dims: RoomDims, _c: &RoomConstraints) -> RoomRecipe {
    let mut rng = Pcg32::new(seed.wrapping_mul(0xD1) ^ 0x_C010_E777);
    let mut r = RoomRecipe::empty(RoomKind::ColorWheel, dims, seed);

    let nx = r.tile_count_x();
    let nz = r.tile_count_z();
    let cx = nx as f32 * 0.5;
    let cz = nz as f32 * 0.5;

    // Polar-angle segmentation. Add a small rng-derived rotation so the
    // wheel orientation is seed-dependent.
    let rotation = rng.next_f32() * std::f32::consts::TAU;
    let two_pi = std::f32::consts::TAU;

    for x in 0..nx {
        for z in 0..nz {
            let dx = x as f32 - cx;
            let dz = z as f32 - cz;
            // Normalise to [0, 2π) using rem_euclid (deterministic + clippy-clean).
            let angle = (dz.atan2(dx) + rotation).rem_euclid(two_pi);
            let segment = ((angle / two_pi) * f32::from(SEGMENTS)).floor() as u8 % SEGMENTS;
            r.tiles.push(TilePlacement {
                x, z,
                layer: TileLayer::Floor,
                material_idx: segment,
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

    // Walls solid.
    for x in 0..nx {
        r.tiles.push(TilePlacement { x, z: 0,      layer: TileLayer::Wall(WallSide::S), material_idx: 3, pattern_idx: 0 });
        r.tiles.push(TilePlacement { x, z: nz - 1, layer: TileLayer::Wall(WallSide::N), material_idx: 3, pattern_idx: 0 });
    }
    for z in 0..nz {
        r.tiles.push(TilePlacement { x: 0,      z, layer: TileLayer::Wall(WallSide::W), material_idx: 3, pattern_idx: 0 });
        r.tiles.push(TilePlacement { x: nx - 1, z, layer: TileLayer::Wall(WallSide::E), material_idx: 3, pattern_idx: 0 });
    }

    // 4 NSEW doorways.
    for side in [WallSide::N, WallSide::S, WallSide::E, WallSide::W] {
        let pos = match side {
            WallSide::N | WallSide::S => dims.width_m  * 0.5,
            WallSide::E | WallSide::W => dims.length_m * 0.5,
        };
        r.doorways.push(Doorway { side, position_m: pos, width_m: 1.1, height_m: 2.1 });
    }

    // Single bright zenith light + 12 segment fill lights at ring radius.
    r.lights.push(LightHint {
        pos_m:     [dims.width_m * 0.5, dims.height_m * 0.95, dims.length_m * 0.5],
        color:     [1.0, 1.0, 1.0],
        intensity: 60.0,
    });
    let ring_r = (dims.width_m.min(dims.length_m)) * 0.35;
    for k in 0..SEGMENTS {
        let theta = (k as f32 / f32::from(SEGMENTS)) * two_pi;
        let lx = dims.width_m  * 0.5 + theta.cos() * ring_r;
        let lz = dims.length_m * 0.5 + theta.sin() * ring_r;
        // Color rotates around HSV-like ring (RGB approximation).
        let h = k as f32 / f32::from(SEGMENTS);
        let (rr, gg, bb) = hsv_to_rgb(h, 1.0, 1.0);
        r.lights.push(LightHint {
            pos_m:     [lx, dims.height_m * 0.4, lz],
            color:     [rr, gg, bb],
            intensity: 12.0,
        });
    }

    r
}

/// § HSV→RGB helper (s=v=1 special-case sufficient ; deterministic).
fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (f32, f32, f32) {
    let i = (h * 6.0).floor() as i32;
    let f = h * 6.0 - i as f32;
    let p = v * (1.0 - s);
    let q = v * (1.0 - f * s);
    let t = v * (1.0 - (1.0 - f) * s);
    match i.rem_euclid(6) {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_wheel_determinism() {
        let dims = RoomDims::default();
        let c = RoomConstraints::default();
        assert_eq!(generate(13, dims, &c), generate(13, dims, &c));
    }

    #[test]
    fn color_wheel_doorway_count() {
        let r = generate(0, RoomDims::default(), &RoomConstraints::default());
        assert_eq!(r.doorways.len(), 4);
    }

    #[test]
    fn color_wheel_uses_12_segment_indices_on_floor() {
        let r = generate(0, RoomDims::default(), &RoomConstraints::default());
        let mut segs = std::collections::HashSet::new();
        for t in &r.tiles {
            if matches!(t.layer, TileLayer::Floor) {
                segs.insert(t.material_idx);
            }
        }
        // At least 8 of 12 segments visible (boundary-tile edge cases allowed).
        assert!(segs.len() >= 8, "color wheel should expose ≥8 segments, got {}", segs.len());
        for s in &segs {
            assert!(*s < 12, "segment idx {s} out of [0,12)");
        }
    }
}
