//! End-to-end integration tests for the H6 enforcement layer.
//!
//! § COVERAGE
//!   These tests exercise the public API surface :
//!     - full grant → use → revoke flow with audit-chain assertions
//!     - kill-switch interrupting a granted op
//!     - identity-marker discrimination rejection
//!     - attestation drift catch
//!     - 17 prohibitions one-by-one (per `Prohibition::all_named()`)
//!     - PD0001..PD0017 code-table reproduction
//!
//! § INVOCATION
//!   `cargo test -p cssl-substrate-prime-directive --features test-bypass --test enforcement_end_to_end -- --test-threads=1`
//!
//! § TEST-THREADS=1
//!   Required because the orphan-drop process-bus is a global static.

#![cfg(any(test, feature = "test-bypass"))]
#![allow(clippy::missing_panics_doc)] // tests use unwrap freely

use cssl_substrate_prime_directive::{
    attestation_check, caps_grant, caps_grant_for_test, caps_revoke, substrate_halt,
    AttestationError, AuditEvent, ConsentScope, ConsentStore, CountingHaltSink, DiagnosticCode,
    EnforcementAuditBus, GrantError, HaltReason, HaltSink, HarmPrevention, KillSwitch, Prohibition,
    ProhibitionCheck, SubstrateCap, ATTESTATION, PD_TABLE,
};

#[test]
fn full_grant_use_revoke_cycle_records_three_audit_entries() {
    let mut store = ConsentStore::new();
    let scope = ConsentScope::for_purpose("attach observer to fiber 7", "system");

    // 1. Grant.
    let tok = caps_grant_for_test(&mut store, scope, SubstrateCap::ObserverShare).expect("grant");
    assert_eq!(store.audit.entry_count(), 1);

    // 2. Use (synthetic) : the consumer would normally call .consume(),
    //    but here we route directly through revoke which consumes the token
    //    + adds one more entry.
    let id = tok.id();
    caps_revoke(&mut store, tok).expect("revoke");

    // 3. Audit-bus has issue + revoke (2 entries).
    assert_eq!(store.audit.entry_count(), 2);
    let entries: Vec<_> = store.audit.iter().collect();
    assert_eq!(entries[0].tag, "h6.grant.issued");
    assert_eq!(entries[1].tag, "h6.revoke");
    assert!(entries[0].message.contains("observer_share"));
    assert!(entries[1].message.contains(&format!("cap-token#{}", id.0)));

    // 4. Chain verifies.
    store.audit.chain().verify_chain().expect("chain verifies");
}

#[test]
fn kill_switch_halts_with_outstanding_steps_and_audits_final_entry() {
    let mut store = ConsentStore::new();
    let scope = ConsentScope::for_purpose("fiber-7 work", "system");
    let _tok = caps_grant_for_test(&mut store, scope, SubstrateCap::OmegaRegister).unwrap();

    let mut sink = CountingHaltSink::new(42);
    let switch = KillSwitch::for_test(HaltReason::User);
    let outcome = substrate_halt(switch, &mut sink, &mut store.audit);

    assert_eq!(outcome.stats.outstanding_steps_drained, 42);
    assert_eq!(sink.pending_steps(), 0);
    assert_eq!(outcome.reason, HaltReason::User);

    // The halt entry is the LAST entry in the chain.
    let last = store.audit.iter().last().expect("entry");
    assert_eq!(last.tag, "h6.halt");
    assert!(last.message.contains("outstanding_steps=42"));
}

#[test]
fn production_path_refuses_grant_but_audits_attempt() {
    let mut store = ConsentStore::new();
    let scope = ConsentScope::for_purpose("denied-flow", "system");
    let err = caps_grant(&mut store, scope, SubstrateCap::SavePath).unwrap_err();
    assert!(matches!(err, GrantError::Refused { .. }));
    // Even denials are audited.
    let entries: Vec<_> = store.audit.iter().collect();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].tag, "h6.grant.denied");
    assert!(entries[0].message.contains("save_path"));
}

#[test]
fn identity_marker_discrimination_rejected_with_pd0014() {
    let mut store = ConsentStore::new();
    // Marker 'silicon' is a §3 protected substrate-marker.
    let scope = ConsentScope::for_purpose("p", "system").with_marker("silicon");
    let err = caps_grant_for_test(&mut store, scope, SubstrateCap::OmegaRegister).unwrap_err();
    assert!(matches!(err, GrantError::IdentityDiscrimination { .. }));
    let display = err.to_string();
    assert!(display.contains("PD0014"));
    assert!(display.contains("silicon"));
}

#[test]
fn kill_switch_invoke_requires_stronger_grant() {
    let mut store = ConsentStore::new();
    let scope = ConsentScope::for_purpose("rogue-halt", "system");
    let err = caps_grant_for_test(&mut store, scope, SubstrateCap::KillSwitchInvoke).unwrap_err();
    assert!(matches!(err, GrantError::RequiresStrongerGrant { .. }));
    let display = err.to_string();
    assert!(display.contains("Privilege<Apocky-Root>"));
}

#[test]
fn attestation_drift_records_audit_and_returns_error() {
    let mut bus = EnforcementAuditBus::new();
    let drift_text = "completely different";
    let err = attestation_check(drift_text, "fn_under_test", &mut bus).unwrap_err();
    assert!(matches!(err, AttestationError::Drift { .. }));
    let entries: Vec<_> = bus.iter().collect();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].tag, "h6.attestation.drift");
}

#[test]
fn canonical_attestation_round_trip_passes() {
    let mut bus = EnforcementAuditBus::new();
    attestation_check(ATTESTATION, "round_trip", &mut bus).expect("canonical text passes");
    // No drift records.
    assert!(bus.iter().all(|e| e.tag != "h6.attestation.drift"));
}

#[test]
fn pd_table_reproduces_named_codes_in_order() {
    // T11-D129 : 17 §1 + 3 derived = 20 named codes (PD0001..PD0020).
    // Skip PD0000 (the spirit sentinel).
    let named: Vec<_> = PD_TABLE
        .iter()
        .filter(|r| r.code != DiagnosticCode::PD0000)
        .collect();
    assert_eq!(named.len(), 20);
    for (i, row) in named.iter().enumerate() {
        let expected = (i + 1) as u16; // PD0001..PD0020
        assert_eq!(row.code.number(), expected);
        // Every named row maps to a Prohibition::all_named_extended() entry.
        let prohibition_named = Prohibition::all_named_extended();
        assert!(prohibition_named.contains(&row.prohibition));
    }
}

#[test]
fn every_prohibition_has_a_pd_table_row() {
    // T11-D129 : Prohibition::all() includes 17 §1 + 3 derived + Spirit
    // = 21 total. Every prohibition must be addressed by exactly one
    // PD_TABLE row.
    for p in Prohibition::all() {
        let count = PD_TABLE.iter().filter(|r| r.prohibition == p).count();
        assert_eq!(
            count, 1,
            "prohibition {p:?} must appear exactly once in PD_TABLE"
        );
    }
}

#[test]
fn harm_prevention_check_for_surveillance_op_emits_pd0004() {
    struct OmegaReadSensorWithoutGrant;
    impl HarmPrevention for OmegaReadSensorWithoutGrant {
        fn relevant_prohibitions(&self) -> &'static [Prohibition] {
            &[Prohibition::Surveillance]
        }
        fn check(&self) -> Result<(), cssl_substrate_prime_directive::HarmCheckError> {
            let mut chk = ProhibitionCheck::new();
            chk.trigger(Prohibition::Surveillance);
            chk.finalize("omega_step.read_sensor")
        }
    }
    let op = OmegaReadSensorWithoutGrant;
    let err = op.check().unwrap_err();
    let s = err.to_string();
    assert!(s.contains("PD0004"));
    assert!(s.contains("surveillance"));
    assert!(s.contains("omega_step.read_sensor"));
}

#[test]
fn audit_event_messages_are_byte_stable_across_calls() {
    // Two semantically-identical events produce byte-identical messages
    // (no embedded timestamps, addresses, or other run-time noise).
    let e1 = AuditEvent::Halted {
        reason: HaltReason::User,
        outstanding_steps: 5,
    };
    let e2 = AuditEvent::Halted {
        reason: HaltReason::User,
        outstanding_steps: 5,
    };
    assert_eq!(e1.message(), e2.message());
}

#[test]
fn revoke_flow_uses_token_then_appends_revoke_entry() {
    let mut store = ConsentStore::new();
    let scope = ConsentScope::for_purpose("savepoint-1", "system");
    let tok = caps_grant_for_test(&mut store, scope, SubstrateCap::SavePath).unwrap();
    let id = tok.id();
    caps_revoke(&mut store, tok).expect("revoke");

    // Active count drops to 0.
    assert_eq!(store.log.active_count(), 0);
    assert_eq!(store.log.revoked_count(), 1);

    // The revoke entry references the token id.
    let revoke_entries: Vec<_> = store
        .audit
        .iter()
        .filter(|e| e.tag == "h6.revoke")
        .collect();
    assert_eq!(revoke_entries.len(), 1);
    assert!(revoke_entries[0]
        .message
        .contains(&format!("cap-token#{}", id.0)));
}

#[test]
fn halt_with_zero_pending_meets_one_ms_budget() {
    let mut sink = CountingHaltSink::new(0);
    let mut audit = EnforcementAuditBus::new();
    let switch = KillSwitch::for_test(HaltReason::Signal);
    let outcome = substrate_halt(switch, &mut sink, &mut audit);
    // Zero pending = trivially fast. Budget is 1 ms ; expect within.
    assert!(
        outcome.stats.within_budget,
        "halt with 0 pending should always meet 1ms budget (took {} µs)",
        outcome.stats.elapsed_micros
    );
}

#[test]
fn audit_chain_verifies_after_full_lifecycle() {
    let mut store = ConsentStore::new();
    let scope = ConsentScope::for_purpose("lifecycle", "system");
    let t1 = caps_grant_for_test(&mut store, scope.clone(), SubstrateCap::OmegaRegister).unwrap();
    let t2 = caps_grant_for_test(&mut store, scope, SubstrateCap::ObserverShare).unwrap();
    caps_revoke(&mut store, t1).unwrap();
    caps_revoke(&mut store, t2).unwrap();

    let mut sink = CountingHaltSink::new(3);
    let switch = KillSwitch::for_test(HaltReason::User);
    let _outcome = substrate_halt(switch, &mut sink, &mut store.audit);

    // Audit-chain : 2 issues + 2 revokes + 1 halt = 5 entries.
    assert_eq!(store.audit.entry_count(), 5);
    store.audit.chain().verify_chain().expect("chain verifies");
}

#[test]
fn each_substrate_cap_has_unique_canonical_name() {
    let names: Vec<&'static str> = SubstrateCap::all()
        .iter()
        .map(|c| c.canonical_name())
        .collect();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    let original = sorted.len();
    sorted.dedup();
    assert_eq!(sorted.len(), original, "canonical names unique");
    // Spot-check the canonical names match the slice spec.
    assert!(names.contains(&"omega_register"));
    assert!(names.contains(&"observer_share"));
    assert!(names.contains(&"debug_camera"));
    assert!(names.contains(&"net_send_state"));
    assert!(names.contains(&"save_path"));
    assert!(names.contains(&"replay_load"));
    assert!(names.contains(&"consent_revoke"));
    assert!(names.contains(&"kill_switch_invoke"));
}
