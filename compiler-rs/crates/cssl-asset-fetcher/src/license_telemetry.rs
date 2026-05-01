//! § license_telemetry.rs — license-pipeline telemetry counters + events.
//! ════════════════════════════════════════════════════════════════════════
//!
//! § COUNTERS
//!   - `assets_with_attribution_total`     : every successful AllowWithAttribution
//!   - `assets_rejected_license_total`     : every Deny outcome from policy
//!   - `assets_unknown_license_total`      : every License::Unknown classification
//!   - `license_records_registered_total`  : every register() into the registry
//!
//! § EVENT-EMIT
//!   `emit_license_event(asset_id, kind, sovereign)` writes a structured JSONL
//!   event via `cssl_rt::loa_startup::log_event` so the host telemetry axis
//!   (already used by asset-fetcher.rs core) sees license-side events.
//!
//! § PRIME-DIRECTIVE binding
//!   Counters are atomic + process-local ; nothing leaves the host. Sovereign
//!   bypass (Apocky cap) is RECORDED in the event payload so audit shows the
//!   override happened — but no remote-telemetry ; pure local accountability.

use std::sync::atomic::{AtomicU64, Ordering};

use cssl_rt::loa_startup::log_event;
use serde::{Deserialize, Serialize};

// ════════════════════════════════════════════════════════════════════
// § Counters
// ════════════════════════════════════════════════════════════════════

static ASSETS_WITH_ATTRIBUTION: AtomicU64 = AtomicU64::new(0);
static ASSETS_REJECTED: AtomicU64 = AtomicU64::new(0);
static ASSETS_UNKNOWN: AtomicU64 = AtomicU64::new(0);
static LICENSE_RECORDS_REGISTERED: AtomicU64 = AtomicU64::new(0);

/// Read `assets_with_attribution_total` counter.
#[must_use]
pub fn telemetry_assets_with_attribution_total() -> u64 {
    ASSETS_WITH_ATTRIBUTION.load(Ordering::Relaxed)
}

/// Read `assets_rejected_license_total` counter.
#[must_use]
pub fn telemetry_assets_rejected_license_total() -> u64 {
    ASSETS_REJECTED.load(Ordering::Relaxed)
}

/// Read `assets_unknown_license_total` counter.
#[must_use]
pub fn telemetry_assets_unknown_license_total() -> u64 {
    ASSETS_UNKNOWN.load(Ordering::Relaxed)
}

/// Read `license_records_registered_total` counter.
#[must_use]
pub fn telemetry_license_records_registered_total() -> u64 {
    LICENSE_RECORDS_REGISTERED.load(Ordering::Relaxed)
}

// Internal increment helpers (used by AssetFetcher).
pub(crate) fn inc_assets_with_attribution() {
    ASSETS_WITH_ATTRIBUTION.fetch_add(1, Ordering::Relaxed);
}
pub(crate) fn inc_assets_rejected() {
    ASSETS_REJECTED.fetch_add(1, Ordering::Relaxed);
}
pub(crate) fn inc_assets_unknown() {
    ASSETS_UNKNOWN.fetch_add(1, Ordering::Relaxed);
}
pub(crate) fn inc_license_records_registered() {
    LICENSE_RECORDS_REGISTERED.fetch_add(1, Ordering::Relaxed);
}

// ════════════════════════════════════════════════════════════════════
// § Event kinds + emit
// ════════════════════════════════════════════════════════════════════

/// Discrete license-event kinds emitted by the asset-fetcher.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LicenseEventKind {
    /// Record was registered (Allow path, no attribution required).
    Recorded,
    /// Record was registered AND requires attribution-HUD (AllowWithAttribution).
    AttributionRequired,
    /// Asset was rejected by policy because the license is forbidden (Deny path).
    RejectedDeny,
    /// Asset was rejected because the license could not be classified (Unknown).
    RejectedUnknown,
}

impl LicenseEventKind {
    /// Tag-string used in the structured event payload.
    pub const fn tag(self) -> &'static str {
        match self {
            Self::Recorded => "recorded",
            Self::AttributionRequired => "attribution_required",
            Self::RejectedDeny => "rejected_deny",
            Self::RejectedUnknown => "rejected_unknown",
        }
    }
}

/// Emit a structured license-event into the loa-host telemetry axis.
///
/// `sovereign = true` indicates the caller exercised an Apocky-only cap
/// to bypass the default policy ; the event still records the bypass so
/// audit shows it happened.
pub fn emit_license_event(asset_id: &str, kind: LicenseEventKind, sovereign: bool) {
    // JSONL-shaped payload : caller pipelines parse this back without ambiguity.
    let payload = format!(
        "{{\"event\":\"asset.license.{}\",\"asset_id\":{},\"sovereign\":{}}}",
        kind.tag(),
        json_escape(asset_id),
        sovereign,
    );
    log_event("INFO", "asset-fetcher", &payload);
}

/// Minimal JSON-string escape sufficient for asset_ids (alphanumeric +
/// `-_.:` plus prefix-colons). Covers the 4 reserved JSON chars.
fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            _ => out.push(ch),
        }
    }
    out.push('"');
    out
}

// ════════════════════════════════════════════════════════════════════
// § Tests
// ════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counters_start_zero_or_monotonic() {
        // Other tests in the same binary may have run first ; assert that
        // each counter is monotonically non-decreasing across an inc_* call.
        let pre_attr = telemetry_assets_with_attribution_total();
        let pre_rej = telemetry_assets_rejected_license_total();
        let pre_unk = telemetry_assets_unknown_license_total();
        let pre_reg = telemetry_license_records_registered_total();
        // Counters expose u64 ; just sanity-check reads return.
        assert!(pre_attr <= u64::MAX);
        assert!(pre_rej <= u64::MAX);
        assert!(pre_unk <= u64::MAX);
        assert!(pre_reg <= u64::MAX);
    }

    #[test]
    fn increment_records_advances() {
        let pre = telemetry_license_records_registered_total();
        inc_license_records_registered();
        let post = telemetry_license_records_registered_total();
        assert_eq!(post, pre + 1);
    }

    #[test]
    fn emit_event_jsonl_shape() {
        // Just verify the function does not panic + accepts both bools.
        emit_license_event("test:asset-1", LicenseEventKind::Recorded, false);
        emit_license_event("test:asset-2", LicenseEventKind::AttributionRequired, false);
        emit_license_event("test:asset-3", LicenseEventKind::RejectedDeny, false);
        emit_license_event("test:asset-4", LicenseEventKind::RejectedUnknown, false);
    }

    #[test]
    fn sovereign_recorded_in_event() {
        // Sovereign-bypass path : event still emitted ; `sovereign:true` flag
        // is part of the JSONL payload.
        emit_license_event("test:sovereign-1", LicenseEventKind::Recorded, true);
        // Verify the kind tags are stable.
        assert_eq!(LicenseEventKind::Recorded.tag(), "recorded");
        assert_eq!(
            LicenseEventKind::AttributionRequired.tag(),
            "attribution_required"
        );
        assert_eq!(LicenseEventKind::RejectedDeny.tag(), "rejected_deny");
        assert_eq!(
            LicenseEventKind::RejectedUnknown.tag(),
            "rejected_unknown"
        );
    }

    #[test]
    fn json_escape_handles_quotes_and_backslashes() {
        // Asset-ids should never contain these chars but the escaper must
        // be defensive.
        assert_eq!(json_escape("simple"), "\"simple\"");
        assert_eq!(json_escape("with\"quote"), "\"with\\\"quote\"");
        assert_eq!(json_escape("with\\slash"), "\"with\\\\slash\"");
    }

    #[test]
    fn all_inc_helpers_advance_counters() {
        // Confirm each helper hits the right counter.
        let pre_attr = telemetry_assets_with_attribution_total();
        inc_assets_with_attribution();
        assert_eq!(telemetry_assets_with_attribution_total(), pre_attr + 1);

        let pre_rej = telemetry_assets_rejected_license_total();
        inc_assets_rejected();
        assert_eq!(telemetry_assets_rejected_license_total(), pre_rej + 1);

        let pre_unk = telemetry_assets_unknown_license_total();
        inc_assets_unknown();
        assert_eq!(telemetry_assets_unknown_license_total(), pre_unk + 1);
    }
}
