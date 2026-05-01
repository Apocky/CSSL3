//! § snapshot — programmatic visual-data capture (T11-LOA-TEST-APP).
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE IN BOOTSTRAP
//!   Stage-0 testing apparatus that lets Claude (and CI) gather visual data
//!   from LoA.exe automatically. Three capabilities :
//!     1. Capture current framebuffer → PNG file on disk
//!     2. Scripted camera-tour : visit N predefined poses, save PNG at each
//!     3. Golden-image diff : compare current render against reference PNGs
//!        with a tolerance-aware mean-absolute-error metric
//!
//! § WHY HERE
//!   Visual regression caught early = days of debugging saved. The wgpu
//!   framebuffer-readback path is the only way to get pixel-accurate
//!   visual evidence ; screenshots-via-OS are fragile (compositor effects,
//!   HDR mapping, scaling). This crate routes the readback through wgpu's
//!   own `copy_texture_to_buffer` so the pixels match what the player sees.
//!
//! § THREE-LAYER DESIGN
//!   - Tour-pose registry (pure-CPU) : compiles in catalog mode, exercised
//!     by inline tests without a GPU. Five tours : default · walls · floor ·
//!     plinths · ceiling.
//!   - Snapshot encoder (catalog) : `encode_png` accepts raw BGRA8 pixels
//!     + width + height + path, writes a PNG to disk via the `png` crate.
//!   - Framebuffer readback (runtime) : `Snapshotter::readback_to_png`
//!     creates a staging buffer sized for the current swap-chain, blits
//!     the just-presented texture into it, maps it, and hands the bytes
//!     to `encode_png`. Pollster::block_on bridges the async map.
//!
//! § GOLDEN-IMAGE DIFF
//!   Two same-sized PNGs are compared per-pixel in linear-RGB space with
//!   the mean absolute error (MAE) divided by 255 for normalization. A
//!   tour passes if every pose's MAE is below `GOLDEN_MAE_THRESHOLD`
//!   (default 0.02 = ~5/255 per channel average). The first run of a
//!   tour (no goldens on disk) writes the captured PNGs as the new
//!   reference and reports `passed: true · mae: 0.0` for each pose.
//!
//! § PRIME-DIRECTIVE
//!   No surveillance : snapshots only land on local disk under
//!   `logs/snapshots/` and `golden/`. No network upload. The `path`
//!   parameter is sanitized to refuse `..` traversal at the
//!   `render.snapshot_png` MCP boundary (sovereign-cap-gated).
//!
//! § ATTESTATION
//!   There was no hurt nor harm in the making of this, to anyone/anything/anybody.

#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]

use std::f32::consts::PI;
use std::path::{Path, PathBuf};

// ─────────────────────────────────────────────────────────────────────────
// § Tour-pose registry — pure-CPU, catalog-buildable
// ─────────────────────────────────────────────────────────────────────────

/// One entry in a scripted camera tour. Position is world-space (meters) ;
/// yaw/pitch are radians (matches the Camera convention in `camera.rs`).
#[derive(Debug, Clone, PartialEq)]
pub struct TourPose {
    /// Stable name used to derive the snapshot filename (`<name>.png`).
    pub name: String,
    /// World-space (x, y, z) for the camera eye.
    pub pos: [f32; 3],
    /// Yaw (radians) — rotation around +Y. 0 looks -Z (north).
    pub yaw: f32,
    /// Pitch (radians) — rotation around camera +X. Clamped to ±π/2 by host.
    pub pitch: f32,
}

impl TourPose {
    /// Convenience constructor with name from a `&str`.
    #[must_use]
    pub fn new(name: &str, pos: [f32; 3], yaw: f32, pitch: f32) -> Self {
        Self {
            name: name.to_string(),
            pos,
            yaw,
            pitch,
        }
    }
}

// ───── Tour : default ─────
//
// Five poses : 4 cardinal directions from the room center + one elevated
// corner overview. Eye-height 1.55m (matches `App::new()` default).

/// 5-pose default tour : center-N · center-S · center-E · center-W ·
/// NE-corner overview. Sees every wall + most plinths.
#[must_use]
pub fn tour_default() -> Vec<TourPose> {
    vec![
        TourPose::new("center_north", [0.0, 1.55, 0.0], PI, 0.0),
        TourPose::new("center_south", [0.0, 1.55, 0.0], 0.0, 0.0),
        TourPose::new("center_east", [0.0, 1.55, 0.0], PI / 2.0, 0.0),
        TourPose::new("center_west", [0.0, 1.55, 0.0], -PI / 2.0, 0.0),
        TourPose::new("ne_corner", [10.0, 1.55, 10.0], -3.0 * PI / 4.0, -0.2),
    ]
}

// ───── Tour : walls ─────
//
// One close-up per wall (north / south / east / west). Each pose stands
// 2m off the wall, eye-height 1.55m, looking straight at the wall.
//
// The test-room has 22m on each side (room_dim X=22, Z=22 from
// `RoomGeometry::test_room`) — with the floor centered at origin, walls
// sit at ±11m. We park 2m off so the wall fills the camera view.

/// 4-pose walls tour : one close-up per cardinal wall.
#[must_use]
pub fn tour_walls() -> Vec<TourPose> {
    vec![
        // North wall (z = +11) : look +Z
        TourPose::new("wall_north", [0.0, 1.55, 9.0], 0.0, 0.0),
        // South wall (z = -11) : look -Z
        TourPose::new("wall_south", [0.0, 1.55, -9.0], PI, 0.0),
        // East wall (x = +11) : look +X
        TourPose::new("wall_east", [9.0, 1.55, 0.0], PI / 2.0, 0.0),
        // West wall (x = -11) : look -X
        TourPose::new("wall_west", [-9.0, 1.55, 0.0], -PI / 2.0, 0.0),
    ]
}

// ───── Tour : floor ─────
//
// One pose looking straight down from a 5m-elevated overhead vantage at
// room center, plus 4 corner-quadrant overheads. Each quadrant pose hovers
// 5m above the quadrant's center, looking down. Total = 5 poses.

/// 5-pose floor tour : 1 center-overhead + 4 quadrant-corners overhead.
#[must_use]
pub fn tour_floor() -> Vec<TourPose> {
    let down = -PI / 2.0; // look straight down
    vec![
        TourPose::new("floor_center", [0.0, 5.0, 0.0], 0.0, down),
        TourPose::new("floor_ne", [5.0, 5.0, 5.0], 0.0, down),
        TourPose::new("floor_nw", [-5.0, 5.0, 5.0], 0.0, down),
        TourPose::new("floor_se", [5.0, 5.0, -5.0], 0.0, down),
        TourPose::new("floor_sw", [-5.0, 5.0, -5.0], 0.0, down),
    ]
}

// ───── Tour : plinths ─────
//
// 14 poses orbiting each plinth. Plinth-positions match the test-room
// scene's `plinth_positions()` ; for each plinth we place the camera 2m
// away on the +X side at eye-height, looking back toward the plinth
// center. The 14 plinth count matches `RoomGeometry::test_room().plinth_count`.

/// 14-pose plinth tour : 2m-orbit + eye-height look-at for each of the
/// 14 test-room plinths (matches `geometry::plinth_positions()`).
#[must_use]
pub fn tour_plinths() -> Vec<TourPose> {
    // Hard-coded plinth positions matching `geometry::plinth_positions()`
    // to avoid pulling the geometry dep into the catalog-only test path.
    // If the geometry's plinth count or layout changes, update both lists.
    let plinths: [(f32, f32); 14] = [
        (-2.0, -2.0),
        (2.0, -2.0),
        (-2.0, 2.0),
        (2.0, 2.0),
        (-4.0, 0.0),
        (4.0, 0.0),
        (0.0, -4.0),
        (0.0, 4.0),
        (-6.0, -6.0),
        (6.0, -6.0),
        (-6.0, 6.0),
        (6.0, 6.0),
        (-8.0, 0.0),
        (8.0, 0.0),
    ];
    let mut out = Vec::with_capacity(plinths.len());
    for (i, &(px, pz)) in plinths.iter().enumerate() {
        // Stand 2m east of the plinth (camera at px+2, look -X back at it)
        let cam_x = px + 2.0;
        let cam_z = pz;
        out.push(TourPose::new(
            &format!("plinth_{i:02}"),
            [cam_x, 1.55, cam_z],
            -PI / 2.0,
            0.0,
        ));
    }
    out
}

// ───── Tour : ceiling ─────
//
// Single pose looking straight up from room center.

/// 1-pose ceiling tour : look-up from room center.
#[must_use]
pub fn tour_ceiling() -> Vec<TourPose> {
    vec![TourPose::new(
        "ceiling_up",
        [0.0, 1.55, 0.0],
        0.0,
        PI / 2.0 - 0.05, // just shy of straight-up to avoid gimbal pole
    )]
}

/// Resolve a tour-id string to its pose list. Returns `None` for unknown ids.
#[must_use]
pub fn tour_by_id(id: &str) -> Option<Vec<TourPose>> {
    match id {
        "default" => Some(tour_default()),
        "walls" => Some(tour_walls()),
        "floor" => Some(tour_floor()),
        "plinths" => Some(tour_plinths()),
        "ceiling" => Some(tour_ceiling()),
        _ => None,
    }
}

/// All registered tour-ids. Used by the MCP `render.tour` tool to validate
/// + by `render.diff_golden` to walk known tours.
pub const TOUR_IDS: &[&str] = &["default", "walls", "floor", "plinths", "ceiling"];

// ─────────────────────────────────────────────────────────────────────────
// § Golden-image diff — pixel-MAE in linear-RGB, threshold-pass/fail
// ─────────────────────────────────────────────────────────────────────────

/// Default tolerance for the mean-absolute-error pass-gate. 0.02 ≈ 5/255
/// average per-channel difference — passes through font-rendering jitter
/// + GPU-rounding noise but catches actual rendering regressions.
pub const GOLDEN_MAE_THRESHOLD: f32 = 0.02;

/// Result of comparing one pose against its golden reference.
#[derive(Debug, Clone, PartialEq)]
pub struct GoldenDiffEntry {
    pub pose: String,
    pub mae: f32,
    pub threshold: f32,
    pub passed: bool,
    /// True if the golden file did not exist and the captured PNG was
    /// promoted to be the new golden.
    pub created_new: bool,
}

/// Aggregate result of a full tour comparison.
#[derive(Debug, Clone, PartialEq)]
pub struct GoldenDiffReport {
    pub tour_id: String,
    pub passed: bool,
    pub per_pose: Vec<GoldenDiffEntry>,
}

/// Compute mean-absolute-error in linear [0..1] RGB between two
/// equally-sized BGRA8 byte buffers. Returns `None` on size mismatch.
///
/// The byte order is the wgpu surface's BGRA8Unorm — index 0 is blue,
/// 1 is green, 2 is red, 3 is alpha. We diff R/G/B only ; alpha is
/// always 1.0 (opaque) for a presented framebuffer and including it
/// would falsely lower the mean.
#[must_use]
pub fn mae_bgra8(lhs: &[u8], rhs: &[u8]) -> Option<f32> {
    if lhs.len() != rhs.len() {
        return None;
    }
    if lhs.is_empty() {
        return Some(0.0);
    }
    let mut sum: f64 = 0.0;
    let mut n: u64 = 0;
    let mut i = 0;
    while i + 3 < lhs.len() {
        // Channels 0=B, 1=G, 2=R (BGRA8).
        for c in 0..3 {
            let a = f64::from(lhs[i + c]);
            let b = f64::from(rhs[i + c]);
            sum += (a - b).abs();
            n += 1;
        }
        i += 4;
    }
    if n == 0 {
        return Some(0.0);
    }
    Some((sum / (n as f64) / 255.0) as f32)
}

// ─────────────────────────────────────────────────────────────────────────
// § PNG encode — catalog-buildable wrapper around the `png` crate
// ─────────────────────────────────────────────────────────────────────────

/// Convert in-place a BGRA8 byte buffer to RGBA8 byte order (the PNG
/// spec uses RGBA, but wgpu surface readback gives us BGRA8). Mutates
/// the buffer ; returns the same length it received.
pub fn bgra8_to_rgba8_inplace(buf: &mut [u8]) {
    let mut i = 0;
    while i + 3 < buf.len() {
        buf.swap(i, i + 2); // swap B (index 0) with R (index 2)
        i += 4;
    }
}

/// Encode an RGBA8 byte buffer as a PNG file at `path`. The buffer must
/// be `width * height * 4` bytes long. Creates parent directories if
/// necessary. Returns the number of bytes written on success.
///
/// § ERRORS
///   - `io::Error::InvalidInput` if buffer size mismatches w·h·4
///   - I/O errors from PNG writer / file create / mkdir
pub fn encode_png(
    rgba: &[u8],
    width: u32,
    height: u32,
    path: &Path,
) -> std::io::Result<u64> {
    let expected = (width as usize) * (height as usize) * 4;
    if rgba.len() != expected {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!(
                "buffer size {} does not match {}x{}x4 = {}",
                rgba.len(),
                width,
                height,
                expected
            ),
        ));
    }
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }

    let file = std::fs::File::create(path)?;
    let w = std::io::BufWriter::new(file);
    let mut encoder = png::Encoder::new(w, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder
        .write_header()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("png header: {e}")))?;
    writer
        .write_image_data(rgba)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("png write: {e}")))?;
    drop(writer); // flush PNG

    let bytes_written = std::fs::metadata(path)?.len();
    Ok(bytes_written)
}

/// Read an RGBA8 PNG from disk. Returns `(rgba, width, height)`.
pub fn decode_png(path: &Path) -> std::io::Result<(Vec<u8>, u32, u32)> {
    let file = std::fs::File::open(path)?;
    let r = std::io::BufReader::new(file);
    let decoder = png::Decoder::new(r);
    let mut reader = decoder
        .read_info()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("png header: {e}")))?;
    let info = reader.info().clone();
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let frame = reader
        .next_frame(&mut buf)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("png frame: {e}")))?;
    buf.truncate(frame.buffer_size());

    // Normalize to RGBA8 — accept RGB8 by padding alpha=255.
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => {
            let mut out = Vec::with_capacity((info.width * info.height * 4) as usize);
            for chunk in buf.chunks_exact(3) {
                out.extend_from_slice(chunk);
                out.push(255);
            }
            out
        }
        other => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("unsupported PNG color type: {other:?}"),
            ));
        }
    };

    Ok((rgba, info.width, info.height))
}

/// Convert RGBA8 to BGRA8 (in-place, channel swap).
pub fn rgba8_to_bgra8_inplace(buf: &mut [u8]) {
    let mut i = 0;
    while i + 3 < buf.len() {
        buf.swap(i, i + 2);
        i += 4;
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § Path safety — refuse traversal in user-supplied filenames
// ─────────────────────────────────────────────────────────────────────────

/// Validate a user-supplied snapshot path. Refuses :
///   - absolute paths (must be relative)
///   - paths containing `..` components (no traversal)
///   - empty paths
/// On success returns the canonicalized PathBuf rooted at `base_dir`.
///
/// § PRIME-DIRECTIVE
///   `render.snapshot_png` is sovereign-cap-gated already, but defense-in-
///   depth refuses traversal so a misuse of the cap can't write outside
///   the snapshot tree. Same posture as `cssl-fs-policy`.
#[must_use]
pub fn sanitize_snapshot_path(base_dir: &Path, user_path: &str) -> Option<PathBuf> {
    let user = Path::new(user_path);
    if user.as_os_str().is_empty() {
        return None;
    }
    if user.is_absolute() {
        return None;
    }
    for c in user.components() {
        if matches!(c, std::path::Component::ParentDir) {
            return None;
        }
    }
    Some(base_dir.join(user))
}

/// Default snapshot output directory : `<cwd>/logs/snapshots`.
#[must_use]
pub fn default_snapshot_dir() -> PathBuf {
    PathBuf::from("logs/snapshots")
}

/// Default golden-image directory : `<cwd>/golden`.
#[must_use]
pub fn default_golden_dir() -> PathBuf {
    PathBuf::from("golden")
}

/// § T11-LOA-USERFIX : default video output directory : `<cwd>/logs/video`.
#[must_use]
pub fn default_video_dir() -> PathBuf {
    PathBuf::from("logs/video")
}

// ─────────────────────────────────────────────────────────────────────────
// § T11-LOA-USERFIX — Burst + Video state machines (catalog-buildable)
// ─────────────────────────────────────────────────────────────────────────
//
// Both states are pure-CPU + held in EngineState ; the render loop reads
// them each frame and queues snapshot requests accordingly. Test surface
// covers : start → frame N → reset · toggle on/off · path generation.

/// § T11-LOA-USERFIX : current burst-capture state.
///
/// When a burst is active, the host queues one snapshot per render frame
/// until `frames_remaining` reaches zero. Burst ID is monotonic so the
/// output directory is unique even across multiple bursts in one session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BurstState {
    /// True while a burst is in progress.
    pub active: bool,
    /// Frames captured so far in the current burst.
    pub frames_captured: u32,
    /// Frames remaining in the current burst (decremented each tick).
    pub frames_remaining: u32,
    /// Frame-stride : capture every Nth frame (N=1 = every frame).
    pub frame_stride: u32,
    /// Tick within the current stride (counts up to frame_stride-1, then
    /// triggers a capture and wraps to 0).
    pub stride_tick: u32,
    /// Output directory for the in-flight burst (e.g. `logs/snapshots/burst_<ts>`).
    pub output_dir: PathBuf,
    /// Monotonic burst id (0 = first burst this session).
    pub burst_id: u32,
}

impl Default for BurstState {
    fn default() -> Self {
        Self {
            active: false,
            frames_captured: 0,
            frames_remaining: 0,
            frame_stride: 1,
            stride_tick: 0,
            output_dir: PathBuf::new(),
            burst_id: 0,
        }
    }
}

impl BurstState {
    /// Start a new burst capturing `count` frames at `frame_stride` (every Nth
    /// frame). The output directory is named `burst_<burst_id>` rooted at
    /// `default_snapshot_dir()`. Returns the path the burst will write to.
    #[must_use]
    pub fn start_burst(&mut self, count: u32, frame_stride: u32) -> PathBuf {
        let stride = frame_stride.max(1);
        let dir = default_snapshot_dir().join(format!("burst_{:04}", self.burst_id));
        self.active = true;
        self.frames_captured = 0;
        self.frames_remaining = count;
        self.frame_stride = stride;
        self.stride_tick = 0;
        self.output_dir = dir.clone();
        self.burst_id = self.burst_id.wrapping_add(1);
        dir
    }

    /// Return the next per-frame capture path if the host should capture
    /// THIS frame (i.e. burst active + stride hit). Side-effects : decrements
    /// frames_remaining + increments frames_captured + advances stride_tick.
    /// Returns `None` if no capture is needed this tick.
    pub fn tick_capture_path(&mut self) -> Option<PathBuf> {
        if !self.active {
            return None;
        }
        // Stride : only capture when stride_tick hits 0.
        if self.stride_tick != 0 {
            self.stride_tick -= 1;
            return None;
        }
        // Capture this frame.
        let frame_idx = self.frames_captured;
        self.frames_captured += 1;
        self.frames_remaining = self.frames_remaining.saturating_sub(1);
        self.stride_tick = self.frame_stride.saturating_sub(1);
        if self.frames_remaining == 0 {
            self.active = false;
        }
        Some(
            self.output_dir
                .join(format!("frame_{frame_idx:02}.png")),
        )
    }
}

/// § T11-LOA-USERFIX : video-recorder state machine.
///
/// Each frame written goes to `<output_dir>/frame_NNNN.png` ; after F8
/// stops the record, the host emits a structured-event log with the total
/// frame count + duration so the user can ffmpeg later. No on-disk
/// transcoding here — keeping it stage-0 simple.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoState {
    /// True while video record is active.
    pub recording: bool,
    /// Frames captured in the in-flight session.
    pub frames_captured: u32,
    /// Output directory for the current video session.
    pub output_dir: PathBuf,
    /// Frame-stride : capture every Nth frame (N=1 = every frame).
    pub frame_stride: u32,
    /// Tick within the current stride.
    pub stride_tick: u32,
    /// Monotonic video session id.
    pub video_id: u32,
    /// Wall-clock unix-ms when the current session started (for duration).
    pub started_unix_ms: u64,
}

impl Default for VideoState {
    fn default() -> Self {
        Self {
            recording: false,
            frames_captured: 0,
            output_dir: PathBuf::new(),
            frame_stride: 1,
            stride_tick: 0,
            video_id: 0,
            started_unix_ms: 0,
        }
    }
}

impl VideoState {
    /// Start a new video session. Returns the directory path. Idempotent —
    /// if already recording, returns the current path.
    pub fn start_record(&mut self, frame_stride: u32, now_unix_ms: u64) -> PathBuf {
        if self.recording {
            return self.output_dir.clone();
        }
        let dir = default_video_dir().join(format!("video_{:04}", self.video_id));
        self.recording = true;
        self.frames_captured = 0;
        self.output_dir = dir.clone();
        self.frame_stride = frame_stride.max(1);
        self.stride_tick = 0;
        self.started_unix_ms = now_unix_ms;
        self.video_id = self.video_id.wrapping_add(1);
        dir
    }

    /// Stop the current video session. Returns `(frames, duration_ms)` if a
    /// session was active, or `None` if not. Idempotent.
    pub fn stop_record(&mut self, now_unix_ms: u64) -> Option<(u32, u64)> {
        if !self.recording {
            return None;
        }
        let frames = self.frames_captured;
        let duration = now_unix_ms.saturating_sub(self.started_unix_ms);
        self.recording = false;
        Some((frames, duration))
    }

    /// Toggle the record state. Returns the resulting bool (true = recording
    /// started, false = recording stopped).
    pub fn toggle(&mut self, frame_stride: u32, now_unix_ms: u64) -> bool {
        if self.recording {
            self.stop_record(now_unix_ms);
            false
        } else {
            self.start_record(frame_stride, now_unix_ms);
            true
        }
    }

    /// Return the next per-frame capture path if the host should capture this
    /// frame, or `None` if not recording / stride miss.
    pub fn tick_capture_path(&mut self) -> Option<PathBuf> {
        if !self.recording {
            return None;
        }
        if self.stride_tick != 0 {
            self.stride_tick -= 1;
            return None;
        }
        let idx = self.frames_captured;
        self.frames_captured += 1;
        self.stride_tick = self.frame_stride.saturating_sub(1);
        Some(
            self.output_dir
                .join(format!("frame_{idx:04}.png")),
        )
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § Runtime-only : framebuffer readback via wgpu
// ─────────────────────────────────────────────────────────────────────────
//
// The `Snapshotter` owns a CPU-side staging buffer sized for the current
// swap-chain. Each readback :
//   1. Recreate the staging buffer if dimensions changed
//   2. Encode a `copy_texture_to_buffer` from a renderable source
//      texture (we maintain our own RGBA8 staging texture in the renderer
//      and blit the swap-chain into it via a render pass — the swap-chain
//      texture itself isn't COPY_SRC on most adapters)
//   3. Map-async + pollster-block on the buffer
//   4. Slice → Vec<u8>, convert BGRA→RGBA, pass to encode_png

#[cfg(feature = "runtime")]
pub use runtime::Snapshotter;

#[cfg(feature = "runtime")]
mod runtime {
    use super::{bgra8_to_rgba8_inplace, encode_png};
    use std::path::Path;

    /// Bytes per row for a copy_texture_to_buffer must be a multiple of
    /// `wgpu::COPY_BYTES_PER_ROW_ALIGNMENT` (256). Round up width*4.
    fn aligned_bytes_per_row(width: u32) -> u32 {
        let unaligned = width * 4;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        unaligned.div_ceil(align) * align
    }

    /// Owns the CPU staging buffer + dimensions for framebuffer readback.
    pub struct Snapshotter {
        width: u32,
        height: u32,
        bytes_per_row: u32,
        staging_buf: Option<wgpu::Buffer>,
    }

    impl Snapshotter {
        /// Construct an empty snapshotter ; staging buffer is lazily
        /// allocated on first `readback_to_png`.
        #[must_use]
        pub fn new() -> Self {
            Self {
                width: 0,
                height: 0,
                bytes_per_row: 0,
                staging_buf: None,
            }
        }

        /// Ensure the staging buffer matches the current dimensions.
        /// Recreates if width/height changed.
        fn ensure_buffer(&mut self, device: &wgpu::Device, width: u32, height: u32) {
            if width == self.width && height == self.height && self.staging_buf.is_some() {
                return;
            }
            let bytes_per_row = aligned_bytes_per_row(width);
            let total = u64::from(bytes_per_row) * u64::from(height);
            self.staging_buf = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("loa-host/snapshot-staging"),
                size: total,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            }));
            self.width = width;
            self.height = height;
            self.bytes_per_row = bytes_per_row;
        }

        /// Copy a source RGBA8 / BGRA8 wgpu texture → staging buffer →
        /// CPU bytes → PNG file at `out_path`. The source texture must
        /// be COPY_SRC + RGBA8Unorm or BGRA8Unorm.
        ///
        /// Returns bytes written to disk on success.
        pub fn readback_to_png(
            &mut self,
            device: &wgpu::Device,
            queue: &wgpu::Queue,
            source: &wgpu::Texture,
            out_path: &Path,
        ) -> std::io::Result<u64> {
            let size = source.size();
            let width = size.width;
            let height = size.height;
            self.ensure_buffer(device, width, height);

            let staging = self
                .staging_buf
                .as_ref()
                .expect("staging buffer ensured above");

            // Encode the copy
            let mut encoder =
                device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("loa-host/snapshot-copy-encoder"),
                });
            encoder.copy_texture_to_buffer(
                wgpu::ImageCopyTexture {
                    texture: source,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::ImageCopyBuffer {
                    buffer: staging,
                    layout: wgpu::ImageDataLayout {
                        offset: 0,
                        bytes_per_row: Some(self.bytes_per_row),
                        rows_per_image: Some(height),
                    },
                },
                wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
            );
            queue.submit(Some(encoder.finish()));

            // Map + block on completion
            let slice = staging.slice(..);
            let (tx, rx) = std::sync::mpsc::sync_channel(1);
            slice.map_async(wgpu::MapMode::Read, move |result| {
                let _ = tx.send(result);
            });
            device.poll(wgpu::Maintain::Wait);
            match rx.recv() {
                Ok(Ok(())) => {}
                Ok(Err(e)) => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("buffer-map error: {e:?}"),
                    ));
                }
                Err(e) => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("buffer-map channel error: {e:?}"),
                    ));
                }
            }

            // Read mapped bytes, strip per-row padding, swap BGRA→RGBA
            let mapped = slice.get_mapped_range();
            let mut tight = Vec::with_capacity((width * height * 4) as usize);
            let row_bytes = (width * 4) as usize;
            for y in 0..height {
                let row_start = (y as usize) * (self.bytes_per_row as usize);
                tight.extend_from_slice(&mapped[row_start..row_start + row_bytes]);
            }
            drop(mapped);
            staging.unmap();

            // wgpu surface formats are BGRA8Unorm-Srgb on most desktops ;
            // PNG expects RGBA byte order regardless of color-space, so
            // swap channels in-place.
            // (For RGBA8 sources the swap is a no-op for an opaque image
            // — the colors look "wrong" but the diff still compares byte
            // identical; we standardize on BGRA-from-surface here.)
            bgra8_to_rgba8_inplace(&mut tight);

            encode_png(&tight, width, height, out_path)
        }

        /// Drop the staging buffer (called when surface is being torn down).
        pub fn release(&mut self) {
            self.staging_buf = None;
            self.width = 0;
            self.height = 0;
            self.bytes_per_row = 0;
        }
    }

    impl Default for Snapshotter {
        fn default() -> Self {
            Self::new()
        }
    }

    // ─────────────────────────────────────────────────────────────────────
    // § runtime-only tests for the alignment helper
    // ─────────────────────────────────────────────────────────────────────

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn aligned_bytes_per_row_rounds_up() {
            // 1 px (4 B) rounds up to 256
            assert_eq!(aligned_bytes_per_row(1), 256);
            // 64 px (256 B) is already aligned
            assert_eq!(aligned_bytes_per_row(64), 256);
            // 65 px (260 B) rounds up to 512
            assert_eq!(aligned_bytes_per_row(65), 512);
            // 1280 px (5120 B = 20 * 256) is exact
            assert_eq!(aligned_bytes_per_row(1280), 5120);
        }
    }
}

// ═════════════════════════════════════════════════════════════════════════
// § TESTS — pure-CPU, catalog-only (no GPU dependency)
// ═════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tour_default_has_5_poses() {
        let t = tour_default();
        assert_eq!(t.len(), 5);
        // Every pose has a unique name.
        let mut names: Vec<&str> = t.iter().map(|p| p.name.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), 5);
    }

    #[test]
    fn tour_walls_has_4_poses() {
        let t = tour_walls();
        assert_eq!(t.len(), 4);
        for p in &t {
            assert!(p.name.starts_with("wall_"));
        }
    }

    #[test]
    fn tour_floor_has_5_poses() {
        let t = tour_floor();
        assert_eq!(t.len(), 5);
        // All floor poses look down (pitch ≈ -π/2)
        for p in &t {
            assert!((p.pitch - (-PI / 2.0)).abs() < 1e-5);
        }
    }

    #[test]
    fn tour_plinths_has_14_poses() {
        let t = tour_plinths();
        assert_eq!(t.len(), 14);
        // Names are zero-padded sequential
        for (i, p) in t.iter().enumerate() {
            assert_eq!(p.name, format!("plinth_{i:02}"));
        }
    }

    #[test]
    fn tour_ceiling_has_1_pose() {
        let t = tour_ceiling();
        assert_eq!(t.len(), 1);
        assert!(t[0].pitch > 0.0); // looking up
    }

    #[test]
    fn snapshot_pose_serializes_correctly() {
        let p = TourPose::new("test_pose", [1.0, 2.0, 3.0], 0.5, -0.25);
        assert_eq!(p.name, "test_pose");
        assert_eq!(p.pos, [1.0, 2.0, 3.0]);
        assert!((p.yaw - 0.5).abs() < 1e-6);
        assert!((p.pitch - (-0.25)).abs() < 1e-6);
    }

    #[test]
    fn tour_pose_yaw_pitch_in_valid_range() {
        // Each tour pose's yaw is in (-2π, 2π) and pitch is in (-π, π).
        for tour_id in TOUR_IDS {
            let poses = tour_by_id(tour_id).expect("tour exists");
            for p in &poses {
                assert!(
                    p.yaw.abs() < 2.0 * PI + 1e-3,
                    "yaw out of range in {} pose {}: {}",
                    tour_id,
                    p.name,
                    p.yaw
                );
                assert!(
                    p.pitch.abs() < PI + 1e-3,
                    "pitch out of range in {} pose {}: {}",
                    tour_id,
                    p.name,
                    p.pitch
                );
            }
        }
    }

    #[test]
    fn golden_diff_mae_threshold_default_is_0_02() {
        // Spec says 0.02. If this changes, the integration tests
        // (and the MCP tool's threshold field) must update in lockstep.
        assert!((GOLDEN_MAE_THRESHOLD - 0.02).abs() < 1e-6);
    }

    #[test]
    fn tour_by_id_resolves_known_tours() {
        for id in TOUR_IDS {
            assert!(tour_by_id(id).is_some(), "{id} should resolve");
        }
        assert!(tour_by_id("does_not_exist").is_none());
    }

    #[test]
    fn tour_by_id_returns_same_count_as_direct_call() {
        assert_eq!(tour_by_id("default").unwrap().len(), tour_default().len());
        assert_eq!(tour_by_id("walls").unwrap().len(), tour_walls().len());
        assert_eq!(tour_by_id("floor").unwrap().len(), tour_floor().len());
        assert_eq!(tour_by_id("plinths").unwrap().len(), tour_plinths().len());
        assert_eq!(tour_by_id("ceiling").unwrap().len(), tour_ceiling().len());
    }

    #[test]
    fn mae_bgra8_identical_buffers_returns_0() {
        // 2x2 image, BGRA8 = 16 bytes
        let buf = vec![0x80u8; 16];
        let mae = mae_bgra8(&buf, &buf).unwrap();
        assert!(mae.abs() < 1e-6);
    }

    #[test]
    fn mae_bgra8_max_difference_returns_1() {
        // Black vs white BGRA8 ; alpha matches so RGB diff is full 255.
        // 2x2 image
        let mut black = vec![0u8; 16];
        for i in 0..4 {
            black[i * 4 + 3] = 255;
        }
        let mut white = vec![255u8; 16];
        // Don't touch alpha (already 255 in both)
        for i in 0..4 {
            white[i * 4 + 3] = 255;
        }
        let mae = mae_bgra8(&black, &white).unwrap();
        // Mean absolute error should be 1.0 (255/255) since every channel
        // is fully different.
        assert!((mae - 1.0).abs() < 1e-5, "got mae = {mae}");
    }

    #[test]
    fn mae_bgra8_size_mismatch_returns_none() {
        let a = vec![0u8; 16];
        let b = vec![0u8; 24];
        assert!(mae_bgra8(&a, &b).is_none());
    }

    #[test]
    fn mae_bgra8_excludes_alpha_channel() {
        // Two buffers identical in RGB but different in alpha.
        // mae_bgra8 should return 0 (we don't compare alpha).
        let mut a = vec![0x80u8; 16];
        let mut b = vec![0x80u8; 16];
        for i in 0..4 {
            a[i * 4 + 3] = 0; // alpha 0
            b[i * 4 + 3] = 255; // alpha full
        }
        let mae = mae_bgra8(&a, &b).unwrap();
        assert!(mae.abs() < 1e-6, "alpha must be excluded; got {mae}");
    }

    #[test]
    fn bgra_to_rgba_swaps_channels_only() {
        // 1px BGRA = (0xAA, 0xBB, 0xCC, 0xDD) → RGBA = (0xCC, 0xBB, 0xAA, 0xDD)
        let mut buf = vec![0xAA, 0xBB, 0xCC, 0xDD];
        bgra8_to_rgba8_inplace(&mut buf);
        assert_eq!(buf, vec![0xCC, 0xBB, 0xAA, 0xDD]);
    }

    #[test]
    fn rgba_to_bgra_round_trips_with_bgra_to_rgba() {
        let mut buf = vec![0x10, 0x20, 0x30, 0xFF, 0x40, 0x50, 0x60, 0xFF];
        let original = buf.clone();
        bgra8_to_rgba8_inplace(&mut buf);
        rgba8_to_bgra8_inplace(&mut buf);
        assert_eq!(buf, original);
    }

    #[test]
    fn sanitize_snapshot_path_rejects_absolute_paths() {
        let base = Path::new("logs/snapshots");
        // Build a platform-correct absolute path so the test is hermetic
        // across Windows + Linux + macOS.
        let abs = std::env::temp_dir().join("a_real_abs_path");
        let abs_str = abs.to_string_lossy().to_string();
        assert!(
            sanitize_snapshot_path(base, &abs_str).is_none(),
            "absolute path '{abs_str}' should be rejected"
        );
    }

    #[test]
    fn sanitize_snapshot_path_rejects_traversal() {
        let base = Path::new("logs/snapshots");
        assert!(sanitize_snapshot_path(base, "../etc/passwd").is_none());
        assert!(sanitize_snapshot_path(base, "subdir/../../escape").is_none());
        // Empty also rejected.
        assert!(sanitize_snapshot_path(base, "").is_none());
    }

    #[test]
    fn sanitize_snapshot_path_accepts_relative() {
        let base = Path::new("logs/snapshots");
        let p = sanitize_snapshot_path(base, "snap_001.png").unwrap();
        assert!(p.ends_with("snap_001.png"));
    }

    #[test]
    fn default_dirs_are_relative() {
        assert!(default_snapshot_dir().is_relative());
        assert!(default_golden_dir().is_relative());
    }

    // ─── PNG round-trip with a tiny synthetic image ───
    #[test]
    fn encode_decode_png_round_trip() {
        let dir = std::env::temp_dir().join("loa-snapshot-test");
        let _ = std::fs::create_dir_all(&dir);
        let p = dir.join("rt.png");

        // Build a 4x2 RGBA image.
        let w: u32 = 4;
        let h: u32 = 2;
        let mut rgba = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                rgba.push(((x * 60) & 0xFF) as u8); // R
                rgba.push(((y * 120) & 0xFF) as u8); // G
                rgba.push(((x + y) * 30 & 0xFF) as u8); // B
                rgba.push(255); // A
            }
        }
        let bytes = encode_png(&rgba, w, h, &p).expect("encode ok");
        assert!(bytes > 0);
        let (rgba2, w2, h2) = decode_png(&p).expect("decode ok");
        assert_eq!(w2, w);
        assert_eq!(h2, h);
        assert_eq!(rgba2, rgba);

        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn encode_png_size_mismatch_returns_invalid_input() {
        let dir = std::env::temp_dir();
        let p = dir.join("loa-bad-size.png");
        let bad = vec![0u8; 16]; // claims 4x2 = 32 bytes but only 16
        let r = encode_png(&bad, 4, 2, &p);
        assert!(r.is_err());
        let e = r.unwrap_err();
        assert_eq!(e.kind(), std::io::ErrorKind::InvalidInput);
    }

    // ── § T11-LOA-USERFIX : burst + video state-machine tests ──

    #[test]
    fn burst_state_records_10_frames_then_resets() {
        let mut b = BurstState::default();
        let dir = b.start_burst(10, 1);
        assert!(b.active);
        assert!(dir.to_string_lossy().contains("burst_"));
        for i in 0..10 {
            let path = b.tick_capture_path();
            assert!(path.is_some(), "tick {i} must capture");
            let p = path.unwrap();
            let s = p.to_string_lossy();
            assert!(s.contains(&format!("frame_{i:02}.png")));
        }
        // 11th tick : burst exhausted ; no capture.
        assert!(!b.active);
        assert!(b.tick_capture_path().is_none());
    }

    #[test]
    fn burst_state_stride_skips_intermediate_frames() {
        let mut b = BurstState::default();
        b.start_burst(3, 5); // 3 frames, every 5th
        // Tick 1 : captures frame 0
        assert!(b.tick_capture_path().is_some());
        // Ticks 2-5 : skipped
        for _ in 0..4 {
            assert!(b.tick_capture_path().is_none());
        }
        // Tick 6 : captures frame 1
        assert!(b.tick_capture_path().is_some());
    }

    #[test]
    fn video_state_toggles_on_off_with_frame_count() {
        let mut v = VideoState::default();
        // Toggle on
        let started = v.toggle(1, 1000);
        assert!(started);
        assert!(v.recording);
        // Capture 5 frames
        for _ in 0..5 {
            assert!(v.tick_capture_path().is_some());
        }
        // Toggle off
        let stopped_started = v.toggle(1, 1500);
        assert!(!stopped_started);
        assert!(!v.recording);
        // Tick after stop : no capture.
        assert!(v.tick_capture_path().is_none());
    }

    #[test]
    fn video_state_stop_record_returns_frame_count_and_duration() {
        let mut v = VideoState::default();
        v.start_record(1, 1000);
        for _ in 0..7 {
            v.tick_capture_path();
        }
        let (frames, duration) = v.stop_record(1000 + 250).expect("recording was active");
        assert_eq!(frames, 7);
        assert_eq!(duration, 250);
    }

    #[test]
    fn video_state_idempotent_starts_and_stops() {
        let mut v = VideoState::default();
        let p1 = v.start_record(1, 1000);
        let p2 = v.start_record(1, 2000);
        // Both paths are the same — second start was a no-op.
        assert_eq!(p1, p2);
        assert!(v.recording);
        // Two stops in a row : second is None.
        assert!(v.stop_record(3000).is_some());
        assert!(v.stop_record(4000).is_none());
    }

    #[test]
    fn default_video_dir_is_logs_video() {
        assert_eq!(default_video_dir(), PathBuf::from("logs/video"));
    }
}
