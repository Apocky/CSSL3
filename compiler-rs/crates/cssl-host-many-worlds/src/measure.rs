//! § measure — Wave-function collapse via observation.
//!
//! Given a Vec of candidate-future `WorldBranch` and an observed `FrameState`,
//! `measure()` returns the index of the branch whose amplitude × similarity-
//! to-observation is highest.

use crate::{FrameState, WorldBranch};

/// Default collapse : pick the branch with the highest score, where
/// score = |amplitude| * max(0, IP(branch.frame_state, observation)).
pub fn measure(branches: &[WorldBranch], observation: &FrameState) -> Option<usize> {
    if branches.is_empty() {
        return None;
    }
    let mut best_idx = 0usize;
    let mut best_score = f32::NEG_INFINITY;
    for (i, b) in branches.iter().enumerate() {
        let ip = b.frame_state.inner_product(observation).max(0.0);
        let score = b.amplitude.abs() * ip;
        if score > best_score
            || (score == best_score && b.timeline_id < branches[best_idx].timeline_id)
        {
            best_score = score;
            best_idx = i;
        }
    }
    if best_score == 0.0 {
        return collapse_amplitude(branches);
    }
    Some(best_idx)
}

/// Pick the branch with the largest |amplitude|.
pub fn collapse_amplitude(branches: &[WorldBranch]) -> Option<usize> {
    if branches.is_empty() {
        return None;
    }
    let mut best_idx = 0usize;
    let mut best_amp = branches[0].amplitude.abs();
    for (i, b) in branches.iter().enumerate().skip(1) {
        let a = b.amplitude.abs();
        if a > best_amp || (a == best_amp && b.timeline_id < branches[best_idx].timeline_id) {
            best_amp = a;
            best_idx = i;
        }
    }
    Some(best_idx)
}

/// Pick the branch whose frame-state has the highest inner-product against
/// the observation. Ignores amplitude.
pub fn collapse_inner_product(
    branches: &[WorldBranch],
    observation: &FrameState,
) -> Option<usize> {
    if branches.is_empty() {
        return None;
    }
    let mut best_idx = 0usize;
    let mut best_ip = branches[0].frame_state.inner_product(observation);
    for (i, b) in branches.iter().enumerate().skip(1) {
        let ip = b.frame_state.inner_product(observation);
        if ip > best_ip || (ip == best_ip && b.timeline_id < branches[best_idx].timeline_id) {
            best_ip = ip;
            best_idx = i;
        }
    }
    Some(best_idx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::branch::branch_present;

    fn axis_state(a: i16, b: i16) -> FrameState {
        FrameState {
            pixel_axes: [a, b, 0, 0, 0, 0, 0, 0],
            narrative_seed: 1,
            intent_hash: 0,
            tick: 0,
        }
    }

    #[test]
    fn measure_empty_returns_none() {
        let result = measure(&[], &FrameState::ZERO);
        assert!(result.is_none());
    }

    #[test]
    fn measure_picks_highest_ip_when_amplitudes_equal() {
        let target = axis_state(100, 0);
        let mut branches = vec![
            WorldBranch::new(1, 1.0, 0.0, axis_state(50, 50)),
            WorldBranch::new(2, 1.0, 0.0, axis_state(100, 0)),
            WorldBranch::new(3, 1.0, 0.0, axis_state(0, 100)),
        ];
        crate::branch::normalize_amplitudes(&mut branches);
        let chosen = measure(&branches, &target).unwrap();
        assert_eq!(branches[chosen].timeline_id, 2);
    }

    #[test]
    fn measure_picks_higher_amplitude_when_ips_equal() {
        let target = axis_state(50, 50);
        let mut branches = vec![
            WorldBranch::new(1, 0.3, 0.0, axis_state(50, 50)),
            WorldBranch::new(2, 0.95, 0.0, axis_state(50, 50)),
        ];
        crate::branch::normalize_amplitudes(&mut branches);
        let chosen = measure(&branches, &target).unwrap();
        assert_eq!(branches[chosen].timeline_id, 2);
    }

    #[test]
    fn collapse_amplitude_picks_max() {
        let branches = vec![
            WorldBranch::new(1, 0.1, 0.0, FrameState::ZERO),
            WorldBranch::new(2, 0.9, 0.0, FrameState::ZERO),
            WorldBranch::new(3, 0.3, 0.0, FrameState::ZERO),
        ];
        let chosen = collapse_amplitude(&branches).unwrap();
        assert_eq!(branches[chosen].timeline_id, 2);
    }

    #[test]
    fn measure_is_deterministic_across_calls() {
        let p = WorldBranch::new(
            0,
            1.0,
            0.0,
            FrameState {
                pixel_axes: [10, 20, 30, 40, 50, 60, 70, 80],
                narrative_seed: 5,
                intent_hash: 0,
                tick: 0,
            },
        );
        let mut branches = branch_present(&p, 0xCAFE_BABE, 6);
        crate::branch::normalize_amplitudes(&mut branches);
        let obs = FrameState {
            pixel_axes: [10, 20, 30, 40, 50, 60, 70, 80],
            ..FrameState::ZERO
        };
        let a = measure(&branches, &obs).unwrap();
        let b = measure(&branches, &obs).unwrap();
        let c = measure(&branches, &obs).unwrap();
        assert_eq!(a, b);
        assert_eq!(b, c);
    }

    #[test]
    fn collapse_inner_product_ignores_amplitude() {
        let target = axis_state(100, 0);
        let branches = vec![
            WorldBranch::new(1, 0.99, 0.0, axis_state(0, 100)),
            WorldBranch::new(2, 0.01, 0.0, axis_state(100, 0)),
        ];
        let chosen = collapse_inner_product(&branches, &target).unwrap();
        assert_eq!(branches[chosen].timeline_id, 2);
    }

    #[test]
    fn measure_falls_back_to_amplitude_when_no_ip_signal() {
        let mut branches = vec![
            WorldBranch::new(1, 0.2, 0.0, FrameState::ZERO),
            WorldBranch::new(2, 0.8, 0.0, FrameState::ZERO),
        ];
        crate::branch::normalize_amplitudes(&mut branches);
        let chosen = measure(&branches, &FrameState::ZERO).unwrap();
        assert_eq!(branches[chosen].timeline_id, 2);
    }
}
