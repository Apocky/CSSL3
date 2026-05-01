// § webhook.rs — Stripe webhook signature verification
// Stage-0 : hmac/sha2/subtle ABSENT from workspace.dependencies.
// We define `HmacSha256Verifier` trait + ship a BLAKE3-keyed-MAC fallback so
// the audit/test path is exercised in CI. G1 wires real HMAC-SHA256.
//
// Constant-time-compare implemented manually with a volatile-byte-loop ;
// G1 may upgrade to `subtle::ConstantTimeEq` when crate is added.

use crate::{StripeError, StripeResult};
use serde::{Deserialize, Serialize};

/// § HmacSha256Verifier — pluggable MAC layer.
///
/// `compute(key, payload) -> 32-byte tag` is what Stripe defines as
/// `HMAC-SHA256(secret, "{timestamp}.{payload}")`. Stage-0 fallback uses
/// `blake3::keyed_hash` which gives an equally-strong 32-byte MAC and lets
/// us exercise the cap-gate + audit-emit + constant-time-compare paths
/// without depending on `hmac` + `sha2`.
pub trait HmacSha256Verifier: Send + Sync {
    fn compute(&self, key: &[u8], payload: &[u8]) -> [u8; 32];

    /// Identifier for audit-trail. `"hmac-sha256"` for real impl,
    /// `"blake3-keyed"` for stage-0 fallback. Wire-format compatibility
    /// note: tags computed here must MATCH whatever the sender produced.
    fn algorithm(&self) -> &'static str;
}

/// § MockHmacVerifier — stage-0 BLAKE3-keyed-MAC fallback.
///
/// G1 : add `Hmac<Sha256>` impl here OR replace this struct entirely.
#[derive(Debug, Default, Clone, Copy)]
pub struct MockHmacVerifier;

impl HmacSha256Verifier for MockHmacVerifier {
    fn compute(&self, key: &[u8], payload: &[u8]) -> [u8; 32] {
        // BLAKE3 keyed hash needs a 32-byte key. We deterministically
        // derive a 32-byte key from arbitrary-length inputs by hashing first.
        let derived = blake3::hash(key);
        let keyed = blake3::keyed_hash(derived.as_bytes(), payload);
        *keyed.as_bytes()
    }

    fn algorithm(&self) -> &'static str {
        "blake3-keyed"
    }
}

/// § constant_time_eq — manual volatile byte-by-byte XOR-accumulate.
///
/// G1 : replace with `subtle::ConstantTimeEq::ct_eq` once crate is added.
/// This must NOT short-circuit on first mismatch (timing-leak).
#[must_use]
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for i in 0..a.len() {
        // black_box prevents the optimizer from short-circuiting.
        let x = std::hint::black_box(a[i]);
        let y = std::hint::black_box(b[i]);
        diff |= x ^ y;
    }
    diff == 0
}

/// § WebhookEventType — minimal enum surface ; G1 expands.
///
/// Custom (de)serializer : Stripe's `type` field is a flat string. We map
/// known values to enum variants and stash the rest in `Other(String)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WebhookEventType {
    CheckoutSessionCompleted,
    ChargeRefunded,
    CustomerSubscriptionCreated,
    CustomerSubscriptionDeleted,
    CustomerSubscriptionUpdated,
    /// Catch-all for events we don't model yet. Carries the raw `type` string.
    Other(String),
}

impl WebhookEventType {
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            WebhookEventType::CheckoutSessionCompleted => "checkout.session.completed",
            WebhookEventType::ChargeRefunded => "charge.refunded",
            WebhookEventType::CustomerSubscriptionCreated => "customer.subscription.created",
            WebhookEventType::CustomerSubscriptionDeleted => "customer.subscription.deleted",
            WebhookEventType::CustomerSubscriptionUpdated => "customer.subscription.updated",
            WebhookEventType::Other(s) => s.as_str(),
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Self {
        match s {
            "checkout.session.completed" => WebhookEventType::CheckoutSessionCompleted,
            "charge.refunded" => WebhookEventType::ChargeRefunded,
            "customer.subscription.created" => WebhookEventType::CustomerSubscriptionCreated,
            "customer.subscription.deleted" => WebhookEventType::CustomerSubscriptionDeleted,
            "customer.subscription.updated" => WebhookEventType::CustomerSubscriptionUpdated,
            other => WebhookEventType::Other(other.to_string()),
        }
    }
}

impl Serialize for WebhookEventType {
    fn serialize<S: serde::Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        ser.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for WebhookEventType {
    fn deserialize<D: serde::Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        let s = String::deserialize(de)?;
        Ok(WebhookEventType::parse(&s))
    }
}

/// § WebhookEvent — parsed Stripe webhook payload (post-signature-verify).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookEvent {
    pub id: String,
    #[serde(rename = "type")]
    pub event_type: WebhookEventType,
    /// Raw `data.object` JSON value (we don't decode all sub-shapes here).
    #[serde(default)]
    pub data: serde_json::Value,
    pub created: i64,
    pub livemode: bool,
}

/// § parse_signature_header — Stripe uses `t=<unix>,v1=<hex>` format.
///
/// Returns `(timestamp, sig_bytes)` or `WebhookSigInvalid`.
pub fn parse_signature_header(header: &str) -> StripeResult<(i64, Vec<u8>)> {
    // Split on commas, find `t=` and `v1=` parts.
    let mut t: Option<i64> = None;
    let mut v1: Option<Vec<u8>> = None;
    for part in header.split(',') {
        let (k, v) = part.split_once('=').ok_or_else(|| StripeError::WebhookSigInvalid {
            reason: format!("malformed part: {part}"),
        })?;
        match k.trim() {
            "t" => t = v.trim().parse::<i64>().ok(),
            "v1" => v1 = hex_decode(v.trim()),
            _ => {} // ignore v0 / scheme · forward-compat
        }
    }
    match (t, v1) {
        (Some(t), Some(v1)) => Ok((t, v1)),
        _ => Err(StripeError::WebhookSigInvalid {
            reason: "missing t= or v1= in Stripe-Signature header".into(),
        }),
    }
}

#[must_use]
fn hex_decode(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(s.len() / 2);
    let bytes = s.as_bytes();
    for i in (0..bytes.len()).step_by(2) {
        let hi = hex_nibble(bytes[i])?;
        let lo = hex_nibble(bytes[i + 1])?;
        out.push((hi << 4) | lo);
    }
    Some(out)
}

#[must_use]
fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

#[must_use]
pub fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// § verify_webhook_signature — pure function, no I/O.
///
/// Constructs `signed_payload = "{timestamp}.{payload}"`, computes the MAC
/// using the injected verifier, then constant-time-compares against the
/// header-supplied tag. Returns the parsed `WebhookEvent` on success.
pub fn verify_webhook_signature(
    payload: &str,
    signature_header: &str,
    secret: &[u8],
    verifier: &dyn HmacSha256Verifier,
) -> StripeResult<WebhookEvent> {
    let (ts, expected_sig) = parse_signature_header(signature_header)?;
    let signed = format!("{ts}.{payload}");
    let computed = verifier.compute(secret, signed.as_bytes());
    if !constant_time_eq(&computed, &expected_sig) {
        return Err(StripeError::WebhookSigInvalid {
            reason: format!("MAC mismatch (algorithm={})", verifier.algorithm()),
        });
    }
    let event: WebhookEvent = serde_json::from_str(payload)?;
    Ok(event)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ct_eq_handles_unequal_lengths() {
        assert!(!constant_time_eq(b"abc", b"abcd"));
    }

    #[test]
    fn ct_eq_matches_equal() {
        assert!(constant_time_eq(b"abcdef", b"abcdef"));
        assert!(!constant_time_eq(b"abcdef", b"abcdez"));
    }

    #[test]
    fn parse_sig_header_ok() {
        let (t, sig) = parse_signature_header("t=1700000000,v1=deadbeef").expect("ok");
        assert_eq!(t, 1_700_000_000);
        assert_eq!(sig, vec![0xde, 0xad, 0xbe, 0xef]);
    }

    #[test]
    fn parse_sig_header_missing_v1() {
        let r = parse_signature_header("t=1700000000");
        assert!(matches!(r, Err(StripeError::WebhookSigInvalid { .. })));
    }

    #[test]
    fn webhook_verify_good_signature() {
        let v = MockHmacVerifier;
        let secret = b"whsec_test";
        let payload =
            r#"{"id":"evt_1","type":"checkout.session.completed","data":{},"created":1700000000,"livemode":false}"#;
        let ts = 1_700_000_000_i64;
        let signed = format!("{ts}.{payload}");
        let mac = v.compute(secret, signed.as_bytes());
        let header = format!("t={ts},v1={}", hex_encode(&mac));
        let evt = verify_webhook_signature(payload, &header, secret, &v).expect("verify");
        assert_eq!(evt.id, "evt_1");
        assert_eq!(evt.event_type, WebhookEventType::CheckoutSessionCompleted);
    }

    #[test]
    fn webhook_verify_bad_signature() {
        let v = MockHmacVerifier;
        let payload = r#"{"id":"evt_1","type":"charge.refunded","data":{},"created":1,"livemode":false}"#;
        // Tag computed with WRONG secret — constant-time-compare must reject.
        let ts = 1_i64;
        let signed = format!("{ts}.{payload}");
        let mac = v.compute(b"wrong_secret", signed.as_bytes());
        let header = format!("t={ts},v1={}", hex_encode(&mac));
        let r = verify_webhook_signature(payload, &header, b"whsec_test", &v);
        assert!(matches!(r, Err(StripeError::WebhookSigInvalid { .. })));
    }

    #[test]
    fn webhook_verify_unknown_event_type_passes_through() {
        let v = MockHmacVerifier;
        let secret = b"whsec_test";
        let payload =
            r#"{"id":"evt_2","type":"invoice.paid","data":{},"created":1700000000,"livemode":true}"#;
        let ts = 1_700_000_000_i64;
        let signed = format!("{ts}.{payload}");
        let mac = v.compute(secret, signed.as_bytes());
        let header = format!("t={ts},v1={}", hex_encode(&mac));
        let evt = verify_webhook_signature(payload, &header, secret, &v).expect("verify");
        match evt.event_type {
            WebhookEventType::Other(s) => assert_eq!(s, "invoice.paid"),
            other => panic!("expected Other, got {other:?}"),
        }
    }

    #[test]
    fn algorithm_label_visible_for_audit() {
        let v = MockHmacVerifier;
        assert_eq!(v.algorithm(), "blake3-keyed");
    }
}
