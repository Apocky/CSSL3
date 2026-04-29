//! § ffi/instance : ash-backed `VkInstance` + validation-layer routing
//!                  (T11-D65, S6-E1).
//!
//! § ROLE
//!   `VkInstanceHandle` is the RAII wrapper that owns:
//!     - the `ash::Entry`  (libloading handle to the Vulkan loader),
//!     - the `ash::Instance` (loaded `vkCreate*` fn-pointer table),
//!     - a debug-utils messenger (when validation is enabled),
//!     - a process-local `VulkanTelemetryRing` placeholder (R18 hook).
//!   Drop unconditionally calls `vkDestroyInstance` (and the messenger's
//!   destructor first if it was allocated).
//!
//! § VALIDATION LAYERS
//!   `VK_LAYER_KHRONOS_validation` is opt-in via [`InstanceConfig::validation`].
//!   In `cfg(debug_assertions)` builds the default `InstanceConfig`
//!   enables validation ; in release builds the default is `false`. This
//!   matches the "debug-builds only" landmine called out in the slice
//!   spec (HANDOFF_SESSION_6.csl § PHASE-E § S6-E1 LANDMINES).
//!
//! § DEBUG-UTILS CALLBACK
//!   When validation is on, `VK_EXT_debug_utils` is also requested and a
//!   messenger is registered. The callback funnels every Vulkan
//!   diagnostic into the [`VulkanTelemetryRing`] that the instance owns
//!   ; nothing escapes the process.

#![allow(unsafe_code)]

use std::ffi::{c_void, CStr, CString};
use std::sync::Arc;

use ash::ext::debug_utils;
use ash::khr::portability_enumeration;
use ash::vk;

use crate::ffi::error::{AshError, LoaderError, VkResultDisplay};
use crate::ffi::telemetry::VulkanTelemetryRing;

/// Knobs for `vkCreateInstance` invocation.
#[derive(Debug, Clone)]
pub struct InstanceConfig {
    /// Application-name fed to `VkApplicationInfo::pApplicationName`.
    pub application_name: String,
    /// Engine-name fed to `VkApplicationInfo::pEngineName`.
    pub engine_name: String,
    /// API version requested. Stage-0 caps at VK 1.4 per `specs/10`.
    pub api_version: u32,
    /// Whether to request `VK_LAYER_KHRONOS_validation`.
    /// Defaulted from `cfg(debug_assertions)` via [`InstanceConfig::default`].
    pub validation: bool,
    /// Whether to request `VK_EXT_debug_utils` — auto-true when
    /// `validation` is true (otherwise the layer can't surface its
    /// callbacks).
    pub debug_utils: bool,
    /// Extra instance-extensions the caller wants enabled.
    pub extra_extensions: Vec<CString>,
    /// Whether to enable `VK_KHR_portability_enumeration` — required to
    /// enumerate non-conformant ICDs (MoltenVK on macOS) without
    /// breaking the conformant-loader contract.
    pub portability: bool,
}

/// Vulkan 1.4 packed api-version. ash 0.38 ships constants for 1.0..1.3
/// only ; we synthesize 1.4 via the `make_api_version` macro shape :
///   bit-31 : variant (0)
///   bits 22-30 : major
///   bits 12-21 : minor
///   bits  0-11 : patch
pub const VK_API_VERSION_1_4: u32 = (1u32 << 22) | (4u32 << 12);

impl Default for InstanceConfig {
    /// Default config : api 1.4, validation gated to debug-builds, no
    /// portability flag, no extra extensions.
    fn default() -> Self {
        Self {
            application_name: "cssl-host-vulkan".to_string(),
            engine_name: "CSSLv3-stage0".to_string(),
            api_version: VK_API_VERSION_1_4,
            // T11-D65 (S6-E1) : validation-layers gated to debug-builds.
            validation: cfg!(debug_assertions),
            debug_utils: cfg!(debug_assertions),
            extra_extensions: Vec::new(),
            portability: false,
        }
    }
}

impl InstanceConfig {
    /// Disable validation regardless of build profile (used by ffi
    /// integration tests that shouldn't hard-fail on missing
    /// validation-layer install).
    #[must_use]
    pub fn no_validation(mut self) -> Self {
        self.validation = false;
        self.debug_utils = false;
        self
    }

    /// Bump the requested api-version (stage-0 keeps at 1.4 by default).
    #[must_use]
    pub fn with_api_version(mut self, v: u32) -> Self {
        self.api_version = v;
        self
    }
}

/// RAII wrapper owning the `ash::Entry` + `ash::Instance` +
/// validation-callback messenger.
pub struct VkInstanceHandle {
    /// Loaded loader. Held to keep dlopen alive for the instance's lifetime.
    entry: ash::Entry,
    /// Underlying ash::Instance ; `Option` so Drop can `take()` before
    /// destroying.
    instance: Option<ash::Instance>,
    /// Messenger pair : `(extension-loader, messenger-handle)`. Both
    /// live exactly as long as the instance.
    messenger: Option<(debug_utils::Instance, vk::DebugUtilsMessengerEXT)>,
    /// Process-local diagnostic ring populated by the messenger callback
    /// (R18 placeholder). Wrapped in `Arc` so the callback closure can
    /// hold a clone.
    telemetry: Arc<VulkanTelemetryRing>,
    /// Config snapshot kept for diagnostic / inspection.
    config: InstanceConfig,
}

impl VkInstanceHandle {
    /// Load the Vulkan loader, request validation/debug-utils per
    /// [`InstanceConfig`], call `vkCreateInstance`, and (when
    /// applicable) register a debug-utils messenger.
    ///
    /// # Errors
    /// - [`AshError::Loader`] if the Vulkan loader is unreachable.
    /// - [`AshError::InstanceCreate`] if the driver rejects the
    ///   requested api-version / extension-set.
    pub fn create(config: InstanceConfig) -> Result<Self, AshError> {
        // SAFETY : `Entry::load()` performs `dlopen("vulkan-1.dll" ...)` /
        // `dlopen("libvulkan.so.1" ...)`. Failure surfaces as a
        // `LoadingError` — we wrap to `LoaderError::Loading` so callers
        // can gate-skip when the loader is absent.
        let entry = unsafe { ash::Entry::load() }.map_err(|e| {
            AshError::from(LoaderError::Loading {
                detail: format!("{e}"),
            })
        })?;

        // Build VkApplicationInfo + names.
        let app_name_c = CString::new(config.application_name.as_bytes()).unwrap_or_default();
        let eng_name_c = CString::new(config.engine_name.as_bytes()).unwrap_or_default();
        let app_info = vk::ApplicationInfo::default()
            .application_name(app_name_c.as_c_str())
            .application_version(0)
            .engine_name(eng_name_c.as_c_str())
            .engine_version(0)
            .api_version(config.api_version);

        // Resolve which layers + extensions to request.
        let layer_names: Vec<CString> = if config.validation {
            vec![CString::new("VK_LAYER_KHRONOS_validation").unwrap()]
        } else {
            Vec::new()
        };
        let layer_ptrs: Vec<*const i8> = layer_names.iter().map(|c| c.as_ptr()).collect();

        let mut ext_names: Vec<CString> = Vec::new();
        if config.debug_utils || config.validation {
            ext_names.push(CString::from(debug_utils::NAME));
        }
        if config.portability {
            ext_names.push(CString::from(portability_enumeration::NAME));
        }
        for e in &config.extra_extensions {
            ext_names.push(e.clone());
        }
        let ext_ptrs: Vec<*const i8> = ext_names.iter().map(|c| c.as_ptr()).collect();

        let mut create_info = vk::InstanceCreateInfo::default()
            .application_info(&app_info)
            .enabled_layer_names(&layer_ptrs)
            .enabled_extension_names(&ext_ptrs);
        if config.portability {
            create_info = create_info.flags(vk::InstanceCreateFlags::ENUMERATE_PORTABILITY_KHR);
        }

        // SAFETY : `entry` was loaded successfully ; `create_info`'s
        // pointer fields all live to the end of this stack-frame
        // (longer than the create-call).
        let instance = unsafe { entry.create_instance(&create_info, None) }
            .map_err(|r| AshError::InstanceCreate(VkResultDisplay::from(r)))?;

        let telemetry = Arc::new(VulkanTelemetryRing::new());
        let messenger = if config.validation || config.debug_utils {
            Some(register_debug_messenger(
                &entry,
                &instance,
                telemetry.clone(),
            )?)
        } else {
            None
        };

        Ok(Self {
            entry,
            instance: Some(instance),
            messenger,
            telemetry,
            config,
        })
    }

    /// Borrow the underlying ash::Instance for downstream FFI.
    #[must_use]
    pub fn raw(&self) -> &ash::Instance {
        // .expect() can never fire : `instance` is `Some` until Drop.
        self.instance.as_ref().expect("instance present until drop")
    }

    /// Borrow the loader entry.
    #[must_use]
    pub const fn entry(&self) -> &ash::Entry {
        &self.entry
    }

    /// Borrow the telemetry ring.
    #[must_use]
    pub fn telemetry(&self) -> Arc<VulkanTelemetryRing> {
        self.telemetry.clone()
    }

    /// Snapshot the configured api-version (for diagnostics).
    #[must_use]
    pub const fn requested_api_version(&self) -> u32 {
        self.config.api_version
    }

    /// True when validation was requested at create-time.
    #[must_use]
    pub const fn has_validation(&self) -> bool {
        self.config.validation
    }
}

impl Drop for VkInstanceHandle {
    fn drop(&mut self) {
        // Tear down in reverse-create order : messenger before instance.
        if let Some((loader, messenger)) = self.messenger.take() {
            // SAFETY : messenger was created against this loader.
            unsafe {
                loader.destroy_debug_utils_messenger(messenger, None);
            }
        }
        if let Some(inst) = self.instance.take() {
            // SAFETY : matched `vkCreateInstance` from `create()`.
            unsafe {
                inst.destroy_instance(None);
            }
        }
    }
}

/// Register a debug-utils messenger that fans every diagnostic into
/// the supplied telemetry ring. Returns the loader + messenger handle
/// pair the instance must keep for its lifetime.
fn register_debug_messenger(
    entry: &ash::Entry,
    instance: &ash::Instance,
    telemetry: Arc<VulkanTelemetryRing>,
) -> Result<(debug_utils::Instance, vk::DebugUtilsMessengerEXT), AshError> {
    let loader = debug_utils::Instance::new(entry, instance);
    let info = vk::DebugUtilsMessengerCreateInfoEXT::default()
        .message_severity(
            vk::DebugUtilsMessageSeverityFlagsEXT::ERROR
                | vk::DebugUtilsMessageSeverityFlagsEXT::WARNING
                | vk::DebugUtilsMessageSeverityFlagsEXT::INFO
                | vk::DebugUtilsMessageSeverityFlagsEXT::VERBOSE,
        )
        .message_type(
            vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
        )
        .pfn_user_callback(Some(debug_callback_trampoline))
        .user_data(Arc::into_raw(telemetry) as *mut c_void);

    // SAFETY : both `entry`+`instance` are alive ; the user-data pointer
    // is `Arc::into_raw` so Vulkan + the callback can safely deref it.
    let messenger = unsafe { loader.create_debug_utils_messenger(&info, None) }.map_err(|r| {
        AshError::Driver {
            stage: "create_debug_utils_messenger".to_string(),
            result: VkResultDisplay::from(r),
        }
    })?;

    Ok((loader, messenger))
}

/// Trampoline registered with `vkCreateDebugUtilsMessengerEXT`. The
/// `user_data` pointer is the `Arc::into_raw` of the telemetry ring.
unsafe extern "system" fn debug_callback_trampoline(
    severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    msg_type: vk::DebugUtilsMessageTypeFlagsEXT,
    callback_data: *const vk::DebugUtilsMessengerCallbackDataEXT<'_>,
    user_data: *mut c_void,
) -> vk::Bool32 {
    if user_data.is_null() || callback_data.is_null() {
        return vk::FALSE;
    }
    // SAFETY : we passed `Arc::into_raw` as user_data ; reconstruct via
    // `Arc::from_raw` only inside `Arc::increment_strong_count` so the
    // ring stays alive as long as the messenger.
    unsafe { Arc::increment_strong_count(user_data.cast::<VulkanTelemetryRing>()) };
    let ring = unsafe { Arc::from_raw(user_data.cast::<VulkanTelemetryRing>()) };

    let cb = unsafe { &*callback_data };
    let msg = if cb.p_message.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(cb.p_message) }
            .to_string_lossy()
            .into_owned()
    };
    ring.record(crate::ffi::telemetry::ValidationEvent {
        severity_bits: severity.as_raw(),
        type_bits: msg_type.as_raw(),
        message: msg,
    });
    vk::FALSE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_validation_matches_build_profile() {
        let cfg = InstanceConfig::default();
        assert_eq!(cfg.validation, cfg!(debug_assertions));
        assert_eq!(cfg.debug_utils, cfg!(debug_assertions));
        assert_eq!(cfg.api_version, VK_API_VERSION_1_4);
    }

    #[test]
    fn vk_api_version_1_4_decodes_correctly() {
        // major = 1, minor = 4, patch = 0
        assert_eq!((VK_API_VERSION_1_4 >> 22) & 0x7F, 1);
        assert_eq!((VK_API_VERSION_1_4 >> 12) & 0x3FF, 4);
        assert_eq!(VK_API_VERSION_1_4 & 0xFFF, 0);
    }

    #[test]
    fn no_validation_disables_both_flags() {
        let cfg = InstanceConfig::default().no_validation();
        assert!(!cfg.validation);
        assert!(!cfg.debug_utils);
    }

    #[test]
    fn with_api_version_overrides_default() {
        let cfg = InstanceConfig::default().with_api_version(vk::API_VERSION_1_3);
        assert_eq!(cfg.api_version, vk::API_VERSION_1_3);
        assert_ne!(cfg.api_version, VK_API_VERSION_1_4);
    }

    #[test]
    fn create_skips_when_loader_missing() {
        // This test is permissive : on hosts without the Vulkan loader
        // we expect `LoaderError::Loading` ; on hosts with the loader
        // we expect either success or a non-loader AshError. Hard-
        // failures only when the loader is present *and* instance
        // creation fails for an unexpected reason.
        let result = VkInstanceHandle::create(InstanceConfig::default().no_validation());
        match result {
            Ok(_inst) => {
                // Hosts with a Vulkan loader and ICD : creation succeeds.
            }
            Err(AshError::Loader(LoaderError::Loading { .. })) => {
                // Expected on minimal / headless CI. Test passes.
            }
            Err(AshError::InstanceCreate(_)) => {
                // Driver rejected the api-version. Acceptable on
                // hosts whose ICD doesn't expose 1.4.
            }
            Err(AshError::Driver { .. }) => {
                // Acceptable : underlying driver-side error.
            }
            Err(other) => panic!("unexpected error : {other}"),
        }
    }
}
