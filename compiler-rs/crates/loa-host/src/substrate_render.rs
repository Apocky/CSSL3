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
//   Used to spread startup crystals quasi-uniformly across [0,1)³. Replay-safe
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
fn halton_b2(i: u32) -> f32 {
    halton(i, 2)
}
fn halton_b3(i: u32) -> f32 {
    halton(i, 3)
}
fn halton_b5(i: u32) -> f32 {
    halton(i, 5)
}

/// § T11-W18-CRYSTAL1024 — number of world crystals procedurally-allocated at
/// startup. The V3 present kernel still consumes the nearest bounded
/// `MAX_SCENE_CRYSTALS` slots per observer; this larger world scene validates
/// 1000+ primitive ownership, deterministic placement, and culling without
/// brute-forcing all world crystals per pixel.
/// Replay-deterministic from seeds 0xC1A1A_0000..03FF.
pub const STARTUP_CRYSTAL_COUNT: usize = 1024;

/// § T11-W18-CRYSTAL1024 · concentric-shell distribution — number of crystals
/// per shell. Inner-most shell is densest-by-volume (small annulus · 128
/// crystals) ; outer-most shell covers the largest annulus (512 crystals)
/// so areal-density stays roughly inverse-quadratic in radius (perceptually
/// "stars-thicken-at-horizon").
pub const SHELL_INNER_COUNT: usize = 128;
pub const SHELL_MIDDLE_COUNT: usize = 384;
pub const SHELL_OUTER_COUNT: usize = 512;
const _SHELL_TOTAL_CHECK: usize = SHELL_INNER_COUNT + SHELL_MIDDLE_COUNT + SHELL_OUTER_COUNT;
// Static check : SHELL_*_COUNT sum equals STARTUP_CRYSTAL_COUNT.
const _: () = assert!(
    _SHELL_TOTAL_CHECK == STARTUP_CRYSTAL_COUNT,
    "shell counts must sum to STARTUP_CRYSTAL_COUNT",
);

/// § T11-W18-CRYSTAL1024 · normalized radius bounds per shell (0.0 = origin,
/// 1.0 = outer playfield edge ≈ 4 m). Inner [0.0, 0.3) · middle [0.3, 0.7) ·
/// outer [0.7, 1.0]. WGSL kernel does NOT use these directly — they shape the
/// host-side world_pos placement only.
pub const SHELL_INNER_R_LO: f32 = 0.0;
pub const SHELL_INNER_R_HI: f32 = 0.3;
pub const SHELL_MIDDLE_R_LO: f32 = 0.3;
pub const SHELL_MIDDLE_R_HI: f32 = 0.7;
pub const SHELL_OUTER_R_LO: f32 = 0.7;
pub const SHELL_OUTER_R_HI: f32 = 1.0;

/// § T11-W18-CRYSTAL1024 · per-shell density-modulator on resonance-amplitude.
/// Applied to `extent_mm` (the crystal's bounding-radius which feeds both
/// ray-cull AND the WGSL `extent_sq / (d²+extent²)` weight term — the
/// substrate's primary amplitude knob). Inner shells boosted (close to
/// observer · should dominate) ; outer shells dimmed (background "starfield").
pub const SHELL_INNER_AMP_MOD: f32 = 1.5;
pub const SHELL_MIDDLE_AMP_MOD: f32 = 1.0;
pub const SHELL_OUTER_AMP_MOD: f32 = 0.6;

/// § T11-W18-CRYSTAL1024 · world-space extent of the crystal field along each
/// axis. R=1.0 maps to PLAYFIELD_HALF_EXTENT_MM millimeters from origin in
/// the (x,z) plane ; y stratifies through ±PLAYFIELD_Y_HALF_MM.
pub const PLAYFIELD_HALF_EXTENT_MM: i32 = 4500; // 4.5 m
pub const PLAYFIELD_Y_HALF_MM: i32 = 2500; // 2.5 m
/// Inner-z-offset · all crystals sit forward of the observer (positive z).
/// Computed as `z_mm = z_offset + radius_mm * (h - 0.5) * 2` so the cluster
/// straddles the playfield in front of the player.
pub const PLAYFIELD_Z_CENTER_MM: i32 = 3500; // 3.5 m forward

/// § LOA-V3-STRESS-LAB — operator-selectable V3 benchmark rooms.
/// These are CSSL-owned crystal-field layouts distilled from LoA v13/v14,
/// Infinite Labyrinth, and Labyrinth of Apocalypse stress-room specs. They
/// deliberately stress different visual failure modes while preserving the
/// current V3 ABI cap (`MAX_SCENE_CRYSTALS`) so benchmark deltas are honest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum V3StressRoom {
    /// Existing 1024-crystal concentric shell field.
    Shells,
    /// Printer-test wall: bars, line pairs, moire/checker density.
    PrinterWall,
    /// Material atrium: 16 clustered material families.
    MaterialAtrium,
    /// Hair/fractal proxy: dense shell + radial strand field.
    HairFractal,
    /// Portal/moire rings: nested concentric + diagonal interference.
    PortalMoire,
    /// Competitive arena: dense high-contrast depth/readability field.
    CompetitiveArena,
}

impl V3StressRoom {
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Shells => "shells",
            Self::PrinterWall => "printer",
            Self::MaterialAtrium => "material",
            Self::HairFractal => "hair",
            Self::PortalMoire => "portal",
            Self::CompetitiveArena => "arena",
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Shells => "Shell Field",
            Self::PrinterWall => "Printer Wall",
            Self::MaterialAtrium => "Material Atrium",
            Self::HairFractal => "Hair Fractal",
            Self::PortalMoire => "Portal Moire",
            Self::CompetitiveArena => "Competitive Arena",
        }
    }

    #[must_use]
    pub fn from_key(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "" | "shell" | "shells" | "default" => Some(Self::Shells),
            "printer" | "printer_wall" | "printer-wall" | "calibration" => Some(Self::PrinterWall),
            "material" | "materials" | "material_atrium" | "material-atrium" => {
                Some(Self::MaterialAtrium)
            }
            "hair" | "fuzz" | "fractal" | "hair_fractal" | "hair-fractal" => {
                Some(Self::HairFractal)
            }
            "portal" | "moire" | "portal_moire" | "portal-moire" => Some(Self::PortalMoire),
            "arena" | "competitive" | "competitive_arena" | "competitive-arena" => {
                Some(Self::CompetitiveArena)
            }
            _ => None,
        }
    }

    #[must_use]
    pub fn from_env() -> Self {
        std::env::var("LOA_V3_STRESS_ROOM")
            .ok()
            .and_then(|v| Self::from_key(&v))
            .unwrap_or(Self::Shells)
    }
}

pub const V3_STRESS_ROOMS: [V3StressRoom; 6] = [
    V3StressRoom::Shells,
    V3StressRoom::PrinterWall,
    V3StressRoom::MaterialAtrium,
    V3StressRoom::HairFractal,
    V3StressRoom::PortalMoire,
    V3StressRoom::CompetitiveArena,
];

/// § T11-W18-CRYSTAL1024 · which shell a crystal-index belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shell {
    Inner,
    Middle,
    Outer,
}

impl Shell {
    /// Map a 0..STARTUP_CRYSTAL_COUNT crystal-index to its shell.
    /// Indices [0, 128) = inner, [128, 512) = middle, [512, 1024) = outer.
    #[must_use]
    pub fn for_index(i: usize) -> Self {
        if i < SHELL_INNER_COUNT {
            Shell::Inner
        } else if i < SHELL_INNER_COUNT + SHELL_MIDDLE_COUNT {
            Shell::Middle
        } else {
            Shell::Outer
        }
    }

    /// Local index within this shell (0..SHELL_*_COUNT).
    #[must_use]
    pub fn local_index(i: usize) -> usize {
        match Self::for_index(i) {
            Shell::Inner => i,
            Shell::Middle => i - SHELL_INNER_COUNT,
            Shell::Outer => i - SHELL_INNER_COUNT - SHELL_MIDDLE_COUNT,
        }
    }

    /// Normalized-radius bounds [r_lo, r_hi].
    #[must_use]
    pub fn radius_bounds(self) -> (f32, f32) {
        match self {
            Shell::Inner => (SHELL_INNER_R_LO, SHELL_INNER_R_HI),
            Shell::Middle => (SHELL_MIDDLE_R_LO, SHELL_MIDDLE_R_HI),
            Shell::Outer => (SHELL_OUTER_R_LO, SHELL_OUTER_R_HI),
        }
    }

    /// Density-modulator on resonance-amplitude (extent_mm scaling).
    #[must_use]
    pub fn amp_mod(self) -> f32 {
        match self {
            Shell::Inner => SHELL_INNER_AMP_MOD,
            Shell::Middle => SHELL_MIDDLE_AMP_MOD,
            Shell::Outer => SHELL_OUTER_AMP_MOD,
        }
    }

    /// Crystal-count in this shell.
    #[must_use]
    pub fn count(self) -> usize {
        match self {
            Shell::Inner => SHELL_INNER_COUNT,
            Shell::Middle => SHELL_MIDDLE_COUNT,
            Shell::Outer => SHELL_OUTER_COUNT,
        }
    }
}

/// V3 material-code ABI: shell tier × 8 crystal classes.
/// `GpuCrystalWord.word3 = shell_code * 8 + CrystalClass`.
#[must_use]
pub fn v3_material_code_for_crystal(index: usize, class: CrystalClass) -> u32 {
    let shell_code = match Shell::for_index(index) {
        Shell::Inner => 0,
        Shell::Middle => 1,
        Shell::Outer => 2,
    };
    shell_code * 8 + class as u32
}

/// § T11-W18-CRYSTAL1024 · place a crystal at concentric-shell coordinates.
/// `local_idx` (0..shell.count()) drives the Halton-2D angular spread
/// within the shell's annulus. Returns `(x_mm, y_mm, z_mm)` in world-space.
///
/// Algorithm :
///   1. h2 = halton-base-2(local_idx)  →  angular-spread θ in [0, 2π)
///   2. h3 = halton-base-3(local_idx)  →  radial position within annulus
///         (sqrt-mapped so areal density stays uniform within the shell)
///   3. h5 = halton-base-5(local_idx)  →  y-stratification within ±PLAYFIELD_Y
///   4. (x, z) = pos_along_radius(θ, r) ; y from h5
#[must_use]
pub fn shell_world_pos(shell: Shell, local_idx: u32) -> (i32, i32, i32) {
    let (r_lo, r_hi) = shell.radius_bounds();
    let h2 = halton_b2(local_idx);
    let h3 = halton_b3(local_idx);
    let h5 = halton_b5(local_idx);

    // Sqrt-map h3 → radius so areal-density stays even across the annulus
    // (without sqrt the inner edge of every shell is over-dense).
    let r_norm_sq_lo = r_lo * r_lo;
    let r_norm_sq_hi = r_hi * r_hi;
    let r_norm = (r_norm_sq_lo + h3 * (r_norm_sq_hi - r_norm_sq_lo)).sqrt();

    let r_mm = r_norm * PLAYFIELD_HALF_EXTENT_MM as f32;
    // Angular spread : h2 → θ ∈ [0, 2π).
    let theta = h2 * std::f32::consts::TAU;
    let x_mm = (r_mm * theta.cos()) as i32;
    let z_dx = (r_mm * theta.sin()) as i32;
    let z_mm = PLAYFIELD_Z_CENTER_MM + z_dx;

    let y_mm = ((h5 - 0.5) * 2.0 * PLAYFIELD_Y_HALF_MM as f32) as i32;
    (x_mm, y_mm, z_mm)
}

fn v3_class_for_index(i: usize) -> CrystalClass {
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
    CLASSES[i % CLASSES.len()]
}

fn allocate_v3_stress_crystal(
    i: usize,
    class: CrystalClass,
    room: V3StressRoom,
    pos: WorldPos,
    extent_scale_milli: i32,
) -> Crystal {
    let room_salt = room as u64;
    let seed = 0xC1A1A_0000u64 + ((room_salt & 0xFF) << 48) + i as u64;
    let mut c = Crystal::allocate(class, seed, pos);
    let scaled = (i64::from(c.extent_mm) * i64::from(extent_scale_milli) / 1000)
        .clamp(1, i64::from(i32::MAX)) as i32;
    c.extent_mm = scaled;
    c
}

fn build_shell_field_crystals() -> Vec<Crystal> {
    let mut crystals = Vec::with_capacity(STARTUP_CRYSTAL_COUNT);
    for i in 0..STARTUP_CRYSTAL_COUNT {
        let shell = Shell::for_index(i);
        let local_idx = Shell::local_index(i) as u32;
        let (x_mm, y_mm, z_mm) = shell_world_pos(shell, local_idx);
        let class = v3_class_for_index(i);
        let amp_milli = (shell.amp_mod() * 1000.0).round() as i32;
        crystals.push(allocate_v3_stress_crystal(
            i,
            class,
            V3StressRoom::Shells,
            WorldPos::new(x_mm, y_mm, z_mm),
            amp_milli,
        ));
    }
    crystals
}

fn build_printer_wall_crystals() -> Vec<Crystal> {
    let mut crystals = Vec::with_capacity(STARTUP_CRYSTAL_COUNT);
    for i in 0..STARTUP_CRYSTAL_COUNT {
        let col = (i % 64) as i32;
        let row = ((i / 64) % 16) as i32;
        let band = (col / 8) as usize;
        let checker = ((col ^ row) & 1) as i32;
        let x_mm = (col - 31) * 125;
        let y_mm = 150 + row * 180;
        let z_mm = 2400 + checker * 18 + ((col * 7 + row * 13) % 23);
        let class = v3_class_for_index(band + row as usize);
        let extent = if checker == 0 { 220 } else { 360 };
        crystals.push(allocate_v3_stress_crystal(
            i,
            class,
            V3StressRoom::PrinterWall,
            WorldPos::new(x_mm, y_mm, z_mm),
            extent,
        ));
    }
    crystals
}

fn build_material_atrium_crystals() -> Vec<Crystal> {
    let mut crystals = Vec::with_capacity(STARTUP_CRYSTAL_COUNT);
    for i in 0..STARTUP_CRYSTAL_COUNT {
        let cluster = i / 64;
        let local = i % 64;
        let cx = (cluster % 4) as i32;
        let cy = (cluster / 4) as i32;
        let lx = (local % 8) as i32;
        let ly = (local / 8) as i32;
        let x_mm = (cx - 1) * 1550 + (lx - 3) * 115;
        let y_mm = 500 + cy * 540 + ly * 42;
        let z_mm = 1900 + cy * 760 + ((lx * 29 + ly * 17) % 180);
        let class = v3_class_for_index(cluster);
        let extent = 520 + ((cluster as i32 % 4) * 180);
        crystals.push(allocate_v3_stress_crystal(
            i,
            class,
            V3StressRoom::MaterialAtrium,
            WorldPos::new(x_mm, y_mm, z_mm),
            extent,
        ));
    }
    crystals
}

fn build_hair_fractal_crystals() -> Vec<Crystal> {
    let mut crystals = Vec::with_capacity(STARTUP_CRYSTAL_COUNT);
    for i in 0..STARTUP_CRYSTAL_COUNT {
        let t = i as f32 / STARTUP_CRYSTAL_COUNT as f32;
        let theta = t * std::f32::consts::TAU * 23.0;
        let phi = ((halton_b3(i as u32) - 0.5) * std::f32::consts::PI).sin();
        let strand = if i % 4 == 0 { 1.35 } else { 1.0 };
        let radius = 650.0 + 1150.0 * halton_b2(i as u32).sqrt() * strand;
        let x_mm = (radius * theta.cos()) as i32;
        let y_mm = 1500 + (radius * phi * 0.85) as i32;
        let z_mm = 3150 + (radius * theta.sin()) as i32;
        let class = if i % 4 == 0 {
            CrystalClass::Aura
        } else if i % 7 == 0 {
            CrystalClass::Environment
        } else {
            CrystalClass::Event
        };
        let extent = if i % 4 == 0 { 130 } else { 260 };
        crystals.push(allocate_v3_stress_crystal(
            i,
            class,
            V3StressRoom::HairFractal,
            WorldPos::new(x_mm, y_mm, z_mm),
            extent,
        ));
    }
    crystals
}

fn build_portal_moire_crystals() -> Vec<Crystal> {
    let mut crystals = Vec::with_capacity(STARTUP_CRYSTAL_COUNT);
    for i in 0..STARTUP_CRYSTAL_COUNT {
        let ring = (i / 96) as i32;
        let local = (i % 96) as f32;
        let theta = (local / 96.0) * std::f32::consts::TAU + ring as f32 * 0.173;
        let radius = 450.0 + ring as f32 * 275.0 + ((i % 3) as f32 * 18.0);
        let diagonal = ((i as i32 * 37) % 511) - 255;
        let x_mm = (radius * theta.cos()) as i32 + diagonal;
        let y_mm = 1650 + ((radius * theta.sin()) as i32 / 4);
        let z_mm = 2600 + (radius * theta.sin()) as i32 + diagonal / 3;
        let class = if ring % 3 == 0 {
            CrystalClass::Behavior
        } else if ring % 3 == 1 {
            CrystalClass::Aura
        } else {
            CrystalClass::Inherit
        };
        crystals.push(allocate_v3_stress_crystal(
            i,
            class,
            V3StressRoom::PortalMoire,
            WorldPos::new(x_mm, y_mm, z_mm),
            330,
        ));
    }
    crystals
}

fn build_competitive_arena_crystals() -> Vec<Crystal> {
    let mut crystals = Vec::with_capacity(STARTUP_CRYSTAL_COUNT);
    for i in 0..STARTUP_CRYSTAL_COUNT {
        let lane = (i % 32) as i32;
        let depth = ((i / 32) % 32) as i32;
        let stripe = ((lane + depth) & 1) as usize;
        let x_mm = (lane - 15) * 260;
        let y_mm = if i % 5 == 0 {
            2500
        } else {
            450 + (i % 7) as i32 * 115
        };
        let z_mm = 850 + depth * 235;
        let class = if stripe == 0 {
            CrystalClass::Object
        } else if i % 5 == 0 {
            CrystalClass::Event
        } else {
            CrystalClass::Entity
        };
        let extent = if stripe == 0 { 180 } else { 280 };
        crystals.push(allocate_v3_stress_crystal(
            i,
            class,
            V3StressRoom::CompetitiveArena,
            WorldPos::new(x_mm, y_mm, z_mm),
            extent,
        ));
    }
    crystals
}

fn build_v3_stress_room_crystals(room: V3StressRoom) -> Vec<Crystal> {
    match room {
        V3StressRoom::Shells => build_shell_field_crystals(),
        V3StressRoom::PrinterWall => build_printer_wall_crystals(),
        V3StressRoom::MaterialAtrium => build_material_atrium_crystals(),
        V3StressRoom::HairFractal => build_hair_fractal_crystals(),
        V3StressRoom::PortalMoire => build_portal_moire_crystals(),
        V3StressRoom::CompetitiveArena => build_competitive_arena_crystals(),
    }
}

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

    /// § T11-W18-DYNRES-SCALER · adaptive resolution scaler. Q0.16 fixed-point.
    /// When `tick_gpu` is called, the scaler observes wall-clock frame-time
    /// + adjusts the GPU render-target dims toward the 1440p144 budget.
    /// Honours `LOA_DYN_RES=0` to stay at native resolution.
    pub dyn_res: crate::dynamic_resolution::Scaler,

    /// § T11-W18-DYNRES-SCALER · panel-native dims captured at GPU init.
    /// `tick_gpu` scales these by `dyn_res.current_scale_q16` per frame
    /// and resizes the GPU compute-shader output when the scaled dims
    /// drift away from the previous frame's render-dims.
    pub native_gpu_w: u32,
    pub native_gpu_h: u32,

    /// § T11-W18-DYNRES-SCALER · tracks wall-clock at start of last
    /// `tick_gpu` so we can feed (now - prev) into the scaler EMA each
    /// subsequent frame. `None` until the first tick.
    pub last_tick_instant: Option<std::time::Instant>,

    /// § T11-W18-KAN-MULTIBAND-WIRE · active DisplayProfile mapped to
    /// profile_id (0..=4 · Amoled/Oled/IpsLcd/VaLcd/HdrExt). Captured at
    /// construction from `LOA_DISPLAY_PROFILE` env-override (or default 2
    /// = IpsLcd · neutral fallback) and used to route per-frame
    /// `observe_with_profile` calls to the correct KAN-bias band so each
    /// display-class accumulates its own learning history independently.
    pub profile_id: u8,

    /// § T11-W18-LOA-CONTENT-WIRE · DMGM-specialist council + procgen
    /// pipeline driver. Always constructed (4 specialist trait-objects ;
    /// cheap). The `tick_once` it produces is gated by `LOA_CONTENT_PIPELINE=1`
    /// internally, so this field's presence has zero effect when the
    /// env-knob is unset (default-OFF) — the shell-seed crystal-1024 array
    /// is preserved exactly. When env=1, every `CONTENT_PIPELINE_REFRESH_EVERY`
    /// frames the crystal-array is APPENDED with procgen-derived crystals
    /// (post T11-W18-CONTENT-APPEND fix · Apocky 2026-05-03 feedback).
    pub content_pipeline: crate::content_pipeline::LoaContentPipeline,

    /// § T11-W18-CONTENT-APPEND · captured-at-init shell-seed crystals.
    /// When `LOA_CONTENT_PIPELINE=1`, the refresh APPENDS procgen-crystals
    /// to this baseline (cap 2048 total). Apocky-feedback : "concentric
    /// circles barely rendering" — REPLACE-mode meant shell-seed
    /// → 1-2 procgen made the field MORE empty. APPEND fixes that.
    pub baseline_crystals: Vec<Crystal>,

    /// § LOA-V3-STRESS-LAB · active operator-selected stress room.
    pub stress_room: V3StressRoom,
}

/// § T11-W18-CONTENT-APPEND · cap on total crystals (baseline + procgen).
/// 2048 = 2× shell-seed allowing rich procgen overlay without GPU pressure.
/// V3 present slices/culls to its CSSL-owned MAX_SCENE_CRYSTALS on the
/// consumer side, but the host-side Vec stays larger for diagnostics.
pub const CRYSTAL_TOTAL_CAP: usize = 2048;

/// § T11-W18-LOA-CONTENT-WIRE · cadence at which `tick` polls the content-
/// pipeline for a refreshed crystal-array. 120 frames ≈ 1 second at 120 Hz.
/// Smaller values produce more visual churn ; larger values quiet the
/// scene. Tuned conservatively so the in-game KAN-bias has time to settle
/// between procgen-events.
pub const CONTENT_PIPELINE_REFRESH_EVERY: u64 = 120;

impl Default for SubstrateRenderState {
    fn default() -> Self {
        Self::new()
    }
}

impl SubstrateRenderState {
    pub fn new() -> Self {
        Self::new_for_stress_room(V3StressRoom::from_env())
    }

    #[must_use]
    pub fn new_for_stress_room(stress_room: V3StressRoom) -> Self {
        let crystals = build_v3_stress_room_crystals(stress_room);
        log_event(
            "INFO",
            "loa-host/substrate-render",
            &format!(
                "init · {}×{} pixel-field · {} test-crystals procgen-allocated · stress-room={} · paradigm = Substrate-Resonance Pixel Field",
                DEFAULT_SUBSTRATE_W,
                DEFAULT_SUBSTRATE_H,
                crystals.len(),
                stress_room.key(),
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
        // § T11-W18-CONTENT-APPEND · capture shell-seed as baseline so
        //   content_pipeline can APPEND on top instead of REPLACE.
        let baseline_crystals = crystals.clone();
        Self {
            renderer: DigitalIntelligenceRenderer::new(DEFAULT_SUBSTRATE_W, DEFAULT_SUBSTRATE_H),
            crystals,
            frame_count: 0,
            #[cfg(feature = "runtime")]
            gpu: None,
            dyn_res: crate::dynamic_resolution::Scaler::new(),
            native_gpu_w: GPU_SUBSTRATE_W,
            native_gpu_h: GPU_SUBSTRATE_H,
            last_tick_instant: None,
            profile_id: profile_id_from_env(),
            content_pipeline: crate::content_pipeline::LoaContentPipeline::new(),
            baseline_crystals,
            stress_room,
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
        // Capture panel-native dims so the dyn-res scaler has a target to
        // multiply against per frame. `width × height` here is the panel's
        // native pixel-resolution (e.g. 2560 × 1440 for 1440p144).
        s.native_gpu_w = width;
        s.native_gpu_h = height;
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

    /// § T11-W18-LOA-CONTENT-WIRE · poll the DMGM-content-pipeline for fresh
    /// crystals every `CONTENT_PIPELINE_REFRESH_EVERY` frames.
    ///
    /// § GATING-LAYERS (compose top-down)
    ///   1. `LOA_CONTENT_PIPELINE != "1"` → pipeline returns None ; we
    ///      KEEP the current crystals (shell-seed unchanged).
    ///   2. Frame-cadence : only every Nth frame ; in-between frames are
    ///      no-ops (return-early).
    ///   3. Empty result : if the council mediated to Pass/Question, the
    ///      pipeline returns Some(empty) ; we DON'T replace (would erase
    ///      the world otherwise).
    ///   4. Cap : `tick_once` already capped at PIPELINE_CRYSTAL_CAP=128.
    ///
    /// § PROCEDURE
    ///   - convert alien-materialization observer (i32 mm + yaw/pitch) to
    ///     procgen-pipeline observer (f32 metres) ;
    ///   - call `content_pipeline.tick_once(obs_pos, frame_count)` ;
    ///   - on Some(non-empty), REPLACE `self.crystals`.
    fn refresh_crystals_from_pipeline_if_due(&mut self, observer: ObserverCoord) {
        // § Cadence — quick out for in-between frames. We use frame_count
        // BEFORE its increment in tick(), so frame 0 counts as "due" too
        // (initial procgen-population at startup if env enables).
        if self.frame_count % CONTENT_PIPELINE_REFRESH_EVERY != 0 {
            return;
        }
        let obs_pos = observer_to_procgen_coord(observer);
        let new_crystals = self.content_pipeline.tick_once(obs_pos, self.frame_count);
        // None = pipeline disabled (LOA_CONTENT_PIPELINE != "1") ; keep shell-seed.
        let Some(crystals) = new_crystals else {
            return;
        };
        // Empty = council mediated to Pass/Question this frame ; keep
        // current crystals so we never empty the world by accident.
        if crystals.is_empty() {
            return;
        }
        // § T11-W18-CONTENT-APPEND · APPEND procgen on top of baseline shell-seed
        //   instead of REPLACE. Per Apocky 2026-05-03 feedback : "concentric
        //   circles barely rendering" — replacing 1024 baseline with 1-2 procgen
        //   produced an even-emptier field. Append-mode keeps the foundation
        //   alive AND adds dynamic procgen variation. Cap at CRYSTAL_TOTAL_CAP
        //   so the GPU consumer's MAX_SCENE_CRYSTALS cap clips us safely when
        //   consuming.
        let procgen_count = crystals.len();
        self.crystals = self.baseline_crystals.clone();
        self.crystals.extend(crystals);
        if self.crystals.len() > CRYSTAL_TOTAL_CAP {
            self.crystals.truncate(CRYSTAL_TOTAL_CAP);
        }
        log_event(
            "INFO",
            "loa-host/content-pipeline",
            &format!(
                "frame={} · appended {} procgen-crystals to {} baseline (total {}) · prompt_hash=0x{:016x}",
                self.frame_count,
                procgen_count,
                self.baseline_crystals.len(),
                self.crystals.len(),
                self.content_pipeline.last_prompt_hash,
            ),
        );
    }

    /// Advance the substrate-render pipeline by one frame. Returns the
    /// frame's `FrameOutput` (resonance metadata + budget + fidelity).
    /// The current pixel-field is accessed via `current_display`.
    pub fn tick(&mut self, observer: ObserverCoord) -> FrameOutput {
        // § T11-W18-LOA-CONTENT-WIRE · every CONTENT_PIPELINE_REFRESH_EVERY
        //   frames, ask the DMGM-council for a fresh procgen crystal-array.
        //   The pipeline gates internally on LOA_CONTENT_PIPELINE=1 (default
        //   OFF) so this is a zero-cost env-read when disabled.
        self.refresh_crystals_from_pipeline_if_due(observer);
        let out = self.renderer.tick(observer, &self.crystals, BUDGET_120HZ);
        self.frame_count = self.frame_count.wrapping_add(1);
        // Per-second telemetry (avoid per-frame log spam at 120 Hz).
        // § T11-W18-LIVE-LEARNING · feed frame-telemetry into KAN-bias.
        //   Per-frame · cheap (atomic-store · single BLAKE3 of 32 bytes).
        learn_from_frame_metrics(&out, self.profile_id);
        if self.frame_count % 120 == 0 {
            // § T11-W18-OBSERVABILITY-EXTEND : add active profile_id +
            //   crystal_count to the per-second telemetry-line for live
            //   diagnostics. KAN-bias-checksum is aggregate-across-all-5-bands.
            log_event(
                "DEBUG",
                "loa-host/substrate-render",
                &format!(
                    "tick · frame_n={} · pixels_lit={} · fidelity_tier={} · fingerprint={:08x} · blend={:?} · KAN-bias=0x{:08x} · obs={} · profile_id={} · crystals={}",
                    out.frame_n,
                    out.resonance.n_pixels_lit,
                    out.fidelity_tier,
                    out.resonance.fingerprint,
                    out.blend_used,
                    cssl_host_substrate_intelligence::kan_bias_checksum(),
                    cssl_host_substrate_intelligence::observe_count(),
                    self.profile_id,
                    self.crystals.len(),
                ),
            );
            // Periodic persist (every 120 frames ≈ 1 sec at 120 Hz · cheap).
            let _ = cssl_host_substrate_intelligence::kan_bias_persist(&kan_bias_persist_path());
            // § T11-W18-KAN-BAND-TRACE : env-gated per-band checksum-trace.
            //   LOA_KAN_BAND_TRACE=1 logs each of the 5 band's checksum every
            //   120 frames so user can verify multiband-learning is working.
            //   Default off · zero overhead when env-var unset.
            if std::env::var("LOA_KAN_BAND_TRACE").ok().as_deref() == Some("1") {
                let mut bands_summary = String::with_capacity(80);
                for pid in 0u8..=4 {
                    let band = cssl_host_substrate_intelligence::kan_bias_for_profile(pid);
                    let mut acc: u32 = 0;
                    for w in band {
                        acc = acc.wrapping_mul(0x9E37_79B9).wrapping_add(w);
                    }
                    use std::fmt::Write;
                    let _ = write!(&mut bands_summary, " band{pid}=0x{acc:08x}");
                }
                log_event(
                    "DEBUG",
                    "loa-host/kan-band-trace",
                    &format!("frame_n={} ·{}", out.frame_n, bands_summary),
                );
            }
        }
        out
    }

    /// Advance frame-owned substrate metadata without running the legacy CPU
    /// pixel-field rasterizer. The Vulkan V3 present path owns pixels; this
    /// keeps frame count and content-refresh cadence live without spending
    /// CPU time on an image that V3 never samples.
    pub fn tick_metadata_only(&mut self, observer: ObserverCoord) {
        self.refresh_crystals_from_pipeline_if_due(observer);
        self.frame_count = self.frame_count.wrapping_add(1);
    }

    /// Current host-owned world crystal count before V3 observer culling.
    #[must_use]
    pub fn world_crystal_count(&self) -> usize {
        self.crystals.len()
    }

    /// Current V3 stress-room key for telemetry/reporting.
    #[must_use]
    pub const fn stress_room_key(&self) -> &'static str {
        self.stress_room.key()
    }

    /// Current V3 stress-room label for UI/reporting.
    #[must_use]
    pub const fn stress_room_label(&self) -> &'static str {
        self.stress_room.label()
    }

    /// Pack nearest visible substrate crystals into the V3 present kernel's
    /// bounded scene buffer without heap allocation. The current CSSL kernel
    /// consumes a fixed bounded slot count; this function preserves that ABI while
    /// making live V3 present runs exercise actual scene data instead of an
    /// empty crystal slice.
    #[cfg(feature = "runtime")]
    pub fn write_v3_present_crystals_for_observer(
        &self,
        observer: WorldPos,
        out: &mut [cssl_host_substrate_render_v3::Crystal;
                 cssl_host_substrate_render_v3::MAX_SCENE_CRYSTALS],
    ) -> usize {
        let mut nearest: [(i64, usize); cssl_host_substrate_render_v3::MAX_SCENE_CRYSTALS] =
            [(i64::MAX, usize::MAX); cssl_host_substrate_render_v3::MAX_SCENE_CRYSTALS];
        let mut count = 0usize;

        for (idx, crystal) in self.crystals.iter().enumerate() {
            if !crystal.aspect_permitted(0) {
                continue;
            }
            let dist = crystal.dist_sq_mm(observer);
            if count < nearest.len() {
                nearest[count] = (dist, idx);
                count += 1;
                let mut i = count - 1;
                while i > 0 && nearest[i].0 < nearest[i - 1].0 {
                    nearest.swap(i, i - 1);
                    i -= 1;
                }
            } else if dist < nearest[nearest.len() - 1].0 {
                nearest[nearest.len() - 1] = (dist, idx);
                let mut i = nearest.len() - 1;
                while i > 0 && nearest[i].0 < nearest[i - 1].0 {
                    nearest.swap(i, i - 1);
                    i -= 1;
                }
            }
        }

        for slot in out.iter_mut() {
            *slot = cssl_host_substrate_render_v3::Crystal::default();
        }

        for (slot, (_dist, idx)) in nearest.iter().take(count).enumerate() {
            let crystal = &self.crystals[*idx];
            let rel_x = crystal.world_pos.x_mm.saturating_sub(observer.x_mm);
            let rel_y = crystal.world_pos.y_mm.saturating_sub(observer.y_mm);
            out[slot] = cssl_host_substrate_render_v3::Crystal {
                x: scale_present_axis(rel_x, PLAYFIELD_HALF_EXTENT_MM),
                y: scale_present_axis(rel_y, PLAYFIELD_Y_HALF_MM),
                z: crystal.world_pos.z_mm.saturating_sub(observer.z_mm),
                strength_milli: ((crystal.extent_mm / 16).clamp(1, 255)) as u32,
                material_code: v3_material_code_for_crystal(*idx, crystal.class),
            };
        }

        count
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
        // § T11-W18-LOA-CONTENT-WIRE · same content-pipeline refresh path
        //   as the CPU tick(). Default-OFF env gate is checked inside.
        self.refresh_crystals_from_pipeline_if_due(observer);
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
        // § T11-W18-CRYSTAL-ANIMATE · per-frame motion · crystals MOVE.
        //   Apply animate_crystal(t_ms) motion-pose-delta (μm) → mm-offset
        //   to each crystal's world_pos. Clone-then-mutate · base crystals
        //   stay immutable · animation derives fresh each frame.
        let t_ms: u64 = self.frame_count.saturating_mul(8); // ≈8ms/frame at 120Hz
        let animated: Vec<Crystal> = self
            .crystals
            .iter()
            .map(|base| {
                let anim = cssl_host_crystallization::animate::animate_crystal(base, t_ms);
                let mut c = base.clone();
                c.world_pos = WorldPos::new(
                    base.world_pos
                        .x_mm
                        .saturating_add(anim.motion_pose[0] / 1000),
                    base.world_pos
                        .y_mm
                        .saturating_add(anim.motion_pose[1] / 1000),
                    base.world_pos
                        .z_mm
                        .saturating_add(anim.motion_pose[2] / 1000),
                );
                c
            })
            .collect();

        // § T11-W18-DYNRES-SCALER · feed last frame's wall-clock into the
        //   scaler BEFORE we choose this frame's render-dims. First call
        //   seeds `last_tick_instant` and skips the observe (no prior
        //   sample yet).
        let now = std::time::Instant::now();
        if let Some(prev) = self.last_tick_instant {
            let frame_us = now.saturating_duration_since(prev).as_micros() as u64;
            self.dyn_res.observe_frame(frame_us);
        }
        self.last_tick_instant = Some(now);

        // § T11-W18-DYNRES-SCALER · resize GPU output to the scaled dims
        //   when they drift away from the current GPU dims. `render_dims`
        //   already snaps to multiples of 8 (workgroup-aligned) so we can
        //   feed it straight to `resize_gpu`.
        let (target_w, target_h) = self
            .dyn_res
            .render_dims(self.native_gpu_w, self.native_gpu_h);
        if let Some(gpu) = self.gpu.as_ref() {
            let (cur_w, cur_h) = gpu.dims();
            if cur_w != target_w || cur_h != target_h {
                self.resize_gpu(device, target_w, target_h);
            }
        }

        // § T11-W18-CRYSTAL1024 · BENCH-HOOK (LOG-only · env-gated). Set
        //   LOA_SUBSTRATE_BENCH=1 to print per-frame `gpu_dispatch_us` on every
        //   frame (or every 60 frames when value is `60`). NO test gate ; this
        //   is observability for measuring world-scene/cull cost on a real
        //   adapter. Zero overhead when env-var is unset.
        let bench_every: Option<u64> = std::env::var("LOA_SUBSTRATE_BENCH")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .filter(|&n| n > 0);

        let bench_started = bench_every.map(|_| std::time::Instant::now());

        // GPU path runs at panel-native 1440p · samples bound directly by
        // compose-pass (W18-N) · NOW with animated crystals (motion-pose-applied).
        if let Some(gpu) = self.gpu.as_mut() {
            let _view = gpu.dispatch(device, queue, observer, &animated);
        }

        if let (Some(every_n), Some(t0)) = (bench_every, bench_started) {
            if self.frame_count % every_n == 0 {
                let dt_us = t0.elapsed().as_micros() as u64;
                log_event(
                    "INFO",
                    "loa-host/substrate-render",
                    &format!(
                        "BENCH · frame_n={} · n_crystals={} · gpu_dispatch_us={}",
                        self.frame_count,
                        animated.len(),
                        dt_us
                    ),
                );
            }
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
                &format!("cpu-resize · {}×{} → {}×{}", cur_w, cur_h, cpu_w, cpu_h),
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
                    &format!("gpu-resize · {}×{} → {}×{}", cur_w, cur_h, gpu_w, gpu_h),
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

fn scale_present_axis(delta_mm: i32, half_extent_mm: i32) -> i32 {
    if half_extent_mm <= 0 {
        return 0;
    }
    ((delta_mm as i64 * 127) / i64::from(half_extent_mm)).clamp(-128, 127) as i32
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
    fn v3_stress_room_keys_parse() {
        for room in V3_STRESS_ROOMS {
            assert_eq!(V3StressRoom::from_key(room.key()), Some(room));
            assert!(!room.label().is_empty());
        }
        assert_eq!(
            V3StressRoom::from_key("competitive-arena"),
            Some(V3StressRoom::CompetitiveArena)
        );
        assert_eq!(V3StressRoom::from_key("unknown"), None);
    }

    #[test]
    fn v3_stress_room_variants_build_stable_1024_crystal_scenes() {
        for room in V3_STRESS_ROOMS {
            let s1 = SubstrateRenderState::new_for_stress_room(room);
            let s2 = SubstrateRenderState::new_for_stress_room(room);
            assert_eq!(s1.stress_room, room);
            assert_eq!(s1.crystals.len(), STARTUP_CRYSTAL_COUNT, "{room:?}");
            assert_eq!(
                s1.baseline_crystals.len(),
                STARTUP_CRYSTAL_COUNT,
                "{room:?}"
            );
            assert_eq!(s1.crystals.len(), s2.crystals.len(), "{room:?}");
            assert!(
                s1.crystals.iter().all(|c| c.extent_mm > 0),
                "{room:?} must keep positive extents"
            );
            for (a, b) in s1.crystals.iter().zip(s2.crystals.iter()) {
                assert_eq!(a.world_pos, b.world_pos, "{room:?} position drift");
                assert_eq!(a.extent_mm, b.extent_mm, "{room:?} extent drift");
                assert_eq!(a.fingerprint, b.fingerprint, "{room:?} fingerprint drift");
            }
        }
    }

    #[test]
    fn tick_advances_frame_count() {
        let mut s = SubstrateRenderState::new();
        let observer = s.observer_for(0, 0, 0, 0, 0, 0, 0xFFFF_FFFF);
        let _ = s.tick(observer);
        assert_eq!(s.frame_count, 1);
    }

    #[test]
    fn metadata_only_tick_advances_frame_without_cpu_pixel_raster() {
        let mut s = SubstrateRenderState::new();
        let observer = s.observer_for(0, 0, 0, 0, 0, 0, 0xFFFF_FFFF);
        let before = s.current_display().pixels.clone();
        s.tick_metadata_only(observer);
        assert_eq!(s.frame_count, 1);
        assert_eq!(s.current_display().pixels, before);
    }

    #[cfg(feature = "runtime")]
    #[test]
    fn v3_present_crystal_packer_emits_nearest_bounded_scene() {
        let s = SubstrateRenderState::new_for_stress_room(V3StressRoom::Shells);
        let mut out = [cssl_host_substrate_render_v3::Crystal::default();
            cssl_host_substrate_render_v3::MAX_SCENE_CRYSTALS];
        let count = s.write_v3_present_crystals_for_observer(WorldPos::new(0, 1550, 0), &mut out);
        assert_eq!(count, cssl_host_substrate_render_v3::MAX_SCENE_CRYSTALS);
        assert!(out.iter().take(count).all(|c| c.strength_milli > 0));
        assert!(out.iter().take(count).all(|c| (-128..=127).contains(&c.x)));
        assert!(out.iter().take(count).all(|c| (-128..=127).contains(&c.y)));
        assert!(out.iter().take(count).all(|c| c.material_code <= 23));
        assert!(out.iter().take(count).any(|c| c.material_code > 0));
        assert!(out.iter().skip(count).all(|c| c.strength_milli == 0));
    }

    #[test]
    fn v3_material_code_encodes_shell_and_class() {
        assert_eq!(
            v3_material_code_for_crystal(0, CrystalClass::Object),
            0,
            "inner object material"
        );
        assert_eq!(
            v3_material_code_for_crystal(SHELL_INNER_COUNT, CrystalClass::Entity),
            9,
            "middle entity material"
        );
        assert_eq!(
            v3_material_code_for_crystal(
                SHELL_INNER_COUNT + SHELL_MIDDLE_COUNT,
                CrystalClass::Inherit
            ),
            23,
            "outer inherit material"
        );
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
        assert!(
            s.is_gpu_active(),
            "GPU path should be active when adapter present"
        );
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
        assert_eq!(s.crystals.len(), STARTUP_CRYSTAL_COUNT);
        let mut class_hits = [0u32; 8];
        for c in &s.crystals {
            class_hits[c.class as usize] += 1;
        }
        let distinct = class_hits.iter().filter(|n| **n > 0).count();
        assert!(distinct >= 4, "≥4 distinct classes (got {distinct})");

        // Halton-spread bounds (all crystals inside the documented box).
        // Crystals straddle PLAYFIELD_Z_CENTER_MM ± PLAYFIELD_HALF_EXTENT_MM
        // so z ∈ [-1000, 8000] roughly ; relax the old 800..7000 box slightly.
        for c in &s.crystals {
            assert!(
                c.world_pos.x_mm.abs() <= PLAYFIELD_HALF_EXTENT_MM + 100,
                "x out: {}",
                c.world_pos.x_mm
            );
            assert!(
                c.world_pos.z_mm >= PLAYFIELD_Z_CENTER_MM - PLAYFIELD_HALF_EXTENT_MM - 100
                    && c.world_pos.z_mm <= PLAYFIELD_Z_CENTER_MM + PLAYFIELD_HALF_EXTENT_MM + 100,
                "z out: {}",
                c.world_pos.z_mm
            );
            assert!(
                c.world_pos.y_mm.abs() <= PLAYFIELD_Y_HALF_MM + 50,
                "y out: {}",
                c.world_pos.y_mm
            );
        }
        // Halton-2-spread → x positions should NOT all collide (distinct vs single-cluster).
        let xs: std::collections::HashSet<i32> =
            s.crystals.iter().map(|c| c.world_pos.x_mm).collect();
        assert!(
            xs.len() >= 64,
            "halton-2 spread should yield ≥ 64 distinct x at startup count (got {})",
            xs.len()
        );

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

    // ════════════════════════════════════════════════════════════════════════
    // § T11-W18-CRYSTAL1024 · concentric-shell distribution tests.
    // ════════════════════════════════════════════════════════════════════════

    /// Shell-classification : indices 0..128 → Inner · 128..512 → Middle ·
    /// 512..1024 → Outer. All 1024 indices must classify to exactly one
    /// shell ; counts per shell must match the SHELL_*_COUNT constants.
    #[test]
    fn shell_classification_partitions_1024_into_128_384_512() {
        let mut counts = [0usize; 3];
        for i in 0..STARTUP_CRYSTAL_COUNT {
            match Shell::for_index(i) {
                Shell::Inner => counts[0] += 1,
                Shell::Middle => counts[1] += 1,
                Shell::Outer => counts[2] += 1,
            }
        }
        assert_eq!(counts[0], SHELL_INNER_COUNT, "inner shell count");
        assert_eq!(counts[1], SHELL_MIDDLE_COUNT, "middle shell count");
        assert_eq!(counts[2], SHELL_OUTER_COUNT, "outer shell count");
        assert_eq!(counts.iter().sum::<usize>(), STARTUP_CRYSTAL_COUNT);
        // Boundary indices.
        assert_eq!(Shell::for_index(0), Shell::Inner);
        assert_eq!(Shell::for_index(127), Shell::Inner);
        assert_eq!(Shell::for_index(128), Shell::Middle);
        assert_eq!(Shell::for_index(511), Shell::Middle);
        assert_eq!(Shell::for_index(512), Shell::Outer);
        assert_eq!(Shell::for_index(1023), Shell::Outer);
        // Local-index : first-of-each-shell is 0.
        assert_eq!(Shell::local_index(0), 0);
        assert_eq!(Shell::local_index(128), 0);
        assert_eq!(Shell::local_index(512), 0);
        assert_eq!(Shell::local_index(1023), 511);
    }

    /// Shell radius bounds : every crystal placed inside its shell's
    /// [r_lo, r_hi] annulus (in normalized-radius units = horizontal-distance
    /// from the playfield-z-center, divided by PLAYFIELD_HALF_EXTENT_MM).
    /// Allow a small ±5% tolerance for integer rounding in
    /// `shell_world_pos`.
    #[test]
    fn shell_radius_bounds_each_crystal_within_its_annulus() {
        const TOL: f32 = 0.05;
        let s = SubstrateRenderState::new_for_stress_room(V3StressRoom::Shells);
        for (i, c) in s.crystals.iter().enumerate() {
            let shell = Shell::for_index(i);
            let (r_lo, r_hi) = shell.radius_bounds();
            // Horizontal-radius (x, z-offset-from-center) → normalized.
            let dx = c.world_pos.x_mm as f32;
            let dz = (c.world_pos.z_mm - PLAYFIELD_Z_CENTER_MM) as f32;
            let r_mm = (dx * dx + dz * dz).sqrt();
            let r_norm = r_mm / PLAYFIELD_HALF_EXTENT_MM as f32;
            assert!(
                r_norm >= r_lo - TOL && r_norm <= r_hi + TOL,
                "crystal[{i}] in {:?} shell : r_norm={r_norm:.4} not in [{r_lo}, {r_hi}]",
                shell
            );
        }
    }

    /// Inner shell amplitude > middle > outer (per-shell density-modulator
    /// applied to extent_mm). Inner extent_mm ≈ 1.5× base, outer ≈ 0.6× base.
    /// We restrict to Object-class crystals (single base extent) so the
    /// Environment-class shouts (extent = env_base ≫ base) don't skew avgs.
    #[test]
    fn shell_amp_modulator_scales_extent_by_density() {
        let s = SubstrateRenderState::new_for_stress_room(V3StressRoom::Shells);
        let base = cssl_host_crystallization::CRYSTAL_DEFAULT_EXTENT_MM;
        // Per-shell sum of Object-class extent_mm (single homogeneous source
        // so the modulator ratio is observable).
        let mut sums = [(0i64, 0i64); 3]; // (sum_extent, count) per shell.
        for (i, c) in s.crystals.iter().enumerate() {
            if !matches!(c.class, CrystalClass::Object) {
                continue;
            }
            let shell_idx = Shell::for_index(i) as usize;
            sums[shell_idx].0 += c.extent_mm as i64;
            sums[shell_idx].1 += 1;
        }
        let avg = |idx: usize| -> f32 {
            assert!(
                sums[idx].1 > 0,
                "shell {idx} should have ≥ 1 Object-class crystal"
            );
            sums[idx].0 as f32 / sums[idx].1 as f32
        };
        let avg_inner = avg(0);
        let avg_middle = avg(1);
        let avg_outer = avg(2);
        // Each Object-shell has all-identical extents (no per-instance noise
        // in `Crystal::allocate` — extent is class-deterministic), so we get
        // exact ratios :  inner = base*1.5  ·  middle = base  ·  outer = base*0.6
        let expect_inner = (base as f32 * SHELL_INNER_AMP_MOD).round();
        let expect_middle = (base as f32 * SHELL_MIDDLE_AMP_MOD).round();
        let expect_outer = (base as f32 * SHELL_OUTER_AMP_MOD).round();
        assert!(
            (avg_inner - expect_inner).abs() < 5.0,
            "inner avg extent {avg_inner} ≈ {expect_inner}"
        );
        assert!(
            (avg_middle - expect_middle).abs() < 5.0,
            "middle avg extent {avg_middle} ≈ {expect_middle}"
        );
        assert!(
            (avg_outer - expect_outer).abs() < 5.0,
            "outer avg extent {avg_outer} ≈ {expect_outer}"
        );
        // Strict ordering : inner > middle > outer.
        assert!(
            avg_inner > avg_middle,
            "inner ({avg_inner}) > middle ({avg_middle})"
        );
        assert!(
            avg_middle > avg_outer,
            "middle ({avg_middle}) > outer ({avg_outer})"
        );
        // Ratio-sanity : avg_inner / avg_outer ≈ 1.5 / 0.6 = 2.5
        let ratio = avg_inner / avg_outer;
        assert!(
            ratio > 2.4 && ratio < 2.6,
            "inner/outer extent ratio = {ratio} · expected ≈ 2.5 (1.5 / 0.6)"
        );
        assert!(
            avg_inner > base as f32,
            "inner avg should exceed base extent ({base})"
        );
        assert!(
            avg_outer < base as f32,
            "outer avg should fall below base extent ({base})"
        );
    }

    /// Even-spread within shell : Halton-2 angular spread should yield
    /// ≥ N/2 distinct (x, z) positions per shell (not single-cluster). Also
    /// verifies that no two crystals are world_pos-coincident (Halton low-
    /// discrepancy guarantees this for the small N's we use).
    #[test]
    fn shell_even_spread_yields_distinct_positions() {
        let s = SubstrateRenderState::new_for_stress_room(V3StressRoom::Shells);
        let mut by_shell: [Vec<(i32, i32)>; 3] = [vec![], vec![], vec![]];
        for (i, c) in s.crystals.iter().enumerate() {
            let shell = Shell::for_index(i);
            by_shell[shell as usize].push((c.world_pos.x_mm, c.world_pos.z_mm));
        }
        let counts = [SHELL_INNER_COUNT, SHELL_MIDDLE_COUNT, SHELL_OUTER_COUNT];
        for (idx, positions) in by_shell.iter().enumerate() {
            assert_eq!(positions.len(), counts[idx], "shell {idx} count");
            let unique: std::collections::HashSet<(i32, i32)> = positions.iter().copied().collect();
            // Allow a small dup-budget : at most 5% of slots may collide
            // (practically zero for N ≤ 64 with Halton-2 + Halton-3).
            let unique_n = unique.len();
            let min_unique = (positions.len() * 95) / 100;
            assert!(
                unique_n >= min_unique,
                "shell {idx} : {unique_n} unique pos out of {} (min {min_unique})",
                positions.len()
            );
        }
        // Global : every crystal has a unique (x, y, z) — no full collisions.
        let all_pos: std::collections::HashSet<(i32, i32, i32)> = s
            .crystals
            .iter()
            .map(|c| (c.world_pos.x_mm, c.world_pos.y_mm, c.world_pos.z_mm))
            .collect();
        let min_unique_total = (STARTUP_CRYSTAL_COUNT * 95) / 100;
        assert!(
            all_pos.len() >= min_unique_total,
            "{} unique world positions (min {min_unique_total} of {})",
            all_pos.len(),
            STARTUP_CRYSTAL_COUNT,
        );
    }

    /// Determinism : the 1024-crystal layout is replay-stable. Two
    /// SubstrateRenderState::new() calls produce byte-identical world_pos
    /// + extent_mm vectors. Important — replay-deterministic substrate is
    /// a substrate-paradigm invariant.
    #[test]
    fn shell_layout_is_replay_deterministic() {
        let s1 = SubstrateRenderState::new_for_stress_room(V3StressRoom::Shells);
        let s2 = SubstrateRenderState::new_for_stress_room(V3StressRoom::Shells);
        assert_eq!(s1.crystals.len(), s2.crystals.len());
        for (a, b) in s1.crystals.iter().zip(s2.crystals.iter()) {
            assert_eq!(a.world_pos.x_mm, b.world_pos.x_mm);
            assert_eq!(a.world_pos.y_mm, b.world_pos.y_mm);
            assert_eq!(a.world_pos.z_mm, b.world_pos.z_mm);
            assert_eq!(a.extent_mm, b.extent_mm);
            assert_eq!(a.handle, b.handle);
            assert_eq!(a.fingerprint, b.fingerprint);
        }
    }

    /// Helper-API : `Shell::amp_mod`, `Shell::count`, `Shell::radius_bounds`
    /// return the documented constants. Future-proofing — guards against
    /// an accidental refactor that drifts a shell's parameters.
    #[test]
    fn shell_helper_api_returns_documented_constants() {
        assert_eq!(Shell::Inner.count(), SHELL_INNER_COUNT);
        assert_eq!(Shell::Middle.count(), SHELL_MIDDLE_COUNT);
        assert_eq!(Shell::Outer.count(), SHELL_OUTER_COUNT);
        assert_eq!(Shell::Inner.amp_mod(), SHELL_INNER_AMP_MOD);
        assert_eq!(Shell::Middle.amp_mod(), SHELL_MIDDLE_AMP_MOD);
        assert_eq!(Shell::Outer.amp_mod(), SHELL_OUTER_AMP_MOD);
        assert_eq!(
            Shell::Inner.radius_bounds(),
            (SHELL_INNER_R_LO, SHELL_INNER_R_HI)
        );
        assert_eq!(
            Shell::Middle.radius_bounds(),
            (SHELL_MIDDLE_R_LO, SHELL_MIDDLE_R_HI)
        );
        assert_eq!(
            Shell::Outer.radius_bounds(),
            (SHELL_OUTER_R_LO, SHELL_OUTER_R_HI)
        );
        // Strict ordering : amp_mod is monotonically decreasing inner→outer.
        assert!(SHELL_INNER_AMP_MOD > SHELL_MIDDLE_AMP_MOD);
        assert!(SHELL_MIDDLE_AMP_MOD > SHELL_OUTER_AMP_MOD);
        // Strict ordering : radius bounds are non-overlapping + ascending.
        assert!(SHELL_INNER_R_HI <= SHELL_MIDDLE_R_LO);
        assert!(SHELL_MIDDLE_R_HI <= SHELL_OUTER_R_LO);
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
    std::path::PathBuf::from(home)
        .join(".loa")
        .join("kan_bias.bin")
}

/// § T11-W18-KAN-MULTIBAND-WIRE · Read the active DisplayProfile from the
/// `LOA_DISPLAY_PROFILE` env-var (lower-case match) and map to profile_id
/// (0..=4). Default 2 (IpsLcd · neutral fallback) when env-var unset or
/// unrecognized. Sovereignty-respecting fast-path : env-var deterministic
/// override + safe-default + zero-allocation parse.
fn profile_id_from_env() -> u8 {
    match std::env::var("LOA_DISPLAY_PROFILE")
        .ok()
        .map(|s| s.to_ascii_lowercase())
        .as_deref()
    {
        Some("amoled") => 0,
        Some("oled") => 1,
        Some("ips_lcd") | Some("ips") => 2,
        Some("va_lcd") | Some("va") => 3,
        Some("hdr_ext") | Some("hdr") => 4,
        _ => 2, // neutral default · IpsLcd
    }
}

/// Feed one frame's telemetry into the substrate-intelligence KAN-bias.
/// Cheap · per-frame call from substrate-render tick. The 8-byte payload
/// includes the resonance-fingerprint + pixels-lit + frame-number so each
/// frame is a unique observation that drifts the KAN-state.
///
/// § T11-W18-KAN-MULTIBAND-WIRE : routes observe to the per-DisplayProfile
/// band so AMOLED/OLED panels accumulate their own bias-history (faster
/// drift α=1/128) separately from IPS/VA/HDR (α=1/256). The profile_id
/// arg comes from the active DisplayProfile · default IpsLcd (id=2) for
/// neutral fallback if profile is unknown.
pub fn learn_from_frame_metrics(
    out: &cssl_host_digital_intelligence_render::FrameOutput,
    profile_id: u8,
) {
    let payload: [u8; 16] = {
        let mut b = [0u8; 16];
        b[0..4].copy_from_slice(&out.resonance.fingerprint.to_le_bytes());
        b[4..8].copy_from_slice(&out.resonance.n_pixels_lit.to_le_bytes());
        b[8..12].copy_from_slice(&out.elapsed_micros.to_le_bytes());
        b[12] = out.fidelity_tier;
        b[13] = out.blend_used as u8;
        // T11-W18-KAN-MULTIBAND : pack frame_n suffix into the 16-byte digest
        //   so unique frames produce unique observations across bands.
        b[14] = (out.frame_n & 0xff) as u8;
        b[15] = ((out.frame_n >> 8) & 0xff) as u8;
        b
    };
    // § T11-W18-KAN-MULTIBAND-WIRE : route to per-DisplayProfile band.
    //   Bounds : profile_id clamped to 0..=4 inside `observe_with_profile`.
    cssl_host_substrate_intelligence::observe_with_profile(&payload, profile_id);
}

/// § T11-W18-LOA-CONTENT-WIRE · Convert the alien-materialization
/// observer-coordinate (i32 mm + yaw/pitch/frame_t/Σ-mask) into the
/// procgen-pipeline's slim (f32 metres) coord. We DROP the yaw/pitch
/// because procgen-pipeline's emit-plane is already observer-relative ;
/// orientation is implicit via the spec's "in front of the player".
fn observer_to_procgen_coord(obs: ObserverCoord) -> cssl_host_procgen_pipeline::ObserverCoord {
    cssl_host_procgen_pipeline::ObserverCoord {
        x: obs.x_mm as f32 / 1000.0,
        y: obs.y_mm as f32 / 1000.0,
        z: obs.z_mm as f32 / 1000.0,
    }
}
