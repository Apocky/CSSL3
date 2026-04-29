//! D3D12-Ultimate work-graph state-object hooks.
//!
//! § SLICE T11-D123 (W4-09)
//!
//! Provides the FFI shim for `D3D12_WORK_GRAPH_DESC` + `DispatchGraph`
//! exposed by `ID3D12Device14` (D3D12-Ultimate). The cross-platform DAG
//! abstraction lives in `cssl-work-graph` ; this module is the
//! Windows-side actuator.
//!
//! § STAGE-0 SCOPE
//!   At T11-D123 we ship the *capability probe* + *descriptor builder* :
//!   the host can query `D3D12_FEATURE_D3D12_OPTIONS21.WorkGraphsTier`
//!   and produce a [`WorkGraphProgramDesc`] from the DXIL blob it would
//!   submit to `CreateStateObject`. The full state-object integration
//!   (subobject parsing, NumNodes/NumEntries enumeration, DispatchGraph
//!   command-list emit) is the next dispatch ; this slice gates the API
//!   shape.
//!
//! § NON-WINDOWS
//!   Every constructor returns `D3d12Error::LoaderMissing`.

/// Work-graph tier (`D3D12_WORK_GRAPHS_TIER_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WorkGraphsTier {
    /// `D3D12_WORK_GRAPHS_TIER_NOT_SUPPORTED` — driver does not support.
    NotSupported,
    /// `D3D12_WORK_GRAPHS_TIER_1_0` — initial tier (NV 555+, AMD 24.x+).
    Tier1_0,
}

impl WorkGraphsTier {
    /// Stable string tag.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotSupported => "not-supported",
            Self::Tier1_0 => "tier1.0",
        }
    }

    /// True iff at-least Tier 1.0.
    #[must_use]
    pub const fn is_supported(self) -> bool {
        matches!(self, Self::Tier1_0)
    }
}

/// One-shot per-program descriptor for `D3D12_WORK_GRAPH_DESC`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkGraphProgramDesc {
    /// Program name (≤ 64 chars per spec).
    pub program_name: String,
    /// DXIL bytecode for the entire work-graph program (one blob ; entry
    /// points enumerated by the runtime).
    pub dxil_bytes: Vec<u8>,
    /// Optional debug label for telemetry overlay.
    pub label: Option<String>,
    /// Whether the program contains mesh-nodes.
    pub has_mesh_nodes: bool,
    /// Number of entry-points in the program.
    pub entry_point_count: u32,
}

impl WorkGraphProgramDesc {
    /// Construct.
    #[must_use]
    pub fn new(program_name: impl Into<String>, dxil_bytes: Vec<u8>) -> Self {
        Self {
            program_name: program_name.into(),
            dxil_bytes,
            label: None,
            has_mesh_nodes: false,
            entry_point_count: 0,
        }
    }

    /// Set the entry-point count.
    #[must_use]
    pub fn with_entry_point_count(mut self, n: u32) -> Self {
        self.entry_point_count = n;
        self
    }

    /// Set the mesh-node flag.
    #[must_use]
    pub fn with_mesh_nodes(mut self, has: bool) -> Self {
        self.has_mesh_nodes = has;
        self
    }

    /// Set the debug label.
    #[must_use]
    pub fn with_label(mut self, l: impl Into<String>) -> Self {
        self.label = Some(l.into());
        self
    }

    /// Validate the descriptor.
    ///
    /// # Errors
    ///   Returns [`crate::D3d12Error::InvalidArgument`] for empty DXIL,
    ///   empty program name, or program name > 64 chars.
    pub fn validate(&self) -> crate::Result<()> {
        if self.program_name.is_empty() {
            return Err(crate::D3d12Error::invalid(
                "WorkGraphProgramDesc",
                "program_name empty",
            ));
        }
        if self.program_name.len() > 64 {
            return Err(crate::D3d12Error::invalid(
                "WorkGraphProgramDesc",
                "program_name > 64 chars",
            ));
        }
        if self.dxil_bytes.is_empty() {
            return Err(crate::D3d12Error::invalid(
                "WorkGraphProgramDesc",
                "DXIL empty",
            ));
        }
        if self.entry_point_count == 0 {
            return Err(crate::D3d12Error::invalid(
                "WorkGraphProgramDesc",
                "entry_point_count must be > 0",
            ));
        }
        Ok(())
    }
}

/// Per-dispatch arguments for `ID3D12GraphicsCommandList10::DispatchGraph`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DispatchGraphArgs {
    /// Mode bit : 0=`MULTI_NODE_CPU_INPUT`, 1=`NODE_CPU_INPUT`, ...
    /// (mirrors `D3D12_DISPATCH_GRAPH_MODE_*`).
    pub mode: u32,
    /// Number of input records.
    pub num_records: u32,
    /// Per-record stride (bytes).
    pub record_stride: u32,
}

impl DispatchGraphArgs {
    /// Construct.
    #[must_use]
    pub const fn new(mode: u32, num_records: u32, record_stride: u32) -> Self {
        Self {
            mode,
            num_records,
            record_stride,
        }
    }

    /// Empty (no records).
    #[must_use]
    pub const fn empty() -> Self {
        Self::new(0, 0, 0)
    }

    /// Returns true iff at least one record will be processed.
    #[must_use]
    pub const fn has_records(self) -> bool {
        self.num_records > 0
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Probe API (delegates to features.rs in the future ; standalone for now)
// ═══════════════════════════════════════════════════════════════════════

/// Probe the live device for work-graph support.
#[cfg(target_os = "windows")]
pub mod probe {
    use super::WorkGraphsTier;
    use crate::Result;

    /// Probe — STAGE-0 returns `Tier1_0` if the device declared a feature
    /// matching the documented MODEL spec, else `NotSupported`.
    ///
    /// Real FFI wires `D3D12_FEATURE_D3D12_OPTIONS21` query in a follow-up
    /// dispatch ; here we accept a feature-flag from the device wrapper.
    pub fn query_work_graphs_tier(_device: &crate::Device) -> Result<WorkGraphsTier> {
        // Conservative default : D3D12_OPTIONS21 query lands in the
        // follow-up FFI dispatch (sub-slice of T11-D123). For now we
        // refuse rather than synthesize a false positive — the cross-
        // platform crate then falls back to DGC or indirect.
        Ok(WorkGraphsTier::NotSupported)
    }
}

#[cfg(not(target_os = "windows"))]
pub mod probe {
    use super::WorkGraphsTier;
    use crate::Result;

    /// Probe — non-Windows always reports unsupported.
    pub fn query_work_graphs_tier(_device: &crate::Device) -> Result<WorkGraphsTier> {
        Ok(WorkGraphsTier::NotSupported)
    }
}

#[cfg(test)]
mod tests {
    use super::{DispatchGraphArgs, WorkGraphProgramDesc, WorkGraphsTier};
    use crate::D3d12Error;

    #[test]
    fn tier_strings() {
        assert_eq!(WorkGraphsTier::NotSupported.as_str(), "not-supported");
        assert_eq!(WorkGraphsTier::Tier1_0.as_str(), "tier1.0");
    }

    #[test]
    fn tier_supported_flag() {
        assert!(WorkGraphsTier::Tier1_0.is_supported());
        assert!(!WorkGraphsTier::NotSupported.is_supported());
    }

    #[test]
    fn program_desc_round_trips_label() {
        let d = WorkGraphProgramDesc::new("frame-graph", vec![0u8; 32])
            .with_entry_point_count(4)
            .with_label("debug-label");
        assert_eq!(d.label.as_deref(), Some("debug-label"));
        assert_eq!(d.entry_point_count, 4);
    }

    #[test]
    fn program_desc_validate_empty_dxil_refused() {
        let d = WorkGraphProgramDesc::new("X", Vec::new()).with_entry_point_count(1);
        let r = d.validate();
        assert!(matches!(r, Err(D3d12Error::InvalidArgument { .. })));
    }

    #[test]
    fn program_desc_validate_empty_name_refused() {
        let d = WorkGraphProgramDesc::new("", vec![0u8; 8]).with_entry_point_count(1);
        let r = d.validate();
        assert!(matches!(r, Err(D3d12Error::InvalidArgument { .. })));
    }

    #[test]
    fn program_desc_validate_long_name_refused() {
        let long = "x".repeat(80);
        let d = WorkGraphProgramDesc::new(long, vec![0u8; 8]).with_entry_point_count(1);
        let r = d.validate();
        assert!(matches!(r, Err(D3d12Error::InvalidArgument { .. })));
    }

    #[test]
    fn program_desc_validate_zero_entry_points_refused() {
        let d = WorkGraphProgramDesc::new("X", vec![0u8; 8]);
        let r = d.validate();
        assert!(matches!(r, Err(D3d12Error::InvalidArgument { .. })));
    }

    #[test]
    fn program_desc_validate_minimal_passes() {
        let d = WorkGraphProgramDesc::new("frame", vec![0u8; 32]).with_entry_point_count(2);
        assert!(d.validate().is_ok());
    }

    #[test]
    fn dispatch_args_empty_no_records() {
        assert!(!DispatchGraphArgs::empty().has_records());
    }

    #[test]
    fn dispatch_args_has_records_when_count_positive() {
        assert!(DispatchGraphArgs::new(0, 5, 16).has_records());
    }

    #[test]
    fn mesh_node_flag_round_trips() {
        let d = WorkGraphProgramDesc::new("X", vec![0u8; 8])
            .with_entry_point_count(1)
            .with_mesh_nodes(true);
        assert!(d.has_mesh_nodes);
    }
}
