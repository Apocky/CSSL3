// § T11-W5c-FAB-PROCGEN : pre-built blueprint library
// ══════════════════════════════════════════════════════════════════
//! Three reference blueprints :
//!  - [`cathedral_blueprint`] — calibration entry + 4 voronoi plazas + 2 halls
//!  - [`maze_dungeon_blueprint`] — 5 pattern-maze rooms with chained connections
//!  - [`color_pavilion_blueprint`] — color-wheel center + 4 material-showcase wings
//!
//! § These are authored blueprints, not procgen seeds. They show how to
//! compose base `RoomKind` recipes into evocative spaces and serve as
//! integration anchors for downstream wave-6 loa-host wiring.

use cssl_host_procgen_rooms::{RoomDims, RoomKind, WallSide};

use crate::blueprint::Blueprint;

/// § "Cathedral" : a central calibration-grid entry with 4 voronoi-plaza
/// transepts at NSEW + 2 connecting scale-halls.
///
/// § Total parts = 7 (1 entry + 4 plazas + 2 halls).
#[must_use]
pub fn cathedral_blueprint(seed: u64) -> Blueprint {
    let mut bp = Blueprint::new("cathedral".to_string(), seed);
    let dims = RoomDims::default();
    let big = RoomDims {
        width_m:     12.0,
        length_m:    12.0,
        height_m:    6.0,
        tile_size_m: 0.5,
    };

    // Central entry (calibration grid).
    let entry = bp.add_part(RoomKind::CalibrationGrid, dims, (0.0, 0.0, 0.0), 0.0);

    // 4 voronoi plazas at NSEW (12m radius).
    let p_n = bp.add_part(RoomKind::VoronoiPlazas, big, (0.0, 0.0, 12.0), 0.0);
    let p_s = bp.add_part(RoomKind::VoronoiPlazas, big, (0.0, 0.0, -12.0), 0.0);
    let p_e = bp.add_part(RoomKind::VoronoiPlazas, big, (12.0, 0.0, 0.0), 0.0);
    let p_w = bp.add_part(RoomKind::VoronoiPlazas, big, (-12.0, 0.0, 0.0), 0.0);

    // 2 connecting halls — east and west axis.
    let hall_e = bp.add_part(RoomKind::ScaleHall, dims, (6.0, 0.0, 0.0), 0.0);
    let hall_w = bp.add_part(RoomKind::ScaleHall, dims, (-6.0, 0.0, 0.0), 0.0);

    // Connect entry to N/S plazas directly + halls bridge to E/W plazas.
    let _ = bp.connect(entry, WallSide::N, p_n, WallSide::S);
    let _ = bp.connect(entry, WallSide::S, p_s, WallSide::N);
    let _ = bp.connect(entry, WallSide::E, hall_e, WallSide::W);
    let _ = bp.connect(hall_e, WallSide::E, p_e, WallSide::W);
    let _ = bp.connect(entry, WallSide::W, hall_w, WallSide::E);
    let _ = bp.connect(hall_w, WallSide::W, p_w, WallSide::E);

    bp
}

/// § "Maze dungeon" : 5 pattern-maze rooms chained linearly.
///
/// § Each room is offset (10,0,0) from its predecessor so doorways align
/// along the +X axis. Connections form a single path A→B→C→D→E.
#[must_use]
pub fn maze_dungeon_blueprint(seed: u64) -> Blueprint {
    let mut bp = Blueprint::new("maze_dungeon".to_string(), seed);
    let dims = RoomDims::default();

    let mut prev: Option<u32> = None;
    for i in 0..5 {
        let pos = (i as f32 * 10.0, 0.0, 0.0);
        let id = bp.add_part(RoomKind::PatternMaze, dims, pos, 0.0);
        if let Some(p) = prev {
            let _ = bp.connect(p, WallSide::E, id, WallSide::W);
        }
        prev = Some(id);
    }

    bp
}

/// § "Color pavilion" : a color-wheel center + 4 material-showcase wings on
/// NSEW.
#[must_use]
pub fn color_pavilion_blueprint(seed: u64) -> Blueprint {
    let mut bp = Blueprint::new("color_pavilion".to_string(), seed);
    let dims = RoomDims::default();

    let center = bp.add_part(RoomKind::ColorWheel, dims, (0.0, 0.0, 0.0), 0.0);
    let n = bp.add_part(RoomKind::MaterialShowcase, dims, (0.0, 0.0, 8.0), 0.0);
    let s = bp.add_part(RoomKind::MaterialShowcase, dims, (0.0, 0.0, -8.0), 0.0);
    let e = bp.add_part(RoomKind::MaterialShowcase, dims, (8.0, 0.0, 0.0), 0.0);
    let w = bp.add_part(RoomKind::MaterialShowcase, dims, (-8.0, 0.0, 0.0), 0.0);

    let _ = bp.connect(center, WallSide::N, n, WallSide::S);
    let _ = bp.connect(center, WallSide::S, s, WallSide::N);
    let _ = bp.connect(center, WallSide::E, e, WallSide::W);
    let _ = bp.connect(center, WallSide::W, w, WallSide::E);

    bp
}

// ══════════════════════════════════════════════════════════════════
// § Tests
// ══════════════════════════════════════════════════════════════════
#[cfg(test)]
mod tests {
    use super::*;

    /// § Each library blueprint passes structural validation.
    #[test]
    fn library_blueprints_validate() {
        cathedral_blueprint(0).validate().expect("cathedral validates");
        maze_dungeon_blueprint(0).validate().expect("maze_dungeon validates");
        color_pavilion_blueprint(0).validate().expect("color_pavilion validates");
    }

    /// § Cathedral has 7 parts.
    #[test]
    fn cathedral_has_seven_parts() {
        let bp = cathedral_blueprint(42);
        assert_eq!(bp.part_count(), 7);
        assert_eq!(bp.name, "cathedral");
        assert!(!bp.connections.is_empty());
    }

    /// § Maze dungeon has 5 parts and exactly 4 connections (A→B→C→D→E).
    #[test]
    fn maze_dungeon_has_five_parts() {
        let bp = maze_dungeon_blueprint(7);
        assert_eq!(bp.part_count(), 5);
        assert_eq!(bp.connections.len(), 4);
        // Every part is a PatternMaze.
        assert!(bp.parts.iter().all(|p| p.kind == RoomKind::PatternMaze));
    }

    /// § Color pavilion has 5 parts (1 center + 4 wings).
    #[test]
    fn color_pavilion_has_five_parts() {
        let bp = color_pavilion_blueprint(13);
        assert_eq!(bp.part_count(), 5);
        assert_eq!(bp.connections.len(), 4);
        assert_eq!(bp.parts[0].kind, RoomKind::ColorWheel);
        for wing in &bp.parts[1..] {
            assert_eq!(wing.kind, RoomKind::MaterialShowcase);
        }
    }
}
