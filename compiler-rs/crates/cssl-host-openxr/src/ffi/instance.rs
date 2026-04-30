//! § ffi::instance : `XrInstance` FFI surface + extension-enumeration.
//!
//! § SPEC : OpenXR 1.0 § 3 (Instance + Layers + Extensions). The
//!          instance is the root of the OpenXR object hierarchy. The
//!          `xrCreateInstance` call passes an `XrInstanceCreateInfo`
//!          including the application-info and an extension-name list.

use super::loader::DispatchTable;
use super::result::XrResult;
use super::types::{
    Atom, StructureType, XR_CURRENT_API_VERSION, XR_MAX_APPLICATION_NAME_SIZE,
    XR_MAX_ENGINE_NAME_SIZE, XR_MAX_EXTENSION_NAME_SIZE,
};

/// FFI handle for `XrInstance`. 64-bit opaque ; null = `XR_NULL_HANDLE`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(transparent)]
pub struct InstanceHandle(pub u64);

impl InstanceHandle {
    pub const NULL: Self = Self(0);

    #[must_use]
    pub const fn is_null(self) -> bool {
        self.0 == 0
    }
}

/// OpenXR version-quad (major.minor.patch packed).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(transparent)]
pub struct ApiVersion(pub u64);

impl ApiVersion {
    /// Build from `(major, minor, patch)`.
    #[must_use]
    pub const fn new(major: u16, minor: u16, patch: u32) -> Self {
        Self(((major as u64) << 48) | ((minor as u64) << 32) | (patch as u64))
    }

    pub const V_1_0_0: Self = Self::new(1, 0, 0);
    pub const V_1_0_30: Self = Self::new(1, 0, 30);
    pub const V_1_1_0: Self = Self::new(1, 1, 0);

    /// Default version the FFI requests (`XR_CURRENT_API_VERSION`).
    #[must_use]
    pub const fn current() -> Self {
        Self(XR_CURRENT_API_VERSION)
    }

    #[must_use]
    pub const fn major(self) -> u16 {
        (self.0 >> 48) as u16
    }
    #[must_use]
    pub const fn minor(self) -> u16 {
        ((self.0 >> 32) & 0xFFFF) as u16
    }
    #[must_use]
    pub const fn patch(self) -> u32 {
        (self.0 & 0xFFFF_FFFF) as u32
    }
}

/// `XrApplicationInfo` ; FFI struct, `#[repr(C)]`.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct ApplicationInfo {
    pub application_name: [u8; XR_MAX_APPLICATION_NAME_SIZE],
    pub application_version: u32,
    pub engine_name: [u8; XR_MAX_ENGINE_NAME_SIZE],
    pub engine_version: u32,
    pub api_version: u64,
}

impl Default for ApplicationInfo {
    fn default() -> Self {
        Self {
            application_name: [0; XR_MAX_APPLICATION_NAME_SIZE],
            application_version: 1,
            engine_name: [0; XR_MAX_ENGINE_NAME_SIZE],
            engine_version: 1,
            api_version: XR_CURRENT_API_VERSION,
        }
    }
}

impl ApplicationInfo {
    /// Build with the canonical CSSLv3 engine identifier wired in.
    #[must_use]
    pub fn cssl_engine(app_name: &str) -> Self {
        let mut info = Self::default();
        copy_into_buf(&mut info.application_name, app_name.as_bytes());
        copy_into_buf(&mut info.engine_name, b"CSSLv3");
        info
    }
}

fn copy_into_buf(buf: &mut [u8], src: &[u8]) {
    let n = core::cmp::min(buf.len().saturating_sub(1), src.len());
    if n > 0 {
        buf[..n].copy_from_slice(&src[..n]);
    }
    if buf.is_empty() {
        return;
    }
    buf[n] = 0;
}

/// `XrInstanceCreateInfo` ; chain-style FFI struct.
#[repr(C)]
pub struct InstanceCreateInfo {
    pub ty: StructureType,
    pub next: *const core::ffi::c_void,
    pub create_flags: u64,
    pub application_info: ApplicationInfo,
    pub enabled_api_layer_count: u32,
    pub enabled_api_layer_names: *const *const u8,
    pub enabled_extension_count: u32,
    pub enabled_extension_names: *const *const u8,
}

impl core::fmt::Debug for InstanceCreateInfo {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("InstanceCreateInfo")
            .field("ty", &self.ty)
            .field("create_flags", &self.create_flags)
            .field("api_version", &self.application_info.api_version)
            .field("enabled_api_layer_count", &self.enabled_api_layer_count)
            .field("enabled_extension_count", &self.enabled_extension_count)
            .finish()
    }
}

/// `XrExtensionProperties` ; matches the FFI struct returned by
/// `xrEnumerateInstanceExtensionProperties`.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct ExtensionProperties {
    pub ty: StructureType,
    pub next: *mut core::ffi::c_void,
    pub extension_name: [u8; XR_MAX_EXTENSION_NAME_SIZE],
    pub extension_version: u32,
}

impl ExtensionProperties {
    /// Construct an empty record with `ty = ExtensionProperties`.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            ty: StructureType::ExtensionProperties,
            next: core::ptr::null_mut(),
            extension_name: [0; XR_MAX_EXTENSION_NAME_SIZE],
            extension_version: 0,
        }
    }

    /// Read the extension-name as a UTF-8 string (trim at first NUL).
    #[must_use]
    pub fn name(&self) -> &str {
        let n = self
            .extension_name
            .iter()
            .position(|b| *b == 0)
            .unwrap_or(XR_MAX_EXTENSION_NAME_SIZE);
        core::str::from_utf8(&self.extension_name[..n]).unwrap_or("")
    }
}

/// Mock-instance for headless tests : carries the negotiated config + a
/// dispatch-table reference.
#[derive(Debug)]
pub struct MockInstance {
    pub handle: InstanceHandle,
    pub api_version: u64,
    pub application_name: [u8; XR_MAX_APPLICATION_NAME_SIZE],
    pub enabled_extensions: Vec<String>,
    pub created: bool,
    pub destroyed: bool,
    pub atom_pool: u64,
}

impl Default for MockInstance {
    fn default() -> Self {
        Self {
            handle: InstanceHandle::NULL,
            api_version: XR_CURRENT_API_VERSION,
            application_name: [0; XR_MAX_APPLICATION_NAME_SIZE],
            enabled_extensions: Vec::new(),
            created: false,
            destroyed: false,
            atom_pool: 0,
        }
    }
}

/// Configuration for the mock-instance builder ; mirrors what would be
/// fed into `xrCreateInstance` in production.
#[derive(Debug, Clone)]
pub struct MockInstanceConfig {
    pub application_name: String,
    pub enabled_extensions: Vec<String>,
    pub api_version: ApiVersion,
}

impl Default for MockInstanceConfig {
    fn default() -> Self {
        Self {
            application_name: String::new(),
            enabled_extensions: Vec::new(),
            api_version: ApiVersion::current(),
        }
    }
}

impl MockInstanceConfig {
    /// Quest-3s canonical config : passthrough + body-tracking + face-
    /// tracking + display-refresh-rate extensions enabled.
    #[must_use]
    pub fn quest_3s_default(app_name: &str) -> Self {
        Self {
            application_name: app_name.to_string(),
            enabled_extensions: vec![
                "XR_KHR_vulkan_enable2".to_string(),
                "XR_FB_passthrough".to_string(),
                "XR_FB_body_tracking".to_string(),
                "XR_FB_face_tracking2".to_string(),
                "XR_FB_display_refresh_rate".to_string(),
                "XR_EXT_eye_gaze_interaction".to_string(),
                "XR_META_environment_depth".to_string(),
            ],
            api_version: ApiVersion::current(),
        }
    }
}

impl MockInstance {
    /// Pretend-create an instance from a config. Validates the API version
    /// + extension count under the canonical Quest-3s constraint set.
    pub fn create(
        cfg: &MockInstanceConfig,
        _dispatch: &DispatchTable,
    ) -> Result<Self, XrResult> {
        if cfg.api_version.major() == 0 {
            return Err(XrResult::ERROR_API_VERSION_UNSUPPORTED);
        }
        if cfg.api_version < ApiVersion::V_1_0_0 {
            return Err(XrResult::ERROR_API_VERSION_UNSUPPORTED);
        }
        if cfg.enabled_extensions.len() > 64 {
            return Err(XrResult::ERROR_LIMIT_REACHED);
        }
        let mut name_buf = [0u8; XR_MAX_APPLICATION_NAME_SIZE];
        copy_into_buf(&mut name_buf, cfg.application_name.as_bytes());
        Ok(Self {
            handle: InstanceHandle(0xC551_0001),
            api_version: cfg.api_version.0,
            application_name: name_buf,
            enabled_extensions: cfg.enabled_extensions.clone(),
            created: true,
            destroyed: false,
            atom_pool: 0x1000,
        })
    }

    /// Mock equivalent of `xrStringToPath` : returns deterministic atoms.
    /// The `_path` argument matches the OpenXR signature ; we only
    /// monotonically advance the atom pool here.
    pub fn string_to_path(&mut self, _path: &str) -> Atom {
        self.atom_pool = self.atom_pool.wrapping_add(1);
        Atom(self.atom_pool)
    }

    /// Pretend-destroy.
    pub fn destroy(&mut self) -> XrResult {
        if !self.created || self.destroyed {
            return XrResult::ERROR_HANDLE_INVALID;
        }
        self.destroyed = true;
        XrResult::SUCCESS
    }

    /// `true` iff Quest-3s required extensions are present.
    #[must_use]
    pub fn quest_3s_minimum_extensions_present(&self) -> bool {
        let want = [
            "XR_KHR_vulkan_enable2",
            "XR_FB_passthrough",
            "XR_FB_body_tracking",
        ];
        want.iter()
            .all(|w| self.enabled_extensions.iter().any(|e| e == w))
    }
}

/// Builder helper that enumerates a fixed-set of extensions matching the
/// Quest-3s runtime advertisement set as of Horizon-OS v75. Used by
/// `tests/ffi_instance_mock.rs`.
#[must_use]
pub fn quest_3s_runtime_advertised_extensions() -> std::vec::Vec<ExtensionProperties> {
    let names: &[&[u8]] = &[
        b"XR_KHR_vulkan_enable",
        b"XR_KHR_vulkan_enable2",
        b"XR_KHR_composition_layer_depth",
        b"XR_KHR_composition_layer_cube",
        b"XR_KHR_composition_layer_cylinder",
        b"XR_KHR_composition_layer_equirect2",
        b"XR_KHR_visibility_mask",
        b"XR_FB_passthrough",
        b"XR_FB_body_tracking",
        b"XR_FB_face_tracking2",
        b"XR_FB_eye_tracking_social",
        b"XR_FB_display_refresh_rate",
        b"XR_FB_foveation",
        b"XR_FB_haptic_pcm",
        b"XR_FB_space_warp",
        b"XR_FB_swapchain_update_state",
        b"XR_EXT_hand_tracking",
        b"XR_EXT_eye_gaze_interaction",
        b"XR_META_environment_depth",
        b"XR_META_foveation_eye_tracked",
        b"XR_META_passthrough_color_lut",
    ];
    names
        .iter()
        .map(|n| {
            let mut p = ExtensionProperties::empty();
            copy_into_buf(&mut p.extension_name, n);
            p.extension_version = 1;
            p
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        quest_3s_runtime_advertised_extensions, ApiVersion, ApplicationInfo, DispatchTable,
        MockInstance, MockInstanceConfig, XrResult,
    };

    #[test]
    fn api_version_pack_unpack_round_trips() {
        let v = ApiVersion::new(1, 1, 30);
        assert_eq!(v.major(), 1);
        assert_eq!(v.minor(), 1);
        assert_eq!(v.patch(), 30);
        assert!(ApiVersion::V_1_1_0 > ApiVersion::V_1_0_30);
    }

    #[test]
    fn mock_instance_create_succeeds_with_quest_3s_config() {
        let dt = DispatchTable::unloaded();
        let cfg = MockInstanceConfig::quest_3s_default("CSSL-LoA-Test");
        let inst = MockInstance::create(&cfg, &dt).expect("create");
        assert!(inst.created);
        assert!(inst.quest_3s_minimum_extensions_present());
    }

    #[test]
    fn mock_instance_destroy_is_idempotent_failure_after_first() {
        let dt = DispatchTable::unloaded();
        let cfg = MockInstanceConfig::quest_3s_default("App");
        let mut inst = MockInstance::create(&cfg, &dt).expect("create");
        assert_eq!(inst.destroy(), XrResult::SUCCESS);
        assert_eq!(inst.destroy(), XrResult::ERROR_HANDLE_INVALID);
    }

    #[test]
    fn application_info_zero_terminates_app_name() {
        let info = ApplicationInfo::cssl_engine("CSSL-Demo");
        let n = info
            .application_name
            .iter()
            .position(|b| *b == 0)
            .expect("zero terminator");
        assert_eq!(&info.application_name[..n], b"CSSL-Demo");
        let en = info
            .engine_name
            .iter()
            .position(|b| *b == 0)
            .expect("zero terminator");
        assert_eq!(&info.engine_name[..en], b"CSSLv3");
    }

    #[test]
    fn quest_3s_runtime_advertises_required_extension_set() {
        let exts = quest_3s_runtime_advertised_extensions();
        assert!(exts.iter().any(|e| e.name() == "XR_KHR_vulkan_enable2"));
        assert!(exts.iter().any(|e| e.name() == "XR_FB_passthrough"));
        assert!(exts.iter().any(|e| e.name() == "XR_FB_body_tracking"));
        assert!(exts.iter().any(|e| e.name() == "XR_FB_face_tracking2"));
        assert!(exts.iter().any(|e| e.name() == "XR_EXT_eye_gaze_interaction"));
    }
}
