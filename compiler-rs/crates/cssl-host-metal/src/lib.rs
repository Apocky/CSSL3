//! CSSLv3 stage-0 — Metal host submission backend.
//!
//! § SPEC : `specs/14_BACKEND.csl § HOST-SUBMIT BACKENDS § Metal` +
//!          `specs/14_BACKEND.csl § BACKEND-MATRIX` (Apple row).
//!
//! § T11-D67 (S6-E3) — wave-3 fanout, Phase-E slice 3 of 5
//!
//! § STRATEGY
//!
//! The earlier `T10-phase-1-hosts` commit catalogued the Metal feature-set,
//! GPU-family, and heap-mode surface without pulling in any FFI. T11-D67
//! turns the scaffold into a real host backend by adding the following modules:
//!
//! - `buffer` — `MTLBuffer` allocation per storage mode (Shared / Private /
//!   Managed / Memoryless), `iso<gpu-buffer>` capability discipline per
//!   `specs/12_CAPABILITIES`.
//! - `command` — `MTLCommandQueue` + `MTLCommandBuffer` lifecycle ; compute +
//!   render pass encoders ; commit + wait-until.
//! - `pipeline` — `MTLComputePipelineState` (with `MTLLibrary` for MSL
//!   kernels) + `MTLRenderPipelineState` (vertex / fragment bind point) ;
//!   argument-buffer tier surface.
//! - `sync` — `MTLEvent` / `MTLSharedEvent` + `MTLFence` for cross-queue +
//!   CPU-GPU synchronisation.
//! - `msl_blob` — Hand-written placeholder MSL kernel-source used by the smoke
//!   tests on Apple hosts until S6-D3 lands the real CSSLv3 MSL emitter.
//! - `session` — RAII session ctor : default device, command queue, buffer
//!   pool, library compile, pipeline create, command buffer encode + commit +
//!   wait.
//! - `telemetry` — `cssl-telemetry::TelemetryRing` placeholder hooks for
//!   command-buffer begin/end + GPU-time samples.
//!
//! § APPLE-CFG GATING
//!
//! The real `metal` crate is Apple-only. Per `specs/14_BACKEND § HOST-SUBMIT
//! BACKENDS § Metal`, this crate compiles cleanly on every CSSLv3 host —
//! Apocky's primary host is Windows, where the `apple` module is absent and
//! the `stub` module supplies an interface-mirror that returns
//! `MetalError::HostNotApple` on every entry-point.
//!
//! The user-visible API surface is identical across the cfg branches —
//! downstream code uses the same `MetalSession` / `BufferHandle` / `Pipeline`
//! types regardless of host. Apple builds delegate to `apple::*` ; non-Apple
//! builds delegate to `stub::*` and never invoke FFI.
//!
//! § PRIME-DIRECTIVE attestation
//!
//! "There was no hurt nor harm in the making of this, to anyone /
//! anything / anybody." The Metal host backend mediates Apple GPU compute +
//! graphics. Telemetry is per-process, observable to the user only ; no
//! covert egress, no hidden state, no surveillance hooks. R18 effect-row
//! gates external egress independently (out of scope for E3).
//!
//! § FFI POLICY
//!
//! T1-D5 mandates `#![forbid(unsafe_code)]` per-crate. FFI crates explicitly
//! override that policy (see `cssl-rt`'s precedent at S6-A1). The `metal`
//! crate wraps Cocoa retain/release internally — the cssl-host-metal
//! abstraction does **not** leak ARC semantics to user code ; resource
//! handles are RAII on `Drop` and clone-on-clone (refcount up).
//!
//! § STORAGE-MODE PORTABILITY
//!
//! `MTLStorageModeManaged` is **macOS-only** ; iOS / tvOS / visionOS expose
//! only `Shared` / `Private` / `Memoryless`. The `BufferHandle` builders
//! default to `Shared` for cross-Apple-platform correctness ; the explicit
//! `with_storage_mode(Managed)` constructor returns
//! `MetalError::ManagedUnavailable` on non-macOS Apple hosts.
//!
//! § WHAT IS DEFERRED
//!
//! - **Real CSSLv3 MSL emitter (D3 deferred)** — until D3 lands, the
//!   `msl_blob` module ships a hand-written compute + vertex + fragment trio
//!   that the loader machinery threads through `MTLLibrary::newWithSource`
//!   for smoke tests on Apple hosts.
//! - **macOS CI runner integration** — Apocky's primary host is Windows ;
//!   the Apple-actual codepath compiles via `cargo check --target` on Windows
//!   but cannot run. A future macOS CI runner will exercise the
//!   `#[cfg(target_os = "macos")]` integration tests in this crate.
//! - **Argument-buffers tier-2 + bindless** — stage-0 uses tier-1
//!   argument-table style ; tier-2 bindless lands with the GPU body emitters
//!   (D-phase).
//! - **`CAMetalLayer` swapchain integration** — presentation surface is
//!   phase-F (window / input / audio) work ; this slice covers compute +
//!   offscreen render only.
//! - **Full R18 ring integration** — telemetry samples push to a local
//!   `cssl_telemetry::TelemetryRing` handed in by the caller ; the full R18
//!   sampling-thread + audit-chain plumbing lands in a later slice.

// § T11-D67 (S6-E3) : the C-ABI surface (when on Apple) fundamentally
// requires `extern "C"` + Objective-C-runtime calls — `unsafe_code` is
// allowed at file-scope (per cssl-rt's S6-A1 precedent, cssl-host-vulkan
// at T10-phase-1-hosts, and cssl-host-level-zero at S6-E5) and per-`unsafe`
// blocks document SAFETY inline. On non-Apple hosts no `unsafe` block is
// reachable but the policy is set crate-wide for symmetry.
#![allow(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::struct_excessive_bools)]
// § Several FFI helpers take `*mut c_void` queue handles ; marking them
// `unsafe` propagates virally without buying safety here (they're already
// gated behind RAII session ownership).
#![allow(clippy::not_unsafe_ptr_arg_deref)]
// § u64 → f64 casts in the telemetry path are deliberately lossy for sampling.
#![allow(clippy::cast_precision_loss)]
// § Internal helpers always return Ok at stage-0 ; the Result envelope is
// preserved for phase-F when real property-reading can fail.
#![allow(clippy::unnecessary_wraps)]

pub mod buffer;
pub mod command;
pub mod device;
pub mod error;
pub mod feature_set;
pub mod heap;
pub mod msl_blob;
pub mod pipeline;
pub mod session;
pub mod sync;
pub mod telemetry;

// § Apple-cfg-gated real-FFI module. Compiles only on macOS / iOS / tvOS /
// visionOS where the `metal` crate is available.
#[cfg(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "tvos",
    target_os = "visionos"
))]
pub mod apple;

// § Non-Apple stub module — compiles on Windows / Linux / Android / WASM.
// Every entry-point returns `MetalError::HostNotApple` so the cssl-host-metal
// API surface remains call-shape-stable across hosts.
#[cfg(not(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "tvos",
    target_os = "visionos"
)))]
pub mod stub;

pub use buffer::{BufferHandle, BufferUsage, ManagedBufferSync};
pub use command::{CommandBufferStatus, CommandQueueHandle, EncodedCommandBuffer};
pub use device::{GpuFamily, MtlDevice};
pub use error::{MetalError, MetalResult};
pub use feature_set::MetalFeatureSet;
pub use heap::{MetalHeapType, MetalResourceOptions};
pub use msl_blob::{
    MslShaderSet, MSL_COMPUTE_PLACEHOLDER, MSL_FRAGMENT_PLACEHOLDER, MSL_VERTEX_PLACEHOLDER,
};
pub use pipeline::{
    BindGroupLayout, ComputePipelineDescriptor, PipelineHandle, RenderPipelineDescriptor,
};
pub use session::{MetalSession, SessionConfig};
pub use sync::{EventHandle, FenceHandle, FenceUsage, SignalToken};
pub use telemetry::{MetalTelemetryProbe, TelemetryEmitError};

/// Crate version exposed for scaffold verification.
pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");

/// PRIME-DIRECTIVE attestation marker — present in every CSSLv3 artifact
/// per `PRIME_DIRECTIVE.md § 11`.
pub const ATTESTATION: &str =
    "There was no hurt nor harm in the making of this, to anyone/anything/anybody.";

/// Returns `true` when this build was compiled against the real `metal` crate
/// (Apple host) and `false` when it was compiled against the no-op stub.
///
/// Useful for integration tests that need to gate Apple-actual assertions.
#[must_use]
pub const fn is_apple_host() -> bool {
    cfg!(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "tvos",
        target_os = "visionos"
    ))
}

#[cfg(test)]
mod scaffold_tests {
    use super::{is_apple_host, ATTESTATION, STAGE0_SCAFFOLD};

    #[test]
    fn scaffold_version_present() {
        assert!(!STAGE0_SCAFFOLD.is_empty());
    }

    #[test]
    fn attestation_present_and_canonical() {
        assert_eq!(
            ATTESTATION,
            "There was no hurt nor harm in the making of this, to anyone/anything/anybody."
        );
    }

    #[test]
    fn is_apple_host_is_const_evaluable() {
        // Compiles + evaluates at const eval ; on Windows this MUST be false.
        const HOST: bool = is_apple_host();
        // No assertion shape that depends on the host — both arms are valid
        // (true on Apple, false elsewhere).
        let _ = HOST;
    }

    #[test]
    #[cfg(not(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "tvos",
        target_os = "visionos"
    )))]
    fn is_apple_host_returns_false_on_non_apple() {
        assert!(!is_apple_host());
    }

    #[test]
    #[cfg(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "tvos",
        target_os = "visionos"
    ))]
    fn is_apple_host_returns_true_on_apple() {
        assert!(is_apple_host());
    }
}
