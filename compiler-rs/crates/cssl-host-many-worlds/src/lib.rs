//! § cssl-host-many-worlds — Quantum superposition temporal-ring
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § T11-W19-E · canonical : `Labyrinth of Apocalypse/systems/many_worlds.csl`
//!
//! § THESIS
//!
//! Conventional temporal-rings (see cssl-host-digital-intelligence-render's
//! `TemporalCoherenceRing` @ depth=3) store 3 LINEAR past-frames and blend
//! them into a single display. That is good for jitter-removal but assumes
//! a SINGLE timeline.
//!
//! `ManyWorldsRing` extends the idea : the present is one branch ; the
//! past is a SUPERPOSITION of N coherent past-branches ; the future is a
//! SUPERPOSITION of K candidate-future-branches that the engine pre-renders
//! in parallel. Player observation collapses the wave-function to one
//! timeline via `measure()`.
//!
//! § WHAT THIS UNLOCKS
//!
//!   · BRANCHING NARRATIVES that genuinely co-exist before player-decision.
//!     The engine renders all K candidate-futures concurrently ; when player
//!     input arrives, we COLLAPSE to the closest-matching branch.
//!
//!   · AKASHIC RECORDS as past-superposition store. Rewinding picks one
//!     past-branch by amplitude × seed (not the linear last-frame).
//!
//!   · "WHAT IF" replay-mode where the player rewinds and explores a
//!     different past-branch.
//!
//!   · PREDICTIVE PROCGEN : K-ahead-frames render in parallel (different
//!     possible inputs) ; collapse picks the one matching the actual input.
//!     Net effect : input-to-display latency goes to zero (the work was
//!     pre-done in the rejected branches).
//!
//! § DETERMINISM
//!
//! Despite the quantum-flavored language, the math is CLASSICAL and
//! REPLAY-DETERMINISTIC. The "amplitude" is a deterministic scalar weight
//! ; "phase" is a deterministic seed-mod that drives narrative-divergence.
//! Given the same (seed · intent · observation) tuple the same branch
//! collapses every time. This is the LoA replay axiom.
//!
//! § ZERO-DEP
//!
//! No `rand`, no `serde`, no allocations beyond `Vec`. All seed-derivation
//! is via inline wyhash-equivalent splittable hash. This keeps the crate
//! sovereign + cheap-to-link + replay-stable across hosts.

#![forbid(unsafe_code)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::similar_names)]

pub mod branch;
pub mod measure;
pub mod rewind;

pub use branch::{branch_present, branch_with_intent, normalize_amplitudes};
pub use measure::{collapse_amplitude, collapse_inner_product, measure};
pub use rewind::{rewind_to_branch, rewind_weighted, RewindKind};

/// 32-byte frame-state primitive. The `ManyWorldsRing` is generic over
/// content but stage-0 uses this fixed-size payload for replay-stability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameState {
    pub pixel_axes: [i16; 8],
    pub narrative_seed: u64,
    pub intent_hash: u32,
    pub tick: u32,
}

impl FrameState {
    pub const ZERO: Self = Self {
        pixel_axes: [0; 8],
        narrative_seed: 0,
        intent_hash: 0,
        tick: 0,
    };

    pub fn new(tick: u32, narrative_seed: u64, intent_hash: u32) -> Self {
        Self {
            pixel_axes: [0; 8],
            narrative_seed,
            intent_hash,
            tick,
        }
    }

    /// Inner-product against another frame-state in axis-space.
    /// Range : [-1.0, 1.0] (cosine-similarity, scale-invariant).
    pub fn inner_product(&self, other: &Self) -> f32 {
        let mut dot: i64 = 0;
        let mut self_sq: i64 = 0;
        let mut other_sq: i64 = 0;
        for i in 0..8 {
            let a = self.pixel_axes[i] as i64;
            let b = other.pixel_axes[i] as i64;
            dot += a * b;
            self_sq += a * a;
            other_sq += b * b;
        }
        if self_sq == 0 || other_sq == 0 {
            return 0.0;
        }
        let denom = ((self_sq as f64) * (other_sq as f64)).sqrt();
        ((dot as f64) / denom) as f32
    }
}

/// One branch of the timeline-superposition.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorldBranch {
    pub timeline_id: u32,
    pub amplitude: f32,
    pub phase: f32,
    pub frame_state: FrameState,
}

impl WorldBranch {
    pub fn new(timeline_id: u32, amplitude: f32, phase: f32, frame_state: FrameState) -> Self {
        Self {
            timeline_id,
            amplitude,
            phase,
            frame_state,
        }
    }

    /// Probability-weight of this branch. Born-rule analogue : |a|^2.
    pub fn probability(&self) -> f32 {
        self.amplitude * self.amplitude
    }
}

/// The many-worlds ring : N past-branches + 1 present + K future-branches.
#[derive(Debug, Clone)]
pub struct ManyWorldsRing {
    pub past: Vec<WorldBranch>,
    pub present: WorldBranch,
    pub futures: Vec<WorldBranch>,
    pub max_past: usize,
    pub max_futures: usize,
    pub next_timeline_id: u32,
}

impl ManyWorldsRing {
    pub fn new(max_past: usize, max_futures: usize) -> Self {
        let present = WorldBranch::new(0, 1.0, 0.0, FrameState::ZERO);
        Self {
            past: Vec::with_capacity(max_past),
            present,
            futures: Vec::with_capacity(max_futures),
            max_past,
            max_futures,
            next_timeline_id: 1,
        }
    }

    pub fn with_present(present: FrameState, max_past: usize, max_futures: usize) -> Self {
        let mut r = Self::new(max_past, max_futures);
        r.present = WorldBranch::new(r.next_timeline_id, 1.0, 0.0, present);
        r.next_timeline_id += 1;
        r
    }

    pub(crate) fn next_id(&mut self) -> u32 {
        let id = self.next_timeline_id;
        self.next_timeline_id += 1;
        id
    }

    /// Push the current present onto the past-superposition.
    pub fn archive_present(&mut self, retained_amplitude: f32, retained_phase: f32) {
        let mut archived = self.present;
        archived.amplitude = retained_amplitude;
        archived.phase = retained_phase;
        self.past.push(archived);
        if self.past.len() > self.max_past {
            self.past.remove(0);
        }
        normalize_amplitudes(&mut self.past);
    }

    /// Branch the present into K candidate futures driven by `intent_seed`.
    pub fn branch(&mut self, intent_seed: u64, k: usize) -> usize {
        let starting_id = self.next_timeline_id;
        let new_branches = branch::branch_present(&self.present, intent_seed, k);
        self.futures.clear();
        let added = new_branches.len().min(self.max_futures);
        for (i, mut b) in new_branches.into_iter().take(added).enumerate() {
            b.timeline_id = starting_id + i as u32;
            self.futures.push(b);
        }
        self.next_timeline_id = starting_id + added as u32;
        normalize_amplitudes(&mut self.futures);
        added
    }

    /// Collapse a future-branch via observation. Returns the timeline_id of
    /// the collapsed branch, or None if futures is empty.
    pub fn measure(&mut self, observation: &FrameState) -> Option<u32> {
        let chosen = measure::measure(&self.futures, observation)?;
        self.archive_present(1.0, self.present.phase);
        let mut new_present = self.futures[chosen];
        let collapsed_id = new_present.timeline_id;
        new_present.amplitude = 1.0;
        new_present.phase = 0.0;
        self.present = new_present;
        self.futures.clear();
        Some(collapsed_id)
    }

    /// Pick a past-branch n-back and re-experience it.
    pub fn rewind(&mut self, n_past: usize) -> Option<u32> {
        let chosen = rewind::rewind_to_branch(&self.past, n_past)?;
        let chosen_branch = self.past[chosen];
        self.archive_present(self.present.amplitude, self.present.phase);
        self.present = WorldBranch::new(
            self.next_id(),
            1.0,
            0.0,
            chosen_branch.frame_state,
        );
        self.past.remove(chosen);
        normalize_amplitudes(&mut self.past);
        Some(chosen_branch.timeline_id)
    }

    pub fn future_probability_sum(&self) -> f32 {
        self.futures.iter().map(WorldBranch::probability).sum()
    }

    pub fn past_probability_sum(&self) -> f32 {
        self.past.iter().map(WorldBranch::probability).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn axis_state(a: i16, b: i16) -> FrameState {
        FrameState {
            pixel_axes: [a, b, 0, 0, 0, 0, 0, 0],
            narrative_seed: 42,
            intent_hash: 7,
            tick: 1,
        }
    }

    #[test]
    fn ring_starts_empty_past_and_futures() {
        let r = ManyWorldsRing::new(8, 4);
        assert!(r.past.is_empty());
        assert!(r.futures.is_empty());
        assert_eq!(r.present.amplitude, 1.0);
    }

    #[test]
    fn frame_state_inner_product_self_is_one() {
        let f = axis_state(100, 50);
        let ip = f.inner_product(&f);
        assert!((ip - 1.0).abs() < 1e-3, "self ip = {}", ip);
    }

    #[test]
    fn frame_state_inner_product_orthogonal() {
        let a = FrameState {
            pixel_axes: [100, 0, 0, 0, 0, 0, 0, 0],
            ..FrameState::ZERO
        };
        let b = FrameState {
            pixel_axes: [0, 100, 0, 0, 0, 0, 0, 0],
            ..FrameState::ZERO
        };
        let ip = a.inner_product(&b);
        assert!(ip.abs() < 1e-3, "orthogonal ip should be ~0, got {}", ip);
    }

    #[test]
    fn world_branch_probability_is_amplitude_squared() {
        let b = WorldBranch::new(1, 0.5, 0.1, FrameState::ZERO);
        let p = b.probability();
        assert!((p - 0.25).abs() < 1e-6);
    }

    #[test]
    fn archive_present_appends_to_past() {
        let mut r = ManyWorldsRing::with_present(axis_state(5, 5), 4, 4);
        r.archive_present(1.0, 0.0);
        assert_eq!(r.past.len(), 1);
        assert!((r.past_probability_sum() - 1.0).abs() < 1e-3);
    }

    #[test]
    fn archive_evicts_oldest_at_capacity() {
        let mut r = ManyWorldsRing::with_present(axis_state(1, 0), 2, 2);
        for tick in 0..5 {
            r.present.frame_state.tick = tick;
            r.archive_present(1.0, 0.0);
        }
        assert_eq!(r.past.len(), 2);
    }
}
