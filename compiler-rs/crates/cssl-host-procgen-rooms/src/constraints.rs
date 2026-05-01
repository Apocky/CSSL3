// § T11-WAVE3-PROCGEN-ROOMS : recipe constraint-validation
// ══════════════════════════════════════════════════════════════════
//! Constraint-set + post-generation validation.
//!
//! § Recipes are checked against a `RoomConstraints` to surface mismatches.
//! Validation is non-destructive : returns an `Err(Vec<ConstraintErr>)` listing
//! every violation rather than short-circuiting.

use serde::{Deserialize, Serialize};

use crate::recipe::{RoomRecipe, TileLayer};

/// § User-supplied constraints on the generated recipe.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoomConstraints {
    pub min_doorways:        u8,
    pub max_doorways:        u8,
    pub allowed_materials:   Vec<u8>,
    pub light_density:       f32,
    pub must_have_floor:     bool,
    pub must_have_ceiling:   bool,
}

impl Default for RoomConstraints {
    fn default() -> Self {
        Self {
            min_doorways:        1,
            max_doorways:        4,
            allowed_materials:   (0..=15).collect(),
            light_density:       1.0,
            must_have_floor:     true,
            must_have_ceiling:   false,
        }
    }
}

/// § Single constraint-violation kind.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ConstraintErr {
    TooFewDoorways,
    TooManyDoorways,
    DisallowedMaterial(u8),
    MissingFloor,
    MissingCeiling,
}

/// § Validate a recipe against constraints. Returns `Ok` if no violations,
/// `Err(violations)` otherwise.
///
/// § Per-violation reporting : the returned `Vec` may have many entries.
pub fn validate_recipe(recipe: &RoomRecipe, c: &RoomConstraints) -> Result<(), Vec<ConstraintErr>> {
    let mut errs = Vec::new();

    // Doorway-count bounds.
    let dc = recipe.doorways.len() as u8;
    if dc < c.min_doorways {
        errs.push(ConstraintErr::TooFewDoorways);
    }
    if dc > c.max_doorways {
        errs.push(ConstraintErr::TooManyDoorways);
    }

    // Disallowed-material scan.
    let allowed = &c.allowed_materials;
    let mut seen_disallowed: Vec<u8> = Vec::new();
    for tile in &recipe.tiles {
        if !allowed.contains(&tile.material_idx) && !seen_disallowed.contains(&tile.material_idx) {
            seen_disallowed.push(tile.material_idx);
            errs.push(ConstraintErr::DisallowedMaterial(tile.material_idx));
        }
    }

    // Surface presence.
    if c.must_have_floor && !recipe.tiles.iter().any(|t| matches!(t.layer, TileLayer::Floor)) {
        errs.push(ConstraintErr::MissingFloor);
    }
    if c.must_have_ceiling && !recipe.tiles.iter().any(|t| matches!(t.layer, TileLayer::Ceiling)) {
        errs.push(ConstraintErr::MissingCeiling);
    }

    if errs.is_empty() {
        Ok(())
    } else {
        Err(errs)
    }
}

// ══════════════════════════════════════════════════════════════════
// § Tests
// ══════════════════════════════════════════════════════════════════
#[cfg(test)]
mod tests {
    use super::*;
    use crate::recipe::{Doorway, RoomDims, RoomKind, TilePlacement, WallSide};

    fn empty_recipe() -> RoomRecipe {
        RoomRecipe::empty(RoomKind::CalibrationGrid, RoomDims::default(), 0)
    }

    /// § Empty recipe (no floor tiles) fails MissingFloor when must_have_floor=true.
    #[test]
    fn empty_recipe_fails_floor_required() {
        let r = empty_recipe();
        let c = RoomConstraints {
            min_doorways: 0,
            ..RoomConstraints::default()
        };
        let result = validate_recipe(&r, &c);
        let errs = result.expect_err("empty recipe should fail");
        assert!(errs.contains(&ConstraintErr::MissingFloor));
    }

    /// § Doorway-count below min and above max both surface as errors.
    #[test]
    fn doorway_count_bounds() {
        let mut r = empty_recipe();
        // Add a single floor tile to satisfy must_have_floor.
        r.tiles.push(TilePlacement {
            x: 0, z: 0, layer: TileLayer::Floor, material_idx: 0, pattern_idx: 0,
        });

        // Zero doorways with min=2 → TooFewDoorways.
        let c_min = RoomConstraints {
            min_doorways: 2,
            max_doorways: 4,
            ..RoomConstraints::default()
        };
        let errs = validate_recipe(&r, &c_min).expect_err("0 doorways < 2");
        assert!(errs.contains(&ConstraintErr::TooFewDoorways));

        // Five doorways with max=2 → TooManyDoorways.
        for _ in 0..5 {
            r.doorways.push(Doorway {
                side: WallSide::N, position_m: 1.0, width_m: 1.0, height_m: 2.0,
            });
        }
        let c_max = RoomConstraints {
            min_doorways: 0,
            max_doorways: 2,
            ..RoomConstraints::default()
        };
        let errs = validate_recipe(&r, &c_max).expect_err("5 doorways > 2");
        assert!(errs.contains(&ConstraintErr::TooManyDoorways));
    }

    /// § Disallowed material index surfaces in error list.
    #[test]
    fn disallowed_material_detected() {
        let mut r = empty_recipe();
        r.tiles.push(TilePlacement {
            x: 0, z: 0, layer: TileLayer::Floor, material_idx: 99, pattern_idx: 0,
        });
        r.doorways.push(Doorway {
            side: WallSide::N, position_m: 1.0, width_m: 1.0, height_m: 2.0,
        });
        let c = RoomConstraints {
            allowed_materials: vec![0, 1, 2],
            min_doorways: 1,
            ..RoomConstraints::default()
        };
        let errs = validate_recipe(&r, &c).expect_err("material 99 not in [0,1,2]");
        assert!(errs.contains(&ConstraintErr::DisallowedMaterial(99)));
    }

    /// § Recipe satisfying all constraints validates Ok.
    #[test]
    fn valid_recipe_passes() {
        let mut r = empty_recipe();
        r.tiles.push(TilePlacement {
            x: 0, z: 0, layer: TileLayer::Floor, material_idx: 1, pattern_idx: 0,
        });
        r.tiles.push(TilePlacement {
            x: 1, z: 1, layer: TileLayer::Ceiling, material_idx: 2, pattern_idx: 0,
        });
        r.doorways.push(Doorway {
            side: WallSide::N, position_m: 1.0, width_m: 1.0, height_m: 2.0,
        });
        let c = RoomConstraints {
            min_doorways: 1,
            max_doorways: 4,
            allowed_materials: vec![0, 1, 2, 3],
            light_density: 1.0,
            must_have_floor: true,
            must_have_ceiling: true,
        };
        validate_recipe(&r, &c).expect("recipe should validate");
    }
}
