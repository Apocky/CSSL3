//! `wgpu::ComputePipeline` + `wgpu::RenderPipeline` creation.
//!
//! § §§ 14_BACKEND § HOST-SUBMIT BACKENDS § per-backend adapter-layer.
//!
//! § DESIGN
//!   `WebGpuComputePipeline` + `WebGpuRenderPipeline` carry the wgpu pipeline
//!   handle plus the bind-group layout the kernel declares. Stage-0 uses
//!   a single bind-group with two storage buffers (input + output) for the
//!   compute path ; the render path uses zero binds (vertex-pulled
//!   procedural geometry).
//!
//!   Real CSSLv3-MIR-driven pipeline-creation lands when S6-D4 (WGSL body
//!   emission) is wired through. Until then, the host backend tests its
//!   own surface against the hand-written kernels in `kernels.rs`.

use crate::device::WebGpuDevice;
use crate::error::WebGpuError;

/// Configuration for `WebGpuComputePipeline::create`.
#[derive(Debug, Clone)]
pub struct WebGpuComputePipelineConfig {
    /// Friendly label for the pipeline + shader-module.
    pub label: Option<String>,
    /// WGSL source. Stage-0 = hand-written ; stage1+ = MIR-emitted.
    pub wgsl_source: String,
    /// Entry-point name in the WGSL source.
    pub entry_point: String,
}

impl WebGpuComputePipelineConfig {
    /// Convenience : copy-kernel from `kernels::COPY_KERNEL_WGSL`.
    #[must_use]
    pub fn copy_kernel() -> Self {
        Self {
            label: Some("cssl-copy-kernel".into()),
            wgsl_source: crate::kernels::COPY_KERNEL_WGSL.to_string(),
            entry_point: "main".into(),
        }
    }

    /// Convenience : add-42-kernel from `kernels::ADD_42_KERNEL_WGSL`.
    #[must_use]
    pub fn add_42_kernel() -> Self {
        Self {
            label: Some("cssl-add-42-kernel".into()),
            wgsl_source: crate::kernels::ADD_42_KERNEL_WGSL.to_string(),
            entry_point: "main".into(),
        }
    }
}

/// CSSLv3 wgpu compute-pipeline + bind-group layout.
#[derive(Debug)]
pub struct WebGpuComputePipeline {
    raw: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,
}

impl WebGpuComputePipeline {
    /// Compile the WGSL + create the pipeline.
    ///
    /// Errors :
    ///   * `WebGpuError::ShaderModule` — WGSL parse / validate failure.
    ///   * `WebGpuError::ComputePipeline` — pipeline-link failure (rare).
    pub fn create(
        device: &WebGpuDevice,
        cfg: &WebGpuComputePipelineConfig,
    ) -> Result<Self, WebGpuError> {
        let dev = device.raw_device();
        // Catch wgpu validation errors via the scope mechanism.
        dev.push_error_scope(wgpu::ErrorFilter::Validation);
        let module = dev.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: cfg.label.as_deref(),
            source: wgpu::ShaderSource::Wgsl(cfg.wgsl_source.clone().into()),
        });
        if let Some(err) = pollster::block_on(dev.pop_error_scope()) {
            return Err(WebGpuError::ShaderModule(err.to_string()));
        }

        let bind_group_layout = dev.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: cfg.label.as_deref(),
            entries: &[
                // @group(0) @binding(0) : storage, read.
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // @group(0) @binding(1) : storage, read_write.
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let pipeline_layout = dev.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: cfg.label.as_deref(),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        dev.push_error_scope(wgpu::ErrorFilter::Validation);
        let pipeline = dev.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: cfg.label.as_deref(),
            layout: Some(&pipeline_layout),
            module: &module,
            entry_point: Some(&cfg.entry_point),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });
        if let Some(err) = pollster::block_on(dev.pop_error_scope()) {
            return Err(WebGpuError::ComputePipeline(err.to_string()));
        }

        Ok(Self {
            raw: pipeline,
            bind_group_layout,
        })
    }

    /// Borrow the wgpu pipeline.
    #[must_use]
    pub fn raw(&self) -> &wgpu::ComputePipeline {
        &self.raw
    }

    /// Borrow the bind-group layout (used to create bind-groups against this
    /// pipeline).
    #[must_use]
    pub fn bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.bind_group_layout
    }
}

/// Configuration for `WebGpuRenderPipeline::create`.
#[derive(Debug, Clone)]
pub struct WebGpuRenderPipelineConfig {
    /// Friendly label.
    pub label: Option<String>,
    /// WGSL source containing both vertex + fragment entry-points.
    pub wgsl_source: String,
    /// Vertex entry-point name.
    pub vs_entry: String,
    /// Fragment entry-point name.
    pub fs_entry: String,
    /// Color-target format (must match the render-target texture's format).
    pub color_target_format: wgpu::TextureFormat,
}

impl WebGpuRenderPipelineConfig {
    /// Convenience : full-screen triangle from `kernels::FULLSCREEN_TRI_WGSL`
    /// rendered to Rgba8Unorm.
    #[must_use]
    pub fn fullscreen_tri() -> Self {
        Self {
            label: Some("cssl-fullscreen-tri".into()),
            wgsl_source: crate::kernels::FULLSCREEN_TRI_WGSL.to_string(),
            vs_entry: "vs_main".into(),
            fs_entry: "fs_main".into(),
            color_target_format: wgpu::TextureFormat::Rgba8Unorm,
        }
    }
}

/// CSSLv3 wgpu render-pipeline.
#[derive(Debug)]
pub struct WebGpuRenderPipeline {
    raw: wgpu::RenderPipeline,
}

impl WebGpuRenderPipeline {
    /// Compile the WGSL + create the render-pipeline.
    pub fn create(
        device: &WebGpuDevice,
        cfg: &WebGpuRenderPipelineConfig,
    ) -> Result<Self, WebGpuError> {
        let dev = device.raw_device();
        dev.push_error_scope(wgpu::ErrorFilter::Validation);
        let module = dev.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: cfg.label.as_deref(),
            source: wgpu::ShaderSource::Wgsl(cfg.wgsl_source.clone().into()),
        });
        if let Some(err) = pollster::block_on(dev.pop_error_scope()) {
            return Err(WebGpuError::ShaderModule(err.to_string()));
        }

        // Empty pipeline-layout : zero binds for the smoke-test render
        // pipeline (full-screen tri uses procedural geometry only).
        let pipeline_layout = dev.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: cfg.label.as_deref(),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });

        dev.push_error_scope(wgpu::ErrorFilter::Validation);
        let pipeline = dev.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: cfg.label.as_deref(),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some(&cfg.vs_entry),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some(&cfg.fs_entry),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: cfg.color_target_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
            cache: None,
        });
        if let Some(err) = pollster::block_on(dev.pop_error_scope()) {
            return Err(WebGpuError::RenderPipeline(err.to_string()));
        }

        Ok(Self { raw: pipeline })
    }

    /// Borrow the wgpu pipeline.
    #[must_use]
    pub fn raw(&self) -> &wgpu::RenderPipeline {
        &self.raw
    }
}

#[cfg(test)]
mod tests {
    use super::{WebGpuComputePipelineConfig, WebGpuRenderPipelineConfig};

    #[test]
    fn copy_kernel_config_uses_main_entry() {
        let cfg = WebGpuComputePipelineConfig::copy_kernel();
        assert_eq!(cfg.entry_point, "main");
        assert!(cfg.wgsl_source.contains("@compute"));
        assert_eq!(cfg.label.as_deref(), Some("cssl-copy-kernel"));
    }

    #[test]
    fn add_42_kernel_carries_constant() {
        let cfg = WebGpuComputePipelineConfig::add_42_kernel();
        assert!(cfg.wgsl_source.contains("42u"));
        assert_eq!(cfg.entry_point, "main");
    }

    #[test]
    fn fullscreen_tri_config_has_vs_and_fs_entries() {
        let cfg = WebGpuRenderPipelineConfig::fullscreen_tri();
        assert_eq!(cfg.vs_entry, "vs_main");
        assert_eq!(cfg.fs_entry, "fs_main");
        assert_eq!(cfg.color_target_format, wgpu::TextureFormat::Rgba8Unorm);
    }

    #[test]
    fn config_clone() {
        let a = WebGpuComputePipelineConfig::copy_kernel();
        let b = WebGpuComputePipelineConfig::clone(&a);
        assert_eq!(a.entry_point, b.entry_point);
    }
}
