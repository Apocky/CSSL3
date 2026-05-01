// § T11-WAVE3-PROCGEN-ROOMS : RoomRecipe data types
// ══════════════════════════════════════════════════════════════════
//! Room-recipe data structures.
//!
//! § A `RoomRecipe` is the deterministic output of any of the 7 procgen
//! recipes (see `crate::recipes`). It describes geometry-only tile placement,
//! doorway positions, and lighting hints — it does NOT carry materials,
//! shaders, or runtime state. Wave-4 maps recipes to renderer-side meshes.
//!
//! § Serde-derived to allow round-trip JSON snapshotting (used by
//! determinism-tests).

use serde::{Deserialize, Serialize};

// ── enums ─────────────────────────────────────────────────────────

/// § Which kind of recipe produced this `RoomRecipe`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RoomKind {
    CalibrationGrid,
    MaterialShowcase,
    ScaleHall,
    ColorWheel,
    PatternMaze,
    NoiseField,
    VoronoiPlazas,
}

/// § Cardinal wall side. North faces +Z, East faces +X, etc.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WallSide {
    N,
    S,
    E,
    W,
}

/// § Which surface a tile sits on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TileLayer {
    Floor,
    Wall(WallSide),
    Ceiling,
}

// ── structs ───────────────────────────────────────────────────────

/// § Physical dimensions of the room (metres).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RoomDims {
    pub width_m:     f32,
    pub length_m:    f32,
    pub height_m:    f32,
    pub tile_size_m: f32,
}

impl Default for RoomDims {
    fn default() -> Self {
        Self {
            width_m:     6.0,
            length_m:    6.0,
            height_m:    3.0,
            tile_size_m: 0.5,
        }
    }
}

/// § Single-tile placement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TilePlacement {
    pub x:            u16,
    pub z:            u16,
    pub layer:        TileLayer,
    pub material_idx: u8,
    pub pattern_idx:  u8,
}

/// § Doorway hole in a wall side.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Doorway {
    pub side:       WallSide,
    pub position_m: f32,
    pub width_m:    f32,
    pub height_m:   f32,
}

/// § Light-source hint for the renderer to instantiate.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LightHint {
    pub pos_m:     [f32; 3],
    pub color:     [f32; 3],
    pub intensity: f32,
}

/// § Full deterministic room recipe.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoomRecipe {
    pub kind:     RoomKind,
    pub dims:     RoomDims,
    pub tiles:    Vec<TilePlacement>,
    pub doorways: Vec<Doorway>,
    pub lights:   Vec<LightHint>,
    pub seed:     u64,
}

impl RoomRecipe {
    /// § Construct an empty recipe of the given kind, dims, and seed.
    #[must_use]
    pub fn empty(kind: RoomKind, dims: RoomDims, seed: u64) -> Self {
        Self {
            kind,
            dims,
            tiles: Vec::new(),
            doorways: Vec::new(),
            lights: Vec::new(),
            seed,
        }
    }

    /// § Tile-grid extent (count along x-axis).
    #[must_use]
    pub fn tile_count_x(&self) -> u16 {
        ((self.dims.width_m / self.dims.tile_size_m).floor() as u16).max(1)
    }

    /// § Tile-grid extent (count along z-axis).
    #[must_use]
    pub fn tile_count_z(&self) -> u16 {
        ((self.dims.length_m / self.dims.tile_size_m).floor() as u16).max(1)
    }
}

// ══════════════════════════════════════════════════════════════════
// § Tests
// ══════════════════════════════════════════════════════════════════
#[cfg(test)]
mod tests {
    use super::*;

    /// § Empty recipe defaults : zero tiles/doorways/lights ; carries seed.
    #[test]
    fn empty_recipe_default() {
        let r = RoomRecipe::empty(RoomKind::CalibrationGrid, RoomDims::default(), 42);
        assert_eq!(r.kind, RoomKind::CalibrationGrid);
        assert!(r.tiles.is_empty());
        assert!(r.doorways.is_empty());
        assert!(r.lights.is_empty());
        assert_eq!(r.seed, 42);
        assert!(r.tile_count_x() > 0);
        assert!(r.tile_count_z() > 0);
    }

    /// § Recipe with tiles serializes and round-trips via JSON.
    #[test]
    fn recipe_with_tiles_serializes() {
        let mut r = RoomRecipe::empty(RoomKind::ScaleHall, RoomDims::default(), 7);
        r.tiles.push(TilePlacement {
            x:            3,
            z:            5,
            layer:        TileLayer::Floor,
            material_idx: 1,
            pattern_idx:  2,
        });
        r.tiles.push(TilePlacement {
            x:            0,
            z:            0,
            layer:        TileLayer::Wall(WallSide::N),
            material_idx: 0,
            pattern_idx:  0,
        });
        let json = serde_json::to_string(&r).expect("serialize recipe");
        let r2: RoomRecipe = serde_json::from_str(&json).expect("deserialize recipe");
        assert_eq!(r, r2);
    }

    /// § Doorway position is non-negative.
    #[test]
    fn doorway_position_positive() {
        let d = Doorway {
            side:       WallSide::N,
            position_m: 3.0,
            width_m:    1.0,
            height_m:   2.1,
        };
        assert!(d.position_m >= 0.0);
        assert!(d.width_m > 0.0);
        assert!(d.height_m > 0.0);
        let json = serde_json::to_string(&d).expect("serialize doorway");
        let d2: Doorway = serde_json::from_str(&json).expect("deserialize doorway");
        assert_eq!(d, d2);
    }

    /// § Light color components are non-negative (linear RGB).
    #[test]
    fn light_color_non_negative() {
        let l = LightHint {
            pos_m:     [1.0, 2.0, 3.0],
            color:     [0.9, 0.85, 0.8],
            intensity: 100.0,
        };
        for c in l.color {
            assert!(c >= 0.0, "light color component {c} is negative");
        }
        assert!(l.intensity >= 0.0);
        let json = serde_json::to_string(&l).expect("serialize light");
        let l2: LightHint = serde_json::from_str(&json).expect("deserialize light");
        assert_eq!(l, l2);
    }
}
