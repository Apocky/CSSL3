//! CSSLv3 stage0 — D3D12 host submission backend.
//!
//! § SPEC : `specs/14_BACKEND.csl` § HOST-SUBMIT BACKENDS § D3D12 +
//!          `specs/10_HW.csl` § SYSMAN AVAILABILITY TABLE (DXGI partial).
//!
//! § STRATEGY (T11-D66, S6-E2)
//!   On Windows targets the impl uses `windows-rs 0.58` to wrap real D3D12 +
//!   DXGI 1.6 surface : factory + adapter enumeration, device creation
//!   negotiating the highest available feature level (12.0..12.2),
//!   command queue / list / allocator, descriptor + resource heaps, root
//!   signatures, pipeline state objects, fence-based synchronization, and
//!   diagnostic capture via DRED + ID3D12InfoQueue.
//!
//!   On non-Windows targets every constructor returns
//!   [`error::D3d12Error::LoaderMissing`] so the workspace `cargo check`
//!   stays green on Linux + macOS.
//!
//! § UNSAFE
//!   FFI work is opt-in : `unsafe` is allowed at specific call sites
//!   wrapping `windows-rs` interfaces, never spread crate-wide. Each unsafe
//!   block carries a `// SAFETY :` comment.
//!
//! § DRED
//!   Per `specs/14_BACKEND.csl § HOST-SUBMIT BACKENDS § D3D12`, DRED (Device
//!   Removed Extended Data) is enabled when the debug layer is on, capturing
//!   GPU breadcrumbs + page-fault details for crash forensics. See
//!   [`dred::DredCapture`].
//!
//! § CAPABILITY MAPPING
//!   `ID3D12Resource` semantically maps to `iso<gpu-buffer>` per
//!   `specs/12_CAPABILITIES.csl § ISO-OWNERSHIP`. The runtime returns a
//!   linear handle the caller must explicitly transfer ; aliasing is not
//!   exposed.

#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::struct_excessive_bools)]

pub mod adapter;
pub mod cmd;
pub mod device;
pub mod dred;
pub mod error;
pub mod features;
pub mod fence;
pub mod ffi;
pub mod heap;
pub mod pipeline;
pub mod pso;
pub mod queue;
pub mod resource;
pub mod root_signature;
pub mod swapchain;
pub mod work_graph;

pub use adapter::{DxgiAdapter, FeatureLevel};
pub use cmd::{CmdOp, CmdQueueDesc, CmdRecorder, Submission, submit_mock, submit_real};
pub use device::{AdapterPreference, AdapterRecord, Device, Factory};
pub use dred::{DiagnosticMessage, DiagnosticSeverity, DredCapture};
pub use error::{D3d12Error, Result};
pub use features::{D3d12FeatureOptions, WaveMatrixTier};
pub use fence::{Fence, FenceWait};
pub use ffi::{
    ComPtr, CommandListTypeRaw, D3DFeatureLevel, DxgiFormat, Guid, HRESULT, IUnknownVTable,
    Loader, S_OK, failed, hr_check, succeeded,
};
pub use heap::{CommandListType, DescriptorHeapType, HeapType};
pub use pipeline::{
    ComputePipelineDesc, DXBC_MAGIC, DxilBytecode, GraphicsPipelineDesc, PipelineHandle,
    PipelineKind, create_compute_pipeline_mock, create_compute_pipeline_real,
    create_graphics_pipeline_mock, synth_dxil_fixture,
};
pub use pso::{ComputePsoDesc, GraphicsPsoDesc, PipelineState};
pub use queue::{CommandAllocator, CommandList, CommandQueue, CommandQueuePriority};
pub use resource::{
    DescriptorHeap, GpuBufferIso, Resource, ResourceDesc, ResourceState, UploadBuffer,
};
pub use root_signature::{
    RootParameter, RootParameterKind, RootSignature, RootSignatureBuilder, ShaderVisibility,
};
pub use swapchain::{Hwnd, PresentMode, SwapChain, SwapChainConfig, SwapEffect};
pub use work_graph::{DispatchGraphArgs, WorkGraphProgramDesc, WorkGraphsTier};

/// Crate version exposed for scaffold verification.
pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod scaffold_tests {
    use super::STAGE0_SCAFFOLD;

    #[test]
    fn scaffold_version_present() {
        assert!(!STAGE0_SCAFFOLD.is_empty());
    }
}
