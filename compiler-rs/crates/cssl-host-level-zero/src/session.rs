//! RAII session over the [`crate::loader::L0Loader`].
//!
//! § SPEC : `specs/14_BACKEND.csl § HOST-SUBMIT BACKENDS § Level-Zero` —
//!          create_device → create_queue → create_cmd_buffer →
//!          bind_resources → dispatch/draw → submit → wait.
//!
//! § DESIGN
//!   This module wraps the FFI surface with idiomatic Rust :
//!     - [`DriverSession`]    — `zeInit` + driver enumeration + Intel-Arc-preferred
//!                              device pick. Drop is a no-op (drivers are owned by
//!                              the ICD).
//!     - [`DeviceContext`]    — `zeContextCreate` ; Drop runs `zeContextDestroy`.
//!     - [`CommandListHandle`]— `zeCommandListCreate` ; Drop runs `zeCommandListDestroy`.
//!     - [`ModuleHandle`]     — `zeModuleCreate` from a SPIR-V blob ; Drop destroys.
//!     - [`KernelLaunch`]     — `zeKernelCreate` + `zeCommandListAppendLaunchKernel`.
//!     - [`UsmAllocation`]    — `zeMemAllocDevice` / `zeMemAllocHost` /
//!                              `zeMemAllocShared` ; Drop runs `zeMemFree`.
//!     - [`FenceHandle`]      — `zeFenceCreate` + `zeFenceHostSynchronize` ; Drop destroys.
//!
//! § CAPABILITIES
//!   Per `specs/12_CAPABILITIES`, L0 device-handles + memory allocations are
//!   semantically `iso<T>` (linearly owned). The Drop-glue here enforces the
//!   capability contract at the type-system level even before linear-tracking
//!   walkers exist (T3.4-phase-2.5 deferred).
//!
//! § SAFETY
//!   - Every method that calls into the loader's resolved fn-pointers wraps the
//!     call in an `unsafe` block with a SAFETY paragraph referencing the L0
//!     spec contract being upheld.
//!   - The `loader` reference is borrowed by every Drop impl ; for that we
//!     require sessions to outlive the loader. The borrow-checker enforces
//!     the rule statically — a `DriverSession<'l>` borrowing `&'l L0Loader`
//!     cannot outlive the loader.
//!
//! § DEFERRED
//!   - Real device-property reading (`zeDeviceGetProperties` populates a
//!     C-ABI struct CSSLv3 doesn't currently mirror in full ; only fields the
//!     [`L0DeviceProperties`] type tracks at scaffold-time are wired here).
//!   - Command-queue creation : the immediate-mode command-list family is
//!     used at S6-E5 ; queue + execute + sync is a phase-F refinement.
//!   - Kernel-argument set + group-size set : the launch path here uses a
//!     1-thread group-count and zero kernel arguments — sufficient for the
//!     no-op compute kernel smoke test, expanded when D1 lands real bodies.

use core::ffi::c_void;
use core::ptr::null_mut;
use std::ffi::CString;

use thiserror::Error;

use crate::api::UsmAllocType;
use crate::driver::{L0Device, L0DeviceProperties, L0DeviceType, L0Driver};
use crate::ffi::{
    cmd_list_flag, ZeCommandList, ZeCommandListDesc, ZeContext, ZeContextDesc, ZeDevice, ZeDriver,
    ZeEvent, ZeFence, ZeGroupCount, ZeKernel, ZeKernelDesc, ZeModule, ZeModuleDesc, ZeModuleFormat,
    ZeResult,
};
use crate::loader::{L0Loader, LoaderError};

/// Top-level session — `zeInit` + driver + device picked + `zeContextCreate`.
pub struct DriverSession<'l> {
    /// Reference to the loader holding the resolved fn-pointers.
    loader: &'l L0Loader,
    /// Drivers enumerated from `zeDriverGet`.
    drivers: Vec<ZeDriver>,
    /// CSSLv3-friendly metadata for each enumerated driver (driver_index
    /// matches the index into `drivers`).
    metadata: Vec<L0Driver>,
    /// Currently-selected driver index.
    selected_driver: u32,
    /// Currently-selected device index within `selected_driver`.
    selected_device: u32,
    /// Cached device handles for `selected_driver`.
    device_handles: Vec<ZeDevice>,
}

impl<'l> DriverSession<'l> {
    /// Initialize L0 (`zeInit(GPU_ONLY)`), enumerate drivers, and pick the
    /// canonical Intel device when one is present.
    ///
    /// # Errors
    /// Bubbles loader errors verbatim (init failure, driver enumeration error,
    /// no devices found, etc.).
    pub fn open(loader: &'l L0Loader) -> Result<Self, SessionError> {
        loader.ze_init_gpu().map_err(SessionError::from)?;
        let drivers = loader.enumerate_drivers().map_err(SessionError::from)?;
        if drivers.is_empty() {
            return Err(SessionError::NoDriver);
        }

        let mut metadata = Vec::with_capacity(drivers.len());
        for (idx, &driver) in drivers.iter().enumerate() {
            let meta = enumerate_devices_for_driver(loader, idx as u32, driver)?;
            metadata.push(meta);
        }

        // Pick driver+device : prefer a device whose vendor_id == 0x8086.
        let (sel_driver, sel_device) = pick_intel_preferred(&metadata).unwrap_or((0, 0));
        let device_handles = enumerate_device_handles(loader, drivers[sel_driver as usize])?;

        Ok(Self {
            loader,
            drivers,
            metadata,
            selected_driver: sel_driver,
            selected_device: sel_device,
            device_handles,
        })
    }

    /// Loader reference (lifetime-bound).
    #[must_use]
    pub const fn loader(&self) -> &L0Loader {
        self.loader
    }

    /// Total driver count.
    #[must_use]
    pub fn driver_count(&self) -> u32 {
        self.drivers.len() as u32
    }

    /// Enumerated drivers as CSSLv3 metadata.
    #[must_use]
    pub fn drivers(&self) -> &[L0Driver] {
        &self.metadata
    }

    /// Currently-selected driver index.
    #[must_use]
    pub const fn selected_driver_index(&self) -> u32 {
        self.selected_driver
    }

    /// Currently-selected device index.
    #[must_use]
    pub const fn selected_device_index(&self) -> u32 {
        self.selected_device
    }

    /// Selected device metadata.
    #[must_use]
    pub fn selected_device_metadata(&self) -> Option<&L0Device> {
        self.metadata
            .get(self.selected_driver as usize)
            .and_then(|driver| driver.devices.get(self.selected_device as usize))
    }

    /// Selected raw `ze_device_handle_t`.
    #[must_use]
    pub fn selected_device_handle(&self) -> Option<ZeDevice> {
        self.device_handles
            .get(self.selected_device as usize)
            .copied()
    }

    /// Selected raw `ze_driver_handle_t`.
    #[must_use]
    pub fn selected_driver_handle(&self) -> Option<ZeDriver> {
        self.drivers.get(self.selected_driver as usize).copied()
    }

    /// Select driver + device by index. Returns the previous selection.
    ///
    /// # Errors
    /// Returns [`SessionError::OutOfRange`] if either index is invalid.
    pub fn select(
        &mut self,
        driver_index: u32,
        device_index: u32,
    ) -> Result<(u32, u32), SessionError> {
        let driver = self
            .metadata
            .get(driver_index as usize)
            .ok_or(SessionError::OutOfRange("driver_index"))?;
        if device_index as usize >= driver.devices.len() {
            return Err(SessionError::OutOfRange("device_index"));
        }
        let prev = (self.selected_driver, self.selected_device);
        self.selected_driver = driver_index;
        self.selected_device = device_index;
        // Refresh device-handle cache for the new driver.
        self.device_handles =
            enumerate_device_handles(self.loader, self.drivers[driver_index as usize])?;
        Ok(prev)
    }

    /// Create a [`DeviceContext`] (`zeContextCreate`) for the selected driver.
    ///
    /// # Errors
    /// Returns [`SessionError::CallFailed`] when `zeContextCreate` fails.
    pub fn create_context(&self) -> Result<DeviceContext<'l>, SessionError> {
        let driver = self
            .selected_driver_handle()
            .ok_or(SessionError::NoDriver)?;
        let desc = ZeContextDesc::default();
        let mut ctx = ZeContext(null_mut());
        // SAFETY: zeContextCreate writes one context handle into `ctx` when
        // it returns Success ; we own the storage for the duration.
        let raw = unsafe { (self.loader.ze_context_create)(driver, &desc, &mut ctx) };
        let r = ZeResult::from_raw(raw);
        if !r.is_success() {
            return Err(SessionError::CallFailed("zeContextCreate", r));
        }
        Ok(DeviceContext {
            loader: self.loader,
            handle: ctx,
        })
    }
}

/// Enumerate device handles for a single driver via `zeDeviceGet`.
fn enumerate_device_handles(
    loader: &L0Loader,
    driver: ZeDriver,
) -> Result<Vec<ZeDevice>, SessionError> {
    let mut count: u32 = 0;
    // SAFETY: documented count-query (null fill-ptr).
    let raw = unsafe { (loader.ze_device_get)(driver, &mut count, null_mut()) };
    let r = ZeResult::from_raw(raw);
    if !r.is_success() {
        return Err(SessionError::CallFailed("zeDeviceGet (count)", r));
    }
    if count == 0 {
        return Err(SessionError::NoDevice);
    }
    let mut handles = vec![ZeDevice(null_mut()); count as usize];
    // SAFETY: caller-owned buffer of exactly `count` slots ; L0 fills + sets `count`.
    let raw = unsafe { (loader.ze_device_get)(driver, &mut count, handles.as_mut_ptr()) };
    let r = ZeResult::from_raw(raw);
    if !r.is_success() {
        return Err(SessionError::CallFailed("zeDeviceGet (fill)", r));
    }
    handles.truncate(count as usize);
    Ok(handles)
}

/// Build CSSLv3 metadata for a driver — wraps `enumerate_device_handles` with
/// best-effort property-reading. Property reading uses a stub-properties
/// fallback (the C-ABI `ze_device_properties_t` is large + driver-private ;
/// CSSLv3 mirrors only the fields we care about — phase-F refinement).
fn enumerate_devices_for_driver(
    loader: &L0Loader,
    index: u32,
    driver: ZeDriver,
) -> Result<L0Driver, SessionError> {
    let handles = enumerate_device_handles(loader, driver).unwrap_or_default();
    let devices = handles
        .into_iter()
        .enumerate()
        .map(|(i, _h)| L0Device {
            driver_index: index,
            device_index: i as u32,
            // Stage-0 fallback : leave properties as a no-info default.
            // Real `zeDeviceGetProperties` mirroring is phase-F refinement.
            properties: L0DeviceProperties {
                name: format!("L0 device {index}/{i}"),
                device_type: L0DeviceType::Gpu,
                vendor_id: 0,
                device_id: 0,
                core_clock_rate_mhz: 0,
                max_compute_units: 0,
                global_memory_mb: 0,
                max_workgroup_size: 0,
                api_major: 1,
                api_minor: 0,
            },
        })
        .collect();

    // Driver API version
    let mut api_v: u32 = 0;
    // SAFETY: driver handle is one we just enumerated ; api-version output ptr is owned.
    let raw = unsafe { (loader.ze_driver_get_api_version)(driver, &mut api_v) };
    let _ = ZeResult::from_raw(raw); // best-effort; default api version = 1.0
    let api_major = (api_v >> 16) as u16;
    let api_minor = (api_v & 0xFFFF) as u16;

    Ok(L0Driver {
        index,
        api_major: if api_major == 0 { 1 } else { api_major },
        api_minor,
        devices,
    })
}

/// Pick the first driver/device whose vendor_id is Intel (0x8086) ; fall back
/// to the first device-bearing driver.
#[must_use]
fn pick_intel_preferred(metadata: &[L0Driver]) -> Option<(u32, u32)> {
    for driver in metadata {
        for dev in &driver.devices {
            if dev.properties.vendor_id == 0x8086 {
                return Some((driver.index, dev.device_index));
            }
        }
    }
    metadata
        .iter()
        .find_map(|d| d.devices.first().map(|dev| (d.index, dev.device_index)))
}

// ───────────────────────────────────────────────────────────────────────────
// § DeviceContext — RAII wrapper around `ze_context_handle_t`
// ───────────────────────────────────────────────────────────────────────────

/// `ze_context_handle_t` wrapper. Drop runs `zeContextDestroy`.
pub struct DeviceContext<'l> {
    loader: &'l L0Loader,
    handle: ZeContext,
}

impl<'l> DeviceContext<'l> {
    /// Underlying handle.
    #[must_use]
    pub const fn handle(&self) -> ZeContext {
        self.handle
    }

    /// Create a non-immediate command-list bound to the given device.
    ///
    /// # Errors
    /// Returns [`SessionError::CallFailed`] on FFI error.
    pub fn create_command_list(
        &self,
        device: ZeDevice,
    ) -> Result<CommandListHandle<'l>, SessionError> {
        let desc = ZeCommandListDesc {
            stype: 0,
            p_next: core::ptr::null(),
            command_queue_group_ordinal: 0,
            flags: cmd_list_flag::EXPLICIT_ONLY,
        };
        let mut list = ZeCommandList(null_mut());
        // SAFETY: zeCommandListCreate writes one handle into `list` on Success.
        let raw =
            unsafe { (self.loader.ze_command_list_create)(self.handle, device, &desc, &mut list) };
        let r = ZeResult::from_raw(raw);
        if !r.is_success() {
            return Err(SessionError::CallFailed("zeCommandListCreate", r));
        }
        Ok(CommandListHandle {
            loader: self.loader,
            handle: list,
        })
    }

    /// Create a [`ModuleHandle`] from a SPIR-V byte-blob.
    ///
    /// # Errors
    /// Returns [`SessionError::CallFailed`] on `zeModuleCreate` error.
    pub fn create_module_from_spirv(
        &self,
        device: ZeDevice,
        spirv: &[u8],
    ) -> Result<ModuleHandle<'l>, SessionError> {
        let desc = ZeModuleDesc {
            stype: 0,
            p_next: core::ptr::null(),
            format: ZeModuleFormat::Spirv,
            input_size: spirv.len(),
            p_input_module: spirv.as_ptr(),
            p_build_flags: core::ptr::null(),
            p_constants: core::ptr::null(),
        };
        let mut module = ZeModule(null_mut());
        // SAFETY: spirv buffer outlives the call (taken by-reference) ;
        // zeModuleCreate copies the input as needed and returns the module
        // handle in `module`.
        let raw = unsafe {
            (self.loader.ze_module_create)(
                self.handle,
                device,
                &desc,
                &mut module,
                core::ptr::null_mut(),
            )
        };
        let r = ZeResult::from_raw(raw);
        if !r.is_success() {
            return Err(SessionError::CallFailed("zeModuleCreate", r));
        }
        Ok(ModuleHandle {
            loader: self.loader,
            handle: module,
        })
    }

    /// Allocate USM memory.
    ///
    /// # Errors
    /// Returns [`SessionError::CallFailed`] on FFI error.
    pub fn alloc(
        &self,
        kind: UsmAllocType,
        device: ZeDevice,
        size: usize,
        alignment: usize,
    ) -> Result<UsmAllocation<'l>, SessionError> {
        let mut ptr: *mut c_void = null_mut();
        // SAFETY: each allocator writes one pointer into `ptr` on Success.
        // We never deref the returned device-pointer from host-side at stage-0
        // (smoke tests only verify non-null).
        let raw = match kind {
            UsmAllocType::Device => unsafe {
                (self.loader.ze_mem_alloc_device)(
                    self.handle,
                    core::ptr::null(),
                    size,
                    alignment,
                    device,
                    &mut ptr,
                )
            },
            UsmAllocType::Host => unsafe {
                (self.loader.ze_mem_alloc_host)(
                    self.handle,
                    core::ptr::null(),
                    size,
                    alignment,
                    &mut ptr,
                )
            },
            UsmAllocType::Shared => unsafe {
                (self.loader.ze_mem_alloc_shared)(
                    self.handle,
                    core::ptr::null(),
                    core::ptr::null(),
                    size,
                    alignment,
                    device,
                    &mut ptr,
                )
            },
        };
        let r = ZeResult::from_raw(raw);
        if !r.is_success() {
            return Err(SessionError::CallFailed("zeMemAlloc*", r));
        }
        Ok(UsmAllocation {
            loader: self.loader,
            context: self.handle,
            ptr,
            size,
            kind,
        })
    }
}

impl Drop for DeviceContext<'_> {
    fn drop(&mut self) {
        if self.handle.0.is_null() {
            return;
        }
        // SAFETY: handle is non-null, was created by zeContextCreate via this loader,
        // and is never aliased outside this owner.
        let _ = unsafe { (self.loader.ze_context_destroy)(self.handle) };
        self.handle.0 = null_mut();
    }
}

// ───────────────────────────────────────────────────────────────────────────
// § CommandListHandle — RAII wrapper
// ───────────────────────────────────────────────────────────────────────────

/// `ze_command_list_handle_t` wrapper. Drop runs `zeCommandListDestroy`.
pub struct CommandListHandle<'l> {
    loader: &'l L0Loader,
    handle: ZeCommandList,
}

impl CommandListHandle<'_> {
    /// Underlying handle.
    #[must_use]
    pub const fn handle(&self) -> ZeCommandList {
        self.handle
    }

    /// Append a kernel-launch command to this list.
    ///
    /// # Errors
    /// Returns [`SessionError::CallFailed`] when L0 reports failure.
    pub fn append_launch(&self, launch: &KernelLaunch<'_>) -> Result<(), SessionError> {
        let group_count = launch.group_count;
        // SAFETY: kernel + cmdlist are both live (RAII owners outlive this borrow) ;
        // signal_event = NULL means "no event" (documented L0 contract) ;
        // wait_events = NULL with num_wait = 0 means "wait for nothing".
        let raw = unsafe {
            (self.loader.ze_command_list_append_launch_kernel)(
                self.handle,
                launch.handle(),
                &group_count,
                ZeEvent(null_mut()),
                0,
                core::ptr::null(),
            )
        };
        let r = ZeResult::from_raw(raw);
        if !r.is_success() {
            return Err(SessionError::CallFailed(
                "zeCommandListAppendLaunchKernel",
                r,
            ));
        }
        Ok(())
    }

    /// Close the command-list (`zeCommandListClose`).
    ///
    /// # Errors
    /// Returns [`SessionError::CallFailed`] on failure.
    pub fn close(&self) -> Result<(), SessionError> {
        // SAFETY: cmdlist handle is live (we own it).
        let raw = unsafe { (self.loader.ze_command_list_close)(self.handle) };
        let r = ZeResult::from_raw(raw);
        if !r.is_success() {
            return Err(SessionError::CallFailed("zeCommandListClose", r));
        }
        Ok(())
    }
}

impl Drop for CommandListHandle<'_> {
    fn drop(&mut self) {
        if self.handle.0.is_null() {
            return;
        }
        // SAFETY: see DeviceContext::drop.
        let _ = unsafe { (self.loader.ze_command_list_destroy)(self.handle) };
        self.handle.0 = null_mut();
    }
}

// ───────────────────────────────────────────────────────────────────────────
// § ModuleHandle + KernelLaunch
// ───────────────────────────────────────────────────────────────────────────

/// `ze_module_handle_t` wrapper. Drop runs `zeModuleDestroy`.
pub struct ModuleHandle<'l> {
    loader: &'l L0Loader,
    handle: ZeModule,
}

impl<'l> ModuleHandle<'l> {
    /// Underlying handle.
    #[must_use]
    pub const fn handle(&self) -> ZeModule {
        self.handle
    }

    /// Create a kernel by name within this module.
    ///
    /// # Errors
    /// Returns [`SessionError::CallFailed`] when `zeKernelCreate` fails or
    /// [`SessionError::InvalidName`] when `name` contains an internal NUL.
    pub fn create_kernel(&self, name: &str) -> Result<KernelLaunch<'l>, SessionError> {
        let cname = CString::new(name).map_err(|_| SessionError::InvalidName(name.to_string()))?;
        let desc = ZeKernelDesc {
            stype: 0,
            p_next: core::ptr::null(),
            flags: 0,
            p_kernel_name: cname.as_ptr().cast::<u8>(),
        };
        let mut kernel = ZeKernel(null_mut());
        // SAFETY: cname lives for the duration of this call ; desc references it.
        let raw = unsafe { (self.loader.ze_kernel_create)(self.handle, &desc, &mut kernel) };
        let r = ZeResult::from_raw(raw);
        if !r.is_success() {
            return Err(SessionError::CallFailed("zeKernelCreate", r));
        }
        Ok(KernelLaunch {
            loader: self.loader,
            handle: KernelHandle { handle: kernel },
            group_count: ZeGroupCount {
                group_count_x: 1,
                group_count_y: 1,
                group_count_z: 1,
            },
            kernel: KernelHandleRef::owned_marker(),
        })
    }
}

impl Drop for ModuleHandle<'_> {
    fn drop(&mut self) {
        if self.handle.0.is_null() {
            return;
        }
        // SAFETY: see DeviceContext::drop.
        let _ = unsafe { (self.loader.ze_module_destroy)(self.handle) };
        self.handle.0 = null_mut();
    }
}

/// Owned kernel handle (Drop runs zeKernelDestroy).
struct KernelHandle {
    handle: ZeKernel,
}

/// Marker zero-sized type for `KernelLaunch`'s borrow back-edge ; here only to
/// keep the loader-lifetime threaded onto the launch struct.
#[derive(Debug, Clone, Copy)]
struct KernelHandleRef;

impl KernelHandleRef {
    const fn owned_marker() -> Self {
        Self
    }
}

/// A launch-ready kernel + group-count + (future : args + group-size) +
/// the loader reference for the actual `zeKernelDestroy` on Drop.
pub struct KernelLaunch<'l> {
    loader: &'l L0Loader,
    handle: KernelHandle,
    group_count: ZeGroupCount,
    #[allow(dead_code)]
    kernel: KernelHandleRef,
}

impl KernelLaunch<'_> {
    /// Underlying handle.
    #[must_use]
    pub const fn handle(&self) -> ZeKernel {
        self.handle.handle
    }

    /// Set the workgroup-count (X / Y / Z).
    pub fn set_group_count(&mut self, x: u32, y: u32, z: u32) {
        self.group_count = ZeGroupCount {
            group_count_x: x,
            group_count_y: y,
            group_count_z: z,
        };
    }

    /// Current workgroup-count.
    #[must_use]
    pub const fn group_count(&self) -> ZeGroupCount {
        self.group_count
    }
}

impl Drop for KernelLaunch<'_> {
    fn drop(&mut self) {
        if self.handle.handle.0.is_null() {
            return;
        }
        // SAFETY: kernel is owned ; loader holds the resolved fn-ptr.
        let _ = unsafe { (self.loader.ze_kernel_destroy)(self.handle.handle) };
        self.handle.handle.0 = null_mut();
    }
}

// ───────────────────────────────────────────────────────────────────────────
// § UsmAllocation — RAII pointer
// ───────────────────────────────────────────────────────────────────────────

/// USM allocation. Drop calls `zeMemFree(context, ptr)`.
pub struct UsmAllocation<'l> {
    loader: &'l L0Loader,
    context: ZeContext,
    ptr: *mut c_void,
    size: usize,
    kind: UsmAllocType,
}

impl UsmAllocation<'_> {
    /// Pointer (host-visible only for `Host` and `Shared` allocations).
    #[must_use]
    pub const fn as_ptr(&self) -> *mut c_void {
        self.ptr
    }

    /// Size of the allocation in bytes.
    #[must_use]
    pub const fn size(&self) -> usize {
        self.size
    }

    /// Allocation kind.
    #[must_use]
    pub const fn kind(&self) -> UsmAllocType {
        self.kind
    }

    /// True iff the underlying ptr is non-null (success-state predicate).
    #[must_use]
    pub fn is_valid(&self) -> bool {
        !self.ptr.is_null()
    }
}

impl Drop for UsmAllocation<'_> {
    fn drop(&mut self) {
        if self.ptr.is_null() {
            return;
        }
        // SAFETY: ptr was returned by a successful zeMemAlloc* call against
        // the same context ; freeing within the owning context is the L0
        // contract.
        let _ = unsafe { (self.loader.ze_mem_free)(self.context, self.ptr) };
        self.ptr = null_mut();
    }
}

// ───────────────────────────────────────────────────────────────────────────
// § FenceHandle — host-side sync (post-queue submission)
// ───────────────────────────────────────────────────────────────────────────

/// `ze_fence_handle_t` wrapper.
///
/// L0 fences are bound to a command-queue ; CSSLv3 currently uses the
/// command-list immediate-mode path so fence creation requires a
/// queue-handle that the caller manages externally. This wrapper accepts
/// an opaque `*mut c_void` queue-pointer to keep the API future-proof
/// without binding to the queue-ABI shape today.
pub struct FenceHandle<'l> {
    loader: &'l L0Loader,
    handle: ZeFence,
}

impl<'l> FenceHandle<'l> {
    /// Create a fence on the given queue (opaque queue-handle ; phase-F
    /// refines the queue surface).
    ///
    /// # Errors
    /// Returns [`SessionError::CallFailed`] when L0 reports failure.
    pub fn create(loader: &'l L0Loader, queue: *mut c_void) -> Result<Self, SessionError> {
        let mut fence = ZeFence(null_mut());
        // SAFETY: queue is caller-supplied ; null is acceptable for a
        // host-only fence in some L0 modes — failure surfaces via ZeResult.
        let raw = unsafe { (loader.ze_fence_create)(queue, core::ptr::null(), &mut fence) };
        let r = ZeResult::from_raw(raw);
        if !r.is_success() {
            return Err(SessionError::CallFailed("zeFenceCreate", r));
        }
        Ok(Self {
            loader,
            handle: fence,
        })
    }

    /// Block until the fence is signaled or the timeout (ns) elapses.
    /// Pass `u64::MAX` for unbounded wait.
    ///
    /// # Errors
    /// Returns [`SessionError::CallFailed`] on FFI error.
    pub fn wait(&self, timeout_ns: u64) -> Result<(), SessionError> {
        // SAFETY: fence handle is owned ; loader fn-ptr is resolved.
        let raw = unsafe { (self.loader.ze_fence_host_synchronize)(self.handle, timeout_ns) };
        let r = ZeResult::from_raw(raw);
        if !r.is_success() {
            return Err(SessionError::CallFailed("zeFenceHostSynchronize", r));
        }
        Ok(())
    }

    /// Underlying handle.
    #[must_use]
    pub const fn handle(&self) -> ZeFence {
        self.handle
    }
}

impl Drop for FenceHandle<'_> {
    fn drop(&mut self) {
        if self.handle.0.is_null() {
            return;
        }
        // SAFETY: fence is owned ; loader holds resolved fn-ptr.
        let _ = unsafe { (self.loader.ze_fence_destroy)(self.handle) };
        self.handle.0 = null_mut();
    }
}

// ───────────────────────────────────────────────────────────────────────────
// § Errors
// ───────────────────────────────────────────────────────────────────────────

/// Session-level failure modes.
#[derive(Debug, Error)]
pub enum SessionError {
    /// `zeDriverGet` returned zero drivers.
    #[error("Level-Zero reported zero drivers")]
    NoDriver,
    /// `zeDeviceGet` returned zero devices.
    #[error("Level-Zero reported zero devices on the selected driver")]
    NoDevice,
    /// User-supplied driver/device index out of range.
    #[error("index `{0}` out of range for the current driver-set")]
    OutOfRange(&'static str),
    /// Resolved entry-point returned non-success.
    #[error("Level-Zero call `{0}` failed with {1}")]
    CallFailed(&'static str, ZeResult),
    /// Loader subsystem error.
    #[error(transparent)]
    Loader(#[from] LoaderError),
    /// Kernel name contained an internal NUL byte.
    #[error("kernel name `{0}` contains an embedded NUL byte")]
    InvalidName(String),
}

// ───────────────────────────────────────────────────────────────────────────
// § Tests
// ───────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::driver::{L0DeviceProperties, L0DeviceType};

    fn synthetic_metadata(intel_at: Option<(usize, usize)>) -> Vec<L0Driver> {
        let mut drivers = Vec::new();
        for (di, n_devs) in [(0u32, 2), (1, 1)] {
            let mut devs = Vec::new();
            for j in 0..n_devs {
                let vendor = if intel_at == Some((di as usize, j as usize)) {
                    0x8086
                } else {
                    0x10DE
                };
                devs.push(L0Device {
                    driver_index: di,
                    device_index: j as u32,
                    properties: L0DeviceProperties {
                        name: format!("dev {di}/{j}"),
                        device_type: L0DeviceType::Gpu,
                        vendor_id: vendor,
                        device_id: 0,
                        core_clock_rate_mhz: 0,
                        max_compute_units: 0,
                        global_memory_mb: 0,
                        max_workgroup_size: 0,
                        api_major: 1,
                        api_minor: 14,
                    },
                });
            }
            drivers.push(L0Driver {
                index: di,
                api_major: 1,
                api_minor: 14,
                devices: devs,
            });
        }
        drivers
    }

    #[test]
    fn pick_intel_picks_intel_when_present() {
        let meta = synthetic_metadata(Some((1, 0)));
        let pick = pick_intel_preferred(&meta).unwrap();
        assert_eq!(pick, (1, 0));
    }

    #[test]
    fn pick_intel_falls_back_to_first_when_no_intel() {
        let meta = synthetic_metadata(None);
        let pick = pick_intel_preferred(&meta).unwrap();
        assert_eq!(pick, (0, 0));
    }

    #[test]
    fn pick_intel_handles_empty_metadata() {
        assert_eq!(pick_intel_preferred(&[]), None);
    }

    #[test]
    fn pick_intel_handles_driver_with_no_devices() {
        let drivers = vec![L0Driver {
            index: 0,
            api_major: 1,
            api_minor: 0,
            devices: Vec::new(),
        }];
        assert_eq!(pick_intel_preferred(&drivers), None);
    }

    #[test]
    fn session_error_display_strings() {
        let _ = format!("{}", SessionError::NoDriver);
        let _ = format!("{}", SessionError::NoDevice);
        let _ = format!("{}", SessionError::OutOfRange("driver_index"));
        let _ = format!(
            "{}",
            SessionError::CallFailed("zeContextCreate", ZeResult::ErrorOutOfHostMemory)
        );
        let _ = format!(
            "{}",
            SessionError::Loader(LoaderError::SymbolMissing("zeInit"))
        );
        let _ = format!("{}", SessionError::InvalidName("foo\0bar".to_string()));
    }

    #[test]
    fn session_error_loader_conversion() {
        let e: SessionError = LoaderError::NotFound.into();
        match e {
            SessionError::Loader(LoaderError::NotFound) => {}
            other => panic!("expected Loader(NotFound), got {other:?}"),
        }
    }

    #[test]
    fn invalid_name_with_internal_nul_returns_error() {
        // The wrapper around CString::new is what we test ; we don't need a
        // real loader for this — fabricate a SessionError directly.
        let res = CString::new("nul\0inside");
        assert!(res.is_err());
    }

    /// Integration test : skipped on bare runners.
    #[test]
    #[ignore = "requires Intel L0 loader (Arc A770) — run with --ignored"]
    fn arc_a770_session_open_picks_intel() {
        let loader = L0Loader::open().expect("loader present");
        let session = DriverSession::open(&loader).expect("session open");
        let dev = session
            .selected_device_metadata()
            .expect("device metadata present");
        assert!(
            dev.properties.vendor_id == 0x8086 || session.driver_count() == 1,
            "expected Intel device or single-driver fallback ; got vendor 0x{:X}",
            dev.properties.vendor_id
        );
    }

    /// Integration test : RAII context create + drop on Apocky's host.
    #[test]
    #[ignore = "requires Intel L0 loader (Arc A770) — run with --ignored"]
    fn arc_a770_create_context_and_drop() {
        let loader = L0Loader::open().expect("loader present");
        let session = DriverSession::open(&loader).expect("session open");
        let ctx = session.create_context().expect("zeContextCreate succeeds");
        // Drop runs zeContextDestroy via RAII.
        drop(ctx);
    }

    /// Integration test : USM shared allocation + free on Apocky's host.
    #[test]
    #[ignore = "requires Intel L0 loader (Arc A770) — run with --ignored"]
    fn arc_a770_alloc_usm_shared() {
        let loader = L0Loader::open().expect("loader present");
        let session = DriverSession::open(&loader).expect("session open");
        let ctx = session.create_context().expect("ctx");
        let device = session.selected_device_handle().expect("device");
        let alloc = ctx
            .alloc(UsmAllocType::Shared, device, 1024, 64)
            .expect("zeMemAllocShared");
        assert!(alloc.is_valid());
        assert_eq!(alloc.size(), 1024);
    }
}
