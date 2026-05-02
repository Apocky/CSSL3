//! § branch — Fork the present into K candidate-future branches.
//!
//! Given a present `WorldBranch` and an `intent_seed`, produce K plausible
//! future-branches each with deterministic axis-perturbations, a derived
//! narrative_seed, and an amplitude-weight that sums to 1 after normalization.

use crate::{FrameState, WorldBranch};

/// Splittable hash : take a 64-bit seed and a 64-bit splitter, return a
/// fresh-but-deterministic 64-bit value. Wyhash-equivalent inline form
/// keeps the crate zero-dep.
#[inline]
const fn split_hash(seed: u64, splitter: u64) -> u64 {
    let mut x = seed.wrapping_add(splitter).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    x ^= x >> 30;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 31;
    x
}

/// Convert a 64-bit hash into a deterministic axis-perturbation in
/// the range [-amplitude, +amplitude].
#[inline]
fn hash_to_perturbation(hash: u64, amplitude_i16: i16) -> i16 {
    let signed = hash as i64;
    let amp = amplitude_i16 as i64;
    if amp == 0 {
        return 0;
    }
    ((signed.wrapping_rem(2 * amp + 1)) - amp) as i16
}

/// Branch the present into `k` candidate futures.
pub fn branch_present(
    present: &WorldBranch,
    intent_seed: u64,
    k: usize,
) -> Vec<WorldBranch> {
    if k == 0 {
        return Vec::new();
    }

    let mut out = Vec::with_capacity(k);
    let perturbation_amplitude: i16 = 32;

    for k_index in 0..k {
        let split_seed = split_hash(intent_seed, k_index as u64);
        let mut perturbed_axes = present.frame_state.pixel_axes;
        for axis_idx in 0..8 {
            let h = split_hash(split_seed, axis_idx as u64);
            let delta = hash_to_perturbation(h, perturbation_amplitude);
            perturbed_axes[axis_idx] = perturbed_axes[axis_idx].saturating_add(delta);
        }

        let raw_amplitude = 1.0_f32 - (k_index as f32) * (0.5_f32 / (k as f32));

        let phase_u32 = (split_seed >> 32) as u32;
        let phase = (phase_u32 as f32 / u32::MAX as f32) * std::f32::consts::TAU;

        let new_narrative_seed =
            split_hash(present.frame_state.narrative_seed, split_seed);

        let frame_state = FrameState {
            pixel_axes: perturbed_axes,
            narrative_seed: new_narrative_seed,
            intent_hash: (intent_seed as u32) ^ (k_index as u32),
            tick: present.frame_state.tick.wrapping_add(1),
        };

        out.push(WorldBranch::new(0, raw_amplitude, phase, frame_state));
    }

    normalize_amplitudes(&mut out);
    out
}

/// Branch with a richer intent-payload : the caller supplies an
/// `intent_descriptor_axes` overlay that BIASES the branch-perturbation
/// toward the player's stated direction. Strength is f32 in [0, 1].
pub fn branch_with_intent(
    present: &WorldBranch,
    intent_seed: u64,
    intent_descriptor_axes: &[i16; 8],
    strength: f32,
    k: usize,
) -> Vec<WorldBranch> {
    if k == 0 {
        return Vec::new();
    }
    let mut branches = branch_present(present, intent_seed, k);
    let s = strength.clamp(0.0, 1.0);
    for b in &mut branches {
        for i in 0..8 {
            let original = b.frame_state.pixel_axes[i] as f32;
            let intent_axis = intent_descriptor_axes[i] as f32;
            let mixed = (original * (1.0 - s)) + (intent_axis * s);
            b.frame_state.pixel_axes[i] = mixed.round().clamp(i16::MIN as f32, i16::MAX as f32) as i16;
        }
    }
    branches
}

/// Normalize a Vec of branches so Σ |a|^2 = 1.
pub fn normalize_amplitudes(branches: &mut [WorldBranch]) -> f32 {
    let total_p: f32 = branches.iter().map(|b| b.amplitude * b.amplitude).sum();
    if total_p <= 0.0 || !total_p.is_finite() {
        return 0.0;
    }
    let scale = 1.0 / total_p.sqrt();
    for b in branches.iter_mut() {
        b.amplitude *= scale;
    }
    total_p.sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn present_branch() -> WorldBranch {
        WorldBranch::new(
            1,
            1.0,
            0.0,
            FrameState {
                pixel_axes: [10, 20, 30, 40, 50, 60, 70, 80],
                narrative_seed: 12345,
                intent_hash: 7,
                tick: 100,
            },
        )
    }

    #[test]
    fn branch_zero_returns_empty() {
        let p = present_branch();
        let branches = branch_present(&p, 0xDEAD_BEEF, 0);
        assert!(branches.is_empty());
    }

    #[test]
    fn branch_k_returns_k_branches() {
        let p = present_branch();
        let branches = branch_present(&p, 0xDEAD_BEEF, 5);
        assert_eq!(branches.len(), 5);
    }

    #[test]
    fn branch_amplitudes_sum_to_one_squared() {
        let p = present_branch();
        let branches = branch_present(&p, 0xDEAD_BEEF, 5);
        let total_p: f32 = branches.iter().map(|b| b.probability()).sum();
        assert!(
            (total_p - 1.0).abs() < 1e-3,
            "Σ probability should be 1.0, got {}",
            total_p
        );
    }

    #[test]
    fn branch_is_deterministic() {
        let p = present_branch();
        let a = branch_present(&p, 0x1234_5678, 4);
        let b = branch_present(&p, 0x1234_5678, 4);
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(b.iter()) {
            assert!((x.amplitude - y.amplitude).abs() < 1e-6);
            assert!((x.phase - y.phase).abs() < 1e-6);
            assert_eq!(x.frame_state.narrative_seed, y.frame_state.narrative_seed);
            assert_eq!(x.frame_state.pixel_axes, y.frame_state.pixel_axes);
        }
    }

    #[test]
    fn branch_different_seeds_diverge() {
        let p = present_branch();
        let a = branch_present(&p, 0x1111_1111, 4);
        let b = branch_present(&p, 0x2222_2222, 4);
        let any_diff = a.iter().zip(b.iter()).any(|(x, y)| {
            x.frame_state.narrative_seed != y.frame_state.narrative_seed
        });
        assert!(any_diff, "different intent_seeds should produce different branches");
    }

    #[test]
    fn branch_k1_keeps_full_amplitude() {
        let p = present_branch();
        let branches = branch_present(&p, 0xABCD, 1);
        assert_eq!(branches.len(), 1);
        assert!((branches[0].amplitude - 1.0).abs() < 1e-3);
    }

    #[test]
    fn intent_strength_zero_equals_branch_present() {
        let p = present_branch();
        let intent = [50, 50, 50, 50, 50, 50, 50, 50];
        let with_intent = branch_with_intent(&p, 0xCAFE, &intent, 0.0, 4);
        let without_intent = branch_present(&p, 0xCAFE, 4);
        for (a, b) in with_intent.iter().zip(without_intent.iter()) {
            assert_eq!(a.frame_state.pixel_axes, b.frame_state.pixel_axes);
        }
    }

    #[test]
    fn intent_strength_one_snaps_axes() {
        let p = present_branch();
        let intent = [100, 100, 100, 100, 100, 100, 100, 100];
        let branches = branch_with_intent(&p, 0xFEED, &intent, 1.0, 3);
        for b in &branches {
            assert_eq!(b.frame_state.pixel_axes, intent);
        }
    }

    #[test]
    fn normalize_zero_branches_does_nothing() {
        let mut branches: Vec<WorldBranch> = vec![];
        let result = normalize_amplitudes(&mut branches);
        assert_eq!(result, 0.0);
    }
}
