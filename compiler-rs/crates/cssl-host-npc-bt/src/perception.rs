// § perception.rs — L0 layer ; LOCAL-ONLY zone-radius perception
// ════════════════════════════════════════════════════════════════════
// § I> per GDD § ARCHITECTURE-LAYERS L0 ; § AXIOMS ¬-surveillance-of-player
// § I> NPC reads ω-field-cell within radius=R ; ¬ player-private-state
// § I> Sensitive<biometric|gaze|face|body> NEVER appears in this struct (SIG0003)
// § I> shape : Perception { zone_radius, npc_position, sensed: Vec<SensedEntity> }
// ════════════════════════════════════════════════════════════════════

use serde::{Deserialize, Serialize};

/// Coarse classification of a sensed entity. Deliberately *narrow* —
/// only kinds an NPC can perceive via local-zone observation.
///
/// § I> ¬ Player-Private — Player kind exists but reveals only public position,
///   never gaze/biometric/face/body (those are Sensitive<*> and structurally banned).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SensedKind {
    /// Another NPC (handle).
    Npc,
    /// Player avatar — public position only ; never private state.
    Player,
    /// Hostile creature (mob).
    Hostile,
    /// Resource node (forageable / mineable).
    Resource,
    /// Inanimate item on ground.
    Item,
    /// Static structure (door / chest / etc).
    Structure,
}

/// One entity sensed within the NPC's zone radius.
///
/// § I> `handle` is an opaque entity-id ; NPC stores nothing beyond what's listed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SensedEntity {
    /// Opaque entity handle from the host.
    pub handle: u64,
    /// What kind of entity this is.
    pub kind: SensedKind,
    /// Distance bucket (whole units) — quantized for replay-bit-equality.
    pub distance: u32,
}

/// Local-only perception snapshot for a single NPC at a single tick.
///
/// § I> Constructed by the host's perception-pass. Never contains
/// Sensitive<biometric|gaze|face|body> data — that's structurally banned.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Perception {
    /// Sensing radius in world-cells (per GDD : R=16 default).
    pub zone_radius: f32,
    /// NPC's own position [x, y, z].
    pub npc_position: [f32; 3],
    /// Entities sensed this tick — sorted by `distance` ascending for determinism.
    pub sensed: Vec<SensedEntity>,
}

impl Perception {
    /// New empty perception at the given position with the given radius.
    #[must_use]
    pub fn new(npc_position: [f32; 3], zone_radius: f32) -> Self {
        Self {
            zone_radius,
            npc_position,
            sensed: Vec::new(),
        }
    }

    /// Insert a sensed-entity ; preserves distance-ascending order via stable insertion.
    pub fn add(&mut self, ent: SensedEntity) {
        let pos = self
            .sensed
            .iter()
            .position(|e| e.distance > ent.distance)
            .unwrap_or(self.sensed.len());
        self.sensed.insert(pos, ent);
    }

    /// True iff at least one sensed entity has the given kind within the zone.
    #[must_use]
    pub fn any_of_kind(&self, kind: SensedKind) -> bool {
        self.sensed.iter().any(|e| e.kind == kind)
    }

    /// Closest sensed entity of the given kind (or None).
    #[must_use]
    pub fn nearest_of_kind(&self, kind: SensedKind) -> Option<&SensedEntity> {
        self.sensed.iter().find(|e| e.kind == kind)
    }

    /// Total count of sensed entities.
    #[must_use]
    pub fn len(&self) -> usize {
        self.sensed.len()
    }

    /// True iff no entities sensed this tick.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.sensed.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perception_construction() {
        let p = Perception::new([1.0, 2.0, 3.0], 16.0);
        assert!((p.zone_radius - 16.0).abs() < 1e-6);
        assert_eq!(p.npc_position, [1.0, 2.0, 3.0]);
        assert!(p.is_empty());
    }

    #[test]
    fn add_preserves_distance_order() {
        let mut p = Perception::new([0.0, 0.0, 0.0], 16.0);
        p.add(SensedEntity {
            handle: 1,
            kind: SensedKind::Npc,
            distance: 8,
        });
        p.add(SensedEntity {
            handle: 2,
            kind: SensedKind::Resource,
            distance: 2,
        });
        p.add(SensedEntity {
            handle: 3,
            kind: SensedKind::Hostile,
            distance: 5,
        });
        assert_eq!(p.sensed[0].distance, 2);
        assert_eq!(p.sensed[1].distance, 5);
        assert_eq!(p.sensed[2].distance, 8);
    }

    #[test]
    fn any_and_nearest_of_kind() {
        let mut p = Perception::new([0.0; 3], 16.0);
        p.add(SensedEntity {
            handle: 7,
            kind: SensedKind::Hostile,
            distance: 10,
        });
        p.add(SensedEntity {
            handle: 8,
            kind: SensedKind::Hostile,
            distance: 4,
        });
        assert!(p.any_of_kind(SensedKind::Hostile));
        assert!(!p.any_of_kind(SensedKind::Player));
        assert_eq!(p.nearest_of_kind(SensedKind::Hostile).unwrap().handle, 8);
    }
}
