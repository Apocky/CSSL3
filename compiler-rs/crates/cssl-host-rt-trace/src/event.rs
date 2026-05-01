//! § event-record + label-interner
//!
//! ## RtEvent layout (compact 32-byte cache-friendly)
//!
//! ```text
//!   off  size  field
//!   0    8     ts_micros : u64        ← micros since epoch (or process-start)
//!   8    1     kind      : RtEventKind ← 1-byte enum (5 variants)
//!   9    1     _pad0
//!   10   2     label_idx : u16        ← interned label table index
//!   12   4     _pad1                  ← align value_a to 8
//!   16   8     value_a   : u64        ← elapsed-micros / counter-value / hist-key
//!   24   8     value_b   : u64        ← counter-delta / hist-bucket-count
//!   ──   ──
//!   32 bytes total
//! ```
//!
//! `repr(C)` + explicit padding ⇒ stable across rust-versions for serde-binary
//! interop + raw-mmap drains. `assert_eq!(size_of::<RtEvent>(), 32)` enforces.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// § discriminant of a single trace-record. 1-byte storage via `repr(u8)`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[repr(u8)]
pub enum RtEventKind {
    /// Begin of a `scoped_mark` region — paired with `MarkEnd`.
    MarkBegin = 0,
    /// End of a `scoped_mark` region. `value_a` = elapsed-micros from `MarkBegin`.
    MarkEnd = 1,
    /// Counter sample — `value_a` = current-value · `value_b` = delta-since-last (optional).
    Counter = 2,
    /// Histogram sample — `value_a` = bucket-key · `value_b` = bucket-count.
    Histogram = 3,
    /// Custom event — `value_a` / `value_b` user-defined.
    Custom = 4,
}

impl Default for RtEventKind {
    fn default() -> Self {
        Self::Custom
    }
}

/// § single trace-record · 32-byte compact layout.
///
/// `repr(C)` + padding fields make the layout deterministic across compilers
/// for binary-serde + raw-mmap-drain scenarios.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[repr(C)]
pub struct RtEvent {
    /// Microseconds since some reference point (typically process-start).
    pub ts_micros: u64,
    /// Discriminant of this event.
    pub kind: RtEventKind,
    #[doc(hidden)]
    #[serde(skip)]
    pub _pad0: u8,
    /// Index into the [`LabelInterner`]'s string-table.
    pub label_idx: u16,
    #[doc(hidden)]
    #[serde(skip)]
    pub _pad1: u32,
    /// Primary payload — semantics depend on `kind` (see variant docs).
    pub value_a: u64,
    /// Secondary payload — semantics depend on `kind`.
    pub value_b: u64,
}

impl RtEvent {
    /// § construct a new event at micros-timestamp `ts`.
    #[must_use]
    pub fn new(ts_micros: u64, kind: RtEventKind, label_idx: u16) -> Self {
        Self {
            ts_micros,
            kind,
            _pad0: 0,
            label_idx,
            _pad1: 0,
            value_a: 0,
            value_b: 0,
        }
    }

    /// § builder : set `value_a`.
    #[must_use]
    pub fn with_a(mut self, a: u64) -> Self {
        self.value_a = a;
        self
    }

    /// § builder : set `value_b`.
    #[must_use]
    pub fn with_b(mut self, b: u64) -> Self {
        self.value_b = b;
        self
    }
}

/// § label-string interner. `intern("foo")` returns a stable `u16` index ;
/// duplicates are deduplicated so identical labels share a slot.
///
/// Capacity-saturates at `u16::MAX = 65 535` — beyond that, [`LabelInterner::intern`]
/// returns `u16::MAX` (a poison index that maps via [`LabelInterner::get`] to
/// `Some("<overflow>")`). This matches the no-half-measures policy : the
/// pipeline never panics on overflow ; drains see the marker.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct LabelInterner {
    strings: Vec<String>,
    lookup: HashMap<String, u16>,
}

impl LabelInterner {
    /// § sentinel returned when capacity exhausted (≥ `u16::MAX`).
    pub const OVERFLOW_MARKER: &'static str = "<overflow>";

    /// § intern `s` → return its index. Duplicates dedup.
    pub fn intern(&mut self, s: &str) -> u16 {
        if let Some(&idx) = self.lookup.get(s) {
            return idx;
        }
        // Saturate at u16::MAX − 1 ; reserve u16::MAX for the overflow marker.
        if self.strings.len() >= (u16::MAX as usize) - 1 {
            return u16::MAX;
        }
        let idx = self.strings.len() as u16;
        self.strings.push(s.to_owned());
        self.lookup.insert(s.to_owned(), idx);
        idx
    }

    /// § retrieve interned string by index. Returns `Some("<overflow>")` for
    /// the saturation sentinel `u16::MAX`.
    #[must_use]
    pub fn get(&self, idx: u16) -> Option<&str> {
        if idx == u16::MAX {
            return Some(Self::OVERFLOW_MARKER);
        }
        self.strings.get(idx as usize).map(String::as_str)
    }

    /// § number of distinct strings interned.
    #[must_use]
    pub fn len(&self) -> usize {
        self.strings.len()
    }

    /// § no strings yet ?
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.strings.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::size_of;

    #[test]
    fn event_bounded_32_bytes() {
        // § cache-friendliness contract : RtEvent must stay 32 bytes.
        assert_eq!(size_of::<RtEvent>(), 32, "RtEvent layout drifted from 32-byte target");
    }

    #[test]
    fn event_roundtrip_serde() {
        let ev = RtEvent::new(1_234_567, RtEventKind::Counter, 42)
            .with_a(99)
            .with_b(7);
        let json = serde_json::to_string(&ev).expect("serialize");
        let back: RtEvent = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(ev, back);
    }

    #[test]
    fn interner_deduplicates() {
        let mut i = LabelInterner::default();
        let a = i.intern("frame");
        let b = i.intern("frame");
        let c = i.intern("draw");
        assert_eq!(a, b, "identical strings must dedup to same idx");
        assert_ne!(a, c, "distinct strings must get distinct idx");
        assert_eq!(i.len(), 2);
    }

    #[test]
    fn interner_roundtrip() {
        let mut i = LabelInterner::default();
        let frame = i.intern("frame");
        let draw = i.intern("draw");
        assert_eq!(i.get(frame), Some("frame"));
        assert_eq!(i.get(draw), Some("draw"));
        assert_eq!(i.get(9999), None);
    }

    #[test]
    fn interner_overflow_saturates() {
        // § stress-saturation : push enough labels to exhaust u16 space.
        // Use a smaller threshold for unit-test speed — the sentinel logic
        // is the same and we verify the marker via direct u16::MAX query.
        let i = LabelInterner::default();
        // The overflow sentinel must round-trip to the marker string.
        assert_eq!(i.get(u16::MAX), Some(LabelInterner::OVERFLOW_MARKER));

        // Functional saturation check : push exactly u16::MAX − 1 strings
        // and verify that the next intern returns u16::MAX.
        let mut full = LabelInterner::default();
        // Simulate fill-to-cap by directly pushing into strings (bypass intern)
        // for test speed ; intern would do same work but slower.
        for k in 0..((u16::MAX as usize) - 1) {
            full.strings.push(format!("s{k}"));
            full.lookup.insert(format!("s{k}"), k as u16);
        }
        assert_eq!(full.intern("one_too_many"), u16::MAX);
        assert_eq!(full.get(u16::MAX), Some(LabelInterner::OVERFLOW_MARKER));
    }
}
