//! § pure_ffi : from-scratch Vulkan FFI declarations (T11-D258, W-H1).
//!
//! § ROLE
//!   Parallel-implementation surface that authors `extern "C"` Vulkan
//!   declarations from scratch — **zero external crates** (no `ash`, no
//!   `vulkano`). Coexists with the pre-existing `ffi/` submodule (which
//!   wraps `ash`) ; the two are independent and selectable by downstream
//!   consumers.
//!
//! § THESIS  (§ feedback_take_words_literally_no_other_languages)
//!   The CSSLv3 Apockalyptic vision is "proprietary everything". Until
//!   the stage-0 compiler can author native Vulkan calls itself, this
//!   submodule is the bootstrap-host FFI exception : pure stdlib Rust
//!   that ports cleanly to a stage-1 self-hosted CSSL backend without
//!   surrendering any Vulkan-shape understanding to a 3rd-party crate.
//!
//! § STAGE
//!   - **Stage A** (this slice, T11-D258) — declarations + Rust-side
//!     wrapper structs + a [`VulkanLoader`] indirection trait so tests
//!     can mock symbol-resolution. Symbol resolution itself is **not**
//!     wired ; calling [`VulkanLoader::resolve`] on the [`StubLoader`]
//!     returns canned NULL handles. Real `dlopen`/`LoadLibrary` lands
//!     in a follow-up wire-in.
//!   - **Stage B** (next slice) — wire `LibVulkanLoader` against
//!     `libloading`-free dynamic loading via stdlib platform shims
//!     (`std::os::unix::ffi` / Win32 `LoadLibraryA` via `windows-sys`
//!     direct-FFI declarations).
//!   - **Stage C** (cssl-rt swap-in) — replace the `STUB` bodies in
//!     `cssl-rt/src/host_gpu.rs` `__cssl_gpu_*` symbols with calls into
//!     this surface (gated by `cfg(target_os = "linux")` /
//!     `cfg(target_os = "windows")` since the new FFI is Vulkan-only).
//!
//! § SUBMODULE LAYOUT
//!   - [`instance`]  — `VkInstance` creation + extension enumeration
//!     declarations.
//!   - [`device`]    — `VkPhysicalDevice` + `VkDevice` + queue-family
//!     declarations.
//!   - [`swapchain`] — `VkSwapchainKHR` acquire/present declarations.
//!   - [`pipeline`]  — `VkPipeline` compile-from-SPIR-V declarations.
//!   - [`cmd`]       — command-buffer record + submit declarations.
//!
//! § UNSAFE POLICY
//!   `lib.rs` declares `#![deny(unsafe_code)]` at the crate-root ; this
//!   submodule (and its children) opt back in via
//!   `#![allow(unsafe_code)]`. The actual `unsafe extern "C"` Vulkan
//!   declarations are kept as `*const ...` / `*mut ...` raw-pointer
//!   signatures. There are **no `unsafe` blocks** at this stage because
//!   nothing is dispatched against a real loader yet — all surface is
//!   declarative.
//!
//! § PRIME-DIRECTIVE
//!   No surveillance / telemetry-exfiltration. Validation-layer
//!   plumbing remains in the `ffi/` (ash-backed) submodule so the new
//!   pure-FFI surface keeps a strict no-side-channel posture.

#![allow(unsafe_code)]

pub mod cmd;
pub mod device;
pub mod instance;
pub mod pipeline;
pub mod swapchain;

// ───────────────────────────────────────────────────────────────────
// § Vulkan core typedefs (matches vulkan_core.h ; opaque handles are
// dispatchable-handle pointers on 64-bit and u64 IDs on 32-bit per
// Khronos VK_DEFINE_HANDLE / VK_DEFINE_NON_DISPATCHABLE_HANDLE rules).
// CSSLv3 stage-0 targets 64-bit only ; both flavours are pointer-sized.
// ───────────────────────────────────────────────────────────────────

/// Vulkan `VkBool32` typedef (32-bit unsigned integer ; 0 = false, 1 = true).
pub type VkBool32 = u32;
/// Vulkan `VkFlags` typedef (32-bit bitmask root).
pub type VkFlags = u32;
/// Vulkan `VkDeviceSize` typedef (64-bit size).
pub type VkDeviceSize = u64;
/// Vulkan `VkSampleMask` typedef.
pub type VkSampleMask = u32;

/// Opaque `VkInstance` handle (dispatchable).
pub type VkInstance = *mut core::ffi::c_void;
/// Opaque `VkPhysicalDevice` handle (dispatchable).
pub type VkPhysicalDevice = *mut core::ffi::c_void;
/// Opaque `VkDevice` handle (dispatchable).
pub type VkDevice = *mut core::ffi::c_void;
/// Opaque `VkQueue` handle (dispatchable).
pub type VkQueue = *mut core::ffi::c_void;
/// Opaque `VkCommandBuffer` handle (dispatchable).
pub type VkCommandBuffer = *mut core::ffi::c_void;

// Non-dispatchable handles : 64-bit IDs on 32-bit platforms, opaque
// pointers on 64-bit. Stage-0 is 64-bit only ; we use `u64` for ABI
// stability across platforms (matches Khronos non-dispatchable-handle
// recommendation when platform-uniform layout is desired).
/// Opaque `VkSurfaceKHR` handle (non-dispatchable).
pub type VkSurfaceKHR = u64;
/// Opaque `VkSwapchainKHR` handle (non-dispatchable).
pub type VkSwapchainKHR = u64;
/// Opaque `VkImage` handle (non-dispatchable).
pub type VkImage = u64;
/// Opaque `VkImageView` handle (non-dispatchable).
pub type VkImageView = u64;
/// Opaque `VkSemaphore` handle (non-dispatchable).
pub type VkSemaphore = u64;
/// Opaque `VkFence` handle (non-dispatchable).
pub type VkFence = u64;
/// Opaque `VkPipeline` handle (non-dispatchable).
pub type VkPipeline = u64;
/// Opaque `VkPipelineLayout` handle (non-dispatchable).
pub type VkPipelineLayout = u64;
/// Opaque `VkPipelineCache` handle (non-dispatchable).
pub type VkPipelineCache = u64;
/// Opaque `VkShaderModule` handle (non-dispatchable).
pub type VkShaderModule = u64;
/// Opaque `VkRenderPass` handle (non-dispatchable).
pub type VkRenderPass = u64;
/// Opaque `VkFramebuffer` handle (non-dispatchable).
pub type VkFramebuffer = u64;
/// Opaque `VkCommandPool` handle (non-dispatchable).
pub type VkCommandPool = u64;
/// Opaque `VkBuffer` handle (non-dispatchable).
pub type VkBuffer = u64;
/// Opaque `VkDeviceMemory` handle (non-dispatchable).
pub type VkDeviceMemory = u64;

/// Sentinel "null handle" for non-dispatchable Vulkan handles.
pub const VK_NULL_HANDLE_NDISP: u64 = 0;

// ───────────────────────────────────────────────────────────────────
// § VkResult enum (selected ; the Vulkan-1.4 spec defines ~50 entries,
// we ship the subset cssl-rt host_gpu actually inspects).
// ───────────────────────────────────────────────────────────────────

/// `VkResult` — return code from Vulkan calls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum VkResult {
    /// Command successfully completed.
    Success = 0,
    /// A fence or query has not yet completed.
    NotReady = 1,
    /// A wait operation has not completed in the specified time.
    Timeout = 2,
    /// An event is signaled.
    EventSet = 3,
    /// An event is unsignaled.
    EventReset = 4,
    /// A return array was too small for the result.
    Incomplete = 5,
    /// A host memory allocation has failed.
    ErrorOutOfHostMemory = -1,
    /// A device memory allocation has failed.
    ErrorOutOfDeviceMemory = -2,
    /// Initialization of an object could not be completed.
    ErrorInitializationFailed = -3,
    /// The logical or physical device has been lost.
    ErrorDeviceLost = -4,
    /// Mapping of a memory object has failed.
    ErrorMemoryMapFailed = -5,
    /// A requested layer is not present.
    ErrorLayerNotPresent = -6,
    /// A requested extension is not present.
    ErrorExtensionNotPresent = -7,
    /// A requested feature is not supported.
    ErrorFeatureNotPresent = -8,
    /// The requested version is not supported.
    ErrorIncompatibleDriver = -9,
    /// Too many objects of the type have already been created.
    ErrorTooManyObjects = -10,
    /// A surface is no longer available.
    ErrorSurfaceLost = -1_000_000_000,
    /// Native window is in use by another instance.
    ErrorNativeWindowInUse = -1_000_000_001,
    /// Swapchain has become out-of-date relative to the surface.
    ErrorOutOfDate = -1_000_001_004,
    /// Display used by swapchain is incompatible with the surface.
    ErrorIncompatibleDisplay = -1_000_003_001,
    /// Validation layer rejected the call.
    ErrorValidationFailed = -1_000_011_001,
    /// Vulkan version is not supported.
    ErrorUnknown = -13,
}

impl VkResult {
    /// True iff the result indicates success (>= 0 by Vulkan convention).
    #[must_use]
    pub const fn is_success(self) -> bool {
        (self as i32) >= 0
    }

    /// True iff the result is a hard error (< 0 by Vulkan convention).
    #[must_use]
    pub const fn is_error(self) -> bool {
        (self as i32) < 0
    }

    /// Decode an `i32` returned by FFI back into a `VkResult` ; falls
    /// back to [`VkResult::ErrorUnknown`] for unrecognized values to
    /// preserve sentinel-decode hygiene.
    #[must_use]
    pub const fn from_raw(code: i32) -> Self {
        match code {
            0 => Self::Success,
            1 => Self::NotReady,
            2 => Self::Timeout,
            3 => Self::EventSet,
            4 => Self::EventReset,
            5 => Self::Incomplete,
            -1 => Self::ErrorOutOfHostMemory,
            -2 => Self::ErrorOutOfDeviceMemory,
            -3 => Self::ErrorInitializationFailed,
            -4 => Self::ErrorDeviceLost,
            -5 => Self::ErrorMemoryMapFailed,
            -6 => Self::ErrorLayerNotPresent,
            -7 => Self::ErrorExtensionNotPresent,
            -8 => Self::ErrorFeatureNotPresent,
            -9 => Self::ErrorIncompatibleDriver,
            -10 => Self::ErrorTooManyObjects,
            -1_000_000_000 => Self::ErrorSurfaceLost,
            -1_000_000_001 => Self::ErrorNativeWindowInUse,
            -1_000_001_004 => Self::ErrorOutOfDate,
            -1_000_003_001 => Self::ErrorIncompatibleDisplay,
            -1_000_011_001 => Self::ErrorValidationFailed,
            _ => Self::ErrorUnknown,
        }
    }

    /// Convert to raw `i32` for FFI return.
    #[must_use]
    pub const fn to_raw(self) -> i32 {
        self as i32
    }

    /// Short symbolic name (for logging without pulling in Display).
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Success => "VK_SUCCESS",
            Self::NotReady => "VK_NOT_READY",
            Self::Timeout => "VK_TIMEOUT",
            Self::EventSet => "VK_EVENT_SET",
            Self::EventReset => "VK_EVENT_RESET",
            Self::Incomplete => "VK_INCOMPLETE",
            Self::ErrorOutOfHostMemory => "VK_ERROR_OUT_OF_HOST_MEMORY",
            Self::ErrorOutOfDeviceMemory => "VK_ERROR_OUT_OF_DEVICE_MEMORY",
            Self::ErrorInitializationFailed => "VK_ERROR_INITIALIZATION_FAILED",
            Self::ErrorDeviceLost => "VK_ERROR_DEVICE_LOST",
            Self::ErrorMemoryMapFailed => "VK_ERROR_MEMORY_MAP_FAILED",
            Self::ErrorLayerNotPresent => "VK_ERROR_LAYER_NOT_PRESENT",
            Self::ErrorExtensionNotPresent => "VK_ERROR_EXTENSION_NOT_PRESENT",
            Self::ErrorFeatureNotPresent => "VK_ERROR_FEATURE_NOT_PRESENT",
            Self::ErrorIncompatibleDriver => "VK_ERROR_INCOMPATIBLE_DRIVER",
            Self::ErrorTooManyObjects => "VK_ERROR_TOO_MANY_OBJECTS",
            Self::ErrorSurfaceLost => "VK_ERROR_SURFACE_LOST_KHR",
            Self::ErrorNativeWindowInUse => "VK_ERROR_NATIVE_WINDOW_IN_USE_KHR",
            Self::ErrorOutOfDate => "VK_ERROR_OUT_OF_DATE_KHR",
            Self::ErrorIncompatibleDisplay => "VK_ERROR_INCOMPATIBLE_DISPLAY_KHR",
            Self::ErrorValidationFailed => "VK_ERROR_VALIDATION_FAILED_EXT",
            Self::ErrorUnknown => "VK_ERROR_UNKNOWN",
        }
    }
}

// ───────────────────────────────────────────────────────────────────
// § Common structure-types shared across submodules. Each Vk*-struct
// starts with `sType` + `pNext` per the Vulkan-pNext-chain ABI.
// ───────────────────────────────────────────────────────────────────

/// `VkStructureType` — selected entries for the surfaces stage-0 cares about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum VkStructureType {
    /// `VK_STRUCTURE_TYPE_APPLICATION_INFO`.
    ApplicationInfo = 0,
    /// `VK_STRUCTURE_TYPE_INSTANCE_CREATE_INFO`.
    InstanceCreateInfo = 1,
    /// `VK_STRUCTURE_TYPE_DEVICE_QUEUE_CREATE_INFO`.
    DeviceQueueCreateInfo = 2,
    /// `VK_STRUCTURE_TYPE_DEVICE_CREATE_INFO`.
    DeviceCreateInfo = 3,
    /// `VK_STRUCTURE_TYPE_SUBMIT_INFO`.
    SubmitInfo = 4,
    /// `VK_STRUCTURE_TYPE_PIPELINE_SHADER_STAGE_CREATE_INFO`.
    PipelineShaderStageCreateInfo = 18,
    /// `VK_STRUCTURE_TYPE_COMPUTE_PIPELINE_CREATE_INFO`.
    ComputePipelineCreateInfo = 29,
    /// `VK_STRUCTURE_TYPE_COMMAND_POOL_CREATE_INFO`.
    CommandPoolCreateInfo = 39,
    /// `VK_STRUCTURE_TYPE_COMMAND_BUFFER_ALLOCATE_INFO`.
    CommandBufferAllocateInfo = 40,
    /// `VK_STRUCTURE_TYPE_COMMAND_BUFFER_BEGIN_INFO`.
    CommandBufferBeginInfo = 42,
    /// `VK_STRUCTURE_TYPE_SHADER_MODULE_CREATE_INFO`.
    ShaderModuleCreateInfo = 16,
    /// `VK_STRUCTURE_TYPE_PIPELINE_LAYOUT_CREATE_INFO`.
    PipelineLayoutCreateInfo = 30,
    /// `VK_STRUCTURE_TYPE_SWAPCHAIN_CREATE_INFO_KHR`.
    SwapchainCreateInfoKhr = 1_000_001_000,
    /// `VK_STRUCTURE_TYPE_PRESENT_INFO_KHR`.
    PresentInfoKhr = 1_000_001_001,
}

/// Vulkan `VkAllocationCallbacks` opaque pointer (always passed as null
/// in stage-0 ; downstream allocator hooks land in stage-1).
pub type PVkAllocationCallbacks = *const core::ffi::c_void;

// ───────────────────────────────────────────────────────────────────
// § Loader indirection — abstracts symbol resolution so unit tests can
// inject a MockLoader without ever touching libvulkan.
// ───────────────────────────────────────────────────────────────────

/// Symbol-resolution policy.
///
/// Implementations either dispatch via a real platform loader (Stage B)
/// or return canned values (`StubLoader` / `MockLoader` for tests).
pub trait VulkanLoader: core::fmt::Debug + Send + Sync {
    /// Resolve a Vulkan entry-point by C-string name. Returns
    /// `Some(addr)` if the symbol exists or `None` to indicate
    /// `vkGetInstanceProcAddr` returned NULL.
    fn resolve(&self, _instance: VkInstance, _name: &str) -> Option<usize> {
        None
    }

    /// Whether this loader represents a real Vulkan ICD (vs a stub).
    fn is_real(&self) -> bool {
        false
    }
}

/// Always-NULL stub loader used by from-scratch tests + by cssl-rt
/// host_gpu STUB bodies until the real `LibVulkanLoader` lands.
#[derive(Debug, Default, Clone, Copy)]
pub struct StubLoader;

impl VulkanLoader for StubLoader {}

/// Test-only mock loader : records every resolve-request + returns a
/// canned non-zero "address" so unit-tests can verify the call shape
/// without invoking real FFI.
#[derive(Debug, Default)]
pub struct MockLoader {
    /// Names of symbols looked up via [`MockLoader::resolve`].
    inner: std::sync::Mutex<MockLoaderInner>,
}

#[derive(Debug, Default)]
struct MockLoaderInner {
    /// Names of symbols looked up via [`MockLoader::resolve`], in order.
    resolved_names: Vec<String>,
    /// Synthetic address counter ; +1 on every successful resolve.
    next_addr: usize,
}

impl MockLoader {
    /// Create a fresh mock loader with an empty resolve-log.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot the recorded resolve-names (clones for safety).
    #[must_use]
    pub fn resolved_names(&self) -> Vec<String> {
        self.inner
            .lock()
            .map(|g| g.resolved_names.clone())
            .unwrap_or_default()
    }

    /// Number of resolve calls served.
    #[must_use]
    pub fn resolve_count(&self) -> usize {
        self.inner.lock().map(|g| g.resolved_names.len()).unwrap_or(0)
    }
}

impl VulkanLoader for MockLoader {
    fn resolve(&self, _instance: VkInstance, name: &str) -> Option<usize> {
        self.inner.lock().map_or(None, |mut g| {
            g.resolved_names.push(name.to_string());
            g.next_addr = g.next_addr.saturating_add(1);
            // Synthetic addresses start at 0x1000 to stay non-NULL +
            // visibly-fake when surfaced in a panic / assertion.
            Some(0x1000 + g.next_addr)
        })
    }

    fn is_real(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod loader_tests {
    use super::{MockLoader, StubLoader, VkResult, VulkanLoader};

    #[test]
    fn vk_result_round_trip() {
        for code in [0, 1, 2, -1, -4, -1_000_001_004, -1_000_011_001, 99] {
            let r = VkResult::from_raw(code);
            // Unknown codes fold to ErrorUnknown ; everything else
            // round-trips.
            let raw = r.to_raw();
            if r == VkResult::ErrorUnknown {
                continue;
            }
            assert_eq!(raw, code);
        }
    }

    #[test]
    fn vk_result_is_success_polarity() {
        assert!(VkResult::Success.is_success());
        assert!(!VkResult::ErrorOutOfHostMemory.is_success());
        assert!(VkResult::ErrorOutOfHostMemory.is_error());
    }

    #[test]
    fn stub_loader_returns_none() {
        let l = StubLoader;
        assert!(l.resolve(core::ptr::null_mut(), "vkCreateInstance").is_none());
        assert!(!l.is_real());
    }

    #[test]
    fn mock_loader_records_lookups() {
        let l = MockLoader::new();
        let _ = l.resolve(core::ptr::null_mut(), "vkCreateInstance");
        let _ = l.resolve(core::ptr::null_mut(), "vkEnumeratePhysicalDevices");
        assert_eq!(l.resolve_count(), 2);
        let names = l.resolved_names();
        assert_eq!(names[0], "vkCreateInstance");
        assert_eq!(names[1], "vkEnumeratePhysicalDevices");
    }
}
