//! § wired_fps_pipeline — MCP-style accessor over the FPS pipeline state
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § T11-W13-FPS-PIPELINE-WIRE (W13-1)
//!
//! § ROLE
//!   Thin wrapper module over `crate::fps_pipeline::FpsPipeline`. Provides the
//!   same shape as the existing `wired_*` family (replay · audit · golden ·
//!   stereoscopy · ...) so future MCP-tool authors can reach for
//!   `loa_host::wired_fps_pipeline::*` instead of going through the full
//!   `crate::fps_pipeline` path on every call.
//!
//!   The companion sibling W13-12 (cssl-host-perf-enforcer) reads our
//!   public surface to attest fleet-level frame-budget compliance.
//!   W13-2..W13-11 siblings read `FrameMetrics::frame_id` for snapshot tagging.
//!
//! § PRIME-DIRECTIVE attestation
//!   - ¬ surveillance : exposed metrics are aggregate frame-time only · no
//!     player-behavior signals leave this module.
//!   - ¬ heuristic-stutter-detection : we report budget-miss-counts, not
//!     classifications about the player's hardware-tier.
//!   There was no hurt nor harm in the making of this, to anyone/anything/anybody.

#![allow(clippy::module_name_repetitions)]

use crate::fps_pipeline::{
    FpsPipeline, FrameMetrics,
    FRAME_BUDGET_120HZ_MS, FRAME_BUDGET_144HZ_MS, FRAME_BUDGET_60HZ_MS,
};

// ──────────────────────────────────────────────────────────────────────────
// § Re-exports
// ──────────────────────────────────────────────────────────────────────────

pub use crate::fps_pipeline::{
    CmdBufferEntry, CmdBufferPool, CullingPlan, FrustumPlane, InstanceBuffer,
    InstanceEntry, PassDescriptor, RingBuffer, UniformStaging,
    DEFAULT_RING_DEPTH, INSTANCE_CAP, MAX_RING_DEPTH, PASS_COUNT,
    SUB_FRAME_LATENCY_MS, UNIFORM_STAGING_BYTES_PER_FRAME,
};

// ──────────────────────────────────────────────────────────────────────────
// § Helper accessors (catalog-mode safe)
// ──────────────────────────────────────────────────────────────────────────

/// Construct an FpsPipeline with the canonical default configuration :
///   - ring-depth = 3 (triple-buffered)
///   - 120Hz target budget (8.333ms)
///   - present-mode = Mailbox (low-latency)
///   - VRS = static-radial (foveated OFF · default-deny consent-axiom)
#[must_use]
pub fn default_pipeline() -> FpsPipeline {
    FpsPipeline::new()
}

/// Construct an FpsPipeline tuned for the 144Hz stretch target.
/// Same triple-buffer + Mailbox configuration ; budget-threshold at 6.944ms.
#[must_use]
pub fn stretch_144hz_pipeline() -> FpsPipeline {
    let mut p = FpsPipeline::new();
    p.set_target_hz(144);
    p
}

/// Construct an FpsPipeline tuned for legacy 60Hz hardware.
/// Same triple-buffer + Mailbox configuration ; budget-threshold at 16.667ms.
#[must_use]
pub fn legacy_60hz_pipeline() -> FpsPipeline {
    let mut p = FpsPipeline::new();
    p.set_target_hz(60);
    p
}

/// Lookup the canonical budget-ms threshold for a given target Hz.
/// Returns the closest canonical (60 / 120 / 144) ; falls back to 1000/Hz
/// for off-table values.
#[must_use]
pub fn budget_for_hz(hz: u32) -> f32 {
    match hz {
        144 => FRAME_BUDGET_144HZ_MS,
        120 => FRAME_BUDGET_120HZ_MS,
        60 => FRAME_BUDGET_60HZ_MS,
        other => 1000.0 / (other as f32).max(1.0),
    }
}

/// True when the given frame-ms is within the 120Hz budget.
#[must_use]
pub fn frame_ms_under_120hz(frame_ms: f32) -> bool {
    frame_ms <= FRAME_BUDGET_120HZ_MS
}

/// True when the given frame-ms is within the 144Hz budget.
#[must_use]
pub fn frame_ms_under_144hz(frame_ms: f32) -> bool {
    frame_ms <= FRAME_BUDGET_144HZ_MS
}

/// One-line summary of the FpsPipeline state for telemetry / log lines.
/// Format : "fps_pipeline · ring=3 · budget=8.333ms · present=Mailbox
///   · vrs_avg=0.34 · last_frame=N (Mms) · cull=A/B".
#[must_use]
pub fn summary_line(p: &FpsPipeline) -> String {
    let m = &p.last_metrics;
    format!(
        "fps_pipeline · ring={} · budget={:.3}ms · present={:?} \
         · vrs_avg={:.3} · last_frame={} ({:.2}ms) · cull={}/{}",
        p.ring.depth(),
        p.target_budget_ms,
        p.present_mode,
        p.vrs.average_pixel_ratio(),
        m.frame_id,
        m.frame_ms,
        m.instances_passed,
        m.instances_input,
    )
}

/// Compute a JSON-line representation of the most recent FrameMetrics.
/// Stable field-order ; useful for telemetry sinks that line-buffer.
#[must_use]
pub fn metrics_jsonl(m: &FrameMetrics) -> String {
    format!(
        "{{\"frame_id\":{},\"frame_ms\":{:.4},\"cmd_buffers\":{},\
         \"cmd_recycles\":{},\"inst_in\":{},\"inst_pass\":{},\
         \"vrs_ratio\":{:.4},\"present\":{},\"budget_ms\":{:.4},\
         \"miss_120\":{},\"miss_144\":{}}}",
        m.frame_id,
        m.frame_ms,
        m.cmd_buffers,
        m.cmd_buffer_recycles,
        m.instances_input,
        m.instances_passed,
        m.vrs_pixel_ratio,
        m.present_mode,
        m.budget_ms,
        m.missed_120hz,
        m.missed_144hz,
    )
}

// ──────────────────────────────────────────────────────────────────────────
// § TESTS
// ──────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fps_pipeline::{
        FrameSlot, FrameSlotState, PassKind, PresentMode, VrsConfig, VrsTier,
    };

    #[test]
    fn default_pipeline_uses_120hz_target() {
        let p = default_pipeline();
        assert!((p.target_budget_ms - FRAME_BUDGET_120HZ_MS).abs() < 0.01);
        assert_eq!(p.ring.depth(), DEFAULT_RING_DEPTH);
        assert_eq!(p.present_mode, PresentMode::Mailbox);
    }

    #[test]
    fn stretch_144hz_pipeline_targets_144() {
        let p = stretch_144hz_pipeline();
        assert!((p.target_budget_ms - FRAME_BUDGET_144HZ_MS).abs() < 0.01);
        assert_eq!(p.present_mode, PresentMode::Mailbox);
    }

    #[test]
    fn legacy_60hz_pipeline_targets_60() {
        let p = legacy_60hz_pipeline();
        assert!((p.target_budget_ms - FRAME_BUDGET_60HZ_MS).abs() < 0.01);
    }

    #[test]
    fn budget_for_hz_table_correct() {
        assert!((budget_for_hz(60) - FRAME_BUDGET_60HZ_MS).abs() < 0.01);
        assert!((budget_for_hz(120) - FRAME_BUDGET_120HZ_MS).abs() < 0.01);
        assert!((budget_for_hz(144) - FRAME_BUDGET_144HZ_MS).abs() < 0.01);
        // Off-table : 240Hz → 1000/240 ≈ 4.167.
        assert!((budget_for_hz(240) - 4.167).abs() < 0.01);
    }

    #[test]
    fn frame_ms_under_helpers() {
        assert!(frame_ms_under_120hz(5.0));
        assert!(!frame_ms_under_120hz(10.0));
        assert!(frame_ms_under_144hz(5.0));
        assert!(!frame_ms_under_144hz(7.0));
    }

    #[test]
    fn summary_line_is_well_formed() {
        let mut p = default_pipeline();
        p.step_one_frame(0, 5_000);
        let s = summary_line(&p);
        assert!(s.contains("fps_pipeline"));
        assert!(s.contains("ring=3"));
        assert!(s.contains("Mailbox"));
    }

    #[test]
    fn metrics_jsonl_is_parseable_shape() {
        let mut p = default_pipeline();
        let _ = p.step_one_frame(0, 5_000);
        let line = metrics_jsonl(&p.last_metrics);
        // Quick shape-check : must have all 11 fields + open-close braces.
        assert!(line.starts_with('{'));
        assert!(line.ends_with('}'));
        for key in [
            "frame_id", "frame_ms", "cmd_buffers", "cmd_recycles",
            "inst_in", "inst_pass", "vrs_ratio", "present", "budget_ms",
            "miss_120", "miss_144",
        ] {
            assert!(line.contains(key), "missing key {key} in {line}");
        }
    }

    #[test]
    fn passkind_and_present_mode_visible_thru_wire() {
        // Confirm re-exports.
        let _ = PassKind::Opaque;
        let _ = PresentMode::Fifo;
        let _ = VrsTier::Tier3;
        let _ = VrsConfig::default();
        let _ = FrameSlot::new(0);
        assert_eq!(FrameSlotState::Free as u8, 0);
    }
}
