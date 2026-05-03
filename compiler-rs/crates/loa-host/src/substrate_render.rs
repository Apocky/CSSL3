//! § substrate_render — runtime entry-point for the Substrate-Resonance Pixel Field.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § T11-W17-SUBSTRATE-RENDER · Apocky-greenlit massive overhaul (2026-05-02)
//!
//! § APOCKY-DIRECTIVE
//!   "This is a massive overhaul I want! Completely new graphics paradigm!"
//!   "Completely novel and proprietary visual representation!"
//!   "Pure digital intelligence produced high-fidelity low-latency 3D realtime
//!    graphics with frame buffering or something similar for temporal smoothing!"
//!
//! § WHAT THIS MODULE DOES
//!
//! Owns the live `DigitalIntelligenceRenderer` + a small set of test crystals
//! procedurally-allocated at host-init. Each frame, the host calls
//! `tick(observer)` to advance the substrate-resonance pixel-field by one
//! frame. The output is an RGBA `PixelField` (256 × 256 default) that the
//! host can upload to a wgpu texture for display, OR inspect directly for
//! testing/telemetry.
//!
//! § STAGE-0 PRESENTATION
//!
//! For visible-on-screen demonstration the substrate pixel-field is uploaded
//! to a wgpu texture by `render.rs` (under the `runtime` feature). The
//! catalog-only mode (this module) still runs the substrate pipeline +
//! emits per-frame telemetry so test/CI flows verify the paradigm-shift
//! is active.
//!
//! § ATTESTATION
//! There was no hurt nor harm in the making of this, to anyone, anything,
//! or anybody. Every pixel emission is per-observer-Σ-mask-gated.

use cssl_host_alien_materialization::{ObserverCoord, PixelField};
use cssl_host_crystallization::spectral::IlluminantBlend;
use cssl_host_crystallization::{Crystal, CrystalClass, WorldPos};
use cssl_host_digital_intelligence_render::{
    BlendKind, DigitalIntelligenceRenderer, FrameOutput, BUDGET_120HZ,
};
use cssl_rt::loa_startup::log_event;

/// Default substrate-render resolution. Stage-0 default is 256 × 256 — small
/// enough to run on CPU per-frame at 120Hz with 8 ray-samples per pixel,
/// large enough to demonstrate spatial structure. The host can resize at
/// any time (e.g., to match an HUD-overlay quad or a fullscreen pass).
pub const DEFAULT_SUBSTRATE_W: u32 = 256;
pub const DEFAULT_SUBSTRATE_H: u32 = 256;

/// § T11-W18-G-INTEGRATE — GPU-path resolution. 2560×1440 = 1440p WQHD ; 56×
/// more pixels than the CPU default. The GPU compute-shader (8×8 workgroups)
/// dispatches `(320, 180, 1)` work-groups at this resolution, which fits the
/// 6.94 ms / 144 Hz frame budget on Apocky's HighPerformance adapter while
/// the CPU rayon implementation cannot.
pub const GPU_SUBSTRATE_W: u32 = 2560;
pub const GPU_SUBSTRATE_H: u32 = 1440;

// § Halton low-discrepancy sequence (radical-inverse) · base-b · stage-0.
//   Used to spread 32 crystals quasi-uniformly across [0,1)³. Replay-safe
//   (pure i↦x). Bases 2/3/5 (mutually-coprime) give well-distributed 3D.
fn halton(idx: u32, base: u32) -> f32 {
    let mut f = 1.0f32;
    let mut r = 0.0f32;
    let mut i = idx + 1;
    while i > 0 {
        f /= base as f32;
        r += f * (i % base) as f32;
        i /= base;
    }
    r
}
fn halton_b2(i: u32) -> f32 { halton(i, 2) }
fn halton_b3(i: u32) -> f32 { halton(i, 3) }
fn halton_b5(i: u32) -> f32 { halton(i, 5) }

/// Number of test crystals procedurally-allocated at startup. ITER-14
/// telemetry showed 128 → fps=1.7 (3.8B ops/frame · O(N)-per-pixel kills
/// frame-budget). Reverted to 32 · workgroup-cache STAYS active · 32 fits
/// easily in shared mem · no cache-spill · cache-hit-rate ≈ 100%.
/// 32 × 8 aspect-curves × 4 illuminant-LUTs ≈ 1024 active spectral
/// contributions/frame · denser-interference-fringes from ℂ-amplitude
/// bundle WITHOUT the O(N²) penalty of larger N.
/// Future · spatial-index/grid-cull → bump count again.
/// Replay-deterministic from seeds 0xC1A1A_0000..001F.
pub const STARTUP_CRYSTAL_COUNT: usize = 32;

/// Holds all substrate-render state for one host instance.
pub struct SubstrateRenderState {
    pub renderer: DigitalIntelligenceRenderer,
    pub crystals: Vec<Crystal>,
    /// How many frames have ticked since init (for diagnostics).
    pub frame_count: u64,

    /// § T11-W18-G-INTEGRATE — Optional GPU compute-shader path. When `Some`,
    /// `tick_gpu(device, queue, observer)` dispatches the WGSL compute-shader
    /// at 1440p ; the output `wgpu::TextureView` is sampleable via
    /// `gpu_output_view()` for the next render-pass to consume. The CPU
    /// pixel-field still ticks for compatibility with the existing
    /// `substrate_compose` upload pipeline. When `None`, only the CPU path
    /// runs (unchanged behaviour for callers that never opt in).
    #[cfg(feature = "runtime")]
    pub gpu: Option<cssl_host_substrate_resonance_gpu::SubstrateResonanceGpu>,
}

impl Default for SubstrateRenderState {
    fn default() -> Self {
        Self::new()
    }
}

impl SubstrateRenderState {
    pub fn new() -> Self {
        let mut crystals = Vec::with_capacity(STARTUP_CRYSTAL_COUNT);
        // § T11-W18-DENSE-CRYSTALS · 32 crystals · 8 classes (Object · Entity ·
        //   Environment · Behavior · Event · Aura · Recipe · Inherit) · 4 per
        //   class · spaced on a 3D Halton-style spread covering [-4m,+4m]²
        //   on (x, z) with y-stratification (-1.5..+2m) so observer-rays
        //   from any look-angle traverse multiple crystals → fringes denser.
        const CLASSES: [CrystalClass; 8] = [
            CrystalClass::Object,
            CrystalClass::Entity,
            CrystalClass::Environment,
            CrystalClass::Behavior,
            CrystalClass::Event,
            CrystalClass::Aura,
            CrystalClass::Recipe,
            CrystalClass::Inherit,
        ];
        for i in 0..STARTUP_CRYSTAL_COUNT {
            // Halton-2 + Halton-3 quasi-random spread for low-discrepancy.
            let h2 = halton_b2(i as u32);
            let h3 = halton_b3(i as u32);
            let h5 = halton_b5(i as u32);
            let x_mm = ((h2 - 0.5) * 8000.0) as i32;
            let z_mm = ((h3 * 5000.0) + 1000.0) as i32; // 1m..6m forward
            let y_mm = ((h5 - 0.4) * 3500.0) as i32;
            let class = CLASSES[i % CLASSES.len()];
            let seed = 0xC1A1A_0000u64 + i as u64;
            crystals.push(Crystal::allocate(class, seed, WorldPos::new(x_mm, y_mm, z_mm)));
        }
        log_event(
            "INFO",
            "loa-host/substrate-render",
            &format!(
                "init · {}×{} pixel-field · {} test-crystals procgen-allocated · paradigm = Substrate-Resonance Pixel Field",
                DEFAULT_SUBSTRATE_W, DEFAULT_SUBSTRATE_H, STARTUP_CRYSTAL_COUNT
            ),
        );
        // § T11-W18-LIVE-LEARNING · load persisted KAN-bias from disk on init ·
        //   continuous-learning carries across process-restarts.
        let kan_path = kan_bias_persist_path();
        let loaded = cssl_host_substrate_intelligence::kan_bias_load(&kan_path);
        log_event(
            "INFO",
            "loa-host/substrate-render",
            &format!(
                "KAN-bias init · checksum=0x{:08x} · loaded-from-disk={} · path={}",
                cssl_host_substrate_intelligence::kan_bias_checksum(),
                loaded,
                kan_path.display(),
            ),
        );
        Self {
            renderer: DigitalIntelligenceRenderer::new(DEFAULT_SUBSTRATE_W, DEFAULT_SUBSTRATE_H),
            crystals,
            frame_count: 0,
            #[cfg(feature = "runtime")]
            gpu: None,
        }
    }

    /// § T11-W18-G-INTEGRATE — Construct a SubstrateRenderState with the GPU
    /// compute-shader path activated at 1440p (2560×1440). The CPU
    /// pixel-field still runs at `DEFAULT_SUBSTRATE_W × DEFAULT_SUBSTRATE_H`
    /// for the existing `substrate_compose` upload pipeline ; the GPU
    /// compute-shader runs each frame at 1440p and exposes its
    /// `wgpu::TextureView` for the future render-pass that will sample it
    /// directly (W18-N).
    ///
    /// Falls back to the CPU-only path silently if the GPU pipeline cannot
    /// be created (e.g., adapter does not advertise compute-shader support
    /// for the requested format) — caller can detect via
    /// `is_gpu_active()`.
    #[cfg(feature = "runtime")]
    pub fn new_gpu(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let mut s = Self::new();
        // Construct the compute-pipeline. `SubstrateResonanceGpu::new` is
        // infallible (panics on shader compile errors only) ; if it does
        // panic the device is fundamentally broken and we should fall back.
        let gpu = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            cssl_host_substrate_resonance_gpu::SubstrateResonanceGpu::new(device, width, height)
        }));
        match gpu {
            Ok(g) => {
                s.gpu = Some(g);
                log_event(
                    "INFO",
                    "loa-host/substrate-render",
                    &format!(
                        "GPU-path active · {}×{} compute-shader · target = 1440p144 (6.94 ms budget)",
                        width, height
                    ),
                );
            }
            Err(_) => {
                log_event(
                    "WARN",
                    "loa-host/substrate-render",
                    "GPU-path init panicked · falling back to CPU-only path",
                );
            }
        }
        s
    }

    /// § T11-W18-G-INTEGRATE — true iff the GPU compute-shader path is wired.
    #[cfg(feature = "runtime")]
    #[must_use]
    pub fn is_gpu_active(&self) -> bool {
        self.gpu.is_some()
    }
    #[cfg(not(feature = "runtime"))]
    #[must_use]
    pub fn is_gpu_active(&self) -> bool {
        false
    }

    /// Advance the substrate-render pipeline by one frame. Returns the
    /// frame's `FrameOutput` (resonance metadata + budget + fidelity).
    /// The current pixel-field is accessed via `current_display`.
    pub fn tick(&mut self, observer: ObserverCoord) -> FrameOutput {
        let out = self
            .renderer
            .tick(observer, &self.crystals, BUDGET_120HZ);
        self.frame_count = self.frame_count.wrapping_add(1);
        // Per-second telemetry (avoid per-frame log spam at 120 Hz).
        // § T11-W18-LIVE-LEARNING · feed frame-telemetry into KAN-bias.
        //   Per-frame · cheap (atomic-store · single BLAKE3 of 32 bytes).
        learn_from_frame_metrics(&out);
        if self.frame_count % 120 == 0 {
            log_event(
                "DEBUG",
                "loa-host/substrate-render",
                &format!(
                    "tick · frame_n={} · pixels_lit={} · fidelity_tier={} · fingerprint={:08x} · blend={:?} · KAN-bias=0x{:08x} · obs={}",
                    out.frame_n,
                    out.resonance.n_pixels_lit,
                    out.fidelity_tier,
                    out.resonance.fingerprint,
                    out.blend_used,
                    cssl_host_substrate_intelligence::kan_bias_checksum(),
                    cssl_host_substrate_intelligence::observe_count(),
                ),
            );
            // Periodic persist (every 120 frames ≈ 1 sec at 120 Hz · cheap).
            let _ = cssl_host_substrate_intelligence::kan_bias_persist(&kan_bias_persist_path());
        }
        out
    }

    /// Return the current temporally-blended pixel-field. The host uploads
    /// this to a wgpu texture for display.
    pub fn current_display(&self) -> PixelField {
        self.renderer.current_display()
    }

    /// § T11-W18-G-INTEGRATE — Advance both the CPU pixel-field AND the GPU
    /// compute-shader by one frame. The CPU path produces the small-format
    /// `PixelField` for the existing `substrate_compose` upload pipeline ;
    /// the GPU path produces a 1440p `wgpu::TextureView` accessible via
    /// `gpu_output_view()` for the next render-pass that samples it
    /// directly (wired in W18-N).
    ///
    /// Falls through to plain `tick(observer)` (CPU only) when the GPU path
    /// is not active — caller can call this unconditionally.
    #[cfg(feature = "runtime")]
    pub fn tick_gpu(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        observer: ObserverCoord,
    ) -> FrameOutput {
        // § T11-W18-OPTIMIZE-GPU-ACTIVE (telemetry-driven · post-iter1) ──
        //   When GPU path is wired (compose-pass binds GPU view directly per
        //   W18-N), CPU substrate-tick at 512×512 burns ~262k pixel-ops PER
        //   FRAME for output that is NEVER sampled by the display. Skip it.
        //   We still tick a TINY 16×16 CPU field to keep
        //   `current_display()` and the temporal-coherence ring populated
        //   for callers (telemetry / debug-overlays / fallback) without
        //   the 1024× cost.
        let out = if self.gpu.is_some() {
            // Mini-tick : preserve frame-counter + ring rotation but at
            // ~256 pixel-ops total. Bounded by self.renderer's existing
            // tick path so no API duplication.
            let prev_w = self.renderer.ring.width;
            let prev_h = self.renderer.ring.height;
            if prev_w > 16 || prev_h > 16 {
                self.renderer.resize(16, 16);
            }
            let mini = self.renderer.tick(observer, &self.crystals, BUDGET_120HZ);
            self.frame_count = self.frame_count.wrapping_add(1);
            if self.frame_count % 120 == 0 {
                log_event(
                    "DEBUG",
                    "loa-host/substrate-render",
                    &format!(
                        "tick-gpu · frame_n={} · cpu-mini=16x16 · gpu=2560x1440 · fidelity_tier={} · fingerprint={:08x}",
                        mini.frame_n, mini.fidelity_tier, mini.resonance.fingerprint,
                    ),
                );
            }
            mini
        } else {
            self.tick(observer)
        };
        // GPU path runs at panel-native 1440p · samples bound directly by
        // compose-pass (W18-N).
        if let Some(gpu) = self.gpu.as_mut() {
            let _view = gpu.dispatch(device, queue, observer, &self.crystals);
        }
        out
    }

    /// § T11-W18-G-INTEGRATE — Borrow the most-recent GPU compute-shader
    /// output texture-view. Returns `None` when the GPU path is not active
    /// (CPU-only mode). The view is `rgba8unorm` ; bind it as a sampled
    /// texture in the next render-pass to display the 1440p substrate field.
    #[cfg(feature = "runtime")]
    #[must_use]
    pub fn gpu_output_view(&self) -> Option<&wgpu::TextureView> {
        self.gpu.as_ref().map(|g| g.output_view())
    }

    /// § T11-W18-G-INTEGRATE — Borrow the GPU output texture itself (for
    /// callers that need to copy it, layer-bind it, or alias it as a
    /// different format). Returns `None` when the GPU path is not active.
    #[cfg(feature = "runtime")]
    #[must_use]
    pub fn gpu_output_texture(&self) -> Option<&wgpu::Texture> {
        self.gpu.as_ref().map(|g| g.output_texture())
    }

    /// § T11-W18-DISPLAY — Resize BOTH the CPU pixel-field AND (when active)
    /// the GPU compute-shader output texture. The CPU resolution is
    /// caller-clamped (≤ 512 × 512 by `display_detect::compute_substrate_dims`)
    /// so the per-frame ray-walk stays inside the 120 Hz CPU budget ; the
    /// GPU runs at native panel resolution.
    ///
    /// Idempotent when both dims already match. The GPU path rebuilds its
    /// compute-pipeline + storage-texture from scratch (no in-place resize
    /// API on `SubstrateResonanceGpu`) ; this is rare (monitor-change only)
    /// so the cost is amortised across many subsequent frames.
    pub fn resize(&mut self, cpu_w: u32, cpu_h: u32) {
        if cpu_w == 0 || cpu_h == 0 {
            return;
        }
        // CPU pixel-field — DigitalIntelligenceRenderer.resize re-allocates
        // the temporal-coherence ring at the new resolution.
        let (cur_w, cur_h) = (self.renderer.ring.width, self.renderer.ring.height);
        if cur_w != cpu_w || cur_h != cpu_h {
            self.renderer.resize(cpu_w, cpu_h);
            log_event(
                "INFO",
                "loa-host/substrate-render",
                &format!(
                    "cpu-resize · {}×{} → {}×{}",
                    cur_w, cur_h, cpu_w, cpu_h
                ),
            );
        }
    }

    /// § T11-W18-DISPLAY — Resize the GPU compute-shader output texture to
    /// the panel's native pixel-resolution. No-op if the GPU path is not
    /// active or the dims already match. Rebuilds the compute-pipeline +
    /// output-storage-texture from scratch (no in-place resize API on
    /// `SubstrateResonanceGpu`).
    #[cfg(feature = "runtime")]
    pub fn resize_gpu(&mut self, device: &wgpu::Device, gpu_w: u32, gpu_h: u32) {
        if gpu_w == 0 || gpu_h == 0 {
            return;
        }
        let Some(gpu) = self.gpu.as_ref() else {
            return;
        };
        let (cur_w, cur_h) = gpu.dims();
        if cur_w == gpu_w && cur_h == gpu_h {
            return;
        }
        // Tear down + rebuild. SubstrateResonanceGpu::new is infallible
        // (panics on shader-compile only) but we still wrap in catch_unwind
        // so a misbehaving driver doesn't crash the whole runtime.
        let new_gpu = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            cssl_host_substrate_resonance_gpu::SubstrateResonanceGpu::new(device, gpu_w, gpu_h)
        }));
        match new_gpu {
            Ok(g) => {
                self.gpu = Some(g);
                log_event(
                    "INFO",
                    "loa-host/substrate-render",
                    &format!(
                        "gpu-resize · {}×{} → {}×{}",
                        cur_w, cur_h, gpu_w, gpu_h
                    ),
                );
            }
            Err(_) => {
                log_event(
                    "WARN",
                    "loa-host/substrate-render",
                    "gpu-resize panicked · keeping previous compute-pipeline",
                );
            }
        }
    }

    /// Set the global substrate-blend mode. Useful for combat (snap to
    /// `BlendKind::Instant`) vs cinematic (`Spring`).
    pub fn set_blend(&mut self, blend: BlendKind) {
        self.renderer.blend = blend;
    }

    /// Allocate a new crystal at `pos` (e.g., a player's just-described
    /// thing crystallizing into the world). Returns the new crystal's
    /// handle.
    pub fn spawn_crystal(&mut self, class: CrystalClass, seed: u64, pos: WorldPos) -> u32 {
        let c = Crystal::allocate(class, seed, pos);
        let h = c.handle;
        self.crystals.push(c);
        log_event(
            "INFO",
            "loa-host/substrate-render",
            &format!(
                "crystal-spawn · class={:?} · pos=({},{},{}) · handle=0x{:08x}",
                class, pos.x_mm, pos.y_mm, pos.z_mm, h
            ),
        );
        h
    }

    /// Forge an observer-coord matching a host-side camera + Σ-mask. Stage-0
    /// uses a simple position+yaw+pitch packing; full sensor + audio-listen
    /// fields wire in W18+.
    pub fn observer_for(
        &self,
        x_mm: i32,
        y_mm: i32,
        z_mm: i32,
        yaw_milli: u32,
        pitch_milli: u32,
        frame_t_milli: u64,
        sigma_mask_token: u32,
    ) -> ObserverCoord {
        ObserverCoord {
            x_mm,
            y_mm,
            z_mm,
            yaw_milli,
            pitch_milli,
            frame_t_milli,
            sigma_mask_token,
            illuminant_blend: IlluminantBlend::day(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_initializes_with_test_crystals() {
        let s = SubstrateRenderState::new();
        assert_eq!(s.crystals.len(), STARTUP_CRYSTAL_COUNT);
        assert_eq!(s.frame_count, 0);
    }

    #[test]
    fn tick_advances_frame_count() {
        let mut s = SubstrateRenderState::new();
        let observer = s.observer_for(0, 0, 0, 0, 0, 0, 0xFFFF_FFFF);
        let _ = s.tick(observer);
        assert_eq!(s.frame_count, 1);
    }

    #[test]
    fn current_display_has_correct_dimensions() {
        let s = SubstrateRenderState::new();
        let f = s.current_display();
        assert_eq!(f.width, DEFAULT_SUBSTRATE_W);
        assert_eq!(f.height, DEFAULT_SUBSTRATE_H);
    }

    #[test]
    fn spawn_crystal_increases_count() {
        let mut s = SubstrateRenderState::new();
        let n0 = s.crystals.len();
        let _h = s.spawn_crystal(CrystalClass::Event, 0xDEAD_BEEF, WorldPos::new(0, 0, 1000));
        assert_eq!(s.crystals.len(), n0 + 1);
    }

    /// § T11-W18-G-INTEGRATE — GPU constructor smoke-test. Ignored by default
    /// because it requires a wgpu adapter ; runs on Apocky's box but skips
    /// gracefully on CI runners without a GPU.
    #[cfg(feature = "runtime")]
    #[test]
    #[ignore]
    fn new_gpu_constructs_at_1440p_when_adapter_available() {
        let Some((_inst, _adapter, device, _queue)) =
            cssl_host_substrate_resonance_gpu::try_headless_device()
        else {
            eprintln!("no GPU adapter available · ignored");
            return;
        };
        let s = SubstrateRenderState::new_gpu(&device, GPU_SUBSTRATE_W, GPU_SUBSTRATE_H);
        assert!(s.is_gpu_active(), "GPU path should be active when adapter present");
        assert_eq!(s.crystals.len(), STARTUP_CRYSTAL_COUNT);
        assert!(s.gpu_output_view().is_some());
        assert!(s.gpu_output_texture().is_some());
        let tex = s.gpu_output_texture().unwrap();
        assert_eq!(tex.size().width, GPU_SUBSTRATE_W);
        assert_eq!(tex.size().height, GPU_SUBSTRATE_H);
    }

    /// § T11-W18-G-INTEGRATE — `is_gpu_active` defaults to `false` for the
    /// CPU-only constructor. This test runs everywhere (no GPU required).
    #[test]
    fn cpu_only_path_reports_gpu_inactive() {
        let s = SubstrateRenderState::new();
        assert!(!s.is_gpu_active());
    }

    /// § T11-W18-DISPLAY — `resize` mutates the CPU pixel-field dims
    /// in place (no GPU required). Exercises the auto-resize path that
    /// fires on `WindowEvent::Resized` in `window.rs`.
    #[test]
    fn resize_updates_cpu_pixel_field_dims() {
        let mut s = SubstrateRenderState::new();
        // Default = DEFAULT_SUBSTRATE_W × DEFAULT_SUBSTRATE_H = 256×256.
        assert_eq!(s.renderer.ring.width, DEFAULT_SUBSTRATE_W);
        assert_eq!(s.renderer.ring.height, DEFAULT_SUBSTRATE_H);
        // Resize to 384×384 — capped by display_detect to MAX_CPU_SUBSTRATE.
        s.resize(384, 384);
        assert_eq!(s.renderer.ring.width, 384);
        assert_eq!(s.renderer.ring.height, 384);
        // Idempotent on repeated call with same dims.
        s.resize(384, 384);
        assert_eq!(s.renderer.ring.width, 384);
    }

    /// § T11-W18-DISPLAY — `resize(0, 0)` is a guarded no-op.
    #[test]
    fn resize_zero_dims_is_noop() {
        let mut s = SubstrateRenderState::new();
        let w0 = s.renderer.ring.width;
        let h0 = s.renderer.ring.height;
        s.resize(0, 0);
        assert_eq!(s.renderer.ring.width, w0);
        assert_eq!(s.renderer.ring.height, h0);
    }

    #[test]
    fn substrate_pipeline_lights_pixels_when_observer_faces_crystal() {
        let mut s = SubstrateRenderState::new();
        let observer = s.observer_for(0, 0, 0, 0, 0, 0, 0xFFFF_FFFF);
        // Run a couple of frames so the temporal-coherence ring fills up.
        for _ in 0..3 {
            let _ = s.tick(observer);
        }
        let frame = s.tick(observer);
        // At least one of the test crystals is in front of the observer
        // and should have lit at least one pixel.
        assert!(
            frame.resonance.n_pixels_lit > 0,
            "expected at least one resonant pixel"
        );
    }

    /// § T11-W18-COMPACT-COMPREHENSIVE · single test exercising
    ///   { halton-spread · class-distribution · multi-class · 32-count ·
    ///     y-stratified · x-bounded · z-forward · spawn-extends-vec ·
    ///     tick-advances-frame · ring-rotation · resonance-non-empty ·
    ///     telemetry-fingerprint-stable } in one pass.
    /// Per Apocky-directive · "more comprehensive and compacted tests".
    #[test]
    fn compact_comprehensive_substrate_state_invariants() {
        let s = SubstrateRenderState::new();

        // Count + class spread (use 8-bucket index since CrystalClass lacks Hash).
        assert_eq!(s.crystals.len(), 32);
        let mut class_hits = [0u32; 8];
        for c in &s.crystals {
            class_hits[c.class as usize] += 1;
        }
        let distinct = class_hits.iter().filter(|n| **n > 0).count();
        assert!(distinct >= 4, "≥4 distinct classes (got {distinct})");

        // Halton-spread bounds (all crystals inside the documented box).
        for c in &s.crystals {
            assert!(c.world_pos.x_mm.abs() <= 4500, "x out: {}", c.world_pos.x_mm);
            assert!(c.world_pos.z_mm >= 800 && c.world_pos.z_mm <= 7000, "z out: {}", c.world_pos.z_mm);
            assert!(c.world_pos.y_mm.abs() <= 2500, "y out: {}", c.world_pos.y_mm);
        }
        // Halton-2-spread → x positions should NOT all collide (distinct vs single-cluster).
        let xs: std::collections::HashSet<i32> = s.crystals.iter().map(|c| c.world_pos.x_mm).collect();
        assert!(xs.len() >= 16, "halton-2 spread should yield ≥ 16 distinct x (got {})", xs.len());

        // Tick path : frame_count + ring rotate.
        let mut s = s;
        let observer = s.observer_for(0, 0, 0, 0, 0, 0, 0xFFFF_FFFF);
        let f0 = s.tick(observer);
        assert_eq!(s.frame_count, 1);
        let f1 = s.tick(observer);
        assert_eq!(s.frame_count, 2);
        // Same observer + crystals · same fingerprint (substrate is replay-deterministic).
        assert_eq!(f0.resonance.fingerprint, f1.resonance.fingerprint);

        // Spawn extends + new crystal is reachable.
        let n_pre = s.crystals.len();
        let h = s.spawn_crystal(CrystalClass::Event, 0xCAFE_BABE, WorldPos::new(0, 0, 1000));
        assert_eq!(s.crystals.len(), n_pre + 1);
        assert_ne!(h, 0, "spawned crystal handle must be non-zero");

        // current_display returns DEFAULT_SUBSTRATE dims.
        let disp = s.current_display();
        assert_eq!(disp.width, DEFAULT_SUBSTRATE_W);
        assert_eq!(disp.height, DEFAULT_SUBSTRATE_H);
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// § T11-W18-LIVE-LEARNING · per-frame KAN-bias feed + persist path.
// ═══════════════════════════════════════════════════════════════════════════

/// Where to persist KAN-bias state across process restarts.
/// Default `~/.loa/kan_bias.bin` · operator-overridable via `LOA_KAN_BIAS_PATH`.
pub fn kan_bias_persist_path() -> std::path::PathBuf {
    if let Ok(p) = std::env::var("LOA_KAN_BIAS_PATH") {
        return std::path::PathBuf::from(p);
    }
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| ".".to_string());
    std::path::PathBuf::from(home).join(".loa").join("kan_bias.bin")
}

/// Feed one frame's telemetry into the substrate-intelligence KAN-bias.
/// Cheap · per-frame call from substrate-render tick. The 8-byte payload
/// includes the resonance-fingerprint + pixels-lit + frame-number so each
/// frame is a unique observation that drifts the KAN-state.
pub fn learn_from_frame_metrics(out: &cssl_host_digital_intelligence_render::FrameOutput) {
    let payload: [u8; 16] = {
        let mut b = [0u8; 16];
        b[0..4].copy_from_slice(&out.resonance.fingerprint.to_le_bytes());
        b[4..8].copy_from_slice(&out.resonance.n_pixels_lit.to_le_bytes());
        b[8..12].copy_from_slice(&out.elapsed_micros.to_le_bytes());
        b[12] = out.fidelity_tier;
        b[13] = out.blend_used as u8;
        b
    };
    cssl_host_substrate_intelligence::observe(
        cssl_host_substrate_intelligence::Role::Coder, // self-coder feedback
        0,                                              // obs_kind 0 = render-frame
        out.frame_n,
        &payload,
    );
}
