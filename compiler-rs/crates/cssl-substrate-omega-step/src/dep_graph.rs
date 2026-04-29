//! Stable topological sort over `OmegaSystem` dependency declarations.
//!
//! § THESIS
//!   `specs/30_SUBSTRATE.csl § OMEGA-STEP § PHASES` orders the per-step
//!   work into well-defined phases (consent-check, sim, projection,
//!   audio, render, telemetry, audit, save). Within phase-4 (sim-substep),
//!   multiple `OmegaSystem`s execute. Their order is determined by the
//!   read+write Ω-tensor dep graph each system declares via
//!   `OmegaSystem::dependencies()`.
//!
//!   For replay-determinism the topological sort MUST be **stable** :
//!   given the same insertion order + the same dep declarations, it MUST
//!   produce the same execution order across runs + machines. We use
//!   Kahn's algorithm with insertion-order tie-breaking.
//!
//! § ALGORITHM (Kahn's, stable form)
//!   1. Compute indegree of every node from the dep edges.
//!   2. Initialize a `roots` queue with every indegree-0 node, in
//!      insertion order.
//!   3. While roots non-empty :
//!        - Pop the next root (FIFO ⇒ preserves insertion order).
//!        - Append it to the output ordering.
//!        - For each node it points-to : decrement indegree ; if
//!          indegree reaches 0, push that node to roots (preserving
//!          insertion-order tie-breaking).
//!   4. If the output count != input count, there is a cycle.
//!
//! § ON THE EDGE DIRECTION
//!   Convention : an edge `a -> b` means "a must run BEFORE b" — i.e.,
//!   b depends on a's output. The `OmegaSystem::dependencies()` method
//!   returns the SystemIds whose output this system reads ; those are
//!   the predecessors.

use crate::system::SystemId;

/// Failure modes of the topological sort.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DepGraphError {
    /// Cycle detected. The `at` field names *one* member of the cycle —
    /// stage-0 does not enumerate the full cycle path (deferred). The
    /// caller surfaces this as `OmegaError::DependencyCycle`.
    Cycle { at: SystemId },
    /// A system declared a dependency on a system-id that was never
    /// registered.
    UnknownDependency {
        dependent: SystemId,
        missing: SystemId,
    },
}

/// Stable topological sort.
///
/// `nodes` is the insertion-ordered list of system-ids ; `deps_of`
/// returns the predecessor-ids for each node.
///
/// Returns the topologically sorted insertion-order on success, or
/// a `DepGraphError` on cycle / unknown-dep.
///
/// § COMPLEXITY  O(V + E) where V = |nodes|, E = total deps.
pub fn topo_sort_stable<F>(nodes: &[SystemId], deps_of: F) -> Result<Vec<SystemId>, DepGraphError>
where
    F: Fn(SystemId) -> Vec<SystemId>,
{
    use std::collections::VecDeque;

    let n = nodes.len();
    if n == 0 {
        return Ok(Vec::new());
    }

    // Index-of-node lookup. Stage-0 form : linear scan would be O(V * E)
    // for sufficiently large graphs ; we use a small Vec-based map keyed
    // by SystemId.0 (a u64). Most schedulers will have < 1k systems
    // ⇒ this is fine ; if it ever grows, swap in a HashMap.
    let id_to_idx = |id: SystemId| -> Option<usize> { nodes.iter().position(|n| *n == id) };

    // Build forward-adjacency : for each node a, list of nodes b such
    // that a -> b (i.e., b depends on a, so a must run first).
    let mut forward: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut indegree: Vec<usize> = vec![0; n];

    for (i, &node) in nodes.iter().enumerate() {
        for dep in deps_of(node) {
            let Some(j) = id_to_idx(dep) else {
                return Err(DepGraphError::UnknownDependency {
                    dependent: node,
                    missing: dep,
                });
            };
            // Edge : dep -> node (j -> i). i depends on j ; j runs before i.
            forward[j].push(i);
            indegree[i] += 1;
        }
    }

    // Insertion-order roots queue.
    let mut roots: VecDeque<usize> = VecDeque::new();
    for (i, &deg) in indegree.iter().enumerate() {
        if deg == 0 {
            roots.push_back(i);
        }
    }

    let mut output: Vec<SystemId> = Vec::with_capacity(n);
    while let Some(idx) = roots.pop_front() {
        output.push(nodes[idx]);
        for &next in &forward[idx] {
            indegree[next] -= 1;
            if indegree[next] == 0 {
                roots.push_back(next);
            }
        }
    }

    if output.len() != n {
        // Find any unprocessed node — that's our cycle witness.
        for (i, &deg) in indegree.iter().enumerate() {
            if deg > 0 {
                return Err(DepGraphError::Cycle { at: nodes[i] });
            }
        }
        // Defensive — should be unreachable.
        return Err(DepGraphError::Cycle { at: nodes[0] });
    }

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(i: u64) -> SystemId {
        SystemId(i)
    }

    #[test]
    fn empty_returns_empty() {
        let out = topo_sort_stable(&[], |_| Vec::new()).unwrap();
        assert!(out.is_empty());
    }

    #[test]
    fn single_node_no_deps() {
        let out = topo_sort_stable(&[id(0)], |_| Vec::new()).unwrap();
        assert_eq!(out, vec![id(0)]);
    }

    #[test]
    fn linear_chain_preserves_order() {
        // 0 -> 1 -> 2 (chain : 1 depends on 0, 2 depends on 1).
        let nodes = vec![id(0), id(1), id(2)];
        let deps = |s: SystemId| match s.0 {
            1 => vec![id(0)],
            2 => vec![id(1)],
            _ => Vec::new(),
        };
        let out = topo_sort_stable(&nodes, deps).unwrap();
        assert_eq!(out, vec![id(0), id(1), id(2)]);
    }

    #[test]
    fn diamond_dag_stable() {
        //     0
        //    / \
        //   1   2
        //    \ /
        //     3
        let nodes = vec![id(0), id(1), id(2), id(3)];
        let deps = |s: SystemId| match s.0 {
            1 | 2 => vec![id(0)],
            3 => vec![id(1), id(2)],
            _ => Vec::new(),
        };
        let out = topo_sort_stable(&nodes, deps).unwrap();
        // Stable form : 0 first, then 1 + 2 in insertion order, then 3.
        assert_eq!(out, vec![id(0), id(1), id(2), id(3)]);
    }

    #[test]
    fn parallel_independent_systems_preserve_insertion() {
        // 0, 1, 2 all independent.
        let nodes = vec![id(2), id(0), id(1)]; // scrambled insertion.
        let out = topo_sort_stable(&nodes, |_| Vec::new()).unwrap();
        // Insertion order is `[2, 0, 1]` ; stable sort preserves it.
        assert_eq!(out, vec![id(2), id(0), id(1)]);
    }

    #[test]
    fn cycle_detected() {
        // 0 -> 1 -> 0
        let nodes = vec![id(0), id(1)];
        let deps = |s: SystemId| match s.0 {
            0 => vec![id(1)],
            1 => vec![id(0)],
            _ => Vec::new(),
        };
        let err = topo_sort_stable(&nodes, deps).unwrap_err();
        assert!(matches!(err, DepGraphError::Cycle { .. }));
    }

    #[test]
    fn self_loop_detected() {
        // 0 -> 0
        let nodes = vec![id(0)];
        let deps = |s: SystemId| match s.0 {
            0 => vec![id(0)],
            _ => Vec::new(),
        };
        let err = topo_sort_stable(&nodes, deps).unwrap_err();
        assert!(matches!(err, DepGraphError::Cycle { at: SystemId(0) }));
    }

    #[test]
    fn unknown_dependency_detected() {
        let nodes = vec![id(0)];
        let deps = |_: SystemId| vec![id(99)];
        let err = topo_sort_stable(&nodes, deps).unwrap_err();
        assert!(matches!(
            err,
            DepGraphError::UnknownDependency {
                dependent: SystemId(0),
                missing: SystemId(99)
            }
        ));
    }

    #[test]
    fn deterministic_across_repeated_runs() {
        // Same graph + same insertion order ⇒ same output across calls.
        let nodes = vec![id(5), id(3), id(7), id(1)];
        let deps = |s: SystemId| match s.0 {
            7 => vec![id(3)],
            1 => vec![id(5), id(7)],
            _ => Vec::new(),
        };
        let out1 = topo_sort_stable(&nodes, deps).unwrap();
        let out2 = topo_sort_stable(&nodes, deps).unwrap();
        assert_eq!(out1, out2);
    }
}
