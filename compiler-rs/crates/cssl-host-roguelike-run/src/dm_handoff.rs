// § DM/GM handoff ← GDDs/ROGUELIKE_LOOP.csl §INTERACTION-WITH-DM/GM
// ════════════════════════════════════════════════════════════════════
// § I> capability-flow : roguelike-run → DM (read-only run-state)
//                                     → GM (read-only narrative-anchor)
// § I> just structured-event-types + serde — NO dep on cssl-host-dm/gm (cycle-risk)
// § I> DM ¬ override player-agency ; GM ¬ rewrite-experienced-reality
// ════════════════════════════════════════════════════════════════════

use crate::biome_dag::Biome;
use serde::{Deserialize, Serialize};

/// § DM scene-edit request — emitted at floor-genesis or biome-transition.
///
/// DM consumes this to decide floor-template · spawn-density · side-objective
/// within ±15% baseline (per GDD difficulty-curve discipline).
///
/// `Eq` not derived because `spawn_density_baseline` is `f32` (NaN-bearing).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DmSceneEditRequest {
    /// Which biome the floor sits in.
    pub biome: Biome,
    /// Floor-index within the biome (1-based).
    pub floor_idx: u8,
    /// Total floors in this biome traversal.
    pub floor_count: u8,
    /// Run-depth (run_n monotonic) ; informs difficulty-curve.
    pub run_depth: u32,
    /// Floor-template hint (caller-specified ; e.g. "combat-arena", "shrine").
    pub floor_template: String,
    /// Spawn-density baseline (1.0 = nominal ; DM may skew ±15% per spec).
    pub spawn_density_baseline: f32,
    /// Optional side-objective tag (e.g. "rescue-NPC", "destroy-totem").
    pub side_objective: Option<String>,
}

/// § GM intro-prose request — emitted at floor-entry.
///
/// GM emits floor-intro-prose · death-prose · victory-prose per Companion
/// voice-register (warm/terse/poetic/pragmatic).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GmIntroProseRequest {
    /// Which biome.
    pub biome: Biome,
    /// Floor-index (1-based).
    pub floor_idx: u8,
    /// Voice-register hint (caller-attested ; e.g. "warm", "terse").
    pub voice_register: String,
    /// Has the player died on this floor before this run ? (informs prose-tone).
    pub revisit: bool,
}

/// § Tagged enum carrying either handoff variant — for serializable event-stream.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HandoffEvent {
    /// `dm.scene_edit_request` (per GDD interaction-with-DM/GM).
    DmSceneEditRequest(DmSceneEditRequest),
    /// `gm.intro_prose_request` (per GDD interaction-with-DM/GM).
    GmIntroProseRequest(GmIntroProseRequest),
}

impl HandoffEvent {
    /// JSON-serialize the handoff event (for IPC / replay-trace).
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dm_request_serializes() {
        let req = DmSceneEditRequest {
            biome: Biome::Crypt,
            floor_idx: 2,
            floor_count: 5,
            run_depth: 3,
            floor_template: "combat-arena".into(),
            spawn_density_baseline: 1.0,
            side_objective: None,
        };
        let event = HandoffEvent::DmSceneEditRequest(req);
        let json = event.to_json().unwrap();
        assert!(json.contains("dm_scene_edit_request"));
        // Biome serializes via its enum-variant-name ("Crypt") since
        // we don't apply rename_all to Biome itself ; just confirm the
        // discriminator round-trips.
        let back: HandoffEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, back);
    }

    #[test]
    fn gm_request_roundtrips() {
        let req = GmIntroProseRequest {
            biome: Biome::Sanctum,
            floor_idx: 3,
            voice_register: "warm".into(),
            revisit: true,
        };
        let event = HandoffEvent::GmIntroProseRequest(req);
        let json = event.to_json().unwrap();
        let back: HandoffEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, back);
    }
}
