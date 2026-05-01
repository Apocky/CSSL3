//! § wired_rt_trace — wrapper around `cssl-host-rt-trace`.
//!
//! § T11-W5c-LOA-HOST-WIRE
//!   Surface the bounded-memory runtime trace ring + label interner so MCP
//!   tools can drain the most recent events without each call-site reaching
//!   into the path-dep.
//!
//! § wrapped surface
//!   - [`RtRing`] — fixed-size lock-free trace ring.
//!   - [`RtEvent`] / [`RtEventKind`] — event envelope.
//!   - [`LabelInterner`] — string-label → u16 idx mapping.
//!   - [`scoped_mark`] / [`ScopedMark`] — RAII scope-mark helpers.
//!
//! § ATTESTATION ¬ harm — wrapper is a re-export shim ; bounded memory only.

pub use cssl_host_rt_trace::{
    scoped_mark, LabelInterner, RtEvent, RtEventKind, RtRing, ScopedMark,
};

/// Convenience : drain a [`RtRing`] into a JSON array string suitable for
/// direct MCP-tool emission. Returns `"[]"` for an empty ring.
pub fn drain_to_json(ring: &RtRing) -> String {
    let events = ring.drain();
    serde_json::to_string(&events).unwrap_or_else(|_| "[]".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_ring_drains_to_empty_array() {
        let ring = RtRing::new(8);
        let s = drain_to_json(&ring);
        assert_eq!(s, "[]");
    }

    #[test]
    fn interner_round_trip() {
        let mut interner = LabelInterner::default();
        let idx = interner.intern("frame");
        assert_eq!(interner.get(idx), Some("frame"));
    }
}
