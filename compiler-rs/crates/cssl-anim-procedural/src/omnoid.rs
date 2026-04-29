//! § BodyOmnoidLayers — five-layer (Aura / Flesh / Bone / Machine / Soul)
//!   body integration per the omega-field substrate.
//!
//! § THESIS
//!   The substrate `08_BODY/03_DIMENSIONAL_TRAVEL.csl` and the cssl-substrate-
//!   omega-field crate define a creature's body as a stack of FIVE layers,
//!   each a projection of the creature's state onto a different field :
//!
//!   - **Aura** — the emission / radiance field. Carries the creature's
//!     light output (energy-being glow, fur-tip iridescence). Driven by
//!     the genome's axis-MANA channel and the creature's emotional
//!     intensity (control-signal channel 7 + active-inference variance).
//!   - **Flesh** — the soft-body / skin layer. Carries deformation,
//!     muscle-bulge, fat-jiggle, blood-flush. Driven by the
//!     [`crate::deformation::BoneSegmentDeformation`] samples.
//!   - **Bone** — the rigid-skeleton layer. The classic skeletal-
//!     animation surface : bone-local transforms + cumulative model-
//!     matrices. The procedural-pose-network drives this layer.
//!   - **Machine** — the prosthetic / equipment / armor layer. Holds
//!     attached objects with their own physics constraints (sword
//!     belted to hip, hat pinned to head, mech-suit articulation).
//!   - **Soul** — the consent + identity carrier. The Sovereign-Φ link
//!     (phi-table handle) lives here. Updates only when the Pattern
//!     fingerprint changes (substrate-class translation, Σ-mask gate ;
//!     never on routine pose evaluation).
//!
//!   Each layer is a separate state object updated independently each
//!   tick. The renderer composes them in a fixed order
//!   (Bone → Flesh → Machine → Aura → Soul-overlay). The order is fixed
//!   so the build is deterministic ; the layers themselves are
//!   stateless w.r.t. each other (no inter-layer feedback).
//!
//! § DETERMINISM
//!   All five layer updates are deterministic functions of their inputs.
//!   No clock reads, no entropy, no global state.
//!
//! § STAGE-0 LIMITATIONS
//!   - **Soul layer** is structurally present but its update rule is a
//!     no-op pending the Soul-link network spec (deferred wave-3γ).
//!   - **Machine layer** is structurally present with a per-attachment
//!     transform field but the attachment-pinning-to-physics-body
//!     reactive logic is deferred ; stage-0 carries a static-pose snapshot.

use cssl_substrate_projections::Vec3;

use crate::deformation::DeformationSample;
use crate::genome::ControlSignal;
use crate::pose::ProceduralPose;
use crate::skeleton::ProceduralSkeleton;

/// Discriminator for the five body-omnoid layers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OmnoidLayerKind {
    /// Aura — emission / radiance.
    Aura,
    /// Flesh — soft-body / skin.
    Flesh,
    /// Bone — rigid skeleton.
    Bone,
    /// Machine — prosthetics / equipment / armor.
    Machine,
    /// Soul — consent + identity.
    Soul,
}

/// One layer's state snapshot. Generic over the kind tag ; the per-kind
/// state is held in a small inline payload.
#[derive(Debug, Clone)]
pub struct OmnoidLayer {
    /// Layer kind tag.
    pub kind: OmnoidLayerKind,
    /// Per-bone scalar value : interpretation depends on `kind`.
    ///   - Aura : emission intensity in `[0, infty)`.
    ///   - Flesh : deformation amplitude (signed).
    ///   - Bone : rigidity factor in `[0, 1]` (1 = fully rigid).
    ///   - Machine : attachment "presence" (0 = no attachment, 1 = pinned).
    ///   - Soul : layer is sentinel-only, payload is always 0.
    pub per_bone_scalar: Vec<f32>,
    /// Per-bone vector value : interpretation depends on `kind`.
    ///   - Aura : light direction (zero = isotropic emission).
    ///   - Flesh : deformation displacement.
    ///   - Bone : bone-tip world-position (set by the model-matrix sweep).
    ///   - Machine : attachment world-offset.
    ///   - Soul : sentinel (always Vec3::ZERO).
    pub per_bone_vector: Vec<Vec3>,
}

impl OmnoidLayer {
    /// Construct an empty layer of the given kind.
    #[must_use]
    pub fn new(kind: OmnoidLayerKind) -> Self {
        Self {
            kind,
            per_bone_scalar: Vec::new(),
            per_bone_vector: Vec::new(),
        }
    }

    /// Resize the layer to match the skeleton's bone count.
    pub fn resize(&mut self, n: usize) {
        self.per_bone_scalar.resize(n, 0.0);
        self.per_bone_vector.resize(n, Vec3::ZERO);
    }

    /// Bone count covered by this layer.
    #[must_use]
    pub fn bone_count(&self) -> usize {
        self.per_bone_scalar.len()
    }
}

/// One projection of the creature's state into a layer-specific output.
/// The renderer reads this projection and produces the per-layer visual.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OmnoidProjection {
    /// Bone this projection is anchored to.
    pub bone_idx: usize,
    /// Layer kind.
    pub kind: OmnoidLayerKind,
    /// Scalar value (interpretation depends on `kind`).
    pub scalar: f32,
    /// Vector value (interpretation depends on `kind`).
    pub vector: Vec3,
}

/// Configuration for the body-omnoid update.
#[derive(Debug, Clone, Copy)]
pub struct BodyOmnoidConfig {
    /// Aura emission gain — multiplies the genome's MANA-axis to produce
    /// the per-bone emission intensity.
    pub aura_gain: f32,
    /// Flesh deformation gain — multiplies the deformation samples.
    pub flesh_gain: f32,
    /// Bone rigidity offset — added to each bone's stiffness to produce
    /// the layer's rigidity scalar. Lets callers tune skeleton "softness"
    /// at the layer level without mutating per-bone stiffness.
    pub bone_rigidity_offset: f32,
    /// Whether the Soul layer participates in the tick. Stage-0 default
    /// is `false` (layer is structurally present but not updated).
    pub soul_active: bool,
}

impl Default for BodyOmnoidConfig {
    fn default() -> Self {
        Self {
            aura_gain: 1.0,
            flesh_gain: 1.0,
            bone_rigidity_offset: 0.0,
            soul_active: false,
        }
    }
}

/// The full five-layer body-omnoid stack.
#[derive(Debug, Clone)]
pub struct BodyOmnoidLayers {
    aura: OmnoidLayer,
    flesh: OmnoidLayer,
    bone: OmnoidLayer,
    machine: OmnoidLayer,
    soul: OmnoidLayer,
    config: BodyOmnoidConfig,
}

impl BodyOmnoidLayers {
    /// New empty stack with default config.
    #[must_use]
    pub fn new() -> Self {
        Self::with_config(BodyOmnoidConfig::default())
    }

    /// New stack with explicit config.
    #[must_use]
    pub fn with_config(config: BodyOmnoidConfig) -> Self {
        Self {
            aura: OmnoidLayer::new(OmnoidLayerKind::Aura),
            flesh: OmnoidLayer::new(OmnoidLayerKind::Flesh),
            bone: OmnoidLayer::new(OmnoidLayerKind::Bone),
            machine: OmnoidLayer::new(OmnoidLayerKind::Machine),
            soul: OmnoidLayer::new(OmnoidLayerKind::Soul),
            config,
        }
    }

    /// Resize all layers to match the skeleton's bone count.
    pub fn resize(&mut self, skeleton: &ProceduralSkeleton) {
        let n = skeleton.bone_count();
        self.aura.resize(n);
        self.flesh.resize(n);
        self.bone.resize(n);
        self.machine.resize(n);
        self.soul.resize(n);
    }

    /// Read a layer.
    #[must_use]
    pub fn layer(&self, kind: OmnoidLayerKind) -> &OmnoidLayer {
        match kind {
            OmnoidLayerKind::Aura => &self.aura,
            OmnoidLayerKind::Flesh => &self.flesh,
            OmnoidLayerKind::Bone => &self.bone,
            OmnoidLayerKind::Machine => &self.machine,
            OmnoidLayerKind::Soul => &self.soul,
        }
    }

    /// Mutable layer access.
    pub fn layer_mut(&mut self, kind: OmnoidLayerKind) -> &mut OmnoidLayer {
        match kind {
            OmnoidLayerKind::Aura => &mut self.aura,
            OmnoidLayerKind::Flesh => &mut self.flesh,
            OmnoidLayerKind::Bone => &mut self.bone,
            OmnoidLayerKind::Machine => &mut self.machine,
            OmnoidLayerKind::Soul => &mut self.soul,
        }
    }

    /// Update the Aura layer from the genome's MANA-axis intensity (a
    /// scalar typically pulled from the genome's index-23 channel) and
    /// the control_signal's emotional-amplitude channel.
    pub fn update_aura(&mut self, mana_intensity: f32, control: &ControlSignal) {
        let n = self.aura.bone_count();
        let breath = control.breathing();
        let combined = mana_intensity * self.config.aura_gain * (1.0 + breath);
        for i in 0..n {
            self.aura.per_bone_scalar[i] = combined.max(0.0);
            self.aura.per_bone_vector[i] = Vec3::ZERO; // isotropic by default
        }
    }

    /// Update the Flesh layer from deformation samples.
    pub fn update_flesh(&mut self, samples: &[DeformationSample]) {
        for s in samples {
            if s.bone_idx >= self.flesh.bone_count() {
                continue;
            }
            self.flesh.per_bone_scalar[s.bone_idx] = s.amplitude * self.config.flesh_gain;
            self.flesh.per_bone_vector[s.bone_idx] = s.displacement * self.config.flesh_gain;
        }
    }

    /// Update the Bone layer from the skeleton's per-bone stiffness +
    /// the cumulative model-matrix translations.
    pub fn update_bone(&mut self, skeleton: &ProceduralSkeleton, pose: &ProceduralPose) {
        let n = skeleton.bone_count();
        for i in 0..n {
            let stiffness = skeleton.bone(i).map_or(1.0, |b| b.stiffness);
            self.bone.per_bone_scalar[i] =
                (stiffness + self.config.bone_rigidity_offset).clamp(0.0, 1.0);
            // Read the bone-tip world-position from the model-matrix's
            // translation column.
            if let Some(m) = pose.model_matrix(i) {
                self.bone.per_bone_vector[i] = Vec3::new(m.cols[3][0], m.cols[3][1], m.cols[3][2]);
            }
        }
    }

    /// Update the Machine layer from caller-supplied attachment transforms.
    pub fn update_machine(&mut self, attachments: &[(usize, Vec3)]) {
        // First clear all attachments.
        for v in &mut self.machine.per_bone_vector {
            *v = Vec3::ZERO;
        }
        for s in &mut self.machine.per_bone_scalar {
            *s = 0.0;
        }
        for (bone_idx, offset) in attachments {
            if *bone_idx < self.machine.bone_count() {
                self.machine.per_bone_vector[*bone_idx] = *offset;
                self.machine.per_bone_scalar[*bone_idx] = 1.0;
            }
        }
    }

    /// Update the Soul layer. Stage-0 : no-op unless `soul_active` is
    /// set ; the layer is structurally present so callers can iterate
    /// uniformly.
    pub fn update_soul(&mut self) {
        if !self.config.soul_active {
            return;
        }
        // Stage-0 : the spec defers Soul-link network detail to wave-3γ.
        // We populate per-bone-scalar with the bone-rigidity overlay
        // (mirror of the Bone layer) as a placeholder ; replace once the
        // Soul-link spec lands.
        for i in 0..self.soul.bone_count() {
            self.soul.per_bone_scalar[i] = self.bone.per_bone_scalar[i];
        }
    }

    /// Iterate every active projection across all five layers. Useful for
    /// the renderer's per-tick layer-fan-out path.
    #[must_use]
    pub fn projections(&self) -> Vec<OmnoidProjection> {
        let mut out: Vec<OmnoidProjection> = Vec::new();
        let layers = [
            (OmnoidLayerKind::Aura, &self.aura),
            (OmnoidLayerKind::Flesh, &self.flesh),
            (OmnoidLayerKind::Bone, &self.bone),
            (OmnoidLayerKind::Machine, &self.machine),
            (OmnoidLayerKind::Soul, &self.soul),
        ];
        for (kind, layer) in layers {
            for i in 0..layer.bone_count() {
                let scalar = layer.per_bone_scalar[i];
                let vector = layer.per_bone_vector[i];
                if scalar.abs() > 1e-6 || vector.length_squared() > 1e-12 {
                    out.push(OmnoidProjection {
                        bone_idx: i,
                        kind,
                        scalar,
                        vector,
                    });
                }
            }
        }
        out
    }

    /// Configuration accessor.
    #[must_use]
    pub fn config(&self) -> &BodyOmnoidConfig {
        &self.config
    }
}

impl Default for BodyOmnoidLayers {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skeleton::{Bone, ROOT_PARENT};
    use crate::transform::Transform;

    fn make_skel() -> ProceduralSkeleton {
        ProceduralSkeleton::from_bones(vec![
            Bone::new("root", ROOT_PARENT, Transform::IDENTITY).with_stiffness(0.9),
            Bone::new("a", 0, Transform::IDENTITY).with_stiffness(0.5),
        ])
        .unwrap()
    }

    #[test]
    fn new_layers_have_zero_bone_count() {
        let l = BodyOmnoidLayers::new();
        assert_eq!(l.layer(OmnoidLayerKind::Aura).bone_count(), 0);
    }

    #[test]
    fn resize_grows_all_five_layers() {
        let mut l = BodyOmnoidLayers::new();
        let s = make_skel();
        l.resize(&s);
        for kind in [
            OmnoidLayerKind::Aura,
            OmnoidLayerKind::Flesh,
            OmnoidLayerKind::Bone,
            OmnoidLayerKind::Machine,
            OmnoidLayerKind::Soul,
        ] {
            assert_eq!(l.layer(kind).bone_count(), s.bone_count());
        }
    }

    #[test]
    fn aura_update_writes_scalars() {
        let mut l = BodyOmnoidLayers::new();
        let s = make_skel();
        l.resize(&s);
        let c = ControlSignal::zero(8);
        l.update_aura(2.0, &c);
        for v in &l.layer(OmnoidLayerKind::Aura).per_bone_scalar {
            assert!(*v >= 0.0);
        }
    }

    #[test]
    fn flesh_update_propagates_samples() {
        let mut l = BodyOmnoidLayers::new();
        let s = make_skel();
        l.resize(&s);
        let samples = vec![DeformationSample {
            bone_idx: 1,
            displacement: Vec3::new(0.1, 0.0, 0.0),
            amplitude: 0.1,
        }];
        l.update_flesh(&samples);
        let f = l.layer(OmnoidLayerKind::Flesh);
        assert!(f.per_bone_vector[1].length() > 0.0);
        assert!((f.per_bone_scalar[1] - 0.1).abs() < 1e-6);
    }

    #[test]
    fn bone_update_writes_stiffness() {
        let mut l = BodyOmnoidLayers::new();
        let s = make_skel();
        l.resize(&s);
        let mut p = ProceduralPose::new();
        p.resize_to_skeleton(&s);
        l.update_bone(&s, &p);
        let b = l.layer(OmnoidLayerKind::Bone);
        assert!((b.per_bone_scalar[0] - 0.9).abs() < 1e-5);
        assert!((b.per_bone_scalar[1] - 0.5).abs() < 1e-5);
    }

    #[test]
    fn machine_update_pins_attachments() {
        let mut l = BodyOmnoidLayers::new();
        let s = make_skel();
        l.resize(&s);
        let attachments = vec![(0, Vec3::new(0.0, 0.5, 0.0))];
        l.update_machine(&attachments);
        let m = l.layer(OmnoidLayerKind::Machine);
        assert!((m.per_bone_scalar[0] - 1.0).abs() < 1e-6);
        assert_eq!(m.per_bone_vector[0], Vec3::new(0.0, 0.5, 0.0));
        assert_eq!(m.per_bone_scalar[1], 0.0);
    }

    #[test]
    fn soul_update_no_op_by_default() {
        let mut l = BodyOmnoidLayers::new();
        let s = make_skel();
        l.resize(&s);
        l.update_soul();
        let so = l.layer(OmnoidLayerKind::Soul);
        for v in &so.per_bone_scalar {
            assert_eq!(*v, 0.0);
        }
    }

    #[test]
    fn soul_update_active_mirrors_bone() {
        let cfg = BodyOmnoidConfig {
            soul_active: true,
            ..BodyOmnoidConfig::default()
        };
        let mut l = BodyOmnoidLayers::with_config(cfg);
        let s = make_skel();
        l.resize(&s);
        let mut p = ProceduralPose::new();
        p.resize_to_skeleton(&s);
        l.update_bone(&s, &p);
        l.update_soul();
        let so = l.layer(OmnoidLayerKind::Soul);
        let bo = l.layer(OmnoidLayerKind::Bone);
        for i in 0..s.bone_count() {
            assert_eq!(so.per_bone_scalar[i], bo.per_bone_scalar[i]);
        }
    }

    #[test]
    fn projections_collect_active_entries() {
        let mut l = BodyOmnoidLayers::new();
        let s = make_skel();
        l.resize(&s);
        let c = ControlSignal::zero(8);
        l.update_aura(1.0, &c);
        let projs = l.projections();
        assert!(projs.len() >= s.bone_count());
        for p in &projs {
            if p.kind == OmnoidLayerKind::Aura {
                assert!(p.scalar > 0.0);
            }
        }
    }

    #[test]
    fn projections_skip_zero_layers() {
        let l = BodyOmnoidLayers::new();
        // No resize, no updates ⇒ no projections.
        assert!(l.projections().is_empty());
    }

    #[test]
    fn flesh_skips_oob_bone_idx() {
        let mut l = BodyOmnoidLayers::new();
        let s = make_skel();
        l.resize(&s);
        let samples = vec![DeformationSample {
            bone_idx: 99,
            displacement: Vec3::new(1.0, 0.0, 0.0),
            amplitude: 1.0,
        }];
        l.update_flesh(&samples);
        let f = l.layer(OmnoidLayerKind::Flesh);
        for v in &f.per_bone_vector {
            assert_eq!(*v, Vec3::ZERO);
        }
    }

    #[test]
    fn config_default_soul_inactive() {
        let c = BodyOmnoidConfig::default();
        assert!(!c.soul_active);
    }

    #[test]
    fn machine_clear_then_repin() {
        let mut l = BodyOmnoidLayers::new();
        let s = make_skel();
        l.resize(&s);
        l.update_machine(&[(0, Vec3::new(1.0, 0.0, 0.0))]);
        l.update_machine(&[]);
        let m = l.layer(OmnoidLayerKind::Machine);
        for v in &m.per_bone_vector {
            assert_eq!(*v, Vec3::ZERO);
        }
    }
}
