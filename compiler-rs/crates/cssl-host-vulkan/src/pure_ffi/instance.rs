//! § pure_ffi::instance — `VkInstance` creation + extension enumeration.
//!
//! § ROLE
//!   From-scratch FFI declarations for the instance-layer Vulkan
//!   surface : `vkCreateInstance` + `vkDestroyInstance` +
//!   `vkEnumerateInstanceExtensionProperties` +
//!   `vkEnumerateInstanceLayerProperties`. No `ash`, no `vulkano`.
//!
//! § DECLARATION-ONLY
//!   The `extern "C"` block is a TYPE declaration that names function
//!   pointers but does not link them ; symbol resolution lands in
//!   Stage B via the [`super::VulkanLoader`] trait.
//!
//! § Rust-WRAPPER
//!   [`InstanceBuilder`] mirrors the `ash` builder ergonomic but
//!   produces an opaque `VkInstance` handle directly. Bodies are
//!   intentionally stub-style today ; cssl-rt host_gpu `STUB` bodies
//!   call into [`InstanceBuilder::build_with_loader`] which returns
//!   the canned `Ok(VkInstance::null())` from a `StubLoader` and
//!   delegates to a real loader once Stage B lands.

#![allow(unsafe_code)]

use core::ffi::c_char;

use super::{
    PVkAllocationCallbacks, VkInstance, VkResult, VkStructureType, VulkanLoader, VK_NULL_HANDLE_NDISP,
};

// ───────────────────────────────────────────────────────────────────
// § Vulkan structures (instance scope).
// ───────────────────────────────────────────────────────────────────

/// `VkApplicationInfo` — application-identifying metadata for the
/// loader (and validation-layer routing).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct VkApplicationInfo {
    /// Must be [`VkStructureType::ApplicationInfo`].
    pub s_type: VkStructureType,
    /// pNext chain head (always null in stage-0).
    pub p_next: *const core::ffi::c_void,
    /// NULL-terminated UTF-8 application name (or null).
    pub p_application_name: *const c_char,
    /// Application-author version (vendor-encoded).
    pub application_version: u32,
    /// NULL-terminated UTF-8 engine name (or null).
    pub p_engine_name: *const c_char,
    /// Engine-author version.
    pub engine_version: u32,
    /// Vulkan API version requested (packed major/minor/patch).
    pub api_version: u32,
}

impl Default for VkApplicationInfo {
    fn default() -> Self {
        Self {
            s_type: VkStructureType::ApplicationInfo,
            p_next: core::ptr::null(),
            p_application_name: core::ptr::null(),
            application_version: 0,
            p_engine_name: core::ptr::null(),
            engine_version: 0,
            api_version: VK_API_VERSION_1_4,
        }
    }
}

/// `VkInstanceCreateInfo` — argument struct for `vkCreateInstance`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct VkInstanceCreateInfo {
    /// Must be [`VkStructureType::InstanceCreateInfo`].
    pub s_type: VkStructureType,
    /// pNext chain head.
    pub p_next: *const core::ffi::c_void,
    /// Bitmask of `VkInstanceCreateFlagBits` (currently always 0 unless
    /// `VK_KHR_portability_enumeration` is being requested ; bit 0 = 0x1
    /// = `VK_INSTANCE_CREATE_ENUMERATE_PORTABILITY_BIT_KHR`).
    pub flags: u32,
    /// Application info (or null for default).
    pub p_application_info: *const VkApplicationInfo,
    /// Number of enabled layer names.
    pub enabled_layer_count: u32,
    /// Pointer to NULL-terminated layer names (length `enabled_layer_count`).
    pub pp_enabled_layer_names: *const *const c_char,
    /// Number of enabled extension names.
    pub enabled_extension_count: u32,
    /// Pointer to NULL-terminated extension names (length `enabled_extension_count`).
    pub pp_enabled_extension_names: *const *const c_char,
}

impl Default for VkInstanceCreateInfo {
    fn default() -> Self {
        Self {
            s_type: VkStructureType::InstanceCreateInfo,
            p_next: core::ptr::null(),
            flags: 0,
            p_application_info: core::ptr::null(),
            enabled_layer_count: 0,
            pp_enabled_layer_names: core::ptr::null(),
            enabled_extension_count: 0,
            pp_enabled_extension_names: core::ptr::null(),
        }
    }
}

/// `VkExtensionProperties` — `vkEnumerateInstanceExtensionProperties` element.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct VkExtensionProperties {
    /// Extension name (NUL-padded ; max 256 bytes per Vulkan).
    pub extension_name: [c_char; 256],
    /// Spec-version of this extension (vendor-encoded).
    pub spec_version: u32,
}

impl Default for VkExtensionProperties {
    fn default() -> Self {
        Self {
            extension_name: [0; 256],
            spec_version: 0,
        }
    }
}

/// `VkLayerProperties` — `vkEnumerateInstanceLayerProperties` element.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct VkLayerProperties {
    /// Layer name (NUL-padded ; max 256 bytes).
    pub layer_name: [c_char; 256],
    /// Vulkan-version this layer was written against.
    pub spec_version: u32,
    /// Layer implementation version.
    pub implementation_version: u32,
    /// Layer description (NUL-padded ; max 256 bytes).
    pub description: [c_char; 256],
}

impl Default for VkLayerProperties {
    fn default() -> Self {
        Self {
            layer_name: [0; 256],
            spec_version: 0,
            implementation_version: 0,
            description: [0; 256],
        }
    }
}

/// `VK_INSTANCE_CREATE_ENUMERATE_PORTABILITY_BIT_KHR` (bit-0 of
/// `VkInstanceCreateFlags`).
pub const VK_INSTANCE_CREATE_ENUMERATE_PORTABILITY_BIT_KHR: u32 = 0x0000_0001;

/// Packed Vulkan-1.0 api-version (major=1, minor=0, patch=0).
pub const VK_API_VERSION_1_0: u32 = 1u32 << 22;
/// Packed Vulkan-1.4 api-version (major=1, minor=4, patch=0).
pub const VK_API_VERSION_1_4: u32 = (1u32 << 22) | (4u32 << 12);

// ───────────────────────────────────────────────────────────────────
// § C signature declarations (TYPE declarations only — no link).
// ───────────────────────────────────────────────────────────────────

/// `vkCreateInstance` C signature (function-pointer typedef).
pub type PfnVkCreateInstance = unsafe extern "C" fn(
    p_create_info: *const VkInstanceCreateInfo,
    p_allocator: PVkAllocationCallbacks,
    p_instance: *mut VkInstance,
) -> i32;

/// `vkDestroyInstance` C signature.
pub type PfnVkDestroyInstance = unsafe extern "C" fn(
    instance: VkInstance,
    p_allocator: PVkAllocationCallbacks,
);

/// `vkEnumerateInstanceExtensionProperties` C signature.
pub type PfnVkEnumerateInstanceExtensionProperties = unsafe extern "C" fn(
    p_layer_name: *const c_char,
    p_property_count: *mut u32,
    p_properties: *mut VkExtensionProperties,
) -> i32;

/// `vkEnumerateInstanceLayerProperties` C signature.
pub type PfnVkEnumerateInstanceLayerProperties = unsafe extern "C" fn(
    p_property_count: *mut u32,
    p_properties: *mut VkLayerProperties,
) -> i32;

// ───────────────────────────────────────────────────────────────────
// § Rust-side wrapper : ergonomic instance-creation.
// ───────────────────────────────────────────────────────────────────

/// Errors surfaced by [`InstanceBuilder::build_with_loader`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstanceBuildError {
    /// The supplied loader has not had `vkCreateInstance` resolved.
    LoaderMissingSymbol(String),
    /// A driver returned a non-success VkResult.
    Vk(VkResult),
    /// Stage A : real loaders not yet wired ; only `MockLoader` /
    /// `StubLoader` paths are exercised at the FFI surface.
    StubLoaderUnsupported,
}

/// Owned-strings builder that produces a `VkInstanceCreateInfo`.
#[derive(Debug, Clone)]
pub struct InstanceBuilder {
    application_name: String,
    engine_name: String,
    api_version: u32,
    flags: u32,
    layers: Vec<String>,
    extensions: Vec<String>,
}

impl Default for InstanceBuilder {
    fn default() -> Self {
        Self {
            application_name: "cssl-host-vulkan".to_string(),
            engine_name: "CSSLv3-stage0".to_string(),
            api_version: VK_API_VERSION_1_4,
            flags: 0,
            layers: Vec::new(),
            extensions: Vec::new(),
        }
    }
}

impl InstanceBuilder {
    /// Begin building a new instance-create-info.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the application-name fed to `VkApplicationInfo::pApplicationName`.
    #[must_use]
    pub fn with_application_name(mut self, name: impl Into<String>) -> Self {
        self.application_name = name.into();
        self
    }

    /// Set the engine-name fed to `VkApplicationInfo::pEngineName`.
    #[must_use]
    pub fn with_engine_name(mut self, name: impl Into<String>) -> Self {
        self.engine_name = name.into();
        self
    }

    /// Set the Vulkan API version (packed major<<22 | minor<<12 | patch).
    #[must_use]
    pub fn with_api_version(mut self, api: u32) -> Self {
        self.api_version = api;
        self
    }

    /// Set the `VkInstanceCreateFlags` bitmask (e.g. portability).
    #[must_use]
    pub fn with_flags(mut self, flags: u32) -> Self {
        self.flags = flags;
        self
    }

    /// Add a validation / utility layer name.
    #[must_use]
    pub fn with_layer(mut self, name: impl Into<String>) -> Self {
        self.layers.push(name.into());
        self
    }

    /// Add an instance-extension name (e.g. `VK_KHR_surface`).
    #[must_use]
    pub fn with_extension(mut self, name: impl Into<String>) -> Self {
        self.extensions.push(name.into());
        self
    }

    /// Number of layers requested (introspection helper).
    #[must_use]
    pub fn layer_count(&self) -> usize {
        self.layers.len()
    }

    /// Number of extensions requested (introspection helper).
    #[must_use]
    pub fn extension_count(&self) -> usize {
        self.extensions.len()
    }

    /// Build the FFI-shape `VkInstanceCreateInfo` along with the C-string
    /// backing storage that MUST outlive the create-info ; the caller
    /// owns the returned `OwnedCreateInfo` for the duration of any FFI
    /// call that takes the create-info pointer.
    #[must_use]
    pub fn into_owned(self) -> OwnedCreateInfo {
        OwnedCreateInfo::from_builder(self)
    }

    /// Resolve `vkCreateInstance` via the supplied loader and (when a
    /// real loader is wired in Stage B) dispatch the call. In Stage A
    /// every loader returns either `None` (StubLoader) or a synthetic
    /// non-NULL address (MockLoader) ; we surface the resolution-shape
    /// to assert the call layering without invoking real FFI.
    ///
    /// # Errors
    /// Returns [`InstanceBuildError::LoaderMissingSymbol`] when the
    /// loader returns NULL for `vkCreateInstance`. Returns
    /// [`InstanceBuildError::StubLoaderUnsupported`] when the resolve
    /// would succeed but Stage A cannot follow through with a real
    /// dispatch yet.
    pub fn build_with_loader<L: VulkanLoader>(
        self,
        loader: &L,
    ) -> Result<VkInstance, InstanceBuildError> {
        match loader.resolve(core::ptr::null_mut(), "vkCreateInstance") {
            None => Err(InstanceBuildError::LoaderMissingSymbol(
                "vkCreateInstance".to_string(),
            )),
            Some(_addr) if !loader.is_real() => Err(InstanceBuildError::StubLoaderUnsupported),
            Some(_addr) => {
                // Stage B will transmute _addr into PfnVkCreateInstance
                // and dispatch the call with the OwnedCreateInfo. Stage
                // A returns a sentinel null instance to keep the surface
                // exercise-able without real FFI.
                let _ = self.into_owned();
                Ok(core::ptr::null_mut())
            }
        }
    }
}

/// FFI-stable owned-storage for an instance-create-info : keeps every
/// CString alive for the lifetime of the wrapper so the raw pointers
/// inside the `create_info` remain valid for the FFI call.
#[derive(Debug)]
#[allow(dead_code)] // owner-of-storage : fields keep raw pointers valid for FFI
pub struct OwnedCreateInfo {
    /// CStr storage for the application-name (pointed-to by `app_info`).
    application_name: std::ffi::CString,
    /// CStr storage for the engine-name.
    engine_name: std::ffi::CString,
    /// CStr storage for every layer-name (Vec keeps CString backing alive).
    _layer_storage: Vec<std::ffi::CString>,
    /// `*const c_char` view into `_layer_storage` for FFI.
    layer_ptrs: Vec<*const c_char>,
    /// CStr storage for every extension-name.
    _extension_storage: Vec<std::ffi::CString>,
    /// `*const c_char` view into `_extension_storage`.
    extension_ptrs: Vec<*const c_char>,
    /// Application-info struct ; refers into `application_name` /
    /// `engine_name` storage above.
    app_info: VkApplicationInfo,
    /// Create-info struct ; refers into `app_info` + ptr-vecs above.
    create_info: VkInstanceCreateInfo,
}

impl OwnedCreateInfo {
    fn from_builder(b: InstanceBuilder) -> Self {
        let application_name = std::ffi::CString::new(b.application_name).unwrap_or_default();
        let engine_name = std::ffi::CString::new(b.engine_name).unwrap_or_default();
        let layer_storage: Vec<_> = b
            .layers
            .into_iter()
            .map(|s| std::ffi::CString::new(s).unwrap_or_default())
            .collect();
        let layer_ptrs: Vec<*const c_char> = layer_storage.iter().map(|s| s.as_ptr()).collect();
        let extension_storage: Vec<_> = b
            .extensions
            .into_iter()
            .map(|s| std::ffi::CString::new(s).unwrap_or_default())
            .collect();
        let extension_ptrs: Vec<*const c_char> =
            extension_storage.iter().map(|s| s.as_ptr()).collect();

        let app_info = VkApplicationInfo {
            s_type: VkStructureType::ApplicationInfo,
            p_next: core::ptr::null(),
            p_application_name: application_name.as_ptr(),
            application_version: 0,
            p_engine_name: engine_name.as_ptr(),
            engine_version: 0,
            api_version: b.api_version,
        };

        // SAFETY-NOTE : the layer/extension ptr-vecs live in `self`
        // (by-value) ; the ptrs to those vec-elements remain valid as
        // long as `OwnedCreateInfo` is not moved (it is `!Unpin`-style
        // semantically because of the self-referential ptrs). Stage A
        // never dispatches with these — Stage B will pin to a Box for
        // long-lived FFI storage.
        let create_info = VkInstanceCreateInfo {
            s_type: VkStructureType::InstanceCreateInfo,
            p_next: core::ptr::null(),
            flags: b.flags,
            p_application_info: core::ptr::addr_of!(app_info),
            enabled_layer_count: u32::try_from(layer_ptrs.len()).unwrap_or(u32::MAX),
            pp_enabled_layer_names: if layer_ptrs.is_empty() {
                core::ptr::null()
            } else {
                layer_ptrs.as_ptr()
            },
            enabled_extension_count: u32::try_from(extension_ptrs.len()).unwrap_or(u32::MAX),
            pp_enabled_extension_names: if extension_ptrs.is_empty() {
                core::ptr::null()
            } else {
                extension_ptrs.as_ptr()
            },
        };

        Self {
            application_name,
            engine_name,
            _layer_storage: layer_storage,
            layer_ptrs,
            _extension_storage: extension_storage,
            extension_ptrs,
            app_info,
            create_info,
        }
    }

    /// Pointer to the FFI-stable `VkInstanceCreateInfo`.
    ///
    /// # Safety
    /// The returned pointer is only valid for the lifetime of `self` ;
    /// callers must not hold it past the `OwnedCreateInfo` drop.
    #[must_use]
    pub fn create_info_ptr(&self) -> *const VkInstanceCreateInfo {
        core::ptr::addr_of!(self.create_info)
    }

    /// Pointer to the FFI-stable `VkApplicationInfo`.
    #[must_use]
    pub fn application_info_ptr(&self) -> *const VkApplicationInfo {
        core::ptr::addr_of!(self.app_info)
    }

    /// Number of layer-name pointers (introspection helper).
    #[must_use]
    pub fn layer_count(&self) -> usize {
        self.layer_ptrs.len()
    }

    /// Number of extension-name pointers (introspection helper).
    #[must_use]
    pub fn extension_count(&self) -> usize {
        self.extension_ptrs.len()
    }
}

/// Sentinel "null instance" handle used by Stage A stub returns and by
/// cssl-rt host_gpu STUB symbols.
#[must_use]
pub fn null_instance() -> VkInstance {
    core::ptr::null_mut()
}

/// Sentinel "no surface" KHR handle.
pub const VK_NULL_SURFACE: u64 = VK_NULL_HANDLE_NDISP;

#[cfg(test)]
mod tests {
    use super::{
        InstanceBuildError, InstanceBuilder, VkApplicationInfo, VkInstanceCreateInfo,
        VK_API_VERSION_1_0, VK_API_VERSION_1_4,
        VK_INSTANCE_CREATE_ENUMERATE_PORTABILITY_BIT_KHR,
    };
    use crate::pure_ffi::{MockLoader, StubLoader, VkStructureType};

    #[test]
    fn application_info_default_is_v1_4() {
        // Compile-time gate : 1.4 > 1.0 (assertion would be const-eval'd ;
        // hoist into a const-block so clippy's `assertions_on_constants`
        // accepts it ; also keeps `items_after_statements` happy).
        const _: () = assert!(VK_API_VERSION_1_4 > VK_API_VERSION_1_0);
        let info = VkApplicationInfo::default();
        assert!(matches!(info.s_type, VkStructureType::ApplicationInfo));
        assert_eq!(info.api_version, VK_API_VERSION_1_4);
    }

    #[test]
    fn portability_flag_bit_is_one() {
        assert_eq!(VK_INSTANCE_CREATE_ENUMERATE_PORTABILITY_BIT_KHR, 0x0000_0001);
    }

    #[test]
    fn instance_create_info_default_is_zero_layers() {
        let info = VkInstanceCreateInfo::default();
        assert_eq!(info.enabled_layer_count, 0);
        assert_eq!(info.enabled_extension_count, 0);
        assert!(info.pp_enabled_layer_names.is_null());
        assert!(info.pp_enabled_extension_names.is_null());
    }

    #[test]
    fn builder_records_layers_and_extensions() {
        let b = InstanceBuilder::new()
            .with_layer("VK_LAYER_KHRONOS_validation")
            .with_extension("VK_KHR_surface")
            .with_extension("VK_EXT_debug_utils");
        assert_eq!(b.layer_count(), 1);
        assert_eq!(b.extension_count(), 2);

        let owned = b.into_owned();
        assert_eq!(owned.layer_count(), 1);
        assert_eq!(owned.extension_count(), 2);
        assert!(!owned.create_info_ptr().is_null());
    }

    #[test]
    fn build_with_stub_loader_errors_with_missing_symbol() {
        let l = StubLoader;
        let r = InstanceBuilder::new().build_with_loader(&l);
        assert!(matches!(r, Err(InstanceBuildError::LoaderMissingSymbol(ref n)) if n == "vkCreateInstance"));
    }

    #[test]
    fn build_with_mock_loader_errors_with_stub_unsupported() {
        let l = MockLoader::new();
        let r = InstanceBuilder::new().build_with_loader(&l);
        assert!(matches!(r, Err(InstanceBuildError::StubLoaderUnsupported)));
        // The mock loader recorded the resolve call.
        assert_eq!(l.resolve_count(), 1);
        assert_eq!(l.resolved_names()[0], "vkCreateInstance");
    }
}
