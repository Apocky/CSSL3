//! Integration tests for `From<T>` impl exhaustiveness.
//!
//! § SPEC : `_drafts/phase_j/05_l0_l1_error_log_spec.md` § 1.2 + § 7.4.
//!
//! § COVERAGE
//!   - Foundation conversions (cssl-telemetry + cssl-substrate-prime-directive)
//!   - std::io::Error lift
//!   - Display-based per-crate-error catcher (`from_crate_err`)
//!   - PrimeDirectiveViolation lift
//!   - PanicReport lift

use cssl_error::{
    CrateErrorPayload, EngineError, IoErrorKind, PanicReport, PrimeDirectiveViolation, Severable,
    Severity, SubsystemTag,
};

#[test]
fn from_telemetry_ring_overflow() {
    let r: cssl_telemetry::RingError = cssl_telemetry::RingError::Overflow;
    let e: EngineError = r.into();
    assert!(matches!(e, EngineError::Telemetry(_)));
    assert_eq!(e.subsystem(), SubsystemTag::Telemetry);
}

#[test]
fn from_telemetry_audit_signature() {
    let a: cssl_telemetry::AuditError = cssl_telemetry::AuditError::SignatureInvalid;
    let e: EngineError = a.into();
    assert!(matches!(e, EngineError::Audit(_)));
    assert_eq!(e.subsystem(), SubsystemTag::Audit);
}

#[test]
fn from_telemetry_audit_chain_break() {
    let a: cssl_telemetry::AuditError = cssl_telemetry::AuditError::ChainBreak { seq: 42 };
    let e: EngineError = a.into();
    assert!(matches!(e, EngineError::Audit(_)));
}

#[test]
fn from_telemetry_audit_genesis_invalid() {
    let a: cssl_telemetry::AuditError = cssl_telemetry::AuditError::GenesisPrevNonZero;
    let e: EngineError = a.into();
    assert!(matches!(e, EngineError::Audit(_)));
}

#[test]
fn from_telemetry_audit_invalid_sequence() {
    let a: cssl_telemetry::AuditError = cssl_telemetry::AuditError::InvalidSequence {
        expected: 7,
        actual: 9,
    };
    let e: EngineError = a.into();
    assert!(matches!(e, EngineError::Audit(_)));
}

#[test]
fn from_path_log_raw_path() {
    let p: cssl_telemetry::PathLogError = cssl_telemetry::PathLogError::RawPathInField {
        field: "/etc/hosts".into(),
    };
    let e: EngineError = p.into();
    assert!(matches!(e, EngineError::PathLog(_)));
}

#[test]
fn from_io_error_not_found() {
    let std_err = std::io::Error::new(std::io::ErrorKind::NotFound, "missing");
    let e: EngineError = std_err.into();
    match e {
        EngineError::Io { kind, retryable } => {
            assert_eq!(kind, IoErrorKind::NotFound);
            assert!(!retryable);
        }
        _ => panic!("expected Io"),
    }
}

#[test]
fn from_io_error_timed_out_is_retryable() {
    let std_err = std::io::Error::new(std::io::ErrorKind::TimedOut, "timeout");
    let e: EngineError = std_err.into();
    match e {
        EngineError::Io { retryable: true, .. } => {}
        _ => panic!("expected retryable Io"),
    }
}

#[test]
fn from_io_error_permission_denied_terminal() {
    let std_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "x");
    let e: EngineError = std_err.into();
    match e {
        EngineError::Io { kind, retryable } => {
            assert_eq!(kind, IoErrorKind::PermissionDenied);
            assert!(!retryable);
        }
        _ => panic!("expected Io"),
    }
}

#[test]
fn from_io_error_unknown_maps_to_other_kind() {
    let std_err = std::io::Error::new(std::io::ErrorKind::AddrInUse, "x");
    let e: EngineError = std_err.into();
    match e {
        EngineError::Io { kind, .. } => {
            assert_eq!(kind, IoErrorKind::Other);
        }
        _ => panic!("expected Io"),
    }
}

#[test]
fn from_pd_violation_canonical_code() {
    let v = PrimeDirectiveViolation::new("PD0001", "harm");
    let e: EngineError = v.into();
    assert!(e.is_prime_directive_violation());
    assert_eq!(e.severity(), Severity::Fatal);
}

#[test]
fn from_pd_violation_extension_code() {
    let v = PrimeDirectiveViolation::new("PD0018", "raw-path");
    let e: EngineError = v.into();
    assert!(e.is_prime_directive_violation());
}

#[test]
fn from_panic_report_basic() {
    let r = PanicReport::new("oops", SubsystemTag::Render);
    let e: EngineError = r.into();
    assert!(e.is_panic());
}

#[test]
fn from_panic_report_pd_tagged_is_fatal() {
    let r = PanicReport::new("PD0001 : harm", SubsystemTag::PrimeDirective)
        .with_pd_violation(true);
    let e: EngineError = r.into();
    assert_eq!(e.severity(), Severity::Fatal);
}

#[test]
fn from_crate_err_render() {
    let custom_err = "shader-compile-failed";
    let e = EngineError::from_crate_err("cssl-render-v2", custom_err);
    assert_eq!(e.subsystem(), SubsystemTag::Render);
    let s = format!("{e}");
    assert!(s.contains("cssl-render-v2"));
    assert!(s.contains("shader-compile-failed"));
}

#[test]
fn from_crate_err_unknown_crate_is_other() {
    let e = EngineError::from_crate_err("not-a-real-crate-name-xyz", "x");
    assert_eq!(e.subsystem(), SubsystemTag::Other);
}

#[test]
fn from_crate_err_with_severity_warning() {
    let e =
        EngineError::from_crate_err_with_severity("cssl-anim", "minor-issue", Severity::Warning);
    assert_eq!(e.severity(), Severity::Warning);
    assert_eq!(e.subsystem(), SubsystemTag::Anim);
}

#[test]
fn from_crate_err_with_severity_info() {
    let e = EngineError::from_crate_err_with_severity("cssl-host", "x", Severity::Info);
    assert_eq!(e.severity(), Severity::Info);
}

#[test]
fn render_constructor_ergonomic() {
    let e = EngineError::render("cssl-render-v2", "shader-compile-failed");
    let s = format!("{e}");
    assert!(s.contains("[render]"));
    assert!(s.contains("shader-compile-failed"));
}

#[test]
fn wave_constructor_ergonomic() {
    let e = EngineError::wave("cssl-wave-solver", "lbm-instability");
    assert!(format!("{e}").contains("[wave]"));
}

#[test]
fn audio_constructor_ergonomic() {
    let e = EngineError::audio("cssl-host-audio", "device-disconnected");
    assert!(format!("{e}").contains("[audio]"));
}

#[test]
fn physics_constructor_ergonomic() {
    let e = EngineError::physics("cssl-physics-wave", "xpbd-divergence");
    assert!(format!("{e}").contains("[physics]"));
}

#[test]
fn anim_constructor_ergonomic() {
    let e = EngineError::anim("cssl-anim-procedural", "curve-overflow");
    assert!(format!("{e}").contains("[anim]"));
}

#[test]
fn codegen_constructor_ergonomic() {
    let e = EngineError::codegen("cssl-cgen-cpu-x64", "regalloc-spill");
    assert!(format!("{e}").contains("[codegen]"));
}

#[test]
fn asset_constructor_ergonomic() {
    let e = EngineError::asset("cssl-asset", "missing-glb");
    assert!(format!("{e}").contains("[asset]"));
}

#[test]
fn effects_constructor_ergonomic() {
    let e = EngineError::effects("cssl-effects", "row-mismatch");
    assert!(format!("{e}").contains("[effects]"));
}

#[test]
fn work_graph_constructor_ergonomic() {
    let e = EngineError::work_graph("cssl-work-graph", "stage-loop-detected");
    assert!(format!("{e}").contains("[work_graph]"));
}

#[test]
fn ai_constructor_ergonomic() {
    let e = EngineError::ai("cssl-ai-behav", "decision-tree-loop");
    assert!(format!("{e}").contains("[ai]"));
}

#[test]
fn gaze_constructor_ergonomic() {
    let e = EngineError::gaze("cssl-gaze-collapse", "calibration-failed");
    assert!(format!("{e}").contains("[gaze]"));
}

#[test]
fn host_constructor_ergonomic() {
    let e = EngineError::host("cssl-host-vulkan", "device-lost");
    assert!(format!("{e}").contains("[host]"));
}

#[test]
fn network_constructor_ergonomic() {
    let e = EngineError::network("cssl-host-net", "connection-reset");
    assert!(format!("{e}").contains("[network]"));
}

#[test]
fn other_constructor_permitted_but_lint_discouraged() {
    let e = EngineError::other("genuinely-untyped-error");
    assert!(format!("{e}").contains("[other]"));
    assert_eq!(e.subsystem(), SubsystemTag::Other);
}

#[test]
fn engine_error_kind_id_for_each_variant_unique_and_nonzero() {
    let variants: Vec<EngineError> = vec![
        EngineError::render("c", "x"),
        EngineError::wave("c", "x"),
        EngineError::audio("c", "x"),
        EngineError::physics("c", "x"),
        EngineError::anim("c", "x"),
        EngineError::codegen("c", "x"),
        EngineError::asset("c", "x"),
        EngineError::effects("c", "x"),
        EngineError::work_graph("c", "x"),
        EngineError::ai("c", "x"),
        EngineError::gaze("c", "x"),
        EngineError::host("c", "x"),
        EngineError::network("c", "x"),
        EngineError::Telemetry(cssl_telemetry::RingError::Overflow),
        EngineError::Audit(cssl_telemetry::AuditError::SignatureInvalid),
        EngineError::PathLog(cssl_telemetry::PathLogError::RawPathInField {
            field: "x".into(),
        }),
        EngineError::io(IoErrorKind::NotFound),
        EngineError::from_crate_err("c", "x"),
        EngineError::Panic(PanicReport::new("p", SubsystemTag::Render)),
        EngineError::PrimeDirective(PrimeDirectiveViolation::new("PD0001", "x")),
        EngineError::other("misc"),
    ];
    let mut ids: Vec<u32> = variants.iter().map(|e| e.kind_id().as_u32()).collect();
    let count_before_dedup = ids.len();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), count_before_dedup, "kind-IDs must be unique");
    // None should be zero (the "unknown" sentinel).
    for id in &ids {
        assert_ne!(*id, 0);
    }
}

#[test]
fn engine_error_question_mark_lifts_telemetry_overflow() {
    fn inner() -> Result<(), EngineError> {
        Err(cssl_telemetry::RingError::Overflow)?
    }
    let r = inner();
    assert!(matches!(r, Err(EngineError::Telemetry(_))));
}

#[test]
fn engine_error_question_mark_lifts_audit_failure() {
    fn inner() -> Result<(), EngineError> {
        Err(cssl_telemetry::AuditError::SignatureInvalid)?
    }
    let r = inner();
    assert!(matches!(r, Err(EngineError::Audit(_))));
}

#[test]
fn engine_error_question_mark_lifts_path_log() {
    fn inner() -> Result<(), EngineError> {
        Err(cssl_telemetry::PathLogError::RawPathInField {
            field: "x".into(),
        })?
    }
    let r = inner();
    assert!(matches!(r, Err(EngineError::PathLog(_))));
}

#[test]
fn engine_error_question_mark_lifts_io_error() {
    fn inner() -> Result<(), EngineError> {
        Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "x"))?
    }
    let r = inner();
    assert!(matches!(r, Err(EngineError::Io { .. })));
}

#[test]
fn crate_payload_constructors_consistent() {
    let p1 = CrateErrorPayload::new("test", "msg", Severity::Warning);
    let p2 = CrateErrorPayload::from_display("test", "msg");
    assert_eq!(p1.crate_name, p2.crate_name);
    assert_eq!(p1.message, p2.message);
    // from_display uses default severity Error ; new uses provided severity.
    assert_eq!(p1.severity, Severity::Warning);
    assert_eq!(p2.severity, Severity::Error);
}

#[test]
fn arbitrary_display_can_be_classified_via_crate_err() {
    // from_crate_err can accept any Display ; this is the catch-all for
    // per-crate errors that don't have a dedicated typed variant.
    let msg = format!("custom-error : {}", 42);
    let e = EngineError::from_crate_err("cssl-substrate-prime-directive", msg);
    assert_eq!(e.subsystem(), SubsystemTag::PrimeDirective);
    let s = format!("{e}");
    assert!(s.contains("custom-error"));
    assert!(s.contains("42"));
}
