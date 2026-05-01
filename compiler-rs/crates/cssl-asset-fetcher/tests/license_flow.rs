//! § license_flow.rs — integration tests for W-W5 license-emit pipeline.
//! ════════════════════════════════════════════════════════════════════
//!
//! Coverage :
//!   1. fetch_records_license              — successful fetch puts record in registry
//!   2. fetch_cc_by_emits_attribution_event — AllowWithAttribution path increments counter
//!   3. fetch_unknown_license_rejected     — License::Unknown via "Other" → Deny + Err
//!   4. fetch_cc_nc_rejected               — caller-provided CCBYNC40 via custom-policy → Deny
//!   5. registry_queryable_after_fetch     — get_license_for + license_registry()
//!   6. sovereign_cap_bypasses_policy      — bypass=true admits Deny-eligible asset
//!   7. counter_increments                 — license_records_registered_total advances
//!   8. cache_hit_still_registers_license  — second fetch (cache-hit) re-registers
//!   9. policy_tightening_blocks_cache_hit — narrow policy mid-session → cache-hit denies
//!  10. multiple_assets_filter_by_license  — registry filters compatible vs requires-attribution

// RwLockReadGuard from license_registry() is intentionally short-lived in these
// tests ; the early-drop hint is a stylistic preference and the explicit
// scope-block is already minimal. Suppress at the file level.
#![allow(clippy::significant_drop_tightening)]

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use cssl_asset_fetcher::{
    telemetry_assets_rejected_license_total, telemetry_assets_with_attribution_total,
    telemetry_license_records_registered_total, AssetFetcher, AttributionLicense, FetcherError,
    LoaLicensePolicy,
};

fn unique_temp_root(tag: &str) -> PathBuf {
    let base = std::env::temp_dir();
    let pid = std::process::id();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    base.join(format!("cssl-asset-fetcher-lic-{tag}-{pid}-{nanos}"))
}

#[test]
fn fetch_records_license() {
    let root = unique_temp_root("rec");
    let f = AssetFetcher::with_cache(root.clone(), 1_000_000).unwrap();
    // CC0 asset from Kenney : Allow path, record registered.
    let _ = f
        .fetch_or_cache("kenney", "kenney:tower-defense-kit")
        .expect("CC0 fetch must succeed");
    let rec = f
        .get_license_for("kenney:tower-defense-kit")
        .expect("record present after fetch");
    assert_eq!(rec.source, "kenney");
    assert_eq!(rec.license, AttributionLicense::CC0);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn fetch_cc_by_emits_attribution_event() {
    let root = unique_temp_root("ccby");
    let f = AssetFetcher::with_cache(root.clone(), 1_000_000).unwrap();
    let pre = telemetry_assets_with_attribution_total();
    // CC-BY asset from Sketchfab : AllowWithAttribution.
    let _ = f
        .fetch_or_cache("sketchfab", "sketchfab:forest-tree-pack")
        .expect("CC-BY fetch must succeed under default policy");
    let post = telemetry_assets_with_attribution_total();
    assert!(
        post > pre,
        "CC-BY fetch must increment attribution counter: pre={pre} post={post}"
    );
    let rec = f
        .get_license_for("sketchfab:forest-tree-pack")
        .expect("record present after CC-BY fetch");
    assert_eq!(rec.license, AttributionLicense::CCBY40);
    assert_eq!(rec.author.as_deref(), Some("ForestArtist"));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn fetch_unknown_license_rejected() {
    let root = unique_temp_root("unk");
    let f = AssetFetcher::with_cache(root.clone(), 1_000_000).unwrap();
    let pre_rej = telemetry_assets_rejected_license_total();
    // The sketchfab catalog has an "Other"-license asset (proprietary tank).
    // fetcher::License::Other → host::License::Unknown → policy Deny under default.
    let r = f.fetch_or_cache("sketchfab", "sketchfab:proprietary-tank");
    let post_rej = telemetry_assets_rejected_license_total();
    match r {
        Err(FetcherError::LicenseDenied(reason)) => {
            assert!(
                reason.contains("unknown") || reason.contains("Unknown"),
                "expected unknown-license deny reason, got: {reason}"
            );
        }
        other => panic!("expected LicenseDenied, got: {other:?}"),
    }
    assert!(
        post_rej > pre_rej,
        "rejection counter must advance on Deny"
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn fetch_cc_nc_rejected() {
    // Synthesize a custom-policy that forbids attribution-required licenses
    // and confirm a CC-BY asset gets Deny'd. This exercises the
    // requires_attribution-but-policy-disallows path.
    let root = unique_temp_root("ccnc");
    let f = AssetFetcher::with_cache(root.clone(), 1_000_000).unwrap();
    let pre_rej = telemetry_assets_rejected_license_total();
    // Tighten policy : forbid attribution-required licenses outright.
    f.set_license_policy(LoaLicensePolicy {
        allow_attribution_required: false,
        allow_unknown: false,
        allow_proprietary: false,
    });
    let r = f.fetch_or_cache("sketchfab", "sketchfab:forest-tree-pack");
    let post_rej = telemetry_assets_rejected_license_total();
    match r {
        Err(FetcherError::LicenseDenied(_)) => {}
        other => panic!("expected LicenseDenied, got: {other:?}"),
    }
    assert!(post_rej > pre_rej);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn registry_queryable_after_fetch() {
    let root = unique_temp_root("reg-query");
    let f = AssetFetcher::with_cache(root.clone(), 1_000_000).unwrap();
    // Fetch two CC0 assets ; registry must know both.
    let _ = f.fetch_or_cache("kenney", "kenney:tower-defense-kit").unwrap();
    let _ = f
        .fetch_or_cache("polyhaven", "polyhaven:wood-planks-01")
        .unwrap();
    {
        let reg = f.license_registry();
        assert!(reg.get("kenney:tower-defense-kit").is_some());
        assert!(reg.get("polyhaven:wood-planks-01").is_some());
        assert!(reg.get("never-fetched").is_none());
        // Filter compatible : both CC0 are LoA-compatible.
        let compat: Vec<String> = reg
            .filter_compatible()
            .map(|r| r.asset_id.clone())
            .collect();
        assert!(compat.iter().any(|s| s == "kenney:tower-defense-kit"));
        assert!(compat.iter().any(|s| s == "polyhaven:wood-planks-01"));
    }
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn sovereign_cap_bypasses_policy() {
    let root = unique_temp_root("sovereign");
    let f = AssetFetcher::with_cache(root.clone(), 1_000_000).unwrap();
    // First confirm the asset would be denied under default policy.
    assert!(!f.sovereign_bypass());
    let r = f.fetch_or_cache("sketchfab", "sketchfab:proprietary-tank");
    assert!(
        matches!(r, Err(FetcherError::LicenseDenied(_))),
        "default policy must deny Other-license asset"
    );
    // Enable sovereign-bypass and retry.
    f.set_sovereign_bypass(true);
    assert!(f.sovereign_bypass());
    let r2 = f.fetch_or_cache("sketchfab", "sketchfab:proprietary-tank");
    assert!(
        r2.is_ok(),
        "sovereign-bypass must admit Deny-eligible asset"
    );
    // The record should be in the registry now.
    let rec = f
        .get_license_for("sketchfab:proprietary-tank")
        .expect("sovereign-bypass record present");
    assert_eq!(rec.license, AttributionLicense::Unknown);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn counter_increments() {
    let root = unique_temp_root("counter");
    let f = AssetFetcher::with_cache(root.clone(), 1_000_000).unwrap();
    let pre = telemetry_license_records_registered_total();
    let _ = f.fetch_or_cache("kenney", "kenney:tower-defense-kit").unwrap();
    let post = telemetry_license_records_registered_total();
    assert!(
        post > pre,
        "license_records_registered_total must advance on Allow"
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn cache_hit_still_registers_license() {
    let root = unique_temp_root("cache-hit-reg");
    let f = AssetFetcher::with_cache(root.clone(), 1_000_000).unwrap();
    // First fetch : cache miss, registers record.
    let _ = f.fetch_or_cache("kenney", "kenney:tower-defense-kit").unwrap();
    let pre_reg = telemetry_license_records_registered_total();
    // Second fetch : cache hit. Idempotent register on the same record →
    // counter still increments (registry-side accepts same-license repeat).
    let _ = f.fetch_or_cache("kenney", "kenney:tower-defense-kit").unwrap();
    let post_reg = telemetry_license_records_registered_total();
    assert!(
        post_reg > pre_reg,
        "cache-hit must still register : pre={pre_reg} post={post_reg}"
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn policy_tightening_blocks_cache_hit() {
    let root = unique_temp_root("tighten");
    let f = AssetFetcher::with_cache(root.clone(), 1_000_000).unwrap();
    // First fetch under default policy : succeeds.
    let _ = f
        .fetch_or_cache("sketchfab", "sketchfab:forest-tree-pack")
        .expect("CC-BY admitted under default policy");
    // Tighten policy : forbid attribution-required.
    f.set_license_policy(LoaLicensePolicy {
        allow_attribution_required: false,
        allow_unknown: false,
        allow_proprietary: false,
    });
    // Cache-hit path now denies because the gate runs again.
    let r = f.fetch_or_cache("sketchfab", "sketchfab:forest-tree-pack");
    match r {
        Err(FetcherError::LicenseDenied(_)) => {}
        other => panic!("expected LicenseDenied on cache-hit after tightening, got: {other:?}"),
    }
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn multiple_assets_filter_by_license() {
    let root = unique_temp_root("filters");
    let f = AssetFetcher::with_cache(root.clone(), 1_000_000).unwrap();
    // Mix CC0 + CC-BY.
    let _ = f.fetch_or_cache("kenney", "kenney:tower-defense-kit").unwrap();
    let _ = f
        .fetch_or_cache("sketchfab", "sketchfab:forest-tree-pack")
        .unwrap();
    let attr_required: Vec<String> = {
        let reg = f.license_registry();
        reg.filter_requires_attribution()
            .map(|r| r.asset_id.clone())
            .collect()
    };
    assert!(
        attr_required.iter().any(|s| s == "sketchfab:forest-tree-pack"),
        "CC-BY should appear in attribution-required filter"
    );
    assert!(
        !attr_required.iter().any(|s| s == "kenney:tower-defense-kit"),
        "CC0 must NOT appear in attribution-required filter"
    );
    let _ = std::fs::remove_dir_all(root);
}
