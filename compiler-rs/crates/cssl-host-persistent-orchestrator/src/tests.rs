// § tests.rs — inline test suite (T11-W14 ≥10 tests covering all bullets).
//
// § coverage map
//   1. cycle_cadence_respected         — schedule mark/pick round-trip
//   2. cap_deny_blocks_mutate          — default-deny matrix returns CapDenied
//   3. cap_grant_unlocks_cycle         — flipping the matrix lets work flow
//   4. idle_mode_detection             — IdleDetector + ActivityHint
//   5. throttle_on_busy                — ThrottlePolicy decisions
//   6. sigma_chain_anchor_cadence      — KAN-threshold mints fresh anchor
//   7. journal_replay_roundtrip        — to_ndjson → JournalReplay::replay
//   8. sovereign_pause_resume          — pause/resume flips correctly
//   9. cross_cycle_determinism         — same hint-stream → same anchors
//  10. anchor_chain_monotonic          — folding never resets seq backwards
//  11. attestation_bootstrap_recorded  — first journal entry is bootstrap
//  12. quiescent_tick_recorded         — empty tick still journals
//  13. cap_revoke_re_locks             — Apocky can revoke mid-run

use crate::{
    AnchorChain, ActivityHint, BusyHint, CapKind, CycleKind, CycleOutcome,
    JournalKind, JournalReplay, JournalStore, NoopDriver, OrchestratorConfig,
    PersistentOrchestrator, SovereignCapMatrix, ThrottleDecision, ThrottlePolicy,
};
use crate::config::{
    KAN_TICK_CADENCE_MS, MYCELIUM_SYNC_CADENCE_MS, PLAYTEST_CADENCE_MS,
    SELF_AUTHOR_CADENCE_MS,
};

fn small_cfg() -> OrchestratorConfig {
    let mut cfg = OrchestratorConfig::default();
    // Make Σ-Chain anchor cadence small so tests don't need 1024 ticks to fire.
    cfg.anchor_every_n_kan_updates = 4;
    // Allow up to 5 cycles per tick so a single tick can advance the schedule.
    cfg.max_cycles_per_tick = 5;
    cfg
}

/// BusyHint that satisfies the IdleDeepProcgen throttle-gate (sustained idle).
/// Use in tests that want all five cycles to fire on a single tick.
fn idle_hint() -> BusyHint {
    BusyHint {
        cpu_pct: 0,
        last_input_age_ms: u64::MAX / 2, // far past the 5-min idle threshold
        apocky_actively_working: false,
        force_bypass: false,
    }
}

fn grant_all() -> SovereignCapMatrix {
    SovereignCapMatrix::grant_all()
}

#[test]
fn t1_cycle_cadence_respected() {
    let cfg = small_cfg();
    let mut orch = PersistentOrchestrator::new(cfg.clone(), grant_all());
    // Tick 0 : every cycle is due → all five run when the idle-hint is satisfied.
    let r0 = orch.tick_with_hints(0, idle_hint(), ActivityHint::default());
    assert_eq!(
        r0.cycles_executed, 5,
        "all five cycles should run on first tick (with idle-hint) : got {r0:?}",
    );
    // Tick 1ms later : nothing new is due.
    let r1 = orch.tick_with_hints(1, idle_hint(), ActivityHint::default());
    assert_eq!(r1.cycles_executed, 0, "no cycle should be due 1ms later");
    // Tick at the mycelium-sync cadence : only mycelium-sync should re-run.
    let r2 = orch.tick_with_hints(MYCELIUM_SYNC_CADENCE_MS, idle_hint(), ActivityHint::default());
    assert_eq!(r2.cycles_executed, 1, "only mycelium-sync should re-run");
    let kind = r2.last_outcomes.iter().find(|o| o.is_success()).unwrap().kind();
    assert_eq!(kind, CycleKind::MyceliumSync);
    // At KAN-tick cadence, mycelium AND kan should both be due.
    let r3 = orch.tick_with_hints(KAN_TICK_CADENCE_MS, idle_hint(), ActivityHint::default());
    assert!(r3.cycles_executed >= 1);
    // At self-author cadence, all the high-cadence cycles + self-author run.
    let r4 = orch.tick_with_hints(SELF_AUTHOR_CADENCE_MS, idle_hint(), ActivityHint::default());
    assert!(r4.cycles_executed >= 2);
    // Sanity : playtest also gets a turn at its own cadence boundary.
    let r5 = orch.tick_with_hints(
        SELF_AUTHOR_CADENCE_MS + PLAYTEST_CADENCE_MS,
        idle_hint(),
        ActivityHint::default(),
    );
    let saw_playtest = r5
        .last_outcomes
        .iter()
        .any(|o| o.is_success() && o.kind() == CycleKind::Playtest);
    assert!(saw_playtest, "playtest should run at playtest-cadence : {r5:?}");
}

#[test]
fn t2_cap_deny_blocks_mutate() {
    let cfg = small_cfg();
    let caps = SovereignCapMatrix::default_deny(); // PRIME-DIRECTIVE default
    let mut orch = PersistentOrchestrator::new(cfg, caps);
    let r = orch.tick(0);
    assert_eq!(
        r.cycles_executed, 0,
        "default-deny matrix must block all cycles"
    );
    assert_eq!(r.cycles_cap_denied, 5, "all five cycles deny in default-deny");
    // Each denial recorded in the journal as a CycleOutcome::CapDenied.
    let cap_denials = orch
        .journal
        .entries()
        .iter()
        .filter(|e| matches!(&e.kind, JournalKind::CycleOutcome(CycleOutcome::CapDenied { .. })))
        .count();
    assert_eq!(cap_denials, 5);
}

#[test]
fn t3_cap_grant_unlocks_cycle() {
    let cfg = small_cfg();
    let mut caps = SovereignCapMatrix::default_deny();
    // Grant ONLY the caps required for KanTick + MyceliumSync.
    caps.grant(CapKind::MutateBias);
    caps.grant(CapKind::SigmaAnchor);
    caps.grant(CapKind::NetworkEgress);
    let mut orch = PersistentOrchestrator::new(cfg, caps);
    let r = orch.tick(0);
    assert!(
        r.cycles_executed >= 2,
        "kan + mycelium-sync should run with their caps granted : {r:?}"
    );
    // Verify the cycles that ran are exactly the cap-allowed pair.
    let success_kinds: Vec<_> = r
        .last_outcomes
        .iter()
        .filter(|o| o.is_success())
        .map(|o| o.kind())
        .collect();
    for k in &success_kinds {
        assert!(
            matches!(k, CycleKind::KanTick | CycleKind::MyceliumSync),
            "only cap-allowed cycles should run : got {k}"
        );
    }
}

#[test]
fn t4_idle_mode_detection() {
    let mut detector = crate::IdleDetector::new(5_000);
    // Fresh detector @ now=10_000 : last_input_at=0 → age=10_000 ≥ 5_000 ⇒ idle.
    let hint = ActivityHint::default();
    assert!(detector.is_idle(10_000, hint));
    // Observe an input at t=10_000.
    detector.observe(ActivityHint {
        last_input_at_ms: 10_000,
        ..Default::default()
    });
    // Now @ 12_000 the age is 2_000 < 5_000 ⇒ NOT idle.
    assert!(!detector.is_idle(12_000, ActivityHint::default()));
    // @ 16_000 the age is 6_000 ≥ 5_000 ⇒ idle.
    assert!(detector.is_idle(16_000, ActivityHint::default()));
    // force_active overrides.
    let force_active = ActivityHint {
        force_active: true,
        ..Default::default()
    };
    assert!(!detector.is_idle(16_000, force_active));
    // force_idle overrides too.
    let force_idle = ActivityHint {
        force_idle: true,
        ..Default::default()
    };
    assert!(detector.is_idle(0, force_idle));
}

#[test]
fn t5_throttle_on_busy() {
    let policy = ThrottlePolicy::new(50, 5_000);
    // High CPU + non-cheap cycle → Throttle.
    let busy = BusyHint {
        cpu_pct: 80,
        last_input_age_ms: 0,
        apocky_actively_working: false,
        force_bypass: false,
    };
    assert_eq!(policy.decide(CycleKind::SelfAuthor, busy), ThrottleDecision::Throttle);
    // High CPU but cheap cycle (mycelium-sync) → Allow.
    assert_eq!(policy.decide(CycleKind::MyceliumSync, busy), ThrottleDecision::Allow);
    // Apocky-active + heavy cycle → Throttle even at low CPU.
    let active = BusyHint {
        cpu_pct: 5,
        apocky_actively_working: true,
        ..Default::default()
    };
    assert_eq!(policy.decide(CycleKind::Playtest, active), ThrottleDecision::Throttle);
    // IdleDeepProcgen requires sustained idle — recent input throttles it.
    let recent_input = BusyHint {
        cpu_pct: 0,
        last_input_age_ms: 1_000, // < 5_000 idle-threshold
        ..Default::default()
    };
    assert_eq!(
        policy.decide(CycleKind::IdleDeepProcgen, recent_input),
        ThrottleDecision::Throttle
    );
    // Force-bypass overrides everything.
    let bypass = BusyHint {
        cpu_pct: 99,
        force_bypass: true,
        ..Default::default()
    };
    assert_eq!(policy.decide(CycleKind::SelfAuthor, bypass), ThrottleDecision::BypassAll);
}

#[test]
fn t6_sigma_chain_anchor_cadence() {
    let cfg = small_cfg(); // anchor_every_n_kan_updates = 4
    let mut orch = PersistentOrchestrator::new(cfg, grant_all());
    let initial_seq = orch.state.anchors.current_seq();
    // Run enough ticks to produce at least 4 KAN updates. NoopDriver returns
    // 1 update per tick, so 4+ KAN ticks should cross the threshold.
    for i in 0..6 {
        let now = (i as u64) * KAN_TICK_CADENCE_MS;
        orch.tick(now);
    }
    let final_seq = orch.state.anchors.current_seq();
    assert!(
        final_seq > initial_seq + 4,
        "anchor-chain should have advanced past the KAN threshold : init={initial_seq} final={final_seq}",
    );
    // Confirm at least one anchor record carries reason = KanThreshold.
    let saw_kan_threshold = orch.journal.entries().iter().any(|e| {
        matches!(
            &e.kind,
            JournalKind::Anchor(rec) if matches!(rec.reason, crate::anchor::AnchorReason::KanThreshold)
        )
    });
    assert!(saw_kan_threshold, "expected at least one KanThreshold anchor in journal");
}

#[test]
fn t7_journal_replay_roundtrip() {
    let cfg = small_cfg();
    let mut orch = PersistentOrchestrator::new(cfg, grant_all());
    orch.tick(0);
    orch.tick(MYCELIUM_SYNC_CADENCE_MS);
    orch.tick(KAN_TICK_CADENCE_MS);
    let ndjson = orch.journal.to_ndjson().expect("ndjson serialize");
    let replayed = JournalReplay::replay(&ndjson).expect("replay parse");
    assert!(!replayed.is_empty(), "replay must produce at least one entry");
    // Replay-determinism : sequences are monotonic + dense.
    for win in replayed.windows(2) {
        assert!(win[1].seq > win[0].seq, "seq must strictly increase");
    }
    // Next-seq matches journal's next-seq logic.
    let next = JournalReplay::next_seq_after(&replayed);
    assert_eq!(
        next,
        replayed.last().unwrap().seq + 1,
        "next-seq is one past the highest replayed seq"
    );
    // Bootstrap entry recovered : the first replayed entry is Bootstrap.
    assert!(
        matches!(replayed[0].kind, JournalKind::Bootstrap { .. }),
        "first replayed entry must be Bootstrap : got {:?}",
        replayed[0].kind
    );
}

#[test]
fn t8_sovereign_pause_resume() {
    let cfg = small_cfg();
    let mut orch = PersistentOrchestrator::new(cfg, grant_all());
    orch.sovereign_pause(100);
    assert!(orch.state.paused);
    let r = orch.tick(200);
    assert_eq!(r.cycles_executed, 0, "paused daemon must not run cycles");
    assert!(r.paused);
    orch.sovereign_resume(300);
    assert!(!orch.state.paused);
    let r = orch.tick(400);
    assert!(r.cycles_executed >= 1, "resumed daemon must catch up : {r:?}");
}

#[test]
fn t9_cross_cycle_determinism() {
    let cfg = small_cfg();
    let mut a = PersistentOrchestrator::new(cfg.clone(), grant_all());
    let mut b = PersistentOrchestrator::new(cfg, grant_all());
    let hints = [(0u64, BusyHint::default()), (1_000, BusyHint::default()), (60_000, BusyHint::default())];
    for (now, hint) in hints {
        a.tick_with_hints(now, hint, ActivityHint::default());
        b.tick_with_hints(now, hint, ActivityHint::default());
    }
    let (a_digest, a_seq) = a.latest_anchor();
    let (b_digest, b_seq) = b.latest_anchor();
    assert_eq!(a_seq, b_seq, "deterministic anchor-seq across replays");
    assert_eq!(a_digest, b_digest, "deterministic anchor-digest across replays");
}

#[test]
fn t10_anchor_chain_monotonic() {
    let mut chain = AnchorChain::genesis();
    let mut last_seq = chain.current_seq();
    let mut last_digest = chain.current_digest();
    for i in 0..32 {
        let payload = format!("payload-{i}");
        let rec = chain.fold(payload.as_bytes(), i * 100, crate::anchor::AnchorReason::CycleClose);
        assert!(rec.seq > last_seq, "seq must strictly increase");
        assert_ne!(rec.digest, last_digest, "digest must change per fold");
        last_seq = rec.seq;
        last_digest = rec.digest;
    }
    // Genesis is reproducible.
    let g1 = AnchorChain::genesis();
    let g2 = AnchorChain::genesis();
    assert_eq!(g1.current_digest(), g2.current_digest());
    assert_eq!(g1.current_seq(), 0);
}

#[test]
fn t11_attestation_bootstrap_recorded() {
    let cfg = small_cfg();
    let orch = PersistentOrchestrator::new(cfg, SovereignCapMatrix::default_deny());
    // Bootstrap entry has been emitted in the constructor.
    let first = orch.journal.entries().first().expect("journal not empty");
    match &first.kind {
        JournalKind::Bootstrap { attestation_blake3 } => {
            // Verify attestation hash matches the constant.
            let expected = blake3::hash(crate::ATTESTATION.as_bytes());
            assert_eq!(*attestation_blake3, *expected.as_bytes());
        }
        other => panic!("first journal entry must be Bootstrap : got {other:?}"),
    }
}

#[test]
fn t12_quiescent_tick_recorded() {
    let cfg = small_cfg();
    let mut orch = PersistentOrchestrator::new(cfg, grant_all());
    orch.tick(0); // burns through all due cycles
    let len_before = orch.journal.entries().len();
    orch.tick(1); // 1ms later → no cycle due, expect a QuiescentTick entry
    let len_after = orch.journal.entries().len();
    assert!(
        len_after > len_before,
        "quiescent tick must still record a journal entry"
    );
    let last = orch.journal.last().expect("journal has entries");
    assert!(
        matches!(last.kind, JournalKind::QuiescentTick { .. }),
        "last entry should be QuiescentTick : got {:?}",
        last.kind
    );
}

#[test]
fn t13_cap_revoke_re_locks() {
    let cfg = small_cfg();
    let mut orch = PersistentOrchestrator::new(cfg, grant_all());
    let r = orch.tick_with_hints(0, idle_hint(), ActivityHint::default());
    assert_eq!(r.cycles_executed, 5, "grant_all + idle-hint → all cycles run");
    // Revoke the SigmaAnchor cap → ALL cycles deny next round.
    orch.revoke_cap(CapKind::SigmaAnchor, 1_000);
    let r = orch.tick_with_hints(SELF_AUTHOR_CADENCE_MS, idle_hint(), ActivityHint::default());
    assert_eq!(r.cycles_executed, 0, "revoking SigmaAnchor must lock all cycles");
    assert!(r.cycles_cap_denied >= 1);
}

#[test]
fn t14_journal_capacity_compaction() {
    // Verify the in-memory journal compacts when capacity is exceeded.
    let mut journal = JournalStore::new(8);
    for i in 0..50 {
        journal.append(i, JournalKind::QuiescentTick { hint_summary: format!("t{i}") });
    }
    // Capacity bound respected (compaction drains 25% at threshold).
    assert!(journal.entries().len() <= 8);
    // Total-appended counter survives compaction.
    assert_eq!(journal.total_appended, 50);
}

#[test]
fn t15_noop_driver_reports_nonempty_stats() {
    // Pure unit check — the noop driver must produce non-empty stat strings
    // so the journal is never silent for a "healthy idle" daemon.
    let mut d = NoopDriver::default();
    use crate::driver::SelfAuthorDriver;
    let s = d.run_self_author_cycle(100, 7).unwrap();
    assert!(s.contains("noop_self_author"));
    let mut d2 = NoopDriver::default();
    use crate::driver::PlaytestDriver;
    let (stat, signals) = d2.run_playtest_cycle(0, 0).unwrap();
    assert!(stat.contains("noop_playtest"));
    assert_eq!(signals.len(), 1, "noop playtest emits one synthetic signal");
}
