//! § license_emit.rs — license-string parsing + AssetLicenseRecord builder.
//! ═════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Bridge between the asset-fetcher's per-source license-strings (free-form
//!   text returned by Sketchfab / OpenGameArt / etc.) and the typed
//!   `cssl_host_license_attribution::License` enum.
//!
//! § DESIGN
//!   - [`map_license_string`] : `(source, license_text) → License`. Per-source
//!     keyword tables ; no regex (the dep policy forbids ad-hoc regex deps in
//!     stage-0). Falls through to `License::Unknown` on no-match — `LoaLicensePolicy::default_policy()`
//!     then REJECTS by default, which is the correct conservative behavior.
//!   - [`build_record`] : assemble an `AssetLicenseRecord` with current
//!     ISO-8601-UTC timestamp.
//!   - All 5 sources have a stable mapping path ; an `unknown` license-text
//!     for any source produces `License::Unknown`.
//!
//! § PRIME-DIRECTIVE binding
//!   - License classification is conservative-by-default : when in doubt,
//!     `Unknown` ; let the policy gate refuse rather than silently shipping
//!     an unclassified asset.
//!   - No telemetry leaves the host ; license-string parsing is local-pure.

use cssl_host_license_attribution::{AssetLicenseRecord, License};

// ════════════════════════════════════════════════════════════════════
// § Public surface
// ════════════════════════════════════════════════════════════════════

/// Convert the fetcher-internal coarse `License` enum (used in `AssetMeta`
/// catalogs) to the host-license-attribution typed `License`.
///
/// The mapping is direct except for `License::Other` which routes to
/// `Unknown` (the LoA default-policy treats this as Deny — correct since
/// "Other" is the catalog's "unclassified / proprietary / custom" bucket).
#[must_use]
pub fn from_fetcher_license(l: crate::License) -> License {
    use crate::License as F;
    match l {
        F::Cc0 => License::CC0,
        F::CcBy => License::CCBY40,
        F::CcBySa => License::CCBYSA40,
        F::Gpl => License::GPLLike,
        F::Other => License::Unknown,
    }
}

/// Map a per-source license-string to the typed `License` enum.
///
/// `source` selects the per-source keyword table ; `license_text = None`
/// means the source did not supply license metadata for this asset.
///
/// Returns `License::Unknown` for unrecognized strings — the LoA default
/// policy treats this as `Deny`, which is the safest behavior.
#[must_use]
pub fn map_license_string(source: &str, license_text: Option<&str>) -> License {
    match source {
        "sketchfab" => map_sketchfab(license_text),
        "polyhaven" => map_polyhaven(license_text),
        "kenney" => map_kenney(license_text),
        "quaternius" => map_quaternius(license_text),
        "opengameart" => map_opengameart(license_text),
        _ => License::Unknown,
    }
}

/// Assemble an `AssetLicenseRecord` from the resolved fields.
///
/// `fetched_at_iso` is filled with the current UTC timestamp formatted
/// per ISO-8601 (`YYYY-MM-DDTHH:MM:SSZ`).
#[must_use]
pub fn build_record(
    asset_id: String,
    source: String,
    license: License,
    author: Option<String>,
    source_url: Option<String>,
    sha256: Option<String>,
) -> AssetLicenseRecord {
    AssetLicenseRecord {
        asset_id,
        source,
        license,
        author,
        source_url,
        fetched_at_iso: now_iso8601_utc(),
        sha256,
    }
}

// ════════════════════════════════════════════════════════════════════
// § Per-source mapping tables
// ════════════════════════════════════════════════════════════════════

fn norm(s: &str) -> String {
    s.trim().to_ascii_lowercase()
}

/// Sketchfab returns license labels like "CC0 Public Domain", "Attribution",
/// "Attribution-ShareAlike", "Attribution-NoDerivs", "Attribution-NonCommercial".
fn map_sketchfab(text: Option<&str>) -> License {
    let Some(t) = text else {
        return License::Unknown;
    };
    let n = norm(t);
    if n.is_empty() {
        return License::Unknown;
    }
    // Order matters : check the most-specific tags first.
    if n.contains("noncommercial") || n.contains("non-commercial") || n.contains("cc-by-nc") {
        return License::CCBYNC40;
    }
    if n.contains("noderivs") || n.contains("no-derivs") || n.contains("cc-by-nd") {
        return License::CCBYND40;
    }
    if n.contains("sharealike") || n.contains("share-alike") || n.contains("cc-by-sa") {
        return License::CCBYSA40;
    }
    if n.contains("cc0") || n.contains("public domain") {
        return License::CC0;
    }
    if n.contains("attribution") || n.contains("cc-by") || n.contains("cc by") {
        return License::CCBY40;
    }
    License::Unknown
}

/// PolyHaven is uniformly CC0 ; we still respect the actual response. An
/// explicit non-CC0 string will be honored.
fn map_polyhaven(text: Option<&str>) -> License {
    // Default-on-None : PolyHaven's catalog is uniformly CC0, so missing
    // text-string maps to CC0 (matches the static catalog truth-data).
    let Some(t) = text else {
        return License::CC0;
    };
    let n = norm(t);
    if n.is_empty() || n.contains("cc0") || n.contains("public domain") {
        return License::CC0;
    }
    if n.contains("noncommercial") || n.contains("cc-by-nc") {
        return License::CCBYNC40;
    }
    if n.contains("noderivs") || n.contains("cc-by-nd") {
        return License::CCBYND40;
    }
    if n.contains("sharealike") || n.contains("cc-by-sa") {
        return License::CCBYSA40;
    }
    if n.contains("attribution") || n.contains("cc-by") {
        return License::CCBY40;
    }
    License::Unknown
}

/// Kenney is uniformly CC0.
fn map_kenney(text: Option<&str>) -> License {
    let Some(t) = text else {
        return License::CC0;
    };
    let n = norm(t);
    if n.is_empty() || n.contains("cc0") || n.contains("public domain") {
        return License::CC0;
    }
    License::Unknown
}

/// Quaternius is uniformly CC0.
fn map_quaternius(text: Option<&str>) -> License {
    let Some(t) = text else {
        return License::CC0;
    };
    let n = norm(t);
    if n.is_empty() || n.contains("cc0") || n.contains("public domain") {
        return License::CC0;
    }
    License::Unknown
}

/// OpenGameArt has the most heterogeneous licensing : CC0 / CC-BY / CC-BY-SA /
/// CC-BY-NC / CC-BY-ND / GPL-2.0 / GPL-3.0 / OGA-BY-3.0 / MIT / Apache.
/// 12-rule keyword table (no regex) :
///   1. cc0 / public-domain → CC0
///   2. cc-by-nc-sa / by-nc-sa → CCBYNC40 (NC dominates)
///   3. cc-by-nc / by-nc / noncommercial → CCBYNC40
///   4. cc-by-nd / by-nd / noderivs → CCBYND40
///   5. cc-by-sa / by-sa / sharealike → CCBYSA40
///   6. cc-by / by 3.0 / by 4.0 / attribution → CCBY40
///   7. oga-by → CCBY40 (OGA-BY is treated as CC-BY-equivalent for our policy)
///   8. gpl → GPLLike
///   9. mit → MITLike
///  10. apache → ApacheLike
///  11. proprietary / commercial-only → ProprietaryUnlicensed
///  12. fallthrough → Unknown
fn map_opengameart(text: Option<&str>) -> License {
    let Some(t) = text else {
        return License::Unknown;
    };
    let n = norm(t);
    if n.is_empty() {
        return License::Unknown;
    }
    // 1. cc0
    if n.contains("cc0") || n.contains("public domain") {
        return License::CC0;
    }
    // 2-3. nc family (must come before plain cc-by)
    if n.contains("noncommercial")
        || n.contains("non-commercial")
        || n.contains("cc-by-nc")
        || n.contains("by-nc")
    {
        return License::CCBYNC40;
    }
    // 4. nd family
    if n.contains("noderivs")
        || n.contains("no-derivs")
        || n.contains("cc-by-nd")
        || n.contains("by-nd")
    {
        return License::CCBYND40;
    }
    // 5. sa family
    if n.contains("sharealike")
        || n.contains("share-alike")
        || n.contains("cc-by-sa")
        || n.contains("by-sa")
    {
        return License::CCBYSA40;
    }
    // 6-7. plain attribution (CC-BY + OGA-BY)
    if n.contains("cc-by") || n.contains("oga-by") || n.contains("attribution") {
        return License::CCBY40;
    }
    // 8-10. permissive code-style licenses
    if n.contains("gpl") {
        return License::GPLLike;
    }
    if n.contains("mit") {
        return License::MITLike;
    }
    if n.contains("apache") {
        return License::ApacheLike;
    }
    // 11. proprietary
    if n.contains("proprietary") || n.contains("commercial-only") || n.contains("all-rights-reserved")
    {
        return License::ProprietaryUnlicensed;
    }
    // 12. fallthrough
    License::Unknown
}

// ════════════════════════════════════════════════════════════════════
// § Time helpers
// ════════════════════════════════════════════════════════════════════

/// Format the current UTC time as ISO-8601 (`YYYY-MM-DDTHH:MM:SSZ`).
///
/// Stage-0 formatter ; uses only `std::time` so we avoid pulling chrono /
/// time-rs. Falls back to `1970-01-01T00:00:00Z` if the clock is broken.
fn now_iso8601_utc() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    secs_to_iso8601(secs)
}

/// Convert UNIX seconds to ISO-8601 UTC string. Civil-calendar arithmetic
/// for dates > 1970 ; matches strftime("%Y-%m-%dT%H:%M:%SZ").
fn secs_to_iso8601(secs: u64) -> String {
    let total_secs = secs;
    let secs_per_day: u64 = 86_400;
    let days = total_secs / secs_per_day;
    let day_secs = total_secs % secs_per_day;
    let hh = day_secs / 3600;
    let mm = (day_secs % 3600) / 60;
    let ss = day_secs % 60;

    // Days from 1970-01-01.
    let (y, mo, d) = days_to_ymd(days);
    format!("{y:04}-{mo:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}Z")
}

/// Days-since-1970-01-01 → (year, month, day). Standard civil-from-days.
#[allow(clippy::similar_names)]
fn days_to_ymd(days: u64) -> (u64, u64, u64) {
    // Howard Hinnant's date algorithms (public domain).
    // Convert days-since-epoch (1970-01-01) to civil date.
    let zd = days as i64 + 719_468;
    let era = if zd >= 0 { zd } else { zd - 146_096 } / 146_097;
    let day_of_era = (zd - era * 146_097) as u64; // [0, 146096]
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365; // [0, 399]
    let y = year_of_era as i64 + era * 400;
    let day_of_year =
        day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100); // [0, 365]
    let month_part = (5 * day_of_year + 2) / 153; // [0, 11]
    let day_of_month = day_of_year - (153 * month_part + 2) / 5 + 1; // [1, 31]
    let month = if month_part < 10 {
        month_part + 3
    } else {
        month_part - 9
    }; // [1, 12]
    let year = if month <= 2 { y + 1 } else { y };
    (year as u64, month, day_of_month)
}

// ════════════════════════════════════════════════════════════════════
// § Tests
// ════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sketchfab_cc0_mapped() {
        assert_eq!(
            map_license_string("sketchfab", Some("CC0 Public Domain")),
            License::CC0
        );
        assert_eq!(map_license_string("sketchfab", Some("cc0")), License::CC0);
    }

    #[test]
    fn sketchfab_ccby_mapped() {
        assert_eq!(
            map_license_string("sketchfab", Some("Attribution")),
            License::CCBY40
        );
        assert_eq!(
            map_license_string("sketchfab", Some("CC Attribution 4.0")),
            License::CCBY40
        );
        assert_eq!(
            map_license_string("sketchfab", Some("Attribution-ShareAlike")),
            License::CCBYSA40
        );
    }

    #[test]
    fn polyhaven_cc0() {
        // PolyHaven is uniformly CC0 — None or empty → CC0 (catalog truth).
        assert_eq!(map_license_string("polyhaven", None), License::CC0);
        assert_eq!(map_license_string("polyhaven", Some("")), License::CC0);
        assert_eq!(map_license_string("polyhaven", Some("CC0")), License::CC0);
    }

    #[test]
    fn openga_mit() {
        assert_eq!(
            map_license_string("opengameart", Some("MIT")),
            License::MITLike
        );
        assert_eq!(
            map_license_string("opengameart", Some("MIT License")),
            License::MITLike
        );
    }

    #[test]
    fn openga_unknown_defaults_to_unknown() {
        // Unparseable license-string → Unknown ; the policy will REJECT.
        assert_eq!(
            map_license_string("opengameart", Some("This is some garbage")),
            License::Unknown
        );
        assert_eq!(map_license_string("opengameart", Some("")), License::Unknown);
        assert_eq!(map_license_string("opengameart", None), License::Unknown);
    }

    #[test]
    fn build_record_iso_now() {
        let r = build_record(
            "test:asset-1".into(),
            "kenney".into(),
            License::CC0,
            None,
            None,
            None,
        );
        assert_eq!(r.asset_id, "test:asset-1");
        assert_eq!(r.source, "kenney");
        assert_eq!(r.license, License::CC0);
        // ISO-8601 with Z suffix + 4-digit year.
        assert!(r.fetched_at_iso.ends_with('Z'), "iso={}", r.fetched_at_iso);
        // Year must be > 2024 since this test runs in 2026+ ; cushion a bit.
        let year_str = &r.fetched_at_iso[..4];
        let year: u64 = year_str.parse().expect("4-digit year");
        assert!(year >= 2024, "year too low: {year}");
    }

    #[test]
    fn build_record_sha_included() {
        let r = build_record(
            "test:asset-2".into(),
            "polyhaven".into(),
            License::CC0,
            Some("Poly Haven".into()),
            Some("https://polyhaven.com/a/x".into()),
            Some("deadbeef".into()),
        );
        assert_eq!(r.author.as_deref(), Some("Poly Haven"));
        assert_eq!(r.source_url.as_deref(), Some("https://polyhaven.com/a/x"));
        assert_eq!(r.sha256.as_deref(), Some("deadbeef"));
    }

    #[test]
    fn all_5_sources_have_mapping_for_empty_string() {
        // Every source must produce a deterministic License (not panic) for
        // empty / None inputs.  Some sources (kenney / quaternius / polyhaven)
        // default to CC0 ; sketchfab + opengameart default to Unknown (which
        // the LoA default-policy treats as Deny).
        assert_eq!(map_license_string("kenney", None), License::CC0);
        assert_eq!(map_license_string("quaternius", None), License::CC0);
        assert_eq!(map_license_string("polyhaven", None), License::CC0);
        assert_eq!(map_license_string("sketchfab", None), License::Unknown);
        assert_eq!(map_license_string("opengameart", None), License::Unknown);
        // Unrecognized source → Unknown.
        assert_eq!(map_license_string("nope", None), License::Unknown);
    }

    #[test]
    fn iso8601_known_date() {
        // 2026-04-30T00:00:00Z = 1777507200.
        // 2025-04-30T00:00:00Z = 1745971200 (cross-check).
        // 1970-01-01T00:00:00Z = 0 (epoch sanity).
        assert_eq!(secs_to_iso8601(0), "1970-01-01T00:00:00Z");
        assert_eq!(secs_to_iso8601(1_745_971_200), "2025-04-30T00:00:00Z");
        assert_eq!(secs_to_iso8601(1_777_507_200), "2026-04-30T00:00:00Z");
    }
}
