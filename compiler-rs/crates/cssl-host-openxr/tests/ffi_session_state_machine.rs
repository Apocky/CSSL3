//! § T11-D260 (W-H3) — FFI integration test : session state-machine.
//!
//! Confirms the `MockSession` advances IDLE → READY → SYNCHRONIZED →
//! VISIBLE → FOCUSED on the runtime-driven event-stream + falls back to
//! IDLE via STOPPING + END_SESSION.

use cssl_host_openxr::ffi::{
    MockSession, SessionBeginInfo, SessionState, StateMachineEvent, XrResult,
};

#[test]
fn full_lifecycle_reaches_focused_then_returns_to_idle() {
    let mut s = MockSession::create(0xC551_BEEF).expect("create");
    assert_eq!(s.state, SessionState::Idle);
    let r = s.drive_to_focused();
    assert_eq!(r, XrResult::SUCCESS);
    assert!(s.reached_focused());
    assert_eq!(s.state, SessionState::Focused);
    assert!(s.running);
    assert_eq!(s.focused_frames, 1);

    let r = s.drive_to_idle_from_focused();
    assert_eq!(r, XrResult::SUCCESS);
    assert_eq!(s.state, SessionState::Idle);
    assert!(!s.running);
}

#[test]
fn state_history_records_each_transition() {
    let mut s = MockSession::create(0xC551_BEEF).expect("create");
    let _ = s.drive_to_focused();
    // History : Idle, Ready, Synchronized, Visible, Focused.
    assert!(s.history.len() >= 5);
    assert_eq!(s.history[0], SessionState::Idle);
    assert!(s.history.contains(&SessionState::Ready));
    assert!(s.history.contains(&SessionState::Synchronized));
    assert!(s.history.contains(&SessionState::Visible));
    assert!(s.history.contains(&SessionState::Focused));
}

#[test]
fn loss_pending_event_advances_to_loss_pending_state() {
    let mut s = MockSession::create(0xC551_BEEF).expect("create");
    let r = s.handle_event(StateMachineEvent::InstanceLossPending);
    assert_eq!(r, XrResult::SUCCESS);
    assert_eq!(s.state, SessionState::LossPending);
}

#[test]
fn begin_then_request_exit_drives_to_exiting() {
    let mut s = MockSession::create(0xC551_BEEF).expect("create");
    let _ = s.handle_event(StateMachineEvent::StateChanged(SessionState::Ready));
    let r = s.begin_session(&SessionBeginInfo::stereo_hmd());
    assert_eq!(r, XrResult::SUCCESS);
    let r = s.request_exit();
    assert_eq!(r, XrResult::SUCCESS);
    assert_eq!(s.state, SessionState::Exiting);
}

#[test]
fn out_of_order_end_session_is_rejected() {
    let mut s = MockSession::create(0xC551_BEEF).expect("create");
    let r = s.end_session();
    assert_eq!(r, XrResult::ERROR_SESSION_NOT_STOPPING);
}
