//! § checkpoint — checkpoint-every-N forward states + recompute-on-demand.
//!
//! Rationale (per specs/36 § Checkpointing) :
//!
//!   Memory-budget : storing all L^{(k)} = O(MAX_ITER · cells · coefs)
//!   Strategy : checkpoint every N=16 iterations · re-compute intermediate
//!     states from checkpoint
//!   Memory ↓ from O(MAX_ITER) to O(MAX_ITER/N + N) = O(√MAX_ITER) sweet-spot
//!
//! API :
//!   - `CheckpointPolicy` : configures stride + storage cap.
//!   - `Checkpoint` : a single saved state (iteration index + flat state vec).
//!   - `CheckpointStore` : ordered ring of checkpoints with O(1) lookup of
//!     the nearest checkpoint at-or-before a given iteration.
//!
//! The store is policy-agnostic about WHAT is stored — `Vec<f32>` lets
//! callers choose granularity (per-cell light-state vector, per-cell KAN
//! coefficient block, etc.). The adjoint driver uses one store per
//! field-trajectory.

use thiserror::Error;

/// Error surfaced by checkpoint operations.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum CheckpointError {
    #[error("checkpoint stride must be ≥ 1, got 0")]
    ZeroStride,
    #[error("checkpoint store is empty")]
    Empty,
    #[error("requested iteration {0} but earliest checkpoint is {1}")]
    BeforeFirst(u32, u32),
    #[error("dimension-mismatch: state has {0} scalars, store expects {1}")]
    DimensionMismatch(u32, u32),
    #[error("checkpoint store at capacity {0}")]
    AtCapacity(u32),
}

/// Policy for when (and how many) checkpoints to retain.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CheckpointPolicy {
    /// Save a checkpoint every `stride` forward iterations. Must be ≥ 1.
    pub stride: u32,
    /// Maximum number of checkpoints to keep. Once the cap is hit the
    /// oldest checkpoint is dropped (ring-buffer semantics).
    pub capacity: u32,
}

impl CheckpointPolicy {
    /// Default policy : stride=16 + capacity=8 → covers MAX_ITER=64+ with
    /// O(√MAX_ITER) memory.
    #[must_use]
    pub const fn standard() -> Self {
        Self {
            stride: crate::DEFAULT_CHECKPOINT_STRIDE,
            capacity: 8,
        }
    }

    /// Tight policy for small-scene fits (MAX_ITER ≤ 16) — checkpoint every
    /// step. Memory = O(MAX_ITER) but trajectory is short.
    #[must_use]
    pub const fn dense(capacity: u32) -> Self {
        Self {
            stride: 1,
            capacity,
        }
    }

    /// Validate the policy.
    pub fn validate(self) -> Result<(), CheckpointError> {
        if self.stride == 0 {
            return Err(CheckpointError::ZeroStride);
        }
        Ok(())
    }
}

/// One stored checkpoint : (iteration-index, flat-state-snapshot).
#[derive(Clone, Debug, PartialEq)]
pub struct Checkpoint {
    /// Iteration index this checkpoint was taken at.
    pub iter: u32,
    /// Flat state snapshot — caller-determined shape ; same length as the
    /// store's expected dimension.
    pub state: Vec<f32>,
}

/// Ordered ring of checkpoints, oldest-first.
#[derive(Clone, Debug)]
pub struct CheckpointStore {
    policy: CheckpointPolicy,
    /// Expected scalar count per checkpoint state. First push fixes this.
    state_dim: Option<u32>,
    /// Inner storage. Sorted by `iter` ascending. We rely on the push-order
    /// invariant (callers append iterations monotonically).
    inner: Vec<Checkpoint>,
}

impl CheckpointStore {
    /// New empty store with `policy`.
    pub fn new(policy: CheckpointPolicy) -> Result<Self, CheckpointError> {
        policy.validate()?;
        Ok(Self {
            policy,
            state_dim: None,
            inner: Vec::with_capacity(policy.capacity as usize),
        })
    }

    /// Number of stored checkpoints.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// `true` when no checkpoints stored.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Should the driver checkpoint at iteration `iter`?
    /// True iff `iter % stride == 0` (catches iter=0 + every stride after).
    #[must_use]
    pub fn should_checkpoint(&self, iter: u32) -> bool {
        iter % self.policy.stride == 0
    }

    /// Push a new checkpoint at `iter` with the given `state`.
    /// Drops the oldest checkpoint if the store is at capacity.
    pub fn push(&mut self, iter: u32, state: Vec<f32>) -> Result<(), CheckpointError> {
        match self.state_dim {
            None => self.state_dim = Some(state.len() as u32),
            Some(d) if d == state.len() as u32 => {}
            Some(d) => {
                return Err(CheckpointError::DimensionMismatch(state.len() as u32, d));
            }
        }
        if self.inner.len() as u32 >= self.policy.capacity {
            // Drop oldest (front).
            self.inner.remove(0);
        }
        // Maintain ascending-iter order.
        if let Some(last) = self.inner.last() {
            if iter < last.iter {
                // Insert in sorted position.
                let pos = self.inner.partition_point(|c| c.iter < iter);
                self.inner.insert(pos, Checkpoint { iter, state });
                return Ok(());
            }
        }
        self.inner.push(Checkpoint { iter, state });
        Ok(())
    }

    /// Find the nearest checkpoint at-or-before `iter`. None if no such
    /// checkpoint exists.
    #[must_use]
    pub fn nearest_at_or_before(&self, iter: u32) -> Option<&Checkpoint> {
        // Linear scan from the back is O(stored) ; with capacity ≤ 8
        // (default) this is ~constant.
        self.inner.iter().rev().find(|c| c.iter <= iter)
    }

    /// Recompute the state at iteration `iter` by re-running `step` from
    /// the nearest checkpoint at-or-before `iter`.
    ///
    /// `step(state, k) → next_state` advances the trajectory by one
    /// iteration (caller-provided semantics).
    pub fn recompute_to<F>(&self, iter: u32, mut step: F) -> Result<Vec<f32>, CheckpointError>
    where
        F: FnMut(&[f32], u32) -> Vec<f32>,
    {
        if self.is_empty() {
            return Err(CheckpointError::Empty);
        }
        let near = self
            .nearest_at_or_before(iter)
            .ok_or_else(|| CheckpointError::BeforeFirst(iter, self.inner[0].iter))?;
        let mut state = near.state.clone();
        let mut k = near.iter;
        while k < iter {
            state = step(&state, k);
            k += 1;
        }
        Ok(state)
    }

    /// Iterate the stored checkpoints in ascending-iter order.
    pub fn iter(&self) -> impl Iterator<Item = &Checkpoint> {
        self.inner.iter()
    }

    /// The policy this store was constructed with.
    #[must_use]
    pub fn policy(&self) -> CheckpointPolicy {
        self.policy
    }

    /// Estimated memory footprint in bytes (states only, ignoring overhead).
    #[must_use]
    pub fn memory_bytes(&self) -> usize {
        self.inner
            .iter()
            .map(|c| c.state.len() * core::mem::size_of::<f32>())
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_policy_matches_documented_default() {
        let p = CheckpointPolicy::standard();
        assert_eq!(p.stride, crate::DEFAULT_CHECKPOINT_STRIDE);
        assert_eq!(p.stride, 16);
    }

    #[test]
    fn zero_stride_rejected() {
        let p = CheckpointPolicy {
            stride: 0,
            capacity: 4,
        };
        assert!(matches!(p.validate(), Err(CheckpointError::ZeroStride)));
    }

    #[test]
    fn store_should_checkpoint_at_stride_boundaries() {
        let p = CheckpointPolicy {
            stride: 4,
            capacity: 8,
        };
        let s = CheckpointStore::new(p).unwrap();
        assert!(s.should_checkpoint(0));
        assert!(!s.should_checkpoint(1));
        assert!(s.should_checkpoint(4));
        assert!(s.should_checkpoint(8));
        assert!(!s.should_checkpoint(7));
    }

    #[test]
    fn nearest_at_or_before_picks_correct_checkpoint() {
        let mut s = CheckpointStore::new(CheckpointPolicy {
            stride: 1,
            capacity: 8,
        })
        .unwrap();
        s.push(0, vec![1.0]).unwrap();
        s.push(4, vec![2.0]).unwrap();
        s.push(8, vec![3.0]).unwrap();
        assert_eq!(s.nearest_at_or_before(0).unwrap().iter, 0);
        assert_eq!(s.nearest_at_or_before(3).unwrap().iter, 0);
        assert_eq!(s.nearest_at_or_before(4).unwrap().iter, 4);
        assert_eq!(s.nearest_at_or_before(7).unwrap().iter, 4);
        assert_eq!(s.nearest_at_or_before(8).unwrap().iter, 8);
        assert_eq!(s.nearest_at_or_before(100).unwrap().iter, 8);
    }

    #[test]
    fn capacity_drops_oldest() {
        let mut s = CheckpointStore::new(CheckpointPolicy {
            stride: 1,
            capacity: 2,
        })
        .unwrap();
        s.push(0, vec![1.0]).unwrap();
        s.push(1, vec![2.0]).unwrap();
        s.push(2, vec![3.0]).unwrap();
        assert_eq!(s.len(), 2);
        assert_eq!(s.iter().next().unwrap().iter, 1);
    }

    #[test]
    fn dimension_mismatch_after_first_push() {
        let mut s = CheckpointStore::new(CheckpointPolicy {
            stride: 1,
            capacity: 4,
        })
        .unwrap();
        s.push(0, vec![1.0, 2.0]).unwrap();
        let r = s.push(1, vec![1.0]);
        assert!(matches!(r, Err(CheckpointError::DimensionMismatch(1, 2))));
    }

    #[test]
    fn recompute_to_replays_step_function() {
        let mut s = CheckpointStore::new(CheckpointPolicy {
            stride: 4,
            capacity: 4,
        })
        .unwrap();
        // Checkpoint at iter=0 with state=[10.0].
        s.push(0, vec![10.0]).unwrap();
        // step(state, k) = state + 1 each iteration.
        let step = |state: &[f32], _k: u32| -> Vec<f32> { state.iter().map(|x| x + 1.0).collect() };
        // Recompute to iter=3 → 10 + 3 = 13.
        let r = s.recompute_to(3, step).unwrap();
        assert!((r[0] - 13.0).abs() < 1e-6);
    }

    #[test]
    fn memory_bytes_correct() {
        let mut s = CheckpointStore::new(CheckpointPolicy {
            stride: 1,
            capacity: 4,
        })
        .unwrap();
        s.push(0, vec![1.0, 2.0, 3.0]).unwrap();
        s.push(1, vec![4.0, 5.0, 6.0]).unwrap();
        // 2 * 3 * 4 = 24 bytes.
        assert_eq!(s.memory_bytes(), 24);
    }
}
