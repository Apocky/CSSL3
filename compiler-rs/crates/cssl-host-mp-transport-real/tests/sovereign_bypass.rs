// § sovereign_bypass.rs : tests for SovereignBypassRecorder + transport-wire
//
// Three tests :
//   (a) recorder defaults disabled
//   (b) enable() flips the flag
//   (c) bypass-active causes audit-event before send

use cssl_host_mp_transport_real::{
    HttpResp, MockHttpClient, MpTransport, RealSupabaseTransport, RecordingAuditSink,
    SovereignBypassRecorder, SupabaseConfig, TRANSPORT_CAP_BOTH,
};
use cssl_host_multiplayer_signaling::{MessageKind, SignalingMessage};
use std::sync::Arc;

#[test]
fn recorder_defaults_disabled() {
    let r = SovereignBypassRecorder::new();
    assert!(!r.is_active());
    assert_eq!(r.records(), 0);
}

#[test]
fn recorder_enable_flips_flag() {
    let r = SovereignBypassRecorder::new();
    assert!(!r.is_active());
    r.enable();
    assert!(r.is_active());
    r.disable();
    assert!(!r.is_active());
}

#[test]
fn bypass_active_causes_audit_event_before_send() {
    // Build a transport with a recording sink + an enabled bypass
    // recorder + a mock that returns a successful POST. After a `send`,
    // the recording sink must contain a `mp.sovereign.bypass` event
    // BEFORE the `mp.send.ok` event.
    let mock = MockHttpClient::new();
    let body = serde_json::to_vec(&serde_json::json!([{
        "id": 7, "from_peer": "alice", "to_peer": "bob",
        "kind": "offer", "payload": {"b64": ""},
        "delivered": false, "created_at": "",
    }]))
    .unwrap();
    mock.add_response(
        "https://test.supabase.co/rest/v1/signaling_messages",
        HttpResp {
            status: 201,
            headers: vec![],
            body,
        },
    );

    let sink: Arc<RecordingAuditSink> = Arc::new(RecordingAuditSink::new());
    let bypass: Arc<SovereignBypassRecorder> = Arc::new(SovereignBypassRecorder::new());
    bypass.enable();

    let t = RealSupabaseTransport::new(
        SupabaseConfig::new("https://test.supabase.co", "anon-key"),
        TRANSPORT_CAP_BOTH,
    )
    .with_http_client(Box::new(mock))
    .with_audit_sink(sink.clone())
    .with_bypass_recorder(bypass.clone());

    let msg = SignalingMessage {
        id: 0,
        from_peer: "alice".into(),
        to_peer: "bob".into(),
        kind: MessageKind::Offer,
        payload: vec![1, 2, 3],
        ts_micros: 1_700_000_000_000_000,
    };
    let r = t.send(&msg).expect("ok");
    assert_eq!(r.sent_id, 7);

    // Bypass record-counter incremented.
    assert_eq!(bypass.records(), 1);

    // Audit-stream : bypass-event precedes the send.ok event.
    let evs = sink.events();
    assert!(
        evs.len() >= 2,
        "expected ≥2 events, got {}",
        evs.len()
    );
    let bypass_idx = evs
        .iter()
        .position(|e| e.kind == "mp.sovereign.bypass")
        .expect("bypass event present");
    let send_ok_idx = evs
        .iter()
        .position(|e| e.kind == "mp.send.ok")
        .expect("send.ok event present");
    assert!(
        bypass_idx < send_ok_idx,
        "bypass must precede send.ok ; got bypass@{bypass_idx} send.ok@{send_ok_idx}"
    );

    // The bypass event carries msg_kind + ts_micros.
    let by = &evs[bypass_idx];
    assert_eq!(
        by.fields.get("msg_kind").map(String::as_str),
        Some("offer")
    );
    assert_eq!(
        by.fields.get("ts_micros").map(String::as_str),
        Some("1700000000000000")
    );
}

#[test]
fn bypass_disabled_does_not_emit_event() {
    // Same transport, but bypass is left disabled. No bypass-event must
    // appear in the audit stream.
    let mock = MockHttpClient::new();
    let body = serde_json::to_vec(&serde_json::json!([{
        "id": 1, "from_peer": "alice", "to_peer": "bob",
        "kind": "hello", "payload": {"b64": ""},
        "delivered": false, "created_at": "",
    }]))
    .unwrap();
    mock.add_response(
        "https://test.supabase.co/rest/v1/signaling_messages",
        HttpResp {
            status: 201,
            headers: vec![],
            body,
        },
    );
    let sink: Arc<RecordingAuditSink> = Arc::new(RecordingAuditSink::new());
    let bypass: Arc<SovereignBypassRecorder> = Arc::new(SovereignBypassRecorder::new());
    // bypass NOT enabled.

    let t = RealSupabaseTransport::new(
        SupabaseConfig::new("https://test.supabase.co", "anon-key"),
        TRANSPORT_CAP_BOTH,
    )
    .with_http_client(Box::new(mock))
    .with_audit_sink(sink.clone())
    .with_bypass_recorder(bypass.clone());

    let msg = SignalingMessage {
        id: 0,
        from_peer: "alice".into(),
        to_peer: "bob".into(),
        kind: MessageKind::Hello,
        payload: vec![],
        ts_micros: 0,
    };
    t.send(&msg).expect("ok");

    assert_eq!(bypass.records(), 0);
    assert_eq!(sink.count_kind("mp.sovereign.bypass"), 0);
    assert_eq!(sink.count_kind("mp.send.ok"), 1);
}
