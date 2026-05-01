//! § gpu — wgpu device, surface, and swapchain bring-up.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § T11-LOA-HOST-1 (W-LOA-host-render) : owns the wgpu Instance / Adapter /
//! Device / Queue + the surface created from the winit window. The render
//! module consumes a `GpuContext` to encode commands each frame.
//!
//! § SAFE DEFAULTS
//!   - If wgpu adapter unavailable → return `None` from `GpuContext::new`.
//!   - Headless path : caller logs + bails cleanly without panicking.
//!   - Surface creation uses winit's `window` reference (winit 0.30 ABI).

#![allow(clippy::cast_precision_loss)] // u32→f32 aspect computation
#![allow(clippy::pub_underscore_fields)] // _window kept for surface lifetime, deliberately public

use std::sync::Arc;

use winit::window::Window;

use cssl_rt::loa_startup::log_event;

use crate::telemetry::{self as telem, GpuAdapterInfo};

/// Bundled GPU context : instance, surface, device, queue, and config.
///
/// Owns the surface lifetime via an `Arc<Window>` so the surface stays valid
/// for the duration of the render loop.
pub struct GpuContext {
    pub _window: Arc<Window>,
    pub surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
    pub surface_format: wgpu::TextureFormat,
}

impl GpuContext {
    /// Async constructor — kept private; public `new` blocks via pollster.
    async fn new_async(window: Arc<Window>) -> Option<Self> {
        let size = window.inner_size();
        if size.width == 0 || size.height == 0 {
            log_event(
                "WARN",
                "loa-host/gpu",
                "window has zero-area surface · skipping GPU init",
            );
            return None;
        }

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });

        // wgpu 23 surface creation — window must outlive the surface, hence Arc.
        let surface = match instance.create_surface(window.clone()) {
            Ok(s) => s,
            Err(e) => {
                log_event(
                    "ERROR",
                    "loa-host/gpu",
                    &format!("create_surface failed : {e}"),
                );
                return None;
            }
        };

        let adapter_opt = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await;

        let Some(adapter) = adapter_opt else {
            log_event(
                "WARN",
                "loa-host/gpu",
                "no wgpu adapter available · running headless",
            );
            return None;
        };

        let info = adapter.get_info();
        log_event(
            "INFO",
            "loa-host/gpu",
            &format!(
                "wgpu adapter : name='{}' backend={:?} type={:?}",
                info.name, info.backend, info.device_type
            ),
        );

        // § T11-LOA-TELEM : capture full adapter info for `telemetry.gpu_info`.
        // Limits + features dump as a single-line summary string. Keeping this
        // out-of-band of the wgpu Limits-struct API stability story.
        let adapter_features = adapter.features();
        let mut feature_names: Vec<String> = Vec::new();
        // The Features bitflags don't expose direct iteration in older wgpu
        // versions ; we hit the most-common bits manually so the JSON dump is
        // useful without a hard dep on a specific wgpu Features API.
        let probe = [
            (wgpu::Features::TIMESTAMP_QUERY, "TIMESTAMP_QUERY"),
            (wgpu::Features::PIPELINE_STATISTICS_QUERY, "PIPELINE_STATISTICS_QUERY"),
            (wgpu::Features::TEXTURE_COMPRESSION_BC, "TEXTURE_COMPRESSION_BC"),
            (wgpu::Features::TEXTURE_COMPRESSION_ETC2, "TEXTURE_COMPRESSION_ETC2"),
            (wgpu::Features::TEXTURE_COMPRESSION_ASTC, "TEXTURE_COMPRESSION_ASTC"),
            (wgpu::Features::INDIRECT_FIRST_INSTANCE, "INDIRECT_FIRST_INSTANCE"),
            (wgpu::Features::SHADER_F16, "SHADER_F16"),
            (wgpu::Features::DEPTH_CLIP_CONTROL, "DEPTH_CLIP_CONTROL"),
            (wgpu::Features::PUSH_CONSTANTS, "PUSH_CONSTANTS"),
        ];
        for (flag, name) in &probe {
            if adapter_features.contains(*flag) {
                feature_names.push((*name).to_string());
            }
        }
        let limits = adapter.limits();
        let limits_summary = format!(
            "max_tex_2d={},max_uniform_buf={},max_storage_buf={},max_vertex_buffers={},\
             max_bind_groups={},max_compute_workgroup_size_x={}",
            limits.max_texture_dimension_2d,
            limits.max_uniform_buffer_binding_size,
            limits.max_storage_buffer_binding_size,
            limits.max_vertex_buffers,
            limits.max_bind_groups,
            limits.max_compute_workgroup_size_x,
        );
        let gpu_info = GpuAdapterInfo {
            name: info.name.clone(),
            backend: format!("{:?}", info.backend),
            device_type: format!("{:?}", info.device_type),
            vendor_id: info.vendor,
            device_id: info.device,
            driver: info.driver.clone(),
            features: feature_names,
            limits_summary,
        };
        telem::record_gpu_info(gpu_info);

        // § T11-LOA-PURE-CSSL : use the adapter's full reported limits rather
        // than `downlevel_defaults()` (which caps surfaces at 2048×2048 — too
        // small for native-res primary monitors). The adapter advertises the
        // hardware's actual limits ; on modern GPUs that's 16384×16384, which
        // covers 4K + 8K monitors with headroom. Fall back to default-limits
        // (universal lowest-common-denominator) only if the adapter probe
        // somehow returns inconsistent values.
        let adapter_limits = adapter.limits();
        let (device, queue) = match adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("loa-host/device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: adapter_limits.clone(),
                    memory_hints: wgpu::MemoryHints::Performance,
                },
                None,
            )
            .await
        {
            Ok(d) => d,
            Err(e) => {
                log_event(
                    "ERROR",
                    "loa-host/gpu",
                    &format!("request_device failed : {e}"),
                );
                return None;
            }
        };

        let caps = surface.get_capabilities(&adapter);
        let surface_format = caps
            .formats
            .iter()
            .copied()
            .find(wgpu::TextureFormat::is_srgb)
            .unwrap_or_else(|| caps.formats[0]);

        // § T11-LOA-TEST-APP : add COPY_SRC if the adapter supports it on
        // the surface, so framebuffer-readback can blit straight to a
        // staging buffer. If the adapter rejects COPY_SRC on the surface
        // texture (some platforms do), we fall back to RENDER_ATTACHMENT
        // only ; the snapshotter then maintains its own offscreen target.
        let mut usage = wgpu::TextureUsages::RENDER_ATTACHMENT;
        if caps.usages.contains(wgpu::TextureUsages::COPY_SRC) {
            usage |= wgpu::TextureUsages::COPY_SRC;
            log_event(
                "INFO",
                "loa-host/gpu",
                "surface usage includes COPY_SRC — direct framebuffer readback enabled",
            );
        } else {
            log_event(
                "INFO",
                "loa-host/gpu",
                "surface usage WITHOUT COPY_SRC — snapshot path will use offscreen mirror",
            );
        }
        let config = wgpu::SurfaceConfiguration {
            usage,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode: caps
                .present_modes
                .iter()
                .copied()
                .find(|m| matches!(m, wgpu::PresentMode::Fifo))
                .unwrap_or(wgpu::PresentMode::AutoVsync),
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        log_event(
            "INFO",
            "loa-host/gpu",
            &format!(
                "surface configured : {}x{} format={:?}",
                config.width, config.height, surface_format
            ),
        );

        Some(Self {
            _window: window,
            surface,
            device,
            queue,
            config,
            surface_format,
        })
    }

    /// Block-on synchronous constructor. Returns `None` if no GPU adapter
    /// is available — caller should treat that as a clean headless exit.
    #[must_use]
    pub fn new(window: Arc<Window>) -> Option<Self> {
        pollster::block_on(Self::new_async(window))
    }

    /// Reconfigure the surface after a window resize. Caller is expected
    /// to clamp to non-zero dimensions before calling.
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        log_event(
            "INFO",
            "loa-host/gpu",
            &format!("surface resized : {width}x{height}"),
        );
    }

    /// Aspect ratio for camera projection.
    #[must_use]
    pub fn aspect(&self) -> f32 {
        if self.config.height == 0 {
            1.0
        } else {
            self.config.width as f32 / self.config.height as f32
        }
    }
}
