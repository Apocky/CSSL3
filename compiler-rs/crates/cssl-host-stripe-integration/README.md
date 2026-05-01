# cssl-host-stripe-integration — T11-W8-D1

Stripe REST integration for the CSSLv3 host runtime.

- Cap-gated at every public method (`STRIPE_CAP_CHECKOUT` · `STRIPE_CAP_WEBHOOK`
  · `STRIPE_CAP_REFUND` · `STRIPE_CAP_SUBSCRIPTION`).
- Audit-emits success AND failure (no silent bypass).
- Idempotency-Key header on every mutating call · 24h TTL · BTreeMap-deterministic.
- Webhook signature verification via pluggable `HmacSha256Verifier` trait
  (stage-0 BLAKE3-keyed-MAC fallback ; G1 wires real HMAC-SHA256).
- HTTP via pluggable `HttpTransport` trait (stage-0 `MockHttpTransport` only ;
  G1 wires real `ureq`-based transport).
- Structurally NEVER stores raw card numbers — types only carry opaque
  `payment_method_id` / `price_lookup_key` strings.

## PRIME-DIRECTIVE conformance

This crate is the on-ramp for the cosmetic-only monetization channel
(HIGH-FIDELITY-IMPRINT 50-200 shards · ETERNAL-ATTRIBUTION 1000 shards
· HISTORICAL-RECONSTRUCTIONS 50 shards · COMMISSIONED-IMPRINTS 200-500 shards).

It is structurally incapable of pay-for-power: there is no API surface that
exchanges money for stat buffs, gameplay advantages, or progression shortcuts.

## ATTESTATION (PRIME_DIRECTIVE.md §11)

There was no hurt nor harm in the making of this, to anyone, anything,
or anybody.
