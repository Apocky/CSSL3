//! Metal device + GPU-family enumeration.

/// Apple GPU-family identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum GpuFamily {
    /// Apple1 (A7).
    Apple1,
    /// Apple2 (A8).
    Apple2,
    /// Apple3 (A9 / A10).
    Apple3,
    /// Apple4 (A11).
    Apple4,
    /// Apple5 (A12 / A12X).
    Apple5,
    /// Apple6 (A13).
    Apple6,
    /// Apple7 (A14 / M1).
    Apple7,
    /// Apple8 (A15 / M2).
    Apple8,
    /// Apple9 (A17 / M3).
    Apple9,
    /// Mac1 (Intel Macs + Apple-Silicon compat).
    Mac1,
    /// Mac2 (M1/M2/M3 native).
    Mac2,
    /// Common1 — shared feature subset.
    Common1,
    /// Common2 — shared feature subset.
    Common2,
    /// Common3 — shared feature subset.
    Common3,
}

impl GpuFamily {
    /// Short name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Apple1 => "apple1",
            Self::Apple2 => "apple2",
            Self::Apple3 => "apple3",
            Self::Apple4 => "apple4",
            Self::Apple5 => "apple5",
            Self::Apple6 => "apple6",
            Self::Apple7 => "apple7",
            Self::Apple8 => "apple8",
            Self::Apple9 => "apple9",
            Self::Mac1 => "mac1",
            Self::Mac2 => "mac2",
            Self::Common1 => "common1",
            Self::Common2 => "common2",
            Self::Common3 => "common3",
        }
    }

    /// All 14 families.
    pub const ALL_FAMILIES: [Self; 14] = [
        Self::Apple1,
        Self::Apple2,
        Self::Apple3,
        Self::Apple4,
        Self::Apple5,
        Self::Apple6,
        Self::Apple7,
        Self::Apple8,
        Self::Apple9,
        Self::Mac1,
        Self::Mac2,
        Self::Common1,
        Self::Common2,
        Self::Common3,
    ];
}

/// Metal device record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtlDevice {
    /// Device name.
    pub name: String,
    /// Registry ID (`[MTLDevice registryID]`).
    pub registry_id: u64,
    /// Supports ray-tracing-2 (Apple9+ / M3+).
    pub supports_raytracing: bool,
    /// Supports function-pointers (Metal-3).
    pub supports_function_pointers: bool,
    /// Supports dynamic-libraries (Metal-3+).
    pub supports_dynamic_libraries: bool,
    /// Max buffer length (bytes).
    pub max_buffer_length: u64,
    /// Unified-memory (Apple-Silicon) vs discrete-memory (Intel Mac + AMD eGPU).
    pub has_unified_memory: bool,
    /// Primary GPU family.
    pub gpu_family: GpuFamily,
}

impl MtlDevice {
    /// Stub M3-Max device record.
    #[must_use]
    pub fn stub_m3_max() -> Self {
        Self {
            name: "Apple M3 Max".into(),
            registry_id: 0x0000_0001,
            supports_raytracing: true,
            supports_function_pointers: true,
            supports_dynamic_libraries: true,
            max_buffer_length: 128 * 1024 * 1024 * 1024,
            has_unified_memory: true,
            gpu_family: GpuFamily::Apple9,
        }
    }

    /// Stub Intel Mac Pro (discrete) device record.
    #[must_use]
    pub fn stub_intel_mac() -> Self {
        Self {
            name: "AMD Radeon Pro W6800X".into(),
            registry_id: 0x0000_0002,
            supports_raytracing: false,
            supports_function_pointers: false,
            supports_dynamic_libraries: false,
            max_buffer_length: 8 * 1024 * 1024 * 1024,
            has_unified_memory: false,
            gpu_family: GpuFamily::Mac1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{GpuFamily, MtlDevice};

    #[test]
    fn gpu_family_names() {
        assert_eq!(GpuFamily::Apple7.as_str(), "apple7");
        assert_eq!(GpuFamily::Mac2.as_str(), "mac2");
    }

    #[test]
    fn gpu_family_count() {
        assert_eq!(GpuFamily::ALL_FAMILIES.len(), 14);
    }

    #[test]
    fn m3_max_has_raytracing_and_unified_memory() {
        let d = MtlDevice::stub_m3_max();
        assert!(d.supports_raytracing);
        assert!(d.supports_function_pointers);
        assert!(d.has_unified_memory);
        assert_eq!(d.gpu_family, GpuFamily::Apple9);
    }

    #[test]
    fn intel_mac_no_raytracing() {
        let d = MtlDevice::stub_intel_mac();
        assert!(!d.supports_raytracing);
        assert!(!d.has_unified_memory);
        assert_eq!(d.gpu_family, GpuFamily::Mac1);
    }
}
