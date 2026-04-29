//! Metric-event — the canonical record-unit appended to a [`ReplayLog`].
//!
//! § SPEC : `_drafts/phase_j/06_l2_telemetry_spec.md` § VI.2 + § VI.3.
//!
//! § DISCIPLINE
//!
//! Each metric-event captures :
//!     - `frame_n` + `sub_phase_index` : logical-frame timestamp (no wallclock)
//!     - `metric_id` : stable u32 derived at registry-build (mock here)
//!     - `kind` : Counter / Gauge / Histogram-record / Timer-record
//!     - `value` : encoded as bit-pattern for f64 (deterministic)
//!     - `tag_hash` : BLAKE3-derived u64 of tag-set (no raw paths, no biometrics)
//!
//!   The byte-encoding is **canonical** — same logical event ⇒ same 32 bytes.
//!   This is what makes two replay-runs of the same seed produce bit-equal
//!   ReplayLog snapshots.
//!
//! [`ReplayLog`]: crate::ReplayLog

use crate::FrameN;

/// One metric-event recorded into the replay-log.
///
/// § SPEC : § VI.2 + § VI.3.
///
/// Layout : 32 bytes total (canonical-byte-form).
///   - frame_n         : 8 bytes (LE u64)
///   - sub_phase_index : 1 byte
///   - kind_disc       : 1 byte
///   - reserved        : 2 bytes (LE u16 zero)
///   - metric_id       : 4 bytes (LE u32)
///   - value_bits      : 8 bytes (LE u64 / bit-pattern of u64 or f64)
///   - tag_hash        : 8 bytes (LE u64)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MetricEvent {
    /// Logical-frame counter at the time of recording.
    pub frame_n: FrameN,
    /// `SubPhase::index()` (0..=6).
    pub sub_phase_index: u8,
    /// Kind discriminant (Counter/Gauge/Histogram-record/Timer-record/SamplerDecision).
    pub kind: MetricEventKind,
    /// Stable u32 metric-id (catalog index ; mock here ; real in cssl-metrics).
    pub metric_id: u32,
    /// Value encoded as 64-bit bit-pattern.
    pub value: MetricValue,
    /// BLAKE3-derived u64 hash of the tag set.
    pub tag_hash: u64,
}

/// Discriminant for metric-event kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MetricEventKind {
    /// `Counter::inc_by(n)` — value = u64 delta as bit-pattern.
    CounterIncBy,
    /// `Counter::set(v)` — value = u64 absolute as bit-pattern.
    CounterSet,
    /// `Gauge::set(v)` — value = f64 bit-pattern.
    GaugeSet,
    /// `Gauge::inc(delta)` — value = f64 bit-pattern.
    GaugeInc,
    /// `Histogram::record(v)` — value = f64 bit-pattern.
    HistogramRecord,
    /// `Timer` recorded ns ; value = u64 delta-ns.
    TimerRecordNs,
    /// `Sampling::OneIn` decimation decision (sampled = 1 ; skipped = 0).
    SamplerDecision,
}

impl MetricEventKind {
    /// Stable byte-discriminant. The byte values are part of the H5 contract.
    #[must_use]
    pub const fn disc(&self) -> u8 {
        match self {
            Self::CounterIncBy => 0x01,
            Self::CounterSet => 0x02,
            Self::GaugeSet => 0x03,
            Self::GaugeInc => 0x04,
            Self::HistogramRecord => 0x05,
            Self::TimerRecordNs => 0x06,
            Self::SamplerDecision => 0x07,
        }
    }

    /// Inverse of [`Self::disc`]. Returns `None` for invalid bytes.
    #[must_use]
    pub const fn from_disc(b: u8) -> Option<Self> {
        match b {
            0x01 => Some(Self::CounterIncBy),
            0x02 => Some(Self::CounterSet),
            0x03 => Some(Self::GaugeSet),
            0x04 => Some(Self::GaugeInc),
            0x05 => Some(Self::HistogramRecord),
            0x06 => Some(Self::TimerRecordNs),
            0x07 => Some(Self::SamplerDecision),
            _ => None,
        }
    }
}

/// Value envelope for a metric event. Internally stored as `u64` so two
/// different-typed events with the same bit-pattern hash identically.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MetricValue(u64);

impl MetricValue {
    /// From a `u64` (counter delta / counter set).
    #[must_use]
    pub const fn from_u64(v: u64) -> Self {
        Self(v)
    }

    /// From an `f64` via `to_bits` (canonical IEEE-754 representation).
    /// NaN bit-pattern is preserved (the `cssl-metrics` Gauge::set guard
    /// is the layer that refuses NaN — this layer only encodes).
    #[must_use]
    pub fn from_f64(v: f64) -> Self {
        Self(v.to_bits())
    }

    /// From a `bool` (sampler decision : sampled = 1, skipped = 0).
    #[must_use]
    pub const fn from_bool(b: bool) -> Self {
        Self(if b { 1 } else { 0 })
    }

    /// Read as raw u64 bit-pattern (canonical).
    #[must_use]
    pub const fn as_bits(&self) -> u64 {
        self.0
    }

    /// Read as `u64` (caller asserts kind ∈ Counter*).
    #[must_use]
    pub const fn as_u64(&self) -> u64 {
        self.0
    }

    /// Read as `f64` via `from_bits`.
    #[must_use]
    pub fn as_f64(&self) -> f64 {
        f64::from_bits(self.0)
    }

    /// Read as `bool` (caller asserts kind = SamplerDecision).
    #[must_use]
    pub const fn as_bool(&self) -> bool {
        self.0 != 0
    }
}

impl MetricEvent {
    /// Canonical byte-form length.
    pub const BYTE_LEN: usize = 32;

    /// Encode into a 32-byte canonical buffer (LE everywhere).
    #[must_use]
    pub fn to_canonical_bytes(&self) -> [u8; Self::BYTE_LEN] {
        let mut buf = [0u8; Self::BYTE_LEN];
        buf[0..8].copy_from_slice(&self.frame_n.to_le_bytes());
        buf[8] = self.sub_phase_index;
        buf[9] = self.kind.disc();
        // 10..12 reserved-zero (already zero).
        buf[12..16].copy_from_slice(&self.metric_id.to_le_bytes());
        buf[16..24].copy_from_slice(&self.value.as_bits().to_le_bytes());
        buf[24..32].copy_from_slice(&self.tag_hash.to_le_bytes());
        buf
    }

    /// Inverse of [`Self::to_canonical_bytes`]. Used by replay-log readers.
    pub fn from_canonical_bytes(bytes: &[u8; Self::BYTE_LEN]) -> Option<Self> {
        let frame_n = u64::from_le_bytes(bytes[0..8].try_into().ok()?);
        let sub_phase_index = bytes[8];
        let kind = MetricEventKind::from_disc(bytes[9])?;
        // bytes[10..12] reserved-zero ; we don't strictly enforce zero on read.
        let metric_id = u32::from_le_bytes(bytes[12..16].try_into().ok()?);
        let value_bits = u64::from_le_bytes(bytes[16..24].try_into().ok()?);
        let tag_hash = u64::from_le_bytes(bytes[24..32].try_into().ok()?);
        Some(Self {
            frame_n,
            sub_phase_index,
            kind,
            metric_id,
            value: MetricValue::from_u64(value_bits),
            tag_hash,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t_kind_disc_round_trip() {
        for &k in &[
            MetricEventKind::CounterIncBy,
            MetricEventKind::CounterSet,
            MetricEventKind::GaugeSet,
            MetricEventKind::GaugeInc,
            MetricEventKind::HistogramRecord,
            MetricEventKind::TimerRecordNs,
            MetricEventKind::SamplerDecision,
        ] {
            let d = k.disc();
            assert_eq!(MetricEventKind::from_disc(d), Some(k));
        }
    }

    #[test]
    fn t_kind_disc_invalid_returns_none() {
        assert_eq!(MetricEventKind::from_disc(0x00), None);
        assert_eq!(MetricEventKind::from_disc(0xFF), None);
    }

    #[test]
    fn t_metric_value_u64_round_trip() {
        let v = MetricValue::from_u64(0xDEAD_BEEF_CAFE_F00D);
        assert_eq!(v.as_u64(), 0xDEAD_BEEF_CAFE_F00D);
    }

    #[test]
    fn t_metric_value_f64_round_trip() {
        let v = MetricValue::from_f64(std::f64::consts::PI);
        assert!((v.as_f64() - std::f64::consts::PI).abs() < f64::EPSILON);
    }

    #[test]
    fn t_metric_value_f64_nan_preserved() {
        let v = MetricValue::from_f64(f64::NAN);
        assert!(v.as_f64().is_nan());
    }

    #[test]
    fn t_metric_value_bool_round_trip() {
        assert!(MetricValue::from_bool(true).as_bool());
        assert!(!MetricValue::from_bool(false).as_bool());
    }

    #[test]
    fn t_canonical_bytes_round_trip() {
        let ev = MetricEvent {
            frame_n: 42,
            sub_phase_index: 2,
            kind: MetricEventKind::HistogramRecord,
            metric_id: 0x1234_5678,
            value: MetricValue::from_f64(1.5),
            tag_hash: 0x00C0_FFEE_BEEF_DEAD,
        };
        let bytes = ev.to_canonical_bytes();
        let back = MetricEvent::from_canonical_bytes(&bytes).unwrap();
        assert_eq!(ev, back);
    }

    #[test]
    fn t_canonical_bytes_byte_len_32() {
        assert_eq!(MetricEvent::BYTE_LEN, 32);
        let ev = MetricEvent {
            frame_n: 0,
            sub_phase_index: 0,
            kind: MetricEventKind::CounterIncBy,
            metric_id: 0,
            value: MetricValue::from_u64(0),
            tag_hash: 0,
        };
        assert_eq!(ev.to_canonical_bytes().len(), 32);
    }

    #[test]
    fn t_canonical_bytes_zeros() {
        let ev = MetricEvent {
            frame_n: 0,
            sub_phase_index: 0,
            kind: MetricEventKind::CounterIncBy,
            metric_id: 0,
            value: MetricValue::from_u64(0),
            tag_hash: 0,
        };
        let bytes = ev.to_canonical_bytes();
        // disc for CounterIncBy = 0x01 ; everything else zero.
        let mut expected = [0u8; 32];
        expected[9] = 0x01;
        assert_eq!(bytes, expected);
    }

    #[test]
    fn t_canonical_bytes_field_layout() {
        let ev = MetricEvent {
            frame_n: 0x0102_0304_0506_0708,
            sub_phase_index: 0xAB,
            kind: MetricEventKind::GaugeSet,
            metric_id: 0x0A0B_0C0D,
            value: MetricValue::from_u64(0x1112_1314_1516_1718),
            tag_hash: 0x2122_2324_2526_2728,
        };
        let bytes = ev.to_canonical_bytes();
        // LE encoding ⇒ byte[0] = LSB.
        assert_eq!(bytes[0], 0x08);
        assert_eq!(bytes[7], 0x01);
        assert_eq!(bytes[8], 0xAB);
        assert_eq!(bytes[9], MetricEventKind::GaugeSet.disc());
        assert_eq!(bytes[10], 0); // reserved
        assert_eq!(bytes[11], 0); // reserved
        assert_eq!(bytes[12], 0x0D);
        assert_eq!(bytes[15], 0x0A);
        assert_eq!(bytes[16], 0x18);
        assert_eq!(bytes[23], 0x11);
        assert_eq!(bytes[24], 0x28);
        assert_eq!(bytes[31], 0x21);
    }

    #[test]
    fn t_two_events_same_input_same_bytes() {
        let ev_a = MetricEvent {
            frame_n: 7,
            sub_phase_index: 1,
            kind: MetricEventKind::TimerRecordNs,
            metric_id: 99,
            value: MetricValue::from_u64(1_234_567),
            tag_hash: 0xAA,
        };
        let ev_b = ev_a;
        assert_eq!(ev_a.to_canonical_bytes(), ev_b.to_canonical_bytes());
    }

    #[test]
    fn t_canonical_bytes_short_buffer_returns_none_via_len() {
        // We use [u8; 32] in API ; runtime validation is on the caller side.
        // This sanity-test ensures an invalid kind in an otherwise-valid
        // 32-byte buffer correctly returns None.
        let mut bytes = [0u8; 32];
        bytes[9] = 0xFF; // invalid kind disc
        assert!(MetricEvent::from_canonical_bytes(&bytes).is_none());
    }

    #[test]
    fn t_value_as_bits_consistency() {
        let v = MetricValue::from_f64(-0.0);
        // -0.0 has a non-zero bit pattern (sign bit).
        assert_eq!(v.as_bits(), (-0.0_f64).to_bits());
    }
}
