//! DXGI adapter + D3D12 feature-level.

use core::fmt;

/// D3D_FEATURE_LEVEL enumeration (subset used by CSSLv3 : 12.0+).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FeatureLevel {
    /// D3D_FEATURE_LEVEL_12_0.
    Fl12_0,
    /// D3D_FEATURE_LEVEL_12_1.
    Fl12_1,
    /// D3D_FEATURE_LEVEL_12_2.
    Fl12_2,
}

impl FeatureLevel {
    /// Dotted form.
    #[must_use]
    pub const fn dotted(self) -> &'static str {
        match self {
            Self::Fl12_0 => "12.0",
            Self::Fl12_1 => "12.1",
            Self::Fl12_2 => "12.2",
        }
    }

    /// Canonical D3D_FEATURE_LEVEL_* integer (0xc000 = 12.0, 0xc100 = 12.1, 0xc200 = 12.2).
    #[must_use]
    pub const fn as_u32(self) -> u32 {
        match self {
            Self::Fl12_0 => 0xc000,
            Self::Fl12_1 => 0xc100,
            Self::Fl12_2 => 0xc200,
        }
    }

    /// All 3 feature levels.
    pub const ALL_LEVELS: [Self; 3] = [Self::Fl12_0, Self::Fl12_1, Self::Fl12_2];
}

impl fmt::Display for FeatureLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.dotted())
    }
}

/// DXGI adapter record (identifying data from `IDXGIAdapter4::GetDesc3`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DxgiAdapter {
    /// Description string.
    pub description: String,
    /// PCI vendor ID.
    pub vendor_id: u32,
    /// PCI device ID.
    pub device_id: u32,
    /// Sub-system-ID.
    pub sub_sys_id: u32,
    /// Revision.
    pub revision: u32,
    /// Dedicated video memory (bytes).
    pub dedicated_video_memory: u64,
    /// Dedicated system memory (bytes).
    pub dedicated_system_memory: u64,
    /// Shared system memory (bytes).
    pub shared_system_memory: u64,
    /// Feature level exposed.
    pub feature_level: FeatureLevel,
    /// Is this adapter software (WARP / reference) ?
    pub is_software: bool,
}

impl DxgiAdapter {
    /// Stub Arc A770 adapter record.
    #[must_use]
    pub fn stub_arc_a770() -> Self {
        Self {
            description: "Intel(R) Arc(TM) A770 Graphics".into(),
            vendor_id: 0x8086,
            device_id: 0x56A0,
            sub_sys_id: 0x00000000,
            revision: 0x08,
            dedicated_video_memory: 16 * 1024 * 1024 * 1024,
            dedicated_system_memory: 0,
            shared_system_memory: 16 * 1024 * 1024 * 1024,
            feature_level: FeatureLevel::Fl12_2,
            is_software: false,
        }
    }

    /// Stub Microsoft Basic / WARP software adapter.
    #[must_use]
    pub fn stub_warp() -> Self {
        Self {
            description: "Microsoft Basic Render Driver".into(),
            vendor_id: 0x1414,
            device_id: 0x008c,
            sub_sys_id: 0,
            revision: 0,
            dedicated_video_memory: 0,
            dedicated_system_memory: 0,
            shared_system_memory: 2 * 1024 * 1024 * 1024,
            feature_level: FeatureLevel::Fl12_1,
            is_software: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DxgiAdapter, FeatureLevel};

    #[test]
    fn feature_level_dotted() {
        assert_eq!(FeatureLevel::Fl12_0.dotted(), "12.0");
        assert_eq!(FeatureLevel::Fl12_2.dotted(), "12.2");
    }

    #[test]
    fn feature_level_integer_monotonic() {
        assert!(FeatureLevel::Fl12_2.as_u32() > FeatureLevel::Fl12_1.as_u32());
        assert!(FeatureLevel::Fl12_1.as_u32() > FeatureLevel::Fl12_0.as_u32());
    }

    #[test]
    fn feature_level_count() {
        assert_eq!(FeatureLevel::ALL_LEVELS.len(), 3);
    }

    #[test]
    fn stub_arc_matches_spec() {
        let a = DxgiAdapter::stub_arc_a770();
        assert_eq!(a.vendor_id, 0x8086);
        assert_eq!(a.device_id, 0x56A0);
        assert_eq!(a.feature_level, FeatureLevel::Fl12_2);
        assert_eq!(a.dedicated_video_memory, 16 * 1024 * 1024 * 1024);
        assert!(!a.is_software);
    }

    #[test]
    fn stub_warp_is_software() {
        let a = DxgiAdapter::stub_warp();
        assert!(a.is_software);
        assert_eq!(a.dedicated_video_memory, 0);
    }
}
