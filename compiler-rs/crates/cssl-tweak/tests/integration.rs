//! cssl-tweak integration tests.
//!
//! § T11-D164 acceptance gate :
//! - 30 default-tunables registered with bounds + validators
//! - `Cap<Tweak>` stub gate enforced on every mutate path
//! - Frame-boundary defer (changes apply at next-frame, not mid-frame)
//! - Replay-log records tweak-events with logical-frame-N
//! - 50+ tests pass
//!
//! These tests treat the crate purely through its public surface.

use cssl_tweak::{
    default_tunable_specs, install_defaults, AuditSink, BudgetMode, Cap, CapTag, ReplayLog, Stage,
    TunableId, TunableKind, TunableRange, TunableRegistry, TunableSpec, TunableValue, Tweak,
    TweakAuditEntry, TweakError, TweakEvent, TweakOrigin, DEFAULT_TUNABLE_COUNT,
};

// ─── helpers ───────────────────────────────────────────────────────────────────

fn fresh_registry_with_defaults() -> TunableRegistry {
    let mut reg = TunableRegistry::new();
    install_defaults(&mut reg).expect("install defaults");
    reg
}

fn id(name: &str) -> TunableId {
    TunableId::of(name)
}

// ─── default-table acceptance gate ─────────────────────────────────────────────

#[test]
fn default_tunable_count_is_thirty() {
    assert_eq!(DEFAULT_TUNABLE_COUNT, 30);
    assert_eq!(default_tunable_specs().len(), 30);
}

#[test]
fn defaults_install_into_registry() {
    let reg = fresh_registry_with_defaults();
    assert_eq!(reg.len(), 30);
    assert!(!reg.is_empty());
}

#[test]
fn every_default_canonical_name_is_unique() {
    let specs = default_tunable_specs();
    let mut names: Vec<&str> = specs.iter().map(|s| s.canonical_name).collect();
    names.sort_unstable();
    let before = names.len();
    names.dedup();
    assert_eq!(before, names.len());
}

#[test]
fn every_default_id_is_unique() {
    let specs = default_tunable_specs();
    let mut ids: Vec<TunableId> = specs.iter().map(TunableSpec::id).collect();
    ids.sort();
    let before = ids.len();
    ids.dedup();
    assert_eq!(before, ids.len());
}

#[test]
fn every_default_value_in_range() {
    for spec in default_tunable_specs() {
        assert!(
            spec.range.check_in_range(&spec.default).is_ok(),
            "default out of range: {}",
            spec.canonical_name
        );
    }
}

#[test]
fn every_default_kind_matches_value_kind() {
    for spec in default_tunable_specs() {
        assert_eq!(spec.kind, spec.default.kind(), "{}", spec.canonical_name);
    }
}

#[test]
fn every_default_kind_matches_range_kind() {
    for spec in default_tunable_specs() {
        assert_eq!(spec.kind, spec.range.kind(), "{}", spec.canonical_name);
    }
}

#[test]
fn render_fovea_detail_budget_registered() {
    let reg = fresh_registry_with_defaults();
    let spec = reg.spec(id("render.fovea_detail_budget")).unwrap();
    assert_eq!(spec.kind, TunableKind::F32);
    assert_eq!(spec.budget_mode, BudgetMode::WarnAndClamp);
}

#[test]
fn render_spectral_bands_active_is_hard_reject() {
    let reg = fresh_registry_with_defaults();
    let spec = reg.spec(id("render.spectral_bands_active")).unwrap();
    assert_eq!(spec.budget_mode, BudgetMode::HardReject);
}

#[test]
fn audio_master_gain_db_is_hard_reject() {
    let reg = fresh_registry_with_defaults();
    let spec = reg.spec(id("audio.master_gain_db")).unwrap();
    assert_eq!(spec.budget_mode, BudgetMode::HardReject);
}

#[test]
fn consent_sigma_check_strict_is_hard_reject_bool() {
    let reg = fresh_registry_with_defaults();
    let spec = reg.spec(id("consent.sigma_check_strict")).unwrap();
    assert_eq!(spec.kind, TunableKind::Bool);
    assert_eq!(spec.budget_mode, BudgetMode::HardReject);
}

#[test]
fn wave_dispersion_constant_is_f64() {
    let reg = fresh_registry_with_defaults();
    let spec = reg.spec(id("wave.dispersion_constant")).unwrap();
    assert_eq!(spec.kind, TunableKind::F64);
}

#[test]
fn ai_kan_band_weights_alpha_and_beta_present() {
    let reg = fresh_registry_with_defaults();
    let alpha = reg.spec(id("ai.kan_band_weight_alpha")).unwrap();
    let beta = reg.spec(id("ai.kan_band_weight_beta")).unwrap();
    assert_eq!(alpha.kind, TunableKind::F32);
    assert_eq!(beta.kind, TunableKind::F32);
}

#[test]
fn render_mise_en_abyme_recursion_cap_present() {
    let reg = fresh_registry_with_defaults();
    let spec = reg.spec(id("render.mise_en_abyme_recursion_cap")).unwrap();
    assert_eq!(spec.kind, TunableKind::U32);
    assert_eq!(spec.budget_mode, BudgetMode::HardReject);
}

#[test]
fn engine_target_frame_rate_default_is_120() {
    let reg = fresh_registry_with_defaults();
    let value = reg.read(id("engine.target_frame_rate_hz")).unwrap();
    assert_eq!(value, TunableValue::U32(120));
}

#[test]
fn install_defaults_twice_fails_with_already_registered() {
    let mut reg = TunableRegistry::new();
    install_defaults(&mut reg).unwrap();
    let err = install_defaults(&mut reg).unwrap_err();
    assert!(matches!(err, TweakError::AlreadyRegistered { .. }));
}

// ─── Cap gate ─────────────────────────────────────────────────────────────────

#[test]
fn cap_gate_blocks_imposter_token() {
    let mut reg = fresh_registry_with_defaults();
    let imposter: Cap<Tweak> = Cap::stub(CapTag("Imposter"));
    let err = reg
        .set(
            id("ai.policy_explore_rate"),
            TunableValue::F32(0.3),
            imposter,
        )
        .unwrap_err();
    assert!(matches!(err, TweakError::CapDenied { .. }));
}

#[test]
fn cap_gate_blocks_inspect_tag() {
    let mut reg = fresh_registry_with_defaults();
    let inspect: Cap<Tweak> = Cap::stub(CapTag("Inspect"));
    let err = reg
        .set(
            id("ai.policy_explore_rate"),
            TunableValue::F32(0.3),
            inspect,
        )
        .unwrap_err();
    assert!(matches!(err, TweakError::CapDenied { .. }));
}

#[test]
fn cap_gate_allows_tweak_token() {
    let mut reg = fresh_registry_with_defaults();
    let stage = reg
        .set(
            id("ai.policy_explore_rate"),
            TunableValue::F32(0.3),
            Cap::tweak(),
        )
        .unwrap();
    assert_eq!(stage, Stage::Pending);
}

#[test]
fn cap_tag_constants_match() {
    assert_eq!(Tweak::TAG, CapTag("Tweak"));
}

// ─── Frame-boundary defer ──────────────────────────────────────────────────────

#[test]
fn pre_tick_read_returns_default() {
    let mut reg = fresh_registry_with_defaults();
    reg.set(
        id("ai.policy_explore_rate"),
        TunableValue::F32(0.7),
        Cap::tweak(),
    )
    .unwrap();
    let v = reg.read(id("ai.policy_explore_rate")).unwrap();
    assert_eq!(v, TunableValue::F32(0.1));
}

#[test]
fn post_tick_read_returns_new_value() {
    let mut reg = fresh_registry_with_defaults();
    reg.set(
        id("ai.policy_explore_rate"),
        TunableValue::F32(0.7),
        Cap::tweak(),
    )
    .unwrap();
    reg.tick_frame();
    let v = reg.read(id("ai.policy_explore_rate")).unwrap();
    assert_eq!(v, TunableValue::F32(0.7));
}

#[test]
fn pending_value_visible_via_read_pending() {
    let mut reg = fresh_registry_with_defaults();
    reg.set(
        id("ai.policy_explore_rate"),
        TunableValue::F32(0.7),
        Cap::tweak(),
    )
    .unwrap();
    let pending = reg
        .read_pending(id("ai.policy_explore_rate"))
        .unwrap()
        .unwrap();
    assert_eq!(pending, TunableValue::F32(0.7));
}

#[test]
fn no_pending_when_unset() {
    let reg = fresh_registry_with_defaults();
    let pending = reg.read_pending(id("ai.policy_explore_rate")).unwrap();
    assert!(pending.is_none());
}

#[test]
fn tick_frame_returns_count_of_pending_writes() {
    let mut reg = fresh_registry_with_defaults();
    reg.set(
        id("ai.policy_explore_rate"),
        TunableValue::F32(0.3),
        Cap::tweak(),
    )
    .unwrap();
    reg.set(
        id("ai.kan_band_weight_alpha"),
        TunableValue::F32(0.7),
        Cap::tweak(),
    )
    .unwrap();
    reg.set(
        id("ai.kan_band_weight_beta"),
        TunableValue::F32(0.4),
        Cap::tweak(),
    )
    .unwrap();
    let applied = reg.tick_frame();
    assert_eq!(applied, 3);
}

#[test]
fn tick_frame_advances_logical_counter() {
    let mut reg = fresh_registry_with_defaults();
    assert_eq!(reg.frame_n(), 0);
    reg.tick_frame();
    assert_eq!(reg.frame_n(), 1);
    reg.tick_frame();
    assert_eq!(reg.frame_n(), 2);
    reg.tick_frame();
    assert_eq!(reg.frame_n(), 3);
}

#[test]
fn last_pending_write_wins() {
    let mut reg = fresh_registry_with_defaults();
    reg.set(
        id("ai.policy_explore_rate"),
        TunableValue::F32(0.3),
        Cap::tweak(),
    )
    .unwrap();
    reg.set(
        id("ai.policy_explore_rate"),
        TunableValue::F32(0.4),
        Cap::tweak(),
    )
    .unwrap();
    reg.set(
        id("ai.policy_explore_rate"),
        TunableValue::F32(0.5),
        Cap::tweak(),
    )
    .unwrap();
    reg.tick_frame();
    assert_eq!(
        reg.read(id("ai.policy_explore_rate")).unwrap(),
        TunableValue::F32(0.5)
    );
}

#[test]
fn stage_reports_pending_then_applied() {
    let mut reg = fresh_registry_with_defaults();
    reg.set(
        id("ai.policy_explore_rate"),
        TunableValue::F32(0.7),
        Cap::tweak(),
    )
    .unwrap();
    assert_eq!(
        reg.stage(id("ai.policy_explore_rate")).unwrap(),
        Stage::Pending
    );
    reg.tick_frame();
    assert_eq!(
        reg.stage(id("ai.policy_explore_rate")).unwrap(),
        Stage::Applied
    );
}

// ─── Replay log integration ────────────────────────────────────────────────────

#[test]
fn replay_log_records_each_apply() {
    let mut reg = fresh_registry_with_defaults();
    reg.set(
        id("ai.policy_explore_rate"),
        TunableValue::F32(0.3),
        Cap::tweak(),
    )
    .unwrap();
    reg.tick_frame();
    reg.set(
        id("ai.kan_band_weight_alpha"),
        TunableValue::F32(0.7),
        Cap::tweak(),
    )
    .unwrap();
    reg.tick_frame();
    assert_eq!(reg.replay_log().len(), 2);
}

#[test]
fn replay_event_carries_logical_frame_n() {
    let mut reg = fresh_registry_with_defaults();
    reg.set(
        id("ai.policy_explore_rate"),
        TunableValue::F32(0.3),
        Cap::tweak(),
    )
    .unwrap();
    reg.tick_frame();
    reg.set(
        id("ai.kan_band_weight_alpha"),
        TunableValue::F32(0.7),
        Cap::tweak(),
    )
    .unwrap();
    reg.tick_frame();
    let events = reg.replay_log().events();
    assert_eq!(events[0].frame_n, 1);
    assert_eq!(events[1].frame_n, 2);
}

#[test]
fn replay_event_carries_canonical_name() {
    let mut reg = fresh_registry_with_defaults();
    reg.set(
        id("ai.policy_explore_rate"),
        TunableValue::F32(0.3),
        Cap::tweak(),
    )
    .unwrap();
    reg.tick_frame();
    let event = &reg.replay_log().events()[0];
    assert_eq!(event.canonical_name, "ai.policy_explore_rate");
}

#[test]
fn replay_event_carries_origin_manual_by_default() {
    let mut reg = fresh_registry_with_defaults();
    reg.set(
        id("ai.policy_explore_rate"),
        TunableValue::F32(0.3),
        Cap::tweak(),
    )
    .unwrap();
    reg.tick_frame();
    let event = &reg.replay_log().events()[0];
    assert_eq!(event.origin, TweakOrigin::Manual);
}

#[test]
fn replay_event_origin_propagates_through_set_with_origin() {
    let mut reg = fresh_registry_with_defaults();
    reg.set_with_origin(
        id("ai.policy_explore_rate"),
        TunableValue::F32(0.3),
        Cap::tweak(),
        TweakOrigin::Mcp,
    )
    .unwrap();
    reg.tick_frame();
    let event = &reg.replay_log().events()[0];
    assert_eq!(event.origin, TweakOrigin::Mcp);
}

#[test]
fn replay_event_carries_post_apply_value() {
    let mut reg = fresh_registry_with_defaults();
    reg.set(
        id("ai.policy_explore_rate"),
        TunableValue::F32(0.42),
        Cap::tweak(),
    )
    .unwrap();
    reg.tick_frame();
    let event = &reg.replay_log().events()[0];
    assert_eq!(event.new_value, TunableValue::F32(0.42));
}

#[test]
fn replay_log_byte_equal_for_same_sequence() {
    fn run_session() -> Vec<TweakEvent> {
        let mut reg = fresh_registry_with_defaults();
        reg.set(
            id("ai.policy_explore_rate"),
            TunableValue::F32(0.3),
            Cap::tweak(),
        )
        .unwrap();
        reg.tick_frame();
        reg.set(
            id("ai.kan_band_weight_alpha"),
            TunableValue::F32(0.7),
            Cap::tweak(),
        )
        .unwrap();
        reg.tick_frame();
        reg.replay_log().events().to_vec()
    }
    let a = run_session();
    let b = run_session();
    assert_eq!(a, b);
}

// ─── Audit chain ───────────────────────────────────────────────────────────────

#[test]
fn audit_records_one_entry_per_apply() {
    let mut reg = fresh_registry_with_defaults();
    reg.set(
        id("ai.policy_explore_rate"),
        TunableValue::F32(0.3),
        Cap::tweak(),
    )
    .unwrap();
    reg.set(
        id("ai.kan_band_weight_alpha"),
        TunableValue::F32(0.7),
        Cap::tweak(),
    )
    .unwrap();
    reg.tick_frame();
    assert_eq!(reg.audit().len(), 2);
}

#[test]
fn audit_entry_includes_old_and_new_value() {
    let mut reg = fresh_registry_with_defaults();
    reg.set(
        id("ai.policy_explore_rate"),
        TunableValue::F32(0.42),
        Cap::tweak(),
    )
    .unwrap();
    reg.tick_frame();
    let entry = &reg.audit().entries()[0];
    assert_eq!(entry.old_value, "0.1");
    assert_eq!(entry.new_value, "0.42");
}

#[test]
fn audit_entry_records_clamp_flag_when_clamped() {
    let mut reg = fresh_registry_with_defaults();
    // foveation_aggression range is 0.0..2.0 (warn) ; 5.0 will be clamped.
    reg.set(
        id("render.foveation_aggression"),
        TunableValue::F32(5.0),
        Cap::tweak(),
    )
    .unwrap();
    reg.tick_frame();
    let entry = &reg.audit().entries()[0];
    assert!(entry.was_clamped);
}

#[test]
fn audit_entry_records_no_clamp_when_in_range() {
    let mut reg = fresh_registry_with_defaults();
    reg.set(
        id("render.foveation_aggression"),
        TunableValue::F32(1.5),
        Cap::tweak(),
    )
    .unwrap();
    reg.tick_frame();
    let entry = &reg.audit().entries()[0];
    assert!(!entry.was_clamped);
}

#[test]
fn audit_entry_records_cap_chain() {
    let mut reg = fresh_registry_with_defaults();
    reg.set(
        id("ai.policy_explore_rate"),
        TunableValue::F32(0.3),
        Cap::tweak(),
    )
    .unwrap();
    reg.tick_frame();
    let entry = &reg.audit().entries()[0];
    assert_eq!(entry.cap_chain, vec![Tweak::TAG]);
}

#[test]
fn audit_entry_records_logical_frame() {
    let mut reg = fresh_registry_with_defaults();
    reg.tick_frame();
    reg.tick_frame();
    reg.set(
        id("ai.policy_explore_rate"),
        TunableValue::F32(0.3),
        Cap::tweak(),
    )
    .unwrap();
    reg.tick_frame();
    let entry = &reg.audit().entries()[0];
    assert_eq!(entry.frame_n, 3);
}

#[test]
fn audit_seq_is_monotonic() {
    let mut reg = fresh_registry_with_defaults();
    reg.set(
        id("ai.policy_explore_rate"),
        TunableValue::F32(0.3),
        Cap::tweak(),
    )
    .unwrap();
    reg.set(
        id("ai.kan_band_weight_alpha"),
        TunableValue::F32(0.7),
        Cap::tweak(),
    )
    .unwrap();
    reg.tick_frame();
    let entries = reg.audit().entries();
    assert_eq!(entries[0].audit_seq, 0);
    assert_eq!(entries[1].audit_seq, 1);
}

// ─── Range / budget enforcement ────────────────────────────────────────────────

#[test]
fn warn_and_clamp_clamps_high() {
    let mut reg = fresh_registry_with_defaults();
    reg.set(
        id("render.foveation_aggression"),
        TunableValue::F32(99.0),
        Cap::tweak(),
    )
    .unwrap();
    reg.tick_frame();
    let v = reg.read(id("render.foveation_aggression")).unwrap();
    if let TunableValue::F32(x) = v {
        assert!(x < 2.0);
        assert!(x > 1.99);
    } else {
        panic!();
    }
}

#[test]
fn warn_and_clamp_clamps_low() {
    let mut reg = fresh_registry_with_defaults();
    reg.set(
        id("render.exposure_compensation"),
        TunableValue::F32(-99.0),
        Cap::tweak(),
    )
    .unwrap();
    reg.tick_frame();
    let v = reg.read(id("render.exposure_compensation")).unwrap();
    if let TunableValue::F32(x) = v {
        assert!((x - -4.0).abs() < f32::EPSILON);
    } else {
        panic!();
    }
}

#[test]
fn hard_reject_refuses_out_of_range_numeric() {
    let mut reg = fresh_registry_with_defaults();
    let err = reg
        .set(
            id("audio.master_gain_db"),
            TunableValue::F32(99.0),
            Cap::tweak(),
        )
        .unwrap_err();
    assert!(matches!(err, TweakError::BudgetExceeded { .. }));
}

#[test]
fn hard_reject_refuses_invalid_string_enum() {
    let mut reg = fresh_registry_with_defaults();
    let err = reg
        .set(
            id("render.tonemap_curve"),
            TunableValue::StringEnum("MysteryCurve".into()),
            Cap::tweak(),
        )
        .unwrap_err();
    assert!(matches!(err, TweakError::StringEnumInvalid { .. }));
}

#[test]
fn warn_and_clamp_falls_back_to_string_enum_invalid() {
    // audio.spatial_quality is StringEnum + WarnAndClamp ; an invalid
    // variant cannot be clamped → must surface as StringEnumInvalid.
    let mut reg = fresh_registry_with_defaults();
    let err = reg
        .set(
            id("audio.spatial_quality"),
            TunableValue::StringEnum("Quadraphonic".into()),
            Cap::tweak(),
        )
        .unwrap_err();
    assert!(matches!(err, TweakError::StringEnumInvalid { .. }));
}

#[test]
fn kind_mismatch_rejects_wrong_type() {
    let mut reg = fresh_registry_with_defaults();
    let err = reg
        .set(
            id("ai.policy_explore_rate"),
            TunableValue::U32(7),
            Cap::tweak(),
        )
        .unwrap_err();
    assert!(matches!(err, TweakError::KindMismatch { .. }));
}

#[test]
fn unknown_tunable_returns_unknown_error() {
    let reg = fresh_registry_with_defaults();
    let err = reg.read(id("nope.does_not_exist")).unwrap_err();
    assert!(matches!(err, TweakError::UnknownTunable(_)));
}

#[test]
fn write_to_unknown_tunable_rejected() {
    let mut reg = fresh_registry_with_defaults();
    let err = reg
        .set(
            id("nope.does_not_exist"),
            TunableValue::F32(1.0),
            Cap::tweak(),
        )
        .unwrap_err();
    assert!(matches!(err, TweakError::UnknownTunable(_)));
}

// ─── Replay-mode determinism hold (AP-10) ──────────────────────────────────────

#[test]
fn replay_mode_blocks_manual_writes() {
    let mut reg = fresh_registry_with_defaults();
    reg.set_replay_mode(true);
    let err = reg
        .set(
            id("ai.policy_explore_rate"),
            TunableValue::F32(0.3),
            Cap::tweak(),
        )
        .unwrap_err();
    assert_eq!(err, TweakError::ReplayDeterminismHold);
}

#[test]
fn replay_mode_blocks_mcp_writes() {
    let mut reg = fresh_registry_with_defaults();
    reg.set_replay_mode(true);
    let err = reg
        .set_with_origin(
            id("ai.policy_explore_rate"),
            TunableValue::F32(0.3),
            Cap::tweak(),
            TweakOrigin::Mcp,
        )
        .unwrap_err();
    assert_eq!(err, TweakError::ReplayDeterminismHold);
}

#[test]
fn replay_mode_allows_replay_origin_writes() {
    let mut reg = fresh_registry_with_defaults();
    reg.set_replay_mode(true);
    let stage = reg
        .set_with_origin(
            id("ai.policy_explore_rate"),
            TunableValue::F32(0.3),
            Cap::tweak(),
            TweakOrigin::Replay,
        )
        .unwrap();
    assert_eq!(stage, Stage::Pending);
}

#[test]
fn replay_mode_allows_default_origin_writes() {
    let mut reg = fresh_registry_with_defaults();
    reg.set_replay_mode(true);
    let stage = reg
        .reset(id("ai.policy_explore_rate"), Cap::tweak())
        .unwrap();
    assert_eq!(stage, Stage::Pending);
}

#[test]
fn replay_mode_toggle_off_restores_manual_writes() {
    let mut reg = fresh_registry_with_defaults();
    reg.set_replay_mode(true);
    reg.set_replay_mode(false);
    let stage = reg
        .set(
            id("ai.policy_explore_rate"),
            TunableValue::F32(0.3),
            Cap::tweak(),
        )
        .unwrap();
    assert_eq!(stage, Stage::Pending);
}

#[test]
fn replay_mode_query_round_trip() {
    let mut reg = fresh_registry_with_defaults();
    assert!(!reg.is_replay_mode());
    reg.set_replay_mode(true);
    assert!(reg.is_replay_mode());
    reg.set_replay_mode(false);
    assert!(!reg.is_replay_mode());
}

// ─── Reset semantics ───────────────────────────────────────────────────────────

#[test]
fn reset_restores_default_value() {
    let mut reg = fresh_registry_with_defaults();
    reg.set(
        id("ai.policy_explore_rate"),
        TunableValue::F32(0.9),
        Cap::tweak(),
    )
    .unwrap();
    reg.tick_frame();
    reg.reset(id("ai.policy_explore_rate"), Cap::tweak())
        .unwrap();
    reg.tick_frame();
    assert_eq!(
        reg.read(id("ai.policy_explore_rate")).unwrap(),
        TunableValue::F32(0.1)
    );
}

#[test]
fn reset_without_cap_denied() {
    let mut reg = fresh_registry_with_defaults();
    let err = reg
        .reset(id("ai.policy_explore_rate"), Cap::stub(CapTag("Imposter")))
        .unwrap_err();
    assert!(matches!(err, TweakError::CapDenied { .. }));
}

// ─── Registry lifecycle ────────────────────────────────────────────────────────

#[test]
fn registry_default_constructor() {
    let reg = TunableRegistry::default();
    assert!(reg.is_empty());
}

#[test]
fn closed_registry_refuses_registrations() {
    let mut reg = fresh_registry_with_defaults();
    reg.close();
    assert!(reg.is_closed());
    let extra = TunableSpec {
        canonical_name: "extra.tunable",
        kind: TunableKind::F32,
        range: TunableRange::F32(0.0..1.0),
        default: TunableValue::F32(0.5),
        budget_mode: BudgetMode::WarnAndClamp,
        description: "",
        units: None,
        frame_boundary_defer: true,
    };
    let err = reg.register(extra).unwrap_err();
    assert_eq!(err, TweakError::RegistryClosed);
}

#[test]
fn iter_yields_all_registered() {
    let reg = fresh_registry_with_defaults();
    let count = reg.iter().count();
    assert_eq!(count, 30);
}

#[test]
fn register_default_out_of_range_rejected() {
    let mut reg = TunableRegistry::new();
    let bad = TunableSpec {
        canonical_name: "bad.tunable",
        kind: TunableKind::F32,
        range: TunableRange::F32(0.0..1.0),
        default: TunableValue::F32(5.0),
        budget_mode: BudgetMode::WarnAndClamp,
        description: "",
        units: None,
        frame_boundary_defer: true,
    };
    let err = reg.register(bad).unwrap_err();
    assert!(matches!(err, TweakError::DefaultOutOfRange { .. }));
}

#[test]
fn register_kind_default_mismatch_rejected() {
    let mut reg = TunableRegistry::new();
    let bad = TunableSpec {
        canonical_name: "bad.kind",
        kind: TunableKind::F32,
        range: TunableRange::F32(0.0..1.0),
        default: TunableValue::U32(0),
        budget_mode: BudgetMode::WarnAndClamp,
        description: "",
        units: None,
        frame_boundary_defer: true,
    };
    let err = reg.register(bad).unwrap_err();
    assert!(matches!(err, TweakError::KindMismatch { .. }));
}

#[test]
fn register_kind_range_mismatch_rejected() {
    let mut reg = TunableRegistry::new();
    let bad = TunableSpec {
        canonical_name: "bad.range",
        kind: TunableKind::F32,
        range: TunableRange::U32(0..10),
        default: TunableValue::F32(0.5),
        budget_mode: BudgetMode::WarnAndClamp,
        description: "",
        units: None,
        frame_boundary_defer: true,
    };
    let err = reg.register(bad).unwrap_err();
    assert!(matches!(err, TweakError::KindMismatch { .. }));
}

// ─── End-to-end iteration-loop scenario ────────────────────────────────────────

#[test]
fn end_to_end_session_replay_byte_equal() {
    fn run() -> (Vec<TweakAuditEntry>, Vec<TweakEvent>) {
        let mut reg = fresh_registry_with_defaults();
        // t=1 : agent observes default explore-rate = 0.1.
        let pre = reg.read(id("ai.policy_explore_rate")).unwrap();
        assert_eq!(pre, TunableValue::F32(0.1));
        // t=3 : agent calls tweak.set(ai.policy_explore_rate, 0.2)
        reg.set_with_origin(
            id("ai.policy_explore_rate"),
            TunableValue::F32(0.2),
            Cap::tweak(),
            TweakOrigin::Mcp,
        )
        .unwrap();
        reg.tick_frame();
        // t=5 : agent records the replay
        (
            reg.audit().entries().to_vec(),
            reg.replay_log().events().to_vec(),
        )
    }
    let a = run();
    let b = run();
    assert_eq!(a.0, b.0);
    assert_eq!(a.1, b.1);
}

// ─── AuditSink + ReplayLog APIs ────────────────────────────────────────────────

#[test]
fn audit_sink_starts_empty() {
    let sink = AuditSink::new();
    assert!(sink.is_empty());
    assert_eq!(sink.len(), 0);
}

#[test]
fn audit_sink_records_entries_with_seq() {
    let mut sink = AuditSink::new();
    sink.record(TweakAuditEntry {
        frame_n: 1,
        audit_seq: 0,
        tunable_id: TunableId::of("a"),
        canonical_name: "a",
        old_value: "0".into(),
        new_value: "1".into(),
        was_clamped: false,
        cap_chain: vec![Tweak::TAG],
        origin: TweakOrigin::Manual,
    });
    sink.record(TweakAuditEntry {
        frame_n: 2,
        audit_seq: 0,
        tunable_id: TunableId::of("b"),
        canonical_name: "b",
        old_value: "1".into(),
        new_value: "2".into(),
        was_clamped: false,
        cap_chain: vec![Tweak::TAG],
        origin: TweakOrigin::Manual,
    });
    let entries = sink.entries();
    assert_eq!(entries[0].audit_seq, 0);
    assert_eq!(entries[1].audit_seq, 1);
}

#[test]
fn replay_log_starts_empty() {
    let log = ReplayLog::new();
    assert!(log.is_empty());
    assert_eq!(log.len(), 0);
}

#[test]
fn replay_log_records_events() {
    let mut log = ReplayLog::new();
    log.push(TweakEvent {
        frame_n: 1,
        tunable_id: TunableId::of("a"),
        canonical_name: "a",
        new_value: TunableValue::F32(0.5),
        origin: TweakOrigin::Mcp,
    });
    assert_eq!(log.len(), 1);
}

// ─── Per-domain bound checks for every safety-critical default ─────────────────

#[test]
fn collision_eps_hard_rejects_below_min() {
    let mut reg = fresh_registry_with_defaults();
    let err = reg
        .set(
            id("physics.collision_eps"),
            TunableValue::F32(1e-9),
            Cap::tweak(),
        )
        .unwrap_err();
    assert!(matches!(err, TweakError::BudgetExceeded { .. }));
}

#[test]
fn cap_budget_strict_hard_reject_kind_mismatch() {
    let mut reg = fresh_registry_with_defaults();
    // Bool with mismatched kind is rejected by KindMismatch, not BudgetExceeded.
    let err = reg
        .set(
            id("engine.cap_budget_strict"),
            TunableValue::U32(1),
            Cap::tweak(),
        )
        .unwrap_err();
    assert!(matches!(err, TweakError::KindMismatch { .. }));
}

#[test]
fn cap_budget_strict_accepts_bool_value() {
    let mut reg = fresh_registry_with_defaults();
    let stage = reg
        .set(
            id("engine.cap_budget_strict"),
            TunableValue::Bool(false),
            Cap::tweak(),
        )
        .unwrap();
    assert_eq!(stage, Stage::Pending);
    reg.tick_frame();
    assert_eq!(
        reg.read(id("engine.cap_budget_strict")).unwrap(),
        TunableValue::Bool(false)
    );
}

#[test]
fn audit_egress_buffer_ms_hard_rejects_above_max() {
    let mut reg = fresh_registry_with_defaults();
    let err = reg
        .set(
            id("consent.audit_egress_buffer_ms"),
            TunableValue::U32(5000),
            Cap::tweak(),
        )
        .unwrap_err();
    assert!(matches!(err, TweakError::BudgetExceeded { .. }));
}

#[test]
fn replay_record_quality_accepts_lossless() {
    let mut reg = fresh_registry_with_defaults();
    let stage = reg
        .set(
            id("engine.replay_record_quality"),
            TunableValue::StringEnum("Lossless".into()),
            Cap::tweak(),
        )
        .unwrap();
    assert_eq!(stage, Stage::Pending);
}

#[test]
fn render_tonemap_curve_accepts_aces() {
    let mut reg = fresh_registry_with_defaults();
    let stage = reg
        .set(
            id("render.tonemap_curve"),
            TunableValue::StringEnum("ACES".into()),
            Cap::tweak(),
        )
        .unwrap();
    assert_eq!(stage, Stage::Pending);
}

#[test]
fn render_mise_en_abyme_recursion_cap_rejects_over_32() {
    let mut reg = fresh_registry_with_defaults();
    let err = reg
        .set(
            id("render.mise_en_abyme_recursion_cap"),
            TunableValue::U32(99),
            Cap::tweak(),
        )
        .unwrap_err();
    assert!(matches!(err, TweakError::BudgetExceeded { .. }));
}

#[test]
fn warn_clamps_do_not_appear_as_errors() {
    let mut reg = fresh_registry_with_defaults();
    // Every WarnAndClamp default should accept extreme inputs without error.
    let warn_specs: Vec<_> = default_tunable_specs()
        .into_iter()
        .filter(|s| {
            s.budget_mode == BudgetMode::WarnAndClamp
                && !matches!(s.range, TunableRange::StringEnum(_))
                && !matches!(s.range, TunableRange::Bool)
        })
        .collect();
    for spec in warn_specs {
        let extreme = match &spec.range {
            TunableRange::F32(_) => TunableValue::F32(f32::MAX),
            TunableRange::F64(_) => TunableValue::F64(f64::MAX),
            TunableRange::U32(_) => TunableValue::U32(u32::MAX),
            TunableRange::U64(_) => TunableValue::U64(u64::MAX),
            TunableRange::I32(_) => TunableValue::I32(i32::MAX),
            TunableRange::I64(_) => TunableValue::I64(i64::MAX),
            _ => continue,
        };
        let stage = reg.set(spec.id(), extreme, Cap::tweak()).unwrap();
        assert_eq!(stage, Stage::Pending, "{}", spec.canonical_name);
    }
}

#[test]
fn read_pending_for_unknown_id() {
    let reg = fresh_registry_with_defaults();
    let err = reg.read_pending(id("nope")).unwrap_err();
    assert!(matches!(err, TweakError::UnknownTunable(_)));
}

#[test]
fn stage_for_unknown_id() {
    let reg = fresh_registry_with_defaults();
    let err = reg.stage(id("nope")).unwrap_err();
    assert!(matches!(err, TweakError::UnknownTunable(_)));
}

#[test]
fn spec_for_unknown_id() {
    let reg = fresh_registry_with_defaults();
    let err = reg.spec(id("nope")).unwrap_err();
    assert!(matches!(err, TweakError::UnknownTunable(_)));
}

#[test]
fn cap_tweak_constructor_returns_correct_tag() {
    let cap = Cap::<Tweak>::tweak();
    assert_eq!(cap.tag, Tweak::TAG);
}

#[test]
fn five_of_five_acceptance_proof() {
    let mut reg = fresh_registry_with_defaults();
    // 1. 30 default-tunables registered.
    assert_eq!(reg.len(), 30);
    // 2. Cap<Tweak> stub-gate enforced.
    let imposter: Cap<Tweak> = Cap::stub(CapTag("X"));
    assert!(matches!(
        reg.set(
            id("ai.policy_explore_rate"),
            TunableValue::F32(0.3),
            imposter
        )
        .unwrap_err(),
        TweakError::CapDenied { .. }
    ));
    // 3. Frame-boundary defer.
    reg.set(
        id("ai.policy_explore_rate"),
        TunableValue::F32(0.3),
        Cap::tweak(),
    )
    .unwrap();
    let pre = reg.read(id("ai.policy_explore_rate")).unwrap();
    assert_eq!(pre, TunableValue::F32(0.1));
    reg.tick_frame();
    let post = reg.read(id("ai.policy_explore_rate")).unwrap();
    assert_eq!(post, TunableValue::F32(0.3));
    // 4. Replay-log records tweak with logical-frame-N.
    let event = &reg.replay_log().events()[0];
    assert_eq!(event.frame_n, 1);
    // 5. Audit chain records old/new with cap-chain.
    let audit = &reg.audit().entries()[0];
    assert_eq!(audit.cap_chain, vec![Tweak::TAG]);
    assert_eq!(audit.old_value, "0.1");
    assert_eq!(audit.new_value, "0.3");
}
