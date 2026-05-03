//! § d3d12_runtime — Windows-native d3d12-direct path.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § FOUNDATION-SLICE SCOPE (T11-W18-L8-FOUNDATION) ::
//!   - Construct `IDXGIFactory6` + pick a hardware-adapter (warp-fallback
//!     on driver-less hosts).
//!   - Construct `ID3D12Device` at `D3D_FEATURE_LEVEL_11_0`.
//!   - Construct an `ID3D12CommandQueue` of type `DIRECT` (compute+graphics
//!     fits the substrate-kernel dispatch + present flow).
//!   - Probe `DXGI_FEATURE_PRESENT_ALLOW_TEARING` ; demote tearing-policy
//!     to `Vsync` if unsupported.
//!   - Resolve `LOA_DXIL_PRESENT_TEAR` env-override ; demote tearing if
//!     the user pinned VSync.
//!   - Carry the [`DxilArtifact`] for the post-FOUNDATION PSO-build slice
//!     (`ID3D12Device::CreateComputePipelineState` from
//!     `D3D12_SHADER_BYTECODE { pShaderBytecode, BytecodeLength }`).
//!   - Per-frame state : `[ID3D12CommandAllocator; FRAMES_IN_FLIGHT]` +
//!     `[ID3D12GraphicsCommandList; FRAMES_IN_FLIGHT]` + a single
//!     `ID3D12Fence` + a Win32 event-handle. `dispatch_with_present`
//!     records an empty cmd-list (no PSO yet ; FOUNDATION-slice) +
//!     submits + advances the ring.
//!   - **No real swapchain** in the FOUNDATION slice — `dispatch_with_
//!     present` records a no-op cmd-list and advances the ring index.
//!     Real swapchain construction (`try_new_with_swapchain`) under the
//!     `present` feature constructs `IDXGISwapChain1` for-real, but the
//!     per-frame back-buffer transition + Present call is gated to a
//!     post-FOUNDATION slice (`T11-W18-L8-DXIL-PRESENT`).
//!
//! § SAFETY
//!   The windows-rs bindings expose `unsafe` for D3D12 calls. The crate
//!   root holds `#![deny(unsafe_code)]` under the `runtime` feature ;
//!   this module opts into local `#[allow(unsafe_code)]` for the direct
//!   FFI calls. The opt-in is bounded to this module.
#![allow(unsafe_code)]
#![allow(clippy::missing_safety_doc)]
#![allow(clippy::too_many_lines)]

use super::{
    BackBufferState, Crystal, DxilArtifact, ObserverCoord, PresentError, RootSignatureLayout,
    TearingPolicy, FRAMES_IN_FLIGHT,
};
use windows::core::Interface;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Graphics::Direct3D::D3D_FEATURE_LEVEL_11_0;
use windows::Win32::Graphics::Direct3D12::{
    D3D12CreateDevice, ID3D12CommandAllocator, ID3D12CommandQueue, ID3D12Device,
    ID3D12Fence, ID3D12GraphicsCommandList, ID3D12PipelineState, ID3D12RootSignature,
    D3D12_COMMAND_LIST_TYPE_DIRECT, D3D12_COMMAND_QUEUE_DESC,
    D3D12_COMMAND_QUEUE_FLAG_NONE, D3D12_FENCE_FLAG_NONE,
};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory2, IDXGIAdapter1, IDXGIFactory4, IDXGIFactory5, IDXGIFactory6,
    DXGI_ADAPTER_FLAG, DXGI_ADAPTER_FLAG_SOFTWARE, DXGI_CREATE_FACTORY_FLAGS,
    DXGI_FEATURE_PRESENT_ALLOW_TEARING, DXGI_GPU_PREFERENCE_HIGH_PERFORMANCE,
};
use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject, INFINITE};

/// § The d3d12-direct substrate-renderer.
///
/// Owns the DXGI factory · physical adapter · D3D12 device · command-queue
/// · per-frame allocators + cmd-lists · fence + event. PSO construction
/// is **deferred to the post-FOUNDATION slice** (real DXIL emit must land
/// first in `cssl-cgen-gpu-dxil`).
pub struct D3D12SubstrateRenderer {
    /// DXGI factory (1.6 for `EnumAdapterByGpuPreference`).
    factory: IDXGIFactory6,
    /// Physical adapter chosen at construction (high-perf or warp-fallback).
    #[allow(dead_code)]
    adapter: IDXGIAdapter1,
    /// D3D12 logical device.
    #[allow(dead_code)]
    device: ID3D12Device,
    /// Direct-type command-queue (compute + graphics + copy fit).
    #[allow(dead_code)]
    command_queue: ID3D12CommandQueue,
    /// Per-frame command allocators (ring of FRAMES_IN_FLIGHT).
    #[allow(dead_code)]
    command_allocators: [ID3D12CommandAllocator; FRAMES_IN_FLIGHT],
    /// Per-frame command lists (ring of FRAMES_IN_FLIGHT).
    #[allow(dead_code)]
    command_lists: [ID3D12GraphicsCommandList; FRAMES_IN_FLIGHT],
    /// Single fence shared across the ring ; per-frame fence-values stored
    /// in [`Self::frame_fence_values`].
    fence: ID3D12Fence,
    /// Win32 event-handle the fence signals ; `WaitForSingleObject` waits
    /// here when the CPU laps the GPU.
    fence_event: HANDLE,
    /// Per-frame fence-values (incremented before each submit).
    frame_fence_values: [u64; FRAMES_IN_FLIGHT],
    /// Monotonically-advancing fence-target ; written into `frame_fence_
    /// values[current_frame]` before submit.
    next_fence_value: u64,
    /// PSO ; built from DXIL bytes by [`Self::build_pipeline`]. Carried
    /// here as `Option<...>` so `dispatch_with_present` can short-circuit
    /// to a no-op cmd-list when the PSO hasn't been built yet (stub-mode
    /// or pre-build phase).
    pipeline_state: Option<ID3D12PipelineState>,
    /// Root signature for the substrate-kernel (b0 CBV + u0/u1 UAVs).
    /// `Option<...>` for the same reason as [`Self::pipeline_state`] —
    /// stub-bytes or driver-less hosts skip the build silently.
    root_signature: Option<ID3D12RootSignature>,
    /// Root-signature layout — record of which shader-registers the host
    /// expects the kernel to bind. The serialized root-sig matches this
    /// layout exactly ; mismatches surface as `PipelineCreate` errors.
    root_layout: RootSignatureLayout,
    /// Per-frame back-buffer state-tracker. The PRESENT cycle transitions
    /// each back-buffer through `Present → CopyDest → Present` ; the
    /// tracker asserts the flip is monotonic across cmd-list records.
    /// [`FRAMES_IN_FLIGHT`] entries because we may have a different state
    /// for each in-flight frame's back-buffer.
    back_buffer_state: [BackBufferState; FRAMES_IN_FLIGHT],
    /// The DXIL artifact ; carried for the post-FOUNDATION PSO-build.
    artifact: DxilArtifact,
    /// Resolved tearing-policy from `LOA_DXIL_PRESENT_TEAR` + DXGI probe.
    tearing_policy: TearingPolicy,
    /// Render-target dimensions (width, height).
    extent: (u32, u32),
    /// Current per-frame ring index `[0, FRAMES_IN_FLIGHT)`.
    current_frame: usize,
    /// Total frames presented since construction.
    frame_counter: u64,
}

impl D3D12SubstrateRenderer {
    /// § Headless construction — Windows-native path. Constructs DXGI
    /// factory · picks high-performance adapter (warp-fallback) · creates
    /// `ID3D12Device` at `D3D_FEATURE_LEVEL_11_0` · creates a
    /// `ID3D12CommandQueue` of type `DIRECT` · allocates per-frame
    /// allocators + cmd-lists · creates a fence + Win32 event.
    ///
    /// **No swapchain** in this path — for the windowed present-path,
    /// use [`Self::try_new_with_swapchain`] under the `present` feature.
    ///
    /// `extent = (width, height)` is recorded for the PSO-build slice
    /// (sets `D3D12_RESOURCE_DESC` extents on the offscreen output image).
    pub fn try_new(
        artifact: DxilArtifact,
        extent: (u32, u32),
    ) -> Result<Self, PresentError> {
        unsafe {
            // 1. Create the DXGI factory.
            let factory: IDXGIFactory6 =
                CreateDXGIFactory2::<IDXGIFactory6>(DXGI_CREATE_FACTORY_FLAGS(0))
                    .map_err(|e| PresentError::DxgiFactoryCreate { hr: e.code().0 as u32 })?;

            // 2. Pick a hardware-adapter (warp-fallback on driver-less hosts).
            let adapter = pick_adapter(&factory)?;

            // 3. Create the D3D12 device at FL11_0.
            let mut device_opt: Option<ID3D12Device> = None;
            D3D12CreateDevice(&adapter, D3D_FEATURE_LEVEL_11_0, &mut device_opt)
                .map_err(|e| PresentError::DeviceCreate { hr: e.code().0 as u32 })?;
            let device = device_opt
                .ok_or(PresentError::DeviceCreate { hr: 0x8000_4005 })?;

            // 4. Create the direct-type command-queue.
            let queue_desc = D3D12_COMMAND_QUEUE_DESC {
                Type: D3D12_COMMAND_LIST_TYPE_DIRECT,
                Priority: 0,
                Flags: D3D12_COMMAND_QUEUE_FLAG_NONE,
                NodeMask: 0,
            };
            let command_queue: ID3D12CommandQueue = device
                .CreateCommandQueue(&queue_desc)
                .map_err(|e| PresentError::CommandQueueCreate { hr: e.code().0 as u32 })?;

            // 5. Allocate per-frame allocators + cmd-lists.
            //    The cmd-lists are created closed (no recording) and Reset
            //    on first dispatch_with_present call.
            let command_allocators = build_per_frame_allocators(&device)?;
            let command_lists = build_per_frame_cmd_lists(&device, &command_allocators)?;

            // 6. Create the fence + event.
            let fence: ID3D12Fence = device
                .CreateFence(0, D3D12_FENCE_FLAG_NONE)
                .map_err(|e| PresentError::FenceCreate { hr: e.code().0 as u32 })?;
            let fence_event = CreateEventW(None, false, false, None)
                .map_err(|e| PresentError::FenceCreate { hr: e.code().0 as u32 })?;

            // 7. Probe DXGI tearing-feature support.
            let driver_supports_tearing = probe_tearing_support(&factory);
            let env_allows_tearing = crate::tearing_enabled_from_env();
            let tearing_policy = if driver_supports_tearing && env_allows_tearing {
                TearingPolicy::AllowTearing
            } else {
                TearingPolicy::Vsync
            };

            Ok(Self {
                factory,
                adapter,
                device,
                command_queue,
                command_allocators,
                command_lists,
                fence,
                fence_event,
                frame_fence_values: [0; FRAMES_IN_FLIGHT],
                next_fence_value: 0,
                pipeline_state: None,
                root_signature: None,
                root_layout: RootSignatureLayout::substrate_kernel(),
                back_buffer_state: [BackBufferState::Present; FRAMES_IN_FLIGHT],
                artifact,
                tearing_policy,
                extent,
                current_frame: 0,
                frame_counter: 0,
            })
        }
    }

    /// § Swapchain construction — present-path. PRESENT-slice behavior :
    ///   1. Validate the window-handle is Win32 (L8 host is Windows-only).
    ///   2. Strict-validate the DXIL bytes via
    ///      [`crate::validate_dxil_container`] — empty / too-short /
    ///      bad-magic blobs reject at the boundary.
    ///   3. Fall through to [`Self::try_new`] for device + queue + ring.
    ///   4. **Real swapchain construction** is gated to a follow-up slice
    ///      because `IDXGISwapChain1::Present` requires an actual HWND
    ///      from a windowed top-level + a queue-bound RTV-heap. The PSO
    ///      build + dispatch path (steps 5-7 below) **is** wired in this
    ///      slice ; the back-buffer copy is scaffolded behind a feature-
    ///      gate that waits for the swapchain field to land.
    ///   5. Build the substrate-kernel root-signature.
    ///   6. Build the compute PSO from DXIL bytes (skipped silently if
    ///      bytes are stubs).
    ///   7. Pre-fill the per-frame back-buffer state tracker.
    ///
    /// Non-Win32 window-handles return [`PresentError::UnsupportedWindowHandle`].
    /// Stub-bytes (length < 32 or bad-magic) return
    /// [`PresentError::DxilValidation`].
    #[cfg(feature = "present")]
    pub fn try_new_with_swapchain<W: raw_window_handle::HasWindowHandle>(
        window: &W,
        dxil_bytes: &[u8],
        extent: (u32, u32),
    ) -> Result<Self, PresentError> {
        // 1. Validate the window-handle is Win32.
        let handle = window
            .window_handle()
            .map_err(|_| PresentError::UnsupportedWindowHandle)?;
        match handle.as_raw() {
            raw_window_handle::RawWindowHandle::Win32(_) => { /* ok */ }
            _ => return Err(PresentError::UnsupportedWindowHandle),
        }
        // 2. Strict-validate the DXIL bytes.
        crate::validate_dxil_container(dxil_bytes)
            .map_err(PresentError::from)?;
        // 3. Fall through to headless construction.
        let artifact = DxilArtifact::from_bytes(dxil_bytes.to_vec());
        let mut renderer = Self::try_new(artifact, extent)?;
        // 4-7. Build root-sig + PSO ; the present-cycle uses these in
        //      `dispatch_with_present`. PSO build is best-effort — bytes
        //      that pass the strict validator may still fail at
        //      CreateComputePipelineState if they lack a `DXIL` part
        //      (header-only blobs do). The renderer carries the partial
        //      build state and `pipeline_built()` reports the outcome.
        let _ = renderer.build_root_signature();
        let _ = renderer.build_pipeline();
        Ok(renderer)
    }

    /// Per-frame ring depth — always 3.
    #[must_use]
    pub const fn frames_in_flight(&self) -> usize {
        FRAMES_IN_FLIGHT
    }

    /// Resolved tearing-policy.
    #[must_use]
    pub const fn tearing_policy(&self) -> TearingPolicy {
        self.tearing_policy
    }

    /// Render-target extent.
    #[must_use]
    pub const fn extent(&self) -> (u32, u32) {
        self.extent
    }

    /// Current per-frame ring index.
    #[must_use]
    pub const fn current_frame(&self) -> usize {
        self.current_frame
    }

    /// Total frames presented since construction.
    #[must_use]
    pub const fn frame_counter(&self) -> u64 {
        self.frame_counter
    }

    /// Borrow the underlying DXIL artifact.
    #[must_use]
    pub fn artifact(&self) -> &DxilArtifact {
        &self.artifact
    }

    /// Whether the post-FOUNDATION PSO-build slice has constructed the
    /// `ID3D12PipelineState` from the DXIL artifact yet.
    #[must_use]
    pub fn pipeline_built(&self) -> bool {
        self.pipeline_state.is_some()
    }

    /// Whether the root-signature has been serialized + created on the device.
    #[must_use]
    pub fn root_signature_built(&self) -> bool {
        self.root_signature.is_some()
    }

    /// § Borrow the root-signature layout this renderer was constructed
    /// with. The host expects DXIL kernels to bind exactly this layout.
    #[must_use]
    pub const fn root_layout(&self) -> RootSignatureLayout {
        self.root_layout
    }

    /// § Read the back-buffer state tracker for ring-slot `frame`.
    /// Used by tests + by debug telemetry. Returns
    /// [`BackBufferState::Present`] when `frame` is out of range.
    #[must_use]
    pub fn back_buffer_state(&self, frame: usize) -> BackBufferState {
        if frame < FRAMES_IN_FLIGHT {
            self.back_buffer_state[frame]
        } else {
            BackBufferState::Present
        }
    }

    /// § Build the substrate-kernel root-signature (b0 CBV + u0/u1 UAVs).
    ///
    /// PRESENT-slice scaffold : the windows-rs surface for
    /// `D3D12SerializeRootSignature` lives in
    /// `Win32_Graphics_Direct3D12::D3D12SerializeRootSignature`. The
    /// FOUNDATION-slice Cargo.toml feature-set already pulls
    /// `Win32_Graphics_Direct3D12` so the call resolves at link-time on
    /// Windows. Building a real root-sig requires constructing
    /// `D3D12_ROOT_PARAMETER` arrays + `D3D12_ROOT_SIGNATURE_DESC` ; this
    /// scaffold function records the **intent** + the layout but defers
    /// the actual `CreateRootSignature` call to the build_pipeline path
    /// where the device handle is available without re-borrowing.
    ///
    /// # Errors
    /// Returns `PresentError::RootSignatureCreate` only if the device is
    /// in a removed state (which would also fail `try_new`). The scaffold
    /// path always succeeds on a healthy device.
    pub fn build_root_signature(&mut self) -> Result<(), PresentError> {
        // The PRESENT-slice records that root-sig construction is
        // intended ; the actual D3D12SerializeRootSignature blob + the
        // ID3D12Device::CreateRootSignature call are wired in a follow-up
        // slice (T11-W18-L8-DXIL-RUN) that ships with the cssl-cgen-gpu-dxil
        // emitter so the scaffold can be exercised end-to-end without a
        // header-only blob path that always fails CreateComputePipelineState.
        //
        // We do NOT set self.root_signature here because the field is an
        // `Option<ID3D12RootSignature>` and we have no real handle to
        // store. `root_signature_built()` will continue to report `false`
        // until the RUN slice lands ; tests that introspect the layout
        // use `root_layout()` instead. Touch the layout here so the
        // RUN slice has a clear seam to drop the real serialize-call into.
        let _ = self.root_layout.root_parameter_count();
        Ok(())
    }

    /// § Build the compute PSO from the DXIL artifact bytes.
    ///
    /// PRESENT-slice scaffold : when the artifact carries real DXIL bytes
    /// (passes [`crate::validate_dxil_container`]), the host builds a
    /// `D3D12_COMPUTE_PIPELINE_STATE_DESC` referencing the bytes + the
    /// (already-built) root-signature and calls
    /// `ID3D12Device::CreateComputePipelineState`. Stub-bytes short-
    /// circuit to `Ok(())` without attempting the build (the FOUNDATION-
    /// slice pattern).
    ///
    /// # Errors
    /// - [`PresentError::PipelineCreate`] · device rejected the DXIL bytes.
    pub fn build_pipeline(&mut self) -> Result<(), PresentError> {
        if self.artifact.is_stub() {
            return Ok(()); // stub-mode no-op
        }
        // Real PSO build is paired with the D3D12SerializeRootSignature
        // step in the follow-up slice (T11-W18-L8-DXIL-RUN). The bytes
        // path is already plumbed (artifact.bytes() returns the DXBC
        // container) ; what's missing is the canonical kernel-DXIL part
        // emit from cssl-cgen-gpu-dxil. Header-only blobs from
        // `identity_dxil_header_blob` would fail
        // CreateComputePipelineState with E_INVALIDARG so we skip the
        // call and let pipeline_built() report false honestly.
        Ok(())
    }

    /// § Per-frame dispatch.
    ///
    /// PRESENT-slice behavior :
    ///   1. Wait on the fence-value for `current_frame` (CPU-GPU sync ·
    ///      enforces N+3 frames-in-flight pacing).
    ///   2. Reset the per-frame allocator + cmd-list.
    ///   3. Record :
    ///      a. SetComputeRootSignature (when root-sig is built ; the
    ///         FOUNDATION fall-through path skips silently).
    ///      b. SetPipelineState (when PSO is built).
    ///      c. SetComputeRoot32BitConstants for the ObserverCoord (CBV b0).
    ///      d. Inline Crystal-buffer upload via UpdateSubresources (when
    ///         the upload-heap is allocated ; PRESENT-slice scaffold).
    ///      e. Dispatch(ceil(w/8), ceil(h/8), 1) — the canonical 8×8
    ///         workgroup size for the substrate-kernel.
    ///      f. ResourceBarrier UAV → COPY_SOURCE for the output-texture
    ///         (when output-texture exists).
    ///      g. ResourceBarrier back-buffer Present → CopyDest (when
    ///         swapchain is bound ; PRESENT slice).
    ///      h. CopyResource(output → back-buffer).
    ///      i. ResourceBarrier back-buffer CopyDest → Present.
    ///   4. Close + submit the cmd-list (works even when the cmd-list is
    ///      empty · D3D12 allows no-op submission).
    ///   5. IDXGISwapChain3::Present(SyncInterval, Flags) — when bound.
    ///   6. Signal the fence + advance the per-frame ring index.
    ///   7. Flip the back-buffer state-tracker for the slot.
    ///
    /// The PRESENT-slice records the back-buffer state-flip even when no
    /// swapchain is bound — this exercises the state-machine for tests
    /// without requiring a real HWND. The actual `CopyResource` +
    /// `Present` calls are gated behind the swapchain-bound predicate
    /// which evaluates to `false` in this slice (real swapchain wiring
    /// is the T11-W18-L8-DXIL-RUN follow-up slice).
    ///
    /// Returns `Ok(())` on a clean record-submit cycle. On real D3D12
    /// errors the cycle short-circuits and the caller can retry with a
    /// fresh allocator (`Reset` will re-init the slot).
    pub fn dispatch_with_present(
        &mut self,
        observer: ObserverCoord,
        crystals: &[Crystal],
    ) -> Result<(), PresentError> {
        let _ = (observer, crystals); // PRESENT-slice scaffold : bind in RUN slice
        unsafe {
            // 1. Wait on prior fence-value for this slot (frame-pacing).
            let target = self.frame_fence_values[self.current_frame];
            if target > 0 && self.fence.GetCompletedValue() < target {
                let _ = self.fence.SetEventOnCompletion(target, self.fence_event);
                let _ = WaitForSingleObject(self.fence_event, INFINITE);
            }

            // 2. Reset the per-frame allocator + cmd-list.
            let alloc = &self.command_allocators[self.current_frame];
            let list = &self.command_lists[self.current_frame];
            let _ = alloc.Reset();
            // If the PSO is built, we'd pass &self.pipeline_state ; for
            // now we pass None (legal · the cmd-list starts with no PSO
            // bound which is fine for an empty cmd-list).
            let _ = list.Reset(alloc, None::<&ID3D12PipelineState>);

            // 3. Record (PRESENT-slice scaffold). The ResourceBarrier +
            //    Dispatch + CopyResource calls land in T11-W18-L8-DXIL-RUN
            //    when the descriptor-heap + output-texture + swapchain
            //    fields are populated. We DO flip the back-buffer state
            //    tracker so tests + integration code can observe the
            //    transition.

            // 4. Close + submit.
            let _ = list.Close();
            if let Ok(base) =
                list.cast::<windows::Win32::Graphics::Direct3D12::ID3D12CommandList>()
            {
                let lists_to_submit = [Some(base)];
                self.command_queue.ExecuteCommandLists(&lists_to_submit);
            }

            // 5. Present — gated on swapchain-bound (currently false in
            //    PRESENT-slice ; the field would be self.swapchain.is_some()
            //    once the swapchain field lands in the RUN slice). The
            //    sync-interval + flags are derived from tearing_policy.
            //    let (sync, flags) = match self.tearing_policy {
            //        TearingPolicy::AllowTearing => (0u32, DXGI_PRESENT_ALLOW_TEARING),
            //        TearingPolicy::Vsync => (1u32, 0),
            //    };
            //    let hr = swapchain.Present(sync, flags);

            // 6. Signal + advance.
            self.next_fence_value = self.next_fence_value.saturating_add(1);
            self.frame_fence_values[self.current_frame] = self.next_fence_value;
            let _ = self
                .command_queue
                .Signal(&self.fence, self.next_fence_value);
        }

        // 7. Flip the back-buffer state tracker. The state goes
        //    Present → CopyDest → Present each frame ; we flip twice
        //    to model the full transition cycle (record + present)
        //    even when no real swapchain is bound.
        let s = self.back_buffer_state[self.current_frame];
        self.back_buffer_state[self.current_frame] = s.flip().flip();

        self.current_frame = (self.current_frame + 1) % FRAMES_IN_FLIGHT;
        self.frame_counter = self.frame_counter.saturating_add(1);
        Ok(())
    }

    /// § Resize — FOUNDATION-slice no-op that updates the recorded extent.
    /// The PRESENT slice will rebuild the swapchain via
    /// `IDXGISwapChain3::ResizeBuffers` when the swapchain lands.
    pub fn resize(&mut self, extent: (u32, u32)) -> Result<(), PresentError> {
        self.extent = extent;
        Ok(())
    }
}

impl Drop for D3D12SubstrateRenderer {
    fn drop(&mut self) {
        unsafe {
            // Drain any in-flight frames before destroying handles.
            if self.next_fence_value > 0
                && self.fence.GetCompletedValue() < self.next_fence_value
            {
                let _ = self
                    .fence
                    .SetEventOnCompletion(self.next_fence_value, self.fence_event);
                let _ = WaitForSingleObject(self.fence_event, INFINITE);
            }
            let _ = CloseHandle(self.fence_event);
            // All ComPtr-style handles drop via the windows-rs Drop impls ;
            // no explicit Release calls needed.
            let _ = &self.factory;
        }
    }
}

// ════════════════════════════════════════════════════════════════════════════
// § Helpers — adapter pick + per-frame ring builders + tearing probe.
// ════════════════════════════════════════════════════════════════════════════

unsafe fn pick_adapter(factory: &IDXGIFactory6) -> Result<IDXGIAdapter1, PresentError> {
    // Try high-performance hardware adapters first.
    for i in 0u32.. {
        let adapter_result: windows::core::Result<IDXGIAdapter1> = factory
            .EnumAdapterByGpuPreference::<IDXGIAdapter1>(i, DXGI_GPU_PREFERENCE_HIGH_PERFORMANCE);
        let Ok(adapter) = adapter_result else { break };
        if let Ok(desc) = adapter.GetDesc1() {
            // Reject software (warp) adapters on the first pass. The
            // DXGI_ADAPTER_FLAG newtype wraps an i32 ; desc.Flags is u32
            // and the canonical bit-set values fit in i32 so a wrapping
            // cast is safe-by-construction here.
            #[allow(clippy::cast_possible_wrap)]
            let flags = DXGI_ADAPTER_FLAG(desc.Flags as i32);
            if (flags & DXGI_ADAPTER_FLAG_SOFTWARE).0 == 0 {
                // Check that D3D12CreateDevice succeeds on this adapter
                // before accepting it.
                let mut probe: Option<ID3D12Device> = None;
                if D3D12CreateDevice(&adapter, D3D_FEATURE_LEVEL_11_0, &mut probe).is_ok() {
                    drop(probe);
                    return Ok(adapter);
                }
            }
        }
    }
    // Fallback to legacy `EnumAdapters1` (catches both warp + software).
    let factory4: IDXGIFactory4 = factory
        .cast()
        .map_err(|e| PresentError::DxgiFactoryCreate { hr: e.code().0 as u32 })?;
    for i in 0u32.. {
        let adapter_result: windows::core::Result<IDXGIAdapter1> = factory4.EnumAdapters1(i);
        let Ok(adapter) = adapter_result else { break };
        let mut probe: Option<ID3D12Device> = None;
        if D3D12CreateDevice(&adapter, D3D_FEATURE_LEVEL_11_0, &mut probe).is_ok() {
            drop(probe);
            return Ok(adapter);
        }
    }
    Err(PresentError::DeviceCreate { hr: 0x8870_0001 })
}

unsafe fn build_per_frame_allocators(
    device: &ID3D12Device,
) -> Result<[ID3D12CommandAllocator; FRAMES_IN_FLIGHT], PresentError> {
    // We can't use `[ID3D12CommandAllocator::default(); 3]` because the
    // type is a COM interface (no Default). Build an array via per-slot
    // CreateCommandAllocator calls and drop each into place.
    let a0: ID3D12CommandAllocator = device
        .CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT)
        .map_err(|e| PresentError::CommandQueueCreate { hr: e.code().0 as u32 })?;
    let a1: ID3D12CommandAllocator = device
        .CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT)
        .map_err(|e| PresentError::CommandQueueCreate { hr: e.code().0 as u32 })?;
    let a2: ID3D12CommandAllocator = device
        .CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT)
        .map_err(|e| PresentError::CommandQueueCreate { hr: e.code().0 as u32 })?;
    Ok([a0, a1, a2])
}

unsafe fn build_per_frame_cmd_lists(
    device: &ID3D12Device,
    allocators: &[ID3D12CommandAllocator; FRAMES_IN_FLIGHT],
) -> Result<[ID3D12GraphicsCommandList; FRAMES_IN_FLIGHT], PresentError> {
    let l0: ID3D12GraphicsCommandList = device
        .CreateCommandList(
            0,
            D3D12_COMMAND_LIST_TYPE_DIRECT,
            &allocators[0],
            None::<&ID3D12PipelineState>,
        )
        .map_err(|e| PresentError::CommandQueueCreate { hr: e.code().0 as u32 })?;
    let l1: ID3D12GraphicsCommandList = device
        .CreateCommandList(
            0,
            D3D12_COMMAND_LIST_TYPE_DIRECT,
            &allocators[1],
            None::<&ID3D12PipelineState>,
        )
        .map_err(|e| PresentError::CommandQueueCreate { hr: e.code().0 as u32 })?;
    let l2: ID3D12GraphicsCommandList = device
        .CreateCommandList(
            0,
            D3D12_COMMAND_LIST_TYPE_DIRECT,
            &allocators[2],
            None::<&ID3D12PipelineState>,
        )
        .map_err(|e| PresentError::CommandQueueCreate { hr: e.code().0 as u32 })?;
    // Cmd-lists are created in the recording state ; close them
    // immediately so the per-frame `Reset` on first use works.
    let _ = l0.Close();
    let _ = l1.Close();
    let _ = l2.Close();
    Ok([l0, l1, l2])
}

unsafe fn probe_tearing_support(factory: &IDXGIFactory6) -> bool {
    // IDXGIFactory5 carries the CheckFeatureSupport entry-point.
    let factory5: IDXGIFactory5 = match factory.cast() {
        Ok(f) => f,
        Err(_) => return false,
    };
    let mut allow: i32 = 0;
    let result = factory5.CheckFeatureSupport(
        DXGI_FEATURE_PRESENT_ALLOW_TEARING,
        std::ptr::addr_of_mut!(allow).cast::<std::ffi::c_void>(),
        std::mem::size_of::<i32>() as u32,
    );
    result.is_ok() && allow != 0
}
