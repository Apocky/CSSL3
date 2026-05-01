// § transport_poll.rs : poll-path tests for RealSupabaseTransport
//
// Four tests :
//   (a) poll without TRANSPORT_CAP_RECV → CapDenied
//   (b) since_id is encoded into the URL filter (?id=gt.X)
//   (c) empty-result body → Ok(empty Vec)
//   (d) malformed-json body → ServerErr

use cssl_host_mp_transport_real::{
    HttpClient, HttpReq, HttpResp, HttpTransportErr, MockHttpClient, MpTransport,
    RealSupabaseTransport, RecordingAuditSink, SupabaseConfig, TransportErr,
    TRANSPORT_CAP_BOTH, TRANSPORT_CAP_RECV, TRANSPORT_CAP_SEND,
};
use std::sync::Arc;

/// Test helper : wrap an `Arc<MockHttpClient>` so it can be shared between
/// transport-construction and assertion-side without losing access to the
/// recorded calls. (The transport takes a `Box<dyn HttpClient>`, so we
/// can't share the same Arc through both sides without a wrapper.)
#[derive(Debug)]
struct ArcWrap(Arc<MockHttpClient>);

impl HttpClient for ArcWrap {
    fn execute(&self, req: &HttpReq) -> Result<HttpResp, HttpTransportErr> {
        self.0.execute(req)
    }
}

fn cfg() -> SupabaseConfig {
    SupabaseConfig::new("https://test.supabase.co", "anon-key")
}

#[test]
fn poll_without_recv_cap_returns_cap_denied() {
    // SEND-only transport — poll must fail before any HTTP.
    let mock = MockHttpClient::new();
    let t = RealSupabaseTransport::new(cfg(), TRANSPORT_CAP_SEND)
        .with_http_client(Box::new(mock));
    let err = t.poll("room-x", "peer-y", 0).expect_err("deny");
    assert_eq!(err, TransportErr::CapDenied(TRANSPORT_CAP_RECV));
}

#[test]
fn poll_encodes_since_id_in_url() {
    let inspect: Arc<MockHttpClient> = Arc::new(MockHttpClient::new());
    inspect.add_response(
        "https://test.supabase.co/rest/v1/signaling_messages",
        HttpResp {
            status: 200,
            headers: vec![],
            body: b"[]".to_vec(),
        },
    );

    let t = RealSupabaseTransport::new(cfg(), TRANSPORT_CAP_BOTH)
        .with_http_client(Box::new(ArcWrap(inspect.clone())));
    let _ = t.poll("room-X", "peer-Y", 100).expect("ok");

    let calls = inspect.calls();
    assert_eq!(calls.len(), 1);
    let url = &calls[0].url;
    assert!(url.contains("room_id=eq.room-X"), "url={url}");
    assert!(url.contains("to_peer.eq.peer-Y"), "url={url}");
    assert!(url.contains("id=gt.100"), "url={url}");
    assert!(url.contains("order=id.asc"), "url={url}");
    assert!(url.contains("limit=128"), "url={url}");
}

#[test]
fn poll_empty_array_body_returns_empty_vec() {
    let mock = MockHttpClient::new();
    mock.add_response(
        "https://test.supabase.co/rest/v1/signaling_messages",
        HttpResp {
            status: 200,
            headers: vec![],
            body: b"[]".to_vec(),
        },
    );
    let t = RealSupabaseTransport::new(cfg(), TRANSPORT_CAP_BOTH)
        .with_http_client(Box::new(mock));
    let got = t.poll("r", "p", 0).expect("ok");
    assert!(got.is_empty());
}

#[test]
fn poll_truly_empty_body_returns_empty_vec() {
    // Some Supabase responses return a 200 with a zero-length body when
    // the query yields no rows ; the transport must treat that as
    // semantic-empty rather than json-parse-failure.
    let mock = MockHttpClient::new();
    mock.add_response(
        "https://test.supabase.co/rest/v1/signaling_messages",
        HttpResp {
            status: 200,
            headers: vec![],
            body: vec![],
        },
    );
    let t = RealSupabaseTransport::new(cfg(), TRANSPORT_CAP_BOTH)
        .with_http_client(Box::new(mock));
    let got = t.poll("r", "p", 0).expect("ok");
    assert!(got.is_empty());
}

#[test]
fn poll_malformed_json_returns_server_err() {
    let mock = MockHttpClient::new();
    mock.add_response(
        "https://test.supabase.co/rest/v1/signaling_messages",
        HttpResp {
            status: 200,
            headers: vec![],
            body: b"this-is-not-json".to_vec(),
        },
    );
    let sink: Arc<RecordingAuditSink> = Arc::new(RecordingAuditSink::new());
    let t = RealSupabaseTransport::new(cfg(), TRANSPORT_CAP_BOTH)
        .with_http_client(Box::new(mock))
        .with_audit_sink(sink.clone());
    let err = t.poll("r", "p", 0).expect_err("must fail");
    match err {
        TransportErr::ServerErr(s) => {
            assert!(s.starts_with("malformed json"), "got: {s}");
        }
        other => panic!("expected ServerErr, got {other:?}"),
    }
    // Audit captured the parse-failure.
    assert_eq!(sink.count_kind("mp.poll.err"), 1);
}

#[test]
fn poll_success_emits_audit_with_count() {
    let mock = MockHttpClient::new();
    let body = serde_json::to_vec(&serde_json::json!([
        {
            "id": 10, "from_peer": "alice", "to_peer": "peer-Y",
            "kind": "hello", "payload": {"b64": ""},
            "delivered": false, "created_at": "",
        },
        {
            "id": 11, "from_peer": "alice", "to_peer": "peer-Y",
            "kind": "ping", "payload": {"b64": ""},
            "delivered": false, "created_at": "",
        },
    ]))
    .unwrap();
    mock.add_response(
        "https://test.supabase.co/rest/v1/signaling_messages",
        HttpResp {
            status: 200,
            headers: vec![],
            body,
        },
    );
    let sink: Arc<RecordingAuditSink> = Arc::new(RecordingAuditSink::new());
    let t = RealSupabaseTransport::new(cfg(), TRANSPORT_CAP_BOTH)
        .with_http_client(Box::new(mock))
        .with_audit_sink(sink.clone());

    let got = t.poll("r", "peer-Y", 0).expect("ok");
    assert_eq!(got.len(), 2);
    assert_eq!(got[0].id, 10);
    assert_eq!(got[1].id, 11);
    assert_eq!(sink.count_kind("mp.poll.ok"), 1);

    let evs = sink.events();
    let poll_ok = evs.iter().find(|e| e.kind == "mp.poll.ok").unwrap();
    assert_eq!(poll_ok.fields.get("count").map(String::as_str), Some("2"));
}
