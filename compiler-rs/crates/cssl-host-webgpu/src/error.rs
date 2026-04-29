//! WebGPU host error-types.
//!
//! § §§ 14_BACKEND § HOST-SUBMIT BACKENDS § per-backend adapter-layer.
//!
//! § DESIGN
//!   `WebGpuError` is the surface for all wgpu-host failures. It maps wgpu's
//!   own error families (RequestAdapterError / RequestDeviceError /
//!   CreateShaderModuleError / etc.) into a single CSSLv3-shaped enum so
//!   downstream code stays backend-agnostic.

use thiserror::Error;

/// Errors originating from the wgpu-backed host backend.
#[derive(Debug, Error)]
pub enum WebGpuError {
    /// `wgpu::Instance::request_adapter` returned `None`.
    #[error("WebGPU adapter request failed (no compatible adapter found)")]
    NoAdapter,

    /// `wgpu::Adapter::request_device` failed.
    #[error("WebGPU device-request failed : {0}")]
    DeviceRequest(String),

    /// `create_shader_module` rejected source (validation / parse / etc.).
    #[error("WGSL shader-module creation failed : {0}")]
    ShaderModule(String),

    /// `create_compute_pipeline` failed.
    #[error("WebGPU compute-pipeline creation failed : {0}")]
    ComputePipeline(String),

    /// `create_render_pipeline` failed.
    #[error("WebGPU render-pipeline creation failed : {0}")]
    RenderPipeline(String),

    /// Buffer slice / mapping / readback failed.
    #[error("WebGPU buffer operation failed : {0}")]
    Buffer(String),

    /// Submit / queue-wait timed out or failed.
    #[error("WebGPU queue-submission failed : {0}")]
    QueueSubmit(String),

    /// Generic catch-all : wgpu surfaced a structured error we can't map
    /// 1:1 into the enum above (rare — most paths hit a typed variant).
    #[error("WebGPU error : {0}")]
    Other(String),
}

#[cfg(test)]
mod tests {
    use super::WebGpuError;

    #[test]
    fn no_adapter_message_is_actionable() {
        let s = format!("{}", WebGpuError::NoAdapter);
        assert!(s.contains("adapter"));
        assert!(s.contains("no compatible"));
    }

    #[test]
    fn device_request_carries_details() {
        let e = WebGpuError::DeviceRequest("limit X exceeded".into());
        let s = format!("{e}");
        assert!(s.contains("limit X exceeded"));
        assert!(s.contains("device-request"));
    }

    #[test]
    fn each_variant_renders_distinct_prefix() {
        let variants = [
            WebGpuError::NoAdapter,
            WebGpuError::DeviceRequest("x".into()),
            WebGpuError::ShaderModule("x".into()),
            WebGpuError::ComputePipeline("x".into()),
            WebGpuError::RenderPipeline("x".into()),
            WebGpuError::Buffer("x".into()),
            WebGpuError::QueueSubmit("x".into()),
            WebGpuError::Other("x".into()),
        ];
        let messages: Vec<String> = variants.iter().map(|v| format!("{v}")).collect();
        // every message non-empty + distinct enough that a substring search differentiates
        for m in &messages {
            assert!(!m.is_empty());
        }
        // distinct prefixes (first 12 chars unique modulo "WebGPU" prefix)
        let prefixes: std::collections::HashSet<_> = messages
            .iter()
            .map(|m| m.split(':').next().unwrap_or(""))
            .collect();
        // Some variants share the "WebGPU buffer/" "WebGPU compute/" etc. prefix idea ;
        // verify the pre-colon span is varied enough.
        assert!(
            prefixes.len() >= 5,
            "want at least 5 distinct error tags, got {prefixes:?}"
        );
    }
}
