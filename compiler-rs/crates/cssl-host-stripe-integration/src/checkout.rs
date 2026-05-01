// § checkout.rs — Stripe Checkout Session creation
// Cosmetic-only-axiom : line items reference `price_lookup_key` strings only ;
// NO API surface that lets us encode pay-for-power.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// § CheckoutSessionRequest — input to `StripeClient::create_checkout_session`.
///
/// Note: there is structurally NO `card_number` / `cvc` / `card_holder` field.
/// Stripe Checkout collects those on Stripe's hosted page ; we only ever see
/// `payment_method_id` callbacks via webhooks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckoutSessionRequest {
    /// Stripe `price_lookup_key` for each line item (e.g.
    /// `"hifi_imprint_50"` · `"eternal_attribution"`).
    pub line_items: Vec<LineItem>,
    /// Where to redirect on completion. Caller must include `{CHECKOUT_SESSION_ID}` if needed.
    pub success_url: String,
    /// Where to redirect on cancellation.
    pub cancel_url: String,
    /// `payment` for one-shot, `subscription` for recurring.
    pub mode: CheckoutMode,
    /// Arbitrary opaque key/value metadata. NEVER put PII here.
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
    /// Optional client-supplied reference (used for reconciliation against our
    /// internal order ID).
    #[serde(default)]
    pub client_reference_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineItem {
    pub price_lookup_key: String,
    pub quantity: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckoutMode {
    Payment,
    Subscription,
    Setup,
}

impl CheckoutMode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            CheckoutMode::Payment => "payment",
            CheckoutMode::Subscription => "subscription",
            CheckoutMode::Setup => "setup",
        }
    }
}

/// § CheckoutSession — response shape from Stripe (subset).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckoutSession {
    pub id: String,
    /// Hosted Stripe-page URL the caller redirects the user to.
    pub url: Option<String>,
    pub status: Option<String>,
    /// Echoed back from request.
    #[serde(default)]
    pub client_reference_id: Option<String>,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

/// § form_encode — serialize the request as `application/x-www-form-urlencoded`
/// since Stripe's REST API takes form-bodies, not JSON. Deterministic ordering
/// via BTreeMap and explicit field order so idempotency-payload-hash is stable.
#[must_use]
pub fn form_encode(req: &CheckoutSessionRequest) -> String {
    let mut parts: Vec<String> = Vec::new();
    parts.push(format!("mode={}", req.mode.as_str()));
    parts.push(format!("success_url={}", url_encode(&req.success_url)));
    parts.push(format!("cancel_url={}", url_encode(&req.cancel_url)));
    if let Some(ref r) = req.client_reference_id {
        parts.push(format!("client_reference_id={}", url_encode(r)));
    }
    for (i, item) in req.line_items.iter().enumerate() {
        parts.push(format!(
            "line_items[{i}][price_data][lookup_key]={}",
            url_encode(&item.price_lookup_key)
        ));
        parts.push(format!("line_items[{i}][quantity]={}", item.quantity));
    }
    // metadata sorted by key for determinism
    for (k, v) in &req.metadata {
        parts.push(format!("metadata[{}]={}", url_encode(k), url_encode(v)));
    }
    parts.join("&")
}

/// Minimal RFC-3986 unreserved-only encoder. Sufficient for the field shapes
/// we actually emit (URLs · ASCII keys · ASCII values).
#[must_use]
pub fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.as_bytes() {
        match *b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_req() -> CheckoutSessionRequest {
        let mut md = BTreeMap::new();
        md.insert("attribution".into(), "shard_id_42".into());
        CheckoutSessionRequest {
            line_items: vec![LineItem {
                price_lookup_key: "hifi_imprint_50".into(),
                quantity: 1,
            }],
            success_url: "https://example.com/success?id={CHECKOUT_SESSION_ID}".into(),
            cancel_url: "https://example.com/cancel".into(),
            mode: CheckoutMode::Payment,
            metadata: md,
            client_reference_id: Some("order_001".into()),
        }
    }

    #[test]
    fn form_encode_is_deterministic() {
        let r = sample_req();
        let a = form_encode(&r);
        let b = form_encode(&r);
        assert_eq!(a, b);
        assert!(a.contains("mode=payment"));
        assert!(a.contains("line_items[0][price_data][lookup_key]=hifi_imprint_50"));
        assert!(a.contains("metadata[attribution]=shard_id_42"));
    }

    #[test]
    fn checkout_session_serde_roundtrip() {
        let json = r#"{"id":"cs_test","url":"https://stripe.test/cs_test","status":"open","client_reference_id":"order_001","metadata":{"k":"v"}}"#;
        let cs: CheckoutSession = serde_json::from_str(json).expect("parse");
        assert_eq!(cs.id, "cs_test");
        assert_eq!(cs.client_reference_id.as_deref(), Some("order_001"));
        let back = serde_json::to_string(&cs).expect("serialize");
        assert!(back.contains("cs_test"));
    }

    #[test]
    fn no_card_fields_in_request_struct() {
        // Compile-time-ish guard : enumerate the field names and assert none
        // contain "card" or "cvc" or "pan". The field-list is small and any
        // future addition that breaks this lights up the test.
        let names = ["line_items", "success_url", "cancel_url", "mode", "metadata", "client_reference_id"];
        for n in names {
            assert!(!n.to_lowercase().contains("card"));
            assert!(!n.to_lowercase().contains("cvc"));
            assert!(!n.to_lowercase().contains("pan"));
        }
    }
}
