//! `XrInstance` : OpenXR instance lifecycle.
//!
//! § SPEC § IV.A : `xrCreateInstance` is the entry-point. The instance
//! carries the negotiated extension-set + the runtime version + the
//! application-info.
//!
//! § STAGE-0 SCOPE
//!   This module ships the **engine-side abstraction** : the `XrInstance`
//!   struct + builder + lifecycle-state-machine. The actual FFI to
//!   `xrCreateInstance` lives in the FFI follow-up slice (gated behind
//!   the `vulkan-binding` / `d3d12-binding` cargo-feature). At stage-0
//!   the headless build constructs a `MockInstance` that records the
//!   negotiation but does not contact a real runtime.

use crate::error::XRFailure;
use crate::extensions::XrExtensionSet;
use crate::runtime_select::{XrRuntime, XrTarget};

/// OpenXR API version, packed `(major << 48) | (minor << 32) | patch`.
/// Convenience aliases via [`Self::current`] etc.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct XrApiVersion(pub u64);

impl XrApiVersion {
    /// Build from `(major, minor, patch)`.
    #[must_use]
    pub const fn new(major: u16, minor: u16, patch: u32) -> Self {
        Self(((major as u64) << 48) | ((minor as u64) << 32) | (patch as u64))
    }

    /// OpenXR 1.0.0 (the earliest mainline release).
    pub const V_1_0_0: Self = Self::new(1, 0, 0);

    /// OpenXR 1.0.30 — first to ship `XR_KHR_vulkan_enable2`.
    pub const V_1_0_30: Self = Self::new(1, 0, 30);

    /// OpenXR 1.1.0 — current cross-vendor tip @ 2026-04.
    pub const V_1_1_0: Self = Self::new(1, 1, 0);

    /// Current default version the engine targets (1.1.0 floor).
    #[must_use]
    pub const fn current() -> Self {
        Self::V_1_1_0
    }

    /// Major version component.
    #[must_use]
    pub const fn major(self) -> u16 {
        (self.0 >> 48) as u16
    }

    /// Minor version component.
    #[must_use]
    pub const fn minor(self) -> u16 {
        ((self.0 >> 32) & 0xFFFF) as u16
    }

    /// Patch version component.
    #[must_use]
    pub const fn patch(self) -> u32 {
        (self.0 & 0xFFFF_FFFF) as u32
    }
}

/// Application-info passed at `xrCreateInstance`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AppInfo {
    /// Application name (NUL-terminated by FFI layer).
    pub app_name: String,
    /// Application version.
    pub app_version: u32,
    /// Engine name.
    pub engine_name: String,
    /// Engine version.
    pub engine_version: u32,
    /// API version the engine is targeting.
    pub api_version: XrApiVersion,
}

impl AppInfo {
    /// Default app-info for a CSSLv3 host application.
    #[must_use]
    pub fn cssl_default() -> Self {
        Self {
            app_name: "cssl-host-openxr".to_string(),
            app_version: 1,
            engine_name: "CSSLv3".to_string(),
            engine_version: 1,
            api_version: XrApiVersion::current(),
        }
    }
}

/// Builder for `XrInstance`.
#[derive(Debug, Clone)]
pub struct XrInstanceBuilder {
    target: XrTarget,
    app_info: AppInfo,
    required_extensions: XrExtensionSet,
    optional_extensions: XrExtensionSet,
    enable_debug_utils: bool,
}

impl XrInstanceBuilder {
    /// New builder seeded with `target`'s default required extensions.
    #[must_use]
    pub fn new(target: XrTarget) -> Self {
        Self {
            target,
            app_info: AppInfo::cssl_default(),
            required_extensions: target.default_required_extensions(),
            optional_extensions: XrExtensionSet::empty(),
            enable_debug_utils: cfg!(debug_assertions),
        }
    }

    /// Override the app-info.
    #[must_use]
    pub fn with_app_info(mut self, info: AppInfo) -> Self {
        self.app_info = info;
        self
    }

    /// Add a required-extension. Negotiation fails if missing.
    #[must_use]
    pub fn require(mut self, ext: crate::extensions::XrExtension) -> Self {
        self.required_extensions.insert(ext);
        self
    }

    /// Add an optional-extension. Negotiation succeeds even if missing
    /// (engine degrades gracefully).
    #[must_use]
    pub fn optional(mut self, ext: crate::extensions::XrExtension) -> Self {
        self.optional_extensions.insert(ext);
        self
    }

    /// Enable or disable `XR_EXT_debug_utils`. Defaults to `cfg(debug_assertions)`.
    #[must_use]
    pub const fn with_debug_utils(mut self, enable: bool) -> Self {
        self.enable_debug_utils = enable;
        self
    }

    /// Build a stage-0 `MockInstance` that records negotiation but does
    /// not contact a real runtime. The FFI follow-up slice supersedes
    /// this with a real `XrInstance`.
    pub fn build_mock(self) -> Result<MockInstance, XRFailure> {
        let advertised = mock_advertised_for_target(self.target);
        // Verify required extensions are advertised.
        let mut missing = Vec::new();
        for ext in self.required_extensions.iter() {
            if !advertised.contains(ext) {
                missing.push(ext);
            }
        }
        if !missing.is_empty() {
            return Err(XRFailure::MissingRequiredExtension(missing[0].name()));
        }
        let mut enabled = self.required_extensions.clone();
        for ext in self.optional_extensions.iter() {
            if advertised.contains(ext) {
                enabled.insert(ext);
            }
        }
        if self.enable_debug_utils
            && advertised.contains(crate::extensions::XrExtension::ExtDebugUtils)
        {
            enabled.insert(crate::extensions::XrExtension::ExtDebugUtils);
        }
        Ok(MockInstance {
            target: self.target,
            runtime: self.target.runtime(),
            app_info: self.app_info,
            advertised_extensions: advertised,
            enabled_extensions: enabled,
            api_version: XrApiVersion::current(),
        })
    }
}

/// A stage-0 mock-instance that records the result of the negotiation
/// without contacting a real runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MockInstance {
    /// The target that was negotiated for.
    pub target: XrTarget,
    /// The runtime the target maps to.
    pub runtime: XrRuntime,
    /// App-info that was passed at create-time.
    pub app_info: AppInfo,
    /// What the (mock) runtime advertised.
    pub advertised_extensions: XrExtensionSet,
    /// What the negotiation actually enabled
    /// (required ∪ optional∩advertised).
    pub enabled_extensions: XrExtensionSet,
    /// API version the (mock) runtime advertised.
    pub api_version: XrApiVersion,
}

impl MockInstance {
    /// Quick-build for tests : Quest3 default profile.
    pub fn quest3_default() -> Result<Self, XRFailure> {
        XrInstanceBuilder::new(XrTarget::Quest3).build_mock()
    }

    /// Quick-build for tests : VisionPro default profile.
    pub fn vision_pro_default() -> Result<Self, XRFailure> {
        XrInstanceBuilder::new(XrTarget::VisionPro).build_mock()
    }

    /// Quick-build for tests : Pimax Crystal Super default profile.
    pub fn pimax_crystal_super_default() -> Result<Self, XRFailure> {
        XrInstanceBuilder::new(XrTarget::PimaxCrystalSuper).build_mock()
    }

    /// Quick-build for tests : flat-monitor degenerate-case.
    pub fn flat_monitor_default() -> Result<Self, XRFailure> {
        XrInstanceBuilder::new(XrTarget::FlatMonitor).build_mock()
    }
}

/// Build the (mock) advertised extension-set for a given target. The
/// FFI follow-up slice supersedes this with `xrEnumerateInstanceExtensionProperties`.
fn mock_advertised_for_target(target: XrTarget) -> XrExtensionSet {
    use crate::extensions::XrExtension;
    let mut s = XrExtensionSet::empty();
    // Always-advertise core base across all runtimes.
    s.insert(XrExtension::KhrCompositionLayerDepth);
    s.insert(XrExtension::KhrCompositionLayerQuad);
    s.insert(XrExtension::KhrCompositionLayerCylinder);
    s.insert(XrExtension::KhrCompositionLayerEquirect2);
    s.insert(XrExtension::KhrCompositionLayerCube);
    s.insert(XrExtension::ExtDebugUtils);
    s.insert(XrExtension::FbColorSpace);
    s.insert(XrExtension::FbDisplayRefreshRate);
    s.insert(XrExtension::ExtLocalFloor);
    match target {
        XrTarget::Quest3 | XrTarget::Quest3S | XrTarget::QuestPro => {
            s.insert(XrExtension::KhrVulkanEnable2);
            s.insert(XrExtension::FbFoveation);
            s.insert(XrExtension::FbFoveationConfiguration);
            s.insert(XrExtension::FbFoveationVulkan);
            s.insert(XrExtension::FbSpaceWarp);
            s.insert(XrExtension::FbPassthrough);
            s.insert(XrExtension::FbPassthroughKeyboardHands);
            s.insert(XrExtension::MetaEnvironmentDepth);
            s.insert(XrExtension::ExtEyeGazeInteraction);
            s.insert(XrExtension::FbEyeTrackingSocial);
            s.insert(XrExtension::ExtHandTracking);
            s.insert(XrExtension::FbHandTrackingAim);
            s.insert(XrExtension::FbHandTrackingCapsules);
            s.insert(XrExtension::FbBodyTracking);
            s.insert(XrExtension::FbFaceTracking2);
            s.insert(XrExtension::MetaFoveationEyeTracked);
            s.insert(XrExtension::FbHapticPcm);
            s.insert(XrExtension::FbSpatialEntity);
            s.insert(XrExtension::FbScene);
        }
        XrTarget::Quest2 => {
            // Quest 2 advertises the same required-set as Quest3 (the
            // engine treats them as the Meta-Quest path) but lacks the
            // optional eye-tracked-foveation + face-tracking2 + body-
            // tracking advertisements ⊗ those degrade gracefully.
            s.insert(XrExtension::KhrVulkanEnable2);
            s.insert(XrExtension::FbFoveation);
            s.insert(XrExtension::FbFoveationConfiguration);
            s.insert(XrExtension::FbFoveationVulkan);
            s.insert(XrExtension::FbSpaceWarp);
            s.insert(XrExtension::FbPassthrough);
            s.insert(XrExtension::ExtHandTracking);
        }
        XrTarget::VisionPro => {
            // visionOS via Compositor-Services bridge ; advertised set
            // surfaces what the bridge presents as "OpenXR-equivalent".
            s.insert(XrExtension::ExtHandTracking);
            s.insert(XrExtension::FbColorSpace);
        }
        XrTarget::PimaxCrystalSuper => {
            s.insert(XrExtension::KhrVulkanEnable2);
            s.insert(XrExtension::KhrD3D12Enable);
            s.insert(XrExtension::VarjoQuadViews);
            s.insert(XrExtension::VarjoFoveatedRendering);
            s.insert(XrExtension::ExtEyeGazeInteraction);
            s.insert(XrExtension::ExtHandTracking);
        }
        XrTarget::Pico4Ultra => {
            s.insert(XrExtension::KhrVulkanEnable2);
            s.insert(XrExtension::FbFoveation);
            s.insert(XrExtension::PicoEyeTracking);
            s.insert(XrExtension::ExtHandTracking);
            s.insert(XrExtension::BdBodyTracking);
        }
        XrTarget::HtcViveXrElite | XrTarget::HtcViveFocusVision => {
            s.insert(XrExtension::KhrVulkanEnable2);
            s.insert(XrExtension::HtcPassthrough);
            s.insert(XrExtension::HtcFacialTracking);
            s.insert(XrExtension::HtcHandInteraction);
            s.insert(XrExtension::HtcBodyTracking);
        }
        XrTarget::ValveIndex | XrTarget::BigscreenBeyond | XrTarget::BigscreenBeyond2 => {
            s.insert(XrExtension::KhrVulkanEnable2);
            s.insert(XrExtension::FbFoveation);
        }
        XrTarget::VarjoXr3 | XrTarget::VarjoXr4 => {
            s.insert(XrExtension::KhrVulkanEnable2);
            s.insert(XrExtension::KhrD3D12Enable);
            s.insert(XrExtension::VarjoQuadViews);
            s.insert(XrExtension::VarjoFoveatedRendering);
            s.insert(XrExtension::ExtEyeGazeInteraction);
            s.insert(XrExtension::ExtHandTracking);
        }
        XrTarget::FlatMonitor => {
            // No XR extensions advertised : flat-monitor is rendered by
            // `cssl-render` directly with viewCount = 1.
        }
        XrTarget::FutureMirrorLake => {
            // 5-yr ; placeholder advertisement : assume everything.
            for ext in XrExtension::ALL {
                s.insert(ext);
            }
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::{AppInfo, MockInstance, XrApiVersion, XrInstanceBuilder};
    use crate::error::XRFailure;
    use crate::extensions::XrExtension;
    use crate::runtime_select::XrTarget;

    #[test]
    fn api_version_packing() {
        let v = XrApiVersion::new(1, 1, 30);
        assert_eq!(v.major(), 1);
        assert_eq!(v.minor(), 1);
        assert_eq!(v.patch(), 30);
    }

    #[test]
    fn api_version_constants() {
        assert_eq!(XrApiVersion::V_1_0_0.major(), 1);
        assert_eq!(XrApiVersion::V_1_0_30.minor(), 0);
        assert_eq!(XrApiVersion::V_1_0_30.patch(), 30);
        assert_eq!(XrApiVersion::V_1_1_0.minor(), 1);
    }

    #[test]
    fn cssl_default_app_info() {
        let info = AppInfo::cssl_default();
        assert_eq!(info.app_name, "cssl-host-openxr");
        assert_eq!(info.engine_name, "CSSLv3");
        assert_eq!(info.api_version, XrApiVersion::current());
    }

    #[test]
    fn quest3_default_instance_builds() {
        let inst = MockInstance::quest3_default().unwrap();
        assert!(inst.enabled_extensions.contains(XrExtension::FbSpaceWarp));
        assert!(inst.enabled_extensions.contains(XrExtension::FbFoveation));
        assert!(inst.enabled_extensions.contains(XrExtension::FbPassthrough));
        assert!(inst
            .enabled_extensions
            .contains(XrExtension::ExtHandTracking));
    }

    #[test]
    fn vision_pro_default_instance_builds() {
        let inst = MockInstance::vision_pro_default().unwrap();
        assert!(inst
            .enabled_extensions
            .contains(XrExtension::ExtHandTracking));
        assert!(inst.runtime.is_compositor_services_bridge());
    }

    #[test]
    fn pimax_default_instance_builds() {
        let inst = MockInstance::pimax_crystal_super_default().unwrap();
        assert!(inst
            .enabled_extensions
            .contains(XrExtension::VarjoQuadViews));
        assert!(inst
            .enabled_extensions
            .contains(XrExtension::KhrD3D12Enable));
    }

    #[test]
    fn flat_monitor_instance_builds_with_minimal_extensions() {
        let inst = MockInstance::flat_monitor_default().unwrap();
        // FlatMonitor has no required extensions ; the only thing that
        // gets enabled is debug-utils when cfg(debug_assertions) is set
        // and the (mock) runtime advertises it. In release builds the
        // set is empty.
        if cfg!(debug_assertions) {
            assert!(inst
                .enabled_extensions
                .contains(crate::extensions::XrExtension::ExtDebugUtils));
        } else {
            assert!(inst.enabled_extensions.is_empty());
        }
    }

    #[test]
    fn missing_required_extension_fails_build() {
        // Force a require() of an extension that the mock advertise-table
        // for FlatMonitor will NOT include.
        let err = XrInstanceBuilder::new(XrTarget::FlatMonitor)
            .require(XrExtension::FbSpaceWarp)
            .build_mock()
            .unwrap_err();
        assert!(matches!(
            err,
            XRFailure::MissingRequiredExtension(name) if name == "XR_FB_space_warp"
        ));
    }

    #[test]
    fn optional_extensions_added_when_advertised() {
        let inst = XrInstanceBuilder::new(XrTarget::Quest3)
            .optional(XrExtension::FbHapticPcm)
            .build_mock()
            .unwrap();
        assert!(inst.enabled_extensions.contains(XrExtension::FbHapticPcm));
    }

    #[test]
    fn optional_extensions_skipped_when_not_advertised() {
        let inst = XrInstanceBuilder::new(XrTarget::Quest2)
            .optional(XrExtension::MetaFoveationEyeTracked)
            .build_mock()
            .unwrap();
        // Quest 2 doesn't advertise eye-tracked foveation.
        assert!(!inst
            .enabled_extensions
            .contains(XrExtension::MetaFoveationEyeTracked));
    }

    #[test]
    fn debug_utils_default_for_debug_builds() {
        let inst = XrInstanceBuilder::new(XrTarget::Quest3)
            .build_mock()
            .unwrap();
        // Debug builds enable, release builds don't (cfg-gated).
        assert_eq!(
            inst.enabled_extensions.contains(XrExtension::ExtDebugUtils),
            cfg!(debug_assertions)
        );
    }
}
