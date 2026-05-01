// § T11-WAVE3-PROCGEN-ROOMS : recipe-implementation dispatch
// ══════════════════════════════════════════════════════════════════
//! 7 recipe implementations + a single `generate(...)` dispatcher.
//!
//! § Each sub-module exposes `pub fn generate(seed, dims, constraints) -> RoomRecipe`
//! and is selected by the top-level `generate()` based on `RoomKind`.

pub mod calibration;
pub mod color_wheel;
pub mod material_showcase;
pub mod noise_field;
pub mod pattern_maze;
pub mod scale_hall;
pub mod voronoi_plazas;

use crate::constraints::RoomConstraints;
use crate::recipe::{RoomDims, RoomKind, RoomRecipe};

/// § Dispatch to the per-kind recipe generator.
#[must_use]
pub fn generate(
    seed: u64,
    dims: RoomDims,
    kind: RoomKind,
    constraints: &RoomConstraints,
) -> RoomRecipe {
    match kind {
        RoomKind::CalibrationGrid  => calibration::generate(seed, dims, constraints),
        RoomKind::MaterialShowcase => material_showcase::generate(seed, dims, constraints),
        RoomKind::ScaleHall        => scale_hall::generate(seed, dims, constraints),
        RoomKind::ColorWheel       => color_wheel::generate(seed, dims, constraints),
        RoomKind::PatternMaze      => pattern_maze::generate(seed, dims, constraints),
        RoomKind::NoiseField       => noise_field::generate(seed, dims, constraints),
        RoomKind::VoronoiPlazas    => voronoi_plazas::generate(seed, dims, constraints),
    }
}

// ══════════════════════════════════════════════════════════════════
// § Tests : dispatcher selects the right generator
// ══════════════════════════════════════════════════════════════════
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatch_returns_correct_kind() {
        let dims = RoomDims::default();
        let c = RoomConstraints::default();
        for k in [
            RoomKind::CalibrationGrid,
            RoomKind::MaterialShowcase,
            RoomKind::ScaleHall,
            RoomKind::ColorWheel,
            RoomKind::PatternMaze,
            RoomKind::NoiseField,
            RoomKind::VoronoiPlazas,
        ] {
            let r = generate(123, dims, k, &c);
            assert_eq!(r.kind, k, "dispatcher returned wrong kind for {k:?}");
            assert_eq!(r.seed, 123);
        }
    }

    #[test]
    fn dispatch_determinism_round_trip() {
        let dims = RoomDims::default();
        let c = RoomConstraints::default();
        let r1 = generate(99, dims, RoomKind::PatternMaze, &c);
        let r2 = generate(99, dims, RoomKind::PatternMaze, &c);
        assert_eq!(r1, r2);
        // Round-trip via JSON.
        let json = serde_json::to_string(&r1).unwrap();
        let r3: RoomRecipe = serde_json::from_str(&json).unwrap();
        assert_eq!(r1, r3);
        let json2 = serde_json::to_string(&r3).unwrap();
        assert_eq!(json, json2);
    }
}
