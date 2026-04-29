//! `AnimSampler` — evaluate animation channels at a given time.
//!
//! § THESIS
//!   Given an `AnimationClip`, a sample time `t` (in seconds), and a
//!   target `Pose`, the sampler walks each channel, locates the
//!   surrounding keyframes via binary search, and writes the interpolated
//!   value into the matching `Transform` slot.
//!
//! § INTERPOLATION
//!   - **Linear** : the standard lerp / slerp blend between adjacent keys.
//!   - **CubicSpline** (GLTF-canonical) : Hermite cubic between adjacent
//!     keys using the `[in_tangent, value, out_tangent]` triplet layout.
//!     Tangent values are scaled by the segment duration `(t1 - t0)` per
//!     the GLTF 2.0 spec.
//!   - **Step** : hold the previous keyframe's value until the next.
//!
//! § FAST-PATH NLERP
//!   For rotation channels with short keyframe deltas, [`SamplerConfig`]
//!   exposes an `nlerp_threshold` (in radians of arc) below which the
//!   sampler uses normalized-linear interpolation instead of slerp. This
//!   trades a bounded-tiny angular error for measurable throughput. The
//!   default threshold is 0 (always slerp).
//!
//! § EDGE CASES
//!   - Single-keyframe channel : value held constant for all `t`.
//!   - `t` before first keyframe : value clamped to first keyframe.
//!   - `t` after last keyframe : value clamped to last keyframe.
//!   - These match the GLTF "extrapolation : clamp" default.

use cssl_substrate_projections::{Quat, Vec3};

use crate::clip::{
    AnimChannel, AnimChannelKind, AnimationClip, Interpolation, KeyframeR, KeyframeS, KeyframeT,
};
use crate::error::AnimError;
use crate::pose::Pose;
use crate::skeleton::Skeleton;
use crate::transform::{nlerp, slerp};

/// Configuration knobs for the sampler.
#[derive(Debug, Clone, Copy)]
pub struct SamplerConfig {
    /// If the angular distance between adjacent rotation keyframes is
    /// below this threshold (in radians), use normalized-linear (nlerp)
    /// interpolation instead of slerp. Default `0.0` = always slerp.
    pub nlerp_threshold: f32,
    /// Whether to renormalize rotations after every interpolation. Default
    /// `true` ; controls a microscopic fidelity / perf trade-off.
    pub renormalize_rotations: bool,
}

impl Default for SamplerConfig {
    fn default() -> Self {
        Self {
            nlerp_threshold: 0.0,
            renormalize_rotations: true,
        }
    }
}

/// Stateless sampler — evaluates clips into poses. Holds configuration
/// only ; per-frame state lives in the `Pose` the caller passes in.
#[derive(Debug, Clone, Copy, Default)]
pub struct AnimSampler {
    config: SamplerConfig,
}

impl AnimSampler {
    /// Construct a sampler with default configuration.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Construct a sampler with explicit configuration.
    #[must_use]
    pub fn with_config(config: SamplerConfig) -> Self {
        Self { config }
    }

    /// Read-only access to the configuration.
    #[must_use]
    pub fn config(&self) -> &SamplerConfig {
        &self.config
    }

    /// Sample a clip at time `t` and write the resulting bone-local
    /// transforms into the target pose. Bones not driven by any channel
    /// retain their existing pose value. After this call returns, the
    /// caller should call `Pose::recompute_model_transforms(skeleton)` to
    /// refresh the cumulative model-space matrices.
    pub fn sample(
        &self,
        clip: &AnimationClip,
        t: f32,
        skeleton: &Skeleton,
        pose: &mut Pose,
    ) -> Result<(), AnimError> {
        let bone_count = skeleton.bone_count();
        if pose.local_transforms.len() < bone_count {
            pose.local_transforms
                .resize(bone_count, crate::transform::Transform::IDENTITY);
        }
        for ch in &clip.channels {
            let bone_idx = ch.target.bone_idx;
            if bone_idx >= bone_count {
                return Err(AnimError::BoneIndexOutOfRange {
                    bone_idx,
                    bone_count,
                });
            }
            self.apply_channel(ch, t, &mut pose.local_transforms[bone_idx])?;
        }
        Ok(())
    }

    /// Apply a single channel to a single transform. Internal — exposed
    /// for advanced callers that want per-channel control.
    pub fn apply_channel(
        &self,
        channel: &AnimChannel,
        t: f32,
        target: &mut crate::transform::Transform,
    ) -> Result<(), AnimError> {
        match channel.target.kind {
            AnimChannelKind::Translation => {
                target.translation = self.sample_translation(channel, t);
            }
            AnimChannelKind::Rotation => {
                let mut rot = self.sample_rotation(channel, t);
                if self.config.renormalize_rotations {
                    rot = rot.normalize();
                }
                target.rotation = rot;
            }
            AnimChannelKind::Scale => {
                target.scale = self.sample_scale(channel, t);
            }
        }
        Ok(())
    }

    /// Sample a translation channel.
    pub fn sample_translation(&self, channel: &AnimChannel, t: f32) -> Vec3 {
        match channel.interpolation {
            Interpolation::Linear => sample_linear_t(&channel.t_samples, t),
            Interpolation::Step => sample_step_t(&channel.t_samples, t),
            Interpolation::CubicSpline => sample_cubic_t(&channel.t_samples, t),
        }
    }

    /// Sample a rotation channel.
    pub fn sample_rotation(&self, channel: &AnimChannel, t: f32) -> Quat {
        match channel.interpolation {
            Interpolation::Linear => {
                sample_linear_r(&channel.r_samples, t, self.config.nlerp_threshold)
            }
            Interpolation::Step => sample_step_r(&channel.r_samples, t),
            Interpolation::CubicSpline => sample_cubic_r(&channel.r_samples, t),
        }
    }

    /// Sample a scale channel.
    pub fn sample_scale(&self, channel: &AnimChannel, t: f32) -> Vec3 {
        match channel.interpolation {
            Interpolation::Linear => sample_linear_s(&channel.s_samples, t),
            Interpolation::Step => sample_step_s(&channel.s_samples, t),
            Interpolation::CubicSpline => sample_cubic_s(&channel.s_samples, t),
        }
    }
}

// ─── Linear interpolation paths ─────────────────────────────────────────

fn sample_linear_t(samples: &[KeyframeT], t: f32) -> Vec3 {
    if samples.is_empty() {
        return Vec3::ZERO;
    }
    if samples.len() == 1 || t <= samples[0].time {
        return samples[0].value;
    }
    if t >= samples[samples.len() - 1].time {
        return samples[samples.len() - 1].value;
    }
    let (i_lo, i_hi, alpha) = locate_segment_t(samples, t);
    let a = samples[i_lo].value;
    let b = samples[i_hi].value;
    Vec3::new(
        a.x + (b.x - a.x) * alpha,
        a.y + (b.y - a.y) * alpha,
        a.z + (b.z - a.z) * alpha,
    )
}

fn sample_linear_r(samples: &[KeyframeR], t: f32, nlerp_threshold: f32) -> Quat {
    if samples.is_empty() {
        return Quat::IDENTITY;
    }
    if samples.len() == 1 || t <= samples[0].time {
        return samples[0].value;
    }
    if t >= samples[samples.len() - 1].time {
        return samples[samples.len() - 1].value;
    }
    let (i_lo, i_hi, alpha) = locate_segment_r(samples, t);
    let a = samples[i_lo].value;
    let b = samples[i_hi].value;
    if nlerp_threshold > 0.0 {
        // Decide between slerp + nlerp on the fly based on chord length.
        let dot = a.x * b.x + a.y * b.y + a.z * b.z + a.w * b.w;
        let dot_abs = dot.abs().min(1.0);
        let omega = dot_abs.acos();
        if omega < nlerp_threshold {
            return nlerp(a, b, alpha);
        }
    }
    slerp(a, b, alpha)
}

fn sample_linear_s(samples: &[KeyframeS], t: f32) -> Vec3 {
    if samples.is_empty() {
        return Vec3::new(1.0, 1.0, 1.0);
    }
    if samples.len() == 1 || t <= samples[0].time {
        return samples[0].value;
    }
    if t >= samples[samples.len() - 1].time {
        return samples[samples.len() - 1].value;
    }
    let (i_lo, i_hi, alpha) = locate_segment_s(samples, t);
    let a = samples[i_lo].value;
    let b = samples[i_hi].value;
    Vec3::new(
        a.x + (b.x - a.x) * alpha,
        a.y + (b.y - a.y) * alpha,
        a.z + (b.z - a.z) * alpha,
    )
}

// ─── Step interpolation paths ───────────────────────────────────────────

fn sample_step_t(samples: &[KeyframeT], t: f32) -> Vec3 {
    if samples.is_empty() {
        return Vec3::ZERO;
    }
    let i = locate_step_index_t(samples, t);
    samples[i].value
}

fn sample_step_r(samples: &[KeyframeR], t: f32) -> Quat {
    if samples.is_empty() {
        return Quat::IDENTITY;
    }
    let i = locate_step_index_r(samples, t);
    samples[i].value
}

fn sample_step_s(samples: &[KeyframeS], t: f32) -> Vec3 {
    if samples.is_empty() {
        return Vec3::new(1.0, 1.0, 1.0);
    }
    let i = locate_step_index_s(samples, t);
    samples[i].value
}

// ─── Cubic-spline (GLTF-canonical) paths ────────────────────────────────
//
// GLTF cubic-spline channels lay out three values per keyframe in the
// `samples` array : `[in_tangent, value, out_tangent]`. So `samples[3*i+0]`
// is keyframe i's in-tangent, `samples[3*i+1]` is the value, and
// `samples[3*i+2]` is the out-tangent. The interpolation is Hermite cubic :
//
//   p(s) = (2s³ - 3s² + 1) * p_a + (s³ - 2s² + s) * (t1-t0) * tan_out_a +
//          (-2s³ + 3s²)    * p_b + (s³ - s²)      * (t1-t0) * tan_in_b
//
// where s = (t - t0) / (t1 - t0), p_a = value@a, p_b = value@b.

fn sample_cubic_t(samples: &[KeyframeT], t: f32) -> Vec3 {
    if samples.is_empty() {
        return Vec3::ZERO;
    }
    if samples.len() < 3 {
        // Fallback : treat as linear if layout is broken.
        return sample_linear_t(samples, t);
    }
    let key_count = samples.len() / 3;
    if key_count == 1 {
        return samples[1].value;
    }
    // Build a "value-only" slice for segment lookup. The "value" sample is
    // at index 3*i + 1 ; we use its time for locating the segment.
    let first_value_time = samples[1].time;
    let last_value_time = samples[(key_count - 1) * 3 + 1].time;
    if t <= first_value_time {
        return samples[1].value;
    }
    if t >= last_value_time {
        return samples[(key_count - 1) * 3 + 1].value;
    }
    // Locate segment by scanning value-times. (Binary search works too ;
    // stage-0 picks the simpler scan + benchmarks any hot path later.)
    let mut a_idx = 0;
    let mut b_idx = 1;
    for i in 0..(key_count - 1) {
        let ta = samples[i * 3 + 1].time;
        let tb = samples[(i + 1) * 3 + 1].time;
        if t >= ta && t <= tb {
            a_idx = i;
            b_idx = i + 1;
            break;
        }
    }
    let t0 = samples[a_idx * 3 + 1].time;
    let t1 = samples[b_idx * 3 + 1].time;
    let dt = (t1 - t0).max(f32::EPSILON);
    let s = (t - t0) / dt;
    let p_a = samples[a_idx * 3 + 1].value;
    let p_b = samples[b_idx * 3 + 1].value;
    let tan_out_a = samples[a_idx * 3 + 2].value; // out-tangent of A
    let tan_in_b = samples[b_idx * 3].value; // in-tangent of B
    hermite_vec3(p_a, p_b, tan_out_a, tan_in_b, s, dt)
}

fn sample_cubic_r(samples: &[KeyframeR], t: f32) -> Quat {
    if samples.is_empty() {
        return Quat::IDENTITY;
    }
    if samples.len() < 3 {
        return sample_linear_r(samples, t, 0.0);
    }
    let key_count = samples.len() / 3;
    if key_count == 1 {
        return samples[1].value;
    }
    let first_value_time = samples[1].time;
    let last_value_time = samples[(key_count - 1) * 3 + 1].time;
    if t <= first_value_time {
        return samples[1].value;
    }
    if t >= last_value_time {
        return samples[(key_count - 1) * 3 + 1].value;
    }
    let mut a_idx = 0;
    let mut b_idx = 1;
    for i in 0..(key_count - 1) {
        let ta = samples[i * 3 + 1].time;
        let tb = samples[(i + 1) * 3 + 1].time;
        if t >= ta && t <= tb {
            a_idx = i;
            b_idx = i + 1;
            break;
        }
    }
    let t0 = samples[a_idx * 3 + 1].time;
    let t1 = samples[b_idx * 3 + 1].time;
    let dt = (t1 - t0).max(f32::EPSILON);
    let s = (t - t0) / dt;
    let p_a = samples[a_idx * 3 + 1].value;
    let p_b = samples[b_idx * 3 + 1].value;
    let tan_out_a = samples[a_idx * 3 + 2].value;
    let tan_in_b = samples[b_idx * 3].value;
    hermite_quat(p_a, p_b, tan_out_a, tan_in_b, s, dt)
}

fn sample_cubic_s(samples: &[KeyframeS], t: f32) -> Vec3 {
    if samples.is_empty() {
        return Vec3::new(1.0, 1.0, 1.0);
    }
    if samples.len() < 3 {
        return sample_linear_s(samples, t);
    }
    let key_count = samples.len() / 3;
    if key_count == 1 {
        return samples[1].value;
    }
    let first_value_time = samples[1].time;
    let last_value_time = samples[(key_count - 1) * 3 + 1].time;
    if t <= first_value_time {
        return samples[1].value;
    }
    if t >= last_value_time {
        return samples[(key_count - 1) * 3 + 1].value;
    }
    let mut a_idx = 0;
    let mut b_idx = 1;
    for i in 0..(key_count - 1) {
        let ta = samples[i * 3 + 1].time;
        let tb = samples[(i + 1) * 3 + 1].time;
        if t >= ta && t <= tb {
            a_idx = i;
            b_idx = i + 1;
            break;
        }
    }
    let t0 = samples[a_idx * 3 + 1].time;
    let t1 = samples[b_idx * 3 + 1].time;
    let dt = (t1 - t0).max(f32::EPSILON);
    let s = (t - t0) / dt;
    let p_a = samples[a_idx * 3 + 1].value;
    let p_b = samples[b_idx * 3 + 1].value;
    let tan_out_a = samples[a_idx * 3 + 2].value;
    let tan_in_b = samples[b_idx * 3].value;
    hermite_vec3(p_a, p_b, tan_out_a, tan_in_b, s, dt)
}

// ─── Helpers ────────────────────────────────────────────────────────────

/// Locate the segment around time `t` in a sorted translation-keyframe
/// array. Returns `(lo_idx, hi_idx, alpha)` where alpha is the local
/// `[0, 1]` parameter inside `[samples[lo_idx].time, samples[hi_idx].time]`.
/// Caller must have already guaranteed `t` is in-range.
fn locate_segment_t(samples: &[KeyframeT], t: f32) -> (usize, usize, f32) {
    // Binary search by time. partition_point finds the first index with
    // `time > t` ; the segment is `[idx-1, idx]`.
    let idx = samples.partition_point(|k| k.time <= t).max(1);
    let lo = idx - 1;
    let hi = idx.min(samples.len() - 1);
    let t0 = samples[lo].time;
    let t1 = samples[hi].time;
    let dt = (t1 - t0).max(f32::EPSILON);
    let alpha = ((t - t0) / dt).clamp(0.0, 1.0);
    (lo, hi, alpha)
}

fn locate_segment_r(samples: &[KeyframeR], t: f32) -> (usize, usize, f32) {
    let idx = samples.partition_point(|k| k.time <= t).max(1);
    let lo = idx - 1;
    let hi = idx.min(samples.len() - 1);
    let t0 = samples[lo].time;
    let t1 = samples[hi].time;
    let dt = (t1 - t0).max(f32::EPSILON);
    let alpha = ((t - t0) / dt).clamp(0.0, 1.0);
    (lo, hi, alpha)
}

fn locate_segment_s(samples: &[KeyframeS], t: f32) -> (usize, usize, f32) {
    let idx = samples.partition_point(|k| k.time <= t).max(1);
    let lo = idx - 1;
    let hi = idx.min(samples.len() - 1);
    let t0 = samples[lo].time;
    let t1 = samples[hi].time;
    let dt = (t1 - t0).max(f32::EPSILON);
    let alpha = ((t - t0) / dt).clamp(0.0, 1.0);
    (lo, hi, alpha)
}

fn locate_step_index_t(samples: &[KeyframeT], t: f32) -> usize {
    if t <= samples[0].time {
        return 0;
    }
    if t >= samples[samples.len() - 1].time {
        return samples.len() - 1;
    }
    samples.partition_point(|k| k.time <= t).saturating_sub(1)
}

fn locate_step_index_r(samples: &[KeyframeR], t: f32) -> usize {
    if t <= samples[0].time {
        return 0;
    }
    if t >= samples[samples.len() - 1].time {
        return samples.len() - 1;
    }
    samples.partition_point(|k| k.time <= t).saturating_sub(1)
}

fn locate_step_index_s(samples: &[KeyframeS], t: f32) -> usize {
    if t <= samples[0].time {
        return 0;
    }
    if t >= samples[samples.len() - 1].time {
        return samples.len() - 1;
    }
    samples.partition_point(|k| k.time <= t).saturating_sub(1)
}

/// Hermite cubic blend for `Vec3`. `dt` is the segment duration in
/// seconds — the GLTF tangent values are scaled by it per spec.
fn hermite_vec3(p_a: Vec3, p_b: Vec3, tan_out_a: Vec3, tan_in_b: Vec3, s: f32, dt: f32) -> Vec3 {
    let s2 = s * s;
    let s3 = s2 * s;
    let h00 = 2.0 * s3 - 3.0 * s2 + 1.0;
    let h10 = s3 - 2.0 * s2 + s;
    let h01 = -2.0 * s3 + 3.0 * s2;
    let h11 = s3 - s2;
    Vec3::new(
        h00 * p_a.x + h10 * dt * tan_out_a.x + h01 * p_b.x + h11 * dt * tan_in_b.x,
        h00 * p_a.y + h10 * dt * tan_out_a.y + h01 * p_b.y + h11 * dt * tan_in_b.y,
        h00 * p_a.z + h10 * dt * tan_out_a.z + h01 * p_b.z + h11 * dt * tan_in_b.z,
    )
}

/// Hermite cubic blend for `Quat`. Tangents are stored as quaternion
/// values per GLTF — interpolated component-wise then renormalized.
fn hermite_quat(p_a: Quat, p_b: Quat, tan_out_a: Quat, tan_in_b: Quat, s: f32, dt: f32) -> Quat {
    let s2 = s * s;
    let s3 = s2 * s;
    let h00 = 2.0 * s3 - 3.0 * s2 + 1.0;
    let h10 = s3 - 2.0 * s2 + s;
    let h01 = -2.0 * s3 + 3.0 * s2;
    let h11 = s3 - s2;
    Quat::new(
        h00 * p_a.x + h10 * dt * tan_out_a.x + h01 * p_b.x + h11 * dt * tan_in_b.x,
        h00 * p_a.y + h10 * dt * tan_out_a.y + h01 * p_b.y + h11 * dt * tan_in_b.y,
        h00 * p_a.z + h10 * dt * tan_out_a.z + h01 * p_b.z + h11 * dt * tan_in_b.z,
        h00 * p_a.w + h10 * dt * tan_out_a.w + h01 * p_b.w + h11 * dt * tan_in_b.w,
    )
    .normalize()
}

#[cfg(test)]
mod tests {
    use super::{AnimSampler, SamplerConfig};
    use crate::clip::{AnimChannel, AnimationClip, Interpolation, KeyframeR, KeyframeS, KeyframeT};
    use crate::pose::Pose;
    use crate::skeleton::{Bone, Skeleton, ROOT_PARENT};
    use crate::transform::Transform;
    use cssl_substrate_projections::{Quat, Vec3};

    fn approx_eq(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() <= eps
    }

    fn vec3_approx_eq(a: Vec3, b: Vec3, eps: f32) -> bool {
        approx_eq(a.x, b.x, eps) && approx_eq(a.y, b.y, eps) && approx_eq(a.z, b.z, eps)
    }

    fn make_skeleton() -> Skeleton {
        let bones = vec![
            Bone::new("root", ROOT_PARENT, Transform::IDENTITY),
            Bone::new("b1", 0, Transform::IDENTITY),
        ];
        Skeleton::from_bones(bones).expect("ok")
    }

    #[test]
    fn linear_translation_at_half_is_midpoint() {
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
                    value: Vec3::new(10.0, 20.0, 30.0),
                },
            ],
        )
        .expect("ok");
        let clip = AnimationClip::new("test", vec![ch]);
        let s = make_skeleton();
        let mut pose = Pose::from_bind_pose(&s);
        let sampler = AnimSampler::new();
        sampler.sample(&clip, 0.5, &s, &mut pose).expect("sample");
        assert!(vec3_approx_eq(
            pose.local_transforms[1].translation,
            Vec3::new(5.0, 10.0, 15.0),
            1e-5
        ));
    }

    #[test]
    fn linear_rotation_at_half_is_slerp_midpoint() {
        let ch = AnimChannel::rotation(
            1,
            Interpolation::Linear,
            vec![
                KeyframeR {
                    time: 0.0,
                    value: Quat::IDENTITY,
                },
                KeyframeR {
                    time: 1.0,
                    value: Quat::from_axis_angle(Vec3::Y, core::f32::consts::FRAC_PI_2),
                },
            ],
        )
        .expect("ok");
        let clip = AnimationClip::new("test", vec![ch]);
        let s = make_skeleton();
        let mut pose = Pose::from_bind_pose(&s);
        let sampler = AnimSampler::new();
        sampler.sample(&clip, 0.5, &s, &mut pose).expect("sample");
        let expected = Quat::from_axis_angle(Vec3::Y, core::f32::consts::FRAC_PI_4);
        let v = Vec3::X;
        let got = pose.local_transforms[1].rotation.rotate(v);
        let want = expected.rotate(v);
        assert!(vec3_approx_eq(got, want, 1e-4));
    }

    #[test]
    fn linear_scale_at_half_is_midpoint() {
        let ch = AnimChannel::scale(
            1,
            Interpolation::Linear,
            vec![
                KeyframeS {
                    time: 0.0,
                    value: Vec3::splat(1.0),
                },
                KeyframeS {
                    time: 1.0,
                    value: Vec3::splat(3.0),
                },
            ],
        )
        .expect("ok");
        let clip = AnimationClip::new("test", vec![ch]);
        let s = make_skeleton();
        let mut pose = Pose::from_bind_pose(&s);
        let sampler = AnimSampler::new();
        sampler.sample(&clip, 0.5, &s, &mut pose).expect("sample");
        assert!(vec3_approx_eq(
            pose.local_transforms[1].scale,
            Vec3::splat(2.0),
            1e-5
        ));
    }

    #[test]
    fn linear_clamps_at_clip_start() {
        let ch = AnimChannel::translation(
            1,
            Interpolation::Linear,
            vec![
                KeyframeT {
                    time: 1.0,
                    value: Vec3::X,
                },
                KeyframeT {
                    time: 2.0,
                    value: Vec3::Y,
                },
            ],
        )
        .expect("ok");
        let clip = AnimationClip::new("test", vec![ch]);
        let s = make_skeleton();
        let mut pose = Pose::from_bind_pose(&s);
        let sampler = AnimSampler::new();
        sampler.sample(&clip, 0.0, &s, &mut pose).expect("sample");
        assert!(vec3_approx_eq(
            pose.local_transforms[1].translation,
            Vec3::X,
            1e-5
        ));
    }

    #[test]
    fn linear_clamps_at_clip_end() {
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
                    value: Vec3::X,
                },
            ],
        )
        .expect("ok");
        let clip = AnimationClip::new("test", vec![ch]);
        let s = make_skeleton();
        let mut pose = Pose::from_bind_pose(&s);
        let sampler = AnimSampler::new();
        sampler.sample(&clip, 99.0, &s, &mut pose).expect("sample");
        assert!(vec3_approx_eq(
            pose.local_transforms[1].translation,
            Vec3::X,
            1e-5
        ));
    }

    #[test]
    fn step_holds_previous_keyframe() {
        let ch = AnimChannel::translation(
            1,
            Interpolation::Step,
            vec![
                KeyframeT {
                    time: 0.0,
                    value: Vec3::ZERO,
                },
                KeyframeT {
                    time: 1.0,
                    value: Vec3::X,
                },
                KeyframeT {
                    time: 2.0,
                    value: Vec3::Y,
                },
            ],
        )
        .expect("ok");
        let clip = AnimationClip::new("test", vec![ch]);
        let s = make_skeleton();
        let mut pose = Pose::from_bind_pose(&s);
        let sampler = AnimSampler::new();
        sampler.sample(&clip, 0.5, &s, &mut pose).expect("sample");
        // At 0.5, step should hold the value at time 0.0 = ZERO.
        assert!(vec3_approx_eq(
            pose.local_transforms[1].translation,
            Vec3::ZERO,
            1e-5
        ));
        sampler.sample(&clip, 1.5, &s, &mut pose).expect("sample");
        // At 1.5, step should hold the value at time 1.0 = X.
        assert!(vec3_approx_eq(
            pose.local_transforms[1].translation,
            Vec3::X,
            1e-5
        ));
    }

    #[test]
    fn cubic_spline_translation_endpoints() {
        // 1 keyframe = 3 samples : [in, value, out] at time 0 ; 2 keyframes = 6 samples.
        let samples = vec![
            // Keyframe 0 : in_tangent, value, out_tangent
            KeyframeT {
                time: 0.0,
                value: Vec3::ZERO,
            }, // in
            KeyframeT {
                time: 0.0,
                value: Vec3::ZERO,
            }, // value
            KeyframeT {
                time: 0.0,
                value: Vec3::ZERO,
            }, // out
            // Keyframe 1
            KeyframeT {
                time: 1.0,
                value: Vec3::ZERO,
            }, // in
            KeyframeT {
                time: 1.0,
                value: Vec3::X,
            }, // value
            KeyframeT {
                time: 1.0,
                value: Vec3::ZERO,
            }, // out
        ];
        let ch = AnimChannel::translation(1, Interpolation::CubicSpline, samples).expect("ok");
        let clip = AnimationClip::new("test", vec![ch]);
        let s = make_skeleton();
        let mut pose = Pose::from_bind_pose(&s);
        let sampler = AnimSampler::new();
        sampler.sample(&clip, 0.0, &s, &mut pose).expect("ok");
        assert!(vec3_approx_eq(
            pose.local_transforms[1].translation,
            Vec3::ZERO,
            1e-5
        ));
        sampler.sample(&clip, 1.0, &s, &mut pose).expect("ok");
        assert!(vec3_approx_eq(
            pose.local_transforms[1].translation,
            Vec3::X,
            1e-5
        ));
    }

    #[test]
    fn cubic_spline_with_zero_tangents_matches_smoothstep() {
        // With zero tangents, cubic interpolation produces the smoothstep
        // curve : at t=0.5, value should be at the midpoint of (0, 1) ⇒ 0.5.
        let samples = vec![
            KeyframeT {
                time: 0.0,
                value: Vec3::ZERO,
            }, // in
            KeyframeT {
                time: 0.0,
                value: Vec3::ZERO,
            }, // value
            KeyframeT {
                time: 0.0,
                value: Vec3::ZERO,
            }, // out
            KeyframeT {
                time: 1.0,
                value: Vec3::ZERO,
            }, // in
            KeyframeT {
                time: 1.0,
                value: Vec3::splat(1.0),
            }, // value
            KeyframeT {
                time: 1.0,
                value: Vec3::ZERO,
            }, // out
        ];
        let ch = AnimChannel::translation(1, Interpolation::CubicSpline, samples).expect("ok");
        let clip = AnimationClip::new("test", vec![ch]);
        let s = make_skeleton();
        let mut pose = Pose::from_bind_pose(&s);
        let sampler = AnimSampler::new();
        sampler.sample(&clip, 0.5, &s, &mut pose).expect("ok");
        // smoothstep(0.5) = 0.5 (the curve is symmetric about t=0.5 here).
        assert!(approx_eq(pose.local_transforms[1].translation.x, 0.5, 1e-5));
    }

    #[test]
    fn single_keyframe_clip_holds_constant() {
        let ch = AnimChannel::translation(
            1,
            Interpolation::Linear,
            vec![KeyframeT {
                time: 0.0,
                value: Vec3::splat(7.0),
            }],
        )
        .expect("ok");
        let clip = AnimationClip::new("test", vec![ch]);
        let s = make_skeleton();
        let mut pose = Pose::from_bind_pose(&s);
        let sampler = AnimSampler::new();
        sampler.sample(&clip, 5.0, &s, &mut pose).expect("ok");
        assert!(vec3_approx_eq(
            pose.local_transforms[1].translation,
            Vec3::splat(7.0),
            1e-5
        ));
    }

    #[test]
    fn out_of_range_bone_idx_errors() {
        let ch = AnimChannel::translation(
            99,
            Interpolation::Linear,
            vec![KeyframeT {
                time: 0.0,
                value: Vec3::ZERO,
            }],
        )
        .expect("ok");
        let clip = AnimationClip::new("test", vec![ch]);
        let s = make_skeleton();
        let mut pose = Pose::from_bind_pose(&s);
        let sampler = AnimSampler::new();
        assert!(sampler.sample(&clip, 0.0, &s, &mut pose).is_err());
    }

    #[test]
    fn nlerp_threshold_uses_nlerp_for_small_arcs() {
        // With nlerp_threshold = π (always nlerp), result must still
        // be unit-length and approximately match the ideal arc midpoint
        // for very-small deltas.
        let ch = AnimChannel::rotation(
            1,
            Interpolation::Linear,
            vec![
                KeyframeR {
                    time: 0.0,
                    value: Quat::IDENTITY,
                },
                KeyframeR {
                    time: 1.0,
                    value: Quat::from_axis_angle(Vec3::Y, 0.001),
                },
            ],
        )
        .expect("ok");
        let clip = AnimationClip::new("test", vec![ch]);
        let s = make_skeleton();
        let mut pose = Pose::from_bind_pose(&s);
        let sampler = AnimSampler::with_config(SamplerConfig {
            nlerp_threshold: core::f32::consts::PI,
            ..Default::default()
        });
        sampler.sample(&clip, 0.5, &s, &mut pose).expect("ok");
        let q = pose.local_transforms[1].rotation;
        assert!(approx_eq(q.length_squared(), 1.0, 1e-5));
    }

    #[test]
    fn determinism_same_input_same_output() {
        // Sampling at the same (t, clip) on two different runs must yield
        // bit-identical poses — the replay-determinism foundation.
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
                    value: Vec3::new(1.0, 2.0, 3.0),
                },
            ],
        )
        .expect("ok");
        let clip = AnimationClip::new("test", vec![ch]);
        let s = make_skeleton();
        let sampler = AnimSampler::new();
        let mut p1 = Pose::from_bind_pose(&s);
        let mut p2 = Pose::from_bind_pose(&s);
        sampler.sample(&clip, 0.37, &s, &mut p1).expect("ok");
        sampler.sample(&clip, 0.37, &s, &mut p2).expect("ok");
        assert_eq!(p1.local_transforms[1], p2.local_transforms[1]);
    }
}
