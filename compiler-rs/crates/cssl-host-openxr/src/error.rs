//! `XRFailure` : unified error-enum for the OpenXR runtime + Compositor-Services
//! bridge surface.
//!
//! § SPEC : `07_AESTHETIC/05_VR_RENDERING.csl` § IV.A (frame-loop returns
//! `Result<(), XRFailure>`).
//!
//! § DESIGN
//!   - One `enum` for every fallible OpenXR-or-bridge call.
//!   - `thiserror` for ergonomic `#[from]` conversions.
//!   - Stage-0 variants cover the surface the engine must reason about ;
//!     the FFI follow-up slice adds rich `XrResult`-codes via the
//!     `Runtime { code }` variant.
//!
//! § PRIME-DIRECTIVE
//!   No `XRFailure` variant carries biometric data. `EyeGazeRefused` /
//!   `BodyRefused` / `FaceRefused` carry only the `SensitiveDomain` that
//!   was rejected, never the gaze/face/body sample itself.

use crate::ifc_shim::SensitiveDomain;
use thiserror::Error;

/// Unified error-type for OpenXR + Compositor-Services bridge calls.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum XRFailure {
    /// The OpenXR runtime is not present on this host (e.g. no
    /// `libopenxr_loader.so` / `openxr_loader.dll` discoverable).
    #[error("openxr runtime not present on host")]
    RuntimeNotPresent,

    /// The OpenXR runtime is present but does not support the requested
    /// API version. Carries the version the runtime advertised.
    #[error("openxr runtime advertised version {advertised} ; required {required}")]
    UnsupportedApiVersion {
        /// Version actually advertised by the runtime.
        advertised: u64,
        /// Version the engine required.
        required: u64,
    },

    /// The runtime does not advertise a required extension. Carries the
    /// extension-name the engine could not find.
    #[error("openxr extension {0:?} required but not advertised by runtime")]
    MissingRequiredExtension(&'static str),

    /// The runtime does not advertise an *optional* extension : engine
    /// may degrade gracefully (e.g. fall back from DFR to FFR when
    /// `XR_FB_foveation_eye_tracked` is missing).
    #[error("openxr extension {0:?} not advertised ; falling back")]
    MissingOptionalExtension(&'static str),

    /// `xrCreateInstance` failed.
    #[error("xrCreateInstance failed (runtime-code {code})")]
    InstanceCreate {
        /// Runtime-specific result-code (XrResult enum value as i32).
        code: i32,
    },

    /// `xrCreateSession` failed.
    #[error("xrCreateSession failed (runtime-code {code})")]
    SessionCreate {
        /// Runtime-specific result-code.
        code: i32,
    },

    /// `xrBeginSession` failed.
    #[error("xrBeginSession failed (runtime-code {code})")]
    SessionBegin {
        /// Runtime-specific result-code.
        code: i32,
    },

    /// `xrCreateSwapchain` failed.
    #[error("xrCreateSwapchain failed (runtime-code {code} ; format {format})")]
    SwapchainCreate {
        /// Runtime-specific result-code.
        code: i32,
        /// Format-hint that was requested.
        format: u32,
    },

    /// `xrAcquireSwapchainImage` / `xrWaitSwapchainImage` /
    /// `xrReleaseSwapchainImage` failed.
    #[error("swapchain-image cycle failed (runtime-code {code})")]
    SwapchainImageCycle {
        /// Runtime-specific result-code.
        code: i32,
    },

    /// `xrWaitFrame` failed.
    #[error("xrWaitFrame failed (runtime-code {code})")]
    FrameWait {
        /// Runtime-specific result-code.
        code: i32,
    },

    /// `xrBeginFrame` / `xrEndFrame` failed.
    #[error("xrBeginFrame/xrEndFrame failed (runtime-code {code})")]
    FrameBoundary {
        /// Runtime-specific result-code.
        code: i32,
    },

    /// `xrLocateViews` failed.
    #[error("xrLocateViews failed (runtime-code {code})")]
    LocateViews {
        /// Runtime-specific result-code.
        code: i32,
    },

    /// `xrLocateSpace` failed for a tracking-related space (gaze / hand /
    /// body / face). Carries the `SensitiveDomain` the locate-was-for so
    /// the engine can degrade gracefully.
    #[error("xrLocateSpace for {domain} failed (runtime-code {code})")]
    LocateTrackingSpace {
        /// Runtime-specific result-code.
        code: i32,
        /// Which biometric-family the locate-call was for.
        domain: SensitiveDomain,
    },

    /// Composition-layer submission via `xrEndFrame` rejected one of the
    /// composition-layers. Carries an index into the layer-array.
    #[error("composition-layer rejected at index {index} (runtime-code {code})")]
    CompositionLayerRejected {
        /// Layer index in the submission array.
        index: usize,
        /// Runtime-specific result-code.
        code: i32,
    },

    /// Foveation-config update was rejected by the runtime.
    #[error("foveation-config update rejected (runtime-code {code})")]
    FoveationConfigRejected {
        /// Runtime-specific result-code.
        code: i32,
    },

    /// AppSW (XR_FB_space_warp) submission was rejected.
    #[error("space-warp (XR_FB_space_warp) submission rejected (runtime-code {code})")]
    SpaceWarpSubmissionRejected {
        /// Runtime-specific result-code.
        code: i32,
    },

    /// Passthrough-layer creation was rejected.
    #[error("passthrough-layer creation rejected (runtime-code {code})")]
    PassthroughCreate {
        /// Runtime-specific result-code.
        code: i32,
    },

    /// Action-set / action-binding install was rejected.
    #[error("action-set install rejected (runtime-code {code})")]
    ActionSetInstall {
        /// Runtime-specific result-code.
        code: i32,
    },

    /// `validate_egress` refused to forward biometric data : the engine
    /// tried to emit a gaze / face / body / hand sample to a
    /// non-on-device sink. This is **non-overridable** : no `Privilege`
    /// capability changes the return-value. (PRIME-DIRECTIVE §1.)
    #[error("biometric-egress refused for {domain}")]
    BiometricEgressRefused {
        /// Which domain triggered the refusal.
        domain: SensitiveDomain,
    },

    /// Compositor-Services bridge (visionOS) is unavailable on this
    /// host — engine falls back to OpenXR-direct.
    #[error("compositor-services bridge unavailable on this host")]
    CompositorServicesUnavailable,

    /// View-count is out-of-range. ViewSet supports
    /// `view_count ∈ {1, 2, 4, N}` with `N ≤ 16` per spec §III + §XIV.B.
    #[error("view-count {got} out-of-range ; allowed 1..=16")]
    ViewCountOutOfRange {
        /// The view-count that was passed.
        got: u32,
    },

    /// IPD-mm out-of-range. Spec §III bound is `50.0..=80.0` mm.
    #[error("ipd-mm {got} out-of-range ; allowed 50.0..=80.0")]
    IpdOutOfRange {
        /// The IPD-mm value that was passed.
        got: f32,
    },

    /// Comfort-floor was violated : a frame missed its
    /// `Realtime<90Hz>` deadline. Carries the actual frame-time-ns.
    #[error("comfort-floor 90Hz violated ; frame-time-ns {ns} > {budget_ns}")]
    ComfortFloorViolated {
        /// The actual frame-time observed.
        ns: u64,
        /// The budget at the active display-rate.
        budget_ns: u64,
    },

    /// The engine attempted to use a runtime feature that is
    /// not-yet-implemented at stage-0 ; the FFI-follow-up slice will
    /// land it.
    #[error("not-yet-implemented at stage-0 : {0}")]
    NotYetImplemented(&'static str),
}

impl XRFailure {
    /// `true` iff this failure is a graceful-degrade signal (engine can
    /// keep running with reduced capability) ; `false` iff it is fatal
    /// for the current frame.
    #[must_use]
    pub const fn is_graceful_degrade(&self) -> bool {
        matches!(
            self,
            Self::MissingOptionalExtension(_) | Self::CompositorServicesUnavailable
        )
    }

    /// `true` iff this failure is a *PRIME §1 anti-surveillance*
    /// non-overridable refusal. Calling code must surface this to the
    /// user (never silently succeed-around it).
    #[must_use]
    pub const fn is_biometric_refusal(&self) -> bool {
        matches!(self, Self::BiometricEgressRefused { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::XRFailure;
    use crate::ifc_shim::SensitiveDomain;

    #[test]
    fn missing_optional_is_graceful() {
        assert!(
            XRFailure::MissingOptionalExtension("XR_FB_foveation_eye_tracked")
                .is_graceful_degrade()
        );
    }

    #[test]
    fn missing_required_is_fatal() {
        assert!(
            !XRFailure::MissingRequiredExtension("XR_KHR_vulkan_enable2").is_graceful_degrade()
        );
    }

    #[test]
    fn biometric_refusal_classified() {
        let f = XRFailure::BiometricEgressRefused {
            domain: SensitiveDomain::Gaze,
        };
        assert!(f.is_biometric_refusal());
        assert!(!f.is_graceful_degrade());
    }

    #[test]
    fn compositor_services_unavailable_is_graceful() {
        assert!(XRFailure::CompositorServicesUnavailable.is_graceful_degrade());
    }

    #[test]
    fn display_format_runs() {
        // sanity : every variant Displays without panic
        let cases = [
            XRFailure::RuntimeNotPresent,
            XRFailure::UnsupportedApiVersion {
                advertised: 1,
                required: 2,
            },
            XRFailure::MissingRequiredExtension("XR_FOO"),
            XRFailure::MissingOptionalExtension("XR_BAR"),
            XRFailure::InstanceCreate { code: -1 },
            XRFailure::SessionCreate { code: -1 },
            XRFailure::SessionBegin { code: -1 },
            XRFailure::SwapchainCreate {
                code: -1,
                format: 0,
            },
            XRFailure::SwapchainImageCycle { code: -1 },
            XRFailure::FrameWait { code: -1 },
            XRFailure::FrameBoundary { code: -1 },
            XRFailure::LocateViews { code: -1 },
            XRFailure::LocateTrackingSpace {
                code: -1,
                domain: SensitiveDomain::Gaze,
            },
            XRFailure::CompositionLayerRejected { index: 0, code: -1 },
            XRFailure::FoveationConfigRejected { code: -1 },
            XRFailure::SpaceWarpSubmissionRejected { code: -1 },
            XRFailure::PassthroughCreate { code: -1 },
            XRFailure::ActionSetInstall { code: -1 },
            XRFailure::BiometricEgressRefused {
                domain: SensitiveDomain::Face,
            },
            XRFailure::CompositorServicesUnavailable,
            XRFailure::ViewCountOutOfRange { got: 999 },
            XRFailure::IpdOutOfRange { got: 999.0 },
            XRFailure::ComfortFloorViolated {
                ns: 20_000_000,
                budget_ns: 11_111_111,
            },
            XRFailure::NotYetImplemented("foo"),
        ];
        for f in &cases {
            let _ = format!("{}", f);
        }
    }
}
