//! § ffi::types : Core OpenXR FFI types declared from-scratch.
//!
//! § SPEC : OpenXR 1.0 § 2.5 (Fundamentals - Type Definitions). Authored
//!          from `openxr.h` reference layout verbatim. All structs are
//!          `#[repr(C)]` so the FFI ABI matches the runtime exactly.
//!
//! § HANDLE-WIDTH
//!   OpenXR uses `XR_DEFINE_HANDLE(name)` which on 64-bit platforms
//!   expands to `typedef struct name##_T* name;` (i.e. a pointer-sized
//!   opaque type). On 32-bit platforms the spec mandates `uint64_t`. We
//!   uniformly model handles as `u64` here ; on 64-bit the runtime
//!   stuffs a pointer into the low 8 bytes (sound on x86_64 + AArch64
//!   where `usize == u64`).

/// 64-bit atom (`XR_DEFINE_ATOM`). Used for action / path identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(transparent)]
pub struct Atom(pub u64);

/// `XR_NULL_HANDLE` constant for opaque-handle nullness.
pub const NULL_HANDLE: u64 = 0;

/// OpenXR `XrTime` ; nanoseconds since an arbitrary epoch (`int64_t`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct Time(pub i64);

/// OpenXR `XrDuration` ; nanoseconds (`int64_t` ; can be negative).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct Duration(pub i64);

impl Duration {
    pub const NONE: Self = Self(0);
    pub const INFINITE: Self = Self(i64::MAX);

    #[must_use]
    pub const fn from_nanos(ns: i64) -> Self {
        Self(ns)
    }
    #[must_use]
    pub const fn from_millis(ms: i64) -> Self {
        Self(ms.saturating_mul(1_000_000))
    }
}

/// Maximum length of an application-name buffer. § 2.5 spec.
pub const XR_MAX_APPLICATION_NAME_SIZE: usize = 128;
/// Maximum length of an engine-name buffer.
pub const XR_MAX_ENGINE_NAME_SIZE: usize = 128;
/// Maximum length of a runtime-name buffer.
pub const XR_MAX_RUNTIME_NAME_SIZE: usize = 128;
/// Maximum length of a system-name buffer.
pub const XR_MAX_SYSTEM_NAME_SIZE: usize = 256;
/// Maximum length of an extension-name buffer.
pub const XR_MAX_EXTENSION_NAME_SIZE: usize = 128;
/// Maximum length of an API-layer-name buffer.
pub const XR_MAX_API_LAYER_NAME_SIZE: usize = 256;
/// Maximum length of an API-layer-description buffer.
pub const XR_MAX_API_LAYER_DESCRIPTION_SIZE: usize = 256;
/// Maximum length of a path string.
pub const XR_MAX_PATH_LENGTH: usize = 256;
/// Maximum length of an action-name string.
pub const XR_MAX_ACTION_NAME_SIZE: usize = 64;
/// Maximum length of an action-set-name string.
pub const XR_MAX_ACTION_SET_NAME_SIZE: usize = 64;
/// Maximum length of a localized-name string.
pub const XR_MAX_LOCALIZED_ACTION_NAME_SIZE: usize = 128;
pub const XR_MAX_LOCALIZED_ACTION_SET_NAME_SIZE: usize = 128;

/// OpenXR `XR_MAKE_VERSION(major, minor, patch)` packed into u64.
#[must_use]
pub const fn xr_make_version(major: u16, minor: u16, patch: u32) -> u64 {
    ((major as u64) << 48) | ((minor as u64) << 32) | (patch as u64)
}

/// OpenXR `XR_VERSION_MAJOR` accessor.
#[must_use]
pub const fn xr_version_major(v: u64) -> u16 {
    (v >> 48) as u16
}

/// OpenXR `XR_VERSION_MINOR` accessor.
#[must_use]
pub const fn xr_version_minor(v: u64) -> u16 {
    ((v >> 32) & 0xFFFF) as u16
}

/// OpenXR `XR_VERSION_PATCH` accessor.
#[must_use]
pub const fn xr_version_patch(v: u64) -> u32 {
    (v & 0xFFFF_FFFF) as u32
}

/// `XR_CURRENT_API_VERSION` per spec : 1.0.34 is the canonical post-1.1
/// floor for production ; we declare 1.1.0 as the default to match
/// `instance::ApiVersion::current()`.
pub const XR_CURRENT_API_VERSION: u64 = xr_make_version(1, 1, 0);

/// OpenXR `XrViewConfigurationType`. § 8.1 spec.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum ViewConfigurationType {
    PrimaryMono = 1,
    PrimaryStereo = 2,
    PrimaryQuadVarjo = 1_000_037_000,
    SecondaryMonoFirstPersonObserverMSFT = 1_000_054_000,
    PrimaryStereoWithFovMutableFB = 1_000_135_000,
}

impl ViewConfigurationType {
    /// Number of views the configuration emits (1 for mono, 2 for stereo,
    /// 4 for quad).
    #[must_use]
    pub const fn view_count(self) -> u32 {
        match self {
            Self::PrimaryMono | Self::SecondaryMonoFirstPersonObserverMSFT => 1,
            Self::PrimaryStereo | Self::PrimaryStereoWithFovMutableFB => 2,
            Self::PrimaryQuadVarjo => 4,
        }
    }
}

/// OpenXR `XrEnvironmentBlendMode`. § 8.4 spec.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum EnvironmentBlendMode {
    Opaque = 1,
    Additive = 2,
    AlphaBlend = 3,
}

/// OpenXR `XrStructureType` discriminator ; every `#[repr(C)]` struct that
/// participates in the chained-extension pattern carries one as its first
/// field. Subset declared here ; full set is enormous.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum StructureType {
    Unknown = 0,
    ApiLayerProperties = 1,
    ExtensionProperties = 2,
    InstanceCreateInfo = 3,
    SystemGetInfo = 4,
    SystemProperties = 5,
    ViewLocateInfo = 6,
    ViewState = 7,
    SessionCreateInfo = 8,
    SwapchainCreateInfo = 9,
    SessionBeginInfo = 10,
    ViewState2 = 11,
    ActionStateBoolean = 23,
    ActionStateFloat = 24,
    ActionStateVector2f = 25,
    ActionStatePose = 27,
    ActionSetCreateInfo = 28,
    ActionCreateInfo = 29,
    InstanceProperties = 32,
    FrameWaitInfo = 33,
    FrameState = 44,
    FrameBeginInfo = 46,
    FrameEndInfo = 12,
    HapticVibration = 100_000_034,
    InteractionProfileSuggestedBinding = 35,
    SessionActionSetsAttachInfo = 36,
    ActionsSyncInfo = 37,
    BoundSourcesForActionEnumerateInfo = 38,
    InputSourceLocalizedNameGetInfo = 40,
    EventDataBuffer = 16,
    EventDataInstanceLossPending = 17,
    EventDataSessionStateChanged = 18,
    SpaceLocation = 42,
    SpaceVelocity = 43,
}
