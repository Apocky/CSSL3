// § state_machine.rs : ClientState — cap-gated mutating actions + inbound dispatch
//
// `ClientState` is the per-player façade over `Room` / `Peer` / `SignalingMessage`.
// Cap-bits gate mutating actions per Apocky §§ 11_IFC :
//   • `MP_CAP_HOST_ROOM`  → `host_room`
//   • `MP_CAP_JOIN_ROOM`  → `join_room`
//   • `MP_CAP_RELAY_DATA` → `process_inbound` (broadcast / relay paths)
//
// `sovereign` flag bypasses cap checks for single-player / sovereign-mode
// substrate-runs per PRIME-DIRECTIVE consent-OS default.
//
// `process_inbound` returns a `Vec<SignalingMessage>` of *outbound* replies the
// transport layer should ship. Replies are deterministic per-input ; this
// makes the state-machine fully reproducible for record/replay testing.

use crate::message::{MessageKind, MsgErr, SignalingMessage};
use crate::room::{Room, RoomErr};

/// Cap bit : authorize hosting a new room.
pub const MP_CAP_HOST_ROOM: u32 = 1;
/// Cap bit : authorize joining an existing room.
pub const MP_CAP_JOIN_ROOM: u32 = 2;
/// Cap bit : authorize relaying / broadcasting data within a room.
pub const MP_CAP_RELAY_DATA: u32 = 4;

/// Per-player state-machine. Holds at most one active room.
#[derive(Debug, Clone)]
pub struct ClientState {
    /// Currently-active room, if any.
    pub room: Option<Room>,
    /// This client's opaque peer-id ; matches `from_peer` on outbound
    /// messages.
    pub my_peer_id: String,
    /// PRIME-DIRECTIVE sovereign-mode flag. When `true`, cap checks are
    /// bypassed (single-player / local-substrate scenarios).
    pub sovereign_cap: bool,
    /// Multiplayer-cap bitfield. OR of the `MP_CAP_*` constants.
    pub mp_cap: u32,
    /// Monotonic outbound-message id counter. Bumped on every reply emitted
    /// by `process_inbound`.
    next_msg_id: u64,
}

/// Errors returned by `ClientState` mutators / dispatchers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MpErr {
    /// Caller lacks the required cap-bit ; the missing bit is reported.
    CapDenied(u32),
    /// Action requires an active room.
    NotInRoom,
    /// Underlying `Room` rejected the operation.
    Room(RoomErr),
    /// Message envelope failed validation.
    Msg(MsgErr),
}

impl core::fmt::Display for MpErr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::CapDenied(bit) => write!(f, "cap denied : missing bit 0x{bit:x}"),
            Self::NotInRoom => f.write_str("client is not in a room"),
            Self::Room(e) => write!(f, "room error : {e}"),
            Self::Msg(e) => write!(f, "message error : {e}"),
        }
    }
}

impl From<RoomErr> for MpErr {
    fn from(e: RoomErr) -> Self {
        Self::Room(e)
    }
}

impl From<MsgErr> for MpErr {
    fn from(e: MsgErr) -> Self {
        Self::Msg(e)
    }
}

impl ClientState {
    /// Construct a fresh state with no active room.
    pub fn new(my_peer_id: String, mp_cap: u32, sovereign: bool) -> Self {
        Self {
            room: None,
            my_peer_id,
            sovereign_cap: sovereign,
            mp_cap,
            next_msg_id: 1,
        }
    }

    fn require_cap(&self, bit: u32) -> Result<(), MpErr> {
        if self.sovereign_cap || (self.mp_cap & bit) == bit {
            Ok(())
        } else {
            Err(MpErr::CapDenied(bit))
        }
    }

    fn alloc_id(&mut self) -> u64 {
        let id = self.next_msg_id;
        self.next_msg_id = self.next_msg_id.saturating_add(1);
        id
    }

    /// Host a fresh room as `my_peer_id`. Cap-gated by `MP_CAP_HOST_ROOM`.
    /// If a room is already active, returns `RoomErr::AlreadyJoined`.
    pub fn host_room(
        &mut self,
        code: String,
        max_peers: u8,
        now_micros: u64,
    ) -> Result<(), MpErr> {
        self.require_cap(MP_CAP_HOST_ROOM)?;
        if self.room.is_some() {
            return Err(MpErr::Room(RoomErr::AlreadyJoined));
        }
        let mut r = Room::new_at(code, self.my_peer_id.clone(), max_peers, now_micros);
        r.add_host(None, now_micros);
        self.room = Some(r);
        Ok(())
    }

    /// Join an existing room (passed in by the transport layer).
    /// Cap-gated by `MP_CAP_JOIN_ROOM`.
    pub fn join_room(&mut self, mut room: Room, now_micros: u64) -> Result<(), MpErr> {
        self.require_cap(MP_CAP_JOIN_ROOM)?;
        if self.room.is_some() {
            return Err(MpErr::Room(RoomErr::AlreadyJoined));
        }
        room.join(self.my_peer_id.clone(), None, now_micros)?;
        self.room = Some(room);
        Ok(())
    }

    /// Leave the active room (clears `self.room`). The caller is expected to
    /// emit a `Bye` envelope through the transport layer ; this method only
    /// updates local state.
    pub fn leave_room(&mut self, _now_micros: u64) -> Result<(), MpErr> {
        if self.room.is_none() {
            return Err(MpErr::NotInRoom);
        }
        self.room = None;
        Ok(())
    }

    /// Process an inbound signaling message. Returns the (possibly empty)
    /// list of outbound replies the transport layer should ship.
    ///
    /// Routing rules :
    ///   • `Hello` from a non-self peer → reply `RoomState` if we're host.
    ///   • `Bye` → remove peer from room.
    ///   • `Ping` → reply `Pong` directly back to sender.
    ///   • `Offer` / `Answer` / `IceCandidate` / `RoomState` / `Custom` /
    ///     `Pong` → no automatic reply ; payload is surfaced to caller via
    ///     state mutation only.
    pub fn process_inbound(
        &mut self,
        msg: SignalingMessage,
        now_micros: u64,
    ) -> Result<Vec<SignalingMessage>, MpErr> {
        msg.validate()?;
        self.require_cap(MP_CAP_RELAY_DATA)?;
        let room = self.room.as_mut().ok_or(MpErr::NotInRoom)?;

        // ignore traffic addressed to a different peer (unless broadcast)
        if msg.to_peer != self.my_peer_id && msg.to_peer != "*" {
            return Ok(Vec::new());
        }

        // touch sender liveness if they're a known member
        let _ = room.touch(&msg.from_peer, now_micros);

        let mut replies: Vec<SignalingMessage> = Vec::new();
        let host_id = room.host_id.clone();
        let am_host = self.my_peer_id == host_id;
        let my_id = self.my_peer_id.clone();

        match &msg.kind {
            MessageKind::Hello => {
                // host snapshots room-state to the new peer ; non-host ignores
                if am_host && msg.from_peer != my_id {
                    let snapshot = serde_json::to_vec(&*room)
                        .unwrap_or_default(); // serialization of HashMap<String,Peer> is total
                    let id = self.alloc_id();
                    replies.push(SignalingMessage {
                        id,
                        from_peer: my_id,
                        to_peer: msg.from_peer.clone(),
                        kind: MessageKind::RoomState,
                        payload: snapshot,
                        ts_micros: now_micros,
                    });
                }
            }
            MessageKind::Bye => {
                let _ = room.leave(&msg.from_peer);
            }
            MessageKind::Ping => {
                let id = self.alloc_id();
                replies.push(SignalingMessage {
                    id,
                    from_peer: my_id,
                    to_peer: msg.from_peer.clone(),
                    kind: MessageKind::Pong,
                    payload: Vec::new(),
                    ts_micros: now_micros,
                });
            }
            // signaling traffic to forward / surface — no auto-reply
            MessageKind::Pong
            | MessageKind::Offer
            | MessageKind::Answer
            | MessageKind::IceCandidate
            | MessageKind::RoomState
            | MessageKind::Custom(_) => {}
        }

        Ok(replies)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::peer::Peer;

    fn t0() -> u64 {
        1_000_000_000
    }

    fn host_room_fixture() -> Room {
        let mut r = Room::new_at("ROOM01".into(), "host".into(), 4, t0());
        r.add_host(None, t0());
        r
    }

    fn msg(from: &str, to: &str, kind: MessageKind) -> SignalingMessage {
        SignalingMessage {
            id: 99,
            from_peer: from.into(),
            to_peer: to.into(),
            kind,
            payload: Vec::new(),
            ts_micros: 0,
        }
    }

    #[test]
    fn host_cap_required() {
        // no caps, not sovereign
        let mut s = ClientState::new("me".into(), 0, false);
        let err = s
            .host_room("X".into(), 4, t0())
            .unwrap_err();
        assert_eq!(err, MpErr::CapDenied(MP_CAP_HOST_ROOM));
        assert!(s.room.is_none());

        // cap granted → success
        let mut s2 = ClientState::new("me".into(), MP_CAP_HOST_ROOM, false);
        s2.host_room("X".into(), 4, t0()).unwrap();
        assert!(s2.room.is_some());
        assert_eq!(s2.room.as_ref().unwrap().host_id, "me");
    }

    #[test]
    fn join_cap_required() {
        let mut s = ClientState::new("me".into(), 0, false);
        let r = host_room_fixture();
        let err = s.join_room(r.clone(), t0()).unwrap_err();
        assert_eq!(err, MpErr::CapDenied(MP_CAP_JOIN_ROOM));

        let mut s2 = ClientState::new("me".into(), MP_CAP_JOIN_ROOM, false);
        s2.join_room(r, t0()).unwrap();
        assert!(s2.room.is_some());
        assert!(s2.room.as_ref().unwrap().peers.contains_key("me"));
    }

    #[test]
    fn sovereign_bypasses_caps() {
        // no caps, but sovereign flag → all actions permitted
        let mut s = ClientState::new("me".into(), 0, true);
        s.host_room("X".into(), 4, t0()).unwrap();
        // process_inbound also permitted
        let m = msg("me", "me", MessageKind::Ping);
        let replies = s.process_inbound(m, t0()).unwrap();
        assert_eq!(replies.len(), 1);
        assert_eq!(replies[0].kind, MessageKind::Pong);
    }

    #[test]
    fn process_hello_replies_with_roomstate() {
        // host receives Hello from a new peer ; replies with RoomState
        let mut s = ClientState::new("host".into(), 0, true);
        s.host_room("ROOM01".into(), 4, t0()).unwrap();
        // simulate the new peer being added to the host's room first
        s.room
            .as_mut()
            .unwrap()
            .peers
            .insert("p2".into(), Peer::new("p2".into(), None, t0()));

        let hello = msg("p2", "host", MessageKind::Hello);
        let replies = s.process_inbound(hello, t0()).unwrap();
        assert_eq!(replies.len(), 1);
        assert_eq!(replies[0].kind, MessageKind::RoomState);
        assert_eq!(replies[0].from_peer, "host");
        assert_eq!(replies[0].to_peer, "p2");
        assert!(!replies[0].payload.is_empty());
    }

    #[test]
    fn process_ice_no_auto_reply() {
        // ICE candidates surface to caller via state mutation only — no auto-reply
        let mut s = ClientState::new("me".into(), 0, true);
        s.host_room("ROOM01".into(), 4, t0()).unwrap();
        s.room
            .as_mut()
            .unwrap()
            .peers
            .insert("p2".into(), Peer::new("p2".into(), None, t0()));

        let m = msg("p2", "me", MessageKind::IceCandidate);
        let replies = s.process_inbound(m, t0() + 1_000_000).unwrap();
        assert!(replies.is_empty());
        // sender liveness was touched
        let p2 = &s.room.as_ref().unwrap().peers["p2"];
        assert_eq!(p2.last_seen_micros, t0() + 1_000_000);

        // broadcast (`*`) is also accepted
        let bm = msg("p2", "*", MessageKind::IceCandidate);
        let replies = s.process_inbound(bm, t0() + 2_000_000).unwrap();
        assert!(replies.is_empty());
    }

    #[test]
    fn leave_clears_room() {
        let mut s = ClientState::new("me".into(), 0, true);
        s.host_room("X".into(), 4, t0()).unwrap();
        assert!(s.room.is_some());
        s.leave_room(t0()).unwrap();
        assert!(s.room.is_none());
        // double-leave is an error
        assert_eq!(s.leave_room(t0()).unwrap_err(), MpErr::NotInRoom);
    }

    #[test]
    fn process_rejects_when_not_in_room() {
        let mut s = ClientState::new("me".into(), 0, true);
        let m = msg("p2", "me", MessageKind::Hello);
        let err = s.process_inbound(m, t0()).unwrap_err();
        assert_eq!(err, MpErr::NotInRoom);
    }

    #[test]
    fn cap_denied_error_carries_bit() {
        // Each cap-denial reports the specific missing bit so the caller can
        // surface a precise error message.
        let mut s = ClientState::new("me".into(), 0, false);
        // host
        match s.host_room("X".into(), 4, t0()) {
            Err(MpErr::CapDenied(b)) => assert_eq!(b, MP_CAP_HOST_ROOM),
            other => panic!("expected CapDenied(HOST), got {other:?}"),
        }
        // join
        let r = host_room_fixture();
        match s.join_room(r, t0()) {
            Err(MpErr::CapDenied(b)) => assert_eq!(b, MP_CAP_JOIN_ROOM),
            other => panic!("expected CapDenied(JOIN), got {other:?}"),
        }
        // process_inbound — needs a room to even reach the cap check, so
        // grant join, then strip the relay cap and confirm the bit reported.
        let mut s2 = ClientState::new("me".into(), MP_CAP_JOIN_ROOM, false);
        s2.join_room(host_room_fixture(), t0()).unwrap();
        // strip caps
        s2.mp_cap = 0;
        let m = msg("host", "me", MessageKind::Ping);
        match s2.process_inbound(m, t0()) {
            Err(MpErr::CapDenied(b)) => assert_eq!(b, MP_CAP_RELAY_DATA),
            other => panic!("expected CapDenied(RELAY), got {other:?}"),
        }
    }

    #[test]
    fn invalid_message_rejected_before_dispatch() {
        // empty from_peer ⇒ MpErr::Msg(MsgErr::EmptyFrom)
        let mut s = ClientState::new("me".into(), 0, true);
        s.host_room("X".into(), 4, t0()).unwrap();
        let bad = msg("", "me", MessageKind::Hello);
        let err = s.process_inbound(bad, t0()).unwrap_err();
        assert_eq!(err, MpErr::Msg(MsgErr::EmptyFrom));
    }
}
