//! § ProceduralAnimationWorld — the OmegaSystem aggregate.
//!
//! § THESIS
//!   The world owns a collection of procedural creatures. Each creature
//!   bundles : a skeleton, a KAN pose-network, a physics rig, an IK
//!   solver, a deformation surface, and a body-omnoid stack. The world
//!   ticks per `omega_step`, advancing the time accumulator and walking
//!   each creature through pose-evaluation → physics-IK → deformation →
//!   omnoid-update.
//!
//! § PLACE IN THE STACK
//!   Implements [`cssl_substrate_omega_step::OmegaSystem`] so the world
//!   joins the canonical omega-step phase. The default effect-row is
//!   `{Sim}` ; downstream consumers may union `{Render}` or `{Audio}`
//!   for skinning-upload + animation-driven sound emission as needed.
//!
//! § DETERMINISM
//!   The world is a deterministic function of `(initial state, dt
//!   sequence)`. Replay-determinism : ticking the same sequence of dt
//!   values from the same initial state produces bit-identical pose
//!   output every time.

use cssl_substrate_omega_step::{OmegaError, OmegaStepCtx, OmegaSystem, SystemId};

use crate::deformation::{BoneSegmentDeformation, WaveFieldProbe, ZeroFieldProbe};
use crate::error::ProceduralAnimError;
use crate::genome::{ControlSignal, GenomeHandle};
use crate::kan_pose::KanPoseNetwork;
use crate::omnoid::BodyOmnoidLayers;
use crate::physics_ik::{PhysicsIk, PhysicsRig};
use crate::pose::ProceduralPose;
use crate::skeleton::ProceduralSkeleton;

/// Stable identifier for one creature registered with a
/// [`ProceduralAnimationWorld`]. Issued in monotone-increasing order at
/// registration time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CreatureId(pub u64);

/// One procedural creature. Owns the skeleton + the procedural surfaces
/// that drive it.
#[derive(Debug)]
pub struct ProceduralCreature {
    /// Skeleton bone hierarchy.
    pub skeleton: ProceduralSkeleton,
    /// Genome handle (stable across the creature's lifetime).
    pub genome: GenomeHandle,
    /// KAN pose-network.
    pub pose_network: KanPoseNetwork,
    /// Most recent pose output.
    pub pose: ProceduralPose,
    /// Physics rig (rigid-body bindings).
    pub physics_ik: PhysicsIk,
    /// Bone-segment deformation surface.
    pub deformation: BoneSegmentDeformation,
    /// Five-layer body-omnoid stack.
    pub omnoid: BodyOmnoidLayers,
    /// Per-creature time accumulator.
    pub time: f32,
    /// Most recent control signal.
    pub control: ControlSignal,
}

/// Builder for a single procedural creature.
#[derive(Debug)]
pub struct ProceduralCreatureBuilder {
    skeleton: ProceduralSkeleton,
    genome: GenomeHandle,
    pose_network: Option<KanPoseNetwork>,
    physics_rig: Option<PhysicsRig>,
    control_dim: usize,
}

impl ProceduralCreatureBuilder {
    /// Begin building a creature from a skeleton + genome handle.
    #[must_use]
    pub fn new(skeleton: ProceduralSkeleton, genome: GenomeHandle) -> Self {
        Self {
            skeleton,
            genome,
            pose_network: None,
            physics_rig: None,
            control_dim: 8,
        }
    }

    /// Provide an explicit pose-network. If omitted, a default pose-net
    /// is generated via [`KanPoseNetwork::default_for`].
    #[must_use]
    pub fn with_pose_network(mut self, net: KanPoseNetwork) -> Self {
        self.pose_network = Some(net);
        self
    }

    /// Provide an explicit physics rig. If omitted, a default rig with
    /// `body_id == bone_idx` is generated.
    #[must_use]
    pub fn with_physics_rig(mut self, rig: PhysicsRig) -> Self {
        self.physics_rig = Some(rig);
        self
    }

    /// Override the control-signal dimensionality.
    #[must_use]
    pub fn with_control_dim(mut self, dim: usize) -> Self {
        self.control_dim = dim;
        self
    }

    /// Finish building.
    #[must_use]
    pub fn build(self) -> ProceduralCreature {
        let pose_network = self
            .pose_network
            .unwrap_or_else(|| KanPoseNetwork::default_for(&self.skeleton, self.control_dim));
        let physics_rig = self
            .physics_rig
            .unwrap_or_else(|| PhysicsRig::default_for(&self.skeleton));
        let mut pose = ProceduralPose::new();
        pose.resize_to_skeleton(&self.skeleton);
        let mut omnoid = BodyOmnoidLayers::new();
        omnoid.resize(&self.skeleton);
        let physics_ik = PhysicsIk::new(physics_rig);
        ProceduralCreature {
            skeleton: self.skeleton,
            genome: self.genome,
            pose_network,
            pose,
            physics_ik,
            deformation: BoneSegmentDeformation::new(),
            omnoid,
            time: 0.0,
            control: ControlSignal::zero(self.control_dim),
        }
    }
}

/// Aggregate of procedural creatures + the per-tick orchestration.
#[derive(Debug)]
pub struct ProceduralAnimationWorld {
    creatures: Vec<(CreatureId, ProceduralCreature)>,
    next_id: u64,
    /// Global time accumulator. Per-creature time is also tracked
    /// independently to support time-warp / pause-per-creature.
    pub global_time: f32,
}

impl Default for ProceduralAnimationWorld {
    fn default() -> Self {
        Self::new()
    }
}

impl ProceduralAnimationWorld {
    /// New empty world.
    #[must_use]
    pub fn new() -> Self {
        Self {
            creatures: Vec::new(),
            next_id: 1,
            global_time: 0.0,
        }
    }

    /// Register a creature ; returns its stable id.
    pub fn register(&mut self, creature: ProceduralCreature) -> CreatureId {
        let id = CreatureId(self.next_id);
        self.next_id += 1;
        self.creatures.push((id, creature));
        id
    }

    /// Despawn a creature by id ; returns whether the creature was found.
    pub fn despawn(&mut self, id: CreatureId) -> bool {
        if let Some(pos) = self.creatures.iter().position(|(cid, _)| *cid == id) {
            self.creatures.remove(pos);
            true
        } else {
            false
        }
    }

    /// Read a creature.
    #[must_use]
    pub fn creature(&self, id: CreatureId) -> Option<&ProceduralCreature> {
        self.creatures
            .iter()
            .find(|(cid, _)| *cid == id)
            .map(|(_, c)| c)
    }

    /// Mutable creature access.
    pub fn creature_mut(&mut self, id: CreatureId) -> Option<&mut ProceduralCreature> {
        self.creatures
            .iter_mut()
            .find(|(cid, _)| *cid == id)
            .map(|(_, c)| c)
    }

    /// Number of registered creatures.
    #[must_use]
    pub fn creature_count(&self) -> usize {
        self.creatures.len()
    }

    /// Iterate over (id, creature) pairs.
    pub fn iter(&self) -> impl Iterator<Item = (CreatureId, &ProceduralCreature)> {
        self.creatures.iter().map(|(id, c)| (*id, c))
    }

    /// Set a creature's control signal.
    pub fn set_control(
        &mut self,
        id: CreatureId,
        signal: ControlSignal,
    ) -> Result<(), ProceduralAnimError> {
        let creature = self
            .creature_mut(id)
            .ok_or(ProceduralAnimError::UnknownCreature(id.0))?;
        if signal.dim() != creature.control.dim() {
            return Err(ProceduralAnimError::ControlSignalShapeMismatch {
                got: signal.dim(),
                expected: creature.control.dim(),
            });
        }
        creature.control = signal;
        Ok(())
    }

    /// Default tick using a `ZeroFieldProbe` (no wave-field forcing).
    /// Useful for unit tests + bring-up. Production callers should use
    /// [`Self::tick_with_probe`].
    pub fn tick(&mut self, dt: f32) -> Result<(), ProceduralAnimError> {
        let probe = ZeroFieldProbe;
        self.tick_with_probe(dt, &probe)
    }

    /// Full tick with a wave-field probe. Walks each creature through :
    ///   1. Pose-evaluation (KAN-pose network).
    ///   2. Compute model matrices.
    ///   3. Compute deformation samples (probe sampled at every bone tip).
    ///   4. Update body-omnoid layers.
    ///   5. (Optional) physics-IK pass — applied via [`Self::run_ik`] for
    ///      callers that want to integrate with their host physics tick.
    pub fn tick_with_probe<P: WaveFieldProbe>(
        &mut self,
        dt: f32,
        probe: &P,
    ) -> Result<(), ProceduralAnimError> {
        let dt_clamped = dt.max(0.0);
        self.global_time += dt_clamped;
        for (_, c) in self.creatures.iter_mut() {
            c.time += dt_clamped;
            // 1. Pose evaluation.
            c.pose_network.evaluate_pose(
                &c.genome.embedding,
                c.time,
                &c.control,
                &c.skeleton,
                &mut c.pose,
            )?;
            // 2. Model matrices.
            c.pose.compute_model_matrices(&c.skeleton);
            // 3. Deformation : extract bone world-positions from the
            //    model matrices.
            let positions: Vec<cssl_substrate_projections::Vec3> = c
                .pose
                .model_matrices()
                .iter()
                .map(|m| {
                    cssl_substrate_projections::Vec3::new(m.cols[3][0], m.cols[3][1], m.cols[3][2])
                })
                .collect();
            c.deformation
                .compute(&c.skeleton, &positions, probe, dt_clamped);
            // 4. Body-omnoid update.
            c.omnoid.resize(&c.skeleton);
            // Aura : pull the genome's MANA-axis (index-23, by convention).
            let mana = c.genome.embedding.values[23];
            c.omnoid.update_aura(mana, &c.control);
            c.omnoid.update_flesh(c.deformation.samples());
            c.omnoid.update_bone(&c.skeleton, &c.pose);
            c.omnoid.update_soul();
        }
        Ok(())
    }

    /// Run a physics-IK pass for a particular creature. Caller-driven so
    /// the host application can interleave physics-IK with its own
    /// physics solver. The `body_world_pose_fn` callback resolves
    /// `body_id → Motor` from the host physics world.
    pub fn run_ik<F>(
        &mut self,
        id: CreatureId,
        dt: f32,
        body_world_pose_fn: F,
    ) -> Result<crate::physics_ik::PhysicsIkOutcome, ProceduralAnimError>
    where
        F: FnMut(u64) -> cssl_pga::Motor,
    {
        let creature = self
            .creature_mut(id)
            .ok_or(ProceduralAnimError::UnknownCreature(id.0))?;
        creature.physics_ik.step(
            &creature.skeleton,
            &mut creature.pose,
            body_world_pose_fn,
            dt,
        )
    }
}

impl OmegaSystem for ProceduralAnimationWorld {
    fn step(&mut self, _ctx: &mut OmegaStepCtx<'_>, dt: f64) -> Result<(), OmegaError> {
        // Convert the f64 dt to f32 for the procedural runtime ; this is
        // the standard convention across the substrate (animation runs
        // in f32-space ; physics in f64-space ; math is dimensionless).
        match self.tick(dt as f32) {
            Ok(()) => Ok(()),
            Err(e) => Err(OmegaError::SystemPanicked {
                system: SystemId(0),
                name: "cssl-anim-procedural".to_string(),
                frame: 0,
                msg: e.to_string(),
            }),
        }
    }

    fn name(&self) -> &str {
        "cssl-anim-procedural"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::GenomeEmbedding;
    use crate::skeleton::{Bone, ROOT_PARENT};
    use crate::transform::Transform;

    fn make_creature(id: u64) -> ProceduralCreature {
        let skel = ProceduralSkeleton::from_bones(vec![
            Bone::new("root", ROOT_PARENT, Transform::IDENTITY),
            Bone::new("a", 0, Transform::IDENTITY),
            Bone::new("b", 1, Transform::IDENTITY),
        ])
        .unwrap();
        let genome = GenomeHandle::new(
            id,
            GenomeEmbedding::from_values([0.5; crate::genome::GENOME_DIM]),
        );
        ProceduralCreatureBuilder::new(skel, genome).build()
    }

    #[test]
    fn world_starts_empty() {
        let w = ProceduralAnimationWorld::new();
        assert_eq!(w.creature_count(), 0);
    }

    #[test]
    fn register_returns_monotone_ids() {
        let mut w = ProceduralAnimationWorld::new();
        let id1 = w.register(make_creature(1));
        let id2 = w.register(make_creature(2));
        assert!(id1 < id2);
    }

    #[test]
    fn creature_lookup_returns_creature() {
        let mut w = ProceduralAnimationWorld::new();
        let id = w.register(make_creature(1));
        assert!(w.creature(id).is_some());
    }

    #[test]
    fn despawn_removes_creature() {
        let mut w = ProceduralAnimationWorld::new();
        let id = w.register(make_creature(1));
        assert!(w.despawn(id));
        assert!(w.creature(id).is_none());
    }

    #[test]
    fn despawn_unknown_returns_false() {
        let mut w = ProceduralAnimationWorld::new();
        assert!(!w.despawn(CreatureId(999)));
    }

    #[test]
    fn tick_advances_global_time() {
        let mut w = ProceduralAnimationWorld::new();
        w.register(make_creature(1));
        w.tick(0.016).unwrap();
        assert!((w.global_time - 0.016).abs() < 1e-6);
    }

    #[test]
    fn tick_advances_per_creature_time() {
        let mut w = ProceduralAnimationWorld::new();
        let id = w.register(make_creature(1));
        w.tick(0.05).unwrap();
        assert!((w.creature(id).unwrap().time - 0.05).abs() < 1e-6);
    }

    #[test]
    fn tick_clamps_negative_dt_to_zero() {
        let mut w = ProceduralAnimationWorld::new();
        w.register(make_creature(1));
        w.tick(-1.0).unwrap();
        assert_eq!(w.global_time, 0.0);
    }

    #[test]
    fn tick_writes_pose_for_creature() {
        let mut w = ProceduralAnimationWorld::new();
        let id = w.register(make_creature(1));
        w.tick(0.016).unwrap();
        let c = w.creature(id).unwrap();
        assert_eq!(c.pose.bone_count(), c.skeleton.bone_count());
    }

    #[test]
    fn determinism_two_worlds_with_same_inputs_match() {
        let mut w1 = ProceduralAnimationWorld::new();
        let mut w2 = ProceduralAnimationWorld::new();
        let id1 = w1.register(make_creature(7));
        let id2 = w2.register(make_creature(7));
        for _ in 0..10 {
            w1.tick(0.016).unwrap();
            w2.tick(0.016).unwrap();
        }
        let p1 = &w1.creature(id1).unwrap().pose;
        let p2 = &w2.creature(id2).unwrap().pose;
        for i in 0..p1.bone_count() {
            assert_eq!(p1.local_transform(i), p2.local_transform(i));
        }
    }

    #[test]
    fn set_control_persists_signal() {
        let mut w = ProceduralAnimationWorld::new();
        let id = w.register(make_creature(1));
        let mut signal = ControlSignal::zero(8);
        signal.set_component(0, 0.7);
        w.set_control(id, signal).unwrap();
        assert!((w.creature(id).unwrap().control.forward_speed() - 0.7).abs() < 1e-6);
    }

    #[test]
    fn set_control_unknown_creature_errors() {
        let mut w = ProceduralAnimationWorld::new();
        let r = w.set_control(CreatureId(99), ControlSignal::zero(8));
        assert!(matches!(r, Err(ProceduralAnimError::UnknownCreature(99))));
    }

    #[test]
    fn set_control_dim_mismatch_errors() {
        let mut w = ProceduralAnimationWorld::new();
        let id = w.register(make_creature(1));
        let r = w.set_control(id, ControlSignal::zero(4));
        assert!(matches!(
            r,
            Err(ProceduralAnimError::ControlSignalShapeMismatch { .. })
        ));
    }

    #[test]
    fn run_ik_returns_outcome() {
        let mut w = ProceduralAnimationWorld::new();
        let id = w.register(make_creature(1));
        let outcome = w.run_ik(id, 0.016, |_| cssl_pga::Motor::IDENTITY).unwrap();
        // No constraints registered ⇒ converged with zero error.
        assert!(outcome.converged);
    }

    #[test]
    fn omega_system_name_is_canonical() {
        let w = ProceduralAnimationWorld::new();
        assert_eq!(w.name(), "cssl-anim-procedural");
    }

    #[test]
    fn iter_yields_all_creatures() {
        let mut w = ProceduralAnimationWorld::new();
        let _i1 = w.register(make_creature(1));
        let _i2 = w.register(make_creature(2));
        let collected: Vec<CreatureId> = w.iter().map(|(id, _)| id).collect();
        assert_eq!(collected.len(), 2);
    }

    #[test]
    fn omnoid_layers_resized_after_tick() {
        let mut w = ProceduralAnimationWorld::new();
        let id = w.register(make_creature(1));
        w.tick(0.016).unwrap();
        let c = w.creature(id).unwrap();
        assert_eq!(
            c.omnoid
                .layer(crate::omnoid::OmnoidLayerKind::Aura)
                .bone_count(),
            c.skeleton.bone_count()
        );
    }
}
