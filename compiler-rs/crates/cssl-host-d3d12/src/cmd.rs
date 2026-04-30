//! § W-H2 (T11-D259) — D3D12 command-queue / command-list façade.
//!
//! § PURPOSE
//!   Unify the existing `queue.rs` (windows-rs path) + a thin own-FFI
//!   record/submit surface so the substrate-renderer's host_gpu trait can
//!   target either.
//!
//! § SCOPE
//!   - `CmdQueueDesc` / `CmdRecorder` — descriptor + record-buffer.
//!   - `record_*` helpers : push commands into a CSL-side opaque buffer.
//!     These are interpreted by the real submission path (windows-rs or
//!     own-FFI) when `submit()` is called.
//!   - `Submission` — handle returned from submit ; carries fence-value +
//!     wait-helpers.
//!
//! § DESIGN
//!   Stage-0 ships an in-memory IR-style command-buffer that is later
//!   translated to either `ID3D12GraphicsCommandList::*` (windows-rs path)
//!   or raw vtable calls (own-FFI path). This indirection lets us keep the
//!   unit tests deterministic + cross-platform — the actual GPU dispatch is
//!   a separate substrate-renderer concern.

use crate::error::{D3d12Error, Result};
use crate::ffi::{ComPtr, CommandListTypeRaw};
use crate::heap::CommandListType;

// ─── command IR ───────────────────────────────────────────────────────────

/// Opaque commands recorded into a `CmdRecorder`. Each variant maps 1:1
/// to a `ID3D12GraphicsCommandList` method ; this IR lets us validate
/// + diff command streams in tests without involving the GPU.
///
/// `Eq` is intentionally not derived : `ClearRenderTargetView { rgba: [f32;4] }`
/// holds floats which cannot satisfy `Eq` (NaN). `PartialEq` is sufficient
/// for the equality-checks the tests perform.
#[derive(Debug, Clone, PartialEq)]
pub enum CmdOp {
    /// `SetPipelineState(pso_index)`.
    SetPipelineState {
        /// Index into the recorder's PSO table.
        pso_index: u32,
    },
    /// `SetGraphicsRootSignature` / `SetComputeRootSignature`.
    SetRootSignature {
        /// Index into the recorder's root-signature table.
        rs_index: u32,
        /// Compute or graphics ?
        is_compute: bool,
    },
    /// `Dispatch(x, y, z)`.
    Dispatch {
        /// Threadgroup count X.
        x: u32,
        /// Threadgroup count Y.
        y: u32,
        /// Threadgroup count Z.
        z: u32,
    },
    /// `DrawInstanced(vertex_count, instance_count, start_vertex, start_instance)`.
    DrawInstanced {
        /// Vertex count per instance.
        vertex_count: u32,
        /// Instance count.
        instance_count: u32,
        /// Start-vertex.
        start_vertex: u32,
        /// Start-instance.
        start_instance: u32,
    },
    /// `DrawIndexedInstanced(index_count, instance_count, start_index, base_vertex, start_instance)`.
    DrawIndexedInstanced {
        /// Index count per instance.
        index_count: u32,
        /// Instance count.
        instance_count: u32,
        /// Start-index.
        start_index: u32,
        /// Base-vertex.
        base_vertex: i32,
        /// Start-instance.
        start_instance: u32,
    },
    /// `ResourceBarrier` — single transition.
    ResourceBarrier {
        /// Resource index in the recorder's table.
        resource_index: u32,
        /// Before-state (raw `D3D12_RESOURCE_STATES`).
        state_before: u32,
        /// After-state.
        state_after: u32,
    },
    /// `ClearRenderTargetView(rtv, rgba)`.
    ClearRenderTargetView {
        /// RTV descriptor-handle index.
        rtv_index: u32,
        /// Clear color.
        rgba: [f32; 4],
    },
    /// `CopyResource(dst, src)`.
    CopyResource {
        /// Dst resource index.
        dst_index: u32,
        /// Src resource index.
        src_index: u32,
    },
}

/// `D3D12_COMMAND_QUEUE_DESC` (CSSL-side mirror).
#[derive(Debug, Clone, Copy)]
pub struct CmdQueueDesc {
    /// `Type`.
    pub list_type: CommandListType,
    /// `Priority` (`D3D12_COMMAND_QUEUE_PRIORITY_*` raw value).
    pub priority: i32,
    /// `Flags` (raw `D3D12_COMMAND_QUEUE_FLAGS`).
    pub flags: u32,
    /// `NodeMask` (multi-GPU ; 0 for single-adapter).
    pub node_mask: u32,
}

impl CmdQueueDesc {
    /// Default direct-queue at NORMAL priority.
    #[must_use]
    pub const fn direct() -> Self {
        Self {
            list_type: CommandListType::Direct,
            priority: 0, // D3D12_COMMAND_QUEUE_PRIORITY_NORMAL
            flags: 0,
            node_mask: 0,
        }
    }

    /// Default async-compute queue.
    #[must_use]
    pub const fn compute() -> Self {
        Self {
            list_type: CommandListType::Compute,
            priority: 0,
            flags: 0,
            node_mask: 0,
        }
    }

    /// Default copy queue.
    #[must_use]
    pub const fn copy() -> Self {
        Self {
            list_type: CommandListType::Copy,
            priority: 0,
            flags: 0,
            node_mask: 0,
        }
    }

    /// Map the public `CommandListType` to the FFI raw enum.
    #[must_use]
    pub const fn list_type_raw(&self) -> CommandListTypeRaw {
        match self.list_type {
            CommandListType::Direct => CommandListTypeRaw::Direct,
            CommandListType::Bundle => CommandListTypeRaw::Bundle,
            CommandListType::Compute => CommandListTypeRaw::Compute,
            CommandListType::Copy => CommandListTypeRaw::Copy,
            // Video types fall back to Direct in the raw enum (D3D12 video
            // types are 4..=6 but are not currently FFI-modeled in our
            // own-FFI path — only the 4 core types are exercised by the
            // substrate-renderer in stage-0).
            CommandListType::VideoDecode
            | CommandListType::VideoProcess
            | CommandListType::VideoEncode => CommandListTypeRaw::Direct,
        }
    }
}

/// In-memory recorder. Buffer-grows the `CmdOp` vec ; `submit()` consumes it.
#[derive(Debug)]
pub struct CmdRecorder {
    desc: CmdQueueDesc,
    ops: Vec<CmdOp>,
    closed: bool,
}

impl CmdRecorder {
    /// Construct with a queue descriptor (governs which ops are valid).
    #[must_use]
    pub fn new(desc: CmdQueueDesc) -> Self {
        Self {
            desc,
            ops: Vec::new(),
            closed: false,
        }
    }

    /// Queue descriptor (cached).
    #[must_use]
    pub const fn desc(&self) -> &CmdQueueDesc {
        &self.desc
    }

    /// Recorded ops (read-only view).
    #[must_use]
    pub fn ops(&self) -> &[CmdOp] {
        &self.ops
    }

    /// Has `close()` been called ?
    #[must_use]
    pub const fn is_closed(&self) -> bool {
        self.closed
    }

    /// Op count (helper).
    #[must_use]
    pub fn len(&self) -> usize {
        self.ops.len()
    }

    /// Empty predicate (helper).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    /// Push a `CmdOp` ; rejects if already closed or if op-kind incompatible
    /// with the queue type.
    ///
    /// # Errors
    /// `D3d12Error::InvalidArgument` for closed-recorder or queue-mismatch.
    pub fn record(&mut self, op: CmdOp) -> Result<()> {
        if self.closed {
            return Err(D3d12Error::invalid(
                "CmdRecorder::record",
                "recorder already closed",
            ));
        }
        // Queue-type compatibility : copy queues only accept copy ops + barriers.
        if matches!(self.desc.list_type, CommandListType::Copy) {
            match op {
                CmdOp::CopyResource { .. } | CmdOp::ResourceBarrier { .. } => (),
                _ => {
                    return Err(D3d12Error::invalid(
                        "CmdRecorder::record",
                        "non-copy op pushed to copy queue",
                    ));
                }
            }
        }
        // Compute queues reject draw ops.
        if matches!(self.desc.list_type, CommandListType::Compute)
            && matches!(
                op,
                CmdOp::DrawInstanced { .. } | CmdOp::DrawIndexedInstanced { .. }
            )
        {
            return Err(D3d12Error::invalid(
                "CmdRecorder::record",
                "draw op pushed to compute queue",
            ));
        }
        self.ops.push(op);
        Ok(())
    }

    /// Helper : `SetPipelineState`.
    ///
    /// # Errors
    /// `D3d12Error::InvalidArgument` if the recorder is closed.
    pub fn record_set_pipeline_state(&mut self, pso_index: u32) -> Result<()> {
        self.record(CmdOp::SetPipelineState { pso_index })
    }

    /// Helper : `Dispatch`.
    ///
    /// # Errors
    /// `D3d12Error::InvalidArgument` for closed-recorder, draw-on-compute
    /// when applicable, or copy-queue rejection.
    pub fn record_dispatch(&mut self, x: u32, y: u32, z: u32) -> Result<()> {
        self.record(CmdOp::Dispatch { x, y, z })
    }

    /// Helper : `DrawIndexedInstanced`.
    ///
    /// # Errors
    /// `D3d12Error::InvalidArgument` for closed-recorder + queue-mismatch.
    pub fn record_draw_indexed(
        &mut self,
        index_count: u32,
        instance_count: u32,
    ) -> Result<()> {
        self.record(CmdOp::DrawIndexedInstanced {
            index_count,
            instance_count,
            start_index: 0,
            base_vertex: 0,
            start_instance: 0,
        })
    }

    /// Helper : `ResourceBarrier` (transition).
    ///
    /// # Errors
    /// `D3d12Error::InvalidArgument` if the recorder is closed.
    pub fn record_resource_barrier(
        &mut self,
        resource_index: u32,
        state_before: u32,
        state_after: u32,
    ) -> Result<()> {
        self.record(CmdOp::ResourceBarrier {
            resource_index,
            state_before,
            state_after,
        })
    }

    /// `ID3D12GraphicsCommandList::Close` analog ; no further ops accepted.
    ///
    /// # Errors
    /// `D3d12Error::InvalidArgument` when called twice.
    pub fn close(&mut self) -> Result<()> {
        if self.closed {
            return Err(D3d12Error::invalid("CmdRecorder::close", "double close"));
        }
        self.closed = true;
        Ok(())
    }
}

/// `ID3D12CommandQueue::ExecuteCommandLists` result handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Submission {
    /// Fence value the queue will signal when this submission completes.
    pub fence_value: u64,
    /// Op count submitted (parity check vs recorder).
    pub op_count: usize,
    /// Queue type the ops landed on.
    pub list_type: CommandListType,
}

/// Mock submission helper : "submits" the recorder by closing it (if open),
/// returning a deterministic fence value derived from a monotonic counter.
///
/// # Errors
/// `D3d12Error::InvalidArgument` when the recorder is empty (D3D12
/// rejects ExecuteCommandLists with `NumLists=0`).
pub fn submit_mock(rec: &mut CmdRecorder, next_fence_value: u64) -> Result<Submission> {
    if rec.is_empty() {
        return Err(D3d12Error::invalid(
            "submit_mock",
            "empty command list — D3D12 rejects ExecuteCommandLists(NumLists=0)",
        ));
    }
    if !rec.closed {
        rec.close()?;
    }
    Ok(Submission {
        fence_value: next_fence_value,
        op_count: rec.ops.len(),
        list_type: rec.desc.list_type,
    })
}

/// Real-FFI submission stub. Stage-0 returns `LoaderMissing` ; the actual
/// vtable dispatch lives in the substrate-renderer once the queue COM-ptr
/// is wired through the host_gpu trait.
///
/// # Errors
/// `D3d12Error::LoaderMissing` always (in stage-0).
pub fn submit_real(_queue: ComPtr, _rec: &mut CmdRecorder) -> Result<Submission> {
    Err(D3d12Error::loader(
        "submit_real : windows-rs path is in queue.rs ; own-FFI submit deferred to host_gpu wire-up",
    ))
}

// ─── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_descriptor_defaults() {
        let d = CmdQueueDesc::direct();
        assert!(matches!(d.list_type, CommandListType::Direct));
        assert_eq!(d.priority, 0);
        assert_eq!(d.flags, 0);
        assert_eq!(d.node_mask, 0);
    }

    #[test]
    fn list_type_raw_round_trip() {
        assert_eq!(
            CmdQueueDesc::direct().list_type_raw() as i32,
            CommandListTypeRaw::Direct as i32
        );
        assert_eq!(
            CmdQueueDesc::compute().list_type_raw() as i32,
            CommandListTypeRaw::Compute as i32
        );
        assert_eq!(
            CmdQueueDesc::copy().list_type_raw() as i32,
            CommandListTypeRaw::Copy as i32
        );
    }

    #[test]
    fn record_basic_dispatch() {
        let mut r = CmdRecorder::new(CmdQueueDesc::compute());
        r.record_set_pipeline_state(7).unwrap();
        r.record_dispatch(64, 1, 1).unwrap();
        r.close().unwrap();
        assert!(r.is_closed());
        assert_eq!(r.len(), 2);
    }

    #[test]
    fn record_after_close_rejected() {
        let mut r = CmdRecorder::new(CmdQueueDesc::direct());
        r.close().unwrap();
        assert!(r.record_dispatch(1, 1, 1).is_err());
    }

    #[test]
    fn double_close_rejected() {
        let mut r = CmdRecorder::new(CmdQueueDesc::direct());
        r.close().unwrap();
        assert!(r.close().is_err());
    }

    #[test]
    fn copy_queue_rejects_dispatch() {
        let mut r = CmdRecorder::new(CmdQueueDesc::copy());
        let e = r.record_dispatch(1, 1, 1);
        assert!(matches!(e, Err(D3d12Error::InvalidArgument { .. })));
    }

    #[test]
    fn copy_queue_accepts_copy_resource() {
        let mut r = CmdRecorder::new(CmdQueueDesc::copy());
        r.record(CmdOp::CopyResource {
            dst_index: 1,
            src_index: 2,
        })
        .unwrap();
        assert_eq!(r.len(), 1);
    }

    #[test]
    fn copy_queue_accepts_resource_barrier() {
        let mut r = CmdRecorder::new(CmdQueueDesc::copy());
        r.record_resource_barrier(0, 0x1, 0x2).unwrap();
    }

    #[test]
    fn compute_queue_rejects_draw() {
        let mut r = CmdRecorder::new(CmdQueueDesc::compute());
        let e = r.record_draw_indexed(36, 1);
        assert!(matches!(e, Err(D3d12Error::InvalidArgument { .. })));
    }

    #[test]
    fn submit_mock_rejects_empty() {
        let mut r = CmdRecorder::new(CmdQueueDesc::direct());
        assert!(submit_mock(&mut r, 1).is_err());
    }

    #[test]
    fn submit_mock_returns_fence_value_and_op_count() {
        let mut r = CmdRecorder::new(CmdQueueDesc::direct());
        r.record_draw_indexed(36, 1).unwrap();
        r.record_resource_barrier(0, 0x4, 0x200).unwrap();
        let s = submit_mock(&mut r, 42).unwrap();
        assert_eq!(s.fence_value, 42);
        assert_eq!(s.op_count, 2);
        assert!(matches!(s.list_type, CommandListType::Direct));
        assert!(r.is_closed());
    }

    #[test]
    fn submit_real_returns_loader_missing_in_stage0() {
        let mut r = CmdRecorder::new(CmdQueueDesc::direct());
        r.record_dispatch(1, 1, 1).unwrap();
        let e = submit_real(ComPtr::null(), &mut r);
        assert!(matches!(e, Err(D3d12Error::LoaderMissing { .. })));
    }

    #[test]
    fn cmdop_eq_distinguishes_kinds() {
        let a = CmdOp::Dispatch { x: 1, y: 2, z: 3 };
        let b = CmdOp::Dispatch { x: 1, y: 2, z: 3 };
        let c = CmdOp::Dispatch { x: 9, y: 2, z: 3 };
        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}
