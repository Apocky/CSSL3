//! § cssl-render::mesh — vertex / index buffers + attribute layouts
//! ═══════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Mesh data : vertex buffer + index buffer + the attribute layout that
//!   tells the GPU how to interpret vertex bytes. The renderer ships one
//!   canonical interleaved-vertex layout (`StandardVertex`) sufficient for
//!   PBR + skinning, plus an extensible [`VertexAttributeLayout`] surface
//!   for callers who need custom layouts (e.g. sprite-batchers, particle
//!   systems with packed data).
//!
//! § GEOMETRY MODEL
//!   - **Vertex buffer** : packed bytes laid out per `VertexAttributeLayout`.
//!     The renderer never inspects vertex contents directly ; the backend
//!     uploads the bytes verbatim and binds them as the shader's vertex
//!     stream.
//!   - **Index buffer** : `u16` or `u32` indices into the vertex buffer.
//!     `u16` saves 50% bandwidth for meshes with `<= 65k` vertices ; `u32`
//!     supports larger meshes. Topology is captured separately in [`Topology`].
//!   - **Topology** : how the indices are interpreted — triangle-list,
//!     triangle-strip, line-list, point-list. Stage-0 ships the four
//!     standard topologies ; tessellation / patch topologies deferred.
//!
//! § STANDARD VERTEX LAYOUT (substrate canonical)
//!   `StandardVertex` : 56 bytes per vertex.
//!     - `position`     : Vec3 (12 bytes) — world / model-space position
//!     - `normal`       : Vec3 (12 bytes) — surface normal, unit
//!     - `tangent`      : Vec4 (16 bytes) — tangent xyz + bitangent-sign w
//!     - `uv`           : Vec2 (8 bytes)  — UV-channel-0
//!     - `_padding`     : 8 bytes — pads to 56 for SIMD-friendly alignment
//!   Skinning (skin_indices + skin_weights) is layered on a SEPARATE
//!   buffer rather than expanded inline to keep the vertex buffer slim
//!   for non-skinned meshes. See [`SkinVertex`].

use crate::asset::AssetHandle;
use crate::math::{Aabb, Vec2, Vec3, Vec4};

// ════════════════════════════════════════════════════════════════════════════
// § Vec2 — UV-coords helper, kept here to avoid bloating math.rs
// ════════════════════════════════════════════════════════════════════════════
//
// Note : `Vec2` lives in `crate::math` — re-exported above for convenience.

// ════════════════════════════════════════════════════════════════════════════
// § Topology — how indices are interpreted
// ════════════════════════════════════════════════════════════════════════════

/// Primitive topology — how the GPU interprets the index buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Topology {
    /// Each triplet of indices forms a triangle. Independent triangles ;
    /// no shared edges. The most common topology — substrate canonical default.
    TriangleList,
    /// First three indices form the first triangle ; each subsequent index
    /// re-uses the previous two. Saves index buffer bandwidth for
    /// strip-friendly meshes.
    TriangleStrip,
    /// Each pair of indices forms a line segment.
    LineList,
    /// Each index is a point primitive.
    PointList,
}

impl Default for Topology {
    fn default() -> Self {
        Self::TriangleList
    }
}

impl Topology {
    /// Number of indices a single primitive consumes (after warm-up for
    /// strips). Triangles consume 3 ; lines 2 ; points 1.
    #[must_use]
    pub const fn indices_per_primitive(self) -> u32 {
        match self {
            Self::TriangleList | Self::TriangleStrip => 3,
            Self::LineList => 2,
            Self::PointList => 1,
        }
    }
}

// ════════════════════════════════════════════════════════════════════════════
// § IndexFormat — index integer width
// ════════════════════════════════════════════════════════════════════════════

/// Index buffer integer format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IndexFormat {
    /// 16-bit indices. Mesh must have `<= 65535` vertices. Saves 50% index-
    /// buffer bandwidth — preferred for high-density scenes.
    U16,
    /// 32-bit indices. No vertex-count limit beyond `u32::MAX`. Required for
    /// terrain / large-world meshes.
    U32,
}

impl IndexFormat {
    /// Bytes per index.
    #[must_use]
    pub const fn bytes_per_index(self) -> u32 {
        match self {
            Self::U16 => 2,
            Self::U32 => 4,
        }
    }

    /// Maximum vertex index this format can address.
    #[must_use]
    pub const fn max_index(self) -> u32 {
        match self {
            Self::U16 => 0xFFFF,
            Self::U32 => u32::MAX,
        }
    }
}

// ════════════════════════════════════════════════════════════════════════════
// § VertexAttribute — single attribute slot in the vertex layout
// ════════════════════════════════════════════════════════════════════════════

/// Semantic role of a vertex attribute. The renderer uses the semantic to
/// match attributes against shader input declarations — the same mesh can
/// drive multiple shader variants as long as the semantics line up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AttributeSemantic {
    /// Vertex position.
    Position,
    /// Surface normal.
    Normal,
    /// Tangent vector (typically 4-component with bitangent sign in `w`).
    Tangent,
    /// UV channel ; `index` selects which one for multi-channel layouts.
    TexCoord(u8),
    /// Vertex color.
    Color(u8),
    /// Skin joint indices (typically 4-component u8 / u16).
    SkinIndices,
    /// Skin joint weights (typically 4-component f32 summing to 1).
    SkinWeights,
    /// Custom application-defined semantic. The `u32` is a free-form ID
    /// agreed upon between mesh-author + shader-author.
    Custom(u32),
}

/// Element type of a single attribute. Determines bytes-per-element + how
/// the shader interprets the bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AttributeFormat {
    /// Single 32-bit float.
    F32x1,
    /// 2 × 32-bit floats (UV, packed-2D-velocity, etc).
    F32x2,
    /// 3 × 32-bit floats (position, normal).
    F32x3,
    /// 4 × 32-bit floats (tangent + bitangent-sign, color).
    F32x4,
    /// 4 × u8 unsigned integers (typically interpreted as normalized in shader).
    U8x4,
    /// 4 × u8, normalized to [0, 1] at shader-fetch time.
    Unorm8x4,
    /// 4 × i16, normalized to [-1, 1] at shader-fetch time.
    Snorm16x4,
    /// 4 × u16 (e.g. skin-indices for skeletons with > 256 joints).
    U16x4,
}

impl AttributeFormat {
    /// Bytes per attribute element.
    #[must_use]
    pub const fn bytes(self) -> u32 {
        match self {
            Self::F32x1 => 4,
            Self::F32x2 => 8,
            Self::F32x3 => 12,
            Self::F32x4 => 16,
            Self::U8x4 | Self::Unorm8x4 => 4,
            Self::Snorm16x4 | Self::U16x4 => 8,
        }
    }
}

/// Single vertex attribute slot : semantic + format + byte offset within the
/// vertex stride.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VertexAttribute {
    pub semantic: AttributeSemantic,
    pub format: AttributeFormat,
    /// Byte offset of this attribute within the vertex.
    pub offset: u32,
}

// ════════════════════════════════════════════════════════════════════════════
// § VertexAttributeLayout — packed vertex schema
// ════════════════════════════════════════════════════════════════════════════

/// Vertex attribute layout : the schema describing how vertex bytes are
/// interpreted. `stride` is the byte stride between consecutive vertices ;
/// each attribute carries an `offset` from the start of the vertex.
///
/// Stage-0 caps the per-mesh attribute count at 8. Real shaders rarely
/// exceed 5-6 attributes (position + normal + tangent + uv + maybe skin
/// indices/weights), so 8 covers the vast majority of cases. Custom
/// layouts requiring more can extend [`MAX_ATTRIBUTES`] in a follow-up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VertexAttributeLayout {
    /// Number of valid entries in `attributes`. `<= MAX_ATTRIBUTES`.
    pub count: u8,
    /// Attribute slots. Entries beyond `count` are ignored.
    pub attributes: [VertexAttribute; MAX_ATTRIBUTES],
    /// Byte stride between consecutive vertices in the vertex buffer.
    pub stride: u32,
}

/// Maximum attributes per vertex layout in stage-0.
pub const MAX_ATTRIBUTES: usize = 8;

impl VertexAttributeLayout {
    /// Empty layout — no attributes, zero stride. Useful as a builder seed.
    pub const EMPTY: Self = Self {
        count: 0,
        attributes: [VertexAttribute {
            semantic: AttributeSemantic::Position,
            format: AttributeFormat::F32x3,
            offset: 0,
        }; MAX_ATTRIBUTES],
        stride: 0,
    };

    /// True if this layout has the position attribute. Most pipelines
    /// require it ; the renderer rejects meshes without one.
    #[must_use]
    pub fn has_position(&self) -> bool {
        self.iter()
            .any(|a| matches!(a.semantic, AttributeSemantic::Position))
    }

    /// Iterate over the valid attributes.
    pub fn iter(&self) -> impl Iterator<Item = &VertexAttribute> {
        self.attributes.iter().take(self.count as usize)
    }

    /// Find the attribute matching the given semantic, if present.
    #[must_use]
    pub fn find(&self, semantic: AttributeSemantic) -> Option<&VertexAttribute> {
        self.iter().find(|a| a.semantic == semantic)
    }

    /// Canonical PBR-ready vertex layout : position + normal + tangent + uv.
    /// Stride = 48 bytes (12 + 12 + 16 + 8). Matches [`StandardVertex`].
    #[must_use]
    pub const fn standard_pbr() -> Self {
        let mut attrs = Self::EMPTY.attributes;
        attrs[0] = VertexAttribute {
            semantic: AttributeSemantic::Position,
            format: AttributeFormat::F32x3,
            offset: 0,
        };
        attrs[1] = VertexAttribute {
            semantic: AttributeSemantic::Normal,
            format: AttributeFormat::F32x3,
            offset: 12,
        };
        attrs[2] = VertexAttribute {
            semantic: AttributeSemantic::Tangent,
            format: AttributeFormat::F32x4,
            offset: 24,
        };
        attrs[3] = VertexAttribute {
            semantic: AttributeSemantic::TexCoord(0),
            format: AttributeFormat::F32x2,
            offset: 40,
        };
        Self {
            count: 4,
            attributes: attrs,
            stride: 48,
        }
    }

    /// Position-only layout. Useful for shadow-map / depth-only passes
    /// where the shader doesn't need surface attributes.
    #[must_use]
    pub const fn position_only() -> Self {
        let mut attrs = Self::EMPTY.attributes;
        attrs[0] = VertexAttribute {
            semantic: AttributeSemantic::Position,
            format: AttributeFormat::F32x3,
            offset: 0,
        };
        Self {
            count: 1,
            attributes: attrs,
            stride: 12,
        }
    }
}

impl Default for VertexAttributeLayout {
    fn default() -> Self {
        Self::standard_pbr()
    }
}

// ════════════════════════════════════════════════════════════════════════════
// § StandardVertex / SkinVertex — canonical packed structs
// ════════════════════════════════════════════════════════════════════════════

/// Canonical interleaved PBR vertex. Layout matches [`VertexAttributeLayout::standard_pbr`].
/// 48 bytes per vertex.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[repr(C)]
pub struct StandardVertex {
    pub position: Vec3,
    pub normal: Vec3,
    pub tangent: Vec4,
    pub uv: Vec2,
}

impl StandardVertex {
    /// Construct from explicit components.
    #[must_use]
    pub const fn new(position: Vec3, normal: Vec3, tangent: Vec4, uv: Vec2) -> Self {
        Self {
            position,
            normal,
            tangent,
            uv,
        }
    }

    /// Quick-construct with default normal (`+Y`), tangent (`+X`), and UV (0,0).
    /// Useful for testing + simple geometry generation.
    #[must_use]
    pub const fn position_only(position: Vec3) -> Self {
        Self {
            position,
            normal: Vec3::Y,
            tangent: Vec4::new(1.0, 0.0, 0.0, 1.0),
            uv: Vec2::new(0.0, 0.0),
        }
    }
}

/// Skinning-extension vertex : 4 joint-indices (u16) + 4 joint-weights (f32).
/// Layered on a SECOND vertex stream rather than packed inline so non-skinned
/// meshes don't pay the bandwidth cost.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[repr(C)]
pub struct SkinVertex {
    /// Indices of the four most-influential joints. `u16::MAX` = no joint.
    pub indices: [u16; 4],
    /// Joint weights, summing to `1.0` at well-formed meshes.
    pub weights: Vec4,
}

// ════════════════════════════════════════════════════════════════════════════
// § Mesh — the renderer-side mesh handle
// ════════════════════════════════════════════════════════════════════════════

/// Mesh data : layout + index format + topology + per-buffer asset handles +
/// local-space bounding box for culling.
///
/// The renderer does NOT own the vertex / index byte storage — it references
/// the asset crate's GPU buffer handles. This keeps the renderer
/// substrate-agnostic + the asset crate the single owner of GPU memory
/// lifecycles.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mesh {
    /// Vertex layout schema. The backend uses this to issue
    /// `vkCmdBindVertexBuffers` / `IASetVertexBuffers` with the right strides.
    pub layout: VertexAttributeLayout,
    /// Vertex buffer handle (asset-crate-owned).
    pub vertex_buffer: AssetHandle<MeshBuffer>,
    /// Optional skin-stream buffer handle, populated only for skinned meshes.
    pub skin_buffer: AssetHandle<MeshBuffer>,
    /// Index buffer handle (asset-crate-owned). May be `INVALID` for
    /// non-indexed draws (e.g. fullscreen-triangle hack).
    pub index_buffer: AssetHandle<MeshBuffer>,
    /// Index integer width. Ignored if `index_buffer` is INVALID.
    pub index_format: IndexFormat,
    /// Number of vertices in `vertex_buffer`. Used as the draw count for
    /// non-indexed draws + as a sanity bound for index validation.
    pub vertex_count: u32,
    /// Number of indices in `index_buffer`. Used as the indexed-draw count.
    pub index_count: u32,
    /// Primitive topology.
    pub topology: Topology,
    /// Local-space AABB for frustum culling. Computed at mesh-author time
    /// (or at asset-import). The renderer transforms this by the node's
    /// world matrix to produce the world-space culling bound.
    pub local_aabb: Aabb,
}

/// Marker type for mesh GPU-buffer asset handles. Keeps `MeshBuffer` handles
/// distinct from `Texture` handles at the type level even though the asset
/// crate may store them in unified storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MeshBuffer;

impl Default for Mesh {
    fn default() -> Self {
        Self::EMPTY
    }
}

impl Mesh {
    /// Empty mesh : no buffers bound, no vertices. Skipped during draw-call
    /// emission rather than producing a backend error.
    pub const EMPTY: Self = Self {
        layout: VertexAttributeLayout::standard_pbr(),
        vertex_buffer: AssetHandle::INVALID,
        skin_buffer: AssetHandle::INVALID,
        index_buffer: AssetHandle::INVALID,
        index_format: IndexFormat::U32,
        vertex_count: 0,
        index_count: 0,
        topology: Topology::TriangleList,
        local_aabb: Aabb::EMPTY,
    };

    /// True if the mesh has the minimum data needed to draw : a position
    /// attribute + a non-empty vertex buffer.
    #[must_use]
    pub fn is_drawable(&self) -> bool {
        self.layout.has_position() && self.vertex_buffer.is_valid() && self.vertex_count > 0
    }

    /// True if the mesh uses an index buffer (indexed-draw vs non-indexed).
    #[must_use]
    pub fn is_indexed(&self) -> bool {
        self.index_buffer.is_valid() && self.index_count > 0
    }

    /// True if the mesh has skinning data attached.
    #[must_use]
    pub fn is_skinned(&self) -> bool {
        self.skin_buffer.is_valid()
    }

    /// Primitive count this mesh produces : `index_count / topology-stride`
    /// for indexed, or `vertex_count / topology-stride` for non-indexed.
    #[must_use]
    pub fn primitive_count(&self) -> u32 {
        let count = if self.is_indexed() {
            self.index_count
        } else {
            self.vertex_count
        };
        let per = self.topology.indices_per_primitive();
        if matches!(self.topology, Topology::TriangleStrip) {
            count.saturating_sub(2)
        } else if per == 0 {
            0
        } else {
            count / per
        }
    }
}

// ════════════════════════════════════════════════════════════════════════════
// § Tests
// ════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topology_default_is_triangle_list() {
        assert_eq!(Topology::default(), Topology::TriangleList);
    }

    #[test]
    fn topology_indices_per_primitive() {
        assert_eq!(Topology::TriangleList.indices_per_primitive(), 3);
        assert_eq!(Topology::TriangleStrip.indices_per_primitive(), 3);
        assert_eq!(Topology::LineList.indices_per_primitive(), 2);
        assert_eq!(Topology::PointList.indices_per_primitive(), 1);
    }

    #[test]
    fn index_format_bytes() {
        assert_eq!(IndexFormat::U16.bytes_per_index(), 2);
        assert_eq!(IndexFormat::U32.bytes_per_index(), 4);
    }

    #[test]
    fn index_format_max() {
        assert_eq!(IndexFormat::U16.max_index(), 0xFFFF);
        assert_eq!(IndexFormat::U32.max_index(), u32::MAX);
    }

    #[test]
    fn attribute_format_bytes() {
        assert_eq!(AttributeFormat::F32x1.bytes(), 4);
        assert_eq!(AttributeFormat::F32x2.bytes(), 8);
        assert_eq!(AttributeFormat::F32x3.bytes(), 12);
        assert_eq!(AttributeFormat::F32x4.bytes(), 16);
        assert_eq!(AttributeFormat::U8x4.bytes(), 4);
        assert_eq!(AttributeFormat::Unorm8x4.bytes(), 4);
        assert_eq!(AttributeFormat::Snorm16x4.bytes(), 8);
        assert_eq!(AttributeFormat::U16x4.bytes(), 8);
    }

    #[test]
    fn standard_pbr_layout_stride_48() {
        let l = VertexAttributeLayout::standard_pbr();
        assert_eq!(l.stride, 48);
        assert_eq!(l.count, 4);
        assert!(l.has_position());
    }

    #[test]
    fn standard_pbr_layout_offsets() {
        let l = VertexAttributeLayout::standard_pbr();
        let pos = l
            .find(AttributeSemantic::Position)
            .expect("position present");
        assert_eq!(pos.offset, 0);
        let nrm = l.find(AttributeSemantic::Normal).expect("normal present");
        assert_eq!(nrm.offset, 12);
        let tan = l.find(AttributeSemantic::Tangent).expect("tangent present");
        assert_eq!(tan.offset, 24);
        let uv = l.find(AttributeSemantic::TexCoord(0)).expect("uv0 present");
        assert_eq!(uv.offset, 40);
    }

    #[test]
    fn position_only_layout_stride_12() {
        let l = VertexAttributeLayout::position_only();
        assert_eq!(l.stride, 12);
        assert_eq!(l.count, 1);
        assert!(l.has_position());
    }

    #[test]
    fn empty_layout_no_position() {
        let l = VertexAttributeLayout::EMPTY;
        assert_eq!(l.count, 0);
        assert!(!l.has_position());
    }

    #[test]
    fn layout_iter_yields_count() {
        let l = VertexAttributeLayout::standard_pbr();
        let n = l.iter().count();
        assert_eq!(n, 4);
    }

    #[test]
    fn standard_vertex_repr_size_matches_layout() {
        // Sanity : the packed struct layout stays in sync with the schema
        // stride. If this trips, either the layout or the struct drifted.
        assert_eq!(
            core::mem::size_of::<StandardVertex>(),
            VertexAttributeLayout::standard_pbr().stride as usize
        );
    }

    #[test]
    fn standard_vertex_position_only_constructor() {
        let v = StandardVertex::position_only(Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(v.position, Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(v.normal, Vec3::Y);
    }

    #[test]
    fn mesh_default_is_empty_and_undrawable() {
        let m = Mesh::default();
        assert!(!m.is_drawable());
        assert!(!m.is_indexed());
        assert!(!m.is_skinned());
        assert_eq!(m.primitive_count(), 0);
    }

    #[test]
    fn mesh_drawable_requires_position_and_buffer() {
        let mut m = Mesh::EMPTY;
        // Empty layout AND invalid buffer = not drawable.
        m.layout = VertexAttributeLayout::EMPTY;
        m.vertex_buffer = AssetHandle::new(0);
        m.vertex_count = 3;
        assert!(!m.is_drawable());

        // PBR layout + valid buffer + nonzero count = drawable.
        m.layout = VertexAttributeLayout::standard_pbr();
        assert!(m.is_drawable());
    }

    #[test]
    fn mesh_primitive_count_indexed_triangles() {
        let mut m = Mesh::EMPTY;
        m.vertex_buffer = AssetHandle::new(0);
        m.index_buffer = AssetHandle::new(1);
        m.vertex_count = 100;
        m.index_count = 30; // 10 triangles
        m.topology = Topology::TriangleList;
        assert_eq!(m.primitive_count(), 10);
    }

    #[test]
    fn mesh_primitive_count_strip_subtracts_two() {
        let mut m = Mesh::EMPTY;
        m.vertex_buffer = AssetHandle::new(0);
        m.vertex_count = 5;
        m.topology = Topology::TriangleStrip;
        // 5 vertices in a strip = 3 triangles (5 - 2).
        assert_eq!(m.primitive_count(), 3);
    }

    #[test]
    fn mesh_primitive_count_lines() {
        let mut m = Mesh::EMPTY;
        m.vertex_buffer = AssetHandle::new(0);
        m.vertex_count = 10;
        m.topology = Topology::LineList;
        // 10 / 2 = 5 lines.
        assert_eq!(m.primitive_count(), 5);
    }

    #[test]
    fn mesh_skinned_check() {
        let mut m = Mesh::EMPTY;
        m.skin_buffer = AssetHandle::new(7);
        assert!(m.is_skinned());
    }
}
