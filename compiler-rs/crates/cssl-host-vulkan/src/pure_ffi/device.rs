//! § pure_ffi::device — `VkPhysicalDevice` + `VkDevice` + queue-families.
//!
//! § ROLE
//!   From-scratch FFI declarations for the device-layer Vulkan
//!   surface : `vkEnumeratePhysicalDevices` +
//!   `vkGetPhysicalDeviceProperties` +
//!   `vkGetPhysicalDeviceQueueFamilyProperties` + `vkCreateDevice` +
//!   `vkDestroyDevice` + `vkGetDeviceQueue`.
//!
//! § DECLARATION-ONLY
//!   No symbols are linked. The Rust-side wrappers exercise the
//!   builder + queue-family-pick logic without any real FFI dispatch.

#![allow(unsafe_code)]

use core::ffi::c_char;

use super::{
    PVkAllocationCallbacks, VkDevice, VkInstance, VkPhysicalDevice, VkQueue, VkResult,
    VkStructureType, VulkanLoader,
};

// ───────────────────────────────────────────────────────────────────
// § Vulkan structures (device scope).
// ───────────────────────────────────────────────────────────────────

/// `VkPhysicalDeviceType` — discriminant of the physical-device class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum VkPhysicalDeviceType {
    /// Other (unclassified).
    Other = 0,
    /// Integrated GPU.
    IntegratedGpu = 1,
    /// Discrete GPU.
    DiscreteGpu = 2,
    /// Virtual GPU.
    VirtualGpu = 3,
    /// CPU.
    Cpu = 4,
}

impl VkPhysicalDeviceType {
    /// Decode an `i32` returned by FFI back into the enum ; unrecognized
    /// values fold to [`VkPhysicalDeviceType::Other`].
    #[must_use]
    pub const fn from_raw(code: i32) -> Self {
        match code {
            1 => Self::IntegratedGpu,
            2 => Self::DiscreteGpu,
            3 => Self::VirtualGpu,
            4 => Self::Cpu,
            _ => Self::Other,
        }
    }
}

/// `VkPhysicalDeviceLimits` — selected entries (the full Vulkan-1.4
/// struct has ~80 fields ; stage-0 stores the 6 cssl-rt actually
/// inspects ; the rest are reserved/zero).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct VkPhysicalDeviceLimitsLite {
    /// `maxImageDimension1D`.
    pub max_image_dimension_1d: u32,
    /// `maxImageDimension2D`.
    pub max_image_dimension_2d: u32,
    /// `maxImageDimension3D`.
    pub max_image_dimension_3d: u32,
    /// `maxImageDimensionCube`.
    pub max_image_dimension_cube: u32,
    /// `maxComputeWorkGroupCount[0]`.
    pub max_compute_workgroup_count_x: u32,
    /// `maxComputeWorkGroupCount[1]`.
    pub max_compute_workgroup_count_y: u32,
    /// `maxComputeWorkGroupCount[2]`.
    pub max_compute_workgroup_count_z: u32,
    /// `maxComputeWorkGroupSize[0]`.
    pub max_compute_workgroup_size_x: u32,
    /// `maxComputeWorkGroupSize[1]`.
    pub max_compute_workgroup_size_y: u32,
    /// `maxComputeWorkGroupSize[2]`.
    pub max_compute_workgroup_size_z: u32,
}

/// `VkPhysicalDeviceProperties` — vendor / device / api-version triple
/// (the full struct also embeds `VkPhysicalDeviceLimits` ; we project a
/// "lite" version here to keep the surface readable).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct VkPhysicalDevicePropertiesLite {
    /// API version supported.
    pub api_version: u32,
    /// Driver version (vendor-encoded).
    pub driver_version: u32,
    /// PCI vendor ID.
    pub vendor_id: u32,
    /// PCI device ID.
    pub device_id: u32,
    /// Device type.
    pub device_type: VkPhysicalDeviceType,
    /// Device name (NUL-padded ; max 256 bytes).
    pub device_name: [c_char; 256],
    /// Pipeline-cache UUID (16 bytes).
    pub pipeline_cache_uuid: [u8; 16],
    /// Limits (lite projection).
    pub limits: VkPhysicalDeviceLimitsLite,
}

impl Default for VkPhysicalDevicePropertiesLite {
    fn default() -> Self {
        Self {
            api_version: 0,
            driver_version: 0,
            vendor_id: 0,
            device_id: 0,
            device_type: VkPhysicalDeviceType::Other,
            device_name: [0; 256],
            pipeline_cache_uuid: [0; 16],
            limits: VkPhysicalDeviceLimitsLite::default(),
        }
    }
}

/// `VkQueueFlagBits` — selected entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum VkQueueFlag {
    /// Graphics queue family.
    Graphics = 0x0000_0001,
    /// Compute queue family.
    Compute = 0x0000_0002,
    /// Transfer queue family.
    Transfer = 0x0000_0004,
    /// Sparse-binding queue family.
    SparseBinding = 0x0000_0008,
    /// Protected-memory queue family (Vulkan 1.1+).
    Protected = 0x0000_0010,
    /// Video-decode queue family (Vulkan 1.3+).
    VideoDecode = 0x0000_0020,
}

/// Bitmask of [`VkQueueFlag`] entries (raw `VkQueueFlags`).
pub type VkQueueFlags = u32;

/// `VkQueueFamilyProperties` — `vkGetPhysicalDeviceQueueFamilyProperties` element.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct VkQueueFamilyProperties {
    /// Bitmask of queue capabilities.
    pub queue_flags: VkQueueFlags,
    /// Number of queues in this family.
    pub queue_count: u32,
    /// Granularity of timestamp-counter (in nanoseconds).
    pub timestamp_valid_bits: u32,
    /// Minimum image-transfer granularity : width.
    pub min_image_transfer_granularity_w: u32,
    /// Minimum image-transfer granularity : height.
    pub min_image_transfer_granularity_h: u32,
    /// Minimum image-transfer granularity : depth.
    pub min_image_transfer_granularity_d: u32,
}

/// `VkDeviceQueueCreateInfo` — argument for `vkCreateDevice` queue-spec.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct VkDeviceQueueCreateInfo {
    /// Must be [`VkStructureType::DeviceQueueCreateInfo`].
    pub s_type: VkStructureType,
    /// pNext chain head.
    pub p_next: *const core::ffi::c_void,
    /// Reserved (must be 0).
    pub flags: u32,
    /// Index into the queue-family array.
    pub queue_family_index: u32,
    /// Number of queues to create from this family.
    pub queue_count: u32,
    /// Pointer to `queue_count` floats giving each queue's priority [0..1].
    pub p_queue_priorities: *const f32,
}

impl Default for VkDeviceQueueCreateInfo {
    fn default() -> Self {
        Self {
            s_type: VkStructureType::DeviceQueueCreateInfo,
            p_next: core::ptr::null(),
            flags: 0,
            queue_family_index: 0,
            queue_count: 0,
            p_queue_priorities: core::ptr::null(),
        }
    }
}

/// `VkDeviceCreateInfo` — argument for `vkCreateDevice`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct VkDeviceCreateInfo {
    /// Must be [`VkStructureType::DeviceCreateInfo`].
    pub s_type: VkStructureType,
    /// pNext chain head.
    pub p_next: *const core::ffi::c_void,
    /// Reserved (must be 0).
    pub flags: u32,
    /// Number of queue-create-info entries.
    pub queue_create_info_count: u32,
    /// Pointer to `queue_create_info_count` entries.
    pub p_queue_create_infos: *const VkDeviceQueueCreateInfo,
    /// Number of enabled layer names (deprecated since Vulkan 1.0.13 ; ignore).
    pub enabled_layer_count: u32,
    /// Pointer to layer names (deprecated ; null).
    pub pp_enabled_layer_names: *const *const c_char,
    /// Number of enabled extension names.
    pub enabled_extension_count: u32,
    /// Pointer to extension names (length `enabled_extension_count`).
    pub pp_enabled_extension_names: *const *const c_char,
    /// Pointer to `VkPhysicalDeviceFeatures` (or null).
    pub p_enabled_features: *const core::ffi::c_void,
}

impl Default for VkDeviceCreateInfo {
    fn default() -> Self {
        Self {
            s_type: VkStructureType::DeviceCreateInfo,
            p_next: core::ptr::null(),
            flags: 0,
            queue_create_info_count: 0,
            p_queue_create_infos: core::ptr::null(),
            enabled_layer_count: 0,
            pp_enabled_layer_names: core::ptr::null(),
            enabled_extension_count: 0,
            pp_enabled_extension_names: core::ptr::null(),
            p_enabled_features: core::ptr::null(),
        }
    }
}

// ───────────────────────────────────────────────────────────────────
// § C signature declarations.
// ───────────────────────────────────────────────────────────────────

/// `vkEnumeratePhysicalDevices` C signature.
pub type PfnVkEnumeratePhysicalDevices = unsafe extern "C" fn(
    instance: VkInstance,
    p_physical_device_count: *mut u32,
    p_physical_devices: *mut VkPhysicalDevice,
) -> i32;

/// `vkGetPhysicalDeviceProperties` C signature.
pub type PfnVkGetPhysicalDeviceProperties = unsafe extern "C" fn(
    physical_device: VkPhysicalDevice,
    p_properties: *mut core::ffi::c_void, // real VkPhysicalDeviceProperties
);

/// `vkGetPhysicalDeviceQueueFamilyProperties` C signature.
pub type PfnVkGetPhysicalDeviceQueueFamilyProperties = unsafe extern "C" fn(
    physical_device: VkPhysicalDevice,
    p_queue_family_property_count: *mut u32,
    p_queue_family_properties: *mut VkQueueFamilyProperties,
);

/// `vkCreateDevice` C signature.
pub type PfnVkCreateDevice = unsafe extern "C" fn(
    physical_device: VkPhysicalDevice,
    p_create_info: *const VkDeviceCreateInfo,
    p_allocator: PVkAllocationCallbacks,
    p_device: *mut VkDevice,
) -> i32;

/// `vkDestroyDevice` C signature.
pub type PfnVkDestroyDevice =
    unsafe extern "C" fn(device: VkDevice, p_allocator: PVkAllocationCallbacks);

/// `vkGetDeviceQueue` C signature.
pub type PfnVkGetDeviceQueue = unsafe extern "C" fn(
    device: VkDevice,
    queue_family_index: u32,
    queue_index: u32,
    p_queue: *mut VkQueue,
);

// ───────────────────────────────────────────────────────────────────
// § Rust-side wrapper : queue-family pick + device-create-info builder.
// ───────────────────────────────────────────────────────────────────

/// Errors surfaced by [`DeviceBuilder::build_with_loader`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceBuildError {
    /// Loader returned NULL for `vkCreateDevice`.
    LoaderMissingSymbol(String),
    /// Driver returned a non-success VkResult.
    Vk(VkResult),
    /// Stage A : real loaders not yet wired.
    StubLoaderUnsupported,
    /// No queue-family in the supplied properties matches the requested
    /// flag-mask.
    NoQueueFamilyMatching(VkQueueFlags),
}

/// Result of [`pick_queue_family`] : index + descriptor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PickedQueueFamily {
    /// Index into the queue-family array passed to `pick_queue_family`.
    pub index: u32,
    /// The matched queue-family's flag bitmask.
    pub queue_flags: VkQueueFlags,
    /// Number of queues available in this family.
    pub queue_count: u32,
}

/// Pick the first queue-family in `families` whose `queue_flags`
/// satisfies the requested `required` bitmask. Returns `None` if no
/// family matches.
#[must_use]
pub fn pick_queue_family(
    families: &[VkQueueFamilyProperties],
    required: VkQueueFlags,
) -> Option<PickedQueueFamily> {
    for (i, fam) in families.iter().enumerate() {
        if (fam.queue_flags & required) == required && fam.queue_count > 0 {
            return Some(PickedQueueFamily {
                index: u32::try_from(i).unwrap_or(u32::MAX),
                queue_flags: fam.queue_flags,
                queue_count: fam.queue_count,
            });
        }
    }
    None
}

/// Owned-strings builder that produces a `VkDeviceCreateInfo`.
#[derive(Debug, Clone)]
pub struct DeviceBuilder {
    queue_family_index: u32,
    queue_priorities: Vec<f32>,
    extensions: Vec<String>,
}

impl Default for DeviceBuilder {
    fn default() -> Self {
        Self {
            queue_family_index: 0,
            queue_priorities: vec![1.0],
            extensions: Vec::new(),
        }
    }
}

impl DeviceBuilder {
    /// Begin building a new device-create-info.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the queue-family index to draw queues from.
    #[must_use]
    pub fn with_queue_family(mut self, idx: u32) -> Self {
        self.queue_family_index = idx;
        self
    }

    /// Set the priority list (one entry per queue ; floats in [0..1]).
    #[must_use]
    pub fn with_queue_priorities(mut self, priorities: Vec<f32>) -> Self {
        self.queue_priorities = priorities;
        self
    }

    /// Add a device-extension name (e.g. `VK_KHR_swapchain`).
    #[must_use]
    pub fn with_extension(mut self, name: impl Into<String>) -> Self {
        self.extensions.push(name.into());
        self
    }

    /// Number of queues that will be requested.
    #[must_use]
    pub fn queue_count(&self) -> usize {
        self.queue_priorities.len()
    }

    /// Number of extensions that will be requested.
    #[must_use]
    pub fn extension_count(&self) -> usize {
        self.extensions.len()
    }

    /// Build the FFI-shape device-create-info backed by owned storage.
    #[must_use]
    pub fn into_owned(self) -> OwnedDeviceCreateInfo {
        OwnedDeviceCreateInfo::from_builder(self)
    }

    /// Resolve `vkCreateDevice` via the supplied loader + return the
    /// canned-stage-A response.
    ///
    /// # Errors
    /// See [`DeviceBuildError`] discriminants.
    pub fn build_with_loader<L: VulkanLoader>(
        self,
        loader: &L,
    ) -> Result<VkDevice, DeviceBuildError> {
        match loader.resolve(core::ptr::null_mut(), "vkCreateDevice") {
            None => Err(DeviceBuildError::LoaderMissingSymbol(
                "vkCreateDevice".to_string(),
            )),
            Some(_addr) if !loader.is_real() => Err(DeviceBuildError::StubLoaderUnsupported),
            Some(_addr) => {
                let _ = self.into_owned();
                Ok(core::ptr::null_mut())
            }
        }
    }
}

/// FFI-stable owned-storage for a device-create-info.
#[derive(Debug)]
#[allow(dead_code)] // owner-of-storage : fields keep raw pointers valid for FFI
pub struct OwnedDeviceCreateInfo {
    /// Priority list backing-storage (slice referenced by `queue_create_info`).
    queue_priorities: Vec<f32>,
    /// CStr storage for every extension-name.
    _extension_storage: Vec<std::ffi::CString>,
    /// `*const c_char` view into `_extension_storage`.
    extension_ptrs: Vec<*const c_char>,
    /// Single-queue-family `VkDeviceQueueCreateInfo`.
    queue_create_info: VkDeviceQueueCreateInfo,
    /// `VkDeviceCreateInfo` referencing the above.
    create_info: VkDeviceCreateInfo,
}

impl OwnedDeviceCreateInfo {
    fn from_builder(b: DeviceBuilder) -> Self {
        let queue_priorities = b.queue_priorities;
        let extension_storage: Vec<_> = b
            .extensions
            .into_iter()
            .map(|s| std::ffi::CString::new(s).unwrap_or_default())
            .collect();
        let extension_ptrs: Vec<*const c_char> =
            extension_storage.iter().map(|s| s.as_ptr()).collect();

        let queue_create_info = VkDeviceQueueCreateInfo {
            s_type: VkStructureType::DeviceQueueCreateInfo,
            p_next: core::ptr::null(),
            flags: 0,
            queue_family_index: b.queue_family_index,
            queue_count: u32::try_from(queue_priorities.len()).unwrap_or(u32::MAX),
            p_queue_priorities: queue_priorities.as_ptr(),
        };

        let create_info = VkDeviceCreateInfo {
            s_type: VkStructureType::DeviceCreateInfo,
            p_next: core::ptr::null(),
            flags: 0,
            queue_create_info_count: 1,
            p_queue_create_infos: core::ptr::addr_of!(queue_create_info),
            enabled_layer_count: 0,
            pp_enabled_layer_names: core::ptr::null(),
            enabled_extension_count: u32::try_from(extension_ptrs.len()).unwrap_or(u32::MAX),
            pp_enabled_extension_names: if extension_ptrs.is_empty() {
                core::ptr::null()
            } else {
                extension_ptrs.as_ptr()
            },
            p_enabled_features: core::ptr::null(),
        };

        Self {
            queue_priorities,
            _extension_storage: extension_storage,
            extension_ptrs,
            queue_create_info,
            create_info,
        }
    }

    /// Pointer to the FFI-stable `VkDeviceCreateInfo`.
    #[must_use]
    pub fn create_info_ptr(&self) -> *const VkDeviceCreateInfo {
        core::ptr::addr_of!(self.create_info)
    }

    /// Number of queue priorities (introspection helper).
    #[must_use]
    pub fn queue_priority_count(&self) -> usize {
        self.queue_priorities.len()
    }

    /// Number of extensions (introspection helper).
    #[must_use]
    pub fn extension_count(&self) -> usize {
        self.extension_ptrs.len()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        pick_queue_family, DeviceBuildError, DeviceBuilder, VkPhysicalDeviceType,
        VkQueueFamilyProperties, VkQueueFlag,
    };
    use crate::pure_ffi::{MockLoader, StubLoader};

    fn families() -> Vec<VkQueueFamilyProperties> {
        vec![
            VkQueueFamilyProperties {
                queue_flags: VkQueueFlag::Transfer as u32,
                queue_count: 1,
                ..Default::default()
            },
            VkQueueFamilyProperties {
                queue_flags: (VkQueueFlag::Graphics as u32) | (VkQueueFlag::Compute as u32),
                queue_count: 4,
                ..Default::default()
            },
            VkQueueFamilyProperties {
                queue_flags: VkQueueFlag::Compute as u32,
                queue_count: 2,
                ..Default::default()
            },
        ]
    }

    #[test]
    fn pick_queue_family_finds_graphics() {
        let f = families();
        let p = pick_queue_family(&f, VkQueueFlag::Graphics as u32).expect("graphics family");
        assert_eq!(p.index, 1);
        assert_eq!(p.queue_count, 4);
    }

    #[test]
    fn pick_queue_family_finds_compute() {
        let f = families();
        // First family that satisfies pure-Compute is index 1 (graphics+compute combo).
        let p = pick_queue_family(&f, VkQueueFlag::Compute as u32).expect("compute family");
        assert_eq!(p.index, 1);
    }

    #[test]
    fn pick_queue_family_no_match() {
        let only_transfer = vec![VkQueueFamilyProperties {
            queue_flags: VkQueueFlag::Transfer as u32,
            queue_count: 1,
            ..Default::default()
        }];
        let r = pick_queue_family(&only_transfer, VkQueueFlag::Graphics as u32);
        assert!(r.is_none());
    }

    #[test]
    fn physical_device_type_round_trip() {
        for code in 0..=4 {
            let t = VkPhysicalDeviceType::from_raw(code);
            // 0 maps to Other, 1..=4 to typed variants.
            match code {
                1 => assert_eq!(t, VkPhysicalDeviceType::IntegratedGpu),
                2 => assert_eq!(t, VkPhysicalDeviceType::DiscreteGpu),
                3 => assert_eq!(t, VkPhysicalDeviceType::VirtualGpu),
                4 => assert_eq!(t, VkPhysicalDeviceType::Cpu),
                _ => assert_eq!(t, VkPhysicalDeviceType::Other),
            }
        }
    }

    #[test]
    fn device_builder_records_extensions_and_priorities() {
        let b = DeviceBuilder::new()
            .with_queue_family(2)
            .with_queue_priorities(vec![1.0, 0.5])
            .with_extension("VK_KHR_swapchain")
            .with_extension("VK_KHR_dynamic_rendering");
        assert_eq!(b.queue_count(), 2);
        assert_eq!(b.extension_count(), 2);
        let owned = b.into_owned();
        assert_eq!(owned.queue_priority_count(), 2);
        assert_eq!(owned.extension_count(), 2);
        assert!(!owned.create_info_ptr().is_null());
    }

    #[test]
    fn build_with_stub_loader_errors_with_missing_symbol() {
        let l = StubLoader;
        let r = DeviceBuilder::new().build_with_loader(&l);
        assert!(matches!(r, Err(DeviceBuildError::LoaderMissingSymbol(ref n)) if n == "vkCreateDevice"));
    }

    #[test]
    fn build_with_mock_loader_errors_with_stub_unsupported() {
        let l = MockLoader::new();
        let r = DeviceBuilder::new().build_with_loader(&l);
        assert!(matches!(r, Err(DeviceBuildError::StubLoaderUnsupported)));
        assert_eq!(l.resolve_count(), 1);
    }
}
