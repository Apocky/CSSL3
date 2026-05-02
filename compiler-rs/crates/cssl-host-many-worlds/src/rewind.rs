//! § rewind — Past-branch retrieval.
//!
//! The `past` Vec in `ManyWorldsRing` is a SUPERPOSITION of past-branches,
//! each carrying amplitude+phase. Rewind selects ONE of them by some rule
//! (n-back, weighted-sample, max-amplitude) and promotes it to the new
//! present.

use crate::WorldBranch;

/// Modes for past-branch selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RewindKind {
    /// Most-recent past-branch. n=0 = newest, n=1 = one-before, etc.
    NBack(usize),
    /// Branch with the highest |amplitude|.
    MaxAmplitude,
    /// Branch sampled by deterministic seed × amplitude^2.
    WeightedSeed(u64),
    /// Branch with the specified timeline_id.
    ByTimelineId(u32),
}

/// Find the index in `past` corresponding to `n_past` (0 = most-recent).
pub fn rewind_to_branch(past: &[WorldBranch], n_past: usize) -> Option<usize> {
    if past.is_empty() || n_past >= past.len() {
        return None;
    }
    Some(past.len() - 1 - n_past)
}

/// Generic dispatch for `RewindKind`.
pub fn rewind_weighted(past: &[WorldBranch], kind: RewindKind) -> Option<usize> {
    match kind {
        RewindKind::NBack(n) => rewind_to_branch(past, n),
        RewindKind::MaxAmplitude => {
            if past.is_empty() {
                return None;
            }
            let mut best_idx = 0usize;
            let mut best_amp = past[0].amplitude.abs();
            for (i, b) in past.iter().enumerate().skip(1) {
                let a = b.amplitude.abs();
                if a > best_amp
                    || (a == best_amp && b.timeline_id < past[best_idx].timeline_id)
                {
                    best_amp = a;
                    best_idx = i;
                }
            }
            Some(best_idx)
        }
        RewindKind::WeightedSeed(seed) => {
            if past.is_empty() {
                return None;
            }
            let total_p: f32 = past.iter().map(|b| b.probability()).sum();
            if total_p <= 0.0 {
                return Some(past.len() - 1);
            }
            let frac = (seed as f64) / (u64::MAX as f64);
            let threshold = (frac * (total_p as f64)) as f32;
            let mut acc = 0.0_f32;
            for (i, b) in past.iter().enumerate() {
                acc += b.probability();
                if acc >= threshold {
                    return Some(i);
                }
            }
            Some(past.len() - 1)
        }
        RewindKind::ByTimelineId(id) => {
            past.iter().position(|b| b.timeline_id == id)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FrameState;

    fn make_past() -> Vec<WorldBranch> {
        let mut v = vec![
            WorldBranch::new(10, 0.1, 0.0, FrameState { tick: 0, ..FrameState::ZERO }),
            WorldBranch::new(11, 0.4, 0.0, FrameState { tick: 1, ..FrameState::ZERO }),
            WorldBranch::new(12, 0.3, 0.0, FrameState { tick: 2, ..FrameState::ZERO }),
            WorldBranch::new(13, 0.5, 0.0, FrameState { tick: 3, ..FrameState::ZERO }),
        ];
        crate::branch::normalize_amplitudes(&mut v);
        v
    }

    #[test]
    fn rewind_to_branch_zero_returns_newest() {
        let past = make_past();
        let idx = rewind_to_branch(&past, 0).unwrap();
        assert_eq!(past[idx].timeline_id, 13);
    }

    #[test]
    fn rewind_to_branch_n_back() {
        let past = make_past();
        let idx_1 = rewind_to_branch(&past, 1).unwrap();
        assert_eq!(past[idx_1].timeline_id, 12);
        let idx_3 = rewind_to_branch(&past, 3).unwrap();
        assert_eq!(past[idx_3].timeline_id, 10);
    }

    #[test]
    fn rewind_too_far_returns_none() {
        let past = make_past();
        let idx = rewind_to_branch(&past, 99);
        assert!(idx.is_none());
    }

    #[test]
    fn rewind_empty_past_returns_none() {
        let idx = rewind_to_branch(&[], 0);
        assert!(idx.is_none());
    }

    #[test]
    fn rewind_max_amplitude_picks_highest() {
        let past = make_past();
        let idx = rewind_weighted(&past, RewindKind::MaxAmplitude).unwrap();
        assert_eq!(past[idx].timeline_id, 13);
    }

    #[test]
    fn rewind_by_timeline_id_finds_match() {
        let past = make_past();
        let idx = rewind_weighted(&past, RewindKind::ByTimelineId(11)).unwrap();
        assert_eq!(past[idx].timeline_id, 11);
    }

    #[test]
    fn rewind_by_timeline_id_returns_none_for_missing() {
        let past = make_past();
        let idx = rewind_weighted(&past, RewindKind::ByTimelineId(999));
        assert!(idx.is_none());
    }

    #[test]
    fn rewind_weighted_seed_is_deterministic() {
        let past = make_past();
        let a = rewind_weighted(&past, RewindKind::WeightedSeed(0x1234_5678_9ABC_DEF0));
        let b = rewind_weighted(&past, RewindKind::WeightedSeed(0x1234_5678_9ABC_DEF0));
        assert_eq!(a, b);
    }

    #[test]
    fn rewind_weighted_zero_probability_falls_back_to_last() {
        let past: Vec<WorldBranch> = vec![
            WorldBranch::new(1, 0.0, 0.0, FrameState::ZERO),
            WorldBranch::new(2, 0.0, 0.0, FrameState::ZERO),
        ];
        let idx = rewind_weighted(&past, RewindKind::WeightedSeed(42)).unwrap();
        assert_eq!(idx, 1);
    }
}
