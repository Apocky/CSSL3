//! § cssl-host-substrate-render-v3-d3d12 — d3d12-direct + DXIL substrate-render.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § T11-W18-L8-DXIL-FOUNDATION · the L7 stack got rid of wgpu + naga + WGSL
//! and runs on `ash` (Vulkan-1.3) consuming SPIR-V emitted by `csslc`. This L8
//! stack is the **Windows-native companion** : it bypasses `ash` altogether
//! and runs on `windows = "0.58"` raw FFI consuming **DXIL** (the canonical
//! Windows GPU bytecode format) emitted by `csslc`. The chain is now :
//!
//! ```text
//! Labyrinth of Apocalypse/systems/substrate_v2_kernel.csl
//!         │  (csslc — proprietary compiler)
//!         ▼
//! cssl-cgen-gpu-dxil : Vec<u8>  — canonical DXIL container bytes
//!         │  (no DXC.dll dependency · no FXC · no HLSL roundtrip)
//!         ▼
//! cssl-host-substrate-render-v3-d3d12::DxilArtifact  — opaque blob carrier
//!         │
//!         ▼
//! ID3D12Device2::CreateComputePipelineState(D3D12_SHADER_BYTECODE)
//!         │
//!         ▼
//! d3d12-direct PSO + RootSignature + DescriptorHeap + CommandList
//!         │
//!         ▼
//! one ID3D12GraphicsCommandList::Dispatch per frame +
//! IDXGISwapChain3::Present(0, DXGI_PRESENT_ALLOW_TEARING) for low-latency
//! ```
//!
//! § PROPRIETARY-EVERYTHING (§ I> spec/14_BACKEND § OWNED DXIL EMITTER)
//!   - Source-of-truth : `Labyrinth of Apocalypse/systems/substrate_v2_kernel.csl`
//!   - Compiler        : `cssl-cgen-gpu-dxil` (TODO ; from-scratch DXIL container ·
//!                       zero ext-dep · no DXC.dll · no FXC.exe · no HLSL roundtrip)
//!   - GPU API         : `windows = "0.58"` (D3D12 raw bindings · single dep)
//!   - NO d3d12-rs vendor-wrapper · NO ash · NO Vulkan · NO wgpu · NO naga · NO HLSL
//!
//! § FOUNDATION-SLICE SCOPE (T11-W18-L8-FOUNDATION)
//!   This crate **does not yet compile real DXIL** — that belongs to a
//!   companion `cssl-cgen-gpu-dxil` slice. The host accepts an opaque
//!   `&[u8]` DXIL blob from any source ; tests pass an **empty-body stub**
//!   matching the v3-vulkan layered-construction pattern. The host's job
//!   in this slice is :
//!     - Cargo + crate skeleton + workspace integration (windows-rs 0.58)
//!     - `DxilArtifact` carrier-type (always-on · headless-CI-safe)
//!     - `D3D12SubstrateRenderer::try_new` headless construction (runtime)
//!     - `try_new_with_swapchain<W: HasWindowHandle>` (present)
//!     - `dispatch_with_present` API matching v3-vulkan exactly
//!     - Triple-buffer (`FRAMES_IN_FLIGHT = 3`) per fps-cap-fix policy
//!     - `DXGI_SWAP_EFFECT_FLIP_DISCARD` + `DXGI_PRESENT_ALLOW_TEARING`
//!     - `LOA_DXIL_PRESENT_TEAR=0` env-override for QA-pinned VSync
//!     - 6+ unit tests (artifact path · enum-only structural surface)
//!
//! § HEADLESS-FIRST DESIGN
//!   The default-build path (`default = []`) **does not pull windows-rs
//!   Direct3D12/DXGI surface** — it exposes only [`DxilArtifact`] + the
//!   structural enums + the env-override helper. CI runners on Linux +
//!   macOS see a clean `cargo check --workspace` because the
//!   `[target.'cfg(target_os = "windows")']` gate skips the windows-rs
//!   dep entirely on non-Windows. Tests #1, #2 always run.
//!
//! § DETERMINISM (§ Apocky-directive)
//!   Same DXIL bytes ⇒ byte-identical PSO graph (verified upstream by
//!   `cssl-cgen-gpu-dxil`'s emit_is_deterministic test once that crate
//!   lands). Same dispatch on the same device ⇒ byte-identical output
//!   image. The L8 host's job is to be a **transparent passthrough** —
//!   no host-side rng, no clock-reads, no nondeterministic ordering.
//!
//! § PRIME-DIRECTIVE
//!   Σ-mask consent gating is encoded **structurally** in the substrate-
//!   kernel `.csl` source (§ ω-FIELD § Σ-mask-check W! consent-gate). The
//!   L8 host never bypasses the kernel — there is exactly one compute
//!   path, exactly one DXIL blob, exactly one entry-point. Just like L7.

// § Crate-level safety policy — the default-build path holds
// `forbid(unsafe_code)`. The optional `runtime` feature opts a single
// inner module into `unsafe_code` for the direct D3D12 FFI calls that
// windows-rs exposes. Without `runtime`, this crate is fully unsafe-free.
#![cfg_attr(not(feature = "runtime"), forbid(unsafe_code))]
#![cfg_attr(feature = "runtime", deny(unsafe_code))]
#![allow(clippy::module_name_repetitions)]

// ════════════════════════════════════════════════════════════════════════════
// § DxilArtifact — the compiled DXIL bytes, available without any GPU dep.
// Carries enough metadata to drive ID3D12Device::CreateComputePipelineState
// but no D3D12 handles itself.
// ════════════════════════════════════════════════════════════════════════════

/// § The DXIL container magic bytes (`DXBC` little-endian header that DXIL
/// piggybacks on, per the canonical container spec). Re-exported so tests +
/// downstream callers can structurally validate without pulling
/// `cssl-cgen-gpu-dxil` directly. `DXBC` = `0x43425844`.
///
/// Note : this is the **container** magic — DXIL bytecode is wrapped in a
/// DXBC container with a `DXIL` part-marker inside. The FOUNDATION slice
/// validates only the container magic ; the part-marker check belongs to
/// `cssl-cgen-gpu-dxil` once that crate lands.
pub const DXBC_CONTAINER_MAGIC: u32 = 0x4342_5844;

/// § Errors that can occur when constructing a [`DxilArtifact`] from raw
/// bytes. Structural-only ; no D3D12 calls fired here.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum DxilArtifactError {
    /// The supplied byte slice is empty. The FOUNDATION slice still
    /// **accepts** this (empty-body stub matches the v3-vulkan layered-
    /// construction pattern) but flags it via [`DxilArtifact::is_stub`]
    /// so callers can branch on real-vs-stub.
    #[error("dxil-artifact rejected : empty bytes (no header · no entry)")]
    Empty,
    /// The supplied bytes are non-empty but do not start with the DXBC
    /// container magic `0x43425844`. Reserved for the post-FOUNDATION
    /// slice when `cssl-cgen-gpu-dxil` lands ; the FOUNDATION slice
    /// **does not** raise this — it accepts arbitrary blobs as stubs.
    #[error("dxil-artifact rejected : missing DXBC container magic 0x43425844 (got {got:#010x})")]
    BadMagic { got: u32 },
}

/// § The DXIL artifact for the substrate-kernel.
///
/// Construct via [`DxilArtifact::from_bytes`] (FOUNDATION slice ; accepts
/// arbitrary blobs incl. the empty-body stub) or [`DxilArtifact::stub`]
/// (returns the canonical empty stub used in tests + scaffolding).
///
/// Carries the raw byte stream + the original entry-point name so callers
/// can drive `D3D12_SHADER_BYTECODE` + `ID3D12RootSignature` construction
/// without re-discovering the entry-name from the container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DxilArtifact {
    /// Canonical DXIL container bytes. May be empty in stub mode.
    bytes: Vec<u8>,
    /// Compute-shader entry-point name (substrate-kernel default = `cs_main`).
    entry_name: String,
    /// Whether the artifact is a stub (empty bytes ; FOUNDATION-slice).
    is_stub: bool,
}

impl DxilArtifact {
    /// § Construct an artifact from raw DXIL bytes.
    ///
    /// FOUNDATION-slice behavior :
    ///   - Empty bytes ⇒ accepted as a stub (`is_stub = true`). Returning
    ///     [`DxilArtifactError::Empty`] would block the layered-construction
    ///     test pattern from v3-vulkan ; we mirror it instead.
    ///   - Non-empty bytes ⇒ accepted unconditionally for now. The DXBC
    ///     magic-check will be enforced in the post-FOUNDATION slice once
    ///     `cssl-cgen-gpu-dxil` lands and the test fixtures carry valid
    ///     containers.
    pub fn from_bytes(bytes: impl Into<Vec<u8>>) -> Self {
        let bytes = bytes.into();
        let is_stub = bytes.is_empty();
        Self {
            bytes,
            entry_name: "cs_main".into(),
            is_stub,
        }
    }

    /// § Construct an artifact with an explicit entry-point name.
    pub fn from_bytes_with_entry(bytes: impl Into<Vec<u8>>, entry_name: impl Into<String>) -> Self {
        let bytes = bytes.into();
        let is_stub = bytes.is_empty();
        Self {
            bytes,
            entry_name: entry_name.into(),
            is_stub,
        }
    }

    /// § Canonical empty-body stub. The FOUNDATION slice uses this for
    /// every test that exercises the layered-construction path without
    /// requiring real DXIL emit. Mirrors the empty-shader-module pattern
    /// from `cssl-host-substrate-render-v3` (vulkan).
    #[must_use]
    pub fn stub() -> Self {
        Self {
            bytes: Vec::new(),
            entry_name: "cs_main".into(),
            is_stub: true,
        }
    }

    /// Borrow the DXIL byte stream (= `D3D12_SHADER_BYTECODE::pShaderBytecode`).
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Total byte length of the DXIL container (= `D3D12_SHADER_BYTECODE::BytecodeLength`).
    #[must_use]
    pub fn byte_len(&self) -> usize {
        self.bytes.len()
    }

    /// Compute-shader entry-point name.
    #[must_use]
    pub fn entry_name(&self) -> &str {
        &self.entry_name
    }

    /// Whether the artifact is a stub (empty-body).
    #[must_use]
    pub const fn is_stub(&self) -> bool {
        self.is_stub
    }

    /// First 4 bytes interpreted as little-endian u32 — should match
    /// [`DXBC_CONTAINER_MAGIC`] for real DXIL containers. Returns 0 for
    /// stubs (where `bytes.len() < 4`).
    #[must_use]
    pub fn container_magic(&self) -> u32 {
        if self.bytes.len() < 4 {
            0
        } else {
            u32::from_le_bytes([self.bytes[0], self.bytes[1], self.bytes[2], self.bytes[3]])
        }
    }
}

// ════════════════════════════════════════════════════════════════════════════
// § FRAMES_IN_FLIGHT — triple-buffer constant, available even on the
// default-build path so callers can size descriptor pools / fence rings
// without flipping the `runtime` feature on. Mirrors the v3-vulkan layout.
// ════════════════════════════════════════════════════════════════════════════

/// § Per-frame ring depth. Triple-buffer (= 3) per the fps-cap-fix policy :
/// CPU records frame N+2 while GPU presents frame N. Same value as
/// `cssl-host-substrate-render-v3::present::FRAMES_IN_FLIGHT`. Hard-coded
/// so the descriptor-heap + fence-ring + command-allocator-array can be
/// stack-allocated `[T; 3]` without `Vec` heap traffic.
pub const FRAMES_IN_FLIGHT: usize = 3;

// ════════════════════════════════════════════════════════════════════════════
// § Tearing-toggle env-override — `LOA_DXIL_PRESENT_TEAR`
// ════════════════════════════════════════════════════════════════════════════

/// § Env-var name that overrides the default tearing-allowed Present mode.
///
/// Default behavior : the L8 host calls `IDXGISwapChain3::Present(0,
/// DXGI_PRESENT_ALLOW_TEARING)` for low-latency low-stutter rendering on
/// variable-refresh displays. When QA needs to pin VSync (e.g. for video
/// capture or CRT-tile emulation), set `LOA_DXIL_PRESENT_TEAR=0` and the
/// host falls back to `Present(1, 0)` (1 sync-interval · no flags).
///
/// Recognized values :
///   - `0` / `false` / `off` ⇒ tearing **disabled** (VSync pinned)
///   - anything else (incl. unset) ⇒ tearing **enabled** (default)
pub const TEAR_ENV_VAR: &str = "LOA_DXIL_PRESENT_TEAR";

/// § Resolve the effective tearing-policy for the current process from
/// the [`TEAR_ENV_VAR`] env-var. Pure function · always-on (no D3D12 dep).
#[must_use]
pub fn tearing_enabled_from_env() -> bool {
    std::env::var(TEAR_ENV_VAR).map_or(true, |s| {
        let v = s.trim().to_ascii_lowercase();
        !matches!(v.as_str(), "0" | "false" | "off" | "no")
    })
}

// ════════════════════════════════════════════════════════════════════════════
// § Per-frame data carriers — observer + crystal stub structs.
// Stable ABI (Pod-style · no padding · no enums) so the host can `memcpy`
// them into mapped upload-heap buffers. Available on default-build so
// downstream callers can stage frame-data on Linux/macOS for replay.
// ════════════════════════════════════════════════════════════════════════════

/// § Per-frame observer-coord uniform-buffer payload. Mirrors the v3-vulkan
/// `present::ObserverCoord` exactly so callers can swap renderer-backends
/// without touching frame-data.
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct ObserverCoord {
    pub world_x: f32,
    pub world_y: f32,
    pub world_z: f32,
    pub gaze_falloff: f32,
}

/// § Per-frame crystal storage-buffer payload. Mirrors the v3-vulkan
/// `present::Crystal` exactly. Up to 256 crystals × 32 bytes = 8 KiB
/// payload per frame (well below the 64 KiB upload-heap chunk limit).
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct Crystal {
    pub world_x: f32,
    pub world_y: f32,
    pub world_z: f32,
    pub radius: f32,
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub intensity: f32,
}

// ════════════════════════════════════════════════════════════════════════════
// § Errors from the present-path. Available without `present` feature so
// callers can match-arms on the enum without conditionally-compiled code.
// ════════════════════════════════════════════════════════════════════════════

/// § Errors from the d3d12-direct present path.
#[derive(Debug, thiserror::Error)]
pub enum PresentError {
    /// Window-handle is not a Win32 `HWND` (web/wayland/x11/cocoa). The L8
    /// host is Windows-only by design ; cross-platform present lives in
    /// L7 (`cssl-host-substrate-render-v3` on `ash`).
    #[error("dxil-present rejected : non-Win32 window-handle (L8 is Windows-only ; use L7 for cross-platform)")]
    UnsupportedWindowHandle,
    /// DXGI factory creation failed. Driver missing or feature-level too
    /// low (D3D12 requires DXGI-1.4+ for `IDXGIFactory4` ; tearing
    /// detection requires `IDXGIFactory5::CheckFeatureSupport`).
    #[error("dxgi factory creation failed (HRESULT={hr:#010x})")]
    DxgiFactoryCreate { hr: u32 },
    /// `D3D12CreateDevice` failed at `D3D_FEATURE_LEVEL_11_0`.
    #[error("d3d12 device creation failed (HRESULT={hr:#010x}) — D3D_FEATURE_LEVEL_11_0 minimum")]
    DeviceCreate { hr: u32 },
    /// Command-queue creation failed.
    #[error("d3d12 command-queue creation failed (HRESULT={hr:#010x})")]
    CommandQueueCreate { hr: u32 },
    /// Swapchain creation failed.
    #[error("dxgi swapchain creation failed (HRESULT={hr:#010x})")]
    SwapchainCreate { hr: u32 },
    /// Fence / event creation failed.
    #[error("d3d12 fence or event creation failed (HRESULT={hr:#010x})")]
    FenceCreate { hr: u32 },
    /// PSO / root-signature build from DXIL bytes failed.
    #[error("d3d12 compute-pipeline build from DXIL bytes failed (HRESULT={hr:#010x}) — likely stub-bytes ; real DXIL emit lands post-FOUNDATION slice")]
    PipelineCreate { hr: u32 },
}

// ════════════════════════════════════════════════════════════════════════════
// § Tearing-policy enum — exposed on default-build so callers can encode
// the runtime choice without flipping features.
// ════════════════════════════════════════════════════════════════════════════

/// § The effective tearing-policy resolved at swapchain-creation time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TearingPolicy {
    /// `IDXGISwapChain3::Present(0, DXGI_PRESENT_ALLOW_TEARING)` — default
    /// for low-latency on variable-refresh displays. Requires the swapchain
    /// to be created with `DXGI_SWAP_CHAIN_FLAG_ALLOW_TEARING`. The L8
    /// host probes `IDXGIFactory5::CheckFeatureSupport(ALLOW_TEARING)`
    /// during `try_new_with_swapchain` and demotes to `Vsync` if the
    /// driver / OS does not support it.
    AllowTearing,
    /// `IDXGISwapChain3::Present(1, 0)` — VSync-pinned. Selected when the
    /// `LOA_DXIL_PRESENT_TEAR=0` env-override is set OR when the driver
    /// does not support `DXGI_FEATURE_PRESENT_ALLOW_TEARING`.
    Vsync,
}

impl TearingPolicy {
    /// § Map a boolean env-resolution to a [`TearingPolicy`]. Convenience
    /// for tests + callers that don't want to import the enum + the helper.
    #[must_use]
    pub const fn from_env_bool(allow: bool) -> Self {
        if allow {
            Self::AllowTearing
        } else {
            Self::Vsync
        }
    }

    /// § Whether this policy permits tearing.
    #[must_use]
    pub const fn allows_tearing(self) -> bool {
        matches!(self, Self::AllowTearing)
    }
}

// ════════════════════════════════════════════════════════════════════════════
// § D3D12SubstrateRenderer — d3d12-direct host wrapper.
// All windows-rs / D3D12 / DXGI interaction is gated behind the `runtime`
// feature so the default crate build doesn't pull `windows-rs` (and the
// implicit dynamic link to `d3d12.dll` / `dxgi.dll`).
// ════════════════════════════════════════════════════════════════════════════

#[cfg(all(feature = "runtime", target_os = "windows"))]
mod d3d12_runtime;
#[cfg(all(feature = "runtime", target_os = "windows"))]
pub use d3d12_runtime::D3D12SubstrateRenderer;

// § Non-Windows or non-runtime stub — still expose the renderer-type-name
// at the crate root so downstream callers can `use cssl_host_substrate_
// render_v3_d3d12::D3D12SubstrateRenderer` without flipping features at
// the use-site. The stub holds only the artifact + tearing-policy and
// errors out on every non-trivial call. This keeps the build green on
// Linux + macOS CI runners + on Windows when `runtime` is off.
#[cfg(not(all(feature = "runtime", target_os = "windows")))]
mod d3d12_stub;
#[cfg(not(all(feature = "runtime", target_os = "windows")))]
pub use d3d12_stub::D3D12SubstrateRenderer;

// ════════════════════════════════════════════════════════════════════════════
// § Tests — structural surface only (the d3d12-direct path tests live in
// `d3d12_runtime` behind `#[cfg(...)]` so non-Windows CI sees them as a
// 0-test module).
// ════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // § Test #1 : DXBC container magic constant matches the canonical
    // value from the DirectX shader compiler header. Pure structural
    // assertion ; no D3D12 dep ; runs on every CI runner.
    #[test]
    fn dxbc_container_magic_matches_canonical() {
        // 'D','X','B','C' little-endian = 0x43425844
        assert_eq!(DXBC_CONTAINER_MAGIC, 0x4342_5844);
        let bytes = [b'D', b'X', b'B', b'C'];
        let actual = u32::from_le_bytes(bytes);
        assert_eq!(actual, DXBC_CONTAINER_MAGIC);
    }

    // § Test #2 : FRAMES_IN_FLIGHT == 3 — the fps-cap-fix policy mandates
    // triple-buffer for the DXIL host to match the L7 vulkan host
    // (`cssl-host-substrate-render-v3::present::FRAMES_IN_FLIGHT == 3`).
    // Hard-coded constant ; mismatch flags a regression in the policy.
    #[test]
    fn frames_in_flight_is_three() {
        assert_eq!(FRAMES_IN_FLIGHT, 3);
    }

    // § Test #3 : Stub artifact reports `is_stub == true` + container
    // magic == 0 + entry-name == "cs_main". Layered-construction sanity
    // for the FOUNDATION slice ; mirrors the v3-vulkan empty-shader-
    // module pattern.
    #[test]
    fn stub_artifact_has_expected_shape() {
        let a = DxilArtifact::stub();
        assert!(a.is_stub());
        assert_eq!(a.byte_len(), 0);
        assert_eq!(a.container_magic(), 0);
        assert_eq!(a.entry_name(), "cs_main");
    }

    // § Test #4 : `from_bytes` accepts arbitrary blobs (FOUNDATION-slice
    // policy ; the strict DXBC-magic check lands post-FOUNDATION when
    // `cssl-cgen-gpu-dxil` emits real containers).
    #[test]
    fn from_bytes_accepts_arbitrary_blob_in_foundation_slice() {
        // Real DXBC header prefix.
        let real = vec![b'D', b'X', b'B', b'C', 0x00, 0x01, 0x02, 0x03];
        let a = DxilArtifact::from_bytes(real);
        assert!(!a.is_stub());
        assert_eq!(a.container_magic(), DXBC_CONTAINER_MAGIC);
        assert_eq!(a.byte_len(), 8);

        // Garbage blob still accepted in FOUNDATION-slice.
        let garbage = vec![0xFFu8, 0xFE, 0xFD, 0xFC];
        let g = DxilArtifact::from_bytes(garbage);
        assert!(!g.is_stub());
        assert_ne!(g.container_magic(), DXBC_CONTAINER_MAGIC);

        // Custom entry-name flows through.
        let custom =
            DxilArtifact::from_bytes_with_entry(vec![1, 2, 3, 4], "substrate_v2_kernel_main");
        assert_eq!(custom.entry_name(), "substrate_v2_kernel_main");
    }

    // § Test #5 : Tearing-policy resolution from env-var. Default = allow
    // tearing ; `LOA_DXIL_PRESENT_TEAR=0` ⇒ vsync. Run with serial-test
    // serialization not needed because each branch sets+unsets the var
    // explicitly within the test body and we never observe cross-test
    // ordering effects (the var-name is unique to this crate).
    #[test]
    fn tearing_env_override_disables_tearing() {
        // Save + restore to avoid clobbering parallel-test env state.
        let prior = std::env::var(TEAR_ENV_VAR).ok();

        // Default (var unset) ⇒ tearing enabled.
        std::env::remove_var(TEAR_ENV_VAR);
        assert!(tearing_enabled_from_env(), "default env ⇒ tearing");
        assert_eq!(
            TearingPolicy::from_env_bool(tearing_enabled_from_env()),
            TearingPolicy::AllowTearing
        );

        // Explicit "0" ⇒ tearing disabled.
        std::env::set_var(TEAR_ENV_VAR, "0");
        assert!(!tearing_enabled_from_env());
        assert_eq!(
            TearingPolicy::from_env_bool(tearing_enabled_from_env()),
            TearingPolicy::Vsync
        );

        // "false" ⇒ tearing disabled.
        std::env::set_var(TEAR_ENV_VAR, "false");
        assert!(!tearing_enabled_from_env());

        // "off" ⇒ tearing disabled.
        std::env::set_var(TEAR_ENV_VAR, "off");
        assert!(!tearing_enabled_from_env());

        // "1" / "true" / anything else ⇒ tearing enabled.
        std::env::set_var(TEAR_ENV_VAR, "1");
        assert!(tearing_enabled_from_env());
        std::env::set_var(TEAR_ENV_VAR, "yes");
        assert!(tearing_enabled_from_env());

        // Restore.
        match prior {
            Some(v) => std::env::set_var(TEAR_ENV_VAR, v),
            None => std::env::remove_var(TEAR_ENV_VAR),
        }
    }

    // § Test #6 : `TearingPolicy::allows_tearing` discriminator is exact.
    // Pure structural assertion — guards against silent enum-variant
    // reorder regressions.
    #[test]
    fn tearing_policy_allows_tearing_discriminator() {
        assert!(TearingPolicy::AllowTearing.allows_tearing());
        assert!(!TearingPolicy::Vsync.allows_tearing());
    }

    // § Test #7 : `D3D12SubstrateRenderer::try_new` headless construction.
    // On non-Windows + non-runtime builds the stub returns `Ok` (mock
    // success) so the full layered-construction surface is exercised by
    // the workspace `cargo test --workspace` pipeline. On Windows +
    // runtime the call hits `D3D12CreateDevice` for-real and may return
    // `PresentError::DeviceCreate` on driver-less CI ; the test accepts
    // either outcome (the GPU-less skip path mirrors the v3-vulkan test
    // pattern via `try_headless_ash_renderer`).
    #[test]
    fn try_new_headless_either_succeeds_or_skips() {
        let artifact = DxilArtifact::stub();
        let result = D3D12SubstrateRenderer::try_new(artifact, (256, 256));
        match result {
            Ok(r) => {
                // Headless dispatch should also be reachable (mock or real).
                assert_eq!(r.frames_in_flight(), FRAMES_IN_FLIGHT);
                assert_eq!(r.tearing_policy(), TearingPolicy::AllowTearing);
            }
            Err(e) => {
                // GPU-less CI / non-Windows : the stub backend never
                // fails ; on real Windows the dev-machine failure-modes
                // are DeviceCreate / DxgiFactoryCreate (driver missing).
                eprintln!("try_new headless skip-path : {e}");
            }
        }
    }

    // § Test #8 : Multi-frame mock dispatch — drives 5 frames through
    // `dispatch_with_present` (mock-mode on non-Windows + non-runtime ;
    // real-mode on Windows + runtime if a real swapchain is present, but
    // this test does not construct one). Validates the per-frame ring
    // index advances modulo FRAMES_IN_FLIGHT.
    #[test]
    fn multi_frame_mock_dispatch_advances_ring() {
        let artifact = DxilArtifact::stub();
        let result = D3D12SubstrateRenderer::try_new(artifact, (256, 256));
        let Ok(mut r) = result else {
            // Real-Windows skip path (driver-less CI).
            return;
        };
        let observer = ObserverCoord::default();
        let crystals = [Crystal::default(), Crystal::default()];
        let mut prior = r.current_frame();
        for _ in 0..=(FRAMES_IN_FLIGHT * 2) {
            // 7 frames
            let _ = r.dispatch_with_present(observer, &crystals);
            let now = r.current_frame();
            assert!(now < FRAMES_IN_FLIGHT);
            assert_ne!(now, prior, "ring must advance every frame");
            prior = now;
        }
    }
}
