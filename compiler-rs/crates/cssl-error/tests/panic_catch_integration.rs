//! Panic-catch integration tests.
//!
//! § SPEC : `_drafts/phase_j/05_l0_l1_error_log_spec.md` § 1.8 + § 4.2 + § 7.3.

use cssl_error::{
    catch_frame_panic, catch_frame_panic_simple, halt_for_pd_violation, payload_is_pd_violation,
    EngineError, PrimeDirectiveOrigin, PrimeDirectiveViolation, Severable, Severity, SubsystemTag,
};
use cssl_substrate_prime_directive::{CountingHaltSink, EnforcementAuditBus};

#[test]
fn frame_boundary_yields_structured_report_not_raw_panic() {
    let r = catch_frame_panic::<_, ()>(SubsystemTag::Render, 7, || panic!("oops"));
    match r {
        Err(EngineError::Panic(report)) => {
            // Structured fields are present : not just a raw string.
            assert_eq!(report.subsystem, SubsystemTag::Render);
            assert_eq!(report.frame_n, 7);
            assert!(report.message.contains("oops"));
            assert!(!report.is_pd_violation());
        }
        other => panic!("expected structured Panic variant, got {other:?}"),
    }
}

#[test]
fn frame_boundary_classifies_pd_panic_as_pd_violation() {
    let r = catch_frame_panic::<_, ()>(SubsystemTag::PrimeDirective, 1, || {
        panic!("PD0001 : harm-detected")
    });
    match r {
        Err(EngineError::Panic(report)) => {
            assert!(report.is_pd_violation());
        }
        other => panic!("expected PD-tagged Panic variant, got {other:?}"),
    }
}

#[test]
fn frame_boundary_pd_panic_severity_is_fatal() {
    let r = catch_frame_panic::<_, ()>(SubsystemTag::PrimeDirective, 1, || panic!("PD0001 : x"));
    if let Err(e) = r {
        assert_eq!(e.severity(), Severity::Fatal);
    } else {
        panic!("expected error");
    }
}

#[test]
fn frame_boundary_normal_panic_severity_is_error() {
    let r = catch_frame_panic::<_, ()>(SubsystemTag::Render, 1, || panic!("oops"));
    if let Err(e) = r {
        assert_eq!(e.severity(), Severity::Error);
    } else {
        panic!("expected error");
    }
}

#[test]
fn frame_boundary_does_not_alter_state_on_success() {
    // catch_frame_panic must not touch state on success paths.
    let mut counter = 0;
    let _ = catch_frame_panic_simple(SubsystemTag::Render, 0, || {
        counter += 1;
        counter
    });
    assert_eq!(counter, 1);
}

#[test]
fn frame_boundary_keeps_engine_running_conceptually() {
    // After catching a panic, the caller can continue.
    let r1 = catch_frame_panic::<_, ()>(SubsystemTag::Render, 1, || panic!("a"));
    assert!(r1.is_err());
    // Subsequent call works fine ; panic was localized.
    let r2 = catch_frame_panic::<_, i32>(SubsystemTag::Render, 2, || Ok(42));
    assert_eq!(r2.unwrap(), 42);
}

#[test]
fn frame_boundary_records_distinct_subsystems() {
    let r1 = catch_frame_panic::<_, ()>(SubsystemTag::Render, 1, || panic!("a"));
    let r2 = catch_frame_panic::<_, ()>(SubsystemTag::Audio, 1, || panic!("a"));
    let r3 = catch_frame_panic::<_, ()>(SubsystemTag::Anim, 1, || panic!("a"));
    if let (Err(EngineError::Panic(p1)), Err(EngineError::Panic(p2)), Err(EngineError::Panic(p3))) =
        (r1, r2, r3)
    {
        assert_eq!(p1.subsystem, SubsystemTag::Render);
        assert_eq!(p2.subsystem, SubsystemTag::Audio);
        assert_eq!(p3.subsystem, SubsystemTag::Anim);
    }
}

#[test]
fn pd_violation_panic_routes_to_halt_bridge() {
    // Simulate the engine's flow : catch panic ⟶ if PD-tagged, fire halt.
    let r = catch_frame_panic::<_, ()>(SubsystemTag::Render, 5, || {
        panic!("PD0001 : harm");
    });
    if let Err(EngineError::Panic(report)) = r {
        assert!(report.is_pd_violation());
        // Now route through halt-bridge.
        let v = PrimeDirectiveViolation::with_origin(
            "PD0001",
            report.message,
            PrimeDirectiveOrigin::PanicPayload,
        );
        let mut sink = CountingHaltSink::new(5);
        let mut audit = EnforcementAuditBus::new();
        let outcome = halt_for_pd_violation(&v, &mut sink, &mut audit);
        // Halt completed.
        assert_eq!(sink.pending, 0);
        assert!(audit.entry_count() >= 1);
        assert!(outcome.stats.outstanding_steps_drained == 5);
    } else {
        panic!("expected Panic variant");
    }
}

#[test]
fn payload_pd_detection_canonical() {
    let p1: Box<dyn std::any::Any + Send> = Box::new("PD0001 violation");
    assert!(payload_is_pd_violation(&*p1));
    let p2: Box<dyn std::any::Any + Send> = Box::new(String::from("PD0017"));
    assert!(payload_is_pd_violation(&*p2));
    let p3: Box<dyn std::any::Any + Send> = Box::new("regular panic");
    assert!(!payload_is_pd_violation(&*p3));
}

#[test]
fn nested_catch_frame_panic_works() {
    // Nested catches should each handle their own panic.
    let r = catch_frame_panic::<_, ()>(SubsystemTag::Render, 1, || {
        let inner = catch_frame_panic::<_, ()>(SubsystemTag::Audio, 1, || panic!("inner"));
        assert!(inner.is_err());
        Ok(())
    });
    assert!(r.is_ok());
}

#[test]
fn frame_boundary_ok_inner_result_propagates() {
    let r = catch_frame_panic::<_, i32>(SubsystemTag::Render, 0, || Ok(99));
    assert_eq!(r.unwrap(), 99);
}

#[test]
fn frame_boundary_inner_err_propagates() {
    let r = catch_frame_panic::<_, ()>(SubsystemTag::Render, 0, || {
        Err(EngineError::other("inner-error"))
    });
    match r {
        Err(EngineError::Other(s)) => assert_eq!(s, "inner-error"),
        _ => panic!("expected propagated inner error"),
    }
}

#[test]
fn frame_boundary_string_payload_extracted() {
    let r = catch_frame_panic::<_, ()>(SubsystemTag::Render, 0, || {
        let msg = String::from("dynamic message");
        panic!("{msg}");
    });
    if let Err(EngineError::Panic(report)) = r {
        assert!(report.message.contains("dynamic message"));
    }
}

#[test]
fn frame_boundary_static_str_payload_extracted() {
    let r = catch_frame_panic::<_, ()>(SubsystemTag::Render, 0, || {
        panic!("static-string");
    });
    if let Err(EngineError::Panic(report)) = r {
        assert_eq!(report.message, "static-string");
    }
}

#[test]
fn replay_determinism_panic_capture_idempotent() {
    // Two identical panic-injections produce identical structured reports
    // (modulo no wall-clock leak ; frame_n is the only time-source).
    let inject = || -> Result<(), EngineError> {
        catch_frame_panic::<_, ()>(SubsystemTag::Render, 100, || panic!("deterministic-panic"))
    };
    let r1 = inject();
    let r2 = inject();
    if let (Err(EngineError::Panic(a)), Err(EngineError::Panic(b))) = (r1, r2) {
        assert_eq!(a.message, b.message);
        assert_eq!(a.frame_n, b.frame_n);
        assert_eq!(a.subsystem, b.subsystem);
        assert_eq!(a.is_pd_violation(), b.is_pd_violation());
    } else {
        panic!("both should be Panic Err");
    }
}
