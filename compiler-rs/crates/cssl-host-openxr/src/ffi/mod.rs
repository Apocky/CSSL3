//! § ffi : From-scratch OpenXR 1.0 FFI bindings (T11-D260 · W-H3).
//!
//! § ROLE
//!   Replaces the `cssl-rt` `host_xr` STUB with real OpenXR 1.0 FFI for
//!   Apocky's Meta Quest 3s + future Pimax / Vision Pro targets. Authored
//!   from the OpenXR 1.0 spec verbatim — **no `openxr` crate dependency**.
//!   This is the proprietary-everything thesis (LoA-v13) : Apocky owns the
//!   binding surface end-to-end.
//!
//! § STRATEGY
//!   - FFI types declared from-scratch (`types.rs`) : XR_DEFINE_HANDLE
//!     (64-bit) + XR_DEFINE_ATOM (64-bit) + struct layouts matching the
//!     OpenXR 1.0 reference headers verbatim, with `#[repr(C)]` on every
//!     interop struct.
//!   - Result codes (`result.rs`) : XrResult + XR_SUCCEEDED / XR_FAILED
//!     as `const fn`.
//!   - Loader (`loader.rs`) : `xrInitializeLoaderKHR` + a dispatch table
//!     populated via `xrGetInstanceProcAddr`. Loader is dlopen'd at
//!     runtime — at stage-0 the FFI compiles but the loader-load surfaces
//!     `Err(LoaderUnavailable)` cleanly when no XR runtime is present
//!     (matches the `cssl-host-vulkan::ffi` `LoaderError::Loading` pattern).
//!   - Instance / system / session / swapchain / pose / input : one file
//!     each, mirroring the `XrInstance` / `XrSystem` / `XrSession` /
//!     `XrSwapchain` / `XrSpace` / `XrAction` lifecycle in the spec.
//!
//! § UNSAFE-FFI POLICY
//!   Crate-level : `#![deny(unsafe_code)]` ; the abstraction modules
//!   (`session.rs` / `instance.rs` / `swapchain.rs` etc. at the crate
//!   root) stay sound-by-default. Only this submodule opts in via
//!   `#![allow(unsafe_code)]` below. Every `unsafe` block carries an
//!   inline `// SAFETY :` paragraph. The convention mirrors the sibling
//!   `cssl-host-vulkan::ffi` boundary.
//!
//! § ZERO-EXTERNAL-DEPS
//!   This subtree depends on `core::*` + `std::*` only. No `openxr`,
//!   no `ash`, no `windows`, no `libloading`. The dynamic loader is
//!   surfaced through `loader.rs` as a thin `*const fn(...)` table the
//!   caller populates after dlopen'ing the runtime in their own code
//!   (matches the §§ 14_BACKEND "owned FFI (volk-like dispatch)" pattern).
//!
//! § QUEST-3S BINDING
//!   `input.rs` provides the canonical Quest-3s controller-binding path
//!   `/interaction_profiles/oculus/touch_controller` + the standard
//!   `/user/hand/left` + `/user/hand/right` subaction paths, the Meta-
//!   suggested `pose` / `value` / `click` / `force` component-paths,
//!   and the haptic-output `/output/haptic` channel.
//!
//! § cssl-rt SWAP-IN POINT
//!   `cssl-rt::host_xr::__cssl_xr_*` STUBs delegate to
//!   `cssl_host_openxr::ffi::HostXrApi` once a session is active. The
//!   `HostXrApi` trait + `OpenXrHostApi` impl in this subtree provide
//!   the swap-in surface : `frame_begin`, `frame_end`, `view_locate`,
//!   `swapchain_acquire`, `swapchain_release`, `action_sync`,
//!   `action_state_pose`, `action_state_bool`, `action_state_float`,
//!   `haptic_apply`. The STUB returns hard-coded identity poses ; the
//!   real impl reads from the OpenXR runtime.
//!
//! § PRIME-DIRECTIVE STRUCTURAL HOOKS
//!   - Eye-gaze + face + body + hand sample paths route through the
//!     existing `eye_gaze` / `face` / `body` / `hand` abstraction
//!     modules ; the FFI here only exposes the **action-input**
//!     surface (controller-buttons / sticks / triggers / haptics) that
//!     does NOT carry biometric-domain payloads. Biometric surface
//!     remains compile-time-egress-refused (§ PRIME-DIRECTIVE §1).
//!   - No telemetry / advertising / fingerprinting primitive. The FFI
//!     is a transport layer ; the engine-side logic stays in the
//!     non-FFI modules.

#![allow(unsafe_code)]
#![allow(clippy::missing_safety_doc)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::module_name_repetitions)]

pub mod input;
pub mod instance;
pub mod loader;
pub mod pose;
pub mod result;
pub mod session;
pub mod swapchain;
pub mod system;
pub mod types;

// ─────────────────────────────────────────────────────────────────────
// § Top-level FFI re-exports for the swap-in surface.
// ─────────────────────────────────────────────────────────────────────

pub use input::{
    Action, ActionSet, ActionState, ActionStateBool, ActionStateFloat, ActionStatePose,
    ActionType, BindingSuggestion, HapticVibration, InteractionProfile, MockInputState,
    QUEST_3S_TOUCH_CONTROLLER, SUBACTION_PATH_LEFT, SUBACTION_PATH_RIGHT,
};
pub use instance::{
    quest_3s_runtime_advertised_extensions, ApiVersion, ApplicationInfo, ExtensionProperties,
    InstanceCreateInfo, InstanceHandle, MockInstance, MockInstanceConfig,
};
pub use loader::{DispatchTable, LoaderError, LoaderInitInfo, MockDispatch};
pub use pose::{
    decode_posef, identity_quaternion, identity_vector3f, mock_locate_space, MockSpace,
    Quaternionf, SpaceHandle, SpaceLocation, SpaceLocationFlags, SpaceVelocity, Vector3f, XrPosef,
};
pub use result::{xr_failed, xr_succeeded, XrResult};
pub use session::{
    MockSession, SessionBeginInfo, SessionCreateInfo, SessionHandle, SessionState,
    StateMachineEvent,
};
pub use swapchain::{
    MockSwapchain, SwapchainAcquireInfo, SwapchainCreateInfo, SwapchainHandle,
    SwapchainImageReleaseInfo, SwapchainImageWaitInfo, SwapchainUsageFlags,
};
pub use system::{
    FormFactor, GraphicsProperties, MockSystem, SystemGetInfo, SystemId, SystemProperties,
    TrackingProperties,
};
pub use types::{
    Atom, Duration as XrDuration, Time as XrTime, ViewConfigurationType, NULL_HANDLE,
    XR_CURRENT_API_VERSION, XR_MAX_APPLICATION_NAME_SIZE, XR_MAX_ENGINE_NAME_SIZE,
    XR_MAX_RUNTIME_NAME_SIZE, XR_MAX_SYSTEM_NAME_SIZE,
};

// ─────────────────────────────────────────────────────────────────────
// § HostXrApi : the swap-in surface for `cssl-rt::host_xr::__cssl_xr_*`.
// ─────────────────────────────────────────────────────────────────────

/// Trait implemented by both `MockOpenXrApi` (test path) and the real
/// `OpenXrHostApi` (production · gated behind dlopen-loaded dispatch
/// table). Mirrors the `__cssl_xr_*` extern surface in `cssl-rt::host_xr`.
pub trait HostXrApi {
    /// `xrBeginFrame` : signal beginning of new frame.
    fn frame_begin(&mut self) -> XrResult;
    /// `xrEndFrame` : submit frame for display.
    fn frame_end(&mut self, predicted_display_time: XrTime) -> XrResult;
    /// `xrLocateViews` : decode left + right view-poses + projection FOVs.
    fn view_locate(&mut self, eye: u32) -> Option<(XrPosef, [f32; 4])>;
    /// `xrAcquireSwapchainImage` + `xrWaitSwapchainImage` : returns image-index.
    fn swapchain_acquire(&mut self, swapchain: SwapchainHandle) -> Option<u32>;
    /// `xrReleaseSwapchainImage`.
    fn swapchain_release(&mut self, swapchain: SwapchainHandle) -> XrResult;
    /// `xrSyncActions` : update action-state cache from the runtime.
    fn action_sync(&mut self) -> XrResult;
    /// `xrGetActionStatePose` : query pose-action.
    fn action_state_pose(&self, action: Action) -> Option<XrPosef>;
    /// `xrGetActionStateBoolean` : query bool-action.
    fn action_state_bool(&self, action: Action) -> Option<bool>;
    /// `xrGetActionStateFloat` : query float-action.
    fn action_state_float(&self, action: Action) -> Option<f32>;
    /// `xrApplyHapticFeedback` : send vibration to controller-haptic.
    fn haptic_apply(&mut self, action: Action, vibration: HapticVibration) -> XrResult;
}

/// Test-mode mock impl of `HostXrApi` that records calls + returns
/// deterministic synthetic data. Used by `cssl-rt` integration tests.
#[derive(Debug, Default)]
pub struct MockOpenXrApi {
    pub frame_begun: u64,
    pub frame_ended: u64,
    pub last_predicted_time: i64,
    pub views_locate_calls: u64,
    pub haptic_calls: u64,
    pub input_state: MockInputState,
}

impl MockOpenXrApi {
    /// Construct a fresh mock with default state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl HostXrApi for MockOpenXrApi {
    fn frame_begin(&mut self) -> XrResult {
        self.frame_begun = self.frame_begun.saturating_add(1);
        XrResult::SUCCESS
    }
    fn frame_end(&mut self, predicted_display_time: XrTime) -> XrResult {
        self.frame_ended = self.frame_ended.saturating_add(1);
        self.last_predicted_time = predicted_display_time.0;
        XrResult::SUCCESS
    }
    fn view_locate(&mut self, eye: u32) -> Option<(XrPosef, [f32; 4])> {
        self.views_locate_calls = self.views_locate_calls.saturating_add(1);
        if eye > 1 {
            return None;
        }
        // Synthetic IPD : left -32.5mm, right +32.5mm in X, identity rot.
        let x = if eye == 0 { -0.0325 } else { 0.0325 };
        let pose = XrPosef {
            orientation: identity_quaternion(),
            position: Vector3f { x, y: 1.6, z: 0.0 },
        };
        // Symmetric FOV ±45° h, ±42° v as Quest-3s nominal.
        // ±π/4 = ±0.7853981633... ≈ 45° (horizontal half-FOV).
        // ±0.733 rad ≈ ±42° (vertical half-FOV).
        let pi_over_4 = core::f32::consts::FRAC_PI_4;
        let fov = [-pi_over_4, pi_over_4, 0.7330382, -0.7330382];
        Some((pose, fov))
    }
    fn swapchain_acquire(&mut self, _swapchain: SwapchainHandle) -> Option<u32> {
        Some(0)
    }
    fn swapchain_release(&mut self, _swapchain: SwapchainHandle) -> XrResult {
        XrResult::SUCCESS
    }
    fn action_sync(&mut self) -> XrResult {
        XrResult::SUCCESS
    }
    fn action_state_pose(&self, action: Action) -> Option<XrPosef> {
        self.input_state.pose_for(action)
    }
    fn action_state_bool(&self, action: Action) -> Option<bool> {
        self.input_state.bool_for(action)
    }
    fn action_state_float(&self, action: Action) -> Option<f32> {
        self.input_state.float_for(action)
    }
    fn haptic_apply(&mut self, _action: Action, _vibration: HapticVibration) -> XrResult {
        self.haptic_calls = self.haptic_calls.saturating_add(1);
        XrResult::SUCCESS
    }
}

#[cfg(test)]
mod tests {
    use super::{HostXrApi, HapticVibration, MockOpenXrApi, XrResult, XrTime};

    #[test]
    fn mock_xr_api_frame_lifecycle_round_trips() {
        let mut api = MockOpenXrApi::new();
        assert_eq!(api.frame_begin(), XrResult::SUCCESS);
        assert_eq!(api.frame_end(XrTime(1_000_000)), XrResult::SUCCESS);
        assert_eq!(api.frame_begun, 1);
        assert_eq!(api.frame_ended, 1);
        assert_eq!(api.last_predicted_time, 1_000_000);
    }

    #[test]
    fn mock_xr_api_view_locate_returns_synthetic_stereo_ipd() {
        let mut api = MockOpenXrApi::new();
        let l = api.view_locate(0).expect("left eye");
        let r = api.view_locate(1).expect("right eye");
        assert!(l.0.position.x < 0.0);
        assert!(r.0.position.x > 0.0);
        // IPD ~ 65mm ; tolerate floating slop.
        let ipd = r.0.position.x - l.0.position.x;
        assert!((0.060..=0.070).contains(&ipd), "IPD = {ipd}");
        // Out-of-range eye returns None.
        assert!(api.view_locate(2).is_none());
    }

    #[test]
    fn mock_xr_api_haptic_increments_counter() {
        let mut api = MockOpenXrApi::new();
        let v = HapticVibration {
            duration_ns: 50_000_000,
            frequency_hz: 200.0,
            amplitude: 0.5,
        };
        let action = super::Action(7);
        let r = api.haptic_apply(action, v);
        assert_eq!(r, XrResult::SUCCESS);
        assert_eq!(api.haptic_calls, 1);
    }
}
