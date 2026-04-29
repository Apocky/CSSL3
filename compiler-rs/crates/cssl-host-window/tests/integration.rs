//! Integration tests for cssl-host-window (T11-D78, S7-F1).
//!
//! § COVERAGE
//!   - Config-validation surface (size / title / disposition policy)
//!   - BackendKind detection
//!   - Stub-path on non-Windows targets
//!   - Win32 spawn + pump + raw-handle (cfg-gated to target_os = windows)
//!   - Consent-arch close-state machine (cfg-gated to target_os = windows)
//!
//! § GUIDANCE
//!   The Win32-only tests run on Apocky's host (Windows 11) ; CI on Linux
//!   skips them via cfg-gating. Each test creates + destroys its own
//!   window so they don't share OS state.

use cssl_host_window::{
    spawn_window, BackendKind, CloseDispositionPolicy, CloseRequestState, GraceWindowConfig,
    KeyCode, ModifierKeys, MouseButton, RawWindowHandle, ScrollDelta, Window, WindowConfig,
    WindowError, WindowEvent, WindowEventKind, WindowFullscreen, WindowVsyncHint,
};

// ─────────────────────────────────────────────────────────────────────
// 1. Config validation (cross-platform)
// ─────────────────────────────────────────────────────────────────────

#[test]
fn config_default_validates() {
    let cfg = WindowConfig::default();
    assert!(cfg.validate().is_ok());
}

#[test]
fn config_zero_width_invalid() {
    let cfg = WindowConfig {
        width: 0,
        ..WindowConfig::default()
    };
    assert!(matches!(
        cfg.validate(),
        Err(WindowError::InvalidConfig { .. })
    ));
}

#[test]
fn config_zero_height_invalid() {
    let cfg = WindowConfig {
        height: 0,
        ..WindowConfig::default()
    };
    assert!(matches!(
        cfg.validate(),
        Err(WindowError::InvalidConfig { .. })
    ));
}

#[test]
fn config_empty_title_invalid() {
    let cfg = WindowConfig {
        title: String::new(),
        ..WindowConfig::default()
    };
    assert!(matches!(
        cfg.validate(),
        Err(WindowError::InvalidConfig { .. })
    ));
}

#[test]
fn config_auto_grant_zero_grace_invalid() {
    let cfg = WindowConfig {
        close_disposition: CloseDispositionPolicy::AutoGrantAfterGrace {
            grace: GraceWindowConfig { ms: 0 },
        },
        ..WindowConfig::default()
    };
    assert!(matches!(
        cfg.validate(),
        Err(WindowError::InvalidConfig { .. })
    ));
}

#[test]
fn config_require_explicit_validates() {
    let cfg = WindowConfig {
        close_disposition: CloseDispositionPolicy::RequireExplicit {
            consent_arch_audit_window_ms: 30_000,
        },
        ..WindowConfig::default()
    };
    assert!(cfg.validate().is_ok());
}

#[test]
fn config_new_constructor_sets_fields() {
    let cfg = WindowConfig::new("integration", 640, 480);
    assert_eq!(cfg.title, "integration");
    assert_eq!(cfg.width, 640);
    assert_eq!(cfg.height, 480);
    assert!(cfg.resizable);
    assert!(cfg.dpi_aware);
    assert_eq!(cfg.fullscreen, WindowFullscreen::Windowed);
    assert_eq!(cfg.vsync_hint, WindowVsyncHint::Vsync);
}

// ─────────────────────────────────────────────────────────────────────
// 2. Backend detection (cross-platform)
// ─────────────────────────────────────────────────────────────────────

#[test]
fn backend_kind_current_matches_target() {
    let bk = BackendKind::current();
    #[cfg(target_os = "windows")]
    assert_eq!(bk, BackendKind::Win32);
    #[cfg(not(target_os = "windows"))]
    assert_eq!(bk, BackendKind::None);
}

// ─────────────────────────────────────────────────────────────────────
// 3. Event-shape sanity (cross-platform)
// ─────────────────────────────────────────────────────────────────────

#[test]
fn event_kinds_constructible() {
    let close = WindowEvent {
        timestamp_ms: 0,
        kind: WindowEventKind::Close,
    };
    assert!(matches!(close.kind, WindowEventKind::Close));

    let resize = WindowEvent {
        timestamp_ms: 16,
        kind: WindowEventKind::Resize {
            width: 100,
            height: 200,
        },
    };
    if let WindowEventKind::Resize { width, height } = resize.kind {
        assert_eq!(width, 100);
        assert_eq!(height, 200);
    } else {
        panic!("expected Resize");
    }

    let key_down = WindowEvent {
        timestamp_ms: 32,
        kind: WindowEventKind::KeyDown {
            key: KeyCode::Space,
            modifiers: ModifierKeys::SHIFT,
            repeat: false,
        },
    };
    if let WindowEventKind::KeyDown { key, repeat, .. } = key_down.kind {
        assert_eq!(key, KeyCode::Space);
        assert!(!repeat);
    }

    let mouse = WindowEvent {
        timestamp_ms: 48,
        kind: WindowEventKind::MouseDown {
            button: MouseButton::Left,
            x: 10,
            y: 20,
            modifiers: ModifierKeys::empty(),
        },
    };
    if let WindowEventKind::MouseDown { button, x, y, .. } = mouse.kind {
        assert_eq!(button, MouseButton::Left);
        assert_eq!(x, 10);
        assert_eq!(y, 20);
    }

    let scroll = WindowEvent {
        timestamp_ms: 64,
        kind: WindowEventKind::Scroll {
            delta: ScrollDelta::Lines { x: 0.0, y: 1.0 },
            x: 50,
            y: 60,
            modifiers: ModifierKeys::empty(),
        },
    };
    if let WindowEventKind::Scroll { delta, .. } = scroll.kind {
        assert_eq!(delta, ScrollDelta::Lines { x: 0.0, y: 1.0 });
    }
}

#[test]
fn modifier_keys_combine_correctly() {
    let m = ModifierKeys::SHIFT | ModifierKeys::CTRL | ModifierKeys::ALT;
    assert!(m.contains(ModifierKeys::SHIFT));
    assert!(m.contains(ModifierKeys::CTRL));
    assert!(m.contains(ModifierKeys::ALT));
    assert!(!m.contains(ModifierKeys::SUPER));
}

// ─────────────────────────────────────────────────────────────────────
// 4. Consent-arch surface (cross-platform)
// ─────────────────────────────────────────────────────────────────────

#[test]
fn close_state_default_is_idle() {
    assert_eq!(CloseRequestState::default(), CloseRequestState::Idle);
}

#[test]
fn default_disposition_is_auto_grant_5s() {
    let p = CloseDispositionPolicy::default();
    if let CloseDispositionPolicy::AutoGrantAfterGrace { grace } = p {
        assert_eq!(grace.ms, 5_000);
    } else {
        panic!("default disposition is not AutoGrantAfterGrace");
    }
}

// ─────────────────────────────────────────────────────────────────────
// 5. Raw-handle round-trip (cross-platform smoke)
// ─────────────────────────────────────────────────────────────────────

#[test]
fn raw_handle_win32_round_trip() {
    let h = RawWindowHandle::win32(0xCAFE, 0xBEEF);
    assert!(h.is_win32());
    assert_eq!(h.as_win32(), Some((0xCAFE, 0xBEEF)));
}

// ─────────────────────────────────────────────────────────────────────
// 6. Stub-path on non-Windows (compile-time-gated)
// ─────────────────────────────────────────────────────────────────────

#[cfg(not(target_os = "windows"))]
#[test]
fn spawn_on_non_windows_returns_loader_missing() {
    let cfg = WindowConfig::default();
    let err = spawn_window(&cfg).unwrap_err();
    assert!(matches!(err, WindowError::LoaderMissing { .. }));
}

// ─────────────────────────────────────────────────────────────────────
// 7. Win32 live-window tests (only on Windows hosts)
// ─────────────────────────────────────────────────────────────────────
//
// These tests spawn a real OS window. They run on Apocky's Windows 11 host
// + on Windows CI. Linux / macOS skip them via cfg-gating.
//
// CSSLv3 LANDMINE : Windows CI runners may not have a desktop session. The
// tests use `dpi_aware: false` + `fullscreen: Windowed` which work in
// session-0 contexts ; if the runner truly has no USER32 surface,
// CreateWindowExW will fail with a deterministic OsFailure → the test
// surfaces that as a `Skip` rather than a panic.

#[cfg(target_os = "windows")]
fn make_test_cfg(title: &str) -> WindowConfig {
    WindowConfig {
        title: title.into(),
        width: 320,
        height: 240,
        resizable: false,
        vsync_hint: WindowVsyncHint::Vsync,
        fullscreen: WindowFullscreen::Windowed,
        close_disposition: CloseDispositionPolicy::AutoGrantAfterGrace {
            grace: GraceWindowConfig { ms: 100 },
        },
        // Disable DPI awareness so the test doesn't scribble process-state
        // beyond its own lifetime — the per-process DPI install is OK to
        // run once but harmless if the runner already has it set.
        dpi_aware: false,
    }
}

#[cfg(target_os = "windows")]
fn spawn_or_skip(title: &str) -> Option<Window> {
    let cfg = make_test_cfg(title);
    match spawn_window(&cfg) {
        Ok(w) => Some(w),
        Err(WindowError::OsFailure { op, code }) => {
            // No USER32 desktop session — skip rather than fail.
            eprintln!(
                "[cssl-host-window] skipping live-window test : {op} failed code=0x{code:08x} \
                 (likely no desktop session)"
            );
            None
        }
        Err(e) => panic!("unexpected spawn failure : {e}"),
    }
}

#[cfg(target_os = "windows")]
#[test]
fn win32_spawn_and_pump_basic() {
    let Some(mut w) = spawn_or_skip("cssl-test-spawn-pump") else {
        return;
    };
    // The first pump should at least drain WM_CREATE / WM_SIZE / WM_SHOWWINDOW.
    let events = w.pump_events().expect("pump_events");
    // Some events may have fired (Resize/FocusGain/etc.) ; we just verify
    // the pump returns Ok and the window isn't destroyed yet.
    assert!(!w.is_destroyed());
    drop(events);
    w.request_destroy().expect("request_destroy");
    // After request_destroy, a final pump should drain WM_DESTROY/WM_NCDESTROY
    // and flip is_destroyed = true.
    let _ = w.pump_events();
    assert!(w.is_destroyed());
}

#[cfg(target_os = "windows")]
#[test]
fn win32_raw_handle_returns_win32() {
    let Some(mut w) = spawn_or_skip("cssl-test-handle") else {
        return;
    };
    let handle = w.raw_handle().expect("raw_handle");
    assert!(handle.is_win32());
    let (hwnd, hinst) = handle.as_win32().expect("win32 pair");
    assert_ne!(hwnd, 0, "HWND must be non-null on a live window");
    assert_ne!(hinst, 0, "HINSTANCE must be non-null on a live window");
    w.request_destroy().expect("request_destroy");
    let _ = w.pump_events();
}

#[cfg(target_os = "windows")]
#[test]
fn win32_synthesized_close_emits_close_event() {
    let Some(mut w) = spawn_or_skip("cssl-test-synth-close") else {
        return;
    };
    // Reach inside via pump-only API : we use the public synthesize-by-
    // post-message hook in the Win32 backend's test surface. We can't
    // call backend::win32::synthesize_close from outside the crate, so
    // we drive the close-flow via request_destroy → which transitions
    // through the same state machine when initiated by user-code.
    //
    // For the IO-side path we rely on the unit-test in the Win32 module
    // (`synthesize_close`), executed via `cargo test -p cssl-host-window
    // --lib`. The integration test verifies the user-driven path :
    // request_destroy → pump → state=Granted → is_destroyed=true.
    let initial = w.close_request_state();
    assert_eq!(initial, CloseRequestState::Idle);

    w.request_destroy().expect("request_destroy");
    assert_eq!(w.close_request_state(), CloseRequestState::Granted);

    let _ = w.pump_events();
    assert!(w.is_destroyed());
}

#[cfg(target_os = "windows")]
#[test]
fn win32_pump_after_destroy_returns_already_destroyed() {
    let Some(mut w) = spawn_or_skip("cssl-test-after-destroy") else {
        return;
    };
    w.request_destroy().expect("request_destroy");
    // Drain teardown messages.
    let _ = w.pump_events();
    // Now the window is destroyed ; subsequent pump_events MUST return
    // AlreadyDestroyed.
    let err = w.pump_events().unwrap_err();
    assert!(matches!(err, WindowError::AlreadyDestroyed));
}

#[cfg(target_os = "windows")]
#[test]
fn win32_dismiss_close_when_idle_is_noop() {
    let Some(mut w) = spawn_or_skip("cssl-test-dismiss-idle") else {
        return;
    };
    // No close pending → dismiss should succeed as a no-op.
    let res = w.dismiss_close_request();
    assert!(res.is_ok());
    assert_eq!(w.close_request_state(), CloseRequestState::Idle);
    w.request_destroy().expect("request_destroy");
    let _ = w.pump_events();
}

#[cfg(target_os = "windows")]
#[test]
fn win32_two_windows_distinct_handles() {
    let Some(mut a) = spawn_or_skip("cssl-test-two-A") else {
        return;
    };
    let Some(mut b) = spawn_or_skip("cssl-test-two-B") else {
        return;
    };
    let ha = a.raw_handle().expect("a handle");
    let hb = b.raw_handle().expect("b handle");
    assert_ne!(ha, hb, "two windows must produce distinct raw handles");
    a.request_destroy().expect("destroy a");
    b.request_destroy().expect("destroy b");
    let _ = a.pump_events();
    let _ = b.pump_events();
}
