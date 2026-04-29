//! Schema metadata + cssl-telemetry ring-buffer wiring.
//!
//! § SPEC : `_drafts/phase_j/06_l2_telemetry_spec.md` § II.7 (schema +
//!         registration) + § II.6 (effect-row gating ; stage-0 runtime check).
//!
//! § DESIGN
//!   - `MetricSchema` is the data-side mirror of [`crate::RegistryEntry`] — it
//!     adds the canonical bucket-set choice (for histograms/timers) + the
//!     stable scope assignment used when emitting into the telemetry ring.
//!   - `emit_into_ring` is the canonical wire-up point : given a metric name
//!     + payload bytes, it constructs a [`TelemetrySlot`] and pushes into the
//!     ring. Failure to push is COUNTED (overflow_count on the ring) — the
//!     producer never blocks (per specs/22).
//!   - Stage-0 also exposes `EffectRow::check` which validates the caller has
//!     permission to record. In stage-0 this is a runtime no-op-when-set
//!     ([`EffectRow::Counters`]) ; the type is reserved so call-sites can
//!     thread the witness now and the lowering pass adds the compile-time
//!     gate later (per § II.6 stage-1 lift).

use cssl_telemetry::{TelemetryKind, TelemetryRing, TelemetryScope, TelemetrySlot};

use crate::error::{MetricError, MetricResult};
use crate::registry::MetricKind;
use crate::strict_clock::monotonic_ns;

/// Effect-row witness handed to record-sites (stage-0 runtime check).
///
/// § DISCIPLINE : a caller must hold one of the variants matching the metric's
/// scope to record into the metric. Stage-1 lifts this to compile-time via
/// cssl-effects ; the structure is preserved so call-sites are stable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectRow {
    /// `Telemetry<Counters>` — the standard scope for Counter/Gauge/Histogram/Timer.
    Counters,
    /// `Telemetry<Spans>` — span-shaped events (used by tracing wrappers).
    Spans,
    /// `Telemetry<Events>` — structured-event records.
    Events,
    /// `Telemetry<Audit>` — audit-chain entries.
    Audit,
}

impl EffectRow {
    /// True iff this row covers a metric requiring `required`.
    #[must_use]
    pub const fn covers(self, required: Self) -> bool {
        // Stage-0 : exact-match suffices. Stage-1 will widen via subscope rules.
        matches!((self, required), (a, b) if a as u8 == b as u8)
    }

    /// Map to the underlying telemetry-scope used in ring-slots.
    #[must_use]
    pub const fn telemetry_scope(self) -> TelemetryScope {
        match self {
            Self::Counters => TelemetryScope::Counters,
            Self::Spans => TelemetryScope::Spans,
            Self::Events => TelemetryScope::Events,
            Self::Audit => TelemetryScope::Audit,
        }
    }

    /// Verify that `held` is sufficient to record into `required`.
    ///
    /// # Errors
    /// Returns [`MetricError::EffectRowMissing`] when `held` does not cover `required`.
    pub fn check(held: Self, required: Self, metric_name: &'static str) -> MetricResult<()> {
        if !held.covers(required) {
            return Err(MetricError::EffectRowMissing { name: metric_name });
        }
        Ok(())
    }
}

/// Static metadata associated with a registered metric.
#[derive(Debug, Clone)]
pub struct MetricSchema {
    /// Stable name.
    pub name: &'static str,
    /// Kind.
    pub kind: MetricKind,
    /// Schema-id.
    pub schema_id: u64,
    /// Effect-row required to record.
    pub effect_row: EffectRow,
    /// Spec-cite (e.g., `"06_l2_telemetry_spec § III.1"`).
    pub spec_cite: &'static str,
}

impl MetricSchema {
    /// Build with default effect-row = `Counters`.
    #[must_use]
    pub const fn counter(name: &'static str, schema_id: u64, spec_cite: &'static str) -> Self {
        Self {
            name,
            kind: MetricKind::Counter,
            schema_id,
            effect_row: EffectRow::Counters,
            spec_cite,
        }
    }

    /// Build a Gauge schema.
    #[must_use]
    pub const fn gauge(name: &'static str, schema_id: u64, spec_cite: &'static str) -> Self {
        Self {
            name,
            kind: MetricKind::Gauge,
            schema_id,
            effect_row: EffectRow::Counters,
            spec_cite,
        }
    }

    /// Build a Histogram schema.
    #[must_use]
    pub const fn histogram(name: &'static str, schema_id: u64, spec_cite: &'static str) -> Self {
        Self {
            name,
            kind: MetricKind::Histogram,
            schema_id,
            effect_row: EffectRow::Counters,
            spec_cite,
        }
    }

    /// Build a Timer schema.
    #[must_use]
    pub const fn timer(name: &'static str, schema_id: u64, spec_cite: &'static str) -> Self {
        Self {
            name,
            kind: MetricKind::Timer,
            schema_id,
            effect_row: EffectRow::Counters,
            spec_cite,
        }
    }
}

/// Emit a metric-event into the telemetry ring as a [`TelemetrySlot`].
///
/// § PAYLOAD-LAYOUT
///   bytes [0..8]  : little-endian schema-id
///   bytes [8..16] : little-endian payload-u64 (for Counter inc-amount,
///                   Gauge bit-pattern, Timer ns, Histogram bit-pattern of value)
///   bytes [16..]  : zero-padded
///
/// § OVERFLOW
///   The ring is non-blocking ; if it's full the slot is dropped and the ring's
///   `overflow_count()` is incremented. Stage-0 returns Ok regardless ; the
///   caller can interrogate the ring directly.
pub fn emit_into_ring(
    ring: &TelemetryRing,
    schema: &MetricSchema,
    payload_u64: u64,
) {
    #[cfg(feature = "metrics-disabled")]
    {
        let _ = (ring, schema, payload_u64);
        return;
    }

    #[cfg(not(feature = "metrics-disabled"))]
    {
        let kind = match schema.kind {
            MetricKind::Counter => TelemetryKind::Counter,
            MetricKind::Gauge | MetricKind::Histogram => TelemetryKind::Sample,
            MetricKind::Timer => TelemetryKind::Sample,
        };
        let scope = schema.effect_row.telemetry_scope();
        let slot = TelemetrySlot::new(monotonic_ns(), scope, kind);
        let mut payload = [0_u8; 40];
        payload[0..8].copy_from_slice(&schema.schema_id.to_le_bytes());
        payload[8..16].copy_from_slice(&payload_u64.to_le_bytes());
        let slot = slot.with_inline_payload(&payload);
        // Producer never blocks ; ignore overflow-error (the ring counts it).
        let _ = ring.push(slot);
    }
}

#[cfg(test)]
mod tests {
    use super::{emit_into_ring, EffectRow, MetricSchema};
    use crate::error::MetricError;
    use crate::registry::MetricKind;
    use cssl_telemetry::{TelemetryKind, TelemetryRing, TelemetryScope};

    #[test]
    fn effect_row_covers_self() {
        assert!(EffectRow::Counters.covers(EffectRow::Counters));
        assert!(EffectRow::Spans.covers(EffectRow::Spans));
    }

    #[test]
    fn effect_row_does_not_cross_widen_in_stage0() {
        assert!(!EffectRow::Counters.covers(EffectRow::Spans));
        assert!(!EffectRow::Spans.covers(EffectRow::Counters));
    }

    #[test]
    fn effect_row_check_passes_on_match() {
        assert!(EffectRow::check(EffectRow::Counters, EffectRow::Counters, "m").is_ok());
    }

    #[test]
    fn effect_row_check_fails_on_mismatch() {
        let r = EffectRow::check(EffectRow::Spans, EffectRow::Counters, "m");
        assert!(matches!(r, Err(MetricError::EffectRowMissing { .. })));
    }

    #[test]
    fn effect_row_telemetry_scope_mapping() {
        assert_eq!(
            EffectRow::Counters.telemetry_scope(),
            TelemetryScope::Counters
        );
        assert_eq!(EffectRow::Spans.telemetry_scope(), TelemetryScope::Spans);
        assert_eq!(EffectRow::Events.telemetry_scope(), TelemetryScope::Events);
        assert_eq!(EffectRow::Audit.telemetry_scope(), TelemetryScope::Audit);
    }

    #[test]
    fn metric_schema_counter_builder() {
        let s = MetricSchema::counter("a", 1, "cite");
        assert_eq!(s.kind, MetricKind::Counter);
        assert_eq!(s.schema_id, 1);
        assert_eq!(s.spec_cite, "cite");
    }

    #[test]
    fn metric_schema_gauge_builder() {
        let s = MetricSchema::gauge("a", 1, "cite");
        assert_eq!(s.kind, MetricKind::Gauge);
    }

    #[test]
    fn metric_schema_histogram_builder() {
        let s = MetricSchema::histogram("a", 1, "cite");
        assert_eq!(s.kind, MetricKind::Histogram);
    }

    #[test]
    fn metric_schema_timer_builder() {
        let s = MetricSchema::timer("a", 1, "cite");
        assert_eq!(s.kind, MetricKind::Timer);
    }

    #[cfg(not(feature = "metrics-disabled"))]
    #[test]
    fn emit_into_ring_advances_total_pushed() {
        let ring = TelemetryRing::new(16);
        let s = MetricSchema::counter("a", 1, "cite");
        emit_into_ring(&ring, &s, 1);
        assert_eq!(ring.total_pushed(), 1);
    }

    #[cfg(not(feature = "metrics-disabled"))]
    #[test]
    fn emit_into_ring_writes_telemetry_slot() {
        let ring = TelemetryRing::new(16);
        let s = MetricSchema::counter("a", 0xdead_beef, "cite");
        emit_into_ring(&ring, &s, 7);
        assert_eq!(ring.len(), 1);
        let slots = ring.drain_all();
        let slot = &slots[0];
        // Schema-id is at bytes[0..8].
        let mut id_bytes = [0_u8; 8];
        id_bytes.copy_from_slice(&slot.payload[0..8]);
        assert_eq!(u64::from_le_bytes(id_bytes), 0xdead_beef);
        // Payload-u64 at bytes[8..16].
        let mut payload_bytes = [0_u8; 8];
        payload_bytes.copy_from_slice(&slot.payload[8..16]);
        assert_eq!(u64::from_le_bytes(payload_bytes), 7);
    }

    #[cfg(not(feature = "metrics-disabled"))]
    #[test]
    fn emit_into_ring_uses_counter_kind_for_counters() {
        let ring = TelemetryRing::new(16);
        let s = MetricSchema::counter("a", 1, "cite");
        emit_into_ring(&ring, &s, 1);
        let slots = ring.drain_all();
        assert_eq!(slots[0].kind, TelemetryKind::Counter.as_u16());
    }

    #[cfg(not(feature = "metrics-disabled"))]
    #[test]
    fn emit_into_ring_uses_sample_kind_for_gauge() {
        let ring = TelemetryRing::new(16);
        let s = MetricSchema::gauge("a", 1, "cite");
        emit_into_ring(&ring, &s, 1);
        let slots = ring.drain_all();
        assert_eq!(slots[0].kind, TelemetryKind::Sample.as_u16());
    }

    #[cfg(not(feature = "metrics-disabled"))]
    #[test]
    fn emit_into_ring_uses_sample_kind_for_histogram() {
        let ring = TelemetryRing::new(16);
        let s = MetricSchema::histogram("a", 1, "cite");
        emit_into_ring(&ring, &s, 1);
        let slots = ring.drain_all();
        assert_eq!(slots[0].kind, TelemetryKind::Sample.as_u16());
    }

    #[cfg(not(feature = "metrics-disabled"))]
    #[test]
    fn emit_into_ring_uses_sample_kind_for_timer() {
        let ring = TelemetryRing::new(16);
        let s = MetricSchema::timer("a", 1, "cite");
        emit_into_ring(&ring, &s, 1);
        let slots = ring.drain_all();
        assert_eq!(slots[0].kind, TelemetryKind::Sample.as_u16());
    }

    #[cfg(not(feature = "metrics-disabled"))]
    #[test]
    fn emit_into_ring_does_not_block_on_overflow() {
        let ring = TelemetryRing::new(2);
        let s = MetricSchema::counter("a", 1, "cite");
        emit_into_ring(&ring, &s, 1);
        emit_into_ring(&ring, &s, 1);
        emit_into_ring(&ring, &s, 1); // overflow ; should not panic
        assert_eq!(ring.total_pushed(), 3);
        // overflow_count is now ≥ 1
        assert!(ring.overflow_count() >= 1);
    }

    #[cfg(not(feature = "metrics-disabled"))]
    #[test]
    fn emit_into_ring_scope_matches_effect_row() {
        let ring = TelemetryRing::new(16);
        let mut s = MetricSchema::counter("a", 1, "cite");
        s.effect_row = EffectRow::Spans;
        emit_into_ring(&ring, &s, 1);
        let slots = ring.drain_all();
        assert_eq!(slots[0].scope, TelemetryScope::Spans.as_u16());
    }

    #[test]
    fn metric_schema_clonable() {
        let s = MetricSchema::counter("a", 1, "cite");
        let _ = s.clone();
    }
}
