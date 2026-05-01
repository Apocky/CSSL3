// § stub_supabase.rs : ABI-shape Supabase transport — captures sends,
// returns empty polls, never touches the network.
//
// The real Supabase REST + realtime impls land in a downstream wave (they
// need an HTTP client crate that we deliberately keep out of stage-0). This
// stub exists to lock the trait-impl shape and let upstream wiring (the
// loa-host orchestrator) develop against the same `Box<dyn MpTransport>`
// it will use against the live backend.

use cssl_host_multiplayer_signaling::SignalingMessage;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Mutex;

use crate::trait_def::{
    MpTransport, TransportErr, TransportResult, TRANSPORT_CAP_RECV, TRANSPORT_CAP_SEND,
};

/// ABI-shape Supabase transport. Holds backend coordinates (URL + anon key)
/// for trait-shape parity with the real impl, but never opens a connection.
/// Captures every send + poll call for test introspection.
#[derive(Debug)]
pub struct StubSupabaseTransport {
    /// Supabase project URL — stored for trait-shape parity with the live
    /// impl, never read.
    url: String,
    /// Public anon key — stored for trait-shape parity, never read. Stored
    /// (not just discarded) so the field stack-layout matches the live impl
    /// when it lands.
    anon_key: String,
    /// Capability bits.
    caps: u32,
    /// Captured `send` invocations ; available via `sent_messages`.
    sent_log: Mutex<Vec<SignalingMessage>>,
    /// Captured `poll` invocations ; (room_id, peer_id, since_id) tuples.
    poll_log: Mutex<Vec<(String, String, u64)>>,
    /// If `Some(n)`, the (n+1)-th send (1-indexed) and every later send
    /// returns `TransportErr::ServerErr` instead of succeeding. Lets tests
    /// drive router-fallback logic deterministically. `None` means never
    /// fail.
    fail_after_n: Option<u32>,
    /// Counter for `fail_after_n` ; increments before the threshold check.
    send_count: AtomicU32,
    /// Monotonic id counter for synthetic `sent_id` values.
    next_id: AtomicU64,
}

impl StubSupabaseTransport {
    /// Construct a new stub. `url` and `anon_key` are stored verbatim ;
    /// `caps` is the capability-bit mask.
    pub fn new(url: String, anon_key: String, caps: u32) -> Self {
        Self {
            url,
            anon_key,
            caps,
            sent_log: Mutex::new(Vec::new()),
            poll_log: Mutex::new(Vec::new()),
            fail_after_n: None,
            send_count: AtomicU32::new(0),
            next_id: AtomicU64::new(1),
        }
    }

    /// Builder : configure the stub to start failing after `n` successful
    /// sends. Used by router-fallback tests.
    #[must_use]
    pub fn with_fail_after(mut self, n: u32) -> Self {
        self.fail_after_n = Some(n);
        self
    }

    /// Test introspection : every message we received via `send`, in order.
    pub fn sent_messages(&self) -> Vec<SignalingMessage> {
        self.sent_log
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default()
    }

    /// Test introspection : every poll-call we received, in order.
    pub fn poll_calls(&self) -> Vec<(String, String, u64)> {
        self.poll_log
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default()
    }

    /// Test introspection : the URL the stub was constructed with.
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Test introspection : the anon key the stub was constructed with.
    pub fn anon_key(&self) -> &str {
        &self.anon_key
    }
}

impl MpTransport for StubSupabaseTransport {
    fn name(&self) -> &'static str {
        "stub-supabase"
    }

    fn send(&self, msg: &SignalingMessage) -> Result<TransportResult, TransportErr> {
        if self.caps & TRANSPORT_CAP_SEND == 0 {
            return Err(TransportErr::CapDenied(TRANSPORT_CAP_SEND));
        }
        msg.validate().map_err(|_| TransportErr::MalformedMessage)?;

        let attempt = self.send_count.fetch_add(1, Ordering::Relaxed) + 1;
        if let Some(threshold) = self.fail_after_n {
            if attempt > threshold {
                return Err(TransportErr::ServerErr(format!(
                    "stub configured to fail after n={threshold} (attempt {attempt})"
                )));
            }
        }

        // Capture the send.
        if let Ok(mut g) = self.sent_log.lock() {
            g.push(msg.clone());
        }

        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        Ok(TransportResult {
            sent_id: id,
            latency_ms: 0,
        })
    }

    fn poll(
        &self,
        room_id: &str,
        peer_id: &str,
        since_id: u64,
    ) -> Result<Vec<SignalingMessage>, TransportErr> {
        if self.caps & TRANSPORT_CAP_RECV == 0 {
            return Err(TransportErr::CapDenied(TRANSPORT_CAP_RECV));
        }
        if let Ok(mut g) = self.poll_log.lock() {
            g.push((room_id.to_owned(), peer_id.to_owned(), since_id));
        }
        // Stub : always returns empty. The live impl will hit the Supabase
        // REST endpoint defined by W4 migration 0006.
        Ok(vec![])
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

    fn mk(from: &str, to: &str) -> SignalingMessage {
        SignalingMessage {
            id: 0,
            from_peer: from.into(),
            to_peer: to.into(),
            kind: MessageKind::Hello,
            payload: vec![],
            ts_micros: 0,
        }
    }

    #[test]
    fn stub_records_every_send_in_order() {
        let t = StubSupabaseTransport::new(
            "https://example.supabase.co".into(),
            "anon-key-12345".into(),
            TRANSPORT_CAP_BOTH,
        );
        for to in ["alice", "bob", "carol"] {
            t.send(&mk("server", to)).expect("ok");
        }
        let sent = t.sent_messages();
        assert_eq!(sent.len(), 3);
        assert_eq!(sent[0].to_peer, "alice");
        assert_eq!(sent[1].to_peer, "bob");
        assert_eq!(sent[2].to_peer, "carol");

        // url+anon_key round-trippable
        assert_eq!(t.url(), "https://example.supabase.co");
        assert_eq!(t.anon_key(), "anon-key-12345");
    }

    #[test]
    fn stub_poll_returns_empty_records_call() {
        let t = StubSupabaseTransport::new(
            "url".into(),
            "key".into(),
            TRANSPORT_CAP_BOTH,
        );
        let got = t.poll("room-X", "peer-Y", 42).expect("ok");
        assert!(got.is_empty());

        let calls = t.poll_calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0], ("room-X".to_string(), "peer-Y".to_string(), 42));
    }

    #[test]
    fn cap_deny_still_blocks_in_stub() {
        // recv-only stub : send is denied, poll is allowed
        let t = StubSupabaseTransport::new(
            "url".into(),
            "key".into(),
            TRANSPORT_CAP_RECV,
        );
        let err = t.send(&mk("a", "b")).expect_err("send must be denied");
        assert_eq!(err, TransportErr::CapDenied(TRANSPORT_CAP_SEND));
        // poll still works
        assert!(t.poll("r", "p", 0).expect("poll ok").is_empty());
    }

    #[test]
    fn fail_after_n_respects_counter() {
        let t = StubSupabaseTransport::new(
            "url".into(),
            "key".into(),
            TRANSPORT_CAP_BOTH,
        )
        .with_fail_after(2);

        // first 2 succeed
        assert!(t.send(&mk("a", "b")).is_ok());
        assert!(t.send(&mk("a", "b")).is_ok());
        // 3rd fails
        let e = t.send(&mk("a", "b")).expect_err("must fail");
        match e {
            TransportErr::ServerErr(msg) => {
                assert!(msg.contains("attempt 3"), "msg = {msg}");
            }
            other => panic!("expected ServerErr, got {other:?}"),
        }
        // and continues to fail
        assert!(matches!(
            t.send(&mk("a", "b")),
            Err(TransportErr::ServerErr(_))
        ));
        // sent_log only contains the 2 successful sends
        assert_eq!(t.sent_messages().len(), 2);
    }

    #[test]
    fn sent_history_roundtrip_preserves_full_message() {
        let t = StubSupabaseTransport::new(
            "url".into(),
            "key".into(),
            TRANSPORT_CAP_BOTH,
        );
        let m = SignalingMessage {
            id: 7,
            from_peer: "alice".into(),
            to_peer: "bob".into(),
            kind: MessageKind::Offer,
            payload: vec![0xde, 0xad, 0xbe, 0xef],
            ts_micros: 1_700_000_000_000_000,
        };
        t.send(&m).expect("ok");
        let sent = t.sent_messages();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0], m, "captured message must be byte-equal");
    }
}
