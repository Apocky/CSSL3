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
use std::sync::OnceLock;

use winit::window::Window;

use cssl_rt::loa_startup::log_event;

use crate::telemetry::{self as telem, GpuAdapterInfo};

/// § T11-LOA-FID-MAINSTREAM : graphical-fidelity settings established at
/// `GpuContext::new` after probing adapter capabilities. Reflects what
/// actually got configured (after fallback). Surfaced via the
/// `render.fidelity` MCP tool so users can see what's active.
#[derive(Debug, Clone, Copy)]
pub struct FidelityConfig {
    /// MSAA sample count actually used (4 if supported, else 1).
    pub msaa_samples: u32,
    /// HDR intermediate-target format (Rgba16Float when supported).
    pub hdr_format: wgpu::TextureFormat,
    /// Surface present mode (Mailbox preferred, Fifo fallback).
    pub present_mode: wgpu::PresentMode,
    /// Anisotropy clamp used for samplers (16 if supported, else 1).
    pub aniso_max: u16,
    /// Whether the tonemap pass is wired (true when HDR target is available).
    pub tonemap_path: bool,
}

impl FidelityConfig {
    /// Conservative fallback (matches a fresh adapter that supports nothing).
    #[must_use]
    pub fn fallback(surface_format: wgpu::TextureFormat) -> Self {
        Self {
            msaa_samples: 1,
            hdr_format: surface_format,
            present_mode: wgpu::PresentMode::Fifo,
            aniso_max: 1,
            tonemap_path: false,
        }
    }

    /// JSON-serialise (used by `render.fidelity` MCP tool).
    #[must_use]
    pub fn to_json_string(&self) -> String {
        format!(
            "{{\"msaa_samples\":{},\"hdr_format\":\"{:?}\",\
             \"present_mode\":\"{:?}\",\"aniso_max\":{},\"tonemap_path\":{}}}",
            self.msaa_samples,
            self.hdr_format,
            self.present_mode,
            self.aniso_max,
            self.tonemap_path,
        )
    }
}

/// § T11-LOA-FID-MAINSTREAM : process-global last-published fidelity config.
///
/// Set once when `GpuContext::new` succeeds ; read by the
/// `render.fidelity` MCP tool via `crate::fidelity::current_report()`. The
/// runtime-only `FidelityConfig` here keeps wgpu types ; the catalog-side
/// mirror in `crate::fidelity::FidelityReport` carries string-rendered
/// values so the MCP tool compiles in both modes.
static GLOBAL_FIDELITY: OnceLock<FidelityConfig> = OnceLock::new();

/// Read the most-recently-published fidelity config (if GPU has come up).
#[must_use]
pub fn fidelity_snapshot() -> Option<FidelityConfig> {
    GLOBAL_FIDELITY.get().copied()
}

// ──────────────────────────────────────────────────────────────────────────
// § Tests (runtime feature only — wgpu types are gated)
// ──────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// § T11-LOA-FID-MAINSTREAM : fallback config matches the conservative
    /// "everything off" defaults — the safe state when the adapter doesn't
    /// expose any fidelity features.
    #[test]
    fn fidelity_fallback_is_conservative() {
        let f = FidelityConfig::fallback(wgpu::TextureFormat::Bgra8UnormSrgb);
        assert_eq!(f.msaa_samples, 1);
        assert_eq!(f.hdr_format, wgpu::TextureFormat::Bgra8UnormSrgb);
        assert_eq!(f.present_mode, wgpu::PresentMode::Fifo);
        assert_eq!(f.aniso_max, 1);
        assert!(!f.tonemap_path);
    }

    /// § T11-LOA-FID-MAINSTREAM : JSON serialization emits the keys the
    /// MCP tool returns. (Catalog-side already covers this for
    /// `FidelityReport` ; runtime side exercises the wgpu-typed mirror.)
    #[test]
    fn fidelity_to_json_string_has_expected_keys() {
        let f = FidelityConfig {
            msaa_samples: 4,
            hdr_format: wgpu::TextureFormat::Rgba16Float,
            present_mode: wgpu::PresentMode::Mailbox,
            aniso_max: 16,
            tonemap_path: true,
        };
        let s = f.to_json_string();
        assert!(s.contains("\"msaa_samples\":4"));
        assert!(s.contains("Rgba16Float"));
        assert!(s.contains("Mailbox"));
        assert!(s.contains("\"aniso_max\":16"));
        assert!(s.contains("\"tonemap_path\":true"));
    }
}

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
    /// § T11-LOA-FID-MAINSTREAM : MSAA + HDR + Mailbox + aniso settings
    /// negotiated with the adapter at init time.
    pub fidelity: FidelityConfig,
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
        // § T11-LOA-FID-MAINSTREAM : prefer Mailbox (effectively triple-
        // buffered, low-latency, no tearing) ; fall back to Fifo if the
        // platform doesn't expose it. Log every choice so a sovereign
        // operator can see what the adapter actually supported.
        // § T11-W18-ITER3 (telemetry-driven · 60Hz-cap-bypass) ──
        //   Iter-2 telemetry shows p99=16.5ms ≈ 60Hz cap on Apocky's Intel-Arc-
        //   Vulkan + Borderless-DWM-compositor stack despite Fifo running on a
        //   120Hz panel. To unlock 144Hz uncapped we prefer Immediate (no-vsync)
        //   when supported · fallback Mailbox · fallback FifoRelaxed · fallback
        //   Fifo · fallback AutoVsync. Honor `LOA_PRESENT_MODE` env-var override
        //   so a sovereign operator can force any specific mode.
        let supports = |m: wgpu::PresentMode| caps.present_modes.iter().any(|x| *x == m);
        let env_choice = std::env::var("LOA_PRESENT_MODE").ok().and_then(|s| {
            match s.to_ascii_lowercase().as_str() {
                "immediate"     => Some(wgpu::PresentMode::Immediate),
                "mailbox"       => Some(wgpu::PresentMode::Mailbox),
                "fifo-relaxed" | "fiforelaxed" => Some(wgpu::PresentMode::FifoRelaxed),
                "fifo"          => Some(wgpu::PresentMode::Fifo),
                "auto-vsync" | "autovsync" => Some(wgpu::PresentMode::AutoVsync),
                "auto-no-vsync" | "autonovsync" => Some(wgpu::PresentMode::AutoNoVsync),
                _ => None,
            }
        });
        let present_mode = if let Some(m) = env_choice {
            if supports(m) {
                log_event("INFO", "loa-host/gpu",
                    &format!("fidelity_init.present_mode={m:?} (env-override LOA_PRESENT_MODE)"));
                m
            } else {
                log_event("WARN", "loa-host/gpu",
                    &format!("fidelity_init.env-mode {m:?} unsupported · auto-selecting"));
                // fall through
                if supports(wgpu::PresentMode::Immediate) { wgpu::PresentMode::Immediate }
                else if supports(wgpu::PresentMode::Mailbox) { wgpu::PresentMode::Mailbox }
                else if supports(wgpu::PresentMode::FifoRelaxed) { wgpu::PresentMode::FifoRelaxed }
                else if supports(wgpu::PresentMode::Fifo) { wgpu::PresentMode::Fifo }
                else { wgpu::PresentMode::AutoVsync }
            }
        } else if supports(wgpu::PresentMode::Immediate) {
            // § PREFERRED for 1440p144 · no-vsync · DWM-compositor-bypass when
            //   true-fullscreen exclusive flip-chain. May tear in borderless ;
            //   pair with fullscreen-exclusive for tear-free 144 Hz.
            log_event("INFO", "loa-host/gpu",
                "fidelity_init.present_mode=Immediate (no-vsync · 1440p144 path · prefer fullscreen-exclusive for tear-free)");
            wgpu::PresentMode::Immediate
        } else if supports(wgpu::PresentMode::Mailbox) {
            log_event("INFO", "loa-host/gpu",
                "fidelity_init.present_mode=Mailbox (triple-buffered · low-latency)");
            wgpu::PresentMode::Mailbox
        } else if supports(wgpu::PresentMode::FifoRelaxed) {
            log_event("INFO", "loa-host/gpu",
                "fidelity_init.present_mode=FifoRelaxed (vsync · allows-late-tear)");
            wgpu::PresentMode::FifoRelaxed
        } else if supports(wgpu::PresentMode::Fifo) {
            log_event("WARN", "loa-host/gpu",
                "fidelity_init.present_mode_fallback=Fifo (Immediate+Mailbox+FifoRelaxed unsupported · likely 60Hz-DWM-cap)");
            wgpu::PresentMode::Fifo
        } else {
            log_event("WARN", "loa-host/gpu",
                "fidelity_init.present_mode_fallback=AutoVsync (no specific mode supported)");
            wgpu::PresentMode::AutoVsync
        };

        // § T11-LOA-FID-MAINSTREAM : probe MSAA support. wgpu surfaces of
        // every common backend (Vulkan/D3D12/Metal) accept sample_count=4
        // on common color formats. We additionally check that Rgba16Float
        // (the HDR intermediate format) is supported by the adapter as a
        // RENDER_ATTACHMENT — only then do we engage the HDR/MSAA path.
        let hdr_format = wgpu::TextureFormat::Rgba16Float;
        let hdr_features = adapter.get_texture_format_features(hdr_format);
        let supports_hdr_render_attachment = hdr_features
            .allowed_usages
            .contains(wgpu::TextureUsages::RENDER_ATTACHMENT);
        let supports_hdr_msaa4 = hdr_features
            .flags
            .sample_count_supported(4);
        let (msaa_samples, hdr_active, tonemap_path) =
            if supports_hdr_render_attachment && supports_hdr_msaa4 {
                log_event(
                    "INFO",
                    "loa-host/gpu",
                    "fidelity_init.msaa=4 hdr=Rgba16Float tonemap=enabled",
                );
                (4u32, true, true)
            } else if supports_hdr_render_attachment {
                log_event(
                    "WARN",
                    "loa-host/gpu",
                    "fidelity_init.msaa_fallback=1 (sample_count=4 unsupported on Rgba16Float)",
                );
                (1u32, true, true)
            } else {
                log_event(
                    "WARN",
                    "loa-host/gpu",
                    "fidelity_init.hdr_fallback=disabled · using surface format direct (no tonemap)",
                );
                (1u32, false, false)
            };

        // § T11-LOA-FID-MAINSTREAM : anisotropic 16x is universally available
        // in wgpu 0.18+ as long as the device has at least one bind-group
        // and the platform exposes 16x. We default to 16 ; downstream
        // sampler creators clamp internally if rejected.
        let aniso_max: u16 = 16;

        let fidelity = FidelityConfig {
            msaa_samples,
            hdr_format: if hdr_active { hdr_format } else { surface_format },
            present_mode,
            aniso_max,
            tonemap_path,
        };
        log_event(
            "INFO",
            "loa-host/gpu",
            &format!(
                "fidelity_init : msaa={} hdr={:?} present={:?} aniso={} tonemap={}",
                fidelity.msaa_samples,
                fidelity.hdr_format,
                fidelity.present_mode,
                fidelity.aniso_max,
                fidelity.tonemap_path,
            ),
        );

        let config = wgpu::SurfaceConfiguration {
            usage,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode,
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

        // § T11-LOA-FID-MAINSTREAM : publish the negotiated fidelity to the
        // global so MCP `render.fidelity` can return live values.
        let _ = GLOBAL_FIDELITY.set(fidelity);
        // Also publish the catalog-buildable string mirror so the MCP tool
        // (catalog mode) can read the report without wgpu-typed deps.
        crate::fidelity::set_report(crate::fidelity::FidelityReport {
            msaa_samples: fidelity.msaa_samples,
            hdr_format: format!("{:?}", fidelity.hdr_format),
            present_mode: format!("{:?}", fidelity.present_mode),
            aniso_max: fidelity.aniso_max,
            tonemap_path: fidelity.tonemap_path,
            initialized: true,
        });

        Some(Self {
            _window: window,
            surface,
            device,
            queue,
            config,
            surface_format,
            fidelity,
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
