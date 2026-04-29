//! § KanPoseNetwork — KAN-driven bone-local pose generator.
//!
//! § THESIS
//!   KAN(genome, time, control\_signal) yields a bone-local Transform stream.
//!
//!   The pose-network is the procedural-runtime substitute for keyframes.
//!   Instead of an artist authoring a `walk` clip and the runtime sampling
//!   it at `t = 0.42`, the runtime computes the bone-local transform
//!   directly :
//!
//!   ```text
//!   pose[bone_i] = decode(KAN.evaluate(input_vector))
//!   input_vector = concat(genome_embedding, time_phase, control_signal)
//!   ```
//!
//! § STORAGE
//!   Per-creature : a single [`KanPoseNetwork`] holding :
//!     - reference to the creature's genome embedding (32-D)
//!     - per-bone channel bindings (which output components feed which
//!       bone's translation / rotation / scale)
//!     - the underlying KAN spline-net evaluator (lightweight inline
//!       version ; full evaluator lands when `cssl-kan` graduates).
//!
//! § SPLINE EVALUATION — STAGE-0 INLINE FORM
//!   The substrate `KanNetwork<I, O>` carries a `control_points` grid +
//!   `knot_grid` + `spline_basis` tag, but the full evaluator is the
//!   responsibility of the `cssl-kan` slice (T11-D115 / wave-3β-04).
//!   For procedural-pose stage-0 we emit poses via a deterministic
//!   composition of :
//!     1. **Linear projection** of the input vector through a per-channel
//!        weight pair `(w_in, w_out)` derived from the genome embedding.
//!     2. **Periodic activation** : a phase-shifted sine over the time-
//!        phase channels produces the smooth periodic motion.
//!     3. **Output decoding** : per-bone (translation / rotation / scale)
//!        decoded from a small float fan via the channel-binding map.
//!
//!   The whole thing is byte-stable + deterministic. When `cssl-kan`
//!   graduates with a real spline-evaluator, this inline form is replaced
//!   by `KanNetwork::evaluate(input)` ; the public surface
//!   ([`KanPoseNetwork::evaluate_pose`]) does NOT change.
//!
//! § DETERMINISM
//!   `evaluate_pose(genome, time, control)` is bit-identical across runs
//!   for identical inputs. No clock reads, no randomness, no global state.

use cssl_substrate_projections::{Quat, Vec3};

use crate::error::ProceduralAnimError;
use crate::genome::{encode_time_phase, ControlSignal, GenomeEmbedding, GENOME_DIM};
use crate::pose::ProceduralPose;
use crate::skeleton::ProceduralSkeleton;
use crate::transform::Transform;

/// Maximum number of channels (translation / rotation / scale slots) the
/// pose-network can drive in stage-0. A more granular fan-out is possible
/// once `cssl-kan` graduates ; stage-0 chooses 3 channels per bone (T, R,
/// S) which gives `KAN_BONE_CHANNELS / 3` bones.
pub const KAN_BONE_CHANNELS: usize = 192;

/// Output components per bone : translation (3) + rotation (4 quaternion)
/// + scale (3) = 10 floats. Matches the `Transform` shape.
pub const COMPONENTS_PER_BONE: usize = 10;

/// Channel kind — which component of the `Transform` a particular KAN
/// channel feeds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BoneChannelKind {
    /// Output drives the bone's translation (3 floats).
    Translation,
    /// Output drives the bone's rotation (4 floats, quaternion).
    Rotation,
    /// Output drives the bone's scale (3 floats).
    Scale,
}

/// One bone's channel binding — points at the slice of the KAN output
/// vector that feeds this bone.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KanPoseChannel {
    /// Bone this channel writes to.
    pub bone_idx: usize,
    /// Channel kind : T / R / S.
    pub kind: BoneChannelKind,
    /// Index into the output fan (start of the slice). The slice length
    /// is determined by `kind` (3 for T/S, 4 for R).
    pub output_offset: usize,
    /// Phase offset applied to the time-phase contribution. Lets two
    /// bones drive at different phase relations (e.g. left/right legs
    /// 180° out of phase for a gait).
    pub phase_offset: f32,
    /// Frequency band weight — emphasizes a specific time-phase band for
    /// this channel. Typical values : `1.0` for general motion, `2.0`+
    /// for fast oscillations (whisker twitch, fast tail flick).
    pub frequency_scale: f32,
    /// Amplitude — output is scaled by this value before being written
    /// to the bone. `1.0` = full range ; `0.1` = subtle motion.
    pub amplitude: f32,
}

impl KanPoseChannel {
    /// Construct a translation channel.
    #[must_use]
    pub const fn translation(bone_idx: usize, output_offset: usize) -> Self {
        Self {
            bone_idx,
            kind: BoneChannelKind::Translation,
            output_offset,
            phase_offset: 0.0,
            frequency_scale: 1.0,
            amplitude: 1.0,
        }
    }

    /// Construct a rotation channel.
    #[must_use]
    pub const fn rotation(bone_idx: usize, output_offset: usize) -> Self {
        Self {
            bone_idx,
            kind: BoneChannelKind::Rotation,
            output_offset,
            phase_offset: 0.0,
            frequency_scale: 1.0,
            amplitude: 1.0,
        }
    }

    /// Construct a scale channel.
    #[must_use]
    pub const fn scale(bone_idx: usize, output_offset: usize) -> Self {
        Self {
            bone_idx,
            kind: BoneChannelKind::Scale,
            output_offset,
            phase_offset: 0.0,
            frequency_scale: 1.0,
            amplitude: 1.0,
        }
    }

    /// Builder : set the phase offset.
    #[must_use]
    pub const fn with_phase_offset(mut self, phase: f32) -> Self {
        self.phase_offset = phase;
        self
    }

    /// Builder : set the frequency scale.
    #[must_use]
    pub const fn with_frequency_scale(mut self, scale: f32) -> Self {
        self.frequency_scale = scale;
        self
    }

    /// Builder : set the amplitude.
    #[must_use]
    pub const fn with_amplitude(mut self, amplitude: f32) -> Self {
        self.amplitude = amplitude;
        self
    }
}

/// The KAN pose-network. Owns the channel bindings + a deterministic
/// inline-evaluator for stage-0 ; swap to a full `KanNetwork::evaluate`
/// path once `cssl-kan` graduates.
#[derive(Debug, Clone)]
pub struct KanPoseNetwork {
    /// Per-bone channel bindings. Indexed by insertion order.
    channels: Vec<KanPoseChannel>,
    /// Number of bones the network expects. Determined at construction
    /// from the maximum bone index in the channel set.
    bone_count: usize,
    /// Control-signal dimensionality the network was constructed for.
    control_dim: usize,
}

impl KanPoseNetwork {
    /// Construct an empty pose network. Add channels via
    /// [`KanPoseNetwork::add_channel`] before calling
    /// [`KanPoseNetwork::evaluate_pose`].
    #[must_use]
    pub fn empty(bone_count: usize, control_dim: usize) -> Self {
        Self {
            channels: Vec::new(),
            bone_count,
            control_dim,
        }
    }

    /// Construct a default network for the given skeleton : every bone
    /// gets a translation channel + rotation channel + scale channel,
    /// each with a unique time-phase offset derived from the bone index.
    /// This produces a reasonable "idle breathing / sway" baseline that
    /// callers can tune by inspecting + replacing channels.
    #[must_use]
    pub fn default_for(skeleton: &ProceduralSkeleton, control_dim: usize) -> Self {
        let mut net = Self::empty(skeleton.bone_count(), control_dim);
        for i in 0..skeleton.bone_count() {
            // Distribute phase offsets so bones desync visually.
            let phase = (i as f32) * 0.37;
            net.channels.push(
                KanPoseChannel::translation(i, i * COMPONENTS_PER_BONE)
                    .with_phase_offset(phase)
                    .with_amplitude(0.02),
            );
            net.channels.push(
                KanPoseChannel::rotation(i, i * COMPONENTS_PER_BONE + 3)
                    .with_phase_offset(phase + 0.5)
                    .with_amplitude(0.04),
            );
            net.channels.push(
                KanPoseChannel::scale(i, i * COMPONENTS_PER_BONE + 7)
                    .with_phase_offset(phase + 1.0)
                    .with_amplitude(0.01),
            );
        }
        net
    }

    /// Add a channel binding.
    pub fn add_channel(&mut self, channel: KanPoseChannel) {
        if channel.bone_idx >= self.bone_count {
            self.bone_count = channel.bone_idx + 1;
        }
        self.channels.push(channel);
    }

    /// Bone count the network expects.
    #[must_use]
    pub fn bone_count(&self) -> usize {
        self.bone_count
    }

    /// Read-only access to the channel bindings.
    #[must_use]
    pub fn channels(&self) -> &[KanPoseChannel] {
        &self.channels
    }

    /// Control-signal dimensionality.
    #[must_use]
    pub fn control_dim(&self) -> usize {
        self.control_dim
    }

    /// Evaluate the pose network at `(genome, time, control)`. Writes
    /// bone-local transforms into the supplied `pose`. The pose's bone
    /// count is grown to match the network's bone count if needed.
    ///
    /// The result is a deterministic function of the inputs ; identical
    /// inputs produce identical pose output across runs.
    pub fn evaluate_pose(
        &self,
        genome: &GenomeEmbedding,
        time: f32,
        control: &ControlSignal,
        skeleton: &ProceduralSkeleton,
        pose: &mut ProceduralPose,
    ) -> Result<PoseEvaluation, ProceduralAnimError> {
        if control.dim() != self.control_dim {
            return Err(ProceduralAnimError::ControlSignalShapeMismatch {
                got: control.dim(),
                expected: self.control_dim,
            });
        }

        // Initialize / grow the pose to bone count.
        pose.resize_to_skeleton(skeleton);

        // Seed each bone's transform from the bind-pose so unbound
        // channels (e.g. bones the network doesn't drive) preserve
        // their bind transform.
        for i in 0..skeleton.bone_count() {
            if let Some(bone) = skeleton.bone(i) {
                pose.set_local_transform(i, bone.local_bind_transform);
            }
        }

        // Build the time-phase vector once.
        let time_phase = encode_time_phase(time);

        // Apply each channel binding.
        let mut channels_evaluated = 0usize;
        for ch in &self.channels {
            if ch.bone_idx >= skeleton.bone_count() {
                continue;
            }
            // Evaluate the inline KAN-style synthesis. The output is a
            // byte-stable function of (genome, time, control, channel).
            let output = synthesize_channel_output(genome, &time_phase, control, ch);
            // Write into the bone's transform.
            Self::write_channel_to_pose(ch, &output, pose)?;
            channels_evaluated += 1;
        }

        Ok(PoseEvaluation {
            channels_evaluated,
            time,
        })
    }

    /// Write a synthesized channel output to the pose at the channel's
    /// bone index.
    fn write_channel_to_pose(
        ch: &KanPoseChannel,
        output: &[f32; 4],
        pose: &mut ProceduralPose,
    ) -> Result<(), ProceduralAnimError> {
        let count = pose.bone_count();
        if ch.bone_idx >= count {
            return Err(ProceduralAnimError::BoneIndexOutOfRange {
                bone_idx: ch.bone_idx,
                bone_count: count,
            });
        }
        let mut t = pose
            .local_transform(ch.bone_idx)
            .unwrap_or(Transform::IDENTITY);
        match ch.kind {
            BoneChannelKind::Translation => {
                let amp = ch.amplitude;
                t.translation = Vec3::new(
                    t.translation.x + output[0] * amp,
                    t.translation.y + output[1] * amp,
                    t.translation.z + output[2] * amp,
                );
            }
            BoneChannelKind::Rotation => {
                // The synthesized output gives a small-angle delta we
                // compose onto the bind rotation. Treat the first 3
                // outputs as bivector components ; the 4th as a sign.
                let amp = ch.amplitude;
                let (sx, sy, sz) = (output[0] * amp, output[1] * amp, output[2] * amp);
                let len_sq = sx * sx + sy * sy + sz * sz;
                let half = len_sq.sqrt() * 0.5;
                let cos_half = half.cos();
                let sin_half_over_len = if len_sq > f32::EPSILON {
                    half.sin() / len_sq.sqrt()
                } else {
                    0.5
                };
                let dq = Quat {
                    x: sx * sin_half_over_len,
                    y: sy * sin_half_over_len,
                    z: sz * sin_half_over_len,
                    w: cos_half,
                };
                t.rotation = crate::transform::quat_mul(t.rotation, dq);
            }
            BoneChannelKind::Scale => {
                let amp = ch.amplitude;
                t.scale = Vec3::new(
                    t.scale.x * (1.0 + output[0] * amp),
                    t.scale.y * (1.0 + output[1] * amp),
                    t.scale.z * (1.0 + output[2] * amp),
                );
            }
        }
        pose.set_local_transform(ch.bone_idx, t);
        Ok(())
    }
}

/// Result of a pose evaluation. Used by callers that want to inspect the
/// number of channels driven this tick (for debug overlays and unit tests).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PoseEvaluation {
    /// Number of channels evaluated.
    pub channels_evaluated: usize,
    /// Time the pose was evaluated at.
    pub time: f32,
}

/// Synthesize the 4-component output of a single channel. Stage-0 inline
/// form ; replaced by `KanNetwork::evaluate` once `cssl-kan` graduates.
///
/// Output is a deterministic function of (genome, time-phase, control,
/// channel). Each output component is built as :
///
///   `out_k = Σ_i genome[g_i] * time_phase[t_i] * control[c_i] * w_ki`
///
/// where the `g_i`, `t_i`, `c_i`, `w_ki` are derived from a deterministic
/// hash of the channel's bone-index + kind + offset. The whole function
/// is total + closed under bounded-output (every `out_k ∈ [-1, 1]`).
fn synthesize_channel_output(
    genome: &GenomeEmbedding,
    time_phase: &[f32; 8],
    control: &ControlSignal,
    ch: &KanPoseChannel,
) -> [f32; 4] {
    let mut out = [0.0_f32; 4];
    for k in 0..4 {
        let mut acc = 0.0;
        for band in 0..time_phase.len() {
            let g_idx = ((ch.bone_idx.wrapping_mul(11))
                + ch.output_offset
                + band
                + (k * 5)
                + match ch.kind {
                    BoneChannelKind::Translation => 0,
                    BoneChannelKind::Rotation => 1,
                    BoneChannelKind::Scale => 2,
                })
                % GENOME_DIM;
            let g = genome.values[g_idx];
            let phase_term = time_phase[band] * ch.frequency_scale + ch.phase_offset.sin() * 0.5;
            // Control modulation : pick a control channel
            // deterministically.
            let c_idx = if control.dim() == 0 {
                0
            } else {
                (band + (k * 3) + ch.bone_idx) % control.dim()
            };
            let c = if control.dim() == 0 {
                1.0
            } else {
                control.values()[c_idx]
            };
            // Smooth weighting ; tanh keeps the output bounded.
            let weight = (g * phase_term + c * 0.25).tanh();
            acc += weight;
        }
        // Average and clamp to keep the output bounded ; the
        // amplitude on the channel is what determines visible motion.
        out[k] = (acc / time_phase.len() as f32).clamp(-1.0, 1.0);
    }
    out
}

/// Seed a KAN pose network from a genome embedding. Used for testing +
/// for the bring-up path : produces a deterministic pose-net with
/// channels initialized from the genome's structural hash.
pub fn seed_from_genome(
    skeleton: &ProceduralSkeleton,
    genome: &GenomeEmbedding,
    control_dim: usize,
) -> KanPoseNetwork {
    let mut net = KanPoseNetwork::default_for(skeleton, control_dim);
    let hash = genome.stable_hash();
    // Tweak amplitudes from the genome hash so different genomes produce
    // visually distinct idle motion.
    for (i, ch) in net.channels.iter_mut().enumerate() {
        let shift = ((hash >> (i & 63)) & 0xFF) as f32 / 255.0; // 0..1
        ch.amplitude *= 0.5 + shift;
        ch.phase_offset += shift * std::f32::consts::TAU;
    }
    net
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skeleton::{Bone, ROOT_PARENT};

    fn make_skel() -> ProceduralSkeleton {
        ProceduralSkeleton::from_bones(vec![
            Bone::new("root", ROOT_PARENT, Transform::IDENTITY),
            Bone::new("spine", 0, Transform::IDENTITY),
            Bone::new("head", 1, Transform::IDENTITY),
        ])
        .unwrap()
    }

    #[test]
    fn default_network_has_three_channels_per_bone() {
        let s = make_skel();
        let n = KanPoseNetwork::default_for(&s, 8);
        assert_eq!(n.channels().len(), s.bone_count() * 3);
    }

    #[test]
    fn evaluate_pose_writes_all_bones() {
        let s = make_skel();
        let n = KanPoseNetwork::default_for(&s, 8);
        let g = GenomeEmbedding::from_values([0.1; GENOME_DIM]);
        let c = ControlSignal::zero(8);
        let mut pose = ProceduralPose::new();
        let r = n.evaluate_pose(&g, 0.0, &c, &s, &mut pose).unwrap();
        assert_eq!(r.channels_evaluated, 3 * s.bone_count());
        assert_eq!(pose.bone_count(), s.bone_count());
    }

    #[test]
    fn deterministic_identical_inputs_produce_identical_pose() {
        let s = make_skel();
        let n = KanPoseNetwork::default_for(&s, 8);
        let g = GenomeEmbedding::from_values([0.3; GENOME_DIM]);
        let c = ControlSignal::zero(8);
        let mut p1 = ProceduralPose::new();
        let mut p2 = ProceduralPose::new();
        n.evaluate_pose(&g, 0.5, &c, &s, &mut p1).unwrap();
        n.evaluate_pose(&g, 0.5, &c, &s, &mut p2).unwrap();
        for i in 0..s.bone_count() {
            assert_eq!(p1.local_transform(i), p2.local_transform(i));
        }
    }

    #[test]
    fn different_time_produces_different_pose_for_nonzero_amplitude() {
        let s = make_skel();
        let mut n = KanPoseNetwork::default_for(&s, 8);
        // Boost amplitude to make the difference obvious.
        for ch in &mut n.channels {
            ch.amplitude *= 5.0;
        }
        let g = GenomeEmbedding::from_values([0.4; GENOME_DIM]);
        let c = ControlSignal::zero(8);
        let mut p1 = ProceduralPose::new();
        let mut p2 = ProceduralPose::new();
        n.evaluate_pose(&g, 0.0, &c, &s, &mut p1).unwrap();
        n.evaluate_pose(&g, 0.5, &c, &s, &mut p2).unwrap();
        let mut differs = false;
        for i in 0..s.bone_count() {
            if p1.local_transform(i) != p2.local_transform(i) {
                differs = true;
                break;
            }
        }
        assert!(differs, "pose at t=0 and t=0.5 should differ");
    }

    #[test]
    fn control_dim_mismatch_is_error() {
        let s = make_skel();
        let n = KanPoseNetwork::default_for(&s, 8);
        let g = GenomeEmbedding::ZERO;
        let c = ControlSignal::zero(4);
        let mut p = ProceduralPose::new();
        let r = n.evaluate_pose(&g, 0.0, &c, &s, &mut p);
        assert!(matches!(
            r,
            Err(ProceduralAnimError::ControlSignalShapeMismatch { .. })
        ));
    }

    #[test]
    fn channel_out_of_range_for_bone_idx_skipped() {
        let s = make_skel();
        let mut n = KanPoseNetwork::empty(s.bone_count(), 8);
        // Add a channel for a bone that doesn't exist in the skeleton.
        n.add_channel(KanPoseChannel::translation(99, 0));
        let g = GenomeEmbedding::ZERO;
        let c = ControlSignal::zero(8);
        let mut p = ProceduralPose::new();
        // Should not error : channel is silently skipped.
        let r = n.evaluate_pose(&g, 0.0, &c, &s, &mut p).unwrap();
        assert_eq!(r.channels_evaluated, 0);
    }

    #[test]
    fn output_components_are_bounded_in_unit_range() {
        let g = GenomeEmbedding::from_values([1.0; GENOME_DIM]);
        let c = ControlSignal::zero(8);
        let tp = encode_time_phase(0.25);
        let ch = KanPoseChannel::rotation(0, 0).with_amplitude(1.0);
        let out = synthesize_channel_output(&g, &tp, &c, &ch);
        for v in out {
            assert!((-1.0..=1.0).contains(&v), "output {v} out of bounds");
        }
    }

    #[test]
    fn seed_from_genome_produces_distinct_amplitudes_per_genome() {
        let s = make_skel();
        let g1 = GenomeEmbedding::from_values([0.1; GENOME_DIM]);
        let g2 = GenomeEmbedding::from_values([0.9; GENOME_DIM]);
        let n1 = seed_from_genome(&s, &g1, 8);
        let n2 = seed_from_genome(&s, &g2, 8);
        let mut differs = false;
        for (c1, c2) in n1.channels.iter().zip(n2.channels.iter()) {
            if (c1.amplitude - c2.amplitude).abs() > 1e-6 {
                differs = true;
                break;
            }
        }
        assert!(
            differs,
            "different genomes should produce different amplitudes"
        );
    }

    #[test]
    fn channel_kind_translation_const_works() {
        let ch = KanPoseChannel::translation(2, 4);
        assert_eq!(ch.kind, BoneChannelKind::Translation);
        assert_eq!(ch.bone_idx, 2);
        assert_eq!(ch.output_offset, 4);
    }

    #[test]
    fn channel_kind_rotation_const_works() {
        let ch = KanPoseChannel::rotation(1, 7);
        assert_eq!(ch.kind, BoneChannelKind::Rotation);
    }

    #[test]
    fn channel_kind_scale_const_works() {
        let ch = KanPoseChannel::scale(0, 3);
        assert_eq!(ch.kind, BoneChannelKind::Scale);
    }

    #[test]
    fn channel_with_phase_offset_assigns() {
        let ch = KanPoseChannel::rotation(0, 0).with_phase_offset(1.5);
        assert_eq!(ch.phase_offset, 1.5);
    }

    #[test]
    fn add_channel_grows_bone_count() {
        let mut n = KanPoseNetwork::empty(2, 8);
        assert_eq!(n.bone_count(), 2);
        n.add_channel(KanPoseChannel::translation(5, 0));
        assert_eq!(n.bone_count(), 6);
    }

    #[test]
    fn pose_evaluation_carries_time() {
        let s = make_skel();
        let n = KanPoseNetwork::default_for(&s, 8);
        let g = GenomeEmbedding::ZERO;
        let c = ControlSignal::zero(8);
        let mut p = ProceduralPose::new();
        let r = n.evaluate_pose(&g, 1.25, &c, &s, &mut p).unwrap();
        assert_eq!(r.time, 1.25);
    }
}
