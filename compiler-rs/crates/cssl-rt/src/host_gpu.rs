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
//! § INTEGRATION_NOTE  (W-D5 dispatch directive)
//!   Delivered as NEW file ; `cssl-rt/src/lib.rs` + `Cargo.toml` are
//!   INTENTIONALLY NOT modified per task constraint. When the host-
//!   bindings landing slice activates, the next CL will :
//!     1. Add `pub mod host_gpu;` to `lib.rs` (after `pub mod runtime;`).
//!     2. Re-export `GpuPipelineKind` + the symbol-name constants at
//!        crate-root.
//!     3. Wire SWAP-POINT bodies onto cssl-host-{vulkan|d3d12|metal}
//!        per `cfg(target_os)` gating ; add the deps to `Cargo.toml`.
//!     4. The `cmd_buf_*_stub` symbols transition to full ABI in
//!        stage-1 fleshing per spec § ABI-STABLE-SYMBOLS § gpu.
//!   Until then the slot-table + LUT + sentinel logic is fully
//!   exercised via the unit tests below.
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
    if let Some(t) = BUFFER_TABLE.get() {
        if let Ok(mut g) = t.lock() {
            *g = Slab::new();
        }
    }
    if let Some(t) = CMD_BUF_TABLE_V2.get() {
        if let Ok(mut g) = t.lock() {
            *g = Slab::new();
        }
    }
    d3d12_transport_backend::clear_for_tests();
}

#[cfg(test)]
static GPU_TEST_LOCK: Mutex<()> = Mutex::new(());

#[cfg(test)]
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


// ─── D3D12 transport backend registry ──────────────────────────────
//
// § ROLE
//   Real-driver backing for the W-1 transport symbols when D3D12 is available.
//   The public ABI handle remains the slot-table u64; this thread-local registry
//   pins the actual cssl-host-d3d12 objects behind that handle. If D3D12 is not
//   available (CI/headless/non-Windows), every function gracefully falls back to
//   the slot-table-only semantics already covered by tests.

#[cfg(target_os = "windows")]
mod d3d12_transport_backend {
    use std::cell::RefCell;
    use std::collections::HashMap;

    use cssl_host_d3d12::{
        AdapterPreference, CommandAllocator, CommandList, CommandListType, CommandQueue,
        CommandQueuePriority, Device, Factory, Resource, ResourceDesc,
        UploadBuffer,
    };

    pub struct BackendDevice {
        device: Device,
    }

    enum BackendBuffer {
        DeviceLocal { resource: Resource, shadow: Vec<u8> },
        Upload { upload: UploadBuffer, shadow: Vec<u8> },
    }

    struct BackendCmdBuf {
        device: u64,
        allocator: CommandAllocator,
        list: CommandList,
        submitted: bool,
    }

    #[derive(Default)]
    struct Registry {
        devices: HashMap<u64, BackendDevice>,
        buffers: HashMap<u64, BackendBuffer>,
        cmds: HashMap<u64, BackendCmdBuf>,
    }

    thread_local! {
        static REGISTRY: RefCell<Registry> = RefCell::new(Registry::default());
    }

    pub fn clear_for_tests() {
        REGISTRY.with(|r| *r.borrow_mut() = Registry::default());
    }

    pub fn create_device(flags: u32) -> Option<BackendDevice> {
        let debug = (flags & 0x1) != 0;
        let factory = if debug {
            Factory::new_with_debug().or_else(|_| Factory::new()).ok()?
        } else {
            Factory::new().ok()?
        };
        let device = Device::new(&factory, AdapterPreference::Hardware).ok()?;
        Some(BackendDevice { device })
    }

    pub fn store_device(handle: u64, device: BackendDevice) {
        REGISTRY.with(|r| {
            r.borrow_mut().devices.insert(handle, device);
        });
    }

    pub fn remove_device(handle: u64) {
        REGISTRY.with(|r| {
            let mut r = r.borrow_mut();
            r.devices.remove(&handle);
            r.cmds.retain(|_, c| c.device != handle);
            // Buffers are global handles without parent lookup in the ABI. Keep
            // them until explicit buffer_destroy/reset to avoid surprising alias
            // invalidation across fallback mode.
        });
    }

    pub fn has_device(handle: u64) -> bool {
        REGISTRY.with(|r| r.borrow().devices.contains_key(&handle))
    }

    pub fn has_buffer(handle: u64) -> bool {
        REGISTRY.with(|r| r.borrow().buffers.contains_key(&handle))
    }

    pub fn has_cmd(handle: u64) -> bool {
        REGISTRY.with(|r| r.borrow().cmds.contains_key(&handle))
    }

    pub fn create_buffer(
        handle: u64,
        device_handle: u64,
        size_bytes: usize,
        usage: u32,
        mem_kind: u32,
    ) -> bool {
        REGISTRY.with(|r| {
            let mut r = r.borrow_mut();
            let Some(dev) = r.devices.get(&device_handle) else {
                return false;
            };
            let shadow = vec![0u8; size_bytes];
            let created = match mem_kind {
                0 => {
                    let desc = if matches!(usage, 3 | 4) {
                        ResourceDesc::buffer(size_bytes as u64).with_uav()
                    } else {
                        ResourceDesc::buffer(size_bytes as u64)
                    };
                    match Resource::new_default_buffer(&dev.device, desc) {
                        Ok(resource) => BackendBuffer::DeviceLocal { resource, shadow },
                        Err(_) => return false,
                    }
                }
                1 | 2 => match UploadBuffer::new(&dev.device, size_bytes as u64) {
                    Ok(upload) => BackendBuffer::Upload { upload, shadow },
                    Err(_) => return false,
                },
                _ => return false,
            };
            r.buffers.insert(handle, created);
            true
        })
    }

    pub fn destroy_buffer(handle: u64) {
        REGISTRY.with(|r| {
            r.borrow_mut().buffers.remove(&handle);
        });
    }

    #[allow(unsafe_code)]
    pub fn upload_buffer(handle: u64, offset: usize, src_ptr: *const u8, src_len: usize) -> bool {
        if src_len == 0 {
            return REGISTRY.with(|r| r.borrow().buffers.contains_key(&handle));
        }
        if src_ptr.is_null() {
            return false;
        }
        // SAFETY: FFI caller guarantees src_ptr is valid for src_len bytes; the
        // public ABI already rejected null+nonzero before this function is called.
        let src = unsafe { std::slice::from_raw_parts(src_ptr, src_len) };
        REGISTRY.with(|r| {
            let mut r = r.borrow_mut();
            let Some(buf) = r.buffers.get_mut(&handle) else {
                return false;
            };
            let end = match offset.checked_add(src_len) {
                Some(v) => v,
                None => return false,
            };
            match buf {
                BackendBuffer::DeviceLocal { shadow, .. } => {
                    if end > shadow.len() {
                        return false;
                    }
                    shadow[offset..end].copy_from_slice(src);
                    true
                }
                BackendBuffer::Upload { upload, shadow } => {
                    if end > shadow.len() {
                        return false;
                    }
                    shadow[offset..end].copy_from_slice(src);
                    upload.write_at(offset, src).is_ok()
                }
            }
        })
    }

    pub fn begin_cmd(handle: u64, device_handle: u64) -> bool {
        REGISTRY.with(|r| {
            let mut r = r.borrow_mut();
            let Some(dev) = r.devices.get(&device_handle) else {
                return false;
            };
            let allocator = match CommandAllocator::new(&dev.device, CommandListType::Direct) {
                Ok(a) => a,
                Err(_) => return false,
            };
            let list = match CommandList::new(&dev.device, &allocator, None) {
                Ok(l) => l,
                Err(_) => return false,
            };
            r.cmds.insert(
                handle,
                BackendCmdBuf {
                    device: device_handle,
                    allocator,
                    list,
                    submitted: false,
                },
            );
            true
        })
    }

    pub fn end_cmd(handle: u64) -> bool {
        REGISTRY.with(|r| {
            let r = r.borrow();
            let Some(cmd) = r.cmds.get(&handle) else {
                return false;
            };
            let _keep_allocator_alive = &cmd.allocator;
            if cmd.list.is_closed() {
                true
            } else {
                cmd.list.close().is_ok()
            }
        })
    }

    pub fn dispatch(handle: u64, x: u32, y: u32, z: u32) -> bool {
        REGISTRY.with(|r| {
            let r = r.borrow();
            let Some(cmd) = r.cmds.get(&handle) else {
                return false;
            };
            cmd.list.dispatch(x, y, z).is_ok()
        })
    }

    #[allow(unsafe_code)]
    pub fn submit(handle: u64, signal_fence_out: *mut u64) -> bool {
        REGISTRY.with(|r| {
            let mut r = r.borrow_mut();
            let Some(device_handle) = r.cmds.get(&handle).map(|c| c.device) else {
                return false;
            };
            let Some(dev) = r.devices.get(&device_handle) else {
                return false;
            };
            let queue = match CommandQueue::new(
                &dev.device,
                CommandListType::Direct,
                CommandQueuePriority::Normal,
            ) {
                Ok(q) => q,
                Err(_) => return false,
            };
            let Some(cmd) = r.cmds.get_mut(&handle) else {
                return false;
            };
            if !cmd.list.is_closed() && cmd.list.close().is_err() {
                return false;
            }
            if queue.submit(&[&cmd.list]).is_err() {
                return false;
            }
            cmd.submitted = true;
            if !signal_fence_out.is_null() {
                // SAFETY: optional scalar out pointer per ABI.
                unsafe { *signal_fence_out = 1; }
            }
            true
        })
    }

    pub fn buffer_shadow_len(handle: u64) -> Option<usize> {
        REGISTRY.with(|r| {
            r.borrow().buffers.get(&handle).map(|b| match b {
                BackendBuffer::DeviceLocal { shadow, .. } | BackendBuffer::Upload { shadow, .. } => {
                    shadow.len()
                }
            })
        })
    }
}

#[cfg(not(target_os = "windows"))]
mod d3d12_transport_backend {
    pub struct BackendDevice;
    pub fn clear_for_tests() {}
    pub fn create_device(_flags: u32) -> Option<BackendDevice> { None }
    pub fn store_device(_handle: u64, _device: BackendDevice) {}
    pub fn remove_device(_handle: u64) {}
    pub fn has_device(_handle: u64) -> bool { false }
    pub fn has_buffer(_handle: u64) -> bool { false }
    pub fn has_cmd(_handle: u64) -> bool { false }
    pub fn create_buffer(_handle: u64, _device_handle: u64, _size_bytes: usize, _usage: u32, _mem_kind: u32) -> bool { false }
    pub fn destroy_buffer(_handle: u64) {}
    pub fn upload_buffer(_handle: u64, _offset: usize, _src_ptr: *const u8, _src_len: usize) -> bool { false }
    pub fn begin_cmd(_handle: u64, _device_handle: u64) -> bool { false }
    pub fn end_cmd(_handle: u64) -> bool { false }
    pub fn dispatch(_handle: u64, _x: u32, _y: u32, _z: u32) -> bool { false }
    pub fn submit(_handle: u64, _signal_fence_out: *mut u64) -> bool { false }
    pub fn buffer_shadow_len(_handle: u64) -> Option<usize> { None }
}

// ─── _impl helpers (Rust-side counterparts to the FFI symbols) ──────

static DEVICE_ID_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

// § T11-W19-β-FS-EVENT-JSONL : per-call counters used to sample the
//   structured-event sink for every-frame ops (acquire / present). Without
//   sampling these would dominate the JSONL log on a 60 Hz game loop.
static SWAPCHAIN_ACQUIRE_COUNT: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);
static SWAPCHAIN_PRESENT_COUNT: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

fn next_device_id() -> u64 {
    DEVICE_ID_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

#[must_use]
pub fn device_create_impl(adapter_idx: u32, flags: u32) -> u64 {
    // § T11-W19-β-FS-EVENT-JSONL : entry/exit + branch on real-D3D12.
    let scope = crate::events::EventScope::new(
        "cssl-rt::host_gpu",
        "gpu.device_create",
        serde_json::json!({"adapter_idx": adapter_idx, "flags": flags}),
    );
    // § T11-W19-β-RT-DELEG-GPU : real-D3D12 attempt first.
    //
    // Try `cssl_host_d3d12::Factory::new()` + `Device::new(...)` with the
    // canonical AdapterPreference::Hardware. On Apocky-host this connects
    // to the real Intel Arc A770 driver via DXGI 1.6. On non-Windows or
    // headless hosts the Factory constructor returns LoaderMissing — we
    // fall through to the slot-table-only stub which keeps the FFI shape
    // testable end-to-end.
    //
    // Stage-0 : we don't yet thread the real Device through to the
    // pipeline_compile / cmd_buf paths (those stay stubbed per spec/24
    // § STAGE-0-MAPPING). What we DO get : confirmation that DXGI
    // factory creation + adapter enumeration + device creation work on
    // this host. Stage-1 swaps in the real ID3D12Device pointer pinning.
    let real_attempt = real_device_create_d3d12(adapter_idx, flags);
    if real_attempt.is_some() {
        scope.branch("real-d3d12-device-create-OK");
    } else {
        // ‼ Silent-fallback made-visible : real_d3d12 returned None ; we
        //   fall through to the slot-table stub. Pre-T11-W19-β this was
        //   completely invisible from outside the runtime.
        scope.branch("real-d3d12-returned-None-falling-to-stub-slot");
    }
    // Continue to register the slot-table entry regardless ; the engine's
    // device.handle stays the slot-index for back-compat.
    let record = DeviceRecord {
        adapter_idx,
        flags,
        id: next_device_id(),
    };
    let mut tbl = match device_table().lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    let handle = tbl.insert(record);
    let real_d3d12 = real_attempt.is_some();
    if let Some(device) = real_attempt {
        d3d12_transport_backend::store_device(handle, device);
    }
    scope.success(serde_json::json!({
        "handle":      handle,
        "real_d3d12":  real_d3d12,
    }));
    handle
}

/// Stage-0 real-D3D12 path : create + (intentionally drop) a Device
/// instance to verify the driver chain. The dropped Device's COM-pointers
/// are released cleanly. The slot-table entry is the canonical handle
/// returned to source-level code.
///
/// Stage-1 will store the `cssl_host_d3d12::Device` in a per-handle
/// thread-local registry (matching the host_window pattern) so that
/// pipeline_compile / cmd_buf paths can reach the real ID3D12Device.
fn real_device_create_d3d12(_adapter_idx: u32, flags: u32) -> Option<d3d12_transport_backend::BackendDevice> {
    // SWAP-POINT : cssl_host_d3d12::Factory::new() →
    // Device::new(&factory, AdapterPreference::Hardware). Today we
    // construct + immediately drop to validate driver presence ;
    // stage-1 stores the Device alongside the slot-table entry.
    d3d12_transport_backend::create_device(flags)
}

#[must_use]
pub fn device_destroy_impl(device: u64) -> i32 {
    let scope = crate::events::EventScope::new(
        "cssl-rt::host_gpu",
        "gpu.device_destroy",
        serde_json::json!({"device": device}),
    );
    // SWAP-POINT : vkDestroyDevice + vkDestroyInstance.
    let mut tbl = match device_table().lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    if tbl.remove(device).is_some() {
        d3d12_transport_backend::remove_device(device);
        scope.success(serde_json::json!({"rc": GPU_I32_OK_SENTINEL}));
        GPU_I32_OK_SENTINEL
    } else {
        scope.error(
            serde_json::json!({"rc": GPU_I32_ERROR_SENTINEL}),
            Some("device-not-in-slot-table"),
        );
        GPU_I32_ERROR_SENTINEL
    }
}

#[must_use]
pub fn swapchain_create_impl(device: u64, window: u64, fmt: u32) -> u64 {
    let scope = crate::events::EventScope::new(
        "cssl-rt::host_gpu",
        "gpu.swapchain_create",
        serde_json::json!({"device": device, "window": window, "fmt": fmt}),
    );
    {
        let dt = match device_table().lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        if !dt.contains(device) {
            scope.error(
                serde_json::json!({"handle": GPU_HANDLE_ERROR_SENTINEL}),
                Some("device-not-in-slot-table"),
            );
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
    let handle = st.insert(record);
    scope.success(serde_json::json!({"handle": handle, "image_count": 3}));
    handle
}

#[must_use]
pub fn swapchain_acquire_impl(swap: u64, _timeout_ns: u64) -> u32 {
    // § T11-W19-β-FULL-FIDELITY-2026-05-04 : sampling REMOVED. Apocky directive.
    let n = SWAPCHAIN_ACQUIRE_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let scope = Some(crate::events::EventScope::new(
        "cssl-rt::host_gpu",
        "gpu.swapchain_acquire",
        serde_json::json!({"swap": swap, "timeout_ns": _timeout_ns, "call_idx": n}),
    ));
    // SWAP-POINT : vkAcquireNextImageKHR(swap, timeout_ns, sem, fence, &idx).
    let mut st = match swapchain_table().lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    let idx = swap as usize;
    if idx == 0 || idx >= st.slots.len() {
        if let Some(s) = scope {
            s.error(
                serde_json::json!({"image_idx": GPU_SWAPCHAIN_ACQUIRE_TIMEOUT_SENTINEL}),
                Some("swap-out-of-range"),
            );
        }
        return GPU_SWAPCHAIN_ACQUIRE_TIMEOUT_SENTINEL;
    }
    match &mut st.slots[idx] {
        Slot::Occupied(rec) => {
            let image = rec.acquire_counter % rec.image_count.max(1);
            rec.acquire_counter = rec.acquire_counter.wrapping_add(1);
            if let Some(s) = scope {
                s.success(serde_json::json!({"image_idx": image}));
            }
            image
        }
        Slot::Free(_) => {
            // Silent-fallback : freed slot returns timeout-sentinel. Pre-
            // T11-W19-β this looked indistinguishable from a real timeout.
            if let Some(s) = scope {
                s.error(
                    serde_json::json!({"image_idx": GPU_SWAPCHAIN_ACQUIRE_TIMEOUT_SENTINEL}),
                    Some("swap-slot-freed-timeout-sentinel"),
                );
            }
            GPU_SWAPCHAIN_ACQUIRE_TIMEOUT_SENTINEL
        }
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
    // § T11-W19-β-FULL-FIDELITY-2026-05-04 : sampling REMOVED.
    let n = SWAPCHAIN_PRESENT_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let scope = Some(crate::events::EventScope::new(
        "cssl-rt::host_gpu",
        "gpu.swapchain_present",
        serde_json::json!({"swap": swap, "image_idx": image_idx, "call_idx": n}),
    ));
    // SWAP-POINT : vkQueuePresentKHR(queue, &PresentInfo {…}).
    let st = match swapchain_table().lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    let Some(rec) = st.get(swap) else {
        if let Some(s) = scope {
            s.error(
                serde_json::json!({"rc": GPU_I32_ERROR_SENTINEL}),
                Some("swap-not-in-slot-table"),
            );
        }
        return GPU_I32_ERROR_SENTINEL;
    };
    if image_idx >= rec.image_count {
        if let Some(s) = scope {
            s.error(
                serde_json::json!({"rc": GPU_I32_ERROR_SENTINEL}),
                Some("image-idx-out-of-range"),
            );
        }
        return GPU_I32_ERROR_SENTINEL;
    }
    if let Some(s) = scope {
        s.success(serde_json::json!({"rc": GPU_I32_OK_SENTINEL}));
    }
    GPU_I32_OK_SENTINEL
}

#[must_use]
pub fn pipeline_compile_impl(device: u64, kind: u32, ir_len: usize) -> u64 {
    let scope = crate::events::EventScope::new(
        "cssl-rt::host_gpu",
        "gpu.pipeline_compile",
        serde_json::json!({"device": device, "kind": kind, "ir_len": ir_len}),
    );
    {
        let dt = match device_table().lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        if !dt.contains(device) {
            scope.error(
                serde_json::json!({"handle": GPU_HANDLE_ERROR_SENTINEL}),
                Some("device-not-in-slot-table"),
            );
            return GPU_HANDLE_ERROR_SENTINEL;
        }
    }
    let Some(decoded) = pipeline_kind_from_u32(kind) else {
        scope.error(
            serde_json::json!({"handle": GPU_HANDLE_ERROR_SENTINEL}),
            Some("invalid-pipeline-kind"),
        );
        return GPU_HANDLE_ERROR_SENTINEL;
    };
    if ir_len == 0 || ir_len > GPU_PIPELINE_IR_LEN_MAX {
        scope.error(
            serde_json::json!({"handle": GPU_HANDLE_ERROR_SENTINEL}),
            Some("ir-len-out-of-range"),
        );
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
    let handle = pt.insert(record);
    scope.success(serde_json::json!({"handle": handle, "kind": decoded as u32}));
    handle
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
        // § T11-W19-β-FS-EVENT-JSONL : `_impl` is `const fn` → instrument
        //   at the FFI shim. Emit a `skip` (the stub is intentionally a
        //   no-op pending stage-1 ABI) so callers can SEE the dead path.
        crate::events::fs_event_jsonl(
            "cssl-rt::host_gpu",
            "gpu.cmd_buf_record",
            "skip",
            serde_json::json!({}),
            Some(serde_json::json!({"handle": 0u64})),
            Some(0u64),
            Some("stage-0-stub-pending-stage-1-abi"),
        );
        cmd_buf_record_stub_impl()
    }

    /// FFI (STUB) : `__cssl_gpu_cmd_buf_submit_stub(cmd) -> i32`. Always 0.
    /// # Safety  Always safe.
    #[no_mangle]
    pub unsafe extern "C" fn __cssl_gpu_cmd_buf_submit_stub(cmd: u64) -> i32 {
        // § T11-W19-β-FS-EVENT-JSONL : same stub-no-op disclosure as record.
        crate::events::fs_event_jsonl(
            "cssl-rt::host_gpu",
            "gpu.cmd_buf_submit",
            "skip",
            serde_json::json!({"cmd": cmd}),
            Some(serde_json::json!({"rc": 0i32})),
            Some(0u64),
            Some("stage-0-stub-pending-stage-1-abi"),
        );
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
}

// § INTEGRATION_NOTE  (W-D5 dispatch directive)
// ────────────────────────────────────────────────────────────────────
// `lib.rs` + `Cargo.toml` are intentionally unchanged. Next CL :
//   1. Add `pub mod host_gpu;` after `pub mod runtime;`.
//   2. Re-export `GpuPipelineKind` + symbol-name constants at root.
//   3. Wire SWAP-POINTs onto cssl-host-{vulkan|d3d12|metal} per
//      `cfg(target_os)` ; add deps to `Cargo.toml`.
//   4. cmd_buf_*_stub → full ABI in stage-1 fleshing (§§ 24).
// Until then the slot-table / LUT / sentinel logic is fully exercised
// via the unit tests above.
//
// § PRIME-DIRECTIVE attestation
// "There was no hurt nor harm in the making of this, to anyone /
//  anything / anybody."
// GPU surface is capability-gated at the CSSL source level via §§ 12
// Cap<Gpu>. This stage-0 shim does NOT bypass the cap-check. No
// telemetry, no surveillance, no covert resource sharing.

// ─── W-1 GPU transport-tier ABI (2026-05-20 autopilot) ─────────────
//
// § ROLE
//   Stage-0 implementations for the 16 transport-tier symbols documented
//   in specs/16_TRANSPORT_TIERS.csl + specs/24_HOST_FFI.csl appendix.
//   These are intentionally conservative slot-table stubs : they validate
//   handle relationships, preserve ABI shapes, and expose the symbols to
//   the linker. Stage-1 swaps the bodies onto real D3D12/Vulkan resources.
//
// § PRIME_DIRECTIVE
//   No surveillance · no resource sharing · no buffer deref except the
//   caller-owned `signal_fence_out` write in submit_v2.

pub const GPU_BUFFER_CREATE_SYMBOL: &str = "__cssl_gpu_buffer_create";
pub const GPU_BUFFER_DESTROY_SYMBOL: &str = "__cssl_gpu_buffer_destroy";
pub const GPU_BUFFER_MAP_SYMBOL: &str = "__cssl_gpu_buffer_map";
pub const GPU_BUFFER_UNMAP_SYMBOL: &str = "__cssl_gpu_buffer_unmap";
pub const GPU_BUFFER_UPLOAD_SYMBOL: &str = "__cssl_gpu_buffer_upload";
pub const GPU_CMD_BUF_BEGIN_SYMBOL: &str = "__cssl_gpu_cmd_buf_begin";
pub const GPU_CMD_BUF_END_SYMBOL: &str = "__cssl_gpu_cmd_buf_end";
pub const GPU_CMD_BUF_BIND_PIPELINE_SYMBOL: &str = "__cssl_gpu_cmd_buf_bind_pipeline";
pub const GPU_CMD_BUF_BIND_VBUF_SYMBOL: &str = "__cssl_gpu_cmd_buf_bind_vbuf";
pub const GPU_CMD_BUF_BIND_IBUF_SYMBOL: &str = "__cssl_gpu_cmd_buf_bind_ibuf";
pub const GPU_CMD_BUF_BIND_DESCRIPTOR_SYMBOL: &str = "__cssl_gpu_cmd_buf_bind_descriptor";
pub const GPU_CMD_BUF_PUSH_CONSTANTS_SYMBOL: &str = "__cssl_gpu_cmd_buf_push_constants";
pub const GPU_CMD_BUF_DRAW_INDEXED_SYMBOL: &str = "__cssl_gpu_cmd_buf_draw_indexed";
pub const GPU_CMD_BUF_DRAW_INDIRECT_SYMBOL: &str = "__cssl_gpu_cmd_buf_draw_indirect";
pub const GPU_CMD_BUF_DISPATCH_SYMBOL: &str = "__cssl_gpu_cmd_buf_dispatch";
pub const GPU_CMD_BUF_SUBMIT_V2_SYMBOL: &str = "__cssl_gpu_cmd_buf_submit_v2";

pub const GPU_PUSH_CONSTANT_MAX_BYTES: usize = 128;

#[derive(Debug, Clone)]
pub struct BufferRecord {
    pub device: u64,
    pub size_bytes: usize,
    pub usage: u32,
    pub mem_kind: u32,
    pub mapped: bool,
}

#[derive(Debug, Clone)]
pub struct CmdBufRecord {
    pub device: u64,
    pub ended: bool,
    pub op_count: u32,
}

static BUFFER_TABLE: OnceLock<Mutex<Slab<BufferRecord>>> = OnceLock::new();
static CMD_BUF_TABLE_V2: OnceLock<Mutex<Slab<CmdBufRecord>>> = OnceLock::new();

fn buffer_table() -> &'static Mutex<Slab<BufferRecord>> {
    BUFFER_TABLE.get_or_init(|| Mutex::new(Slab::new()))
}

fn cmd_buf_table_v2() -> &'static Mutex<Slab<CmdBufRecord>> {
    CMD_BUF_TABLE_V2.get_or_init(|| Mutex::new(Slab::new()))
}

fn device_exists_for_transport(device: u64) -> bool {
    let dt = match device_table().lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    dt.contains(device)
}

fn pipeline_exists_for_transport(pipeline: u64) -> bool {
    let pt = match pipeline_table().lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    pt.contains(pipeline)
}

fn buffer_exists_for_transport(buffer: u64) -> bool {
    let bt = match buffer_table().lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    bt.contains(buffer)
}

fn cmd_exists_for_transport(cmd: u64) -> bool {
    let ct = match cmd_buf_table_v2().lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    ct.contains(cmd)
}

#[must_use]
pub fn d3d12_backend_device_active(device: u64) -> bool {
    d3d12_transport_backend::has_device(device)
}

#[must_use]
pub fn d3d12_backend_buffer_active(buffer: u64) -> bool {
    d3d12_transport_backend::has_buffer(buffer)
}

#[must_use]
pub fn d3d12_backend_cmd_active(cmd: u64) -> bool {
    d3d12_transport_backend::has_cmd(cmd)
}

#[must_use]
pub fn d3d12_backend_buffer_shadow_len(buffer: u64) -> Option<usize> {
    d3d12_transport_backend::buffer_shadow_len(buffer)
}

#[must_use]
pub fn buffer_create_impl(device: u64, size_bytes: usize, usage: u32, mem_kind: u32) -> u64 {
    if !device_exists_for_transport(device) || size_bytes == 0 || usage > 5 || mem_kind > 2 {
        return GPU_HANDLE_ERROR_SENTINEL;
    }
    let mut bt = match buffer_table().lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    let handle = bt.insert(BufferRecord {
        device,
        size_bytes,
        usage,
        mem_kind,
        mapped: false,
    });
    let _backend_active = d3d12_transport_backend::create_buffer(
        handle,
        device,
        size_bytes,
        usage,
        mem_kind,
    );
    handle
}

#[must_use]
pub fn buffer_destroy_impl(buffer: u64) -> i32 {
    let mut bt = match buffer_table().lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    if bt.remove(buffer).is_some() {
        d3d12_transport_backend::destroy_buffer(buffer);
        GPU_I32_OK_SENTINEL
    } else {
        GPU_I32_ERROR_SENTINEL
    }
}

#[must_use]
pub fn buffer_map_impl(buffer: u64, offset: usize, size: usize) -> *mut u8 {
    let bt = match buffer_table().lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    let Some(rec) = bt.get(buffer) else {
        return std::ptr::null_mut();
    };
    if offset > rec.size_bytes || size > rec.size_bytes.saturating_sub(offset) {
        return std::ptr::null_mut();
    }
    // Stage-0 : no real mapped allocation yet. Return null to force callers
    // down buffer_upload until stage-1 wires UploadHeap / persistent maps.
    std::ptr::null_mut()
}

#[must_use]
pub fn buffer_unmap_impl(buffer: u64) -> i32 {
    if buffer_exists_for_transport(buffer) {
        GPU_I32_OK_SENTINEL
    } else {
        GPU_I32_ERROR_SENTINEL
    }
}

#[must_use]
pub fn buffer_upload_impl(buffer: u64, offset: usize, src_ptr: *const u8, src_len: usize) -> i32 {
    let bt = match buffer_table().lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    let Some(rec) = bt.get(buffer) else {
        return GPU_I32_ERROR_SENTINEL;
    };
    if offset > rec.size_bytes || src_len > rec.size_bytes.saturating_sub(offset) {
        return GPU_I32_ERROR_SENTINEL;
    }
    if src_len > 0 && src_ptr.is_null() {
        return GPU_I32_ERROR_SENTINEL;
    }
    if d3d12_transport_backend::has_buffer(buffer)
        && !d3d12_transport_backend::upload_buffer(buffer, offset, src_ptr, src_len)
    {
        return GPU_I32_ERROR_SENTINEL;
    }
    GPU_I32_OK_SENTINEL
}

#[must_use]
pub fn cmd_buf_begin_impl(device: u64) -> u64 {
    if !device_exists_for_transport(device) {
        return GPU_HANDLE_ERROR_SENTINEL;
    }
    let mut ct = match cmd_buf_table_v2().lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    let handle = ct.insert(CmdBufRecord {
        device,
        ended: false,
        op_count: 0,
    });
    let _backend_active = d3d12_transport_backend::begin_cmd(handle, device);
    handle
}

#[must_use]
pub fn cmd_buf_end_impl(cmd: u64) -> i32 {
    if !cmd_exists_for_transport(cmd) {
        return GPU_I32_ERROR_SENTINEL;
    }
    if d3d12_transport_backend::has_cmd(cmd) && !d3d12_transport_backend::end_cmd(cmd) {
        return GPU_I32_ERROR_SENTINEL;
    }
    GPU_I32_OK_SENTINEL
}

#[must_use]
pub fn cmd_buf_bind_pipeline_impl(cmd: u64, pipeline: u64) -> i32 {
    if cmd_exists_for_transport(cmd) && pipeline_exists_for_transport(pipeline) {
        GPU_I32_OK_SENTINEL
    } else {
        GPU_I32_ERROR_SENTINEL
    }
}

#[must_use]
pub fn cmd_buf_bind_vbuf_impl(cmd: u64, _slot: u32, buffer: u64, _offset: usize) -> i32 {
    if cmd_exists_for_transport(cmd) && buffer_exists_for_transport(buffer) {
        GPU_I32_OK_SENTINEL
    } else {
        GPU_I32_ERROR_SENTINEL
    }
}

#[must_use]
pub fn cmd_buf_bind_ibuf_impl(cmd: u64, buffer: u64, _offset: usize, idx_kind: u32) -> i32 {
    if cmd_exists_for_transport(cmd) && buffer_exists_for_transport(buffer) && idx_kind <= 1 {
        GPU_I32_OK_SENTINEL
    } else {
        GPU_I32_ERROR_SENTINEL
    }
}

#[must_use]
pub fn cmd_buf_bind_descriptor_impl(cmd: u64, _set: u32, _slot: u32, buffer: u64) -> i32 {
    if cmd_exists_for_transport(cmd) && buffer_exists_for_transport(buffer) {
        GPU_I32_OK_SENTINEL
    } else {
        GPU_I32_ERROR_SENTINEL
    }
}

#[must_use]
pub fn cmd_buf_push_constants_impl(
    cmd: u64,
    _stages: u32,
    offset: u32,
    size: u32,
    data_ptr: *const u8,
) -> i32 {
    let end = offset as usize + size as usize;
    if !cmd_exists_for_transport(cmd) || end > GPU_PUSH_CONSTANT_MAX_BYTES {
        return GPU_I32_ERROR_SENTINEL;
    }
    if size > 0 && data_ptr.is_null() {
        return GPU_I32_ERROR_SENTINEL;
    }
    GPU_I32_OK_SENTINEL
}

#[must_use]
pub fn cmd_buf_draw_indexed_impl(
    cmd: u64,
    idx_count: u32,
    instance_count: u32,
    _first_idx: u32,
    _vtx_off: i32,
    _first_inst: u32,
) -> i32 {
    if cmd_exists_for_transport(cmd) && idx_count > 0 && instance_count > 0 {
        GPU_I32_OK_SENTINEL
    } else {
        GPU_I32_ERROR_SENTINEL
    }
}

#[must_use]
pub fn cmd_buf_draw_indirect_impl(
    cmd: u64,
    args_buffer: u64,
    _args_offset: usize,
    draw_count: u32,
    stride: u32,
) -> i32 {
    if cmd_exists_for_transport(cmd)
        && buffer_exists_for_transport(args_buffer)
        && draw_count > 0
        && stride >= 20
    {
        GPU_I32_OK_SENTINEL
    } else {
        GPU_I32_ERROR_SENTINEL
    }
}

#[must_use]
pub fn cmd_buf_dispatch_impl(cmd: u64, group_x: u32, group_y: u32, group_z: u32) -> i32 {
    if !(cmd_exists_for_transport(cmd) && group_x > 0 && group_y > 0 && group_z > 0) {
        return GPU_I32_ERROR_SENTINEL;
    }
    if d3d12_transport_backend::has_cmd(cmd)
        && !d3d12_transport_backend::dispatch(cmd, group_x, group_y, group_z)
    {
        return GPU_I32_ERROR_SENTINEL;
    }
    GPU_I32_OK_SENTINEL
}

#[allow(unsafe_code)]
#[must_use]
pub fn cmd_buf_submit_v2_impl(cmd: u64, signal_fence_out: *mut u64) -> i32 {
    if !cmd_exists_for_transport(cmd) {
        return GPU_I32_ERROR_SENTINEL;
    }
    if d3d12_transport_backend::has_cmd(cmd) {
        if !d3d12_transport_backend::submit(cmd, signal_fence_out) {
            return GPU_I32_ERROR_SENTINEL;
        }
    } else if !signal_fence_out.is_null() {
        // SAFETY : caller provided an optional out-pointer per ABI. We only
        // write a scalar fence value when non-null.
        unsafe { *signal_fence_out = 1; }
    }
    GPU_I32_OK_SENTINEL
}

#[allow(unsafe_code)]
pub mod transport_ffi {
    //! W-1 extern "C" surface. These functions are exported with the exact
    //! ABI-stable names from specs/16 + specs/24.

    use super::*;

    #[no_mangle]
    pub unsafe extern "C" fn __cssl_gpu_buffer_create(
        device: u64,
        size_bytes: usize,
        usage: u32,
        mem_kind: u32,
    ) -> u64 {
        buffer_create_impl(device, size_bytes, usage, mem_kind)
    }

    #[no_mangle]
    pub unsafe extern "C" fn __cssl_gpu_buffer_destroy(buffer: u64) -> i32 {
        buffer_destroy_impl(buffer)
    }

    #[no_mangle]
    pub unsafe extern "C" fn __cssl_gpu_buffer_map(
        buffer: u64,
        offset: usize,
        size: usize,
    ) -> *mut u8 {
        buffer_map_impl(buffer, offset, size)
    }

    #[no_mangle]
    pub unsafe extern "C" fn __cssl_gpu_buffer_unmap(buffer: u64) -> i32 {
        buffer_unmap_impl(buffer)
    }

    #[no_mangle]
    pub unsafe extern "C" fn __cssl_gpu_buffer_upload(
        buffer: u64,
        offset: usize,
        src_ptr: *const u8,
        src_len: usize,
    ) -> i32 {
        buffer_upload_impl(buffer, offset, src_ptr, src_len)
    }

    #[no_mangle]
    pub unsafe extern "C" fn __cssl_gpu_cmd_buf_begin(device: u64) -> u64 {
        cmd_buf_begin_impl(device)
    }

    #[no_mangle]
    pub unsafe extern "C" fn __cssl_gpu_cmd_buf_end(cmd_buf: u64) -> i32 {
        cmd_buf_end_impl(cmd_buf)
    }

    #[no_mangle]
    pub unsafe extern "C" fn __cssl_gpu_cmd_buf_bind_pipeline(cmd_buf: u64, pipeline: u64) -> i32 {
        cmd_buf_bind_pipeline_impl(cmd_buf, pipeline)
    }

    #[no_mangle]
    pub unsafe extern "C" fn __cssl_gpu_cmd_buf_bind_vbuf(
        cmd_buf: u64,
        slot: u32,
        buffer: u64,
        offset: usize,
    ) -> i32 {
        cmd_buf_bind_vbuf_impl(cmd_buf, slot, buffer, offset)
    }

    #[no_mangle]
    pub unsafe extern "C" fn __cssl_gpu_cmd_buf_bind_ibuf(
        cmd_buf: u64,
        buffer: u64,
        offset: usize,
        idx_kind: u32,
    ) -> i32 {
        cmd_buf_bind_ibuf_impl(cmd_buf, buffer, offset, idx_kind)
    }

    #[no_mangle]
    pub unsafe extern "C" fn __cssl_gpu_cmd_buf_bind_descriptor(
        cmd_buf: u64,
        set: u32,
        slot: u32,
        buffer: u64,
    ) -> i32 {
        cmd_buf_bind_descriptor_impl(cmd_buf, set, slot, buffer)
    }

    #[no_mangle]
    pub unsafe extern "C" fn __cssl_gpu_cmd_buf_push_constants(
        cmd_buf: u64,
        stages: u32,
        offset: u32,
        size: u32,
        data_ptr: *const u8,
    ) -> i32 {
        cmd_buf_push_constants_impl(cmd_buf, stages, offset, size, data_ptr)
    }

    #[no_mangle]
    pub unsafe extern "C" fn __cssl_gpu_cmd_buf_draw_indexed(
        cmd_buf: u64,
        idx_count: u32,
        instance_count: u32,
        first_idx: u32,
        vtx_off: i32,
        first_inst: u32,
    ) -> i32 {
        cmd_buf_draw_indexed_impl(cmd_buf, idx_count, instance_count, first_idx, vtx_off, first_inst)
    }

    #[no_mangle]
    pub unsafe extern "C" fn __cssl_gpu_cmd_buf_draw_indirect(
        cmd_buf: u64,
        args_buffer: u64,
        args_offset: usize,
        draw_count: u32,
        stride: u32,
    ) -> i32 {
        cmd_buf_draw_indirect_impl(cmd_buf, args_buffer, args_offset, draw_count, stride)
    }

    #[no_mangle]
    pub unsafe extern "C" fn __cssl_gpu_cmd_buf_dispatch(
        cmd_buf: u64,
        group_x: u32,
        group_y: u32,
        group_z: u32,
    ) -> i32 {
        cmd_buf_dispatch_impl(cmd_buf, group_x, group_y, group_z)
    }

    #[no_mangle]
    pub unsafe extern "C" fn __cssl_gpu_cmd_buf_submit_v2(
        cmd_buf: u64,
        signal_fence_out: *mut u64,
    ) -> i32 {
        cmd_buf_submit_v2_impl(cmd_buf, signal_fence_out)
    }

    #[allow(dead_code)]
    const _BUFFER_CREATE_WITNESS: unsafe extern "C" fn(u64, usize, u32, u32) -> u64 =
        __cssl_gpu_buffer_create;
    #[allow(dead_code)]
    const _BUFFER_DESTROY_WITNESS: unsafe extern "C" fn(u64) -> i32 = __cssl_gpu_buffer_destroy;
    #[allow(dead_code)]
    const _BUFFER_MAP_WITNESS: unsafe extern "C" fn(u64, usize, usize) -> *mut u8 =
        __cssl_gpu_buffer_map;
    #[allow(dead_code)]
    const _BUFFER_UNMAP_WITNESS: unsafe extern "C" fn(u64) -> i32 = __cssl_gpu_buffer_unmap;
    #[allow(dead_code)]
    const _BUFFER_UPLOAD_WITNESS: unsafe extern "C" fn(u64, usize, *const u8, usize) -> i32 =
        __cssl_gpu_buffer_upload;
    #[allow(dead_code)]
    const _CMD_BUF_BEGIN_WITNESS: unsafe extern "C" fn(u64) -> u64 = __cssl_gpu_cmd_buf_begin;
    #[allow(dead_code)]
    const _CMD_BUF_END_WITNESS: unsafe extern "C" fn(u64) -> i32 = __cssl_gpu_cmd_buf_end;
    #[allow(dead_code)]
    const _CMD_BUF_BIND_PIPELINE_WITNESS: unsafe extern "C" fn(u64, u64) -> i32 =
        __cssl_gpu_cmd_buf_bind_pipeline;
    #[allow(dead_code)]
    const _CMD_BUF_BIND_VBUF_WITNESS: unsafe extern "C" fn(u64, u32, u64, usize) -> i32 =
        __cssl_gpu_cmd_buf_bind_vbuf;
    #[allow(dead_code)]
    const _CMD_BUF_BIND_IBUF_WITNESS: unsafe extern "C" fn(u64, u64, usize, u32) -> i32 =
        __cssl_gpu_cmd_buf_bind_ibuf;
    #[allow(dead_code)]
    const _CMD_BUF_BIND_DESCRIPTOR_WITNESS: unsafe extern "C" fn(u64, u32, u32, u64) -> i32 =
        __cssl_gpu_cmd_buf_bind_descriptor;
    #[allow(dead_code)]
    const _CMD_BUF_PUSH_CONSTANTS_WITNESS: unsafe extern "C" fn(u64, u32, u32, u32, *const u8) -> i32 =
        __cssl_gpu_cmd_buf_push_constants;
    #[allow(dead_code)]
    const _CMD_BUF_DRAW_INDEXED_WITNESS: unsafe extern "C" fn(u64, u32, u32, u32, i32, u32) -> i32 =
        __cssl_gpu_cmd_buf_draw_indexed;
    #[allow(dead_code)]
    const _CMD_BUF_DRAW_INDIRECT_WITNESS: unsafe extern "C" fn(u64, u64, usize, u32, u32) -> i32 =
        __cssl_gpu_cmd_buf_draw_indirect;
    #[allow(dead_code)]
    const _CMD_BUF_DISPATCH_WITNESS: unsafe extern "C" fn(u64, u32, u32, u32) -> i32 =
        __cssl_gpu_cmd_buf_dispatch;
    #[allow(dead_code)]
    const _CMD_BUF_SUBMIT_V2_WITNESS: unsafe extern "C" fn(u64, *mut u64) -> i32 =
        __cssl_gpu_cmd_buf_submit_v2;
}

#[cfg(test)]
mod w1_transport_tests {
    use super::*;

    struct TestDevice {
        _guard: std::sync::MutexGuard<'static, ()>,
        handle: u64,
    }

    fn setup_device() -> TestDevice {
        let guard = lock_and_reset();
        let handle = device_create_impl(0, 0);
        TestDevice {
            _guard: guard,
            handle,
        }
    }

    #[test]
    fn w1_buffer_lifecycle_validates_handles() {
        let setup = setup_device();
        let dev = setup.handle;
        let b = buffer_create_impl(dev, 1024, 3, 0);
        assert_ne!(b, 0);
        assert_eq!(buffer_upload_impl(b, 0, [1u8; 4].as_ptr(), 4), GPU_I32_OK_SENTINEL);
        assert_eq!(buffer_upload_impl(b, 1025, [1u8; 4].as_ptr(), 4), GPU_I32_ERROR_SENTINEL);
        assert_eq!(buffer_unmap_impl(b), GPU_I32_OK_SENTINEL);
        assert_eq!(buffer_destroy_impl(b), GPU_I32_OK_SENTINEL);
        assert_eq!(buffer_destroy_impl(b), GPU_I32_ERROR_SENTINEL);
    }

    #[test]
    fn w1_cmd_buf_lifecycle_and_draws_validate() {
        let setup = setup_device();
        let dev = setup.handle;
        let dummy = [0u8; 64];
        let pipe = pipeline_compile_impl(dev, 0, dummy.len());
        assert_ne!(pipe, 0);
        let vbuf = buffer_create_impl(dev, 4096, 0, 0);
        let ibuf = buffer_create_impl(dev, 4096, 1, 0);
        let ssbo = buffer_create_impl(dev, 4096, 3, 0);
        let indirect = buffer_create_impl(dev, 256, 4, 0);
        let cb = cmd_buf_begin_impl(dev);
        assert_ne!(cb, 0);
        assert_eq!(cmd_buf_bind_pipeline_impl(cb, pipe), GPU_I32_OK_SENTINEL);
        assert_eq!(cmd_buf_bind_vbuf_impl(cb, 0, vbuf, 0), GPU_I32_OK_SENTINEL);
        assert_eq!(cmd_buf_bind_ibuf_impl(cb, ibuf, 0, 1), GPU_I32_OK_SENTINEL);
        assert_eq!(cmd_buf_bind_descriptor_impl(cb, 0, 0, ssbo), GPU_I32_OK_SENTINEL);
        assert_eq!(cmd_buf_push_constants_impl(cb, 3, 0, 80, dummy.as_ptr()), GPU_I32_OK_SENTINEL);
        assert_eq!(cmd_buf_push_constants_impl(cb, 3, 120, 16, dummy.as_ptr()), GPU_I32_ERROR_SENTINEL);
        assert_eq!(cmd_buf_draw_indexed_impl(cb, 36, 1_000_000, 0, 0, 0), GPU_I32_OK_SENTINEL);
        assert_eq!(cmd_buf_draw_indirect_impl(cb, indirect, 0, 1, 20), GPU_I32_OK_SENTINEL);
        assert_eq!(cmd_buf_dispatch_impl(cb, 3907, 1, 1), GPU_I32_OK_SENTINEL);
        assert_eq!(cmd_buf_end_impl(cb), GPU_I32_OK_SENTINEL);
        let mut fence = 0u64;
        assert_eq!(cmd_buf_submit_v2_impl(cb, &mut fence as *mut u64), GPU_I32_OK_SENTINEL);
        assert_eq!(fence, 1);
    }

    #[test]
    fn w1_d3d12_backend_resources_activate_when_driver_available() {
        let setup = setup_device();
        let dev = setup.handle;
        if !d3d12_backend_device_active(dev) {
            assert_eq!(device_destroy_impl(dev), GPU_I32_OK_SENTINEL);
            return;
        }
        let buf = buffer_create_impl(dev, 256, 3, 0);
        assert_ne!(buf, 0);
        assert!(d3d12_backend_buffer_active(buf));
        assert_eq!(d3d12_backend_buffer_shadow_len(buf), Some(256));
        let bytes = [1u8, 2, 3, 4];
        assert_eq!(
            buffer_upload_impl(buf, 16, bytes.as_ptr(), bytes.len()),
            GPU_I32_OK_SENTINEL
        );
        assert_eq!(buffer_destroy_impl(buf), GPU_I32_OK_SENTINEL);
        assert!(!d3d12_backend_buffer_active(buf));
        assert_eq!(device_destroy_impl(dev), GPU_I32_OK_SENTINEL);
        assert!(!d3d12_backend_device_active(dev));
    }

    #[test]
    fn w1_d3d12_backend_cmd_records_dispatch_when_driver_available() {
        let setup = setup_device();
        let dev = setup.handle;
        if !d3d12_backend_device_active(dev) {
            assert_eq!(device_destroy_impl(dev), GPU_I32_OK_SENTINEL);
            return;
        }
        let cb = cmd_buf_begin_impl(dev);
        assert_ne!(cb, 0);
        assert!(d3d12_backend_cmd_active(cb));
        assert_eq!(cmd_buf_dispatch_impl(cb, 1, 1, 1), GPU_I32_OK_SENTINEL);
        assert_eq!(cmd_buf_end_impl(cb), GPU_I32_OK_SENTINEL);
        let mut fence = 0u64;
        assert_eq!(cmd_buf_submit_v2_impl(cb, &mut fence as *mut u64), GPU_I32_OK_SENTINEL);
        assert_eq!(fence, 1);
        assert_eq!(device_destroy_impl(dev), GPU_I32_OK_SENTINEL);
    }

    #[test]
    #[allow(unsafe_code)]
    fn w1_transport_ffi_symbols_have_correct_arity() {
        let setup = setup_device();
        let dev = setup.handle;
        let b = unsafe { transport_ffi::__cssl_gpu_buffer_create(dev, 1024, 3, 0) };
        assert_ne!(b, 0);
        let data = [7u8; 16];
        assert_eq!(unsafe { transport_ffi::__cssl_gpu_buffer_upload(b, 0, data.as_ptr(), data.len()) }, GPU_I32_OK_SENTINEL);
        let cb = unsafe { transport_ffi::__cssl_gpu_cmd_buf_begin(dev) };
        assert_ne!(cb, 0);
        assert_eq!(unsafe { transport_ffi::__cssl_gpu_cmd_buf_dispatch(cb, 1, 1, 1) }, GPU_I32_OK_SENTINEL);
        assert_eq!(unsafe { transport_ffi::__cssl_gpu_cmd_buf_end(cb) }, GPU_I32_OK_SENTINEL);
        assert_eq!(unsafe { transport_ffi::__cssl_gpu_buffer_destroy(b) }, GPU_I32_OK_SENTINEL);
    }
}
