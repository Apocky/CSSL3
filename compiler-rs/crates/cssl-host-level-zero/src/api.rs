//! Level-Zero API surface enumeration.

use core::fmt;

/// Canonical L0 API entry-points CSSLv3 exercises.
///
/// Each variant maps to one `ze*` or `zes*` FFI entry-point ; the enumeration lets
/// the telemetry layer + diagnostic output name the exact API being called without
/// pulling in the FFI shim at stage-0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum L0ApiSurface {
    /// `zeInit` — process-level initialization.
    ZeInit,
    /// `zeDriverGet` — driver enumeration.
    ZeDriverGet,
    /// `zeDeviceGet` — device enumeration per driver.
    ZeDeviceGet,
    /// `zeDeviceGetProperties`.
    ZeDeviceGetProperties,
    /// `zeContextCreate`.
    ZeContextCreate,
    /// `zeCommandListCreate`.
    ZeCommandListCreate,
    /// `zeCommandListCreateImmediate`.
    ZeCommandListCreateImmediate,
    /// `zeEventPoolCreate`.
    ZeEventPoolCreate,
    /// `zeEventCreate`.
    ZeEventCreate,
    /// `zeModuleCreate` — consumes SPIR-V directly.
    ZeModuleCreate,
    /// `zeKernelCreate`.
    ZeKernelCreate,
    /// `zeCommandListAppendLaunchKernel`.
    ZeCommandListAppendLaunchKernel,
    /// `zeMemAllocDevice` — device USM.
    ZeMemAllocDevice,
    /// `zeMemAllocHost` — host USM.
    ZeMemAllocHost,
    /// `zeMemAllocShared` — shared USM.
    ZeMemAllocShared,
    /// `zesDeviceGetProperties` (sysman).
    ZesDeviceGetProperties,
    /// `zesPowerGetEnergyCounter`.
    ZesPowerGetEnergyCounter,
    /// `zesPowerSetLimits`.
    ZesPowerSetLimits,
    /// `zesTemperatureGetState`.
    ZesTemperatureGetState,
    /// `zesFrequencyGetState`.
    ZesFrequencyGetState,
    /// `zesFrequencyOcGet`.
    ZesFrequencyOcGet,
    /// `zesEngineGetActivity`.
    ZesEngineGetActivity,
    /// `zesRasGetState`.
    ZesRasGetState,
    /// `zesDeviceProcessesGetState`.
    ZesDeviceProcessesGetState,
}

impl L0ApiSurface {
    /// Canonical entry-point name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ZeInit => "zeInit",
            Self::ZeDriverGet => "zeDriverGet",
            Self::ZeDeviceGet => "zeDeviceGet",
            Self::ZeDeviceGetProperties => "zeDeviceGetProperties",
            Self::ZeContextCreate => "zeContextCreate",
            Self::ZeCommandListCreate => "zeCommandListCreate",
            Self::ZeCommandListCreateImmediate => "zeCommandListCreateImmediate",
            Self::ZeEventPoolCreate => "zeEventPoolCreate",
            Self::ZeEventCreate => "zeEventCreate",
            Self::ZeModuleCreate => "zeModuleCreate",
            Self::ZeKernelCreate => "zeKernelCreate",
            Self::ZeCommandListAppendLaunchKernel => "zeCommandListAppendLaunchKernel",
            Self::ZeMemAllocDevice => "zeMemAllocDevice",
            Self::ZeMemAllocHost => "zeMemAllocHost",
            Self::ZeMemAllocShared => "zeMemAllocShared",
            Self::ZesDeviceGetProperties => "zesDeviceGetProperties",
            Self::ZesPowerGetEnergyCounter => "zesPowerGetEnergyCounter",
            Self::ZesPowerSetLimits => "zesPowerSetLimits",
            Self::ZesTemperatureGetState => "zesTemperatureGetState",
            Self::ZesFrequencyGetState => "zesFrequencyGetState",
            Self::ZesFrequencyOcGet => "zesFrequencyOcGet",
            Self::ZesEngineGetActivity => "zesEngineGetActivity",
            Self::ZesRasGetState => "zesRasGetState",
            Self::ZesDeviceProcessesGetState => "zesDeviceProcessesGetState",
        }
    }

    /// True iff this is a sysman API (`zes*` prefix).
    #[must_use]
    pub const fn is_sysman(self) -> bool {
        matches!(
            self,
            Self::ZesDeviceGetProperties
                | Self::ZesPowerGetEnergyCounter
                | Self::ZesPowerSetLimits
                | Self::ZesTemperatureGetState
                | Self::ZesFrequencyGetState
                | Self::ZesFrequencyOcGet
                | Self::ZesEngineGetActivity
                | Self::ZesRasGetState
                | Self::ZesDeviceProcessesGetState
        )
    }
}

impl fmt::Display for L0ApiSurface {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Unified-shared-memory allocation class (USM).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UsmAllocType {
    /// Host-resident, device-accessible.
    Host,
    /// Device-resident, CPU-unmapped.
    Device,
    /// Shared — transparently migrates between host + device.
    Shared,
}

impl UsmAllocType {
    /// Short name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Host => "host",
            Self::Device => "device",
            Self::Shared => "shared",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{L0ApiSurface, UsmAllocType};

    #[test]
    fn api_names() {
        assert_eq!(L0ApiSurface::ZeInit.as_str(), "zeInit");
        assert_eq!(L0ApiSurface::ZeModuleCreate.as_str(), "zeModuleCreate");
        assert_eq!(
            L0ApiSurface::ZesPowerGetEnergyCounter.as_str(),
            "zesPowerGetEnergyCounter"
        );
    }

    #[test]
    fn sysman_flag() {
        assert!(L0ApiSurface::ZesPowerGetEnergyCounter.is_sysman());
        assert!(L0ApiSurface::ZesTemperatureGetState.is_sysman());
        assert!(!L0ApiSurface::ZeInit.is_sysman());
        assert!(!L0ApiSurface::ZeModuleCreate.is_sysman());
    }

    #[test]
    fn usm_alloc_types() {
        assert_eq!(UsmAllocType::Host.as_str(), "host");
        assert_eq!(UsmAllocType::Device.as_str(), "device");
        assert_eq!(UsmAllocType::Shared.as_str(), "shared");
    }
}
