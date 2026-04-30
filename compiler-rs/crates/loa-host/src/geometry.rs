//! § geometry — test-room mesh generator per scenes/test_room.cssl design.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § T11-LOA-HOST-1 (W-LOA-host-render) : produces vertex + index buffers
//! for the canonical 40m × 8m × 40m test-room. Pure CPU mesh generation —
//! no SDF, no GPU compute. The shader (`scene.wgsl`) does Lambertian + ambient
//! lit shading using per-vertex normals + per-vertex colors.
//!
//! § ROOM LAYOUT  (top-down, +X right, +Z forward, origin at room center)
//!
//! ```text
//!     +Z (north)
//!      ┌──────────────┐         The room is 40m wide × 40m deep × 8m tall.
//!      │  NW    NE    │         Quadrants split by the X=0 and Z=0 axes.
//!      │ cool   warm  │         Floor tinted per-quadrant ; each quadrant
//!      │ blue   red   │         hosts 2 plinths (8 quadrant-plinths total).
//!      ├──────┼───────┤         4 calibration-corner plinths sit at the
//!      │  SW    SE    │         corner intersections. 2 center-axis plinths
//!      │ warm   cool  │         sit on the X=0 / Z=0 line (1 each).
//!      │ green  violet│         Total : 8+4+2 = 14 plinths.
//!      └──────────────┘
//!     -Z (south)            -X (west) <--> +X (east)
//! ```
//!
//! § PLINTH GEOMETRY
//!   1m × 2m × 1m vermillion-painted box (base) + 0.5m gold cap on top.
//!   Cap is centered on the base, sits at y = 2.0 (top of base) up to y = 2.5.
//!
//! § FLOOR
//!   40m × 40m subdivided into 4 quadrants ; each quadrant is a single quad
//!   (2 triangles · 4 vertices), tinted by quadrant-id.
//!
//! § CEILING
//!   40m × 40m flat at y=8 ; single quad with neutral tone.
//!
//! § WALLS
//!   4 walls (8m × 40m × thin) bounding the room. Each wall is rendered as
//!   a single quad per inner face + a stripe-pattern row of 0.1m grid-line
//!   inset boxes? — stage-0 keeps it as plain colored quads ; the "grid"
//!   pattern is signaled by a subtle stripe of darker vertex-color along
//!   the inner edge, which gives a 1m calibration-grid aesthetic without
//!   exploding vertex count. Richer KAN-shaded grids land in cssl-render-v2.

#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::suboptimal_flops)] // mesh-emit hot-path readability > mul_add micro-opt

use bytemuck::{Pod, Zeroable};

/// Single GPU vertex. Layout matches `Vertex::desc()` and `scene.wgsl` VsIn.
///
/// Bit-pattern : 9 × f32 = 36 bytes (12 + 12 + 12), 4-byte aligned.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub color: [f32; 3],
}

#[cfg(feature = "runtime")]
impl Vertex {
    /// `wgpu::VertexBufferLayout` for the render pipeline.
    #[must_use]
    pub fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        use std::mem::size_of;
        wgpu::VertexBufferLayout {
            array_stride: size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: (size_of::<[f32; 3]>()) as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: (size_of::<[f32; 3]>() * 2) as wgpu::BufferAddress,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x3,
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
}

// ──────────────────────────────────────────────────────────────────────────
// § ROOM CONSTANTS — scenes/test_room.cssl design
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

// Quadrant-tint colors (warm-red NE, cool-blue NW, warm-green SW, cool-violet SE).
const TINT_NE: [f32; 3] = [0.62, 0.36, 0.30]; // warm-red
const TINT_NW: [f32; 3] = [0.30, 0.42, 0.62]; // cool-blue
const TINT_SW: [f32; 3] = [0.32, 0.58, 0.36]; // warm-green
const TINT_SE: [f32; 3] = [0.50, 0.32, 0.58]; // cool-violet

// Wall + ceiling tones.
const WALL_TONE: [f32; 3] = [0.78, 0.76, 0.72]; // off-white limestone
const CEIL_TONE: [f32; 3] = [0.92, 0.92, 0.95]; // near-white sky-tinted

// Plinth materials.
const VERMILLION: [f32; 3] = [0.88, 0.27, 0.18]; // vermillion red
const GOLD: [f32; 3] = [0.96, 0.78, 0.27]; // bright gold cap

// ──────────────────────────────────────────────────────────────────────────
// § PUBLIC ENTRY
// ──────────────────────────────────────────────────────────────────────────

impl RoomGeometry {
    /// Construct the canonical test-room mesh.
    #[must_use]
    pub fn test_room() -> Self {
        let mut g = Self {
            vertices: Vec::with_capacity(2048),
            indices: Vec::with_capacity(4096),
            plinth_count: 0,
        };
        g.emit_floor();
        g.emit_ceiling();
        g.emit_walls();
        g.emit_plinths();
        g
    }
}

// ──────────────────────────────────────────────────────────────────────────
// § PLINTH POSITIONS — 14 total per the brief
// ──────────────────────────────────────────────────────────────────────────

/// Returns the 14 plinth (x, z) center positions on the floor.
/// 8 quadrant-plinths (2 per quadrant) + 4 corner-calibration + 2 center-axis.
#[must_use]
pub fn plinth_positions() -> [(f32, f32); 14] {
    [
        // 8 quadrant-plinths (2 per quadrant). Inner ring at radius~6 for one
        // pair per quadrant; outer ring at radius~12 for the second pair.
        // NE quadrant
        (6.0, 6.0),
        (12.0, 12.0),
        // NW quadrant
        (-6.0, 6.0),
        (-12.0, 12.0),
        // SW quadrant
        (-6.0, -6.0),
        (-12.0, -12.0),
        // SE quadrant
        (6.0, -6.0),
        (12.0, -12.0),
        // 4 calibration-corner plinths (just inside the corners).
        (16.0, 16.0),
        (-16.0, 16.0),
        (-16.0, -16.0),
        (16.0, -16.0),
        // 2 center-axis plinths (one on +X axis, one on +Z axis).
        (10.0, 0.0),
        (0.0, 10.0),
    ]
}

// ──────────────────────────────────────────────────────────────────────────
// § GEOMETRY EMISSION HELPERS
// ──────────────────────────────────────────────────────────────────────────

impl RoomGeometry {
    /// Emit a single quad (4 verts, 6 indices) given 4 corner positions, a
    /// shared normal, and a per-quad color. Corners CCW when viewed from
    /// the side the normal points toward.
    fn emit_quad(&mut self, corners: [[f32; 3]; 4], normal: [f32; 3], color: [f32; 3]) {
        let base = self.vertices.len() as u32;
        for c in corners {
            self.vertices.push(Vertex {
                position: c,
                normal,
                color,
            });
        }
        // Two triangles : (0,1,2) and (0,2,3) — assumes CCW input.
        self.indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    /// Emit an axis-aligned box centered at `(cx, cy, cz)` with full extents
    /// `(sx, sy, sz)`. 6 quads, outward-facing normals, single color.
    fn emit_box(&mut self, center: [f32; 3], size: [f32; 3], color: [f32; 3]) {
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

        // +X face (right) — normal +X, CCW when looking from +X toward -X.
        self.emit_quad(
            [[xp, yn, zn], [xp, yn, zp], [xp, yp, zp], [xp, yp, zn]],
            [1.0, 0.0, 0.0],
            color,
        );
        // -X face (left)
        self.emit_quad(
            [[xn, yn, zp], [xn, yn, zn], [xn, yp, zn], [xn, yp, zp]],
            [-1.0, 0.0, 0.0],
            color,
        );
        // +Y face (top)
        self.emit_quad(
            [[xn, yp, zn], [xp, yp, zn], [xp, yp, zp], [xn, yp, zp]],
            [0.0, 1.0, 0.0],
            color,
        );
        // -Y face (bottom)
        self.emit_quad(
            [[xn, yn, zp], [xp, yn, zp], [xp, yn, zn], [xn, yn, zn]],
            [0.0, -1.0, 0.0],
            color,
        );
        // +Z face (front)
        self.emit_quad(
            [[xp, yn, zp], [xn, yn, zp], [xn, yp, zp], [xp, yp, zp]],
            [0.0, 0.0, 1.0],
            color,
        );
        // -Z face (back)
        self.emit_quad(
            [[xn, yn, zn], [xp, yn, zn], [xp, yp, zn], [xn, yp, zn]],
            [0.0, 0.0, -1.0],
            color,
        );
    }

    fn emit_floor(&mut self) {
        // 4 quadrants : NE (+X +Z), NW (-X +Z), SW (-X -Z), SE (+X -Z).
        // Each quadrant : single quad covering its half of the floor.
        let normal = [0.0, 1.0, 0.0];
        let y = 0.0;
        let h = ROOM_HALF_X;
        // NE
        self.emit_quad(
            [[0.0, y, 0.0], [h, y, 0.0], [h, y, h], [0.0, y, h]],
            normal,
            TINT_NE,
        );
        // NW
        self.emit_quad(
            [[-h, y, 0.0], [0.0, y, 0.0], [0.0, y, h], [-h, y, h]],
            normal,
            TINT_NW,
        );
        // SW
        self.emit_quad(
            [[-h, y, -h], [0.0, y, -h], [0.0, y, 0.0], [-h, y, 0.0]],
            normal,
            TINT_SW,
        );
        // SE
        self.emit_quad(
            [[0.0, y, -h], [h, y, -h], [h, y, 0.0], [0.0, y, 0.0]],
            normal,
            TINT_SE,
        );
    }

    fn emit_ceiling(&mut self) {
        // Single quad ; normal points down (-Y). CCW when viewed from below.
        let y = ROOM_HEIGHT;
        let h = ROOM_HALF_X;
        let normal = [0.0, -1.0, 0.0];
        self.emit_quad(
            [[-h, y, -h], [-h, y, h], [h, y, h], [h, y, -h]],
            normal,
            CEIL_TONE,
        );
    }

    fn emit_walls(&mut self) {
        // 4 inner-facing wall quads. Each is 40m wide, 8m tall.
        let h = ROOM_HALF_X;
        let top = ROOM_HEIGHT;
        // North wall (z = +h, inner-face normal -Z, CCW viewed from inside).
        self.emit_quad(
            [[h, 0.0, h], [-h, 0.0, h], [-h, top, h], [h, top, h]],
            [0.0, 0.0, -1.0],
            WALL_TONE,
        );
        // South wall (z = -h, inner-face normal +Z).
        self.emit_quad(
            [[-h, 0.0, -h], [h, 0.0, -h], [h, top, -h], [-h, top, -h]],
            [0.0, 0.0, 1.0],
            WALL_TONE,
        );
        // East wall (x = +h, inner-face normal -X).
        self.emit_quad(
            [[h, 0.0, -h], [h, 0.0, h], [h, top, h], [h, top, -h]],
            [-1.0, 0.0, 0.0],
            WALL_TONE,
        );
        // West wall (x = -h, inner-face normal +X).
        self.emit_quad(
            [[-h, 0.0, h], [-h, 0.0, -h], [-h, top, -h], [-h, top, h]],
            [1.0, 0.0, 0.0],
            WALL_TONE,
        );
    }

    fn emit_plinths(&mut self) {
        for (x, z) in plinth_positions() {
            // Vermillion base : 1m × 2m × 1m, center at (x, 1, z).
            self.emit_box(
                [x, PLINTH_BASE_H * 0.5, z],
                [PLINTH_HALF_XZ * 2.0, PLINTH_BASE_H, PLINTH_HALF_XZ * 2.0],
                VERMILLION,
            );
            // Gold cap : 1m × 0.5m × 1m, sits on top of base, centered.
            self.emit_box(
                [x, PLINTH_BASE_H + PLINTH_CAP_H * 0.5, z],
                [PLINTH_HALF_XZ * 2.0, PLINTH_CAP_H, PLINTH_HALF_XZ * 2.0],
                GOLD,
            );
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

    /// Floor=4 quads, ceiling=1 quad, walls=4 quads, plinths=14×2 boxes (each
    /// box=6 quads). Each quad = 4 vertices.
    /// Total quads = 4 + 1 + 4 + 14×2×6 = 177. Total verts = 177×4 = 708.
    #[test]
    fn room_geometry_has_expected_vertex_count() {
        let g = RoomGeometry::test_room();
        let expected_quads: usize = 4 + 1 + 4 + 14 * 2 * 6;
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
    #[allow(clippy::float_cmp)] // exact-bit-pattern test of zeroed POD fields
    fn vertex_struct_is_pod() {
        // Compile-time check : Vertex implements Pod + Zeroable via derive.
        let v: Vertex = bytemuck::Zeroable::zeroed();
        assert_eq!(v.position, [0.0, 0.0, 0.0]);
        assert_eq!(v.normal, [0.0, 0.0, 0.0]);
        assert_eq!(v.color, [0.0, 0.0, 0.0]);
    }

    #[test]
    fn floor_quadrants_have_distinct_tints() {
        let mut tints = std::collections::HashSet::new();
        for c in [TINT_NE, TINT_NW, TINT_SW, TINT_SE] {
            tints.insert([c[0].to_bits(), c[1].to_bits(), c[2].to_bits()]);
        }
        assert_eq!(tints.len(), 4, "all four quadrant tints must be distinct");
    }
}
