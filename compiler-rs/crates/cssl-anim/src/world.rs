//! `AnimationWorld` — aggregates skeletons + active clip instances + blend
//! trees and ticks per `omega_step`. Implements
//! [`cssl_substrate_omega_step::OmegaSystem`].
//!
//! § THESIS
//!   The animation world is the runtime container for live animation
//!   state. It holds :
//!     - registered skeletons (one per rigged character)
//!     - active clip instances (one per running clip — a clip + a
//!       skeleton + a phase)
//!     - per-skeleton output poses
//!   Each omega tick advances every clip instance's phase by `dt` and
//!   re-samples the underlying clip into the matching pose. Downstream
//!   systems (renderer, IK, blend-tree post-processing) read the resulting
//!   poses out of the world.
//!
//! § DETERMINISM
//!   The world's tick function is a pure function of `(world_state, dt)`.
//!   No clock reads, no entropy. Replaying the same dt sequence on
//!   identical initial state produces bit-identical pose outputs across
//!   runs.
//!
//! § PRIME-DIRECTIVE
//!   - **Effect-row** : `{Sim}` by default. Animation evaluation does not
//!     require `{Render}` ; the renderer reads poses out of the world
//!     under its own effect-row.
//!   - **Consent** : registration of an `AnimationWorld` as a system in
//!     the omega scheduler requires the standard
//!     `caps_grant(omega_register)` from the sibling
//!     `cssl-substrate-omega-step` crate.

use std::collections::BTreeMap;

use cssl_substrate_omega_step::{
    EffectRow, OmegaError, OmegaStepCtx, OmegaSystem, RngStreamId, SystemId,
};

use crate::clip::AnimationClip;
use crate::error::AnimError;
use crate::pose::Pose;
use crate::sampler::AnimSampler;
use crate::skeleton::Skeleton;

/// Stable identifier for a registered skeleton.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SkeletonId(pub u64);

/// Stable identifier for a registered clip instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ClipInstanceId(pub u64);

/// One running animation : a clip + the skeleton it drives + the current
/// phase (in seconds, before wrap).
#[derive(Debug, Clone)]
pub struct ClipInstance {
    /// Skeleton this instance writes into.
    pub skeleton: SkeletonId,
    /// The clip itself. Stage-0 stores by-value ; production builds may
    /// keep clips in a separate pool keyed by handle.
    pub clip: AnimationClip,
    /// Current phase in seconds, before wrap. Advanced by `dt * speed`
    /// each tick.
    pub phase: f32,
    /// Playback speed multiplier. 1.0 = real-time, 0.0 = paused, 2.0 =
    /// double speed. Negative speeds run the clip in reverse.
    pub speed: f32,
    /// Whether to loop the clip when the phase exceeds the duration.
    /// `true` wraps via `clip.wrap_time` ; `false` clamps and stops.
    pub looping: bool,
}

impl ClipInstance {
    /// Construct a fresh instance with default speed (1.0) + looping
    /// enabled.
    #[must_use]
    pub fn new(skeleton: SkeletonId, clip: AnimationClip) -> Self {
        Self {
            skeleton,
            clip,
            phase: 0.0,
            speed: 1.0,
            looping: true,
        }
    }

    /// Builder method : set the playback speed.
    #[must_use]
    pub fn with_speed(mut self, speed: f32) -> Self {
        self.speed = speed;
        self
    }

    /// Builder method : disable looping (clip clamps + stops at end).
    #[must_use]
    pub fn with_looping(mut self, looping: bool) -> Self {
        self.looping = looping;
        self
    }

    /// Advance the phase by `dt * speed` seconds, wrapping or clamping
    /// based on `looping`.
    pub fn advance(&mut self, dt: f32) {
        self.phase += dt * self.speed;
        if self.looping {
            self.phase = self.clip.wrap_time(self.phase);
        } else {
            self.phase = self.phase.clamp(0.0, self.clip.duration);
        }
    }
}

/// The animation runtime container.
///
/// § FIELDS
///   - `skeletons` : registered skeletons keyed by id.
///   - `instances` : active clip instances keyed by id.
///   - `poses` : per-skeleton output pose, refreshed each tick.
///   - `sampler` : the sampler used for evaluation. Stage-0 uses default
///     configuration ; the caller can override via `set_sampler`.
///   - `next_skeleton_id` / `next_instance_id` : monotone counters.
///
/// § STAGE-0 BEHAVIOUR
///   Each tick, every clip instance is :
///     1. Advanced by `dt * speed`.
///     2. Sampled into the skeleton's pose ; multiple instances on the
///        same skeleton overwrite each other in instance-id order.
///     3. The skeleton's model-space matrices are recomputed.
///   For multi-clip blending, callers should drive a `BlendTree` outside
///   the world and write the result back via `set_pose`.
#[derive(Debug, Clone)]
pub struct AnimationWorld {
    skeletons: BTreeMap<SkeletonId, Skeleton>,
    instances: BTreeMap<ClipInstanceId, ClipInstance>,
    poses: BTreeMap<SkeletonId, Pose>,
    sampler: AnimSampler,
    next_skeleton_id: u64,
    next_instance_id: u64,
    /// Number of ticks executed by this world. Useful for diagnostics +
    /// invariant assertions across replay.
    pub tick_count: u64,
}

impl AnimationWorld {
    /// Construct an empty world.
    #[must_use]
    pub fn new() -> Self {
        Self {
            skeletons: BTreeMap::new(),
            instances: BTreeMap::new(),
            poses: BTreeMap::new(),
            sampler: AnimSampler::new(),
            next_skeleton_id: 0,
            next_instance_id: 0,
            tick_count: 0,
        }
    }

    /// Replace the sampler. Useful for runtime knob tweaking + tests.
    pub fn set_sampler(&mut self, sampler: AnimSampler) {
        self.sampler = sampler;
    }

    /// Read-only access to the active sampler.
    #[must_use]
    pub fn sampler(&self) -> &AnimSampler {
        &self.sampler
    }

    /// Register a skeleton. Returns a fresh `SkeletonId`.
    pub fn register_skeleton(&mut self, skeleton: Skeleton) -> SkeletonId {
        let id = SkeletonId(self.next_skeleton_id);
        self.next_skeleton_id += 1;
        let initial_pose = Pose::from_bind_pose(&skeleton);
        self.poses.insert(id, initial_pose);
        self.skeletons.insert(id, skeleton);
        id
    }

    /// Spawn a clip instance against a registered skeleton. Returns
    /// `Err(AnimError::UnknownSkeleton)` if the skeleton is not registered.
    pub fn spawn_clip_instance(
        &mut self,
        instance: ClipInstance,
    ) -> Result<ClipInstanceId, AnimError> {
        if !self.skeletons.contains_key(&instance.skeleton) {
            return Err(AnimError::UnknownSkeleton {
                id: instance.skeleton.0,
            });
        }
        let id = ClipInstanceId(self.next_instance_id);
        self.next_instance_id += 1;
        self.instances.insert(id, instance);
        Ok(id)
    }

    /// Despawn a clip instance.
    pub fn despawn_clip_instance(&mut self, id: ClipInstanceId) -> Result<(), AnimError> {
        if self.instances.remove(&id).is_none() {
            return Err(AnimError::UnknownClipInstance { id: id.0 });
        }
        Ok(())
    }

    /// Read-only access to a skeleton by id.
    pub fn skeleton(&self, id: SkeletonId) -> Result<&Skeleton, AnimError> {
        self.skeletons
            .get(&id)
            .ok_or(AnimError::UnknownSkeleton { id: id.0 })
    }

    /// Read-only access to a clip instance.
    pub fn clip_instance(&self, id: ClipInstanceId) -> Result<&ClipInstance, AnimError> {
        self.instances
            .get(&id)
            .ok_or(AnimError::UnknownClipInstance { id: id.0 })
    }

    /// Mutable access to a clip instance — for runtime knob adjustments
    /// like setting the playback speed mid-flight.
    pub fn clip_instance_mut(
        &mut self,
        id: ClipInstanceId,
    ) -> Result<&mut ClipInstance, AnimError> {
        self.instances
            .get_mut(&id)
            .ok_or(AnimError::UnknownClipInstance { id: id.0 })
    }

    /// Read-only access to the current pose of a registered skeleton.
    pub fn pose(&self, id: SkeletonId) -> Result<&Pose, AnimError> {
        self.poses
            .get(&id)
            .ok_or(AnimError::UnknownSkeleton { id: id.0 })
    }

    /// Mutable access to a pose — for callers that drive their own
    /// blend tree and want to write the final pose back into the world.
    pub fn pose_mut(&mut self, id: SkeletonId) -> Result<&mut Pose, AnimError> {
        self.poses
            .get_mut(&id)
            .ok_or(AnimError::UnknownSkeleton { id: id.0 })
    }

    /// Replace the pose of a skeleton — the canonical hook for writing a
    /// blend-tree result back into the world.
    pub fn set_pose(&mut self, id: SkeletonId, pose: Pose) -> Result<(), AnimError> {
        if !self.skeletons.contains_key(&id) {
            return Err(AnimError::UnknownSkeleton { id: id.0 });
        }
        self.poses.insert(id, pose);
        Ok(())
    }

    /// Number of registered skeletons.
    #[must_use]
    pub fn skeleton_count(&self) -> usize {
        self.skeletons.len()
    }

    /// Number of active clip instances.
    #[must_use]
    pub fn instance_count(&self) -> usize {
        self.instances.len()
    }

    /// Internal tick implementation — called by `OmegaSystem::step` and
    /// also by direct test fixtures that don't bring up the full
    /// scheduler.
    ///
    /// § ALGORITHM
    ///   1. For each instance in id order : advance phase, then sample
    ///      the clip into the skeleton's pose.
    ///   2. After all sampling, recompute model-space matrices for every
    ///      skeleton's pose (single forward sweep per skeleton).
    pub fn tick(&mut self, dt: f32) -> Result<(), AnimError> {
        // Phase advance + sample.
        for (_id, inst) in self.instances.iter_mut() {
            inst.advance(dt);
            let skel = self
                .skeletons
                .get(&inst.skeleton)
                .ok_or(AnimError::UnknownSkeleton {
                    id: inst.skeleton.0,
                })?;
            let pose = self
                .poses
                .get_mut(&inst.skeleton)
                .ok_or(AnimError::UnknownSkeleton {
                    id: inst.skeleton.0,
                })?;
            self.sampler.sample(&inst.clip, inst.phase, skel, pose)?;
        }
        // Recompute model-space matrices once per skeleton.
        for (id, skel) in &self.skeletons {
            if let Some(pose) = self.poses.get_mut(id) {
                pose.recompute_model_transforms(skel);
            }
        }
        self.tick_count += 1;
        Ok(())
    }
}

impl Default for AnimationWorld {
    fn default() -> Self {
        Self::new()
    }
}

impl OmegaSystem for AnimationWorld {
    fn step(&mut self, _ctx: &mut OmegaStepCtx<'_>, dt: f64) -> Result<(), OmegaError> {
        // Cast `dt` to f32 — animation precision is f32 throughout the
        // pipeline. We keep the `dt: f64` interface to match the omega-step
        // ABI ; the cast is well-defined for the timestep ranges
        // animation runs in (microseconds to ~1 sec).
        if let Err(e) = self.tick(dt as f32) {
            // Translate AnimError into a SystemPanicked omega failure ;
            // include the diagnostic code for audit-walker bucketing.
            return Err(OmegaError::SystemPanicked {
                system: SystemId(0),
                name: self.name().to_string(),
                frame: 0,
                msg: format!("[{}] {}", e.code(), e),
            });
        }
        Ok(())
    }

    fn name(&self) -> &str {
        "cssl-anim::AnimationWorld"
    }

    fn effect_row(&self) -> EffectRow {
        EffectRow::sim()
    }

    fn rng_streams(&self) -> &[RngStreamId] {
        &[]
    }
}

#[cfg(test)]
mod tests {
    use super::{AnimationWorld, ClipInstance, ClipInstanceId, SkeletonId};
    use crate::clip::{AnimChannel, AnimationClip, Interpolation, KeyframeT};
    use crate::error::AnimError;
    use crate::skeleton::{Bone, Skeleton, ROOT_PARENT};
    use crate::transform::Transform;
    use cssl_substrate_omega_step::{EffectRow, OmegaSystem, SubstrateEffect};
    use cssl_substrate_projections::Vec3;

    fn approx_eq(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() <= eps
    }

    fn make_skel() -> Skeleton {
        let bones = vec![
            Bone::new("root", ROOT_PARENT, Transform::IDENTITY),
            Bone::new("b1", 0, Transform::IDENTITY),
        ];
        Skeleton::from_bones(bones).expect("ok")
    }

    fn make_clip() -> AnimationClip {
        let ch = AnimChannel::translation(
            1,
            Interpolation::Linear,
            vec![
                KeyframeT {
                    time: 0.0,
                    value: Vec3::ZERO,
                },
                KeyframeT {
                    time: 1.0,
                    value: Vec3::new(10.0, 0.0, 0.0),
                },
            ],
        )
        .expect("ok");
        AnimationClip::new("walk", vec![ch])
    }

    #[test]
    fn empty_world_has_no_skeletons() {
        let w = AnimationWorld::new();
        assert_eq!(w.skeleton_count(), 0);
        assert_eq!(w.instance_count(), 0);
    }

    #[test]
    fn register_skeleton_assigns_unique_ids() {
        let mut w = AnimationWorld::new();
        let id_a = w.register_skeleton(make_skel());
        let id_b = w.register_skeleton(make_skel());
        assert_ne!(id_a, id_b);
        assert_eq!(w.skeleton_count(), 2);
    }

    #[test]
    fn spawn_instance_with_unknown_skeleton_errors() {
        let mut w = AnimationWorld::new();
        let bogus = SkeletonId(999);
        let result = w.spawn_clip_instance(ClipInstance::new(bogus, make_clip()));
        assert!(matches!(result, Err(AnimError::UnknownSkeleton { .. })));
    }

    #[test]
    fn despawn_unknown_instance_errors() {
        let mut w = AnimationWorld::new();
        let bogus = ClipInstanceId(999);
        let result = w.despawn_clip_instance(bogus);
        assert!(matches!(result, Err(AnimError::UnknownClipInstance { .. })));
    }

    #[test]
    fn tick_advances_phase_and_updates_pose() {
        let mut w = AnimationWorld::new();
        let s_id = w.register_skeleton(make_skel());
        let inst = ClipInstance::new(s_id, make_clip());
        let _i_id = w.spawn_clip_instance(inst).expect("ok");
        // Tick by 0.5 seconds — phase advances to 0.5 ⇒ translation = 5.
        w.tick(0.5).expect("tick");
        let pose = w.pose(s_id).expect("pose");
        assert!(approx_eq(pose.local_transforms[1].translation.x, 5.0, 1e-4));
    }

    #[test]
    fn tick_increments_tick_count() {
        let mut w = AnimationWorld::new();
        assert_eq!(w.tick_count, 0);
        w.tick(0.016).expect("tick");
        w.tick(0.016).expect("tick");
        assert_eq!(w.tick_count, 2);
    }

    #[test]
    fn looping_clip_wraps_phase() {
        let mut w = AnimationWorld::new();
        let s_id = w.register_skeleton(make_skel());
        let inst = ClipInstance::new(s_id, make_clip());
        let i_id = w.spawn_clip_instance(inst).expect("ok");
        // Tick by 1.5 — looping should wrap phase to 0.5 (clip duration 1.0).
        w.tick(1.5).expect("ok");
        let phase = w.clip_instance(i_id).expect("ok").phase;
        assert!(approx_eq(phase, 0.5, 1e-4));
    }

    #[test]
    fn non_looping_clip_clamps_phase() {
        let mut w = AnimationWorld::new();
        let s_id = w.register_skeleton(make_skel());
        let inst = ClipInstance::new(s_id, make_clip()).with_looping(false);
        let i_id = w.spawn_clip_instance(inst).expect("ok");
        w.tick(2.0).expect("ok");
        let phase = w.clip_instance(i_id).expect("ok").phase;
        assert!(approx_eq(phase, 1.0, 1e-4));
    }

    #[test]
    fn speed_multiplier_scales_phase_advance() {
        let mut w = AnimationWorld::new();
        let s_id = w.register_skeleton(make_skel());
        let inst = ClipInstance::new(s_id, make_clip()).with_speed(2.0);
        let i_id = w.spawn_clip_instance(inst).expect("ok");
        w.tick(0.25).expect("ok");
        // 0.25 * 2.0 = 0.5.
        let phase = w.clip_instance(i_id).expect("ok").phase;
        assert!(approx_eq(phase, 0.5, 1e-4));
    }

    #[test]
    fn omega_system_name_set() {
        let w = AnimationWorld::new();
        assert!(w.name().contains("AnimationWorld"));
    }

    #[test]
    fn omega_system_default_effect_row_is_sim() {
        let w = AnimationWorld::new();
        let row: EffectRow = w.effect_row();
        assert!(row.contains(SubstrateEffect::Sim));
    }

    #[test]
    fn omega_system_no_rng_streams_by_default() {
        let w = AnimationWorld::new();
        assert!(w.rng_streams().is_empty());
    }

    #[test]
    fn determinism_two_worlds_same_dt_same_pose() {
        // Two identically-seeded worlds with identical inputs must produce
        // identical poses. Replay-determinism foundation.
        let mut w1 = AnimationWorld::new();
        let s1 = w1.register_skeleton(make_skel());
        w1.spawn_clip_instance(ClipInstance::new(s1, make_clip()))
            .expect("ok");
        let mut w2 = AnimationWorld::new();
        let s2 = w2.register_skeleton(make_skel());
        w2.spawn_clip_instance(ClipInstance::new(s2, make_clip()))
            .expect("ok");
        for _ in 0..10 {
            w1.tick(0.05).expect("ok");
            w2.tick(0.05).expect("ok");
        }
        let p1 = w1.pose(s1).expect("pose");
        let p2 = w2.pose(s2).expect("pose");
        assert_eq!(p1.local_transforms, p2.local_transforms);
    }

    #[test]
    fn pose_mut_allows_external_writes() {
        let mut w = AnimationWorld::new();
        let s = w.register_skeleton(make_skel());
        let pose = w.pose_mut(s).expect("ok");
        pose.local_transforms[1] = Transform::from_translation(Vec3::new(99.0, 0.0, 0.0));
        let read_back = w.pose(s).expect("ok");
        assert!(approx_eq(
            read_back.local_transforms[1].translation.x,
            99.0,
            1e-5
        ));
    }

    #[test]
    fn set_pose_unknown_skeleton_errors() {
        let mut w = AnimationWorld::new();
        let bogus = SkeletonId(99);
        let pose = crate::pose::Pose::identity(2);
        assert!(matches!(
            w.set_pose(bogus, pose),
            Err(AnimError::UnknownSkeleton { .. })
        ));
    }
}
