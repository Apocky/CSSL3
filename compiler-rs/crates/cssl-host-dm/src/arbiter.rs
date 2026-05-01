//! Scene-arbiter : the DM's decision-policy surface.
//!
//! § SPEC : `specs/grand-vision/10_INTELLIGENCE.csl` § ROLE-DM § STAGE-0/1
//! § SIBLING : `specs/grand-vision/11_KAN_RIDE.csl` § SP-4 dm::scene_arbiter
//!
//! § INTERFACE
//!   The trait is **object-safe** — the [`crate::DirectorMaster`] holds
//!   `Box<dyn SceneArbiter>` so a stage-0 implementation and a stage-1 stub
//!   can be swapped at construction-time without touching call-sites. This
//!   mirrors the same registry-edit-not-refactor pattern as
//!   `cssl-host-kan-substrate-bridge::IntentClassifier`.
//!
//! § DETERMINISM
//!   Stage-0 is a pure table-lookup — same `(intents, scene_state)` →
//!   bit-identical `ScenePick`. Replay-bit-equal is a hard invariant.
//!   Stage-1's KAN-spline-table replacement preserves this invariant
//!   (splines BAKED @ comptime ; no online grad-update).

use serde::{Deserialize, Serialize};

use crate::types::IntentSummary;
use crate::SceneStateSnapshot;

// ───────────────────────────────────────────────────────────────────────
// § ScenePick — the arbiter's decision output
// ───────────────────────────────────────────────────────────────────────

/// Discriminator for the arbiter's chosen action.
///
/// `Silent` is returned when no rule fires — implements the
/// `no-action-fallback → SILENT-PASS counter-incr ; ¬ spawn ; await-next`
/// failure-mode from spec § FAILURE-MODES.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum ScenePick {
    /// Stamp a seed-cell at the named zone.
    SceneEdit { location: String },
    /// Spawn the named NPC archetype at the named zone.
    SpawnNpc { npc_handle: String, zone_id: String },
    /// Spawn a generic condensation in the named zone.
    SpawnCondensation { zone_id: String },
    /// Forward a companion-prompt with the given text-hash.
    CompanionPrompt { text_hash: u64 },
    /// No action — silent-pass per spec § FAILURE-MODES.
    Silent,
}

// ───────────────────────────────────────────────────────────────────────
// § SceneArbiter trait
// ───────────────────────────────────────────────────────────────────────

/// Trait for any scene-arbitration backend.
///
/// Stage-0 (`Stage0HeuristicArbiter`) → table-lookup heuristic.
/// Stage-1 (`Stage1KanStubArbiter`)   → KAN-bridge swap-point (stub today).
pub trait SceneArbiter: Send + Sync {
    /// Stable identifier for this backend (e.g. `"stage0-heuristic"`).
    fn name(&self) -> &'static str;

    /// Arbitrate over the supplied intents + current scene snapshot.
    ///
    /// Implementations MUST NOT panic. Empty `intents` MUST return
    /// `ScenePick::Silent`. Low-confidence intents (< 0.25) MUST be
    /// treated as `unknown` per spec failure-mode `intent-confidence-low`.
    fn arbitrate(
        &self,
        intents: &[IntentSummary],
        scene_state: &SceneStateSnapshot,
    ) -> ScenePick;
}

// ───────────────────────────────────────────────────────────────────────
// § Stage0HeuristicArbiter
// ───────────────────────────────────────────────────────────────────────

/// Confidence threshold below which an intent is treated as `unknown`.
/// Implements the `intent-confidence-low → DROP-to-Unknown` failure-mode.
pub const CONFIDENCE_LOW: f32 = 0.25;

/// Stage-0 heuristic arbiter : table-rule + scene-state driven.
///
/// The decision-table is fully deterministic ; same input → same output.
/// Tie-break on multiple matching intents = highest-confidence first ;
/// equal-confidence break by stable index (first-seen wins).
#[derive(Debug, Default, Clone, Copy)]
pub struct Stage0HeuristicArbiter;

impl Stage0HeuristicArbiter {
    /// Construct the stage-0 arbiter.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Pure helper : pick the highest-confidence above-threshold intent
    /// from the supplied list. Returns `None` for empty / all-low-confidence.
    fn highest_confidence_intent(intents: &[IntentSummary]) -> Option<&IntentSummary> {
        intents
            .iter()
            .filter(|i| i.confidence >= CONFIDENCE_LOW)
            .max_by(|a, b| {
                a.confidence
                    .partial_cmp(&b.confidence)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    }
}

impl SceneArbiter for Stage0HeuristicArbiter {
    fn name(&self) -> &'static str {
        "stage0-heuristic"
    }

    fn arbitrate(
        &self,
        intents: &[IntentSummary],
        scene_state: &SceneStateSnapshot,
    ) -> ScenePick {
        // failure-mode : empty → silent-pass.
        if intents.is_empty() {
            return ScenePick::Silent;
        }

        let Some(pick) = Self::highest_confidence_intent(intents) else {
            // failure-mode : all-low-confidence → silent-pass.
            return ScenePick::Silent;
        };

        // § Decision table. Order matters : checked top-to-bottom.
        match pick.kind.as_str() {
            "spawn" => {
                // Prefer scene-state's zone if intent doesn't specify.
                let zone_id = pick
                    .target
                    .clone()
                    .unwrap_or_else(|| scene_state.zone_id.clone());
                ScenePick::SpawnCondensation { zone_id }
            }
            "spawn_npc" => {
                let zone_id = pick
                    .target
                    .clone()
                    .unwrap_or_else(|| scene_state.zone_id.clone());
                // npc_handle defaults to the kind tag if no target given.
                let npc_handle = pick
                    .target
                    .clone()
                    .unwrap_or_else(|| String::from("npc:default"));
                ScenePick::SpawnNpc {
                    npc_handle,
                    zone_id,
                }
            }
            "examine" | "scene_edit" => {
                let location = pick
                    .target
                    .clone()
                    .unwrap_or_else(|| scene_state.zone_id.clone());
                ScenePick::SceneEdit { location }
            }
            "talk" | "companion" => {
                // Hash the kind+target+confidence quantum into a stable u64
                // (deterministic ; no global RNG ; no time-source).
                let text_hash = stable_hash_for_companion(pick, scene_state);
                ScenePick::CompanionPrompt { text_hash }
            }
            // unknown / fallthrough → silent-pass (spec failure-mode).
            _ => ScenePick::Silent,
        }
    }
}

/// Fast deterministic 64-bit digest over the pick + zone tag. Pure FNV-1a
/// flavour ; no allocator beyond the inputs ; no time-source ; collision-
/// resistance is *not* a goal — replay-bit-equality is.
fn stable_hash_for_companion(pick: &IntentSummary, scene: &SceneStateSnapshot) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut h: u64 = FNV_OFFSET;

    let mut feed = |bytes: &[u8]| {
        for b in bytes {
            h ^= u64::from(*b);
            h = h.wrapping_mul(FNV_PRIME);
        }
    };

    feed(pick.kind.as_bytes());
    feed(scene.zone_id.as_bytes());
    if let Some(t) = pick.target.as_ref() {
        feed(t.as_bytes());
    }
    // Confidence quantized to a u16 step ; small wobble doesn't change hash.
    let q: u16 = (pick.confidence.clamp(0.0, 1.0) * 1000.0) as u16;
    feed(&q.to_le_bytes());
    h
}

// ───────────────────────────────────────────────────────────────────────
// § Stage1KanStubArbiter
// ───────────────────────────────────────────────────────────────────────

/// Stage-1 KAN-bridge stub arbiter.
///
/// Carries an opaque `kan_handle` (identifier-only ; the real KAN-handle
/// type lands alongside `cssl-substrate-kan` integration in a later wave).
/// When `kan_handle` is `Some`, returns a canned `ScenePick::Silent`
/// (the conservative default — KAN-stub never fabricates spawn-orders) ;
/// when `None`, delegates to `fallback`. Lets call-sites exercise the
/// stage-1 trait-object without a KAN dependency.
pub struct Stage1KanStubArbiter {
    /// Inner fallback (typically a [`Stage0HeuristicArbiter`]).
    pub fallback: Box<dyn SceneArbiter>,
    /// Mock KAN handle (presence flips the canned-response path on).
    pub kan_handle: Option<String>,
}

impl Stage1KanStubArbiter {
    /// Construct with a fallback + no KAN handle (always delegates).
    #[must_use]
    pub fn new(fallback: Box<dyn SceneArbiter>) -> Self {
        Self {
            fallback,
            kan_handle: None,
        }
    }

    /// Construct with a fallback + a mock KAN handle (canned-response path).
    #[must_use]
    pub fn with_handle(fallback: Box<dyn SceneArbiter>, handle: String) -> Self {
        Self {
            fallback,
            kan_handle: Some(handle),
        }
    }
}

impl SceneArbiter for Stage1KanStubArbiter {
    fn name(&self) -> &'static str {
        "stage1-kan-stub"
    }

    fn arbitrate(
        &self,
        intents: &[IntentSummary],
        scene_state: &SceneStateSnapshot,
    ) -> ScenePick {
        if self.kan_handle.is_some() {
            // Mocked KAN response : conservative silent-pass. Stage-2
            // replaces this with a real KAN forward-pass over scene-state
            // + intent-vector.
            ScenePick::Silent
        } else {
            self.fallback.arbitrate(intents, scene_state)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn neutral() -> SceneStateSnapshot {
        SceneStateSnapshot::neutral("zone:test")
    }

    #[test]
    fn empty_intents_silent_pass() {
        let a = Stage0HeuristicArbiter::new();
        assert_eq!(a.arbitrate(&[], &neutral()), ScenePick::Silent);
    }

    #[test]
    fn unknown_kind_silent_pass() {
        let a = Stage0HeuristicArbiter::new();
        let i = vec![IntentSummary::new("nonsense", 0.9, None)];
        assert_eq!(a.arbitrate(&i, &neutral()), ScenePick::Silent);
    }

    #[test]
    fn low_confidence_silent_pass() {
        let a = Stage0HeuristicArbiter::new();
        let i = vec![IntentSummary::new("spawn", 0.1, None)];
        assert_eq!(a.arbitrate(&i, &neutral()), ScenePick::Silent);
    }

    #[test]
    fn spawn_intent_picks_condensation() {
        let a = Stage0HeuristicArbiter::new();
        let i = vec![IntentSummary::new(
            "spawn",
            0.9,
            Some(String::from("zone:atrium-1")),
        )];
        let pick = a.arbitrate(&i, &neutral());
        assert_eq!(
            pick,
            ScenePick::SpawnCondensation {
                zone_id: String::from("zone:atrium-1")
            }
        );
    }

    #[test]
    fn examine_intent_picks_scene_edit() {
        let a = Stage0HeuristicArbiter::new();
        let i = vec![IntentSummary::new(
            "examine",
            0.9,
            Some(String::from("altar")),
        )];
        let pick = a.arbitrate(&i, &neutral());
        assert_eq!(
            pick,
            ScenePick::SceneEdit {
                location: String::from("altar")
            }
        );
    }

    #[test]
    fn highest_confidence_wins_tiebreak() {
        let a = Stage0HeuristicArbiter::new();
        let i = vec![
            IntentSummary::new("examine", 0.4, Some(String::from("a"))),
            IntentSummary::new("spawn", 0.95, Some(String::from("zone:hi"))),
        ];
        let pick = a.arbitrate(&i, &neutral());
        assert_eq!(
            pick,
            ScenePick::SpawnCondensation {
                zone_id: String::from("zone:hi")
            }
        );
    }

    #[test]
    fn stage1_no_kan_falls_through_to_stage0() {
        let s1 = Stage1KanStubArbiter::new(Box::new(Stage0HeuristicArbiter::new()));
        let i = vec![IntentSummary::new(
            "spawn",
            0.9,
            Some(String::from("zone:fall")),
        )];
        let pick = s1.arbitrate(&i, &neutral());
        assert_eq!(
            pick,
            ScenePick::SpawnCondensation {
                zone_id: String::from("zone:fall")
            }
        );
    }

    #[test]
    fn stage1_with_kan_returns_canned_silent() {
        let s1 = Stage1KanStubArbiter::with_handle(
            Box::new(Stage0HeuristicArbiter::new()),
            String::from("kan-v0-mock"),
        );
        let i = vec![IntentSummary::new(
            "spawn",
            0.9,
            Some(String::from("zone:any")),
        )];
        // Stage-1 stub is conservative — silent regardless of input.
        assert_eq!(s1.arbitrate(&i, &neutral()), ScenePick::Silent);
    }

    #[test]
    fn arbiter_trait_object_safe() {
        // Compile-time check : the trait is object-safe.
        let _v: Vec<Box<dyn SceneArbiter>> = vec![
            Box::new(Stage0HeuristicArbiter::new()),
            Box::new(Stage1KanStubArbiter::new(Box::new(
                Stage0HeuristicArbiter::new(),
            ))),
        ];
    }

    #[test]
    fn determinism_replay_bit_equal() {
        let a = Stage0HeuristicArbiter::new();
        let i = vec![
            IntentSummary::new("talk", 0.7, Some(String::from("companion"))),
            IntentSummary::new("examine", 0.5, Some(String::from("altar"))),
        ];
        let p1 = a.arbitrate(&i, &neutral());
        let p2 = a.arbitrate(&i, &neutral());
        assert_eq!(p1, p2);
    }
}
