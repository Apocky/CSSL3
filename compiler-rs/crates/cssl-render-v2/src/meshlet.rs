//! § meshlet — optional mesh-shader fallback for static UI / fonts / debug.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   The SDF-native path is canonical for world-geometry. Some Stage-5 use-
//!   cases — text glyphs, debug-overlay wireframes, opaque UI billboards —
//!   are more efficient as classical meshlet draws than as on-the-fly SDF
//!   ray-marches. This module provides a thin "meshlet hybrid" hook that
//!   Stage-5 can dispatch to when the input geometry is flagged
//!   [`MeshletKind::StaticUi`] / [`MeshletKind::DebugOverlay`] /
//!   [`MeshletKind::Font`].
//!
//! § SPEC
//!   - `Omniverse/07_AESTHETIC/01_SDF_NATIVE_RENDER.csl.md § VIII anti-pattern
//!     table` : "Triangle-mesh as primary geometry" is a violation. Triangle-
//!     mesh export-only (cold-tier) is allowed (acceptance § VII bullet 5).
//!   - `Omniverse/07_AESTHETIC/06_RENDERING_PIPELINE.csl § X` — work-graph
//!     fusion : meshlet path is OPTIONAL and fits in the cold-tier export
//!     branch.
//!
//! § DISCIPLINE
//!   - Meshlet dispatch is OFF-PATH for world-geometry. Calling sites must
//!     flag the meshlet-kind explicitly (no automatic-conversion-from-mesh-to-
//!     world is allowed).
//!   - The fallback exposes a consumer-facing test surface ; the actual
//!     mesh-shader bytecode emission lives in the per-host backends
//!     (`cssl-host-vulkan` / `cssl-host-d3d12` / `cssl-host-metal`).

use thiserror::Error;

/// What the meshlet describes. Each kind has a different rasterizer pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MeshletKind {
    /// Static UI billboard / quad-layer.
    StaticUi,
    /// Bitmap or signed-distance-field font glyph.
    Font,
    /// Debug-overlay wireframe (frustum, AABB, axis-marker).
    DebugOverlay,
    /// Cold-tier mesh export (allowed but flagged ; never the world-default).
    ColdTierExport,
}

impl MeshletKind {
    /// Returns `true` if this kind is permitted on the live render-pipeline.
    /// Cold-tier exports are NOT permitted on the live pipeline ; only the
    /// other three are.
    #[must_use]
    pub fn allowed_on_live_pipeline(self) -> bool {
        !matches!(self, MeshletKind::ColdTierExport)
    }
}

/// Descriptor of a single meshlet : kind + vertex/index handle + primitive count.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeshletDescriptor {
    /// Kind of meshlet (drives shader-pipeline selection).
    pub kind: MeshletKind,
    /// Opaque vertex-buffer handle (managed by the per-host backend).
    pub vertex_handle: u32,
    /// Opaque index-buffer handle.
    pub index_handle: u32,
    /// Triangle count.
    pub triangle_count: u32,
    /// World-space bounding-radius (for foveation-aware skip).
    pub bounding_radius: f32,
    /// Layer-priority (lower = drawn earlier).
    pub layer_priority: u8,
}

impl MeshletDescriptor {
    /// New descriptor.
    #[must_use]
    pub fn new(
        kind: MeshletKind,
        vertex_handle: u32,
        index_handle: u32,
        triangle_count: u32,
    ) -> Self {
        MeshletDescriptor {
            kind,
            vertex_handle,
            index_handle,
            triangle_count,
            bounding_radius: 1.0,
            layer_priority: 0,
        }
    }
}

/// Errors from the meshlet hybrid fallback path.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum MeshletError {
    /// Meshlet was emitted with a kind that is not permitted on the live
    /// render-pipeline (e.g. `ColdTierExport`).
    #[error("meshlet kind {kind:?} not permitted on live pipeline")]
    KindNotPermitted { kind: MeshletKind },
    /// Triangle count is zero (or out of bounds for the targeted hardware).
    #[error("meshlet has invalid triangle count {triangles}")]
    InvalidTriangleCount { triangles: u32 },
}

/// Meshlet hybrid fallback. Holds a list of descriptors + a soundness check.
#[derive(Debug, Clone, Default)]
pub struct MeshletHybridFallback {
    /// Pending meshlet descriptors. Drained on Stage-5 dispatch.
    pub descriptors: Vec<MeshletDescriptor>,
    /// Whether the fallback is currently active. False = SDF-only path.
    pub enabled: bool,
}

impl MeshletHybridFallback {
    /// New disabled fallback.
    #[must_use]
    pub fn new() -> Self {
        MeshletHybridFallback::default()
    }

    /// Enable the fallback.
    pub fn enable(&mut self) {
        self.enabled = true;
    }

    /// Disable the fallback.
    pub fn disable(&mut self) {
        self.enabled = false;
    }

    /// Append a descriptor. Returns an error if not permitted on the live
    /// pipeline.
    pub fn append(&mut self, desc: MeshletDescriptor) -> Result<(), MeshletError> {
        if !desc.kind.allowed_on_live_pipeline() {
            return Err(MeshletError::KindNotPermitted { kind: desc.kind });
        }
        if desc.triangle_count == 0 {
            return Err(MeshletError::InvalidTriangleCount {
                triangles: desc.triangle_count,
            });
        }
        self.descriptors.push(desc);
        Ok(())
    }

    /// Total triangle-count across all queued descriptors.
    #[must_use]
    pub fn total_triangles(&self) -> u32 {
        self.descriptors.iter().map(|d| d.triangle_count).sum()
    }

    /// Drain all descriptors (Stage-5 dispatch path) ; returns the queued list.
    pub fn drain(&mut self) -> Vec<MeshletDescriptor> {
        std::mem::take(&mut self.descriptors)
    }

    /// Sort descriptors by layer-priority (ascending) — stage-5 emits in this
    /// order.
    pub fn sort_by_priority(&mut self) {
        self.descriptors
            .sort_by_key(|d| d.layer_priority);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_kinds_permitted() {
        assert!(MeshletKind::StaticUi.allowed_on_live_pipeline());
        assert!(MeshletKind::Font.allowed_on_live_pipeline());
        assert!(MeshletKind::DebugOverlay.allowed_on_live_pipeline());
    }

    #[test]
    fn cold_tier_not_permitted_on_live() {
        assert!(!MeshletKind::ColdTierExport.allowed_on_live_pipeline());
    }

    #[test]
    fn descriptor_round_trip() {
        let d = MeshletDescriptor::new(MeshletKind::Font, 1, 2, 100);
        assert_eq!(d.kind, MeshletKind::Font);
        assert_eq!(d.vertex_handle, 1);
        assert_eq!(d.triangle_count, 100);
    }

    #[test]
    fn fallback_default_disabled() {
        let f = MeshletHybridFallback::new();
        assert!(!f.enabled);
    }

    #[test]
    fn fallback_enable_disable_round_trip() {
        let mut f = MeshletHybridFallback::new();
        f.enable();
        assert!(f.enabled);
        f.disable();
        assert!(!f.enabled);
    }

    #[test]
    fn fallback_append_static_ui_ok() {
        let mut f = MeshletHybridFallback::new();
        let r = f.append(MeshletDescriptor::new(MeshletKind::StaticUi, 0, 0, 1));
        assert!(r.is_ok());
        assert_eq!(f.descriptors.len(), 1);
    }

    #[test]
    fn fallback_append_cold_tier_errors() {
        let mut f = MeshletHybridFallback::new();
        let r = f.append(MeshletDescriptor::new(MeshletKind::ColdTierExport, 0, 0, 1));
        assert!(r.is_err());
    }

    #[test]
    fn fallback_append_zero_triangles_errors() {
        let mut f = MeshletHybridFallback::new();
        let r = f.append(MeshletDescriptor::new(MeshletKind::Font, 0, 0, 0));
        assert!(r.is_err());
    }

    #[test]
    fn fallback_total_triangles_sums() {
        let mut f = MeshletHybridFallback::new();
        let _ = f.append(MeshletDescriptor::new(MeshletKind::StaticUi, 0, 0, 5));
        let _ = f.append(MeshletDescriptor::new(MeshletKind::Font, 0, 0, 7));
        assert_eq!(f.total_triangles(), 12);
    }

    #[test]
    fn fallback_drain_clears_list() {
        let mut f = MeshletHybridFallback::new();
        let _ = f.append(MeshletDescriptor::new(MeshletKind::StaticUi, 0, 0, 5));
        let drained = f.drain();
        assert_eq!(drained.len(), 1);
        assert_eq!(f.descriptors.len(), 0);
    }

    #[test]
    fn fallback_sort_by_priority() {
        let mut f = MeshletHybridFallback::new();
        let mut a = MeshletDescriptor::new(MeshletKind::StaticUi, 0, 0, 1);
        a.layer_priority = 5;
        let mut b = MeshletDescriptor::new(MeshletKind::Font, 0, 0, 1);
        b.layer_priority = 1;
        let _ = f.append(a);
        let _ = f.append(b);
        f.sort_by_priority();
        assert_eq!(f.descriptors[0].kind, MeshletKind::Font);
    }
}
