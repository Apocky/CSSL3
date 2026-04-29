//! `wgpu::Instance` creation + per-platform backend negotiation.
//!
//! § §§ 14_BACKEND § HOST-SUBMIT BACKENDS § WebGPU :
//!   stage-0 wgpu-core ; stage-1+ direct browser-API.
//!
//! § DESIGN
//!   Stage-0 exposes a sync wrapper over wgpu's async surface. The real
//!   `wgpu::Instance` is created via `wgpu::Instance::new` (sync). Adapter +
//!   device-request are async ; we wrap them with `pollster::block_on` so
//!   the call-site stays sync at stage-0. Async / .await integration ties to
//!   CSSLv3's effect-row + async story (deferred slice).
//!
//! § BACKEND-NEGOTIATION
//!   On Apocky's host (Windows + Arc A770), wgpu defaults to DX12. We can
//!   force a specific backend via `WebGpuInstanceConfig::force_backend`. On
//!   wasm32 the only available backend is `BROWSER_WEBGPU`.

use crate::error::WebGpuError;

/// Backend-selection hint for `WebGpuInstance::new`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BackendHint {
    /// Let wgpu pick per-platform default (recommended).
    Default,
    /// Prefer Vulkan (Linux + Windows-Vulkan-explicit).
    Vulkan,
    /// Prefer D3D12 (Windows).
    Dx12,
    /// Prefer Metal (macOS / iOS).
    Metal,
    /// Prefer GLES / WebGL2 (fallback).
    Gl,
    /// Prefer real browser-WebGPU (wasm32 only).
    BrowserWebGpu,
}

impl BackendHint {
    /// Map the hint to wgpu's `Backends` bitset.
    #[must_use]
    pub fn to_wgpu_backends(self) -> wgpu::Backends {
        match self {
            Self::Default => wgpu::Backends::all(),
            Self::Vulkan => wgpu::Backends::VULKAN,
            Self::Dx12 => wgpu::Backends::DX12,
            Self::Metal => wgpu::Backends::METAL,
            Self::Gl => wgpu::Backends::GL,
            Self::BrowserWebGpu => wgpu::Backends::BROWSER_WEBGPU,
        }
    }
}

/// Configuration for `WebGpuInstance::new`.
#[derive(Debug, Clone)]
pub struct WebGpuInstanceConfig {
    /// Which backend(s) wgpu may select among.
    pub backends: BackendHint,
    /// Optional adapter-power preference (low-power vs high-perf).
    pub power_pref: wgpu::PowerPreference,
    /// Force a software / fallback adapter (CPU-WebGPU). Useful for CI
    /// runners that have no GPU.
    pub force_fallback: bool,
}

impl Default for WebGpuInstanceConfig {
    fn default() -> Self {
        Self {
            backends: BackendHint::Default,
            power_pref: wgpu::PowerPreference::HighPerformance,
            force_fallback: false,
        }
    }
}

/// CSSLv3 wgpu-instance handle.
///
/// Owns the underlying `wgpu::Instance` and the negotiated `wgpu::Backend`
/// after a successful adapter request. `WebGpuInstance::new` does NOT request
/// an adapter — that's `request_device` / `request_adapter` on the same
/// instance, which the device-module exposes.
pub struct WebGpuInstance {
    raw: wgpu::Instance,
    cfg: WebGpuInstanceConfig,
}

impl WebGpuInstance {
    /// Create a new wgpu instance with the given config.
    ///
    /// ‼ This is sync — `wgpu::Instance::new` is sync ; only `request_adapter`
    /// + `request_device` (in the device module) are async-wrapped.
    #[must_use]
    pub fn new(cfg: WebGpuInstanceConfig) -> Self {
        let descriptor = wgpu::InstanceDescriptor {
            backends: cfg.backends.to_wgpu_backends(),
            ..Default::default()
        };
        let raw = wgpu::Instance::new(descriptor);
        Self { raw, cfg }
    }

    /// Convenience : default-config instance (let wgpu pick).
    #[must_use]
    pub fn new_default() -> Self {
        Self::new(WebGpuInstanceConfig::default())
    }

    /// Borrow the underlying wgpu instance.
    #[must_use]
    pub fn raw(&self) -> &wgpu::Instance {
        &self.raw
    }

    /// Borrow the active config.
    #[must_use]
    pub fn config(&self) -> &WebGpuInstanceConfig {
        &self.cfg
    }

    /// Sync-wrapped `request_adapter` ; returns the negotiated wgpu backend
    /// alongside the adapter. Returns `WebGpuError::NoAdapter` if wgpu can't
    /// find any compatible adapter (common in CI without GPU + no software
    /// fallback enabled).
    pub fn request_adapter_sync(&self) -> Result<wgpu::Adapter, WebGpuError> {
        let opts = wgpu::RequestAdapterOptions {
            power_preference: self.cfg.power_pref,
            force_fallback_adapter: self.cfg.force_fallback,
            compatible_surface: None,
        };
        let adapter = pollster::block_on(self.raw.request_adapter(&opts));
        adapter.ok_or(WebGpuError::NoAdapter)
    }

    /// Probe : negotiate an adapter without retaining it ; return the
    /// chosen `wgpu::Backend`. Useful for telemetry / smoke-tests.
    pub fn negotiate_backend(&self) -> Result<wgpu::Backend, WebGpuError> {
        let adapter = self.request_adapter_sync()?;
        Ok(adapter.get_info().backend)
    }
}

impl core::fmt::Debug for WebGpuInstance {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("WebGpuInstance")
            .field("backends", &self.cfg.backends)
            .field("power_pref", &self.cfg.power_pref)
            .field("force_fallback", &self.cfg.force_fallback)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::{BackendHint, WebGpuInstance, WebGpuInstanceConfig};

    #[test]
    fn backend_hint_maps_to_wgpu_bits() {
        assert_eq!(
            BackendHint::Vulkan.to_wgpu_backends(),
            wgpu::Backends::VULKAN
        );
        assert_eq!(BackendHint::Dx12.to_wgpu_backends(), wgpu::Backends::DX12);
        assert_eq!(BackendHint::Metal.to_wgpu_backends(), wgpu::Backends::METAL);
        assert_eq!(BackendHint::Gl.to_wgpu_backends(), wgpu::Backends::GL);
        assert_eq!(
            BackendHint::BrowserWebGpu.to_wgpu_backends(),
            wgpu::Backends::BROWSER_WEBGPU
        );
        // Default = all
        assert_eq!(
            BackendHint::Default.to_wgpu_backends(),
            wgpu::Backends::all()
        );
    }

    #[test]
    fn instance_default_creates_without_panic() {
        let inst = WebGpuInstance::new_default();
        let cfg = inst.config();
        assert_eq!(cfg.backends, BackendHint::Default);
        assert_eq!(cfg.power_pref, wgpu::PowerPreference::HighPerformance);
        assert!(!cfg.force_fallback);
    }

    #[test]
    fn instance_with_explicit_dx12_config() {
        let cfg = WebGpuInstanceConfig {
            backends: BackendHint::Dx12,
            power_pref: wgpu::PowerPreference::LowPower,
            force_fallback: false,
        };
        let inst = WebGpuInstance::new(cfg);
        assert_eq!(inst.config().backends, BackendHint::Dx12);
        assert_eq!(inst.config().power_pref, wgpu::PowerPreference::LowPower);
    }

    #[test]
    fn instance_debug_render_includes_backend_hint() {
        let inst = WebGpuInstance::new_default();
        let s = format!("{inst:?}");
        assert!(s.contains("WebGpuInstance"));
        assert!(s.contains("Default"));
    }

    #[test]
    fn config_default_is_high_performance_no_fallback() {
        let cfg = WebGpuInstanceConfig::default();
        assert_eq!(cfg.power_pref, wgpu::PowerPreference::HighPerformance);
        assert!(!cfg.force_fallback);
    }

    #[test]
    fn raw_instance_handle_accessible() {
        let inst = WebGpuInstance::new_default();
        // Just verify `.raw()` returns a usable reference (don't actually
        // request an adapter ; tests that need adapters live in `device.rs`).
        let _r: &wgpu::Instance = inst.raw();
    }
}
