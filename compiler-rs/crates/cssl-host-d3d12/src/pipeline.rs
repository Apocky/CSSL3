//! § W-H2 (T11-D259) — D3D12 pipeline-state object loader (DXIL bytecode path).
//!
//! § PURPOSE
//!   `cssl-cgen-gpu-dxil` (W-Y) emits DXIL containers ; this module accepts
//!   those byte-buffers and produces an `ID3D12PipelineState`-equivalent
//!   handle. Stage-0 ships descriptor + container-validation surface ; the
//!   real `ID3D12Device::CreateGraphicsPipelineState` / CreateComputePSO
//!   call lives in `pso.rs` (windows-rs) and is mirrored in own-FFI here
//!   for the zero-deps build.
//!
//! § DXIL CONTAINER
//!   `DXBC` magic = `'DXBC'` (LE: `0x42435844`). Microsoft's DXIL signed
//!   containers prefix the bytecode with this header + a 16-byte hash,
//!   followed by `DxilContainer` chunks. We do not parse the chunks here —
//!   we only verify the magic so callers can fail fast on truncated /
//!   wrong-format buffers before paying the FFI round-trip.

use crate::error::{D3d12Error, Result};
use crate::ffi::ComPtr;

// ─── DXIL bytecode ────────────────────────────────────────────────────────

/// `DXBC` four-CC = `'DXBC'` little-endian.
pub const DXBC_MAGIC: u32 = u32::from_le_bytes([b'D', b'X', b'B', b'C']);

/// Owned DXIL bytecode buffer. Intentionally `Vec<u8>` (not `Box<[u8]>`)
/// so the caller can incrementally build it from the codegen pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DxilBytecode {
    bytes: Vec<u8>,
}

impl DxilBytecode {
    /// Wrap a buffer ; validates the `DXBC` four-CC.
    ///
    /// # Errors
    /// `D3d12Error::InvalidArgument` if the buffer is < 4 bytes or the
    /// four-CC mismatches.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self> {
        if bytes.len() < 4 {
            return Err(D3d12Error::invalid(
                "DxilBytecode::from_bytes",
                format!("buffer length {} < 4 (no DXBC magic possible)", bytes.len()),
            ));
        }
        let magic = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        if magic != DXBC_MAGIC {
            return Err(D3d12Error::invalid(
                "DxilBytecode::from_bytes",
                format!("DXBC magic mismatch: got 0x{magic:08x}, want 0x{DXBC_MAGIC:08x}"),
            ));
        }
        Ok(Self { bytes })
    }

    /// Construct without magic validation. Useful for fixture-injection in
    /// tests where the goal is to exercise a downstream branch and the
    /// container-validation is not the subject under test.
    #[must_use]
    pub fn from_bytes_unchecked(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }

    /// Raw byte view.
    #[must_use]
    pub fn as_slice(&self) -> &[u8] {
        &self.bytes
    }

    /// Byte length.
    #[must_use]
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// Empty predicate.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Container-magic getter (call site might already have validated and
    /// want to confirm).
    #[must_use]
    pub fn magic(&self) -> Option<u32> {
        if self.bytes.len() < 4 {
            return None;
        }
        Some(u32::from_le_bytes([
            self.bytes[0],
            self.bytes[1],
            self.bytes[2],
            self.bytes[3],
        ]))
    }
}

// ─── pipeline kind ────────────────────────────────────────────────────────

/// Pipeline shape — graphics (raster) vs compute. Stage-0 surface ; mesh /
/// raytracing / work-graph PSOs are extension variants in `work_graph.rs` +
/// `pso.rs` (windows-rs path).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PipelineKind {
    /// `ID3D12PipelineState` from `D3D12_GRAPHICS_PIPELINE_STATE_DESC`.
    Graphics,
    /// `ID3D12PipelineState` from `D3D12_COMPUTE_PIPELINE_STATE_DESC`.
    Compute,
}

/// Compute-pipeline descriptor (own-FFI side ; windows-rs equivalent is
/// `pso::ComputePsoDesc`).
#[derive(Debug, Clone)]
pub struct ComputePipelineDesc {
    /// Compute-shader DXIL.
    pub cs: DxilBytecode,
    /// Root-signature reference (table index ; resolved on PSO create).
    pub root_signature_index: u32,
    /// `NodeMask` (multi-GPU).
    pub node_mask: u32,
}

impl ComputePipelineDesc {
    /// Validate : DXIL non-empty, magic OK, root-signature in-range.
    ///
    /// # Errors
    /// `D3d12Error::InvalidArgument` for any failed check.
    pub fn validate(&self, root_signature_table_len: u32) -> Result<()> {
        if self.cs.is_empty() {
            return Err(D3d12Error::invalid(
                "ComputePipelineDesc",
                "empty compute-shader DXIL",
            ));
        }
        if !matches!(self.cs.magic(), Some(DXBC_MAGIC)) {
            return Err(D3d12Error::invalid(
                "ComputePipelineDesc",
                "compute-shader bytecode missing DXBC magic",
            ));
        }
        if self.root_signature_index >= root_signature_table_len {
            return Err(D3d12Error::invalid(
                "ComputePipelineDesc",
                format!(
                    "root_signature_index {} ≥ table-len {}",
                    self.root_signature_index, root_signature_table_len
                ),
            ));
        }
        Ok(())
    }
}

/// Graphics-pipeline descriptor — minimal subset (vs/ps + render-target +
/// depth-stencil format). Full 70+-field `D3D12_GRAPHICS_PIPELINE_STATE_DESC`
/// lives in `pso.rs` (windows-rs path) ; this surface covers the substrate-
/// renderer's stage-0 raster needs.
#[derive(Debug, Clone)]
pub struct GraphicsPipelineDesc {
    /// Vertex-shader DXIL.
    pub vs: DxilBytecode,
    /// Pixel-shader DXIL.
    pub ps: DxilBytecode,
    /// Root-signature reference.
    pub root_signature_index: u32,
    /// Render-target format (raw `DXGI_FORMAT`).
    pub rtv_format: u32,
    /// Depth-stencil format (raw `DXGI_FORMAT` ; 0 = no depth).
    pub dsv_format: u32,
    /// Sample count (MSAA).
    pub sample_count: u32,
    /// `NodeMask` (multi-GPU).
    pub node_mask: u32,
}

impl GraphicsPipelineDesc {
    /// Validate VS+PS magic + root-sig + sample-count.
    ///
    /// # Errors
    /// `D3d12Error::InvalidArgument` for any failed check.
    pub fn validate(&self, root_signature_table_len: u32) -> Result<()> {
        for (label, stage) in [("VS", &self.vs), ("PS", &self.ps)] {
            if stage.is_empty() {
                return Err(D3d12Error::invalid(
                    "GraphicsPipelineDesc",
                    format!("empty {label} bytecode"),
                ));
            }
            if !matches!(stage.magic(), Some(DXBC_MAGIC)) {
                return Err(D3d12Error::invalid(
                    "GraphicsPipelineDesc",
                    format!("{label} bytecode missing DXBC magic"),
                ));
            }
        }
        if self.root_signature_index >= root_signature_table_len {
            return Err(D3d12Error::invalid(
                "GraphicsPipelineDesc",
                format!(
                    "root_signature_index {} ≥ table-len {}",
                    self.root_signature_index, root_signature_table_len
                ),
            ));
        }
        if !matches!(self.sample_count, 1 | 2 | 4 | 8 | 16) {
            return Err(D3d12Error::invalid(
                "GraphicsPipelineDesc",
                format!("sample_count {} not in {{1,2,4,8,16}}", self.sample_count),
            ));
        }
        Ok(())
    }
}

/// Opaque pipeline-state handle. Real-FFI : a `ComPtr` to
/// `ID3D12PipelineState`. Mock : carries a stable index + the validated
/// descriptor kind so tests can route the binding logic without GPU work.
#[derive(Debug)]
pub struct PipelineHandle {
    /// Real-FFI : `ID3D12PipelineState` COM pointer ; `null` in mock mode.
    /// Stage-0 : not yet dispatched-through ; reserved for the
    /// `create_*_pipeline_real` wire-through (reserved-storage, see
    /// `create_compute_pipeline_real` doc-comment).
    #[allow(dead_code)]
    inner: ComPtr,
    kind: PipelineKind,
    mock_index: u32,
    is_mock: bool,
}

impl PipelineHandle {
    /// Pipeline shape (graphics or compute).
    #[must_use]
    pub const fn kind(&self) -> PipelineKind {
        self.kind
    }

    /// Mock-handle index (deterministic ; useful in record-buffer tests).
    #[must_use]
    pub const fn mock_index(&self) -> u32 {
        self.mock_index
    }

    /// Is this a mock handle ?
    #[must_use]
    pub const fn is_mock(&self) -> bool {
        self.is_mock
    }
}

/// Create a compute-pipeline mock-handle.
///
/// # Errors
/// Whatever `ComputePipelineDesc::validate` returns.
pub fn create_compute_pipeline_mock(
    desc: &ComputePipelineDesc,
    root_signature_table_len: u32,
    mock_index: u32,
) -> Result<PipelineHandle> {
    desc.validate(root_signature_table_len)?;
    Ok(PipelineHandle {
        inner: ComPtr::null(),
        kind: PipelineKind::Compute,
        mock_index,
        is_mock: true,
    })
}

/// Create a graphics-pipeline mock-handle.
///
/// # Errors
/// Whatever `GraphicsPipelineDesc::validate` returns.
pub fn create_graphics_pipeline_mock(
    desc: &GraphicsPipelineDesc,
    root_signature_table_len: u32,
    mock_index: u32,
) -> Result<PipelineHandle> {
    desc.validate(root_signature_table_len)?;
    Ok(PipelineHandle {
        inner: ComPtr::null(),
        kind: PipelineKind::Graphics,
        mock_index,
        is_mock: true,
    })
}

/// Real-FFI compute-pipeline create stub. Stage-0 returns `LoaderMissing` ;
/// the actual call lives in `pso.rs` (windows-rs path) until the own-FFI
/// device-vtable dispatch is wired.
///
/// # Errors
/// `D3d12Error::LoaderMissing` always (in stage-0).
pub fn create_compute_pipeline_real(
    _device: ComPtr,
    _desc: &ComputePipelineDesc,
) -> Result<PipelineHandle> {
    Err(D3d12Error::loader(
        "create_compute_pipeline_real : own-FFI ID3D12Device::CreateComputePipelineState deferred to host_gpu wire-up",
    ))
}

// ─── Helpers : DXIL fixture for tests ─────────────────────────────────────

/// Synthesize a minimal-but-valid (magic-only) DXIL container for fixtures.
/// Real DXIL has a 124-byte header ; this fixture is the smallest buffer
/// that passes `DxilBytecode::from_bytes`.
#[must_use]
pub fn synth_dxil_fixture(payload_len: usize) -> Vec<u8> {
    let mut v = Vec::with_capacity(4 + payload_len);
    v.extend_from_slice(&DXBC_MAGIC.to_le_bytes());
    v.resize(4 + payload_len, 0);
    v
}

// ─── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dxbc_magic_constant_correct() {
        assert_eq!(DXBC_MAGIC, 0x4342_5844);
    }

    #[test]
    fn from_bytes_rejects_short_buffer() {
        let r = DxilBytecode::from_bytes(vec![1, 2, 3]);
        assert!(matches!(r, Err(D3d12Error::InvalidArgument { .. })));
    }

    #[test]
    fn from_bytes_rejects_wrong_magic() {
        let r = DxilBytecode::from_bytes(vec![b'M', b'Z', 0, 0]);
        assert!(matches!(r, Err(D3d12Error::InvalidArgument { .. })));
    }

    #[test]
    fn from_bytes_accepts_valid_magic() {
        let v = synth_dxil_fixture(16);
        let b = DxilBytecode::from_bytes(v).unwrap();
        assert_eq!(b.magic(), Some(DXBC_MAGIC));
        assert_eq!(b.len(), 20);
    }

    #[test]
    fn unchecked_constructor_skips_magic_check() {
        let b = DxilBytecode::from_bytes_unchecked(vec![0, 0, 0, 0]);
        assert_eq!(b.len(), 4);
        assert_eq!(b.magic(), Some(0));
    }

    #[test]
    fn synth_fixture_round_trips() {
        let v = synth_dxil_fixture(32);
        assert_eq!(v.len(), 36);
        let b = DxilBytecode::from_bytes(v).unwrap();
        assert_eq!(b.magic(), Some(DXBC_MAGIC));
    }

    #[test]
    fn compute_desc_validate_passes_on_good_input() {
        let cs = DxilBytecode::from_bytes(synth_dxil_fixture(8)).unwrap();
        let d = ComputePipelineDesc {
            cs,
            root_signature_index: 0,
            node_mask: 0,
        };
        d.validate(1).unwrap();
    }

    #[test]
    fn compute_desc_rejects_root_index_oob() {
        let cs = DxilBytecode::from_bytes(synth_dxil_fixture(8)).unwrap();
        let d = ComputePipelineDesc {
            cs,
            root_signature_index: 5,
            node_mask: 0,
        };
        assert!(d.validate(1).is_err());
    }

    #[test]
    fn compute_desc_rejects_empty_dxil() {
        let cs = DxilBytecode::from_bytes_unchecked(vec![]);
        let d = ComputePipelineDesc {
            cs,
            root_signature_index: 0,
            node_mask: 0,
        };
        assert!(d.validate(1).is_err());
    }

    #[test]
    fn graphics_desc_validate_passes_on_good_input() {
        let vs = DxilBytecode::from_bytes(synth_dxil_fixture(8)).unwrap();
        let ps = DxilBytecode::from_bytes(synth_dxil_fixture(8)).unwrap();
        let d = GraphicsPipelineDesc {
            vs,
            ps,
            root_signature_index: 0,
            rtv_format: 28,
            dsv_format: 0,
            sample_count: 1,
            node_mask: 0,
        };
        d.validate(1).unwrap();
    }

    #[test]
    fn graphics_desc_rejects_bad_sample_count() {
        let vs = DxilBytecode::from_bytes(synth_dxil_fixture(8)).unwrap();
        let ps = DxilBytecode::from_bytes(synth_dxil_fixture(8)).unwrap();
        let d = GraphicsPipelineDesc {
            vs,
            ps,
            root_signature_index: 0,
            rtv_format: 28,
            dsv_format: 0,
            sample_count: 3,
            node_mask: 0,
        };
        assert!(d.validate(1).is_err());
    }

    #[test]
    fn graphics_desc_rejects_missing_magic_via_unchecked() {
        let vs = DxilBytecode::from_bytes(synth_dxil_fixture(8)).unwrap();
        let ps = DxilBytecode::from_bytes_unchecked(vec![0, 0, 0, 0]);
        let d = GraphicsPipelineDesc {
            vs,
            ps,
            root_signature_index: 0,
            rtv_format: 28,
            dsv_format: 0,
            sample_count: 1,
            node_mask: 0,
        };
        assert!(d.validate(1).is_err());
    }

    #[test]
    fn create_compute_pipeline_mock_returns_handle() {
        let cs = DxilBytecode::from_bytes(synth_dxil_fixture(8)).unwrap();
        let d = ComputePipelineDesc {
            cs,
            root_signature_index: 0,
            node_mask: 0,
        };
        let h = create_compute_pipeline_mock(&d, 1, 9001).unwrap();
        assert!(h.is_mock());
        assert_eq!(h.mock_index(), 9001);
        assert!(matches!(h.kind(), PipelineKind::Compute));
    }

    #[test]
    fn create_graphics_pipeline_mock_returns_handle() {
        let vs = DxilBytecode::from_bytes(synth_dxil_fixture(8)).unwrap();
        let ps = DxilBytecode::from_bytes(synth_dxil_fixture(8)).unwrap();
        let d = GraphicsPipelineDesc {
            vs,
            ps,
            root_signature_index: 0,
            rtv_format: 28,
            dsv_format: 0,
            sample_count: 1,
            node_mask: 0,
        };
        let h = create_graphics_pipeline_mock(&d, 1, 42).unwrap();
        assert!(h.is_mock());
        assert!(matches!(h.kind(), PipelineKind::Graphics));
    }

    #[test]
    fn create_compute_pipeline_real_is_loader_missing_in_stage0() {
        let cs = DxilBytecode::from_bytes(synth_dxil_fixture(8)).unwrap();
        let d = ComputePipelineDesc {
            cs,
            root_signature_index: 0,
            node_mask: 0,
        };
        let r = create_compute_pipeline_real(ComPtr::null(), &d);
        assert!(matches!(r, Err(D3d12Error::LoaderMissing { .. })));
    }
}
