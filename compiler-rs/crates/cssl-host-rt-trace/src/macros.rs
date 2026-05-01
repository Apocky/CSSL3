//! § convenience macros for hot-path instrumentation
//!
//! - [`rt_mark!`] : create a [`ScopedMark`] at the call-site · pairs with
//!   the surrounding scope's lifetime.
//! - [`rt_counter!`] : push a single `Counter` event into a ring.
//!
//! These macros keep label-interning out of the hot-path : the label
//! is a `&str` literal at the call-site, but the macro expects the
//! caller has already interned it (via a one-shot `LabelInterner` field
//! held by the host). For ad-hoc interning, the convenience form
//! `rt_mark!(ring, interner, "label")` is also provided.
//!
//! ## Example
//!
//! ```ignore
//! use cssl_host_rt_trace::{rt_counter, rt_mark, LabelInterner, RtRing};
//!
//! struct Engine {
//!     ring: RtRing,
//!     labels: LabelInterner,
//!     frame_idx: u16,
//! }
//!
//! impl Engine {
//!     fn render(&mut self) {
//!         let _scope = rt_mark!(&self.ring, self.frame_idx);
//!         // … render work …
//!         rt_counter!(&self.ring, self.frame_idx, 16_667);
//!     }
//! }
//! ```

/// § convenience : create a [`ScopedMark`](crate::scope::ScopedMark)
/// with a pre-interned label index.
#[macro_export]
macro_rules! rt_mark {
    ($ring:expr, $label_idx:expr) => {
        $crate::scope::scoped_mark($ring, $label_idx)
    };
    ($ring:expr, $interner:expr, $label_str:expr) => {{
        let idx = $interner.intern($label_str);
        $crate::scope::scoped_mark($ring, idx)
    }};
}

/// § push a `Counter` event with `value` in `value_a`.
#[macro_export]
macro_rules! rt_counter {
    ($ring:expr, $label_idx:expr, $value:expr) => {{
        let ts = $crate::scope::now_micros();
        $ring.push(
            $crate::event::RtEvent::new(ts, $crate::event::RtEventKind::Counter, $label_idx)
                .with_a($value as u64),
        );
    }};
    ($ring:expr, $interner:expr, $label_str:expr, $value:expr) => {{
        let idx = $interner.intern($label_str);
        let ts = $crate::scope::now_micros();
        $ring.push(
            $crate::event::RtEvent::new(ts, $crate::event::RtEventKind::Counter, idx)
                .with_a($value as u64),
        );
    }};
}

#[cfg(test)]
mod tests {
    use crate::event::RtEventKind;
    use crate::ring::RtRing;
    use crate::LabelInterner;

    #[test]
    fn rt_mark_pre_interned_idx() {
        let ring = RtRing::new(16);
        {
            let _s = crate::rt_mark!(&ring, 5);
            // Begin pushed inside macro.
            assert_eq!(ring.snapshot().len(), 1);
        }
        let snap = ring.snapshot();
        assert_eq!(snap.len(), 2);
        assert_eq!(snap[0].kind, RtEventKind::MarkBegin);
        assert_eq!(snap[0].label_idx, 5);
        assert_eq!(snap[1].kind, RtEventKind::MarkEnd);
    }

    #[test]
    fn rt_mark_with_interner() {
        let ring = RtRing::new(16);
        let mut interner = LabelInterner::default();
        {
            let _s = crate::rt_mark!(&ring, &mut interner, "frame");
        }
        // 2 events : Begin, End.
        let snap = ring.snapshot();
        assert_eq!(snap.len(), 2);
        let frame_idx = interner.intern("frame");
        assert_eq!(snap[0].label_idx, frame_idx);
        assert_eq!(snap[1].label_idx, frame_idx);
    }

    #[test]
    fn rt_counter_pushes_single_event() {
        let ring = RtRing::new(16);
        crate::rt_counter!(&ring, 11, 42_u64);
        let snap = ring.snapshot();
        assert_eq!(snap.len(), 1);
        assert_eq!(snap[0].kind, RtEventKind::Counter);
        assert_eq!(snap[0].label_idx, 11);
        assert_eq!(snap[0].value_a, 42);
    }
}
