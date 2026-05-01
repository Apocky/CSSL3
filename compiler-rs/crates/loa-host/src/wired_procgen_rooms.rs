//! § wired_procgen_rooms — loa-host wrapper around `cssl-host-procgen-rooms`.
//!
//! § T11-W5c-LOA-HOST-WIRE
//!   Re-exports the deterministic room-recipe types + the dispatcher
//!   `recipes::generate` so MCP tools can list the 7 RoomKind variants and
//!   spawn a recipe by kind without reaching into the path-dep directly.
//!
//! § wrapped surface
//!   - [`RoomRecipe`] / [`RoomKind`] / [`RoomDims`] — recipe envelope.
//!   - [`RoomConstraints`] / [`validate_recipe`] — input + post-validation.
//!   - [`generate`] — top-level kind-dispatcher.
//!
//! § ATTESTATION ¬ harm — wrapper is a re-export shim ; deterministic gen.

pub use cssl_host_procgen_rooms::{
    generate, validate_recipe, ConstraintErr, Doorway, LightHint, Pcg32, RoomConstraints, RoomDims,
    RoomKind, RoomRecipe, TileLayer, TilePlacement, WallSide,
};

/// Convenience : list the 7 canonical [`RoomKind`] variants as `&'static str`.
/// Order matches the canonical `RoomKind` enum declaration.
#[must_use]
pub fn all_room_kinds() -> &'static [&'static str] {
    &[
        "CalibrationGrid",
        "MaterialShowcase",
        "ScaleHall",
        "ColorWheel",
        "PatternMaze",
        "NoiseField",
        "VoronoiPlazas",
    ]
}

/// Convenience : every variant of [`RoomKind`] iterated in declaration order.
#[must_use]
pub fn all_kinds_typed() -> [RoomKind; 7] {
    [
        RoomKind::CalibrationGrid,
        RoomKind::MaterialShowcase,
        RoomKind::ScaleHall,
        RoomKind::ColorWheel,
        RoomKind::PatternMaze,
        RoomKind::NoiseField,
        RoomKind::VoronoiPlazas,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_room_kinds_lists_seven() {
        assert_eq!(all_room_kinds().len(), 7);
        assert_eq!(all_kinds_typed().len(), 7);
    }

    #[test]
    fn generate_round_trip_deterministic() {
        let dims = RoomDims::default();
        let c = RoomConstraints::default();
        let r1 = generate(42, dims, RoomKind::CalibrationGrid, &c);
        let r2 = generate(42, dims, RoomKind::CalibrationGrid, &c);
        assert_eq!(r1, r2, "same-seed recipes must be bit-identical");
    }
}
