// § transport_send_validate.rs : send-path tests for RealSupabaseTransport
//
// Five tests covering the per-call send pipeline :
//   (a) malformed message → MalformedMessage
//   (b) cap-deny without TRANSPORT_CAP_SEND → CapDenied
//   (c) 429 with Retry-After → Backoff(parsed)
//   (d) timeout from HTTP layer → Timeout
//   (e) success → audit_emit fires + sent_id is non-zero

use cssl_host_mp_transport_real::{
    HttpClient, HttpResp, HttpTransportErr, MockHttpClient, MpTransport, NoopAuditSink,
    RealSupabaseTransport, RecordingAuditSink, SupabaseConfig, TransportErr,
    TRANSPORT_CAP_BOTH, TRANSPORT_CAP_RECV, TRANSPORT_CAP_SEND,
};
use cssl_host_multiplayer_signaling::{MessageKind, SignalingMessage};
use std::sync::Arc;

fn cfg() -> SupabaseConfig {
    SupabaseConfig::new("https://test.supabase.co", "anon-key").with_default_backoff_ms(2_000)
}

fn mk(from: &str, to: &str) -> SignalingMessage {
    SignalingMessage {
        id: 1,
        from_peer: from.into(),
        to_peer: to.into(),
        kind: MessageKind::Hello,
        payload: vec![1, 2, 3],
        ts_micros: 1_700_000_000_000_000,
    }
}

fn ok_response_with_id(id: u64) -> HttpResp {
    let body = serde_json::to_vec(&serde_json::json!([{
        "id": id,
        "from_peer": "x",
        "to_peer": "y",
        "kind": "hello",
        "payload": {"b64": ""},
        "delivered": false,
        "created_at": "",
    }]))
    .unwrap();
    HttpResp {
        status: 201,
        headers: vec![("Content-Type".into(), "application/json".into())],
        body,
    }
}

#[test]
fn send_rejects_malformed_message() {
    let mock = MockHttpClient::new();
    let t = RealSupabaseTransport::new(cfg(), TRANSPORT_CAP_BOTH)
        .with_http_client(Box::new(mock));

    // empty from_peer → fails validate.
    let bad = SignalingMessage {
        id: 1,
        from_peer: String::new(),
        to_peer: "x".into(),
        kind: MessageKind::Hello,
        payload: vec![],
        ts_micros: 0,
    };
    let err = t.send(&bad).expect_err("must fail");
    assert_eq!(err, TransportErr::MalformedMessage);
}

#[test]
fn send_without_send_cap_returns_cap_denied() {
    // RECV-only — send must short-circuit without ever touching the
    // mock's call log.
    let mock: Arc<MockHttpClient> = Arc::new(MockHttpClient::new());
    let mock_dyn: Box<dyn HttpClient> = Box::new(MockHttpClient::new());
    let t = RealSupabaseTransport::new(cfg(), TRANSPORT_CAP_RECV)
        .with_http_client(mock_dyn);

    let err = t.send(&mk("alice", "bob")).expect_err("must deny");
    assert_eq!(err, TransportErr::CapDenied(TRANSPORT_CAP_SEND));
    // Mock should not have seen any call.
    assert_eq!(mock.calls().len(), 0);
}

#[test]
fn send_429_with_retry_after_parses_backoff_ms() {
    let mock = MockHttpClient::new();
    mock.add_response(
        "https://test.supabase.co/rest/v1/signaling_messages",
        HttpResp {
            status: 429,
            headers: vec![("Retry-After".into(), "7".into())],
            body: br#"{"error":"rate-limited"}"#.to_vec(),
        },
    );

    let t = RealSupabaseTransport::new(cfg(), TRANSPORT_CAP_BOTH)
        .with_http_client(Box::new(mock));
    let err = t.send(&mk("alice", "bob")).expect_err("must backoff");
    // 7 seconds → 7000 ms.
    assert_eq!(err, TransportErr::Backoff(7_000));
}

#[test]
fn send_429_without_retry_after_falls_back_to_default() {
    let mock = MockHttpClient::new();
    mock.add_response(
        "https://test.supabase.co/rest/v1/signaling_messages",
        HttpResp {
            status: 429,
            headers: vec![],
            body: vec![],
        },
    );

    let t = RealSupabaseTransport::new(
        SupabaseConfig::new("https://test.supabase.co", "anon-key")
            .with_default_backoff_ms(1_500),
        TRANSPORT_CAP_BOTH,
    )
    .with_http_client(Box::new(mock));

    let err = t.send(&mk("alice", "bob")).expect_err("must backoff");
    assert_eq!(err, TransportErr::Backoff(1_500));
}

#[test]
fn send_timeout_maps_to_transport_timeout() {
    let mock = MockHttpClient::new();
    mock.add_error(
        "https://test.supabase.co/rest/v1/signaling_messages",
        HttpTransportErr::Timeout,
    );

    let t = RealSupabaseTransport::new(cfg(), TRANSPORT_CAP_BOTH)
        .with_http_client(Box::new(mock));
    let err = t.send(&mk("alice", "bob")).expect_err("must timeout");
    assert_eq!(err, TransportErr::Timeout);
}

#[test]
fn send_success_emits_audit_and_returns_server_id() {
    let mock = MockHttpClient::new();
    mock.add_response(
        "https://test.supabase.co/rest/v1/signaling_messages",
        ok_response_with_id(42),
    );
    let sink: Arc<RecordingAuditSink> = Arc::new(RecordingAuditSink::new());

    let t = RealSupabaseTransport::new(cfg(), TRANSPORT_CAP_BOTH)
        .with_http_client(Box::new(mock))
        .with_audit_sink(sink.clone());

    let r = t.send(&mk("alice", "bob")).expect("must succeed");
    assert_eq!(r.sent_id, 42);

    // Audit captured the success event.
    assert_eq!(sink.count_kind("mp.send.ok"), 1);
    assert_eq!(sink.count_kind("mp.send.err"), 0);
}

#[test]
fn send_5xx_maps_to_server_err_with_status_in_message() {
    let mock = MockHttpClient::new();
    mock.add_response(
        "https://test.supabase.co/rest/v1/signaling_messages",
        HttpResp {
            status: 503,
            headers: vec![],
            body: b"upstream service unavailable".to_vec(),
        },
    );
    let t = RealSupabaseTransport::new(cfg(), TRANSPORT_CAP_BOTH)
        .with_http_client(Box::new(mock));
    let err = t.send(&mk("alice", "bob")).expect_err("must fail");
    match err {
        TransportErr::ServerErr(s) => {
            assert!(s.contains("503"), "msg should contain status: {s}");
            assert!(
                s.contains("upstream"),
                "msg should contain body snippet: {s}"
            );
        }
        other => panic!("expected ServerErr, got {other:?}"),
    }
}

#[test]
fn send_smoke_with_noop_sink_does_not_panic() {
    // Belt-and-suspenders : NoopAuditSink path must not panic on success.
    let mock = MockHttpClient::new();
    mock.add_response(
        "https://test.supabase.co/rest/v1/signaling_messages",
        ok_response_with_id(99),
    );
    let t = RealSupabaseTransport::new(cfg(), TRANSPORT_CAP_BOTH)
        .with_http_client(Box::new(mock))
        .with_audit_sink(Arc::new(NoopAuditSink));
    let r = t.send(&mk("alice", "bob")).expect("ok");
    assert_eq!(r.sent_id, 99);
}
