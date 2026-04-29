//! `wgpu::Device` + `wgpu::Queue` creation, sync-wrapped.
//!
//! § §§ 14_BACKEND § HOST-SUBMIT BACKENDS § per-backend adapter-layer.
//!
//! § DESIGN
//!   `WebGpuDevice` owns the `Device` + `Queue` pair returned by
//!   `request_device` (async — wrapped in `pollster::block_on`). The
//!   underlying wgpu types are `Clone`-able / cheap-to-clone (Arc-internal),
//!   so we expose `.raw_device()` / `.raw_queue()` for downstream code that
//!   needs to issue API-calls against them directly.
//!
//! § TELEMETRY HOOK
//!   `WebGpuDevice` carries a callback-hook for `cssl-rt`-routed telemetry
//!   (R18). Stage-0 it's a placeholder ; the full integration ties to
//!   `cssl-telemetry` once the trait-resolve infrastructure lands.

use crate::error::WebGpuError;
use crate::instance::WebGpuInstance;

/// Configuration for `WebGpuDevice::request`.
#[derive(Debug, Clone)]
pub struct WebGpuDeviceConfig {
    /// Friendly label for diagnostics (forwarded to wgpu).
    pub label: Option<String>,
    /// Optional features to request beyond the WebGPU baseline.
    pub required_features: wgpu::Features,
    /// Optional limits override. Use `wgpu::Limits::default()` for the
    /// downlevel-baseline ; `wgpu::Limits::downlevel_defaults()` for
    /// browser-WebGPU compat.
    pub required_limits: wgpu::Limits,
    /// Memory hint (Performance / MemoryUsage).
    pub memory_hints: wgpu::MemoryHints,
}

impl Default for WebGpuDeviceConfig {
    fn default() -> Self {
        Self {
            label: Some("cssl-host-webgpu".into()),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::Performance,
        }
    }
}

/// CSSLv3 wgpu device + queue handle.
#[derive(Debug)]
pub struct WebGpuDevice {
    device: wgpu::Device,
    queue: wgpu::Queue,
    backend: wgpu::Backend,
    adapter_info: wgpu::AdapterInfo,
}

impl WebGpuDevice {
    /// Sync-wrapped device-request from a fresh adapter.
    ///
    /// Combines `request_adapter` + `request_device` under one
    /// `pollster::block_on` so the call-site stays sync.
    pub fn request(
        instance: &WebGpuInstance,
        cfg: &WebGpuDeviceConfig,
    ) -> Result<Self, WebGpuError> {
        let adapter = instance.request_adapter_sync()?;
        Self::from_adapter(adapter, cfg)
    }

    /// Sync-wrapped device-request from a pre-acquired adapter.
    pub fn from_adapter(
        adapter: wgpu::Adapter,
        cfg: &WebGpuDeviceConfig,
    ) -> Result<Self, WebGpuError> {
        let info = adapter.get_info();
        let backend = info.backend;
        let descriptor = wgpu::DeviceDescriptor {
            label: cfg.label.as_deref(),
            required_features: cfg.required_features,
            required_limits: cfg.required_limits.clone(),
            memory_hints: cfg.memory_hints.clone(),
        };
        let result = pollster::block_on(adapter.request_device(&descriptor, None));
        match result {
            Ok((device, queue)) => Ok(Self {
                device,
                queue,
                backend,
                adapter_info: info,
            }),
            Err(e) => Err(WebGpuError::DeviceRequest(e.to_string())),
        }
    }

    /// Borrow the wgpu device.
    #[must_use]
    pub fn raw_device(&self) -> &wgpu::Device {
        &self.device
    }

    /// Borrow the wgpu queue.
    #[must_use]
    pub fn raw_queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    /// The wgpu backend that was negotiated.
    #[must_use]
    pub const fn backend(&self) -> wgpu::Backend {
        self.backend
    }

    /// The adapter info (vendor / device / driver-description) snapshot.
    #[must_use]
    pub const fn adapter_info(&self) -> &wgpu::AdapterInfo {
        &self.adapter_info
    }

    /// Telemetry placeholder : record a "frame submitted" tick.
    ///
    /// Stage-0 is a no-op ; real R18 integration ties to `cssl-telemetry`
    /// once it lands as a callable from this crate.
    pub fn telemetry_tick(&self, _label: &str) {
        // intentionally empty — see module-doc TELEMETRY HOOK note.
    }
}

#[cfg(test)]
mod tests {
    use super::WebGpuDeviceConfig;

    #[test]
    fn config_default_has_label() {
        let cfg = WebGpuDeviceConfig::default();
        assert_eq!(cfg.label.as_deref(), Some("cssl-host-webgpu"));
    }

    #[test]
    fn config_default_features_empty() {
        let cfg = WebGpuDeviceConfig::default();
        assert!(cfg.required_features.is_empty());
    }

    #[test]
    fn config_default_memory_hint_performance() {
        let cfg = WebGpuDeviceConfig::default();
        assert!(matches!(cfg.memory_hints, wgpu::MemoryHints::Performance));
    }

    #[test]
    fn config_can_be_clone() {
        let a = WebGpuDeviceConfig::default();
        let b = WebGpuDeviceConfig::clone(&a);
        assert_eq!(a.label, b.label);
    }
}
