//! Level-Zero C-ABI declarations + opaque-handle newtypes.
//!
//! § SPEC : `specs/10_HW.csl § LEVEL-ZERO BASELINE` — the host-API surface
//!          CSSLv3 exercises (`ze_driver_handle_t`, `ze_device_handle_t`,
//!          `ze_command_list_t`, `ze_event_t`/`ze_fence_t`, `ze_module_t`,
//!          `ze_kernel_t`, USM allocators, sysman R18 metrics).
//!
//! § DESIGN
//!   This module defines :
//!     - opaque-handle newtypes wrapping `*mut c_void` so the `LiveTelemetryProbe`
//!       and `DriverSession` types carry handle-identity through the type-system,
//!     - C-style `repr(C)` structs mirroring the Level-Zero descriptor types
//!       CSSLv3 needs to populate at FFI-call boundaries,
//!     - typed function-pointer typedefs for each `ze*` / `zes*` entry-point
//!       CSSLv3 calls. The actual function pointers are resolved at runtime
//!       via [`crate::loader::L0Loader`] ; this module only declares the
//!       *shapes* the loader produces.
//!
//! § SAFETY
//!   The newtypes are `#[repr(transparent)]` over `*mut c_void`. The L0 spec
//!   guarantees an opaque-pointer ABI for every `*_handle_t`, so transparent
//!   reinterpretation is sound. Passing a stale or null handle to a real L0
//!   call is UB — `DriverSession`'s RAII discipline + the `is_null` checks at
//!   loader resolution prevent observation of stale state.
//!
//! § REFERENCES
//!   `specs/10_HW.csl § LEVEL-ZERO BASELINE` (canonical entry-point list).
//!   `specs/14_BACKEND.csl § HOST-SUBMIT BACKENDS § Level-Zero`.
//!   Level-Zero spec : <https://spec.oneapi.io/level-zero/latest/>

use core::ffi::c_void;

// ───────────────────────────────────────────────────────────────────────────
// § Opaque handle newtypes
// ───────────────────────────────────────────────────────────────────────────

/// `ze_driver_handle_t` — opaque pointer to an L0 driver.
#[repr(transparent)]
#[derive(Debug, Clone, Copy)]
pub struct ZeDriver(pub *mut c_void);

/// `ze_device_handle_t` — opaque pointer to a physical (or sub-) device.
#[repr(transparent)]
#[derive(Debug, Clone, Copy)]
pub struct ZeDevice(pub *mut c_void);

/// `ze_context_handle_t` — opaque pointer to a context.
#[repr(transparent)]
#[derive(Debug, Clone, Copy)]
pub struct ZeContext(pub *mut c_void);

/// `ze_command_list_handle_t` — opaque pointer to a command-list.
#[repr(transparent)]
#[derive(Debug, Clone, Copy)]
pub struct ZeCommandList(pub *mut c_void);

/// `ze_module_handle_t` — opaque pointer to a compiled SPIR-V module.
#[repr(transparent)]
#[derive(Debug, Clone, Copy)]
pub struct ZeModule(pub *mut c_void);

/// `ze_kernel_handle_t` — opaque pointer to a kernel.
#[repr(transparent)]
#[derive(Debug, Clone, Copy)]
pub struct ZeKernel(pub *mut c_void);

/// `ze_fence_handle_t` — opaque pointer to a fence (host-side sync).
#[repr(transparent)]
#[derive(Debug, Clone, Copy)]
pub struct ZeFence(pub *mut c_void);

/// `ze_event_handle_t` — opaque pointer to an event.
#[repr(transparent)]
#[derive(Debug, Clone, Copy)]
pub struct ZeEvent(pub *mut c_void);

/// `ze_event_pool_handle_t` — opaque pointer to an event-pool.
#[repr(transparent)]
#[derive(Debug, Clone, Copy)]
pub struct ZeEventPool(pub *mut c_void);

/// `zes_device_handle_t` — sysman-scoped device handle (separate from `ze_device_handle_t`).
#[repr(transparent)]
#[derive(Debug, Clone, Copy)]
pub struct ZesDevice(pub *mut c_void);

/// `zes_pwr_handle_t` — sysman power domain handle.
#[repr(transparent)]
#[derive(Debug, Clone, Copy)]
pub struct ZesPwr(pub *mut c_void);

/// `zes_temp_handle_t` — sysman temperature sensor handle.
#[repr(transparent)]
#[derive(Debug, Clone, Copy)]
pub struct ZesTemp(pub *mut c_void);

/// `zes_freq_handle_t` — sysman frequency domain handle.
#[repr(transparent)]
#[derive(Debug, Clone, Copy)]
pub struct ZesFreq(pub *mut c_void);

// ───────────────────────────────────────────────────────────────────────────
// § Result codes
// ───────────────────────────────────────────────────────────────────────────

/// `ze_result_t` — common L0 / sysman result enum (32-bit).
///
/// The values match the canonical Level-Zero spec encoding ; any unknown value
/// is mapped to [`ZeResult::Other`] without losing the integer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ZeResult {
    /// `ZE_RESULT_SUCCESS` (0).
    Success,
    /// `ZE_RESULT_NOT_READY` (1).
    NotReady,
    /// `ZE_RESULT_ERROR_DEVICE_LOST` (0x70000001).
    ErrorDeviceLost,
    /// `ZE_RESULT_ERROR_OUT_OF_HOST_MEMORY` (0x70000002).
    ErrorOutOfHostMemory,
    /// `ZE_RESULT_ERROR_OUT_OF_DEVICE_MEMORY` (0x70000003).
    ErrorOutOfDeviceMemory,
    /// `ZE_RESULT_ERROR_MODULE_BUILD_FAILURE` (0x70000004).
    ErrorModuleBuildFailure,
    /// `ZE_RESULT_ERROR_MODULE_LINK_FAILURE` (0x70000005).
    ErrorModuleLinkFailure,
    /// `ZE_RESULT_ERROR_UNINITIALIZED` (0x78000001).
    ErrorUninitialized,
    /// `ZE_RESULT_ERROR_UNSUPPORTED_VERSION` (0x78000002).
    ErrorUnsupportedVersion,
    /// `ZE_RESULT_ERROR_UNSUPPORTED_FEATURE` (0x78000003).
    ErrorUnsupportedFeature,
    /// `ZE_RESULT_ERROR_INVALID_ARGUMENT` (0x78000004).
    ErrorInvalidArgument,
    /// `ZE_RESULT_ERROR_INVALID_NULL_HANDLE` (0x78000005).
    ErrorInvalidNullHandle,
    /// `ZE_RESULT_ERROR_INVALID_NULL_POINTER` (0x78000006).
    ErrorInvalidNullPointer,
    /// `ZE_RESULT_ERROR_INVALID_SIZE` (0x78000007).
    ErrorInvalidSize,
    /// `ZE_RESULT_ERROR_NOT_AVAILABLE` (0x78000010).
    ErrorNotAvailable,
    /// Any other value not currently classified — preserved as raw `u32`.
    Other(u32),
}

impl ZeResult {
    /// Decode the C-ABI `u32` produced by an L0 entry-point.
    #[must_use]
    pub const fn from_raw(raw: u32) -> Self {
        match raw {
            0x0000_0000 => Self::Success,
            0x0000_0001 => Self::NotReady,
            0x7000_0001 => Self::ErrorDeviceLost,
            0x7000_0002 => Self::ErrorOutOfHostMemory,
            0x7000_0003 => Self::ErrorOutOfDeviceMemory,
            0x7000_0004 => Self::ErrorModuleBuildFailure,
            0x7000_0005 => Self::ErrorModuleLinkFailure,
            0x7800_0001 => Self::ErrorUninitialized,
            0x7800_0002 => Self::ErrorUnsupportedVersion,
            0x7800_0003 => Self::ErrorUnsupportedFeature,
            0x7800_0004 => Self::ErrorInvalidArgument,
            0x7800_0005 => Self::ErrorInvalidNullHandle,
            0x7800_0006 => Self::ErrorInvalidNullPointer,
            0x7800_0007 => Self::ErrorInvalidSize,
            0x7800_0010 => Self::ErrorNotAvailable,
            other => Self::Other(other),
        }
    }

    /// True iff the result is `Success`.
    #[must_use]
    pub const fn is_success(self) -> bool {
        matches!(self, Self::Success)
    }

    /// Round-trip back to the C-ABI `u32`.
    #[must_use]
    pub const fn as_raw(self) -> u32 {
        match self {
            Self::Success => 0x0000_0000,
            Self::NotReady => 0x0000_0001,
            Self::ErrorDeviceLost => 0x7000_0001,
            Self::ErrorOutOfHostMemory => 0x7000_0002,
            Self::ErrorOutOfDeviceMemory => 0x7000_0003,
            Self::ErrorModuleBuildFailure => 0x7000_0004,
            Self::ErrorModuleLinkFailure => 0x7000_0005,
            Self::ErrorUninitialized => 0x7800_0001,
            Self::ErrorUnsupportedVersion => 0x7800_0002,
            Self::ErrorUnsupportedFeature => 0x7800_0003,
            Self::ErrorInvalidArgument => 0x7800_0004,
            Self::ErrorInvalidNullHandle => 0x7800_0005,
            Self::ErrorInvalidNullPointer => 0x7800_0006,
            Self::ErrorInvalidSize => 0x7800_0007,
            Self::ErrorNotAvailable => 0x7800_0010,
            Self::Other(raw) => raw,
        }
    }

    /// Short textual name for diagnostics.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Success => "ZE_RESULT_SUCCESS",
            Self::NotReady => "ZE_RESULT_NOT_READY",
            Self::ErrorDeviceLost => "ZE_RESULT_ERROR_DEVICE_LOST",
            Self::ErrorOutOfHostMemory => "ZE_RESULT_ERROR_OUT_OF_HOST_MEMORY",
            Self::ErrorOutOfDeviceMemory => "ZE_RESULT_ERROR_OUT_OF_DEVICE_MEMORY",
            Self::ErrorModuleBuildFailure => "ZE_RESULT_ERROR_MODULE_BUILD_FAILURE",
            Self::ErrorModuleLinkFailure => "ZE_RESULT_ERROR_MODULE_LINK_FAILURE",
            Self::ErrorUninitialized => "ZE_RESULT_ERROR_UNINITIALIZED",
            Self::ErrorUnsupportedVersion => "ZE_RESULT_ERROR_UNSUPPORTED_VERSION",
            Self::ErrorUnsupportedFeature => "ZE_RESULT_ERROR_UNSUPPORTED_FEATURE",
            Self::ErrorInvalidArgument => "ZE_RESULT_ERROR_INVALID_ARGUMENT",
            Self::ErrorInvalidNullHandle => "ZE_RESULT_ERROR_INVALID_NULL_HANDLE",
            Self::ErrorInvalidNullPointer => "ZE_RESULT_ERROR_INVALID_NULL_POINTER",
            Self::ErrorInvalidSize => "ZE_RESULT_ERROR_INVALID_SIZE",
            Self::ErrorNotAvailable => "ZE_RESULT_ERROR_NOT_AVAILABLE",
            Self::Other(_) => "ZE_RESULT_OTHER",
        }
    }
}

impl core::fmt::Display for ZeResult {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Other(raw) => write!(f, "ZE_RESULT_OTHER(0x{raw:08X})"),
            other => f.write_str(other.as_str()),
        }
    }
}

// ───────────────────────────────────────────────────────────────────────────
// § C-ABI structs (only the descriptor types CSSLv3 populates)
// ───────────────────────────────────────────────────────────────────────────

/// `ze_init_flag_t` bits.
pub mod init_flag {
    /// `ZE_INIT_FLAG_GPU_ONLY`.
    pub const GPU_ONLY: u32 = 1;
    /// `ZE_INIT_FLAG_VPU_ONLY`.
    pub const VPU_ONLY: u32 = 2;
}

/// `ze_command_queue_flag_t` bits.
pub mod cmd_queue_flag {
    /// `ZE_COMMAND_QUEUE_FLAG_EXPLICIT_ONLY`.
    pub const EXPLICIT_ONLY: u32 = 1;
}

/// `ze_command_list_flag_t` bits.
pub mod cmd_list_flag {
    /// `ZE_COMMAND_LIST_FLAG_RELAXED_ORDERING`.
    pub const RELAXED_ORDERING: u32 = 1;
    /// `ZE_COMMAND_LIST_FLAG_MAXIMIZE_THROUGHPUT`.
    pub const MAXIMIZE_THROUGHPUT: u32 = 2;
    /// `ZE_COMMAND_LIST_FLAG_EXPLICIT_ONLY`.
    pub const EXPLICIT_ONLY: u32 = 4;
}

/// `ze_module_format_t`.
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZeModuleFormat {
    /// `ZE_MODULE_FORMAT_IL_SPIRV`.
    Spirv = 0,
    /// `ZE_MODULE_FORMAT_NATIVE`.
    Native = 1,
}

/// `ze_module_desc_t` — partial mirror (CSSLv3 only sets the SPIR-V-relevant fields).
#[repr(C)]
#[derive(Debug)]
pub struct ZeModuleDesc {
    /// `stype` — opaque structure type tag (`ZE_STRUCTURE_TYPE_MODULE_DESC`).
    pub stype: u32,
    /// `pNext` — extension chain pointer.
    pub p_next: *const c_void,
    /// Module-format selector.
    pub format: ZeModuleFormat,
    /// Length of the input (SPIR-V word-count × 4).
    pub input_size: usize,
    /// Pointer to the SPIR-V (or native) blob.
    pub p_input_module: *const u8,
    /// Optional build-options C-string.
    pub p_build_flags: *const u8,
    /// Optional constant-specialization descriptor.
    pub p_constants: *const c_void,
}

/// `ze_command_list_desc_t` — common subset CSSLv3 populates.
#[repr(C)]
#[derive(Debug)]
pub struct ZeCommandListDesc {
    /// `stype`.
    pub stype: u32,
    /// `pNext`.
    pub p_next: *const c_void,
    /// Command-queue group ordinal.
    pub command_queue_group_ordinal: u32,
    /// Flag bits (see [`cmd_list_flag`]).
    pub flags: u32,
}

impl Default for ZeCommandListDesc {
    fn default() -> Self {
        Self {
            stype: 0,
            p_next: core::ptr::null(),
            command_queue_group_ordinal: 0,
            flags: 0,
        }
    }
}

/// `ze_context_desc_t`.
#[repr(C)]
#[derive(Debug)]
pub struct ZeContextDesc {
    /// `stype`.
    pub stype: u32,
    /// `pNext`.
    pub p_next: *const c_void,
    /// Reserved bits.
    pub flags: u32,
}

impl Default for ZeContextDesc {
    fn default() -> Self {
        Self {
            stype: 0,
            p_next: core::ptr::null(),
            flags: 0,
        }
    }
}

/// `ze_kernel_desc_t` — CSSLv3 only sets `pKernelName`.
#[repr(C)]
#[derive(Debug)]
pub struct ZeKernelDesc {
    /// `stype`.
    pub stype: u32,
    /// `pNext`.
    pub p_next: *const c_void,
    /// Reserved bits.
    pub flags: u32,
    /// Null-terminated kernel-name (matches a SPIR-V `OpEntryPoint` name).
    pub p_kernel_name: *const u8,
}

/// `ze_group_count_t` — number of workgroups dispatched.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ZeGroupCount {
    /// Workgroups along X.
    pub group_count_x: u32,
    /// Workgroups along Y.
    pub group_count_y: u32,
    /// Workgroups along Z.
    pub group_count_z: u32,
}

// ───────────────────────────────────────────────────────────────────────────
// § FFI entry-point fn-pointer typedefs
// ───────────────────────────────────────────────────────────────────────────
//
//   These are the function-pointer shapes the loader resolves dynamically.
//   The actual `extern "C" fn(...)` casts happen in [`crate::loader`].

/// `ze_result_t zeInit(ze_init_flag_t flags)`.
pub type FnZeInit = unsafe extern "C" fn(flags: u32) -> u32;

/// `ze_result_t zeDriverGet(uint32_t * pCount, ze_driver_handle_t * phDrivers)`.
pub type FnZeDriverGet = unsafe extern "C" fn(p_count: *mut u32, p_drivers: *mut ZeDriver) -> u32;

/// `ze_result_t zeDriverGetApiVersion(ze_driver_handle_t hDriver, ze_api_version_t * version)`.
pub type FnZeDriverGetApiVersion =
    unsafe extern "C" fn(driver: ZeDriver, p_version: *mut u32) -> u32;

/// `ze_result_t zeDeviceGet(ze_driver_handle_t, uint32_t * pCount, ze_device_handle_t * phDevices)`.
pub type FnZeDeviceGet =
    unsafe extern "C" fn(driver: ZeDriver, p_count: *mut u32, p_devices: *mut ZeDevice) -> u32;

/// `ze_result_t zeDeviceGetProperties(ze_device_handle_t hDevice, ze_device_properties_t *)`.
pub type FnZeDeviceGetProperties =
    unsafe extern "C" fn(device: ZeDevice, p_props: *mut c_void) -> u32;

/// `ze_result_t zeContextCreate(ze_driver_handle_t, const ze_context_desc_t *, ze_context_handle_t *)`.
pub type FnZeContextCreate = unsafe extern "C" fn(
    driver: ZeDriver,
    p_desc: *const ZeContextDesc,
    p_context: *mut ZeContext,
) -> u32;

/// `ze_result_t zeContextDestroy(ze_context_handle_t)`.
pub type FnZeContextDestroy = unsafe extern "C" fn(context: ZeContext) -> u32;

/// `ze_result_t zeCommandListCreate(ze_context_handle_t, ze_device_handle_t, const desc *, list *)`.
pub type FnZeCommandListCreate = unsafe extern "C" fn(
    context: ZeContext,
    device: ZeDevice,
    p_desc: *const ZeCommandListDesc,
    p_list: *mut ZeCommandList,
) -> u32;

/// `ze_result_t zeCommandListDestroy(ze_command_list_handle_t)`.
pub type FnZeCommandListDestroy = unsafe extern "C" fn(list: ZeCommandList) -> u32;

/// `ze_result_t zeModuleCreate(context, device, desc, module, build_log)`.
pub type FnZeModuleCreate = unsafe extern "C" fn(
    context: ZeContext,
    device: ZeDevice,
    p_desc: *const ZeModuleDesc,
    p_module: *mut ZeModule,
    p_build_log: *mut c_void,
) -> u32;

/// `ze_result_t zeModuleDestroy(ze_module_handle_t)`.
pub type FnZeModuleDestroy = unsafe extern "C" fn(module: ZeModule) -> u32;

/// `ze_result_t zeKernelCreate(ze_module_handle_t, const desc *, ze_kernel_handle_t *)`.
pub type FnZeKernelCreate = unsafe extern "C" fn(
    module: ZeModule,
    p_desc: *const ZeKernelDesc,
    p_kernel: *mut ZeKernel,
) -> u32;

/// `ze_result_t zeKernelDestroy(ze_kernel_handle_t)`.
pub type FnZeKernelDestroy = unsafe extern "C" fn(kernel: ZeKernel) -> u32;

/// `ze_result_t zeCommandListAppendLaunchKernel(list, kernel, group_count, signal_event, num_wait, wait)`.
pub type FnZeCommandListAppendLaunchKernel = unsafe extern "C" fn(
    list: ZeCommandList,
    kernel: ZeKernel,
    p_group_count: *const ZeGroupCount,
    h_signal_event: ZeEvent,
    num_wait_events: u32,
    p_wait_events: *const ZeEvent,
) -> u32;

/// `ze_result_t zeCommandListClose(ze_command_list_handle_t)`.
pub type FnZeCommandListClose = unsafe extern "C" fn(list: ZeCommandList) -> u32;

/// `ze_result_t zeMemAllocDevice(context, device_desc, size, alignment, device, pp)`.
pub type FnZeMemAllocDevice = unsafe extern "C" fn(
    context: ZeContext,
    p_device_desc: *const c_void,
    size: usize,
    alignment: usize,
    device: ZeDevice,
    pp_ptr: *mut *mut c_void,
) -> u32;

/// `ze_result_t zeMemAllocHost(context, host_desc, size, alignment, pp)`.
pub type FnZeMemAllocHost = unsafe extern "C" fn(
    context: ZeContext,
    p_host_desc: *const c_void,
    size: usize,
    alignment: usize,
    pp_ptr: *mut *mut c_void,
) -> u32;

/// `ze_result_t zeMemAllocShared(context, device_desc, host_desc, size, alignment, device, pp)`.
pub type FnZeMemAllocShared = unsafe extern "C" fn(
    context: ZeContext,
    p_device_desc: *const c_void,
    p_host_desc: *const c_void,
    size: usize,
    alignment: usize,
    device: ZeDevice,
    pp_ptr: *mut *mut c_void,
) -> u32;

/// `ze_result_t zeMemFree(context, ptr)`.
pub type FnZeMemFree = unsafe extern "C" fn(context: ZeContext, ptr: *mut c_void) -> u32;

/// `ze_result_t zeFenceCreate(queue, desc, fence)`.
pub type FnZeFenceCreate =
    unsafe extern "C" fn(queue: *mut c_void, p_desc: *const c_void, p_fence: *mut ZeFence) -> u32;

/// `ze_result_t zeFenceDestroy(fence)`.
pub type FnZeFenceDestroy = unsafe extern "C" fn(fence: ZeFence) -> u32;

/// `ze_result_t zeFenceHostSynchronize(fence, timeout)`.
pub type FnZeFenceHostSynchronize = unsafe extern "C" fn(fence: ZeFence, timeout: u64) -> u32;

// § sysman R18 fn-pointer typedefs

/// `ze_result_t zesInit(ze_init_flag_t flags)`.
pub type FnZesInit = unsafe extern "C" fn(flags: u32) -> u32;

/// `ze_result_t zesDriverGet(uint32_t *, zes_driver_handle_t *)`.
pub type FnZesDriverGet = unsafe extern "C" fn(p_count: *mut u32, p_drivers: *mut ZeDriver) -> u32;

/// `ze_result_t zesDeviceGet(driver, count, devices)`.
pub type FnZesDeviceGet =
    unsafe extern "C" fn(driver: ZeDriver, p_count: *mut u32, p_devices: *mut ZesDevice) -> u32;

/// `ze_result_t zesDeviceGetProperties(zes_device_handle_t, props)`.
pub type FnZesDeviceGetProperties =
    unsafe extern "C" fn(device: ZesDevice, p_props: *mut c_void) -> u32;

/// `ze_result_t zesDeviceEnumPowerDomains(device, count, domains)`.
pub type FnZesDeviceEnumPowerDomains =
    unsafe extern "C" fn(device: ZesDevice, p_count: *mut u32, p_pwr: *mut ZesPwr) -> u32;

/// `ze_result_t zesPowerGetEnergyCounter(pwr, counter)`.
pub type FnZesPowerGetEnergyCounter =
    unsafe extern "C" fn(pwr: ZesPwr, p_counter: *mut ZesEnergyCounter) -> u32;

/// `ze_result_t zesDeviceEnumTemperatureSensors(device, count, sensors)`.
pub type FnZesDeviceEnumTemperatureSensors =
    unsafe extern "C" fn(device: ZesDevice, p_count: *mut u32, p_temp: *mut ZesTemp) -> u32;

/// `ze_result_t zesTemperatureGetState(temp, state)`.
pub type FnZesTemperatureGetState = unsafe extern "C" fn(temp: ZesTemp, p_state: *mut f64) -> u32;

/// `ze_result_t zesDeviceEnumFrequencyDomains(device, count, freqs)`.
pub type FnZesDeviceEnumFrequencyDomains =
    unsafe extern "C" fn(device: ZesDevice, p_count: *mut u32, p_freq: *mut ZesFreq) -> u32;

/// `ze_result_t zesFrequencyGetState(freq, state)`.
pub type FnZesFrequencyGetState =
    unsafe extern "C" fn(freq: ZesFreq, p_state: *mut ZesFreqState) -> u32;

// § sysman C-ABI structs

/// `zes_power_energy_counter_t` — 16-byte counter snapshot.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ZesEnergyCounter {
    /// Cumulative energy in micro-Joules.
    pub energy_uj: u64,
    /// Timestamp in micro-seconds (monotonic).
    pub timestamp_us: u64,
}

/// `zes_freq_state_t` — frequency domain state snapshot.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ZesFreqState {
    /// Currently-resolved frequency in MHz.
    pub actual_mhz: f64,
    /// Voltage in volts (driver-dependent).
    pub voltage: f64,
    /// Requested frequency in MHz.
    pub request_mhz: f64,
    /// Throttling reasons bitmask.
    pub throttle_reasons: u32,
    /// TDP-clamped frequency in MHz.
    pub tdp_mhz: f64,
    /// Efficient-clock floor in MHz.
    pub efficient_mhz: f64,
    /// Maximum frequency in MHz.
    pub max_mhz: f64,
}

// ───────────────────────────────────────────────────────────────────────────
// § Tests
// ───────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ze_result_round_trip() {
        for raw in [
            0x0000_0000,
            0x0000_0001,
            0x7000_0001,
            0x7000_0002,
            0x7800_0001,
            0x7800_0007,
            0x7800_0010,
            0xDEAD_BEEF, // unknown -> Other
        ] {
            let r = ZeResult::from_raw(raw);
            assert_eq!(r.as_raw(), raw);
        }
    }

    #[test]
    fn ze_result_success_predicate() {
        assert!(ZeResult::Success.is_success());
        assert!(!ZeResult::ErrorUninitialized.is_success());
        assert!(!ZeResult::Other(0xFFFF_FFFF).is_success());
    }

    #[test]
    fn ze_result_display_includes_other_hex() {
        let s = format!("{}", ZeResult::Other(0xCAFE_F00D));
        assert!(s.contains("0xCAFEF00D"), "got: {s}");
    }

    #[test]
    fn handle_layout_is_pointer_sized() {
        // Opaque-handle newtypes are #[repr(transparent)] over *mut c_void,
        // so layout matches the platform pointer-size exactly.
        assert_eq!(
            core::mem::size_of::<ZeDriver>(),
            core::mem::size_of::<*mut c_void>()
        );
        assert_eq!(
            core::mem::size_of::<ZesDevice>(),
            core::mem::size_of::<*mut c_void>()
        );
        assert_eq!(
            core::mem::align_of::<ZeKernel>(),
            core::mem::align_of::<*mut c_void>()
        );
    }

    #[test]
    fn module_format_repr_u32() {
        assert_eq!(ZeModuleFormat::Spirv as u32, 0);
        assert_eq!(ZeModuleFormat::Native as u32, 1);
    }

    #[test]
    fn group_count_default_zero() {
        let g = ZeGroupCount::default();
        assert_eq!(g.group_count_x, 0);
        assert_eq!(g.group_count_y, 0);
        assert_eq!(g.group_count_z, 0);
    }

    #[test]
    fn init_flag_constants() {
        assert_eq!(init_flag::GPU_ONLY, 1);
        assert_eq!(init_flag::VPU_ONLY, 2);
    }

    #[test]
    fn cmd_list_flag_constants_distinct() {
        assert_ne!(
            cmd_list_flag::RELAXED_ORDERING,
            cmd_list_flag::EXPLICIT_ONLY
        );
        assert_ne!(
            cmd_list_flag::RELAXED_ORDERING,
            cmd_list_flag::MAXIMIZE_THROUGHPUT
        );
    }

    #[test]
    fn energy_counter_repr_size() {
        assert_eq!(core::mem::size_of::<ZesEnergyCounter>(), 16);
    }
}
