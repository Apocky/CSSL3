//! § ffi::session : `XrSession` lifecycle FFI + state-machine.
//!
//! § SPEC : OpenXR 1.0 § 7 (Session). The session is the bridge between
//!          the application's graphics-API (Vulkan / D3D12 / GLES /
//!          Metal-via-CompositorServices) and the runtime's compositor.
//!          The state-machine progresses :
//!            UNKNOWN → IDLE → READY → SYNCHRONIZED → VISIBLE → FOCUSED
//!          and back-down via STOPPING / LOSS_PENDING / EXITING.

use super::result::XrResult;
use super::types::{StructureType, ViewConfigurationType};

/// FFI handle for `XrSession`. Refcounted opaque on real runtimes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(transparent)]
pub struct SessionHandle(pub u64);

impl SessionHandle {
    pub const NULL: Self = Self(0);

    #[must_use]
    pub const fn is_null(self) -> bool {
        self.0 == 0
    }
}

/// `XrSessionState`. § 7.1 spec.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum SessionState {
    Unknown = 0,
    Idle = 1,
    Ready = 2,
    Synchronized = 3,
    Visible = 4,
    Focused = 5,
    Stopping = 6,
    LossPending = 7,
    Exiting = 8,
}

/// State-machine event delivered by `xrPollEvent` carrying an
/// `XrEventDataSessionStateChanged`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateMachineEvent {
    /// Runtime advanced to a new session state.
    StateChanged(SessionState),
    /// Runtime is recoverably losing the instance.
    InstanceLossPending,
    /// Engine called `xrRequestExitSession`.
    ExitRequested,
}

/// `XrSessionCreateInfo`. § 7 spec ; `next` chain carries the graphics-
/// API binding (`XrGraphicsBindingVulkan2KHR` etc.).
#[repr(C)]
pub struct SessionCreateInfo {
    pub ty: StructureType,
    pub next: *const core::ffi::c_void,
    pub create_flags: u64,
    pub system_id: u64,
}

impl core::fmt::Debug for SessionCreateInfo {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SessionCreateInfo")
            .field("ty", &self.ty)
            .field("create_flags", &self.create_flags)
            .field("system_id", &self.system_id)
            .finish()
    }
}

/// `XrSessionBeginInfo`. § 7.4 spec.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct SessionBeginInfo {
    pub ty: StructureType,
    pub next: *const core::ffi::c_void,
    pub primary_view_configuration_type: ViewConfigurationType,
}

impl SessionBeginInfo {
    /// Stereo-HMD canonical begin-info.
    #[must_use]
    pub const fn stereo_hmd() -> Self {
        Self {
            ty: StructureType::SessionBeginInfo,
            next: core::ptr::null(),
            primary_view_configuration_type: ViewConfigurationType::PrimaryStereo,
        }
    }
}

/// In-memory mock of an `XrSession` for tests + the `cssl-rt` STUB swap-in.
/// Carries the session-state machine + a recording log of state-transitions.
#[derive(Debug, Clone)]
pub struct MockSession {
    pub handle: SessionHandle,
    pub system_id: u64,
    pub state: SessionState,
    pub history: Vec<SessionState>,
    pub running: bool,
    pub focused_frames: u64,
}

impl MockSession {
    /// `xrCreateSession` mock : initial state UNKNOWN → IDLE on success.
    pub fn create(system_id: u64) -> Result<Self, XrResult> {
        if system_id == 0 {
            return Err(XrResult::ERROR_HANDLE_INVALID);
        }
        Ok(Self {
            handle: SessionHandle(0xC551_5E55),
            system_id,
            state: SessionState::Idle,
            history: vec![SessionState::Idle],
            running: false,
            focused_frames: 0,
        })
    }

    /// `xrBeginSession` mock. Allowed only from READY.
    pub fn begin_session(&mut self, _info: &SessionBeginInfo) -> XrResult {
        if self.state != SessionState::Ready {
            return XrResult::ERROR_SESSION_NOT_READY;
        }
        self.transition_to(SessionState::Synchronized);
        self.running = true;
        XrResult::SUCCESS
    }

    /// `xrEndSession` mock. Allowed only from STOPPING.
    pub fn end_session(&mut self) -> XrResult {
        if self.state != SessionState::Stopping {
            return XrResult::ERROR_SESSION_NOT_STOPPING;
        }
        self.transition_to(SessionState::Idle);
        self.running = false;
        XrResult::SUCCESS
    }

    /// `xrRequestExitSession` mock. Always advances to EXITING.
    pub fn request_exit(&mut self) -> XrResult {
        if !self.running {
            return XrResult::ERROR_SESSION_NOT_RUNNING;
        }
        self.transition_to(SessionState::Exiting);
        XrResult::SUCCESS
    }

    /// Apply a runtime-emitted state-changed event to advance the state-
    /// machine. Validates the legal transitions per OpenXR § 7.1.
    pub fn handle_event(&mut self, event: StateMachineEvent) -> XrResult {
        match event {
            StateMachineEvent::StateChanged(target) => {
                if !is_legal_transition(self.state, target) {
                    return XrResult::ERROR_CALL_ORDER_INVALID;
                }
                self.transition_to(target);
                if target == SessionState::Focused {
                    self.focused_frames = self.focused_frames.saturating_add(1);
                }
                XrResult::SUCCESS
            }
            StateMachineEvent::InstanceLossPending => {
                self.transition_to(SessionState::LossPending);
                XrResult::SUCCESS
            }
            StateMachineEvent::ExitRequested => {
                self.transition_to(SessionState::Exiting);
                XrResult::SUCCESS
            }
        }
    }

    fn transition_to(&mut self, target: SessionState) {
        self.state = target;
        self.history.push(target);
    }

    /// `true` iff this session has progressed all the way to FOCUSED.
    #[must_use]
    pub fn reached_focused(&self) -> bool {
        self.history.iter().any(|s| *s == SessionState::Focused)
    }

    /// Drive the session through the canonical happy-path startup sequence :
    /// IDLE → READY → SYNCHRONIZED (via begin_session) → VISIBLE → FOCUSED.
    pub fn drive_to_focused(&mut self) -> XrResult {
        // IDLE → READY (event from runtime).
        let r = self.handle_event(StateMachineEvent::StateChanged(SessionState::Ready));
        if r.is_failure() {
            return r;
        }
        // READY → SYNCHRONIZED (begin_session).
        let r = self.begin_session(&SessionBeginInfo::stereo_hmd());
        if r.is_failure() {
            return r;
        }
        // SYNCHRONIZED → VISIBLE.
        let r = self.handle_event(StateMachineEvent::StateChanged(SessionState::Visible));
        if r.is_failure() {
            return r;
        }
        // VISIBLE → FOCUSED.
        self.handle_event(StateMachineEvent::StateChanged(SessionState::Focused))
    }

    /// Drive the session through the canonical shutdown sequence :
    /// FOCUSED → VISIBLE → SYNCHRONIZED → STOPPING → IDLE (via end_session).
    pub fn drive_to_idle_from_focused(&mut self) -> XrResult {
        if self.state != SessionState::Focused {
            return XrResult::ERROR_CALL_ORDER_INVALID;
        }
        let r = self.handle_event(StateMachineEvent::StateChanged(SessionState::Visible));
        if r.is_failure() {
            return r;
        }
        let r = self.handle_event(StateMachineEvent::StateChanged(SessionState::Synchronized));
        if r.is_failure() {
            return r;
        }
        let r = self.handle_event(StateMachineEvent::StateChanged(SessionState::Stopping));
        if r.is_failure() {
            return r;
        }
        self.end_session()
    }
}

/// Per OpenXR 1.0 § 7.1, the legal state-transitions form a directed graph.
#[must_use]
pub fn is_legal_transition(from: SessionState, to: SessionState) -> bool {
    use SessionState::{
        Exiting, Focused, Idle, LossPending, Ready, Stopping, Synchronized, Unknown, Visible,
    };
    match (from, to) {
        // From UNKNOWN (pre-create) the only path is to IDLE.
        (Unknown, Idle) => true,
        // IDLE allows the runtime to push to READY when ready, or to
        // EXITING / LOSS_PENDING.
        (Idle, Ready | Exiting | LossPending) => true,
        // READY → SYNCHRONIZED happens via xrBeginSession (modelled here
        // as the engine-driven path inside begin_session).
        // READY can also bail to EXITING / LOSS_PENDING.
        (Ready, Synchronized | Exiting | LossPending) => true,
        // SYNCHRONIZED ↔ VISIBLE ↔ FOCUSED (visibility ladder).
        (Synchronized, Visible | Stopping | Exiting | LossPending) => true,
        (Visible, Synchronized | Focused | Stopping | Exiting | LossPending) => true,
        (Focused, Visible | Stopping | Exiting | LossPending) => true,
        // STOPPING → IDLE (via xrEndSession) ; STOPPING can also be lost.
        (Stopping, Idle | LossPending) => true,
        // LOSS_PENDING / EXITING are terminal.
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::{MockSession, SessionState, StateMachineEvent, XrResult};

    #[test]
    fn create_starts_in_idle() {
        let s = MockSession::create(0xC551_BEEF).expect("create");
        assert_eq!(s.state, SessionState::Idle);
        assert!(!s.running);
    }

    #[test]
    fn create_with_null_system_id_is_handle_invalid() {
        let r = MockSession::create(0);
        assert_eq!(r.unwrap_err(), XrResult::ERROR_HANDLE_INVALID);
    }

    #[test]
    fn drive_to_focused_then_idle_round_trips_history() {
        let mut s = MockSession::create(0xC551_BEEF).expect("create");
        let r = s.drive_to_focused();
        assert_eq!(r, XrResult::SUCCESS);
        assert!(s.reached_focused());
        assert_eq!(s.state, SessionState::Focused);
        let r = s.drive_to_idle_from_focused();
        assert_eq!(r, XrResult::SUCCESS);
        assert_eq!(s.state, SessionState::Idle);
        assert!(!s.running);
    }

    #[test]
    fn begin_from_idle_is_session_not_ready() {
        let mut s = MockSession::create(0xC551_BEEF).expect("create");
        let r = s.begin_session(&super::SessionBeginInfo::stereo_hmd());
        assert_eq!(r, XrResult::ERROR_SESSION_NOT_READY);
    }

    #[test]
    fn illegal_transition_is_call_order_invalid() {
        let mut s = MockSession::create(0xC551_BEEF).expect("create");
        // IDLE → FOCUSED is not legal.
        let r = s.handle_event(StateMachineEvent::StateChanged(SessionState::Focused));
        assert_eq!(r, XrResult::ERROR_CALL_ORDER_INVALID);
    }
}
