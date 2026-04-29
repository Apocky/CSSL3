//! `XrSession` lifecycle + state-machine.
//!
//! § SPEC § IV.A : OpenXR session-state-machine. The states are :
//!   `Idle → Ready → Synchronized → Visible → Focused → Stopping → Idle`.
//!   plus the terminal states `LossPending` + `Exiting`.
//!
//! § DESIGN
//!   - `XrSessionState` enum mirrors the OpenXR `XrSessionState` enum.
//!   - `XrSession` struct carries the state + the bound graphics-API
//!     binding (Vulkan / D3D12 / Metal-via-Compositor-Services).
//!   - `step()` advances the state-machine on a `XrEventDataSessionStateChanged`.
//!   - Stage-0 ships a `MockSession` that exercises the state-machine
//!     transitions without contacting a real runtime.

use crate::error::XRFailure;
use crate::instance::MockInstance;

/// OpenXR session-state. § IV.A state-machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum XrSessionState {
    /// Before `xrCreateSession`.
    Unknown,
    /// `xrCreateSession` succeeded ; runtime not yet ready.
    Idle,
    /// Runtime signalled READY ; engine should call `xrBeginSession`.
    Ready,
    /// `xrBeginSession` succeeded ; runtime synchronizing frames.
    Synchronized,
    /// Runtime composing engine output ; engine actively rendering.
    Visible,
    /// User-focus on this session ; controllers + tracking active.
    Focused,
    /// Runtime signalled STOPPING ; engine should call `xrEndSession`.
    Stopping,
    /// Runtime is losing the session (user removed headset, etc.).
    LossPending,
    /// Engine called `xrRequestExitSession` ; transition to Idle.
    Exiting,
}

impl XrSessionState {
    /// `true` iff the session is in a state that allows submitting frames.
    #[must_use]
    pub const fn allows_render(self) -> bool {
        matches!(self, Self::Synchronized | Self::Visible | Self::Focused)
    }

    /// `true` iff the session is in a state that allows reading
    /// controller / hand / gaze input.
    #[must_use]
    pub const fn allows_input(self) -> bool {
        matches!(self, Self::Focused)
    }

    /// `true` iff the state-machine has reached a terminal state.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::LossPending | Self::Exiting)
    }

    /// Display-name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Idle => "idle",
            Self::Ready => "ready",
            Self::Synchronized => "synchronized",
            Self::Visible => "visible",
            Self::Focused => "focused",
            Self::Stopping => "stopping",
            Self::LossPending => "loss-pending",
            Self::Exiting => "exiting",
        }
    }
}

/// Graphics-API binding mode for the session. § IV : OpenXR-Vulkan +
/// OpenXR-D3D12 + Compositor-Services-bridge (visionOS).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GraphicsBinding {
    /// `XR_KHR_vulkan_enable2`.
    Vulkan,
    /// `XR_KHR_D3D12_enable`.
    D3D12,
    /// `XR_KHR_D3D11_enable`.
    D3D11,
    /// Compositor-Services bridge (visionOS).
    CompositorServices,
    /// No graphics binding (engine running headless ; e.g. CI).
    Headless,
}

impl GraphicsBinding {
    /// Display-name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Vulkan => "vulkan",
            Self::D3D12 => "d3d12",
            Self::D3D11 => "d3d11",
            Self::CompositorServices => "compositor-services",
            Self::Headless => "headless",
        }
    }

    /// `true` iff this binding requires the visionOS Compositor-Services
    /// bridge (¬ OpenXR-direct).
    #[must_use]
    pub const fn is_bridge(self) -> bool {
        matches!(self, Self::CompositorServices)
    }
}

/// Stage-0 mock-session exercising the state-machine. The FFI follow-up
/// slice supersedes this with a real `XrSession`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MockSession {
    /// State-machine current state.
    state: XrSessionState,
    /// Graphics-API binding selected at create-time.
    binding: GraphicsBinding,
    /// Frame-counter (sanity for tests).
    frame_count: u64,
}

impl MockSession {
    /// Construct a new mock-session in the `Idle` state with the given
    /// graphics binding. Returns `XRFailure::SessionCreate` if the
    /// instance does not support the binding's required extension.
    pub fn create(instance: &MockInstance, binding: GraphicsBinding) -> Result<Self, XRFailure> {
        // Validate the binding matches the instance's extension-set.
        let needs = match binding {
            GraphicsBinding::Vulkan => Some(crate::extensions::XrExtension::KhrVulkanEnable2),
            GraphicsBinding::D3D12 => Some(crate::extensions::XrExtension::KhrD3D12Enable),
            GraphicsBinding::D3D11 => Some(crate::extensions::XrExtension::KhrD3D11Enable),
            GraphicsBinding::CompositorServices | GraphicsBinding::Headless => None,
        };
        if let Some(ext) = needs {
            if !instance.enabled_extensions.contains(ext) {
                return Err(XRFailure::SessionCreate { code: -1 });
            }
        }
        // visionOS demands the bridge.
        if instance.runtime.is_compositor_services_bridge()
            && !matches!(binding, GraphicsBinding::CompositorServices | GraphicsBinding::Headless)
        {
            return Err(XRFailure::SessionCreate { code: -2 });
        }
        Ok(Self {
            state: XrSessionState::Idle,
            binding,
            frame_count: 0,
        })
    }

    /// Advance the state-machine on a runtime event. Stage-0 mock just
    /// transitions to the new-state ; FFI follow-up slice will validate
    /// the transition is legal per OpenXR spec.
    pub fn step(&mut self, new_state: XrSessionState) {
        self.state = new_state;
    }

    /// Drive a happy-path lifecycle :
    ///   `Idle → Ready → Synchronized → Visible → Focused`.
    pub fn run_to_focused(&mut self) {
        self.step(XrSessionState::Ready);
        self.step(XrSessionState::Synchronized);
        self.step(XrSessionState::Visible);
        self.step(XrSessionState::Focused);
    }

    /// Drive shutdown :
    ///   `Focused → Visible → Synchronized → Stopping → Idle → Exiting`.
    pub fn run_shutdown(&mut self) {
        self.step(XrSessionState::Visible);
        self.step(XrSessionState::Synchronized);
        self.step(XrSessionState::Stopping);
        self.step(XrSessionState::Idle);
        self.step(XrSessionState::Exiting);
    }

    /// Current state.
    #[must_use]
    pub const fn state(&self) -> XrSessionState {
        self.state
    }

    /// Graphics binding selected at create-time.
    #[must_use]
    pub const fn binding(&self) -> GraphicsBinding {
        self.binding
    }

    /// Frame-counter — incremented by `tick_frame`.
    #[must_use]
    pub const fn frame_count(&self) -> u64 {
        self.frame_count
    }

    /// Increment the frame-counter (mock for `xrEndFrame` succeeding).
    /// Returns `XRFailure::FrameBoundary` if the session is not in a
    /// state that allows rendering.
    pub fn tick_frame(&mut self) -> Result<(), XRFailure> {
        if !self.state.allows_render() {
            return Err(XRFailure::FrameBoundary { code: -3 });
        }
        self.frame_count += 1;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{GraphicsBinding, MockSession, XrSessionState};
    use crate::instance::MockInstance;

    #[test]
    fn state_predicates() {
        assert!(!XrSessionState::Idle.allows_render());
        assert!(!XrSessionState::Ready.allows_render());
        assert!(XrSessionState::Synchronized.allows_render());
        assert!(XrSessionState::Visible.allows_render());
        assert!(XrSessionState::Focused.allows_render());
        assert!(!XrSessionState::Stopping.allows_render());
        assert!(XrSessionState::Focused.allows_input());
        assert!(!XrSessionState::Visible.allows_input());
        assert!(XrSessionState::LossPending.is_terminal());
        assert!(XrSessionState::Exiting.is_terminal());
    }

    #[test]
    fn quest3_session_creates_with_vulkan() {
        let inst = MockInstance::quest3_default().unwrap();
        let s = MockSession::create(&inst, GraphicsBinding::Vulkan).unwrap();
        assert_eq!(s.state(), XrSessionState::Idle);
        assert_eq!(s.binding(), GraphicsBinding::Vulkan);
    }

    #[test]
    fn pimax_session_creates_with_d3d12() {
        let inst = MockInstance::pimax_crystal_super_default().unwrap();
        let s = MockSession::create(&inst, GraphicsBinding::D3D12).unwrap();
        assert_eq!(s.binding(), GraphicsBinding::D3D12);
    }

    #[test]
    fn vision_pro_session_demands_compositor_services() {
        let inst = MockInstance::vision_pro_default().unwrap();
        // visionOS must use the bridge ; vulkan binding refused.
        assert!(MockSession::create(&inst, GraphicsBinding::Vulkan).is_err());
        let s = MockSession::create(&inst, GraphicsBinding::CompositorServices).unwrap();
        assert_eq!(s.binding(), GraphicsBinding::CompositorServices);
    }

    #[test]
    fn quest3_session_refuses_d3d12_binding() {
        let inst = MockInstance::quest3_default().unwrap();
        // Quest 3 does not advertise D3D12_enable.
        assert!(MockSession::create(&inst, GraphicsBinding::D3D12).is_err());
    }

    #[test]
    fn happy_path_lifecycle() {
        let inst = MockInstance::quest3_default().unwrap();
        let mut s = MockSession::create(&inst, GraphicsBinding::Vulkan).unwrap();
        s.run_to_focused();
        assert_eq!(s.state(), XrSessionState::Focused);
    }

    #[test]
    fn shutdown_lifecycle() {
        let inst = MockInstance::quest3_default().unwrap();
        let mut s = MockSession::create(&inst, GraphicsBinding::Vulkan).unwrap();
        s.run_to_focused();
        s.run_shutdown();
        assert_eq!(s.state(), XrSessionState::Exiting);
    }

    #[test]
    fn tick_frame_only_in_render_states() {
        let inst = MockInstance::quest3_default().unwrap();
        let mut s = MockSession::create(&inst, GraphicsBinding::Vulkan).unwrap();
        // Idle : refused.
        assert!(s.tick_frame().is_err());
        s.run_to_focused();
        assert!(s.tick_frame().is_ok());
        assert_eq!(s.frame_count(), 1);
        s.tick_frame().unwrap();
        s.tick_frame().unwrap();
        assert_eq!(s.frame_count(), 3);
    }

    #[test]
    fn graphics_binding_as_str() {
        assert_eq!(GraphicsBinding::Vulkan.as_str(), "vulkan");
        assert_eq!(GraphicsBinding::CompositorServices.as_str(), "compositor-services");
        assert!(GraphicsBinding::CompositorServices.is_bridge());
        assert!(!GraphicsBinding::Vulkan.is_bridge());
    }

    #[test]
    fn state_as_str() {
        assert_eq!(XrSessionState::Idle.as_str(), "idle");
        assert_eq!(XrSessionState::Focused.as_str(), "focused");
        assert_eq!(XrSessionState::LossPending.as_str(), "loss-pending");
    }
}
