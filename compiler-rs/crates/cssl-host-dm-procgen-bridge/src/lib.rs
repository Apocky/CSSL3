//! § cssl-host-dm-procgen-bridge — wire DMGM-specialists ↔ procgen-pipeline.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   THIN ADAPTER between two foundation crates :
//!
//!   ```text
//!   ┌────────────────────────────────┐    ┌────────────────────────────────┐
//!   │ cssl-host-dmgm-specialists     │    │ cssl-host-procgen-pipeline     │
//!   │   - SpecialistRole (4 roles)   │    │   - IntentSemantic (4 verbs)   │
//!   │   - Decision enum              │ →  │   - ProcgenRequest             │
//!   │   - role_council mediator      │    │   - generate (BLAKE3-seeded)   │
//!   │   - 4 Specialist impls         │    │   - ProcgenOutput              │
//!   └────────────────────────────────┘    └────────────────────────────────┘
//!                    │                                       ▲
//!                    │                                       │
//!                    │   ┌──────────────────────────────┐    │
//!                    └──→│ cssl-host-dm-procgen-bridge  │────┘
//!                        │  decision_to_procgen_request │
//!                        │  council_to_procgen           │
//!                        │  run_council_and_generate     │
//!                        └──────────────────────────────┘
//!   ```
//!
//! § APOCKY-DIRECTIVE (verbatim · feedback_dm_gm_not_general_ai.md)
//!   "engine intelligence scope = DM (orchestrator) + GM (narrator) +
//!    Collaborator (co-author) + Coder (runtime-mutate) ONLY · self-sufficient
//!    for those roles · NOT generic AGI · reuse Cognitive-Field-Engine
//!    substrate AS PLUMBING for DM-specialist scenes"
//!
//! § PURE-FN ENTRYPOINTS
//!   - [`decision_to_procgen_request`] — map ONE Decision → ProcgenRequest.
//!   - [`council_to_procgen`]          — mediate decisions → ProcgenRequest.
//!   - [`run_council_and_generate`]    — end-to-end specialists → output.
//!
//! § ACTION-ID NAMESPACES (mirrors `dmgm-specialists::decide_inner`)
//!   - `0x1000-0x1FFF`  DM   → CreateEntity (orchestrator-driven)
//!   - `0x2000-0x2FFF`  GM   → CreateEntity (narration-anchor entity)
//!   - `0x3000-0x3FFF`  Coll → ModifyEntity (co-author refines existing)
//!   - `0x4000-0x4FFF`  Coder→ ModifyEntity (runtime-mutate target)
//!
//!   The bridge translates each role's action-id into the appropriate
//!   IntentSemantic verb. Question/Pass decisions yield `None` since
//!   they don't map to procgen-mutations (Question is "ask the player",
//!   Pass is "abstain").
//!
//! § DETERMINISM
//!   All three fns are pure : no global state, no clock, no rng. The
//!   underlying procgen-pipeline `generate` is itself BLAKE3-seeded ;
//!   same inputs → bit-identical outputs across hosts.
//!
//! § PRIME-DIRECTIVE alignment
//!   - No I/O. No network. No LLM. No GPU. Pure CPU translation.
//!   - No Sensitive<*> ; no PII ; no telemetry.
//!   - Same prompt-hash + same specialists + same observer_pos →
//!     bit-identical ProcgenOutput. Replay-determinism preserved.

#![forbid(unsafe_code)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::must_use_candidate)]

use cssl_host_dmgm_specialists::{
    role_council, Decision, Specialist, SpecialistRole,
};
use cssl_host_procgen_pipeline::{
    generate, IntentSemantic, ObserverCoord, ProcgenOutput, ProcgenRequest,
};

// ════════════════════════════════════════════════════════════════════════════
// § Constants — default budget for the consensus-bridge entrypoint
// ════════════════════════════════════════════════════════════════════════════

/// Default budget passed to [`council_to_procgen`] / [`run_council_and_generate`].
///
/// 16 ms = one 60Hz frame ; aligns with the runtime-procgen "fits in a
/// frame" expectation. Callers wanting tighter or wider slices should
/// use [`decision_to_procgen_request`] directly with an explicit budget.
pub const DEFAULT_BUDGET_MS: u32 = 16;

// ════════════════════════════════════════════════════════════════════════════
// § decision_to_procgen_request — single-Decision → ProcgenRequest
// ════════════════════════════════════════════════════════════════════════════

/// Map a single [`Decision`] into a [`ProcgenRequest`].
///
/// § ROUTING
///   - `ProposeAction { id, params }`
///       - id ∈ `0x1000-0x1FFF` (DM)   → `CreateEntity { kind: "dm-arc", ... }`
///       - id ∈ `0x2000-0x2FFF` (GM)   → `CreateEntity { kind: "gm-narration", ... }`
///       - id ∈ `0x3000-0x3FFF` (Coll) → `ModifyEntity { target: "scene", ... }`
///       - id ∈ `0x4000-0x4FFF` (Coder)→ `ModifyEntity { target: "engine", ... }`
///       - id outside any role-bracket → `Other(<hex>)` (LLM-bridge slot)
///   - `Question { hash }`              → `None` (not procgen-actionable)
///   - `Pass`                           → `None` (specialist abstained)
///
/// § DETERMINISM
///   Pure fn ; no allocation outside the returned struct's heap members.
///   Same inputs → bit-identical output. Stage-0 stand-in for the F1
///   LLM-bridge slice that will widen the verb-derivation downstream.
///
/// § PARAMS-ENCODING
///   We pin the 8 bytes of `params` (zero-padded) into the IntentSemantic's
///   props as `(b"action_params", "<hex>")` so the BLAKE3 fingerprint
///   downstream incorporates them. This means two different Decisions with
///   the same id but different params produce DIFFERENT ProcgenOutput.
#[must_use]
pub fn decision_to_procgen_request(
    decision: &Decision,
    observer_pos: ObserverCoord,
    budget_ms: u32,
) -> Option<ProcgenRequest> {
    let semantic = decision_to_intent_semantic(decision)?;
    Some(ProcgenRequest {
        semantic,
        observer_pos,
        budget_ms,
    })
}

/// Helper : map Decision → IntentSemantic. Pulled out so council_to_procgen
/// and run_council_and_generate can share the routing logic.
fn decision_to_intent_semantic(decision: &Decision) -> Option<IntentSemantic> {
    match decision {
        Decision::Pass | Decision::Question { .. } => None,
        Decision::ProposeAction { id, params } => {
            let role = role_for_action_id(*id);
            let params_hex = encode_params_hex(params);
            Some(intent_for_role(role, *id, &params_hex))
        }
    }
}

/// Identify which specialist's bracket an action-id falls into.
///
/// Returns `None` when the id is outside all four canonical brackets ;
/// callers in the bridge then route to `IntentSemantic::Other` to
/// preserve the original blob through the procgen fingerprint.
#[must_use]
pub fn role_for_action_id(id: u32) -> Option<SpecialistRole> {
    match id & 0xF000 {
        0x1000 => Some(SpecialistRole::DungeonMaster),
        0x2000 => Some(SpecialistRole::GameMaster),
        0x3000 => Some(SpecialistRole::Collaborator),
        0x4000 => Some(SpecialistRole::Coder),
        _ => None,
    }
}

/// Lower-case hex of params bytes. Stable, deterministic, no allocation
/// beyond the result String.
fn encode_params_hex(params: &[u8]) -> String {
    let mut out = String::with_capacity(params.len() * 2);
    for b in params {
        // Manual hex-encode (no `format!` per byte for a measurable
        // micro-savings ; this fn is hot in the council path).
        out.push(hex_digit(b >> 4));
        out.push(hex_digit(b & 0x0F));
    }
    out
}

const fn hex_digit(nibble: u8) -> char {
    match nibble {
        0..=9 => (b'0' + nibble) as char,
        10..=15 => (b'a' + (nibble - 10)) as char,
        _ => '?',
    }
}

/// Build the role-appropriate IntentSemantic for a given action-id + params.
///
/// - DM/GM   → CreateEntity (orchestrator + narrator both spawn content).
/// - Coll/Coder → ModifyEntity (co-author + runtime-mutate both refine).
/// - None    → Other (preserves the blob for the F1 LLM-bridge to retry).
fn intent_for_role(
    role: Option<SpecialistRole>,
    id: u32,
    params_hex: &str,
) -> IntentSemantic {
    let id_hex = format!("{id:08x}");
    match role {
        Some(SpecialistRole::DungeonMaster) => IntentSemantic::CreateEntity {
            kind: "dm-arc".to_string(),
            props: build_props(&id_hex, params_hex),
        },
        Some(SpecialistRole::GameMaster) => IntentSemantic::CreateEntity {
            kind: "gm-narration".to_string(),
            props: build_props(&id_hex, params_hex),
        },
        Some(SpecialistRole::Collaborator) => IntentSemantic::ModifyEntity {
            target: "scene".to_string(),
            props: build_props(&id_hex, params_hex),
        },
        Some(SpecialistRole::Coder) => IntentSemantic::ModifyEntity {
            target: "engine".to_string(),
            props: build_props(&id_hex, params_hex),
        },
        // Out-of-bracket → preserve the action-id+params blob for the
        // LLM-bridge slot to retry. The procgen-pipeline currently treats
        // Other as 0-crystal + 0-uri ; it still yields a stable fingerprint.
        None => IntentSemantic::Other(format!("action:{id_hex}/{params_hex}")),
    }
}

fn build_props(id_hex: &str, params_hex: &str) -> Vec<(String, String)> {
    vec![
        ("action_id".to_string(), id_hex.to_string()),
        ("action_params".to_string(), params_hex.to_string()),
    ]
}

// ════════════════════════════════════════════════════════════════════════════
// § council_to_procgen — mediate decisions → ProcgenRequest
// ════════════════════════════════════════════════════════════════════════════

/// Run [`role_council`] over the input slice, then map the consensus
/// Decision into a [`ProcgenRequest`] with [`DEFAULT_BUDGET_MS`].
///
/// Returns `None` when the council mediates to `Pass` or `Question`
/// (neither is procgen-actionable). Returns `Some(ProcgenRequest)` when
/// the council picks a `ProposeAction`.
///
/// § DETERMINISM
///   `role_council` is itself deterministic ; this fn adds no new
///   non-determinism. Identical input slice → identical output.
#[must_use]
pub fn council_to_procgen(
    decisions: &[Decision],
    observer_pos: ObserverCoord,
) -> Option<ProcgenRequest> {
    let consensus = role_council(decisions);
    decision_to_procgen_request(&consensus, observer_pos, DEFAULT_BUDGET_MS)
}

// ════════════════════════════════════════════════════════════════════════════
// § run_council_and_generate — end-to-end specialists → output
// ════════════════════════════════════════════════════════════════════════════

/// End-to-end : ask each [`Specialist`] for a Decision, mediate via
/// [`role_council`], map consensus to a [`ProcgenRequest`], call
/// procgen-pipeline [`generate`].
///
/// Returns `None` when the council mediates to a non-actionable decision
/// (Pass / Question), `Some(ProcgenOutput)` otherwise.
///
/// § DETERMINISM
///   - `Specialist::decide` is deterministic in `prompt_hash` (per
///     dmgm-specialists doc).
///   - `role_council` is deterministic.
///   - `generate` is deterministic (BLAKE3 fingerprint).
///   ⇒ end-to-end deterministic. Same `(specialists, prompt_hash, observer_pos)`
///     → bit-identical ProcgenOutput.
///
/// § BUDGET
///   Uses [`DEFAULT_BUDGET_MS`]. Callers wanting custom budgets should
///   compose [`decision_to_procgen_request`] manually.
#[must_use]
pub fn run_council_and_generate(
    specialists: &[&dyn Specialist],
    prompt_hash: u64,
    observer_pos: ObserverCoord,
) -> Option<ProcgenOutput> {
    if specialists.is_empty() {
        return None;
    }
    let decisions: Vec<Decision> =
        specialists.iter().map(|s| s.decide(prompt_hash)).collect();
    let req = council_to_procgen(&decisions, observer_pos)?;
    Some(generate(&req))
}

// ════════════════════════════════════════════════════════════════════════════
// § TESTS — 11 unit-tests covering :
//   • decision-roundtrip (ProposeAction → ProcgenRequest)
//   • consensus-bridge (council picks PA, bridge maps it)
//   • empty-decisions (council returns Pass → bridge returns None)
//   • budget-respected (explicit budget threads through)
//   • pass-through-Pass (Pass → None)
//   • question-becomes-no-procgen (Question → None)
//   • proposeaction-becomes-procgen (PA → Some)
//   • deterministic-given-same-prompt
//   • role-bracket routing (DM→CreateEntity, Coder→ModifyEntity)
//   • out-of-bracket → Other-preserved
//   • run_council_and_generate end-to-end
// ════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use cssl_host_dmgm_specialists::{
        CoderSpecialist, CollaboratorSpecialist, DmSpecialist, GmSpecialist,
    };

    // ── decision-roundtrip ─────────────────────────────────────────────────

    #[test]
    fn decision_roundtrip_dm_proposeaction() {
        // DM action-id 0x1234 → CreateEntity { kind: "dm-arc", ... }
        let d = Decision::ProposeAction {
            id: 0x1234,
            params: vec![0xDE, 0xAD, 0xBE, 0xEF],
        };
        let req =
            decision_to_procgen_request(&d, ObserverCoord::ORIGIN, 16).unwrap();
        match req.semantic {
            IntentSemantic::CreateEntity { kind, props } => {
                assert_eq!(kind, "dm-arc");
                assert_eq!(props.len(), 2);
                assert_eq!(props[0].0, "action_id");
                assert_eq!(props[0].1, "00001234");
                assert_eq!(props[1].0, "action_params");
                assert_eq!(props[1].1, "deadbeef");
            }
            _ => panic!("expected CreateEntity for DM"),
        }
        assert_eq!(req.budget_ms, 16);
    }

    // ── consensus-bridge ───────────────────────────────────────────────────

    #[test]
    fn consensus_bridge_picks_proposeaction() {
        // Mix of decisions ; council should pick the ProposeAction.
        let decisions = [
            Decision::Pass,
            Decision::Question { hash: 0xAA },
            Decision::ProposeAction {
                id: 0x2050,
                params: vec![0x01, 0x02],
            },
        ];
        let req =
            council_to_procgen(&decisions, ObserverCoord::new(1.0, 0.0, 0.0))
                .unwrap();
        match req.semantic {
            IntentSemantic::CreateEntity { kind, .. } => {
                assert_eq!(kind, "gm-narration", "0x2050 is GM bracket");
            }
            _ => panic!("expected CreateEntity for GM"),
        }
        assert_eq!(req.budget_ms, DEFAULT_BUDGET_MS);
        assert_eq!(req.observer_pos, ObserverCoord::new(1.0, 0.0, 0.0));
    }

    // ── empty-decisions ────────────────────────────────────────────────────

    #[test]
    fn empty_decisions_yields_none() {
        let r = council_to_procgen(&[], ObserverCoord::ORIGIN);
        assert!(r.is_none(), "council on empty slice → Pass → None");
    }

    // ── budget-respected ───────────────────────────────────────────────────

    #[test]
    fn budget_threaded_through_decision_to_procgen() {
        let d = Decision::ProposeAction {
            id: 0x1ABC,
            params: vec![],
        };
        for budget in [1, 16, 64, 1000] {
            let req = decision_to_procgen_request(
                &d,
                ObserverCoord::ORIGIN,
                budget,
            )
            .unwrap();
            assert_eq!(req.budget_ms, budget);
        }
    }

    // ── pass-through-Pass ──────────────────────────────────────────────────

    #[test]
    fn pass_decision_yields_none() {
        let r = decision_to_procgen_request(
            &Decision::Pass,
            ObserverCoord::ORIGIN,
            16,
        );
        assert!(r.is_none(), "Pass is not procgen-actionable");
    }

    // ── question-becomes-no-procgen ────────────────────────────────────────

    #[test]
    fn question_decision_yields_none() {
        let r = decision_to_procgen_request(
            &Decision::Question { hash: 0xDEAD_BEEF },
            ObserverCoord::ORIGIN,
            16,
        );
        assert!(r.is_none(), "Question is not procgen-actionable");
    }

    // ── proposeaction-becomes-procgen ──────────────────────────────────────

    #[test]
    fn proposeaction_decision_yields_some() {
        let d = Decision::ProposeAction {
            id: 0x4123,
            params: vec![0xFF],
        };
        let r =
            decision_to_procgen_request(&d, ObserverCoord::ORIGIN, 16).unwrap();
        match r.semantic {
            IntentSemantic::ModifyEntity { target, .. } => {
                assert_eq!(target, "engine", "0x4123 is Coder bracket");
            }
            _ => panic!("expected ModifyEntity for Coder"),
        }
    }

    // ── determinism ────────────────────────────────────────────────────────

    #[test]
    fn deterministic_given_same_prompt() {
        let dm = DmSpecialist::new();
        let gm = GmSpecialist::new();
        let coll = CollaboratorSpecialist::new();
        let coder = CoderSpecialist::new();
        let specs: Vec<&dyn Specialist> = vec![&dm, &gm, &coll, &coder];

        let prompt = 0xCAFE_BABE_DEAD_BEEF_u64;
        let observer = ObserverCoord::new(2.5, 0.0, -1.5);

        let a = run_council_and_generate(&specs, prompt, observer);
        let b = run_council_and_generate(&specs, prompt, observer);

        assert_eq!(a, b, "same inputs must yield bit-identical output");
    }

    // ── role-bracket routing ───────────────────────────────────────────────

    #[test]
    fn role_bracket_routing_all_four() {
        // DM 0x1xxx → CreateEntity(dm-arc)
        let dm = Decision::ProposeAction { id: 0x1000, params: vec![] };
        match decision_to_procgen_request(&dm, ObserverCoord::ORIGIN, 16)
            .unwrap()
            .semantic
        {
            IntentSemantic::CreateEntity { kind, .. } => {
                assert_eq!(kind, "dm-arc");
            }
            _ => panic!("DM must CreateEntity"),
        }

        // GM 0x2xxx → CreateEntity(gm-narration)
        let gm = Decision::ProposeAction { id: 0x2999, params: vec![] };
        match decision_to_procgen_request(&gm, ObserverCoord::ORIGIN, 16)
            .unwrap()
            .semantic
        {
            IntentSemantic::CreateEntity { kind, .. } => {
                assert_eq!(kind, "gm-narration");
            }
            _ => panic!("GM must CreateEntity"),
        }

        // Coll 0x3xxx → ModifyEntity(scene)
        let coll = Decision::ProposeAction { id: 0x3500, params: vec![] };
        match decision_to_procgen_request(&coll, ObserverCoord::ORIGIN, 16)
            .unwrap()
            .semantic
        {
            IntentSemantic::ModifyEntity { target, .. } => {
                assert_eq!(target, "scene");
            }
            _ => panic!("Coll must ModifyEntity"),
        }

        // Coder 0x4xxx → ModifyEntity(engine)
        let coder = Decision::ProposeAction { id: 0x4321, params: vec![] };
        match decision_to_procgen_request(&coder, ObserverCoord::ORIGIN, 16)
            .unwrap()
            .semantic
        {
            IntentSemantic::ModifyEntity { target, .. } => {
                assert_eq!(target, "engine");
            }
            _ => panic!("Coder must ModifyEntity"),
        }
    }

    // ── out-of-bracket → Other ─────────────────────────────────────────────

    #[test]
    fn out_of_bracket_routes_to_other() {
        // 0x5000 is outside all 4 brackets ; should land in Other.
        let d = Decision::ProposeAction {
            id: 0x5000,
            params: vec![0xAB, 0xCD],
        };
        let req =
            decision_to_procgen_request(&d, ObserverCoord::ORIGIN, 16).unwrap();
        match req.semantic {
            IntentSemantic::Other(s) => {
                assert!(s.starts_with("action:"));
                assert!(s.contains("00005000"));
                assert!(s.contains("abcd"));
            }
            _ => panic!("out-of-bracket must route to Other"),
        }
    }

    // ── role_for_action_id sanity ──────────────────────────────────────────

    #[test]
    fn role_for_action_id_brackets() {
        assert_eq!(role_for_action_id(0x1000), Some(SpecialistRole::DungeonMaster));
        assert_eq!(role_for_action_id(0x1FFF), Some(SpecialistRole::DungeonMaster));
        assert_eq!(role_for_action_id(0x2050), Some(SpecialistRole::GameMaster));
        assert_eq!(role_for_action_id(0x3ABC), Some(SpecialistRole::Collaborator));
        assert_eq!(role_for_action_id(0x4321), Some(SpecialistRole::Coder));
        assert_eq!(role_for_action_id(0x0000), None);
        assert_eq!(role_for_action_id(0x5000), None);
        assert_eq!(role_for_action_id(0xFFFF), None);
    }

    // ── run_council_and_generate end-to-end ────────────────────────────────

    #[test]
    fn run_council_and_generate_end_to_end() {
        // Real specialists ; the council always emits at least a ProposeAction
        // because DM/GM never Pass per the dmgm-specialists doc.
        let dm = DmSpecialist::new();
        let gm = GmSpecialist::new();
        let coll = CollaboratorSpecialist::new();
        let coder = CoderSpecialist::new();
        let specs: Vec<&dyn Specialist> = vec![&dm, &gm, &coll, &coder];

        let prompt = 0xFEED_FACE_DEAD_BEEF_u64;
        let out =
            run_council_and_generate(&specs, prompt, ObserverCoord::ORIGIN);
        // Council picks ProposeAction (highest tag). PA always maps to a
        // procgen-actionable verb (CreateEntity or ModifyEntity), so we
        // expect Some.
        let out = out.expect("council should yield procgen output");
        // Fingerprint must be non-zero (sanity).
        assert_ne!(out.fingerprint, 0);
    }

    #[test]
    fn run_council_empty_specs_yields_none() {
        let specs: Vec<&dyn Specialist> = vec![];
        let r = run_council_and_generate(
            &specs,
            0xABCD,
            ObserverCoord::ORIGIN,
        );
        assert!(r.is_none(), "empty specialists → None");
    }

    // ── only-Question council → None ───────────────────────────────────────

    #[test]
    fn only_questions_council_yields_none() {
        let decisions = [
            Decision::Question { hash: 0x01 },
            Decision::Question { hash: 0x02 },
        ];
        let r = council_to_procgen(&decisions, ObserverCoord::ORIGIN);
        assert!(r.is_none(), "council of Questions → no procgen");
    }

    // ── only-Pass council → None ───────────────────────────────────────────

    #[test]
    fn only_pass_council_yields_none() {
        let decisions = [Decision::Pass, Decision::Pass, Decision::Pass];
        let r = council_to_procgen(&decisions, ObserverCoord::ORIGIN);
        assert!(r.is_none(), "council of Pass → no procgen");
    }

    // ── observer_pos changes fingerprint downstream ────────────────────────

    #[test]
    fn observer_pos_changes_fingerprint() {
        let d = Decision::ProposeAction {
            id: 0x1ABC,
            params: vec![0xDE, 0xAD],
        };
        let req_a = decision_to_procgen_request(
            &d,
            ObserverCoord::new(0.0, 0.0, 0.0),
            16,
        )
        .unwrap();
        let req_b = decision_to_procgen_request(
            &d,
            ObserverCoord::new(0.0, 0.0, 1.0),
            16,
        )
        .unwrap();
        let out_a = generate(&req_a);
        let out_b = generate(&req_b);
        assert_ne!(
            out_a.fingerprint, out_b.fingerprint,
            "observer move must perturb fingerprint"
        );
    }
}
