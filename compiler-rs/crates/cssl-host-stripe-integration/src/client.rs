// § client.rs — StripeClient (cap-gated · audit-emit · idempotent)
//
// Every public method:
//   1. cap_check  → CapDenied + audit-emit on miss
//   2. idempotency-key required for mutating calls
//   3. compose HttpRequest with mandatory headers (Authorization · Stripe-Version
//      · Idempotency-Key · Content-Type)
//   4. dispatch via injected HttpTransport
//   5. audit-emit Success or Failure
//
// G1 hooks: replace `MockHttpTransport` injection with real `UreqHttpTransport`,
// replace `MockHmacVerifier` with `HmacSha256` impl, plumb `audit` to
// `cssl-host-attestation`.

use crate::cap::{
    StripeCap, STRIPE_CAP_CHECKOUT, STRIPE_CAP_REFUND, STRIPE_CAP_SUBSCRIPTION, STRIPE_CAP_WEBHOOK,
};
use crate::checkout::{form_encode as checkout_form_encode, CheckoutSession, CheckoutSessionRequest};
use crate::idempotency::{now_unix, IdempotencyEntry, IdempotencyKey, IdempotencyStore};
use crate::refund::{form_encode as refund_form_encode, RefundRequest, RefundResponse};
use crate::subscription::{form_encode as sub_form_encode, Subscription, SubscriptionRequest};
use crate::transport::{HttpMethod, HttpRequest, HttpTransport};
use crate::webhook::{verify_webhook_signature, HmacSha256Verifier, WebhookEvent};
use crate::{
    cap_check, AuditEvent, AuditOutcome, AuditSink, SecretString, StripeError, StripeResult,
};
use std::collections::BTreeMap;
use std::sync::Arc;

/// § StripeClientConfig — construction-time-only ; secret-key handed in once.
pub struct StripeClientConfig {
    pub secret_key: SecretString,
    /// Default `https://api.stripe.com`. Tests override to a mock URL.
    pub base_url: String,
    /// Stripe-Version header (e.g. `"2024-06-20"`). Pinned for deterministic
    /// API behavior across calls.
    pub stripe_version: String,
    /// Granted cap bits.
    pub granted_caps: StripeCap,
    pub transport: Arc<dyn HttpTransport>,
    pub hmac_verifier: Arc<dyn HmacSha256Verifier>,
    pub audit: Arc<dyn AuditSink>,
}

impl StripeClientConfig {
    /// Convenience builder for tests.
    #[must_use]
    pub fn new_with_caps(
        secret: &str,
        base_url: impl Into<String>,
        caps: StripeCap,
        transport: Arc<dyn HttpTransport>,
        hmac: Arc<dyn HmacSha256Verifier>,
        audit: Arc<dyn AuditSink>,
    ) -> Self {
        Self {
            secret_key: SecretString::new(secret.to_string()),
            base_url: base_url.into(),
            stripe_version: "2024-06-20".into(),
            granted_caps: caps,
            transport,
            hmac_verifier: hmac,
            audit,
        }
    }
}

/// § StripeClient — cap-gated facade for Stripe REST.
pub struct StripeClient {
    config: StripeClientConfig,
    idempotency: IdempotencyStore,
}

impl core::fmt::Debug for StripeClient {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("StripeClient")
            .field("base_url", &self.config.base_url)
            .field("stripe_version", &self.config.stripe_version)
            .field("granted_caps", &self.config.granted_caps)
            .field("idempotency_entries", &self.idempotency.len())
            // secret_key intentionally NOT included (SecretString redacts on Debug)
            .finish()
    }
}

impl StripeClient {
    #[must_use]
    pub fn new(config: StripeClientConfig) -> Self {
        Self {
            config,
            idempotency: IdempotencyStore::new(),
        }
    }

    /// Snapshot of cap bits this client was constructed with.
    #[must_use]
    pub fn granted_caps(&self) -> StripeCap {
        self.config.granted_caps
    }

    /// Number of idempotency-store entries (test/observability).
    #[must_use]
    pub fn idempotency_len(&self) -> usize {
        self.idempotency.len()
    }

    fn auth_headers(&self, idempotency_key: Option<&str>) -> BTreeMap<String, String> {
        let mut h = BTreeMap::new();
        h.insert(
            "Authorization".into(),
            format!("Bearer {}", self.config.secret_key.expose_secret()),
        );
        h.insert("Stripe-Version".into(), self.config.stripe_version.clone());
        h.insert(
            "Content-Type".into(),
            "application/x-www-form-urlencoded".into(),
        );
        if let Some(k) = idempotency_key {
            h.insert("Idempotency-Key".into(), k.to_string());
        }
        h
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.config.base_url)
    }

    fn audit_emit(
        &self,
        method: &'static str,
        outcome: AuditOutcome,
        idempotency_key: Option<String>,
        note: impl Into<String>,
    ) {
        self.config.audit.emit(AuditEvent {
            method,
            outcome,
            cap_required: 0,
            cap_granted: self.config.granted_caps.bits(),
            idempotency_key,
            note: note.into(),
        });
    }

    /// § create_checkout_session — Stripe Checkout flow entry-point.
    /// Cap : `STRIPE_CAP_CHECKOUT` · Idempotency-Key : required.
    pub fn create_checkout_session(
        &self,
        idempotency_key: IdempotencyKey,
        req: &CheckoutSessionRequest,
    ) -> StripeResult<CheckoutSession> {
        cap_check(
            "create_checkout_session",
            STRIPE_CAP_CHECKOUT,
            self.config.granted_caps.bits(),
            self.config.audit.as_ref(),
        )?;
        let body = checkout_form_encode(req);
        let payload_hash = IdempotencyStore::hash_payload(&body);
        // Replay short-circuit
        if let Some(existing) = self.idempotency.get(&idempotency_key, now_unix()) {
            if existing.payload_hash != payload_hash {
                self.audit_emit(
                    "create_checkout_session",
                    AuditOutcome::Failure,
                    Some(idempotency_key.as_str().into()),
                    "idempotency conflict (payload hash divergent)",
                );
                return Err(StripeError::IdempotencyConflict);
            }
            // matching reuse · return cached body
            let cs: CheckoutSession = serde_json::from_str(&existing.response_body)?;
            self.audit_emit(
                "create_checkout_session",
                AuditOutcome::Success,
                Some(idempotency_key.as_str().into()),
                "idempotent replay (cache hit)",
            );
            return Ok(cs);
        }
        let http_req = HttpRequest {
            method: HttpMethod::Post,
            url: self.url("/v1/checkout/sessions"),
            headers: self.auth_headers(Some(idempotency_key.as_str())),
            body,
        };
        let resp = match self.config.transport.send(http_req) {
            Ok(r) => r,
            Err(e) => {
                self.audit_emit(
                    "create_checkout_session",
                    AuditOutcome::Failure,
                    Some(idempotency_key.as_str().into()),
                    format!("transport error: {e}"),
                );
                return Err(e);
            }
        };
        if !resp.is_success() {
            let err = stripe_api_error(&resp);
            self.audit_emit(
                "create_checkout_session",
                AuditOutcome::Failure,
                Some(idempotency_key.as_str().into()),
                format!("api error: {err}"),
            );
            return Err(err);
        }
        let cs: CheckoutSession = serde_json::from_str(&resp.body)?;
        let _ = self.idempotency.insert(IdempotencyEntry {
            key: idempotency_key.clone(),
            payload_hash,
            stored_at_unix: now_unix(),
            method: "create_checkout_session",
            response_body: resp.body.clone(),
            response_status: resp.status,
        });
        self.audit_emit(
            "create_checkout_session",
            AuditOutcome::Success,
            Some(idempotency_key.as_str().into()),
            format!("checkout session created (id={})", cs.id),
        );
        Ok(cs)
    }

    /// § verify_webhook — signature-verify only (no I/O · pure).
    /// Cap : `STRIPE_CAP_WEBHOOK`.
    pub fn verify_webhook(
        &self,
        payload: &str,
        signature_header: &str,
        webhook_secret: &SecretString,
    ) -> StripeResult<WebhookEvent> {
        cap_check(
            "verify_webhook",
            STRIPE_CAP_WEBHOOK,
            self.config.granted_caps.bits(),
            self.config.audit.as_ref(),
        )?;
        match verify_webhook_signature(
            payload,
            signature_header,
            webhook_secret.expose_secret().as_bytes(),
            self.config.hmac_verifier.as_ref(),
        ) {
            Ok(evt) => {
                self.audit_emit(
                    "verify_webhook",
                    AuditOutcome::Success,
                    None,
                    format!("webhook verified (id={} type={:?})", evt.id, evt.event_type),
                );
                Ok(evt)
            }
            Err(e) => {
                self.audit_emit("verify_webhook", AuditOutcome::Failure, None, format!("{e}"));
                Err(e)
            }
        }
    }

    /// § create_refund — cap-gated, idempotency-key required.
    pub fn create_refund(
        &self,
        idempotency_key: IdempotencyKey,
        req: &RefundRequest,
    ) -> StripeResult<RefundResponse> {
        cap_check(
            "create_refund",
            STRIPE_CAP_REFUND,
            self.config.granted_caps.bits(),
            self.config.audit.as_ref(),
        )?;
        let body = refund_form_encode(req);
        let payload_hash = IdempotencyStore::hash_payload(&body);
        if let Some(existing) = self.idempotency.get(&idempotency_key, now_unix()) {
            if existing.payload_hash != payload_hash {
                self.audit_emit(
                    "create_refund",
                    AuditOutcome::Failure,
                    Some(idempotency_key.as_str().into()),
                    "idempotency conflict",
                );
                return Err(StripeError::IdempotencyConflict);
            }
            let r: RefundResponse = serde_json::from_str(&existing.response_body)?;
            self.audit_emit(
                "create_refund",
                AuditOutcome::Success,
                Some(idempotency_key.as_str().into()),
                "idempotent replay",
            );
            return Ok(r);
        }
        let http = HttpRequest {
            method: HttpMethod::Post,
            url: self.url("/v1/refunds"),
            headers: self.auth_headers(Some(idempotency_key.as_str())),
            body,
        };
        let resp = match self.config.transport.send(http) {
            Ok(r) => r,
            Err(e) => {
                self.audit_emit(
                    "create_refund",
                    AuditOutcome::Failure,
                    Some(idempotency_key.as_str().into()),
                    format!("transport error: {e}"),
                );
                return Err(e);
            }
        };
        if !resp.is_success() {
            let err = stripe_api_error(&resp);
            self.audit_emit(
                "create_refund",
                AuditOutcome::Failure,
                Some(idempotency_key.as_str().into()),
                format!("api error: {err}"),
            );
            return Err(err);
        }
        let r: RefundResponse = serde_json::from_str(&resp.body)?;
        let _ = self.idempotency.insert(IdempotencyEntry {
            key: idempotency_key.clone(),
            payload_hash,
            stored_at_unix: now_unix(),
            method: "create_refund",
            response_body: resp.body.clone(),
            response_status: resp.status,
        });
        self.audit_emit(
            "create_refund",
            AuditOutcome::Success,
            Some(idempotency_key.as_str().into()),
            format!("refund created (id={})", r.id),
        );
        Ok(r)
    }

    /// § create_subscription — cap-gated, idempotency-key required.
    pub fn create_subscription(
        &self,
        idempotency_key: IdempotencyKey,
        req: &SubscriptionRequest,
    ) -> StripeResult<Subscription> {
        cap_check(
            "create_subscription",
            STRIPE_CAP_SUBSCRIPTION,
            self.config.granted_caps.bits(),
            self.config.audit.as_ref(),
        )?;
        let body = sub_form_encode(req);
        let http = HttpRequest {
            method: HttpMethod::Post,
            url: self.url("/v1/subscriptions"),
            headers: self.auth_headers(Some(idempotency_key.as_str())),
            body,
        };
        let resp = match self.config.transport.send(http) {
            Ok(r) => r,
            Err(e) => {
                self.audit_emit(
                    "create_subscription",
                    AuditOutcome::Failure,
                    Some(idempotency_key.as_str().into()),
                    format!("transport error: {e}"),
                );
                return Err(e);
            }
        };
        if !resp.is_success() {
            let err = stripe_api_error(&resp);
            self.audit_emit(
                "create_subscription",
                AuditOutcome::Failure,
                Some(idempotency_key.as_str().into()),
                format!("api error: {err}"),
            );
            return Err(err);
        }
        let s: Subscription = serde_json::from_str(&resp.body)?;
        self.audit_emit(
            "create_subscription",
            AuditOutcome::Success,
            Some(idempotency_key.as_str().into()),
            format!("subscription created (id={})", s.id),
        );
        Ok(s)
    }

    /// § cancel_subscription — cap-gated.
    pub fn cancel_subscription(&self, subscription_id: &str) -> StripeResult<Subscription> {
        cap_check(
            "cancel_subscription",
            STRIPE_CAP_SUBSCRIPTION,
            self.config.granted_caps.bits(),
            self.config.audit.as_ref(),
        )?;
        let http = HttpRequest {
            method: HttpMethod::Delete,
            url: self.url(&format!("/v1/subscriptions/{subscription_id}")),
            headers: self.auth_headers(None),
            body: String::new(),
        };
        let resp = match self.config.transport.send(http) {
            Ok(r) => r,
            Err(e) => {
                self.audit_emit(
                    "cancel_subscription",
                    AuditOutcome::Failure,
                    None,
                    format!("transport error: {e}"),
                );
                return Err(e);
            }
        };
        if !resp.is_success() {
            let err = stripe_api_error(&resp);
            self.audit_emit(
                "cancel_subscription",
                AuditOutcome::Failure,
                None,
                format!("api error: {err}"),
            );
            return Err(err);
        }
        let s: Subscription = serde_json::from_str(&resp.body)?;
        self.audit_emit(
            "cancel_subscription",
            AuditOutcome::Success,
            None,
            format!("subscription cancelled (id={subscription_id})"),
        );
        Ok(s)
    }

    /// § list_subscriptions — cap-gated.
    pub fn list_subscriptions(&self, customer_id: &str, limit: u32) -> StripeResult<Vec<Subscription>> {
        cap_check(
            "list_subscriptions",
            STRIPE_CAP_SUBSCRIPTION,
            self.config.granted_caps.bits(),
            self.config.audit.as_ref(),
        )?;
        let http = HttpRequest {
            method: HttpMethod::Get,
            url: self.url(&format!(
                "/v1/subscriptions?customer={customer_id}&limit={limit}"
            )),
            headers: self.auth_headers(None),
            body: String::new(),
        };
        let resp = match self.config.transport.send(http) {
            Ok(r) => r,
            Err(e) => {
                self.audit_emit(
                    "list_subscriptions",
                    AuditOutcome::Failure,
                    None,
                    format!("transport error: {e}"),
                );
                return Err(e);
            }
        };
        if !resp.is_success() {
            let err = stripe_api_error(&resp);
            self.audit_emit(
                "list_subscriptions",
                AuditOutcome::Failure,
                None,
                format!("api error: {err}"),
            );
            return Err(err);
        }
        // Stripe wraps lists in `{"data":[...]}`.
        #[derive(serde::Deserialize)]
        struct ListBody {
            data: Vec<Subscription>,
        }
        let body: ListBody = serde_json::from_str(&resp.body)?;
        self.audit_emit(
            "list_subscriptions",
            AuditOutcome::Success,
            None,
            format!("listed {} subscriptions", body.data.len()),
        );
        Ok(body.data)
    }
}

/// § stripe_api_error — convert non-2xx HttpResponse into structured StripeError.
fn stripe_api_error(resp: &crate::transport::HttpResponse) -> StripeError {
    if resp.status == 401 {
        return StripeError::Auth {
            status: resp.status,
            message: resp.body.clone(),
        };
    }
    // Best-effort parse of Stripe's `{"error":{"code":"…","message":"…"}}`.
    #[derive(serde::Deserialize)]
    struct ErrEnv {
        error: Option<ErrInner>,
    }
    #[derive(serde::Deserialize)]
    struct ErrInner {
        code: Option<String>,
        message: Option<String>,
    }
    let parsed: Option<ErrEnv> = serde_json::from_str(&resp.body).ok();
    let (code, message) = parsed
        .and_then(|e| e.error)
        .map(|i| {
            (
                i.code.unwrap_or_else(|| "unknown".into()),
                i.message.unwrap_or_else(|| resp.body.clone()),
            )
        })
        .unwrap_or_else(|| ("unknown".into(), resp.body.clone()));
    StripeError::ApiError {
        status: resp.status,
        code,
        message,
    }
}
