// § cssl-host-multiplayer-signaling : pure state-machine for LoA player-to-player
// signaling. Inputs = typed messages ; outputs = state-transitions + outbound replies.
// NO network transport here ; the Supabase / WebRTC wire-up lands in a downstream
// wave. Cap-gated mutating actions per §§ 11_IFC.
//
// § design
//   • `SignalingMessage` is the canonical envelope — `from_peer` → `to_peer`,
//     a typed `MessageKind` discriminator, and an opaque `Vec<u8>` payload
//     (≤ 64 KiB) that downstream code interprets per-kind.
//   • `Peer` tracks per-participant liveness via `last_seen_micros` ; idle
//     peers are evictable after `IDLE_THRESHOLD_SECS`.
//   • `Room` is the authoritative membership-set keyed by short room-code ;
//     enforces max-peers cap, open/closed gate, duplicate-join rejection,
//     and idle-eviction sweeps.
//   • `ClientState` is the per-player façade ; cap-bits gate `host_room` /
//     `join_room` / `process_inbound`. `sovereign` flag bypasses cap checks
//     per Apocky PRIME-DIRECTIVE single-player-mode default.
//
// § non-goals
//   • Wire serialization for the network (use `serde_json` directly on the
//     types here ; transport-layer adds frame-headers / WebRTC mux).
//   • Cryptographic identity (peer-IDs are opaque strings ; auth lives in
//     the Supabase tables).
//   • TURN / STUN negotiation (offers/answers/ICE candidates are passed
//     through opaquely as `MessageKind` variants).

#![forbid(unsafe_code)]
#![deny(missing_debug_implementations)]
#![allow(clippy::module_name_repetitions)]

pub mod message;
pub mod peer;
pub mod room;
pub mod state_machine;

pub use message::{MessageKind, MsgErr, SignalingMessage, MAX_PAYLOAD_BYTES};
pub use peer::{Peer, IDLE_THRESHOLD_SECS};
pub use room::{Room, RoomErr};
pub use state_machine::{
    ClientState, MpErr, MP_CAP_HOST_ROOM, MP_CAP_JOIN_ROOM, MP_CAP_RELAY_DATA,
};

/// Crate version string for handshake / debug logs. Pulls from the package
/// version at build time so it tracks `Cargo.toml` automatically.
pub const SIGNALING_PROTO_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod smoke {
    use super::*;

    #[test]
    fn proto_version_non_empty() {
        assert!(!SIGNALING_PROTO_VERSION.is_empty());
    }

    #[test]
    fn re_exports_visible() {
        // touch every re-export to lock the public surface
        let _ = MessageKind::Hello;
        let _ = MAX_PAYLOAD_BYTES;
        let _ = IDLE_THRESHOLD_SECS;
        let _ = MP_CAP_HOST_ROOM;
        let _ = MP_CAP_JOIN_ROOM;
        let _ = MP_CAP_RELAY_DATA;
    }
}
