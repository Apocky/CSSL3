//! Runtime + form-factor + target detection.
//!
//! § SPEC : `07_AESTHETIC/05_VR_RENDERING.csl` § II.A + § II.B + § IV
//! (visionOS exception : Compositor-Services bridge).
//!
//! § DESIGN
//!   `XrRuntime` enumerates known XR runtimes (Quest, Vision Pro, Pimax,
//!   SteamVR, Pico, HTC, Valve Index, Bigscreen, Varjo). The engine
//!   queries `xrEnumerateRuntime` (FFI follow-up) to populate this ; the
//!   stage-0 catalog seed is from spec § II.A + § II.B.
//!
//!   `XrTarget` represents the *engine's* day-one + secondary-day-one
//!   targets. Each carries a default extension-require set and a
//!   default `RefreshRatePolicy`.

use crate::extensions::{XrExtension, XrExtensionSet};

/// Known OpenXR runtime identities. § II.A + § II.B.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum XrRuntime {
    /// Meta Quest OS OpenXR runtime (Quest 2 / Pro / 3 / 3S).
    MetaQuest,
    /// Apple Vision Pro Compositor-Services bridge (¬ OpenXR-direct).
    AppleVisionPro,
    /// Pimax — PimaxXR.
    PimaxXR,
    /// Valve SteamVR-OpenXR (PC tethered).
    SteamVR,
    /// Pico OpenXR runtime.
    PicoXR,
    /// HTC Vive XR Elite / Vive Focus Vision OpenXR runtime.
    Htc,
    /// Varjo OpenXR runtime (XR-3 / XR-4).
    Varjo,
    /// Bigscreen Beyond — uses SteamVR-OpenXR.
    BigscreenBeyond,
    /// Monado (open-source reference OpenXR runtime).
    Monado,
    /// Mock-runtime for stage-0 testing (no real device).
    MockTestRuntime,
}

impl XrRuntime {
    /// Display-name (canonical, stable).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MetaQuest => "meta-quest",
            Self::AppleVisionPro => "apple-vision-pro",
            Self::PimaxXR => "pimax-xr",
            Self::SteamVR => "steamvr",
            Self::PicoXR => "pico-xr",
            Self::Htc => "htc",
            Self::Varjo => "varjo",
            Self::BigscreenBeyond => "bigscreen-beyond",
            Self::Monado => "monado",
            Self::MockTestRuntime => "mock-test-runtime",
        }
    }

    /// `true` iff this runtime is implemented via the visionOS
    /// Compositor-Services bridge (¬ OpenXR-direct).
    #[must_use]
    pub const fn is_compositor_services_bridge(self) -> bool {
        matches!(self, Self::AppleVisionPro)
    }

    /// `true` iff this runtime is a "mobile-standalone" form-factor
    /// (no external GPU). Drives default AppSW-engagement.
    #[must_use]
    pub const fn is_mobile_standalone(self) -> bool {
        matches!(self, Self::MetaQuest | Self::PicoXR | Self::AppleVisionPro)
    }

    /// `true` iff this runtime is "PCVR-tethered" (external GPU).
    #[must_use]
    pub const fn is_pcvr_tethered(self) -> bool {
        matches!(
            self,
            Self::PimaxXR
                | Self::SteamVR
                | Self::Varjo
                | Self::BigscreenBeyond
                | Self::Monado
                | Self::Htc
        )
    }
}

/// Engine-side target categories. § II.A (day-one) + § II.B (secondary).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum XrTarget {
    /// Meta Quest 3 — DAY-ONE tier-1 reference.
    Quest3,
    /// Meta Quest 3S — secondary-day-one (Quest3 path with foveation diffs).
    Quest3S,
    /// Meta Quest 2 — secondary-day-one.
    Quest2,
    /// Meta Quest Pro — secondary-day-one.
    QuestPro,
    /// Apple Vision Pro — DAY-ONE tier-1 reference.
    VisionPro,
    /// Pimax Crystal Super — DAY-ONE tier-1 reference (PCVR enthusiast).
    PimaxCrystalSuper,
    /// Pico 4 Ultra — secondary-day-one.
    Pico4Ultra,
    /// HTC Vive XR Elite — secondary-day-one.
    HtcViveXrElite,
    /// HTC Vive Focus Vision — secondary-day-one.
    HtcViveFocusVision,
    /// Valve Index — secondary-day-one (FFR only ; no eye-track).
    ValveIndex,
    /// Varjo XR-3 — secondary-day-one (canonical quad-view ref impl).
    VarjoXr3,
    /// Varjo XR-4 — secondary-day-one.
    VarjoXr4,
    /// Bigscreen Beyond (stock) — secondary-day-one.
    BigscreenBeyond,
    /// Bigscreen Beyond 2 — secondary-day-one.
    BigscreenBeyond2,
    /// Flat-monitor degenerate-case (viewCount = 1 ; same render-graph).
    FlatMonitor,
    /// Future Mirror-Lake-class — 5-yr-out forward-compat (§ II.C).
    FutureMirrorLake,
}

impl XrTarget {
    /// Display-name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Quest3 => "quest-3",
            Self::Quest3S => "quest-3s",
            Self::Quest2 => "quest-2",
            Self::QuestPro => "quest-pro",
            Self::VisionPro => "vision-pro",
            Self::PimaxCrystalSuper => "pimax-crystal-super",
            Self::Pico4Ultra => "pico-4-ultra",
            Self::HtcViveXrElite => "htc-vive-xr-elite",
            Self::HtcViveFocusVision => "htc-vive-focus-vision",
            Self::ValveIndex => "valve-index",
            Self::VarjoXr3 => "varjo-xr-3",
            Self::VarjoXr4 => "varjo-xr-4",
            Self::BigscreenBeyond => "bigscreen-beyond",
            Self::BigscreenBeyond2 => "bigscreen-beyond-2",
            Self::FlatMonitor => "flat-monitor",
            Self::FutureMirrorLake => "future-mirror-lake",
        }
    }

    /// `true` iff this target is on the **DAY-ONE tier-1 ship-list**.
    /// § II.A.
    #[must_use]
    pub const fn is_day_one_tier_1(self) -> bool {
        matches!(self, Self::Quest3 | Self::VisionPro | Self::PimaxCrystalSuper)
    }

    /// `true` iff this target is on the secondary-day-one list. § II.B.
    #[must_use]
    pub const fn is_secondary_day_one(self) -> bool {
        matches!(
            self,
            Self::Quest3S
                | Self::Quest2
                | Self::QuestPro
                | Self::Pico4Ultra
                | Self::HtcViveXrElite
                | Self::HtcViveFocusVision
                | Self::ValveIndex
                | Self::VarjoXr3
                | Self::VarjoXr4
                | Self::BigscreenBeyond
                | Self::BigscreenBeyond2
                | Self::FlatMonitor
        )
    }

    /// `true` iff this target is the 5-yr-out forward-compat hook
    /// (Mirror-Lake-class).
    #[must_use]
    pub const fn is_5yr_future(self) -> bool {
        matches!(self, Self::FutureMirrorLake)
    }

    /// Mapped `XrRuntime` for this target (§ II.A + § II.B).
    #[must_use]
    pub const fn runtime(self) -> XrRuntime {
        match self {
            Self::Quest3 | Self::Quest3S | Self::Quest2 | Self::QuestPro => XrRuntime::MetaQuest,
            Self::VisionPro => XrRuntime::AppleVisionPro,
            Self::PimaxCrystalSuper => XrRuntime::PimaxXR,
            Self::Pico4Ultra => XrRuntime::PicoXR,
            Self::HtcViveXrElite | Self::HtcViveFocusVision => XrRuntime::Htc,
            Self::ValveIndex | Self::BigscreenBeyond | Self::BigscreenBeyond2 => {
                XrRuntime::SteamVR
            }
            Self::VarjoXr3 | Self::VarjoXr4 => XrRuntime::Varjo,
            Self::FlatMonitor => XrRuntime::MockTestRuntime,
            Self::FutureMirrorLake => XrRuntime::Monado,
        }
    }

    /// Default required-extension set for this target.
    #[must_use]
    pub fn default_required_extensions(self) -> XrExtensionSet {
        let mut set = XrExtensionSet::empty();
        match self {
            Self::Quest3 | Self::Quest3S | Self::Quest2 | Self::QuestPro => {
                for ext in XrExtension::ALL {
                    if ext.is_required_quest3() {
                        set.insert(ext);
                    }
                }
            }
            Self::VisionPro => {
                for ext in XrExtension::ALL {
                    if ext.is_required_vision_pro() {
                        set.insert(ext);
                    }
                }
            }
            Self::PimaxCrystalSuper | Self::VarjoXr3 | Self::VarjoXr4 => {
                for ext in XrExtension::ALL {
                    if ext.is_required_pimax_crystal_super() {
                        set.insert(ext);
                    }
                }
            }
            Self::Pico4Ultra => {
                set.insert(XrExtension::KhrVulkanEnable2);
                set.insert(XrExtension::KhrCompositionLayerDepth);
                set.insert(XrExtension::FbFoveation);
                set.insert(XrExtension::ExtHandTracking);
            }
            Self::HtcViveXrElite | Self::HtcViveFocusVision => {
                set.insert(XrExtension::KhrVulkanEnable2);
                set.insert(XrExtension::KhrCompositionLayerDepth);
                set.insert(XrExtension::HtcHandInteraction);
            }
            Self::ValveIndex | Self::BigscreenBeyond | Self::BigscreenBeyond2 => {
                set.insert(XrExtension::KhrVulkanEnable2);
                set.insert(XrExtension::KhrCompositionLayerDepth);
                set.insert(XrExtension::FbFoveation); // FFR-only via SteamVR-OpenXR
            }
            Self::FlatMonitor => {
                // Flat-monitor : no XR runtime required ; viewCount = 1
                // degenerate path runs entirely in `cssl-render`.
            }
            Self::FutureMirrorLake => {
                // 5-yr placeholder ; required extensions TBD when the
                // hardware lands.
            }
        }
        set
    }

    /// Default refresh-rate floor for this target (§ XII.A).
    /// Returns Hz as `f32`.
    #[must_use]
    pub const fn refresh_rate_floor_hz(self) -> f32 {
        match self {
            Self::Quest3
            | Self::Quest3S
            | Self::QuestPro
            | Self::PimaxCrystalSuper
            | Self::Pico4Ultra
            | Self::HtcViveXrElite
            | Self::HtcViveFocusVision
            | Self::ValveIndex
            | Self::VarjoXr3
            | Self::VarjoXr4
            | Self::BigscreenBeyond
            | Self::BigscreenBeyond2 => 90.0,
            Self::Quest2 => 72.0,
            Self::VisionPro => 90.0,
            Self::FlatMonitor => 60.0,
            Self::FutureMirrorLake => 144.0, // § II.C "comfortable-floor-of-the-future"
        }
    }

    /// Default native panel-refresh-rate (selectable max) (§ II.A + B).
    #[must_use]
    pub const fn native_refresh_rate_hz(self) -> f32 {
        match self {
            Self::Quest3 | Self::Quest3S | Self::QuestPro => 120.0,
            Self::Quest2 => 90.0,
            Self::VisionPro => 100.0,
            Self::PimaxCrystalSuper | Self::VarjoXr3 | Self::VarjoXr4 => 90.0,
            Self::Pico4Ultra => 90.0,
            Self::HtcViveXrElite | Self::HtcViveFocusVision => 90.0,
            Self::ValveIndex => 144.0,
            Self::BigscreenBeyond => 75.0,
            Self::BigscreenBeyond2 => 90.0,
            Self::FlatMonitor => 144.0,
            Self::FutureMirrorLake => 240.0,
        }
    }

    /// `true` iff AppSW (XR_FB_space_warp) is REQUIRED-shipped on this target.
    /// § II.A Quest 3 quirk : "mobile-GPU = ½-rate render-path is mandatory not optional".
    #[must_use]
    pub const fn requires_app_sw(self) -> bool {
        matches!(
            self,
            Self::Quest3 | Self::Quest3S | Self::Quest2 | Self::QuestPro | Self::Pico4Ultra
        )
    }

    /// `true` iff DFR (eye-tracked dynamic foveation) is engaged-by-default
    /// on this target. § V.B.
    #[must_use]
    pub const fn dfr_default_engaged(self) -> bool {
        matches!(
            self,
            Self::QuestPro
                | Self::Quest3
                | Self::VisionPro
                | Self::PimaxCrystalSuper
                | Self::VarjoXr3
                | Self::VarjoXr4
        )
    }

    /// All catalog targets.
    pub const ALL: [Self; 16] = [
        Self::Quest3,
        Self::Quest3S,
        Self::Quest2,
        Self::QuestPro,
        Self::VisionPro,
        Self::PimaxCrystalSuper,
        Self::Pico4Ultra,
        Self::HtcViveXrElite,
        Self::HtcViveFocusVision,
        Self::ValveIndex,
        Self::VarjoXr3,
        Self::VarjoXr4,
        Self::BigscreenBeyond,
        Self::BigscreenBeyond2,
        Self::FlatMonitor,
        Self::FutureMirrorLake,
    ];

    /// All targets on the day-one tier-1 ship-list.
    pub const DAY_ONE_TIER_1: [Self; 3] =
        [Self::Quest3, Self::VisionPro, Self::PimaxCrystalSuper];
}

#[cfg(test)]
mod tests {
    use super::{XrRuntime, XrTarget};

    #[test]
    fn day_one_tier_1_is_three_targets() {
        let tier_1: Vec<_> = XrTarget::ALL
            .iter()
            .copied()
            .filter(|t| t.is_day_one_tier_1())
            .collect();
        assert_eq!(tier_1.len(), 3);
        assert!(tier_1.contains(&XrTarget::Quest3));
        assert!(tier_1.contains(&XrTarget::VisionPro));
        assert!(tier_1.contains(&XrTarget::PimaxCrystalSuper));
    }

    #[test]
    fn day_one_tier_1_constant_matches_predicate() {
        let predicate: Vec<_> = XrTarget::ALL
            .iter()
            .copied()
            .filter(|t| t.is_day_one_tier_1())
            .collect();
        let mut sorted_pred = predicate;
        sorted_pred.sort();
        let mut sorted_const: Vec<_> = XrTarget::DAY_ONE_TIER_1.to_vec();
        sorted_const.sort();
        assert_eq!(sorted_pred, sorted_const);
    }

    #[test]
    fn vision_pro_uses_compositor_services_bridge() {
        assert_eq!(XrTarget::VisionPro.runtime(), XrRuntime::AppleVisionPro);
        assert!(XrTarget::VisionPro.runtime().is_compositor_services_bridge());
    }

    #[test]
    fn quest3_uses_meta_quest_runtime() {
        assert_eq!(XrTarget::Quest3.runtime(), XrRuntime::MetaQuest);
        assert!(!XrTarget::Quest3.runtime().is_compositor_services_bridge());
    }

    #[test]
    fn pimax_uses_pimax_xr_runtime() {
        assert_eq!(XrTarget::PimaxCrystalSuper.runtime(), XrRuntime::PimaxXR);
        assert!(XrTarget::PimaxCrystalSuper.runtime().is_pcvr_tethered());
    }

    #[test]
    fn quest3_required_extensions_includes_space_warp_and_foveation() {
        let set = XrTarget::Quest3.default_required_extensions();
        assert!(set.contains(crate::extensions::XrExtension::FbSpaceWarp));
        assert!(set.contains(crate::extensions::XrExtension::FbFoveation));
        assert!(set.contains(crate::extensions::XrExtension::FbPassthrough));
    }

    #[test]
    fn pimax_required_extensions_includes_quad_views() {
        let set = XrTarget::PimaxCrystalSuper.default_required_extensions();
        assert!(set.contains(crate::extensions::XrExtension::VarjoQuadViews));
        assert!(set.contains(crate::extensions::XrExtension::KhrD3D12Enable));
    }

    #[test]
    fn flat_monitor_target_has_no_required_extensions() {
        let set = XrTarget::FlatMonitor.default_required_extensions();
        assert!(set.is_empty());
    }

    #[test]
    fn refresh_rate_floor_at_least_90hz_for_xr_targets() {
        for t in XrTarget::ALL {
            if t.is_day_one_tier_1() {
                assert!(t.refresh_rate_floor_hz() >= 90.0, "{:?}", t);
            }
        }
    }

    #[test]
    fn flat_monitor_floor_is_60hz() {
        assert!((XrTarget::FlatMonitor.refresh_rate_floor_hz() - 60.0).abs() < f32::EPSILON);
    }

    #[test]
    fn future_mirror_lake_floor_is_144hz() {
        assert!((XrTarget::FutureMirrorLake.refresh_rate_floor_hz() - 144.0).abs() < f32::EPSILON);
    }

    #[test]
    fn app_sw_required_on_mobile_standalone() {
        assert!(XrTarget::Quest3.requires_app_sw());
        assert!(XrTarget::Quest2.requires_app_sw());
        assert!(XrTarget::Pico4Ultra.requires_app_sw());
        assert!(!XrTarget::PimaxCrystalSuper.requires_app_sw());
        assert!(!XrTarget::VisionPro.requires_app_sw()); // visionOS internally
    }

    #[test]
    fn dfr_default_engaged_on_eye_tracked_targets() {
        assert!(XrTarget::QuestPro.dfr_default_engaged());
        assert!(XrTarget::Quest3.dfr_default_engaged());
        assert!(XrTarget::VisionPro.dfr_default_engaged());
        assert!(XrTarget::PimaxCrystalSuper.dfr_default_engaged());
        assert!(!XrTarget::ValveIndex.dfr_default_engaged()); // no eye-track
        assert!(!XrTarget::FlatMonitor.dfr_default_engaged());
    }

    #[test]
    fn all_targets_unique() {
        let mut seen = std::collections::HashSet::new();
        for t in XrTarget::ALL {
            assert!(seen.insert(t));
        }
        assert_eq!(seen.len(), 16);
    }

    #[test]
    fn runtime_as_str_canonical() {
        assert_eq!(XrRuntime::MetaQuest.as_str(), "meta-quest");
        assert_eq!(XrRuntime::AppleVisionPro.as_str(), "apple-vision-pro");
        assert_eq!(XrRuntime::PimaxXR.as_str(), "pimax-xr");
    }

    #[test]
    fn target_as_str_canonical() {
        assert_eq!(XrTarget::Quest3.as_str(), "quest-3");
        assert_eq!(XrTarget::VisionPro.as_str(), "vision-pro");
        assert_eq!(XrTarget::PimaxCrystalSuper.as_str(), "pimax-crystal-super");
    }
}
