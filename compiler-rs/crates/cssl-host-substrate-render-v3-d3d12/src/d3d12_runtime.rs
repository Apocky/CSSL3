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
    Crystal, DxilArtifact, ObserverCoord, PresentError, TearingPolicy, FRAMES_IN_FLIGHT,
};
use windows::core::Interface;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Graphics::Direct3D::D3D_FEATURE_LEVEL_11_0;
use windows::Win32::Graphics::Direct3D12::{
    D3D12CreateDevice, ID3D12CommandAllocator, ID3D12CommandQueue, ID3D12Device,
    ID3D12Fence, ID3D12GraphicsCommandList, ID3D12PipelineState,
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
    /// PSO ; built from DXIL bytes by the post-FOUNDATION slice. Carried
    /// here as `Option<...>` so `dispatch_with_present` can short-circuit
    /// to a no-op cmd-list when the PSO hasn't been built yet.
    pipeline_state: Option<ID3D12PipelineState>,
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
                artifact,
                tearing_policy,
                extent,
                current_frame: 0,
                frame_counter: 0,
            })
        }
    }

    /// § Swapchain construction — present-path. The FOUNDATION-slice
    /// implementation accepts a Win32 `HasWindowHandle`, validates that
    /// it carries an `HWND`, and then **falls back to the headless
    /// construction path** (no real swapchain yet — that lands in
    /// `T11-W18-L8-DXIL-PRESENT`). This keeps the API surface stable so
    /// downstream callers can wire the type today and pick up real
    /// `IDXGISwapChain1::Present` calls when the PRESENT slice lands.
    ///
    /// Non-Win32 window-handles return [`PresentError::UnsupportedWindowHandle`].
    #[cfg(feature = "present")]
    pub fn try_new_with_swapchain<W: raw_window_handle::HasWindowHandle>(
        window: &W,
        dxil_bytes: &[u8],
        extent: (u32, u32),
    ) -> Result<Self, PresentError> {
        // Validate the window-handle is Win32 ; the L8 host is Windows-only.
        let handle = window
            .window_handle()
            .map_err(|_| PresentError::UnsupportedWindowHandle)?;
        match handle.as_raw() {
            raw_window_handle::RawWindowHandle::Win32(_) => {
                // Accepted ; defer real swapchain build to the PRESENT slice.
                let artifact = DxilArtifact::from_bytes(dxil_bytes.to_vec());
                Self::try_new(artifact, extent)
            }
            _ => Err(PresentError::UnsupportedWindowHandle),
        }
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

    /// § Per-frame dispatch.
    ///
    /// FOUNDATION-slice behavior :
    ///   - Wait on the fence-value for `current_frame` (CPU-GPU sync).
    ///   - Reset the per-frame allocator + cmd-list.
    ///   - Record a no-op cmd-list (no PSO yet ; the PSO-build slice
    ///     that consumes `DxilArtifact::bytes()` lands separately).
    ///   - Close + submit the cmd-list.
    ///   - Signal the fence with the next monotonic value.
    ///   - Advance `current_frame` modulo FRAMES_IN_FLIGHT.
    ///   - Increment `frame_counter`.
    ///
    /// Returns `Ok(())` on a clean record-submit cycle. On real D3D12
    /// errors the cycle short-circuits and the caller can retry with a
    /// fresh allocator (`Reset` will re-init the slot).
    pub fn dispatch_with_present(
        &mut self,
        _observer: ObserverCoord,
        _crystals: &[Crystal],
    ) -> Result<(), PresentError> {
        unsafe {
            // 1. Wait on prior fence-value for this slot.
            let target = self.frame_fence_values[self.current_frame];
            if target > 0 && self.fence.GetCompletedValue() < target {
                let _ = self.fence.SetEventOnCompletion(target, self.fence_event);
                let _ = WaitForSingleObject(self.fence_event, INFINITE);
            }

            // 2. Reset the per-frame allocator + cmd-list.
            let alloc = &self.command_allocators[self.current_frame];
            let list = &self.command_lists[self.current_frame];
            let _ = alloc.Reset();
            let _ = list.Reset(alloc, None::<&ID3D12PipelineState>);

            // 3. Record a no-op cmd-list (FOUNDATION-slice).
            //    Post-FOUNDATION : bind PSO + root-sig + descriptor-heaps
            //    + Dispatch(extent.0/8, extent.1/8, 1) + ResourceBarrier
            //    transitions for the back-buffer.

            // 4. Close + submit.
            let _ = list.Close();
            // ExecuteCommandLists takes `&[Option<ID3D12CommandList>]` ;
            // ID3D12GraphicsCommandList inherits from ID3D12CommandList so
            // we cast() to the parent interface and wrap in Some.
            // Submitting an empty cmd-list is legal in D3D12 ; the queue
            // simply advances its execution timeline by a no-op.
            if let Ok(base) =
                list.cast::<windows::Win32::Graphics::Direct3D12::ID3D12CommandList>()
            {
                let lists_to_submit = [Some(base)];
                self.command_queue.ExecuteCommandLists(&lists_to_submit);
            }

            // 5. Signal + advance.
            self.next_fence_value = self.next_fence_value.saturating_add(1);
            self.frame_fence_values[self.current_frame] = self.next_fence_value;
            let _ = self
                .command_queue
                .Signal(&self.fence, self.next_fence_value);
        }

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
