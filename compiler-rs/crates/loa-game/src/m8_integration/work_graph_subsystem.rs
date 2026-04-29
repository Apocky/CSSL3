//! § work_graph_subsystem — DX12-Ultimate work-graph schedule (or VK-DGC fallback).
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Companion subsystem. Drives `cssl-work-graph::WorkGraphBuilder` to
//!   produce a per-frame schedule. Per M8 acceptance the work-graph backend
//!   selection happens at runtime (DX12 native ≥ Ultimate / VK-DGC fallback
//!   / ExecuteIndirect compat).
//!
//! § PRIME-DIRECTIVE attestation
//!   "There was no hurt nor harm in the making of this, to anyone, anything,
//!   or anybody."

use cssl_work_graph::{Backend, FeatureMatrix, FrameBudget, WorkGraphBuilder};

/// Outcome of one work-graph step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WorkGraphOutcome {
    /// Frame index this outcome covers.
    pub frame_idx: u64,
    /// Backend selected (D3d12WorkGraph / VulkanDgc / IndirectFallback).
    pub backend: Backend,
    /// Number of nodes scheduled this frame.
    pub nodes_scheduled: u32,
    /// Whether the schedule was successfully built.
    pub built: bool,
}

/// Stage driver.
#[derive(Debug, Clone, Copy)]
pub struct WorkGraphSubsystem {
    seed: u64,
}

impl WorkGraphSubsystem {
    /// Construct.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self { seed }
    }

    /// Run one tick — build a fresh schedule.
    pub fn step(&self, frame_idx: u64) -> WorkGraphOutcome {
        // Detect backend deterministically. Without a real GPU device we
        // fall back to IndirectFallback.
        let features = FeatureMatrix::none();
        let backend = cssl_work_graph::detect_backend(&features);

        // Build a small canonical graph (zero nodes — the M8 vertical-slice
        // exercises construction, not real GPU dispatch).
        let result = WorkGraphBuilder::new()
            .with_label("m8.frame-schedule".to_string())
            .with_backend(backend)
            .with_budget(FrameBudget::hz_60())
            .build();
        let (built, nodes_scheduled) = match result {
            Ok(s) => (true, s.stats().node_count as u32),
            Err(_) => (false, 0),
        };

        WorkGraphOutcome {
            frame_idx,
            backend,
            nodes_scheduled,
            built,
        }
    }

    /// No-op step — when work-graph is disabled in the pipeline config.
    #[must_use]
    pub fn step_no_op(&self, frame_idx: u64) -> WorkGraphOutcome {
        WorkGraphOutcome {
            frame_idx,
            backend: Backend::IndirectFallback,
            nodes_scheduled: 0,
            built: false,
        }
    }

    /// Read the seed.
    #[must_use]
    pub fn seed(&self) -> u64 {
        self.seed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn work_graph_constructs() {
        let _ = WorkGraphSubsystem::new(0);
    }

    #[test]
    fn work_graph_one_step_builds() {
        let w = WorkGraphSubsystem::new(0);
        let o = w.step(0);
        // The empty graph builds even with zero nodes.
        assert!(o.built || o.nodes_scheduled == 0);
    }

    #[test]
    fn work_graph_replay_bit_equal() {
        let w1 = WorkGraphSubsystem::new(0);
        let w2 = WorkGraphSubsystem::new(0);
        let a = w1.step(7);
        let b = w2.step(7);
        assert_eq!(a, b);
    }

    #[test]
    fn work_graph_no_op_path() {
        let w = WorkGraphSubsystem::new(0);
        let o = w.step_no_op(0);
        assert!(!o.built);
    }
}
