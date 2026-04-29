//! § multiview — view-instance configuration for stereo + multi-view rendering.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Stage-5 dispatches `1` cmd-buf with `viewCount = 2` view-instances on
//!   the canonical-VR path (per-eye stereo). For passthrough debugging the
//!   path supports `viewCount = 1` (mono) ; for advanced rigs (split-screen
//!   collaborative-VR or four-camera-ar overlay) `viewCount = 4` is also
//!   allowed. The general `N` form covers post-research configs.
//!
//! § SPEC
//!   - `Omniverse/07_AESTHETIC/06_RENDERING_PIPELINE.csl § VIII` — multi-view
//!     stereo discipline + view-projection per-eye derivation.
//!   - `Omniverse/07_AESTHETIC/06_RENDERING_PIPELINE.csl § VI` — Vulkan
//!     `VK_KHR_multiview` / D3D12 `ViewInstancing` / Metal vertex-amplification
//!     mapping.

use smallvec::SmallVec;

use crate::camera::EyeCamera;

/// Number of view-instances dispatched per Stage-5 invocation. Must be ≥ 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewCount {
    /// Single mono view (passthrough debug or non-XR).
    Mono,
    /// Stereo two-eye view (canonical-VR path).
    Stereo,
    /// Four-view (split-screen collaborative-VR or quad-camera AR).
    Quad,
    /// General `N` views (research / multi-rig).
    N(u8),
}

impl ViewCount {
    /// Return the integer count.
    #[must_use]
    pub fn count(&self) -> u32 {
        match self {
            ViewCount::Mono => 1,
            ViewCount::Stereo => 2,
            ViewCount::Quad => 4,
            ViewCount::N(n) => *n as u32,
        }
    }

    /// Returns `true` if this is the canonical Stage-5 stereo path.
    #[must_use]
    pub fn is_stereo(&self) -> bool {
        matches!(self, ViewCount::Stereo)
    }
}

/// One concrete view-instance — index + camera + render-target slice.
#[derive(Debug, Clone)]
pub struct ViewInstance {
    /// View index (0..viewCount). Used by VK multiview gl_ViewIndex / D3D12
    /// SV_ViewID / Metal `view_amp_id`.
    pub index: u32,
    /// Camera for this view.
    pub camera: EyeCamera,
    /// Slice index into the multi-view render-target array.
    pub rt_slice: u32,
}

/// Multi-view configuration : counts + view-instances + render-target layout.
#[derive(Debug, Clone)]
pub struct MultiViewConfig {
    /// Number of views to render.
    pub view_count: ViewCount,
    /// One [`ViewInstance`] per view. Length must equal `view_count.count()`.
    pub views: SmallVec<[ViewInstance; 4]>,
    /// Width of the render-target (per-view).
    pub width: u32,
    /// Height of the render-target (per-view).
    pub height: u32,
}

impl MultiViewConfig {
    /// Construct a stereo config from two cameras (left, right). Indexes
    /// follow the OpenXR convention (0 = left eye, 1 = right eye).
    #[must_use]
    pub fn stereo(left: EyeCamera, right: EyeCamera) -> Self {
        let width = left.width;
        let height = left.height;
        let mut views = SmallVec::new();
        views.push(ViewInstance {
            index: 0,
            camera: left,
            rt_slice: 0,
        });
        views.push(ViewInstance {
            index: 1,
            camera: right,
            rt_slice: 1,
        });
        MultiViewConfig {
            view_count: ViewCount::Stereo,
            views,
            width,
            height,
        }
    }

    /// Construct a mono config from one camera.
    #[must_use]
    pub fn mono(cam: EyeCamera) -> Self {
        let width = cam.width;
        let height = cam.height;
        let mut views = SmallVec::new();
        views.push(ViewInstance {
            index: 0,
            camera: cam,
            rt_slice: 0,
        });
        MultiViewConfig {
            view_count: ViewCount::Mono,
            views,
            width,
            height,
        }
    }

    /// Validate the config. The view-list length must equal the view-count.
    #[must_use]
    pub fn is_consistent(&self) -> bool {
        self.views.len() == self.view_count.count() as usize
    }

    /// Iterate `(view_index, camera)` pairs for use in the raymarcher's outer
    /// loop. Multiview-aware backends fold this into a single GPU-side
    /// dispatch with `gl_ViewIndex` ; the CPU mock-loop walks them serially.
    pub fn iter_views(&self) -> impl Iterator<Item = &ViewInstance> {
        self.views.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera::EyeCamera;

    #[test]
    fn view_count_mono_one() {
        assert_eq!(ViewCount::Mono.count(), 1);
        assert!(!ViewCount::Mono.is_stereo());
    }

    #[test]
    fn view_count_stereo_two() {
        assert_eq!(ViewCount::Stereo.count(), 2);
        assert!(ViewCount::Stereo.is_stereo());
    }

    #[test]
    fn view_count_quad_four() {
        assert_eq!(ViewCount::Quad.count(), 4);
        assert!(!ViewCount::Quad.is_stereo());
    }

    #[test]
    fn view_count_n_arbitrary() {
        assert_eq!(ViewCount::N(7).count(), 7);
    }

    #[test]
    fn stereo_config_has_two_views() {
        let l = EyeCamera::at_origin_quest3(8, 8);
        let r = EyeCamera::at_origin_quest3(8, 8);
        let cfg = MultiViewConfig::stereo(l, r);
        assert_eq!(cfg.views.len(), 2);
        assert_eq!(cfg.views[0].index, 0);
        assert_eq!(cfg.views[1].index, 1);
        assert_eq!(cfg.views[0].rt_slice, 0);
        assert_eq!(cfg.views[1].rt_slice, 1);
        assert!(cfg.is_consistent());
    }

    #[test]
    fn mono_config_has_one_view() {
        let cam = EyeCamera::at_origin_quest3(4, 4);
        let cfg = MultiViewConfig::mono(cam);
        assert_eq!(cfg.views.len(), 1);
        assert!(cfg.is_consistent());
    }

    #[test]
    fn iter_views_walks_all() {
        let l = EyeCamera::at_origin_quest3(2, 2);
        let r = EyeCamera::at_origin_quest3(2, 2);
        let cfg = MultiViewConfig::stereo(l, r);
        let count = cfg.iter_views().count();
        assert_eq!(count, 2);
    }
}
