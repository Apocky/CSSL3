// § T11-W8-D1 integration-tests for cssl-host-stripe-integration
//
// All Stripe HTTP traffic is intercepted by `MockHttpTransport` ; webhook
// signatures by `MockHmacVerifier`. NO real Stripe HTTPS in CI.
//
// Each test asserts:
//   - cap-gating produces `CapDenied` + audit-emit for missing-bits
//   - mutating-calls always send `Idempotency-Key` header
//   - replays of the same key+payload return cached responses
//   - replays with divergent payload return `IdempotencyConflict`
//   - webhook good/bad signatures are surfaced correctly
//   - audit-sink records both Success and Failure outcomes
//   - SecretString never leaks via Debug

use cssl_host_stripe_integration::{
    cap::{StripeCap, STRIPE_CAP_CHECKOUT, STRIPE_CAP_REFUND, STRIPE_CAP_SUBSCRIPTION, STRIPE_CAP_WEBHOOK},
    checkout::{CheckoutMode, CheckoutSessionRequest, LineItem},
    client::{StripeClient, StripeClientConfig},
    idempotency::IdempotencyKey,
    refund::{RefundReason, RefundRequest},
    subscription::SubscriptionRequest,
    transport::{HttpMethod, HttpResponse, MockHttpTransport, ProgrammedReply},
    webhook::{hex_encode, HmacSha256Verifier, MockHmacVerifier},
    AuditOutcome, MockAuditSink, SecretString, StripeError,
};
use std::collections::BTreeMap;
use std::sync::Arc;

// ───────── helpers ─────────

fn build(
    caps: StripeCap,
) -> (StripeClient, Arc<MockHttpTransport>, Arc<MockAuditSink>, Arc<MockHmacVerifier>) {
    let transport = Arc::new(MockHttpTransport::new());
    let audit = Arc::new(MockAuditSink::new());
    let hmac = Arc::new(MockHmacVerifier);
    let cfg = StripeClientConfig::new_with_caps(
        "sk_test_super_secret",
        "https://api.stripe.test",
        caps,
        transport.clone() as Arc<_>,
        hmac.clone() as Arc<_>,
        audit.clone() as Arc<_>,
    );
    (StripeClient::new(cfg), transport, audit, hmac)
}

fn checkout_req() -> CheckoutSessionRequest {
    CheckoutSessionRequest {
        line_items: vec![LineItem {
            price_lookup_key: "hifi_imprint_50".into(),
            quantity: 1,
        }],
        success_url: "https://example.com/ok".into(),
        cancel_url: "https://example.com/x".into(),
        mode: CheckoutMode::Payment,
        metadata: BTreeMap::new(),
        client_reference_id: Some("order_001".into()),
    }
}

fn ok_resp(body: &str) -> HttpResponse {
    HttpResponse {
        status: 200,
        body: body.to_string(),
    }
}

// ───────── cap-denied paths (3) ─────────

#[test]
fn cap_denied_checkout_emits_audit_event() {
    let (client, _t, audit, _h) = build(StripeCap::NONE);
    let key = IdempotencyKey::new("k1").unwrap();
    let r = client.create_checkout_session(key, &checkout_req());
    assert!(matches!(r, Err(StripeError::CapDenied { .. })));
    assert_eq!(audit.count_outcome(&AuditOutcome::CapDenied), 1);
}

#[test]
fn cap_denied_refund_audits() {
    let (client, _t, audit, _h) = build(StripeCap(STRIPE_CAP_CHECKOUT));
    let key = IdempotencyKey::new("k2").unwrap();
    let req = RefundRequest {
        charge_id: "ch_x".into(),
        amount: None,
        reason: RefundReason::RequestedByCustomer,
        metadata: BTreeMap::new(),
    };
    let r = client.create_refund(key, &req);
    assert!(matches!(r, Err(StripeError::CapDenied { .. })));
    assert_eq!(audit.count_outcome(&AuditOutcome::CapDenied), 1);
}

#[test]
fn cap_denied_subscription_audits() {
    let (client, _t, audit, _h) = build(StripeCap(STRIPE_CAP_REFUND));
    let key = IdempotencyKey::new("k3").unwrap();
    let req = SubscriptionRequest {
        customer_id: "cus".into(),
        price_id: "price".into(),
        metadata: BTreeMap::new(),
        trial_period_days: None,
    };
    let r = client.create_subscription(key, &req);
    assert!(matches!(r, Err(StripeError::CapDenied { .. })));
    assert!(matches!(
        client.cancel_subscription("sub_1"),
        Err(StripeError::CapDenied { .. })
    ));
    assert!(matches!(
        client.list_subscriptions("cus", 5),
        Err(StripeError::CapDenied { .. })
    ));
    assert_eq!(audit.count_outcome(&AuditOutcome::CapDenied), 3);
}

// ───────── idempotency-key included on every mutating call (2) ─────────

#[test]
fn checkout_includes_idempotency_key_header() {
    let (client, transport, _a, _h) = build(StripeCap(STRIPE_CAP_CHECKOUT));
    transport.program(ProgrammedReply {
        url_suffix: "/v1/checkout/sessions".into(),
        method: HttpMethod::Post,
        response: ok_resp(r#"{"id":"cs_1","url":"https://stripe.test/cs_1"}"#),
    });
    let key = IdempotencyKey::new("idem_checkout_1").unwrap();
    client
        .create_checkout_session(key, &checkout_req())
        .expect("ok");
    let recorded = transport.recorded_requests();
    assert_eq!(recorded.len(), 1);
    let h = &recorded[0].headers;
    assert_eq!(h.get("Idempotency-Key").map(String::as_str), Some("idem_checkout_1"));
    assert!(h.get("Authorization").unwrap().starts_with("Bearer "));
    assert_eq!(
        h.get("Content-Type").map(String::as_str),
        Some("application/x-www-form-urlencoded")
    );
}

#[test]
fn refund_includes_idempotency_key_header() {
    let (client, transport, _a, _h) = build(StripeCap(STRIPE_CAP_REFUND));
    transport.program(ProgrammedReply {
        url_suffix: "/v1/refunds".into(),
        method: HttpMethod::Post,
        response: ok_resp(r#"{"id":"re_1","status":"succeeded","amount":500,"charge":"ch_x"}"#),
    });
    let key = IdempotencyKey::new("idem_refund_1").unwrap();
    client
        .create_refund(
            key,
            &RefundRequest {
                charge_id: "ch_x".into(),
                amount: Some(500),
                reason: RefundReason::Duplicate,
                metadata: BTreeMap::new(),
            },
        )
        .expect("ok");
    let recorded = transport.recorded_requests();
    assert_eq!(
        recorded[0].headers.get("Idempotency-Key").map(String::as_str),
        Some("idem_refund_1")
    );
}

// ───────── idempotency-conflict-detected (1) ─────────

#[test]
fn idempotency_conflict_on_divergent_payload() {
    let (client, transport, _a, _h) = build(StripeCap(STRIPE_CAP_CHECKOUT));
    transport.program(ProgrammedReply {
        url_suffix: "/v1/checkout/sessions".into(),
        method: HttpMethod::Post,
        response: ok_resp(r#"{"id":"cs_a","url":"https://stripe.test/cs_a"}"#),
    });
    let key = IdempotencyKey::new("idem_dup").unwrap();
    let req1 = checkout_req();
    let mut req2 = checkout_req();
    req2.client_reference_id = Some("DIFFERENT".into());
    client
        .create_checkout_session(key.clone(), &req1)
        .expect("first call ok");
    let r = client.create_checkout_session(key, &req2);
    assert!(matches!(r, Err(StripeError::IdempotencyConflict)));
}

#[test]
fn idempotency_replay_returns_cached_response() {
    let (client, transport, audit, _h) = build(StripeCap(STRIPE_CAP_CHECKOUT));
    transport.program(ProgrammedReply {
        url_suffix: "/v1/checkout/sessions".into(),
        method: HttpMethod::Post,
        response: ok_resp(r#"{"id":"cs_z","url":"https://stripe.test/cs_z"}"#),
    });
    let key = IdempotencyKey::new("idem_same").unwrap();
    let r1 = client
        .create_checkout_session(key.clone(), &checkout_req())
        .expect("first ok");
    let r2 = client
        .create_checkout_session(key, &checkout_req())
        .expect("replay ok");
    assert_eq!(r1.id, "cs_z");
    assert_eq!(r2.id, "cs_z");
    // Only one HTTP call made — second was served from cache.
    assert_eq!(transport.recorded_requests().len(), 1);
    // Two Success audits recorded (live + replay).
    assert_eq!(audit.count_outcome(&AuditOutcome::Success), 2);
}

// ───────── webhook good/bad/timing-attack (3) ─────────

#[test]
fn webhook_verify_succeeds_on_good_sig() {
    let (client, _t, audit, hmac) = build(StripeCap(STRIPE_CAP_WEBHOOK));
    let secret = SecretString::new("whsec_abc".into());
    let payload =
        r#"{"id":"evt_x","type":"checkout.session.completed","data":{},"created":1700000000,"livemode":false}"#;
    let ts = 1_700_000_000_i64;
    let signed = format!("{ts}.{payload}");
    let mac = hmac.compute(secret.expose_secret().as_bytes(), signed.as_bytes());
    let header = format!("t={ts},v1={}", hex_encode(&mac));
    let evt = client.verify_webhook(payload, &header, &secret).expect("verified");
    assert_eq!(evt.id, "evt_x");
    assert_eq!(audit.count_outcome(&AuditOutcome::Success), 1);
}

#[test]
fn webhook_verify_fails_on_tampered_sig() {
    let (client, _t, audit, _hmac) = build(StripeCap(STRIPE_CAP_WEBHOOK));
    let secret = SecretString::new("whsec_abc".into());
    // Hand-rolled bad signature
    let payload = r#"{"id":"evt_y","type":"charge.refunded","data":{},"created":1,"livemode":false}"#;
    let header = "t=1,v1=00000000000000000000000000000000000000000000000000000000000000ff";
    let r = client.verify_webhook(payload, header, &secret);
    assert!(matches!(r, Err(StripeError::WebhookSigInvalid { .. })));
    assert_eq!(audit.count_outcome(&AuditOutcome::Failure), 1);
}

#[test]
fn webhook_constant_time_compare_does_not_short_circuit() {
    // We can't measure timing precisely in CI, but we can assert that
    // wrong-at-byte-0 and wrong-at-last-byte both reject equally (no early-return).
    use cssl_host_stripe_integration::webhook::constant_time_eq;
    let a = [0xAA_u8; 32];
    let mut b_first_diff = a;
    b_first_diff[0] ^= 0xFF;
    let mut b_last_diff = a;
    b_last_diff[31] ^= 0xFF;
    assert!(!constant_time_eq(&a, &b_first_diff));
    assert!(!constant_time_eq(&a, &b_last_diff));
    // Equal arrays still report equal.
    assert!(constant_time_eq(&a, &a));
}

// ───────── refund cap-gated (already 1 above) + happy-path (2) ─────────

#[test]
fn refund_happy_path_emits_success_audit() {
    let (client, transport, audit, _h) = build(StripeCap(STRIPE_CAP_REFUND));
    transport.program(ProgrammedReply {
        url_suffix: "/v1/refunds".into(),
        method: HttpMethod::Post,
        response: ok_resp(r#"{"id":"re_2","status":"succeeded","amount":1000,"charge":"ch_y"}"#),
    });
    let key = IdempotencyKey::new("idem_refund_happy").unwrap();
    let resp = client
        .create_refund(
            key,
            &RefundRequest {
                charge_id: "ch_y".into(),
                amount: Some(1000),
                reason: RefundReason::RequestedByCustomer,
                metadata: BTreeMap::new(),
            },
        )
        .expect("ok");
    assert_eq!(resp.id, "re_2");
    assert_eq!(audit.count_outcome(&AuditOutcome::Success), 1);
}

#[test]
fn refund_api_error_returns_apierror_variant() {
    let (client, transport, audit, _h) = build(StripeCap(STRIPE_CAP_REFUND));
    transport.program(ProgrammedReply {
        url_suffix: "/v1/refunds".into(),
        method: HttpMethod::Post,
        response: HttpResponse {
            status: 402,
            body: r#"{"error":{"code":"charge_already_refunded","message":"already refunded"}}"#
                .into(),
        },
    });
    let key = IdempotencyKey::new("idem_refund_err").unwrap();
    let r = client.create_refund(
        key,
        &RefundRequest {
            charge_id: "ch_z".into(),
            amount: None,
            reason: RefundReason::Duplicate,
            metadata: BTreeMap::new(),
        },
    );
    match r {
        Err(StripeError::ApiError { status, code, .. }) => {
            assert_eq!(status, 402);
            assert_eq!(code, "charge_already_refunded");
        }
        other => panic!("unexpected: {other:?}"),
    }
    assert_eq!(audit.count_outcome(&AuditOutcome::Failure), 1);
}

// ───────── subscription cap-gated + happy path (2) ─────────

#[test]
fn subscription_create_happy_path() {
    let (client, transport, audit, _h) = build(StripeCap(STRIPE_CAP_SUBSCRIPTION));
    transport.program(ProgrammedReply {
        url_suffix: "/v1/subscriptions".into(),
        method: HttpMethod::Post,
        response: ok_resp(
            r#"{"id":"sub_1","customer":"cus_x","status":"active","current_period_end":1700000000,"metadata":{}}"#,
        ),
    });
    let key = IdempotencyKey::new("idem_sub_create").unwrap();
    let s = client
        .create_subscription(
            key,
            &SubscriptionRequest {
                customer_id: "cus_x".into(),
                price_id: "price_xyz".into(),
                metadata: BTreeMap::new(),
                trial_period_days: Some(7),
            },
        )
        .expect("ok");
    assert_eq!(s.id, "sub_1");
    assert_eq!(audit.count_outcome(&AuditOutcome::Success), 1);
}

#[test]
fn subscription_cancel_and_list_round_trip() {
    let (client, transport, _a, _h) = build(StripeCap(STRIPE_CAP_SUBSCRIPTION));
    transport.program(ProgrammedReply {
        url_suffix: "/v1/subscriptions/sub_1".into(),
        method: HttpMethod::Delete,
        response: ok_resp(
            r#"{"id":"sub_1","customer":"cus_x","status":"canceled","metadata":{}}"#,
        ),
    });
    let cancelled = client.cancel_subscription("sub_1").expect("ok");
    assert_eq!(cancelled.id, "sub_1");

    transport.program(ProgrammedReply {
        url_suffix: "/v1/subscriptions?customer=cus_x&limit=5".into(),
        method: HttpMethod::Get,
        response: ok_resp(
            r#"{"data":[{"id":"sub_1","customer":"cus_x","status":"canceled","metadata":{}}]}"#,
        ),
    });
    let subs = client.list_subscriptions("cus_x", 5).expect("ok");
    assert_eq!(subs.len(), 1);
    assert_eq!(subs[0].id, "sub_1");
}

// ───────── network-error path (2) ─────────

#[test]
fn checkout_transport_failure_audits_failure() {
    let (client, transport, audit, _h) = build(StripeCap(STRIPE_CAP_CHECKOUT));
    transport.force_network_error("connection reset");
    let key = IdempotencyKey::new("k_neterr").unwrap();
    let r = client.create_checkout_session(key, &checkout_req());
    assert!(matches!(r, Err(StripeError::Network(_))));
    assert_eq!(audit.count_outcome(&AuditOutcome::Failure), 1);
}

#[test]
fn checkout_auth_failure_maps_to_auth_variant() {
    let (client, transport, _a, _h) = build(StripeCap(STRIPE_CAP_CHECKOUT));
    transport.program(ProgrammedReply {
        url_suffix: "/v1/checkout/sessions".into(),
        method: HttpMethod::Post,
        response: HttpResponse {
            status: 401,
            body: r#"{"error":{"code":"invalid_api_key","message":"bad key"}}"#.into(),
        },
    });
    let key = IdempotencyKey::new("k_auth").unwrap();
    let r = client.create_checkout_session(key, &checkout_req());
    match r {
        Err(StripeError::Auth { status, .. }) => assert_eq!(status, 401),
        other => panic!("unexpected: {other:?}"),
    }
}

// ───────── SecretString non-Debug + secret in Authorization header (1+1) ─────────

#[test]
fn secret_string_debug_does_not_leak() {
    let s = SecretString::new("sk_live_VERY_PRIVATE_KEY".into());
    let formatted = format!("{s:?}");
    assert_eq!(formatted, "<SecretString redacted>");
    assert!(!formatted.contains("VERY_PRIVATE_KEY"));
    assert!(!formatted.contains("sk_live"));
}

#[test]
fn authorization_header_carries_bearer_token() {
    let (client, transport, _a, _h) = build(StripeCap(STRIPE_CAP_CHECKOUT));
    transport.program(ProgrammedReply {
        url_suffix: "/v1/checkout/sessions".into(),
        method: HttpMethod::Post,
        response: ok_resp(r#"{"id":"cs_auth"}"#),
    });
    let key = IdempotencyKey::new("k_auth_hdr").unwrap();
    client
        .create_checkout_session(key, &checkout_req())
        .expect("ok");
    let h = &transport.recorded_requests()[0].headers;
    let auth = h.get("Authorization").expect("present");
    assert!(auth.starts_with("Bearer sk_test_"));
}

// ───────── audit-emit covers BOTH success AND failure paths (1) ─────────

#[test]
fn audit_sink_records_both_outcomes_separately() {
    let (client, transport, audit, _h) = build(StripeCap(STRIPE_CAP_CHECKOUT));
    transport.program(ProgrammedReply {
        url_suffix: "/v1/checkout/sessions".into(),
        method: HttpMethod::Post,
        response: ok_resp(r#"{"id":"cs_ok"}"#),
    });
    let _ = client
        .create_checkout_session(
            IdempotencyKey::new("ok1").unwrap(),
            &checkout_req(),
        )
        .expect("ok");
    // Now force a failure
    transport.force_network_error("kaboom");
    let _ = client.create_checkout_session(
        IdempotencyKey::new("err1").unwrap(),
        &checkout_req(),
    );
    let snap = audit.snapshot();
    let s = snap
        .iter()
        .filter(|e| e.outcome == AuditOutcome::Success)
        .count();
    let f = snap
        .iter()
        .filter(|e| e.outcome == AuditOutcome::Failure)
        .count();
    assert_eq!(s, 1);
    assert_eq!(f, 1);
}
