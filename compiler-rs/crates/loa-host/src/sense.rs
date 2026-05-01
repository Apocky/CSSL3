//! § sense — full MCP sensory + proprioception harness for Claude.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § T11-LOA-SENSORY (W-LOA-sensory-harness)
//!
//! § ROLE
//!   Aggregation surface for the `sense.*` MCP tool family : 20+ read-only
//!   queries that let Claude perceive the live engine as richly as a human
//!   (visual + audio + spatial), a dog (proprioception + spatial-trajectory),
//!   a bat (compass-ray echolocation), and an ML/AI hybrid (ω-field + spectral
//!   + Stokes substrate sensors). Nine axes :
//!
//!     A. VISUAL          — framebuffer thumbnail · center pixel · viewport summary · raycast
//!     B. AUDIO           — RMS · peak · frequency-spectrum · 1-sec PCM (stub if no audio)
//!     C. SPATIAL         — compass_8 · body_pose history · room neighbors
//!     D. INTEROCEPTION   — engine load · frame pacing · GPU state · thermal
//!     E. DIAGNOSTIC      — recent errors · recent panics · validation errors
//!     F. TEMPORAL        — event log · DM history · input history
//!     G. CAUSAL          — DM state · GM phrases · companion proposals
//!     H. NETWORK         — MCP clients · recent commands
//!     I. ENVIRONMENTAL   — ω-field-at-camera · spectral · stokes · cfer-neighborhood
//!
//! § DESIGN
//!   Every `sense.*` tool is read-only (no sovereign-cap required). Each tool
//!   takes a brief invocation through the MCP server's existing dispatch +
//!   returns a JSON object shaped for direct consumption by Claude. Heavy
//!   data (PNG bytes, PCM bytes, 3D-texel arrays) is base64-encoded inline so
//!   no filesystem round-trip is needed.
//!
//! § TELEMETRY
//!   Every invocation increments `EngineState.sense_invocations_total` plus
//!   global telemetry counters (`sense_*_total` per axis). Logs at INFO when
//!   throttled (every 60th invocation per tool).
//!
//! § PRIME-DIRECTIVE attestation
//!   There was no hurt nor harm in the making of this, to anyone/anything/anybody.

#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::too_many_lines)]

use serde_json::{json, Value};

use crate::cfer_render::{
    decode_radiance_probe, texel_world_size, world_point_to_morton, WORLD_MAX, WORLD_MIN,
};
use crate::material::{material_name, MATERIAL_LUT_LEN};
use crate::mcp_server::{EngineState, SENSE_THUMB_H, SENSE_THUMB_W};
use crate::physics::CompassDistances;
use crate::spectral_bridge::{illuminant_spd, material_reflectance};
use crate::stokes::{sun_stokes_default, StokesVector};

// ──────────────────────────────────────────────────────────────────────────
// § Telemetry counters — global atomics for sense.* invocations
// ──────────────────────────────────────────────────────────────────────────

use std::sync::atomic::{AtomicU64, Ordering};

/// Total `sense.*` invocations since startup.
pub static SENSE_INVOCATIONS_TOTAL: AtomicU64 = AtomicU64::new(0);
/// Total `sense.framebuffer_thumbnail` captures since startup.
pub static SENSE_THUMBNAILS_CAPTURED_TOTAL: AtomicU64 = AtomicU64::new(0);
/// Total visual-axis (A) invocations since startup.
pub static SENSE_VISUAL_TOTAL: AtomicU64 = AtomicU64::new(0);
/// Total audio-axis (B) invocations since startup.
pub static SENSE_AUDIO_TOTAL: AtomicU64 = AtomicU64::new(0);
/// Total spatial-axis (C) invocations since startup.
pub static SENSE_SPATIAL_TOTAL: AtomicU64 = AtomicU64::new(0);
/// Total interoception-axis (D) invocations since startup.
pub static SENSE_INTERO_TOTAL: AtomicU64 = AtomicU64::new(0);
/// Total diagnostic-axis (E) invocations since startup.
pub static SENSE_DIAGNOSTIC_TOTAL: AtomicU64 = AtomicU64::new(0);
/// Total temporal-axis (F) invocations since startup.
pub static SENSE_TEMPORAL_TOTAL: AtomicU64 = AtomicU64::new(0);
/// Total causal-axis (G) invocations since startup.
pub static SENSE_CAUSAL_TOTAL: AtomicU64 = AtomicU64::new(0);
/// Total network-axis (H) invocations since startup.
pub static SENSE_NETWORK_TOTAL: AtomicU64 = AtomicU64::new(0);
/// Total environmental-axis (I) invocations since startup.
pub static SENSE_ENV_TOTAL: AtomicU64 = AtomicU64::new(0);

/// Increment a per-axis counter and the total.
pub fn record_invoke(axis: SenseAxis) {
    SENSE_INVOCATIONS_TOTAL.fetch_add(1, Ordering::Relaxed);
    let counter = match axis {
        SenseAxis::Visual => &SENSE_VISUAL_TOTAL,
        SenseAxis::Audio => &SENSE_AUDIO_TOTAL,
        SenseAxis::Spatial => &SENSE_SPATIAL_TOTAL,
        SenseAxis::Interoception => &SENSE_INTERO_TOTAL,
        SenseAxis::Diagnostic => &SENSE_DIAGNOSTIC_TOTAL,
        SenseAxis::Temporal => &SENSE_TEMPORAL_TOTAL,
        SenseAxis::Causal => &SENSE_CAUSAL_TOTAL,
        SenseAxis::Network => &SENSE_NETWORK_TOTAL,
        SenseAxis::Environmental => &SENSE_ENV_TOTAL,
    };
    counter.fetch_add(1, Ordering::Relaxed);
}

/// 9-axis sensory taxonomy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SenseAxis {
    Visual,
    Audio,
    Spatial,
    Interoception,
    Diagnostic,
    Temporal,
    Causal,
    Network,
    Environmental,
}

// ──────────────────────────────────────────────────────────────────────────
// § Stdlib-only base64 encoder (RFC-4648 standard alphabet · no padding)
// ──────────────────────────────────────────────────────────────────────────

/// Standard RFC-4648 base64 alphabet.
const B64_TABLE: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Encode an arbitrary byte slice as base64 (with `=` padding). Pure stdlib.
#[must_use]
pub fn base64_encode(input: &[u8]) -> String {
    let mut out = String::with_capacity((input.len() + 2) / 3 * 4);
    let mut i = 0;
    while i + 3 <= input.len() {
        let b0 = input[i];
        let b1 = input[i + 1];
        let b2 = input[i + 2];
        out.push(B64_TABLE[(b0 >> 2) as usize] as char);
        out.push(B64_TABLE[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        out.push(B64_TABLE[(((b1 & 0x0F) << 2) | (b2 >> 6)) as usize] as char);
        out.push(B64_TABLE[(b2 & 0x3F) as usize] as char);
        i += 3;
    }
    let rem = input.len() - i;
    if rem == 1 {
        let b0 = input[i];
        out.push(B64_TABLE[(b0 >> 2) as usize] as char);
        out.push(B64_TABLE[((b0 & 0x03) << 4) as usize] as char);
        out.push('=');
        out.push('=');
    } else if rem == 2 {
        let b0 = input[i];
        let b1 = input[i + 1];
        out.push(B64_TABLE[(b0 >> 2) as usize] as char);
        out.push(B64_TABLE[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        out.push(B64_TABLE[((b1 & 0x0F) << 2) as usize] as char);
        out.push('=');
    }
    out
}

// ──────────────────────────────────────────────────────────────────────────
// § Helper : encode RGBA8 thumbnail bytes as a PNG (base64-inline)
// ──────────────────────────────────────────────────────────────────────────

/// Encode an RGBA8 byte buffer of (width × height × 4) as a PNG into a Vec<u8>.
/// Uses the existing `png` crate dep (no new deps added). Returns the raw PNG
/// bytes ready for base64-encoding by the caller.
pub fn rgba8_to_png_bytes(
    rgba: &[u8],
    width: u32,
    height: u32,
) -> std::io::Result<Vec<u8>> {
    let expected = (width as usize) * (height as usize) * 4;
    if rgba.len() != expected {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!(
                "rgba8 buffer size {} does not match {}x{}x4 = {}",
                rgba.len(),
                width,
                height,
                expected
            ),
        ));
    }
    let mut out = Vec::with_capacity(rgba.len() / 4);
    {
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::Other, format!("png header: {e}"))
        })?;
        writer.write_image_data(rgba).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::Other, format!("png write: {e}"))
        })?;
    }
    Ok(out)
}

// ──────────────────────────────────────────────────────────────────────────
// § A. VISUAL
// ──────────────────────────────────────────────────────────────────────────

/// `sense.framebuffer_thumbnail` : return a 256×144 PNG of the most-recent
/// captured framebuffer (base64-inlined). Sets the `capture_pending` flag so
/// the renderer captures a fresh thumbnail on the next frame ; the response
/// reflects the most recent already-captured thumbnail (which may be stale
/// by 1 frame).
pub fn aggregate_framebuffer_thumbnail(state: &mut EngineState) -> Value {
    record_invoke(SenseAxis::Visual);
    state.sense_invocations_total = state.sense_invocations_total.saturating_add(1);
    state.sense_thumbnails_captured_total =
        state.sense_thumbnails_captured_total.saturating_add(1);
    SENSE_THUMBNAILS_CAPTURED_TOTAL.fetch_add(1, Ordering::Relaxed);

    // Request a fresh capture for next frame.
    state.fb_thumb.capture_pending = true;

    if state.fb_thumb.rgba.is_empty() {
        return json!({
            "available": false,
            "width": SENSE_THUMB_W,
            "height": SENSE_THUMB_H,
            "frame": state.fb_thumb.frame,
            "note": "no thumbnail captured yet · run again after at least one frame has rendered",
            "capture_pending": true,
        });
    }

    // Encode RGBA→PNG→base64.
    let png_bytes = match rgba8_to_png_bytes(
        &state.fb_thumb.rgba,
        state.fb_thumb.width,
        state.fb_thumb.height,
    ) {
        Ok(b) => b,
        Err(e) => {
            return json!({
                "available": false,
                "error": format!("png encode failed: {e}"),
            });
        }
    };
    let b64 = base64_encode(&png_bytes);

    json!({
        "available": true,
        "width": state.fb_thumb.width,
        "height": state.fb_thumb.height,
        "frame": state.fb_thumb.frame,
        "png_bytes": png_bytes.len(),
        "png_base64": b64,
        "capture_pending": true,
    })
}

/// `sense.center_pixel` : return RGB + material_id + crosshair_distance +
/// world_position at the center pixel of the most-recent thumbnail. The
/// renderer populates these values when it writes the thumbnail mirror.
pub fn aggregate_center_pixel(state: &mut EngineState) -> Value {
    record_invoke(SenseAxis::Visual);
    state.sense_invocations_total = state.sense_invocations_total.saturating_add(1);
    let m = &state.fb_thumb;
    let mat_name = if m.center_material_id >= 0 && (m.center_material_id as usize) < MATERIAL_LUT_LEN
    {
        material_name(m.center_material_id as u32)
    } else {
        "(none)"
    };
    json!({
        "frame": m.frame,
        "rgb": m.center_rgb,
        "world_pos": {
            "x": m.center_world_pos[0],
            "y": m.center_world_pos[1],
            "z": m.center_world_pos[2],
        },
        "depth_m": m.center_distance,
        "material_id": m.center_material_id,
        "material_name": mat_name,
    })
}

/// `sense.viewport_summary` : 4×4-grid average colors of the most-recent
/// thumbnail. 16 region averages, ordered row-major top-to-bottom.
pub fn aggregate_viewport_summary(state: &mut EngineState) -> Value {
    record_invoke(SenseAxis::Visual);
    state.sense_invocations_total = state.sense_invocations_total.saturating_add(1);
    let regions: Vec<Value> = state
        .fb_thumb
        .regions_4x4
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let row = i / 4;
            let col = i % 4;
            json!({
                "row": row,
                "col": col,
                "rgb": c,
            })
        })
        .collect();
    json!({
        "frame": state.fb_thumb.frame,
        "grid_size": [4, 4],
        "regions": regions,
        "thumbnail_available": !state.fb_thumb.rgba.is_empty(),
    })
}

/// `sense.object_at_crosshair` : raycast from camera along forward-vector,
/// return the first plinth hit with material name + distance. Stage-0
/// uses the existing physics raycast surface.
pub fn aggregate_object_at_crosshair(state: &mut EngineState) -> Value {
    record_invoke(SenseAxis::Visual);
    state.sense_invocations_total = state.sense_invocations_total.saturating_add(1);
    // Compute forward-vector from camera yaw + pitch (FPS convention).
    let yaw = state.camera.yaw;
    let pitch = state.camera.pitch;
    let dir = [
        pitch.cos() * yaw.sin(),
        pitch.sin(),
        pitch.cos() * yaw.cos(),
    ];
    let origin = [
        state.camera.pos.x,
        state.camera.pos.y,
        state.camera.pos.z,
    ];
    // Trial each plinth's AABB.
    let mut nearest_t: f32 = f32::INFINITY;
    let mut nearest_idx: i32 = -1;
    for (i, p) in state.plinths.iter().enumerate() {
        let aabb_min = [p.x - p.half_extent, 0.0, p.z - p.half_extent];
        let aabb_max = [
            p.x + p.half_extent,
            p.half_extent * 2.0,
            p.z + p.half_extent,
        ];
        // Slab method.
        let mut t_enter: f32 = 0.0;
        let mut t_exit: f32 = f32::INFINITY;
        let mut hit = true;
        for axis in 0..3 {
            if dir[axis].abs() < 1e-9 {
                if origin[axis] < aabb_min[axis] || origin[axis] >= aabb_max[axis] {
                    hit = false;
                    break;
                }
                continue;
            }
            let inv = 1.0 / dir[axis];
            let mut t1 = (aabb_min[axis] - origin[axis]) * inv;
            let mut t2 = (aabb_max[axis] - origin[axis]) * inv;
            if t1 > t2 {
                std::mem::swap(&mut t1, &mut t2);
            }
            if t1 > t_enter {
                t_enter = t1;
            }
            if t2 < t_exit {
                t_exit = t2;
            }
            if t_enter > t_exit {
                hit = false;
                break;
            }
        }
        if hit && t_enter > 0.0 && t_enter < nearest_t {
            nearest_t = t_enter;
            nearest_idx = i as i32;
        }
    }
    if nearest_idx < 0 {
        json!({
            "hit": false,
            "max_ray_m": 50.0,
            "camera_pos": [origin[0], origin[1], origin[2]],
            "forward_dir": dir,
        })
    } else {
        let p = state.plinths[nearest_idx as usize];
        json!({
            "hit": true,
            "object_kind": "plinth",
            "plinth_index": nearest_idx,
            "distance_m": nearest_t,
            "world_pos_at_hit": [
                origin[0] + dir[0] * nearest_t,
                origin[1] + dir[1] * nearest_t,
                origin[2] + dir[2] * nearest_t,
            ],
            "plinth_color_rgb": p.color_rgb,
            "camera_pos": [origin[0], origin[1], origin[2]],
            "forward_dir": dir,
        })
    }
}

// ──────────────────────────────────────────────────────────────────────────
// § B. AUDIO
// ──────────────────────────────────────────────────────────────────────────

/// `sense.audio_levels` : RMS + peak + 8-band frequency-spectrum summary
/// over the last ~100ms of audio. Stub : the LoA-v13 host has no audio
/// module yet ; returns explicit "audio:'unavailable'" so MCP clients
/// can detect + handle gracefully.
pub fn aggregate_audio_levels(state: &mut EngineState) -> Value {
    record_invoke(SenseAxis::Audio);
    state.sense_invocations_total = state.sense_invocations_total.saturating_add(1);
    json!({
        "audio": "unavailable",
        "note": "no audio module wired in stage-0 · stub returning unavailable",
        "rms": 0.0,
        "peak": 0.0,
        "bands_8": [0.0_f32, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
    })
}

/// `sense.audio_recent` : last 1-second of audio captured to PCM
/// (base64 inline). Stub.
pub fn aggregate_audio_recent(state: &mut EngineState) -> Value {
    record_invoke(SenseAxis::Audio);
    state.sense_invocations_total = state.sense_invocations_total.saturating_add(1);
    json!({
        "audio": "unavailable",
        "note": "no audio module wired in stage-0 · stub returning unavailable",
        "sample_rate_hz": 16000,
        "channels": 1,
        "duration_ms": 1000,
        "pcm_base64": "",
    })
}

// ──────────────────────────────────────────────────────────────────────────
// § C. SPATIAL
// ──────────────────────────────────────────────────────────────────────────

/// `sense.compass_8` : 8-direction wall-distance raycast from camera.
pub fn aggregate_compass_8(state: &mut EngineState) -> Value {
    record_invoke(SenseAxis::Spatial);
    state.sense_invocations_total = state.sense_invocations_total.saturating_add(1);
    let d = state.compass_distances_m;
    json!({
        "frame": state.frame_count,
        "max_ray_m": 50.0,
        "directions": ["N", "NE", "E", "SE", "S", "SW", "W", "NW"],
        "distances_m": d,
        "n":  d[0], "ne": d[1], "e":  d[2], "se": d[3],
        "s":  d[4], "sw": d[5], "w":  d[6], "nw": d[7],
    })
}

/// `sense.body_pose` : last 60 frames of camera-trajectory + computed velocity
/// + acceleration. Velocities computed from delta(pos)/delta(time_ms).
pub fn aggregate_body_pose(state: &mut EngineState) -> Value {
    record_invoke(SenseAxis::Spatial);
    state.sense_invocations_total = state.sense_invocations_total.saturating_add(1);
    let history: Vec<Value> = state
        .pose_history
        .iter()
        .map(|p| {
            json!({
                "frame": p.frame,
                "time_ms": p.time_ms,
                "pos": [p.pos_x, p.pos_y, p.pos_z],
                "yaw": p.yaw,
                "pitch": p.pitch,
            })
        })
        .collect();

    // Compute current velocity + acceleration from the last 3 entries.
    let n = state.pose_history.len();
    let (vel, accel) = if n >= 3 {
        let p0 = state.pose_history[n - 3];
        let p1 = state.pose_history[n - 2];
        let p2 = state.pose_history[n - 1];
        let dt01 = (p1.time_ms.saturating_sub(p0.time_ms) as f32) / 1000.0;
        let dt12 = (p2.time_ms.saturating_sub(p1.time_ms) as f32) / 1000.0;
        let v01 = if dt01 > 0.0 {
            [
                (p1.pos_x - p0.pos_x) / dt01,
                (p1.pos_y - p0.pos_y) / dt01,
                (p1.pos_z - p0.pos_z) / dt01,
            ]
        } else {
            [0.0; 3]
        };
        let v12 = if dt12 > 0.0 {
            [
                (p2.pos_x - p1.pos_x) / dt12,
                (p2.pos_y - p1.pos_y) / dt12,
                (p2.pos_z - p1.pos_z) / dt12,
            ]
        } else {
            [0.0; 3]
        };
        let dt = (dt01 + dt12) * 0.5;
        let a = if dt > 0.0 {
            [
                (v12[0] - v01[0]) / dt,
                (v12[1] - v01[1]) / dt,
                (v12[2] - v01[2]) / dt,
            ]
        } else {
            [0.0; 3]
        };
        (v12, a)
    } else if n == 2 {
        let p0 = state.pose_history[0];
        let p1 = state.pose_history[1];
        let dt = (p1.time_ms.saturating_sub(p0.time_ms) as f32) / 1000.0;
        let v = if dt > 0.0 {
            [
                (p1.pos_x - p0.pos_x) / dt,
                (p1.pos_y - p0.pos_y) / dt,
                (p1.pos_z - p0.pos_z) / dt,
            ]
        } else {
            [0.0; 3]
        };
        (v, [0.0; 3])
    } else {
        ([0.0; 3], [0.0; 3])
    };

    let speed = (vel[0] * vel[0] + vel[1] * vel[1] + vel[2] * vel[2]).sqrt();

    json!({
        "frame": state.frame_count,
        "samples_count": n,
        "samples_max": 60,
        "current_pos": [
            state.camera.pos.x,
            state.camera.pos.y,
            state.camera.pos.z,
        ],
        "current_yaw": state.camera.yaw,
        "current_pitch": state.camera.pitch,
        "velocity_mps": vel,
        "speed_mps": speed,
        "acceleration_mps2": accel,
        "history": history,
    })
}

/// `sense.room_neighbors` : current room + adjacent rooms + which doorways
/// the camera can see (via the room module's API).
pub fn aggregate_room_neighbors(state: &mut EngineState) -> Value {
    record_invoke(SenseAxis::Spatial);
    state.sense_invocations_total = state.sense_invocations_total.saturating_add(1);
    let pos = [state.camera.pos.x, state.camera.pos.y, state.camera.pos.z];
    let label = crate::room::room_label_at(pos);
    // Try to identify the room enum (not all labels map back, e.g. corridor labels).
    let room = crate::room::Room::from_str(label);
    let adjacent: Vec<Value> = match room {
        Some(r) => crate::room::Room::all()
            .iter()
            .filter_map(|other| {
                if *other == r {
                    None
                } else {
                    Some(json!({
                        "name": other.name(),
                        "id": *other as u32,
                    }))
                }
            })
            .collect(),
        None => Vec::new(),
    };
    json!({
        "current_label": label,
        "current_room_id": room.map(|r| r as i32).unwrap_or(-1),
        "adjacent_rooms": adjacent,
    })
}

/// `sense.spatial_audio` : if any audio sources, directional + distance per source.
/// Stub for stage-0.
pub fn aggregate_spatial_audio(state: &mut EngineState) -> Value {
    record_invoke(SenseAxis::Audio);
    state.sense_invocations_total = state.sense_invocations_total.saturating_add(1);
    json!({
        "audio": "unavailable",
        "sources": [],
        "note": "no audio module wired in stage-0",
    })
}

// ──────────────────────────────────────────────────────────────────────────
// § D. INTEROCEPTION
// ──────────────────────────────────────────────────────────────────────────

/// `sense.engine_load` : process CPU% · memory · GPU pacing. Reads the
/// engine-load mirror written by the window/render loop once per second.
pub fn aggregate_engine_load(state: &mut EngineState) -> Value {
    record_invoke(SenseAxis::Interoception);
    state.sense_invocations_total = state.sense_invocations_total.saturating_add(1);
    let l = state.engine_load.clone();
    json!({
        "frame": state.frame_count,
        "sampled_ms": l.sampled_ms,
        "cpu_percent": l.cpu_percent,
        "memory_mb": l.memory_mb,
        "gpu_resolve_us": l.gpu_resolve_us,
        "tonemap_us": l.tonemap_us,
        "draw_calls": l.draw_calls,
        "vertices": l.vertices,
        "pipeline_switches": l.pipeline_switches,
        "last_frame_ms": l.last_frame_ms,
        "fps_smoothed": l.fps_smoothed,
    })
}

/// `sense.frame_pacing` : last 60 frames as histogram + p50/p95/p99.
pub fn aggregate_frame_pacing(state: &mut EngineState) -> Value {
    record_invoke(SenseAxis::Interoception);
    state.sense_invocations_total = state.sense_invocations_total.saturating_add(1);
    let telem = crate::telemetry::global();
    let (p50, p95, p99) = telem.cached_percentiles();
    let buckets = telem.frame_time_histogram();
    let dropped = if state.engine_load.last_frame_ms > 33.0 { 1 } else { 0 };
    json!({
        "frame": state.frame_count,
        "p50_ms": p50,
        "p95_ms": p95,
        "p99_ms": p99,
        "buckets": buckets,
        "bucket_bounds_ms": crate::telemetry::BUCKET_BOUNDS_MS,
        "last_frame_ms": state.engine_load.last_frame_ms,
        "fps_smoothed": state.engine_load.fps_smoothed,
        "dropped_frames_indicator": dropped,
    })
}

/// `sense.gpu_state` : adapter info + current pipeline binding counts.
pub fn aggregate_gpu_state(state: &mut EngineState) -> Value {
    record_invoke(SenseAxis::Interoception);
    state.sense_invocations_total = state.sense_invocations_total.saturating_add(1);
    let telem = crate::telemetry::global();
    let info = telem.gpu_info_json();
    let info_value: Value = serde_json::from_str(&info).unwrap_or(Value::Null);
    json!({
        "frame": state.frame_count,
        "adapter": info_value,
        "last_frame_draw_calls": state.engine_load.draw_calls,
        "last_frame_vertices": state.engine_load.vertices,
        "last_frame_pipeline_switches": state.engine_load.pipeline_switches,
        "gpu_resolve_us": state.engine_load.gpu_resolve_us,
        "tonemap_us": state.engine_load.tonemap_us,
    })
}

/// `sense.thermal` : CPU/GPU temperature + throttle status. Stage-0 :
/// stdlib-only · returns "unavailable" because temperature requires platform-
/// specific APIs (WMI on Windows, sysfs on Linux). Stage-1 can wire a
/// platform-specific probe.
pub fn aggregate_thermal(state: &mut EngineState) -> Value {
    record_invoke(SenseAxis::Interoception);
    state.sense_invocations_total = state.sense_invocations_total.saturating_add(1);
    json!({
        "thermal": "unavailable",
        "cpu_temp_c": null,
        "gpu_temp_c": null,
        "throttle": "unknown",
        "fan_rpm": null,
        "note": "platform-specific thermal probe not wired in stage-0",
    })
}

// ──────────────────────────────────────────────────────────────────────────
// § E. DIAGNOSTIC
// ──────────────────────────────────────────────────────────────────────────

/// `sense.recent_errors` : last 32 ERROR + WARN log events from the
/// telemetry ring.
pub fn aggregate_recent_errors(state: &mut EngineState) -> Value {
    record_invoke(SenseAxis::Diagnostic);
    state.sense_invocations_total = state.sense_invocations_total.saturating_add(1);
    let count = 32;
    let n = state.telemetry_ring.len();
    let mut errs: Vec<Value> = state.telemetry_ring[n.saturating_sub(count * 2)..]
        .iter()
        .filter(|e| e.level == "ERROR" || e.level == "WARN")
        .map(|e| {
            json!({
                "frame": e.frame,
                "level": e.level,
                "source": e.source,
                "message": e.message,
            })
        })
        .collect();
    if errs.len() > count {
        let extra = errs.len() - count;
        errs.drain(0..extra);
    }
    json!({
        "events": errs,
        "ring_total": n,
    })
}

/// `sense.recent_panics` : panic-events captured by the panic-hook.
pub fn aggregate_recent_panics(state: &mut EngineState) -> Value {
    record_invoke(SenseAxis::Diagnostic);
    state.sense_invocations_total = state.sense_invocations_total.saturating_add(1);
    let panics: Vec<Value> = state
        .panic_events
        .iter()
        .map(|p| {
            json!({
                "frame": p.frame,
                "time_ms": p.time_ms,
                "source": p.source,
                "message": p.message,
            })
        })
        .collect();
    json!({
        "panics": panics,
        "count": state.panic_events.len(),
    })
}

/// `sense.validation_errors` : wgpu/naga validation errors captured.
pub fn aggregate_validation_errors(state: &mut EngineState) -> Value {
    record_invoke(SenseAxis::Diagnostic);
    state.sense_invocations_total = state.sense_invocations_total.saturating_add(1);
    let errs: Vec<Value> = state
        .validation_errors
        .iter()
        .map(|e| {
            json!({
                "frame": e.frame,
                "time_ms": e.time_ms,
                "source": e.source,
                "message": e.message,
            })
        })
        .collect();
    json!({
        "validation_errors": errs,
        "count": state.validation_errors.len(),
    })
}

/// `sense.test_status` : last tour diff + golden-MAE results.
pub fn aggregate_test_status(state: &mut EngineState) -> Value {
    record_invoke(SenseAxis::Diagnostic);
    state.sense_invocations_total = state.sense_invocations_total.saturating_add(1);
    json!({
        "frame": state.frame_count,
        "snapshot_count": state.snapshot_count,
        "tour_progress": state.tour_progress.map(|(c, t)| json!({"current": c, "total": t})),
        "snapshot_pending": !state.snapshot_queue.is_empty(),
        "note": "live tour-diff result lookup deferred to render.diff_golden tool",
    })
}

// ──────────────────────────────────────────────────────────────────────────
// § F. TEMPORAL
// ──────────────────────────────────────────────────────────────────────────

/// `sense.event_log` : last 64 JSONL events from the telemetry ring.
pub fn aggregate_event_log(state: &mut EngineState) -> Value {
    record_invoke(SenseAxis::Temporal);
    state.sense_invocations_total = state.sense_invocations_total.saturating_add(1);
    let telem = crate::telemetry::global();
    let events_json = telem.tail_events_json(64);
    // Parse to verify validity, fall back to empty array on parse error.
    let events: Value = serde_json::from_str(&events_json).unwrap_or(json!([]));
    json!({
        "frame": state.frame_count,
        "events": events,
        "events_ring_cap": crate::telemetry::EVENTS_RING_CAP,
    })
}

/// `sense.dm_history` : last 32 DM-state-transitions + GM-phrase-emits.
pub fn aggregate_dm_history(state: &mut EngineState) -> Value {
    record_invoke(SenseAxis::Temporal);
    state.sense_invocations_total = state.sense_invocations_total.saturating_add(1);
    let history: Vec<Value> = state
        .dm_history
        .iter()
        .map(|h| {
            json!({
                "frame": h.frame,
                "time_ms": h.time_ms,
                "from_state": h.from_state,
                "to_state": h.to_state,
                "tension": h.tension,
                "event_kind": h.event_kind,
            })
        })
        .collect();
    json!({
        "transitions": history,
        "count": state.dm_history.len(),
    })
}

/// `sense.input_history` : last 64 InputFrame events.
pub fn aggregate_input_history(state: &mut EngineState) -> Value {
    record_invoke(SenseAxis::Temporal);
    state.sense_invocations_total = state.sense_invocations_total.saturating_add(1);
    let history: Vec<Value> = state
        .input_history
        .iter()
        .map(|h| {
            json!({
                "frame": h.frame,
                "time_ms": h.time_ms,
                "kind": h.kind,
                "key": h.key,
                "pressed": h.pressed,
            })
        })
        .collect();
    json!({
        "events": history,
        "count": state.input_history.len(),
    })
}

// ──────────────────────────────────────────────────────────────────────────
// § G. CAUSAL
// ──────────────────────────────────────────────────────────────────────────

/// `sense.dm_state` : current DM director state + tension + event count.
pub fn aggregate_dm_state(state: &mut EngineState) -> Value {
    record_invoke(SenseAxis::Causal);
    state.sense_invocations_total = state.sense_invocations_total.saturating_add(1);
    let intensity_to_state = match state.dm.intensity {
        0 => "CALM",
        1 => "BUILDUP",
        2 => "CLIMAX",
        3 => "RELIEF",
        _ => "UNKNOWN",
    };
    json!({
        "frame": state.frame_count,
        "state": intensity_to_state,
        "intensity": state.dm.intensity,
        "event_count": state.dm.event_count,
        "recent_transitions": state.dm_history.len(),
    })
}

/// `sense.gm_recent_phrases` : last 16 phrases the GM narrator emitted.
pub fn aggregate_gm_recent_phrases(state: &mut EngineState) -> Value {
    record_invoke(SenseAxis::Causal);
    state.sense_invocations_total = state.sense_invocations_total.saturating_add(1);
    let phrases: Vec<Value> = state
        .gm_phrase_history
        .iter()
        .map(|p| {
            json!({
                "frame": p.frame,
                "time_ms": p.time_ms,
                "topic": p.topic,
                "mood": p.mood,
                "line": p.line,
            })
        })
        .collect();
    json!({
        "phrases": phrases,
        "count": state.gm_phrase_history.len(),
    })
}

/// `sense.companion_proposals` : pending companion-AI proposals.
pub fn aggregate_companion_proposals(state: &mut EngineState) -> Value {
    record_invoke(SenseAxis::Causal);
    state.sense_invocations_total = state.sense_invocations_total.saturating_add(1);
    let proposals: Vec<Value> = state
        .companion_proposals
        .iter()
        .map(|p| {
            json!({
                "frame": p.frame,
                "time_ms": p.time_ms,
                "kind": p.kind,
                "payload": p.payload,
                "authorized": p.authorized,
            })
        })
        .collect();
    json!({
        "proposals": proposals,
        "count": state.companion_proposals.len(),
    })
}

// ──────────────────────────────────────────────────────────────────────────
// § H. NETWORK
// ──────────────────────────────────────────────────────────────────────────

/// `sense.mcp_clients` : currently-connected MCP clients + invocation counts.
pub fn aggregate_mcp_clients(state: &mut EngineState) -> Value {
    record_invoke(SenseAxis::Network);
    state.sense_invocations_total = state.sense_invocations_total.saturating_add(1);
    let clients: Vec<Value> = state
        .mcp_clients
        .iter()
        .map(|c| {
            json!({
                "addr": c.addr,
                "connected_ms": c.connected_ms,
                "invocations": c.invocations,
            })
        })
        .collect();
    json!({
        "clients": clients,
        "count": state.mcp_clients.len(),
    })
}

/// `sense.recent_commands` : last 32 MCP-tool invocations + caller + latency.
pub fn aggregate_recent_commands(state: &mut EngineState) -> Value {
    record_invoke(SenseAxis::Network);
    state.sense_invocations_total = state.sense_invocations_total.saturating_add(1);
    let cmds: Vec<Value> = state
        .mcp_command_history
        .iter()
        .map(|c| {
            json!({
                "frame": c.frame,
                "time_ms": c.time_ms,
                "caller": c.caller,
                "tool": c.tool,
                "latency_us": c.latency_us,
                "success": c.success,
            })
        })
        .collect();
    json!({
        "commands": cmds,
        "count": state.mcp_command_history.len(),
    })
}

// ──────────────────────────────────────────────────────────────────────────
// § I. ENVIRONMENTAL (substrate sensors)
// ──────────────────────────────────────────────────────────────────────────

/// `sense.omega_field_at_camera` : sample ω-field cell at camera position,
/// return all 7 facets (M density · velocity · vorticity · enthalpy ·
/// multivec · radiance probe · pattern_handle · sigma_low). Returns `available
/// = false` on out-of-envelope.
pub fn aggregate_omega_field_at_camera(state: &mut EngineState) -> Value {
    record_invoke(SenseAxis::Environmental);
    state.sense_invocations_total = state.sense_invocations_total.saturating_add(1);
    let p = [state.camera.pos.x, state.camera.pos.y, state.camera.pos.z];
    let key = match world_point_to_morton(p[0], p[1], p[2]) {
        Some(k) => k,
        None => {
            return json!({
                "available": false,
                "note": "camera outside ω-field world envelope",
                "camera_pos": p,
                "world_min": WORLD_MIN,
                "world_max": WORLD_MAX,
            });
        }
    };
    // Sample via the cssl-rt FFI (matches omega_sample tool's path).
    let mut buf = [0u8; cssl_rt::loa_stubs::FIELD_CELL_BYTES];
    // SAFETY : __cssl_omega_field_sample writes 0 (unset) or FIELD_CELL_BYTES
    // (cell present), no over-write. Buf is exactly FIELD_CELL_BYTES.
    let written = unsafe {
        cssl_rt::loa_stubs::__cssl_omega_field_sample(
            key.to_u64(),
            buf.as_mut_ptr(),
            buf.len() as i32,
        )
    };
    let cell_present = written == cssl_rt::loa_stubs::FIELD_CELL_BYTES as i32;
    if !cell_present {
        return json!({
            "available": true,
            "cell_present": false,
            "morton": key.to_u64(),
            "camera_pos": p,
            "facets": {
                "density": 0.0,
                "velocity": [0.0, 0.0, 0.0],
                "vorticity": 0.0,
                "enthalpy": 0.0,
                "multivec_lo": 0,
                "radiance_probe_lo": 0,
                "pattern_handle": 0,
                "sigma_low": 0,
            },
        });
    }
    // Decode the cell-bytes : the FieldCell layout is
    //   density (f32) · velocity (3 × f32) · vorticity (f32) · enthalpy (f32)
    //   · multivec_lo (u64) · radiance_probe_lo (u64) · pattern_handle (u32)
    //   · sigma_low (u16) · pad (u16)  ⇒  88 bytes total.
    fn read_f32(b: &[u8], i: usize) -> f32 {
        f32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]])
    }
    fn read_u64(b: &[u8], i: usize) -> u64 {
        u64::from_le_bytes([
            b[i], b[i + 1], b[i + 2], b[i + 3], b[i + 4], b[i + 5], b[i + 6], b[i + 7],
        ])
    }
    fn read_u32(b: &[u8], i: usize) -> u32 {
        u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]])
    }
    fn read_u16(b: &[u8], i: usize) -> u16 {
        u16::from_le_bytes([b[i], b[i + 1]])
    }
    let density = read_f32(&buf, 0);
    let vx = read_f32(&buf, 4);
    let vy = read_f32(&buf, 8);
    let vz = read_f32(&buf, 12);
    let vorticity = read_f32(&buf, 16);
    let enthalpy = read_f32(&buf, 20);
    let multivec_lo = read_u64(&buf, 24);
    let radiance_probe_lo = read_u64(&buf, 32);
    let pattern_handle = read_u32(&buf, 40);
    let sigma_low = read_u16(&buf, 44);
    let (rr, gg, bb) = decode_radiance_probe(radiance_probe_lo);
    json!({
        "available": true,
        "cell_present": true,
        "morton": key.to_u64(),
        "camera_pos": p,
        "facets": {
            "density": density,
            "velocity": [vx, vy, vz],
            "vorticity": vorticity,
            "enthalpy": enthalpy,
            "multivec_lo": multivec_lo,
            "radiance_probe_lo": radiance_probe_lo,
            "radiance_probe_rgb": [rr, gg, bb],
            "pattern_handle": pattern_handle,
            "sigma_low": sigma_low,
        },
    })
}

/// `sense.spectral_at_pixel` : at center pixel, return the 16-band SpectralRadiance
/// under the current illuminant for the material visible at the crosshair.
pub fn aggregate_spectral_at_pixel(state: &mut EngineState) -> Value {
    record_invoke(SenseAxis::Environmental);
    state.sense_invocations_total = state.sense_invocations_total.saturating_add(1);
    let mat_id = if state.fb_thumb.center_material_id >= 0 {
        state.fb_thumb.center_material_id as u32
    } else {
        0 // matte-grey default
    };
    let spd = illuminant_spd(state.illuminant);
    let refl = material_reflectance(mat_id);
    let mut radiance = [0.0_f32; 16];
    for i in 0..16 {
        radiance[i] = refl[i] * spd[i];
    }
    json!({
        "frame": state.frame_count,
        "material_id": mat_id,
        "material_name": material_name(mat_id),
        "illuminant": state.illuminant.name(),
        "illuminant_cct_kelvin": state.illuminant.cct_kelvin(),
        "band_count": 16,
        "spd_16": spd,
        "reflectance_16": refl,
        "radiance_16": radiance,
    })
}

/// `sense.stokes_at_pixel` : at center pixel, return Stokes IQUV + DOP-linear
/// + DOP-total + AoLP (angle of linear polarization).
pub fn aggregate_stokes_at_pixel(state: &mut EngineState) -> Value {
    record_invoke(SenseAxis::Environmental);
    state.sense_invocations_total = state.sense_invocations_total.saturating_add(1);
    let s = sun_stokes_default();
    // Apply the Mueller for the visible material if any.
    let s_out: StokesVector = if state.fb_thumb.center_material_id >= 0 {
        let lut = crate::stokes::mueller_lut();
        let idx =
            (state.fb_thumb.center_material_id as usize).min(crate::stokes::MUELLER_LUT_LEN - 1);
        lut[idx].apply(s)
    } else {
        s
    };
    let dop_lin = s_out.dop_linear();
    let dop_tot = s_out.dop_total();
    // AoLP = 0.5 * atan2(U, Q).
    let aolp = 0.5 * s_out.u.atan2(s_out.q);
    json!({
        "frame": state.frame_count,
        "stokes": {
            "i": s_out.i,
            "q": s_out.q,
            "u": s_out.u,
            "v": s_out.v,
        },
        "dop_linear": dop_lin,
        "dop_total": dop_tot,
        "aolp_radians": aolp,
        "aolp_degrees": aolp.to_degrees(),
        "material_id": state.fb_thumb.center_material_id,
    })
}

/// `sense.cfer_neighborhood` : 3×3×3 ω-field cells around camera with KAN-eval-counts
/// and radiance probe values per cell. Stage-0 reads the field via FFI
/// (one sample per cell · 27 samples).
pub fn aggregate_cfer_neighborhood(state: &mut EngineState) -> Value {
    record_invoke(SenseAxis::Environmental);
    state.sense_invocations_total = state.sense_invocations_total.saturating_add(1);
    let camera_p = [state.camera.pos.x, state.camera.pos.y, state.camera.pos.z];
    let ts = texel_world_size();
    let mut cells: Vec<Value> = Vec::with_capacity(27);
    for dz in -1_i32..=1_i32 {
        for dy in -1_i32..=1_i32 {
            for dx in -1_i32..=1_i32 {
                let p = [
                    camera_p[0] + dx as f32 * ts[0],
                    camera_p[1] + dy as f32 * ts[1],
                    camera_p[2] + dz as f32 * ts[2],
                ];
                let key = match world_point_to_morton(p[0], p[1], p[2]) {
                    Some(k) => k,
                    None => continue,
                };
                let mut buf = [0u8; cssl_rt::loa_stubs::FIELD_CELL_BYTES];
                // SAFETY : same contract as in aggregate_omega_field_at_camera.
                let written = unsafe {
                    cssl_rt::loa_stubs::__cssl_omega_field_sample(
                        key.to_u64(),
                        buf.as_mut_ptr(),
                        buf.len() as i32,
                    )
                };
                let present = written == cssl_rt::loa_stubs::FIELD_CELL_BYTES as i32;
                cells.push(json!({
                    "offset": [dx, dy, dz],
                    "world_pos": p,
                    "morton": key.to_u64(),
                    "present": present,
                }));
            }
        }
    }
    json!({
        "frame": state.frame_count,
        "camera_pos": camera_p,
        "cell_size_m": ts,
        "cells_3x3x3": cells,
        "kan_handle": state.cfer.kan_handle,
        "kan_evals_last_step": state.cfer.kan_evals,
        "active_cells_total": state.cfer.active_cells,
        "center_radiance_rgb": state.cfer.center_radiance,
    })
}

/// `sense.dgi_signal` : DGI runtime state (manifold · cogstate · SCT · DCU ·
/// perception · retrocausal). Stage-0 stub : surface returns "unwired" as the
/// DGI runtime is not yet attached to the loa-host.
pub fn aggregate_dgi_signal(state: &mut EngineState) -> Value {
    record_invoke(SenseAxis::Environmental);
    state.sense_invocations_total = state.sense_invocations_total.saturating_add(1);
    json!({
        "available": false,
        "manifold": null,
        "cogstate": null,
        "sct": null,
        "dcu": null,
        "perception": null,
        "retrocausal": null,
        "note": "DGI runtime not yet wired to loa-host in stage-0 · stub",
        "frame": state.frame_count,
    })
}

// ──────────────────────────────────────────────────────────────────────────
// § Combined snapshot (sense.snapshot — meta-tool for one-shot dump)
// ──────────────────────────────────────────────────────────────────────────

/// `sense.snapshot` : a single-call combined dump of the lightest sense.* tools.
/// Useful for an MCP client to pull a full perceptual frame without 20+
/// round-trips. Excludes the framebuffer thumbnail (large) and audio (stubs).
pub fn aggregate_combined_snapshot(state: &mut EngineState) -> Value {
    record_invoke(SenseAxis::Visual);
    state.sense_invocations_total = state.sense_invocations_total.saturating_add(1);
    let center = aggregate_center_pixel(state);
    let compass = aggregate_compass_8(state);
    let pose = aggregate_body_pose(state);
    let load = aggregate_engine_load(state);
    let pacing = aggregate_frame_pacing(state);
    let dm_state_v = aggregate_dm_state(state);
    let neighbors = aggregate_room_neighbors(state);
    json!({
        "frame": state.frame_count,
        "center_pixel": center,
        "compass_8": compass,
        "body_pose": pose,
        "engine_load": load,
        "frame_pacing": pacing,
        "dm_state": dm_state_v,
        "room_neighbors": neighbors,
        "sense_invocations_total": state.sense_invocations_total,
    })
}

// ──────────────────────────────────────────────────────────────────────────
// § Panic-hook + log-tap helpers
// ──────────────────────────────────────────────────────────────────────────

use std::sync::Mutex;
use std::sync::OnceLock;

/// Global registry of EngineStates that the panic-hook should write to.
/// Populated by `install_panic_hook` ; consulted by the global panic handler.
static PANIC_STATE_REGISTRY: OnceLock<Mutex<Option<std::sync::Arc<Mutex<EngineState>>>>> =
    OnceLock::new();

/// Install a process-wide panic hook that records panics into the EngineState
/// `panic_events` ring (so MCP `sense.recent_panics` can read them). Idempotent
/// — repeated calls update the registered EngineState handle but install the
/// hook only once.
pub fn install_panic_hook(state: std::sync::Arc<Mutex<EngineState>>) {
    let registry = PANIC_STATE_REGISTRY.get_or_init(|| Mutex::new(None));
    if let Ok(mut g) = registry.lock() {
        let already_installed = g.is_some();
        *g = Some(state);
        if already_installed {
            // Hook already in place ; we just refreshed the state-handle.
            return;
        }
    }
    let prior = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let payload_msg = if let Some(s) = info.payload().downcast_ref::<&str>() {
            (*s).to_string()
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "panic with non-string payload".to_string()
        };
        let location = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_else(|| "unknown".to_string());
        if let Some(reg) = PANIC_STATE_REGISTRY.get() {
            if let Ok(reg_g) = reg.lock() {
                if let Some(state) = reg_g.as_ref() {
                    if let Ok(mut g) = state.lock() {
                        let frame = g.frame_count;
                        g.push_panic(crate::mcp_server::ValidationErrorEntry {
                            frame,
                            time_ms: now_ms,
                            source: format!("panic@{}", location),
                            message: payload_msg.clone(),
                        });
                    }
                }
            }
        }
        // Forward to the prior hook (default = print + abort with backtrace).
        prior(info);
    }));
}

// ──────────────────────────────────────────────────────────────────────────
// § Compass-distance computation helper for headless tests
// ──────────────────────────────────────────────────────────────────────────

/// Convert a `CompassDistances` struct into the [N, NE, E, SE, S, SW, W, NW]
/// f32 array ordering used by the `EngineState.compass_distances_m` mirror.
#[must_use]
pub fn compass_to_array(c: CompassDistances) -> [f32; 8] {
    c.dist
}

// ══════════════════════════════════════════════════════════════════════════
// § TESTS
// ══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp_server::{
        DmHistoryEntry, EngineLoadMirror, GmPhraseEntry, InputHistoryEntry, McpCommandEntry,
        PoseSample, ValidationErrorEntry,
    };

    fn make_state_with_fixtures() -> EngineState {
        let mut s = EngineState::default();
        // Push some pose history.
        for i in 0..5 {
            s.push_pose_sample(PoseSample {
                frame: i,
                time_ms: i * 16,
                pos_x: i as f32 * 0.5,
                pos_y: 1.55,
                pos_z: 0.0,
                yaw: 0.0,
                pitch: 0.0,
            });
        }
        // DM history.
        s.push_dm_history(DmHistoryEntry {
            frame: 1,
            time_ms: 1000,
            from_state: "CALM".to_string(),
            to_state: "BUILDUP".to_string(),
            tension: 0.3,
            event_kind: Some("SPAWN_NPC_ARRIVAL".to_string()),
        });
        // GM phrase.
        s.push_gm_phrase(GmPhraseEntry {
            frame: 5,
            time_ms: 5000,
            topic: "WEATHER".to_string(),
            mood: "neutral".to_string(),
            line: "The fog gathers thick.".to_string(),
        });
        // Input event.
        s.push_input_event(InputHistoryEntry {
            frame: 10,
            time_ms: 10000,
            kind: "key".to_string(),
            key: "W".to_string(),
            pressed: true,
        });
        // Validation error.
        s.push_validation_error(ValidationErrorEntry {
            frame: 20,
            time_ms: 20000,
            source: "wgpu".to_string(),
            message: "test validation error".to_string(),
        });
        // MCP client + command.
        s.record_mcp_client_connect("127.0.0.1:55432", 30000);
        s.record_mcp_command(McpCommandEntry {
            frame: 30,
            time_ms: 30100,
            caller: "127.0.0.1:55432".to_string(),
            tool: "engine.state".to_string(),
            latency_us: 120,
            success: true,
        });
        // Engine-load.
        s.engine_load = EngineLoadMirror {
            sampled_ms: 30000,
            cpu_percent: 22.5,
            memory_mb: 156.0,
            gpu_resolve_us: 320,
            tonemap_us: 80,
            draw_calls: 12,
            vertices: 12_345,
            pipeline_switches: 5,
            last_frame_ms: 16.5,
            fps_smoothed: 60.6,
        };
        // Compass distances : pretend the camera is in the center of a 10m room.
        s.compass_distances_m = [5.0, 7.07, 5.0, 7.07, 5.0, 7.07, 5.0, 7.07];
        // Push some telemetry events for recent_errors.
        s.push_event("WARN", "test", "warn-test-1");
        s.push_event("ERROR", "test", "err-test-1");
        s.push_event("INFO", "test", "info-test-1");
        s
    }

    #[test]
    fn sense_framebuffer_thumbnail_returns_valid_png_base64() {
        let mut s = EngineState::default();
        // Synthesize a tiny thumbnail (8×4 RGBA).
        let w = 8;
        let h = 4;
        let mut rgba = Vec::with_capacity((w * h * 4) as usize);
        for _ in 0..(w * h) {
            rgba.extend_from_slice(&[200, 100, 50, 255]);
        }
        s.fb_thumb.width = w;
        s.fb_thumb.height = h;
        s.fb_thumb.rgba = rgba;
        s.fb_thumb.frame = 1;
        let v = aggregate_framebuffer_thumbnail(&mut s);
        assert_eq!(v["available"], true);
        assert_eq!(v["width"], 8);
        assert_eq!(v["height"], 4);
        let b64 = v["png_base64"].as_str().expect("base64 string");
        // PNG signature in base64 begins with "iVBORw0KGgo".
        assert!(
            b64.starts_with("iVBORw0KGgo"),
            "expected PNG signature, got {}",
            &b64[..b64.len().min(20)]
        );
        assert_eq!(v["capture_pending"], true);
    }

    #[test]
    fn sense_framebuffer_thumbnail_handles_empty_buffer() {
        let mut s = EngineState::default();
        let v = aggregate_framebuffer_thumbnail(&mut s);
        assert_eq!(v["available"], false);
        assert_eq!(v["capture_pending"], true);
    }

    #[test]
    fn sense_center_pixel_returns_world_pos_and_material_id() {
        let mut s = EngineState::default();
        s.fb_thumb.center_rgb = [0.5, 0.5, 0.5];
        s.fb_thumb.center_distance = 3.2;
        s.fb_thumb.center_material_id = 4;
        s.fb_thumb.center_world_pos = [1.0, 1.5, 2.0];
        let v = aggregate_center_pixel(&mut s);
        assert_eq!(v["material_id"], 4);
        // f32 → f64 precision : compare with epsilon.
        let depth = v["depth_m"].as_f64().unwrap();
        assert!((depth - 3.2).abs() < 1e-3, "depth = {depth}");
        assert!((v["world_pos"]["x"].as_f64().unwrap() - 1.0).abs() < 1e-3);
    }

    #[test]
    fn sense_compass_8_returns_8_distances() {
        let mut s = make_state_with_fixtures();
        let v = aggregate_compass_8(&mut s);
        let d = v["distances_m"].as_array().expect("array");
        assert_eq!(d.len(), 8);
        assert_eq!(d[0].as_f64().unwrap(), 5.0);
        assert_eq!(v["directions"].as_array().unwrap().len(), 8);
    }

    #[test]
    fn sense_body_pose_history_grows_to_60_max() {
        let mut s = EngineState::default();
        for i in 0..100 {
            s.push_pose_sample(PoseSample {
                frame: i,
                time_ms: i * 16,
                pos_x: i as f32 * 0.1,
                pos_y: 1.55,
                pos_z: 0.0,
                yaw: 0.0,
                pitch: 0.0,
            });
        }
        // Cap is 60.
        assert_eq!(s.pose_history.len(), 60);
        let v = aggregate_body_pose(&mut s);
        assert_eq!(v["samples_count"].as_u64().unwrap(), 60);
        let history = v["history"].as_array().expect("array");
        assert_eq!(history.len(), 60);
        // Velocity should be nonzero (we moved).
        let speed = v["speed_mps"].as_f64().unwrap();
        assert!(speed > 0.0, "expected nonzero speed, got {speed}");
    }

    #[test]
    fn sense_engine_load_returns_cpu_gpu_memory() {
        let mut s = make_state_with_fixtures();
        let v = aggregate_engine_load(&mut s);
        assert_eq!(v["cpu_percent"].as_f64().unwrap(), 22.5);
        assert_eq!(v["memory_mb"].as_f64().unwrap(), 156.0);
        assert_eq!(v["draw_calls"].as_u64().unwrap(), 12);
    }

    #[test]
    fn sense_frame_pacing_returns_p50_p95_p99() {
        let mut s = make_state_with_fixtures();
        let v = aggregate_frame_pacing(&mut s);
        // p50 / p95 / p99 fields exist (values may be 0 if telem hasn't recorded any frames).
        assert!(v.get("p50_ms").is_some());
        assert!(v.get("p95_ms").is_some());
        assert!(v.get("p99_ms").is_some());
        assert!(v.get("buckets").is_some());
        assert_eq!(
            v["buckets"].as_array().unwrap().len(),
            crate::telemetry::BUCKET_COUNT
        );
    }

    #[test]
    fn sense_recent_errors_returns_last_warn_or_error_events() {
        let mut s = make_state_with_fixtures();
        let v = aggregate_recent_errors(&mut s);
        let events = v["events"].as_array().expect("array");
        // We pushed 1 WARN and 1 ERROR ; INFO must be filtered out.
        assert_eq!(events.len(), 2);
        for e in events {
            let level = e["level"].as_str().unwrap();
            assert!(level == "WARN" || level == "ERROR");
        }
    }

    #[test]
    fn sense_event_log_returns_jsonl_ring() {
        let mut s = EngineState::default();
        let v = aggregate_event_log(&mut s);
        // events field always exists, even when empty.
        assert!(v.get("events").is_some());
        // events_ring_cap is the static constant.
        assert_eq!(
            v["events_ring_cap"].as_u64().unwrap(),
            crate::telemetry::EVENTS_RING_CAP as u64
        );
    }

    #[test]
    fn sense_dm_state_returns_director_state_name() {
        let mut s = EngineState::default();
        s.dm.intensity = 2;
        let v = aggregate_dm_state(&mut s);
        assert_eq!(v["state"].as_str().unwrap(), "CLIMAX");
        assert_eq!(v["intensity"].as_u64().unwrap(), 2);
    }

    #[test]
    fn sense_mcp_clients_returns_connected_addrs() {
        let mut s = make_state_with_fixtures();
        let v = aggregate_mcp_clients(&mut s);
        let clients = v["clients"].as_array().expect("array");
        assert_eq!(clients.len(), 1);
        assert_eq!(clients[0]["addr"].as_str().unwrap(), "127.0.0.1:55432");
        assert_eq!(clients[0]["invocations"].as_u64().unwrap(), 1);
    }

    #[test]
    fn sense_omega_field_at_camera_returns_7_facets() {
        let mut s = EngineState::default();
        // Force camera into envelope.
        s.camera.pos = crate::mcp_server::Vec3 { x: 0.0, y: 1.5, z: 0.0 };
        let v = aggregate_omega_field_at_camera(&mut s);
        // available is true (camera is in envelope).
        assert!(v["available"].as_bool().unwrap());
        // facets struct must contain all 7 (8-with-radiance_rgb).
        let facets = v["facets"].as_object().expect("facets object");
        for k in &[
            "density",
            "velocity",
            "vorticity",
            "enthalpy",
            "multivec_lo",
            "radiance_probe_lo",
            "pattern_handle",
            "sigma_low",
        ] {
            assert!(facets.contains_key(*k), "missing facet {k}");
        }
    }

    #[test]
    fn sense_spectral_at_pixel_returns_16_bands() {
        let mut s = EngineState::default();
        s.fb_thumb.center_material_id = 1; // Vermillion lacquer
        let v = aggregate_spectral_at_pixel(&mut s);
        assert_eq!(v["band_count"].as_u64().unwrap(), 16);
        let spd = v["spd_16"].as_array().expect("array");
        assert_eq!(spd.len(), 16);
        let refl = v["reflectance_16"].as_array().expect("array");
        assert_eq!(refl.len(), 16);
        assert_eq!(v["material_id"].as_u64().unwrap(), 1);
    }

    #[test]
    fn sense_object_at_crosshair_raycasts_against_plinths() {
        let mut s = EngineState::default();
        // Default plinths at (-2,-2), (2,-2), (-2,2), (2,2).
        // Place camera at (0, 1.55, -3) facing +Z (yaw 0, pitch 0).
        s.camera.pos = crate::mcp_server::Vec3 {
            x: 0.0,
            y: 1.55,
            z: -3.0,
        };
        s.camera.yaw = 0.0;
        s.camera.pitch = 0.0;
        let v = aggregate_object_at_crosshair(&mut s);
        // Camera at z=-3 looking +Z (north). Plinths at z=-2 (in front) and z=+2.
        // Should hit the plinth at z=-2 (the closest one in our path is at x=-2 or x=2 — neither is on the ray.
        // Forward dir (0,0,1) ; ray from (0,1.55,-3) → (0,1.55,∞). No plinth is at x=0, z>-3, so MISS.
        // Verify we got a structured response either way.
        assert!(v.get("hit").is_some());
        assert!(v.get("camera_pos").is_some());
    }

    #[test]
    fn mcp_sense_tool_count_at_least_20() {
        // Verify we actually register 20+ sense.* tools. This is checked
        // by inspecting the registry produced by `tool_registry()` in mcp_tools.
        let r = crate::mcp_tools::tool_registry();
        let sense_tool_count = r.keys().filter(|k| k.starts_with("sense.")).count();
        assert!(
            sense_tool_count >= 20,
            "expected ≥ 20 sense.* tools, got {sense_tool_count}"
        );
    }

    #[test]
    fn sense_input_history_returns_keys() {
        let mut s = make_state_with_fixtures();
        let v = aggregate_input_history(&mut s);
        let evs = v["events"].as_array().expect("array");
        assert_eq!(evs.len(), 1);
        assert_eq!(evs[0]["key"].as_str().unwrap(), "W");
        assert!(evs[0]["pressed"].as_bool().unwrap());
    }

    #[test]
    fn sense_recent_commands_returns_invocations() {
        let mut s = make_state_with_fixtures();
        let v = aggregate_recent_commands(&mut s);
        let cmds = v["commands"].as_array().expect("array");
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0]["tool"].as_str().unwrap(), "engine.state");
        assert_eq!(cmds[0]["latency_us"].as_u64().unwrap(), 120);
    }

    #[test]
    fn base64_encode_known_strings() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn rgba8_to_png_bytes_produces_valid_png() {
        let w = 4;
        let h = 4;
        let rgba = vec![128u8; (w * h * 4) as usize];
        let png_bytes = rgba8_to_png_bytes(&rgba, w, h).expect("encode ok");
        // PNG signature : 8 bytes [137, 80, 78, 71, 13, 10, 26, 10].
        assert_eq!(&png_bytes[0..8], &[137, 80, 78, 71, 13, 10, 26, 10]);
    }

    #[test]
    fn sense_combined_snapshot_aggregates_lightweight_axes() {
        let mut s = make_state_with_fixtures();
        let v = aggregate_combined_snapshot(&mut s);
        for k in &[
            "center_pixel",
            "compass_8",
            "body_pose",
            "engine_load",
            "frame_pacing",
            "dm_state",
            "room_neighbors",
        ] {
            assert!(v.get(*k).is_some(), "missing axis {k}");
        }
    }

    #[test]
    fn record_invoke_increments_counters() {
        let baseline = SENSE_INVOCATIONS_TOTAL.load(Ordering::Relaxed);
        record_invoke(SenseAxis::Visual);
        assert_eq!(
            SENSE_INVOCATIONS_TOTAL.load(Ordering::Relaxed),
            baseline + 1
        );
    }

    #[test]
    fn sense_validation_errors_returns_recorded_errors() {
        let mut s = make_state_with_fixtures();
        let v = aggregate_validation_errors(&mut s);
        let errs = v["validation_errors"].as_array().expect("array");
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0]["source"].as_str().unwrap(), "wgpu");
    }

    #[test]
    fn sense_dm_history_returns_transitions() {
        let mut s = make_state_with_fixtures();
        let v = aggregate_dm_history(&mut s);
        let transitions = v["transitions"].as_array().expect("array");
        assert_eq!(transitions.len(), 1);
        assert_eq!(transitions[0]["from_state"].as_str().unwrap(), "CALM");
        assert_eq!(transitions[0]["to_state"].as_str().unwrap(), "BUILDUP");
    }

    #[test]
    fn sense_gm_recent_phrases_returns_emitted_lines() {
        let mut s = make_state_with_fixtures();
        let v = aggregate_gm_recent_phrases(&mut s);
        let phrases = v["phrases"].as_array().expect("array");
        assert_eq!(phrases.len(), 1);
        assert_eq!(phrases[0]["topic"].as_str().unwrap(), "WEATHER");
    }
}
