// § cssl-host-mp-transport : transport-adapter ABI for cssl-host-multiplayer-signaling
//
// § purpose
//   The W4 signaling crate (`cssl-host-multiplayer-signaling`) is a *pure* state
//   machine — `SignalingMessage` envelopes go in, replies come out. To actually
//   move bytes between peers we need a transport. This crate defines the trait
//   that pairs the signaling envelope with arbitrary backends + ships two
//   reference impls (loopback for tests + stub-Supabase for ABI shape).
//
// § shape
//   • `trait_def` — `MpTransport` trait + cap-bits + result/error enums
//   • `loopback`  — in-memory mailbox impl (for tests / single-process LAN)
//   • `stub_supabase` — records sends + returns empty polls (ABI shape ; no IO)
//   • `router`    — `TransportRouter` — primary + optional fallback transport
//
// § non-goals (this crate)
//   • Real network IO ; the live Supabase REST polling + WebSocket impls live
//     in a downstream wave.
//   • Cryptographic identity ; peer-id auth lives in the Supabase RLS layer.
//
// § PRIME-DIRECTIVE
//   • `#![forbid(unsafe_code)]` ← inherited consent-architecture default
//   • Library code never panics ; all error paths return `TransportErr`.

#![forbid(unsafe_code)]
#![deny(missing_debug_implementations)]
#![allow(clippy::module_name_repetitions)]

pub mod loopback;
pub mod router;
pub mod stub_supabase;
pub mod trait_def;

pub use loopback::LoopbackTransport;
pub use router::TransportRouter;
pub use stub_supabase::StubSupabaseTransport;
pub use trait_def::{
    MpTransport, TransportErr, TransportResult, TRANSPORT_CAP_BOTH, TRANSPORT_CAP_RECV,
    TRANSPORT_CAP_SEND,
};

// Re-export the canonical message envelope so transport users have a single
// import-target for the full ABI surface.
pub use cssl_host_multiplayer_signaling::SignalingMessage;

/// Crate version string for handshake / debug logs.
pub const TRANSPORT_PROTO_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod smoke {
    use super::*;

    #[test]
    fn proto_version_non_empty() {
        assert!(!TRANSPORT_PROTO_VERSION.is_empty());
    }

    #[test]
    fn re_exports_visible() {
        // touch every re-export to pin the public surface
        let _ = TRANSPORT_CAP_SEND;
        let _ = TRANSPORT_CAP_RECV;
        let _ = TRANSPORT_CAP_BOTH;
    }
}
