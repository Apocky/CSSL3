//! § Read-only snapshot types.
//!
//! Phase-J § 2.5 mandates :
//!   - FieldCellSnapshot carries a 32-bit cached Σ low-half ; the FULL 16-byte
//!     overlay is PRIVATE to the substrate and never inlined into a snapshot.
//!   - EntitySnapshot's `reproductive_state` field is DELIBERATELY ABSENT.
//!   - Audit-sequence is monotone across all reads.
//!
//! In this MVP we omit biometric-class fields entirely (the type-system gate
//! is enforced by D132 in the real-impl ; for the MVP we simply do not put
//! such fields in the schemas and the Σ-mask gate refuses any tag flagged
//! with the `"biometric"` substring).
//!
//! The snapshot types are `Clone + Debug + PartialEq` — they are pure data
//! and intentionally cheap to copy, because MCP-tool consumers will round-
//! trip them via JSON.

use crate::mock_substrate::{MortonKey, SigmaConsentBits, SigmaOverlay};

/// Stable entity identifier. Production-impl is `cssl_substrate::EntityId`
/// (a `u64` newtype) ; this mock is a `u64` newtype.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EntityId(u64);

impl EntityId {
    /// Construct an entity-id from a raw u64.
    #[must_use]
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    /// The raw u64 backing value.
    #[must_use]
    pub const fn raw(self) -> u64 {
        self.0
    }
}

/// Material-view facet of a field cell. Phase-J § 2.5 carries a 1-bit-tag +
/// 63-bit-payload + decoded class ; the MVP uses a class-name string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterialView {
    /// Class name (e.g. "wood", "metal", "skin").
    pub class: String,
    /// Material ID payload.
    pub payload: u64,
}

impl MaterialView {
    /// Construct a new material-view.
    #[must_use]
    pub fn new(class: impl Into<String>, payload: u64) -> Self {
        Self {
            class: class.into(),
            payload,
        }
    }
}

/// A read-only snapshot of one field-cell. Phase-J § 2.5.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldCellSnapshot {
    /// The morton-key of the cell.
    pub morton_key: MortonKey,
    /// Tag string used by the mock-Σ-overlay to decide consent.
    pub tag: String,
    /// Material facet view.
    pub facet_m: MaterialView,
    /// Density (facet-S simplified).
    pub density: f32,
    /// Velocity vector.
    pub velocity: [f32; 3],
    /// Cached low-half of the Σ-overlay (32-bit, NEVER the full mask).
    pub facet_sigma_low_only: SigmaConsentBits,
    /// Capture epoch (mock).
    pub capture_epoch: u64,
    /// Audit sequence — monotone across all reads.
    pub audit_seq: u64,
}

impl FieldCellSnapshot {
    /// Construct a cell snapshot from raw inputs. The Σ-cached-bits are
    /// looked up from the mock overlay using `tag`.
    #[must_use]
    pub fn new(
        morton_key: MortonKey,
        tag: impl Into<String>,
        facet_m: MaterialView,
        density: f32,
        velocity: [f32; 3],
        capture_epoch: u64,
    ) -> Self {
        let tag = tag.into();
        let cached = SigmaOverlay::at(&tag).cached_bits();
        Self {
            morton_key,
            tag,
            facet_m,
            density,
            velocity,
            facet_sigma_low_only: cached,
            capture_epoch,
            audit_seq: 0,
        }
    }

    /// Return a clone with `audit_seq` set to `seq`. The inspector calls
    /// this on the way out so the snapshot records the read-time sequence.
    #[must_use]
    pub fn with_audit_seq(mut self, seq: u64) -> Self {
        self.audit_seq = seq;
        self
    }
}

/// A read-only snapshot of one entity. Phase-J § 2.5 mandates the
/// `reproductive_state` field is DELIBERATELY ABSENT — and it is. The
/// body-omnoid layer summaries are simple counters in the MVP ; later
/// slices replace them with the real summaries.
#[derive(Debug, Clone, PartialEq)]
pub struct EntitySnapshot {
    /// Entity id.
    pub entity_id: EntityId,
    /// Tag string for Σ-mask gate.
    pub tag: String,
    /// Aura summary (mock — wave-field amplitude+phase summary).
    pub aura_amp: f32,
    /// Flesh summary (mock — SDF param).
    pub flesh_sdf_param: f32,
    /// Bone summary (mock — joint count).
    pub bone_joint_count: u32,
    /// Machine summary (mock — mechanism-state code).
    pub machine_state_code: u32,
    /// Soul summary (mock — pattern-handle ; 0 = unclaimed).
    pub soul_pattern_handle: u32,
    /// Audit sequence.
    pub audit_seq: u64,
    // § DELIBERATELY ABSENT : reproductive_state. Phase-J § 2.5 invariant.
}

impl EntitySnapshot {
    /// Construct an entity snapshot.
    #[must_use]
    pub fn new(entity_id: EntityId, tag: impl Into<String>) -> Self {
        Self {
            entity_id,
            tag: tag.into(),
            aura_amp: 0.0,
            flesh_sdf_param: 0.0,
            bone_joint_count: 0,
            machine_state_code: 0,
            soul_pattern_handle: 0,
            audit_seq: 0,
        }
    }

    /// Builder: set aura summary.
    #[must_use]
    pub fn with_aura(mut self, amp: f32) -> Self {
        self.aura_amp = amp;
        self
    }

    /// Builder: set bone joint count.
    #[must_use]
    pub fn with_bones(mut self, n: u32) -> Self {
        self.bone_joint_count = n;
        self
    }

    /// Builder: set audit-seq.
    #[must_use]
    pub fn with_audit_seq(mut self, seq: u64) -> Self {
        self.audit_seq = seq;
        self
    }
}

/// A read-only snapshot of an entire scene graph. The MVP stores cells +
/// entities in flat `Vec`s ; production-impl will hand back a proxy that
/// dereferences into the real sparse-morton + ECS storage.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SceneGraphSnapshot {
    cells: Vec<FieldCellSnapshot>,
    entities: Vec<EntitySnapshot>,
}

impl SceneGraphSnapshot {
    /// An empty scene.
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Construct from explicit cell + entity vectors.
    #[must_use]
    pub fn from_parts(cells: Vec<FieldCellSnapshot>, entities: Vec<EntitySnapshot>) -> Self {
        Self { cells, entities }
    }

    /// All cells (read-only iterator).
    pub fn cells(&self) -> impl Iterator<Item = &FieldCellSnapshot> {
        self.cells.iter()
    }

    /// All entities (read-only iterator).
    pub fn entities(&self) -> impl Iterator<Item = &EntitySnapshot> {
        self.entities.iter()
    }

    /// Cell count.
    #[must_use]
    pub fn cell_count(&self) -> usize {
        self.cells.len()
    }

    /// Entity count.
    #[must_use]
    pub fn entity_count(&self) -> usize {
        self.entities.len()
    }

    /// Look up a cell by morton-key.
    #[must_use]
    pub fn cell_by_key(&self, key: MortonKey) -> Option<&FieldCellSnapshot> {
        self.cells.iter().find(|c| c.morton_key == key)
    }

    /// Look up an entity by id.
    #[must_use]
    pub fn entity_by_id(&self, id: EntityId) -> Option<&EntitySnapshot> {
        self.entities.iter().find(|e| e.entity_id == id)
    }

    /// Push a cell. Read-only API note : this is a build-time mutator on
    /// the *snapshot scaffold*, not on the live world. Tests use it to
    /// fixture a scene ; production-impl will hand back a snapshot
    /// constructed by the substrate, never mutated by inspector users.
    pub fn push_cell(&mut self, cell: FieldCellSnapshot) {
        self.cells.push(cell);
    }

    /// Push an entity (see note on `push_cell`).
    pub fn push_entity(&mut self, entity: EntitySnapshot) {
        self.entities.push(entity);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entity_id_round_trip() {
        let e = EntityId::new(42);
        assert_eq!(e.raw(), 42);
    }

    #[test]
    fn material_view_construct() {
        let m = MaterialView::new("wood", 7);
        assert_eq!(m.class, "wood");
        assert_eq!(m.payload, 7);
    }

    #[test]
    fn cell_snapshot_open_tag_has_observe() {
        let c = FieldCellSnapshot::new(
            MortonKey::new(1),
            "wood",
            MaterialView::new("wood", 0),
            1.0,
            [0.0; 3],
            0,
        );
        assert!(c
            .facet_sigma_low_only
            .permits(crate::mock_substrate::ConsentBit::Observe));
    }

    #[test]
    fn cell_snapshot_biometric_tag_refuses_observe() {
        let c = FieldCellSnapshot::new(
            MortonKey::new(2),
            "biometric:fingerprint",
            MaterialView::new("skin", 0),
            1.0,
            [0.0; 3],
            0,
        );
        assert!(!c
            .facet_sigma_low_only
            .permits(crate::mock_substrate::ConsentBit::Observe));
    }

    #[test]
    fn cell_with_audit_seq_sets_seq() {
        let c = FieldCellSnapshot::new(
            MortonKey::new(3),
            "tag",
            MaterialView::new("metal", 0),
            0.5,
            [1.0, 2.0, 3.0],
            10,
        )
        .with_audit_seq(99);
        assert_eq!(c.audit_seq, 99);
    }

    #[test]
    fn entity_snapshot_default_zeros() {
        let e = EntitySnapshot::new(EntityId::new(0), "tag");
        assert!(e.aura_amp.abs() < f32::EPSILON);
        assert_eq!(e.bone_joint_count, 0);
        assert_eq!(e.audit_seq, 0);
    }

    #[test]
    fn entity_builder_chains() {
        let e = EntitySnapshot::new(EntityId::new(7), "human")
            .with_aura(0.5)
            .with_bones(206)
            .with_audit_seq(1);
        assert!((e.aura_amp - 0.5).abs() < f32::EPSILON);
        assert_eq!(e.bone_joint_count, 206);
        assert_eq!(e.audit_seq, 1);
    }

    #[test]
    fn scene_empty_has_zero_counts() {
        let s = SceneGraphSnapshot::empty();
        assert_eq!(s.cell_count(), 0);
        assert_eq!(s.entity_count(), 0);
    }

    #[test]
    fn scene_push_and_count() {
        let mut s = SceneGraphSnapshot::empty();
        s.push_cell(FieldCellSnapshot::new(
            MortonKey::new(11),
            "ok",
            MaterialView::new("metal", 0),
            1.0,
            [0.0; 3],
            0,
        ));
        s.push_entity(EntitySnapshot::new(EntityId::new(11), "ok"));
        assert_eq!(s.cell_count(), 1);
        assert_eq!(s.entity_count(), 1);
    }

    #[test]
    fn scene_lookup_cell_by_key() {
        let mut s = SceneGraphSnapshot::empty();
        s.push_cell(FieldCellSnapshot::new(
            MortonKey::new(99),
            "ok",
            MaterialView::new("metal", 0),
            1.0,
            [0.0; 3],
            0,
        ));
        assert!(s.cell_by_key(MortonKey::new(99)).is_some());
        assert!(s.cell_by_key(MortonKey::new(0)).is_none());
    }

    #[test]
    fn scene_lookup_entity_by_id() {
        let mut s = SceneGraphSnapshot::empty();
        s.push_entity(EntitySnapshot::new(EntityId::new(5), "ok"));
        assert!(s.entity_by_id(EntityId::new(5)).is_some());
        assert!(s.entity_by_id(EntityId::new(99)).is_none());
    }

    #[test]
    fn scene_iters_round_trip() {
        let mut s = SceneGraphSnapshot::empty();
        for i in 0..3 {
            s.push_cell(FieldCellSnapshot::new(
                MortonKey::new(i),
                "ok",
                MaterialView::new("a", 0),
                0.0,
                [0.0; 3],
                0,
            ));
            s.push_entity(EntitySnapshot::new(EntityId::new(i), "ok"));
        }
        assert_eq!(s.cells().count(), 3);
        assert_eq!(s.entities().count(), 3);
    }

    #[test]
    fn scene_from_parts() {
        let cells = vec![FieldCellSnapshot::new(
            MortonKey::new(1),
            "ok",
            MaterialView::new("a", 0),
            0.0,
            [0.0; 3],
            0,
        )];
        let entities = vec![EntitySnapshot::new(EntityId::new(2), "ok")];
        let s = SceneGraphSnapshot::from_parts(cells, entities);
        assert_eq!(s.cell_count(), 1);
        assert_eq!(s.entity_count(), 1);
    }
}
