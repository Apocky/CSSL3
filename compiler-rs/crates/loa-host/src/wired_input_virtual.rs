//! § wired_input_virtual — wrapper around `cssl-host-input-virtual`.
//!
//! § T11-W5c-LOA-HOST-WIRE
//!   Re-exports the synthetic input-event generators + scenario shapes so
//!   MCP tools can list available scenarios and dispatch a deterministic
//!   event-stream into the replay pipeline.
//!
//! § wrapped surface
//!   - [`navigate_test_room`] / [`type_intent_phrase`] /
//!     [`full_qa_session`] — high-level scenario generators.
//!   - [`ascii_text_to_keystrokes`] / [`random_typing_session`] — typing.
//!   - [`circle_path`] / [`drag`] / [`lissajous`] / [`random_walk`] — mouse.
//!   - [`Pcg32`] — deterministic stdlib-only PRNG.
//!
//! § ATTESTATION ¬ harm — wrapper is a re-export shim ; pure-CPU generation.

pub use cssl_host_input_virtual::{
    ascii_text_to_keystrokes, circle_path, drag, full_qa_session, lissajous, navigate_test_room,
    random_typing_session, random_walk, type_intent_phrase, Pcg32, ReplayEvent, ReplayEventKind,
};

/// Convenience : list the canonical scenario names. Stable order matches
/// the `scenarios::*` declaration in the underlying crate.
#[must_use]
pub fn list_scenarios() -> &'static [&'static str] {
    &["navigate_test_room", "type_intent_phrase", "full_qa_session"]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_scenarios_lists_three() {
        assert_eq!(list_scenarios().len(), 3);
        assert_eq!(list_scenarios()[0], "navigate_test_room");
    }

    #[test]
    fn navigate_test_room_is_deterministic() {
        let a = navigate_test_room(42);
        let b = navigate_test_room(42);
        assert_eq!(a.len(), b.len());
        assert_eq!(a, b, "same-seed event streams must be bit-identical");
    }
}
