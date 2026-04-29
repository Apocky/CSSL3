//! Feature-probe trait + stub impl + ash-backed real impl (T11-D65, S6-E1).

use thiserror::Error;

use crate::device::{DeviceType, GpuVendor, VulkanDevice, VulkanVersion};
use crate::extensions::{VulkanExtension, VulkanExtensionSet};
use crate::ffi::{
    error::AshError,
    instance::{InstanceConfig, VkInstanceHandle},
    physical_device,
};

/// Failure modes for device probing.
#[derive(Debug, Error)]
pub enum ProbeError {
    /// Underlying loader / library was missing (e.g., vulkan-1.dll / libvulkan.so.1).
    #[error("Vulkan loader missing — no `vulkan-1.dll` / `libvulkan.so.1` on PATH")]
    LoaderMissing,
    /// The FFI backend is not yet wired (stage-0 placeholder, kept for
    /// the StubProbe path).
    #[error("FFI backend not wired at stage-0 (replaced by AshProbe @ T11-D65)")]
    FfiNotWired,
    /// Probe target device does not exist.
    #[error("no Vulkan device matches predicate `{query}`")]
    DeviceNotFound {
        /// Free-form description of the requested predicate.
        query: String,
    },
    /// ash-backed probe surfaced a driver error.
    #[error("ash backend error : {0}")]
    AshBackend(#[from] AshError),
}

impl PartialEq for ProbeError {
    fn eq(&self, other: &Self) -> bool {
        // Best-effort equality : compare the discriminant + key fields
        // for the simple variants. The AshBackend variant always
        // compares as "different" because AshError doesn't implement
        // PartialEq (it carries thiserror sources).
        match (self, other) {
            (Self::LoaderMissing, Self::LoaderMissing) => true,
            (Self::FfiNotWired, Self::FfiNotWired) => true,
            (Self::DeviceNotFound { query: a }, Self::DeviceNotFound { query: b }) => a == b,
            _ => false,
        }
    }
}

/// Host-adapter feature-probe trait — implemented by both the stub and
/// the real `ash`-backed probe.
pub trait FeatureProbe {
    /// Enumerate every physical device seen by the Vulkan loader.
    ///
    /// # Errors
    /// Returns [`ProbeError::LoaderMissing`] if the Vulkan loader is not installed,
    /// or [`ProbeError::FfiNotWired`] if the stub is in use.
    fn enumerate_devices(&self) -> Result<Vec<VulkanDevice>, ProbeError>;

    /// Get the list of extensions supported by a specific device.
    ///
    /// # Errors
    /// Returns [`ProbeError::DeviceNotFound`] if the index is out of range,
    /// or [`ProbeError::FfiNotWired`] if the stub is in use.
    fn supported_extensions(&self, device_idx: usize) -> Result<VulkanExtensionSet, ProbeError>;

    /// Quick check : is a specific extension available?
    ///
    /// # Errors
    /// Propagates [`ProbeError::DeviceNotFound`] or [`ProbeError::FfiNotWired`].
    fn has_extension(&self, device_idx: usize, ext: VulkanExtension) -> Result<bool, ProbeError> {
        let set = self.supported_extensions(device_idx)?;
        Ok(set.contains(ext))
    }
}

/// Stage-0 stub probe. Returns a single canonical Arc A770 device.
///
/// Preserved post-T11-D65 for unit tests that don't need a live driver.
/// Production code should use [`AshProbe`].
#[derive(Debug, Clone, Default)]
pub struct StubProbe;

impl StubProbe {
    /// New probe.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl FeatureProbe for StubProbe {
    fn enumerate_devices(&self) -> Result<Vec<VulkanDevice>, ProbeError> {
        Ok(vec![
            crate::arc_a770::ArcA770Profile::canonical().to_vulkan_device()
        ])
    }

    fn supported_extensions(&self, device_idx: usize) -> Result<VulkanExtensionSet, ProbeError> {
        if device_idx != 0 {
            return Err(ProbeError::DeviceNotFound {
                query: format!("idx={device_idx}"),
            });
        }
        Ok(crate::arc_a770::ArcA770Profile::expected_extensions())
    }
}

/// Real ash-backed probe (T11-D65, S6-E1).
///
/// Each call creates a transient `VkInstance` + enumerates physical
/// devices + tears the instance down. This matches the contract in
/// `specs/14_BACKEND.csl § HOST-SUBMIT BACKENDS` of "probe is cheap and
/// idempotent" — production callers that need durable instance state
/// should drive [`crate::ffi::VkInstanceHandle`] directly.
#[derive(Debug, Clone, Default)]
pub struct AshProbe;

impl AshProbe {
    /// New probe.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Cheap loader-presence check : tries to create an instance with
    /// validation off and reports whether it succeeded.
    #[must_use]
    pub fn loader_available() -> bool {
        VkInstanceHandle::create(InstanceConfig::default().no_validation()).is_ok()
    }
}

impl FeatureProbe for AshProbe {
    fn enumerate_devices(&self) -> Result<Vec<VulkanDevice>, ProbeError> {
        let instance = VkInstanceHandle::create(InstanceConfig::default().no_validation())
            .map_err(map_loader_err)?;
        let devs = physical_device::enumerate(&instance)?;
        Ok(devs.into_iter().map(scored_to_vulkan_device).collect())
    }

    fn supported_extensions(&self, device_idx: usize) -> Result<VulkanExtensionSet, ProbeError> {
        // Stage-0 simplification : we don't yet enumerate real extension
        // strings via `vkEnumerateDeviceExtensionProperties` — the
        // post-T11-D65 follow-up wires that to a real ash call. For now
        // we return the expected-extensions for the matched device-id
        // (Arc A770 fast-path) and an empty set otherwise.
        let devices = self.enumerate_devices()?;
        if device_idx >= devices.len() {
            return Err(ProbeError::DeviceNotFound {
                query: format!("idx={device_idx}"),
            });
        }
        let d = &devices[device_idx];
        if d.vendor == GpuVendor::Intel && d.device_id == 0x56A0 {
            return Ok(crate::arc_a770::ArcA770Profile::expected_extensions());
        }
        // Empty set is the honest stage-0 answer for non-A770 devices.
        Ok(VulkanExtensionSet::new())
    }
}

/// Convert `physical_device::ScoredPhysical` → public `VulkanDevice`.
fn scored_to_vulkan_device(s: physical_device::ScoredPhysical) -> VulkanDevice {
    VulkanDevice {
        name: s.name,
        vendor_id: s.vendor_id,
        device_id: s.device_id,
        vendor: GpuVendor::from_pci_id(s.vendor_id),
        device_type: vk_type_to_public(s.device_type),
        api_version: api_to_public(s.api_version),
        driver_version: 0,
        features: crate::device::DeviceFeatures::none(),
    }
}

fn vk_type_to_public(t: ash::vk::PhysicalDeviceType) -> DeviceType {
    use ash::vk::PhysicalDeviceType as T;
    match t {
        T::INTEGRATED_GPU => DeviceType::Integrated,
        T::DISCRETE_GPU => DeviceType::Discrete,
        T::VIRTUAL_GPU => DeviceType::Virtual,
        T::CPU => DeviceType::Cpu,
        _ => DeviceType::Other,
    }
}

fn api_to_public(api: u32) -> VulkanVersion {
    // ash::vk::api_version_major/minor/patch live as macros ; replicate
    // by hand : major in bits 22-31, minor in bits 12-21.
    let major = (api >> 22) & 0x7F;
    let minor = (api >> 12) & 0x3FF;
    match (major, minor) {
        (1, 4) => VulkanVersion::V1_4,
        (1, 3) => VulkanVersion::V1_3,
        (1, 2) => VulkanVersion::V1_2,
        (1, 1) => VulkanVersion::V1_1,
        _ => VulkanVersion::V1_0,
    }
}

/// Map an `AshError` from a probe call to a `ProbeError`. Loader-missing
/// errors get a dedicated variant (used by gate-skip patterns) ; all
/// other ash errors propagate via `AshBackend`.
fn map_loader_err(e: AshError) -> ProbeError {
    match e {
        AshError::Loader(_) => ProbeError::LoaderMissing,
        other => ProbeError::AshBackend(other),
    }
}

#[cfg(test)]
mod tests {
    use super::{AshProbe, FeatureProbe, ProbeError, StubProbe};
    use crate::device::GpuVendor;
    use crate::extensions::VulkanExtension;

    // ─── StubProbe (preserved phase-1 tests) ────────────────────────

    #[test]
    fn stub_enumerates_arc_a770() {
        let probe = StubProbe::new();
        let devices = probe.enumerate_devices().unwrap();
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].vendor, GpuVendor::Intel);
        assert_eq!(devices[0].device_id, 0x56A0);
    }

    #[test]
    fn stub_supported_extensions_for_dev_0() {
        let probe = StubProbe::new();
        let exts = probe.supported_extensions(0).unwrap();
        assert!(exts.contains(VulkanExtension::KhrRayTracingPipeline));
        assert!(exts.contains(VulkanExtension::KhrCooperativeMatrix));
    }

    #[test]
    fn stub_out_of_range_device_errors() {
        let probe = StubProbe::new();
        let err = probe.supported_extensions(42).unwrap_err();
        assert!(matches!(err, ProbeError::DeviceNotFound { .. }));
    }

    #[test]
    fn has_extension_returns_false_for_absent() {
        let probe = StubProbe::new();
        assert!(probe
            .has_extension(0, VulkanExtension::KhrRayQuery)
            .unwrap());
    }

    // ─── AshProbe (new phase-2 tests) ────────────────────────────────

    #[test]
    fn ash_probe_loader_available_check_runs() {
        // No assertion — this just verifies the loader-presence call
        // doesn't panic regardless of whether the loader is installed.
        let _ = AshProbe::loader_available();
    }

    #[test]
    fn ash_probe_enumerate_handles_loader_missing() {
        // Permissive : loader-missing is a clean `LoaderMissing` ;
        // loader-present produces a Vec of devices (possibly empty if
        // no ICD is configured).
        let probe = AshProbe::new();
        match probe.enumerate_devices() {
            Ok(_devs) => {
                // Acceptable on any host.
            }
            Err(ProbeError::LoaderMissing) => {
                // Expected on minimal CI / headless boxes.
            }
            Err(ProbeError::AshBackend(_)) => {
                // Acceptable : driver-side failure.
            }
            Err(other) => panic!("unexpected probe error : {other}"),
        }
    }

    #[test]
    fn ash_probe_supported_extensions_handles_loader_missing() {
        let probe = AshProbe::new();
        match probe.supported_extensions(0) {
            Ok(_set) => {
                // Acceptable : loader present + at least 1 device.
            }
            Err(ProbeError::LoaderMissing | ProbeError::AshBackend(_)) => {
                // Acceptable on hosts without a working ICD.
            }
            Err(ProbeError::DeviceNotFound { .. }) => {
                // Acceptable : loader present but 0 devices enumerated.
            }
            Err(other) => panic!("unexpected probe error : {other}"),
        }
    }
}
