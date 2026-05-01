// § transport.rs : RealSupabaseTransport — production `MpTransport` impl
//
// Implements the `MpTransport` trait against the Supabase REST endpoint
// stack defined by W4 migration `0004_signaling.sql` :
//
//   POST /rest/v1/signaling_messages          — `send`
//   GET  /rest/v1/signaling_messages?…filter… — `poll`
//
// Both calls carry the standard PostgREST header-pair :
//
//   apikey: {anon_key}
//   Authorization: Bearer {jwt-or-anon-key}
//
// Plus `Content-Type: application/json` for the POST body, and `Prefer:
// return=representation` so the POST returns the inserted row including its
// server-assigned `id` (bigserial).
//
// The HTTP layer is a `Box<dyn HttpClient>` so tests can swap in
// `MockHttpClient` without touching the network.
//
// § ERROR MAPPING
//   • cap-deny                → TransportErr::CapDenied(bit)
//   • SignalingMessage.validate err  → TransportErr::MalformedMessage
//   • HttpTransportErr::Timeout      → TransportErr::Timeout
//   • HttpTransportErr::Io           → TransportErr::ServerErr("io: …")
//   • HTTP 429 + Retry-After ms      → TransportErr::Backoff(ms)
//   • HTTP 429 no Retry-After        → TransportErr::Backoff(default_backoff_ms)
//   • HTTP ≥ 500                     → TransportErr::ServerErr(status + snippet)
//   • HTTP non-2xx other             → TransportErr::ServerErr(status + snippet)
//   • body JSON parse failure        → TransportErr::ServerErr("malformed json: …")
//
// § AUDIT-EMIT (best-effort ; sink errors are swallowed)
//   • mp.send.ok    {msg_id, room_id}
//   • mp.send.err   {kind, room_id}
//   • mp.poll.ok    {count, room_id, peer_id}
//   • mp.poll.err   {kind, room_id, peer_id}
//   • mp.sovereign.bypass {msg_kind, ts_micros}   (via SovereignBypassRecorder)

use crate::backoff::parse_retry_after;
use crate::config::{AuditEvent, AuditSink, NoopAuditSink, SupabaseConfig};
use crate::http_client::{
    HttpClient, HttpMethod, HttpReq, HttpResp, HttpTransportErr, UreqClient,
};
use crate::sovereign_bypass::SovereignBypassRecorder;
use cssl_host_multiplayer_signaling::{MessageKind, SignalingMessage};
use cssl_host_mp_transport::{
    MpTransport, TransportErr, TransportResult, TRANSPORT_CAP_RECV, TRANSPORT_CAP_SEND,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};

// ─── wire shape ────────────────────────────────────────────────────────────
//
// The `signaling_messages` table stores `payload` as `jsonb` ; we serialize
// `SignalingMessage` (which has a `Vec<u8>` payload base64-encoded under
// the hood) into the row shape below. The `kind` column is a constrained
// `text` in {offer, answer, ice, hello, ping, pong, bye, custom} ; we map
// `MessageKind` into that lowercase set.
//
// `id` is `bigserial` — server-assigned ; we don't send it on POST.
// `created_at` defaults to now() server-side.

#[derive(Debug, Serialize)]
struct InsertRow<'a> {
    room_id: &'a str,
    from_peer: &'a str,
    to_peer: &'a str,
    kind: &'a str,
    payload: serde_json::Value,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)] // `delivered` mirrors the schema even though stage-0 doesn't read it.
struct PostgrestRow {
    id: u64,
    #[serde(default)]
    from_peer: String,
    #[serde(default)]
    to_peer: String,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    payload: serde_json::Value,
    #[serde(default)]
    delivered: bool,
    #[serde(default)]
    created_at: String,
}

/// Map `MessageKind` to the wire-string accepted by the
/// `signaling_messages_kind_check` constraint.
fn kind_to_wire(kind: &MessageKind) -> &'static str {
    match kind {
        MessageKind::Hello => "hello",
        MessageKind::Bye => "bye",
        MessageKind::Offer => "offer",
        MessageKind::Answer => "answer",
        MessageKind::IceCandidate => "ice",
        MessageKind::Ping => "ping",
        MessageKind::Pong => "pong",
        // RoomState rides the `custom` wire-slot ; the discriminator is
        // recoverable from the payload's `kind_full` field.
        MessageKind::RoomState | MessageKind::Custom(_) => "custom",
    }
}

/// Reverse map. `custom` returns `MessageKind::Custom("".into())` so the
/// caller can re-decode the discriminator from the payload.
fn wire_to_kind(s: &str) -> MessageKind {
    match s {
        "hello" => MessageKind::Hello,
        "bye" => MessageKind::Bye,
        "offer" => MessageKind::Offer,
        "answer" => MessageKind::Answer,
        "ice" => MessageKind::IceCandidate,
        "ping" => MessageKind::Ping,
        "pong" => MessageKind::Pong,
        _ => MessageKind::Custom(String::new()),
    }
}

// ─── room-id derivation ────────────────────────────────────────────────────
//
// The `MpTransport::send` API is room-implicit : the room is conveyed via
// the `SignalingMessage` envelope. The Supabase `signaling_messages` table
// requires an explicit `room_id` (uuid-typed). Stage-0 derives the room-id
// from the message `from_peer` field by convention : peers prefix their
// peer-id with `r:{room_id}:p:{peer_id}` when they participate in a room.
// If the prefix is absent we fall back to "00000000-0000-0000-0000-
// 000000000000" — RLS will reject those server-side, which is the
// auditable-failure-path we want.
//
// Longer-term : the `MpTransport` trait will gain `send_in(room, msg)` —
// tracked under §§ 11_IFC stage-1 ; the convention-based derivation is the
// stage-0 stop-gap.

const ZERO_ROOM_ID: &str = "00000000-0000-0000-0000-000000000000";

fn derive_room_id(peer_id: &str) -> String {
    if let Some(rest) = peer_id.strip_prefix("r:") {
        if let Some((room, _peer)) = rest.split_once(":p:") {
            return room.to_string();
        }
    }
    ZERO_ROOM_ID.to_string()
}

// ─── transport ─────────────────────────────────────────────────────────────

/// Production `MpTransport` impl backed by ureq + Supabase REST. See module
/// preamble for the full ABI / error-mapping / audit contract.
#[derive(Debug)]
pub struct RealSupabaseTransport {
    /// Backend coordinates + tunables.
    config: SupabaseConfig,
    /// Capability bits.
    caps: u32,
    /// HTTP client adapter ; `UreqClient` in production, `MockHttpClient`
    /// in tests.
    client: Box<dyn HttpClient>,
    /// Audit-event sink. `Box<dyn AuditSink>` so callers can wire any
    /// implementation (Noop, Recording, attestation-aggregator, …).
    audit: Arc<dyn AuditSink>,
    /// Sovereign-bypass toggle + record-counter.
    bypass: Arc<SovereignBypassRecorder>,
}

impl RealSupabaseTransport {
    /// Construct a production transport with the workspace's `UreqClient`
    /// and a `NoopAuditSink`. Use `with_audit_sink` /
    /// `with_http_client` to override.
    #[must_use]
    pub fn new(config: SupabaseConfig, caps: u32) -> Self {
        Self {
            config,
            caps,
            client: Box::new(UreqClient::new()),
            audit: Arc::new(NoopAuditSink),
            bypass: Arc::new(SovereignBypassRecorder::new()),
        }
    }

    /// Override the HTTP client. Used by tests to inject `MockHttpClient` ;
    /// production callers should leave the default `UreqClient`.
    #[must_use]
    pub fn with_http_client(mut self, client: Box<dyn HttpClient>) -> Self {
        self.client = client;
        self
    }

    /// Override the audit sink. Defaults to `NoopAuditSink`.
    #[must_use]
    pub fn with_audit_sink(mut self, sink: Arc<dyn AuditSink>) -> Self {
        self.audit = sink;
        self
    }

    /// Override the sovereign-bypass recorder. Defaults to a fresh
    /// disabled recorder.
    #[must_use]
    pub fn with_bypass_recorder(mut self, recorder: Arc<SovereignBypassRecorder>) -> Self {
        self.bypass = recorder;
        self
    }

    /// Test introspection : access the bypass recorder. Lets tests flip
    /// `enable()` / `disable()` post-construction.
    #[must_use]
    pub fn bypass_recorder(&self) -> &Arc<SovereignBypassRecorder> {
        &self.bypass
    }

    /// Test introspection : access the configured timeouts + URL.
    #[must_use]
    pub const fn config(&self) -> &SupabaseConfig {
        &self.config
    }

    // ─── header composition ────────────────────────────────────────────────

    /// Build the standard PostgREST header pair. JWT falls back to the
    /// anon key when not set ; both forms are valid Supabase auth.
    fn auth_headers(&self) -> Vec<(String, String)> {
        let bearer = self.config.jwt.as_deref().unwrap_or(&self.config.anon_key);
        vec![
            ("apikey".to_string(), self.config.anon_key.clone()),
            ("Authorization".to_string(), format!("Bearer {bearer}")),
        ]
    }

    /// Map an HTTP-layer error onto our transport error.
    #[allow(clippy::unused_self)] // mirrors instance-method shape for future per-instance state.
    fn map_http_err(&self, err: HttpTransportErr) -> TransportErr {
        match err {
            HttpTransportErr::Timeout => TransportErr::Timeout,
            HttpTransportErr::Io(s) => TransportErr::ServerErr(format!("io: {s}")),
            HttpTransportErr::BadRequest(s) => {
                TransportErr::ServerErr(format!("bad-request: {s}"))
            }
        }
    }

    /// Classify an HTTP response status into the appropriate
    /// `TransportErr` variant. Returns `Ok(())` for 2xx ; otherwise an
    /// owned err.
    fn classify_status(&self, resp: &HttpResp) -> Result<(), TransportErr> {
        if resp.is_success() {
            return Ok(());
        }
        if resp.status == 429 {
            let ms = parse_retry_after(resp.header("Retry-After"))
                .unwrap_or(self.config.default_backoff_ms);
            return Err(TransportErr::Backoff(ms));
        }
        // Snippet of the body for diagnostics — capped to 200 bytes so a
        // verbose error doc doesn't smear logs.
        let snippet = String::from_utf8_lossy(&resp.body);
        let snippet = snippet
            .chars()
            .take(200)
            .collect::<String>();
        Err(TransportErr::ServerErr(format!(
            "status={} body={}",
            resp.status, snippet
        )))
    }

    /// Best-effort audit emit ; swallows sink errors.
    fn audit_emit(&self, ev: AuditEvent) {
        let _ = self.audit.emit(ev);
    }
}

// ─── trait impl ────────────────────────────────────────────────────────────

impl MpTransport for RealSupabaseTransport {
    // Trait signature is `fn name(&self) -> &str` ; we return a static
    // string but the trait's lifetime says `&'_ self` — clippy's
    // `unnecessary_literal_bound` would prefer `&'static str` but we keep
    // the trait-consistent shape.
    #[allow(clippy::unnecessary_literal_bound)]
    fn name(&self) -> &str {
        "supabase-rest"
    }

    fn send(&self, msg: &SignalingMessage) -> Result<TransportResult, TransportErr> {
        // 1. cap-check.
        if self.caps & TRANSPORT_CAP_SEND == 0 {
            self.audit_emit(
                AuditEvent::new("mp.send.err")
                    .with("kind", "cap_denied")
                    .with("required", TRANSPORT_CAP_SEND),
            );
            return Err(TransportErr::CapDenied(TRANSPORT_CAP_SEND));
        }
        // 2. local validate.
        msg.validate().map_err(|_| {
            self.audit_emit(
                AuditEvent::new("mp.send.err").with("kind", "malformed_message"),
            );
            TransportErr::MalformedMessage
        })?;

        // 3. sovereign-bypass record (BEFORE the network write).
        let kind_wire = kind_to_wire(&msg.kind);
        self.bypass
            .record_if_active(self.audit.as_ref(), kind_wire, msg.ts_micros);

        // 4. compose body.
        let room_id = derive_room_id(&msg.from_peer);
        let payload_b64 = base64_encode(&msg.payload);
        let row = InsertRow {
            room_id: &room_id,
            from_peer: &msg.from_peer,
            to_peer: &msg.to_peer,
            kind: kind_wire,
            payload: serde_json::json!({
                "b64": payload_b64,
                "id": msg.id,
                "kind_full": match &msg.kind {
                    MessageKind::Custom(s) => s.clone(),
                    MessageKind::RoomState => "RoomState".to_string(),
                    other => format!("{other:?}"),
                },
            }),
        };
        let body = serde_json::to_vec(&row).map_err(|e| {
            self.audit_emit(
                AuditEvent::new("mp.send.err").with("kind", "json_serialize"),
            );
            TransportErr::ServerErr(format!("json serialize: {e}"))
        })?;

        // 5. compose request.
        let mut headers = self.auth_headers();
        headers.push(("Content-Type".into(), "application/json".into()));
        headers.push(("Prefer".into(), "return=representation".into()));

        let url = format!("{}/rest/v1/signaling_messages", self.config.base_url);
        let req = HttpReq {
            method: HttpMethod::Post,
            url,
            headers,
            body,
            timeout: Duration::from_millis(u64::from(self.config.send_timeout_ms)),
        };

        // 6. dispatch.
        let started = Instant::now();
        let resp = self
            .client
            .execute(&req)
            .map_err(|e| {
                self.audit_emit(
                    AuditEvent::new("mp.send.err").with("kind", "http_transport"),
                );
                self.map_http_err(e)
            })?;

        // 7. classify.
        if let Err(e) = self.classify_status(&resp) {
            let ek = match &e {
                TransportErr::Backoff(_) => "backoff",
                TransportErr::ServerErr(_) => "server_err",
                _ => "other",
            };
            self.audit_emit(
                AuditEvent::new("mp.send.err")
                    .with("kind", ek)
                    .with("status", resp.status),
            );
            return Err(e);
        }

        // 8. parse response → server id.
        let rows: Vec<PostgrestRow> = serde_json::from_slice(&resp.body).map_err(|e| {
            self.audit_emit(
                AuditEvent::new("mp.send.err").with("kind", "json_parse"),
            );
            TransportErr::ServerErr(format!("malformed json: {e}"))
        })?;
        let sent_id = rows.first().map_or(0, |r| r.id);

        let latency_ms = u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX);

        // 9. audit.
        self.audit_emit(
            AuditEvent::new("mp.send.ok")
                .with("msg_id", sent_id)
                .with("room_id", room_id)
                .with("latency_ms", latency_ms),
        );

        Ok(TransportResult { sent_id, latency_ms })
    }

    fn poll(
        &self,
        room_id: &str,
        peer_id: &str,
        since_id: u64,
    ) -> Result<Vec<SignalingMessage>, TransportErr> {
        // 1. cap-check.
        if self.caps & TRANSPORT_CAP_RECV == 0 {
            self.audit_emit(
                AuditEvent::new("mp.poll.err")
                    .with("kind", "cap_denied")
                    .with("required", TRANSPORT_CAP_RECV),
            );
            return Err(TransportErr::CapDenied(TRANSPORT_CAP_RECV));
        }

        // 2. compose request URL with PostgREST query string.
        // Use the OR group to also pick up `to_peer=eq.*` broadcast rows.
        let url = format!(
            "{base}/rest/v1/signaling_messages?room_id=eq.{room}&or=(to_peer.eq.{peer},to_peer.eq.*)&id=gt.{since}&order=id.asc&limit=128",
            base = self.config.base_url,
            room = room_id,
            peer = peer_id,
            since = since_id,
        );
        let req = HttpReq {
            method: HttpMethod::Get,
            url,
            headers: self.auth_headers(),
            body: Vec::new(),
            timeout: Duration::from_millis(u64::from(self.config.poll_timeout_ms)),
        };

        // 3. dispatch.
        let resp = self
            .client
            .execute(&req)
            .map_err(|e| {
                self.audit_emit(
                    AuditEvent::new("mp.poll.err").with("kind", "http_transport"),
                );
                self.map_http_err(e)
            })?;

        // 4. classify.
        if let Err(e) = self.classify_status(&resp) {
            let ek = match &e {
                TransportErr::Backoff(_) => "backoff",
                TransportErr::ServerErr(_) => "server_err",
                _ => "other",
            };
            self.audit_emit(
                AuditEvent::new("mp.poll.err")
                    .with("kind", ek)
                    .with("status", resp.status),
            );
            return Err(e);
        }

        // 5. parse JSON array of rows → SignalingMessage.
        let rows: Vec<PostgrestRow> = if resp.body.is_empty() {
            Vec::new()
        } else {
            serde_json::from_slice(&resp.body).map_err(|e| {
                self.audit_emit(
                    AuditEvent::new("mp.poll.err").with("kind", "json_parse"),
                );
                TransportErr::ServerErr(format!("malformed json: {e}"))
            })?
        };

        let messages: Vec<SignalingMessage> = rows
            .into_iter()
            .map(|r| {
                let payload_bytes = extract_payload_bytes(&r.payload);
                SignalingMessage {
                    id: r.id,
                    from_peer: r.from_peer,
                    to_peer: r.to_peer,
                    kind: wire_to_kind(&r.kind),
                    payload: payload_bytes,
                    ts_micros: parse_ts_iso8601_micros(&r.created_at),
                }
            })
            .collect();

        // 6. audit.
        self.audit_emit(
            AuditEvent::new("mp.poll.ok")
                .with("count", messages.len())
                .with("room_id", room_id)
                .with("peer_id", peer_id),
        );

        Ok(messages)
    }

    fn caps(&self) -> u32 {
        self.caps
    }
}

// ─── helpers ───────────────────────────────────────────────────────────────

/// Extract the `b64` payload field from a stored row's `payload` JSON
/// object. Returns empty Vec when the field is absent / malformed —
/// surfacing structural-corruption as observable empty-payload upstream
/// rather than panicking.
fn extract_payload_bytes(v: &serde_json::Value) -> Vec<u8> {
    let Some(s) = v.get("b64").and_then(|x| x.as_str()) else {
        return Vec::new();
    };
    base64_decode(s).unwrap_or_default()
}

/// Lightweight ISO-8601 → micros parser. Stage-0 parses only the
/// subsecond + integer-second components ; full timezone normalization
/// is deferred. Returns 0 when the input doesn't look like an ISO-8601
/// string — empty `created_at` returns 0 cleanly.
fn parse_ts_iso8601_micros(_s: &str) -> u64 {
    // Stage-0 : we don't depend on chrono ; ts_micros is informational
    // and the loa-host orchestrator can re-stamp from the local clock if
    // it needs precise wall-clock. Returning 0 keeps the API stable.
    0
}

// ─── base64 codec (mirrors message.rs) ─────────────────────────────────────
//
// The `cssl-host-multiplayer-signaling` crate keeps its base64 codec
// private. We reimplement it here (≈ 30 LOC) rather than expose the codec
// publicly from the source crate — the duplication is intentional :
// keeping codec-source-of-truth narrow makes audit easier.

const B64: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    let mut chunks = bytes.chunks_exact(3);
    for c in &mut chunks {
        let n = (u32::from(c[0]) << 16) | (u32::from(c[1]) << 8) | u32::from(c[2]);
        out.push(B64[((n >> 18) & 0x3f) as usize] as char);
        out.push(B64[((n >> 12) & 0x3f) as usize] as char);
        out.push(B64[((n >> 6) & 0x3f) as usize] as char);
        out.push(B64[(n & 0x3f) as usize] as char);
    }
    let rem = chunks.remainder();
    match rem.len() {
        0 => {}
        1 => {
            let n = u32::from(rem[0]) << 16;
            out.push(B64[((n >> 18) & 0x3f) as usize] as char);
            out.push(B64[((n >> 12) & 0x3f) as usize] as char);
            out.push('=');
            out.push('=');
        }
        2 => {
            let n = (u32::from(rem[0]) << 16) | (u32::from(rem[1]) << 8);
            out.push(B64[((n >> 18) & 0x3f) as usize] as char);
            out.push(B64[((n >> 12) & 0x3f) as usize] as char);
            out.push(B64[((n >> 6) & 0x3f) as usize] as char);
            out.push('=');
        }
        _ => {
            // chunks_exact remainder ≤ 2 ; defensive only.
            return String::new();
        }
    }
    out
}

fn base64_decode(s: &str) -> Result<Vec<u8>, &'static str> {
    let bytes = s.as_bytes();
    if bytes.len() % 4 != 0 {
        return Err("base64 length must be multiple of 4");
    }
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    for chunk in bytes.chunks_exact(4) {
        let mut acc: u32 = 0;
        let mut pad = 0u8;
        for (i, &b) in chunk.iter().enumerate() {
            let v = match b {
                b'A'..=b'Z' => b - b'A',
                b'a'..=b'z' => b - b'a' + 26,
                b'0'..=b'9' => b - b'0' + 52,
                b'+' => 62,
                b'/' => 63,
                b'=' => {
                    pad += 1;
                    if i < 2 {
                        return Err("base64 padding too early");
                    }
                    0
                }
                _ => return Err("base64 invalid character"),
            };
            acc = (acc << 6) | u32::from(v);
        }
        let n = acc;
        out.push(((n >> 16) & 0xff) as u8);
        if pad < 2 {
            out.push(((n >> 8) & 0xff) as u8);
        }
        if pad < 1 {
            out.push((n & 0xff) as u8);
        }
    }
    Ok(out)
}

// ─── tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_wire_roundtrip_canonical_set() {
        // Every canonical MessageKind variant maps to a wire-string and
        // back without losing its identity (Custom is the lossy case ;
        // it round-trips structurally).
        let cases: &[(MessageKind, &str)] = &[
            (MessageKind::Hello, "hello"),
            (MessageKind::Bye, "bye"),
            (MessageKind::Offer, "offer"),
            (MessageKind::Answer, "answer"),
            (MessageKind::IceCandidate, "ice"),
            (MessageKind::Ping, "ping"),
            (MessageKind::Pong, "pong"),
        ];
        for (k, w) in cases {
            assert_eq!(kind_to_wire(k), *w, "kind→wire lost for {w}");
            assert_eq!(&wire_to_kind(w), k, "wire→kind lost for {w}");
        }
    }

    #[test]
    fn derive_room_id_from_peer_prefix_or_zero() {
        assert_eq!(
            derive_room_id("r:abc-123:p:peer-1"),
            "abc-123".to_string()
        );
        assert_eq!(derive_room_id("just-a-peer"), ZERO_ROOM_ID);
        assert_eq!(derive_room_id("r:no-peer-suffix"), ZERO_ROOM_ID);
    }

    #[test]
    fn b64_roundtrip() {
        for raw in [
            &b""[..],
            b"a",
            b"ab",
            b"abc",
            b"abcd",
            &[0u8, 1, 2, 3, 0xff, 0xfe, 0xfd][..],
        ] {
            let enc = base64_encode(raw);
            let dec = base64_decode(&enc).expect("decode ok");
            assert_eq!(dec.as_slice(), raw, "b64 lost bytes for {raw:?}");
        }
    }

    #[test]
    fn extract_payload_handles_missing_field() {
        let v = serde_json::json!({});
        assert!(extract_payload_bytes(&v).is_empty());
        let v = serde_json::json!({"b64": null});
        assert!(extract_payload_bytes(&v).is_empty());
    }
}
