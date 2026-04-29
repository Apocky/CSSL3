//! § BoneSegmentDeformation — bones as soft-body points in the wave-field.
//!
//! § THESIS
//!   Rigid skeletal animation hard-pins every bone to a single transform.
//!   Real flesh is not rigid : muscles bulge, fat jiggles, fur ripples,
//!   skin slides. The procedural runtime captures this by treating each
//!   bone segment as a **soft-body chain** of N points immersed in the
//!   wave-field. Local pressure (Λ token-density gradient + multivec-
//!   dynamics bivector field) deforms the segment by displacing points
//!   along the wave-field gradient ; the bone's stiffness coefficient
//!   determines how strongly the segment resists deformation.
//!
//! § FORWARD MODEL
//!   For each bone segment of length `L`, sample `n` uniformly-spaced
//!   probe points along the bone-local Y axis. At each probe, sample the
//!   wave-field's Λ density (pressure-like scalar) and the
//!   `multivec_dynamics_lo` bivector (vector-field-like force direction).
//!   The probe's displacement is :
//!
//!   ```text
//!   delta_p = (1 - stiffness) * pressure * dt * force_dir
//!   ```
//!
//!   where `force_dir` is the unit-normalized bivector projection. Higher
//!   stiffness produces less deformation ; lower stiffness produces more.
//!   The output is written as a per-bone deformation sample that the
//!   pose-matrix sweep applies as a small displacement on top of the
//!   bone-local transform.
//!
//! § DECOUPLING FROM `cssl-substrate-omega-field`
//!   We do not import the omega-field crate here ; the deformation API
//!   takes a [`WaveFieldProbe`] callback that the host wires up. This
//!   keeps the build graph clean + lets host applications swap probe
//!   sources (real omega-field, mocked field for testing, recorded-field
//!   for replay).
//!
//! § DETERMINISM
//!   Bounded-output, total math. Identical probe values + dt produce
//!   bit-identical deformation samples.

use cssl_substrate_projections::Vec3;

use crate::skeleton::ProceduralSkeleton;

/// Per-bone deformation sample : displacement vector + scalar amplitude.
/// Applied on top of the bone-local transform during the pose-matrix
/// sweep.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct DeformationSample {
    /// Bone index this sample applies to.
    pub bone_idx: usize,
    /// Local-space displacement (bone-tip relative to bone-origin).
    pub displacement: Vec3,
    /// Scalar amplitude (visualization aid + test diagnostic).
    pub amplitude: f32,
}

impl DeformationSample {
    /// Construct a zero sample for a particular bone (no deformation).
    #[must_use]
    pub const fn zero(bone_idx: usize) -> Self {
        Self {
            bone_idx,
            displacement: Vec3::ZERO,
            amplitude: 0.0,
        }
    }
}

/// Wave-field probe — caller-supplied function that returns the local
/// pressure (`Λ`) + force direction (unit `Vec3`) at a world-space point.
pub trait WaveFieldProbe {
    /// Sample the wave-field at `world_point`.
    fn sample(&self, world_point: Vec3) -> (f32, Vec3);
}

/// Stub probe : zero pressure + zero force direction. Useful for tests.
#[derive(Debug, Clone, Copy, Default)]
pub struct ZeroFieldProbe;

impl WaveFieldProbe for ZeroFieldProbe {
    fn sample(&self, _world_point: Vec3) -> (f32, Vec3) {
        (0.0, Vec3::ZERO)
    }
}

/// Constant-uniform probe : returns the same pressure + force direction
/// at every point. Useful for tests + smoke checks.
#[derive(Debug, Clone, Copy)]
pub struct UniformFieldProbe {
    /// Constant pressure value.
    pub pressure: f32,
    /// Constant force direction (will be normalized at sample time).
    pub force_dir: Vec3,
}

impl WaveFieldProbe for UniformFieldProbe {
    fn sample(&self, _world_point: Vec3) -> (f32, Vec3) {
        let dir = self.force_dir.normalize();
        (self.pressure, dir)
    }
}

/// Configuration for the deformation step.
#[derive(Debug, Clone, Copy)]
pub struct DeformationConfig {
    /// Number of probe points per bone segment. Higher = more accurate
    /// integration along the bone axis but more probe-sample calls.
    pub probes_per_segment: usize,
    /// Master gain on the deformation amplitude.
    pub gain: f32,
    /// Maximum displacement (meters) per tick. Caps runaway deformation.
    pub max_displacement: f32,
}

impl Default for DeformationConfig {
    fn default() -> Self {
        Self {
            probes_per_segment: 4,
            gain: 0.1,
            max_displacement: 0.05,
        }
    }
}

/// The bone-segment deformation surface. Computes a [`DeformationSample`]
/// per bone given a wave-field probe + the current pose's world-space
/// bone positions.
#[derive(Debug, Clone)]
pub struct BoneSegmentDeformation {
    config: DeformationConfig,
    samples: Vec<DeformationSample>,
}

impl BoneSegmentDeformation {
    /// Construct with default config.
    #[must_use]
    pub fn new() -> Self {
        Self::with_config(DeformationConfig::default())
    }

    /// Construct with explicit config.
    #[must_use]
    pub fn with_config(config: DeformationConfig) -> Self {
        Self {
            config,
            samples: Vec::new(),
        }
    }

    /// Read-only access to the most recent samples.
    #[must_use]
    pub fn samples(&self) -> &[DeformationSample] {
        &self.samples
    }

    /// Configuration accessor.
    #[must_use]
    pub fn config(&self) -> &DeformationConfig {
        &self.config
    }

    /// Compute deformation samples for every bone in the skeleton. The
    /// `bone_world_positions` slice gives the current world-space
    /// position of each bone's origin (typically computed from the
    /// pose's model-matrix translation column). The probe is sampled at
    /// each probe-point along each bone segment.
    ///
    /// Returns the number of bones for which a non-zero sample was
    /// produced. The caller can read [`Self::samples`] for the per-bone
    /// vectors after this call.
    pub fn compute<P: WaveFieldProbe>(
        &mut self,
        skeleton: &ProceduralSkeleton,
        bone_world_positions: &[Vec3],
        probe: &P,
        dt: f32,
    ) -> usize {
        let n = skeleton.bone_count();
        self.samples.clear();
        self.samples.reserve(n);
        let mut nonzero = 0;
        for (i, b) in skeleton.bones().iter().enumerate() {
            if i >= bone_world_positions.len() {
                self.samples.push(DeformationSample::zero(i));
                continue;
            }
            let origin = bone_world_positions[i];
            let probes = self.config.probes_per_segment.max(1);
            let mut accumulated = Vec3::ZERO;
            let mut total_pressure = 0.0;
            for k in 0..probes {
                let along = if probes == 1 {
                    0.5
                } else {
                    (k as f32) / ((probes - 1).max(1) as f32)
                };
                let probe_pt = origin + Vec3::new(0.0, along * b.segment_length, 0.0);
                let (pressure, dir) = probe.sample(probe_pt);
                total_pressure += pressure;
                accumulated = accumulated + dir * pressure;
            }
            // Average across probes.
            let inv = (probes as f32).recip();
            let avg_dir = accumulated * inv;
            let avg_pressure = total_pressure * inv;

            // Stiffness governs how much deformation we let through.
            // stiffness = 1 → no deformation ; stiffness = 0 → full.
            let displacement_factor = (1.0 - b.stiffness).max(0.0) * self.config.gain * dt;
            let mut disp = avg_dir * displacement_factor;

            // Cap the magnitude.
            let mag = disp.length();
            if mag > self.config.max_displacement {
                let scale = self.config.max_displacement / mag.max(f32::EPSILON);
                disp = disp * scale;
            }
            let amp = disp.length() * avg_pressure.signum();
            self.samples.push(DeformationSample {
                bone_idx: i,
                displacement: disp,
                amplitude: amp,
            });
            if disp.length() > 0.0 {
                nonzero += 1;
            }
        }
        nonzero
    }

    /// Find the deformation sample for a particular bone.
    #[must_use]
    pub fn sample_for_bone(&self, bone_idx: usize) -> Option<&DeformationSample> {
        self.samples.iter().find(|s| s.bone_idx == bone_idx)
    }

    /// Reset all samples to zero.
    pub fn clear(&mut self) {
        self.samples.clear();
    }
}

impl Default for BoneSegmentDeformation {
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
            Bone::new("root", ROOT_PARENT, Transform::IDENTITY).with_segment_length(1.0),
            Bone::new("a", 0, Transform::IDENTITY).with_segment_length(1.0),
        ])
        .unwrap()
    }

    #[test]
    fn zero_field_produces_zero_displacement() {
        let s = make_skel();
        let mut d = BoneSegmentDeformation::new();
        let positions = vec![Vec3::ZERO, Vec3::new(0.0, 1.0, 0.0)];
        let n = d.compute(&s, &positions, &ZeroFieldProbe, 0.016);
        assert_eq!(n, 0);
        for sample in d.samples() {
            assert!(sample.displacement.length() < 1e-6);
        }
    }

    #[test]
    fn uniform_field_produces_displacement_in_force_dir() {
        let s = make_skel();
        let mut d = BoneSegmentDeformation::new();
        let positions = vec![Vec3::ZERO, Vec3::new(0.0, 1.0, 0.0)];
        let probe = UniformFieldProbe {
            pressure: 5.0,
            force_dir: Vec3::new(1.0, 0.0, 0.0),
        };
        let n = d.compute(&s, &positions, &probe, 0.1);
        assert!(n > 0);
        let s0 = d.sample_for_bone(0).unwrap();
        // Displacement should point along +X.
        assert!(s0.displacement.x > 0.0);
        assert!(s0.displacement.y.abs() < 1e-3);
        assert!(s0.displacement.z.abs() < 1e-3);
    }

    #[test]
    fn high_stiffness_reduces_displacement() {
        let bones = vec![
            Bone::new("a", ROOT_PARENT, Transform::IDENTITY).with_stiffness(1.0),
            Bone::new("b", ROOT_PARENT, Transform::IDENTITY).with_stiffness(0.0),
        ];
        let s = ProceduralSkeleton::from_bones(bones).unwrap();
        let mut d = BoneSegmentDeformation::new();
        let positions = vec![Vec3::ZERO, Vec3::ZERO];
        let probe = UniformFieldProbe {
            pressure: 1.0,
            force_dir: Vec3::new(1.0, 0.0, 0.0),
        };
        d.compute(&s, &positions, &probe, 0.1);
        let stiff = d.sample_for_bone(0).unwrap();
        let soft = d.sample_for_bone(1).unwrap();
        assert!(stiff.displacement.length() < soft.displacement.length());
    }

    #[test]
    fn max_displacement_caps_magnitude() {
        let s = make_skel();
        let cfg = DeformationConfig {
            max_displacement: 0.001,
            gain: 100.0,
            ..DeformationConfig::default()
        };
        let mut d = BoneSegmentDeformation::with_config(cfg);
        let positions = vec![Vec3::ZERO, Vec3::ZERO];
        let probe = UniformFieldProbe {
            pressure: 100.0,
            force_dir: Vec3::new(1.0, 0.0, 0.0),
        };
        d.compute(&s, &positions, &probe, 0.1);
        for sample in d.samples() {
            assert!(sample.displacement.length() <= 0.001 + 1e-5);
        }
    }

    #[test]
    fn deformation_sample_zero_constructor() {
        let s = DeformationSample::zero(7);
        assert_eq!(s.bone_idx, 7);
        assert_eq!(s.displacement, Vec3::ZERO);
        assert_eq!(s.amplitude, 0.0);
    }

    #[test]
    fn config_defaults_are_reasonable() {
        let c = DeformationConfig::default();
        assert!(c.probes_per_segment >= 1);
        assert!(c.max_displacement > 0.0);
    }

    #[test]
    fn missing_position_produces_zero_sample() {
        let s = make_skel();
        let mut d = BoneSegmentDeformation::new();
        // Only one position for two bones ; second bone's sample should
        // be zero.
        let positions = vec![Vec3::ZERO];
        d.compute(
            &s,
            &positions,
            &UniformFieldProbe {
                pressure: 1.0,
                force_dir: Vec3::X,
            },
            0.1,
        );
        let s1 = d.sample_for_bone(1).unwrap();
        assert_eq!(s1.displacement, Vec3::ZERO);
    }

    #[test]
    fn clear_drops_samples() {
        let s = make_skel();
        let mut d = BoneSegmentDeformation::new();
        let positions = vec![Vec3::ZERO, Vec3::ZERO];
        d.compute(
            &s,
            &positions,
            &UniformFieldProbe {
                pressure: 1.0,
                force_dir: Vec3::X,
            },
            0.1,
        );
        d.clear();
        assert!(d.samples().is_empty());
    }

    #[test]
    fn deterministic_repeated_compute_produces_identical_samples() {
        let s = make_skel();
        let positions = vec![Vec3::ZERO, Vec3::new(0.0, 1.0, 0.0)];
        let probe = UniformFieldProbe {
            pressure: 2.0,
            force_dir: Vec3::new(0.0, 0.0, 1.0),
        };
        let mut d1 = BoneSegmentDeformation::new();
        let mut d2 = BoneSegmentDeformation::new();
        d1.compute(&s, &positions, &probe, 0.05);
        d2.compute(&s, &positions, &probe, 0.05);
        for (a, b) in d1.samples().iter().zip(d2.samples().iter()) {
            assert_eq!(a, b);
        }
    }
}
