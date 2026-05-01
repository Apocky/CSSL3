//! § geometry — diagnostic-dense test-room mesh generator.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § T11-LOA-RICH-RENDER (W-LOA-rich-render-overhaul)
//!
//! § ROLE
//!   Builds the vertex + index buffers for the diagnostic-dense test-room :
//!     - 4 walls, each carrying a unique calibration pattern (Macbeth chart ·
//!       Snellen eye chart · QR-aesthetic block · barcode + frequency sweep)
//!     - 4 floor quadrants, each with a different procedural pattern
//!     - Ceiling : grid + emissive cyan label
//!     - 14 plinths, each topped with a UNIQUE stress object
//!
//! § VERTEX
//!   Extended from the stage-0 Vertex {pos,normal,color} to carry per-vertex
//!   `material_id: u32` + `pattern_id: u32` + `uv: vec2` indices into the
//!   material/pattern LUTs in the uniform buffer. The uber-shader reads these
//!   and procedurally emits the final fragment color.
//!
//! § ROOM LAYOUT  (top-down, +X right, +Z forward)
//!
//! ```text
//!     +Z (north · Macbeth)
//!      ┌──────────────┐         The room is 40m × 40m × 8m.
//!      │  NW    NE    │         Quadrants split by X=0 / Z=0.
//!      │ rad-   check │         NE = 1m checkerboard
//!      │ grad   board │         NW = radial-gradient grayscale
//!      ├──────┼───────┤         SW = value-noise pattern
//!      │  SW    SE    │         SE = concentric rings
//!      │ noise  rings │
//!      └──────────────┘
//!     -Z (south · Snellen)  -X (W=barcode) <--> +X (E=QR)
//! ```
//!
//! § WINDING + CULL
//!   Pipeline runs `cull_mode = Some(Face::Back)` with `front_face = Ccw`.
//!   Every face below is hand-audited to ensure CCW from the viewer side :
//!     - Walls : CCW from inside
//!     - Floor : CCW from above
//!     - Ceiling : CCW from below
//!     - Plinth + stress-object boxes : CCW from outside

#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::suboptimal_flops)] // mesh-emit hot-path readability > mul_add micro-opt
#![allow(clippy::too_many_lines)]
#![allow(clippy::float_cmp)] // tests use exact f32 bit-pattern equality

use bytemuck::{Pod, Zeroable};

use crate::material::{
    MAT_BRUSHED_STEEL, MAT_DEEP_INDIGO, MAT_DICHROIC_VIOLET, MAT_EMISSIVE_CYAN,
    MAT_GOLD_LEAF, MAT_GRADIENT_RED, MAT_HAIRY_FUR, MAT_HOLOGRAPHIC, MAT_IRIDESCENT,
    MAT_MATTE_GREY, MAT_NEON_MAGENTA, MAT_OFF_WHITE, MAT_PINK_NOISE_VOL,
    MAT_TRANSPARENT_GLASS, MAT_VERMILLION_LACQUER, MAT_WARM_SKY, MATERIAL_LUT_LEN,
};
use crate::pattern::{
    PAT_CHECKERBOARD, PAT_CONCENTRIC_RINGS, PAT_EAN13_BARCODE, PAT_FREQUENCY_SWEEP,
    PAT_GRADIENT_GRAYSCALE, PAT_GRADIENT_HUE_WHEEL, PAT_GRID_100MM, PAT_GRID_1M,
    PAT_MACBETH_COLOR_CHART, PAT_PERLIN_NOISE, PAT_QR_CODE_STUB, PAT_RADIAL_GRADIENT,
    PAT_RADIAL_SPOKES, PAT_RAYMARCH_GYROID, PAT_RAYMARCH_JULIA, PAT_RAYMARCH_MANDELBULB,
    PAT_RAYMARCH_MENGER, PAT_RAYMARCH_SPHERE, PAT_RAYMARCH_TORUS, PAT_SNELLEN_EYE_CHART,
    PAT_SOLID, PAT_ZONEPLATE,
};
use crate::room::{
    doorways, AxisAlignedBox, Corridor, Direction, Room, CORRIDOR_HEIGHT,
};

// ──────────────────────────────────────────────────────────────────────────
// § Vertex layout (uber-shader compatible)
// ──────────────────────────────────────────────────────────────────────────

/// Single GPU vertex — extended for the diagnostic-dense renderer.
///
/// Bit-pattern : 13 × f32 + 2 × u32 = 52 + 8 = 60 bytes (14-step layout pads
/// out to 64 bytes for clean 16-byte alignment).
///
/// Layout (locations matching `scene.wgsl` VsIn) :
///   0 → position [f32; 3]
///   1 → normal   [f32; 3]
///   2 → color    [f32; 3]   (base tint multiplied by pattern-color)
///   3 → uv       [f32; 2]   (procedural-pattern coord)
///   4 → material_id u32     (LUT index into MATERIAL_LUT)
///   5 → pattern_id  u32     (LUT index into PATTERN_LUT)
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub color: [f32; 3],
    pub uv: [f32; 2],
    pub material_id: u32,
    pub pattern_id: u32,
}

#[cfg(feature = "runtime")]
impl Vertex {
    /// `wgpu::VertexBufferLayout` for the uber-shader pipeline.
    #[must_use]
    pub fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        use std::mem::size_of;
        const POSITION_OFF: u64 = 0;
        const NORMAL_OFF: u64 = 12;
        const COLOR_OFF: u64 = 24;
        const UV_OFF: u64 = 36;
        const MATID_OFF: u64 = 44;
        const PATID_OFF: u64 = 48;
        wgpu::VertexBufferLayout {
            array_stride: size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: POSITION_OFF,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: NORMAL_OFF,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: COLOR_OFF,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: UV_OFF,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: MATID_OFF,
                    shader_location: 4,
                    format: wgpu::VertexFormat::Uint32,
                },
                wgpu::VertexAttribute {
                    offset: PATID_OFF,
                    shader_location: 5,
                    format: wgpu::VertexFormat::Uint32,
                },
            ],
        }
    }
}

/// Bundled CPU-side mesh : vertex buffer + index buffer.
#[derive(Debug, Clone)]
pub struct RoomGeometry {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub plinth_count: u32,
    /// Range of indices in `indices` covering the transparent stress objects.
    /// Used by the renderer to draw transparent geometry in a separate pass.
    pub transparent_index_range: Option<(u32, u32)>,
}

// ──────────────────────────────────────────────────────────────────────────
// § ROOM CONSTANTS
// ──────────────────────────────────────────────────────────────────────────

/// Room half-width (X axis). Total room is 40m, half-width = 20m.
pub const ROOM_HALF_X: f32 = 20.0;
/// Room half-depth (Z axis). Total room is 40m, half-depth = 20m.
pub const ROOM_HALF_Z: f32 = 20.0;
/// Room height. Floor at y=0, ceiling at y=8.
pub const ROOM_HEIGHT: f32 = 8.0;

/// Plinth base size (X / Z half-extents) — 1m wide.
pub const PLINTH_HALF_XZ: f32 = 0.5;
/// Plinth base height — 2m tall.
pub const PLINTH_BASE_H: f32 = 2.0;
/// Plinth gold-cap height — 0.5m tall.
pub const PLINTH_CAP_H: f32 = 0.5;

/// Stress-object cube edge length (sits on top of the plinth's gold cap).
pub const STRESS_SIZE: f32 = 0.8;

// ──────────────────────────────────────────────────────────────────────────
// § PUBLIC ENTRY
// ──────────────────────────────────────────────────────────────────────────

impl RoomGeometry {
    /// Construct the canonical diagnostic-dense test-room mesh.
    #[must_use]
    pub fn test_room() -> Self {
        let mut g = Self {
            vertices: Vec::with_capacity(4096),
            indices: Vec::with_capacity(8192),
            plinth_count: 0,
            transparent_index_range: None,
        };
        g.emit_floor();
        g.emit_ceiling();
        g.emit_walls();
        g.emit_plinths_and_stress();
        g
    }

    /// § T11-LOA-ROOMS · Construct the FULL multi-room test-suite mesh
    /// (TestRoom hub + 4 satellite rooms + 4 corridors + doorways).
    ///
    /// The TestRoom-hub portion is identical to `test_room()` PLUS each
    /// of its 4 walls is rebuilt with a doorway gap. The 4 satellite
    /// rooms each emit their own diagnostic interior and one wall with
    /// a matching doorway. The 4 corridors emit floor + ceiling + 2
    /// side-walls (4m wide × 8m tall × 8m long).
    #[must_use]
    pub fn full_world() -> Self {
        let mut g = Self {
            vertices: Vec::with_capacity(8192),
            indices: Vec::with_capacity(16384),
            plinth_count: 0,
            transparent_index_range: None,
        };
        // 1. TestRoom (hub) — floor + ceiling + 4 walls (with doors) + plinths.
        g.emit_test_room_hub();
        // 2. Satellite rooms.
        g.emit_material_room();
        g.emit_pattern_room();
        g.emit_scale_room();
        g.emit_color_room();
        // 3. Corridors connecting the hub to each spoke.
        g.emit_corridors();
        g
    }
}

// ──────────────────────────────────────────────────────────────────────────
// § PLINTH POSITIONS — 14 total
// ──────────────────────────────────────────────────────────────────────────

/// Returns the 14 plinth (x, z) center positions on the floor.
#[must_use]
pub fn plinth_positions() -> [(f32, f32); 14] {
    [
        (6.0, 6.0),
        (12.0, 12.0),
        (-6.0, 6.0),
        (-12.0, 12.0),
        (-6.0, -6.0),
        (-12.0, -12.0),
        (6.0, -6.0),
        (12.0, -12.0),
        (16.0, 16.0),
        (-16.0, 16.0),
        (-16.0, -16.0),
        (16.0, -16.0),
        (10.0, 0.0),
        (0.0, 10.0),
    ]
}

// ──────────────────────────────────────────────────────────────────────────
// § T11-WAVE3-GLTF · MaterialRoom-Annex marker zone
// ──────────────────────────────────────────────────────────────────────────
//
// The MaterialRoom occupies x ∈ [-15, 15] · z ∈ [28, 58]. The Annex sits
// directly NORTH of it (z > 58) and provides 4 designated landing pads
// for spawned glTF assets. Each pad has its own (x, z) so up to 4
// simultaneously-spawned models stay clearly separated for diagnostic
// inspection. The pads are arranged in a 2×2 grid centered at z=70.

/// Y-coordinate of the Annex floor (matches the room floor at y=0).
pub const ANNEX_FLOOR_Y: f32 = 0.0;

/// Returns the 4 (x, z) marker positions inside the MaterialRoom-Annex
/// zone (north of the MaterialRoom). Returned in deterministic order so
/// telemetry + golden-image tests are stable :
///   0 : NW pad
///   1 : NE pad
///   2 : SW pad
///   3 : SE pad
#[must_use]
pub const fn material_room_annex_marker_positions() -> [(f32, f32); 4] {
    [
        (-7.5, 75.0), // NW
        (7.5, 75.0),  // NE
        (-7.5, 65.0), // SW
        (7.5, 65.0),  // SE
    ]
}

/// World-space center of the MaterialRoom-Annex (drop point for the
/// "default" spawn when no explicit position is provided).
#[must_use]
pub const fn material_room_annex_center() -> [f32; 3] {
    [0.0, ANNEX_FLOOR_Y + 1.0, 70.0]
}

/// Stress-object kind id (0..13).
///
/// § T11-LOA-RAYMARCH (W-LOA-raymarched-primitives) : slots 0..5 now drive
/// the fragment-shader sphere-tracer for true 3D fractal/SDF surfaces ;
/// slots 6..13 stay cube-based with 2D-UV procedurals.
///
/// Slot layout :
///   0 raymarch-mandelbulb + iridescent       (true 3D fractal)
///   1 raymarch-sphere     + brushed-steel    (analytic baseline)
///   2 raymarch-torus      + gold-leaf        (toroidal SDF)
///   3 raymarch-gyroid     + emissive-cyan    (gyroid surface)
///   4 raymarch-julia      + neon-magenta     (quaternion-Julia)
///   5 raymarch-menger     + dichroic-violet  (menger sponge)
///   6 vermillion-classic-cube      (kept ; was slot 13)
///   7 holographic-cube             (kept ; was slot 5)
///   8 transparent-glass-cube       (kept ; was slot 2)
///   9 matte-reference-cube         (kept ; was slot 10)
///   10 pink-noise-cube             (kept ; was slot 12)
///   11 macbeth-cube                (kept ; was slot 8)
///   12 zoneplate-cube              (kept ; was slot 9)
///   13 hairy-fur-cube              (kept ; was slot 3)
#[must_use]
pub const fn stress_object_count() -> u32 {
    14
}

/// Per-stress-object material id.
#[must_use]
pub const fn stress_object_material(kind: u32) -> u32 {
    match kind {
        // § Raymarched (kinds 0..5)
        0 => MAT_IRIDESCENT,
        1 => MAT_BRUSHED_STEEL,
        2 => MAT_GOLD_LEAF,
        3 => MAT_EMISSIVE_CYAN,
        4 => MAT_NEON_MAGENTA,
        5 => MAT_DICHROIC_VIOLET,
        // § Cube-based (kinds 6..13)
        6 => MAT_VERMILLION_LACQUER,
        7 => MAT_HOLOGRAPHIC,
        8 => MAT_TRANSPARENT_GLASS,
        9 => MAT_MATTE_GREY,
        10 => MAT_PINK_NOISE_VOL,
        11 => MAT_GRADIENT_RED,
        12 => MAT_DEEP_INDIGO,
        13 => MAT_HAIRY_FUR,
        _ => MAT_MATTE_GREY,
    }
}

/// Per-stress-object pattern id.
#[must_use]
pub const fn stress_object_pattern(kind: u32) -> u32 {
    match kind {
        // § Raymarched (kinds 0..5) — pattern-id triggers fragment-shader SDF tracer
        0 => PAT_RAYMARCH_MANDELBULB,
        1 => PAT_RAYMARCH_SPHERE,
        2 => PAT_RAYMARCH_TORUS,
        3 => PAT_RAYMARCH_GYROID,
        4 => PAT_RAYMARCH_JULIA,
        5 => PAT_RAYMARCH_MENGER,
        // § Cube-based (kinds 6..13)
        6 => PAT_SOLID,
        7 => PAT_QR_CODE_STUB,
        8 => PAT_SOLID,
        9 => PAT_GRID_1M,
        10 => PAT_PERLIN_NOISE,
        11 => PAT_MACBETH_COLOR_CHART,
        12 => PAT_ZONEPLATE,
        13 => PAT_RADIAL_SPOKES,
        _ => PAT_SOLID,
    }
}

/// Human-readable stress-object name (for HUD).
#[must_use]
pub const fn stress_object_name(kind: u32) -> &'static str {
    match kind {
        0 => "Raymarch-Mandelbulb",
        1 => "Raymarch-Sphere",
        2 => "Raymarch-Torus",
        3 => "Raymarch-Gyroid",
        4 => "Raymarch-Julia",
        5 => "Raymarch-Menger",
        6 => "Vermillion-Classic",
        7 => "Holographic",
        8 => "Glass-Cube",
        9 => "Matte-Reference",
        10 => "Pink-Noise",
        11 => "Macbeth-Cube",
        12 => "Zoneplate-Cube",
        13 => "Hairy-Ball",
        _ => "Unknown",
    }
}

// ──────────────────────────────────────────────────────────────────────────
// § GEOMETRY EMISSION HELPERS
// ──────────────────────────────────────────────────────────────────────────

impl RoomGeometry {
    /// Emit a single quad with explicit per-vertex UVs + uniform material/pattern.
    ///
    /// Corners must be CCW when viewed from the side the normal points toward.
    /// UVs are paired by index (corner i ↔ uv i).
    #[allow(clippy::too_many_arguments)]
    fn emit_quad_uv(
        &mut self,
        corners: [[f32; 3]; 4],
        uvs: [[f32; 2]; 4],
        normal: [f32; 3],
        color: [f32; 3],
        material_id: u32,
        pattern_id: u32,
    ) {
        let base = self.vertices.len() as u32;
        for i in 0..4 {
            self.vertices.push(Vertex {
                position: corners[i],
                normal,
                color,
                uv: uvs[i],
                material_id,
                pattern_id,
            });
        }
        self.indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    /// Convenience : emit a quad with default UVs `[(0,0),(1,0),(1,1),(0,1)]`.
    fn emit_quad(
        &mut self,
        corners: [[f32; 3]; 4],
        normal: [f32; 3],
        color: [f32; 3],
        material_id: u32,
        pattern_id: u32,
    ) {
        self.emit_quad_uv(
            corners,
            [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            normal,
            color,
            material_id,
            pattern_id,
        );
    }

    /// Emit an axis-aligned box with consistent CCW-from-outside winding.
    /// Per-face material/pattern uniform across the whole box.
    ///
    /// § Winding contract : each face's triangle (v0,v1,v2) has a positive
    /// dot product with the stored normal — i.e. cull-mode `Back` shows the
    /// face from outside the box (every test case in `tests::plinth_box_*`
    /// validates this).
    fn emit_box(
        &mut self,
        center: [f32; 3],
        size: [f32; 3],
        color: [f32; 3],
        material_id: u32,
        pattern_id: u32,
    ) {
        let [cx, cy, cz] = center;
        let hx = size[0] * 0.5;
        let hy = size[1] * 0.5;
        let hz = size[2] * 0.5;

        let xn = cx - hx;
        let xp = cx + hx;
        let yn = cy - hy;
        let yp = cy + hy;
        let zn = cz - hz;
        let zp = cz + hz;

        // +X face — stored normal +X. CCW from +X side.
        // Choose : (xp,yn,zn) → (xp,yp,zn) → (xp,yp,zp) → (xp,yn,zp).
        // tri_normal(v0,v1,v2) = (yp-yn,0,0) × (yp-yn,0,zp-zn) = (Δy·Δz, 0, 0) > 0 along +X. ✓
        self.emit_quad(
            [[xp, yn, zn], [xp, yp, zn], [xp, yp, zp], [xp, yn, zp]],
            [1.0, 0.0, 0.0],
            color,
            material_id,
            pattern_id,
        );
        // -X face — stored normal -X.
        // (xn,yn,zp) → (xn,yp,zp) → (xn,yp,zn) → (xn,yn,zn).
        // edge1=(0,Δy,0), edge2=(0,Δy,-Δz), cross=(Δy·(-Δz)-0·Δy, 0, 0) = (-Δy·Δz, 0, 0) ⇒ -X ✓
        self.emit_quad(
            [[xn, yn, zp], [xn, yp, zp], [xn, yp, zn], [xn, yn, zn]],
            [-1.0, 0.0, 0.0],
            color,
            material_id,
            pattern_id,
        );
        // +Y face — stored normal +Y. CCW from above.
        // (xn,yp,zn) → (xn,yp,zp) → (xp,yp,zp) → (xp,yp,zn).
        // edge1=(0,0,Δz), edge2=(Δx,0,Δz), cross=(0·Δz - Δz·0, Δz·Δx - 0·Δz, 0·0 - 0·Δx) = (0, Δx·Δz, 0) ⇒ +Y ✓
        self.emit_quad(
            [[xn, yp, zn], [xn, yp, zp], [xp, yp, zp], [xp, yp, zn]],
            [0.0, 1.0, 0.0],
            color,
            material_id,
            pattern_id,
        );
        // -Y face — stored normal -Y.
        // (xn,yn,zp) → (xn,yn,zn) → (xp,yn,zn) → (xp,yn,zp).
        // edge1=(0,0,-Δz), edge2=(Δx,0,-Δz), cross=(0·(-Δz)-(-Δz)·0, (-Δz)·Δx - 0·(-Δz), 0·0 - 0·Δx) = (0, -Δx·Δz, 0) ⇒ -Y ✓
        self.emit_quad(
            [[xn, yn, zp], [xn, yn, zn], [xp, yn, zn], [xp, yn, zp]],
            [0.0, -1.0, 0.0],
            color,
            material_id,
            pattern_id,
        );
        // +Z face — stored normal +Z. CCW from +Z.
        // (xp,yn,zp) → (xp,yp,zp) → (xn,yp,zp) → (xn,yn,zp).
        // edge1=(0,Δy,0), edge2=(-Δx,Δy,0), cross=(Δy·0 - 0·Δy, 0·(-Δx) - 0·0, 0·Δy - Δy·(-Δx)) = (0, 0, Δx·Δy) ⇒ +Z ✓
        self.emit_quad(
            [[xp, yn, zp], [xp, yp, zp], [xn, yp, zp], [xn, yn, zp]],
            [0.0, 0.0, 1.0],
            color,
            material_id,
            pattern_id,
        );
        // -Z face — stored normal -Z.
        // (xn,yn,zn) → (xn,yp,zn) → (xp,yp,zn) → (xp,yn,zn).
        // edge1=(0,Δy,0), edge2=(Δx,Δy,0), cross=(Δy·0 - 0·Δy, 0·Δx - 0·0, 0·Δy - Δy·Δx) = (0, 0, -Δx·Δy) ⇒ -Z ✓
        self.emit_quad(
            [[xn, yn, zn], [xn, yp, zn], [xp, yp, zn], [xp, yn, zn]],
            [0.0, 0.0, -1.0],
            color,
            material_id,
            pattern_id,
        );
    }

    fn emit_floor(&mut self) {
        let normal = [0.0, 1.0, 0.0];
        let y = 0.0;
        let h = ROOM_HALF_X;
        let white = [1.0, 1.0, 1.0];

        // CCW from above (camera looking -Y) requires tracing with
        // right-handed orientation : viewing the XZ plane from +Y, +X is
        // right but +Z is INTO the screen, so visually-CCW means winding
        // (x,z): (a,a)→(a,b)→(b,b)→(b,a) where the increasing z-step comes
        // before the increasing x-step.

        // NE (+X +Z) — checkerboard
        // (0,0) → (0,h) → (h,h) → (h,0)
        self.emit_quad_uv(
            [[0.0, y, 0.0], [0.0, y, h], [h, y, h], [h, y, 0.0]],
            [[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]],
            normal,
            white,
            MAT_OFF_WHITE,
            PAT_CHECKERBOARD,
        );
        // NW (-X +Z) — radial gradient grayscale
        // (-h,0) → (-h,h) → (0,h) → (0,0)
        self.emit_quad_uv(
            [[-h, y, 0.0], [-h, y, h], [0.0, y, h], [0.0, y, 0.0]],
            [[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]],
            normal,
            white,
            MAT_OFF_WHITE,
            PAT_RADIAL_GRADIENT,
        );
        // SW (-X -Z) — value-noise pattern
        self.emit_quad_uv(
            [[-h, y, -h], [-h, y, 0.0], [0.0, y, 0.0], [0.0, y, -h]],
            [[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]],
            normal,
            white,
            MAT_OFF_WHITE,
            PAT_PERLIN_NOISE,
        );
        // SE (+X -Z) — concentric rings
        self.emit_quad_uv(
            [[0.0, y, -h], [0.0, y, 0.0], [h, y, 0.0], [h, y, -h]],
            [[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]],
            normal,
            white,
            MAT_OFF_WHITE,
            PAT_CONCENTRIC_RINGS,
        );
    }

    fn emit_ceiling(&mut self) {
        let y = ROOM_HEIGHT;
        let h = ROOM_HALF_X;
        let normal = [0.0, -1.0, 0.0];
        let white = [1.0, 1.0, 1.0];
        // Ceiling normal points down (-Y); CCW from below.
        // Looking from -Y up toward +Y : +X right, +Z down-on-screen.
        // CCW from below : (-h,-h) → (h,-h) → (h,h) → (-h,h) in (x,z).
        self.emit_quad_uv(
            [[-h, y, -h], [h, y, -h], [h, y, h], [-h, y, h]],
            [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            normal,
            white,
            MAT_WARM_SKY,
            PAT_GRID_1M,
        );
    }

    fn emit_walls(&mut self) {
        let h = ROOM_HALF_X;
        let top = ROOM_HEIGHT;
        let white = [1.0, 1.0, 1.0];

        // North wall : z = +h, inner-face normal is -Z (pointing into the room).
        // CCW with normal -Z. Verts: BR=(h,0,h) → BL=(-h,0,h) → TL=(-h,top,h) → TR=(h,top,h).
        // edge1=(-2h,0,0), edge2=(-2h,top,0), cross=(0,0,-2h·top) → -Z ✓
        self.emit_quad_uv(
            [[h, 0.0, h], [-h, 0.0, h], [-h, top, h], [h, top, h]],
            [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
            [0.0, 0.0, -1.0],
            white,
            MAT_OFF_WHITE,
            PAT_MACBETH_COLOR_CHART,
        );

        // South wall : z = -h, inner-face normal is +Z.
        // BL=(-h,0,-h) → BR=(h,0,-h) → TR=(h,top,-h) → TL=(-h,top,-h).
        // edge1=(2h,0,0), edge2=(2h,top,0), cross=(0,0,2h·top) → +Z ✓
        self.emit_quad_uv(
            [[-h, 0.0, -h], [h, 0.0, -h], [h, top, -h], [-h, top, -h]],
            [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
            [0.0, 0.0, 1.0],
            white,
            MAT_OFF_WHITE,
            PAT_SNELLEN_EYE_CHART,
        );

        // East wall : x = +h, inner-face normal is -X.
        // verts: (h,0,-h) → (h,0,h) → (h,top,h) → (h,top,-h)
        // edge1=(0,0,2h), edge2=(0,top,2h), cross=(0·2h-2h·top, 2h·0-0·2h, 0·top-0·0)
        //              = (-2h·top, 0, 0) → -X ✓
        self.emit_quad_uv(
            [[h, 0.0, -h], [h, 0.0, h], [h, top, h], [h, top, -h]],
            [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
            [-1.0, 0.0, 0.0],
            white,
            MAT_OFF_WHITE,
            PAT_QR_CODE_STUB,
        );

        // West wall : x = -h, inner-face normal is +X.
        // verts: (-h,0,h) → (-h,0,-h) → (-h,top,-h) → (-h,top,h)
        // edge1=(0,0,-2h), edge2=(0,top,-2h), cross=(0·(-2h)-(-2h)·top, ...)
        //              = (2h·top, 0, 0) → +X ✓
        self.emit_quad_uv(
            [[-h, 0.0, h], [-h, 0.0, -h], [-h, top, -h], [-h, top, h]],
            [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
            [1.0, 0.0, 0.0],
            white,
            MAT_OFF_WHITE,
            PAT_EAN13_BARCODE,
        );

        // Frequency-sweep stripe — accent on west wall.
        // At x = -h + inset, normal +X. Same winding as west wall.
        let inset = 0.05_f32;
        let stripe_y0 = 0.5_f32;
        let stripe_y1 = 1.5_f32;
        self.emit_quad_uv(
            [
                [-h + inset, stripe_y0, h],
                [-h + inset, stripe_y0, -h],
                [-h + inset, stripe_y1, -h],
                [-h + inset, stripe_y1, h],
            ],
            [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
            [1.0, 0.0, 0.0],
            white,
            MAT_OFF_WHITE,
            PAT_FREQUENCY_SWEEP,
        );
    }

    fn emit_plinths_and_stress(&mut self) {
        let positions = plinth_positions();
        for (idx, (x, z)) in positions.iter().enumerate() {
            let kind = idx as u32;
            let stress_mat = stress_object_material(kind);
            let stress_pat = stress_object_pattern(kind);

            // Vermillion base : 1m × 2m × 1m, center at (x, 1, z).
            self.emit_box(
                [*x, PLINTH_BASE_H * 0.5, *z],
                [PLINTH_HALF_XZ * 2.0, PLINTH_BASE_H, PLINTH_HALF_XZ * 2.0],
                [1.0, 1.0, 1.0],
                MAT_VERMILLION_LACQUER,
                PAT_SOLID,
            );
            // Gold cap : 1m × 0.5m × 1m at top of base.
            self.emit_box(
                [*x, PLINTH_BASE_H + PLINTH_CAP_H * 0.5, *z],
                [PLINTH_HALF_XZ * 2.0, PLINTH_CAP_H, PLINTH_HALF_XZ * 2.0],
                [1.0, 1.0, 1.0],
                MAT_GOLD_LEAF,
                PAT_SOLID,
            );

            // Stress object : 0.8m cube on top of cap, center at
            // (x, base_h + cap_h + size/2, z).
            let stress_y = PLINTH_BASE_H + PLINTH_CAP_H + STRESS_SIZE * 0.5;

            // Special-case kind 6 (emissive-ring) : emit a thin disc-cube to
            // suggest ring-form. Stage-0 keeps it as a cube ; the emissive
            // material + the normal map gives the visual.
            // Special-case kind 2 (glass cube) : keep as transparent cube ;
            // material handles alpha.

            // § T11-LOA-RAYMARCH : glass-cube relocated from slot-2 to slot-8
            // (slot-2 now hosts the raymarched torus). Update the index marker.
            let track_transparent = kind == 8;
            let pre_idx = self.indices.len();
            self.emit_box(
                [*x, stress_y, *z],
                [STRESS_SIZE, STRESS_SIZE, STRESS_SIZE],
                [1.0, 1.0, 1.0],
                stress_mat,
                stress_pat,
            );
            if track_transparent {
                let lo = pre_idx as u32;
                let hi = self.indices.len() as u32;
                self.transparent_index_range = Some((lo, hi));
            }

            self.plinth_count += 1;
        }
    }

    // ──────────────────────────────────────────────────────────────────────
    // § T11-LOA-ROOMS · Per-room emission helpers
    // ──────────────────────────────────────────────────────────────────────

    /// TestRoom hub : the original test_room() geometry, but the 4 walls
    /// are rebuilt with doorway gaps cut into them so the player can walk
    /// from the hub into each of the 4 corridors.
    fn emit_test_room_hub(&mut self) {
        // Floor + ceiling + plinths are unchanged from the original
        // test_room() — they don't intersect the doorways.
        self.emit_floor();
        self.emit_ceiling();
        self.emit_walls_with_doorways();
        self.emit_plinths_and_stress();
    }

    /// Re-emit TestRoom's 4 walls with a 2m × 3m doorway gap centered on
    /// each wall. Each "wall" becomes 4 sub-quads :
    ///   left-of-door    ·  right-of-door  ·  lintel-above-door  ·  freq-stripe
    fn emit_walls_with_doorways(&mut self) {
        let h = ROOM_HALF_X;
        let top = ROOM_HEIGHT;
        let white = [1.0, 1.0, 1.0];
        let dh_half = crate::room::DOORWAY_WIDTH * 0.5; // door half-width = 1.0
        let door_h = crate::room::DOORWAY_HEIGHT;       // door height = 3.0

        // North wall : z=+h, inner-face normal -Z.
        // Wall is split horizontally at x ∈ [-dh_half, dh_half] up to y=door_h.
        // 1. Left-of-door : x ∈ [-h, -dh_half], full height
        self.emit_quad_uv(
            [[-dh_half, 0.0, h], [-h, 0.0, h], [-h, top, h], [-dh_half, top, h]],
            [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
            [0.0, 0.0, -1.0],
            white,
            MAT_OFF_WHITE,
            PAT_MACBETH_COLOR_CHART,
        );
        // 2. Right-of-door : x ∈ [dh_half, h], full height
        self.emit_quad_uv(
            [[h, 0.0, h], [dh_half, 0.0, h], [dh_half, top, h], [h, top, h]],
            [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
            [0.0, 0.0, -1.0],
            white,
            MAT_OFF_WHITE,
            PAT_MACBETH_COLOR_CHART,
        );
        // 3. Lintel above door : x ∈ [-dh_half, dh_half], y ∈ [door_h, top]
        self.emit_quad_uv(
            [[dh_half, door_h, h], [-dh_half, door_h, h], [-dh_half, top, h], [dh_half, top, h]],
            [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
            [0.0, 0.0, -1.0],
            white,
            MAT_OFF_WHITE,
            PAT_MACBETH_COLOR_CHART,
        );

        // South wall : z=-h, inner-face normal +Z.
        // 1. Left-of-door : x ∈ [-h, -dh_half] (when looking from inside, left
        //    is on +X side because we're facing -Z. Use the same "left of door"
        //    via x-coords, winding stays CCW from inside.)
        self.emit_quad_uv(
            [[-h, 0.0, -h], [-dh_half, 0.0, -h], [-dh_half, top, -h], [-h, top, -h]],
            [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
            [0.0, 0.0, 1.0],
            white,
            MAT_OFF_WHITE,
            PAT_SNELLEN_EYE_CHART,
        );
        // 2. Right-of-door
        self.emit_quad_uv(
            [[dh_half, 0.0, -h], [h, 0.0, -h], [h, top, -h], [dh_half, top, -h]],
            [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
            [0.0, 0.0, 1.0],
            white,
            MAT_OFF_WHITE,
            PAT_SNELLEN_EYE_CHART,
        );
        // 3. Lintel
        self.emit_quad_uv(
            [[-dh_half, door_h, -h], [dh_half, door_h, -h], [dh_half, top, -h], [-dh_half, top, -h]],
            [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
            [0.0, 0.0, 1.0],
            white,
            MAT_OFF_WHITE,
            PAT_SNELLEN_EYE_CHART,
        );

        // East wall : x=+h, inner-face normal -X.
        // 1. Left-of-door : z ∈ [-h, -dh_half]
        self.emit_quad_uv(
            [[h, 0.0, -h], [h, 0.0, -dh_half], [h, top, -dh_half], [h, top, -h]],
            [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
            [-1.0, 0.0, 0.0],
            white,
            MAT_OFF_WHITE,
            PAT_QR_CODE_STUB,
        );
        // 2. Right-of-door : z ∈ [dh_half, h]
        self.emit_quad_uv(
            [[h, 0.0, dh_half], [h, 0.0, h], [h, top, h], [h, top, dh_half]],
            [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
            [-1.0, 0.0, 0.0],
            white,
            MAT_OFF_WHITE,
            PAT_QR_CODE_STUB,
        );
        // 3. Lintel
        self.emit_quad_uv(
            [[h, door_h, -dh_half], [h, door_h, dh_half], [h, top, dh_half], [h, top, -dh_half]],
            [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
            [-1.0, 0.0, 0.0],
            white,
            MAT_OFF_WHITE,
            PAT_QR_CODE_STUB,
        );

        // West wall : x=-h, inner-face normal +X.
        // 1. Left-of-door : z ∈ [dh_half, h]
        self.emit_quad_uv(
            [[-h, 0.0, h], [-h, 0.0, dh_half], [-h, top, dh_half], [-h, top, h]],
            [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
            [1.0, 0.0, 0.0],
            white,
            MAT_OFF_WHITE,
            PAT_EAN13_BARCODE,
        );
        // 2. Right-of-door : z ∈ [-h, -dh_half]
        self.emit_quad_uv(
            [[-h, 0.0, -dh_half], [-h, 0.0, -h], [-h, top, -h], [-h, top, -dh_half]],
            [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
            [1.0, 0.0, 0.0],
            white,
            MAT_OFF_WHITE,
            PAT_EAN13_BARCODE,
        );
        // 3. Lintel
        self.emit_quad_uv(
            [[-h, door_h, dh_half], [-h, door_h, -dh_half], [-h, top, -dh_half], [-h, top, dh_half]],
            [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
            [1.0, 0.0, 0.0],
            white,
            MAT_OFF_WHITE,
            PAT_EAN13_BARCODE,
        );

        // Frequency-sweep stripe — accent on west wall (unchanged).
        let inset = 0.05_f32;
        let stripe_y0 = 0.5_f32;
        let stripe_y1 = 1.5_f32;
        self.emit_quad_uv(
            [
                [-h + inset, stripe_y0, h],
                [-h + inset, stripe_y0, -h],
                [-h + inset, stripe_y1, -h],
                [-h + inset, stripe_y1, h],
            ],
            [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
            [1.0, 0.0, 0.0],
            white,
            MAT_OFF_WHITE,
            PAT_FREQUENCY_SWEEP,
        );
    }

    /// MaterialRoom : 30×6×30m room at z ∈ [28, 58]. Floor + ceiling + 4
    /// walls (south wall has a 2m × 3m doorway connecting to corridor-N) +
    /// 16 hovering material spheres in a 4×4 grid (one material per sphere).
    fn emit_material_room(&mut self) {
        let b = Room::MaterialRoom.bounds();
        // Floor (off-white grid)
        self.emit_room_floor(b, MAT_MATTE_GREY, PAT_GRID_1M);
        // Ceiling
        self.emit_room_ceiling(b, MAT_WARM_SKY, PAT_SOLID);
        // 4 walls — south wall (z=28) has a doorway for corridor-N.
        self.emit_room_wall_with_door(b, Direction::South, MAT_OFF_WHITE, PAT_SOLID, true);
        self.emit_room_wall_with_door(b, Direction::North, MAT_OFF_WHITE, PAT_GRADIENT_GRAYSCALE, false);
        self.emit_room_wall_with_door(b, Direction::East, MAT_OFF_WHITE, PAT_GRADIENT_GRAYSCALE, false);
        self.emit_room_wall_with_door(b, Direction::West, MAT_OFF_WHITE, PAT_GRADIENT_GRAYSCALE, false);

        // 16 spheres (rendered as 1.5m-radius cubes for stage-0 — same
        // approximation used by the diagnostic stress objects). Layout :
        // 4 × 4 grid centered on the room, spaced 6m apart.
        let cx = b.center()[0];
        let cz = b.center()[2];
        let radius = 1.5_f32;
        let spacing = 6.0_f32;
        let sphere_y = 3.0_f32; // hover at room-center y
        for i in 0..4 {
            for j in 0..4 {
                let id = (i * 4 + j) as u32;
                let mat = id % MATERIAL_LUT_LEN as u32;
                let dx = (i as f32 - 1.5) * spacing;
                let dz = (j as f32 - 1.5) * spacing;
                let pos = [cx + dx, sphere_y, cz + dz];
                self.emit_box(
                    pos,
                    [radius * 2.0, radius * 2.0, radius * 2.0],
                    [1.0, 1.0, 1.0],
                    mat,
                    PAT_SOLID,
                );
            }
        }

        // § T11-LOA-USERFIX : render-mode demo wall — 10 panels on the
        // south wall (one per F1-F10 mode) so Apocky can walk past each
        // panel and see what each render-mode does at-a-glance. Each
        // panel uses MAT_OFF_WHITE + a unique pattern that makes the
        // mode's effect visually distinct (Macbeth · Snellen · QR ·
        // EAN13 · gradient · grid · zoneplate · spokes · rings ·
        // hue-wheel). The panels are 2 m × 2 m × ~0.05 m thick and sit
        // at z = b.min[2] + 0.05 (just inside the south wall).
        //
        // Layout : 10 panels along the 30 m-wide south wall, centers at
        //   -13.5 -10.5 -7.5 -4.5 -1.5 +1.5 +4.5 +7.5 +10.5 +13.5
        // (3 m spacing, 12 m below ceiling-line so y_lo=0.5, y_hi=2.5).
        let z_demo = b.min[2] + 0.05;
        let demo_y_lo = 0.5_f32;
        let demo_y_hi = 2.5_f32;
        let demo_w = 2.0_f32;
        let demo_centers = [
            -13.5_f32, -10.5, -7.5, -4.5, -1.5, 1.5, 4.5, 7.5, 10.5, 13.5,
        ];
        // One pattern per panel — visually distinctive across F1..F10.
        let demo_patterns = [
            crate::pattern::PAT_GRID_1M,
            crate::pattern::PAT_CHECKERBOARD,
            crate::pattern::PAT_MACBETH_COLOR_CHART,
            crate::pattern::PAT_SNELLEN_EYE_CHART,
            crate::pattern::PAT_QR_CODE_STUB,
            crate::pattern::PAT_EAN13_BARCODE,
            crate::pattern::PAT_GRADIENT_GRAYSCALE,
            crate::pattern::PAT_GRADIENT_HUE_WHEEL,
            crate::pattern::PAT_ZONEPLATE,
            crate::pattern::PAT_RADIAL_SPOKES,
        ];
        for (cx_p, &pat) in demo_centers.iter().zip(demo_patterns.iter()) {
            let xn = cx_p - demo_w * 0.5;
            let xp = cx_p + demo_w * 0.5;
            // Normal points +Z (into the room), CCW from +Z side :
            self.emit_quad_uv(
                [
                    [xn, demo_y_lo, z_demo],
                    [xp, demo_y_lo, z_demo],
                    [xp, demo_y_hi, z_demo],
                    [xn, demo_y_hi, z_demo],
                ],
                [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
                [0.0, 0.0, 1.0],
                [1.0, 1.0, 1.0],
                MAT_OFF_WHITE,
                pat,
            );
        }

        // § T11-LOA-FID-MAINSTREAM : 4 HDR-test emissive panels on the north
        // wall, with intensities 1× / 4× / 16× / 64× the base unit. Each
        // panel is a 2 m × 2 m quad at z = b.max[2] - 0.05 (slightly inside
        // the wall to avoid z-fighting), spaced evenly across the wall. The
        // ACES tonemap should compress the brightest panel without clipping
        // — a striking visual confirmation that HDR is engaged.
        //
        // We achieve "4× / 16× / 64×" without material-LUT pollution by
        // tinting each panel via vertex `color` (uniform multiplier in the
        // uber-shader's `albedo = m.albedo * pat_col * in.base_color`). The
        // base material is `MAT_EMISSIVE_CYAN` whose emissive is already
        // ~1.6 ; multiplied by 1/4/16/64 we land at the four target stops.
        let z_panel = b.max[2] - 0.05;
        let y_lo = 1.0_f32;
        let y_hi = 3.0_f32;
        let panel_w = 2.0_f32;
        // Spread across the 30 m-wide north wall : centers at -10, -3, +3, +10.
        let centers = [-10.0_f32, -3.0, 3.0, 10.0];
        let intensities = [1.0_f32, 4.0, 16.0, 64.0];
        for (cx_p, intensity) in centers.iter().zip(intensities.iter()) {
            let xn = cx_p - panel_w * 0.5;
            let xp = cx_p + panel_w * 0.5;
            // Normal points -Z (into the room) so back-face cull keeps the
            // outside-of-room face hidden. CCW from -Z side :
            //   (xp, y_lo, z) → (xn, y_lo, z) → (xn, y_hi, z) → (xp, y_hi, z)
            let tint = [*intensity, *intensity, *intensity];
            self.emit_quad_uv(
                [
                    [xp, y_lo, z_panel],
                    [xn, y_lo, z_panel],
                    [xn, y_hi, z_panel],
                    [xp, y_hi, z_panel],
                ],
                [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
                [0.0, 0.0, -1.0],
                tint,
                MAT_EMISSIVE_CYAN,
                PAT_SOLID,
            );
        }
    }

    /// PatternRoom : 30×6×30m room at x ∈ [28, 58]. Floor is divided into
    /// 16 squares (4×4) each rendering a different procedural pattern.
    /// 4 walls — west wall (x=28) has a doorway for corridor-E.
    fn emit_pattern_room(&mut self) {
        let b = Room::PatternRoom.bounds();
        // Floor : 16 quads, one per pattern. Iterate in a 4×4 grid.
        let normal = [0.0, 1.0, 0.0];
        let y = 0.0;
        let xmin = b.min[0];
        let zmin = b.min[2];
        let lx = b.max[0] - b.min[0];
        let lz = b.max[2] - b.min[2];
        let dx = lx / 4.0;
        let dz = lz / 4.0;
        for i in 0..4 {
            for j in 0..4 {
                let id = (i * 4 + j) as u32;
                let pat = id % crate::pattern::PATTERN_LUT_LEN as u32;
                let x0 = xmin + i as f32 * dx;
                let x1 = x0 + dx;
                let z0 = zmin + j as f32 * dz;
                let z1 = z0 + dz;
                self.emit_quad_uv(
                    [[x0, y, z0], [x0, y, z1], [x1, y, z1], [x1, y, z0]],
                    [[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]],
                    normal,
                    [1.0, 1.0, 1.0],
                    MAT_MATTE_GREY,
                    pat,
                );
            }
        }
        // Ceiling
        self.emit_room_ceiling(b, MAT_OFF_WHITE, PAT_GRID_1M);
        // 4 walls — west wall (x=28) has the door from corridor-E.
        self.emit_room_wall_with_door(b, Direction::West, MAT_OFF_WHITE, PAT_SOLID, true);
        self.emit_room_wall_with_door(b, Direction::East, MAT_OFF_WHITE, PAT_GRID_100MM, false);
        self.emit_room_wall_with_door(b, Direction::North, MAT_OFF_WHITE, PAT_GRID_100MM, false);
        self.emit_room_wall_with_door(b, Direction::South, MAT_OFF_WHITE, PAT_GRID_100MM, false);
    }

    /// ScaleRoom : 60×12×30m at z ∈ [-58, -28]. Long axis is X. Reference
    /// markers at heights 1m·2m·3m·5m·10m every 5m along X. Grid floor.
    /// 4 walls — north wall (z=-28) has a doorway for corridor-S.
    fn emit_scale_room(&mut self) {
        let b = Room::ScaleRoom.bounds();
        // Floor : full 60×30 grid-1m
        self.emit_room_floor(b, MAT_MATTE_GREY, PAT_GRID_1M);
        // Ceiling
        self.emit_room_ceiling(b, MAT_OFF_WHITE, PAT_SOLID);
        // Walls — north wall (z=-28) has the door from corridor-S.
        self.emit_room_wall_with_door(b, Direction::North, MAT_OFF_WHITE, PAT_SOLID, true);
        self.emit_room_wall_with_door(b, Direction::South, MAT_OFF_WHITE, PAT_GRID_1M, false);
        self.emit_room_wall_with_door(b, Direction::East, MAT_OFF_WHITE, PAT_GRID_1M, false);
        self.emit_room_wall_with_door(b, Direction::West, MAT_OFF_WHITE, PAT_GRID_1M, false);

        // Height-reference towers : at every 5m along X, place a column of
        // boxes at heights 1m, 2m, 3m, 5m, 10m. Each is a 0.5m × Hm × 0.5m
        // pillar. Z-position = b.min[2] + 5.0 (a row near the south wall).
        let z_pos = b.min[2] + 5.0;
        let heights = [1.0_f32, 2.0, 3.0, 5.0, 10.0];
        let mat_palette = [
            MAT_VERMILLION_LACQUER,
            MAT_GOLD_LEAF,
            MAT_BRUSHED_STEEL,
            MAT_DICHROIC_VIOLET,
            MAT_EMISSIVE_CYAN,
        ];
        let mut x_pos = b.min[0] + 5.0;
        let mut idx = 0;
        while x_pos < b.max[0] - 4.0 {
            let h = heights[idx % heights.len()];
            let mat = mat_palette[idx % mat_palette.len()];
            self.emit_box(
                [x_pos, h * 0.5, z_pos],
                [0.5, h, 0.5],
                [1.0, 1.0, 1.0],
                mat,
                PAT_SOLID,
            );
            x_pos += 5.0;
            idx += 1;
        }
    }

    /// ColorRoom : 30×6×30m at x ∈ [-58, -28]. Walls + floor + ceiling all
    /// render different color-spaces : sRGB ramp on floor, linear ramp on
    /// ceiling, gradient HSV on walls.
    /// East wall (x=-28) has the doorway for corridor-W.
    fn emit_color_room(&mut self) {
        let b = Room::ColorRoom.bounds();
        // Floor : sRGB grayscale ramp
        self.emit_room_floor(b, MAT_MATTE_GREY, PAT_GRADIENT_GRAYSCALE);
        // Ceiling : linear ramp (rendered as same gradient in stage-0)
        self.emit_room_ceiling(b, MAT_OFF_WHITE, PAT_GRADIENT_HUE_WHEEL);
        // 4 walls — east wall (x=-28) has the door from corridor-W.
        // Each wall gets a different color gradient pattern :
        //   N (z=+15) : hue gradient
        //   E (x=-28) : door-side · saturation gradient
        //   S (z=-15) : value gradient
        //   W (x=-58) : Macbeth chart for direct comparison
        self.emit_room_wall_with_door(b, Direction::East, MAT_OFF_WHITE, PAT_GRADIENT_HUE_WHEEL, true);
        self.emit_room_wall_with_door(b, Direction::North, MAT_OFF_WHITE, PAT_GRADIENT_HUE_WHEEL, false);
        self.emit_room_wall_with_door(b, Direction::South, MAT_OFF_WHITE, PAT_GRADIENT_GRAYSCALE, false);
        self.emit_room_wall_with_door(b, Direction::West, MAT_OFF_WHITE, PAT_MACBETH_COLOR_CHART, false);
    }

    /// Emit floor for a room (a single quad covering the room footprint).
    /// Floor normal = +Y. CCW from above.
    fn emit_room_floor(&mut self, b: AxisAlignedBox, mat: u32, pat: u32) {
        let xn = b.min[0];
        let xp = b.max[0];
        let zn = b.min[2];
        let zp = b.max[2];
        let y = b.min[1];
        // CCW from above : (xn, zn) → (xn, zp) → (xp, zp) → (xp, zn).
        self.emit_quad_uv(
            [[xn, y, zn], [xn, y, zp], [xp, y, zp], [xp, y, zn]],
            [[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]],
            [0.0, 1.0, 0.0],
            [1.0, 1.0, 1.0],
            mat,
            pat,
        );
    }

    /// Emit ceiling for a room (a single quad). Normal = -Y. CCW from below.
    fn emit_room_ceiling(&mut self, b: AxisAlignedBox, mat: u32, pat: u32) {
        let xn = b.min[0];
        let xp = b.max[0];
        let zn = b.min[2];
        let zp = b.max[2];
        let y = b.max[1];
        self.emit_quad_uv(
            [[xn, y, zn], [xp, y, zn], [xp, y, zp], [xn, y, zp]],
            [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            [0.0, -1.0, 0.0],
            [1.0, 1.0, 1.0],
            mat,
            pat,
        );
    }

    /// Emit one wall of a room. If `with_door=true`, cuts a 2m × 3m gap in
    /// the wall centered on the wall-axis (wall-x for N/S walls, wall-z for
    /// E/W walls). Walls face INWARD (CCW from inside the room).
    fn emit_room_wall_with_door(
        &mut self,
        b: AxisAlignedBox,
        dir: Direction,
        mat: u32,
        pat: u32,
        with_door: bool,
    ) {
        let dh_half = crate::room::DOORWAY_WIDTH * 0.5;
        let door_h = crate::room::DOORWAY_HEIGHT;
        let yb = b.min[1]; // floor
        let yt = b.max[1]; // ceiling
        let white = [1.0, 1.0, 1.0];

        match dir {
            Direction::North => {
                // North wall : z = b.max[2], inner-normal = -Z. CCW from -Z side.
                let z = b.max[2];
                let xn = b.min[0];
                let xp = b.max[0];
                if !with_door {
                    self.emit_quad_uv(
                        [[xp, yb, z], [xn, yb, z], [xn, yt, z], [xp, yt, z]],
                        [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
                        [0.0, 0.0, -1.0],
                        white,
                        mat,
                        pat,
                    );
                } else {
                    // Door centered on x=cx, width 2m, height 3m.
                    let cx = (xn + xp) * 0.5;
                    let dxn = cx - dh_half;
                    let dxp = cx + dh_half;
                    // Left of door : x ∈ [xn, dxn]
                    self.emit_quad_uv(
                        [[dxn, yb, z], [xn, yb, z], [xn, yt, z], [dxn, yt, z]],
                        [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
                        [0.0, 0.0, -1.0],
                        white, mat, pat,
                    );
                    // Right of door : x ∈ [dxp, xp]
                    self.emit_quad_uv(
                        [[xp, yb, z], [dxp, yb, z], [dxp, yt, z], [xp, yt, z]],
                        [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
                        [0.0, 0.0, -1.0],
                        white, mat, pat,
                    );
                    // Lintel : x ∈ [dxn, dxp], y ∈ [door_h, yt]
                    self.emit_quad_uv(
                        [[dxp, door_h, z], [dxn, door_h, z], [dxn, yt, z], [dxp, yt, z]],
                        [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
                        [0.0, 0.0, -1.0],
                        white, mat, pat,
                    );
                }
            }
            Direction::South => {
                // South wall : z = b.min[2], inner-normal = +Z. CCW from +Z side.
                let z = b.min[2];
                let xn = b.min[0];
                let xp = b.max[0];
                if !with_door {
                    self.emit_quad_uv(
                        [[xn, yb, z], [xp, yb, z], [xp, yt, z], [xn, yt, z]],
                        [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
                        [0.0, 0.0, 1.0],
                        white,
                        mat,
                        pat,
                    );
                } else {
                    let cx = (xn + xp) * 0.5;
                    let dxn = cx - dh_half;
                    let dxp = cx + dh_half;
                    // Left of door
                    self.emit_quad_uv(
                        [[xn, yb, z], [dxn, yb, z], [dxn, yt, z], [xn, yt, z]],
                        [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
                        [0.0, 0.0, 1.0],
                        white, mat, pat,
                    );
                    // Right of door
                    self.emit_quad_uv(
                        [[dxp, yb, z], [xp, yb, z], [xp, yt, z], [dxp, yt, z]],
                        [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
                        [0.0, 0.0, 1.0],
                        white, mat, pat,
                    );
                    // Lintel
                    self.emit_quad_uv(
                        [[dxn, door_h, z], [dxp, door_h, z], [dxp, yt, z], [dxn, yt, z]],
                        [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
                        [0.0, 0.0, 1.0],
                        white, mat, pat,
                    );
                }
            }
            Direction::East => {
                // East wall : x = b.max[0], inner-normal = -X. CCW from -X side.
                let x = b.max[0];
                let zn = b.min[2];
                let zp = b.max[2];
                if !with_door {
                    self.emit_quad_uv(
                        [[x, yb, zn], [x, yb, zp], [x, yt, zp], [x, yt, zn]],
                        [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
                        [-1.0, 0.0, 0.0],
                        white,
                        mat,
                        pat,
                    );
                } else {
                    let cz = (zn + zp) * 0.5;
                    let dzn = cz - dh_half;
                    let dzp = cz + dh_half;
                    // Left of door (z ∈ [zn, dzn])
                    self.emit_quad_uv(
                        [[x, yb, zn], [x, yb, dzn], [x, yt, dzn], [x, yt, zn]],
                        [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
                        [-1.0, 0.0, 0.0],
                        white, mat, pat,
                    );
                    // Right of door
                    self.emit_quad_uv(
                        [[x, yb, dzp], [x, yb, zp], [x, yt, zp], [x, yt, dzp]],
                        [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
                        [-1.0, 0.0, 0.0],
                        white, mat, pat,
                    );
                    // Lintel
                    self.emit_quad_uv(
                        [[x, door_h, dzn], [x, door_h, dzp], [x, yt, dzp], [x, yt, dzn]],
                        [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
                        [-1.0, 0.0, 0.0],
                        white, mat, pat,
                    );
                }
            }
            Direction::West => {
                // West wall : x = b.min[0], inner-normal = +X. CCW from +X side.
                let x = b.min[0];
                let zn = b.min[2];
                let zp = b.max[2];
                if !with_door {
                    self.emit_quad_uv(
                        [[x, yb, zp], [x, yb, zn], [x, yt, zn], [x, yt, zp]],
                        [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
                        [1.0, 0.0, 0.0],
                        white,
                        mat,
                        pat,
                    );
                } else {
                    let cz = (zn + zp) * 0.5;
                    let dzn = cz - dh_half;
                    let dzp = cz + dh_half;
                    // Left of door (z ∈ [dzp, zp])
                    self.emit_quad_uv(
                        [[x, yb, zp], [x, yb, dzp], [x, yt, dzp], [x, yt, zp]],
                        [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
                        [1.0, 0.0, 0.0],
                        white, mat, pat,
                    );
                    // Right of door
                    self.emit_quad_uv(
                        [[x, yb, dzn], [x, yb, zn], [x, yt, zn], [x, yt, dzn]],
                        [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
                        [1.0, 0.0, 0.0],
                        white, mat, pat,
                    );
                    // Lintel
                    self.emit_quad_uv(
                        [[x, door_h, dzp], [x, door_h, dzn], [x, yt, dzn], [x, yt, dzp]],
                        [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
                        [1.0, 0.0, 0.0],
                        white, mat, pat,
                    );
                }
            }
        }
    }

    /// Emit floor + ceiling + 2 side walls for each of the 4 corridors.
    /// Corridors don't have end walls (those are the room walls with the
    /// doorways). Each corridor is 4m wide × 8m tall × 8m long.
    fn emit_corridors(&mut self) {
        for c in Corridor::all() {
            let b = c.bounds();
            // Floor + ceiling (full corridor footprint)
            self.emit_room_floor(b, MAT_MATTE_GREY, PAT_GRID_1M);
            self.emit_room_ceiling(b, MAT_OFF_WHITE, PAT_GRID_1M);
            // Side walls (the long sides of the corridor)
            match c {
                Corridor::North | Corridor::South => {
                    // Corridor runs along Z ; side walls are at x=b.min[0] and
                    // x=b.max[0] (extending the full Z range of the corridor).
                    self.emit_room_wall_with_door(b, Direction::East, MAT_OFF_WHITE, PAT_GRID_100MM, false);
                    self.emit_room_wall_with_door(b, Direction::West, MAT_OFF_WHITE, PAT_GRID_100MM, false);
                }
                Corridor::East | Corridor::West => {
                    // Corridor runs along X ; side walls are at z=b.min[2] and
                    // z=b.max[2].
                    self.emit_room_wall_with_door(b, Direction::North, MAT_OFF_WHITE, PAT_GRID_100MM, false);
                    self.emit_room_wall_with_door(b, Direction::South, MAT_OFF_WHITE, PAT_GRID_100MM, false);
                }
            }
        }
        // Suppress unused-import warning when the corridor heights happen to
        // already match ROOM_HEIGHT (8m) ; we keep the import for clarity.
        let _ = CORRIDOR_HEIGHT;
        let _ = doorways;
    }
}

// ──────────────────────────────────────────────────────────────────────────
// § TESTS
// ──────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Floor=4 quads · ceiling=1 quad · walls=4+1 (1 frequency-sweep stripe) ·
    /// plinths=14 × (base+cap+stress)=14 × 3 boxes = 42 boxes × 6 quads = 252.
    /// Total quads = 4 + 1 + 5 + 252 = 262. Each quad = 4 verts.
    #[test]
    fn room_geometry_has_expected_vertex_count() {
        let g = RoomGeometry::test_room();
        let expected_quads: usize = 4 + 1 + 5 + 14 * 3 * 6;
        assert_eq!(g.vertices.len(), expected_quads * 4);
        assert_eq!(g.indices.len(), expected_quads * 6);
    }

    #[test]
    fn plinth_count_is_14() {
        let g = RoomGeometry::test_room();
        assert_eq!(g.plinth_count, 14);
        assert_eq!(plinth_positions().len(), 14);
    }

    #[test]
    fn plinth_positions_are_inside_room() {
        for (x, z) in plinth_positions() {
            assert!(x.abs() <= ROOM_HALF_X);
            assert!(z.abs() <= ROOM_HALF_Z);
        }
    }

    #[test]
    fn vertex_struct_is_pod() {
        let v: Vertex = bytemuck::Zeroable::zeroed();
        assert_eq!(v.position, [0.0, 0.0, 0.0]);
        assert_eq!(v.normal, [0.0, 0.0, 0.0]);
        assert_eq!(v.material_id, 0);
        assert_eq!(v.pattern_id, 0);
        assert_eq!(v.uv, [0.0, 0.0]);
    }

    #[test]
    fn geometry_vertex_carries_material_id_field() {
        let v = Vertex {
            position: [0.0; 3],
            normal: [0.0, 1.0, 0.0],
            color: [1.0; 3],
            uv: [0.0; 2],
            material_id: 42,
            pattern_id: 0,
        };
        assert_eq!(v.material_id, 42);
    }

    #[test]
    fn geometry_vertex_carries_pattern_id_field() {
        let v = Vertex {
            position: [0.0; 3],
            normal: [0.0, 1.0, 0.0],
            color: [1.0; 3],
            uv: [0.0; 2],
            material_id: 0,
            pattern_id: 7,
        };
        assert_eq!(v.pattern_id, 7);
    }

    // ─── Winding tests : verify CCW from the expected viewer side ───

    /// Helper : compute the geometric normal of a triangle ABC. Positive
    /// dot product with the expected normal ⇒ winding is CCW from that side.
    fn tri_normal(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> [f32; 3] {
        let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        [
            ab[1] * ac[2] - ab[2] * ac[1],
            ab[2] * ac[0] - ab[0] * ac[2],
            ab[0] * ac[1] - ab[1] * ac[0],
        ]
    }

    fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
        a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
    }

    /// Find the first quad in the mesh with the given (approximate) normal.
    /// Returns the 4 corner positions in their stored order.
    fn first_quad_with_normal(g: &RoomGeometry, expected_normal: [f32; 3]) -> [[f32; 3]; 4] {
        let mut i = 0;
        while i < g.vertices.len() {
            let v0 = g.vertices[i];
            if (v0.normal[0] - expected_normal[0]).abs() < 1e-3
                && (v0.normal[1] - expected_normal[1]).abs() < 1e-3
                && (v0.normal[2] - expected_normal[2]).abs() < 1e-3
            {
                return [
                    g.vertices[i].position,
                    g.vertices[i + 1].position,
                    g.vertices[i + 2].position,
                    g.vertices[i + 3].position,
                ];
            }
            i += 4;
        }
        panic!("no quad with normal {expected_normal:?}");
    }

    /// Find first quad with given normal AND first position-coordinate match
    /// (used to disambiguate walls + floor).
    fn first_quad_with_normal_and_axis_value(
        g: &RoomGeometry,
        expected_normal: [f32; 3],
        axis: usize,
        value: f32,
    ) -> [[f32; 3]; 4] {
        let mut i = 0;
        while i < g.vertices.len() {
            let v0 = g.vertices[i];
            if (v0.normal[0] - expected_normal[0]).abs() < 1e-3
                && (v0.normal[1] - expected_normal[1]).abs() < 1e-3
                && (v0.normal[2] - expected_normal[2]).abs() < 1e-3
                && (v0.position[axis] - value).abs() < 0.5
            {
                return [
                    g.vertices[i].position,
                    g.vertices[i + 1].position,
                    g.vertices[i + 2].position,
                    g.vertices[i + 3].position,
                ];
            }
            i += 4;
        }
        panic!(
            "no quad with normal {expected_normal:?} at axis {axis}={value}"
        );
    }

    #[test]
    fn wall_winding_north_ccw_from_inside() {
        // North wall : z=+20, inner normal -Z. The triangle (0,1,2) of the
        // quad must have a face-normal whose dot with -Z is positive.
        let g = RoomGeometry::test_room();
        let q = first_quad_with_normal_and_axis_value(&g, [0.0, 0.0, -1.0], 2, 20.0);
        let n = tri_normal(q[0], q[1], q[2]);
        let d = dot(n, [0.0, 0.0, -1.0]);
        assert!(d > 0.0, "north wall winding must be CCW from inside (n={n:?})");
    }

    #[test]
    fn wall_winding_south_ccw_from_inside() {
        let g = RoomGeometry::test_room();
        let q = first_quad_with_normal_and_axis_value(&g, [0.0, 0.0, 1.0], 2, -20.0);
        let n = tri_normal(q[0], q[1], q[2]);
        let d = dot(n, [0.0, 0.0, 1.0]);
        assert!(d > 0.0, "south wall winding must be CCW from inside (n={n:?})");
    }

    #[test]
    fn wall_winding_east_ccw_from_inside() {
        let g = RoomGeometry::test_room();
        let q = first_quad_with_normal_and_axis_value(&g, [-1.0, 0.0, 0.0], 0, 20.0);
        let n = tri_normal(q[0], q[1], q[2]);
        let d = dot(n, [-1.0, 0.0, 0.0]);
        assert!(d > 0.0, "east wall winding must be CCW from inside (n={n:?})");
    }

    #[test]
    fn wall_winding_west_ccw_from_inside() {
        let g = RoomGeometry::test_room();
        let q = first_quad_with_normal_and_axis_value(&g, [1.0, 0.0, 0.0], 0, -20.0);
        let n = tri_normal(q[0], q[1], q[2]);
        let d = dot(n, [1.0, 0.0, 0.0]);
        assert!(d > 0.0, "west wall winding must be CCW from inside (n={n:?})");
    }

    #[test]
    fn floor_winding_ccw_viewed_from_above() {
        let g = RoomGeometry::test_room();
        // Floor normal is +Y. CCW viewed from +Y ⇒ tri_normal · (+Y) > 0.
        let q = first_quad_with_normal(&g, [0.0, 1.0, 0.0]);
        let n = tri_normal(q[0], q[1], q[2]);
        let d = dot(n, [0.0, 1.0, 0.0]);
        assert!(d > 0.0, "floor winding must be CCW viewed from above (n={n:?})");
    }

    #[test]
    fn ceiling_winding_ccw_viewed_from_below() {
        let g = RoomGeometry::test_room();
        // Ceiling normal is -Y. CCW viewed from -Y ⇒ tri_normal · (-Y) > 0.
        let q = first_quad_with_normal(&g, [0.0, -1.0, 0.0]);
        let n = tri_normal(q[0], q[1], q[2]);
        let d = dot(n, [0.0, -1.0, 0.0]);
        assert!(
            d > 0.0,
            "ceiling winding must be CCW viewed from below (n={n:?})"
        );
    }

    #[test]
    fn plinth_box_winding_ccw_from_outside_each_face() {
        // For every face on every plinth, the triangle winding's geometric
        // normal must agree with the stored `normal` field (positive dot).
        let g = RoomGeometry::test_room();
        let mut i = 0;
        while i < g.vertices.len() {
            let v0 = g.vertices[i];
            let v1 = g.vertices[i + 1];
            let v2 = g.vertices[i + 2];
            let geom_n = tri_normal(v0.position, v1.position, v2.position);
            let stored_n = v0.normal;
            let d = dot(geom_n, stored_n);
            assert!(
                d > -1e-3,
                "winding mismatch at vert {i} (stored={stored_n:?} geom={geom_n:?})"
            );
            i += 4;
        }
    }

    #[test]
    fn floor_quadrants_have_distinct_patterns() {
        use std::collections::HashSet;
        let g = RoomGeometry::test_room();
        // Find all floor-normal quads ; they have +Y normal.
        let mut patterns = HashSet::new();
        let mut i = 0;
        while i < g.vertices.len() {
            let v = g.vertices[i];
            if (v.normal[1] - 1.0).abs() < 1e-3 && v.position[1].abs() < 1e-3 {
                patterns.insert(v.pattern_id);
            }
            i += 4;
        }
        assert_eq!(patterns.len(), 4, "all four floor quadrants must have distinct patterns");
    }

    /// Look up the pattern_id of the first vertex matching the predicate.
    /// More-targeted than position-only matching (avoids hitting plinth
    /// boxes that happen to share corner-coords with a wall).
    fn first_pattern_with_normal_and_axis(
        g: &RoomGeometry,
        expected_normal: [f32; 3],
        axis: usize,
        value: f32,
    ) -> u32 {
        for v in &g.vertices {
            if (v.normal[0] - expected_normal[0]).abs() < 1e-3
                && (v.normal[1] - expected_normal[1]).abs() < 1e-3
                && (v.normal[2] - expected_normal[2]).abs() < 1e-3
                && (v.position[axis] - value).abs() < 0.5
            {
                return v.pattern_id;
            }
        }
        panic!("no vertex with normal {expected_normal:?} at axis {axis}={value}");
    }

    #[test]
    fn walls_have_distinct_patterns() {
        // North/South/East/West walls must each have a different pattern id.
        let g = RoomGeometry::test_room();
        let np = first_pattern_with_normal_and_axis(&g, [0.0, 0.0, -1.0], 2, 20.0);
        let sp = first_pattern_with_normal_and_axis(&g, [0.0, 0.0, 1.0], 2, -20.0);
        let ep = first_pattern_with_normal_and_axis(&g, [-1.0, 0.0, 0.0], 0, 20.0);
        // West has 2 quads with normal +X at x=-20 (the wall itself + the
        // frequency-sweep stripe). Either one is "the west wall" for this
        // test ; we want the wall, so look for the floor-aligned base vert.
        let mut wp = u32::MAX;
        for v in &g.vertices {
            if (v.normal[0] - 1.0).abs() < 1e-3
                && v.normal[1].abs() < 1e-3
                && v.normal[2].abs() < 1e-3
                && (v.position[0] + 20.0).abs() < 0.01
                && v.position[1] < 0.01
            {
                wp = v.pattern_id;
                break;
            }
        }
        assert_ne!(wp, u32::MAX, "couldn't locate west wall vert");

        let mut bag = std::collections::HashSet::new();
        bag.insert(np);
        bag.insert(sp);
        bag.insert(ep);
        bag.insert(wp);
        assert_eq!(
            bag.len(),
            4,
            "4 walls must have 4 distinct patterns ({np},{sp},{ep},{wp})"
        );
    }

    #[test]
    fn stress_object_count_is_14() {
        assert_eq!(stress_object_count(), 14);
    }

    #[test]
    fn stress_object_materials_cover_at_least_8_distinct() {
        use std::collections::HashSet;
        let mut bag = HashSet::new();
        for k in 0..stress_object_count() {
            bag.insert(stress_object_material(k));
        }
        assert!(bag.len() >= 8, "at least 8 distinct materials across stress objects (got {})", bag.len());
    }

    #[test]
    fn stress_object_names_unique() {
        use std::collections::HashSet;
        let mut bag = HashSet::new();
        for k in 0..stress_object_count() {
            bag.insert(stress_object_name(k));
        }
        assert_eq!(bag.len() as u32, stress_object_count());
    }

    #[test]
    fn transparent_stress_index_range_set() {
        let g = RoomGeometry::test_room();
        // § T11-LOA-RAYMARCH : glass cube is now kind=8 (was 2) →
        // transparent_index_range should still be Some.
        assert!(g.transparent_index_range.is_some());
        let (lo, hi) = g.transparent_index_range.unwrap();
        assert!(hi > lo, "transparent range must be nonempty");
    }

    // ─── § T11-LOA-RAYMARCH tests ────────────────────────────────────────

    #[test]
    fn geometry_assigns_6_stress_objects_to_raymarch_kinds() {
        use crate::pattern::pattern_is_raymarch;
        // Exactly 6 of the 14 stress slots must trigger fragment-shader
        // raymarching. The other 8 stay cube-based with 2D-UV procedurals.
        let total = stress_object_count();
        let mut raymarch_count = 0u32;
        let mut cube_count = 0u32;
        for k in 0..total {
            let p = stress_object_pattern(k);
            if pattern_is_raymarch(p) {
                raymarch_count += 1;
            } else {
                cube_count += 1;
            }
        }
        assert_eq!(raymarch_count, 6, "exactly 6 stress objects must be raymarched");
        assert_eq!(cube_count, 8, "exactly 8 stress objects must stay cube-based");
        assert_eq!(raymarch_count + cube_count, total);
    }

    #[test]
    fn geometry_raymarch_kinds_cover_all_6_sdf_types() {
        use crate::pattern::pattern_is_raymarch;
        // The 6 raymarched stress slots must collectively cover all 6
        // distinct SDF kinds (mandelbulb, sphere, torus, gyroid, julia, menger)
        // — not 6 copies of the same.
        use std::collections::HashSet;
        let mut bag = HashSet::new();
        for k in 0..stress_object_count() {
            let p = stress_object_pattern(k);
            if pattern_is_raymarch(p) {
                bag.insert(p);
            }
        }
        assert_eq!(bag.len(), 6, "all 6 SDF kinds must be represented");
    }

    // ──────────────────────────────────────────────────────────────────
    // § T11-LOA-ROOMS · multi-room geometry tests
    // ──────────────────────────────────────────────────────────────────

    /// `full_world()` returns geometry strictly LARGER than `test_room()`.
    /// (5 rooms + 4 corridors emit thousands of additional vertices.)
    #[test]
    fn full_world_has_more_vertices_than_test_room() {
        let one = RoomGeometry::test_room();
        let all = RoomGeometry::full_world();
        assert!(
            all.vertices.len() > one.vertices.len(),
            "full_world ({}) must exceed test_room ({})",
            all.vertices.len(),
            one.vertices.len()
        );
    }

    /// `full_world` plinth-count : the 14 TestRoom plinths are still emitted.
    #[test]
    fn full_world_preserves_test_room_plinth_count() {
        let g = RoomGeometry::full_world();
        assert_eq!(
            g.plinth_count, 14,
            "full_world keeps the 14 TestRoom plinths intact"
        );
    }

    /// MaterialRoom must contribute exactly 16 cube-spheres (= 96 quads = 384 verts).
    /// We detect them by counting boxes at y ≈ 3.0 within the MaterialRoom AABB.
    #[test]
    fn room_material_room_has_16_spheres() {
        let g = RoomGeometry::full_world();
        let mb = crate::room::Room::MaterialRoom.bounds();
        // Each sphere is a box ; we detect the +Y face's first-vertex which
        // lives at y = 3.0 + 1.5 = 4.5 (top of the cube). The +Y face has
        // normal = (0,1,0). 16 spheres × 4 verts on +Y face = 64 verts.
        let mut count = 0;
        let mut i = 0;
        while i < g.vertices.len() {
            let v = g.vertices[i];
            let inside = v.position[0] >= mb.min[0] - 2.0
                && v.position[0] <= mb.max[0] + 2.0
                && v.position[2] >= mb.min[2] - 2.0
                && v.position[2] <= mb.max[2] + 2.0;
            if inside
                && (v.normal[1] - 1.0).abs() < 1e-3
                && (v.position[1] - 4.5).abs() < 1e-3
            {
                count += 1;
            }
            i += 4;
        }
        assert_eq!(count, 16, "MaterialRoom must emit 16 hovering spheres");
    }

    /// § T11-LOA-FID-MAINSTREAM : MaterialRoom carries 4 emissive HDR-test
    /// panels on the north wall (intensities 1× / 4× / 16× / 64×). The
    /// panels are quads with normal (0, 0, -1) at z = MaterialRoom.max[2]
    /// - 0.05 ; vertex.color encodes the intensity multiplier.
    #[test]
    fn room_material_room_has_four_hdr_test_panels() {
        let g = RoomGeometry::full_world();
        let mb = crate::room::Room::MaterialRoom.bounds();
        let z_panel = mb.max[2] - 0.05;
        // Each panel emits 4 verts ; we look for the unique x-tints
        // 1.0 / 4.0 / 16.0 / 64.0.
        use std::collections::BTreeSet;
        let mut intensities: BTreeSet<u32> = BTreeSet::new();
        let mut panel_quads = 0;
        let mut i = 0;
        while i < g.vertices.len() {
            let v = g.vertices[i];
            let normal_match = (v.normal[0]).abs() < 1e-3
                && (v.normal[1]).abs() < 1e-3
                && (v.normal[2] + 1.0).abs() < 1e-3;
            let z_match = (v.position[2] - z_panel).abs() < 1e-3;
            let inside_x = v.position[0] >= mb.min[0] && v.position[0] <= mb.max[0];
            if normal_match && z_match && inside_x {
                panel_quads += 1;
                intensities.insert((v.color[0] * 1000.0) as u32);
            }
            i += 4;
        }
        assert_eq!(
            panel_quads, 4,
            "MaterialRoom must emit 4 HDR-test panels on the north wall"
        );
        // All four discrete intensity stops must be present (1.0, 4.0, 16.0, 64.0).
        assert!(intensities.contains(&1000), "missing 1× panel");
        assert!(intensities.contains(&4000), "missing 4× panel");
        assert!(intensities.contains(&16000), "missing 16× panel");
        assert!(intensities.contains(&64000), "missing 64× panel");
    }

    /// PatternRoom floor must be tiled into 16 distinct floor quads, each
    /// using a different pattern_id (modulo PATTERN_LUT_LEN).
    #[test]
    fn room_pattern_room_has_16_floor_tiles() {
        let g = RoomGeometry::full_world();
        let pb = crate::room::Room::PatternRoom.bounds();
        // Find +Y-normal vertices at y=0 inside the PatternRoom bounds.
        let mut tile_count = 0;
        let mut i = 0;
        while i < g.vertices.len() {
            let v = g.vertices[i];
            let inside = v.position[0] >= pb.min[0]
                && v.position[0] <= pb.max[0]
                && v.position[2] >= pb.min[2]
                && v.position[2] <= pb.max[2];
            if inside && (v.normal[1] - 1.0).abs() < 1e-3 && v.position[1].abs() < 1e-3 {
                tile_count += 1;
            }
            i += 4;
        }
        assert_eq!(tile_count, 16, "PatternRoom must emit 16 floor tiles");
    }

    /// World-bounds envelope check : `full_world()`'s vertices are all
    /// within the 120m × 12m × 120m budget computed by `world_envelope`.
    #[test]
    fn full_world_vertices_within_envelope() {
        let g = RoomGeometry::full_world();
        let env = crate::room::world_envelope();
        for v in &g.vertices {
            assert!(v.position[0] >= env.min[0] - 0.5);
            assert!(v.position[0] <= env.max[0] + 0.5);
            assert!(v.position[1] >= env.min[1] - 0.5);
            assert!(v.position[1] <= env.max[1] + 0.5);
            assert!(v.position[2] >= env.min[2] - 0.5);
            assert!(v.position[2] <= env.max[2] + 0.5);
        }
    }

    /// Bonus : verify the full_world vertex budget is reasonable. With 5
    /// rooms + 4 corridors + doorways + plinths + 16 spheres + 12 height
    /// markers, total vertex-count should land below 8000. (Loose upper
    /// bound — mostly a sanity check that we're not accidentally emitting
    /// quads in a hot loop.)
    #[test]
    fn geometry_emit_all_5_rooms_total_vertex_count_under_8000() {
        let g = RoomGeometry::full_world();
        assert!(
            g.vertices.len() < 8000,
            "full_world emitted {} verts (budget 8000)",
            g.vertices.len()
        );
    }
}
