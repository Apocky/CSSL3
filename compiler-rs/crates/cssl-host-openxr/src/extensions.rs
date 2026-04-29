//! OpenXR extension catalog : the 60+ extensions referenced by
//! `07_AESTHETIC/05_VR_RENDERING.csl` § II + § VIII + § IX + § X +
//! § XIV.D + the spec's Anti-Patterns table.
//!
//! § DESIGN
//!   - One enum-variant per documented extension.
//!   - Bitmask aggregation via `XrExtensionSet` for advertise/require
//!     diff'ing.
//!   - Const string-constant per variant : the canonical `XR_*` name
//!     OpenXR's extension-enumeration call returns. The strings are
//!     `&'static str` so they can be passed straight into the FFI
//!     follow-up slice without re-allocation.
//!   - Categorization predicates (`is_required_quest3`,
//!     `is_required_vision_pro`, `is_required_pimax_crystal_super`,
//!     `is_optional_dfr`, `is_optional_passthrough`, `is_5yr_future`)
//!     drive the runtime's "advertise vs. require" negotiation.

use core::fmt;

/// OpenXR extension catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum XrExtension {
    // ─────────────────────────────────────────────────────────────────
    // § Khronos core (graphics-api binding)
    // ─────────────────────────────────────────────────────────────────
    /// Vulkan binding (XR_KHR_vulkan_enable).
    KhrVulkanEnable,
    /// Vulkan binding 2 (XR_KHR_vulkan_enable2) — preferred at OpenXR 1.0.30+.
    KhrVulkanEnable2,
    /// D3D11 binding (XR_KHR_D3D11_enable).
    KhrD3D11Enable,
    /// D3D12 binding (XR_KHR_D3D12_enable).
    KhrD3D12Enable,
    /// OpenGL binding (XR_KHR_opengl_enable).
    KhrOpenglEnable,
    /// OpenGL-ES binding (XR_KHR_opengl_es_enable).
    KhrOpenglEsEnable,
    // ─────────────────────────────────────────────────────────────────
    // § Composition layers
    // ─────────────────────────────────────────────────────────────────
    /// Depth composition layer (XR_KHR_composition_layer_depth).
    KhrCompositionLayerDepth,
    /// Quad composition layer (XR_KHR_composition_layer_quad — core).
    KhrCompositionLayerQuad,
    /// Cylinder composition layer (XR_KHR_composition_layer_cylinder).
    KhrCompositionLayerCylinder,
    /// Equirect composition layer 2 (XR_KHR_composition_layer_equirect2).
    KhrCompositionLayerEquirect2,
    /// Cube composition layer (XR_KHR_composition_layer_cube).
    KhrCompositionLayerCube,
    /// Color-scale-bias composition layer modifier
    /// (XR_KHR_composition_layer_color_scale_bias).
    KhrCompositionLayerColorScaleBias,
    /// Passthrough-color composition layer
    /// (XR_KHR_composition_layer_passthrough_color — anticipated).
    KhrCompositionLayerPassthroughColor,
    // ─────────────────────────────────────────────────────────────────
    // § Foveation
    // ─────────────────────────────────────────────────────────────────
    /// Fixed-foveated-rendering (XR_FB_foveation).
    FbFoveation,
    /// Foveation-config (XR_FB_foveation_configuration).
    FbFoveationConfiguration,
    /// Vulkan-binding for foveation (XR_FB_foveation_vulkan).
    FbFoveationVulkan,
    /// Eye-tracked dynamic-foveation (XR_META_foveation_eye_tracked).
    MetaFoveationEyeTracked,
    /// Varjo foveated rendering (XR_VARJO_foveated_rendering).
    VarjoFoveatedRendering,
    // ─────────────────────────────────────────────────────────────────
    // § Application-spacewarp (½-rate render-path)
    // ─────────────────────────────────────────────────────────────────
    /// XR_FB_space_warp — AppSW canonical.
    FbSpaceWarp,
    // ─────────────────────────────────────────────────────────────────
    // § Passthrough
    // ─────────────────────────────────────────────────────────────────
    /// Meta passthrough (XR_FB_passthrough).
    FbPassthrough,
    /// Passthrough keyboard + hands cutout (XR_FB_passthrough_keyboard_hands).
    FbPassthroughKeyboardHands,
    /// Environment-depth (XR_META_environment_depth).
    MetaEnvironmentDepth,
    /// HTC passthrough (XR_HTC_passthrough).
    HtcPassthrough,
    // ─────────────────────────────────────────────────────────────────
    // § Quad-views (Varjo)
    // ─────────────────────────────────────────────────────────────────
    /// Quad-view rendering (XR_VARJO_quad_views).
    VarjoQuadViews,
    // ─────────────────────────────────────────────────────────────────
    // § Eye-tracking + gaze
    // ─────────────────────────────────────────────────────────────────
    /// Khronos eye-gaze interaction profile (XR_EXT_eye_gaze_interaction).
    ExtEyeGazeInteraction,
    /// Meta high-rate social eye-tracking (XR_FB_eye_tracking_social).
    FbEyeTrackingSocial,
    /// Pico eye-tracking (XR_PICO_eye_tracking).
    PicoEyeTracking,
    /// HTC facial-tracking (XR_HTC_facial_tracking — combines eye+face).
    HtcFacialTracking,
    // ─────────────────────────────────────────────────────────────────
    // § Hand-tracking
    // ─────────────────────────────────────────────────────────────────
    /// Khronos hand-tracking (XR_EXT_hand_tracking).
    ExtHandTracking,
    /// Meta hand-tracking aim (XR_FB_hand_tracking_aim).
    FbHandTrackingAim,
    /// Meta hand-tracking capsules (XR_FB_hand_tracking_capsules).
    FbHandTrackingCapsules,
    /// Meta hand-tracking mesh (XR_FB_hand_tracking_mesh).
    FbHandTrackingMesh,
    /// HTC hand-interaction (XR_HTC_hand_interaction).
    HtcHandInteraction,
    /// Khronos hand-joint motion-range (XR_EXT_hand_joints_motion_range).
    ExtHandJointsMotionRange,
    // ─────────────────────────────────────────────────────────────────
    // § Body-tracking
    // ─────────────────────────────────────────────────────────────────
    /// Meta body-tracking (XR_FB_body_tracking).
    FbBodyTracking,
    /// HTC body-tracking (XR_HTC_body_tracking).
    HtcBodyTracking,
    /// ByteDance/Pico body-tracking (XR_BD_body_tracking).
    BdBodyTracking,
    // ─────────────────────────────────────────────────────────────────
    // § Face-tracking
    // ─────────────────────────────────────────────────────────────────
    /// Meta face-tracking 2 (XR_FB_face_tracking2).
    FbFaceTracking2,
    // ─────────────────────────────────────────────────────────────────
    // § Display-rate / refresh
    // ─────────────────────────────────────────────────────────────────
    /// Display refresh-rate switching (XR_FB_display_refresh_rate).
    FbDisplayRefreshRate,
    /// Color-space negotiation (XR_FB_color_space).
    FbColorSpace,
    // ─────────────────────────────────────────────────────────────────
    // § Haptics
    // ─────────────────────────────────────────────────────────────────
    /// Meta PCM haptics (XR_FB_haptic_pcm).
    FbHapticPcm,
    /// HTC tracker haptics (XR_HTC_vive_tracker_interaction).
    HtcViveTrackerInteraction,
    // ─────────────────────────────────────────────────────────────────
    // § Reference spaces
    // ─────────────────────────────────────────────────────────────────
    /// Local-floor reference-space (XR_EXT_local_floor).
    ExtLocalFloor,
    // ─────────────────────────────────────────────────────────────────
    // § Misc / debug / diagnostics
    // ─────────────────────────────────────────────────────────────────
    /// Debug-utils (XR_EXT_debug_utils).
    ExtDebugUtils,
    /// Performance-settings (XR_EXT_performance_settings).
    ExtPerformanceSettings,
    /// Thermal-settings (XR_EXT_thermal_settings).
    ExtThermalSettings,
    /// View-configuration depth-range (XR_EXT_view_configuration_depth_range).
    ExtViewConfigurationDepthRange,
    /// View-instances + amplification (XR_KHR_visibility_mask).
    KhrVisibilityMask,
    /// Future: anticipated lattice-haptics extension placeholder.
    AnticipatedLatticeHaptics,
    // ─────────────────────────────────────────────────────────────────
    // § Anchor + spatial-tracking (5-yr forward)
    // ─────────────────────────────────────────────────────────────────
    /// Meta spatial-anchor (XR_FB_spatial_entity).
    FbSpatialEntity,
    /// Meta scene-mesh (XR_FB_scene).
    FbScene,
    /// Meta scene-capture (XR_FB_scene_capture).
    FbSceneCapture,
}

impl XrExtension {
    /// Canonical `XR_*` name (zero-allocation; passable to FFI).
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::KhrVulkanEnable => "XR_KHR_vulkan_enable",
            Self::KhrVulkanEnable2 => "XR_KHR_vulkan_enable2",
            Self::KhrD3D11Enable => "XR_KHR_D3D11_enable",
            Self::KhrD3D12Enable => "XR_KHR_D3D12_enable",
            Self::KhrOpenglEnable => "XR_KHR_opengl_enable",
            Self::KhrOpenglEsEnable => "XR_KHR_opengl_es_enable",
            Self::KhrCompositionLayerDepth => "XR_KHR_composition_layer_depth",
            Self::KhrCompositionLayerQuad => "XR_KHR_composition_layer_quad",
            Self::KhrCompositionLayerCylinder => "XR_KHR_composition_layer_cylinder",
            Self::KhrCompositionLayerEquirect2 => "XR_KHR_composition_layer_equirect2",
            Self::KhrCompositionLayerCube => "XR_KHR_composition_layer_cube",
            Self::KhrCompositionLayerColorScaleBias => "XR_KHR_composition_layer_color_scale_bias",
            Self::KhrCompositionLayerPassthroughColor => {
                "XR_KHR_composition_layer_passthrough_color"
            }
            Self::FbFoveation => "XR_FB_foveation",
            Self::FbFoveationConfiguration => "XR_FB_foveation_configuration",
            Self::FbFoveationVulkan => "XR_FB_foveation_vulkan",
            Self::MetaFoveationEyeTracked => "XR_META_foveation_eye_tracked",
            Self::VarjoFoveatedRendering => "XR_VARJO_foveated_rendering",
            Self::FbSpaceWarp => "XR_FB_space_warp",
            Self::FbPassthrough => "XR_FB_passthrough",
            Self::FbPassthroughKeyboardHands => "XR_FB_passthrough_keyboard_hands",
            Self::MetaEnvironmentDepth => "XR_META_environment_depth",
            Self::HtcPassthrough => "XR_HTC_passthrough",
            Self::VarjoQuadViews => "XR_VARJO_quad_views",
            Self::ExtEyeGazeInteraction => "XR_EXT_eye_gaze_interaction",
            Self::FbEyeTrackingSocial => "XR_FB_eye_tracking_social",
            Self::PicoEyeTracking => "XR_PICO_eye_tracking",
            Self::HtcFacialTracking => "XR_HTC_facial_tracking",
            Self::ExtHandTracking => "XR_EXT_hand_tracking",
            Self::FbHandTrackingAim => "XR_FB_hand_tracking_aim",
            Self::FbHandTrackingCapsules => "XR_FB_hand_tracking_capsules",
            Self::FbHandTrackingMesh => "XR_FB_hand_tracking_mesh",
            Self::HtcHandInteraction => "XR_HTC_hand_interaction",
            Self::ExtHandJointsMotionRange => "XR_EXT_hand_joints_motion_range",
            Self::FbBodyTracking => "XR_FB_body_tracking",
            Self::HtcBodyTracking => "XR_HTC_body_tracking",
            Self::BdBodyTracking => "XR_BD_body_tracking",
            Self::FbFaceTracking2 => "XR_FB_face_tracking2",
            Self::FbDisplayRefreshRate => "XR_FB_display_refresh_rate",
            Self::FbColorSpace => "XR_FB_color_space",
            Self::FbHapticPcm => "XR_FB_haptic_pcm",
            Self::HtcViveTrackerInteraction => "XR_HTC_vive_tracker_interaction",
            Self::ExtLocalFloor => "XR_EXT_local_floor",
            Self::ExtDebugUtils => "XR_EXT_debug_utils",
            Self::ExtPerformanceSettings => "XR_EXT_performance_settings",
            Self::ExtThermalSettings => "XR_EXT_thermal_settings",
            Self::ExtViewConfigurationDepthRange => "XR_EXT_view_configuration_depth_range",
            Self::KhrVisibilityMask => "XR_KHR_visibility_mask",
            Self::AnticipatedLatticeHaptics => "XR_ANTICIPATED_lattice_haptics",
            Self::FbSpatialEntity => "XR_FB_spatial_entity",
            Self::FbScene => "XR_FB_scene",
            Self::FbSceneCapture => "XR_FB_scene_capture",
        }
    }

    /// Resolve from a runtime-advertised name. Unknown names return `None`.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|ext| ext.name() == name)
    }

    /// `true` iff this extension is **REQUIRED** for the Quest 3 day-one
    /// ship target. Failing this list = `XRFailure::MissingRequiredExtension`.
    /// § II.A Quest 3 row.
    #[must_use]
    pub const fn is_required_quest3(self) -> bool {
        matches!(
            self,
            Self::KhrVulkanEnable2
                | Self::KhrCompositionLayerDepth
                | Self::FbFoveation
                | Self::FbFoveationConfiguration
                | Self::FbFoveationVulkan
                | Self::FbSpaceWarp
                | Self::FbPassthrough
                | Self::ExtHandTracking
        )
    }

    /// `true` iff REQUIRED for Apple Vision Pro target. (visionOS uses
    /// Compositor-Services, not OpenXR-direct ; the bridge presents
    /// these as runtime-equivalents.) § II.A Vision Pro row.
    #[must_use]
    pub const fn is_required_vision_pro(self) -> bool {
        matches!(
            self,
            Self::KhrCompositionLayerDepth | Self::ExtHandTracking | Self::FbColorSpace
        )
    }

    /// `true` iff REQUIRED for Pimax Crystal Super target.
    /// § II.A Pimax Crystal Super row.
    #[must_use]
    pub const fn is_required_pimax_crystal_super(self) -> bool {
        matches!(
            self,
            Self::KhrVulkanEnable2
                | Self::KhrD3D12Enable
                | Self::KhrCompositionLayerDepth
                | Self::VarjoQuadViews
                | Self::VarjoFoveatedRendering
                | Self::ExtEyeGazeInteraction
        )
    }

    /// `true` iff this extension is OPTIONAL for DFR (eye-tracked dynamic
    /// foveation) ; missing = graceful-degrade to FFR. § V.B.
    #[must_use]
    pub const fn is_optional_dfr(self) -> bool {
        matches!(
            self,
            Self::MetaFoveationEyeTracked
                | Self::VarjoFoveatedRendering
                | Self::ExtEyeGazeInteraction
                | Self::FbEyeTrackingSocial
        )
    }

    /// `true` iff OPTIONAL for passthrough (mixed-reality). § IX.
    #[must_use]
    pub const fn is_optional_passthrough(self) -> bool {
        matches!(
            self,
            Self::FbPassthrough
                | Self::FbPassthroughKeyboardHands
                | Self::MetaEnvironmentDepth
                | Self::HtcPassthrough
                | Self::KhrCompositionLayerPassthroughColor
        )
    }

    /// `true` iff this extension is in the 5-yr forward-compat
    /// (Mirror-Lake-class) hook-list. § XIV.D.
    #[must_use]
    pub const fn is_5yr_future(self) -> bool {
        matches!(
            self,
            Self::KhrCompositionLayerPassthroughColor
                | Self::AnticipatedLatticeHaptics
                | Self::FbHapticPcm
        )
    }

    /// `true` iff this extension is part of the biometric-tracking
    /// surface (eye / hand / body / face) ; output values must carry
    /// `cssl-ifc::SensitiveDomain::{Gaze, Body, Face}` tags.
    #[must_use]
    pub const fn is_biometric_tracking(self) -> bool {
        matches!(
            self,
            Self::ExtEyeGazeInteraction
                | Self::FbEyeTrackingSocial
                | Self::PicoEyeTracking
                | Self::HtcFacialTracking
                | Self::ExtHandTracking
                | Self::FbHandTrackingAim
                | Self::FbHandTrackingCapsules
                | Self::FbHandTrackingMesh
                | Self::HtcHandInteraction
                | Self::ExtHandJointsMotionRange
                | Self::FbBodyTracking
                | Self::HtcBodyTracking
                | Self::BdBodyTracking
                | Self::FbFaceTracking2
        )
    }

    /// All 50+ catalog entries (table-driven traversal).
    pub const ALL: [Self; 50] = [
        Self::KhrVulkanEnable,
        Self::KhrVulkanEnable2,
        Self::KhrD3D11Enable,
        Self::KhrD3D12Enable,
        Self::KhrOpenglEnable,
        Self::KhrOpenglEsEnable,
        Self::KhrCompositionLayerDepth,
        Self::KhrCompositionLayerQuad,
        Self::KhrCompositionLayerCylinder,
        Self::KhrCompositionLayerEquirect2,
        Self::KhrCompositionLayerCube,
        Self::KhrCompositionLayerColorScaleBias,
        Self::KhrCompositionLayerPassthroughColor,
        Self::FbFoveation,
        Self::FbFoveationConfiguration,
        Self::FbFoveationVulkan,
        Self::MetaFoveationEyeTracked,
        Self::VarjoFoveatedRendering,
        Self::FbSpaceWarp,
        Self::FbPassthrough,
        Self::FbPassthroughKeyboardHands,
        Self::MetaEnvironmentDepth,
        Self::HtcPassthrough,
        Self::VarjoQuadViews,
        Self::ExtEyeGazeInteraction,
        Self::FbEyeTrackingSocial,
        Self::PicoEyeTracking,
        Self::HtcFacialTracking,
        Self::ExtHandTracking,
        Self::FbHandTrackingAim,
        Self::FbHandTrackingCapsules,
        Self::FbHandTrackingMesh,
        Self::HtcHandInteraction,
        Self::ExtHandJointsMotionRange,
        Self::FbBodyTracking,
        Self::HtcBodyTracking,
        Self::BdBodyTracking,
        Self::FbFaceTracking2,
        Self::FbDisplayRefreshRate,
        Self::FbColorSpace,
        Self::FbHapticPcm,
        Self::HtcViveTrackerInteraction,
        Self::ExtLocalFloor,
        Self::ExtDebugUtils,
        Self::ExtPerformanceSettings,
        Self::ExtThermalSettings,
        Self::ExtViewConfigurationDepthRange,
        Self::KhrVisibilityMask,
        Self::FbSpatialEntity,
        Self::FbScene,
    ];
}

impl fmt::Display for XrExtension {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Bitmask-style set of supported extensions. Used for advertise/require
/// diff'ing in the runtime-init path.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct XrExtensionSet {
    set: std::collections::BTreeSet<XrExtension>,
}

impl XrExtensionSet {
    /// Empty set.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            set: std::collections::BTreeSet::new(),
        }
    }

    /// Insert an extension. Returns `true` if newly inserted.
    pub fn insert(&mut self, ext: XrExtension) -> bool {
        self.set.insert(ext)
    }

    /// Remove an extension. Returns `true` if it was present.
    pub fn remove(&mut self, ext: XrExtension) -> bool {
        self.set.remove(&ext)
    }

    /// `true` iff the set contains the extension.
    #[must_use]
    pub fn contains(&self, ext: XrExtension) -> bool {
        self.set.contains(&ext)
    }

    /// Number of extensions in the set.
    #[must_use]
    pub fn len(&self) -> usize {
        self.set.len()
    }

    /// `true` iff empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.set.is_empty()
    }

    /// Iterate over the contained extensions in canonical order.
    pub fn iter(&self) -> impl Iterator<Item = XrExtension> + '_ {
        self.set.iter().copied()
    }

    /// All extensions required for Quest 3 day-one ship that are
    /// **missing** from this set.
    #[must_use]
    pub fn missing_required_quest3(&self) -> Vec<XrExtension> {
        XrExtension::ALL
            .iter()
            .copied()
            .filter(|e| e.is_required_quest3() && !self.contains(*e))
            .collect()
    }

    /// All extensions required for Vision Pro that are missing.
    #[must_use]
    pub fn missing_required_vision_pro(&self) -> Vec<XrExtension> {
        XrExtension::ALL
            .iter()
            .copied()
            .filter(|e| e.is_required_vision_pro() && !self.contains(*e))
            .collect()
    }

    /// All extensions required for Pimax Crystal Super that are missing.
    #[must_use]
    pub fn missing_required_pimax_crystal_super(&self) -> Vec<XrExtension> {
        XrExtension::ALL
            .iter()
            .copied()
            .filter(|e| e.is_required_pimax_crystal_super() && !self.contains(*e))
            .collect()
    }

    /// `true` iff every Quest 3 required extension is present.
    #[must_use]
    pub fn quest3_ready(&self) -> bool {
        self.missing_required_quest3().is_empty()
    }

    /// `true` iff every Vision Pro required extension is present.
    #[must_use]
    pub fn vision_pro_ready(&self) -> bool {
        self.missing_required_vision_pro().is_empty()
    }

    /// `true` iff every Pimax Crystal Super required extension is present.
    #[must_use]
    pub fn pimax_crystal_super_ready(&self) -> bool {
        self.missing_required_pimax_crystal_super().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::{XrExtension, XrExtensionSet};

    #[test]
    fn extension_names_are_canonical() {
        assert_eq!(
            XrExtension::KhrVulkanEnable2.name(),
            "XR_KHR_vulkan_enable2"
        );
        assert_eq!(XrExtension::FbSpaceWarp.name(), "XR_FB_space_warp");
        assert_eq!(
            XrExtension::MetaFoveationEyeTracked.name(),
            "XR_META_foveation_eye_tracked"
        );
        assert_eq!(XrExtension::VarjoQuadViews.name(), "XR_VARJO_quad_views");
        assert_eq!(
            XrExtension::ExtEyeGazeInteraction.name(),
            "XR_EXT_eye_gaze_interaction"
        );
        assert_eq!(XrExtension::FbBodyTracking.name(), "XR_FB_body_tracking");
        assert_eq!(XrExtension::FbFaceTracking2.name(), "XR_FB_face_tracking2");
    }

    #[test]
    fn from_name_roundtrip() {
        for ext in XrExtension::ALL {
            assert_eq!(XrExtension::from_name(ext.name()), Some(ext));
        }
    }

    #[test]
    fn from_name_unknown_returns_none() {
        assert!(XrExtension::from_name("XR_UNKNOWN_ext").is_none());
        assert!(XrExtension::from_name("").is_none());
    }

    #[test]
    fn quest3_required_set_is_canonical() {
        let required: Vec<_> = XrExtension::ALL
            .iter()
            .copied()
            .filter(|e| e.is_required_quest3())
            .collect();
        assert!(required.contains(&XrExtension::KhrVulkanEnable2));
        assert!(required.contains(&XrExtension::FbSpaceWarp));
        assert!(required.contains(&XrExtension::FbFoveation));
        assert!(required.contains(&XrExtension::FbPassthrough));
        assert!(required.contains(&XrExtension::ExtHandTracking));
    }

    #[test]
    fn vision_pro_required_minimal() {
        let required: Vec<_> = XrExtension::ALL
            .iter()
            .copied()
            .filter(|e| e.is_required_vision_pro())
            .collect();
        // visionOS uses Compositor-Services so the OpenXR-required set is
        // mostly composition-layer + tracking-equivalent surfaces.
        assert!(required.contains(&XrExtension::KhrCompositionLayerDepth));
        assert!(required.contains(&XrExtension::ExtHandTracking));
        assert!(required.contains(&XrExtension::FbColorSpace));
    }

    #[test]
    fn pimax_required_includes_quad_views() {
        assert!(XrExtension::VarjoQuadViews.is_required_pimax_crystal_super());
        assert!(XrExtension::VarjoFoveatedRendering.is_required_pimax_crystal_super());
        assert!(XrExtension::ExtEyeGazeInteraction.is_required_pimax_crystal_super());
        assert!(XrExtension::KhrD3D12Enable.is_required_pimax_crystal_super());
    }

    #[test]
    fn dfr_optional_set_classification() {
        assert!(XrExtension::MetaFoveationEyeTracked.is_optional_dfr());
        assert!(XrExtension::VarjoFoveatedRendering.is_optional_dfr());
        assert!(XrExtension::ExtEyeGazeInteraction.is_optional_dfr());
        assert!(!XrExtension::FbFoveation.is_optional_dfr()); // FFR is REQUIRED
    }

    #[test]
    fn passthrough_optional_set_classification() {
        assert!(XrExtension::FbPassthrough.is_optional_passthrough());
        assert!(XrExtension::MetaEnvironmentDepth.is_optional_passthrough());
        assert!(XrExtension::FbPassthroughKeyboardHands.is_optional_passthrough());
    }

    #[test]
    fn biometric_tracking_classification_covers_eye_hand_body_face() {
        assert!(XrExtension::ExtEyeGazeInteraction.is_biometric_tracking());
        assert!(XrExtension::ExtHandTracking.is_biometric_tracking());
        assert!(XrExtension::FbBodyTracking.is_biometric_tracking());
        assert!(XrExtension::FbFaceTracking2.is_biometric_tracking());
        // Foveation-config is NOT biometric (the foveation-rate-map is
        // derived from gaze on-device, but the rate-map itself is render-
        // side metadata, not a biometric sample).
        assert!(!XrExtension::FbFoveation.is_biometric_tracking());
    }

    #[test]
    fn set_insert_remove() {
        let mut s = XrExtensionSet::empty();
        assert!(s.is_empty());
        assert!(s.insert(XrExtension::FbSpaceWarp));
        assert!(!s.insert(XrExtension::FbSpaceWarp)); // dup
        assert!(s.contains(XrExtension::FbSpaceWarp));
        assert_eq!(s.len(), 1);
        assert!(s.remove(XrExtension::FbSpaceWarp));
        assert!(!s.remove(XrExtension::FbSpaceWarp));
        assert!(s.is_empty());
    }

    #[test]
    fn quest3_ready_only_when_full_required_set() {
        let mut s = XrExtensionSet::empty();
        for ext in XrExtension::ALL {
            if ext.is_required_quest3() {
                s.insert(ext);
            }
        }
        assert!(s.quest3_ready());
        assert!(s.missing_required_quest3().is_empty());
    }

    #[test]
    fn quest3_ready_false_when_missing_one() {
        let mut s = XrExtensionSet::empty();
        for ext in XrExtension::ALL {
            if ext.is_required_quest3() && ext != XrExtension::FbSpaceWarp {
                s.insert(ext);
            }
        }
        assert!(!s.quest3_ready());
        let missing = s.missing_required_quest3();
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0], XrExtension::FbSpaceWarp);
    }

    #[test]
    fn pimax_ready_distinct_from_quest3_ready() {
        let mut s = XrExtensionSet::empty();
        for ext in XrExtension::ALL {
            if ext.is_required_quest3() {
                s.insert(ext);
            }
        }
        // Quest3 ready but missing Pimax-specific (quad-views, D3D12).
        assert!(s.quest3_ready());
        assert!(!s.pimax_crystal_super_ready());
    }

    #[test]
    fn all_table_size() {
        assert_eq!(XrExtension::ALL.len(), 50);
    }

    #[test]
    fn all_table_unique_names() {
        let mut seen = std::collections::HashSet::new();
        for ext in XrExtension::ALL {
            assert!(seen.insert(ext.name()), "dup name : {}", ext.name());
        }
    }
}
