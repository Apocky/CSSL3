//! L0 driver + device enumeration.

/// `ze_device_type_t` mirror.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum L0DeviceType {
    /// `ZE_DEVICE_TYPE_GPU`.
    Gpu,
    /// `ZE_DEVICE_TYPE_CPU`.
    Cpu,
    /// `ZE_DEVICE_TYPE_FPGA`.
    Fpga,
    /// `ZE_DEVICE_TYPE_MCA` (Media Content Accelerator).
    Mca,
    /// `ZE_DEVICE_TYPE_VPU` (Vision-Processing Unit).
    Vpu,
}

impl L0DeviceType {
    /// Short name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Gpu => "gpu",
            Self::Cpu => "cpu",
            Self::Fpga => "fpga",
            Self::Mca => "mca",
            Self::Vpu => "vpu",
        }
    }
}

/// L0 device properties (mirrors the fields we care about from `ze_device_properties_t`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct L0DeviceProperties {
    /// Device name.
    pub name: String,
    /// Device type.
    pub device_type: L0DeviceType,
    /// Vendor-ID (Intel 0x8086 ; uniformly on L0 for now).
    pub vendor_id: u32,
    /// Device-ID (PCI).
    pub device_id: u32,
    /// SSCID (sub-system) / core-clock rate (MHz).
    pub core_clock_rate_mhz: u32,
    /// Max compute-unit count (Xe-cores on Arc).
    pub max_compute_units: u32,
    /// Global-memory size in MB.
    pub global_memory_mb: u32,
    /// Max workgroup size along a single dimension.
    pub max_workgroup_size: u32,
    /// L0 API version exposed (major.minor encoded : `1_14` = 1.14+).
    pub api_major: u16,
    pub api_minor: u16,
}

/// L0 device handle (stage-0 opaque structure).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct L0Device {
    /// Driver index that exposed this device.
    pub driver_index: u32,
    /// Device index within the driver.
    pub device_index: u32,
    /// Device properties.
    pub properties: L0DeviceProperties,
}

/// L0 driver handle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct L0Driver {
    /// Driver index (`zeDriverGet` ordinal).
    pub index: u32,
    /// API version major.
    pub api_major: u16,
    /// API version minor.
    pub api_minor: u16,
    /// Devices exposed by this driver.
    pub devices: Vec<L0Device>,
}

impl L0Driver {
    /// Stage-0 stub : one driver exposing a canonical Intel Arc A770 device.
    #[must_use]
    pub fn stub_arc_a770() -> Self {
        let props = L0DeviceProperties {
            name: "Intel(R) Arc(TM) A770 Graphics".into(),
            device_type: L0DeviceType::Gpu,
            vendor_id: 0x8086,
            device_id: 0x56A0,
            core_clock_rate_mhz: 2100,
            max_compute_units: 32,
            global_memory_mb: 16 * 1024,
            max_workgroup_size: 1024,
            api_major: 1,
            api_minor: 14,
        };
        Self {
            index: 0,
            api_major: 1,
            api_minor: 14,
            devices: vec![L0Device {
                driver_index: 0,
                device_index: 0,
                properties: props,
            }],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{L0Device, L0DeviceType, L0Driver};

    #[test]
    fn device_type_names() {
        assert_eq!(L0DeviceType::Gpu.as_str(), "gpu");
        assert_eq!(L0DeviceType::Fpga.as_str(), "fpga");
    }

    #[test]
    fn stub_driver_exposes_arc_a770() {
        let d = L0Driver::stub_arc_a770();
        assert_eq!(d.api_major, 1);
        assert!(d.api_minor >= 14);
        assert_eq!(d.devices.len(), 1);
        let dev: &L0Device = &d.devices[0];
        assert_eq!(dev.properties.vendor_id, 0x8086);
        assert_eq!(dev.properties.device_id, 0x56A0);
        assert_eq!(dev.properties.device_type, L0DeviceType::Gpu);
        assert_eq!(dev.properties.max_compute_units, 32);
        assert_eq!(dev.properties.global_memory_mb, 16 * 1024);
    }

    #[test]
    fn stub_device_name() {
        let d = L0Driver::stub_arc_a770();
        assert!(d.devices[0].properties.name.contains("Arc"));
    }
}
