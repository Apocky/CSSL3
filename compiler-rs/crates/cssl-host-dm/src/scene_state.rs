//! Snapshot of the current scene-state consumed by [`crate::SceneArbiter`].
//!
//! § SPEC : `specs/grand-vision/10_INTELLIGENCE.csl` § ROLE-DM § INPUTS
//!
//! The DM treats this as a *read-only frozen frame* of the live ω-field
//! summary that the host hands it. The arbiter table-rules switch on these
//! fields ; the snapshot is small (≈24 bytes) so cloning per arbitrate-call
//! is cheap.

use serde::{Deserialize, Serialize};

/// Read-only snapshot of the scene-state at arbitration-time.
///
/// `radiance` is the zone-aggregated ω-field radiance ∈ [0.0, 1.0].
/// `tension_target` is the DM-set narrative-tension goal ∈ [0.0, 1.0].
/// Both are clamped at construction-time via [`SceneStateSnapshot::new`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneStateSnapshot {
    /// Zone identifier (e.g. `"zone:atrium-7"`).
    pub zone_id: String,
    /// Aggregated ω-field radiance ∈ [0.0, 1.0]. Clamped on construction.
    pub radiance: f32,
    /// Number of NPCs currently in the zone.
    pub npc_count: u32,
    /// Whether the player's companion is currently present in the zone.
    pub companion_present: bool,
    /// DM-set narrative-tension target ∈ [0.0, 1.0]. Handed to GM @ S3-dispatch.
    pub tension_target: f32,
}

impl SceneStateSnapshot {
    /// Construct a clamped snapshot (radiance + tension_target clamped to
    /// [0.0, 1.0] for invariant-safety).
    #[must_use]
    pub fn new(
        zone_id: impl Into<String>,
        radiance: f32,
        npc_count: u32,
        companion_present: bool,
        tension_target: f32,
    ) -> Self {
        Self {
            zone_id: zone_id.into(),
            radiance: radiance.clamp(0.0, 1.0),
            npc_count,
            companion_present,
            tension_target: tension_target.clamp(0.0, 1.0),
        }
    }

    /// A neutral snapshot used in tests + as a default for empty calls.
    #[must_use]
    pub fn neutral(zone_id: impl Into<String>) -> Self {
        Self::new(zone_id, 0.5, 0, false, 0.5)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neutral_zone_is_balanced() {
        let s = SceneStateSnapshot::neutral("zone:test");
        assert_eq!(s.zone_id, "zone:test");
        assert!((s.radiance - 0.5).abs() < 1e-6);
        assert_eq!(s.npc_count, 0);
        assert!(!s.companion_present);
        assert!((s.tension_target - 0.5).abs() < 1e-6);
    }

    #[test]
    fn radiance_clamped_to_unit_range() {
        let lo = SceneStateSnapshot::new("z", -0.5, 0, false, 0.0);
        let hi = SceneStateSnapshot::new("z", 1.5, 0, false, 0.0);
        assert!((lo.radiance - 0.0).abs() < 1e-6);
        assert!((hi.radiance - 1.0).abs() < 1e-6);
    }

    #[test]
    fn tension_clamped_to_unit_range() {
        let lo = SceneStateSnapshot::new("z", 0.0, 0, false, -2.0);
        let hi = SceneStateSnapshot::new("z", 0.0, 0, false, 9.9);
        assert!((lo.tension_target - 0.0).abs() < 1e-6);
        assert!((hi.tension_target - 1.0).abs() < 1e-6);
    }

    #[test]
    fn serde_round_trip() {
        let s = SceneStateSnapshot::new("zone:hall-3", 0.7, 4, true, 0.85);
        let j = serde_json::to_string(&s).expect("serialize");
        let back: SceneStateSnapshot = serde_json::from_str(&j).expect("deserialize");
        assert_eq!(s, back);
    }
}
