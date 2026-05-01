// § refund.rs — Stripe refund creation
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// § RefundReason — Stripe's documented set + Other for forward-compat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefundReason {
    Duplicate,
    Fraudulent,
    RequestedByCustomer,
    /// Free-form reason (e.g. "shard_revoked_by_owner"). Not transmitted to
    /// Stripe as `reason` (it expects the canonical strings) — instead it is
    /// stuffed into `metadata.local_reason` so we keep the audit trail.
    Other(String),
}

impl RefundReason {
    /// Stripe-accepted enum value, or `None` for `Other(_)`.
    #[must_use]
    pub fn stripe_reason(&self) -> Option<&'static str> {
        match self {
            RefundReason::Duplicate => Some("duplicate"),
            RefundReason::Fraudulent => Some("fraudulent"),
            RefundReason::RequestedByCustomer => Some("requested_by_customer"),
            RefundReason::Other(_) => None,
        }
    }

    /// Returns the local-reason string for `Other`, else `None`.
    #[must_use]
    pub fn local_reason(&self) -> Option<&str> {
        match self {
            RefundReason::Other(s) => Some(s),
            _ => None,
        }
    }
}

/// § RefundRequest — opaque charge-id only ; NO card data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefundRequest {
    /// Stripe charge or payment-intent id (e.g. `ch_…` or `pi_…`).
    pub charge_id: String,
    /// Optional partial-refund amount in smallest currency unit (cents for USD).
    /// `None` = full refund.
    pub amount: Option<u64>,
    pub reason: RefundReason,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

/// § RefundResponse — subset of Stripe's refund object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefundResponse {
    pub id: String,
    pub status: Option<String>,
    pub amount: Option<u64>,
    pub charge: Option<String>,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

#[must_use]
pub fn form_encode(req: &RefundRequest) -> String {
    use crate::checkout::url_encode;
    let mut parts = vec![format!("charge={}", url_encode(&req.charge_id))];
    if let Some(a) = req.amount {
        parts.push(format!("amount={a}"));
    }
    if let Some(r) = req.reason.stripe_reason() {
        parts.push(format!("reason={r}"));
    }
    let mut md = req.metadata.clone();
    if let Some(local) = req.reason.local_reason() {
        md.insert("local_reason".into(), local.to_string());
    }
    for (k, v) in &md {
        parts.push(format!("metadata[{}]={}", url_encode(k), url_encode(v)));
    }
    parts.join("&")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refund_reason_maps_to_stripe_strings() {
        assert_eq!(RefundReason::Duplicate.stripe_reason(), Some("duplicate"));
        assert_eq!(
            RefundReason::RequestedByCustomer.stripe_reason(),
            Some("requested_by_customer")
        );
        assert_eq!(RefundReason::Other("x".into()).stripe_reason(), None);
        assert_eq!(
            RefundReason::Other("shard_revoked".into()).local_reason(),
            Some("shard_revoked")
        );
    }

    #[test]
    fn refund_form_encode_includes_local_reason_in_metadata() {
        let r = RefundRequest {
            charge_id: "ch_abc".into(),
            amount: Some(500),
            reason: RefundReason::Other("shard_revoked_by_owner".into()),
            metadata: BTreeMap::new(),
        };
        let enc = form_encode(&r);
        assert!(enc.contains("charge=ch_abc"));
        assert!(enc.contains("amount=500"));
        assert!(enc.contains("metadata[local_reason]=shard_revoked_by_owner"));
        // Other(_) doesn't transmit `reason=…`
        assert!(!enc.contains("reason="));
    }

    #[test]
    fn refund_response_serde_roundtrip() {
        let j = r#"{"id":"re_1","status":"succeeded","amount":500,"charge":"ch_abc","metadata":{"k":"v"}}"#;
        let r: RefundResponse = serde_json::from_str(j).expect("parse");
        assert_eq!(r.id, "re_1");
        assert_eq!(r.amount, Some(500));
    }
}
