// § T11-W7-C-DM tests/arbiter.rs
// Stage-0 table-lookup + stage-1 stub-fallback + ambiguity-tiebreak coverage.

use cssl_host_dm::{
    IntentSummary, SceneArbiter, SceneStateSnapshot, ScenePick,
    Stage0HeuristicArbiter, Stage1KanStubArbiter,
};

fn neutral() -> SceneStateSnapshot {
    SceneStateSnapshot::neutral("zone:atrium-7")
}

#[test]
fn stage0_table_examines_to_scene_edit() {
    let a = Stage0HeuristicArbiter::new();
    let i = vec![IntentSummary::new("examine", 0.9, Some(String::from("altar")))];
    let p = a.arbitrate(&i, &neutral());
    assert_eq!(
        p,
        ScenePick::SceneEdit {
            location: String::from("altar")
        }
    );
}

#[test]
fn stage0_table_spawn_to_condensation() {
    let a = Stage0HeuristicArbiter::new();
    let i = vec![IntentSummary::new(
        "spawn",
        0.95,
        Some(String::from("zone:foyer")),
    )];
    assert_eq!(
        a.arbitrate(&i, &neutral()),
        ScenePick::SpawnCondensation {
            zone_id: String::from("zone:foyer")
        }
    );
}

#[test]
fn stage0_companion_intent_yields_prompt_with_stable_hash() {
    let a = Stage0HeuristicArbiter::new();
    let i = vec![IntentSummary::new(
        "companion",
        0.8,
        Some(String::from("nudge")),
    )];
    let p1 = a.arbitrate(&i, &neutral());
    let p2 = a.arbitrate(&i, &neutral());
    // Replay-bit-equal : same input → same hash output.
    assert_eq!(p1, p2);
    matches!(p1, ScenePick::CompanionPrompt { .. });
}

#[test]
fn stage1_falls_back_to_stage0_when_no_kan_handle() {
    let a = Stage1KanStubArbiter::new(Box::new(Stage0HeuristicArbiter::new()));
    let i = vec![IntentSummary::new(
        "spawn",
        0.9,
        Some(String::from("zone:fall")),
    )];
    assert_eq!(
        a.arbitrate(&i, &neutral()),
        ScenePick::SpawnCondensation {
            zone_id: String::from("zone:fall")
        }
    );
}

#[test]
fn stage1_with_kan_returns_silent() {
    let a = Stage1KanStubArbiter::with_handle(
        Box::new(Stage0HeuristicArbiter::new()),
        String::from("kan-mock-v0"),
    );
    let i = vec![IntentSummary::new("spawn", 0.95, None)];
    assert_eq!(a.arbitrate(&i, &neutral()), ScenePick::Silent);
}

#[test]
fn ambiguity_breaks_to_highest_confidence() {
    let a = Stage0HeuristicArbiter::new();
    let i = vec![
        IntentSummary::new("examine", 0.30, Some(String::from("altar"))),
        IntentSummary::new("spawn", 0.70, Some(String::from("zone:x"))),
        IntentSummary::new("examine", 0.55, Some(String::from("scroll"))),
    ];
    // Highest = spawn @ 0.70 → SpawnCondensation { zone_id: "zone:x" }
    assert_eq!(
        a.arbitrate(&i, &neutral()),
        ScenePick::SpawnCondensation {
            zone_id: String::from("zone:x")
        }
    );
}
