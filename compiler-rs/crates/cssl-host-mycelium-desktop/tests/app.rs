//! § app tests — `MyceliumApp` lifecycle + cap orchestration.
//!
//! § Note : these tests intentionally hold the `agent_loop` mutex through
//! a sequence of assertions to verify a stable cap-policy snapshot ;
//! clippy's significant_drop_tightening lint flags the held-then-dropped
//! pattern. The held lock IS the unit of test-atomicity, so allow at
//! file-level.
#![allow(clippy::significant_drop_tightening)]

use cssl_host_mycelium_desktop::{AppConfig, GrantMode, MyceliumApp, ToolName};

#[path = "common/mod.rs"]
mod common;

#[test]
fn app_new_with_default_config_succeeds() {
    let app = MyceliumApp::new(AppConfig::default()).expect("default config builds");
    // Default cap-mode is `Default` → all 12 tools allowed, only reads auto.
    let snap = app.get_session();
    assert!(snap.id.starts_with("session-"), "session id shape");
    assert_eq!(snap.turn_count, 0);
}

#[test]
fn run_turn_substrate_mode_completes_e2e() {
    let app = common::make_app();
    let result = app
        .run_turn("describe the system architecture")
        .expect("turn ok");
    assert_eq!(result.turn_id, 1);
    assert!(!result.final_reply.is_empty(), "Mode-C produces a reply");
    // Mode-C ¬ tool-loop yet → 0 dispatched.
    assert_eq!(result.tool_calls_executed, 0);
}

#[test]
fn run_turn_returns_turn_result_with_id() {
    let app = common::make_app();
    let r1 = app.run_turn("plan the orchestration").expect("turn 1");
    let r2 = app.run_turn("compile the bug fix").expect("turn 2");
    assert_eq!(r1.turn_id, 1);
    assert_eq!(r2.turn_id, 2);
    let snap = app.get_session();
    assert_eq!(snap.turn_count, 2);
}

#[test]
fn cancel_current_turn_no_op_when_idle() {
    let app = common::make_app();
    // Cancelling without an in-flight turn must succeed (idempotent).
    app.cancel_current_turn().expect("cancel idempotent");
    app.cancel_current_turn().expect("cancel idempotent twice");
}

#[test]
fn revoke_all_sovereign_caps_resets_to_paranoid() {
    let app = common::make_app();
    app.revoke_all_sovereign_caps().expect("revoke ok");
    // Verify the cap-mode is now paranoid by attempting a write-grant —
    // paranoid does not include FileWrite in `allow`, but `grant_cap` adds it.
    // We assert via the loop's caps shape directly :
    let loop_ = app.agent_loop.lock().expect("lock");
    assert!(
        !loop_.caps.allow.contains(&ToolName::FileWrite),
        "paranoid mode must drop FileWrite from allow"
    );
    assert!(
        loop_.caps.allow.contains(&ToolName::FileRead),
        "paranoid mode keeps FileRead"
    );
}

#[test]
fn grant_cap_then_revoke_cycles() {
    let app = common::make_default_user_app();
    // Grant FileWrite as auto.
    app.grant_cap(ToolName::FileWrite, GrantMode::Auto)
        .expect("grant ok");
    {
        let loop_ = app.agent_loop.lock().expect("lock");
        assert!(loop_.caps.auto_approve.contains(&ToolName::FileWrite));
    }
    // Revoke.
    app.revoke_cap(ToolName::FileWrite).expect("revoke ok");
    {
        let loop_ = app.agent_loop.lock().expect("lock");
        assert!(!loop_.caps.allow.contains(&ToolName::FileWrite));
        assert!(!loop_.caps.auto_approve.contains(&ToolName::FileWrite));
    }
}
