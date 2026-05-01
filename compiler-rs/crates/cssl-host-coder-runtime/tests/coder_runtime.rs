// tests/coder_runtime.rs — ≥ 32 tests covering every acceptance-row
// ══════════════════════════════════════════════════════════════════
// § HardCap : substrate-* (3) · specs/grand-vision/00..15 (3) · TIER-C-secret (3) · happy-path (3)
// § Rate-limit : 10 ok · 11th rejected (3)
// § Sandbox : staging-only-no-file-touch (3)
// § Validation : pass + fail (2)
// § Approval : approved · denied · timed-out (3)
// § Apply : after-approval only (2)
// § Revert : within-30s · after-window blocked (3)
// § Sovereign-cap : without-cap denied · with-cap allowed (2)
// § Audit-emit : every-state-transition emits (2)
// § Serde round-trip (1)
// total = 32
// ══════════════════════════════════════════════════════════════════

use cssl_host_coder_runtime::approval::{MockApprovalHandler, PromptOutcome};
use cssl_host_coder_runtime::audit::{AuditEvent, AuditLog, InMemoryAuditLog};
use cssl_host_coder_runtime::cap::{CoderCap, SovereignBit};
use cssl_host_coder_runtime::edit::{CoderEditId, EditKind, EditState, StagedEdit};
use cssl_host_coder_runtime::hard_cap::{HardCapDecision, HardCapPolicy};
use cssl_host_coder_runtime::revert::{RevertOutcome, RevertWindow};
use cssl_host_coder_runtime::sandbox::SandboxApplyError;
use cssl_host_coder_runtime::validation::ValidationOutcome;
use cssl_host_coder_runtime::CoderRuntime;

// ─── helpers ──────────────────────────────────────────────────────

fn fresh_runtime(
    approval: MockApprovalHandler,
) -> CoderRuntime<MockApprovalHandler, InMemoryAuditLog> {
    CoderRuntime::new(HardCapPolicy::default(), approval, InMemoryAuditLog::new())
}

fn sample_blake3(seed: u8) -> [u8; 32] {
    let mut out = [0u8; 32];
    out[0] = seed;
    out
}

fn pubkey(seed: u8) -> [u8; 32] {
    let mut out = [0u8; 32];
    out[31] = seed;
    out
}

#[allow(clippy::too_many_arguments)]
fn submit(
    rt: &mut CoderRuntime<MockApprovalHandler, InMemoryAuditLog>,
    kind: EditKind,
    path: &str,
    sov: SovereignBit,
    caps: CoderCap,
    now_ms: u64,
    player_seed: u8,
    diff_seed: u8,
) -> Result<CoderEditId, HardCapDecision> {
    rt.submit_edit(
        kind,
        path.to_string(),
        sample_blake3(diff_seed),
        sample_blake3(diff_seed.wrapping_add(1)),
        format!("test diff {diff_seed}"),
        now_ms,
        pubkey(player_seed),
        sov,
        caps,
    )
}

// ═══ HardCap : substrate-* (3) ════════════════════════════════════

#[test]
fn hardcap_substrate_omega_field_rejected() {
    let mut rt = fresh_runtime(MockApprovalHandler::default());
    let r = submit(
        &mut rt,
        EditKind::CosmeticTweak,
        "compiler-rs/crates/cssl-substrate-omega-field/src/lib.rs",
        SovereignBit::Held,
        CoderCap::AST_EDIT,
        100,
        1,
        7,
    );
    assert_eq!(r, Err(HardCapDecision::DenySubstrateEdit));
}

#[test]
fn hardcap_substrate_sigma_mask_rejected_windows_path() {
    let mut rt = fresh_runtime(MockApprovalHandler::default());
    let r = submit(
        &mut rt,
        EditKind::CosmeticTweak,
        "compiler-rs\\crates\\cssl-substrate-sigma-mask\\src\\foo.rs",
        SovereignBit::Held,
        CoderCap::AST_EDIT,
        100,
        1,
        8,
    );
    assert_eq!(r, Err(HardCapDecision::DenySubstrateEdit));
}

#[test]
fn hardcap_substrate_emit_audit_event() {
    let mut rt = fresh_runtime(MockApprovalHandler::default());
    let _ = submit(
        &mut rt,
        EditKind::CosmeticTweak,
        "compiler-rs/crates/cssl-substrate-kan-runtime/src/lib.rs",
        SovereignBit::Held,
        CoderCap::AST_EDIT,
        100,
        1,
        9,
    );
    let snap = rt.audit_log().snapshot();
    assert!(matches!(
        snap.first(),
        Some(AuditEvent::HardCapRejected {
            decision: HardCapDecision::DenySubstrateEdit,
            ..
        })
    ));
}

// ═══ HardCap : specs/grand-vision/00..15 (3) ══════════════════════

#[test]
fn hardcap_spec_gv_00_rejected() {
    let mut rt = fresh_runtime(MockApprovalHandler::default());
    let r = submit(
        &mut rt,
        EditKind::CosmeticTweak,
        "specs/grand-vision/00_OVERVIEW.csl",
        SovereignBit::Held,
        CoderCap::AST_EDIT,
        100,
        1,
        10,
    );
    assert_eq!(r, Err(HardCapDecision::DenySpecGrandVision00to15));
}

#[test]
fn hardcap_spec_gv_15_rejected() {
    let mut rt = fresh_runtime(MockApprovalHandler::default());
    let r = submit(
        &mut rt,
        EditKind::CosmeticTweak,
        "specs/grand-vision/15_UNIFIED_SUBSTRATE.csl",
        SovereignBit::Held,
        CoderCap::AST_EDIT,
        100,
        1,
        11,
    );
    assert_eq!(r, Err(HardCapDecision::DenySpecGrandVision00to15));
}

#[test]
fn hardcap_spec_gv_16_allowed_passes_path_check() {
    let mut rt = fresh_runtime(MockApprovalHandler::default());
    let r = submit(
        &mut rt,
        EditKind::CosmeticTweak,
        "specs/grand-vision/16_MYCELIAL_NETWORK.csl",
        SovereignBit::Held,
        CoderCap::AST_EDIT,
        100,
        1,
        12,
    );
    // path-check should NOT reject ; cosmetic-tweak is soft-cap so this returns Ok.
    assert!(r.is_ok());
}

// ═══ HardCap : TIER-C-secret (3) ══════════════════════════════════

#[test]
fn hardcap_dotenv_rejected() {
    let mut rt = fresh_runtime(MockApprovalHandler::default());
    let r = submit(
        &mut rt,
        EditKind::CosmeticTweak,
        ".env",
        SovereignBit::Held,
        CoderCap::AST_EDIT,
        100,
        1,
        13,
    );
    assert_eq!(r, Err(HardCapDecision::DenyTierCSecret));
}

#[test]
fn hardcap_loa_secrets_rejected() {
    let mut rt = fresh_runtime(MockApprovalHandler::default());
    let r = submit(
        &mut rt,
        EditKind::CosmeticTweak,
        "compiler-rs/.loa-secrets/keys.json",
        SovereignBit::Held,
        CoderCap::AST_EDIT,
        100,
        1,
        14,
    );
    assert_eq!(r, Err(HardCapDecision::DenyTierCSecret));
}

#[test]
fn hardcap_supabase_credentials_rejected() {
    let mut rt = fresh_runtime(MockApprovalHandler::default());
    let r = submit(
        &mut rt,
        EditKind::CosmeticTweak,
        "compiler-rs/crates/cssl-supabase/credentials.toml",
        SovereignBit::Held,
        CoderCap::AST_EDIT,
        100,
        1,
        15,
    );
    assert_eq!(r, Err(HardCapDecision::DenyTierCSecret));
}

// ═══ HardCap : happy-path soft-cap allowed (3) ═════════════════════

#[test]
fn happy_path_content_scene_allowed() {
    let mut rt = fresh_runtime(MockApprovalHandler::default());
    let r = submit(
        &mut rt,
        EditKind::CosmeticTweak,
        "content/scenes/test_room.csl",
        SovereignBit::NotHeld,
        CoderCap::AST_EDIT,
        100,
        1,
        16,
    );
    assert!(r.is_ok());
}

#[test]
fn happy_path_balance_constant_no_sovereign_needed() {
    let mut rt = fresh_runtime(MockApprovalHandler::default());
    let r = submit(
        &mut rt,
        EditKind::BalanceConstantTune,
        "content/balance/damage_table.csl",
        SovereignBit::NotHeld,
        CoderCap::AST_EDIT,
        100,
        1,
        17,
    );
    assert!(r.is_ok());
}

#[test]
fn happy_path_returns_monotonic_id() {
    let mut rt = fresh_runtime(MockApprovalHandler::default());
    let id1 = submit(
        &mut rt,
        EditKind::CosmeticTweak,
        "content/a.csl",
        SovereignBit::NotHeld,
        CoderCap::AST_EDIT,
        100,
        1,
        20,
    )
    .unwrap();
    let id2 = submit(
        &mut rt,
        EditKind::CosmeticTweak,
        "content/b.csl",
        SovereignBit::NotHeld,
        CoderCap::AST_EDIT,
        101,
        1,
        21,
    )
    .unwrap();
    assert!(id2.0 > id1.0);
}

// ═══ Rate-limit (3) ════════════════════════════════════════════════

#[test]
fn rate_limit_10_edits_allowed() {
    let mut rt = fresh_runtime(MockApprovalHandler::default());
    for i in 0..10u8 {
        let r = submit(
            &mut rt,
            EditKind::CosmeticTweak,
            "content/foo.csl",
            SovereignBit::NotHeld,
            CoderCap::AST_EDIT,
            (i as u64) * 1000,
            1,
            i,
        );
        assert!(r.is_ok(), "edit {i} should be allowed");
    }
}

#[test]
fn rate_limit_11th_edit_rejected() {
    let mut rt = fresh_runtime(MockApprovalHandler::default());
    for i in 0..10u8 {
        let _ = submit(
            &mut rt,
            EditKind::CosmeticTweak,
            "content/foo.csl",
            SovereignBit::NotHeld,
            CoderCap::AST_EDIT,
            (i as u64) * 1000,
            1,
            i,
        )
        .unwrap();
    }
    let r = submit(
        &mut rt,
        EditKind::CosmeticTweak,
        "content/foo.csl",
        SovereignBit::NotHeld,
        CoderCap::AST_EDIT,
        10_500,
        1,
        99,
    );
    assert_eq!(r, Err(HardCapDecision::DenyRateLimit));
}

#[test]
fn rate_limit_independent_per_player() {
    let mut rt = fresh_runtime(MockApprovalHandler::default());
    for i in 0..10u8 {
        let _ = submit(
            &mut rt,
            EditKind::CosmeticTweak,
            "content/foo.csl",
            SovereignBit::NotHeld,
            CoderCap::AST_EDIT,
            (i as u64) * 1000,
            1,
            i,
        )
        .unwrap();
    }
    // different player should be unaffected
    let r = submit(
        &mut rt,
        EditKind::CosmeticTweak,
        "content/foo.csl",
        SovereignBit::NotHeld,
        CoderCap::AST_EDIT,
        10_500,
        2,
        50,
    );
    assert!(r.is_ok());
}

// ═══ Sandbox : staging-only-no-file-touch (3) ═════════════════════

#[test]
fn sandbox_submit_does_not_touch_filesystem() {
    let mut rt = fresh_runtime(MockApprovalHandler::default());
    let id = submit(
        &mut rt,
        EditKind::CosmeticTweak,
        "/path/that/definitely/does/not/exist/foo.csl",
        SovereignBit::NotHeld,
        CoderCap::AST_EDIT,
        100,
        1,
        30,
    )
    .unwrap();
    // edit lives in sandbox, real file untouched (we never opened it).
    assert!(rt.sandbox().get(id).is_some());
    assert!(!std::path::Path::new("/path/that/definitely/does/not/exist/foo.csl").exists());
}

#[test]
fn sandbox_holds_edit_in_staged_state() {
    let mut rt = fresh_runtime(MockApprovalHandler::default());
    let id = submit(
        &mut rt,
        EditKind::CosmeticTweak,
        "content/x.csl",
        SovereignBit::NotHeld,
        CoderCap::AST_EDIT,
        100,
        1,
        31,
    )
    .unwrap();
    assert_eq!(rt.sandbox().get(id).unwrap().state, EditState::Staged);
}

#[test]
fn sandbox_iterates_in_id_order() {
    let mut rt = fresh_runtime(MockApprovalHandler::default());
    let id1 = submit(
        &mut rt,
        EditKind::CosmeticTweak,
        "content/a.csl",
        SovereignBit::NotHeld,
        CoderCap::AST_EDIT,
        100,
        1,
        40,
    )
    .unwrap();
    let id2 = submit(
        &mut rt,
        EditKind::CosmeticTweak,
        "content/b.csl",
        SovereignBit::NotHeld,
        CoderCap::AST_EDIT,
        101,
        1,
        41,
    )
    .unwrap();
    let collected: Vec<CoderEditId> = rt.sandbox().iter().map(|(k, _)| *k).collect();
    assert_eq!(collected, vec![id1, id2]);
}

// ═══ Validation : pass + fail (2) ══════════════════════════════════

#[test]
fn validation_passes_for_real_diff() {
    let mut rt = fresh_runtime(MockApprovalHandler::default());
    let id = submit(
        &mut rt,
        EditKind::CosmeticTweak,
        "content/x.csl",
        SovereignBit::NotHeld,
        CoderCap::AST_EDIT,
        100,
        1,
        50,
    )
    .unwrap();
    let outcome = rt.validate(id, 200);
    assert!(matches!(outcome, ValidationOutcome::Pass(_)));
    assert_eq!(rt.sandbox().get(id).unwrap().state, EditState::ValidationPassed);
}

#[test]
fn validation_fails_for_noop_edit() {
    let mut rt = fresh_runtime(MockApprovalHandler::default());
    // before == after blake3 → no-op edit
    let id = rt
        .submit_edit(
            EditKind::CosmeticTweak,
            "content/x.csl".to_string(),
            sample_blake3(7),
            sample_blake3(7),
            "noop".to_string(),
            100,
            pubkey(1),
            SovereignBit::NotHeld,
            CoderCap::AST_EDIT,
        )
        .unwrap();
    let outcome = rt.validate(id, 200);
    assert!(matches!(outcome, ValidationOutcome::Fail(_)));
    assert_eq!(rt.sandbox().get(id).unwrap().state, EditState::Rejected);
}

// ═══ Approval : approved · denied · timed-out (3) ══════════════════

#[test]
fn approval_approved_transitions_to_approved_state() {
    let approval = MockApprovalHandler::with_script(vec![PromptOutcome::Approved]);
    let mut rt = fresh_runtime(approval);
    let id = submit(
        &mut rt,
        EditKind::CosmeticTweak,
        "content/x.csl",
        SovereignBit::NotHeld,
        CoderCap::AST_EDIT,
        100,
        1,
        60,
    )
    .unwrap();
    rt.validate(id, 200);
    let outcome = rt.request_approval(id, 300);
    assert_eq!(outcome, PromptOutcome::Approved);
    assert_eq!(rt.sandbox().get(id).unwrap().state, EditState::Approved);
}

#[test]
fn approval_denied_transitions_to_rejected() {
    let approval = MockApprovalHandler::with_script(vec![PromptOutcome::Denied]);
    let mut rt = fresh_runtime(approval);
    let id = submit(
        &mut rt,
        EditKind::CosmeticTweak,
        "content/x.csl",
        SovereignBit::NotHeld,
        CoderCap::AST_EDIT,
        100,
        1,
        61,
    )
    .unwrap();
    rt.validate(id, 200);
    let outcome = rt.request_approval(id, 300);
    assert_eq!(outcome, PromptOutcome::Denied);
    assert_eq!(rt.sandbox().get(id).unwrap().state, EditState::Rejected);
}

#[test]
fn approval_timeout_treated_as_rejected_failsafe() {
    let approval = MockApprovalHandler::with_script(vec![PromptOutcome::TimedOut]);
    let mut rt = fresh_runtime(approval);
    let id = submit(
        &mut rt,
        EditKind::CosmeticTweak,
        "content/x.csl",
        SovereignBit::NotHeld,
        CoderCap::AST_EDIT,
        100,
        1,
        62,
    )
    .unwrap();
    rt.validate(id, 200);
    let outcome = rt.request_approval(id, 300);
    assert_eq!(outcome, PromptOutcome::TimedOut);
    // fail-safe : timeout → Rejected
    assert_eq!(rt.sandbox().get(id).unwrap().state, EditState::Rejected);
}

// ═══ Apply : after-approval only (2) ═══════════════════════════════

#[test]
fn apply_after_approved_writes_via_writer_and_arms_revert_window() {
    let approval = MockApprovalHandler::with_script(vec![PromptOutcome::Approved]);
    let mut rt = fresh_runtime(approval);
    let id = submit(
        &mut rt,
        EditKind::CosmeticTweak,
        "content/x.csl",
        SovereignBit::NotHeld,
        CoderCap::AST_EDIT,
        100,
        1,
        70,
    )
    .unwrap();
    rt.validate(id, 200);
    rt.request_approval(id, 300);
    let writer_called = std::cell::RefCell::new(0);
    let r = rt.apply(id, 1_000, |_e: &StagedEdit| {
        *writer_called.borrow_mut() += 1;
        Ok(())
    });
    assert!(r.is_ok());
    assert_eq!(*writer_called.borrow(), 1);
    assert_eq!(rt.sandbox().get(id).unwrap().state, EditState::Applied);
    assert!(rt.has_active_revert_window(id));
}

#[test]
fn apply_before_approval_returns_not_approved() {
    let mut rt = fresh_runtime(MockApprovalHandler::default());
    let id = submit(
        &mut rt,
        EditKind::CosmeticTweak,
        "content/x.csl",
        SovereignBit::NotHeld,
        CoderCap::AST_EDIT,
        100,
        1,
        71,
    )
    .unwrap();
    rt.validate(id, 200);
    // skip approval entirely — should reject
    let r = rt.apply(id, 300, |_| Ok(()));
    assert!(matches!(r, Err(SandboxApplyError::NotApproved(_))));
}

// ═══ Revert : within-30s · after-window blocked (3) ════════════════

#[test]
fn revert_within_window_succeeds() {
    let approval = MockApprovalHandler::with_script(vec![PromptOutcome::Approved]);
    let mut rt = fresh_runtime(approval);
    let id = submit(
        &mut rt,
        EditKind::CosmeticTweak,
        "content/x.csl",
        SovereignBit::NotHeld,
        CoderCap::AST_EDIT,
        100,
        1,
        80,
    )
    .unwrap();
    rt.validate(id, 200);
    rt.request_approval(id, 300);
    let _ = rt.apply(id, 1_000, |_| Ok(())).unwrap();
    // 15 seconds later — well within 30s window
    let outcome = rt.manual_revert(id, 16_000);
    assert_eq!(outcome, RevertOutcome::Reverted);
    assert_eq!(rt.sandbox().get(id).unwrap().state, EditState::ManualReverted);
}

#[test]
fn revert_after_window_blocked() {
    let approval = MockApprovalHandler::with_script(vec![PromptOutcome::Approved]);
    let mut rt = fresh_runtime(approval);
    let id = submit(
        &mut rt,
        EditKind::CosmeticTweak,
        "content/x.csl",
        SovereignBit::NotHeld,
        CoderCap::AST_EDIT,
        100,
        1,
        81,
    )
    .unwrap();
    rt.validate(id, 200);
    rt.request_approval(id, 300);
    let _ = rt.apply(id, 1_000, |_| Ok(())).unwrap();
    // 60 seconds later — past 30s window (deadline = 1_000 + 30_000 = 31_000)
    let outcome = rt.manual_revert(id, 60_000);
    assert_eq!(outcome, RevertOutcome::WindowExpired);
    // state remains Applied (i.e. Permanent)
    assert_eq!(rt.sandbox().get(id).unwrap().state, EditState::Applied);
}

#[test]
fn auto_revert_within_window_succeeds() {
    let approval = MockApprovalHandler::with_script(vec![PromptOutcome::Approved]);
    let mut rt = fresh_runtime(approval);
    let id = submit(
        &mut rt,
        EditKind::CosmeticTweak,
        "content/x.csl",
        SovereignBit::NotHeld,
        CoderCap::AST_EDIT,
        100,
        1,
        82,
    )
    .unwrap();
    rt.validate(id, 200);
    rt.request_approval(id, 300);
    let _ = rt.apply(id, 1_000, |_| Ok(())).unwrap();
    let outcome = rt.auto_revert(id, 5_000);
    assert_eq!(outcome, RevertOutcome::Reverted);
    assert_eq!(rt.sandbox().get(id).unwrap().state, EditState::AutoReverted);
}

// ═══ Sovereign-cap (2) ═════════════════════════════════════════════

#[test]
fn sovereign_required_for_ast_node_replace_without_bit() {
    let mut rt = fresh_runtime(MockApprovalHandler::default());
    let r = submit(
        &mut rt,
        EditKind::AstNodeReplace,
        "content/x.csl",
        SovereignBit::NotHeld,
        CoderCap::AST_EDIT,
        100,
        1,
        90,
    );
    assert_eq!(r, Err(HardCapDecision::DenySovereignRequired));
}

#[test]
fn sovereign_held_allows_ast_node_replace() {
    let mut rt = fresh_runtime(MockApprovalHandler::default());
    let r = submit(
        &mut rt,
        EditKind::AstNodeReplace,
        "content/x.csl",
        SovereignBit::Held,
        CoderCap::AST_EDIT,
        100,
        1,
        91,
    );
    assert!(r.is_ok());
}

// ═══ Audit-emit : every state-transition emits (2) ═════════════════

#[test]
fn audit_emits_on_full_lifecycle() {
    let approval = MockApprovalHandler::with_script(vec![PromptOutcome::Approved]);
    let mut rt = fresh_runtime(approval);
    let id = submit(
        &mut rt,
        EditKind::CosmeticTweak,
        "content/x.csl",
        SovereignBit::NotHeld,
        CoderCap::AST_EDIT,
        100,
        1,
        100,
    )
    .unwrap();
    rt.validate(id, 200);
    rt.request_approval(id, 300);
    let _ = rt.apply(id, 1_000, |_| Ok(())).unwrap();
    let snap = rt.audit_log().snapshot();
    // Submit (Draft→Staged), validate (Staged→ValidationPending, then →Passed),
    // approve (Passed→Pending, then →Approved), apply (Approved→Applied) = 6 events
    assert!(snap.len() >= 6);
    // every event should be a state-transition (no rejects in this happy-path)
    for e in &snap {
        assert!(matches!(e, AuditEvent::StateTransition { .. }));
    }
}

#[test]
fn audit_emits_on_hard_cap_rejection() {
    let mut rt = fresh_runtime(MockApprovalHandler::default());
    let _ = submit(
        &mut rt,
        EditKind::CosmeticTweak,
        ".env",
        SovereignBit::Held,
        CoderCap::AST_EDIT,
        100,
        1,
        110,
    );
    let snap = rt.audit_log().snapshot();
    assert_eq!(snap.len(), 1);
    assert!(matches!(
        &snap[0],
        AuditEvent::HardCapRejected { decision: HardCapDecision::DenyTierCSecret, .. }
    ));
    assert_eq!(snap[0].directive_axis(), "ImplementationTransparency");
}

// ═══ Serde round-trip (1) ══════════════════════════════════════════

#[test]
fn serde_roundtrip_staged_edit() {
    let original = StagedEdit {
        id: CoderEditId(42),
        kind: EditKind::BalanceConstantTune,
        target_file: "content/balance.csl".to_string(),
        before_blake3: sample_blake3(1),
        after_blake3: sample_blake3(2),
        diff_summary: "tune sword damage 10→11".to_string(),
        staged_at_ms: 12345,
        staged_by_player_pubkey: pubkey(7),
        state: EditState::Staged,
    };
    let json = serde_json::to_string(&original).expect("serialize");
    let back: StagedEdit = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(original, back);
}

// ═══ Bonus : revert window math ═══════════════════════════════════

#[test]
fn revert_window_deadline_math() {
    let w = RevertWindow::arm(1_000, 30_000);
    assert_eq!(w.deadline_ms(), 31_000);
    assert!(w.is_open_at(1_000));
    assert!(w.is_open_at(31_000)); // boundary inclusive
    assert!(!w.is_open_at(31_001));
}

#[test]
fn cap_bitset_contains_semantics() {
    let combined = CoderCap::AST_EDIT.union(CoderCap::HOT_RELOAD);
    assert!(combined.contains(CoderCap::AST_EDIT));
    assert!(combined.contains(CoderCap::HOT_RELOAD));
    assert!(!combined.contains(CoderCap::SCHEMA_EVOLVE));
}

#[test]
fn no_cap_at_all_rejected() {
    let mut rt = fresh_runtime(MockApprovalHandler::default());
    let r = submit(
        &mut rt,
        EditKind::CosmeticTweak,
        "content/x.csl",
        SovereignBit::NotHeld,
        CoderCap::NONE,
        100,
        1,
        120,
    );
    assert!(r.is_err());
}
