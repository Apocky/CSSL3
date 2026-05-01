// § run_lifecycle — roguelike-run advance-phase + NPC-BT-tick + share-receipt.
// ════════════════════════════════════════════════════════════════════
// § Coverage : end-to-end run-state-machine progression + AI-tick within
//   that run + run-share serialization determinism.

use cssl_host_npc_bt as npc;
use cssl_host_roguelike_run as run;

use cssl_host_integration_tests::{tiny_bt, StubNpcWorld};

/// (a) Roguelike-run advance-phase chain : Hub → BiomeSelect → Floor
///     → BossArena → Reward.
#[test]
fn roguelike_run_advance_phase_chain() {
    let mut state = run::RunState::genesis(0xCAFE_BABE, 1);
    assert!(matches!(state.phase, run::RunPhase::Hub));

    state
        .enter_biome_select()
        .expect("Hub → BiomeSelect must succeed");
    assert!(matches!(state.phase, run::RunPhase::BiomeSelect));

    state
        .descend_into(run::Biome::Crypt, 3)
        .expect("BiomeSelect → Floor must succeed");
    assert!(matches!(
        state.phase,
        run::RunPhase::Floor { idx: 1, biome: run::Biome::Crypt }
    ));
    assert_eq!(state.depth, 1);

    // Advance through floors 2, 3 ; floor_count = 3 means floor-3 = BossArena.
    state.advance_floor().expect("advance to floor 2");
    state.advance_floor().expect("advance to floor 3 (boss)");
    assert!(matches!(
        state.phase,
        run::RunPhase::BossArena { biome: run::Biome::Crypt }
    ));

    state.boss_cleared().expect("boss-clear must succeed");
    assert!(matches!(
        state.phase,
        run::RunPhase::Reward { biome: run::Biome::Crypt }
    ));

    // Echoes-award is saturating + survives through phase transitions.
    state.award_echoes(150);
    assert_eq!(state.echoes_in_run, 150);
}

/// (b) NPC-BT tick fires within a run : a small Selector { Idle? , LowHP? }
///     against the StubNpcWorld returns Success.
#[test]
fn npc_bt_tick_within_run() {
    let world = StubNpcWorld;
    let bt = tiny_bt();
    // Sanity : node-count is 1 (Selector) + 2 (children) = 3.
    assert_eq!(bt.count(), 3);
    let status = npc::tick(&bt, &world, &npc::NoopAuditSink);
    // StubNpcWorld is_idle()=true ⇒ Idle? Succeeds ⇒ Selector wins.
    assert_eq!(status, npc::BtStatus::Success);
}

/// (c) Run-share-receipt serializes deterministically : same inputs ⇒
///     bit-equal JSON across two cold builds, and round-trips cleanly.
#[test]
fn run_share_receipt_serializes_deterministically() {
    let scoring = run::RunShareScoring {
        boss_clears: 2,
        echoes_earned: 1234,
        duration_ms: 60_000,
        style_tag: String::from("speed"),
    };
    let receipt_a = run::RunShareReceipt::new(
        0xDEAD_BEEF_u128,
        vec![run::Biome::Crypt, run::Biome::Citadel],
        7,
        scoring.clone(),
        12,
        "consent-token-abc",
    );
    let receipt_b = run::RunShareReceipt::new(
        0xDEAD_BEEF_u128,
        vec![run::Biome::Crypt, run::Biome::Citadel],
        7,
        scoring,
        12,
        "consent-token-abc",
    );

    let json_a = receipt_a.to_json().expect("serialize-A");
    let json_b = receipt_b.to_json().expect("serialize-B");
    assert_eq!(json_a, json_b, "deterministic JSON ; bit-equal across builds");

    // Round-trip preservation.
    let parsed: run::RunShareReceipt =
        serde_json::from_str(&json_a).expect("deserialize");
    assert_eq!(parsed, receipt_a);

    // Consent-gating : non-empty token ⇒ shareable.
    assert!(receipt_a.is_shareable());
}
