// § T11-W8-E1 : season-extension integration tests
// ════════════════════════════════════════════════════════════════════
// § I> public-API surface tests for SeasonMode/Hard-perma/memorial-imprint.
// § I> structural-permadeath + meta-isolation + gift-economy invariants.
// § I> NO leaderboards · cosmetic-channel-only-axiom.
// ════════════════════════════════════════════════════════════════════

use cssl_host_roguelike_run::{
    apply_seasonal_permadeath, dispatch_season_end_memorials, DeathCause, MockMemorialDispatcher,
    RunPhase, RunState, SeasonCharacter, SeasonId, SeasonMetaProgress, SeasonMode,
    SEASON_CYCLE_DAYS,
};

#[test]
fn season_cycle_is_90_days() {
    assert_eq!(SEASON_CYCLE_DAYS, 90);
}

#[test]
fn run_state_genesis_with_season_marks_hard_perma() {
    let s = RunState::genesis_with_season(0xDEAD, 1, SeasonId(42), SeasonMode::Hard);
    assert!(s.is_hard_perma());
    assert_eq!(s.season_id, Some(SeasonId(42)));
    assert_eq!(s.season_mode, SeasonMode::Hard);
}

#[test]
fn default_genesis_is_not_hard_perma() {
    let s = RunState::genesis(0xDEAD, 1);
    assert!(!s.is_hard_perma());
    assert_eq!(s.season_mode, SeasonMode::Soft);
    assert_eq!(s.season_id, None);
}

#[test]
fn end_to_end_seasonal_run_through_memorial() {
    // 1. Genesis hard-perma run.
    let mut state = RunState::genesis_with_season(0xCAFE, 7, SeasonId(3), SeasonMode::Hard);
    let mut character = SeasonCharacter::new(
        "char-uuid-aaa",
        "pubkey-hex",
        SeasonId(3),
        SeasonMode::Hard,
    );

    // 2. Run accumulates echoes.
    state.echoes_in_run = 1500;

    // 3. Death event.
    apply_seasonal_permadeath(
        &mut state,
        &mut character,
        DeathCause::NemesisDefeat,
        0.62,
        "Lord Argaroth bested them at Floor 4",
    )
    .unwrap();

    // 4. Permadeath structural-invariants hold.
    assert_eq!(state.phase, RunPhase::Death);
    assert_eq!(state.echoes_in_run, 0);
    assert!(!character.alive);

    // 5. Hard-perma revival is structurally forbidden.
    assert!(character.try_revive().is_err());

    // 6. Season-end memorial-imprint dispatches.
    let mut dispatcher = MockMemorialDispatcher::default();
    let imprint_ids =
        dispatch_season_end_memorials(&[character.clone()], &mut dispatcher, Some("pubkey-hex"))
            .unwrap();
    assert_eq!(imprint_ids.len(), 1);
    assert_eq!(dispatcher.imprints.len(), 1);
    assert!(imprint_ids[0].contains("char-uuid-aaa"));

    // 7. NO leaderboard data attached (gift-economy invariant).
    let (_char_id, _attribution, imprint_id) = &dispatcher.imprints[0];
    // Recorded-tuple has 3 fields, not 4 ; no rank/score-position embedded.
    assert!(!imprint_id.is_empty());
}

#[test]
fn meta_isolation_soft_track_unaffected_by_hard_grinding() {
    let mut meta = SeasonMetaProgress::default();
    // Hard-mode death-grinding 1000 hard-echoes.
    for _ in 0..10 {
        meta.deposit(SeasonMode::Hard, 100);
    }
    // Soft track is zero ← hard grinding does NOT pollute soft track.
    assert_eq!(meta.echoes_for(SeasonMode::Soft), 0);
    assert_eq!(meta.echoes_for(SeasonMode::Hard), 1000);
}

#[test]
fn season_state_serde_round_trip_preserves_mode() {
    let s = RunState::genesis_with_season(1, 2, SeasonId(99), SeasonMode::Hard);
    let json = serde_json::to_string(&s).unwrap();
    let back: RunState = serde_json::from_str(&json).unwrap();
    assert_eq!(back.season_id, Some(SeasonId(99)));
    assert_eq!(back.season_mode, SeasonMode::Hard);
    assert!(back.is_hard_perma());
}

#[test]
fn legacy_run_state_json_deserializes_with_default_season_fields() {
    // Legacy serialization (before W8-E1) lacked season_id + season_mode.
    // serde(default) must permit forward-compat reads.
    let legacy_json = r#"{
        "phase": "Hub",
        "current_biome": null,
        "floor_count": 3,
        "depth": 0,
        "echoes_pre": 0,
        "echoes_in_run": 0,
        "run_id": 1,
        "seed": 0
    }"#;
    let s: RunState = serde_json::from_str(legacy_json).unwrap();
    assert_eq!(s.season_id, None);
    assert_eq!(s.season_mode, SeasonMode::Soft);
    assert!(!s.is_hard_perma());
}
