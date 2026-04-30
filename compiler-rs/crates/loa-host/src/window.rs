//! § window — winit Event Loop driver for the LoA test-room window.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § T11-LOA-HOST-1 (W-LOA-host-render) : winit 0.30's `ApplicationHandler`
//! trait pattern. The application owns the optional GPU context + renderer ;
//! both are built lazily on first `resumed` (per winit 0.30 docs : surface
//! creation must wait until the window is fully resumed, otherwise platform
//! backends like Wayland/Android cannot resolve a valid handle).
//!
//! § FRAME LOOP
//!   We request a redraw on every `AboutToWait` so we drive the scene at
//!   the platform's vsync rate (typically 60 Hz). Closing the window or
//!   pressing Escape exits cleanly.

use std::sync::Arc;

use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

use cssl_rt::loa_startup::log_event;

use crate::camera::Camera;
use crate::gpu::GpuContext;
use crate::render::Renderer;

/// Initial window dimensions per the brief. 1280×720 = 720p HD.
pub const INITIAL_WIDTH: u32 = 1280;
pub const INITIAL_HEIGHT: u32 = 720;

/// Application state — owned by the winit event loop.
#[derive(Default)]
pub struct App {
    window: Option<Arc<Window>>,
    gpu: Option<GpuContext>,
    renderer: Option<Renderer>,
    camera: Camera,
    /// Cached for tests + headless mode : did we ever bring up the GPU?
    pub gpu_alive: bool,
}

impl App {
    /// Returns the current camera (read-only). Sibling `W-LOA-host-input`
    /// will mutate via a separate API.
    #[must_use]
    pub fn camera(&self) -> Camera {
        self.camera
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("Labyrinth of Apockalypse · stage-0 host")
            .with_inner_size(PhysicalSize::new(INITIAL_WIDTH, INITIAL_HEIGHT));
        let window = match event_loop.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                log_event(
                    "ERROR",
                    "loa-host/window",
                    &format!("create_window failed : {e} · exiting cleanly"),
                );
                event_loop.exit();
                return;
            }
        };
        log_event(
            "INFO",
            "loa-host/window",
            &format!("window-created · {INITIAL_WIDTH}x{INITIAL_HEIGHT}"),
        );

        // Try to bring up the GPU. If it fails, we still keep the window open
        // (so the user sees a black window + clean exit) but don't render.
        if let Some(gpu) = GpuContext::new(window.clone()) {
            let renderer = Renderer::new(&gpu);
            self.gpu = Some(gpu);
            self.renderer = Some(renderer);
            self.gpu_alive = true;
        } else {
            log_event(
                "WARN",
                "loa-host/window",
                "no GPU context available · window will be blank",
            );
        }
        self.window = Some(window);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                log_event("INFO", "loa-host/window", "close-requested · exiting");
                event_loop.exit();
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        state: ElementState::Pressed,
                        logical_key: Key::Named(NamedKey::Escape),
                        ..
                    },
                ..
            } => {
                log_event("INFO", "loa-host/window", "escape pressed · exiting");
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                if let (Some(gpu), Some(renderer)) = (self.gpu.as_mut(), self.renderer.as_mut()) {
                    gpu.resize(size.width, size.height);
                    renderer.resize(gpu);
                }
            }
            WindowEvent::RedrawRequested => {
                if let (Some(gpu), Some(renderer), Some(window)) = (
                    self.gpu.as_ref(),
                    self.renderer.as_mut(),
                    self.window.as_ref(),
                ) {
                    match renderer.render_frame(gpu, &self.camera, window) {
                        // Ok | Lost | Outdated all advance to the next frame
                        // without further action ; resize-handler reconfigures
                        // the surface when the window-size event arrives.
                        Ok(()) | Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {}
                        Err(wgpu::SurfaceError::OutOfMemory) => {
                            log_event("ERROR", "loa-host/render", "surface OOM · exiting cleanly");
                            event_loop.exit();
                        }
                        Err(e) => {
                            log_event("ERROR", "loa-host/render", &format!("frame error : {e:?}"));
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(w) = self.window.as_ref() {
            w.request_redraw();
        }
    }
}

/// Run the engine event loop. Blocks until the window is closed. On
/// platforms where no event loop / display is available, returns
/// `Ok(())` silently after logging the condition.
pub fn run() -> std::io::Result<()> {
    let event_loop = match EventLoop::new() {
        Ok(el) => el,
        Err(e) => {
            log_event(
                "WARN",
                "loa-host/window",
                &format!("EventLoop::new failed : {e} · running headless"),
            );
            return Ok(());
        }
    };
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App::default();
    if let Err(e) = event_loop.run_app(&mut app) {
        log_event(
            "ERROR",
            "loa-host/window",
            &format!("event loop terminated abnormally : {e}"),
        );
        // Don't propagate the error — we want clean exit.
    }
    log_event("INFO", "loa-host/exit", "loop-exited · clean");
    Ok(())
}
