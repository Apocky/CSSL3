// § peer.rs : Peer record + idle-detection helpers
//
// `Peer` is the per-participant record kept inside a `Room`. The host vs.
// non-host distinction matters because only the host owns room-lifecycle
// transitions (open/close, eviction policy). Liveness is tracked via
// `last_seen_micros` ; sweepers compare against `IDLE_THRESHOLD_SECS` to
// decide whether to evict a stale peer.

use serde::{Deserialize, Serialize};

/// Number of seconds without a `touch` after which a peer is considered idle
/// and eligible for eviction. 30 s matches typical NAT-keepalive cadences
/// (15 s WebRTC heartbeat × 2-strikes-and-out).
pub const IDLE_THRESHOLD_SECS: u64 = 30;

/// Per-participant record kept inside a `Room`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Peer {
    /// Opaque player-id (typically a UUID-string).
    pub player_id: String,
    /// Optional human-facing display name ; absent when the player has not
    /// provided one yet.
    pub display_name: Option<String>,
    /// Wall-clock at first-join (microseconds since UNIX epoch).
    pub joined_at_micros: u64,
    /// Wall-clock at most-recent activity ; `evict_idle` compares this
    /// against `IDLE_THRESHOLD_SECS`.
    pub last_seen_micros: u64,
    /// True if this peer is the room-host. Exactly one peer per room
    /// should have this flag set.
    pub is_host: bool,
}

impl Peer {
    /// Construct a new non-host peer joined at `now_micros`.
    pub fn new(player_id: String, display_name: Option<String>, now_micros: u64) -> Self {
        Self {
            player_id,
            display_name,
            joined_at_micros: now_micros,
            last_seen_micros: now_micros,
            is_host: false,
        }
    }

    /// Construct a new host peer joined at `now_micros`.
    pub fn new_host(player_id: String, display_name: Option<String>, now_micros: u64) -> Self {
        Self {
            is_host: true,
            ..Self::new(player_id, display_name, now_micros)
        }
    }

    /// Update `last_seen_micros` to `now_micros`. Called on every inbound
    /// message from this peer.
    pub fn touch(&mut self, now_micros: u64) {
        // saturating-max so out-of-order timestamps never rewind liveness
        self.last_seen_micros = self.last_seen_micros.max(now_micros);
    }

    /// Seconds since this peer was last seen, computed against `now_micros`.
    /// Returns `0` if `now_micros` is in the past relative to `last_seen`.
    pub fn idle_for_secs(&self, now_micros: u64) -> u64 {
        let delta = now_micros.saturating_sub(self.last_seen_micros);
        delta / 1_000_000
    }

    /// True iff `idle_for_secs(now_micros) >= IDLE_THRESHOLD_SECS`.
    pub fn is_idle(&self, now_micros: u64) -> bool {
        self.idle_for_secs(now_micros) >= IDLE_THRESHOLD_SECS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t0() -> u64 {
        1_000_000_000
    } // arbitrary baseline

    #[test]
    fn new_peer_not_idle() {
        let p = Peer::new("p1".into(), Some("Alice".into()), t0());
        assert!(!p.is_idle(t0()));
        assert_eq!(p.idle_for_secs(t0()), 0);
        // 1 s later still fresh
        assert!(!p.is_idle(t0() + 1_000_000));
        assert_eq!(p.idle_for_secs(t0() + 1_000_000), 1);
    }

    #[test]
    fn idle_after_30s() {
        let p = Peer::new("p1".into(), None, t0());
        // exactly at the threshold
        let then = t0() + IDLE_THRESHOLD_SECS * 1_000_000;
        assert!(p.is_idle(then));
        // 1 µs before the threshold not idle
        assert!(!p.is_idle(then - 1));
    }

    #[test]
    fn last_seen_update_resets_idle() {
        let mut p = Peer::new("p1".into(), None, t0());
        let stale = t0() + 60 * 1_000_000;
        assert!(p.is_idle(stale));
        p.touch(stale);
        assert!(!p.is_idle(stale));
        assert_eq!(p.last_seen_micros, stale);

        // out-of-order timestamp must not rewind liveness
        p.touch(t0());
        assert_eq!(p.last_seen_micros, stale);
    }

    #[test]
    fn serde_roundtrip() {
        let p = Peer {
            player_id: "p1".into(),
            display_name: Some("Alice".into()),
            joined_at_micros: 100,
            last_seen_micros: 200,
            is_host: true,
        };
        let json = serde_json::to_string(&p).unwrap();
        let back: Peer = serde_json::from_str(&json).unwrap();
        assert_eq!(p, back);

        // None display_name also round-trips
        let p2 = Peer::new("p2".into(), None, 0);
        let json2 = serde_json::to_string(&p2).unwrap();
        assert_eq!(serde_json::from_str::<Peer>(&json2).unwrap(), p2);
    }

    #[test]
    fn host_flag_preserved() {
        let h = Peer::new_host("host".into(), None, t0());
        assert!(h.is_host);
        let g = Peer::new("guest".into(), None, t0());
        assert!(!g.is_host);

        // round-trip preserves the host bit
        let json = serde_json::to_string(&h).unwrap();
        let back: Peer = serde_json::from_str(&json).unwrap();
        assert!(back.is_host);
    }
}
