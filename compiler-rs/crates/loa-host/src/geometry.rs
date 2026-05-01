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
    MAT_TRANSPARENT_GLASS, MAT_VERMILLION_LACQUER, MAT_WARM_SKY,
};
use crate::pattern::{
    PAT_CHECKERBOARD, PAT_CONCENTRIC_RINGS, PAT_EAN13_BARCODE, PAT_FREQUENCY_SWEEP,
    PAT_GRADIENT_GRAYSCALE, PAT_GRID_1M, PAT_MACBETH_COLOR_CHART, PAT_PERLIN_NOISE,
    PAT_QR_CODE_STUB, PAT_RADIAL_GRADIENT, PAT_RADIAL_SPOKES, PAT_SNELLEN_EYE_CHART,
    PAT_SOLID, PAT_ZONEPLATE,
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

/// Stress-object kind id (0..13). The visual + material per id is :
///   0 mandelbulb-fractal · 1 reflective-sphere · 2 glass-cube · 3 hairy-ball
///   4 iridescent-torus · 5 holographic · 6 emissive-cyan-ring · 7 icosphere
///   8 macbeth-cube · 9 zoneplate-cube · 10 matte-grey · 11 gold-leaf
///   12 pink-noise · 13 vermillion-classic
#[must_use]
pub const fn stress_object_count() -> u32 {
    14
}

/// Per-stress-object material id.
#[must_use]
pub const fn stress_object_material(kind: u32) -> u32 {
    match kind {
        0 => MAT_DEEP_INDIGO,
        1 => MAT_BRUSHED_STEEL,
        2 => MAT_TRANSPARENT_GLASS,
        3 => MAT_HAIRY_FUR,
        4 => MAT_IRIDESCENT,
        5 => MAT_HOLOGRAPHIC,
        6 => MAT_EMISSIVE_CYAN,
        7 => MAT_DICHROIC_VIOLET,
        8 => MAT_GRADIENT_RED,
        9 => MAT_NEON_MAGENTA,
        10 => MAT_MATTE_GREY,
        11 => MAT_GOLD_LEAF,
        12 => MAT_PINK_NOISE_VOL,
        13 => MAT_VERMILLION_LACQUER,
        _ => MAT_MATTE_GREY,
    }
}

/// Per-stress-object pattern id.
#[must_use]
pub const fn stress_object_pattern(kind: u32) -> u32 {
    match kind {
        0 => PAT_PERLIN_NOISE,
        1 => PAT_SOLID,
        2 => PAT_SOLID,
        3 => PAT_RADIAL_SPOKES,
        4 => PAT_GRADIENT_GRAYSCALE,
        5 => PAT_QR_CODE_STUB,
        6 => PAT_SOLID,
        7 => PAT_CONCENTRIC_RINGS,
        8 => PAT_MACBETH_COLOR_CHART,
        9 => PAT_ZONEPLATE,
        10 => PAT_GRID_1M,
        11 => PAT_SOLID,
        12 => PAT_PERLIN_NOISE,
        13 => PAT_SOLID,
        _ => PAT_SOLID,
    }
}

/// Human-readable stress-object name (for HUD).
#[must_use]
pub const fn stress_object_name(kind: u32) -> &'static str {
    match kind {
        0 => "Mandelbulb",
        1 => "Reflective-Sphere",
        2 => "Glass-Cube",
        3 => "Hairy-Ball",
        4 => "Iridescent-Torus",
        5 => "Holographic",
        6 => "Emissive-Ring",
        7 => "Icosphere-Subdiv",
        8 => "Macbeth-Cube",
        9 => "Zoneplate-Cube",
        10 => "Matte-Reference",
        11 => "Gold-Leaf",
        12 => "Pink-Noise",
        13 => "Vermillion-Classic",
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

            let track_transparent = kind == 2;
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
        // Glass cube is kind=2 → transparent_index_range should be Some.
        assert!(g.transparent_index_range.is_some());
        let (lo, hi) = g.transparent_index_range.unwrap();
        assert!(hi > lo, "transparent range must be nonempty");
    }
}
