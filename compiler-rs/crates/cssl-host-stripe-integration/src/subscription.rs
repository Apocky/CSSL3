// § subscription.rs — minimal Stripe subscription CRUD
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// § SubscriptionStatus — Stripe's documented subset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubscriptionStatus {
    Active,
    PastDue,
    Canceled,
    Incomplete,
    IncompleteExpired,
    Trialing,
    Unpaid,
    /// Forward-compat catch-all.
    Other(String),
}

impl SubscriptionStatus {
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            SubscriptionStatus::Active => "active",
            SubscriptionStatus::PastDue => "past_due",
            SubscriptionStatus::Canceled => "canceled",
            SubscriptionStatus::Incomplete => "incomplete",
            SubscriptionStatus::IncompleteExpired => "incomplete_expired",
            SubscriptionStatus::Trialing => "trialing",
            SubscriptionStatus::Unpaid => "unpaid",
            SubscriptionStatus::Other(s) => s.as_str(),
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Self {
        match s {
            "active" => SubscriptionStatus::Active,
            "past_due" => SubscriptionStatus::PastDue,
            "canceled" => SubscriptionStatus::Canceled,
            "incomplete" => SubscriptionStatus::Incomplete,
            "incomplete_expired" => SubscriptionStatus::IncompleteExpired,
            "trialing" => SubscriptionStatus::Trialing,
            "unpaid" => SubscriptionStatus::Unpaid,
            other => SubscriptionStatus::Other(other.to_string()),
        }
    }
}

impl Serialize for SubscriptionStatus {
    fn serialize<S: serde::Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        ser.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for SubscriptionStatus {
    fn deserialize<D: serde::Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        let s = String::deserialize(de)?;
        Ok(SubscriptionStatus::parse(&s))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionRequest {
    pub customer_id: String,
    /// Stripe price-id (`price_…`).
    pub price_id: String,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
    #[serde(default)]
    pub trial_period_days: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subscription {
    pub id: String,
    pub customer: String,
    pub status: SubscriptionStatus,
    #[serde(default)]
    pub current_period_end: Option<i64>,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

#[must_use]
pub fn form_encode(req: &SubscriptionRequest) -> String {
    use crate::checkout::url_encode;
    let mut parts = vec![
        format!("customer={}", url_encode(&req.customer_id)),
        format!("items[0][price]={}", url_encode(&req.price_id)),
    ];
    if let Some(t) = req.trial_period_days {
        parts.push(format!("trial_period_days={t}"));
    }
    for (k, v) in &req.metadata {
        parts.push(format!("metadata[{}]={}", url_encode(k), url_encode(v)));
    }
    parts.join("&")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subscription_form_encode_basic() {
        let r = SubscriptionRequest {
            customer_id: "cus_abc".into(),
            price_id: "price_xyz".into(),
            metadata: BTreeMap::new(),
            trial_period_days: Some(7),
        };
        let enc = form_encode(&r);
        assert!(enc.contains("customer=cus_abc"));
        assert!(enc.contains("items[0][price]=price_xyz"));
        assert!(enc.contains("trial_period_days=7"));
    }

    #[test]
    fn subscription_serde_roundtrip() {
        let j = r#"{"id":"sub_1","customer":"cus_abc","status":"active","current_period_end":1700000000,"metadata":{}}"#;
        let s: Subscription = serde_json::from_str(j).expect("parse");
        assert_eq!(s.id, "sub_1");
        assert_eq!(s.status, SubscriptionStatus::Active);
    }
}
