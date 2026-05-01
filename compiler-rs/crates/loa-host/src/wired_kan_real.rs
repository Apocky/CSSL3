//! § wired_kan_real — wrapper around `cssl-host-kan-real`.
//!
//! § T11-W7-G-LOA-HOST-WIRE
//!   Re-exports the REAL stage-1 KAN classifier surface + canary-gate so
//!   MCP tools can probe intent-label cardinality + canary enrollment
//!   without each call-site reaching across the path-dep.
//!
//! § wrapped surface
//!   - [`RealIntentKanClassifier`] — REAL stage-1 KAN intent classifier.
//!   - [`CanaryGate`] — 10% session-id-hash A/B-rollout gate.
//!   - [`DisagreementKind`] — structured disagreement events for rollback.
//!   - [`IntentLabel`] — the 8 canonical intent classes (I=32 → O=8).
//!
//! § ATTESTATION ¬ harm — wrapper is a re-export shim ; pure-math + bit-
//!   stable RNG only ; no I/O ; no caps granted.

#![forbid(unsafe_code)]

pub use cssl_host_kan_real::{
    CanaryGate, DisagreementKind, IntentLabel, RealIntentKanClassifier, INTENT_LABEL_COUNT,
};

/// Convenience : the canonical intent-label cardinality (I=32 → O=8 head
/// per `specs/grand-vision/11_KAN_RIDE.csl`). Used by the
/// `kan_real.canary_check` MCP tool to surface a basic shape probe.
#[must_use]
pub fn intent_kind_count() -> u8 {
    // INTENT_LABEL_COUNT is `usize` ; the spec mandates 8.
    debug_assert!(INTENT_LABEL_COUNT == 8);
    INTENT_LABEL_COUNT as u8
}

/// Convenience : check whether a given session-id is enrolled in the
/// 10% canary cohort. Spawns a fresh [`CanaryGate`] @ default config so
/// the probe is deterministic + side-effect-free. The u128 is rendered
/// as a fixed-width hex string to match the `CanaryGate::enrolled(&str)`
/// API while keeping the MCP-tool ABI a 16-byte session-id.
#[must_use]
pub fn is_session_in_canary(session_id: u128) -> bool {
    let gate = CanaryGate::default();
    let s = format!("{session_id:032x}");
    gate.enrolled(&s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intent_kind_count_is_eight() {
        assert_eq!(intent_kind_count(), 8);
    }

    #[test]
    fn canary_check_is_deterministic() {
        // Same input → same output across calls (no RNG drift).
        let id: u128 = 0xDEAD_BEEF_CAFE_BABE_0123_4567_89AB_CDEF;
        let a = is_session_in_canary(id);
        let b = is_session_in_canary(id);
        assert_eq!(a, b);
    }
}
