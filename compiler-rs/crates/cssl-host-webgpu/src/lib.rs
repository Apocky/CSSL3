//! CSSLv3 stage0 — WebGPU host submission via `wgpu`.
//!
//! § SPEC : `specs/14_BACKEND.csl` § HOST-SUBMIT BACKENDS § WebGPU + §§
//!         07_CODEGEN.csl § GPU BACKEND — WGSL path.
//!
//! § STRATEGY (T11-D68 / S6-E4)
//!   This crate has two layers :
//!
//!   Phase-1 (default-build, always available) — the catalog modules
//!   ([`adapter`], [`features`]) carry the WebGPU adapter / feature / limits
//!   surface description WITHOUT pulling the `wgpu` crate. Pure-Rust,
//!   builds on every host, useful for target-spec consumers like
//!   `cssl-cgen-gpu-wgsl` that need the type-names but not the runtime.
//!
//!   Phase-2 (`wgpu-runtime` feature, opt-in) — wires real `wgpu::Instance`
//!   / `Adapter` / `Device` / `Queue` / `Buffer` / `Texture` /
//!   `ComputePipeline` / `RenderPipeline` / `CommandEncoder` paths against
//!   the wgpu crate. Compiles to native (DX12 / Vulkan / Metal) AND
//!   wasm32 (real browser-WebGPU).
//!
//! § BACKEND-NEGOTIATION (wgpu-runtime feature)
//!   On native, wgpu picks the highest-priority backend available :
//!     - Windows  : DX12 (MSVC toolchain) or Vulkan (GNU toolchain)
//!                  (Apocky's Arc A770 = either ; the Vulkan path is well
//!                  tested and ships on the GNU toolchain that the workspace
//!                  pins per T11-D20 R16-anchor).
//!     - Linux    : Vulkan
//!     - macOS    : Metal
//!     - Android  : Vulkan (then GLES)
//!   On wasm32-unknown-unknown, the only backend is real browser-WebGPU.
//!
//! § ASYNC API (wgpu-runtime feature)
//!   wgpu uses async/await for `request_adapter` + `request_device` +
//!   `map_async` + `pop_error_scope`. Stage-0 wraps these with
//!   `pollster::block_on` so the CSSLv3 host API stays sync. Full async
//!   integration ties to CSSLv3's effect-row + async story (deferred slice).
//!
//! § CAP-SYSTEM MAPPING (wgpu-runtime feature)
//!   `wgpu::Buffer` ≡ `iso<gpu-buffer>` ; sync via
//!   `Queue::on_submitted_work_done`. See `sync::submit_with_callback`.
//!
//! § STAGE-0 SCOPE
//!
//!   Phase-1 catalog (always) :
//!   - [`adapter::WebGpuBackend`]          — Backend enum.
//!   - [`adapter::WebGpuAdapter`]          — Adapter identification record.
//!   - [`adapter::AdapterPowerPref`]       — Power preference.
//!   - [`features::SupportedFeatureSet`]   — Feature catalog.
//!   - [`features::WebGpuFeature`]         — 14-variant feature enum.
//!   - [`features::WebGpuLimits`]          — Limits snapshot.
//!
//!   Phase-2 wgpu-runtime (feature-gated) :
//!   - `instance::WebGpuInstance`           — `wgpu::Instance` + adapter probe.
//!   - `device::WebGpuDevice`               — `Device` + `Queue` (sync wrap).
//!   - `buffer::WebGpuBuffer`               — Buffer alloc + initialized-upload.
//!   - `texture::WebGpuTexture`             — Texture alloc + view.
//!   - `pipeline::WebGpuComputePipeline`    — ComputePipeline create.
//!   - `pipeline::WebGpuRenderPipeline`     — RenderPipeline create.
//!   - `command::WebGpuCommandEncoder`      — CommandEncoder + dispatch.
//!   - `sync::submit_and_block`             — Queue::submit + poll(Wait).
//!   - `sync::submit_with_callback`         — Queue::on_submitted_work_done.
//!   - `sync::read_buffer_sync`             — Buffer readback to CPU.
//!   - `kernels`                             — Hand-written WGSL until D4 ships.
//!   - `error::WebGpuError`                 — Error-types.

#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::struct_excessive_bools)]
// Without the wgpu-runtime feature, this crate is pure-Rust + no `unsafe`.
// With the feature, wgpu's internal traits may surface unsafe-trait-bounds
// into our types, so we use `deny(unsafe_op_in_unsafe_fn)` instead of the
// stronger `forbid(unsafe_code)`. The wgpu-runtime layer never writes any
// `unsafe` blocks of its own.
#![cfg_attr(not(feature = "wgpu-runtime"), forbid(unsafe_code))]
#![cfg_attr(feature = "wgpu-runtime", deny(unsafe_op_in_unsafe_fn))]

// Phase-1 catalog modules — always available.
pub mod adapter;
pub mod features;

// Phase-2 wgpu-runtime modules — feature-gated.
#[cfg(feature = "wgpu-runtime")]
pub mod buffer;
#[cfg(feature = "wgpu-runtime")]
pub mod command;
#[cfg(feature = "wgpu-runtime")]
pub mod device;
#[cfg(feature = "wgpu-runtime")]
pub mod error;
#[cfg(feature = "wgpu-runtime")]
pub mod instance;
#[cfg(feature = "wgpu-runtime")]
pub mod kernels;
#[cfg(feature = "wgpu-runtime")]
pub mod pipeline;
#[cfg(feature = "wgpu-runtime")]
pub mod sync;
#[cfg(feature = "wgpu-runtime")]
pub mod texture;

// Phase-1 catalog re-exports (always).
pub use adapter::{AdapterPowerPref, WebGpuAdapter, WebGpuBackend};
pub use features::{SupportedFeatureSet, WebGpuFeature, WebGpuLimits};

// Phase-2 wgpu-runtime re-exports (feature-gated).
#[cfg(feature = "wgpu-runtime")]
pub use buffer::{WebGpuBuffer, WebGpuBufferConfig};
#[cfg(feature = "wgpu-runtime")]
pub use command::WebGpuCommandEncoder;
#[cfg(feature = "wgpu-runtime")]
pub use device::{WebGpuDevice, WebGpuDeviceConfig};
#[cfg(feature = "wgpu-runtime")]
pub use error::WebGpuError;
#[cfg(feature = "wgpu-runtime")]
pub use instance::{BackendHint, WebGpuInstance, WebGpuInstanceConfig};
#[cfg(feature = "wgpu-runtime")]
pub use pipeline::{
    WebGpuComputePipeline, WebGpuComputePipelineConfig, WebGpuRenderPipeline,
    WebGpuRenderPipelineConfig,
};
#[cfg(feature = "wgpu-runtime")]
pub use sync::{read_buffer_sync, submit_and_block, submit_with_callback};
#[cfg(feature = "wgpu-runtime")]
pub use texture::{WebGpuTexture, WebGpuTextureConfig};

/// Crate version exposed for scaffold verification.
pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");

/// wgpu version this crate is pinned against (workspace dep version-string).
/// Surfaced as a const so downstream telemetry / R16-anchor tooling can
/// record the negotiated backend version alongside the wgpu library version.
pub const WGPU_VERSION: &str = "23";

/// Whether the wgpu-runtime layer is compiled in. False = catalog-only build.
pub const WGPU_RUNTIME_ENABLED: bool = cfg!(feature = "wgpu-runtime");

#[cfg(test)]
mod scaffold_tests {
    use super::{STAGE0_SCAFFOLD, WGPU_RUNTIME_ENABLED, WGPU_VERSION};

    #[test]
    fn scaffold_version_present() {
        assert!(!STAGE0_SCAFFOLD.is_empty());
    }

    #[test]
    fn wgpu_version_pinned_to_23() {
        // T11-D68 anchor : if this changes, document the wgpu API delta.
        assert_eq!(WGPU_VERSION, "23");
    }

    #[test]
    fn wgpu_runtime_flag_matches_compile_cfg() {
        // Cross-check : the constant tracks whether the feature is on.
        assert_eq!(WGPU_RUNTIME_ENABLED, cfg!(feature = "wgpu-runtime"));
    }
}
