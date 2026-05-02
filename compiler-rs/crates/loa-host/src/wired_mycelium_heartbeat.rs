//! § wired_mycelium_heartbeat — 60s mycelium-federation tick wired into loa-host.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § T11-W16-WIREUP-MYCELIUM-HEARTBEAT (W14-L → loa-host event-loop)
//!
//! § ROLE
//!   Pre-allocates a `HeartbeatService` with default-period of 60s. The
//!   per-frame `tick(state, dt_ms, allow_emit)` accumulates real-time and
//!   fires `service.tick(now_unix)` only when ≥ period_secs has elapsed
//!   since the last fire AND `allow_emit` is true (Σ-cap default-deny).
//!
//!   The host integrator is responsible for HTTP-POSTing the returned
//!   `FederationBundle` to the cssl-edge endpoint ; this slice produces
//!   the bundle WITHOUT touching the network (¬ tokio · ¬ reqwest).
//!
//! § Σ-CAP-GATE attestation
//!   - default-deny : `allow_emit=false` produces NO bundle. The
//!     host's sovereign-cap is the only path to broadcast.
//!   - k-anonymity floor (k ≥ 10) is enforced inside the wrapped crate ;
//!     this slice cannot bypass it.
//!   - sovereign-revoke is exposed via the `revoke` helper which fires a
//!     `PurgeRequest` ; the host integrator HTTP-POSTs that to peers.
//!
//! § ATTESTATION
//!   ¬ harm · ¬ surveillance · ¬ profiling-individual-players.
//!   There was no hurt nor harm in the making of this, to anyone, anything,
//!   or anybody.

#![allow(clippy::module_name_repetitions)]

pub use cssl_host_mycelium_heartbeat::{
    BackpressureQueue, BundleError, CloudHealth, FederationBundle, FederationCapPolicy,
    FederationKind, FederationPattern, FederationPatternBuilder, HeartbeatRing,
    HeartbeatService, HeartbeatServiceBuilder, HeartbeatStats, PatternError, PurgeRequest,
    CAP_FED_EMIT_ALLOWED, DEFAULT_HEARTBEAT_PERIOD_SECS, K_ANONYMITY_FLOOR,
};
use std::sync::Arc;

/// § Persistent heartbeat-state held by the host.
/// Wraps an `HeartbeatService` plus per-frame ms-accumulator so the 60s
/// cadence can be driven from a 16ms-per-frame event-loop without
/// drifting (we tick whenever the accumulator crosses period_ms).
pub struct MyceliumHeartbeatState {
    pub service: Arc<HeartbeatService>,
    /// Accumulated ms since the last successful tick.
    pub accum_ms: f32,
    /// Period in ms (default 60_000).
    pub period_ms: f32,
    /// Counter of bundles emitted (telemetry).
    pub bundles_emitted: u64,
    /// Counter of cap-denials (helps detect stuck caps).
    pub gate_denials: u64,
}

impl MyceliumHeartbeatState {
    /// Construct fresh state with default period (60s).
    /// Caller passes a pre-built `HeartbeatService` (typically from
    /// `HeartbeatServiceBuilder::default().build()`).
    #[must_use]
    pub fn new(service: Arc<HeartbeatService>) -> Self {
        Self {
            service,
            accum_ms: 0.0,
            period_ms: DEFAULT_HEARTBEAT_PERIOD_SECS as f32 * 1000.0,
            bundles_emitted: 0,
            gate_denials: 0,
        }
    }

    /// Build the canonical default service (used by tests + the engine
    /// integrator's bootstrap path). The pubkey-seed is the integrator's
    /// 32-byte BLAKE3-input ; here we use a `node_handle` u64 zero-padded.
    #[must_use]
    pub fn build_default_service(node_handle: u64) -> Arc<HeartbeatService> {
        let mut pubkey = [0_u8; 32];
        pubkey[..8].copy_from_slice(&node_handle.to_le_bytes());
        let svc = HeartbeatService::builder().node_pubkey(pubkey).build();
        Arc::new(svc)
    }

    /// Override the period (test-only helper).
    pub fn set_period_ms(&mut self, ms: f32) {
        self.period_ms = ms.max(1.0);
    }
}

/// Per-frame tick — accumulates `dt_ms` and fires the wrapped service when
/// the accumulator crosses the period AND the cap is granted.
///
/// Returns Some(FederationBundle) when a bundle was produced this frame,
/// None otherwise.
pub fn tick(
    state: &mut MyceliumHeartbeatState,
    dt_ms: f32,
    now_unix: u64,
    allow_emit: bool,
) -> Option<FederationBundle> {
    state.accum_ms += dt_ms.max(0.0);
    if state.accum_ms < state.period_ms {
        return None;
    }
    // Period elapsed ; reset the accumulator regardless of cap-state so
    // a stuck-cap doesn't keep firing the period-condition every frame.
    state.accum_ms = 0.0;
    if !allow_emit {
        state.gate_denials = state.gate_denials.saturating_add(1);
        return None;
    }
    match state.service.tick(now_unix) {
        Ok(Some(bundle)) => {
            state.bundles_emitted = state.bundles_emitted.saturating_add(1);
            Some(bundle)
        }
        Ok(None) => None,
        Err(_) => None, // bundle-build error : drop quietly + try next period
    }
}

// ─── tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_state() -> MyceliumHeartbeatState {
        let svc = MyceliumHeartbeatState::build_default_service(0xCAFE_BABE);
        MyceliumHeartbeatState::new(svc)
    }

    #[test]
    fn state_default_period_is_60s() {
        let s = make_state();
        assert!((s.period_ms - 60_000.0).abs() < 1e-3);
        assert_eq!(s.bundles_emitted, 0);
    }

    #[test]
    fn tick_below_period_no_emit() {
        let mut s = make_state();
        let r = tick(&mut s, 16.6, 1_700_000_000, true);
        assert!(r.is_none());
        assert_eq!(s.bundles_emitted, 0);
    }

    #[test]
    fn tick_period_elapsed_default_deny() {
        let mut s = make_state();
        s.set_period_ms(100.0); // shorten for the test
        // Two frames of 60ms each → 120ms > 100ms period.
        let _ = tick(&mut s, 60.0, 1_700_000_000, false);
        let r = tick(&mut s, 60.0, 1_700_000_001, false); // CAP DENIED
        assert!(r.is_none());
        assert_eq!(s.bundles_emitted, 0);
        assert_eq!(s.gate_denials, 1);
    }

    #[test]
    fn tick_period_elapsed_with_cap_attempts_emit() {
        let mut s = make_state();
        s.set_period_ms(100.0);
        let _ = tick(&mut s, 60.0, 1_700_000_000, true);
        // Period crossed. With empty ring, the wrapped service returns None,
        // but the accumulator was reset.
        let _ = tick(&mut s, 60.0, 1_700_000_001, true);
        // accum_ms was reset to 0 after the cross.
        assert!(s.accum_ms < 100.0);
    }

    #[test]
    fn set_period_ms_clamps_to_one() {
        let mut s = make_state();
        s.set_period_ms(-50.0);
        assert!(s.period_ms >= 1.0);
        s.set_period_ms(0.0);
        assert!(s.period_ms >= 1.0);
    }

    #[test]
    fn negative_dt_does_not_advance() {
        let mut s = make_state();
        s.set_period_ms(100.0);
        let r = tick(&mut s, -50.0, 1_700_000_000, true);
        assert!(r.is_none());
        assert_eq!(s.bundles_emitted, 0);
    }

    #[test]
    fn accumulator_resets_on_period_cross() {
        let mut s = make_state();
        s.set_period_ms(100.0);
        // 200ms accumulated in one frame → cross AND reset
        let _ = tick(&mut s, 200.0, 1_700_000_000, true);
        // Next 50ms tick should NOT cross again.
        let r = tick(&mut s, 50.0, 1_700_000_001, true);
        assert!(r.is_none());
    }
}
