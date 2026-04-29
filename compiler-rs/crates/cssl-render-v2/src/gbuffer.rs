//! § gbuffer — multi-view G-Buffer layout for Stage-5 → Stage-6+ handoff.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Stage-5 emits a [`MultiViewGBuffer`] that is consumed by Stage-6
//!   (KANBRDFEval) and Stage-11 (AppSWPass). The G-Buffer carries the per-pixel
//!   `(depth, position, normal, material-handle)` quadruple needed for
//!   subsequent shading + reprojection. Per the multi-view discipline the
//!   buffer is `RT_array_2_slices` for stereo (more for 4+ view configs).
//!
//! § SPEC
//!   - `Omniverse/07_AESTHETIC/06_RENDERING_PIPELINE.csl § III` Stage-5 outputs :
//!     `GBuffer<MultiView, 2>`, `VisibilityMask × 2`, `FirstHitDistance × 2`,
//!     `VolumetricAccum`.
//!   - `Omniverse/07_AESTHETIC/06_RENDERING_PIPELINE.csl § VIII` — multi-view
//!     stereo discipline.
//!
//! § STD430 LAYOUT (per-row : 32 bytes ; 8-byte aligned)
//!   ```text
//!   offset | bytes | field
//!   -------+-------+-------------------------------
//!     0    |   4   | depth (linear, meters)
//!     4    |   4   | _pad0
//!     8    |  12   | world-position xyz (f32×3)
//!    20    |  12   | normal xyz (f32×3 unit)
//!    32    |   4   | sdf_distance (negative = inside)
//!    36    |   4   | material_handle (u32 → M-facet)
//!    40    |   4   | sigma_low_consent_bits (u32)
//!    44    |   4   | view_index (u32)
//!   -------+-------+-------------------------------
//!         |  48   | TOTAL (round up to 8B-multiple = 48B)
//!   ```

use bytemuck::{Pod, Zeroable};

use crate::camera::EyeCamera;
use crate::multiview::MultiViewConfig;
use crate::normals::SurfaceNormal;

/// One G-Buffer row : depth + world-position + normal + material handle.
#[derive(Debug, Clone, Copy, PartialEq, Zeroable, Pod)]
#[repr(C, align(8))]
pub struct GBufferRow {
    /// Linear depth in meters from the eye.
    pub depth_meters: f32,
    /// Padding to align world-position to 8B.
    pub _pad0: f32,
    /// World-space hit-position.
    pub world_pos: [f32; 3],
    /// Unit-length world-space normal.
    pub normal: [f32; 3],
    /// SDF distance at hit (negative means inside, ~0 on surface).
    pub sdf_distance: f32,
    /// Material handle (M-facet handle from the OmegaField).
    pub material_handle: u32,
    /// Σ-low consent bits (for downstream stages to gate before reading
    /// material-coords or pattern-handles).
    pub sigma_low_consent_bits: u32,
    /// View index (0=left, 1=right for stereo).
    pub view_index: u32,
}

impl Default for GBufferRow {
    fn default() -> Self {
        GBufferRow {
            depth_meters: f32::INFINITY,
            _pad0: 0.0,
            world_pos: [0.0; 3],
            normal: [0.0, 1.0, 0.0],
            sdf_distance: f32::INFINITY,
            material_handle: 0,
            sigma_low_consent_bits: 0,
            view_index: 0,
        }
    }
}

impl GBufferRow {
    /// Compute the std430 byte-offset table — used for layout-tests + GPU-side
    /// upload-buffer plumbing.
    #[must_use]
    pub fn layout_offsets() -> GBufferLayout {
        GBufferLayout {
            depth_meters: 0,
            world_pos: 8,
            normal: 20,
            sdf_distance: 32,
            material_handle: 36,
            sigma_low_consent_bits: 40,
            view_index: 44,
            row_size: core::mem::size_of::<GBufferRow>() as u32,
        }
    }

    /// Convenience constructor : hit row from a position + normal + handle.
    #[must_use]
    pub fn hit(
        depth: f32,
        world_pos: [f32; 3],
        normal: SurfaceNormal,
        sdf_distance: f32,
        material_handle: u32,
        view_index: u32,
    ) -> Self {
        GBufferRow {
            depth_meters: depth,
            _pad0: 0.0,
            world_pos,
            normal: normal.0,
            sdf_distance,
            material_handle,
            sigma_low_consent_bits: 0,
            view_index,
        }
    }

    /// Convenience constructor : miss-row (no surface-hit along the ray).
    #[must_use]
    pub fn miss(view_index: u32) -> Self {
        GBufferRow {
            depth_meters: f32::INFINITY,
            view_index,
            ..GBufferRow::default()
        }
    }
}

/// std430 byte-offset table for [`GBufferRow`] layout-tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GBufferLayout {
    /// Offset of `depth_meters`.
    pub depth_meters: u32,
    /// Offset of `world_pos`.
    pub world_pos: u32,
    /// Offset of `normal`.
    pub normal: u32,
    /// Offset of `sdf_distance`.
    pub sdf_distance: u32,
    /// Offset of `material_handle`.
    pub material_handle: u32,
    /// Offset of `sigma_low_consent_bits`.
    pub sigma_low_consent_bits: u32,
    /// Offset of `view_index`.
    pub view_index: u32,
    /// Size of one row (in bytes ; multiple of 8).
    pub row_size: u32,
}

/// Single-view G-Buffer (one render-target slice).
#[derive(Debug, Clone)]
pub struct GBuffer {
    /// Per-pixel rows in row-major order. `len() == width * height`.
    pub rows: Vec<GBufferRow>,
    pub width: u32,
    pub height: u32,
    pub view_index: u32,
}

impl GBuffer {
    /// Allocate a fresh GBuffer for `width × height` pixels (defaulted to miss).
    #[must_use]
    pub fn new(width: u32, height: u32, view_index: u32) -> Self {
        let mut rows = Vec::with_capacity((width * height) as usize);
        for _ in 0..(width * height) {
            rows.push(GBufferRow::miss(view_index));
        }
        GBuffer {
            rows,
            width,
            height,
            view_index,
        }
    }

    /// Write a row at `(px, py)`.
    pub fn write(&mut self, px: u32, py: u32, row: GBufferRow) {
        let idx = (py * self.width + px) as usize;
        if idx < self.rows.len() {
            self.rows[idx] = row;
        }
    }

    /// Read a row at `(px, py)` (defaulted to miss-row if out of range).
    #[must_use]
    pub fn at(&self, px: u32, py: u32) -> GBufferRow {
        let idx = (py * self.width + px) as usize;
        self.rows.get(idx).copied().unwrap_or_default()
    }

    /// Hit-fraction : how many pixels contain a real hit (depth != ∞).
    #[must_use]
    pub fn hit_fraction(&self) -> f32 {
        if self.rows.is_empty() {
            return 0.0;
        }
        let hits = self
            .rows
            .iter()
            .filter(|r| r.depth_meters.is_finite())
            .count() as f32;
        hits / (self.rows.len() as f32)
    }
}

/// Multi-view G-Buffer : one [`GBuffer`] per view-index. The std430 backing
/// store on the GPU is a 2D texture array (Vulkan multiview) or array-of-RTs
/// (D3D12 view-instancing).
#[derive(Debug, Clone)]
pub struct MultiViewGBuffer {
    /// Per-view GBuffers. `len()` matches the multi-view config.
    pub views: Vec<GBuffer>,
    pub width: u32,
    pub height: u32,
}

impl MultiViewGBuffer {
    /// Allocate fresh G-Buffers for the given multi-view config.
    #[must_use]
    pub fn from_config(cfg: &MultiViewConfig) -> Self {
        let mut views = Vec::with_capacity(cfg.views.len());
        for v in &cfg.views {
            views.push(GBuffer::new(cfg.width, cfg.height, v.index));
        }
        MultiViewGBuffer {
            views,
            width: cfg.width,
            height: cfg.height,
        }
    }

    /// Allocate stereo G-Buffers from per-eye cameras.
    #[must_use]
    pub fn stereo(left: &EyeCamera, right: &EyeCamera) -> Self {
        MultiViewGBuffer {
            views: vec![
                GBuffer::new(left.width, left.height, 0),
                GBuffer::new(right.width, right.height, 1),
            ],
            width: left.width,
            height: left.height,
        }
    }

    /// Write a hit-row at `(view, px, py)`.
    pub fn write(&mut self, view: usize, px: u32, py: u32, row: GBufferRow) {
        if let Some(v) = self.views.get_mut(view) {
            v.write(px, py, row);
        }
    }

    /// Total pixel count across all views.
    #[must_use]
    pub fn total_pixels(&self) -> usize {
        self.views.iter().map(|v| v.rows.len()).sum()
    }

    /// Aggregate hit fraction across all views (counts hits / total).
    #[must_use]
    pub fn aggregate_hit_fraction(&self) -> f32 {
        if self.views.is_empty() {
            return 0.0;
        }
        let total: usize = self.views.iter().map(|v| v.rows.len()).sum();
        if total == 0 {
            return 0.0;
        }
        let hits: usize = self
            .views
            .iter()
            .map(|v| v.rows.iter().filter(|r| r.depth_meters.is_finite()).count())
            .sum();
        hits as f32 / total as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera::EyeCamera;
    use crate::multiview::MultiViewConfig;

    #[test]
    fn row_size_is_48_bytes() {
        // The std430-aligned row-size is 48B (rounded up from 44B raw fields
        // to next 8B-multiple).
        assert_eq!(core::mem::size_of::<GBufferRow>(), 48);
    }

    #[test]
    fn row_alignment_is_8() {
        assert_eq!(core::mem::align_of::<GBufferRow>(), 8);
    }

    #[test]
    fn layout_offsets_match_spec() {
        let l = GBufferRow::layout_offsets();
        assert_eq!(l.depth_meters, 0);
        assert_eq!(l.world_pos, 8);
        assert_eq!(l.normal, 20);
        assert_eq!(l.sdf_distance, 32);
        assert_eq!(l.material_handle, 36);
        assert_eq!(l.sigma_low_consent_bits, 40);
        assert_eq!(l.view_index, 44);
        assert_eq!(l.row_size, 48);
    }

    #[test]
    fn default_row_is_miss() {
        let r = GBufferRow::default();
        assert!(r.depth_meters.is_infinite());
    }

    #[test]
    fn miss_constructor_sets_view_index() {
        let r = GBufferRow::miss(7);
        assert_eq!(r.view_index, 7);
        assert!(r.depth_meters.is_infinite());
    }

    #[test]
    fn hit_constructor_round_trips() {
        let r = GBufferRow::hit(
            2.5,
            [1.0, 2.0, 3.0],
            SurfaceNormal::from_grad([1.0, 0.0, 0.0]),
            0.0,
            42,
            1,
        );
        assert!((r.depth_meters - 2.5).abs() < 1e-6);
        assert_eq!(r.world_pos, [1.0, 2.0, 3.0]);
        assert_eq!(r.material_handle, 42);
        assert_eq!(r.view_index, 1);
    }

    #[test]
    fn gbuffer_new_all_miss() {
        let g = GBuffer::new(8, 8, 0);
        assert!((g.hit_fraction() - 0.0).abs() < 1e-6);
    }

    #[test]
    fn gbuffer_write_bumps_hit_fraction() {
        let mut g = GBuffer::new(2, 2, 0);
        let r = GBufferRow::hit(
            1.0,
            [0.0, 0.0, 0.0],
            SurfaceNormal::from_grad([0.0, 1.0, 0.0]),
            0.0,
            0,
            0,
        );
        g.write(0, 0, r);
        let f = g.hit_fraction();
        assert!((f - 0.25).abs() < 1e-6);
    }

    #[test]
    fn multiview_from_config_matches_view_count() {
        let cam = EyeCamera::at_origin_quest3(4, 4);
        let cfg = MultiViewConfig::stereo(cam, cam);
        let mvg = MultiViewGBuffer::from_config(&cfg);
        assert_eq!(mvg.views.len(), 2);
    }

    #[test]
    fn multiview_total_pixels() {
        let cam = EyeCamera::at_origin_quest3(8, 8);
        let cfg = MultiViewConfig::stereo(cam, cam);
        let mvg = MultiViewGBuffer::from_config(&cfg);
        assert_eq!(mvg.total_pixels(), 8 * 8 * 2);
    }
}
