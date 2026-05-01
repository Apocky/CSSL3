//! § scene_input.rs — GM-side mirror of DM scene-state.
//!
//! Defines [`GmSceneInput`] — a flat snapshot the GM consumes per
//! emit-cycle. This mirrors the semantics of `cssl-host-dm`'s
//! `SceneStateSnapshot` without depending on it (W7-C lands in
//! parallel ; cycle-risk forbids the direct dep).
//!
//! § FIELDS
//! - `zone_id` : the active zone (`u32` ; matches DM zone-namespace).
//! - `companion_present` : is the player's companion in this scene.
//! - `radiance` : `[0.0..1.0]` — luminance / hope axis.
//! - `tension_target` : `[0.0..1.0]` — the DM-requested tension level
//!   the GM should pace towards.
//! - `phi_tags` : Φ-tag ids that label whatever entities / surfaces /
//!   intent are anchored in the scene. The GM uses these to slot-fill
//!   templates ; missing-tags degrade to a generic prose fallback.
//! - `player_utterance` : optional inbound text from the player —
//!   typically the most recent voice-STT transcript or text-input.

use serde::{Deserialize, Serialize};

/// One frame of input to the GM narrator.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GmSceneInput {
    pub zone_id: u32,
    pub companion_present: bool,
    pub radiance: f32,
    pub tension_target: f32,
    pub phi_tags: Vec<u32>,
    pub player_utterance: Option<String>,
}

impl GmSceneInput {
    /// Construct a minimal default scene (zone 0, no companion, neutral
    /// lighting + tension, no Φ-tags, no utterance).
    #[must_use]
    pub fn default_empty() -> Self {
        Self {
            zone_id: 0,
            companion_present: false,
            radiance: 0.5,
            tension_target: 0.5,
            phi_tags: Vec::new(),
            player_utterance: None,
        }
    }

    /// Stable hash of `phi_tags` (FNV-1a-32 over little-endian bytes).
    ///
    /// Used as one factor of the deterministic seed for template
    /// selection. The hash is intentionally simple : the dominant
    /// requirement is replay-bit-equal across runs, not collision
    /// resistance.
    #[must_use]
    pub fn phi_tags_hash_fnv1a(&self) -> u32 {
        let mut h: u32 = 0x811c_9dc5;
        for tag in &self.phi_tags {
            for b in tag.to_le_bytes() {
                h ^= u32::from(b);
                h = h.wrapping_mul(0x0100_0193);
            }
        }
        h
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_empty_is_neutral() {
        let s = GmSceneInput::default_empty();
        assert_eq!(s.zone_id, 0);
        assert!(!s.companion_present);
        assert_eq!(s.radiance, 0.5);
        assert_eq!(s.tension_target, 0.5);
        assert!(s.phi_tags.is_empty());
        assert!(s.player_utterance.is_none());
    }

    #[test]
    fn fnv_hash_stable_across_calls() {
        let mut s = GmSceneInput::default_empty();
        s.phi_tags = vec![1, 2, 3, 4];
        let a = s.phi_tags_hash_fnv1a();
        let b = s.phi_tags_hash_fnv1a();
        assert_eq!(a, b);
    }

    #[test]
    fn fnv_hash_changes_with_tags() {
        let mut s1 = GmSceneInput::default_empty();
        s1.phi_tags = vec![1, 2, 3];
        let mut s2 = GmSceneInput::default_empty();
        s2.phi_tags = vec![4, 5, 6];
        assert_ne!(s1.phi_tags_hash_fnv1a(), s2.phi_tags_hash_fnv1a());
    }
}
