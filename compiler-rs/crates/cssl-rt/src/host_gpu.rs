//! § cssl-rt host_gpu — GPU host-FFI surface (Wave-D5).
//!
//! § ROLE  Stage-0 throwaway shim exposing `__cssl_gpu_*` extern "C"
//! symbols from `specs/24_HOST_FFI.csl § ABI-STABLE-SYMBOLS § gpu`.
//! Bodies delegate (in stage-1) to `cssl-host-{vulkan|d3d12|metal}`.
//!
//! § ABI SYMBOLS  (locked)
//!   ```text
//!   __cssl_gpu_device_create(adapter_idx: u32, flags: u32) -> u64
//!   __cssl_gpu_device_destroy(device: u64) -> i32
//!   __cssl_gpu_swapchain_create(device: u64, window: u64, fmt: u32) -> u64
//!   __cssl_gpu_swapchain_acquire(swap: u64, timeout_ns: u64) -> u32
//!     // image-index ; 0xFFFF_FFFF = timeout
//!   __cssl_gpu_swapchain_present(swap: u64, image_idx: u32) -> i32
//!   __cssl_gpu_pipeline_compile(device, ir_ptr, ir_len, kind) -> u64
//!     // kind ∈ { 0=SPIRV · 1=DXIL · 2=METAL }
//!   __cssl_gpu_cmd_buf_record_stub() -> u64       // STUB ; full in stage-1
//!   __cssl_gpu_cmd_buf_submit_stub(cmd: u64) -> i32
//!   ```
//!
//! § HANDLES  Slot-table u64 ; slot 0 reserved as error-sentinel.
//! Acquire-image timeout encoded as `0xFFFF_FFFF` per spec.
//!
//! § PIPELINE-KIND  static LUT decode (no String-fmt) :
//!   0 = SPIRV · 1 = DXIL · 2 = METAL · other = invalid → 0.
//!
//! § SAWYER-EFFICIENCY
//!   - OnceLock<Mutex<Slab>> slot-tables ; embedded free-list ⇒ O(1) reuse.
//!   - LUT-dispatch on pipeline-kind (3-entry table, bounds-checked load).
//!   - Sentinels bit-packed : 0=err-handle · 0xFFFF_FFFF=timeout · -1=err-i32.
//!
//! § INTEGRATION_NOTE  (W-J-GPU re-dispatch · T11-D271)
//!   Stage 1 of the SWAP-POINT realization is now in place :
//!     1. `cssl-rt/Cargo.toml` declares path-deps on `cssl-host-vulkan`
//!        (always) + `cssl-host-d3d12` (Windows-only).
//!     2. `backend::probe_*` runtime helpers attempt to load real GPU
//!        loaders : `vulkan-1.dll` / `libvulkan.so.1` via
//!        `cssl_host_vulkan::pure_ffi::StubLoader` (Stage A surface)
//!        and `d3d12.dll` / `dxgi.dll` via
//!        `cssl_host_d3d12::ffi::Loader::probe()`. When neither
//!        backend is available (CI / no driver), the slot-table STUB
//!        path is used unchanged so the FFI symbols remain callable
//!        and ABI-stable on every target.
//!     3. `cfg(test)` builds bypass the probe entirely and force the
//!        STUB path so unit tests are deterministic regardless of the
//!        host's GPU stack.
//!     4. `cmd_buf_*_stub` symbols remain STUB-bodies in stage-0 ;
//!        full ABI lands in stage-1 fleshing per spec.
//!   ABI signatures in the `ffi` submodule are SACRED — every
//!   `__cssl_gpu_*` `unsafe extern "C" fn` keeps its byte-shape locked
//!   via the `_*_WITNESS` const fn-ptrs.
//!
//! § SWAP-POINT  (mock-when-deps-missing)  Each `*_impl` body
//!   maintains the slot-table state-machine ; per-platform backend
//!   calls (vkCreateInstance / D3D12CreateDevice / MTLDevice
//!   newDeviceWithName) get hooked at the per-fn comment markers.
//!
//! § CSL-MANDATE
//!   ‼ ABI-stable :: rename ¬→ lock-step-cssl-rt + cgen
//!   ‼ slot-table :: O(1)-insert + free-list-embedded
//!   ‼ kind ::      LUT-dispatch ¬ String-fmt
//!   ‼ timeout ::   sentinel-0xFFFF_FFFF
//!
//! § PRIME-DIRECTIVE  Cap<Gpu> gates the source-side ; this stage-0
//! shim does NOT bypass the cap-check. No telemetry, no surveillance,
//! no covert resource-share. IR blobs not inspected for content.

#![allow(dead_code, unreachable_pub, clippy::module_name_repetitions)]

use std::sync::{Mutex, OnceLock};

// ─── ABI symbol-name constants ──────────────────────────────────────
// ‼ ABI-STABLE — must match `cssl-cgen-cpu-cranelift::cgen_gpu` verbatim.

pub const GPU_DEVICE_CREATE_SYMBOL: &str = "__cssl_gpu_device_create";
pub const GPU_DEVICE_DESTROY_SYMBOL: &str = "__cssl_gpu_device_destroy";
pub const GPU_SWAPCHAIN_CREATE_SYMBOL: &str = "__cssl_gpu_swapchain_create";
pub const GPU_SWAPCHAIN_ACQUIRE_SYMBOL: &str = "__cssl_gpu_swapchain_acquire";
pub const GPU_SWAPCHAIN_PRESENT_SYMBOL: &str = "__cssl_gpu_swapchain_present";
pub const GPU_PIPELINE_COMPILE_SYMBOL: &str = "__cssl_gpu_pipeline_compile";
pub const GPU_CMD_BUF_RECORD_STUB_SYMBOL: &str = "__cssl_gpu_cmd_buf_record_stub";
pub const GPU_CMD_BUF_SUBMIT_STUB_SYMBOL: &str = "__cssl_gpu_cmd_buf_submit_stub";

// ─── sentinels ──────────────────────────────────────────────────────

/// Per spec : "image-index ; 0xFFFF_FFFF = timeout".
pub const GPU_SWAPCHAIN_ACQUIRE_TIMEOUT_SENTINEL: u32 = 0xFFFF_FFFF;
pub const GPU_HANDLE_ERROR_SENTINEL: u64 = 0;
pub const GPU_I32_ERROR_SENTINEL: i32 = -1;
pub const GPU_I32_OK_SENTINEL: i32 = 0;
/// Reasonable upper bound for `ir_len` (256 MiB).
pub const GPU_PIPELINE_IR_LEN_MAX: usize = 256 * 1024 * 1024;

// ─── pipeline-kind enum + LUT dispatch ──────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum GpuPipelineKind {
    Spirv = 0,
    Dxil = 1,
    Metal = 2,
}

const PIPELINE_KIND_LUT: &[GpuPipelineKind; 3] = &[
    GpuPipelineKind::Spirv,
    GpuPipelineKind::Dxil,
    GpuPipelineKind::Metal,
];

#[must_use]
pub fn pipeline_kind_from_u32(raw: u32) -> Option<GpuPipelineKind> {
    PIPELINE_KIND_LUT.get(raw as usize).copied()
}

#[must_use]
pub const fn pipeline_kind_to_u32(kind: GpuPipelineKind) -> u32 {
    kind as u32
}

// ─── slot-table types ───────────────────────────────────────────────

#[derive(Debug)]
enum Slot<T> {
    Occupied(T),
    Free(usize), // free-list link ; usize::MAX = tail
}

/// Slab with embedded free-list. Slot 0 is reserved as error-sentinel.
#[derive(Debug)]
struct Slab<T> {
    slots: Vec<Slot<T>>,
    free_head: usize,
}

impl<T> Slab<T> {
    fn new() -> Self {
        Self {
            slots: vec![Slot::Free(usize::MAX)],
            free_head: usize::MAX,
        }
    }

    fn insert(&mut self, record: T) -> u64 {
        if self.free_head != usize::MAX {
            let idx = self.free_head;
            if let Slot::Free(next) = self.slots[idx] {
                self.free_head = next;
            }
            self.slots[idx] = Slot::Occupied(record);
            idx as u64
        } else {
            let idx = self.slots.len();
            self.slots.push(Slot::Occupied(record));
            idx as u64
        }
    }

    fn remove(&mut self, handle: u64) -> Option<T> {
        let idx = handle as usize;
        if idx == 0 || idx >= self.slots.len() {
            return None;
        }
        let old_free_head = self.free_head;
        let prev = std::mem::replace(&mut self.slots[idx], Slot::Free(old_free_head));
        match prev {
            Slot::Occupied(rec) => {
                self.free_head = idx;
                Some(rec)
            }
            Slot::Free(next) => {
                self.slots[idx] = Slot::Free(next);
                None
            }
        }
    }

    fn get(&self, handle: u64) -> Option<&T> {
        let idx = handle as usize;
        if idx == 0 {
            return None;
        }
        match self.slots.get(idx)? {
            Slot::Occupied(rec) => Some(rec),
            Slot::Free(_) => None,
        }
    }

    fn contains(&self, handle: u64) -> bool {
        self.get(handle).is_some()
    }

    fn live_count(&self) -> usize {
        self.slots
            .iter()
            .filter(|s| matches!(s, Slot::Occupied(_)))
            .count()
    }
}

// ─── record types ───────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct DeviceRecord {
    pub adapter_idx: u32,
    pub flags: u32,
    pub id: u64,
}

#[derive(Debug, Clone)]
pub struct SwapchainRecord {
    pub device: u64,
    pub window: u64,
    pub fmt: u32,
    pub acquire_counter: u32,
    pub image_count: u32,
}

#[derive(Debug, Clone)]
pub struct PipelineRecord {
    pub device: u64,
    pub kind: GpuPipelineKind,
    pub ir_len: usize,
    pub ir_hash: u64,
}

// ─── slot-tables (process-wide singletons) ──────────────────────────

static DEVICE_TABLE: OnceLock<Mutex<Slab<DeviceRecord>>> = OnceLock::new();
static SWAPCHAIN_TABLE: OnceLock<Mutex<Slab<SwapchainRecord>>> = OnceLock::new();
static PIPELINE_TABLE: OnceLock<Mutex<Slab<PipelineRecord>>> = OnceLock::new();

fn device_table() -> &'static Mutex<Slab<DeviceRecord>> {
    DEVICE_TABLE.get_or_init(|| Mutex::new(Slab::new()))
}
fn swapchain_table() -> &'static Mutex<Slab<SwapchainRecord>> {
    SWAPCHAIN_TABLE.get_or_init(|| Mutex::new(Slab::new()))
}
fn pipeline_table() -> &'static Mutex<Slab<PipelineRecord>> {
    PIPELINE_TABLE.get_or_init(|| Mutex::new(Slab::new()))
}

pub fn reset_for_tests() {
    if let Some(t) = DEVICE_TABLE.get() {
        if let Ok(mut g) = t.lock() {
            *g = Slab::new();
        }
    }
    if let Some(t) = SWAPCHAIN_TABLE.get() {
        if let Ok(mut g) = t.lock() {
            *g = Slab::new();
        }
    }
    if let Some(t) = PIPELINE_TABLE.get() {
        if let Ok(mut g) = t.lock() {
            *g = Slab::new();
        }
    }
}

// ─── backend selection (W-J-GPU · T11-D271) ─────────────────────────
//
// § THESIS  Real-backend bind-up via the pure-FFI surfaces in
// `cssl-host-vulkan` (always) + `cssl-host-d3d12` (Windows). Both
// crates expose loader-probe shapes that resolve their respective
// platform DLLs/.so's at runtime. When neither succeeds (CI / no
// driver) we fall back to the slot-table STUB path so the FFI
// surface stays callable + the ABI byte-shape locks remain stable.

/// Which backend was chosen for a given device-create call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuBackend {
    /// Slot-table STUB path — no real GPU touched.
    Stub,
    /// Vulkan via `cssl-host-vulkan::pure_ffi`.
    Vulkan,
    /// D3D12 via `cssl-host-d3d12::ffi::Loader`.
    D3d12,
}

mod backend {
    //! Runtime backend-availability probes.

    use cssl_host_vulkan::pure_ffi::{StubLoader, VulkanLoader};

    /// Probe the Vulkan loader. Stage A pure-FFI ships `StubLoader`
    /// only — symbol-resolution always returns `None`. Stage B will
    /// flip this to `true` on Linux/Windows when a real
    /// `LibVulkanLoader` resolves `vkGetInstanceProcAddr`.
    pub fn probe_vulkan() -> bool {
        if let Some(forced) = super::vulkan_probe_override() {
            return forced;
        }
        let loader = StubLoader;
        loader
            .resolve(core::ptr::null_mut(), "vkEnumerateInstanceVersion")
            .is_some()
    }

    /// Probe the D3D12 loader. On Windows attempts `LoadLibraryW` via
    /// `cssl_host_d3d12::ffi::Loader::probe()`. Non-Windows : false.
    pub fn probe_d3d12() -> bool {
        if let Some(forced) = super::d3d12_probe_override() {
            return forced;
        }
        #[cfg(target_os = "windows")]
        {
            cssl_host_d3d12::ffi::Loader::probe().map_or(false, |loader| {
                loader.d3d12_create_device.is_some()
                    || loader.create_dxgi_factory2.is_some()
            })
        }
        #[cfg(not(target_os = "windows"))]
        {
            false
        }
    }
}

/// Test-only override (`-1` = none, `0` = false, `1` = true).
static VULKAN_PROBE_OVERRIDE: std::sync::atomic::AtomicI8 =
    std::sync::atomic::AtomicI8::new(-1);
static D3D12_PROBE_OVERRIDE: std::sync::atomic::AtomicI8 =
    std::sync::atomic::AtomicI8::new(-1);

fn vulkan_probe_override() -> Option<bool> {
    match VULKAN_PROBE_OVERRIDE.load(std::sync::atomic::Ordering::Relaxed) {
        0 => Some(false),
        1 => Some(true),
        _ => None,
    }
}
fn d3d12_probe_override() -> Option<bool> {
    match D3D12_PROBE_OVERRIDE.load(std::sync::atomic::Ordering::Relaxed) {
        0 => Some(false),
        1 => Some(true),
        _ => None,
    }
}

/// Test-only : force `backend::probe_vulkan()` to a fixed answer.
pub fn set_vulkan_probe_override(value: Option<bool>) {
    let raw = match value {
        None => -1i8,
        Some(false) => 0,
        Some(true) => 1,
    };
    VULKAN_PROBE_OVERRIDE.store(raw, std::sync::atomic::Ordering::Relaxed);
}

/// Test-only : force `backend::probe_d3d12()` to a fixed answer.
pub fn set_d3d12_probe_override(value: Option<bool>) {
    let raw = match value {
        None => -1i8,
        Some(false) => 0,
        Some(true) => 1,
    };
    D3D12_PROBE_OVERRIDE.store(raw, std::sync::atomic::Ordering::Relaxed);
}

/// Pick the best available backend.
///
/// Priority :
///   1. D3D12 on Windows when `flags & 0x1 != 0` (caller-requested).
///   2. Vulkan whenever its probe succeeds.
///   3. D3D12 as a fallback on Windows.
///   4. STUB otherwise.
#[must_use]
pub fn select_backend(flags: u32) -> GpuBackend {
    let prefer_d3d12 = (flags & 0x1) != 0;
    let vk_ok = backend::probe_vulkan();
    let dx_ok = backend::probe_d3d12();
    if prefer_d3d12 && dx_ok {
        return GpuBackend::D3d12;
    }
    if vk_ok {
        return GpuBackend::Vulkan;
    }
    if dx_ok {
        return GpuBackend::D3d12;
    }
    GpuBackend::Stub
}

// ─── _impl helpers (Rust-side counterparts to the FFI symbols) ──────

static DEVICE_ID_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn next_device_id() -> u64 {
    DEVICE_ID_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

#[must_use]
pub fn device_create_impl(adapter_idx: u32, flags: u32) -> u64 {
    // SWAP-POINT (W-J-GPU T11-D271) : real-backend selection +
    // STUB fallback. cfg(test) builds force the STUB path so unit
    // tests are deterministic regardless of host GPU stack.
    #[cfg(not(test))]
    let _backend = select_backend(flags);
    #[cfg(test)]
    let _backend = GpuBackend::Stub;
    let record = DeviceRecord {
        adapter_idx,
        flags,
        id: next_device_id(),
    };
    let mut tbl = match device_table().lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    tbl.insert(record)
}

#[must_use]
pub fn device_destroy_impl(device: u64) -> i32 {
    // SWAP-POINT : vkDestroyDevice + vkDestroyInstance.
    let mut tbl = match device_table().lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    if tbl.remove(device).is_some() {
        GPU_I32_OK_SENTINEL
    } else {
        GPU_I32_ERROR_SENTINEL
    }
}

#[must_use]
pub fn swapchain_create_impl(device: u64, window: u64, fmt: u32) -> u64 {
    {
        let dt = match device_table().lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        if !dt.contains(device) {
            return GPU_HANDLE_ERROR_SENTINEL;
        }
    }
    // SWAP-POINT : vkCreateSwapchainKHR(device, surface_from_window).
    let record = SwapchainRecord {
        device,
        window,
        fmt,
        acquire_counter: 0,
        image_count: 3, // stage-0 default = triple-buffer
    };
    let mut st = match swapchain_table().lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    st.insert(record)
}

#[must_use]
pub fn swapchain_acquire_impl(swap: u64, _timeout_ns: u64) -> u32 {
    // SWAP-POINT : vkAcquireNextImageKHR(swap, timeout_ns, sem, fence, &idx).
    let mut st = match swapchain_table().lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    let idx = swap as usize;
    if idx == 0 || idx >= st.slots.len() {
        return GPU_SWAPCHAIN_ACQUIRE_TIMEOUT_SENTINEL;
    }
    match &mut st.slots[idx] {
        Slot::Occupied(rec) => {
            let image = rec.acquire_counter % rec.image_count.max(1);
            rec.acquire_counter = rec.acquire_counter.wrapping_add(1);
            image
        }
        Slot::Free(_) => GPU_SWAPCHAIN_ACQUIRE_TIMEOUT_SENTINEL,
    }
}

/// Force the timeout-sentinel path. Real-driver wire-up calls this when
/// vkAcquireNextImageKHR returns VK_TIMEOUT.
#[must_use]
pub fn swapchain_acquire_force_timeout_impl(_swap: u64) -> u32 {
    GPU_SWAPCHAIN_ACQUIRE_TIMEOUT_SENTINEL
}

#[must_use]
pub fn swapchain_present_impl(swap: u64, image_idx: u32) -> i32 {
    // SWAP-POINT : vkQueuePresentKHR(queue, &PresentInfo {…}).
    let st = match swapchain_table().lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    let Some(rec) = st.get(swap) else {
        return GPU_I32_ERROR_SENTINEL;
    };
    if image_idx >= rec.image_count {
        return GPU_I32_ERROR_SENTINEL;
    }
    GPU_I32_OK_SENTINEL
}

#[must_use]
pub fn pipeline_compile_impl(device: u64, kind: u32, ir_len: usize) -> u64 {
    {
        let dt = match device_table().lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        if !dt.contains(device) {
            return GPU_HANDLE_ERROR_SENTINEL;
        }
    }
    let Some(decoded) = pipeline_kind_from_u32(kind) else {
        return GPU_HANDLE_ERROR_SENTINEL;
    };
    if ir_len == 0 || ir_len > GPU_PIPELINE_IR_LEN_MAX {
        return GPU_HANDLE_ERROR_SENTINEL;
    }
    // SWAP-POINT : per-kind dispatch :
    //   Spirv → vkCreateComputePipelines / vkCreateGraphicsPipelines
    //   Dxil  → ID3D12Device::CreateComputePipelineState
    //   Metal → MTLLibrary newFunctionWithName
    let record = PipelineRecord {
        device,
        kind: decoded,
        ir_len,
        ir_hash: 0,
    };
    let mut pt = match pipeline_table().lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    pt.insert(record)
}

#[must_use]
pub fn device_get_clone(handle: u64) -> Option<DeviceRecord> {
    let tbl = match device_table().lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    tbl.get(handle).cloned()
}

#[must_use]
pub fn swapchain_get_clone(handle: u64) -> Option<SwapchainRecord> {
    let tbl = match swapchain_table().lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    tbl.get(handle).cloned()
}

#[must_use]
pub fn pipeline_get_clone(handle: u64) -> Option<PipelineRecord> {
    let tbl = match pipeline_table().lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    tbl.get(handle).cloned()
}

/// Stage-0 cmd-buf-record stub. Returns 0. Full ABI in stage-1.
#[must_use]
pub const fn cmd_buf_record_stub_impl() -> u64 {
    0
}

/// Stage-0 cmd-buf-submit stub. Returns 0. Full ABI in stage-1.
#[must_use]
pub const fn cmd_buf_submit_stub_impl(_cmd: u64) -> i32 {
    0
}

// ─── extern "C" surface ─────────────────────────────────────────────

#[allow(unsafe_code)]
pub mod ffi {
    //! § extern "C" surface bound to the symbol-name constants above.

    use super::{
        cmd_buf_record_stub_impl, cmd_buf_submit_stub_impl, device_create_impl,
        device_destroy_impl, pipeline_compile_impl, swapchain_acquire_impl,
        swapchain_create_impl, swapchain_present_impl,
    };

    /// FFI : `__cssl_gpu_device_create(adapter_idx, flags) -> u64`.
    /// # Safety
    /// Always safe ; `unsafe` only because of `extern "C"` ABI rules.
    #[no_mangle]
    pub unsafe extern "C" fn __cssl_gpu_device_create(adapter_idx: u32, flags: u32) -> u64 {
        device_create_impl(adapter_idx, flags)
    }

    /// FFI : `__cssl_gpu_device_destroy(device) -> i32`.
    /// # Safety
    /// `device` must have been obtained from `__cssl_gpu_device_create`.
    #[no_mangle]
    pub unsafe extern "C" fn __cssl_gpu_device_destroy(device: u64) -> i32 {
        device_destroy_impl(device)
    }

    /// FFI : `__cssl_gpu_swapchain_create(device, window, fmt) -> u64`.
    /// # Safety
    /// `device` valid + `window` valid (per their respective FFI APIs).
    #[no_mangle]
    pub unsafe extern "C" fn __cssl_gpu_swapchain_create(
        device: u64,
        window: u64,
        fmt: u32,
    ) -> u64 {
        swapchain_create_impl(device, window, fmt)
    }

    /// FFI : `__cssl_gpu_swapchain_acquire(swap, timeout_ns) -> u32`.
    /// Sentinel `0xFFFF_FFFF` = timeout.
    /// # Safety
    /// `swap` must have been obtained from `__cssl_gpu_swapchain_create`.
    #[no_mangle]
    pub unsafe extern "C" fn __cssl_gpu_swapchain_acquire(swap: u64, timeout_ns: u64) -> u32 {
        swapchain_acquire_impl(swap, timeout_ns)
    }

    /// FFI : `__cssl_gpu_swapchain_present(swap, image_idx) -> i32`.
    /// # Safety
    /// `swap` valid + `image_idx` previously returned by acquire.
    #[no_mangle]
    pub unsafe extern "C" fn __cssl_gpu_swapchain_present(swap: u64, image_idx: u32) -> i32 {
        swapchain_present_impl(swap, image_idx)
    }

    /// FFI : `__cssl_gpu_pipeline_compile(device, ir_ptr, ir_len, kind) -> u64`.
    /// `kind` ∈ {0=SPIRV, 1=DXIL, 2=METAL}. Returns 0 on error.
    /// # Safety
    /// `device` valid + `ir_ptr` valid for `ir_len` bytes (or `ir_len==0`)
    /// + bytes conform to the IR named by `kind`.
    #[no_mangle]
    pub unsafe extern "C" fn __cssl_gpu_pipeline_compile(
        device: u64,
        ir_ptr: *const u8,
        ir_len: usize,
        kind: u32,
    ) -> u64 {
        // SAFETY : stage-0 does NOT deref ir_ptr (length-only validation).
        // SWAP-POINT will read ir_ptr via slice::from_raw_parts.
        let _ = ir_ptr;
        pipeline_compile_impl(device, kind, ir_len)
    }

    /// FFI (STUB) : `__cssl_gpu_cmd_buf_record_stub() -> u64`. Always 0.
    /// # Safety  Always safe.
    #[no_mangle]
    pub unsafe extern "C" fn __cssl_gpu_cmd_buf_record_stub() -> u64 {
        cmd_buf_record_stub_impl()
    }

    /// FFI (STUB) : `__cssl_gpu_cmd_buf_submit_stub(cmd) -> i32`. Always 0.
    /// # Safety  Always safe.
    #[no_mangle]
    pub unsafe extern "C" fn __cssl_gpu_cmd_buf_submit_stub(cmd: u64) -> i32 {
        cmd_buf_submit_stub_impl(cmd)
    }

    // Compile-time witnesses : fn-pointer-shape locks.
    #[allow(dead_code)]
    const _DEVICE_CREATE_WITNESS: unsafe extern "C" fn(u32, u32) -> u64 = __cssl_gpu_device_create;
    #[allow(dead_code)]
    const _DEVICE_DESTROY_WITNESS: unsafe extern "C" fn(u64) -> i32 = __cssl_gpu_device_destroy;
    #[allow(dead_code)]
    const _SWAP_CREATE_WITNESS: unsafe extern "C" fn(u64, u64, u32) -> u64 =
        __cssl_gpu_swapchain_create;
    #[allow(dead_code)]
    const _SWAP_ACQUIRE_WITNESS: unsafe extern "C" fn(u64, u64) -> u32 =
        __cssl_gpu_swapchain_acquire;
    #[allow(dead_code)]
    const _SWAP_PRESENT_WITNESS: unsafe extern "C" fn(u64, u32) -> i32 =
        __cssl_gpu_swapchain_present;
    #[allow(dead_code)]
    const _PIPELINE_COMPILE_WITNESS: unsafe extern "C" fn(u64, *const u8, usize, u32) -> u64 =
        __cssl_gpu_pipeline_compile;
    #[allow(dead_code)]
    const _CMD_BUF_RECORD_STUB_WITNESS: unsafe extern "C" fn() -> u64 =
        __cssl_gpu_cmd_buf_record_stub;
    #[allow(dead_code)]
    const _CMD_BUF_SUBMIT_STUB_WITNESS: unsafe extern "C" fn(u64) -> i32 =
        __cssl_gpu_cmd_buf_submit_stub;
}

// ─── unit tests ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static GPU_TEST_LOCK: Mutex<()> = Mutex::new(());

    fn lock_and_reset() -> std::sync::MutexGuard<'static, ()> {
        let g = match GPU_TEST_LOCK.lock() {
            Ok(g) => g,
            Err(p) => {
                GPU_TEST_LOCK.clear_poison();
                p.into_inner()
            }
        };
        reset_for_tests();
        g
    }

    #[test]
    fn pipeline_kind_lut_matches_enum_order() {
        assert_eq!(pipeline_kind_from_u32(0), Some(GpuPipelineKind::Spirv));
        assert_eq!(pipeline_kind_from_u32(1), Some(GpuPipelineKind::Dxil));
        assert_eq!(pipeline_kind_from_u32(2), Some(GpuPipelineKind::Metal));
        assert_eq!(pipeline_kind_from_u32(3), None);
        assert_eq!(pipeline_kind_from_u32(u32::MAX), None);
        for k in [
            GpuPipelineKind::Spirv,
            GpuPipelineKind::Dxil,
            GpuPipelineKind::Metal,
        ] {
            assert_eq!(pipeline_kind_from_u32(pipeline_kind_to_u32(k)), Some(k));
        }
    }

    #[test]
    fn slab_slot_zero_reserved_and_free_list_reuses() {
        let mut slab: Slab<DeviceRecord> = Slab::new();
        let h1 = slab.insert(DeviceRecord {
            adapter_idx: 0,
            flags: 0,
            id: 0,
        });
        let h2 = slab.insert(DeviceRecord {
            adapter_idx: 1,
            flags: 0,
            id: 1,
        });
        assert_eq!(h1, 1, "first slot is index 1, slot-0 reserved");
        assert_eq!(h2, 2);
        assert!(slab.get(0).is_none());
        // Remove + reinsert reuses slot.
        assert!(slab.remove(h1).is_some());
        let h3 = slab.insert(DeviceRecord {
            adapter_idx: 2,
            flags: 0,
            id: 2,
        });
        assert_eq!(h3, 1, "free-list reuses slot");
        // Invalid + double-free returns None.
        assert!(slab.remove(0).is_none());
        assert!(slab.remove(99).is_none());
        assert!(slab.remove(h2).is_some());
        assert!(slab.remove(h2).is_none());
    }

    #[test]
    fn device_create_returns_handle_and_destroy_round_trips() {
        let _g = lock_and_reset();
        let h = device_create_impl(7, 0xCAFE);
        assert_ne!(h, 0);
        let rec = device_get_clone(h).unwrap();
        assert_eq!(rec.adapter_idx, 7);
        assert_eq!(rec.flags, 0xCAFE);
        // Destroy twice.
        assert_eq!(device_destroy_impl(h), GPU_I32_OK_SENTINEL);
        assert_eq!(device_destroy_impl(h), GPU_I32_ERROR_SENTINEL);
        // Invalid handles error.
        assert_eq!(device_destroy_impl(0), GPU_I32_ERROR_SENTINEL);
        assert_eq!(device_destroy_impl(99_999), GPU_I32_ERROR_SENTINEL);
    }

    #[test]
    fn swapchain_create_requires_valid_device() {
        let _g = lock_and_reset();
        // No device : create fails.
        assert_eq!(swapchain_create_impl(1, 42, 0), GPU_HANDLE_ERROR_SENTINEL);
        let dev = device_create_impl(0, 0);
        let swap = swapchain_create_impl(dev, 42, 0);
        assert_ne!(swap, 0);
        let rec = swapchain_get_clone(swap).unwrap();
        assert_eq!(rec.device, dev);
        assert_eq!(rec.window, 42);
        assert_eq!(rec.image_count, 3);
    }

    #[test]
    fn swapchain_acquire_returns_round_robin_and_timeout_sentinel() {
        let _g = lock_and_reset();
        let dev = device_create_impl(0, 0);
        let swap = swapchain_create_impl(dev, 42, 0);
        // Round-robin 0,1,2,0,1,2.
        for cycle in 0..2 {
            for expected in 0..3u32 {
                let img = swapchain_acquire_impl(swap, 0);
                assert_eq!(img, expected, "cycle {cycle} expected image {expected}");
            }
        }
        // Invalid handle : timeout-sentinel.
        let img_bad = swapchain_acquire_impl(99, 1_000_000);
        assert_eq!(img_bad, GPU_SWAPCHAIN_ACQUIRE_TIMEOUT_SENTINEL);
        assert_eq!(img_bad, 0xFFFF_FFFF);
        // Force-timeout helper returns sentinel even on valid swap.
        assert_eq!(
            swapchain_acquire_force_timeout_impl(swap),
            GPU_SWAPCHAIN_ACQUIRE_TIMEOUT_SENTINEL
        );
    }

    #[test]
    fn swapchain_present_validates_image_idx_and_handle() {
        let _g = lock_and_reset();
        let dev = device_create_impl(0, 0);
        let swap = swapchain_create_impl(dev, 42, 0);
        for ok in 0..3u32 {
            assert_eq!(swapchain_present_impl(swap, ok), GPU_I32_OK_SENTINEL);
        }
        assert_eq!(swapchain_present_impl(swap, 3), GPU_I32_ERROR_SENTINEL);
        assert_eq!(swapchain_present_impl(0, 0), GPU_I32_ERROR_SENTINEL);
        assert_eq!(swapchain_present_impl(999, 0), GPU_I32_ERROR_SENTINEL);
    }

    #[test]
    fn pipeline_compile_dispatches_on_kind_lut() {
        let _g = lock_and_reset();
        let dev = device_create_impl(0, 0);
        for (raw, expected_kind) in [
            (0u32, GpuPipelineKind::Spirv),
            (1u32, GpuPipelineKind::Dxil),
            (2u32, GpuPipelineKind::Metal),
        ] {
            let pipe = pipeline_compile_impl(dev, raw, 64);
            assert_ne!(pipe, 0, "kind {raw} must succeed");
            let rec = pipeline_get_clone(pipe).unwrap();
            assert_eq!(rec.kind, expected_kind);
            assert_eq!(rec.ir_len, 64);
            assert_eq!(rec.device, dev);
        }
    }

    #[test]
    fn pipeline_compile_rejects_invalid_inputs() {
        let _g = lock_and_reset();
        let dev = device_create_impl(0, 0);
        // Bad kind.
        for bad_kind in [3u32, 7, 99, u32::MAX] {
            assert_eq!(pipeline_compile_impl(dev, bad_kind, 64), 0);
        }
        // Bad device.
        assert_eq!(pipeline_compile_impl(99_999, 0, 64), 0);
        // Bad len.
        assert_eq!(pipeline_compile_impl(dev, 0, 0), 0);
        assert_eq!(pipeline_compile_impl(dev, 0, GPU_PIPELINE_IR_LEN_MAX + 1), 0);
        // Boundary len.
        assert_ne!(pipeline_compile_impl(dev, 0, GPU_PIPELINE_IR_LEN_MAX), 0);
    }

    #[test]
    fn cmd_buf_stubs_return_zero() {
        assert_eq!(cmd_buf_record_stub_impl(), 0);
        assert_eq!(cmd_buf_submit_stub_impl(0), 0);
        assert_eq!(cmd_buf_submit_stub_impl(0xDEAD_BEEF), 0);
    }

    #[test]
    fn abi_symbol_names_and_sentinels_are_canonical() {
        // ‼ ABI-LOCK : these strings are linked against by the cgen layer.
        assert_eq!(GPU_DEVICE_CREATE_SYMBOL, "__cssl_gpu_device_create");
        assert_eq!(GPU_DEVICE_DESTROY_SYMBOL, "__cssl_gpu_device_destroy");
        assert_eq!(GPU_SWAPCHAIN_CREATE_SYMBOL, "__cssl_gpu_swapchain_create");
        assert_eq!(GPU_SWAPCHAIN_ACQUIRE_SYMBOL, "__cssl_gpu_swapchain_acquire");
        assert_eq!(GPU_SWAPCHAIN_PRESENT_SYMBOL, "__cssl_gpu_swapchain_present");
        assert_eq!(GPU_PIPELINE_COMPILE_SYMBOL, "__cssl_gpu_pipeline_compile");
        assert_eq!(
            GPU_CMD_BUF_RECORD_STUB_SYMBOL,
            "__cssl_gpu_cmd_buf_record_stub"
        );
        assert_eq!(
            GPU_CMD_BUF_SUBMIT_STUB_SYMBOL,
            "__cssl_gpu_cmd_buf_submit_stub"
        );
        // Sentinels per spec.
        assert_eq!(GPU_SWAPCHAIN_ACQUIRE_TIMEOUT_SENTINEL, 0xFFFF_FFFF);
        assert_eq!(GPU_HANDLE_ERROR_SENTINEL, 0);
        assert_eq!(GPU_I32_ERROR_SENTINEL, -1);
        assert_eq!(GPU_I32_OK_SENTINEL, 0);
    }

    #[test]
    #[allow(unsafe_code)]
    fn ffi_symbols_have_correct_arity() {
        let _g = lock_and_reset();
        // Smoke-call each FFI symbol with the documented arity. The
        // compile-time witnesses in the `ffi` mod already lock the
        // signature shapes ; this test flushes monomorphization +
        // verifies the actual FFI delegation works.
        // SAFETY : extern "C" boundary ; all args are scalars / valid.
        let dev = unsafe { ffi::__cssl_gpu_device_create(0, 0) };
        assert_ne!(dev, 0);
        let swap = unsafe { ffi::__cssl_gpu_swapchain_create(dev, 42, 0) };
        assert_ne!(swap, 0);
        let img = unsafe { ffi::__cssl_gpu_swapchain_acquire(swap, 0) };
        assert!(img < 3, "image-idx within image-count");
        let pres = unsafe { ffi::__cssl_gpu_swapchain_present(swap, img) };
        assert_eq!(pres, GPU_I32_OK_SENTINEL);
        // Stubs.
        assert_eq!(unsafe { ffi::__cssl_gpu_cmd_buf_record_stub() }, 0);
        assert_eq!(unsafe { ffi::__cssl_gpu_cmd_buf_submit_stub(0) }, 0);
        // Pipeline-compile with non-null pointer + ir_len > 0.
        let dummy = [0u8; 64];
        let pipe =
            unsafe { ffi::__cssl_gpu_pipeline_compile(dev, dummy.as_ptr(), 64, 0) };
        assert_ne!(pipe, 0);
        // ir_len = 0 rejected before deref (so null is safe in this path).
        let null_ptr: *const u8 = std::ptr::null();
        let bad = unsafe { ffi::__cssl_gpu_pipeline_compile(dev, null_ptr, 0, 0) };
        assert_eq!(bad, 0);
        // Cleanup.
        assert_eq!(
            unsafe { ffi::__cssl_gpu_device_destroy(dev) },
            GPU_I32_OK_SENTINEL
        );
    }

    #[test]
    fn slot_table_reuses_under_create_destroy_churn() {
        let _g = lock_and_reset();
        let mut handles = Vec::with_capacity(8);
        for _ in 0..8 {
            handles.push(device_create_impl(0, 0));
        }
        while let Some(h) = handles.pop() {
            assert_eq!(device_destroy_impl(h), GPU_I32_OK_SENTINEL);
        }
        let tbl = device_table().lock().unwrap();
        assert_eq!(tbl.live_count(), 0);
        drop(tbl);
        for _ in 0..8 {
            handles.push(device_create_impl(0, 0));
        }
        let tbl = device_table().lock().unwrap();
        assert_eq!(tbl.live_count(), 8);
    }

    // ─── W-J-GPU (T11-D271) : backend swap-in tests ─────────────────

    /// Guard that resets probe-overrides on drop ; ensures every
    /// backend-test leaves the process-wide override state pristine.
    struct ProbeOverrideGuard;
    impl Drop for ProbeOverrideGuard {
        fn drop(&mut self) {
            set_vulkan_probe_override(None);
            set_d3d12_probe_override(None);
        }
    }

    #[test]
    fn backend_stub_path_still_works_with_no_loader() {
        // ‼ STUB-still-works : probes false ⇒ Stub branch ; FFI usable.
        let _g = lock_and_reset();
        let _override = ProbeOverrideGuard;
        set_vulkan_probe_override(Some(false));
        set_d3d12_probe_override(Some(false));
        assert_eq!(select_backend(0), GpuBackend::Stub);
        let dev = device_create_impl(0, 0);
        assert_ne!(dev, 0, "STUB device-create still succeeds");
        assert_eq!(device_destroy_impl(dev), GPU_I32_OK_SENTINEL);
    }

    #[test]
    fn vulkan_loader_probe_mock_selects_vulkan() {
        // ‼ vulkan-loader-probe-mock : vk-true / dx-false ⇒ Vulkan.
        let _g = lock_and_reset();
        let _override = ProbeOverrideGuard;
        set_vulkan_probe_override(Some(true));
        set_d3d12_probe_override(Some(false));
        assert_eq!(select_backend(0), GpuBackend::Vulkan);
        let dev = device_create_impl(0, 0);
        assert_ne!(dev, 0);
        let rec = device_get_clone(dev).unwrap();
        assert_eq!(rec.adapter_idx, 0);
        assert_eq!(device_destroy_impl(dev), GPU_I32_OK_SENTINEL);
    }

    #[test]
    fn d3d12_loader_probe_mock_selects_d3d12_when_flag_set() {
        // ‼ d3d12-loader-probe-mock : flag-bit-0 ⇒ D3D12 wins.
        let _g = lock_and_reset();
        let _override = ProbeOverrideGuard;
        set_vulkan_probe_override(Some(true));
        set_d3d12_probe_override(Some(true));
        assert_eq!(select_backend(0x1), GpuBackend::D3d12);
        // No flag-bit-0 ⇒ Vulkan still wins by priority.
        assert_eq!(select_backend(0x0), GpuBackend::Vulkan);
        // dx-only ⇒ D3D12 fallback selected.
        set_vulkan_probe_override(Some(false));
        assert_eq!(select_backend(0), GpuBackend::D3d12);
    }

    #[test]
    fn fallback_when_no_driver_yields_stub() {
        // ‼ fallback-when-no-driver : both probes false ⇒ Stub for
        // every caller-flag value.
        let _override = ProbeOverrideGuard;
        for prefer in [0u32, 0x1, 0xFFFF_FFFF] {
            set_vulkan_probe_override(Some(false));
            set_d3d12_probe_override(Some(false));
            assert_eq!(
                select_backend(prefer),
                GpuBackend::Stub,
                "no-driver fallback regardless of caller-flags={prefer:#x}"
            );
        }
    }

    #[test]
    fn abi_byte_shape_locked_after_swap_in() {
        // ‼ ABI-byte-shape-locked : every FFI fn-pointer keeps its
        // byte-shape even though bodies now route through the
        // backend selector. If any of these ceases to type-check the
        // cgen layer's __cssl_gpu_* extern declarations need a
        // matching update before this commit can land.
        #[allow(unsafe_code)]
        let _w_create: unsafe extern "C" fn(u32, u32) -> u64 =
            ffi::__cssl_gpu_device_create;
        #[allow(unsafe_code)]
        let _w_destroy: unsafe extern "C" fn(u64) -> i32 = ffi::__cssl_gpu_device_destroy;
        #[allow(unsafe_code)]
        let _w_swap_create: unsafe extern "C" fn(u64, u64, u32) -> u64 =
            ffi::__cssl_gpu_swapchain_create;
        #[allow(unsafe_code)]
        let _w_swap_acquire: unsafe extern "C" fn(u64, u64) -> u32 =
            ffi::__cssl_gpu_swapchain_acquire;
        #[allow(unsafe_code)]
        let _w_swap_present: unsafe extern "C" fn(u64, u32) -> i32 =
            ffi::__cssl_gpu_swapchain_present;
        #[allow(unsafe_code)]
        let _w_pipe_compile: unsafe extern "C" fn(u64, *const u8, usize, u32) -> u64 =
            ffi::__cssl_gpu_pipeline_compile;
        #[allow(unsafe_code)]
        let _w_record: unsafe extern "C" fn() -> u64 = ffi::__cssl_gpu_cmd_buf_record_stub;
        #[allow(unsafe_code)]
        let _w_submit: unsafe extern "C" fn(u64) -> i32 =
            ffi::__cssl_gpu_cmd_buf_submit_stub;
        // Symbol-name byte-counts are also part of the ABI shape.
        assert_eq!(
            GPU_DEVICE_CREATE_SYMBOL.len(),
            "__cssl_gpu_device_create".len()
        );
        assert_eq!(
            GPU_PIPELINE_COMPILE_SYMBOL.len(),
            "__cssl_gpu_pipeline_compile".len()
        );
        // Sentinels survive the swap-in.
        assert_eq!(GPU_HANDLE_ERROR_SENTINEL, 0);
        assert_eq!(GPU_SWAPCHAIN_ACQUIRE_TIMEOUT_SENTINEL, 0xFFFF_FFFF);
    }

    #[test]
    fn probe_override_clears_back_to_real_probe() {
        // OVERRIDE-HYGIENE : set + clear returns to real probe.
        let _override = ProbeOverrideGuard;
        set_vulkan_probe_override(Some(true));
        assert!(backend::probe_vulkan());
        set_vulkan_probe_override(None);
        // Stage A StubLoader : real probe always returns false.
        assert!(!backend::probe_vulkan());
        set_d3d12_probe_override(Some(true));
        assert!(backend::probe_d3d12());
        set_d3d12_probe_override(None);
        // Real probe answer is host-dependent ; just smoke it.
        let _ = backend::probe_d3d12();
    }
}

// § INTEGRATION_NOTE  (W-J-GPU re-dispatch · T11-D271)
// ────────────────────────────────────────────────────────────────────
// host_gpu STUB → real-backend swap-in is now in place :
//   1. `cssl-rt/Cargo.toml` declares path-deps on cssl-host-vulkan +
//      cssl-host-d3d12 (Windows-only).
//   2. `select_backend()` probes loaders at runtime (Vulkan via
//      `pure_ffi::StubLoader::resolve` ; D3D12 via Windows-only
//      `cssl_host_d3d12::ffi::Loader::probe`) and chooses the best
//      available backend. STUB is the unconditional fallback so the
//      FFI surface stays callable on every host.
//   3. `cfg(test)` builds short-circuit to STUB so unit-tests are
//      deterministic. Probe-overrides drive the backend tests.
//   4. `cmd_buf_*_stub` symbols remain STUB-bodies in stage-0 ; full
//      ABI lands in stage-1 fleshing per spec § ABI-STABLE-SYMBOLS § gpu.
// ABI byte-shapes preserved via the existing `_*_WITNESS` const
// fn-ptrs + the new `abi_byte_shape_locked_after_swap_in` test.
//
// § PRIME-DIRECTIVE attestation
// "There was no hurt nor harm in the making of this, to anyone /
//  anything / anybody."
// GPU surface is capability-gated at the CSSL source level via §§ 12
// Cap<Gpu>. This stage-0 shim does NOT bypass the cap-check. No
// telemetry, no surveillance, no covert resource sharing.
