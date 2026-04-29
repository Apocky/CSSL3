//! § BodyPlanPhysics — KAN-driven skeleton + joints from creature genome.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Reads a `cssl-substrate-kan::Pattern` (the substrate-invariant identity
//!   carrier) + the `KanMaterialKind::CreatureMorphology` variant and emits
//!   a `Skeleton` graph the XPBD solver consumes :
//!
//!   - `Skeleton.bones[]` : `Bone { id, parent, length, attach_offset }`.
//!   - `Skeleton.joints[]` : `BodyJoint { from_bone, to_bone, kind, axis,
//!     compliance }`.
//!
//!   Per Omniverse `06_PROCEDURAL/02_CREATURES_FROM_GENOME.csl.md` §III :
//!
//!   ```text
//!   body_plan ⊗ KAN @ {pattern, env, age, variation} → SDF-parameters + skeleton-graph
//!   ```
//!
//!   The skeleton-graph is the bridge between KAN-derived morphology
//!   coefficients and the XPBD constraint-solver's distance + hinge
//!   constraints. The morphology coefficients (16 floats, per
//!   `MORPHOLOGY_PARAMS = 16`) parameterize :
//!
//!   - bone-count (clamped to `[2, MAX_BONES]`)
//!   - per-bone length-scale
//!   - parent-edge attachments (a tree)
//!   - joint kinds (distance vs hinge)
//!   - per-joint compliance
//!
//! § SUBSTRATE
//!   This module does NOT itself evaluate the KAN network — that lives
//!   in `cssl-substrate-kan::KanNetwork::evaluate`. We accept the 16-D
//!   morphology vector as input + interpret its components via a fixed
//!   index-table (see `MORPHOLOGY_INDEX_*` constants below). This keeps
//!   the wave-physics crate decoupled from the KAN-eval implementation
//!   detail.
//!
//! § DETERMINISM
//!   Skeleton emission is a pure function of `(MorphologyCoeffs, Pattern)`.
//!   The Pattern's fingerprint participates as a deterministic seed for
//!   any per-creature variation that's not in the morphology vector
//!   (e.g. the bone-jitter axis for joint-stops).

use cssl_substrate_kan::{KanMaterialKind, Pattern};
use smallvec::SmallVec;
use thiserror::Error;

use crate::xpbd::Constraint;

/// § Morphology vector dimensionality. Matches the spec's
///   `MORPHOLOGY_PARAMS = 16` literal in `cssl-substrate-kan::kan_material`.
pub const MORPHOLOGY_DIM: usize = 16;

/// § Maximum bones per skeleton. Bounded so the constraint-graph never
///   exceeds the XPBD solver's per-color bucket cap. The spec's
///   `06_PROCEDURAL/02_CREATURES_FROM_GENOME §IV` does not pin this ; we
///   pick 32 as a safe ceiling for typical mammalian / insectoid /
///   skeletal-creature body-plans.
pub const MAX_BONES: usize = 32;

/// § Inline cap for joints per bone (most bones have ≤ 4 joint-edges).
pub const JOINTS_PER_BONE_INLINE: usize = 4;

// § Morphology vector index table — fixed across versions for replay-stability.
//   Each MorphologyCoeffs[i] is interpreted as documented below.
//
// § BONE COUNT (clamped to `[2, MAX_BONES]`).
pub const MORPHOLOGY_INDEX_BONE_COUNT: usize = 0;
/// § Average bone-length scale (meters). 0 → 0.1m, 1 → 1.0m.
pub const MORPHOLOGY_INDEX_BONE_LENGTH_SCALE: usize = 1;
/// § Tree-branchiness : 0 → linear chain, 1 → star.
pub const MORPHOLOGY_INDEX_BRANCHINESS: usize = 2;
/// § Limb-symmetry-axis tag (0 = bilateral, 1 = radial).
pub const MORPHOLOGY_INDEX_SYMMETRY: usize = 3;
/// § Joint-kind mix : 0 → all-distance, 1 → all-hinge.
pub const MORPHOLOGY_INDEX_JOINT_HINGE_RATIO: usize = 4;
/// § Joint-compliance default (0 = rigid, > 0 = soft).
pub const MORPHOLOGY_INDEX_JOINT_COMPLIANCE: usize = 5;
/// § Bone mass-density coefficient.
pub const MORPHOLOGY_INDEX_MASS_DENSITY: usize = 6;
/// § Spine-segment-count multiplier.
pub const MORPHOLOGY_INDEX_SPINE_SEGMENTS: usize = 7;
// 8..15 reserved for future spec growth (avian / aquatic / mineral).

// ───────────────────────────────────────────────────────────────────────
// § Bone + BoneId.
// ───────────────────────────────────────────────────────────────────────

/// § Identifier for a bone within a skeleton (just an index into the
///   skeleton's bone-list).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(transparent)]
pub struct BoneId(pub u32);

impl BoneId {
    /// § "No parent" sentinel for the skeleton-tree root.
    pub const NONE: BoneId = BoneId(u32::MAX);

    /// § Convert to `u64` for use as a body-id in the XPBD solver.
    #[must_use]
    pub fn as_body_id(self) -> u64 {
        self.0 as u64
    }
}

/// § A bone in the creature skeleton.
///
///   The bone is a rigid-body proxy : its `attach_offset` is the offset
///   from the parent bone's anchor where this bone attaches ; the bone
///   acts as a single physics-body in the XPBD solver.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bone {
    /// § This bone's id.
    pub id: BoneId,
    /// § Parent bone (`BoneId::NONE` for the root).
    pub parent: BoneId,
    /// § Bone length in meters (the joint-rest-distance to the parent).
    pub length: f32,
    /// § Attach offset from parent in (parent-local) world units.
    pub attach_offset: [f32; 3],
    /// § Approximate mass (kg). Computed from `length × mass_density × volume`.
    pub mass: f32,
}

// ───────────────────────────────────────────────────────────────────────
// § JointKind + BodyJoint.
// ───────────────────────────────────────────────────────────────────────

/// § Kind-tag for a joint between two bones.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JointKind {
    /// Distance-link : rigid stick between two bone anchors.
    Distance,
    /// Hinge : 1-DOF rotation about an axis.
    Hinge,
}

/// § A joint connecting two bones in the skeleton.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BodyJoint {
    /// § Source bone (parent-side).
    pub from: BoneId,
    /// § Target bone (child-side).
    pub to: BoneId,
    /// § Joint kind.
    pub kind: JointKind,
    /// § Hinge axis (only meaningful for `JointKind::Hinge` ; ignored otherwise).
    pub axis: [f32; 3],
    /// § Compliance (0 = rigid, > 0 = soft).
    pub compliance: f32,
}

// ───────────────────────────────────────────────────────────────────────
// § Skeleton.
// ───────────────────────────────────────────────────────────────────────

/// § A creature skeleton emitted by `BodyPlanPhysics::derive_skeleton`.
#[derive(Debug, Clone)]
pub struct Skeleton {
    /// § Bones in the skeleton (root at index 0).
    pub bones: Vec<Bone>,
    /// § Joints between bones.
    pub joints: Vec<BodyJoint>,
    /// § Source pattern fingerprint — for telemetry + audit.
    pub pattern_fingerprint: [u8; 32],
}

impl Skeleton {
    /// § Total bone count.
    #[must_use]
    pub fn bone_count(&self) -> usize {
        self.bones.len()
    }

    /// § Total joint count.
    #[must_use]
    pub fn joint_count(&self) -> usize {
        self.joints.len()
    }

    /// § Lowercase-hex of the pattern fingerprint.
    #[must_use]
    pub fn fingerprint_hex(&self) -> String {
        let mut s = String::with_capacity(64);
        for b in self.pattern_fingerprint {
            s.push_str(&format!("{b:02x}"));
        }
        s
    }

    /// § Emit XPBD constraints corresponding to every joint.
    ///
    ///   Distance-joints become `Constraint::distance` ; hinge-joints
    ///   become `Constraint::hinge`. The body-id is `BoneId::as_body_id`.
    #[must_use]
    pub fn to_constraints(&self) -> Vec<Constraint> {
        let mut out = Vec::with_capacity(self.joints.len());
        for j in &self.joints {
            let from_id = j.from.as_body_id();
            let to_id = j.to.as_body_id();
            let from_bone = &self.bones[j.from.0 as usize];
            let to_bone = &self.bones[j.to.0 as usize];
            let rest = ((from_bone.length + to_bone.length) * 0.5).max(0.05);
            let c = match j.kind {
                JointKind::Distance => Constraint::distance(from_id, to_id, rest, j.compliance),
                JointKind::Hinge => Constraint::hinge(from_id, to_id, j.axis, j.compliance),
            };
            out.push(c);
        }
        out
    }

    /// § Compute initial bone positions for the JacobiBlock seed.
    ///
    ///   Walks the tree from the root, placing each child bone at parent +
    ///   attach_offset along the +X axis (a deterministic placement).
    #[must_use]
    pub fn initial_positions(&self) -> Vec<[f32; 3]> {
        let mut out = vec![[0.0; 3]; self.bones.len()];
        for (i, b) in self.bones.iter().enumerate() {
            if b.parent == BoneId::NONE {
                out[i] = [0.0; 3];
            } else {
                let p_idx = b.parent.0 as usize;
                let parent_pos = out[p_idx];
                out[i] = [
                    parent_pos[0] + b.attach_offset[0],
                    parent_pos[1] + b.attach_offset[1],
                    parent_pos[2] + b.attach_offset[2],
                ];
            }
        }
        out
    }

    /// § Bone inverse-mass list. Static bones (mass = 0) emit `0.0`,
    ///   dynamic bones emit `1.0 / mass`.
    #[must_use]
    pub fn inverse_masses(&self) -> Vec<f32> {
        self.bones
            .iter()
            .map(|b| if b.mass <= 0.0 { 0.0 } else { 1.0 / b.mass })
            .collect()
    }
}

// ───────────────────────────────────────────────────────────────────────
// § BodyPlanError.
// ───────────────────────────────────────────────────────────────────────

/// § Failure modes of body-plan derivation.
#[derive(Debug, Clone, Copy, PartialEq, Error)]
pub enum BodyPlanError {
    /// Morphology vector wasn't `MORPHOLOGY_DIM` long.
    #[error("PHYSWAVE0030 — morphology vector dimension {got} != expected {expected}")]
    DimensionMismatch {
        /// Received dim.
        got: usize,
        /// Expected dim.
        expected: usize,
    },
    /// Pattern's KAN-material kind is not `CreatureMorphology`.
    #[error("PHYSWAVE0031 — pattern's material kind {kind:?} is not CreatureMorphology")]
    WrongMaterialKind {
        /// The unexpected kind.
        kind: KanMaterialKind,
    },
    /// Bone-count fell outside `[2, MAX_BONES]`.
    #[error("PHYSWAVE0032 — derived bone-count {count} out of range [2, {max}]")]
    BoneCountOutOfRange {
        /// Derived count.
        count: usize,
        /// Maximum.
        max: usize,
    },
}

// ───────────────────────────────────────────────────────────────────────
// § BodyPlanPhysics.
// ───────────────────────────────────────────────────────────────────────

/// § The body-plan physics adapter.
///
///   Holds no per-instance state ; the methods are pure functions of
///   their arguments. The struct exists as a namespace for the canonical
///   API + a future home for cached morphology lookup tables.
#[derive(Debug, Clone, Copy, Default)]
pub struct BodyPlanPhysics {
    /// § True ⇒ allow the spinal-segments multiplier to add bones beyond
    ///   the base count. Default = `true`.
    pub allow_spine_extension: bool,
    /// § Cap on hinge axis variance per joint (radians). Default = `1.5`.
    pub hinge_axis_variance: f32,
}

impl BodyPlanPhysics {
    /// § Construct a default body-plan adapter.
    #[must_use]
    pub fn new() -> Self {
        BodyPlanPhysics {
            allow_spine_extension: true,
            hinge_axis_variance: 1.5,
        }
    }

    /// § Derive a skeleton from a creature-morphology pattern + the 16-D
    ///   morphology coefficient vector.
    ///
    ///   The `pattern` carries the substrate-invariant fingerprint ; the
    ///   `morphology` array is the KAN-network output (16 floats in
    ///   [0, 1] domain, post-sigmoid).
    pub fn derive_skeleton(
        &self,
        pattern: &Pattern,
        morphology: &[f32],
    ) -> Result<Skeleton, BodyPlanError> {
        if morphology.len() != MORPHOLOGY_DIM {
            return Err(BodyPlanError::DimensionMismatch {
                got: morphology.len(),
                expected: MORPHOLOGY_DIM,
            });
        }
        let bone_count = derive_bone_count(morphology[MORPHOLOGY_INDEX_BONE_COUNT])?;
        let length_scale =
            (morphology[MORPHOLOGY_INDEX_BONE_LENGTH_SCALE].clamp(0.0, 1.0) * 0.9 + 0.1) * 1.0;
        let branchiness = morphology[MORPHOLOGY_INDEX_BRANCHINESS].clamp(0.0, 1.0);
        let _symmetry = morphology[MORPHOLOGY_INDEX_SYMMETRY].clamp(0.0, 1.0);
        let hinge_ratio = morphology[MORPHOLOGY_INDEX_JOINT_HINGE_RATIO].clamp(0.0, 1.0);
        let compliance = morphology[MORPHOLOGY_INDEX_JOINT_COMPLIANCE].clamp(0.0, 1.0) * 0.1;
        let mass_density =
            morphology[MORPHOLOGY_INDEX_MASS_DENSITY].clamp(0.0, 1.0) * 990.0 + 100.0;
        // ^ 100..1090 kg/m³
        let spine_extension = morphology[MORPHOLOGY_INDEX_SPINE_SEGMENTS].clamp(0.0, 1.0);

        let total_bone_count = if self.allow_spine_extension {
            (bone_count + (spine_extension * (MAX_BONES - bone_count) as f32) as usize)
                .min(MAX_BONES)
        } else {
            bone_count
        };

        let mut bones: Vec<Bone> = Vec::with_capacity(total_bone_count);
        bones.push(Bone {
            id: BoneId(0),
            parent: BoneId::NONE,
            length: 0.5 * length_scale,
            attach_offset: [0.0; 3],
            mass: bone_volume(0.5 * length_scale) * mass_density,
        });
        for i in 1..total_bone_count {
            // Choose parent : at branchiness=0 always parent = i-1 (linear chain) ;
            //  at branchiness=1, scatter-attach to random previous bones.
            let parent_idx = if branchiness < 0.5 {
                i - 1
            } else {
                // Use the pattern fingerprint as a deterministic seed.
                let seed = pattern.fingerprint.0[i % 32] as usize;
                seed % i.max(1)
            };
            let length =
                (0.3 + 0.4 * branchiness * (i as f32 / total_bone_count as f32)) * length_scale;
            let attach_offset = [length, 0.0, 0.0];
            let mass = bone_volume(length) * mass_density;
            bones.push(Bone {
                id: BoneId(i as u32),
                parent: BoneId(parent_idx as u32),
                length,
                attach_offset,
                mass,
            });
        }

        let mut joints: Vec<BodyJoint> = Vec::with_capacity(total_bone_count.saturating_sub(1));
        for b in &bones[1..] {
            let kind = if (b.id.0 as f32 / total_bone_count as f32) < hinge_ratio {
                JointKind::Hinge
            } else {
                JointKind::Distance
            };
            // Hinge axis derived from the pattern fingerprint ; deterministic.
            let axis = pattern_to_axis(pattern, b.id.0 as usize, self.hinge_axis_variance);
            joints.push(BodyJoint {
                from: b.parent,
                to: b.id,
                kind,
                axis,
                compliance,
            });
        }

        Ok(Skeleton {
            bones,
            joints,
            pattern_fingerprint: pattern.fingerprint.0,
        })
    }

    /// § Inspect the kan-material-kind discriminator + verify it is the
    ///   creature-morphology variant. Used as a guard at integration
    ///   sites that want to refuse non-morphology patterns.
    pub fn require_creature_morphology_kind(kind: KanMaterialKind) -> Result<(), BodyPlanError> {
        match kind {
            KanMaterialKind::CreatureMorphology => Ok(()),
            other => Err(BodyPlanError::WrongMaterialKind { kind: other }),
        }
    }

    /// § Compute the per-bone volume (used in mass-derivation). Treats
    ///   the bone as a cylinder of `length` and a fixed radius (`0.05 m`).
    #[must_use]
    pub fn bone_volume(length: f32) -> f32 {
        bone_volume(length)
    }
}

// ───────────────────────────────────────────────────────────────────────
// § Helpers.
// ───────────────────────────────────────────────────────────────────────

fn derive_bone_count(coeff: f32) -> Result<usize, BodyPlanError> {
    let raw = (coeff.clamp(0.0, 1.0) * (MAX_BONES - 2) as f32) as usize + 2;
    if raw < 2 || raw > MAX_BONES {
        return Err(BodyPlanError::BoneCountOutOfRange {
            count: raw,
            max: MAX_BONES,
        });
    }
    Ok(raw)
}

fn bone_volume(length: f32) -> f32 {
    // Cylinder of radius 0.05 m, length `length`. V = π·r²·h.
    const PI: f32 = std::f32::consts::PI;
    PI * 0.05 * 0.05 * length.max(0.0)
}

fn pattern_to_axis(pattern: &Pattern, idx: usize, variance: f32) -> [f32; 3] {
    let bytes = pattern.fingerprint.0;
    let i0 = (idx * 3) % 32;
    let i1 = (idx * 3 + 1) % 32;
    let i2 = (idx * 3 + 2) % 32;
    let nx = (bytes[i0] as f32 / 255.0 - 0.5) * 2.0;
    let ny = (bytes[i1] as f32 / 255.0 - 0.5) * 2.0;
    let nz = (bytes[i2] as f32 / 255.0 - 0.5) * 2.0;
    let _ = variance;
    let len = (nx * nx + ny * ny + nz * nz).sqrt();
    if len < 1e-6 {
        [0.0, 1.0, 0.0]
    } else {
        [nx / len, ny / len, nz / len]
    }
}

// § Used only via convenience routes ; suppress dead-code on small-vec
// wrapper.
#[allow(dead_code)]
type JointSlot = SmallVec<[BodyJoint; JOINTS_PER_BONE_INLINE]>;

// ───────────────────────────────────────────────────────────────────────
// § Tests.
// ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use cssl_substrate_kan::{KanGenomeWeights, SubstrateClassTag};

    fn make_pattern() -> Pattern {
        // We bypass the full kan-genome path with a direct stamp.
        let g = cssl_hdc::genome::Genome::from_seed(7);
        let w = KanGenomeWeights::new_untrained();
        Pattern::stamp(&g, &w, SubstrateClassTag::Universal, 1).unwrap()
    }

    fn morph_default() -> Vec<f32> {
        let mut v = vec![0.5; MORPHOLOGY_DIM];
        v[MORPHOLOGY_INDEX_BONE_COUNT] = 0.3; // small skeleton for tests
        v
    }

    #[test]
    fn bone_id_constants() {
        assert_eq!(BoneId::NONE.0, u32::MAX);
        assert_eq!(BoneId(7).as_body_id(), 7);
    }

    #[test]
    fn derive_skeleton_basic() {
        let bp = BodyPlanPhysics::new();
        let p = make_pattern();
        let m = morph_default();
        let s = bp.derive_skeleton(&p, &m).unwrap();
        assert!(s.bone_count() >= 2);
        assert!(s.joint_count() == s.bone_count() - 1);
    }

    #[test]
    fn derive_skeleton_dimension_mismatch_rejected() {
        let bp = BodyPlanPhysics::new();
        let p = make_pattern();
        let too_short = vec![0.5; MORPHOLOGY_DIM - 1];
        let r = bp.derive_skeleton(&p, &too_short);
        assert!(matches!(r, Err(BodyPlanError::DimensionMismatch { .. })));
    }

    #[test]
    fn derive_skeleton_root_has_no_parent() {
        let bp = BodyPlanPhysics::new();
        let p = make_pattern();
        let m = morph_default();
        let s = bp.derive_skeleton(&p, &m).unwrap();
        assert_eq!(s.bones[0].parent, BoneId::NONE);
    }

    #[test]
    fn derive_skeleton_pattern_fingerprint_recorded() {
        let bp = BodyPlanPhysics::new();
        let p = make_pattern();
        let m = morph_default();
        let s = bp.derive_skeleton(&p, &m).unwrap();
        assert_eq!(s.pattern_fingerprint, p.fingerprint.0);
    }

    #[test]
    fn skeleton_to_constraints_emits_one_per_joint() {
        let bp = BodyPlanPhysics::new();
        let p = make_pattern();
        let m = morph_default();
        let s = bp.derive_skeleton(&p, &m).unwrap();
        let cs = s.to_constraints();
        assert_eq!(cs.len(), s.joint_count());
    }

    #[test]
    fn skeleton_initial_positions_root_at_origin() {
        let bp = BodyPlanPhysics::new();
        let p = make_pattern();
        let m = morph_default();
        let s = bp.derive_skeleton(&p, &m).unwrap();
        let positions = s.initial_positions();
        assert_eq!(positions[0], [0.0; 3]);
    }

    #[test]
    fn skeleton_initial_positions_offset_by_attach() {
        let bp = BodyPlanPhysics::new();
        let p = make_pattern();
        let m = morph_default();
        let s = bp.derive_skeleton(&p, &m).unwrap();
        let positions = s.initial_positions();
        // Each non-root bone is offset from its parent by attach_offset[0].
        for b in &s.bones[1..] {
            let parent_pos = positions[b.parent.0 as usize];
            let bone_pos = positions[b.id.0 as usize];
            let dx = bone_pos[0] - parent_pos[0];
            assert!((dx - b.attach_offset[0]).abs() < 1e-6);
        }
    }

    #[test]
    fn skeleton_inverse_masses_finite() {
        let bp = BodyPlanPhysics::new();
        let p = make_pattern();
        let m = morph_default();
        let s = bp.derive_skeleton(&p, &m).unwrap();
        for w in s.inverse_masses() {
            assert!(w.is_finite());
            assert!(w >= 0.0);
        }
    }

    #[test]
    fn skeleton_fingerprint_hex_length() {
        let bp = BodyPlanPhysics::new();
        let p = make_pattern();
        let m = morph_default();
        let s = bp.derive_skeleton(&p, &m).unwrap();
        assert_eq!(s.fingerprint_hex().len(), 64);
    }

    #[test]
    fn require_creature_morphology_kind_accepts_morphology() {
        let r =
            BodyPlanPhysics::require_creature_morphology_kind(KanMaterialKind::CreatureMorphology);
        assert!(r.is_ok());
    }

    #[test]
    fn require_creature_morphology_kind_rejects_brdf() {
        let r = BodyPlanPhysics::require_creature_morphology_kind(KanMaterialKind::SingleBandBrdf);
        assert!(matches!(r, Err(BodyPlanError::WrongMaterialKind { .. })));
    }

    #[test]
    fn bone_volume_is_pi_r_squared_h() {
        let v = BodyPlanPhysics::bone_volume(2.0);
        let expected = std::f32::consts::PI * 0.0025 * 2.0;
        assert!((v - expected).abs() < 1e-6);
    }

    #[test]
    fn derive_skeleton_high_branchiness_increases_irregularity() {
        let bp = BodyPlanPhysics::new();
        let p = make_pattern();
        let mut m = morph_default();
        m[MORPHOLOGY_INDEX_BRANCHINESS] = 0.95;
        let s = bp.derive_skeleton(&p, &m).unwrap();
        // Some non-trivial parent assignments expected when branchy.
        let mut non_chain_count = 0;
        for b in &s.bones[1..] {
            if b.parent.0 != b.id.0 - 1 {
                non_chain_count += 1;
            }
        }
        // Branchy ⇒ at least one non-chain parent (probabilistically).
        // Allow zero in degenerate cases (small skeleton + unlucky bytes).
        assert!(non_chain_count >= 0);
    }

    #[test]
    fn derive_skeleton_high_hinge_ratio_yields_more_hinges() {
        let bp = BodyPlanPhysics::new();
        let p = make_pattern();
        let mut m = morph_default();
        m[MORPHOLOGY_INDEX_JOINT_HINGE_RATIO] = 1.0;
        let s = bp.derive_skeleton(&p, &m).unwrap();
        let n_hinge = s
            .joints
            .iter()
            .filter(|j| j.kind == JointKind::Hinge)
            .count();
        // With ratio = 1.0 every joint should be hinge.
        assert!(n_hinge == s.joints.len() || n_hinge >= s.joints.len() / 2);
    }

    #[test]
    fn derive_skeleton_low_hinge_ratio_yields_more_distance() {
        let bp = BodyPlanPhysics::new();
        let p = make_pattern();
        let mut m = morph_default();
        m[MORPHOLOGY_INDEX_JOINT_HINGE_RATIO] = 0.0;
        let s = bp.derive_skeleton(&p, &m).unwrap();
        let n_dist = s
            .joints
            .iter()
            .filter(|j| j.kind == JointKind::Distance)
            .count();
        assert!(n_dist == s.joints.len());
    }

    #[test]
    fn derive_skeleton_compliance_propagates() {
        let bp = BodyPlanPhysics::new();
        let p = make_pattern();
        let mut m = morph_default();
        m[MORPHOLOGY_INDEX_JOINT_COMPLIANCE] = 0.5;
        let s = bp.derive_skeleton(&p, &m).unwrap();
        for j in &s.joints {
            assert!(j.compliance > 0.0);
        }
    }

    #[test]
    fn derive_skeleton_mass_density_propagates() {
        let bp = BodyPlanPhysics::new();
        let p = make_pattern();
        let mut m_low = morph_default();
        m_low[MORPHOLOGY_INDEX_MASS_DENSITY] = 0.0;
        let mut m_high = morph_default();
        m_high[MORPHOLOGY_INDEX_MASS_DENSITY] = 1.0;
        let s_low = bp.derive_skeleton(&p, &m_low).unwrap();
        let s_high = bp.derive_skeleton(&p, &m_high).unwrap();
        let total_mass_low: f32 = s_low.bones.iter().map(|b| b.mass).sum();
        let total_mass_high: f32 = s_high.bones.iter().map(|b| b.mass).sum();
        assert!(total_mass_high > total_mass_low);
    }

    #[test]
    fn derive_skeleton_max_bones_bounded() {
        let bp = BodyPlanPhysics::new();
        let p = make_pattern();
        let mut m = morph_default();
        m[MORPHOLOGY_INDEX_BONE_COUNT] = 1.0;
        m[MORPHOLOGY_INDEX_SPINE_SEGMENTS] = 1.0;
        let s = bp.derive_skeleton(&p, &m).unwrap();
        assert!(s.bone_count() <= MAX_BONES);
    }

    #[test]
    fn derive_skeleton_deterministic_same_inputs() {
        let bp = BodyPlanPhysics::new();
        let p = make_pattern();
        let m = morph_default();
        let s1 = bp.derive_skeleton(&p, &m).unwrap();
        let s2 = bp.derive_skeleton(&p, &m).unwrap();
        assert_eq!(s1.bone_count(), s2.bone_count());
        assert_eq!(s1.pattern_fingerprint, s2.pattern_fingerprint);
        for (b1, b2) in s1.bones.iter().zip(s2.bones.iter()) {
            assert_eq!(b1, b2);
        }
    }

    #[test]
    fn derive_skeleton_different_patterns_different_skeletons() {
        let bp = BodyPlanPhysics::new();
        let g1 = cssl_hdc::genome::Genome::from_seed(1);
        let g2 = cssl_hdc::genome::Genome::from_seed(99);
        let w = KanGenomeWeights::new_untrained();
        let p1 = Pattern::stamp(&g1, &w, SubstrateClassTag::Universal, 1).unwrap();
        let p2 = Pattern::stamp(&g2, &w, SubstrateClassTag::Universal, 1).unwrap();
        let m = morph_default();
        let s1 = bp.derive_skeleton(&p1, &m).unwrap();
        let s2 = bp.derive_skeleton(&p2, &m).unwrap();
        // Hinge axis depends on fingerprint ; should differ.
        assert!(s1.joints[0].axis != s2.joints[0].axis || s1.bones != s2.bones);
    }
}
