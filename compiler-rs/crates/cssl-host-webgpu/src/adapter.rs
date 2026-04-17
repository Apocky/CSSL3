//! WebGPU adapter + backend enum.

use core::fmt;

/// Underlying backend the WebGPU implementation dispatches to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WebGpuBackend {
    /// Browser-WebGPU (Dawn / wgpu-over-browser).
    Browser,
    /// Vulkan passthrough (Linux + Android + Windows-wgpu-standalone).
    Vulkan,
    /// Metal passthrough (Apple).
    Metal,
    /// DirectX-12 passthrough (Windows).
    Dx12,
    /// OpenGL ES 3+ fallback.
    Gl,
}

impl WebGpuBackend {
    /// Short name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Browser => "browser",
            Self::Vulkan => "vulkan",
            Self::Metal => "metal",
            Self::Dx12 => "dx12",
            Self::Gl => "gl",
        }
    }

    /// All 5 backends.
    pub const ALL_BACKENDS: [Self; 5] = [
        Self::Browser,
        Self::Vulkan,
        Self::Metal,
        Self::Dx12,
        Self::Gl,
    ];
}

impl fmt::Display for WebGpuBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Power-preference hint for adapter selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AdapterPowerPref {
    /// Prefer integrated / low-power adapter.
    LowPower,
    /// Prefer discrete / high-performance adapter.
    HighPerformance,
    /// No preference — runtime picks.
    NoPreference,
}

impl AdapterPowerPref {
    /// Short name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LowPower => "low-power",
            Self::HighPerformance => "high-performance",
            Self::NoPreference => "no-preference",
        }
    }
}

/// WebGPU adapter record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebGpuAdapter {
    /// Adapter name.
    pub name: String,
    /// PCI vendor ID (best-effort — 0 on Browser-WebGPU).
    pub vendor_id: u32,
    /// PCI device ID.
    pub device_id: u32,
    /// Backend the adapter uses.
    pub backend: WebGpuBackend,
    /// Driver description (vendor-specific).
    pub driver_description: String,
    /// Is this a software adapter (software-WebGPU / CPU fallback) ?
    pub is_fallback: bool,
}

impl WebGpuAdapter {
    /// Stub Arc A770 via Vulkan-passthrough.
    #[must_use]
    pub fn stub_arc_a770_vulkan() -> Self {
        Self {
            name: "Intel(R) Arc(TM) A770 Graphics".into(),
            vendor_id: 0x8086,
            device_id: 0x56A0,
            backend: WebGpuBackend::Vulkan,
            driver_description: "Mesa ANV / Intel ISV 32.0.101.8629".into(),
            is_fallback: false,
        }
    }

    /// Stub Browser-WebGPU adapter (fingerprint-constrained ; vendor/device = 0).
    #[must_use]
    pub fn stub_browser_webgpu() -> Self {
        Self {
            name: "Unknown (Browser-WebGPU)".into(),
            vendor_id: 0,
            device_id: 0,
            backend: WebGpuBackend::Browser,
            driver_description: "Dawn via Chromium".into(),
            is_fallback: false,
        }
    }

    /// Stub CPU-fallback adapter (software-WebGPU).
    #[must_use]
    pub fn stub_software() -> Self {
        Self {
            name: "WebGPU Software Adapter".into(),
            vendor_id: 0,
            device_id: 0,
            backend: WebGpuBackend::Gl,
            driver_description: "SwiftShader".into(),
            is_fallback: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{AdapterPowerPref, WebGpuAdapter, WebGpuBackend};

    #[test]
    fn backend_names() {
        assert_eq!(WebGpuBackend::Browser.as_str(), "browser");
        assert_eq!(WebGpuBackend::Vulkan.as_str(), "vulkan");
        assert_eq!(WebGpuBackend::Dx12.as_str(), "dx12");
    }

    #[test]
    fn backend_count() {
        assert_eq!(WebGpuBackend::ALL_BACKENDS.len(), 5);
    }

    #[test]
    fn power_pref_names() {
        assert_eq!(AdapterPowerPref::LowPower.as_str(), "low-power");
        assert_eq!(
            AdapterPowerPref::HighPerformance.as_str(),
            "high-performance"
        );
    }

    #[test]
    fn stub_arc_a770_vulkan() {
        let a = WebGpuAdapter::stub_arc_a770_vulkan();
        assert_eq!(a.vendor_id, 0x8086);
        assert_eq!(a.device_id, 0x56A0);
        assert_eq!(a.backend, WebGpuBackend::Vulkan);
        assert!(!a.is_fallback);
    }

    #[test]
    fn stub_browser_zeros_vendor_id() {
        let a = WebGpuAdapter::stub_browser_webgpu();
        assert_eq!(a.vendor_id, 0);
        assert_eq!(a.backend, WebGpuBackend::Browser);
    }

    #[test]
    fn stub_software_marked_fallback() {
        let a = WebGpuAdapter::stub_software();
        assert!(a.is_fallback);
    }
}
