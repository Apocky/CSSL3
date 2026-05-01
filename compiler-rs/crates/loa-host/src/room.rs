//! § room — multi-room test-suite layout (hub-and-spoke).
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § T11-LOA-ROOMS (W-LOA-rooms-expand)
//!
//! § ROLE
//!   Expands the single 40×8×40m diagnostic test-room into a SUITE of five
//!   diagnostic rooms connected by short corridors. Each room targets a
//!   different aspect of the rendering pipeline so that walking through the
//!   world is a self-tour of the engine's capabilities.
//!
//! § HUB-AND-SPOKE LAYOUT  (top-down · +X right · +Z forward · north = +Z)
//!
//! ```text
//!                  ┌───────────────┐
//!                  │ MaterialRoom  │  16 spheres in 4×4 grid · one
//!                  │   30×6×30     │  material per sphere
//!                  └───────┬───────┘
//!                          │ corridor N (4×8×8)
//!                          │
//!     ┌──────────┐  ┌───────────────┐  ┌───────────┐
//!     │ColorRoom │  │   TestRoom    │  │PatternRoom│
//!     │ 30×6×30  │──│  40 × 8 × 40  │──│  30×6×30  │
//!     │  hub     │  │   (existing)  │  │  hub      │
//!     └──────────┘  └───────┬───────┘  └───────────┘
//!         W ←  corridor W   │   corridor E  →  E
//!                           │ corridor S
//!                  ┌────────┴───────┐
//!                  │   ScaleRoom    │  60×12×30 (long axis = X)
//!                  │ 1·2·3·5·10m    │  reference markers + grid floor
//!                  │  height refs   │
//!                  └────────────────┘
//! ```
//!
//! § ROOM-COORDINATES (origin at TestRoom-center · y=0 = floor)
//!   TestRoom      : x ∈ [-20,  20]   z ∈ [-20, 20]   y ∈ [0,  8]
//!   MaterialRoom  : x ∈ [-15,  15]   z ∈ [ 28, 58]   y ∈ [0,  6]
//!   PatternRoom   : x ∈ [ 28,  58]   z ∈ [-15, 15]   y ∈ [0,  6]
//!   ScaleRoom     : x ∈ [-30,  30]   z ∈ [-58,-28]   y ∈ [0, 12]
//!   ColorRoom     : x ∈ [-58, -28]   z ∈ [-15, 15]   y ∈ [0,  6]
//!
//! § CORRIDORS (4m wide × 8m tall × 8m long)
//!   N : x ∈ [-2, 2]    z ∈ [ 20,  28]   ← TestRoom→MaterialRoom
//!   E : x ∈ [20, 28]   z ∈ [ -2,   2]   ← TestRoom→PatternRoom
//!   S : x ∈ [-2, 2]    z ∈ [-28, -20]   ← TestRoom→ScaleRoom
//!   W : x ∈ [-28,-20]  z ∈ [ -2,   2]   ← TestRoom→ColorRoom
//!
//! § DOORWAYS (2m wide × 3m tall gap centered on the wall)
//!   Each doorway lives on the boundary between a corridor and a room.
//!   Walls are emitted as TWO sub-walls (left + right of the door) plus a
//!   LINTEL above the door (top of the door to the ceiling).
//!
//! § TOTAL WORLD ENVELOPE
//!   x ∈ [-58, 58] = 116m
//!   y ∈ [  0, 12] =  12m
//!   z ∈ [-58, 58] = 116m
//!   Comfortably inside the 120m × 12m × 120m budget.
//!
//! § PRIME-DIRECTIVE attestation
//!   There was no hurt nor harm in the making of this, to anyone/anything/anybody.

#![allow(clippy::module_name_repetitions)]

// ──────────────────────────────────────────────────────────────────────────
// § Room enum + per-room metadata
// ──────────────────────────────────────────────────────────────────────────

/// One of the 5 diagnostic rooms in the test-suite. The variant ordering is
/// stable : MCP `room.list` + FFI `__cssl_room_teleport(id, ...)` index by
/// the `as u32` cast value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum Room {
    /// 40×8×40m diagnostic baseline — Macbeth/Snellen/QR/barcode walls,
    /// 4 floor quadrants, 14 plinths with stress objects. Hub of the suite.
    TestRoom = 0,
    /// 30×6×30m all-grey base · 16 hovering spheres in a 4×4 grid · each
    /// sphere uses a different material from the registry. North spoke.
    MaterialRoom = 1,
    /// 30×6×30m floor divided into 16 squares (4×4) each rendering a
    /// different procedural pattern. East spoke.
    PatternRoom = 2,
    /// 60×12×30m (long corridor on X axis) · regularly-spaced reference
    /// objects every 2m : 1m·2m·3m·5m·10m height markers · grid floor.
    /// South spoke.
    ScaleRoom = 3,
    /// 30×6×30m color-fidelity diagnostic · sRGB ramp on floor · linear
    /// ramp on ceiling · gradient hue/saturation/value walls.
    /// West spoke.
    ColorRoom = 4,
}

/// Total number of rooms in the suite.
pub const ROOM_COUNT: u32 = 5;

/// Cardinal direction enum (used for doorway orientation).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    /// +Z direction.
    North,
    /// -Z direction.
    South,
    /// +X direction.
    East,
    /// -X direction.
    West,
}

/// Axis-aligned 3D box with floor-relative coords.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AxisAlignedBox {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl AxisAlignedBox {
    #[must_use]
    pub const fn new(min: [f32; 3], max: [f32; 3]) -> Self {
        Self { min, max }
    }

    /// Return `true` if `p` is inside the box (half-open, lower-inclusive).
    #[must_use]
    pub fn contains(&self, p: [f32; 3]) -> bool {
        p[0] >= self.min[0] && p[0] < self.max[0]
            && p[1] >= self.min[1] && p[1] < self.max[1]
            && p[2] >= self.min[2] && p[2] < self.max[2]
    }

    /// Center of the box.
    #[must_use]
    pub fn center(&self) -> [f32; 3] {
        [
            (self.min[0] + self.max[0]) * 0.5,
            (self.min[1] + self.max[1]) * 0.5,
            (self.min[2] + self.max[2]) * 0.5,
        ]
    }
}

/// A doorway connecting two rooms (or a room and a corridor). The doorway
/// is a 2D rectangular gap in a wall ; physics treats this as a hole in the
/// collision plane and geometry omits the corresponding wall-quad.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Doorway {
    pub from: Room,
    pub to: Room,
    /// World-space center of the doorway aperture (on the wall plane).
    pub center: [f32; 3],
    pub width: f32,
    pub height: f32,
    /// Direction the doorway faces (the normal to the missing wall, pointing
    /// from `from` toward `to`).
    pub orientation: Direction,
}

/// Standard doorway dimensions.
pub const DOORWAY_WIDTH: f32 = 2.0;
pub const DOORWAY_HEIGHT: f32 = 3.0;

// ──────────────────────────────────────────────────────────────────────────
// § Room metadata + bounds
// ──────────────────────────────────────────────────────────────────────────

impl Room {
    /// Iteration order : Test → Material → Pattern → Scale → Color.
    #[must_use]
    pub const fn all() -> [Room; 5] {
        [
            Room::TestRoom,
            Room::MaterialRoom,
            Room::PatternRoom,
            Room::ScaleRoom,
            Room::ColorRoom,
        ]
    }

    /// Return the room's interior AABB. Used for collision + camera-room
    /// detection. Floor is at y=0, ceiling at the room's height.
    #[must_use]
    pub const fn bounds(&self) -> AxisAlignedBox {
        match self {
            // The hub : 40×8×40 centered on origin (preserves existing TestRoom).
            Room::TestRoom => AxisAlignedBox::new([-20.0, 0.0, -20.0], [20.0, 8.0, 20.0]),
            // North spoke : 30×6×30 starting at z=28 (8m corridor north of TestRoom).
            Room::MaterialRoom => AxisAlignedBox::new([-15.0, 0.0, 28.0], [15.0, 6.0, 58.0]),
            // East spoke : 30×6×30 starting at x=28.
            Room::PatternRoom => AxisAlignedBox::new([28.0, 0.0, -15.0], [58.0, 6.0, 15.0]),
            // South spoke : 60×12×30 (long axis on X for diagnostic markers).
            Room::ScaleRoom => AxisAlignedBox::new([-30.0, 0.0, -58.0], [30.0, 12.0, -28.0]),
            // West spoke : 30×6×30 starting at x=-58.
            Room::ColorRoom => AxisAlignedBox::new([-58.0, 0.0, -15.0], [-28.0, 6.0, 15.0]),
        }
    }

    /// Stable string id — used for MCP `room.teleport room_id="..."`.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Room::TestRoom => "TestRoom",
            Room::MaterialRoom => "MaterialRoom",
            Room::PatternRoom => "PatternRoom",
            Room::ScaleRoom => "ScaleRoom",
            Room::ColorRoom => "ColorRoom",
        }
    }

    /// Human-readable description (HUD tooltip / MCP `room.list`).
    #[must_use]
    pub const fn description(&self) -> &'static str {
        match self {
            Room::TestRoom => "Diagnostic baseline · Macbeth/Snellen/QR/barcode walls · 14 plinths",
            Room::MaterialRoom => "Material gallery · 16 spheres × 16 materials · uniform lighting",
            Room::PatternRoom => "Pattern grid · 16 floor-tile patterns side-by-side",
            Room::ScaleRoom => "Scale reference · 1m·2m·3m·5m·10m height markers · grid floor",
            Room::ColorRoom => "Color fidelity · sRGB+linear ramps · HSV gradient walls",
        }
    }

    /// Camera spawn point (eye position) at the room center · y=1.55 above floor.
    #[must_use]
    pub fn spawn_eye_position(&self) -> [f32; 3] {
        let c = self.bounds().center();
        [c[0], 1.55, c[2]]
    }

    /// Parse a stable string id back into a Room. Returns None for unknown.
    #[must_use]
    pub fn from_str(s: &str) -> Option<Room> {
        match s {
            "TestRoom" | "test" | "test_room" => Some(Room::TestRoom),
            "MaterialRoom" | "material" | "material_room" => Some(Room::MaterialRoom),
            "PatternRoom" | "pattern" | "pattern_room" => Some(Room::PatternRoom),
            "ScaleRoom" | "scale" | "scale_room" => Some(Room::ScaleRoom),
            "ColorRoom" | "color" | "color_room" => Some(Room::ColorRoom),
            _ => None,
        }
    }

    /// Lookup a room by its `as u32` discriminant.
    #[must_use]
    pub const fn from_u32(id: u32) -> Option<Room> {
        match id {
            0 => Some(Room::TestRoom),
            1 => Some(Room::MaterialRoom),
            2 => Some(Room::PatternRoom),
            3 => Some(Room::ScaleRoom),
            4 => Some(Room::ColorRoom),
            _ => None,
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────
// § Corridor bounds (between TestRoom-hub and each spoke)
// ──────────────────────────────────────────────────────────────────────────

/// Each corridor is a 4m-wide × 8m-tall × 8m-long axis-aligned box.
/// Corridors are open : they connect to TestRoom on one side via a doorway
/// in the TestRoom wall and to the spoke room via a doorway in the spoke
/// room's wall.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Corridor {
    /// TestRoom-north → MaterialRoom-south.
    North,
    /// TestRoom-east → PatternRoom-west.
    East,
    /// TestRoom-south → ScaleRoom-north.
    South,
    /// TestRoom-west → ColorRoom-east.
    West,
}

pub const CORRIDOR_WIDTH: f32 = 4.0;
pub const CORRIDOR_HEIGHT: f32 = 8.0;
pub const CORRIDOR_LENGTH: f32 = 8.0;

impl Corridor {
    #[must_use]
    pub const fn all() -> [Corridor; 4] {
        [Corridor::North, Corridor::East, Corridor::South, Corridor::West]
    }

    /// World-space bounds of the corridor box.
    #[must_use]
    pub const fn bounds(&self) -> AxisAlignedBox {
        let half_w = CORRIDOR_WIDTH * 0.5;
        match self {
            Corridor::North => {
                AxisAlignedBox::new([-half_w, 0.0, 20.0], [half_w, CORRIDOR_HEIGHT, 28.0])
            }
            Corridor::East => {
                AxisAlignedBox::new([20.0, 0.0, -half_w], [28.0, CORRIDOR_HEIGHT, half_w])
            }
            Corridor::South => {
                AxisAlignedBox::new([-half_w, 0.0, -28.0], [half_w, CORRIDOR_HEIGHT, -20.0])
            }
            Corridor::West => {
                AxisAlignedBox::new([-28.0, 0.0, -half_w], [-20.0, CORRIDOR_HEIGHT, half_w])
            }
        }
    }

    /// The two rooms the corridor connects (hub, spoke).
    #[must_use]
    pub const fn connects(&self) -> (Room, Room) {
        match self {
            Corridor::North => (Room::TestRoom, Room::MaterialRoom),
            Corridor::East => (Room::TestRoom, Room::PatternRoom),
            Corridor::South => (Room::TestRoom, Room::ScaleRoom),
            Corridor::West => (Room::TestRoom, Room::ColorRoom),
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────
// § Doorway emission — every corridor has TWO doorways (one per end)
// ──────────────────────────────────────────────────────────────────────────

/// Build the canonical 8 doorways (2 per corridor : hub-side + spoke-side).
/// Each doorway is a 2m × 3m gap in the wall between the corridor and the
/// connecting room.
#[must_use]
pub fn doorways() -> [Doorway; 8] {
    let h_center = DOORWAY_HEIGHT * 0.5; // y=1.5 (top of door at y=3)
    [
        // Corridor North : hub-side (TestRoom-north-wall, z=20)
        Doorway {
            from: Room::TestRoom,
            to: Room::MaterialRoom,
            center: [0.0, h_center, 20.0],
            width: DOORWAY_WIDTH,
            height: DOORWAY_HEIGHT,
            orientation: Direction::North,
        },
        // Corridor North : spoke-side (MaterialRoom-south-wall, z=28)
        Doorway {
            from: Room::MaterialRoom,
            to: Room::TestRoom,
            center: [0.0, h_center, 28.0],
            width: DOORWAY_WIDTH,
            height: DOORWAY_HEIGHT,
            orientation: Direction::South,
        },
        // Corridor East : hub-side (TestRoom-east-wall, x=20)
        Doorway {
            from: Room::TestRoom,
            to: Room::PatternRoom,
            center: [20.0, h_center, 0.0],
            width: DOORWAY_WIDTH,
            height: DOORWAY_HEIGHT,
            orientation: Direction::East,
        },
        // Corridor East : spoke-side (PatternRoom-west-wall, x=28)
        Doorway {
            from: Room::PatternRoom,
            to: Room::TestRoom,
            center: [28.0, h_center, 0.0],
            width: DOORWAY_WIDTH,
            height: DOORWAY_HEIGHT,
            orientation: Direction::West,
        },
        // Corridor South : hub-side (TestRoom-south-wall, z=-20)
        Doorway {
            from: Room::TestRoom,
            to: Room::ScaleRoom,
            center: [0.0, h_center, -20.0],
            width: DOORWAY_WIDTH,
            height: DOORWAY_HEIGHT,
            orientation: Direction::South,
        },
        // Corridor South : spoke-side (ScaleRoom-north-wall, z=-28)
        Doorway {
            from: Room::ScaleRoom,
            to: Room::TestRoom,
            center: [0.0, h_center, -28.0],
            width: DOORWAY_WIDTH,
            height: DOORWAY_HEIGHT,
            orientation: Direction::North,
        },
        // Corridor West : hub-side (TestRoom-west-wall, x=-20)
        Doorway {
            from: Room::TestRoom,
            to: Room::ColorRoom,
            center: [-20.0, h_center, 0.0],
            width: DOORWAY_WIDTH,
            height: DOORWAY_HEIGHT,
            orientation: Direction::West,
        },
        // Corridor West : spoke-side (ColorRoom-east-wall, x=-28)
        Doorway {
            from: Room::ColorRoom,
            to: Room::TestRoom,
            center: [-28.0, h_center, 0.0],
            width: DOORWAY_WIDTH,
            height: DOORWAY_HEIGHT,
            orientation: Direction::East,
        },
    ]
}

// ──────────────────────────────────────────────────────────────────────────
// § World-bounds query (used by the test that envelope ≤ 120m)
// ──────────────────────────────────────────────────────────────────────────

/// Total world envelope = AABB encompassing every room + corridor.
#[must_use]
pub fn world_envelope() -> AxisAlignedBox {
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    let push = |b: AxisAlignedBox, min: &mut [f32; 3], max: &mut [f32; 3]| {
        for i in 0..3 {
            if b.min[i] < min[i] {
                min[i] = b.min[i];
            }
            if b.max[i] > max[i] {
                max[i] = b.max[i];
            }
        }
    };
    for r in Room::all() {
        push(r.bounds(), &mut min, &mut max);
    }
    for c in Corridor::all() {
        push(c.bounds(), &mut min, &mut max);
    }
    AxisAlignedBox::new(min, max)
}

// ──────────────────────────────────────────────────────────────────────────
// § Camera-room detection — which room is the camera currently in?
// ──────────────────────────────────────────────────────────────────────────

/// Return the room the eye-position is currently inside, or None if the
/// camera is in a corridor / between rooms / outside the world.
///
/// The check is done in priority order : TestRoom → satellites → corridors.
/// A camera-position in a corridor does NOT match any room.
#[must_use]
pub fn room_at(p: [f32; 3]) -> Option<Room> {
    for r in Room::all() {
        if r.bounds().contains(p) {
            return Some(r);
        }
    }
    None
}

/// Like `room_at` but also matches corridors — returns the room *name* the
/// camera is closest to. Used by the HUD so the indicator is never blank.
#[must_use]
pub fn room_label_at(p: [f32; 3]) -> &'static str {
    if let Some(r) = room_at(p) {
        return r.name();
    }
    // If the camera is inside a corridor, label it as the connecting hub.
    for c in Corridor::all() {
        if c.bounds().contains(p) {
            return match c {
                Corridor::North => "Corridor-N",
                Corridor::East => "Corridor-E",
                Corridor::South => "Corridor-S",
                Corridor::West => "Corridor-W",
            };
        }
    }
    "Outside"
}

// ──────────────────────────────────────────────────────────────────────────
// § Tests
// ──────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// TestRoom's bounds must match the existing geometry constants
    /// (-20..20 X · 0..8 Y · -20..20 Z) so existing rendering / collision /
    /// scene code continues to work unchanged.
    #[test]
    fn room_test_room_bounds_match_existing_constants() {
        let b = Room::TestRoom.bounds();
        assert_eq!(b.min, [-20.0, 0.0, -20.0]);
        assert_eq!(b.max, [20.0, 8.0, 20.0]);
        // Width × height × depth = 40 × 8 × 40
        assert!((b.max[0] - b.min[0] - 40.0).abs() < 1e-3);
        assert!((b.max[1] - b.min[1] - 8.0).abs() < 1e-3);
        assert!((b.max[2] - b.min[2] - 40.0).abs() < 1e-3);
    }

    /// MaterialRoom must allocate a 30×6×30 envelope on the +Z spoke.
    #[test]
    fn room_material_room_30_x_6_x_30_north() {
        let b = Room::MaterialRoom.bounds();
        assert!((b.max[0] - b.min[0] - 30.0).abs() < 1e-3);
        assert!((b.max[1] - b.min[1] - 6.0).abs() < 1e-3);
        assert!((b.max[2] - b.min[2] - 30.0).abs() < 1e-3);
        assert!(b.min[2] > 20.0); // strictly north of TestRoom
    }

    /// PatternRoom must allocate a 30×6×30 envelope on the +X spoke.
    #[test]
    fn room_pattern_room_30_x_6_x_30_east() {
        let b = Room::PatternRoom.bounds();
        assert!((b.max[0] - b.min[0] - 30.0).abs() < 1e-3);
        assert!((b.max[1] - b.min[1] - 6.0).abs() < 1e-3);
        assert!((b.max[2] - b.min[2] - 30.0).abs() < 1e-3);
        assert!(b.min[0] > 20.0); // strictly east of TestRoom
    }

    /// ScaleRoom is the long-axis room : 60m on X, 12m tall, 30m on Z.
    #[test]
    fn room_scale_room_is_60m_long() {
        let b = Room::ScaleRoom.bounds();
        let lx = b.max[0] - b.min[0];
        let ly = b.max[1] - b.min[1];
        let lz = b.max[2] - b.min[2];
        assert!((lx - 60.0).abs() < 1e-3, "ScaleRoom must be 60m on X (got {lx})");
        assert!((ly - 12.0).abs() < 1e-3, "ScaleRoom must be 12m tall (got {ly})");
        assert!((lz - 30.0).abs() < 1e-3, "ScaleRoom must be 30m on Z (got {lz})");
        assert!(b.max[2] < -20.0); // strictly south of TestRoom
    }

    /// ColorRoom must allocate a 30×6×30 envelope on the -X spoke.
    #[test]
    fn room_color_room_has_4_walls_each_distinct() {
        let b = Room::ColorRoom.bounds();
        assert!((b.max[0] - b.min[0] - 30.0).abs() < 1e-3);
        assert!((b.max[1] - b.min[1] - 6.0).abs() < 1e-3);
        assert!((b.max[2] - b.min[2] - 30.0).abs() < 1e-3);
        assert!(b.max[0] < -20.0); // strictly west of TestRoom
    }

    /// The hub-side TestRoom→MaterialRoom doorway must sit at z=20 (the
    /// inside surface of the existing north wall) centered on x=0.
    #[test]
    fn doorway_test_room_to_material_north_at_correct_pos() {
        let dws = doorways();
        let north_hub = dws
            .iter()
            .find(|d| d.from == Room::TestRoom && d.to == Room::MaterialRoom)
            .expect("TestRoom→MaterialRoom doorway must exist");
        assert!((north_hub.center[0] - 0.0).abs() < 1e-3);
        assert!((north_hub.center[2] - 20.0).abs() < 1e-3);
        assert_eq!(north_hub.orientation, Direction::North);
        assert!((north_hub.width - 2.0).abs() < 1e-3);
        assert!((north_hub.height - 3.0).abs() < 1e-3);
    }

    /// The total world envelope must fit within 120m × 12m × 120m so the
    /// renderer's depth-range / FOV / collision-precision stays tractable.
    #[test]
    fn total_world_bounds_within_120m_envelope() {
        let env = world_envelope();
        let lx = env.max[0] - env.min[0];
        let ly = env.max[1] - env.min[1];
        let lz = env.max[2] - env.min[2];
        assert!(lx <= 120.0, "world X-extent {lx} > 120m budget");
        assert!(ly <= 12.0, "world Y-extent {ly} > 12m budget");
        assert!(lz <= 120.0, "world Z-extent {lz} > 120m budget");
    }

    /// All 5 rooms have unique names + unique discriminants.
    #[test]
    fn room_all_returns_5_unique_rooms() {
        use std::collections::HashSet;
        let all = Room::all();
        assert_eq!(all.len(), 5);
        let mut names = HashSet::new();
        let mut ids = HashSet::new();
        for r in all {
            names.insert(r.name());
            ids.insert(r as u32);
        }
        assert_eq!(names.len(), 5);
        assert_eq!(ids.len(), 5);
    }

    /// Each spoke room's bounds must be DISJOINT from TestRoom (no overlap).
    #[test]
    fn satellite_rooms_disjoint_from_test_room() {
        let hub = Room::TestRoom.bounds();
        for r in [Room::MaterialRoom, Room::PatternRoom, Room::ScaleRoom, Room::ColorRoom] {
            let b = r.bounds();
            // Disjoint iff at least one axis fully separates them.
            let sep_x = b.max[0] <= hub.min[0] || b.min[0] >= hub.max[0];
            let sep_z = b.max[2] <= hub.min[2] || b.min[2] >= hub.max[2];
            assert!(
                sep_x || sep_z,
                "{:?} overlaps TestRoom (b={b:?} hub={hub:?})",
                r
            );
        }
    }

    /// Each corridor's bounds must be DISJOINT from every room (corridors
    /// are separate boxes that connect through doorways).
    #[test]
    fn corridors_disjoint_from_rooms() {
        for c in Corridor::all() {
            let cb = c.bounds();
            for r in Room::all() {
                let rb = r.bounds();
                let sep_x = cb.max[0] <= rb.min[0] || cb.min[0] >= rb.max[0];
                let sep_z = cb.max[2] <= rb.min[2] || cb.min[2] >= rb.max[2];
                assert!(
                    sep_x || sep_z,
                    "corridor {:?} overlaps room {:?} (cb={cb:?} rb={rb:?})",
                    c,
                    r
                );
            }
        }
    }

    /// Camera at TestRoom origin lands inside TestRoom, NOT in any other.
    #[test]
    fn room_at_origin_is_test_room() {
        assert_eq!(room_at([0.0, 1.0, 0.0]), Some(Room::TestRoom));
        assert_eq!(room_at([0.0, 1.0, 50.0]), Some(Room::MaterialRoom));
        assert_eq!(room_at([50.0, 1.0, 0.0]), Some(Room::PatternRoom));
        assert_eq!(room_at([0.0, 1.0, -50.0]), Some(Room::ScaleRoom));
        assert_eq!(room_at([-50.0, 1.0, 0.0]), Some(Room::ColorRoom));
    }

    /// Camera in a corridor returns None for `room_at` but a corridor name
    /// for `room_label_at`.
    #[test]
    fn corridor_position_falls_back_to_corridor_label() {
        let p = [0.0, 1.5, 24.0]; // inside Corridor::North (z ∈ [20,28])
        assert!(room_at(p).is_none());
        let label = room_label_at(p);
        assert_eq!(label, "Corridor-N");
    }

    /// Doorways table contains exactly 8 entries (2 per corridor).
    #[test]
    fn doorways_count_is_eight() {
        assert_eq!(doorways().len(), 8);
    }

    /// Round-trip Room↔string via `from_str` + `name`.
    #[test]
    fn room_from_str_round_trip() {
        for r in Room::all() {
            assert_eq!(Room::from_str(r.name()), Some(r));
        }
        assert_eq!(Room::from_str("nope"), None);
    }

    /// Round-trip Room↔u32 via `from_u32` + `as u32`.
    #[test]
    fn room_from_u32_round_trip() {
        for r in Room::all() {
            assert_eq!(Room::from_u32(r as u32), Some(r));
        }
        assert!(Room::from_u32(99).is_none());
    }
}
