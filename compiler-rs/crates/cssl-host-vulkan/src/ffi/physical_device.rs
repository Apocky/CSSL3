//! § ffi/physical_device : enumerate + Arc-A770-preference picker
//!                         (T11-D65, S6-E1).
//!
//! § ROLE
//!   Wraps `vkEnumeratePhysicalDevices` + `vkGetPhysicalDeviceProperties`
//!   + `vkGetPhysicalDeviceQueueFamilyProperties` ; produces a scored
//!   list of candidate devices with the canonical Arc A770 (Intel
//!   `device_id = 0x56A0`) preferred. Other vendors get lower scores
//!   but still appear so portable testing on non-Intel hardware works.
//!
//! § SCORING
//!   - Arc A770 (Intel + 0x56A0)              : 1000
//!   - Other Intel discrete GPU               :  800
//!   - Any discrete GPU                       :  500
//!   - Integrated GPU                         :  300
//!   - Virtual / CPU implementation           :  100
//!   This means `pick_for_arc_a770_or_best` always returns the A770
//!   when present, and a portable best-pick on other hosts.

#![allow(unsafe_code)]

use std::ffi::CStr;

use ash::vk;

use crate::ffi::error::{AshError, VkResultDisplay};
use crate::ffi::instance::VkInstanceHandle;

/// Resolved properties of a single physical device.
#[derive(Debug, Clone)]
pub struct ScoredPhysical {
    /// Underlying ash handle.
    pub raw: vk::PhysicalDevice,
    /// `VkPhysicalDeviceProperties::deviceName`.
    pub name: String,
    /// PCI vendor ID.
    pub vendor_id: u32,
    /// PCI device ID.
    pub device_id: u32,
    /// `VkPhysicalDeviceProperties::deviceType`.
    pub device_type: vk::PhysicalDeviceType,
    /// `VkPhysicalDeviceProperties::apiVersion`.
    pub api_version: u32,
    /// Score per scoring policy above.
    pub score: i32,
    /// Available queue families (index, capability bits, count).
    pub queue_families: Vec<QueueFamilyInfo>,
}

/// Capability summary of a single queue family slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueueFamilyInfo {
    /// `vkGetPhysicalDeviceQueueFamilyProperties` index.
    pub index: u32,
    /// `VkQueueFamilyProperties::queueFlags`.
    pub flags: vk::QueueFlags,
    /// `VkQueueFamilyProperties::queueCount`.
    pub count: u32,
}

impl QueueFamilyInfo {
    /// True iff this family supports graphics operations.
    #[must_use]
    pub fn supports_graphics(&self) -> bool {
        self.flags.contains(vk::QueueFlags::GRAPHICS)
    }

    /// True iff this family supports compute.
    #[must_use]
    pub fn supports_compute(&self) -> bool {
        self.flags.contains(vk::QueueFlags::COMPUTE)
    }

    /// True iff this family supports transfer.
    #[must_use]
    pub fn supports_transfer(&self) -> bool {
        // Per Vulkan spec : graphics or compute queues implicitly
        // support transfer.
        self.flags.contains(vk::QueueFlags::TRANSFER)
            || self.supports_graphics()
            || self.supports_compute()
    }
}

/// Result of `pick_*` : the chosen device + the queue-family index
/// selected for graphics + compute.
#[derive(Debug, Clone)]
pub struct PhysicalDevicePick {
    /// Chosen device.
    pub device: ScoredPhysical,
    /// Queue family index satisfying graphics + compute.
    pub graphics_compute_family: u32,
}

/// Enumerate every physical device the loader sees, scoring each.
///
/// # Errors
/// [`AshError::EnumeratePhysical`] propagated from the driver.
pub fn enumerate(instance: &VkInstanceHandle) -> Result<Vec<ScoredPhysical>, AshError> {
    // SAFETY : `instance.raw()` is alive ; `enumerate_physical_devices`
    // signature requires no extra preconditions.
    let raw_list = unsafe { instance.raw().enumerate_physical_devices() }
        .map_err(|r| AshError::EnumeratePhysical(VkResultDisplay::from(r)))?;

    let mut out = Vec::with_capacity(raw_list.len());
    for handle in raw_list {
        let scored = score_one(instance.raw(), handle);
        out.push(scored);
    }
    // Sort descending by score so callers can pick `out[0]`.
    out.sort_by(|a, b| b.score.cmp(&a.score));
    Ok(out)
}

/// Score policy : Arc A770 first, then Intel discrete, then any
/// discrete, then integrated, then everything else.
fn score_one(instance: &ash::Instance, handle: vk::PhysicalDevice) -> ScoredPhysical {
    // SAFETY : `handle` was just returned by `enumerate_physical_devices`
    // ; queue-property / properties calls are safe with any valid
    // physical-device handle.
    let props = unsafe { instance.get_physical_device_properties(handle) };
    let name = device_name_from_props(&props);
    let vendor_id = props.vendor_id;
    let device_id = props.device_id;
    let device_type = props.device_type;
    let api_version = props.api_version;

    let raw_q = unsafe { instance.get_physical_device_queue_family_properties(handle) };
    let queue_families: Vec<QueueFamilyInfo> = raw_q
        .iter()
        .enumerate()
        .map(|(i, qf)| QueueFamilyInfo {
            index: u32::try_from(i).unwrap_or(u32::MAX),
            flags: qf.queue_flags,
            count: qf.queue_count,
        })
        .collect();

    let score = compute_score(vendor_id, device_id, device_type);

    ScoredPhysical {
        raw: handle,
        name,
        vendor_id,
        device_id,
        device_type,
        api_version,
        score,
        queue_families,
    }
}

/// Pure scoring fn — extracted so unit tests can exercise it without a
/// real driver.
#[must_use]
pub fn compute_score(vendor_id: u32, device_id: u32, device_type: vk::PhysicalDeviceType) -> i32 {
    const INTEL_VID: u32 = 0x8086;
    const ARC_A770_DID: u32 = 0x56A0;

    if vendor_id == INTEL_VID && device_id == ARC_A770_DID {
        return 1000;
    }
    if vendor_id == INTEL_VID && device_type == vk::PhysicalDeviceType::DISCRETE_GPU {
        return 800;
    }
    match device_type {
        vk::PhysicalDeviceType::DISCRETE_GPU => 500,
        vk::PhysicalDeviceType::INTEGRATED_GPU => 300,
        vk::PhysicalDeviceType::VIRTUAL_GPU | vk::PhysicalDeviceType::CPU => 100,
        _ => 50,
    }
}

/// Extract the device name from `VkPhysicalDeviceProperties::deviceName`
/// (256-byte fixed array of i8).
fn device_name_from_props(props: &vk::PhysicalDeviceProperties) -> String {
    // SAFETY : ash exposes `device_name` as `[i8; 256]` ; reading via
    // `from_ptr` on a non-null pointer is sound provided we stop at
    // the first NUL ; `CStr::from_ptr` does that.
    let cstr = unsafe { CStr::from_ptr(props.device_name.as_ptr()) };
    cstr.to_string_lossy().into_owned()
}

/// Pick the Arc A770 if present, else the highest-scoring device.
/// The returned [`PhysicalDevicePick`] also carries the queue-family
/// index that supports both graphics + compute.
///
/// # Errors
/// - [`AshError::EnumeratePhysical`] propagated from `enumerate`.
/// - [`AshError::NoSuitableDevice`] if no device exposes a
///   graphics+compute queue-family.
pub fn pick_for_arc_a770_or_best(
    instance: &VkInstanceHandle,
) -> Result<PhysicalDevicePick, AshError> {
    let devices = enumerate(instance)?;
    if devices.is_empty() {
        return Err(AshError::NoSuitableDevice("0 devices enumerated".into()));
    }

    // First-pass : prefer A770 if present.
    let chosen = devices
        .iter()
        .find(|d| d.vendor_id == 0x8086 && d.device_id == 0x56A0)
        .cloned()
        .unwrap_or_else(|| devices[0].clone());

    let family = chosen
        .queue_families
        .iter()
        .find(|qf| qf.supports_graphics() && qf.supports_compute())
        .map(|qf| qf.index)
        .ok_or_else(|| {
            AshError::NoSuitableDevice(format!(
                "device `{}` exposes no graphics+compute queue-family",
                chosen.name
            ))
        })?;

    Ok(PhysicalDevicePick {
        device: chosen,
        graphics_compute_family: family,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn score_arc_a770_is_1000() {
        let s = compute_score(0x8086, 0x56A0, vk::PhysicalDeviceType::DISCRETE_GPU);
        assert_eq!(s, 1000);
    }

    #[test]
    fn score_intel_other_discrete_is_800() {
        let s = compute_score(0x8086, 0x1234, vk::PhysicalDeviceType::DISCRETE_GPU);
        assert_eq!(s, 800);
    }

    #[test]
    fn score_nvidia_discrete_is_500() {
        let s = compute_score(0x10DE, 0x2204, vk::PhysicalDeviceType::DISCRETE_GPU);
        assert_eq!(s, 500);
    }

    #[test]
    fn score_integrated_is_300() {
        let s = compute_score(0x8086, 0x9A49, vk::PhysicalDeviceType::INTEGRATED_GPU);
        assert_eq!(s, 300);
    }

    #[test]
    fn score_cpu_is_100() {
        let s = compute_score(0x10005, 0, vk::PhysicalDeviceType::CPU);
        assert_eq!(s, 100);
    }

    #[test]
    fn score_other_device_type_is_50() {
        let s = compute_score(0xDEAD, 0xBEEF, vk::PhysicalDeviceType::OTHER);
        assert_eq!(s, 50);
    }

    #[test]
    fn queue_family_graphics_implies_transfer() {
        let qf = QueueFamilyInfo {
            index: 0,
            flags: vk::QueueFlags::GRAPHICS,
            count: 1,
        };
        assert!(qf.supports_graphics());
        assert!(qf.supports_transfer());
    }

    #[test]
    fn queue_family_compute_implies_transfer() {
        let qf = QueueFamilyInfo {
            index: 0,
            flags: vk::QueueFlags::COMPUTE,
            count: 1,
        };
        assert!(qf.supports_compute());
        assert!(qf.supports_transfer());
    }

    #[test]
    fn queue_family_explicit_transfer() {
        let qf = QueueFamilyInfo {
            index: 0,
            flags: vk::QueueFlags::TRANSFER,
            count: 1,
        };
        assert!(qf.supports_transfer());
        assert!(!qf.supports_graphics());
        assert!(!qf.supports_compute());
    }
}
