//! D3D12-Ultimate work-graph emission.
//!
//! § DESIGN
//!   This module is the bridge between [`crate::Schedule`] (the
//!   backend-neutral DAG) and the D3D12_WORK_GRAPH_DESC FFI surface in
//!   `cssl-host-d3d12`. It produces a compiled `WorkGraphD3d12` artifact
//!   carrying :
//!     - the program-name (matches `D3D12_WORK_GRAPH_DESC.ProgramName`)
//!     - per-node entry-point names (matches the `EntryPointName` in the
//!       state-object subobjects)
//!     - per-node dispatch grids
//!     - mesh-node flags
//!
//!   At runtime the host calls `device.CreateStateObject(WORK_GRAPH)` once
//!   to get an `ID3D12StateObject`, queries
//!   `ID3D12WorkGraphProperties1::GetEntrypointIndex` for each entry, and
//!   calls `ID3D12GraphicsCommandList10::DispatchGraph` to fire the entire
//!   schedule from a single command-buffer record.
//!
//! § FFI-DEFER
//!   For T11-D123 the actual `windows-rs` FFI for the WORK_GRAPH state-object
//!   structs lands in a follow-up dispatch — at this slice we focus on the
//!   compile/dispatch ABI : a `WorkGraphD3d12` is the *source-of-truth*
//!   representation that the future FFI will read. All fields are public
//!   for backend consumption.

use crate::backend::Backend;
use crate::dispatch::DispatchArgs;
use crate::error::{Result, WorkGraphError};
use crate::node::NodeKind;
use crate::schedule::Schedule;

/// Per-node entry record (mirrors `D3D12_NODE_TYPE` + entry-point info).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkGraphEntry {
    /// Node id (= `EntryPointName` in the state-object subobject).
    pub entry_point: String,
    /// Dispatch grid (used by `DispatchGraph(NumNodes,*,thread_group=...)`).
    pub dispatch: DispatchArgs,
    /// True if this is a mesh-node.
    pub is_mesh: bool,
    /// Shader-blob tag the runtime resolves to a real DXIL blob.
    pub shader_tag: String,
    /// Estimated cost (us).
    pub est_cost_us: u32,
}

/// A compiled D3D12-Ultimate work-graph descriptor ready for state-object
/// creation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkGraphD3d12 {
    /// `D3D12_WORK_GRAPH_DESC.ProgramName` — required, ≤ 64 chars.
    pub program_name: String,
    /// Topo-sorted entry list ; mirrors `Schedule::order()`.
    pub entries: Vec<WorkGraphEntry>,
    /// Total estimated cost (us) for telemetry overlay.
    pub est_cost_us: u32,
    /// Whether this work-graph contains mesh-nodes.
    pub has_mesh_nodes: bool,
}

impl WorkGraphD3d12 {
    /// Build from a schedule.
    ///
    /// # Errors
    ///   Returns `BackendMismatch` if the schedule was compiled for a
    ///   non-D3D12 backend.
    pub fn from_schedule(schedule: &Schedule) -> Result<Self> {
        if schedule.backend() != Backend::D3d12WorkGraph {
            return Err(WorkGraphError::BackendMismatch {
                compiled: schedule.backend().tag().to_owned(),
                live: Backend::D3d12WorkGraph.tag().to_owned(),
            });
        }
        let program_name = schedule
            .label()
            .map_or_else(|| "cssl-work-graph".to_owned(), str::to_owned);
        if program_name.len() > 64 {
            return Err(WorkGraphError::invalid_args(
                program_name.as_str(),
                "ProgramName must be ≤ 64 chars",
            ));
        }
        let mut entries: Vec<WorkGraphEntry> = Vec::with_capacity(schedule.len());
        let mut has_mesh = false;
        for node in schedule.iter_in_order() {
            // Consumer nodes do not produce a state-object entry-point ; they
            // are dataflow-only sinks.
            if matches!(node.kind, NodeKind::Consumer) {
                continue;
            }
            let is_mesh = matches!(node.kind, NodeKind::Mesh);
            has_mesh = has_mesh || is_mesh;
            entries.push(WorkGraphEntry {
                entry_point: node.id.as_str().to_owned(),
                dispatch: node.dispatch,
                is_mesh,
                shader_tag: node.shader_tag.clone(),
                est_cost_us: node.est_cost_us,
            });
        }
        Ok(Self {
            program_name,
            entries,
            est_cost_us: schedule.est_cost_us(),
            has_mesh_nodes: has_mesh,
        })
    }

    /// Number of entry points.
    #[must_use]
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    /// Iterate entry points.
    pub fn iter_entries(&self) -> std::slice::Iter<'_, WorkGraphEntry> {
        self.entries.iter()
    }

    /// Number of mesh-node entries.
    #[must_use]
    pub fn mesh_entry_count(&self) -> usize {
        self.entries.iter().filter(|e| e.is_mesh).count()
    }

    /// Number of compute-node entries.
    #[must_use]
    pub fn compute_entry_count(&self) -> usize {
        self.entries.iter().filter(|e| !e.is_mesh).count()
    }

    /// Returns the worst-case backing-memory size (bytes) for the work-graph
    /// per `ID3D12WorkGraphProperties1::GetWorkGraphMemoryRequirements`.
    ///
    /// This is a conservative estimate ; the runtime will query the real
    /// value from the driver and over-allocate if needed.
    #[must_use]
    pub fn estimated_backing_memory_bytes(&self) -> u64 {
        // 64 KB per entry as a conservative scratch budget. NV / AMD drivers
        // typically ask for less ; this is upper-bound for budgeting.
        (self.entries.len() as u64) * 64 * 1024
    }
}

#[cfg(test)]
mod tests {
    use super::WorkGraphD3d12;
    use crate::backend::{Backend, FeatureMatrix};
    use crate::dispatch::DispatchArgs;
    use crate::node::WorkGraphNode;
    use crate::stage_layout::StageId;
    use crate::WorkGraphBuilder;

    fn build_d3d12_schedule(label: &str) -> super::Schedule {
        WorkGraphBuilder::new()
            .with_label(label)
            .auto_select(FeatureMatrix::ultimate())
            .node(
                WorkGraphNode::compute(
                    "WaveSolver",
                    StageId::WaveSolver,
                    DispatchArgs::new(8, 8, 1),
                )
                .with_shader_tag("wave_solver_dxil")
                .with_cost_us(1_500),
            )
            .unwrap()
            .node(
                WorkGraphNode::compute(
                    "SDFRaymarch",
                    StageId::SdfRaymarch,
                    DispatchArgs::new(64, 64, 1),
                )
                .with_input("WaveSolver")
                .with_shader_tag("sdf_raymarch_dxil")
                .with_cost_us(2_500),
            )
            .unwrap()
            .build()
            .unwrap()
    }

    #[test]
    fn from_schedule_carries_label_as_program_name() {
        let s = build_d3d12_schedule("frame-graph");
        let wg = WorkGraphD3d12::from_schedule(&s).unwrap();
        assert_eq!(wg.program_name, "frame-graph");
    }

    #[test]
    fn from_schedule_emits_one_entry_per_compute_node() {
        let s = build_d3d12_schedule("g");
        let wg = WorkGraphD3d12::from_schedule(&s).unwrap();
        assert_eq!(wg.entry_count(), 2);
    }

    #[test]
    fn from_schedule_refuses_non_dx12_backend() {
        let s = WorkGraphBuilder::new()
            .auto_select(FeatureMatrix::dgc_only())
            .node(WorkGraphNode::compute(
                "X",
                StageId::WaveSolver,
                DispatchArgs::new(1, 1, 1),
            ))
            .unwrap()
            .build()
            .unwrap();
        let r = WorkGraphD3d12::from_schedule(&s);
        assert!(matches!(
            r,
            Err(crate::error::WorkGraphError::BackendMismatch { .. })
        ));
    }

    #[test]
    fn from_schedule_refuses_program_name_over_64_chars() {
        let long = "x".repeat(80);
        let s = WorkGraphBuilder::new()
            .with_label(long)
            .auto_select(FeatureMatrix::ultimate())
            .node(WorkGraphNode::compute(
                "A",
                StageId::WaveSolver,
                DispatchArgs::new(1, 1, 1),
            ))
            .unwrap()
            .build()
            .unwrap();
        let r = WorkGraphD3d12::from_schedule(&s);
        assert!(matches!(
            r,
            Err(crate::error::WorkGraphError::InvalidArgs { .. })
        ));
    }

    #[test]
    fn mesh_entry_count_zero_for_pure_compute() {
        let s = build_d3d12_schedule("g");
        let wg = WorkGraphD3d12::from_schedule(&s).unwrap();
        assert_eq!(wg.mesh_entry_count(), 0);
        assert_eq!(wg.compute_entry_count(), 2);
        assert!(!wg.has_mesh_nodes);
    }

    #[test]
    fn mesh_entry_count_when_mesh_node_present() {
        let s = WorkGraphBuilder::new()
            .auto_select(FeatureMatrix::ultimate())
            .node(WorkGraphNode::mesh(
                "MeshN",
                StageId::SdfRaymarch,
                DispatchArgs::new(8, 8, 1),
            ))
            .unwrap()
            .build()
            .unwrap();
        let wg = WorkGraphD3d12::from_schedule(&s).unwrap();
        assert_eq!(wg.mesh_entry_count(), 1);
        assert!(wg.has_mesh_nodes);
    }

    #[test]
    fn estimated_memory_scales_with_entry_count() {
        let s = build_d3d12_schedule("g");
        let wg = WorkGraphD3d12::from_schedule(&s).unwrap();
        let m = wg.estimated_backing_memory_bytes();
        assert!(m >= 64 * 1024);
    }

    #[test]
    fn iter_entries_visits_all_in_order() {
        let s = build_d3d12_schedule("g");
        let wg = WorkGraphD3d12::from_schedule(&s).unwrap();
        let names: Vec<_> = wg.iter_entries().map(|e| e.entry_point.clone()).collect();
        assert_eq!(
            names,
            vec!["WaveSolver".to_string(), "SDFRaymarch".to_string()]
        );
    }

    #[test]
    fn shader_tag_carried_through_to_entry() {
        let s = build_d3d12_schedule("g");
        let wg = WorkGraphD3d12::from_schedule(&s).unwrap();
        assert!(wg
            .iter_entries()
            .all(|e| e.shader_tag.starts_with("wave_solver")
                || e.shader_tag.starts_with("sdf_raymarch")));
    }

    #[test]
    fn backend_must_match() {
        // Building a schedule against the indirect-fallback backend then
        // trying to lower to work-graphs is a mismatch.
        let s = WorkGraphBuilder::new()
            .auto_select(FeatureMatrix::none())
            .node(WorkGraphNode::compute(
                "X",
                StageId::WaveSolver,
                DispatchArgs::new(1, 1, 1),
            ))
            .unwrap()
            .build()
            .unwrap();
        assert_eq!(s.backend(), Backend::IndirectFallback);
        assert!(WorkGraphD3d12::from_schedule(&s).is_err());
    }
}
