//! Work-graph error types.
//!
//! § DESIGN
//!   `WorkGraphError` folds the failure cases of work-graph build + dispatch
//!   into one diagnosable enum. Each variant carries enough context that a
//!   harness can attribute the failure to a specific node, a missing buffer,
//!   or a backend-feature gap.

use thiserror::Error;

/// Errors emitted by the work-graph crate.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum WorkGraphError {
    /// The selected backend is not available on this host.
    #[error("work-graph backend `{backend}` not supported : {reason}")]
    BackendUnsupported {
        /// Backend tag (e.g., `"d3d12-work-graph"`, `"vulkan-dgc"`).
        backend: String,
        /// Detail for the user-facing diagnostic.
        reason: String,
    },

    /// A node references a producer that was never declared.
    #[error("work-graph node `{node}` references undeclared input `{input}`")]
    UndeclaredInput {
        /// Node that requested the missing input.
        node: String,
        /// Missing input handle.
        input: String,
    },

    /// The DAG has a cycle.
    #[error("work-graph DAG contains cycle through node `{node}`")]
    Cycle {
        /// One node along the cycle.
        node: String,
    },

    /// A node was added with the same id twice.
    #[error("work-graph already contains node `{node}`")]
    DuplicateNode {
        /// Duplicate id.
        node: String,
    },

    /// The graph is empty when at least one node is required.
    #[error("work-graph is empty (no nodes added)")]
    Empty,

    /// A node's dispatch arguments are invalid (e.g., zero thread groups).
    #[error("work-graph node `{node}` invalid args : {reason}")]
    InvalidArgs {
        /// Offending node.
        node: String,
        /// Why the args are invalid.
        reason: String,
    },

    /// A mesh-node was requested but the backend does not support mesh-nodes.
    #[error("work-graph mesh-node `{node}` requested but backend `{backend}` does not support mesh-nodes")]
    MeshNodeUnsupported {
        /// Mesh-node id.
        node: String,
        /// Backend that refused.
        backend: String,
    },

    /// The compiled schedule does not match the live backend (e.g., compiled
    /// against work-graphs but trying to dispatch via DGC).
    #[error("schedule compiled for `{compiled}` but dispatched on `{live}`")]
    BackendMismatch {
        /// Backend the schedule was compiled for.
        compiled: String,
        /// Backend currently active.
        live: String,
    },

    /// A capacity bound was exceeded (e.g., > 16K mesh-node groups per
    /// dispatch).
    #[error("work-graph capacity exceeded for `{what}` : got {got}, limit {limit}")]
    CapacityExceeded {
        /// Knob that overflowed (e.g., `"mesh-node groups"`).
        what: String,
        /// Observed count.
        got: u32,
        /// Hard limit.
        limit: u32,
    },

    /// Density-budget refused : entity count exceeds spec § IV envelope.
    #[error("density-budget refusal : {what} = {observed} > limit {limit}")]
    DensityBudget {
        /// What overran (e.g., `"T1 entity count"`).
        what: String,
        /// Observed count.
        observed: u64,
        /// Spec limit.
        limit: u64,
    },

    /// Frame-budget refusal : projected ms-cost exceeds 8.33ms / 16.67ms.
    #[error("frame-budget refusal : projected {projected_us}us > {budget_us}us @ {target_hz}Hz")]
    FrameBudget {
        /// Projected dispatch cost in microseconds.
        projected_us: u32,
        /// Frame budget in microseconds.
        budget_us: u32,
        /// Target refresh rate.
        target_hz: u32,
    },

    /// Underlying D3D12 backend error (proxied).
    #[error("d3d12 backend : {0}")]
    D3d12(String),

    /// Underlying Vulkan backend error (proxied).
    #[error("vulkan backend : {0}")]
    Vulkan(String),
}

impl WorkGraphError {
    /// Build an `UndeclaredInput` variant.
    #[must_use]
    pub fn undeclared(node: impl Into<String>, input: impl Into<String>) -> Self {
        Self::UndeclaredInput {
            node: node.into(),
            input: input.into(),
        }
    }

    /// Build a `Cycle` variant.
    #[must_use]
    pub fn cycle(node: impl Into<String>) -> Self {
        Self::Cycle { node: node.into() }
    }

    /// Build a `DuplicateNode` variant.
    #[must_use]
    pub fn duplicate(node: impl Into<String>) -> Self {
        Self::DuplicateNode { node: node.into() }
    }

    /// Build an `InvalidArgs` variant.
    #[must_use]
    pub fn invalid_args(node: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::InvalidArgs {
            node: node.into(),
            reason: reason.into(),
        }
    }

    /// Build a `BackendUnsupported` variant.
    #[must_use]
    pub fn backend_unsupported(backend: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::BackendUnsupported {
            backend: backend.into(),
            reason: reason.into(),
        }
    }

    /// True iff this is a "host has no capable hardware" condition (skip-test
    /// territory).
    #[must_use]
    pub const fn is_unsupported(&self) -> bool {
        matches!(
            self,
            Self::BackendUnsupported { .. } | Self::MeshNodeUnsupported { .. }
        )
    }
}

/// Crate-wide `Result` alias.
pub type Result<T> = core::result::Result<T, WorkGraphError>;

#[cfg(test)]
mod tests {
    use super::WorkGraphError;

    #[test]
    fn undeclared_constructor() {
        let e = WorkGraphError::undeclared("KANBRDFEval", "RC-cascade-2");
        let s = format!("{e}");
        assert!(s.contains("KANBRDFEval"));
        assert!(s.contains("RC-cascade-2"));
    }

    #[test]
    fn cycle_constructor() {
        let e = WorkGraphError::cycle("Stage7");
        assert!(format!("{e}").contains("Stage7"));
    }

    #[test]
    fn invalid_args_constructor() {
        let e = WorkGraphError::invalid_args("WaveSolver", "thread_groups_x=0");
        let s = format!("{e}");
        assert!(s.contains("WaveSolver"));
        assert!(s.contains("thread_groups_x=0"));
    }

    #[test]
    fn backend_unsupported_classifies() {
        let e =
            WorkGraphError::backend_unsupported("d3d12-work-graph", "WorkGraphsTier=NotSupported");
        assert!(e.is_unsupported());
    }

    #[test]
    fn duplicate_constructor() {
        let e = WorkGraphError::duplicate("WaveSolver");
        assert!(format!("{e}").contains("WaveSolver"));
    }

    #[test]
    fn empty_renders() {
        assert_eq!(
            format!("{}", WorkGraphError::Empty),
            "work-graph is empty (no nodes added)"
        );
    }

    #[test]
    fn capacity_exceeded_renders() {
        let e = WorkGraphError::CapacityExceeded {
            what: "mesh-node groups".into(),
            got: 32_768,
            limit: 16_384,
        };
        let s = format!("{e}");
        assert!(s.contains("mesh-node groups"));
        assert!(s.contains("32768"));
        assert!(s.contains("16384"));
    }

    #[test]
    fn density_budget_renders_with_units() {
        let e = WorkGraphError::DensityBudget {
            what: "T1 entity count".into(),
            observed: 6_000,
            limit: 5_000,
        };
        assert!(format!("{e}").contains("T1 entity count"));
    }

    #[test]
    fn frame_budget_carries_target_hz() {
        let e = WorkGraphError::FrameBudget {
            projected_us: 9_500,
            budget_us: 8_330,
            target_hz: 120,
        };
        assert!(format!("{e}").contains("120Hz"));
    }
}
