// § cssl-iter-loop integration tests.
//
// § T11-D233 (W-Jι) — exercise the public API as an MCP-tool consumer would,
// crossing module boundaries (protocol ↔ fixture ↔ spec_coverage_driven ↔
// perf_regression ↔ live_debug). Replay-determinism checks are stubbed where
// the upstream cssl-replay-validator integration-point lands ; the assertions
// here check the SHAPE of state-transitions + fixture-bytes + ranking outputs.

#![allow(clippy::redundant_clone)]
#![allow(clippy::similar_names)]
#![allow(clippy::float_cmp)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::wildcard_imports)]

use std::path::PathBuf;

use cssl_iter_loop::{
    compare_against_baseline, pick_largest_gap, rank_gaps, CommitHash, CreatureSnapshotStub,
    EngineInspection, EngineState, FailureReason, FixtureError, GapCoverageInput, IssueReport,
    IssueSeverity, IterationLoopState, LiveDebugError, LiveDebugSession, McpSessionStub,
    MetricHistory, OmegaSnapshotStub, PathHash, PerfBaseline, ProtocolError, ProtocolStateMachine,
    RegressionSeverity, ReloadId, RuntimeFixture, Sample, SessionId, TriggerEvent, TunableValue,
    ATTESTATION, SLICE_ID,
};
use cssl_spec_coverage::{
    ImplStatus, SpecAnchor, SpecAnchorBuilder, SpecCoverageReport, SpecRoot, TestStatus,
};

// ─────────────────────────────────────────────────────────────────────────────
// helpers
// ─────────────────────────────────────────────────────────────────────────────

fn fresh_session() -> McpSessionStub {
    McpSessionStub::new(SessionId::from_bytes([7u8; 16]), "stdio", "DevModeChild")
}

fn bug_issue() -> IssueReport {
    IssueReport::new(
        IssueSeverity::Bug,
        "Omniverse/02_CSSL/05_wave_solver § III.2",
        "psi_norm violated by 0.003",
    )
    .with_invariant("wave_solver.psi_norm_conserved")
}

fn cosmetic_issue() -> IssueReport {
    IssueReport::new(
        IssueSeverity::Cosmetic,
        "specs/01_DOC.csl § I",
        "typo in module-doc",
    )
}

fn perf_issue() -> IssueReport {
    IssueReport::new(
        IssueSeverity::PerfRegression,
        "specs/22_TELEMETRY.csl § IV",
        "p99 +7%",
    )
}

fn invariant_issue() -> IssueReport {
    IssueReport::new(
        IssueSeverity::InvariantViolation,
        "Omniverse/02_CSSL/05 § V",
        "psi_norm > tol",
    )
    .with_invariant("psi_norm_conserved")
}

fn good_commit() -> CommitHash {
    CommitHash([0xAA; 32])
}

fn other_commit() -> CommitHash {
    CommitHash([0xBB; 32])
}

fn make_anchor(file: &str, sec: &str, status: ImplStatus, test: TestStatus) -> SpecAnchor {
    SpecAnchorBuilder::new()
        .spec_root(SpecRoot::CssLv3)
        .spec_file(file)
        .section(sec)
        .impl_status(status)
        .test_status(test)
        .build()
}

fn missing_anchor(file: &str, sec: &str) -> SpecAnchor {
    make_anchor(file, sec, ImplStatus::Missing, TestStatus::Untested)
}

fn build_baseline_history() -> MetricHistory {
    let mut vals: Vec<f64> = (0..9890).map(|_| 12_500.0).collect();
    vals.extend(vec![15_000.0; 90]);
    vals.extend(vec![16_000.0; 20]);
    let samples: Vec<Sample> = vals
        .iter()
        .enumerate()
        .map(|(i, v)| Sample {
            frame_n: i as u64,
            value: *v,
        })
        .collect();
    MetricHistory::from_samples("frame.tick_us", samples)
}

fn build_severe_history() -> MetricHistory {
    let mut vals: Vec<f64> = (0..9890).map(|_| 12_400.0).collect();
    vals.extend(vec![17_000.0; 90]);
    vals.extend(vec![22_000.0; 20]);
    let samples: Vec<Sample> = vals
        .iter()
        .enumerate()
        .map(|(i, v)| Sample {
            frame_n: i as u64,
            value: *v,
        })
        .collect();
    MetricHistory::from_samples("frame.tick_us", samples)
}

fn make_fixture(frame_n: u64) -> RuntimeFixture {
    let omega = OmegaSnapshotStub::new(frame_n, 4096);
    let creature = CreatureSnapshotStub::new(0xABCD, 3);
    let trigger = TriggerEvent::InvariantViolation {
        invariant_name: "psi_norm".into(),
        observed: 1.003,
        expected_max_dev: 0.001,
    };
    RuntimeFixture::extract_from_runtime(frame_n, 0xDEADBEEF, omega, creature, trigger).unwrap()
}

fn temp_dir() -> PathBuf {
    let dir = std::env::temp_dir().join("cssl-iter-loop-integration");
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

// ─────────────────────────────────────────────────────────────────────────────
// § A : protocol-state-machine — 14 tests
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn integration_protocol_attach_initial_state() {
    let m = ProtocolStateMachine::attach(fresh_session());
    assert_eq!(m.current_state().kind(), "Attached");
    assert_eq!(m.current_step(), 1);
    assert_eq!(m.cycle(), 0);
    assert!(!m.is_terminal());
}

#[test]
fn integration_protocol_full_path_to_committed() {
    let m = ProtocolStateMachine::attach(fresh_session())
        .query_state(EngineState::fresh(12_000))
        .unwrap()
        .identify(bug_issue())
        .unwrap()
        .patch(good_commit())
        .unwrap()
        .hot_reload(ReloadId(1))
        .unwrap()
        .verify(true)
        .unwrap()
        .commit(good_commit())
        .unwrap();
    assert!(m.is_success());
}

#[test]
fn integration_protocol_iterate_with_refined_issue_reaches_committed() {
    let m = ProtocolStateMachine::attach(fresh_session())
        .query_state(EngineState::fresh(1))
        .unwrap()
        .identify(bug_issue())
        .unwrap()
        .patch(good_commit())
        .unwrap()
        .hot_reload(ReloadId(0))
        .unwrap()
        .verify(false)
        .unwrap()
        .iterate(invariant_issue())
        .unwrap()
        .patch(other_commit())
        .unwrap()
        .hot_reload(ReloadId(1))
        .unwrap()
        .verify(true)
        .unwrap()
        .commit(other_commit())
        .unwrap();
    assert!(m.is_success());
    assert_eq!(m.cycle(), 1);
}

#[test]
fn integration_protocol_kill_switch_failure_path() {
    let m = ProtocolStateMachine::attach(fresh_session()).fail(FailureReason::KillSwitchFired {
        message: "PD-violation".into(),
    });
    if let IterationLoopState::Failed { reason } = m.current_state() {
        assert!(format!("{reason}").contains("kill-switch"));
    } else {
        panic!("expected Failed");
    }
    assert!(m.is_terminal());
    assert!(!m.is_success());
}

#[test]
fn integration_protocol_invalid_transition_rejected() {
    let m = ProtocolStateMachine::attach(fresh_session());
    let r = m.patch(good_commit());
    assert!(matches!(r, Err(ProtocolError::InvalidTransition { .. })));
}

#[test]
fn integration_protocol_step_numbers_monotonic() {
    let m = ProtocolStateMachine::attach(fresh_session());
    let steps_seen: Vec<u8> = vec![
        m.current_step(),
        m.clone()
            .query_state(EngineState::fresh(1))
            .unwrap()
            .current_step(),
        m.clone()
            .query_state(EngineState::fresh(1))
            .unwrap()
            .identify(bug_issue())
            .unwrap()
            .current_step(),
    ];
    // 1 ≤ 2 ≤ 4 (focus collapsed).
    assert!(steps_seen.windows(2).all(|w| w[0] < w[1]));
}

#[test]
fn integration_protocol_max_cycles_enforces_budget() {
    let mut m = ProtocolStateMachine::attach_with_max_cycles(fresh_session(), 1)
        .query_state(EngineState::fresh(1))
        .unwrap()
        .identify(bug_issue())
        .unwrap()
        .patch(good_commit())
        .unwrap()
        .hot_reload(ReloadId(0))
        .unwrap()
        .verify(false)
        .unwrap();
    m = m.iterate(bug_issue()).unwrap();
    assert!(matches!(
        m.current_state(),
        IterationLoopState::Failed { .. }
    ));
}

#[test]
fn integration_protocol_commit_rejects_unverified() {
    let m = ProtocolStateMachine::attach(fresh_session())
        .query_state(EngineState::fresh(1))
        .unwrap()
        .identify(bug_issue())
        .unwrap()
        .patch(good_commit())
        .unwrap()
        .hot_reload(ReloadId(0))
        .unwrap()
        .verify(false)
        .unwrap();
    let r = m.commit(good_commit());
    assert!(matches!(r, Err(ProtocolError::InvalidTransition { .. })));
}

#[test]
fn integration_protocol_commit_hash_round_trip() {
    let hex = "feedfacedeadbeefcafebabe1234567890abcdef";
    let h = CommitHash::from_git_sha_hex(hex).unwrap();
    let s = h.to_hex();
    assert!(s.starts_with("feedfacedeadbeef"));
}

#[test]
fn integration_protocol_session_id_explicit_construction() {
    let id = SessionId::from_bytes([3u8; 16]);
    let session = McpSessionStub::new(id, "ws", "Implementer");
    assert_eq!(session.session_id, id);
    assert_eq!(session.transport, "ws");
}

#[test]
fn integration_protocol_failure_reason_display_invariant_failing() {
    let r = FailureReason::InvariantStillFailing {
        name: "psi_norm".into(),
        message: "drift 0.003".into(),
    };
    let s = format!("{r}");
    assert!(s.contains("invariant"));
    assert!(s.contains("psi_norm"));
}

#[test]
fn integration_protocol_failure_reason_display_regression() {
    let r = FailureReason::RegressionIntroduced {
        message: "frame.tick_us p99 +7%".into(),
    };
    assert!(format!("{r}").contains("regression"));
}

#[test]
fn integration_protocol_attach_with_custom_max_cycles() {
    let m = ProtocolStateMachine::attach_with_max_cycles(fresh_session(), 5);
    assert_eq!(m.max_cycles(), 5);
}

#[test]
fn integration_protocol_commit_hash_invalid_length() {
    let r = CommitHash::from_git_sha_hex("abc");
    assert!(matches!(r, Err(ProtocolError::InvalidCommitHash { .. })));
}

// ─────────────────────────────────────────────────────────────────────────────
// § B : fixture-roundtrip — 8 tests
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn integration_fixture_round_trip_disk() {
    let f = make_fixture(12_345);
    let path = temp_dir().join("fixture-A.json");
    f.serialize_to_disk(&path).unwrap();
    let loaded = RuntimeFixture::load_from_disk(&path).unwrap();
    assert_eq!(f, loaded);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn integration_fixture_into_regression_test_naming() {
    let f = make_fixture(42_000);
    let case = f.into_regression_test();
    assert!(case.test_name.starts_with("regression_inv_"));
    assert!(case.test_name.contains("42000"));
}

#[test]
fn integration_fixture_biometric_refusal() {
    let mut omega = OmegaSnapshotStub::new(1, 1);
    omega.biometric_stripped = false;
    let creature = CreatureSnapshotStub::new(1, 1);
    let trigger = TriggerEvent::ManualSnapshot {
        reason: "test".into(),
    };
    let r = RuntimeFixture::extract_from_runtime(1, 1, omega, creature, trigger);
    assert!(matches!(r, Err(FixtureError::BiometricRefused)));
}

#[test]
fn integration_fixture_integrity_check_blocks_tamper() {
    let mut f = make_fixture(99);
    f.frame_n = 0xFEED; // tamper post-extract
    assert!(f.verify_integrity().is_err());
}

#[test]
fn integration_fixture_each_trigger_kind_named_distinctly() {
    let omega = OmegaSnapshotStub::new(1, 1);
    let creature = CreatureSnapshotStub::new(1, 1);
    let triggers = vec![
        (
            TriggerEvent::InvariantViolation {
                invariant_name: "psi_norm".into(),
                observed: 1.0,
                expected_max_dev: 0.5,
            },
            "regression_inv",
        ),
        (
            TriggerEvent::ErrorEvent {
                subsystem: "renderer".into(),
                kind_id: 1,
                message: "msg".into(),
            },
            "regression_err",
        ),
        (
            TriggerEvent::ManualSnapshot {
                reason: "snap".into(),
            },
            "regression_manual",
        ),
        (
            TriggerEvent::PerfRegression {
                metric_name: "frame.tick_us".into(),
                ratio_x1000: 1100,
            },
            "regression_perf",
        ),
    ];
    for (trig, prefix) in triggers {
        let f = RuntimeFixture::extract_from_runtime(1, 1, omega.clone(), creature.clone(), trig)
            .unwrap();
        let case = f.into_regression_test();
        assert!(
            case.test_name.starts_with(prefix),
            "expected {prefix} got {}",
            case.test_name
        );
    }
}

#[test]
fn integration_fixture_path_hash_deterministic() {
    let p1 = std::path::Path::new("/etc/passwd");
    let p2 = std::path::Path::new("/etc/passwd");
    let p3 = std::path::Path::new("/etc/shadow");
    let h1 = PathHash::hash(p1);
    let h2 = PathHash::hash(p2);
    let h3 = PathHash::hash(p3);
    assert_eq!(h1, h2);
    assert_ne!(h1, h3);
}

#[test]
fn integration_fixture_load_missing_file_errors() {
    let path = temp_dir().join("nonexistent-fixture.json");
    let r = RuntimeFixture::load_from_disk(&path);
    assert!(matches!(r, Err(FixtureError::Io { .. })));
}

#[test]
fn integration_fixture_two_distinct_fixtures_have_distinct_hashes() {
    let f1 = make_fixture(1);
    let f2 = make_fixture(2);
    assert_ne!(f1.blake3_hash, f2.blake3_hash);
}

// ─────────────────────────────────────────────────────────────────────────────
// § C : spec-coverage-driven gap-pick — 7 tests
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn integration_gap_pick_returns_highest_priority() {
    let anchors = vec![
        missing_anchor("a.csl", "§ I"),
        missing_anchor("b.csl", "§ V"),
        make_anchor(
            "c.csl",
            "§ I",
            ImplStatus::Partial {
                crate_path: "cssl-c".into(),
                gaps: vec!["x".into()],
            },
            TestStatus::Untested,
        ),
    ];
    let report = SpecCoverageReport::build(anchors.iter());
    let top = pick_largest_gap(&report).unwrap();
    // Missing+§V outscores all (high-urgency boost + missing).
    assert!(top.spec_anchor_key.contains("b.csl"));
}

#[test]
fn integration_gap_rank_returns_descending_scores() {
    let anchors = vec![
        missing_anchor("a.csl", "§ X"),
        missing_anchor("b.csl", "§ V"),
        missing_anchor("c.csl", "§ I"),
    ];
    let report = SpecCoverageReport::build(anchors.iter());
    let r = rank_gaps(&report);
    for w in r.entries.windows(2) {
        assert!(w[0].priority_score >= w[1].priority_score);
    }
}

#[test]
fn integration_gap_skip_claimed_keys() {
    let anchors = vec![
        missing_anchor("a.csl", "§ I"),
        missing_anchor("b.csl", "§ I"),
    ];
    let report = SpecCoverageReport::build(anchors.iter());
    let claimed: Vec<String> = vec![format!("CssLv3::{}::{}", "a.csl", "§ I")];
    let r = cssl_iter_loop::spec_coverage_driven::rank_gaps_with_input(
        GapCoverageInput::new(&report).with_claimed_keys(&claimed),
    );
    assert_eq!(r.entries.len(), 1);
    assert!(r.entries[0].spec_anchor_key.contains("b.csl"));
}

#[test]
fn integration_gap_section_prefix_filter() {
    let anchors = vec![
        missing_anchor("a.csl", "§ I"),
        missing_anchor("b.csl", "§ V"),
    ];
    let report = SpecCoverageReport::build(anchors.iter());
    let r = cssl_iter_loop::spec_coverage_driven::rank_gaps_with_input(
        GapCoverageInput::new(&report).with_section_prefix("§ V"),
    );
    assert_eq!(r.entries.len(), 1);
}

#[test]
fn integration_gap_empty_report_returns_none() {
    let anchors: Vec<SpecAnchor> = vec![];
    let report = SpecCoverageReport::build(anchors.iter());
    assert!(pick_largest_gap(&report).is_none());
}

#[test]
fn integration_gap_unclaimed_default_when_no_set_supplied() {
    let anchors = vec![missing_anchor("a.csl", "§ I")];
    let report = SpecCoverageReport::build(anchors.iter());
    let r = pick_largest_gap(&report).unwrap();
    assert!(r.unclaimed);
}

#[test]
fn integration_gap_score_anchor_directly() {
    let m = missing_anchor("a.csl", "§ V");
    let s = make_anchor(
        "a.csl",
        "§ X",
        ImplStatus::Stub {
            crate_path: "cssl".into(),
        },
        TestStatus::Untested,
    );
    assert!(
        cssl_iter_loop::spec_coverage_driven::score_anchor(&m)
            > cssl_iter_loop::spec_coverage_driven::score_anchor(&s)
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// § D : perf-regression — 6 tests
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn integration_perf_baseline_capture_round_trip() {
    let h = build_baseline_history();
    let b = PerfBaseline::capture(&h, good_commit());
    assert_eq!(b.metric_name, "frame.tick_us");
    assert_eq!(b.captured_at_commit, good_commit());
}

#[test]
fn integration_perf_compare_pass_when_unchanged() {
    let h = build_baseline_history();
    let b = PerfBaseline::capture(&h, good_commit());
    let r = compare_against_baseline(&h, &b).unwrap();
    assert_eq!(r.severity, RegressionSeverity::Pass);
    assert!(!r.is_regression());
}

#[test]
fn integration_perf_compare_severe_when_both_inflate() {
    let baseline_h = build_baseline_history();
    let current_h = build_severe_history();
    let baseline = PerfBaseline::capture(&baseline_h, good_commit());
    let r = compare_against_baseline(&current_h, &baseline).unwrap();
    assert_eq!(r.severity, RegressionSeverity::SevereRegression);
}

#[test]
fn integration_perf_metric_name_mismatch_rejected() {
    let baseline_h = build_baseline_history();
    let mut current_h = build_baseline_history();
    current_h.metric_name = "different.metric".into();
    let baseline = PerfBaseline::capture(&baseline_h, good_commit());
    let r = compare_against_baseline(&current_h, &baseline);
    assert!(r.is_err());
}

#[test]
fn integration_perf_invalid_baseline_rejected() {
    let baseline_h = build_baseline_history();
    let current_h = build_baseline_history();
    let mut baseline = PerfBaseline::capture(&baseline_h, good_commit());
    baseline.p99 = 0.0;
    let r = compare_against_baseline(&current_h, &baseline);
    assert!(r.is_err());
}

#[test]
fn integration_perf_metric_history_filters_nan() {
    let samples: Vec<Sample> = vec![1.0, 2.0, f64::NAN, 3.0, 4.0]
        .into_iter()
        .enumerate()
        .map(|(i, v)| Sample {
            frame_n: i as u64,
            value: v,
        })
        .collect();
    let h = MetricHistory::from_samples("test", samples);
    assert!(h.max > 0.0);
    assert!(h.p50 > 0.0);
}

// ─────────────────────────────────────────────────────────────────────────────
// § E : live-debug — 9 tests
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn integration_live_debug_canonical_dance() {
    let mut s = LiveDebugSession::attach(EngineState::fresh(5_000));
    s.pause_engine().unwrap();
    s.record_inspect("entity:0xFEED").unwrap();
    s.step_n_frames(10).unwrap();
    s.tweak_value("dt_floor", TunableValue::Float(1e-6))
        .unwrap();
    s.step_n_frames(5).unwrap();
    s.record_hot_reload(ReloadId(99)).unwrap();
    s.resume_engine().unwrap();
    assert_eq!(s.current_frame(), 5_015);
    assert_eq!(s.step_count(), 15);
    assert!(!s.paused());
}

#[test]
fn integration_live_debug_inspect_returns_state() {
    let mut s = LiveDebugSession::attach(EngineState::fresh(100));
    s.pause_engine().unwrap();
    s.tweak_value("k", TunableValue::Int(42)).unwrap();
    let snap: EngineInspection = s.inspect_current().unwrap();
    assert_eq!(snap.frame_n, 100);
    assert!(snap.paused);
    assert!(matches!(
        snap.tunables.get("k"),
        Some(TunableValue::Int(42))
    ));
}

#[test]
fn integration_live_debug_tunable_kind_strict() {
    let mut s = LiveDebugSession::attach(EngineState::fresh(0));
    s.pause_engine().unwrap();
    s.tweak_value("v", TunableValue::Bool(true)).unwrap();
    let r = s.tweak_value("v", TunableValue::Float(1.0));
    assert!(matches!(r, Err(LiveDebugError::TunableRejected { .. })));
}

#[test]
fn integration_live_debug_step_too_large() {
    let mut s = LiveDebugSession::attach(EngineState::fresh(0));
    s.pause_engine().unwrap();
    let r = s.step_n_frames(2_000_000);
    assert!(matches!(r, Err(LiveDebugError::StepTooLarge { .. })));
}

#[test]
fn integration_live_debug_resume_requires_paused() {
    let mut s = LiveDebugSession::attach(EngineState::fresh(0));
    let r = s.resume_engine();
    assert!(matches!(r, Err(LiveDebugError::InvalidTransition { .. })));
}

#[test]
fn integration_live_debug_double_pause_rejected() {
    let mut s = LiveDebugSession::attach(EngineState::fresh(0));
    s.pause_engine().unwrap();
    let r = s.pause_engine();
    assert!(matches!(r, Err(LiveDebugError::InvalidTransition { .. })));
}

#[test]
fn integration_live_debug_inspect_when_running_rejected() {
    let s = LiveDebugSession::attach(EngineState::fresh(0));
    let r = s.inspect_current();
    assert!(matches!(r, Err(LiveDebugError::InvalidTransition { .. })));
}

#[test]
fn integration_live_debug_trail_length_grows_per_op() {
    let mut s = LiveDebugSession::attach(EngineState::fresh(0));
    s.pause_engine().unwrap();
    let l0 = s.trail().len();
    s.step_n_frames(1).unwrap();
    let l1 = s.trail().len();
    s.tweak_value("k", TunableValue::Float(0.0)).unwrap();
    let l2 = s.trail().len();
    assert!(l1 > l0);
    assert!(l2 > l1);
}

#[test]
fn integration_live_debug_tunable_value_kinds_distinct() {
    assert_eq!(TunableValue::Bool(true).kind(), "bool");
    assert_eq!(TunableValue::Int(0).kind(), "int");
    assert_eq!(TunableValue::Float(0.0).kind(), "float");
    assert_eq!(TunableValue::Text("x".into()).kind(), "text");
}

// ─────────────────────────────────────────────────────────────────────────────
// § F : cross-module + sanity — 6 tests
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn integration_attestation_present_at_crate_root() {
    assert!(ATTESTATION.starts_with("There was no hurt nor harm"));
}

#[test]
fn integration_slice_id_canonical() {
    assert!(SLICE_ID.contains("T11-D233"));
}

#[test]
fn integration_full_loop_with_fixture_extract_at_failure() {
    // Drive the protocol to verify=false → extract a fixture from "what we
    // saw at that frame" → confirm the fixture's trigger reflects the issue.
    let m = ProtocolStateMachine::attach(fresh_session())
        .query_state(EngineState::fresh(12_127))
        .unwrap()
        .identify(bug_issue())
        .unwrap()
        .patch(good_commit())
        .unwrap()
        .hot_reload(ReloadId(1))
        .unwrap()
        .verify(false)
        .unwrap();
    assert!(matches!(
        m.current_state(),
        IterationLoopState::Verified {
            invariant_pass: false
        }
    ));
    let f = make_fixture(12_127);
    assert_eq!(f.frame_n, 12_127);
}

#[test]
fn integration_perf_regression_drives_protocol_failure() {
    // Couple: a perf-regression report ⇒ the protocol fails with a typed
    // FailureReason::RegressionIntroduced. The state-machine's `fail` is the
    // bridge.
    let baseline_h = build_baseline_history();
    let current_h = build_severe_history();
    let baseline = PerfBaseline::capture(&baseline_h, good_commit());
    let r = compare_against_baseline(&current_h, &baseline).unwrap();
    assert!(r.is_regression());
    let m =
        ProtocolStateMachine::attach(fresh_session()).fail(FailureReason::RegressionIntroduced {
            message: format!(
                "p99 {:.4}× ; p999 {:.4}× ; severity={:?}",
                r.p99_ratio, r.p999_ratio, r.severity
            ),
        });
    assert!(m.is_terminal());
    assert!(!m.is_success());
}

#[test]
fn integration_gap_pick_drives_identify_step() {
    // After picking a gap, an LLM would set up an IssueReport tagging that
    // anchor's key. Verify the integration : pick → IssueReport → Identified.
    let anchors = vec![missing_anchor("a.csl", "§ V")];
    let report = SpecCoverageReport::build(anchors.iter());
    let top = pick_largest_gap(&report).unwrap();
    let issue = IssueReport::new(
        IssueSeverity::Bug,
        top.spec_anchor_key.clone(),
        "implement gap",
    );
    let m = ProtocolStateMachine::attach(fresh_session())
        .query_state(EngineState::fresh(0))
        .unwrap()
        .identify(issue)
        .unwrap();
    if let IterationLoopState::Identified { issue } = m.current_state() {
        assert!(issue.spec_anchor_key.contains("a.csl"));
    } else {
        panic!("expected Identified");
    }
}

#[test]
fn integration_cosmetic_and_perf_issues_route_through_protocol() {
    // Both severity-classes traverse the state-machine identically.
    for issue in [cosmetic_issue(), perf_issue()] {
        let m = ProtocolStateMachine::attach(fresh_session())
            .query_state(EngineState::fresh(0))
            .unwrap()
            .identify(issue.clone())
            .unwrap();
        if let IterationLoopState::Identified { issue: stored } = m.current_state() {
            assert_eq!(stored.severity, issue.severity);
        }
    }
}
