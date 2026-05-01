//! § wired_causal_seed — wrapper around `cssl-host-causal-seed`.
//!
//! § T11-W5c-LOA-HOST-WIRE
//!   Re-exports the story-as-physics DAG types + integrator so MCP tools
//!   can probe the DAG node count without each call-site reaching across
//!   the path-dep.
//!
//! § wrapped surface
//!   - [`CausalDag`] / [`DagErr`] — DAG structure + result envelope.
//!   - [`CausalNode`] / [`NodeKind`] — node types.
//!   - [`CausalEdge`] / [`EdgeKind`] / [`EdgeErr`] — edge types.
//!   - [`CausalIntegrator`] / [`CausalEffect`] / [`LinearEffect`] /
//!     [`WorldVector`] — integration surface.
//!
//! § ATTESTATION ¬ harm — wrapper is a re-export shim ; pure-math only.

pub use cssl_host_causal_seed::{
    CausalDag, CausalEdge, CausalEffect, CausalIntegrator, CausalNode, DagErr, EdgeErr, EdgeKind,
    LinearEffect, NodeKind, WorldVector, PRIME_DIRECTIVE_BANNER, VERSION,
};

/// Convenience : node count of an optional DAG. Returns 0 if no DAG is
/// attached. Used by the `causal.dag_node_count` MCP tool as a basic
/// shape probe before any session has wired its narrative-DAG.
#[must_use]
pub fn dag_node_count(dag: Option<&CausalDag>) -> usize {
    dag.map_or(0, CausalDag::node_count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_dag_yields_zero_count() {
        assert_eq!(dag_node_count(None), 0);
    }

    #[test]
    fn fresh_dag_has_zero_nodes() {
        let dag = CausalDag::new();
        assert_eq!(dag_node_count(Some(&dag)), 0);
    }

    #[test]
    fn dag_after_add_has_correct_count() {
        let mut dag = CausalDag::new();
        let _ = dag.add_node(NodeKind::StoryBeat, "spawn");
        let _ = dag.add_node(NodeKind::Event, "look");
        assert_eq!(dag_node_count(Some(&dag)), 2);
    }
}
