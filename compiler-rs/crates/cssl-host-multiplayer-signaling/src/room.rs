// § room.rs : Room membership-set + lifecycle gates
//
// `Room` is the authoritative member-list keyed by short room-code. The host
// owns lifecycle (open/close, idle-eviction, max-peers cap) ; non-host peers
// can `touch` themselves on activity but cannot mutate other peers' liveness.
//
// `Room::new` returns a closed-eligible record open by default ; `expires_at`
// is computed at construction time as `now + DEFAULT_ROOM_LIFETIME_SECS`. The
// state-machine rejects joins past expiry but does not auto-close — the host
// is expected to invoke `close` (or the transport-layer enforces TTL).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::peer::Peer;

/// Default lifetime for a freshly-hosted room (24 hours). Can be overridden
/// post-construction by mutating `expires_at_micros` directly.
pub const DEFAULT_ROOM_LIFETIME_SECS: u64 = 24 * 60 * 60;

/// Authoritative membership-set for a hosted multiplayer session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Room {
    /// Short, human-shareable room code (typically 6-8 chars).
    pub code: String,
    /// `player_id` of the host peer ; the host is the only peer authorized
    /// to mutate `is_open` / evict peers / change `max_peers`.
    pub host_id: String,
    /// Member set keyed by `player_id`.
    pub peers: HashMap<String, Peer>,
    /// Hard cap on peers (host included). Reaching this cap rejects further
    /// `join` calls with `RoomErr::Full`.
    pub max_peers: u8,
    /// `false` after `close` ; rejects further joins.
    pub is_open: bool,
    /// Wall-clock at construction (microseconds since UNIX epoch).
    pub created_at_micros: u64,
    /// Wall-clock past which `join` fails with `RoomErr::Expired`.
    pub expires_at_micros: u64,
}

/// Errors returned by `Room` mutators.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RoomErr {
    /// Reached `max_peers`.
    Full,
    /// `is_open == false`.
    Closed,
    /// Player-id is already in `peers`.
    AlreadyJoined,
    /// Player-id is not in `peers`.
    NotFound,
    /// Caller is not the host (reserved for future host-only operations).
    NotHost,
    /// `now_micros >= expires_at_micros`.
    Expired,
}

impl core::fmt::Display for RoomErr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Full => f.write_str("room is full"),
            Self::Closed => f.write_str("room is closed"),
            Self::AlreadyJoined => f.write_str("player already in room"),
            Self::NotFound => f.write_str("player not found in room"),
            Self::NotHost => f.write_str("operation requires host privilege"),
            Self::Expired => f.write_str("room has expired"),
        }
    }
}

impl Room {
    /// Construct a fresh open room with the given host. The host is NOT
    /// auto-added to `peers` — the caller is expected to invoke
    /// `add_host` (or call `join` after construction) so liveness tracking
    /// starts at a known wall-clock.
    pub fn new(code: String, host_id: String, max_peers: u8) -> Self {
        Self::new_at(code, host_id, max_peers, 0)
    }

    /// Like `new` but with an explicit `now_micros` baseline. Used by
    /// `ClientState::host_room` to anchor `created_at` / `expires_at` to
    /// the same clock the rest of the state-machine reads.
    pub fn new_at(code: String, host_id: String, max_peers: u8, now_micros: u64) -> Self {
        let lifetime = DEFAULT_ROOM_LIFETIME_SECS.saturating_mul(1_000_000);
        Self {
            code,
            host_id,
            peers: HashMap::new(),
            max_peers,
            is_open: true,
            created_at_micros: now_micros,
            expires_at_micros: now_micros.saturating_add(lifetime),
        }
    }

    /// Insert the host peer (sets `is_host: true`).
    pub fn add_host(&mut self, display_name: Option<String>, now_micros: u64) {
        let host = Peer::new_host(self.host_id.clone(), display_name, now_micros);
        self.peers.insert(self.host_id.clone(), host);
    }

    /// Insert a non-host peer. Validates :
    ///   • room not expired
    ///   • room is open
    ///   • `peers.len() < max_peers`
    ///   • player_id not already present
    pub fn join(
        &mut self,
        player_id: String,
        display_name: Option<String>,
        now_micros: u64,
    ) -> Result<(), RoomErr> {
        self.gate_join(now_micros)?;
        if self.peers.contains_key(&player_id) {
            return Err(RoomErr::AlreadyJoined);
        }
        if self.peers.len() >= usize::from(self.max_peers) {
            return Err(RoomErr::Full);
        }
        let p = Peer::new(player_id.clone(), display_name, now_micros);
        self.peers.insert(player_id, p);
        Ok(())
    }

    fn gate_join(&self, now_micros: u64) -> Result<(), RoomErr> {
        if !self.is_open {
            return Err(RoomErr::Closed);
        }
        if now_micros >= self.expires_at_micros && self.expires_at_micros != 0 {
            return Err(RoomErr::Expired);
        }
        Ok(())
    }

    /// Remove a peer by `player_id`. Returns `RoomErr::NotFound` if absent.
    pub fn leave(&mut self, player_id: &str) -> Result<(), RoomErr> {
        if self.peers.remove(player_id).is_none() {
            return Err(RoomErr::NotFound);
        }
        Ok(())
    }

    /// Update a peer's `last_seen_micros`. Returns `RoomErr::NotFound` if
    /// the peer is absent.
    pub fn touch(&mut self, player_id: &str, now_micros: u64) -> Result<(), RoomErr> {
        self.peers.get_mut(player_id).map_or(Err(RoomErr::NotFound), |p| {
            p.touch(now_micros);
            Ok(())
        })
    }

    /// Sweep idle peers. Returns the list of evicted `player_id`s in
    /// arbitrary order (HashMap iteration). The host is NEVER evicted by
    /// this sweep — host-rotation requires an explicit transition.
    pub fn evict_idle(&mut self, now_micros: u64) -> Vec<String> {
        let evicted: Vec<String> = self
            .peers
            .iter()
            .filter(|(_, p)| !p.is_host && p.is_idle(now_micros))
            .map(|(id, _)| id.clone())
            .collect();
        for id in &evicted {
            self.peers.remove(id);
        }
        evicted
    }

    /// Mark the room closed. Subsequent `join` calls fail with `Closed`.
    pub fn close(&mut self) {
        self.is_open = false;
    }

    /// Number of peers currently in the room.
    pub fn peer_count(&self) -> usize {
        self.peers.len()
    }

    /// Look up the host peer if present.
    pub fn host(&self) -> Option<&Peer> {
        self.peers.get(&self.host_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t0() -> u64 {
        1_000_000_000
    }

    fn fresh() -> Room {
        let mut r = Room::new_at("ROOM01".into(), "host".into(), 4, t0());
        r.add_host(Some("Host".into()), t0());
        r
    }

    #[test]
    fn new_empty_room_has_only_host() {
        let r = fresh();
        assert_eq!(r.peer_count(), 1);
        assert!(r.is_open);
        assert_eq!(r.code, "ROOM01");
        assert_eq!(r.max_peers, 4);
        assert!(r.host().is_some());
        assert!(r.host().unwrap().is_host);
    }

    #[test]
    fn join_adds_peer() {
        let mut r = fresh();
        r.join("p2".into(), Some("Bob".into()), t0()).unwrap();
        assert_eq!(r.peer_count(), 2);
        assert!(r.peers.contains_key("p2"));
        assert!(!r.peers["p2"].is_host);
    }

    #[test]
    fn join_rejects_when_full() {
        // max_peers=2 ; host occupies one slot
        let mut r = Room::new_at("X".into(), "h".into(), 2, t0());
        r.add_host(None, t0());
        r.join("p2".into(), None, t0()).unwrap();
        let err = r.join("p3".into(), None, t0()).unwrap_err();
        assert_eq!(err, RoomErr::Full);
    }

    #[test]
    fn join_rejects_when_closed() {
        let mut r = fresh();
        r.close();
        assert!(!r.is_open);
        let err = r.join("p2".into(), None, t0()).unwrap_err();
        assert_eq!(err, RoomErr::Closed);
    }

    #[test]
    fn join_rejects_duplicate() {
        let mut r = fresh();
        r.join("p2".into(), None, t0()).unwrap();
        let err = r.join("p2".into(), None, t0()).unwrap_err();
        assert_eq!(err, RoomErr::AlreadyJoined);
    }

    #[test]
    fn leave_removes_peer() {
        let mut r = fresh();
        r.join("p2".into(), None, t0()).unwrap();
        assert_eq!(r.peer_count(), 2);
        r.leave("p2").unwrap();
        assert_eq!(r.peer_count(), 1);
        assert_eq!(r.leave("p2").unwrap_err(), RoomErr::NotFound);
    }

    #[test]
    fn touch_updates_last_seen() {
        let mut r = fresh();
        r.join("p2".into(), None, t0()).unwrap();
        let later = t0() + 5_000_000;
        r.touch("p2", later).unwrap();
        assert_eq!(r.peers["p2"].last_seen_micros, later);
        assert_eq!(r.touch("ghost", later).unwrap_err(), RoomErr::NotFound);
    }

    #[test]
    fn evict_idle_removes_stale() {
        let mut r = fresh();
        r.join("stale".into(), None, t0()).unwrap();
        r.join("fresh".into(), None, t0()).unwrap();
        // 60s later — both stale by clock — but we touch "fresh"
        let later = t0() + 60 * 1_000_000;
        r.touch("fresh", later).unwrap();
        let evicted = r.evict_idle(later);
        assert_eq!(evicted, vec!["stale".to_string()]);
        assert!(r.peers.contains_key("fresh"));
        assert!(r.peers.contains_key("host"));
    }

    #[test]
    fn evict_idle_keeps_fresh_and_host() {
        let mut r = fresh();
        r.join("p2".into(), None, t0()).unwrap();
        // Don't touch anyone ; advance well past idle threshold
        let later = t0() + 60 * 1_000_000;
        let evicted = r.evict_idle(later);
        // host is never evicted even when idle
        assert_eq!(evicted, vec!["p2".to_string()]);
        assert!(r.peers.contains_key("host"));
    }

    #[test]
    fn close_rejects_future_joins() {
        let mut r = fresh();
        r.close();
        for id in ["a", "b", "c"] {
            assert_eq!(
                r.join(id.into(), None, t0()).unwrap_err(),
                RoomErr::Closed
            );
        }
    }

    #[test]
    fn expired_room_rejects_join() {
        let mut r = Room::new_at("X".into(), "h".into(), 4, t0());
        r.add_host(None, t0());
        let after = r.expires_at_micros + 1;
        assert_eq!(
            r.join("p2".into(), None, after).unwrap_err(),
            RoomErr::Expired
        );
    }
}
