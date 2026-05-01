// § loopback.rs : in-memory transport for tests + single-process LAN sessions.
//
// `LoopbackTransport` keeps a per-peer mailbox in a `Mutex<HashMap>`. `send`
// drops the message into the recipient's inbox (or every peer's inbox when
// `to_peer == "*"`) ; `poll` drains the inbox for messages whose
// monotonically-assigned id exceeds `since_id`. No network IO ; works only
// within a single process — the entire point is to let test-suites and
// single-machine multi-window LoA sessions run the signaling protocol without
// standing up a Supabase backend.

use cssl_host_multiplayer_signaling::SignalingMessage;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use crate::trait_def::{
    MpTransport, TransportErr, TransportResult, TRANSPORT_CAP_RECV, TRANSPORT_CAP_SEND,
};

/// In-memory transport. Cap-gated like the real backends — pass
/// `TRANSPORT_CAP_BOTH` for the typical case, or restrict caps when the test
/// is exercising cap-deny behaviour.
#[derive(Debug)]
pub struct LoopbackTransport {
    /// `peer_id` → ordered (server-id, message) inbox. Entries are appended
    /// in `send` and drained in `poll` ; the per-peer Vec stays sorted by
    /// `server_id` because the global `next_id` is monotonic.
    peers: Mutex<HashMap<String, Vec<(u64, SignalingMessage)>>>,
    /// Monotonic global-id counter ; assigned per `send` call.
    next_id: AtomicU64,
    /// Capability bits this transport exposes.
    caps: u32,
}

impl LoopbackTransport {
    /// Construct a fresh loopback with the given capability bits. Pass
    /// `TRANSPORT_CAP_BOTH` for the typical bidirectional case ; pass
    /// `TRANSPORT_CAP_SEND` (or `_RECV`) only to exercise cap-deny tests.
    pub fn new(caps: u32) -> Self {
        Self {
            peers: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            caps,
        }
    }

    /// Test introspection : how many messages are currently buffered for
    /// `peer_id`. Returns 0 if the peer has no inbox yet.
    pub fn inbox_len(&self, peer_id: &str) -> usize {
        self.peers
            .lock()
            .map(|g| g.get(peer_id).map_or(0, Vec::len))
            .unwrap_or(0)
    }
}

impl MpTransport for LoopbackTransport {
    fn name(&self) -> &'static str {
        "loopback"
    }

    fn send(&self, msg: &SignalingMessage) -> Result<TransportResult, TransportErr> {
        if self.caps & TRANSPORT_CAP_SEND == 0 {
            return Err(TransportErr::CapDenied(TRANSPORT_CAP_SEND));
        }
        msg.validate().map_err(|_| TransportErr::MalformedMessage)?;

        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        // § scoped lock : tighten Mutex hold-time so concurrent senders /
        // pollers don't pile up behind a long-lived guard. clippy's
        // `significant_drop_tightening` lint nudges this pattern.
        {
            let mut peers = self
                .peers
                .lock()
                .map_err(|_| TransportErr::ServerErr("loopback mutex poisoned".into()))?;

            if msg.is_broadcast() {
                // Broadcast to every known peer except the sender.
                let recipients: Vec<String> = peers
                    .keys()
                    .filter(|k| **k != msg.from_peer)
                    .cloned()
                    .collect();
                for r in recipients {
                    peers.entry(r).or_default().push((id, msg.clone()));
                }
                // Ensure the sender's own peer-entry exists (so it can be
                // polled for, even if no one else has joined yet) — without
                // queuing the broadcast back to the originator.
                peers.entry(msg.from_peer.clone()).or_default();
            } else {
                peers
                    .entry(msg.to_peer.clone())
                    .or_default()
                    .push((id, msg.clone()));
            }
        }

        Ok(TransportResult {
            sent_id: id,
            latency_ms: 0,
        })
    }

    fn poll(
        &self,
        _room_id: &str,
        peer_id: &str,
        since_id: u64,
    ) -> Result<Vec<SignalingMessage>, TransportErr> {
        if self.caps & TRANSPORT_CAP_RECV == 0 {
            return Err(TransportErr::CapDenied(TRANSPORT_CAP_RECV));
        }
        let mut peers = self
            .peers
            .lock()
            .map_err(|_| TransportErr::ServerErr("loopback mutex poisoned".into()))?;
        let inbox = peers.entry(peer_id.to_owned()).or_default();
        // Drain entries strictly newer than `since_id`. IDs are monotonic
        // and pushed in order, so a single `retain` walk both yields the
        // drained items and shrinks the inbox.
        let mut out = Vec::with_capacity(inbox.len());
        inbox.retain(|(id, msg)| {
            if *id > since_id {
                out.push(msg.clone());
                false
            } else {
                true
            }
        });
        drop(peers);
        Ok(out)
    }

    fn caps(&self) -> u32 {
        self.caps
    }
}

// ─── tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trait_def::{TRANSPORT_CAP_BOTH, TRANSPORT_CAP_RECV};
    use cssl_host_multiplayer_signaling::{MessageKind, SignalingMessage};
    use std::sync::Arc;
    use std::thread;

    fn mk(from: &str, to: &str, kind: MessageKind) -> SignalingMessage {
        SignalingMessage {
            id: 1,
            from_peer: from.into(),
            to_peer: to.into(),
            kind,
            payload: vec![],
            ts_micros: 0,
        }
    }

    #[test]
    fn send_then_recv_loopback_roundtrip() {
        let t = LoopbackTransport::new(TRANSPORT_CAP_BOTH);
        let m = mk("alice", "bob", MessageKind::Hello);
        let r = t.send(&m).expect("send ok");
        assert_eq!(r.sent_id, 1);

        let got = t.poll("room", "bob", 0).expect("poll ok");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].from_peer, "alice");
        assert_eq!(got[0].kind, MessageKind::Hello);

        // second poll : inbox now empty
        let again = t.poll("room", "bob", 0).expect("poll ok");
        assert!(again.is_empty());
    }

    #[test]
    fn cap_deny_send_when_only_recv() {
        let t = LoopbackTransport::new(TRANSPORT_CAP_RECV);
        let m = mk("alice", "bob", MessageKind::Hello);
        let err = t.send(&m).expect_err("send should be denied");
        assert_eq!(err, TransportErr::CapDenied(TRANSPORT_CAP_SEND));
    }

    #[test]
    fn multi_peer_inbox_isolation() {
        let t = LoopbackTransport::new(TRANSPORT_CAP_BOTH);
        t.send(&mk("a", "b", MessageKind::Hello)).expect("ok");
        t.send(&mk("a", "c", MessageKind::Hello)).expect("ok");
        t.send(&mk("a", "b", MessageKind::Ping)).expect("ok");

        let b = t.poll("room", "b", 0).expect("ok");
        let c = t.poll("room", "c", 0).expect("ok");
        assert_eq!(b.len(), 2, "b got 2 msgs");
        assert_eq!(c.len(), 1, "c got 1 msg");
        // ordering preserved
        assert_eq!(b[0].kind, MessageKind::Hello);
        assert_eq!(b[1].kind, MessageKind::Ping);
    }

    #[test]
    fn since_id_filters_drained_messages() {
        let t = LoopbackTransport::new(TRANSPORT_CAP_BOTH);
        // first send : id=1, drains
        t.send(&mk("a", "b", MessageKind::Hello)).expect("ok");
        let got = t.poll("room", "b", 0).expect("ok");
        assert_eq!(got.len(), 1);

        // since_id=999 : nothing should come back even after a new send
        // because new id will be 2 (≤ 999)
        t.send(&mk("a", "b", MessageKind::Ping)).expect("ok");
        let got2 = t.poll("room", "b", 999).expect("ok");
        assert!(got2.is_empty());
        // but with since_id=1 we get the new one
        let got3 = t.poll("room", "b", 1).expect("ok");
        assert_eq!(got3.len(), 1);
        assert_eq!(got3[0].kind, MessageKind::Ping);
    }

    #[test]
    fn broadcast_fans_out_to_all_known_peers() {
        let t = LoopbackTransport::new(TRANSPORT_CAP_BOTH);
        // First : prime the peer set with two unicasts so the broadcast has
        // recipients. The loopback only knows about peers it has seen as
        // recipients in `send`.
        t.send(&mk("a", "b", MessageKind::Hello)).expect("ok");
        t.send(&mk("a", "c", MessageKind::Hello)).expect("ok");

        let star = mk("a", "*", MessageKind::RoomState);
        t.send(&star).expect("ok");

        // both b and c should now have the broadcast (each had 1 unicast +
        // 1 broadcast)
        let b = t.poll("room", "b", 0).expect("ok");
        let c = t.poll("room", "c", 0).expect("ok");
        assert_eq!(b.len(), 2);
        assert_eq!(c.len(), 2);
        assert!(b.iter().any(|m| m.kind == MessageKind::RoomState));
        assert!(c.iter().any(|m| m.kind == MessageKind::RoomState));

        // sender 'a' should NOT have echoed itself
        let a = t.poll("room", "a", 0).expect("ok");
        assert!(a.is_empty(), "broadcast must not echo back to sender");
    }

    #[test]
    fn concurrent_multipeer_no_corruption() {
        let t = Arc::new(LoopbackTransport::new(TRANSPORT_CAP_BOTH));
        let mut handles = vec![];
        // 4 senders × 25 messages = 100 sends across 4 inboxes
        for i in 0..4u32 {
            let t = Arc::clone(&t);
            handles.push(thread::spawn(move || {
                let from = format!("p{i}");
                let to = format!("p{}", (i + 1) % 4);
                for _ in 0..25 {
                    let mut m = SignalingMessage {
                        id: 0,
                        from_peer: from.clone(),
                        to_peer: to.clone(),
                        kind: MessageKind::Ping,
                        payload: vec![],
                        ts_micros: 0,
                    };
                    m.id = i.into();
                    t.send(&m).expect("send ok");
                }
            }));
        }
        for h in handles {
            h.join().expect("thread panic");
        }
        // each peer should have exactly 25 inbound (from its single sender)
        for i in 0..4u32 {
            let inbox = t.poll("room", &format!("p{i}"), 0).expect("poll ok");
            assert_eq!(inbox.len(), 25, "peer p{i} expected 25 msgs");
        }
    }
}
