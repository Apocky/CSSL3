//! § wired_mp_transport_real — wrapper around `cssl-host-mp-transport-real`.
//!
//! § T11-W7-G-LOA-HOST-WIRE
//!   Re-exports the REAL ureq-backed Supabase `MpTransport` surface +
//!   sovereign-bypass recorder so MCP tools can probe the canonical
//!   transport cap-bits without each call-site reaching across the
//!   path-dep.
//!
//! § wrapped surface
//!   - [`RealSupabaseTransport`] — production-grade `MpTransport` impl.
//!   - [`SupabaseConfig`] — base-url + apikey + bearer-token bundle.
//!   - [`SovereignBypassRecorder`] — loud-audit recorder for sovereign
//!     bypass events (mp.sovereign.bypass).
//!   - [`TRANSPORT_CAP_BOTH`] — the SEND|RECV cap-bit constant.
//!
//! § ATTESTATION ¬ harm — wrapper is a re-export shim ; the transport
//!   surface is cap-gated (default-deny) ; this wire grants no caps.

#![forbid(unsafe_code)]

pub use cssl_host_mp_transport_real::{
    RealSupabaseTransport, SovereignBypassRecorder, SupabaseConfig, TRANSPORT_CAP_BOTH,
    TRANSPORT_CAP_RECV, TRANSPORT_CAP_SEND,
};

/// Convenience : the canonical SEND|RECV cap-bit constant from the
/// transport spec. Used by the `mp_transport.real_caps_query` MCP tool
/// to surface a basic shape probe.
#[must_use]
pub fn mp_transport_cap_bits() -> u32 {
    TRANSPORT_CAP_BOTH
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cap_bits_match_send_or_recv() {
        // BOTH must equal the OR of the two atomic caps.
        assert_eq!(mp_transport_cap_bits(), TRANSPORT_CAP_SEND | TRANSPORT_CAP_RECV);
    }

    #[test]
    fn cap_bits_popcount_is_two() {
        assert_eq!(TRANSPORT_CAP_BOTH.count_ones(), 2);
    }
}
