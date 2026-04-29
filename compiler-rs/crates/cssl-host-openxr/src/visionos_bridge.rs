//! visionOS Compositor-Services bridge.
//!
//! § SPEC : `07_AESTHETIC/05_VR_RENDERING.csl` § IV (visionOS exception).
//!
//! § DESIGN
//!   "‼ shim is engine-layer ⊗ ¬ shader-layer ⊗ shaders-see-the-same-ViewSet on-both-platforms"
//!
//!   The bridge presents Compositor-Services (`cp_layer_renderer`,
//!   `cp_drawable`, ARKit pose, rasterization-rate-map) as the
//!   OpenXR-equivalent surface. The real bridge implementation lives
//!   under `cfg(target_os = "visionos")` + the `metal-binding` cargo-
//!   feature ; the headless build provides a stub.
//!
//! § BRIDGE-MAPPINGS (§ IV)
//!   - `xrCreateSession`     ↔ `cp_layer_renderer`-creation
//!   - `xrLocateViews`       ↔ `cp_drawable`-poses + ARKit pose
//!   - `xrEndFrame`          ↔ `cp_layer_renderer.present`
//!   - `foveation_config`    ↔ `rasterization-rate-map`
//!   - `eye-tracking`        ↔ ARKit eye-tracking (system-level only ;
//!                             raw-stream NOT exposed by Apple)
//!
//! § ARKit DATA
//!   ARKit hand-tracking : 90 Hz, exposes per-bone poses.
//!   ARKit body-tracking : 27-joint canonical skeleton.
//!   ARKit face-tracking : Persona blendshape weights (52 ARKit-canonical).
//!   ARKit world-tracking : `WorldTrackingProvider` for environment-mesh.
//!   ARKit scene-reconstruction : `SceneReconstructionProvider`.

use crate::error::XRFailure;
use crate::view::ViewSet;

/// ARKit data-providers we wrap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ArkitDataProvider {
    /// `WorldTrackingProvider` (head pose + reference-spaces).
    WorldTracking,
    /// `HandTrackingProvider`.
    HandTracking,
    /// `BodyTrackingProvider`.
    BodyTracking,
    /// `SceneReconstructionProvider` (environment-mesh).
    SceneReconstruction,
    /// `PlaneDetectionProvider`.
    PlaneDetection,
    /// `ImageTrackingProvider`.
    ImageTracking,
    /// `ObjectTrackingProvider`.
    ObjectTracking,
    /// Persona blendshapes (visionOS only ; face-tracking).
    PersonaBlendshapes,
}

impl ArkitDataProvider {
    /// Display-name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::WorldTracking => "world-tracking",
            Self::HandTracking => "hand-tracking",
            Self::BodyTracking => "body-tracking",
            Self::SceneReconstruction => "scene-reconstruction",
            Self::PlaneDetection => "plane-detection",
            Self::ImageTracking => "image-tracking",
            Self::ObjectTracking => "object-tracking",
            Self::PersonaBlendshapes => "persona-blendshapes",
        }
    }

    /// All known providers.
    pub const ALL: [Self; 8] = [
        Self::WorldTracking,
        Self::HandTracking,
        Self::BodyTracking,
        Self::SceneReconstruction,
        Self::PlaneDetection,
        Self::ImageTracking,
        Self::ObjectTracking,
        Self::PersonaBlendshapes,
    ];
}

/// Compositor-Services foveation rate-map descriptor.
/// § IV bridge-mapping `foveation_config ← rasterization-rate-map`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RasterRateMap {
    /// Number of horizontal cells.
    pub h_cells: u32,
    /// Number of vertical cells.
    pub v_cells: u32,
    /// Per-cell horizontal rate (1.0 = full-rate, 0.5 = half-rate).
    /// Stored as a flat row-major array of length `h_cells * v_cells`.
    /// Stage-0 ships a fixed 8×8 grid ; the real rate-map is supplied by
    /// the bridge-impl when feature `metal-binding` is enabled.
    pub h_rates: [f32; 64],
    /// Per-cell vertical rate.
    pub v_rates: [f32; 64],
}

impl RasterRateMap {
    /// Identity rate-map : all-ones (no foveation).
    #[must_use]
    pub const fn identity() -> Self {
        Self {
            h_cells: 8,
            v_cells: 8,
            h_rates: [1.0; 64],
            v_rates: [1.0; 64],
        }
    }

    /// Default fixed-foveation rate-map matching FFR profile "High".
    /// Center 1×1, periphery 1/8 (matches `FFRProfile::High`).
    #[must_use]
    pub fn ffr_high() -> Self {
        let mut h = [0.125_f32; 64];
        let mut v = [0.125_f32; 64];
        // Center 4 cells (row 3-4, col 3-4) = full-rate.
        for r in 3..=4 {
            for c in 3..=4 {
                h[r * 8 + c] = 1.0;
                v[r * 8 + c] = 1.0;
            }
        }
        Self {
            h_cells: 8,
            v_cells: 8,
            h_rates: h,
            v_rates: v,
        }
    }
}

/// Bridge-state. Stage-0 carries the negotiated provider-set + rate-map.
#[derive(Debug, Clone)]
pub struct CompositorServicesBridge {
    /// Active ARKit providers.
    pub providers: Vec<ArkitDataProvider>,
    /// Rasterization-rate-map.
    pub rate_map: RasterRateMap,
    /// `true` iff the bridge is on a real visionOS host.
    pub on_visionos: bool,
}

impl CompositorServicesBridge {
    /// Stage-0 mock bridge. The FFI follow-up slice supersedes with a
    /// real `cp_layer_renderer` + `WorldTrackingProvider` build.
    #[must_use]
    pub fn mock_default() -> Self {
        Self {
            providers: vec![
                ArkitDataProvider::WorldTracking,
                ArkitDataProvider::HandTracking,
                ArkitDataProvider::SceneReconstruction,
                ArkitDataProvider::PersonaBlendshapes,
            ],
            rate_map: RasterRateMap::ffr_high(),
            on_visionos: cfg!(target_os = "visionos"),
        }
    }

    /// `xrCreateSession` ↔ `cp_layer_renderer`-creation. Stage-0 mock.
    pub fn create_layer_renderer(&self) -> Result<u64, XRFailure> {
        if !self.on_visionos {
            // Headless build : return mock-handle.
            return Ok(0xC0_0000_0001);
        }
        Err(XRFailure::NotYetImplemented(
            "compositor-services-bridge create_layer_renderer (real impl gated behind metal-binding feature)",
        ))
    }

    /// `xrLocateViews` ↔ `cp_drawable.poses + ARKit pose`. Stage-0
    /// returns identity stereo views.
    pub fn locate_views_via_arkit(
        &self,
        ipd_mm: f32,
        display_time_ns: u64,
    ) -> Result<ViewSet, XRFailure> {
        let mut vs = ViewSet::stereo_identity(ipd_mm);
        vs.display_time_ns = display_time_ns;
        Ok(vs)
    }

    /// `xrEndFrame` ↔ `cp_layer_renderer.present`. Stage-0 no-op.
    pub fn present(&self) -> Result<(), XRFailure> {
        Ok(())
    }

    /// Add an ARKit provider.
    pub fn add_provider(&mut self, p: ArkitDataProvider) {
        if !self.providers.contains(&p) {
            self.providers.push(p);
        }
    }

    /// Update rate-map.
    pub fn set_rate_map(&mut self, m: RasterRateMap) {
        self.rate_map = m;
    }

    /// `true` iff the given provider is active.
    #[must_use]
    pub fn has_provider(&self, p: ArkitDataProvider) -> bool {
        self.providers.contains(&p)
    }
}

#[cfg(test)]
mod tests {
    use super::{ArkitDataProvider, CompositorServicesBridge, RasterRateMap};

    #[test]
    fn arkit_provider_as_str() {
        assert_eq!(ArkitDataProvider::WorldTracking.as_str(), "world-tracking");
        assert_eq!(ArkitDataProvider::HandTracking.as_str(), "hand-tracking");
        assert_eq!(
            ArkitDataProvider::PersonaBlendshapes.as_str(),
            "persona-blendshapes"
        );
    }

    #[test]
    fn arkit_provider_all_unique() {
        let mut seen = std::collections::HashSet::new();
        for p in ArkitDataProvider::ALL {
            assert!(seen.insert(p));
        }
        assert_eq!(seen.len(), 8);
    }

    #[test]
    fn rate_map_identity_all_ones() {
        let m = RasterRateMap::identity();
        for r in m.h_rates {
            assert_eq!(r, 1.0);
        }
        for r in m.v_rates {
            assert_eq!(r, 1.0);
        }
    }

    #[test]
    fn rate_map_ffr_high_periphery_eighth() {
        let m = RasterRateMap::ffr_high();
        // Corner cell (0, 0) ⇒ periphery ⇒ 0.125.
        assert!((m.h_rates[0] - 0.125).abs() < 1e-6);
        // Center cell (3, 3) ⇒ full-rate.
        assert!((m.h_rates[3 * 8 + 3] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn bridge_mock_default_has_canonical_providers() {
        let b = CompositorServicesBridge::mock_default();
        assert!(b.has_provider(ArkitDataProvider::WorldTracking));
        assert!(b.has_provider(ArkitDataProvider::HandTracking));
        assert!(b.has_provider(ArkitDataProvider::SceneReconstruction));
        assert!(b.has_provider(ArkitDataProvider::PersonaBlendshapes));
    }

    #[test]
    fn bridge_create_layer_renderer_mock_returns_handle() {
        let b = CompositorServicesBridge::mock_default();
        // Off-visionOS host : returns mock handle.
        if !b.on_visionos {
            assert!(b.create_layer_renderer().is_ok());
        }
    }

    #[test]
    fn bridge_locate_views_returns_stereo() {
        let b = CompositorServicesBridge::mock_default();
        let vs = b.locate_views_via_arkit(64.0, 0).unwrap();
        assert!(vs.is_stereo());
    }

    #[test]
    fn bridge_present_noop_succeeds() {
        let b = CompositorServicesBridge::mock_default();
        assert!(b.present().is_ok());
    }

    #[test]
    fn bridge_add_provider_idempotent() {
        let mut b = CompositorServicesBridge::mock_default();
        let n0 = b.providers.len();
        b.add_provider(ArkitDataProvider::PlaneDetection);
        let n1 = b.providers.len();
        b.add_provider(ArkitDataProvider::PlaneDetection); // dup
        let n2 = b.providers.len();
        assert_eq!(n1, n0 + 1);
        assert_eq!(n2, n1);
    }

    #[test]
    fn bridge_set_rate_map() {
        let mut b = CompositorServicesBridge::mock_default();
        b.set_rate_map(RasterRateMap::identity());
        assert_eq!(b.rate_map.h_rates[0], 1.0);
    }
}
