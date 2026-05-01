// § message.rs : SignalingMessage envelope + MessageKind discriminator + validate
//
// All inter-peer signaling rides on `SignalingMessage` — `from_peer` → `to_peer`,
// a typed kind, and an opaque payload (≤ MAX_PAYLOAD_BYTES). `Vec<u8>` payloads
// are encoded as base64 strings on-the-wire so the envelope round-trips through
// JSON / Supabase row columns without binary-corruption surprises.

use serde::de::{self, Deserializer};
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};

/// Hard cap on the opaque payload size. Picked to comfortably hold an SDP
/// offer/answer (≤ ~8 KiB typical) plus headroom for ICE candidate batches
/// and small game-state deltas. Anything larger should be chunked at the
/// transport layer.
pub const MAX_PAYLOAD_BYTES: usize = 64 * 1024;

/// Typed envelope for client-to-client signaling. The payload is interpreted
/// per-`MessageKind` by downstream code (e.g. `Offer` payload is an SDP blob).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignalingMessage {
    /// Monotonic per-sender id ; lets receivers de-dupe and order replies.
    pub id: u64,
    /// Opaque sender peer-id (typically a UUID-string).
    pub from_peer: String,
    /// Opaque recipient peer-id ; `*` == broadcast within room.
    pub to_peer: String,
    /// Discriminator — receivers switch on this to decode `payload`.
    pub kind: MessageKind,
    /// Opaque payload bytes ; serialized as base64 over JSON.
    #[serde(serialize_with = "ser_b64", deserialize_with = "de_b64")]
    pub payload: Vec<u8>,
    /// Sender wall-clock at send time (microseconds since UNIX epoch).
    pub ts_micros: u64,
}

/// Discriminator for `SignalingMessage` payloads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "name", rename_all = "snake_case")]
pub enum MessageKind {
    /// Peer announces presence on join.
    Hello,
    /// Peer announces graceful departure.
    Bye,
    /// SDP offer (caller→callee).
    Offer,
    /// SDP answer (callee→caller).
    Answer,
    /// Trickle ICE candidate.
    IceCandidate,
    /// Liveness ping ; expects `Pong` reply.
    Ping,
    /// Liveness response.
    Pong,
    /// Room-state snapshot push (host→peer on join).
    RoomState,
    /// Application-defined extension ; the inner string scopes the variant
    /// so distinct app-channels don't collide.
    Custom(String),
}

/// Validation errors for `SignalingMessage::validate`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MsgErr {
    /// `from_peer` was the empty string.
    EmptyFrom,
    /// `to_peer` was the empty string.
    EmptyTo,
    /// `payload.len()` exceeded `MAX_PAYLOAD_BYTES`.
    PayloadTooLarge,
}

impl core::fmt::Display for MsgErr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::EmptyFrom => f.write_str("from_peer must be non-empty"),
            Self::EmptyTo => f.write_str("to_peer must be non-empty"),
            Self::PayloadTooLarge => {
                write!(f, "payload exceeds {MAX_PAYLOAD_BYTES} bytes")
            }
        }
    }
}

impl SignalingMessage {
    /// Validate envelope invariants. Must be called by the transport-layer
    /// before send and by the state-machine before processing inbound
    /// messages — the pure-state-machine layer (`process_inbound`) calls
    /// this internally.
    pub fn validate(&self) -> Result<(), MsgErr> {
        if self.from_peer.is_empty() {
            return Err(MsgErr::EmptyFrom);
        }
        if self.to_peer.is_empty() {
            return Err(MsgErr::EmptyTo);
        }
        if self.payload.len() > MAX_PAYLOAD_BYTES {
            return Err(MsgErr::PayloadTooLarge);
        }
        Ok(())
    }

    /// True if `to_peer` is the broadcast wildcard.
    pub fn is_broadcast(&self) -> bool {
        self.to_peer == "*"
    }
}

// ─── base64 codec for payload ──────────────────────────────────────────────
// Self-contained ; we don't pull a base64 crate just to round-trip Vec<u8>.

const B64: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn encode_b64(bytes: &[u8]) -> String {
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
        _ => unreachable!("chunks_exact remainder ≤ 2"),
    }
    out
}

fn decode_b64(s: &str) -> Result<Vec<u8>, &'static str> {
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

fn ser_b64<S: Serializer>(bytes: &[u8], s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&encode_b64(bytes))
}

fn de_b64<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
    let s = String::deserialize(d)?;
    decode_b64(&s).map_err(de::Error::custom)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk(from: &str, to: &str, payload: Vec<u8>) -> SignalingMessage {
        SignalingMessage {
            id: 1,
            from_peer: from.into(),
            to_peer: to.into(),
            kind: MessageKind::Hello,
            payload,
            ts_micros: 0,
        }
    }

    #[test]
    fn valid_message_passes() {
        let m = mk("alice", "bob", vec![1, 2, 3]);
        assert_eq!(m.validate(), Ok(()));
    }

    #[test]
    fn empty_from_rejected() {
        let m = mk("", "bob", vec![]);
        assert_eq!(m.validate(), Err(MsgErr::EmptyFrom));
    }

    #[test]
    fn empty_to_rejected() {
        let m = mk("alice", "", vec![]);
        assert_eq!(m.validate(), Err(MsgErr::EmptyTo));
    }

    #[test]
    fn payload_cap_enforced() {
        let big = vec![0u8; MAX_PAYLOAD_BYTES + 1];
        let m = mk("a", "b", big);
        assert_eq!(m.validate(), Err(MsgErr::PayloadTooLarge));

        // boundary : exactly at cap is fine
        let edge = mk("a", "b", vec![0u8; MAX_PAYLOAD_BYTES]);
        assert_eq!(edge.validate(), Ok(()));
    }

    #[test]
    fn roundtrip_base64_vec_u8() {
        // covers all three padding paths : 0 / 1 / 2 trailing pad-chars
        let cases: &[&[u8]] = &[
            b"",
            b"a",
            b"ab",
            b"abc",
            b"abcd",
            b"\x00\x01\x02\x03\xff\xfe\xfd",
            &[0u8; 257],
        ];
        for raw in cases {
            let m = mk("a", "b", raw.to_vec());
            let json = serde_json::to_string(&m).expect("serialize");
            let back: SignalingMessage = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(back.payload, *raw, "roundtrip lost bytes");
            assert_eq!(back, m);
        }
    }

    #[test]
    fn all_message_kind_variants_distinct() {
        // pin every variant — test fails to compile if a variant is renamed
        let kinds = [
            MessageKind::Hello,
            MessageKind::Bye,
            MessageKind::Offer,
            MessageKind::Answer,
            MessageKind::IceCandidate,
            MessageKind::Ping,
            MessageKind::Pong,
            MessageKind::RoomState,
            MessageKind::Custom("x".into()),
            MessageKind::Custom("y".into()),
        ];
        // Custom("x") != Custom("y") proves discriminant is content-aware
        for (i, a) in kinds.iter().enumerate() {
            for (j, b) in kinds.iter().enumerate() {
                if i == j {
                    assert_eq!(a, b);
                } else {
                    assert_ne!(a, b, "{i} vs {j}");
                }
            }
        }
    }
}
