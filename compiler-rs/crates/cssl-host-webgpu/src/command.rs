//! `wgpu::CommandEncoder` + ComputePass / RenderPass helpers.
//!
//! § §§ 14_BACKEND § HOST-SUBMIT BACKENDS § per-backend adapter-layer §
//!   create_cmd_buffer + dispatch + draw + submit.
//!
//! § DESIGN
//!   wgpu's `CommandEncoder` is single-shot : it records a stream of commands
//!   then `.finish()` produces a `CommandBuffer` consumed by `Queue::submit`.
//!   `WebGpuCommandEncoder` is a thin newtype that adds CSSLv3-shaped
//!   convenience helpers (`begin_compute_pass` with auto-bind-group, etc.)
//!   without hiding the underlying wgpu API.

use crate::buffer::WebGpuBuffer;
use crate::device::WebGpuDevice;
use crate::error::WebGpuError;
use crate::pipeline::WebGpuComputePipeline;

/// CSSLv3 wgpu command-encoder.
///
/// One-shot : `.finish()` consumes the encoder and yields a `CommandBuffer`.
pub struct WebGpuCommandEncoder {
    raw: wgpu::CommandEncoder,
}

impl WebGpuCommandEncoder {
    /// Create a fresh encoder.
    #[must_use]
    pub fn new(device: &WebGpuDevice, label: &str) -> Self {
        let raw = device
            .raw_device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some(label) });
        Self { raw }
    }

    /// Borrow the underlying wgpu encoder mutably (for low-level work).
    pub fn raw_mut(&mut self) -> &mut wgpu::CommandEncoder {
        &mut self.raw
    }

    /// Record a compute-pass that runs the given pipeline with the given
    /// input + output buffers, dispatched at (workgroups_x, 1, 1).
    ///
    /// Convenience for the canonical 1-bind-group-2-storage-buffers compute
    /// shape from `kernels.rs`. Real CSSLv3-MIR-emitted compute shapes will
    /// extend this once D4 lands.
    pub fn dispatch_compute(
        &mut self,
        device: &WebGpuDevice,
        pipeline: &WebGpuComputePipeline,
        in_buf: &WebGpuBuffer,
        out_buf: &WebGpuBuffer,
        workgroups_x: u32,
    ) -> Result<(), WebGpuError> {
        let dev = device.raw_device();
        let bind_group = dev.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cssl-compute-bg"),
            layout: pipeline.bind_group_layout(),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: in_buf.raw().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: out_buf.raw().as_entire_binding(),
                },
            ],
        });

        let mut pass = self.raw.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("cssl-compute-pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(pipeline.raw());
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups(workgroups_x, 1, 1);
        // pass dropped here — recording finalized.
        drop(pass);
        Ok(())
    }

    /// Record a buffer-to-buffer copy.
    pub fn copy_buffer_to_buffer(
        &mut self,
        src: &WebGpuBuffer,
        dst: &WebGpuBuffer,
        size: u64,
    ) -> Result<(), WebGpuError> {
        if size > src.size() {
            return Err(WebGpuError::Buffer(format!(
                "copy size ({size}) exceeds src-size ({})",
                src.size()
            )));
        }
        if size > dst.size() {
            return Err(WebGpuError::Buffer(format!(
                "copy size ({size}) exceeds dst-size ({})",
                dst.size()
            )));
        }
        self.raw
            .copy_buffer_to_buffer(src.raw(), 0, dst.raw(), 0, size);
        Ok(())
    }

    /// Finish recording → produce a `CommandBuffer` ready for submission.
    #[must_use]
    pub fn finish(self) -> wgpu::CommandBuffer {
        self.raw.finish()
    }
}

impl core::fmt::Debug for WebGpuCommandEncoder {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("WebGpuCommandEncoder").finish()
    }
}

#[cfg(test)]
mod tests {
    // No host-only unit tests here ; the encoder needs a real device which
    // is covered by the integration tests in `tests::compute_pipeline_smoke`.
    use super::WebGpuCommandEncoder;

    #[test]
    fn encoder_type_is_debug() {
        // Compile-only check : `Debug` is implemented manually (the inner
        // wgpu::CommandEncoder isn't Debug). Tautological — real coverage
        // is in the integration tests.
        fn assert_debug<T: std::fmt::Debug>() {}
        assert_debug::<WebGpuCommandEncoder>();
    }
}
