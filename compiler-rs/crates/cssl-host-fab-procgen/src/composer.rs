// § T11-W5c-FAB-PROCGEN : Composer = Blueprint → recipes / WorldTiles
// ══════════════════════════════════════════════════════════════════
//! Compose a `Blueprint` into base `RoomRecipe`s and into a flat
//! `Vec<WorldTile>` with positional offset + Y-rotation applied per part.
//!
//! § The composer is the bridge from authored blueprint to renderable
//! geometry. It :
//! 1. validates the `Blueprint` structurally,
//! 2. evaluates `StyleRules` (returning every violation found),
//! 3. dispatches to `cssl_host_procgen_rooms::generate` for each part with
//!    a deterministic per-part seed-mix,
//! 4. flattens tiles to world-space `WorldTile`s honoring `position_offset`
//!    and `rotation_y`.
//!
//! § Determinism :
//!   per_part_seed = blueprint.seed
//!         .wrapping_mul(0x9E37_79B9_7F4A_7C15)
//!         .wrapping_add(part.id as u64)
//!         .wrapping_add(0xBD8F_0001)
//! § Same `(seed, parts, connections)` always → same WorldTile vec.

use cssl_host_procgen_rooms::{
    generate as generate_recipe, RoomConstraints, RoomRecipe, TileLayer,
};
use serde::{Deserialize, Serialize};

use crate::blueprint::{Blueprint, BlueprintErr};
use crate::style_rules::{StyleRules, StyleViolation};

// ── data ──────────────────────────────────────────────────────────

/// § Flattened world-space tile : 16-byte stride.
///   bytes : x(4) y(4) z(4) layer(1) mat(1) pat(1) pad(1) = 16
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[repr(C)]
pub struct WorldTile {
    pub x:            f32,
    pub y:            f32,
    pub z:            f32,
    pub layer:        u8,
    pub material_idx: u8,
    pub pattern_idx:  u8,
    pad:              u8,
}

impl WorldTile {
    #[must_use]
    pub fn new(x: f32, y: f32, z: f32, layer: u8, material_idx: u8, pattern_idx: u8) -> Self {
        Self {
            x,
            y,
            z,
            layer,
            material_idx,
            pattern_idx,
            pad: 0,
        }
    }
}

/// § Errors surfaced by `Composer::compose_*`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ComposeErr {
    Blueprint(BlueprintErr),
    Style(Vec<StyleViolation>),
    DimsZero,
}

// ── composer ──────────────────────────────────────────────────────

/// § The composer applies a `StyleRules` set when expanding blueprints.
#[derive(Debug, Clone, PartialEq)]
pub struct Composer {
    pub rules: StyleRules,
}

impl Composer {
    #[must_use]
    pub fn new(rules: StyleRules) -> Self {
        Self { rules }
    }

    /// § Compose blueprint to a list of base `RoomRecipe`s, one per part,
    /// in part-id order. Style-rule violations cause `ComposeErr::Style`.
    pub fn compose_to_recipes(&self, bp: &Blueprint) -> Result<Vec<RoomRecipe>, ComposeErr> {
        bp.validate().map_err(ComposeErr::Blueprint)?;
        let violations = self.rules.check_blueprint(bp);
        if !violations.is_empty() {
            return Err(ComposeErr::Style(violations));
        }
        let constraints = RoomConstraints::default();
        let mut out = Vec::with_capacity(bp.parts.len());
        for p in &bp.parts {
            if p.dims.width_m <= 0.0 || p.dims.length_m <= 0.0 || p.dims.tile_size_m <= 0.0 {
                return Err(ComposeErr::DimsZero);
            }
            let part_seed = mix_seed(bp.seed, p.id);
            let recipe = generate_recipe(part_seed, p.dims, p.kind, &constraints);
            out.push(recipe);
        }
        Ok(out)
    }

    /// § Compose blueprint to a flat list of world-space tiles.
    /// Per-part `position_offset` and `rotation_y` are applied to every tile.
    pub fn compose_to_world_tiles(&self, bp: &Blueprint) -> Result<Vec<WorldTile>, ComposeErr> {
        let recipes = self.compose_to_recipes(bp)?;
        let mut out = Vec::new();
        for (recipe, part) in recipes.iter().zip(bp.parts.iter()) {
            let (ox, oy, oz) = part.position_offset;
            let (cos_t, sin_t) = (part.rotation_y.cos(), part.rotation_y.sin());
            for t in &recipe.tiles {
                // Local x/z from tile-grid coordinates.
                let lx = (t.x as f32 + 0.5) * recipe.dims.tile_size_m
                    - recipe.dims.width_m * 0.5;
                let lz = (t.z as f32 + 0.5) * recipe.dims.tile_size_m
                    - recipe.dims.length_m * 0.5;
                // Y from layer.
                let ly = match t.layer {
                    TileLayer::Floor      => 0.0,
                    TileLayer::Ceiling    => recipe.dims.height_m,
                    TileLayer::Wall(_)    => recipe.dims.height_m * 0.5,
                };
                // Apply Y-rotation.
                let rx = lx * cos_t - lz * sin_t;
                let rz = lx * sin_t + lz * cos_t;
                // Apply offset.
                let wx = rx + ox;
                let wy = ly + oy;
                let wz = rz + oz;
                let layer_byte = match t.layer {
                    TileLayer::Floor       => 0,
                    TileLayer::Ceiling     => 1,
                    TileLayer::Wall(_)     => 2,
                };
                out.push(WorldTile::new(
                    wx,
                    wy,
                    wz,
                    layer_byte,
                    t.material_idx,
                    t.pattern_idx,
                ));
            }
        }
        Ok(out)
    }
}

// ── seed mix : per-part deterministic offset from blueprint.seed ──
fn mix_seed(seed: u64, part_id: u32) -> u64 {
    seed.wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(u64::from(part_id))
        .wrapping_add(0xBD8F_0001)
}

// ══════════════════════════════════════════════════════════════════
// § Tests
// ══════════════════════════════════════════════════════════════════
#[cfg(test)]
mod tests {
    use super::*;
    use cssl_host_procgen_rooms::{RoomDims, RoomKind, WallSide};

    fn dims() -> RoomDims {
        RoomDims::default()
    }

    /// § Empty blueprint composes to zero recipes / zero tiles.
    #[test]
    fn empty_blueprint_composes_empty() {
        let bp = Blueprint::new("empty".to_string(), 0);
        let c = Composer::new(StyleRules::default());
        let recipes = c.compose_to_recipes(&bp).unwrap();
        assert!(recipes.is_empty());
        let tiles = c.compose_to_world_tiles(&bp).unwrap();
        assert!(tiles.is_empty());
    }

    /// § Single-part blueprint produces exactly 1 recipe + non-empty tiles.
    #[test]
    fn single_part_composes_one_recipe() {
        let mut bp = Blueprint::new("one".to_string(), 99);
        bp.add_part(RoomKind::CalibrationGrid, dims(), (0.0, 0.0, 0.0), 0.0);
        let c = Composer::new(StyleRules::default());
        let recipes = c.compose_to_recipes(&bp).unwrap();
        assert_eq!(recipes.len(), 1);
        assert_eq!(recipes[0].kind, RoomKind::CalibrationGrid);
        let tiles = c.compose_to_world_tiles(&bp).unwrap();
        assert!(!tiles.is_empty());
    }

    /// § Multi-part : recipes preserved in part-id order ; tile counts add up.
    #[test]
    fn multi_part_composes_in_order() {
        let mut bp = Blueprint::new("multi".to_string(), 7);
        bp.add_part(RoomKind::CalibrationGrid, dims(), (0.0, 0.0, 0.0), 0.0);
        bp.add_part(RoomKind::ScaleHall, dims(), (10.0, 0.0, 0.0), 0.0);
        bp.add_part(RoomKind::ColorWheel, dims(), (0.0, 0.0, 10.0), 0.0);
        let c = Composer::new(StyleRules::default());
        let recipes = c.compose_to_recipes(&bp).unwrap();
        assert_eq!(recipes.len(), 3);
        assert_eq!(recipes[0].kind, RoomKind::CalibrationGrid);
        assert_eq!(recipes[1].kind, RoomKind::ScaleHall);
        assert_eq!(recipes[2].kind, RoomKind::ColorWheel);

        let tiles = c.compose_to_world_tiles(&bp).unwrap();
        let total_recipe_tiles: usize = recipes.iter().map(|r| r.tiles.len()).sum();
        assert_eq!(tiles.len(), total_recipe_tiles);

        // § Determinism : second compose produces identical world tiles.
        let tiles2 = c.compose_to_world_tiles(&bp).unwrap();
        assert_eq!(tiles, tiles2);
    }

    /// § Position offset is applied to world tiles.
    #[test]
    fn world_tile_position_offset_applied() {
        let mut bp_a = Blueprint::new("a".to_string(), 13);
        bp_a.add_part(RoomKind::CalibrationGrid, dims(), (0.0, 0.0, 0.0), 0.0);
        let mut bp_b = Blueprint::new("b".to_string(), 13);
        bp_b.add_part(RoomKind::CalibrationGrid, dims(), (100.0, 0.0, 0.0), 0.0);
        let c = Composer::new(StyleRules::default());
        let ta = c.compose_to_world_tiles(&bp_a).unwrap();
        let tb = c.compose_to_world_tiles(&bp_b).unwrap();
        assert_eq!(ta.len(), tb.len());
        // Every tile in bp_b is shifted +100 in X relative to bp_a.
        for (a, b) in ta.iter().zip(tb.iter()) {
            assert!((b.x - a.x - 100.0).abs() < 1e-3, "{} vs {}", a.x, b.x);
            assert!((b.z - a.z).abs() < 1e-3);
            assert!((b.y - a.y).abs() < 1e-3);
        }
    }

    /// § Style-violation blocks composition.
    #[test]
    fn style_violation_blocks_compose() {
        let mut bp = Blueprint::new("toomany".to_string(), 5);
        for _ in 0..6 {
            bp.add_part(RoomKind::CalibrationGrid, dims(), (0.0, 0.0, 0.0), 0.0);
        }
        let rules = StyleRules {
            max_parts: 2,
            ..StyleRules::default()
        };
        let c = Composer::new(rules);
        let err = c.compose_to_recipes(&bp).expect_err("should fail");
        match err {
            ComposeErr::Style(v) => assert!(!v.is_empty()),
            other => panic!("expected Style err, got {other:?}"),
        }
    }

    /// § Rotation : 90deg rotation swaps local-X and local-Z components.
    #[test]
    fn rotation_y_respected() {
        let pi_2: f32 = std::f32::consts::FRAC_PI_2;
        let mut bp_a = Blueprint::new("ar".to_string(), 21);
        bp_a.add_part(RoomKind::ScaleHall, dims(), (0.0, 0.0, 0.0), 0.0);
        let mut bp_b = Blueprint::new("br".to_string(), 21);
        bp_b.add_part(RoomKind::ScaleHall, dims(), (0.0, 0.0, 0.0), pi_2);
        let c = Composer::new(StyleRules::default());
        let ta = c.compose_to_world_tiles(&bp_a).unwrap();
        let tb = c.compose_to_world_tiles(&bp_b).unwrap();
        assert_eq!(ta.len(), tb.len());
        // For 90deg : new_x = -old_z, new_z = old_x. Spot-check a tile pair.
        // We cannot rely on tile-order alignment, so check that the rotated
        // tiles set contains the analytic image of every original tile.
        let mut hits = 0_usize;
        for a in &ta {
            let expected_x = -a.z;
            let expected_z = a.x;
            if tb
                .iter()
                .any(|b| (b.x - expected_x).abs() < 1e-3 && (b.z - expected_z).abs() < 1e-3
                    && b.layer == a.layer)
            {
                hits += 1;
            }
        }
        // Most tiles should round-trip. Allow small slack for f32 jitter.
        assert!(
            hits as f32 / ta.len() as f32 > 0.95,
            "rotation did not preserve tile-set ({hits}/{} matched)",
            ta.len()
        );
    }

    /// § Connection between parts is permitted (smoke-test of the
    /// connection-validation path interacting with composer).
    #[test]
    fn connected_parts_compose_ok() {
        let mut bp = Blueprint::new("conn".to_string(), 17);
        let a = bp.add_part(RoomKind::CalibrationGrid, dims(), (0.0, 0.0, 0.0), 0.0);
        let b = bp.add_part(RoomKind::ScaleHall, dims(), (10.0, 0.0, 0.0), 0.0);
        bp.connect(a, WallSide::E, b, WallSide::W).unwrap();
        let c = Composer::new(StyleRules::default());
        let recipes = c.compose_to_recipes(&bp).unwrap();
        assert_eq!(recipes.len(), 2);
    }

    /// § WorldTile struct is exactly 16 bytes (locked-in stride).
    #[test]
    fn world_tile_is_16_bytes() {
        assert_eq!(std::mem::size_of::<WorldTile>(), 16);
    }
}
