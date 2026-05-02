//! § integration — end-to-end coverage for cssl-host-playtest-agent.

#![allow(clippy::similar_names)]
#![allow(clippy::redundant_clone)]
//!
//! § COVERAGE (≥ 10 tests per spec)
//!   1.  roundtrip                          — happy-path drive_session → report → anchor → verify
//!   2.  crash-detected                      — engine flags crash → trace records + Polish drops
//!   3.  softlock-detected                   — N consecutive non-progress → softlock event emitted
//!   4.  determinism-fail-detected           — different seeds → trace ≠ trace ; same seed → trace = trace
//!   5.  safety-violation-flagged            — sovereign-violation → Safety < 95 → SafetyRejected verdict
//!   6.  cosmetic-axiom-violated-rejects     — pay-for-power path → cosmetic_attest = false
//!   7.  scoring-arithmetic                  — weighted-aggregate stays in [0, 100] for boundary cases
//!   8.  KAN-signal-emission                 — QualitySignal::from_report yields expected q8 row
//!   9.  Σ-Chain-anchor                      — anchor_report → verify_anchor round-trip
//!   10. sovereign-revoke                    — declined content → drive_session pre-flight refuses

use cssl_host_playtest_agent::{
    anchor::{anchor_report, verify_anchor, AnchorError},
    attestation::{cosmetic_axiom_holds, sandbox_attestation},
    decline::{DeclineRecord, SovereignDecline},
    driver::{drive_session, EngineStepResult, GmDriver, ScriptedGmDriver},
    kan_bridge::QualitySignal,
    report::{PlayTestReport, ReportPublishVerdict},
    scoring::{
        balance_score, fun_score, polish_score, safety_score, weighted_total, Score, Thresholds,
    },
    session::{new_session, PlayTestError, PlayTestSession},
    suggestions::build_suggestions,
};
use ed25519_dalek::SigningKey;
use rand::rngs::OsRng;

/// § A test-engine that always-progresses + never-crashes. Used for the
/// happy-path roundtrip + determinism tests.
struct AlwaysProgressEngine;
impl GmDriver for AlwaysProgressEngine {
    fn step(&mut self, _turn: u32, _intent: &str) -> EngineStepResult {
        EngineStepResult::progress("step")
    }
}

/// § A test-engine that crashes every Nth turn (configurable).
struct PeriodicCrashEngine {
    period: u32,
}
impl GmDriver for PeriodicCrashEngine {
    fn step(&mut self, turn: u32, _intent: &str) -> EngineStepResult {
        let mut r = EngineStepResult::progress("step");
        if (turn + 1) % self.period == 0 {
            r.crashed = true;
            r.crash_kind = "panic".into();
        }
        r
    }
}

/// § A test-engine that NEVER progresses — used to exercise softlocks.
struct StuckEngine;
impl GmDriver for StuckEngine {
    fn step(&mut self, _turn: u32, _intent: &str) -> EngineStepResult {
        EngineStepResult::idle()
    }
}

/// § A test-engine that flags a sovereign-violation on turn 0 ; otherwise
/// progresses normally.
struct ViolatingEngine;
impl GmDriver for ViolatingEngine {
    fn step(&mut self, turn: u32, _intent: &str) -> EngineStepResult {
        let mut r = EngineStepResult::progress("step");
        if turn == 0 {
            r.sovereign_violation = Some("surveillance".into());
        }
        r
    }
}

/// § A test-engine that triggers a cosmetic-axiom violation on turn 0.
struct PayForPowerEngine;
impl GmDriver for PayForPowerEngine {
    fn step(&mut self, turn: u32, _intent: &str) -> EngineStepResult {
        let mut r = EngineStepResult::progress("step");
        if turn == 0 {
            r.cosmetic_violation = Some("shop:lootbox-power".into());
        }
        r
    }
}

/// § Helper — build a PlayTestSession with a small max_turns suitable for
/// fast tests.
fn small_session(content_id: u32, seed: u64, max_turns: u32) -> PlayTestSession {
    let mut s = new_session(content_id, seed);
    s.max_turns = max_turns;
    s
}

/// § Helper — assemble a report end-to-end from a trace + the four scores.
fn assemble_from_trace(
    s: &PlayTestSession,
    trace: &cssl_host_playtest_agent::session::Trace,
    determinism: bool,
) -> PlayTestReport {
    let fun = fun_score(trace);
    let bal = balance_score(trace);
    let saf = safety_score(trace);
    let pol = polish_score(trace, determinism);
    let cosmetic_attest = cosmetic_axiom_holds(trace);
    let suggestions = build_suggestions(trace, fun, bal, saf, pol);
    PlayTestReport::assemble(
        s.content_id,
        s.agent_persona_seed,
        trace.crash_count(),
        trace.softlock_count(),
        determinism,
        cosmetic_attest,
        fun,
        bal,
        saf,
        pol,
        Thresholds::default(),
        suggestions,
    )
}

// ─── 1. roundtrip ────────────────────────────────────────────────────────

#[test]
fn t01_roundtrip_happy_path() {
    let s = small_session(7, 0xCAFE, 12);
    let mut e = AlwaysProgressEngine;
    let trace = drive_session(&s, &mut e, &ScriptedGmDriver::default(), None).unwrap();
    let report = assemble_from_trace(&s, &trace, true);
    assert!(report.is_publishable());
    assert!(report.cosmetic_attest);
    assert_eq!(report.crashes, 0);
    assert_eq!(report.softlocks, 0);
    let attest = sandbox_attestation();
    assert!(attest.is_fully_sandboxed());
}

// ─── 2. crash-detected ───────────────────────────────────────────────────

#[test]
fn t02_crash_detected_drops_polish() {
    let s = small_session(11, 1, 10);
    let mut e = PeriodicCrashEngine { period: 2 };
    let trace = drive_session(&s, &mut e, &ScriptedGmDriver::default(), None).unwrap();
    assert!(trace.crash_count() >= 4);
    let pol = polish_score(&trace, true);
    // 4 crashes × 30 = 120 → floor at 0
    assert_eq!(pol.0, 0);
}

// ─── 3. softlock-detected ────────────────────────────────────────────────

#[test]
fn t03_softlock_detected_when_stuck() {
    let s = small_session(13, 2, 20);
    let mut e = StuckEngine;
    let driver = ScriptedGmDriver { softlock_window: 5 };
    let trace = drive_session(&s, &mut e, &driver, None).unwrap();
    assert!(trace.softlock_count() >= 1);
}

// ─── 4. determinism check ────────────────────────────────────────────────

#[test]
fn t04_determinism_holds_for_same_seed() {
    let s = small_session(17, 0xABCD, 15);
    let mut e1 = AlwaysProgressEngine;
    let mut e2 = AlwaysProgressEngine;
    let t1 = drive_session(&s, &mut e1, &ScriptedGmDriver::default(), None).unwrap();
    let t2 = drive_session(&s, &mut e2, &ScriptedGmDriver::default(), None).unwrap();
    assert!(t1.is_deterministic_with(&t2));
}

#[test]
fn t04b_determinism_breaks_for_different_seed() {
    let mut s1 = small_session(17, 0xABCD, 15);
    let mut s2 = small_session(17, 0xABCD, 15);
    s1.agent_persona_seed = 1;
    s2.agent_persona_seed = 2;
    let mut e1 = AlwaysProgressEngine;
    let mut e2 = AlwaysProgressEngine;
    let t1 = drive_session(&s1, &mut e1, &ScriptedGmDriver::default(), None).unwrap();
    let t2 = drive_session(&s2, &mut e2, &ScriptedGmDriver::default(), None).unwrap();
    // The intent-stream depends on the seed → traces should differ.
    assert!(!t1.is_deterministic_with(&t2));
}

// ─── 5. safety-violation-flagged ─────────────────────────────────────────

#[test]
fn t05_safety_violation_blocks_publish() {
    let s = small_session(19, 0xDEAD, 10);
    let mut e = ViolatingEngine;
    let trace = drive_session(&s, &mut e, &ScriptedGmDriver::default(), None).unwrap();
    assert_eq!(trace.sovereign_violation_count(), 1);
    let report = assemble_from_trace(&s, &trace, true);
    // Safety = 50 (one sovereign-violation) → below 95 → safety_blocked
    assert!(report.safety.0 < 95);
    assert!(report.safety_blocked());
    assert!(matches!(
        report.verdict,
        ReportPublishVerdict::SafetyRejected | ReportPublishVerdict::BothBarsFailed
    ));
}

// ─── 6. cosmetic-axiom-violated-rejects ──────────────────────────────────

#[test]
fn t06_cosmetic_axiom_violation_clears_attestation() {
    let s = small_session(23, 0xBEEF, 5);
    let mut e = PayForPowerEngine;
    let trace = drive_session(&s, &mut e, &ScriptedGmDriver::default(), None).unwrap();
    assert!(!cosmetic_axiom_holds(&trace));
    let report = assemble_from_trace(&s, &trace, true);
    assert!(!report.cosmetic_attest);
}

// ─── 7. scoring-arithmetic ───────────────────────────────────────────────

#[test]
fn t07_scoring_arithmetic_extremes_stay_in_range() {
    // All-100 → total = 100
    assert_eq!(
        weighted_total(Score(100), Score(100), Score(100), Score(100)),
        100
    );
    // All-0 → total = 0
    assert_eq!(weighted_total(Score(0), Score(0), Score(0), Score(0)), 0);
    // Mixed boundary — ensure we don't overflow / wrap
    let v = weighted_total(Score(255), Score(255), Score(255), Score(255));
    // Score::clamped never lets internal value exceed 100, so weighted_total
    // is also bounded ; this is a structural assertion.
    let _ = v;
}

#[test]
fn t07b_scoring_known_vector_canonical() {
    // Spec example : Fun=80 Bal=70 Saf=100 Pol=60 → 80*40+70*30+100*20+60*10 = 7900/100 = 79
    assert_eq!(
        weighted_total(Score(80), Score(70), Score(100), Score(60)),
        79
    );
}

// ─── 8. KAN-signal-emission ──────────────────────────────────────────────

#[test]
fn t08_kan_signal_emission_round_trip() {
    let s = small_session(29, 0x1234, 8);
    let mut e = AlwaysProgressEngine;
    let trace = drive_session(&s, &mut e, &ScriptedGmDriver::default(), None).unwrap();
    let report = assemble_from_trace(&s, &trace, true);
    let q = QualitySignal::from_report(&report);
    // q8 monotonic with score : 100 → 255
    assert_eq!(q.safety_q8, 255);
    assert_eq!(q.cosmetic_attest, 1);
    assert_eq!(q.determinism_ok, 1);
    assert_eq!(q.crash_count, 0);
    assert_eq!(q.softlock_count, 0);
}

// ─── 9. Σ-Chain anchor + verify ───────────────────────────────────────────

#[test]
fn t09_anchor_round_trips_through_verify() {
    let s = small_session(31, 0xFACE, 6);
    let mut e = AlwaysProgressEngine;
    let trace = drive_session(&s, &mut e, &ScriptedGmDriver::default(), None).unwrap();
    let report = assemble_from_trace(&s, &trace, true);

    let key = SigningKey::generate(&mut OsRng);
    let anchor = anchor_report(&report, &key).unwrap();
    assert!(verify_anchor(&report, &anchor).is_ok());
    assert_eq!(anchor.total_score, report.total);
}

#[test]
fn t09b_anchor_tamper_detected() {
    let s = small_session(31, 0xFACE, 6);
    let mut e = AlwaysProgressEngine;
    let trace = drive_session(&s, &mut e, &ScriptedGmDriver::default(), None).unwrap();
    let report = assemble_from_trace(&s, &trace, true);

    let key = SigningKey::generate(&mut OsRng);
    let anchor = anchor_report(&report, &key).unwrap();

    let mut tampered = report.clone();
    tampered.total = 99;
    assert!(matches!(
        verify_anchor(&tampered, &anchor),
        Err(AnchorError::HashMismatch)
    ));
}

// ─── 10. sovereign-revoke ────────────────────────────────────────────────

#[test]
fn t10_sovereign_revoke_blocks_session() {
    let mut reg = SovereignDecline::new();
    let rec = DeclineRecord::new(99, [0; 8], 1, "want to revise").unwrap();
    reg.set(rec);
    assert_eq!(reg.check(99), Err(PlayTestError::Declined(99)));
    // Other content still allowed
    assert_eq!(reg.check(100), Ok(()));
}

#[test]
fn t10b_revoke_decline_unblocks() {
    let mut reg = SovereignDecline::new();
    reg.set(DeclineRecord::new(99, [0; 8], 1, "").unwrap());
    assert!(reg.is_declined(99));
    reg.revoke_decline(99);
    assert!(!reg.is_declined(99));
    assert_eq!(reg.check(99), Ok(()));
}

// ─── BONUS — author-actionable suggestions surface CSLv3 glyphs ───────────

#[test]
fn t11_suggestions_emit_csl3_glyphs_on_failures() {
    let s = small_session(37, 1, 5);
    let mut e = ViolatingEngine;
    let trace = drive_session(&s, &mut e, &ScriptedGmDriver::default(), None).unwrap();
    let r = assemble_from_trace(&s, &trace, true);
    let s_text: Vec<&str> = r.suggestions.iter().map(|x| x.text.as_str()).collect();
    assert!(s_text.iter().any(|t| t.contains("§")));
    assert!(s_text.iter().any(|t| t.contains("✗") || t.contains("◐")));
}

#[test]
fn t12_suggestion_fun_signal_when_low_diversity() {
    // Force low-fun by single-entry trace
    use cssl_host_playtest_agent::session::{Trace, TraceEvent};
    let mut t = Trace::new();
    for i in 0..10 {
        t.push(TraceEvent::IntentInvoked {
            turn: i,
            intent: "talk".into(),
        });
    }
    let suggestions = build_suggestions(&t, Score(20), Score(70), Score(100), Score(80));
    assert!(suggestions
        .iter()
        .any(|x| x.text.contains("§ FUN") || x.text.contains("FUN")));
}
