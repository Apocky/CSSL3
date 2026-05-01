//! § wired_multiplayer — wrapper around `cssl-host-multiplayer-signaling`.
//!
//! § T11-W5c-LOA-HOST-WIRE
//!   Re-exports the multiplayer signaling state-machine + envelope types so
//!   MCP tools can surface the room-membership status without each call-site
//!   reaching into the path-dep.
//!
//! § wrapped surface
//!   - [`Room`] / [`RoomErr`] — authoritative membership-set.
//!   - [`Peer`] — per-participant liveness tracking.
//!   - [`SignalingMessage`] / [`MessageKind`] / [`MsgErr`] — envelope.
//!   - [`ClientState`] / [`MpErr`] — per-player façade.
//!
//! § ATTESTATION ¬ harm — wrapper is a re-export shim ; cap-gated state-machine.

pub use cssl_host_multiplayer_signaling::{
    ClientState, MessageKind, MpErr, MsgErr, Peer, Room, RoomErr, SignalingMessage,
    IDLE_THRESHOLD_SECS, MAX_PAYLOAD_BYTES, MP_CAP_HOST_ROOM, MP_CAP_JOIN_ROOM, MP_CAP_RELAY_DATA,
    SIGNALING_PROTO_VERSION,
};

/// Convenience : short-form room status string. Returns `"no-room"` for an
/// `Option::None` (no room joined yet) or a room-code summary otherwise.
/// `code` + `peers` are public fields on the underlying `Room` struct.
#[must_use]
pub fn room_status(room: Option<&Room>) -> String {
    match room {
        Some(r) => format!("room:{} peers:{}", r.code, r.peers.len()),
        None => "no-room".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_room_returns_no_room_marker() {
        assert_eq!(room_status(None), "no-room");
    }

    #[test]
    fn signaling_proto_version_is_non_empty() {
        assert!(!SIGNALING_PROTO_VERSION.is_empty());
    }
}
