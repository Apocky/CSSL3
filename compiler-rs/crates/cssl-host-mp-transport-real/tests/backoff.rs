// § backoff.rs : tests for the Retry-After parser + exponential backoff helper

use cssl_host_mp_transport_real::{exponential_backoff, parse_retry_after};

#[test]
fn parse_retry_after_falls_back_when_unparseable() {
    assert_eq!(parse_retry_after(None), None);
    assert_eq!(parse_retry_after(Some("")), None);
    assert_eq!(parse_retry_after(Some("not-a-number")), None);
    // HTTP-date form intentionally unsupported in stage-0.
    assert_eq!(
        parse_retry_after(Some("Wed, 21 Oct 2025 07:28:00 GMT")),
        None
    );
}

#[test]
fn parse_retry_after_numeric_seconds() {
    assert_eq!(parse_retry_after(Some("1")), Some(1_000));
    assert_eq!(parse_retry_after(Some("30")), Some(30_000));
    // Whitespace is trimmed.
    assert_eq!(parse_retry_after(Some("  60   ")), Some(60_000));
    // Zero is treated as None ; caller should use its default backoff.
    assert_eq!(parse_retry_after(Some("0")), None);
}

#[test]
fn exponential_backoff_caps_correctly() {
    // base=200, cap=2000 → 200, 400, 800, 1600, 2000, 2000, …
    assert_eq!(exponential_backoff(0, 200, 2_000), 200);
    assert_eq!(exponential_backoff(1, 200, 2_000), 400);
    assert_eq!(exponential_backoff(2, 200, 2_000), 800);
    assert_eq!(exponential_backoff(3, 200, 2_000), 1_600);
    assert_eq!(exponential_backoff(4, 200, 2_000), 2_000);
    assert_eq!(exponential_backoff(20, 200, 2_000), 2_000);
    // Very large attempt saturates without panic.
    assert_eq!(exponential_backoff(u32::MAX, 200, 2_000), 2_000);
}
