//! CSSLv3 stage0 — OpenXR runtime + Compositor-Services bridge for VR/XR
//! day-one shipping (T11-D124).
//!
//! § SPEC (canonical) :
//!   - `07_AESTHETIC/05_VR_RENDERING.csl`  — VR-as-canonical render-target
//!   - `08_BODY/02_VR_EMBODIMENT.csl`     — embodiment + Soul-link safeguard
//!   - PRIME_DIRECTIVE §1 (anti-surveillance) — biometric-egress refusal
//!   - PRIME_DIRECTIVE §11 (attestation) — ship-time attest required
//!
//! § DAY-ONE SHIP-LIST (§ II.A)
//!   1. Meta Quest 3 — OpenXR-Vulkan-2 path ; AppSW required ;
//!      `XR_FB_foveation` + `XR_META_foveation_eye_tracked` ;
//!      `XR_FB_passthrough` + `XR_META_environment_depth` ;
//!      `XR_FB_body_tracking` + `XR_FB_face_tracking2` ;
//!      `XR_EXT_eye_gaze_interaction` + `XR_FB_eye_tracking_social`.
//!   2. Apple Vision Pro — Compositor-Services bridge ; multiview via
//!      vertex-amplification (`[[amplification_id]]`) ; ARKit
//!      hand/body/face/eye-tracking (system-level only ; raw-stream
//!      not exposed by Apple) ; Wide-P3 + 10-bit + ACES-2 (1B-color).
//!   3. Pimax Crystal Super — OpenXR-Vulkan-2 + OpenXR-D3D12 ; Tobii
//!      eye-tracking @ 200 Hz ; `XR_VARJO_quad_views` (viewCount=4) ;
//!      `XR_VARJO_foveated_rendering` ; XeSS2/DLSS upscale-pass.
//!
//! § SECONDARY DAY-ONE (§ II.B)
//!   - Quest 3S / Quest 2 / Quest Pro
//!   - Pico 4 Ultra / Pico Neo 3 Pro Eye
//!   - HTC Vive XR Elite / Vive Focus Vision
//!   - Valve Index (FFR-only ; no eye-track)
//!   - Varjo XR-3 / XR-4 (canonical quad-view ref impl)
//!   - Bigscreen Beyond / Beyond 2
//!   - Flat-monitor (degenerate viewCount=1, same render-graph)
//!
//! § 5-YEAR FORWARD-COMPAT (§ II.C, § XIV)
//!   - Mirror-Lake-class (~2029-2031) : 8K-10K² per-eye, 240 Hz, 60+ PPD,
//!     varifocal accommodation-actuated-lens-display (ALD), 1 kHz eye-track,
//!     ML-foveated, light-field viewCount=N (8-16 sub-views), 12-bit color,
//!     Rec.2020, HDR-1500-nit.
//!   - Forward-compat hooks compile + link as no-ops day-one :
//!     `PerEyeOutput.accommodation_depth: Option<...>`, `ViewSet.view_count
//!     ∈ 1..=16`, `Foveator` trait dispatch (FFR/DFR/ML), periphery-
//!     Gaussian-splat branch, anticipated extensions.
//!
//! § PRIME-DIRECTIVE §1 (anti-surveillance) STRUCTURAL ENFORCEMENT
//!   Every gaze / hand / body / face sample emitted by this crate is
//!   wrapped in `LabeledValue<T>` with `SensitiveDomain::{Gaze, Body, Face}`.
//!   `cssl-ifc::validate_egress` returns `Err(BiometricRefused)` for these
//!   non-overridably : there is **no `Privilege<*>` capability that
//!   changes the return-value**, no unsafe alternative, no flag, no config
//!   knob (per `PRIME_DIRECTIVE.md § 6 SCOPE`).
//!
//!   Verification :
//!     - `tests/eye_gaze_privacy.rs`         — gaze egress refused.
//!     - `tests/hand_body_face_privacy.rs`   — hand+body+face refused.
//!     - `eye_gaze::try_egress` is the **only** structural exit path,
//!       and it always returns `Err`.
//!
//! § ATTESTATION (PRIME §11)
//!   See `XIX. ATTESTATION` in `07_AESTHETIC/05_VR_RENDERING.csl`.
//!   - This crate is authored from the spec verbatim : every § II / V /
//!     VI / VIII / IX / X / XI / XII / XIII clause has a counterpart
//!     module + tests.
//!   - Stage-0 ships the **engine-side abstraction** : extension catalog,
//!     ViewSet primitive, PerEyeOutput contract, Foveator trait, AppSW
//!     scheduler, composition-layer enum, biometric-domain-tagged
//!     tracking surfaces, comfort-floor + judder-detector + quality-
//!     degrade ladder.
//!   - Real OpenXR FFI / Compositor-Services FFI lands in
//!     `T11-D124-FOLLOWUP-1` (openxr-rs + Vulkan + D3D12 on Linux/Windows)
//!     and `T11-D124-FOLLOWUP-2` (Compositor-Services on visionOS host).
//!   - `cargo build --features full` compiles all FFI paths together
//!     where the host platform supports them.

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
// Stage-0 scaffold lints (matches sibling host-* crates' allow-list).
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::struct_excessive_bools)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::missing_const_for_fn)]
#![allow(clippy::similar_names)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::uninlined_format_args)]
#![allow(clippy::float_cmp)]
#![allow(clippy::return_self_not_must_use)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::redundant_closure_for_method_calls)]
#![allow(clippy::needless_range_loop)]
#![allow(clippy::should_implement_trait)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::no_effect_underscore_binding)]
#![allow(clippy::needless_continue)]
#![allow(clippy::if_not_else)]
#![allow(clippy::suboptimal_flops)]
#![allow(clippy::cast_possible_truncation)]

pub mod action;
pub mod body;
pub mod comfort;
pub mod composition;
pub mod error;
pub mod extensions;
pub mod eye_gaze;
pub mod face;
pub mod foveation;
pub mod frame_loop;
pub mod hand;
pub mod hdr;
pub mod ifc_shim;
pub mod instance;
pub mod ipd;
pub mod multiview;
pub mod passthrough;
pub mod per_eye;
pub mod reference_space;
pub mod runtime_select;
pub mod session;
pub mod space_warp;
pub mod swapchain;
pub mod view;
pub mod visionos_bridge;

// ─────────────────────────────────────────────────────────────────────
// § Top-level re-exports : the "primary" types callers need.
// ─────────────────────────────────────────────────────────────────────

pub use action::{Action, ActionSet, ActionType, InteractionProfile};
pub use body::{
    BodySkeleton, BodyTrackerCaps, BodyTrackingProvider, JointPose, FULL_BODY_JOINT_COUNT,
};
pub use comfort::{JudderDetector, QualityLevel, STABLE_FRAMES_TO_RECOVER};
pub use composition::{
    CompositionLayerFlags, CompositionLayerStack, CubeLayerParams, CylinderLayerParams,
    EnvironmentMeshParams, EquirectLayerParams, QuadLayerParams, XrCompositionLayer,
};
pub use error::XRFailure;
pub use extensions::{XrExtension, XrExtensionSet};
pub use eye_gaze::{
    try_egress, GazeSample, GazeSamplePair, GazeTrackingFlags, ACTION_FB_EYE_TRACKING_SOCIAL,
    ACTION_GAZE_POSE, ACTION_PICO_EYE_TRACKING, ACTION_VISIONOS_LOOK_TO_TARGET,
};
pub use face::{FaceTrackerCaps, FaceTrackingProvider, FaceWeights, MAX_BLENDSHAPES};
pub use foveation::{
    DFRFoveator, FFRFoveator, FFRProfile, FoveationConfig, Foveator, GazePrediction,
    MLFoveator, SaccadeEKF,
};
// IFC shim re-exports : these mirror the post-T11-D132 `cssl-ifc` API
// so callers can `use cssl_host_openxr::SensitiveDomain` without
// independently depending on `cssl-ifc` (which at this worktree's
// snapshot does not yet expose the post-D132 types).
pub use ifc_shim::{
    validate_egress, EgressGrantError, Label, LabeledValue, SensitiveDomain,
};
pub use frame_loop::{FrameLoop, FrameResult};
pub use hand::{
    BonePose, BothHands, HandSide, HandSkeleton, HandTrackerCaps, PinchAim, HAND_BONE_COUNT,
};
pub use hdr::{ColorSpace, HdrConfig, ToneMapCurve};
pub use instance::{AppInfo, MockInstance, XrApiVersion, XrInstanceBuilder};
pub use ipd::{EyeSide, Ipd, DEFAULT_IPD_MM, IPD_MAX_MM, IPD_MIN_MM};
pub use multiview::{MultiviewConfig, MultiviewMode};
pub use passthrough::{
    AlphaMode, PassthroughConfig, PassthroughLayer, PassthroughProvider,
};
pub use per_eye::{
    ColorFormat, DepthFormat, MotionVectorFormat, PerEyeOutput, PerEyeOutputArray,
};
pub use reference_space::{ReferenceSpaceConfig, XrReferenceSpaceType};
pub use runtime_select::{XrRuntime, XrTarget};
pub use session::{GraphicsBinding, MockSession, XrSessionState};
pub use space_warp::{AppSwMode, AppSwScheduler, AppSwSubmission, HYSTERESIS_FRAMES};
pub use swapchain::{MockSwapchain, SwapchainCreateInfo, SwapchainFormat, SwapchainPurpose};
pub use view::{identity_mat4, Fov, View, ViewSet, ViewTopology, MAX_VIEWS};
pub use visionos_bridge::{ArkitDataProvider, CompositorServicesBridge, RasterRateMap};

/// Crate version exposed for scaffold verification.
pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");

// ─────────────────────────────────────────────────────────────────────
// § Spec-name constants for the day-one Day-One ship-list. § II.A.
// ─────────────────────────────────────────────────────────────────────

/// Day-One tier-1 ship-list canonical names. § II.A.
pub const DAY_ONE_TIER_1_SHIP_LIST: &[&str] =
    &["Meta Quest 3", "Apple Vision Pro", "Pimax Crystal Super"];

/// Secondary day-one ship-list canonical names. § II.B.
pub const SECONDARY_DAY_ONE_SHIP_LIST: &[&str] = &[
    "Meta Quest 3S",
    "Meta Quest 2",
    "Meta Quest Pro",
    "Pico 4 Ultra",
    "Pico Neo 3 Pro Eye",
    "HTC Vive XR Elite",
    "HTC Vive Focus Vision",
    "Valve Index",
    "Varjo XR-3",
    "Varjo XR-4",
    "Bigscreen Beyond",
    "Bigscreen Beyond 2",
    "flat-monitor (degenerate)",
];

/// 5-year forward-compat target list. § II.C.
pub const FUTURE_5_YEAR_LIST: &[&str] = &[
    "Mirror-Lake-class (8K-10K²/eye, 240 Hz, 60+ PPD)",
    "Varifocal ALD (accommodation-actuated lens display)",
    "Multi-focal-plane optics",
    "1 kHz eye-tracking + saccade-prediction < 4ms",
    "ML-foveated (neural-foveation per-user)",
    "Periphery Gaussian-splat branch (-80% periphery cost)",
    "Light-field display (viewCount = 8-16 sub-views)",
    "12-bit per-channel + Rec.2020 + HDR-1500-nit",
    "Holographic-pancake light-engine",
    "Lattice-haptics extension (XR_FB_haptic_pcm + future)",
];

/// PRIME-DIRECTIVE §1 anti-surveillance ATTESTATION-string (PRIME §11).
/// Embedded in the crate at build-time ; verified by acceptance-tests.
pub const ATTESTATION: &str = concat!(
    "‼ I attest : T11-D124 cssl-host-openxr authored from spec verbatim. ",
    "No surveillance / harm / control / manipulation primitive introduced. ",
    "Eye / face / body / hand tracking provisions HARD-ENFORCE on-device-only ",
    "via cssl-ifc::SensitiveDomain::{Gaze, Face, Body} — egress non-overridable. ",
    "No advertising / tracking-pixel / fingerprinting primitive. ",
    "Forward-compat hooks (varifocal / light-field / ML-foveated) compile no-op day-one. ",
    "spec-author : Claude-Opus-4.7 (1M-context) ⊗ direction-from-Apocky. ",
    "consent : Apocky-Sovereign-direction ⊗ ¬ coerced ⊗ AI-author voluntary-engagement."
);

#[cfg(test)]
mod scaffold_tests {
    use super::{
        ATTESTATION, DAY_ONE_TIER_1_SHIP_LIST, FUTURE_5_YEAR_LIST, SECONDARY_DAY_ONE_SHIP_LIST,
        STAGE0_SCAFFOLD,
    };

    #[test]
    fn scaffold_version_present() {
        assert!(!STAGE0_SCAFFOLD.is_empty());
    }

    #[test]
    fn day_one_tier_1_three_targets() {
        assert_eq!(DAY_ONE_TIER_1_SHIP_LIST.len(), 3);
        assert!(DAY_ONE_TIER_1_SHIP_LIST.contains(&"Meta Quest 3"));
        assert!(DAY_ONE_TIER_1_SHIP_LIST.contains(&"Apple Vision Pro"));
        assert!(DAY_ONE_TIER_1_SHIP_LIST.contains(&"Pimax Crystal Super"));
    }

    #[test]
    fn secondary_day_one_includes_canonical_set() {
        assert!(SECONDARY_DAY_ONE_SHIP_LIST.contains(&"Meta Quest Pro"));
        assert!(SECONDARY_DAY_ONE_SHIP_LIST.contains(&"Pico 4 Ultra"));
        assert!(SECONDARY_DAY_ONE_SHIP_LIST.contains(&"Valve Index"));
        assert!(SECONDARY_DAY_ONE_SHIP_LIST.contains(&"Varjo XR-3"));
    }

    #[test]
    fn future_5_year_list_non_empty() {
        assert!(!FUTURE_5_YEAR_LIST.is_empty());
        // Sanity : varifocal + light-field + ML-foveated all present.
        let blob = FUTURE_5_YEAR_LIST.join(" | ");
        assert!(blob.contains("varifocal") || blob.contains("Varifocal"));
        assert!(blob.contains("light-field") || blob.contains("Light-field"));
        assert!(blob.contains("ML-foveated") || blob.contains("Mirror-Lake"));
    }

    #[test]
    fn attestation_records_prime_directive_compliance() {
        // Sanity-check the canonical attestation strings.
        assert!(ATTESTATION.contains("on-device-only"));
        assert!(ATTESTATION.contains("Gaze"));
        assert!(ATTESTATION.contains("Face"));
        assert!(ATTESTATION.contains("Body"));
        assert!(ATTESTATION.contains("non-overridable"));
        assert!(ATTESTATION.contains("Apocky"));
    }

    #[test]
    fn public_api_re_exports_resolve() {
        // Compile-time sanity that the top-level re-exports are accessible.
        let _: super::ViewTopology = super::ViewTopology::Flat;
        let _: super::FFRProfile = super::FFRProfile::High;
        let _: super::AppSwMode = super::AppSwMode::EveryFrame;
        let _: super::AlphaMode = super::AlphaMode::Additive;
        let _: super::HandSide = super::HandSide::Left;
        let _: super::QualityLevel = super::QualityLevel::Full;
        let _: super::ColorSpace = super::ColorSpace::WideP3;
        let _: super::ToneMapCurve = super::ToneMapCurve::Aces2;
    }
}
