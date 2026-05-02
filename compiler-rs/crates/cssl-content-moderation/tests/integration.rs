//! § integration · cssl-content-moderation
//! ════════════════════════════════════════════════════════════════════════
//!
//! End-to-end coverage of PRIME-DIRECTIVE invariants :
//!   1. flag-submission cap-gated
//!   2. k-anon T1 floor (single-flag-private)
//!   3. k-anon T2 author-aggregate-after-3
//!   4. k-anon T3 needs-review (≥10 distinct + weight ≥75)
//!   5. T5 auto-restore-after-7-days-no-decision
//!   6. sovereign-revoke-during-review wins
//!   7. NO-shadowban attestation (5+ ways)
//!   8. appeal-roundtrip
//!   9. curator-decision Σ-Chain-anchored
//!  10. flagger-revoke-own-flag any-stage

use cssl_content_moderation::{
    aggregate::{
        K_AUTHOR_AGGREGATE_FLOOR, K_NEEDS_REVIEW_DISTINCT, K_NEEDS_REVIEW_WEIGHTED,
    },
    appeal::{Appeal, K_APPEAL_CURATOR_QUORUM, T_APPEAL_WINDOW_DAYS, T_AUTO_RESTORE_DAYS},
    cap::{
        CapClass, CapPolicy, MOD_CAP_AGGREGATE_READ, MOD_CAP_APPEAL, MOD_CAP_CHAIN_ANCHOR,
        MOD_CAP_CURATE_A, MOD_CAP_FLAG_SUBMIT,
    },
    decision::{CuratorDecision, DecisionKind},
    prime_directive_attestation,
    record::{FlagKind, FlagRecord},
    store::{ModerationStore, StoreError},
};

const SECONDS_PER_DAY: u32 = 86_400;

fn flagger_cap() -> CapPolicy {
    CapPolicy::new(MOD_CAP_FLAG_SUBMIT, 0)
}
fn curator_cap() -> CapPolicy {
    CapPolicy::new(MOD_CAP_CURATE_A | MOD_CAP_CHAIN_ANCHOR, 0)
}
fn author_read_cap() -> CapPolicy {
    CapPolicy::new(MOD_CAP_AGGREGATE_READ | MOD_CAP_APPEAL, 0)
}
fn mk_flag(handle: u64, content_id: u32, kind: FlagKind, severity: u8, ts: u32) -> FlagRecord {
    FlagRecord::pack(
        handle,
        content_id,
        kind,
        severity,
        MOD_CAP_FLAG_SUBMIT,
        ts,
        0,
        0,
    )
    .unwrap()
}

#[test]
fn t1_flag_submission_cap_gated() {
    let s = ModerationStore::new();
    // Caller WITHOUT MOD_CAP_FLAG_SUBMIT is denied.
    let nocap = CapPolicy::new(0, 0);
    let r = mk_flag(0xA, 1, FlagKind::Spam, 5, 100);
    let err = s.submit_flag(nocap, r, 100).unwrap_err();
    assert!(matches!(err, StoreError::CapDenied { .. }));
}

#[test]
fn t1_single_flag_private() {
    let s = ModerationStore::new();
    s.submit_flag(flagger_cap(), mk_flag(0xA, 2, FlagKind::Spam, 5, 100), 100).unwrap();
    let agg = s.aggregate(2, None, 101).unwrap();
    assert_eq!(agg.total_flags, 1);
    assert!(!agg.visible_to_author, "T1 single-flag is private to flagger+admin");
}

#[test]
fn t2_author_aggregate_after_three_flags() {
    let s = ModerationStore::new();
    for i in 1..=3u64 {
        s.submit_flag(flagger_cap(), mk_flag(i, 3, FlagKind::HarmTowardOthers, 50, 100), 100)
            .unwrap();
    }
    let agg = s.aggregate(3, Some(author_read_cap()), 101).unwrap();
    assert_eq!(agg.total_flags, K_AUTHOR_AGGREGATE_FLOOR);
    assert!(agg.visible_to_author, "T2 author sees aggregate at ≥ 3");
    assert_eq!(agg.distinct_flaggers, 3);
}

#[test]
fn t3_needs_review_requires_ten_distinct_and_weighted_75() {
    let s = ModerationStore::new();
    // 10 distinct flaggers each with severity 80 ⟶ weighted = 10 * (80/10) = 80 ≥ 75
    for i in 1..=10u64 {
        s.submit_flag(
            flagger_cap(),
            mk_flag(i, 4, FlagKind::PrimeDirectiveViolation, 80, 100),
            100,
        )
        .unwrap();
    }
    let agg = s.aggregate(4, None, 101).unwrap();
    assert!(agg.needs_review, "T3 ≥ 10 distinct + weighted ≥ 75 ⟶ needs-review");
    assert_eq!(agg.distinct_flaggers, K_NEEDS_REVIEW_DISTINCT);
    assert!(agg.severity_weighted >= K_NEEDS_REVIEW_WEIGHTED);
}

#[test]
fn t5_auto_restore_after_seven_days_no_decision() {
    let mut a = Appeal::file(
        1,
        5,
        0xA071,
        1_700_000_000,
        0,
        0,
        b"appeal - 7-day auto-restore expected",
        [0u8; 64],
    )
    .unwrap();
    let day7 = 1_700_000_000 + T_AUTO_RESTORE_DAYS * SECONDS_PER_DAY;
    assert!(a.auto_restore_eligible(day7));
    a.resolve(DecisionKind::AutoRestored, day7);
    assert_eq!(a.resolution_kind, DecisionKind::AutoRestored as u8);
}

#[test]
fn sovereign_revoke_during_review_wins() {
    let s = ModerationStore::new();
    // 5 flags filed mid-review.
    for i in 1..=5u64 {
        s.submit_flag(
            flagger_cap(),
            mk_flag(i, 6, FlagKind::HarmTowardOthers, 90, 100),
            100,
        )
        .unwrap();
    }
    // Curator opens a decision (FlagUpheld) — pending.
    let pending = CuratorDecision::new(
        0,
        6,
        0xC0DA,
        CapClass::CommunityElected,
        DecisionKind::FlagUpheld,
        200,
        b"upheld",
        [0u8; 64],
    )
    .unwrap();
    s.record_decision(curator_cap(), pending, 201).unwrap();
    // Author sovereign-revokes mid-review → wins UNCONDITIONALLY.
    let anchor = s.sovereign_revoke(6, 0xA071, 300).unwrap();
    assert!(s.is_sovereign_revoked(6));
    assert_ne!(anchor, [0u8; 32]);
    let history = s.decisions_for(6);
    // History contains both the pending FlagUpheld AND the SovereignRevoked.
    let kinds: Vec<DecisionKind> = history.iter().map(|d| d.kind).collect();
    assert!(kinds.contains(&DecisionKind::SovereignRevoked));
}

#[test]
fn no_shadowban_attestation_5_plus_ways() {
    // 1. Attestation string carries the canonical no-shadowban claim.
    let s = prime_directive_attestation();
    assert!(s.contains("NO-shadowban"));
    assert!(s.contains("NO-algo-suppression"));
    assert!(s.contains("author-transparent"));
    assert!(s.contains("Sigma-Chain-anchor"));
    assert!(s.contains("sovereign-revoke-wins"));
    // 2. visible_to_author flips at T2 floor (no algorithmic delay).
    let store = ModerationStore::new();
    for i in 1..=3u64 {
        store
            .submit_flag(flagger_cap(), mk_flag(i, 7, FlagKind::Spam, 10, 100), 100)
            .unwrap();
    }
    let agg = store.aggregate(7, None, 101).unwrap();
    assert!(agg.visible_to_author);
    // 3. needs-review is a transparent flag — NOT a hidden-state.
    for i in 4..=12u64 {
        store
            .submit_flag(
                flagger_cap(),
                mk_flag(i, 7, FlagKind::PrimeDirectiveViolation, 80, 100),
                100,
            )
            .unwrap();
    }
    let agg2 = store.aggregate(7, None, 102).unwrap();
    assert!(agg2.needs_review, "needs-review is publicly observable");
    // 4. ALL DecisionKinds require Σ-Chain-anchor (no silent-action).
    for kind in [
        DecisionKind::FlagDismissed,
        DecisionKind::FlagUpheld,
        DecisionKind::ContentRestricted,
        DecisionKind::ContentRemoved,
        DecisionKind::AppealAccepted,
        DecisionKind::AppealRejected,
        DecisionKind::SovereignRevoked,
        DecisionKind::AutoRestored,
    ] {
        assert!(kind.requires_chain_anchor());
    }
    // 5. Per-kind histogram visible to author (no hidden categories).
    let counts = agg2.per_kind_counts;
    assert_eq!(counts.iter().sum::<u32>(), agg2.total_flags);
}

#[test]
fn appeal_roundtrip() {
    let s = ModerationStore::new();
    // Curator removes content.
    let removal = CuratorDecision::new(
        0,
        8,
        0xC0DA,
        CapClass::CommunityElected,
        DecisionKind::ContentRemoved,
        1_700_000_100,
        b"removed",
        [0u8; 64],
    )
    .unwrap();
    let decision_id = s.record_decision(curator_cap(), removal, 1_700_000_101).unwrap();
    // Author appeals within 30-day window.
    let mut appeal = Appeal::file(
        0,
        8,
        0xA071,
        1_700_000_200,
        decision_id,
        1_700_000_100,
        b"this content is parody - please review",
        [0u8; 64],
    )
    .unwrap();
    let appeal_id = s.file_appeal(appeal.clone()).unwrap();
    assert!(appeal_id > 0);
    // Three curators review ⟶ quorum reached.
    appeal.mark_quorum(K_APPEAL_CURATOR_QUORUM);
    assert!(appeal.curator_quorum_reached);
    // Decision: AppealAccepted (curator-cap REQUIRED + Σ-Chain anchor).
    let decision = CuratorDecision::new(
        0,
        8,
        0xC0DA,
        CapClass::CommunityElected,
        DecisionKind::AppealAccepted,
        1_700_000_300,
        b"upheld appeal - restored",
        [0u8; 64],
    )
    .unwrap();
    let id2 = s.record_decision(curator_cap(), decision, 1_700_000_301).unwrap();
    assert!(id2 > decision_id);
    let history = s.decisions_for(8);
    assert_eq!(history.len(), 2);
    assert!(history.iter().all(|d| d.verify_anchor()));
}

#[test]
fn appeal_outside_window_rejected() {
    let too_late = 1_700_000_000 + (T_APPEAL_WINDOW_DAYS + 1) * SECONDS_PER_DAY;
    let err = Appeal::file(
        0,
        9,
        0xA071,
        too_late,
        5,
        1_700_000_000,
        b"too late",
        [0u8; 64],
    )
    .unwrap_err();
    use cssl_content_moderation::AppealError;
    assert!(matches!(err, AppealError::AppealWindowExpired { .. }));
}

#[test]
fn curator_decision_sigma_chain_anchored() {
    let d = CuratorDecision::new(
        42,
        10,
        0xC1,
        CapClass::SubstrateAppointed,
        DecisionKind::ContentRestricted,
        1_700_000_500,
        b"locale-restricted",
        [0u8; 64],
    )
    .unwrap();
    assert!(d.verify_anchor());
    assert_ne!(d.sigma_chain_anchor, [0u8; 32]);
    // Anchor depends on inputs — change → anchor diverges.
    let d2 = CuratorDecision::new(
        42,
        10,
        0xC1,
        CapClass::SubstrateAppointed,
        DecisionKind::ContentRemoved, // changed
        1_700_000_500,
        b"locale-restricted",
        [0u8; 64],
    )
    .unwrap();
    assert_ne!(d.sigma_chain_anchor, d2.sigma_chain_anchor);
}

#[test]
fn flagger_revoke_own_flag_at_any_stage() {
    let s = ModerationStore::new();
    s.submit_flag(flagger_cap(), mk_flag(0xAAA, 11, FlagKind::Spam, 30, 100), 100).unwrap();
    s.submit_flag(flagger_cap(), mk_flag(0xBBB, 11, FlagKind::Spam, 30, 101), 101).unwrap();
    assert_eq!(s.flag_count(11), 2);
    // Curator opens a decision (mid-review).
    let pending = CuratorDecision::new(
        0,
        11,
        0xC,
        CapClass::CommunityElected,
        DecisionKind::FlagUpheld,
        200,
        b"reviewing",
        [0u8; 64],
    )
    .unwrap();
    s.record_decision(curator_cap(), pending, 201).unwrap();
    // Flagger revokes own-flag mid-review.
    let removed = s.revoke_own_flag(11, 0xAAA).unwrap();
    assert_eq!(removed, 1);
    assert_eq!(s.flag_count(11), 1);
}

#[test]
fn aggregate_below_floor_without_author_cap_still_works_for_admin() {
    let s = ModerationStore::new();
    s.submit_flag(flagger_cap(), mk_flag(0xA, 12, FlagKind::Spam, 5, 100), 100).unwrap();
    // No cap supplied (admin path) → returns aggregate without checking
    // visibility-bit (admin sees all).
    let agg = s.aggregate(12, None, 101).unwrap();
    assert_eq!(agg.total_flags, 1);
    assert!(!agg.visible_to_author, "T1 floor : invisible to author");
}

#[test]
fn determinism_pack_unpack_replay_stable() {
    // Pack → raw → from_raw_validated → fields-match.
    let r1 = FlagRecord::pack(
        0xDEAD_BEEF,
        99,
        FlagKind::AttributionFraud,
        77,
        MOD_CAP_FLAG_SUBMIT,
        1_700_000_000,
        0xCAFE,
        0xBABE,
    )
    .unwrap();
    let r2 = FlagRecord::from_raw_validated(r1.raw).unwrap();
    assert_eq!(r1, r2);
    assert_eq!(r2.flag_kind(), FlagKind::AttributionFraud);
    assert_eq!(r2.severity(), 77);
}
