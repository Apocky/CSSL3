//! § wired_histograms — loa-host wrapper around `cssl-host-histograms`.
//!
//! § T11-W5c-LOA-HOST-WIRE
//!   Re-exports the bounded-memory streaming histogram types so telemetry
//!   handlers can record + snapshot P50/P95/P99 distributions without each
//!   call-site reaching across the path-dep.
//!
//! § wrapped surface
//!   - [`Histogram`] — single-stream 64-bucket recorder.
//!   - [`HistogramRegistry`] — `name → Histogram` map with get-or-create.
//!   - [`ScopedTimer`] — RAII timer that records elapsed-µs on drop.
//!
//! § ATTESTATION ¬ harm — wrapper is a re-export shim ; pure-math + memory.

pub use cssl_host_histograms::{
    bucket_index, bucket_lower_bound, bucket_upper_bound, scoped, Histogram, HistogramRegistry,
    ScopedTimer, BUCKETS,
};

/// Convenience : render an empty registry as a short text summary.
/// Returns `"<empty>"` if the registry has no recorded streams. Shape-stable
/// across calls so MCP-tool output stays diffable.
pub fn snapshot_text(reg: &HistogramRegistry) -> String {
    let snap = reg.snapshot();
    if snap.is_empty() {
        return "<empty>".to_string();
    }
    let mut out = String::new();
    for h in snap {
        out.push_str(&h.name);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_registry_renders_empty_marker() {
        let reg = HistogramRegistry::new();
        assert_eq!(snapshot_text(&reg), "<empty>");
    }

    #[test]
    fn record_and_snapshot_round_trip() {
        let mut reg = HistogramRegistry::new();
        reg.record("frame_us", 1_000);
        reg.record("frame_us", 2_000);
        let txt = snapshot_text(&reg);
        assert!(txt.contains("frame_us"));
    }
}
