// § cap_gating.rs : capability-bit gate tests for RealSupabaseTransport
//
// Three tests :
//   (a) caps() reports the constructed bits exactly
//   (b) send-without-SEND fails fast (no HTTP call) with CapDenied
//   (c) poll-without-RECV fails fast (no HTTP call) with CapDenied

use cssl_host_mp_transport_real::{
    HttpClient, HttpReq, HttpResp, HttpTransportErr, MpTransport, RealSupabaseTransport,
    SupabaseConfig, TransportErr, TRANSPORT_CAP_BOTH, TRANSPORT_CAP_RECV, TRANSPORT_CAP_SEND,
};
use cssl_host_multiplayer_signaling::{MessageKind, SignalingMessage};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

/// A trip-wire HTTP client : asserts via panic if it's ever called. Used
/// by the cap-gate tests to prove the transport short-circuits BEFORE
/// reaching the HTTP layer.
#[derive(Debug)]
struct PanicClient {
    calls: AtomicU32,
}

impl PanicClient {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            calls: AtomicU32::new(0),
        })
    }
    fn call_count(&self) -> u32 {
        self.calls.load(Ordering::Relaxed)
    }
}

#[derive(Debug)]
struct PanicClientWrap(Arc<PanicClient>);

impl HttpClient for PanicClientWrap {
    fn execute(&self, _req: &HttpReq) -> Result<HttpResp, HttpTransportErr> {
        self.0.calls.fetch_add(1, Ordering::Relaxed);
        panic!("PanicClient was called — transport failed to short-circuit cap-gate");
    }
}

fn cfg() -> SupabaseConfig {
    SupabaseConfig::new("https://test.supabase.co", "anon-key")
}

fn mk(from: &str, to: &str) -> SignalingMessage {
    SignalingMessage {
        id: 1,
        from_peer: from.into(),
        to_peer: to.into(),
        kind: MessageKind::Hello,
        payload: vec![],
        ts_micros: 0,
    }
}

#[test]
fn caps_reports_constructed_bits() {
    let t1 = RealSupabaseTransport::new(cfg(), TRANSPORT_CAP_SEND);
    assert_eq!(t1.caps(), TRANSPORT_CAP_SEND);

    let t2 = RealSupabaseTransport::new(cfg(), TRANSPORT_CAP_RECV);
    assert_eq!(t2.caps(), TRANSPORT_CAP_RECV);

    let t3 = RealSupabaseTransport::new(cfg(), TRANSPORT_CAP_BOTH);
    assert_eq!(t3.caps(), TRANSPORT_CAP_BOTH);
    assert_eq!(t3.caps(), TRANSPORT_CAP_SEND | TRANSPORT_CAP_RECV);

    // No-cap transport reports 0.
    let t4 = RealSupabaseTransport::new(cfg(), 0);
    assert_eq!(t4.caps(), 0);
}

#[test]
fn send_without_send_cap_short_circuits_before_http() {
    let panic_client = PanicClient::new();
    let t = RealSupabaseTransport::new(cfg(), TRANSPORT_CAP_RECV)
        .with_http_client(Box::new(PanicClientWrap(panic_client.clone())));

    let err = t.send(&mk("alice", "bob")).expect_err("must deny");
    assert_eq!(err, TransportErr::CapDenied(TRANSPORT_CAP_SEND));
    assert_eq!(
        panic_client.call_count(),
        0,
        "transport must short-circuit before reaching HTTP layer"
    );
}

#[test]
fn poll_without_recv_cap_short_circuits_before_http() {
    let panic_client = PanicClient::new();
    let t = RealSupabaseTransport::new(cfg(), TRANSPORT_CAP_SEND)
        .with_http_client(Box::new(PanicClientWrap(panic_client.clone())));

    let err = t.poll("room", "peer", 0).expect_err("must deny");
    assert_eq!(err, TransportErr::CapDenied(TRANSPORT_CAP_RECV));
    assert_eq!(
        panic_client.call_count(),
        0,
        "transport must short-circuit before reaching HTTP layer"
    );
}

#[test]
fn name_returns_supabase_rest() {
    let t = RealSupabaseTransport::new(cfg(), TRANSPORT_CAP_BOTH);
    assert_eq!(t.name(), "supabase-rest");
}
