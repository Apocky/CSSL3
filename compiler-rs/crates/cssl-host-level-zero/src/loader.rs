//! Level-Zero ICD loader — `libloading`-driven dynamic-load.
//!
//! § SPEC : `specs/14_BACKEND.csl § HOST-SUBMIT BACKENDS § Level-Zero` —
//!          stage-0 backs L0 with `level-zero-sys` per the workspace plan ;
//!          T11-D62 substitutes a libloading-driven owned-FFI because the
//!          `level-zero-sys` crate is **not on crates.io** as of toolchain
//!          1.85.0. The owned-FFI matches the stage1+ "owned FFI (volk-like
//!          dispatch)" trajectory and avoids 3rd-party crate-risk.
//!
//! § DESIGN
//!   - Detects the platform-canonical loader filename
//!     (`libze_loader.so` / `libze_loader.so.1` on Unix, `ze_loader.dll` on
//!     Windows) and `dlopen`s it via [`libloading::Library`].
//!   - Resolves the [`crate::ffi`] entry-point fn-pointers up-front and stores
//!     them on a [`L0Loader`] struct ; subsequent calls dispatch through the
//!     resolved table.
//!   - When the loader is absent → returns [`LoaderError::NotFound`] cleanly
//!     (no panic). Calling code can fall back to the [`crate::sysman::StubTelemetryProbe`]
//!     or skip the work.
//!   - When the loader is present but missing a symbol → [`LoaderError::SymbolMissing`].
//!   - Optional sysman entry-points are looked-up but tolerated-absent ; the
//!     [`L0Loader::has_sysman`] predicate reports availability without erroring.
//!
//! § PROBE  (without ever loading the lib)
//!   [`LoaderProbe`] separates "is the loader file present?" from "load it" so
//!   tests can introspect environment readiness without forcing an `unsafe`
//!   load. This is the path used by the `#[cfg_attr(not(arc_a770), ignore)]`
//!   test gating.
//!
//! § SAFETY
//!   - `unsafe` is required to (1) call `Library::new` (loads arbitrary
//!     bytes from disk into the process), (2) dereference resolved
//!     function-pointers. Both are wrapped in narrow `unsafe` blocks with
//!     SAFETY paragraphs naming the contract being upheld.
//!   - The loaded library is held by the [`L0Loader`] struct ; dropping the
//!     struct unloads it. Resolved fn-ptrs become invalid at that point — the
//!     fn-ptrs are never exposed publicly outside [`L0Loader`]'s methods.

use std::path::PathBuf;

use libloading::{Library, Symbol};
use thiserror::Error;

use crate::ffi::{
    init_flag, FnZeCommandListAppendLaunchKernel, FnZeCommandListClose, FnZeCommandListCreate,
    FnZeCommandListDestroy, FnZeContextCreate, FnZeContextDestroy, FnZeDeviceGet,
    FnZeDeviceGetProperties, FnZeDriverGet, FnZeDriverGetApiVersion, FnZeFenceCreate,
    FnZeFenceDestroy, FnZeFenceHostSynchronize, FnZeInit, FnZeKernelCreate, FnZeKernelDestroy,
    FnZeMemAllocDevice, FnZeMemAllocHost, FnZeMemAllocShared, FnZeMemFree, FnZeModuleCreate,
    FnZeModuleDestroy, FnZesDeviceEnumFrequencyDomains, FnZesDeviceEnumPowerDomains,
    FnZesDeviceEnumTemperatureSensors, FnZesDeviceGet, FnZesDeviceGetProperties, FnZesDriverGet,
    FnZesFrequencyGetState, FnZesInit, FnZesPowerGetEnergyCounter, FnZesTemperatureGetState,
    ZeDriver, ZeResult,
};

/// Loader probe — describes the L0 loader's file-presence on the host without
/// trying to load it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoaderProbe {
    /// Filenames the probe checked, in priority order.
    pub candidates: Vec<PathBuf>,
    /// First candidate that exists on disk (if any).
    pub resolved: Option<PathBuf>,
}

impl LoaderProbe {
    /// Probe the canonical L0-loader filenames for the host platform.
    ///
    /// Order :
    ///   - Windows : `ze_loader.dll`
    ///   - Linux + Unix : `libze_loader.so.1`, then `libze_loader.so`
    ///   - macOS : `libze_loader.dylib`
    ///
    /// The probe walks `PATH` (Windows) / `LD_LIBRARY_PATH` (Unix) / system
    /// loader directories ; it does NOT call `dlopen`. The first candidate
    /// found on the filesystem populates [`Self::resolved`].
    #[must_use]
    pub fn detect() -> Self {
        let candidates = canonical_loader_candidates();
        let resolved = candidates.iter().find(|p| p.exists()).cloned();
        Self {
            candidates,
            resolved,
        }
    }

    /// True iff the loader was found at any candidate path.
    #[must_use]
    pub fn is_present(&self) -> bool {
        self.resolved.is_some()
    }
}

/// Resolved Level-Zero entry-point table — produced by [`L0Loader::open`].
///
/// All `Option<...>` slots are sysman entry-points : present on Intel drivers
/// 32.0.x and newer ; older drivers may expose a partial set. Compute entry-points
/// (the non-`Option<...>` fields) MUST be present — a loader missing those is
/// mis-installed and produces [`LoaderError::SymbolMissing`].
pub struct L0Loader {
    /// The `libloading::Library` handle. Held to keep the resolved fn-ptrs
    /// valid for the lifetime of [`L0Loader`].
    library: Library,

    /// Path on disk the loader was opened from.
    pub library_path: PathBuf,

    // § core compute entry-points
    pub ze_init: FnZeInit,
    pub ze_driver_get: FnZeDriverGet,
    pub ze_driver_get_api_version: FnZeDriverGetApiVersion,
    pub ze_device_get: FnZeDeviceGet,
    pub ze_device_get_properties: FnZeDeviceGetProperties,
    pub ze_context_create: FnZeContextCreate,
    pub ze_context_destroy: FnZeContextDestroy,
    pub ze_command_list_create: FnZeCommandListCreate,
    pub ze_command_list_destroy: FnZeCommandListDestroy,
    pub ze_command_list_close: FnZeCommandListClose,
    pub ze_command_list_append_launch_kernel: FnZeCommandListAppendLaunchKernel,
    pub ze_module_create: FnZeModuleCreate,
    pub ze_module_destroy: FnZeModuleDestroy,
    pub ze_kernel_create: FnZeKernelCreate,
    pub ze_kernel_destroy: FnZeKernelDestroy,
    pub ze_mem_alloc_device: FnZeMemAllocDevice,
    pub ze_mem_alloc_host: FnZeMemAllocHost,
    pub ze_mem_alloc_shared: FnZeMemAllocShared,
    pub ze_mem_free: FnZeMemFree,
    pub ze_fence_create: FnZeFenceCreate,
    pub ze_fence_destroy: FnZeFenceDestroy,
    pub ze_fence_host_synchronize: FnZeFenceHostSynchronize,

    // § sysman R18 entry-points  (optional — driver-version-dependent)
    pub zes_init: Option<FnZesInit>,
    pub zes_driver_get: Option<FnZesDriverGet>,
    pub zes_device_get: Option<FnZesDeviceGet>,
    pub zes_device_get_properties: Option<FnZesDeviceGetProperties>,
    pub zes_device_enum_power_domains: Option<FnZesDeviceEnumPowerDomains>,
    pub zes_power_get_energy_counter: Option<FnZesPowerGetEnergyCounter>,
    pub zes_device_enum_temperature_sensors: Option<FnZesDeviceEnumTemperatureSensors>,
    pub zes_temperature_get_state: Option<FnZesTemperatureGetState>,
    pub zes_device_enum_frequency_domains: Option<FnZesDeviceEnumFrequencyDomains>,
    pub zes_frequency_get_state: Option<FnZesFrequencyGetState>,
}

impl core::fmt::Debug for L0Loader {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("L0Loader")
            .field("library_path", &self.library_path)
            .field("has_sysman", &self.has_sysman())
            .finish()
    }
}

impl L0Loader {
    /// Locate + open the L0 ICD loader and resolve all fn-pointers.
    ///
    /// # Errors
    /// Returns [`LoaderError::NotFound`] when no candidate filename exists on
    /// disk, [`LoaderError::LoadFailed`] when the file exists but `dlopen`
    /// fails (e.g., wrong-bitness), or [`LoaderError::SymbolMissing`] when a
    /// required compute entry-point is absent in the loader. Sysman absence
    /// is non-fatal — see [`Self::has_sysman`].
    pub fn open() -> Result<Self, LoaderError> {
        let probe = LoaderProbe::detect();
        let path = probe.resolved.ok_or(LoaderError::NotFound)?;

        // SAFETY: `Library::new` loads arbitrary bytes from disk and runs
        // initializer code from the target binary. The path is the L0 ICD
        // loader expected to be installed by Intel's GPU driver bundle.
        // Loading is the sole way to obtain function-pointers ; per the
        // `libloading` contract, the returned `Library` MUST outlive every
        // resolved `Symbol<F>` — encoded by holding it on `Self`.
        let library = unsafe { Library::new(&path) }
            .map_err(|e| LoaderError::LoadFailed(format!("{path:?}: {e}")))?;

        let ze_init = unsafe { resolve_required(&library, b"zeInit\0")? };
        let ze_driver_get = unsafe { resolve_required(&library, b"zeDriverGet\0")? };
        let ze_driver_get_api_version =
            unsafe { resolve_required(&library, b"zeDriverGetApiVersion\0")? };
        let ze_device_get = unsafe { resolve_required(&library, b"zeDeviceGet\0")? };
        let ze_device_get_properties =
            unsafe { resolve_required(&library, b"zeDeviceGetProperties\0")? };
        let ze_context_create = unsafe { resolve_required(&library, b"zeContextCreate\0")? };
        let ze_context_destroy = unsafe { resolve_required(&library, b"zeContextDestroy\0")? };
        let ze_command_list_create =
            unsafe { resolve_required(&library, b"zeCommandListCreate\0")? };
        let ze_command_list_destroy =
            unsafe { resolve_required(&library, b"zeCommandListDestroy\0")? };
        let ze_command_list_close = unsafe { resolve_required(&library, b"zeCommandListClose\0")? };
        let ze_command_list_append_launch_kernel =
            unsafe { resolve_required(&library, b"zeCommandListAppendLaunchKernel\0")? };
        let ze_module_create = unsafe { resolve_required(&library, b"zeModuleCreate\0")? };
        let ze_module_destroy = unsafe { resolve_required(&library, b"zeModuleDestroy\0")? };
        let ze_kernel_create = unsafe { resolve_required(&library, b"zeKernelCreate\0")? };
        let ze_kernel_destroy = unsafe { resolve_required(&library, b"zeKernelDestroy\0")? };
        let ze_mem_alloc_device = unsafe { resolve_required(&library, b"zeMemAllocDevice\0")? };
        let ze_mem_alloc_host = unsafe { resolve_required(&library, b"zeMemAllocHost\0")? };
        let ze_mem_alloc_shared = unsafe { resolve_required(&library, b"zeMemAllocShared\0")? };
        let ze_mem_free = unsafe { resolve_required(&library, b"zeMemFree\0")? };
        let ze_fence_create = unsafe { resolve_required(&library, b"zeFenceCreate\0")? };
        let ze_fence_destroy = unsafe { resolve_required(&library, b"zeFenceDestroy\0")? };
        let ze_fence_host_synchronize =
            unsafe { resolve_required(&library, b"zeFenceHostSynchronize\0")? };

        // sysman : tolerate absence
        let zes_init = unsafe { resolve_optional(&library, b"zesInit\0") };
        let zes_driver_get = unsafe { resolve_optional(&library, b"zesDriverGet\0") };
        let zes_device_get = unsafe { resolve_optional(&library, b"zesDeviceGet\0") };
        let zes_device_get_properties =
            unsafe { resolve_optional(&library, b"zesDeviceGetProperties\0") };
        let zes_device_enum_power_domains =
            unsafe { resolve_optional(&library, b"zesDeviceEnumPowerDomains\0") };
        let zes_power_get_energy_counter =
            unsafe { resolve_optional(&library, b"zesPowerGetEnergyCounter\0") };
        let zes_device_enum_temperature_sensors =
            unsafe { resolve_optional(&library, b"zesDeviceEnumTemperatureSensors\0") };
        let zes_temperature_get_state =
            unsafe { resolve_optional(&library, b"zesTemperatureGetState\0") };
        let zes_device_enum_frequency_domains =
            unsafe { resolve_optional(&library, b"zesDeviceEnumFrequencyDomains\0") };
        let zes_frequency_get_state =
            unsafe { resolve_optional(&library, b"zesFrequencyGetState\0") };

        Ok(Self {
            library,
            library_path: path,
            ze_init,
            ze_driver_get,
            ze_driver_get_api_version,
            ze_device_get,
            ze_device_get_properties,
            ze_context_create,
            ze_context_destroy,
            ze_command_list_create,
            ze_command_list_destroy,
            ze_command_list_close,
            ze_command_list_append_launch_kernel,
            ze_module_create,
            ze_module_destroy,
            ze_kernel_create,
            ze_kernel_destroy,
            ze_mem_alloc_device,
            ze_mem_alloc_host,
            ze_mem_alloc_shared,
            ze_mem_free,
            ze_fence_create,
            ze_fence_destroy,
            ze_fence_host_synchronize,
            zes_init,
            zes_driver_get,
            zes_device_get,
            zes_device_get_properties,
            zes_device_enum_power_domains,
            zes_power_get_energy_counter,
            zes_device_enum_temperature_sensors,
            zes_temperature_get_state,
            zes_device_enum_frequency_domains,
            zes_frequency_get_state,
        })
    }

    /// True iff every sysman entry-point CSSLv3 uses for R18 telemetry was
    /// resolved. Drivers older than Intel 32.0.101.x may expose a subset —
    /// callers can branch on this predicate to degrade gracefully.
    #[must_use]
    pub const fn has_sysman(&self) -> bool {
        self.zes_init.is_some()
            && self.zes_driver_get.is_some()
            && self.zes_device_get.is_some()
            && self.zes_device_enum_power_domains.is_some()
            && self.zes_power_get_energy_counter.is_some()
            && self.zes_device_enum_temperature_sensors.is_some()
            && self.zes_temperature_get_state.is_some()
            && self.zes_device_enum_frequency_domains.is_some()
            && self.zes_frequency_get_state.is_some()
    }

    /// Force a `_unused` reference to `library` to silence dead-field warnings
    /// while making it explicit the field is the lifetime-anchor of every
    /// fn-ptr above. Returns the path the loader resolved.
    #[must_use]
    pub fn library_path(&self) -> &std::path::Path {
        // Sanity: prove `library` has at least one resolvable null-terminator
        // by re-borrowing it (no FFI call).
        let _: &Library = &self.library;
        &self.library_path
    }

    /// Convenience : call `zeInit(GPU_ONLY)` and return the decoded result.
    ///
    /// # Errors
    /// Returns [`LoaderError::CallFailed`] when `zeInit` returns non-success.
    pub fn ze_init_gpu(&self) -> Result<(), LoaderError> {
        // SAFETY: `ze_init` is the resolved fn-ptr held by `self.library`.
        // `init_flag::GPU_ONLY` is a documented L0 init-flag value (= 1).
        let raw = unsafe { (self.ze_init)(init_flag::GPU_ONLY) };
        let r = ZeResult::from_raw(raw);
        if r.is_success() {
            Ok(())
        } else {
            Err(LoaderError::CallFailed("zeInit", r))
        }
    }

    /// Enumerate drivers via `zeDriverGet` ; returns an empty vec if no drivers.
    ///
    /// # Errors
    /// Returns [`LoaderError::CallFailed`] when L0 reports failure.
    pub fn enumerate_drivers(&self) -> Result<Vec<ZeDriver>, LoaderError> {
        let mut count: u32 = 0;
        // SAFETY: ze_driver_get with a null `phDrivers` pointer is the documented
        // count-query pattern — sets `*pCount` to driver count, returns Success.
        let raw = unsafe { (self.ze_driver_get)(&mut count, core::ptr::null_mut()) };
        let r = ZeResult::from_raw(raw);
        if !r.is_success() {
            return Err(LoaderError::CallFailed("zeDriverGet (count)", r));
        }
        if count == 0 {
            return Ok(Vec::new());
        }

        let mut drivers = vec![ZeDriver(core::ptr::null_mut()); count as usize];
        // SAFETY: `drivers.len() == count` guarantees the L0 driver writes
        // exactly `count` slots ; the buffer is owned by us for the call.
        let raw = unsafe { (self.ze_driver_get)(&mut count, drivers.as_mut_ptr()) };
        let r = ZeResult::from_raw(raw);
        if !r.is_success() {
            return Err(LoaderError::CallFailed("zeDriverGet (fill)", r));
        }
        drivers.truncate(count as usize);
        Ok(drivers)
    }

    /// Mirror `enumerate_drivers` for sysman drivers.
    ///
    /// # Errors
    /// Returns [`LoaderError::SysmanUnavailable`] when the driver-set lacks
    /// `zesDriverGet`, or [`LoaderError::CallFailed`] on FFI error.
    pub fn enumerate_sysman_drivers(&self) -> Result<Vec<ZeDriver>, LoaderError> {
        let zes_driver_get = self.zes_driver_get.ok_or(LoaderError::SysmanUnavailable)?;
        let mut count: u32 = 0;
        // SAFETY: `zes_driver_get` is the resolved fn-ptr ; null fill-ptr is
        // the documented count-query.
        let raw = unsafe { (zes_driver_get)(&mut count, core::ptr::null_mut()) };
        let r = ZeResult::from_raw(raw);
        if !r.is_success() {
            return Err(LoaderError::CallFailed("zesDriverGet (count)", r));
        }
        if count == 0 {
            return Ok(Vec::new());
        }
        let mut drivers = vec![ZeDriver(core::ptr::null_mut()); count as usize];
        // SAFETY: see above — buffer-owned-by-caller for the duration.
        let raw = unsafe { (zes_driver_get)(&mut count, drivers.as_mut_ptr()) };
        let r = ZeResult::from_raw(raw);
        if !r.is_success() {
            return Err(LoaderError::CallFailed("zesDriverGet (fill)", r));
        }
        drivers.truncate(count as usize);
        Ok(drivers)
    }
}

// ───────────────────────────────────────────────────────────────────────────
// § Errors
// ───────────────────────────────────────────────────────────────────────────

/// Loader failure modes.
///
/// `LoadFailed` carries an OS-error string ; we deliberately do not derive
/// `Eq` since the embedded `String` makes equality a debug-only concept.
/// Tests pattern-match on the variant rather than `assert_eq!`-ing.
#[derive(Debug, Error)]
pub enum LoaderError {
    /// No L0-loader filename was found on the host (`libze_loader.so` /
    /// `ze_loader.dll` absent). On bare CI runners this is the common case ;
    /// integration tests gate on [`LoaderProbe::is_present`] and `#[ignore]`
    /// when absent.
    #[error("Level-Zero loader not found — install Intel GPU driver / oneAPI Level-Zero runtime")]
    NotFound,
    /// The loader file was found but could not be mapped (often a 32-vs-64-bit
    /// mismatch or corrupt install).
    #[error("Level-Zero loader load failed: {0}")]
    LoadFailed(String),
    /// A required compute entry-point was missing from the loaded library.
    #[error("Level-Zero loader missing symbol `{0}`")]
    SymbolMissing(&'static str),
    /// A resolved entry-point returned a non-success result.
    #[error("Level-Zero call `{0}` failed with {1}")]
    CallFailed(&'static str, ZeResult),
    /// Sysman entry-points were absent or partial.
    #[error("Level-Zero sysman subsystem unavailable on this driver")]
    SysmanUnavailable,
}

// ───────────────────────────────────────────────────────────────────────────
// § Internal helpers — symbol resolution
// ───────────────────────────────────────────────────────────────────────────

/// Resolve a required function-pointer or return [`LoaderError::SymbolMissing`].
///
/// # Safety
/// Caller MUST guarantee `name` ends in a NUL byte. The returned fn-ptr's
/// validity is bounded by the lifetime of `lib`.
unsafe fn resolve_required<F: Copy>(lib: &Library, name: &[u8]) -> Result<F, LoaderError> {
    // SAFETY: `name` is a NUL-terminated symbol-name (caller invariant) ;
    // libloading::Library::get reads it as a C-string + dispatches to dlsym /
    // GetProcAddress, both safe with that input shape.
    match unsafe { lib.get::<F>(name) } {
        Ok(sym) => Ok(deref_symbol(sym)),
        Err(_e) => {
            // Convert NUL-terminator off the symbol-name for the diagnostic.
            let nameless = if name.last() == Some(&0) {
                &name[..name.len() - 1]
            } else {
                name
            };
            // Static-string mapping for the canonical names we resolve.
            Err(LoaderError::SymbolMissing(static_name(nameless)))
        }
    }
}

/// Resolve an optional function-pointer ; returns `None` instead of erroring.
///
/// # Safety
/// Same contract as [`resolve_required`].
unsafe fn resolve_optional<F: Copy>(lib: &Library, name: &[u8]) -> Option<F> {
    // SAFETY: see resolve_required.
    let sym = unsafe { lib.get::<F>(name) }.ok()?;
    Some(deref_symbol(sym))
}

/// Deref a `Symbol<F>` into the bare fn-ptr value.
///
/// `Symbol<F>` is `repr(transparent)` over `*mut c_void` ; since `F: Copy`
/// (a fn-ptr type), copying the underlying pointer is sound. The returned
/// `F` value remains valid only while `lib` is alive — we hold `lib` on
/// [`L0Loader`] for that reason.
fn deref_symbol<F: Copy>(sym: Symbol<'_, F>) -> F {
    // `*sym` is the deref impl on Symbol that yields F by value (Copy).
    *sym
}

/// Map a raw symbol byte-slice to its `&'static str` form for diagnostics.
fn static_name(bytes: &[u8]) -> &'static str {
    match bytes {
        b"zeInit" => "zeInit",
        b"zeDriverGet" => "zeDriverGet",
        b"zeDriverGetApiVersion" => "zeDriverGetApiVersion",
        b"zeDeviceGet" => "zeDeviceGet",
        b"zeDeviceGetProperties" => "zeDeviceGetProperties",
        b"zeContextCreate" => "zeContextCreate",
        b"zeContextDestroy" => "zeContextDestroy",
        b"zeCommandListCreate" => "zeCommandListCreate",
        b"zeCommandListDestroy" => "zeCommandListDestroy",
        b"zeCommandListClose" => "zeCommandListClose",
        b"zeCommandListAppendLaunchKernel" => "zeCommandListAppendLaunchKernel",
        b"zeModuleCreate" => "zeModuleCreate",
        b"zeModuleDestroy" => "zeModuleDestroy",
        b"zeKernelCreate" => "zeKernelCreate",
        b"zeKernelDestroy" => "zeKernelDestroy",
        b"zeMemAllocDevice" => "zeMemAllocDevice",
        b"zeMemAllocHost" => "zeMemAllocHost",
        b"zeMemAllocShared" => "zeMemAllocShared",
        b"zeMemFree" => "zeMemFree",
        b"zeFenceCreate" => "zeFenceCreate",
        b"zeFenceDestroy" => "zeFenceDestroy",
        b"zeFenceHostSynchronize" => "zeFenceHostSynchronize",
        _ => "zeUnknown",
    }
}

// ───────────────────────────────────────────────────────────────────────────
// § Cross-platform loader-filename discovery
// ───────────────────────────────────────────────────────────────────────────

#[must_use]
fn canonical_loader_candidates() -> Vec<PathBuf> {
    // Searched in order ; `LoaderProbe::detect` keeps the first match.
    #[cfg(target_os = "windows")]
    {
        vec_with_search_path(&["ze_loader.dll"])
    }
    #[cfg(target_os = "macos")]
    {
        vec_with_search_path(&["libze_loader.dylib"])
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        vec_with_search_path(&["libze_loader.so.1", "libze_loader.so"])
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", unix)))]
    {
        Vec::new()
    }
}

/// For each filename, build candidate paths from system loader directories +
/// `PATH` / `LD_LIBRARY_PATH` so [`Path::exists`] can short-circuit.
fn vec_with_search_path(filenames: &[&str]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for fname in filenames {
        // 1. bare filename (let the OS loader resolve it later if dlopen)
        out.push(PathBuf::from(*fname));
        // 2. cwd
        if let Ok(cwd) = std::env::current_dir() {
            out.push(cwd.join(fname));
        }
        // 3. PATH entries
        if let Ok(path_env) = std::env::var(if cfg!(windows) {
            "PATH"
        } else {
            "LD_LIBRARY_PATH"
        }) {
            for entry in std::env::split_paths(&path_env) {
                out.push(entry.join(fname));
            }
        }
        // 4. canonical install paths
        for canon in canonical_install_dirs() {
            out.push(canon.join(fname));
        }
    }
    out
}

#[must_use]
fn canonical_install_dirs() -> Vec<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        vec![
            PathBuf::from(r"C:\Windows\System32"),
            PathBuf::from(r"C:\Program Files\Intel\oneAPI\compiler\latest\bin"),
        ]
    }
    #[cfg(target_os = "macos")]
    {
        vec![PathBuf::from("/usr/local/lib")]
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        vec![
            PathBuf::from("/usr/lib/x86_64-linux-gnu"),
            PathBuf::from("/usr/lib64"),
            PathBuf::from("/usr/local/lib"),
        ]
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", unix)))]
    {
        Vec::new()
    }
}

// ───────────────────────────────────────────────────────────────────────────
// § Tests
// ───────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_returns_candidates() {
        let p = LoaderProbe::detect();
        assert!(!p.candidates.is_empty(), "probe must return candidate set");
    }

    #[test]
    fn probe_resolved_implies_exists() {
        let p = LoaderProbe::detect();
        if let Some(path) = &p.resolved {
            assert!(path.exists(), "resolved={path:?} but exists() = false");
        }
    }

    #[test]
    fn probe_is_present_matches_resolved() {
        let p = LoaderProbe::detect();
        assert_eq!(p.is_present(), p.resolved.is_some());
    }

    #[test]
    fn loader_open_returns_not_found_when_loader_absent() {
        // We can't determine whether the loader IS present on this CI runner,
        // but we CAN assert that if it isn't, `open` returns NotFound (not a panic).
        let res = L0Loader::open();
        match res {
            Ok(loader) => {
                // When we DO get a loader, sanity-check the path resolves.
                assert!(loader.library_path().exists());
            }
            Err(LoaderError::NotFound) => {
                // Expected on bare CI runners ; the test passes by virtue of
                // returning the documented sentinel rather than panicking.
            }
            Err(LoaderError::LoadFailed(msg)) => {
                panic!("loader file present but load failed: {msg}");
            }
            Err(other) => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn loader_error_display_strings() {
        // miette-style diagnostic surface — Display must not panic for any variant.
        let _ = format!("{}", LoaderError::NotFound);
        let _ = format!("{}", LoaderError::SymbolMissing("zeInit"));
        let _ = format!("{}", LoaderError::SysmanUnavailable);
        let _ = format!(
            "{}",
            LoaderError::CallFailed("zeDriverGet", ZeResult::ErrorUninitialized)
        );
        let _ = format!("{}", LoaderError::LoadFailed("dlopen barfed".into()));
    }

    #[test]
    fn probe_detect_idempotent() {
        let a = LoaderProbe::detect();
        let b = LoaderProbe::detect();
        assert_eq!(a.is_present(), b.is_present());
    }

    #[test]
    fn static_name_known_symbols() {
        assert_eq!(static_name(b"zeInit"), "zeInit");
        assert_eq!(static_name(b"zeDriverGet"), "zeDriverGet");
        assert_eq!(static_name(b"zeMemAllocDevice"), "zeMemAllocDevice");
        assert_eq!(static_name(b"zeUnknownXyz"), "zeUnknown");
    }

    #[test]
    fn canonical_install_dirs_nonempty() {
        let dirs = canonical_install_dirs();
        // On supported platforms (Linux / macOS / Windows) the list is non-empty.
        assert!(
            !dirs.is_empty() || cfg!(not(any(target_os = "windows", target_os = "macos", unix))),
            "canonical install dirs empty on supported platform"
        );
    }

    /// Integration test : skipped via `#[ignore]` on CI runners without an Arc A770 +
    /// Intel L0 driver. Re-enable via `cargo test -- --ignored --test-threads=1` on
    /// Apocky's host where the Arc A770 + Intel ISV driver are installed.
    #[test]
    #[ignore = "requires Intel L0 loader installed (Arc A770 canonical host) — run with --ignored"]
    fn arc_a770_loader_resolves_compute_entry_points() {
        let loader = L0Loader::open().expect("L0 loader must be present on Apocky host");
        // Compute entry-points must all be non-null when open() succeeded.
        let _ = loader.ze_init;
        let _ = loader.ze_driver_get;
        // sysman should be present on Intel 32.0.x driver.
        assert!(
            loader.has_sysman(),
            "Apocky's Arc A770 driver should expose full sysman"
        );
    }

    /// Same gate, but exercises `ze_init_gpu` end-to-end on Apocky's host.
    #[test]
    #[ignore = "requires Intel L0 loader (Arc A770) — run with --ignored"]
    fn arc_a770_ze_init_succeeds() {
        let loader = L0Loader::open().expect("L0 loader must be present");
        loader.ze_init_gpu().expect("zeInit(GPU_ONLY) must succeed");
    }

    /// Driver enumeration on Apocky's host : at least one Intel driver expected.
    #[test]
    #[ignore = "requires Intel L0 loader (Arc A770) — run with --ignored"]
    fn arc_a770_enumerate_drivers_returns_intel() {
        let loader = L0Loader::open().expect("L0 loader must be present");
        loader.ze_init_gpu().expect("zeInit must succeed");
        let drivers = loader
            .enumerate_drivers()
            .expect("zeDriverGet must succeed");
        assert!(!drivers.is_empty(), "no L0 drivers returned");
    }
}

// All fn-pointer typedefs are referenced as field types of `L0Loader` ;
// the compiler counts the type-use as live. No use-anchors needed.
