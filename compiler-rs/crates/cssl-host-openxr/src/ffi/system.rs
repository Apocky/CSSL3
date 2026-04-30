//! § ffi::system : `XrSystemId` + `XrSystemProperties` FFI surface.
//!
//! § SPEC : OpenXR 1.0 § 6 (System). The system is the physical headset
//!          + tracking hardware. `xrGetSystem` accepts a form-factor and
//!          returns an opaque `XrSystemId`. `xrGetSystemProperties`
//!          fills out a property struct.

use super::result::XrResult;
use super::types::{StructureType, XR_MAX_SYSTEM_NAME_SIZE};

/// FFI handle for `XrSystemId`. 64-bit atom (not a refcounted handle).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(transparent)]
pub struct SystemId(pub u64);

impl SystemId {
    pub const NULL: Self = Self(0);

    #[must_use]
    pub const fn is_null(self) -> bool {
        self.0 == 0
    }
}

/// `XrFormFactor`. § 6.1 spec.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum FormFactor {
    HeadMountedDisplay = 1,
    HandheldDisplay = 2,
}

/// `XrSystemGetInfo` ; FFI struct.
#[repr(C)]
pub struct SystemGetInfo {
    pub ty: StructureType,
    pub next: *const core::ffi::c_void,
    pub form_factor: FormFactor,
}

impl core::fmt::Debug for SystemGetInfo {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SystemGetInfo")
            .field("ty", &self.ty)
            .field("form_factor", &self.form_factor)
            .finish()
    }
}

impl SystemGetInfo {
    #[must_use]
    pub const fn hmd() -> Self {
        Self {
            ty: StructureType::SystemGetInfo,
            next: core::ptr::null(),
            form_factor: FormFactor::HeadMountedDisplay,
        }
    }
}

/// `XrSystemGraphicsProperties`. § 6.2 spec.
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct GraphicsProperties {
    pub max_swapchain_image_height: u32,
    pub max_swapchain_image_width: u32,
    pub max_layer_count: u32,
}

/// `XrSystemTrackingProperties`. § 6.2 spec.
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct TrackingProperties {
    /// 1 = orientation tracking supported.
    pub orientation_tracking: u32,
    /// 1 = position tracking supported (6DoF).
    pub position_tracking: u32,
}

/// `XrSystemProperties` aggregate.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct SystemProperties {
    pub ty: StructureType,
    pub next: *mut core::ffi::c_void,
    pub system_id: u64,
    pub vendor_id: u32,
    pub system_name: [u8; XR_MAX_SYSTEM_NAME_SIZE],
    pub graphics_properties: GraphicsProperties,
    pub tracking_properties: TrackingProperties,
}

impl SystemProperties {
    #[must_use]
    pub fn empty() -> Self {
        Self {
            ty: StructureType::SystemProperties,
            next: core::ptr::null_mut(),
            system_id: 0,
            vendor_id: 0,
            system_name: [0; XR_MAX_SYSTEM_NAME_SIZE],
            graphics_properties: GraphicsProperties::default(),
            tracking_properties: TrackingProperties::default(),
        }
    }

    /// Read the system-name as a UTF-8 slice (trim at first NUL).
    #[must_use]
    pub fn name(&self) -> &str {
        let n = self
            .system_name
            .iter()
            .position(|b| *b == 0)
            .unwrap_or(XR_MAX_SYSTEM_NAME_SIZE);
        core::str::from_utf8(&self.system_name[..n]).unwrap_or("")
    }
}

/// In-memory mock of an `XrSystem` for tests.
#[derive(Debug, Clone)]
pub struct MockSystem {
    pub system_id: SystemId,
    pub form_factor: FormFactor,
    pub properties: SystemProperties,
}

impl MockSystem {
    /// Pretend `xrGetSystem(HMD)` against a Quest-3s runtime. Returns a
    /// `MockSystem` with the canonical Quest-3s spec values.
    #[must_use]
    pub fn quest_3s_hmd() -> Self {
        let mut props = SystemProperties::empty();
        props.system_id = 0xC551_BEEF;
        props.vendor_id = 0x2833; // Meta Platforms PCI vendor id
        let name = b"Meta Quest 3S";
        props.system_name[..name.len()].copy_from_slice(name);
        // Quest-3s : 2064x2208 per-eye max swapchain ; 16-layer compositor cap.
        props.graphics_properties = GraphicsProperties {
            max_swapchain_image_width: 2064,
            max_swapchain_image_height: 2208,
            max_layer_count: 16,
        };
        props.tracking_properties = TrackingProperties {
            orientation_tracking: 1,
            position_tracking: 1,
        };
        Self {
            system_id: SystemId(0xC551_BEEF),
            form_factor: FormFactor::HeadMountedDisplay,
            properties: props,
        }
    }

    /// Pretend `xrGetSystem(handheld)` against a phone-AR runtime.
    #[must_use]
    pub fn phone_handheld_ar() -> Self {
        let mut props = SystemProperties::empty();
        props.system_id = 0x1111_2222;
        props.vendor_id = 0x1234;
        let name = b"Phone AR";
        props.system_name[..name.len()].copy_from_slice(name);
        props.tracking_properties = TrackingProperties {
            orientation_tracking: 1,
            position_tracking: 0,
        };
        Self {
            system_id: SystemId(0x1111_2222),
            form_factor: FormFactor::HandheldDisplay,
            properties: props,
        }
    }

    /// Mock `xrGetSystem` against a list of available systems.
    pub fn try_get(form_factor: FormFactor) -> Result<Self, XrResult> {
        match form_factor {
            FormFactor::HeadMountedDisplay => Ok(Self::quest_3s_hmd()),
            FormFactor::HandheldDisplay => Ok(Self::phone_handheld_ar()),
        }
    }

    /// `true` iff this system supports 6DoF position-tracking. Quest-3s yes,
    /// phone-AR no.
    #[must_use]
    pub fn has_6dof(&self) -> bool {
        self.properties.tracking_properties.position_tracking != 0
    }
}

#[cfg(test)]
mod tests {
    use super::{FormFactor, MockSystem};

    #[test]
    fn quest_3s_system_advertises_6dof() {
        let s = MockSystem::quest_3s_hmd();
        assert!(s.has_6dof());
        assert_eq!(s.form_factor, FormFactor::HeadMountedDisplay);
        assert_eq!(s.properties.name(), "Meta Quest 3S");
    }

    #[test]
    fn handheld_ar_lacks_position_tracking() {
        let s = MockSystem::phone_handheld_ar();
        assert!(!s.has_6dof());
    }

    #[test]
    fn try_get_resolves_form_factors() {
        let hmd = MockSystem::try_get(FormFactor::HeadMountedDisplay).unwrap();
        let phone = MockSystem::try_get(FormFactor::HandheldDisplay).unwrap();
        assert_eq!(hmd.form_factor, FormFactor::HeadMountedDisplay);
        assert_eq!(phone.form_factor, FormFactor::HandheldDisplay);
    }
}
