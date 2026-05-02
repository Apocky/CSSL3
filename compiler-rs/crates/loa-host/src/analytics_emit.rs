//! § analytics_emit — bridge from telemetry to cssl-analytics-aggregator.
//! ════════════════════════════════════════════════════════════════════════
//!
//! § T11-W11-ANALYTICS-WIRE-PENDING
//!   This module is checked-in as a SIDECAR awaiting two follow-up wires :
//!     1. Add `cssl-analytics-aggregator = { path = "../cssl-analytics-aggregator" }`
//!        to `loa-host/Cargo.toml` `[dependencies]`
//!     2. Add `pub mod analytics_emit;` to `loa-host/src/lib.rs`
//!   Once those are in place the module compiles + tests pass. This split-
//!   commit pattern lets sibling-agent file-locks on Cargo.toml + lib.rs
//!   resolve cleanly via orchestrator merge.
//!
//! § T11-W11-ANALYTICS · loa-host-side wiring of the analytics pipeline.
//!   - Owns a `RingBuffer` for hot-path zero-alloc emission
//!   - Owns an `Aggregator` for bucketed rollups (drained periodically)
//!   - Writes JSONL to `<log_dir>/analytics.jsonl` (alongside the existing
//!     `loa_events.jsonl` written by `telemetry::TelemetrySink`)
//!   - Σ-mask : every emit takes a `ConsentCap` ; cap=Deny ⇒ silent drop
//!     before the ring touches the wire-format
//!
//! § DESIGN
//!   The hot-path callers (telemetry · ffi · intent_router · gm_narrator)
//!   call `record_*` shorthands here. The shorthands :
//!     1. Build the bit-pack `EventRecord` via the aggregator-crate factory
//!     2. Stamp consent-cap into the flag-bits
//!     3. Drop on ring-full (incrementing `dropped_total`)
//!   The drain-thread (or periodic explicit drain via MCP) reads the ring,
//!   ingests into the aggregator, and emits a JSONL line per event. This
//!   keeps the hot-path lock-free.
//!
//! § PRIME-DIRECTIVE attestation
//!   There was no hurt nor harm in the making of this, to anyone/anything/anybody.

use cssl_analytics_aggregator::{
    Aggregator, BucketTier, ConsentCap, EventRecord, RingBuffer,
};
use std::fs::{self, OpenOptions};
use std::io::Write as _;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

/// Capacity of the analytics-emit ring (events). Power-of-two ≥ 1024 ⇒
/// 1024 (matches loa_telemetry FRAME_RING_CAP).
pub const ANALYTICS_RING_CAP: usize = 1024;

/// Process-wide analytics emitter. One instance per process, lazy.
pub struct AnalyticsEmitter {
    pub ring: Mutex<RingBuffer>,
    pub aggregator: Mutex<Aggregator>,
    pub jsonl_path: PathBuf,
    pub disabled: std::sync::atomic::AtomicBool,
    pub emitted_total: AtomicU64,
    pub dropped_total: AtomicU64,
    pub session_start_frame: AtomicU64,
}

impl AnalyticsEmitter {
    /// Construct a new emitter targeting `<log_dir>/analytics.jsonl`.
    #[must_use]
    pub fn new(log_dir: PathBuf) -> Self {
        let mut disabled = false;
        if let Err(_e) = fs::create_dir_all(&log_dir) {
            disabled = true;
        }
        let jsonl_path = log_dir.join("analytics.jsonl");
        Self {
            ring: Mutex::new(RingBuffer::new(ANALYTICS_RING_CAP)),
            aggregator: Mutex::new(Aggregator::new()),
            jsonl_path,
            disabled: std::sync::atomic::AtomicBool::new(disabled),
            emitted_total: AtomicU64::new(0),
            dropped_total: AtomicU64::new(0),
            session_start_frame: AtomicU64::new(0),
        }
    }

    /// Submit an event with the given consent-cap. Cap=Deny ⇒ silent drop.
    /// Returns true if the event was admitted into the ring.
    pub fn emit(&self, mut ev: EventRecord, cap: ConsentCap) -> bool {
        if !cap.allows_local() {
            self.dropped_total.fetch_add(1, Ordering::Relaxed);
            return false;
        }
        ev.stamp_consent(cap);
        match self.ring.lock() {
            Ok(mut r) => {
                if r.push(ev) {
                    self.emitted_total.fetch_add(1, Ordering::Relaxed);
                    true
                } else {
                    self.dropped_total.fetch_add(1, Ordering::Relaxed);
                    false
                }
            }
            Err(_) => {
                self.dropped_total.fetch_add(1, Ordering::Relaxed);
                false
            }
        }
    }

    /// Drain ring → aggregator + JSONL. Called periodically by the
    /// drain-thread or explicitly via MCP. Returns count drained.
    pub fn drain(&self, max: usize) -> usize {
        let mut buf: Vec<EventRecord> = Vec::with_capacity(max.min(ANALYTICS_RING_CAP));
        let drained = match self.ring.lock() {
            Ok(mut r) => r.drain_to_vec(&mut buf, max),
            Err(_) => 0,
        };
        if drained == 0 {
            return 0;
        }
        // Ingest into aggregator.
        if let Ok(mut agg) = self.aggregator.lock() {
            for ev in &buf {
                agg.ingest(ev);
            }
        }
        // Append JSONL lines.
        if !self.disabled.load(Ordering::Relaxed) {
            if let Ok(mut f) = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.jsonl_path)
            {
                for ev in &buf {
                    let _ = writeln!(f, "{}", ev.to_json());
                }
            }
        }
        drained
    }

    /// Snapshot the aggregator at the requested bucket-tier.
    #[must_use]
    pub fn snapshot_metrics(&self, tier: BucketTier) -> String {
        match self.aggregator.lock() {
            Ok(a) => a.snapshot_bucket(tier),
            Err(_) => "{\"error\":\"aggregator-poisoned\"}".to_string(),
        }
    }

    /// Lifetime emitted (passed Σ-mask gate + into ring).
    #[must_use]
    pub fn emitted(&self) -> u64 {
        self.emitted_total.load(Ordering::Relaxed)
    }

    /// Lifetime dropped (Σ-mask deny + ring-full + lock-poison).
    #[must_use]
    pub fn dropped(&self) -> u64 {
        self.dropped_total.load(Ordering::Relaxed)
    }
}

static GLOBAL_EMITTER: OnceLock<AnalyticsEmitter> = OnceLock::new();

/// Get (lazy-init) the global analytics-emitter. The emitter writes to
/// `<CSSL_LOG_DIR or 'logs'>/analytics.jsonl`.
pub fn global() -> &'static AnalyticsEmitter {
    GLOBAL_EMITTER.get_or_init(|| {
        let dir = std::env::var("CSSL_LOG_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("logs"));
        AnalyticsEmitter::new(dir)
    })
}

// ───────────────────────────────────────────────────────────────────────
// § Convenience shorthands — one per event-kind.
// ───────────────────────────────────────────────────────────────────────

/// Default consent-cap when caller passes None. We default to LocalOnly
/// because the loa-host process is local-by-construction ; relays only
/// happen via the explicit /api/analytics/event endpoint with the
/// caller's cap embedded.
#[inline]
pub fn default_cap() -> ConsentCap {
    ConsentCap::LocalOnly
}

/// Emit `engine.frame_tick`.
pub fn emit_frame_tick(frame: u32, dt_us: u32, fps_q14: u32) {
    let ev = EventRecord::frame_tick(frame, dt_us, fps_q14);
    global().emit(ev, default_cap());
}

/// Emit `engine.render_mode_changed`.
pub fn emit_render_mode_changed(frame: u32, from: u8, to: u8) {
    let ev = EventRecord::render_mode_change(frame, from, to);
    global().emit(ev, default_cap());
}

/// Emit `input.text_typed` (length only · NO content).
pub fn emit_text_typed(frame: u32, len: u32) {
    let ev = EventRecord::text_typed(frame, len);
    global().emit(ev, default_cap());
}

/// Emit `input.text_submitted` (length + intent only · NO content).
pub fn emit_text_submitted(frame: u32, len: u16, intent_kind: u16, conf_q14: u16) {
    let ev = EventRecord::text_submitted(frame, len, intent_kind, conf_q14);
    global().emit(ev, default_cap());
}

/// Emit `intent.classified`.
pub fn emit_intent_classified(frame: u32, intent_kind: u16, conf_q14: u16, fallback: bool) {
    let ev = EventRecord::intent_classified(frame, intent_kind, conf_q14, fallback);
    global().emit(ev, default_cap());
}

/// Emit `intent.routed`.
pub fn emit_intent_routed(frame: u32, intent_kind: u16, response_kind: u16, latency_us: u32) {
    let ev = EventRecord::intent_routed(frame, intent_kind, response_kind, latency_us);
    global().emit(ev, default_cap());
}

/// Emit `gm.response_emitted`.
pub fn emit_gm_response(frame: u32, length_chars: u16, persona_seed: u16, kind_id: u8) {
    let ev = EventRecord::gm_response(frame, length_chars, persona_seed, kind_id);
    global().emit(ev, default_cap());
}

/// Emit `dm.phase_transition`.
pub fn emit_dm_transition(frame: u32, from: u8, to: u8, trigger: u8) {
    let ev = EventRecord::dm_transition(frame, from, to, trigger);
    global().emit(ev, default_cap());
}

/// Emit `procgen.scene_built`.
pub fn emit_procgen_scene(
    frame: u32,
    ms: u32,
    npcs_l0: u8,
    npcs_l1: u8,
    npcs_l2: u8,
    npcs_l3: u8,
) {
    let ev = EventRecord::procgen_scene(frame, ms, npcs_l0, npcs_l1, npcs_l2, npcs_l3);
    global().emit(ev, default_cap());
}

/// Emit `mcp.tool_called`.
pub fn emit_mcp_call(frame: u32, latency_us: u32, ok: bool, tool_idx: u16) {
    let ev = EventRecord::mcp_call(frame, latency_us, ok, tool_idx);
    global().emit(ev, default_cap());
}

/// Emit `kan.classified`.
pub fn emit_kan_classified(frame: u32, swap_point: u16, fallback: bool) {
    let ev = EventRecord::kan_classified(frame, swap_point, fallback);
    global().emit(ev, default_cap());
}

/// Emit `mycelium.sync_event`.
pub fn emit_mycelium_sync(frame: u32, peer_count: u16, bytes_xfer: u32) {
    let ev = EventRecord::mycelium_sync(frame, peer_count, bytes_xfer);
    global().emit(ev, default_cap());
}

/// Emit `consent.cap_granted`.
pub fn emit_consent_granted(frame: u32, cap_id: u16, audience: u8) {
    let ev = EventRecord::consent(frame, cap_id, audience, true);
    global().emit(ev, default_cap());
}

/// Emit `consent.cap_revoked`.
pub fn emit_consent_revoked(frame: u32, cap_id: u16, audience: u8) {
    let ev = EventRecord::consent(frame, cap_id, audience, false);
    global().emit(ev, default_cap());
}

/// Drain up to `max` events from the ring → aggregator + JSONL.
pub fn drain(max: usize) -> usize {
    global().drain(max)
}

/// Snapshot aggregator at the requested bucket-tier.
#[must_use]
pub fn snapshot_metrics(tier: BucketTier) -> String {
    global().snapshot_metrics(tier)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_emitter() -> AnalyticsEmitter {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "loa-analytics-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        AnalyticsEmitter::new(dir)
    }

    #[test]
    fn emit_local_only_admits_event() {
        let e = fresh_emitter();
        let ev = EventRecord::frame_tick(1, 16_000, 14_336);
        assert!(e.emit(ev, ConsentCap::LocalOnly));
        assert_eq!(e.emitted(), 1);
        assert_eq!(e.dropped(), 0);
    }

    #[test]
    fn emit_deny_drops_event_silently() {
        let e = fresh_emitter();
        let ev = EventRecord::frame_tick(1, 16_000, 14_336);
        assert!(!e.emit(ev, ConsentCap::Deny));
        assert_eq!(e.emitted(), 0);
        assert_eq!(e.dropped(), 1);
    }

    #[test]
    fn drain_writes_jsonl_and_ingests_aggregator() {
        let e = fresh_emitter();
        for i in 0..10 {
            e.emit(
                EventRecord::frame_tick(i, 16_000, 14_336),
                ConsentCap::LocalOnly,
            );
        }
        let drained = e.drain(100);
        assert_eq!(drained, 10);
        // Aggregator should have counted them.
        let snap = e.snapshot_metrics(BucketTier::Min1);
        assert!(snap.contains("\"name\":\"engine.frame_tick\""));
        assert!(snap.contains("\"count\":10"));
    }

    #[test]
    fn ring_overflow_increments_dropped() {
        let e = fresh_emitter();
        // Push past the ring cap (1024 - 1 useful slots).
        for i in 0..ANALYTICS_RING_CAP + 100 {
            e.emit(
                EventRecord::frame_tick(i as u32, 0, 0),
                ConsentCap::LocalOnly,
            );
        }
        // Should have accepted ~1023 + dropped the rest.
        assert!(e.emitted() >= 1023);
        assert!(e.dropped() >= 100);
    }

    #[test]
    fn shorthand_emit_increments_global() {
        // This test uses GLOBAL_EMITTER but only checks that calling does
        // not panic ; the global may be polluted from previous tests so
        // we don't assert exact counts.
        emit_frame_tick(1, 16_000, 14_336);
        emit_text_typed(2, 5);
        emit_intent_classified(3, 1, 8000, false);
        emit_dm_transition(4, 0, 1, 2);
        emit_mycelium_sync(5, 3, 1024);
        emit_consent_granted(6, 1, 0);
        emit_consent_revoked(7, 1, 0);
    }

    #[test]
    fn snapshot_metrics_returns_valid_json() {
        let e = fresh_emitter();
        e.emit(
            EventRecord::frame_tick(1, 16_000, 14_336),
            ConsentCap::LocalOnly,
        );
        e.drain(10);
        let snap = e.snapshot_metrics(BucketTier::Min1);
        assert!(snap.starts_with("{"));
        assert!(snap.ends_with("}"));
        assert!(snap.contains("\"bucket\":\"1min\""));
    }
}
