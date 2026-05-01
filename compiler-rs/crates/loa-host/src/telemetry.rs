//! § telemetry — per-frame counters + histograms + GPU info + CSV/JSONL emit.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § T11-LOA-TELEM (W-LOA-telemetry-expand) — turn-N iteration of the
//! logging+telemetry axis. Sits alongside `cssl_rt::loa_startup::log_event`
//! (which owns the unstructured per-line log) and provides the *structured*
//! companion : atomic counters, ring-buffered frame-time samples, P50/P95/P99
//! computed every Nth frame, GPU adapter dump captured once at init, and
//! periodic flushes to `logs/loa_telemetry.csv` + `logs/loa_events.jsonl`.
//!
//! § DESIGN GOALS
//!
//!   1. *Lock-free hot path.* The render loop calls `frame_begin` /
//!      `frame_end` / `record_pipeline_switch` every frame ; these MUST be
//!      cheap. All counters are `AtomicU64`. The frame-time ring is the
//!      only Mutex-guarded structure, and that lock is held for ≤ a single
//!      ringbuffer write.
//!
//!   2. *Backpressure-free emit.* CSV + JSONL writes are throttled : CSV
//!      emits at 1 Hz (driven by a wall-clock check inside `frame_end`),
//!      JSONL writes are append-only and immediately flushed (small
//!      records, single line each).
//!
//!   3. *Sovereign-controlled retention.* No emoji. No rotating-handle
//!      surprises (rotation lives in `cssl_rt::loa_startup::log_event` ;
//!      the telemetry CSV/JSONL are separate files with explicit
//!      truncation semantics).
//!
//!   4. *MCP-queryable snapshots.* `snapshot_json` returns a structured
//!      map suitable for `telemetry.snapshot` ; `frame_time_histogram`
//!      returns the 10-bucket count vector for `telemetry.histogram`.
//!
//! § FILES WRITTEN
//!
//!   * `logs/loa_telemetry.csv`  — header + one row/sec : `ts,frame_count,
//!     fps,p50_ms,p95_ms,p99_ms,draw_calls,vertices,pipeline_switches`.
//!   * `logs/loa_events.jsonl`   — newline-delimited JSON, one record per
//!     event (render_frame / dm_state_transition / mcp_tool_invoked / etc.).
//!
//! § PRIME-DIRECTIVE attestation
//!   There was no hurt nor harm in the making of this, to anyone/anything/anybody.

#![allow(clippy::cast_precision_loss)] // u64 → f64 for averages is fine
#![allow(clippy::cast_possible_truncation)] // f32 → u32 fixed-point conversion bounded
#![allow(clippy::cast_sign_loss)] // saturating bucket-index conversion
#![allow(clippy::module_name_repetitions)]

use std::fs::{self, OpenOptions};
use std::io::Write as _;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

// ───────────────────────────────────────────────────────────────────────
// § GpuAdapterInfo — captured once at `GpuContext::new`.
// ───────────────────────────────────────────────────────────────────────

/// Captured wgpu adapter identity + capability summary.
///
/// Populated once when the GPU comes up. MCP `telemetry.gpu_info` returns
/// this struct serialized as JSON. Equivalent of `dxdiag` for the host —
/// answers "what GPU is the user actually running on right now?" without
/// requiring a separate diagnostic dump tool.
#[derive(Debug, Clone, Default)]
pub struct GpuAdapterInfo {
    pub name: String,
    pub backend: String,
    pub device_type: String,
    pub vendor_id: u32,
    pub device_id: u32,
    pub driver: String,
    pub features: Vec<String>,
    pub limits_summary: String,
}

impl GpuAdapterInfo {
    /// Render as a compact JSON object literal (no allocation pool — built
    /// from `format!` which is fine at 1Hz / once-per-init rates).
    #[must_use]
    pub fn to_json_string(&self) -> String {
        let features = self
            .features
            .iter()
            .map(|f| format!("\"{}\"", json_escape(f)))
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{{\"name\":\"{}\",\"backend\":\"{}\",\"device_type\":\"{}\",\
             \"vendor_id\":{},\"device_id\":{},\"driver\":\"{}\",\
             \"features\":[{}],\"limits_summary\":\"{}\"}}",
            json_escape(&self.name),
            json_escape(&self.backend),
            json_escape(&self.device_type),
            self.vendor_id,
            self.device_id,
            json_escape(&self.driver),
            features,
            json_escape(&self.limits_summary),
        )
    }
}

// ───────────────────────────────────────────────────────────────────────
// § FrameToken — RAII token returned by `frame_begin`.
// ───────────────────────────────────────────────────────────────────────

/// Token returned from `TelemetrySink::frame_begin`. Carries the start
/// instant so `frame_end` can compute dt without a second clock read.
#[derive(Debug, Clone, Copy)]
pub struct FrameToken {
    pub start: Instant,
    pub frame_index: u64,
}

// ───────────────────────────────────────────────────────────────────────
// § Histogram buckets
// ───────────────────────────────────────────────────────────────────────

/// Frame-time bucket boundaries (in milliseconds).
///
/// `BUCKET_COUNT == BUCKET_BOUNDS.len() + 1` (the trailing bucket catches
/// frame-times ≥ 250ms, which is "your engine is in trouble" territory).
pub const BUCKET_BOUNDS_MS: &[f32] = &[1.0, 2.0, 4.0, 8.0, 16.0, 33.0, 66.0, 100.0, 250.0];

/// Total number of histogram buckets (9 boundaries → 10 buckets including
/// "≥ 250ms" tail).
pub const BUCKET_COUNT: usize = 10;

/// Compute which bucket a sample falls into.
///
/// Returns a value in `0..BUCKET_COUNT`. Negative + NaN samples land in
/// bucket 0 (degenerate ; the caller should already have clamped).
#[must_use]
pub fn bucket_for_ms(sample_ms: f32) -> usize {
    if !sample_ms.is_finite() || sample_ms < 0.0 {
        return 0;
    }
    for (i, &bound) in BUCKET_BOUNDS_MS.iter().enumerate() {
        if sample_ms < bound {
            return i;
        }
    }
    BUCKET_COUNT - 1
}

// ───────────────────────────────────────────────────────────────────────
// § RingBuffer — fixed-capacity `Vec<f32>` with O(1) overwrite-on-full.
// ───────────────────────────────────────────────────────────────────────

/// Fixed-capacity ring of frame-time samples. Capacity is set at construction
/// time so we can avoid the `const-generic` syntax-sugar rabbit hole.
#[derive(Debug)]
pub struct FrameTimeRing {
    pub samples: Vec<f32>,
    pub capacity: usize,
    pub head: usize,
}

impl FrameTimeRing {
    /// Construct an empty ring of the given capacity.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            samples: Vec::with_capacity(capacity),
            capacity,
            head: 0,
        }
    }

    /// Append a sample, overwriting the oldest if full.
    pub fn push(&mut self, sample_ms: f32) {
        if self.samples.len() < self.capacity {
            self.samples.push(sample_ms);
        } else {
            self.samples[self.head] = sample_ms;
            self.head = (self.head + 1) % self.capacity;
        }
    }

    /// Snapshot the current contents (oldest-first).
    #[must_use]
    pub fn snapshot(&self) -> Vec<f32> {
        if self.samples.len() < self.capacity {
            self.samples.clone()
        } else {
            let mut out = Vec::with_capacity(self.capacity);
            out.extend_from_slice(&self.samples[self.head..]);
            out.extend_from_slice(&self.samples[..self.head]);
            out
        }
    }

    /// Compute the 10-bucket histogram count vector.
    #[must_use]
    pub fn histogram(&self) -> [u32; BUCKET_COUNT] {
        let mut counts = [0u32; BUCKET_COUNT];
        for &s in &self.samples {
            counts[bucket_for_ms(s)] = counts[bucket_for_ms(s)].saturating_add(1);
        }
        counts
    }

    /// Compute (P50, P95, P99) in ms. Returns (0.0, 0.0, 0.0) when empty.
    #[must_use]
    pub fn percentiles(&self) -> (f32, f32, f32) {
        if self.samples.is_empty() {
            return (0.0, 0.0, 0.0);
        }
        let mut sorted: Vec<f32> = self.samples.iter().copied().collect();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let n = sorted.len();
        // Nearest-rank percentile (no interpolation — adequate for a P50/P95/P99
        // dashboard signal).
        let p_at = |p: f32| -> f32 {
            let idx = ((p / 100.0) * n as f32).ceil() as usize;
            let idx = idx.saturating_sub(1).min(n - 1);
            sorted[idx]
        };
        (p_at(50.0), p_at(95.0), p_at(99.0))
    }
}

// ───────────────────────────────────────────────────────────────────────
// § TelemetrySink — the global instance lives behind a `OnceLock`.
// ───────────────────────────────────────────────────────────────────────

/// Capacity of the frame-time ring (= ~17 seconds at 60Hz).
pub const FRAME_RING_CAP: usize = 1024;

/// CSV-emit cadence in milliseconds. 1Hz keeps file-size manageable
/// (~30 rows / 30 seconds → < 4KB) while still resolving spikes.
pub const CSV_EMIT_INTERVAL_MS: u64 = 1000;

/// In-memory ring of recent JSONL events (for `telemetry.tail_events`).
/// Matches the on-disk JSONL contents but lets MCP clients query without
/// re-reading the file.
pub const EVENTS_RING_CAP: usize = 1024;

/// The structured-telemetry sink. One instance per process, lazily created.
pub struct TelemetrySink {
    // § Counters (atomic for lock-free hot path) ──────────────────────
    pub frame_count: AtomicU64,
    pub draw_calls_total: AtomicU64,
    pub vertices_drawn_total: AtomicU64,
    pub pipeline_switches_total: AtomicU64,
    pub mcp_calls_total: AtomicU64,
    pub dm_events_total: AtomicU64,

    // § T11-LOA-FID-MAINSTREAM : per-frame fidelity counters (microseconds
    // for the most-recent frame ; cheap overwriting writers — no rolling
    // average, just a live last-value snapshot).
    pub gpu_resolve_us: AtomicU64,
    pub tonemap_us: AtomicU64,

    // § T11-LOA-FID-STOKES : per-frame Mueller-apply count + DOP avg/max.
    /// Total Mueller-matrix applications across all frames since startup.
    pub mueller_applies_total: AtomicU64,
    /// Last frame's mueller_apply_count (snapshot, reset per frame).
    pub mueller_apply_count_per_frame: AtomicU32,
    /// Last frame's DOP average in Q14 fixed-point.
    pub dop_avg_per_frame_q14: AtomicU32,
    /// Last frame's DOP max in Q14 fixed-point.
    pub dop_max_per_frame_q14: AtomicU32,

    // § T11-LOA-USERFIX : F-key + capture telemetry.
    /// Total render-mode changes (F1-F10 presses) since startup.
    pub render_mode_changes_total: AtomicU64,
    /// Total single-frame screenshots captured (F12 + MCP `render.snapshot_png`).
    pub screenshot_captures_total: AtomicU64,
    /// Total video frames recorded (each F8 record-session adds N frames).
    pub video_frames_recorded_total: AtomicU64,
    /// Current CFER atmospheric intensity in Q14 fixed-point (0..16383 = 0..1).
    /// Updated whenever the host or MCP changes the intensity.
    pub cfer_intensity_current_q14: AtomicU32,
    /// Total burst-capture sequences started (each F9 starts one burst).
    pub burst_captures_total: AtomicU64,

    // § T11-WAVE3-GLTF : per-spawn counters for external 3D-model imports.
    /// Total successful glTF / GLB spawns since startup.
    pub gltf_spawns_total: AtomicU64,
    /// Total rejected glTF / GLB spawns (parse-fail · cap-mismatch · over-cap).
    pub gltf_spawns_rejected_total: AtomicU64,
    /// Total dynamic-mesh draw-calls issued (rolls up across all spawned meshes).
    pub gltf_draws_total: AtomicU64,

    // § Sliding-window percentile snapshots (Q14 fixed-point milliseconds)
    pub last_p50_q14: AtomicU32,
    pub last_p95_q14: AtomicU32,
    pub last_p99_q14: AtomicU32,

    // § Last CSV-emit timestamp (Unix ms ; 0 = never).
    pub last_csv_ts_ms: AtomicU64,

    // § Process start time (Unix ms ; for uptime computation).
    pub start_ts_ms: AtomicU64,

    // § Disable flag (set when CSV/JSONL paths cannot be opened).
    pub disabled: AtomicBool,

    // § Frame-time ring buffer (Mutex-guarded ; only locked for tiny
    //   bounded windows — sample push or snapshot).
    pub frame_ring: Mutex<FrameTimeRing>,

    // § GPU info captured once at init.
    pub gpu_info: Mutex<Option<GpuAdapterInfo>>,

    // § Recent JSONL events (in-memory ring) for `telemetry.tail_events`.
    pub events_ring: Mutex<Vec<String>>,

    // § Output paths.
    pub csv_path: PathBuf,
    pub jsonl_path: PathBuf,

    // § Log-level threshold (DEBUG/INFO/WARN/ERROR ⇒ 0/1/2/3).
    pub log_level: AtomicU32,
}

/// Q14 fixed-point conversion scale (0..=2^14 ⇒ 0..1.0).
const Q14_SCALE: f32 = 16384.0;

impl TelemetrySink {
    /// Construct a fresh sink targeting `<log_dir>/loa_telemetry.csv` +
    /// `<log_dir>/loa_events.jsonl`. Creates the log dir if needed ;
    /// flips `disabled = true` if the directory can't be reached.
    #[must_use]
    pub fn new(log_dir: PathBuf) -> Self {
        let mut disabled = false;
        if let Err(_e) = fs::create_dir_all(&log_dir) {
            disabled = true;
        }
        let csv_path = log_dir.join("loa_telemetry.csv");
        let jsonl_path = log_dir.join("loa_events.jsonl");

        // Write CSV header if file doesn't exist or is empty. Append mode
        // afterwards so we keep prior runs' history (until a sovereign
        // operator truncates).
        if !disabled {
            let header_needed = match fs::metadata(&csv_path) {
                Ok(m) => m.len() == 0,
                Err(_) => true,
            };
            if header_needed {
                if let Ok(mut f) = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&csv_path)
                {
                    let _ = writeln!(
                        f,
                        "ts,frame_count,fps,p50_ms,p95_ms,p99_ms,draw_calls,vertices,pipeline_switches,mcp_calls,dm_events"
                    );
                }
            }
        }

        let now_ms = unix_ms();

        Self {
            frame_count: AtomicU64::new(0),
            draw_calls_total: AtomicU64::new(0),
            vertices_drawn_total: AtomicU64::new(0),
            pipeline_switches_total: AtomicU64::new(0),
            mcp_calls_total: AtomicU64::new(0),
            dm_events_total: AtomicU64::new(0),
            gpu_resolve_us: AtomicU64::new(0),
            tonemap_us: AtomicU64::new(0),
            mueller_applies_total: AtomicU64::new(0),
            mueller_apply_count_per_frame: AtomicU32::new(0),
            dop_avg_per_frame_q14: AtomicU32::new(0),
            dop_max_per_frame_q14: AtomicU32::new(0),
            render_mode_changes_total: AtomicU64::new(0),
            screenshot_captures_total: AtomicU64::new(0),
            video_frames_recorded_total: AtomicU64::new(0),
            // Default intensity 0.10 → Q14 = 1638.
            cfer_intensity_current_q14: AtomicU32::new(1638),
            burst_captures_total: AtomicU64::new(0),
            // § T11-WAVE3-GLTF : start at zero · ffi::spawn_gltf_path
            // increments on every successful import.
            gltf_spawns_total: AtomicU64::new(0),
            gltf_spawns_rejected_total: AtomicU64::new(0),
            gltf_draws_total: AtomicU64::new(0),
            last_p50_q14: AtomicU32::new(0),
            last_p95_q14: AtomicU32::new(0),
            last_p99_q14: AtomicU32::new(0),
            last_csv_ts_ms: AtomicU64::new(0),
            start_ts_ms: AtomicU64::new(now_ms),
            disabled: AtomicBool::new(disabled),
            frame_ring: Mutex::new(FrameTimeRing::new(FRAME_RING_CAP)),
            gpu_info: Mutex::new(None),
            events_ring: Mutex::new(Vec::with_capacity(EVENTS_RING_CAP)),
            csv_path,
            jsonl_path,
            log_level: AtomicU32::new(1), // INFO default
        }
    }

    /// Mark the start of a frame. Returns a token to pass to `frame_end`.
    #[must_use]
    pub fn frame_begin(&self) -> FrameToken {
        let frame_index = self.frame_count.load(Ordering::Relaxed);
        FrameToken {
            start: Instant::now(),
            frame_index,
        }
    }

    /// Mark the end of a frame. Records dt + draw-call/vertex counts +
    /// (every 60 frames) recomputes percentiles + (every 1Hz wall-clock)
    /// flushes a CSV row.
    pub fn frame_end(&self, token: FrameToken, draw_calls: u32, vertices: u64) {
        let dt_ms = token.start.elapsed().as_secs_f32() * 1000.0;
        let frame_n = self.frame_count.fetch_add(1, Ordering::Relaxed) + 1;
        self.draw_calls_total
            .fetch_add(u64::from(draw_calls), Ordering::Relaxed);
        self.vertices_drawn_total
            .fetch_add(vertices, Ordering::Relaxed);

        // Push to ring (brief lock).
        if let Ok(mut ring) = self.frame_ring.lock() {
            ring.push(dt_ms);
            // Recompute percentiles every 60 frames.
            if frame_n % 60 == 0 {
                let (p50, p95, p99) = ring.percentiles();
                self.last_p50_q14
                    .store(ms_to_q14(p50), Ordering::Relaxed);
                self.last_p95_q14
                    .store(ms_to_q14(p95), Ordering::Relaxed);
                self.last_p99_q14
                    .store(ms_to_q14(p99), Ordering::Relaxed);
            }
        }

        // 1Hz CSV emit.
        let now_ms = unix_ms();
        let last_csv = self.last_csv_ts_ms.load(Ordering::Relaxed);
        if now_ms.saturating_sub(last_csv) >= CSV_EMIT_INTERVAL_MS {
            // CAS to single-writer the emit ; if another thread won, skip.
            if self
                .last_csv_ts_ms
                .compare_exchange(last_csv, now_ms, Ordering::AcqRel, Ordering::Relaxed)
                .is_ok()
            {
                self.emit_csv_row(now_ms, dt_ms);
            }
        }

        // Append per-frame structured event (lightweight ; only every 60 frames
        // to keep JSONL volume reasonable).
        if frame_n % 60 == 0 {
            let evt = format!(
                "{{\"ts\":\"{}\",\"kind\":\"render_frame\",\"frame\":{},\"dt_ms\":{:.3},\"draw_calls\":{},\"vertices\":{}}}",
                iso_utc(now_ms),
                frame_n,
                dt_ms,
                draw_calls,
                vertices,
            );
            self.append_jsonl(&evt);
        }
    }

    /// Record a pipeline-switch (called by the renderer).
    pub fn record_pipeline_switch(&self) {
        self.pipeline_switches_total
            .fetch_add(1, Ordering::Relaxed);
    }

    /// § T11-LOA-FID-MAINSTREAM : record the most-recent frame's GPU
    /// MSAA-resolve elapsed time in microseconds. Live overwrite, no
    /// histogram (the value is just for last-frame display + 1Hz CSV).
    pub fn record_gpu_resolve_us(&self, us: u64) {
        self.gpu_resolve_us.store(us, Ordering::Relaxed);
    }

    /// § T11-LOA-FID-MAINSTREAM : record the most-recent frame's tonemap
    /// pass elapsed time in microseconds.
    pub fn record_tonemap_us(&self, us: u64) {
        self.tonemap_us.store(us, Ordering::Relaxed);
    }

    /// Record an MCP-tool invocation (called by the server).
    pub fn record_mcp_call(&self, tool: &str, latency_us: u64, caller: &str) {
        self.mcp_calls_total.fetch_add(1, Ordering::Relaxed);
        let evt = format!(
            "{{\"ts\":\"{}\",\"kind\":\"mcp_tool_invoked\",\"tool\":\"{}\",\"latency_us\":{},\"caller\":\"{}\"}}",
            iso_utc(unix_ms()),
            json_escape(tool),
            latency_us,
            json_escape(caller),
        );
        self.append_jsonl(&evt);
    }

    /// Record a DM-state-transition event.
    pub fn record_dm_transition(&self, from: &str, to: &str, tension: f32) {
        self.dm_events_total.fetch_add(1, Ordering::Relaxed);
        let evt = format!(
            "{{\"ts\":\"{}\",\"kind\":\"dm_state_transition\",\"from\":\"{}\",\"to\":\"{}\",\"tension\":{:.3}}}",
            iso_utc(unix_ms()),
            json_escape(from),
            json_escape(to),
            tension,
        );
        self.append_jsonl(&evt);
    }

    /// § T11-LOA-USERFIX : record a render-mode change (F-key direct apply
    /// or MCP `render.set_mode`). Increments the lifetime counter ; the
    /// caller should also append a JSONL event for the timeline.
    pub fn record_render_mode_change(&self, new_mode: u8) {
        self.render_mode_changes_total
            .fetch_add(1, Ordering::Relaxed);
        let evt = format!(
            "{{\"ts\":\"{}\",\"kind\":\"render_mode_change\",\"new_mode\":{}}}",
            iso_utc(unix_ms()),
            new_mode,
        );
        self.append_jsonl(&evt);
    }

    /// § T11-LOA-USERFIX : record a single screenshot capture.
    pub fn record_screenshot_capture(&self) {
        self.screenshot_captures_total
            .fetch_add(1, Ordering::Relaxed);
    }

    /// § T11-LOA-USERFIX : record a burst-capture sequence start.
    pub fn record_burst_capture_start(&self, frame_count: u32) {
        self.burst_captures_total.fetch_add(1, Ordering::Relaxed);
        let evt = format!(
            "{{\"ts\":\"{}\",\"kind\":\"burst_capture_start\",\"frame_count\":{}}}",
            iso_utc(unix_ms()),
            frame_count,
        );
        self.append_jsonl(&evt);
    }

    /// § T11-LOA-USERFIX : record a single video-frame written to disk.
    pub fn record_video_frame(&self) {
        self.video_frames_recorded_total
            .fetch_add(1, Ordering::Relaxed);
    }

    /// § T11-WAVE3-GLTF : record a successful glTF / GLB spawn. Bumps
    /// `gltf_spawns_total` and emits a JSONL event with the per-spawn
    /// metadata so the user can audit which assets were loaded.
    pub fn record_gltf_spawn(&self, instance_id: u32, verts: u32, tris: u32, mat_id: u32) {
        self.gltf_spawns_total.fetch_add(1, Ordering::Relaxed);
        let evt = format!(
            "{{\"ts\":\"{}\",\"kind\":\"gltf_spawn\",\"instance_id\":{},\"verts\":{},\"tris\":{},\"material_id\":{}}}",
            iso_utc(unix_ms()),
            instance_id,
            verts,
            tris,
            mat_id,
        );
        self.append_jsonl(&evt);
    }

    /// § T11-WAVE3-GLTF : record a rejected glTF / GLB spawn (parse fail,
    /// cap-mismatch, OOM-cap, etc.). The `reason` is a short tag for the
    /// telemetry event (full message lives in the structured-event log).
    pub fn record_gltf_spawn_reject(&self, reason: &str) {
        self.gltf_spawns_rejected_total
            .fetch_add(1, Ordering::Relaxed);
        let evt = format!(
            "{{\"ts\":\"{}\",\"kind\":\"gltf_spawn_reject\",\"reason\":\"{}\"}}",
            iso_utc(unix_ms()),
            reason,
        );
        self.append_jsonl(&evt);
    }

    /// § T11-WAVE3-GLTF : record dynamic-mesh draw-calls issued this frame.
    /// One increment per draw-indexed call against a `DynamicMesh` slot.
    pub fn record_gltf_draws(&self, count: u32) {
        self.gltf_draws_total
            .fetch_add(u64::from(count), Ordering::Relaxed);
    }

    /// § T11-LOA-USERFIX : update the live CFER intensity gauge.
    /// `intensity` is clamped to 0..1 and stored as Q14 fixed-point.
    pub fn record_cfer_intensity(&self, intensity: f32) {
        let clamped = intensity.clamp(0.0, 1.0);
        let q14 = (clamped * 16383.0) as u32;
        self.cfer_intensity_current_q14
            .store(q14, Ordering::Relaxed);
    }

    /// § T11-LOA-FID-STOKES : record a per-frame Mueller-apply roll-up.
    ///
    /// Updates the per-frame snapshot counters AND the cumulative total.
    /// `dop_avg_q14` + `dop_max_q14` are Q14 fixed-point representations of
    /// the average + maximum degree-of-polarization observed during the
    /// frame (0..16383 → 0.0..1.0).
    pub fn record_stokes_frame(&self, applies: u32, dop_avg_q14: u32, dop_max_q14: u32) {
        self.mueller_apply_count_per_frame
            .store(applies, Ordering::Relaxed);
        self.dop_avg_per_frame_q14
            .store(dop_avg_q14, Ordering::Relaxed);
        self.dop_max_per_frame_q14
            .store(dop_max_q14, Ordering::Relaxed);
        self.mueller_applies_total
            .fetch_add(u64::from(applies), Ordering::Relaxed);
    }

    /// Capture GPU adapter info (called once at GPU init).
    pub fn record_gpu_info(&self, info: GpuAdapterInfo) {
        let evt = format!(
            "{{\"ts\":\"{}\",\"kind\":\"gpu_adapter_info\",\"info\":{}}}",
            iso_utc(unix_ms()),
            info.to_json_string(),
        );
        self.append_jsonl(&evt);
        if let Ok(mut g) = self.gpu_info.lock() {
            *g = Some(info);
        }
    }

    /// Snapshot current counters as CSV row text (for tests + MCP tool).
    #[must_use]
    pub fn snapshot_csv_row(&self) -> String {
        let now = unix_ms();
        let frames = self.frame_count.load(Ordering::Relaxed);
        let uptime_ms = now.saturating_sub(self.start_ts_ms.load(Ordering::Relaxed));
        let fps = if uptime_ms > 0 {
            (frames as f64 * 1000.0 / uptime_ms as f64) as f32
        } else {
            0.0
        };
        let (p50, p95, p99) = self.cached_percentiles();
        format!(
            "{},{},{:.2},{:.3},{:.3},{:.3},{},{},{},{},{}",
            iso_utc(now),
            frames,
            fps,
            p50,
            p95,
            p99,
            self.draw_calls_total.load(Ordering::Relaxed),
            self.vertices_drawn_total.load(Ordering::Relaxed),
            self.pipeline_switches_total.load(Ordering::Relaxed),
            self.mcp_calls_total.load(Ordering::Relaxed),
            self.dm_events_total.load(Ordering::Relaxed),
        )
    }

    /// Snapshot the full sink as a JSON object string (for `telemetry.snapshot`).
    #[must_use]
    pub fn snapshot_json(&self) -> String {
        let now = unix_ms();
        let frames = self.frame_count.load(Ordering::Relaxed);
        let uptime_ms = now.saturating_sub(self.start_ts_ms.load(Ordering::Relaxed));
        let fps = if uptime_ms > 0 {
            (frames as f64 * 1000.0 / uptime_ms as f64) as f32
        } else {
            0.0
        };
        let (p50, p95, p99) = self.cached_percentiles();
        let buckets = self.frame_time_histogram();
        let buckets_str = buckets
            .iter()
            .map(|c| c.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let dop_avg = q14_to_dop(self.dop_avg_per_frame_q14.load(Ordering::Relaxed));
        let dop_max = q14_to_dop(self.dop_max_per_frame_q14.load(Ordering::Relaxed));
        let cfer_intensity = q14_to_dop(
            self.cfer_intensity_current_q14.load(Ordering::Relaxed),
        );
        format!(
            "{{\"ts\":\"{}\",\"uptime_ms\":{},\"frame_count\":{},\"fps\":{:.2},\
             \"p50_ms\":{:.3},\"p95_ms\":{:.3},\"p99_ms\":{:.3},\
             \"draw_calls_total\":{},\"vertices_drawn_total\":{},\
             \"pipeline_switches_total\":{},\"mcp_calls_total\":{},\
             \"dm_events_total\":{},\"gpu_resolve_us\":{},\"tonemap_us\":{},\
             \"histogram\":[{}],\"log_level\":{},\
             \"mueller_applies_total\":{},\"mueller_apply_count_per_frame\":{},\
             \"dop_avg_per_frame\":{:.4},\"dop_max_per_frame\":{:.4},\
             \"render_mode_changes_total\":{},\"screenshot_captures_total\":{},\
             \"video_frames_recorded_total\":{},\"burst_captures_total\":{},\
             \"cfer_intensity\":{:.4}}}",
            iso_utc(now),
            uptime_ms,
            frames,
            fps,
            p50,
            p95,
            p99,
            self.draw_calls_total.load(Ordering::Relaxed),
            self.vertices_drawn_total.load(Ordering::Relaxed),
            self.pipeline_switches_total.load(Ordering::Relaxed),
            self.mcp_calls_total.load(Ordering::Relaxed),
            self.dm_events_total.load(Ordering::Relaxed),
            self.gpu_resolve_us.load(Ordering::Relaxed),
            self.tonemap_us.load(Ordering::Relaxed),
            buckets_str,
            self.log_level.load(Ordering::Relaxed),
            self.mueller_applies_total.load(Ordering::Relaxed),
            self.mueller_apply_count_per_frame.load(Ordering::Relaxed),
            dop_avg,
            dop_max,
            self.render_mode_changes_total.load(Ordering::Relaxed),
            self.screenshot_captures_total.load(Ordering::Relaxed),
            self.video_frames_recorded_total.load(Ordering::Relaxed),
            self.burst_captures_total.load(Ordering::Relaxed),
            cfer_intensity,
        )
    }

    /// Return the frame-time histogram as a `[u32; 10]` of bucket counts.
    #[must_use]
    pub fn frame_time_histogram(&self) -> [u32; BUCKET_COUNT] {
        match self.frame_ring.lock() {
            Ok(r) => r.histogram(),
            Err(_) => [0; BUCKET_COUNT],
        }
    }

    /// Return the cached (P50, P95, P99) in ms.
    #[must_use]
    pub fn cached_percentiles(&self) -> (f32, f32, f32) {
        (
            q14_to_ms(self.last_p50_q14.load(Ordering::Relaxed)),
            q14_to_ms(self.last_p95_q14.load(Ordering::Relaxed)),
            q14_to_ms(self.last_p99_q14.load(Ordering::Relaxed)),
        )
    }

    /// Return the captured GPU info as a JSON-string fragment, or `null`
    /// if not yet captured.
    #[must_use]
    pub fn gpu_info_json(&self) -> String {
        match self.gpu_info.lock() {
            Ok(g) => g
                .as_ref()
                .map(GpuAdapterInfo::to_json_string)
                .unwrap_or_else(|| "null".to_string()),
            Err(_) => "null".to_string(),
        }
    }

    /// Return up to `limit` most-recent JSONL events as a JSON array.
    #[must_use]
    pub fn tail_events_json(&self, limit: usize) -> String {
        let take = limit.min(EVENTS_RING_CAP);
        let events = match self.events_ring.lock() {
            Ok(r) => {
                let n = r.len();
                let from = n.saturating_sub(take);
                r[from..].to_vec()
            }
            Err(_) => Vec::new(),
        };
        format!("[{}]", events.join(","))
    }

    /// Set the log level (DEBUG=0, INFO=1, WARN=2, ERROR=3).
    pub fn set_log_level(&self, level: u32) {
        self.log_level.store(level.min(3), Ordering::Relaxed);
    }

    /// Force-flush CSV + JSONL files. Returns Ok(()) on success.
    pub fn flush(&self) -> std::io::Result<()> {
        if self.disabled.load(Ordering::Relaxed) {
            return Ok(());
        }
        // Emit a current snapshot row immediately.
        self.emit_csv_row(unix_ms(), 0.0);
        Ok(())
    }

    // § ────────────────────── private helpers ──────────────────────

    fn emit_csv_row(&self, _now_ms: u64, _last_dt_ms: f32) {
        if self.disabled.load(Ordering::Relaxed) {
            return;
        }
        let row = self.snapshot_csv_row();
        if let Ok(mut f) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.csv_path)
        {
            let _ = writeln!(f, "{row}");
        }
    }

    fn append_jsonl(&self, line: &str) {
        if self.disabled.load(Ordering::Relaxed) {
            return;
        }
        // Append to disk.
        if let Ok(mut f) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.jsonl_path)
        {
            let _ = writeln!(f, "{line}");
        }
        // Append to in-memory ring (bounded).
        if let Ok(mut r) = self.events_ring.lock() {
            if r.len() >= EVENTS_RING_CAP {
                r.remove(0);
            }
            r.push(line.to_string());
        }
    }
}

// ───────────────────────────────────────────────────────────────────────
// § Global instance + helpers
// ───────────────────────────────────────────────────────────────────────

static GLOBAL_SINK: OnceLock<TelemetrySink> = OnceLock::new();

/// Get (lazy-init) the global telemetry sink. The sink writes to
/// `<CSSL_LOG_DIR or 'logs'>/loa_telemetry.csv` + `loa_events.jsonl`.
pub fn global() -> &'static TelemetrySink {
    GLOBAL_SINK.get_or_init(|| {
        let dir = std::env::var("CSSL_LOG_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("logs"));
        TelemetrySink::new(dir)
    })
}

/// Convenience : record a DM-state transition via the global sink.
pub fn record_dm_transition(from: &str, to: &str, tension: f32) {
    global().record_dm_transition(from, to, tension);
}

/// Convenience : record an MCP tool-invocation via the global sink.
pub fn record_mcp_call(tool: &str, latency_us: u64, caller: &str) {
    global().record_mcp_call(tool, latency_us, caller);
}

/// Convenience : record GPU-adapter info via the global sink.
pub fn record_gpu_info(info: GpuAdapterInfo) {
    global().record_gpu_info(info);
}

// ───────────────────────────────────────────────────────────────────────
// § Q14 fixed-point + ISO timestamp helpers
// ───────────────────────────────────────────────────────────────────────

/// Convert ms (f32, 0..2^18) → Q14 (u32). Saturates at u32::MAX/Q14_SCALE.
fn ms_to_q14(ms: f32) -> u32 {
    if !ms.is_finite() || ms < 0.0 {
        return 0;
    }
    let scaled = ms * Q14_SCALE;
    if scaled >= u32::MAX as f32 {
        u32::MAX
    } else {
        scaled as u32
    }
}

/// Convert Q14 (u32) → ms (f32).
fn q14_to_ms(q: u32) -> f32 {
    q as f32 / Q14_SCALE
}

/// § T11-LOA-FID-STOKES : convert Q14 DOP fixed-point (0..16383 = 0.0..1.0)
/// back to f32 in 0.0..1.0.
fn q14_to_dop(q: u32) -> f32 {
    (q as f32) / 16383.0
}

/// Current Unix timestamp in milliseconds.
fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Format a Unix-ms timestamp as ISO-UTC (`YYYY-MM-DDTHH:MM:SSZ`).
fn iso_utc(unix_ms_v: u64) -> String {
    let secs = unix_ms_v / 1000;
    let days = secs / 86_400;
    let hms = secs % 86_400;
    let h = hms / 3600;
    let m = (hms % 3600) / 60;
    let s = hms % 60;
    let (y, mo, d) = days_to_ymd(days as i64);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{m:02}:{s:02}Z")
}

/// Civil-from-days (Howard-Hinnant algorithm) — matches loa_startup.rs.
fn days_to_ymd(mut days: i64) -> (i32, u32, u32) {
    days += 719_468;
    let era = if days >= 0 { days / 146_097 } else { (days - 146_096) / 146_097 };
    let doe = (days - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp.wrapping_sub(9) };
    let year = (y + i64::from(m <= 2)) as i32;
    (year, m as u32, d as u32)
}

/// Minimal JSON-string escaping : `\` ⇒ `\\`, `"` ⇒ `\"`, control chars
/// ⇒ `\uXXXX`. Sufficient for the limited input shapes (tool names,
/// adapter strings, error messages) the telemetry pipeline emits.
fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

// ═══════════════════════════════════════════════════════════════════════
// § TESTS
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn fresh_sink() -> TelemetrySink {
        // Use a unique tmp-dir per test to avoid CSV cross-pollution.
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "loa-telem-test-{}-{}",
            std::process::id(),
            unix_ms()
        ));
        TelemetrySink::new(dir)
    }

    #[test]
    fn telemetry_frame_begin_end_increments_counter() {
        let s = fresh_sink();
        assert_eq!(s.frame_count.load(Ordering::Relaxed), 0);
        let t = s.frame_begin();
        std::thread::sleep(Duration::from_millis(2));
        s.frame_end(t, 5, 1024);
        assert_eq!(s.frame_count.load(Ordering::Relaxed), 1);
        assert_eq!(s.draw_calls_total.load(Ordering::Relaxed), 5);
        assert_eq!(s.vertices_drawn_total.load(Ordering::Relaxed), 1024);
    }

    #[test]
    fn telemetry_p50_p95_p99_computed_correctly_for_known_distribution() {
        let mut ring = FrameTimeRing::new(100);
        // 50 frames @ 16.7ms, 45 frames @ 33ms, 4 frames @ 60ms, 1 frame @ 120ms
        for _ in 0..50 {
            ring.push(16.7);
        }
        for _ in 0..45 {
            ring.push(33.0);
        }
        for _ in 0..4 {
            ring.push(60.0);
        }
        ring.push(120.0);
        let (p50, p95, p99) = ring.percentiles();
        // P50 lands in the 16.7 cluster.
        assert!((p50 - 16.7).abs() < 0.5, "p50={p50}");
        // P95 lands in the 33ms cluster (95th of 100 = idx 94 = 33ms).
        assert!((p95 - 33.0).abs() < 0.5, "p95={p95}");
        // P99 lands at the 60ms tail.
        assert!(p99 >= 60.0 - 0.5, "p99={p99}");
    }

    #[test]
    fn telemetry_csv_format_has_required_columns() {
        let s = fresh_sink();
        let row = s.snapshot_csv_row();
        // 11 commas-separated columns : ts,frame_count,fps,p50,p95,p99,
        // draw_calls,vertices,pipeline_switches,mcp_calls,dm_events.
        let cols: Vec<&str> = row.split(',').collect();
        assert_eq!(cols.len(), 11, "row='{row}'");
        // First column must be ISO-UTC.
        assert!(cols[0].len() == 20 && cols[0].ends_with('Z'));
    }

    #[test]
    fn telemetry_jsonl_event_serialization_roundtrip() {
        let s = fresh_sink();
        s.record_dm_transition("CALM", "BUILDUP", 0.42);
        let tail = s.tail_events_json(10);
        assert!(tail.contains("\"kind\":\"dm_state_transition\""));
        assert!(tail.contains("\"from\":\"CALM\""));
        assert!(tail.contains("\"to\":\"BUILDUP\""));
        assert!(tail.contains("0.420"));
    }

    #[test]
    fn telemetry_histogram_bucketization_correct() {
        // Boundary checks : 0.5 → bucket 0, 1.5 → bucket 1, 16.7 → bucket 5.
        assert_eq!(bucket_for_ms(0.5), 0);
        assert_eq!(bucket_for_ms(1.5), 1);
        assert_eq!(bucket_for_ms(3.0), 2);
        assert_eq!(bucket_for_ms(7.0), 3);
        assert_eq!(bucket_for_ms(16.7), 5);
        assert_eq!(bucket_for_ms(50.0), 6);
        assert_eq!(bucket_for_ms(80.0), 7);
        assert_eq!(bucket_for_ms(150.0), 8);
        assert_eq!(bucket_for_ms(500.0), 9);
        // Negative + NaN saturate to 0.
        assert_eq!(bucket_for_ms(-5.0), 0);
        assert_eq!(bucket_for_ms(f32::NAN), 0);
    }

    #[test]
    fn auto_rotate_triggers_at_10mb_threshold() {
        // Synthetic file : write 10MB+1 bytes then call should_rotate.
        let mut path = std::env::temp_dir();
        path.push(format!("loa-rotate-test-{}.log", std::process::id()));
        {
            let mut f = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&path)
                .unwrap();
            // 10MB + 1.
            let buf = vec![b'.'; 1024 * 1024];
            for _ in 0..10 {
                f.write_all(&buf).unwrap();
            }
            f.write_all(b"X").unwrap();
        }
        let len = fs::metadata(&path).unwrap().len();
        assert!(len > 10 * 1024 * 1024);
        // Cleanup.
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn mcp_telemetry_snapshot_returns_valid_json() {
        let s = fresh_sink();
        let t = s.frame_begin();
        s.frame_end(t, 3, 600);
        let json = s.snapshot_json();
        // Quick shape-check : must contain key fields.
        assert!(json.starts_with("{"));
        assert!(json.ends_with("}"));
        assert!(json.contains("\"frame_count\":1"));
        assert!(json.contains("\"draw_calls_total\":3"));
        assert!(json.contains("\"vertices_drawn_total\":600"));
        assert!(json.contains("\"histogram\":["));
    }

    #[test]
    fn gpu_adapter_info_serialization_includes_name_backend() {
        let info = GpuAdapterInfo {
            name: "Intel Arc A770".to_string(),
            backend: "Vulkan".to_string(),
            device_type: "DiscreteGpu".to_string(),
            vendor_id: 0x8086,
            device_id: 0x56A0,
            driver: "31.0.101.5333".to_string(),
            features: vec!["TIMESTAMP_QUERY".to_string()],
            limits_summary: "max_uniform=64KB".to_string(),
        };
        let s = info.to_json_string();
        assert!(s.contains("\"name\":\"Intel Arc A770\""));
        assert!(s.contains("\"backend\":\"Vulkan\""));
        assert!(s.contains("\"vendor_id\":32902"));
        assert!(s.contains("TIMESTAMP_QUERY"));
    }

    #[test]
    fn pipeline_switch_counter_increments() {
        let s = fresh_sink();
        s.record_pipeline_switch();
        s.record_pipeline_switch();
        s.record_pipeline_switch();
        assert_eq!(s.pipeline_switches_total.load(Ordering::Relaxed), 3);
    }

    #[test]
    fn ringbuffer_overwrites_when_full() {
        let mut r = FrameTimeRing::new(4);
        r.push(1.0);
        r.push(2.0);
        r.push(3.0);
        r.push(4.0);
        r.push(5.0); // overwrites slot 0
        let snap = r.snapshot();
        assert_eq!(snap.len(), 4);
        // Oldest is 2.0 now (1.0 was overwritten by 5.0).
        assert!((snap[0] - 2.0).abs() < 1e-6);
        assert!((snap[3] - 5.0).abs() < 1e-6);
    }

    #[test]
    fn log_level_set_clamps_to_3() {
        let s = fresh_sink();
        s.set_log_level(99);
        assert_eq!(s.log_level.load(Ordering::Relaxed), 3);
        s.set_log_level(0);
        assert_eq!(s.log_level.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn iso_utc_format_well_formed() {
        let s = iso_utc(0);
        assert_eq!(s, "1970-01-01T00:00:00Z");
        let s = iso_utc(1_761_667_200_000); // 2025-10-28T16:00:00Z (approx)
        assert!(s.starts_with("2025-"));
        assert!(s.ends_with("Z"));
    }

    #[test]
    fn json_escape_handles_quotes_and_newlines() {
        let s = json_escape("a\"b\nc\\d");
        assert_eq!(s, "a\\\"b\\nc\\\\d");
    }

    #[test]
    fn fidelity_resolve_and_tonemap_us_recorded_and_serialized() {
        // § T11-LOA-FID-MAINSTREAM : `record_gpu_resolve_us` +
        // `record_tonemap_us` must be live-readable from `snapshot_json`.
        let s = fresh_sink();
        s.record_gpu_resolve_us(123);
        s.record_tonemap_us(456);
        assert_eq!(s.gpu_resolve_us.load(Ordering::Relaxed), 123);
        assert_eq!(s.tonemap_us.load(Ordering::Relaxed), 456);
        let json = s.snapshot_json();
        assert!(json.contains("\"gpu_resolve_us\":123"));
        assert!(json.contains("\"tonemap_us\":456"));
    }

    #[test]
    fn snapshot_json_includes_histogram_array() {
        let s = fresh_sink();
        for _ in 0..5 {
            let t = s.frame_begin();
            s.frame_end(t, 1, 10);
        }
        let json = s.snapshot_json();
        assert!(json.contains("\"histogram\":["));
        // Should have BUCKET_COUNT counts separated by commas (9 commas).
        let h_start = json.find("\"histogram\":[").unwrap() + "\"histogram\":[".len();
        let h_end = json[h_start..].find(']').unwrap() + h_start;
        let arr = &json[h_start..h_end];
        let count_commas = arr.matches(',').count();
        assert_eq!(count_commas, BUCKET_COUNT - 1);
    }
}
