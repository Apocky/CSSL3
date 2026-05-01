// § backoff.rs : Retry-After parser + exponential-backoff helper
//
// The Supabase REST surface returns 429 (rate-limited) with an optional
// `Retry-After` header. Per RFC 7231 § 7.1.3 the value is either an
// integer count of seconds OR an HTTP-date. We support the integer form
// only — Supabase emits seconds in practice, and an HTTP-date parser is
// out of scope for stage-0. When the header is absent or unparseable we
// fall back to the configured default (passed via `SupabaseConfig`).
//
// The exponential-backoff helper implements 2^attempt × base, capped at
// `cap_ms`. Used by the loa-host orchestrator on top of this transport ;
// the transport itself never retries internally — it surfaces the
// `TransportErr::Backoff(ms)` and the caller decides.

/// Parse the value of an HTTP `Retry-After` header into milliseconds.
///
/// Returns `Some(ms)` when the value is a positive integer count of
/// seconds (multiplied by 1000 ≤ `u32::MAX`). Returns `None` for absent
/// headers or unparseable values. The integer-form-only constraint is
/// documented in the module preamble.
#[must_use]
pub fn parse_retry_after(header_value: Option<&str>) -> Option<u32> {
    let raw = header_value?.trim();
    let secs: u64 = raw.parse().ok()?;
    // Cap the multiplied value at u32::MAX so callers can safely store
    // it in a u32 field without overflow.
    let ms = secs.saturating_mul(1000);
    if ms == 0 {
        // Servers that return `Retry-After: 0` are legitimately asking
        // for an immediate retry ; treat that as None so the caller
        // applies its default backoff rather than racing.
        return None;
    }
    if ms > u64::from(u32::MAX) {
        return Some(u32::MAX);
    }
    Some(ms as u32)
}

/// Exponential backoff formula : `min(base × 2^attempt, cap)`.
///
/// `attempt` is 0-indexed — pass 0 for the first retry, 1 for the second,
/// etc. The shift is saturated so very-large attempts yield `cap_ms`
/// without overflow.
#[must_use]
pub fn exponential_backoff(attempt: u32, base_ms: u32, cap_ms: u32) -> u32 {
    if base_ms == 0 || cap_ms == 0 {
        return 0;
    }
    // 2^attempt as u64 to avoid overflow ; cap at cap_ms then downcast.
    let shift = attempt.min(31); // 2^31 fits in u32 ; beyond is moot since we cap.
    let mult: u64 = 1u64 << shift;
    let backoff: u64 = u64::from(base_ms).saturating_mul(mult);
    if backoff > u64::from(cap_ms) {
        cap_ms
    } else {
        backoff as u32
    }
}

// ─── tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_retry_after_handles_integer_seconds() {
        assert_eq!(parse_retry_after(Some("5")), Some(5_000));
        assert_eq!(parse_retry_after(Some("  120 ")), Some(120_000));
        assert_eq!(parse_retry_after(Some("1")), Some(1_000));
    }

    #[test]
    fn parse_retry_after_returns_none_for_unparseable() {
        assert_eq!(parse_retry_after(None), None);
        assert_eq!(parse_retry_after(Some("")), None);
        assert_eq!(parse_retry_after(Some("not-a-number")), None);
        // HTTP-date form intentionally NOT supported.
        assert_eq!(parse_retry_after(Some("Wed, 21 Oct 2025 07:28:00 GMT")), None);
        // Zero is treated as None so caller falls back to its default.
        assert_eq!(parse_retry_after(Some("0")), None);
    }

    #[test]
    fn parse_retry_after_caps_at_u32_max() {
        // 5_000_000 secs × 1000 = 5_000_000_000 ms > u32::MAX (4_294_967_295)
        assert_eq!(parse_retry_after(Some("5000000")), Some(u32::MAX));
    }

    #[test]
    fn exponential_backoff_doubles_then_caps() {
        // base=100, cap=1600 → 100, 200, 400, 800, 1600, 1600, 1600 …
        assert_eq!(exponential_backoff(0, 100, 1_600), 100);
        assert_eq!(exponential_backoff(1, 100, 1_600), 200);
        assert_eq!(exponential_backoff(2, 100, 1_600), 400);
        assert_eq!(exponential_backoff(3, 100, 1_600), 800);
        assert_eq!(exponential_backoff(4, 100, 1_600), 1_600);
        assert_eq!(exponential_backoff(5, 100, 1_600), 1_600);
        assert_eq!(exponential_backoff(31, 100, 1_600), 1_600);
        // very-large attempts saturate at cap
        assert_eq!(exponential_backoff(u32::MAX, 100, 1_600), 1_600);
    }

    #[test]
    fn exponential_backoff_zero_inputs_return_zero() {
        assert_eq!(exponential_backoff(5, 0, 1_000), 0);
        assert_eq!(exponential_backoff(5, 100, 0), 0);
    }
}
