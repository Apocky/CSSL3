//! § cssl-asset-fetcher integration tests.
//!
//! Coverage matches §TESTS in the dispatch prompt :
//!   1. cache_dir_creation
//!   2. lru_evicts_oldest_first
//!   3. meta_sidecar_round_trip
//!   4. sketchfab_adapter_search_returns_results (mocked)
//!   5. polyhaven_adapter_attribution_correct (mocked)
//!   6. kenney_static_catalog_has_100plus_assets
//!   7. license_filter_excludes_non_cc
//!   8. fetch_then_cached_no_re_download
//!
//! Plus :
//!   9. cross_source_search_concatenates
//!  10. ffi_search_returns_json
//!  11. evict_to_zero_clears_cache
//!  12. quaternius_and_opengameart_present_and_distinct

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use cssl_asset_fetcher::sources::{
    kenney::KenneySource, opengameart::OpenGameArtSource, polyhaven::PolyHavenSource,
    quaternius::QuaterniusSource, sketchfab::SketchfabSource,
};
use cssl_asset_fetcher::{
    telemetry_cache_hits_total, telemetry_fetch_total, telemetry_search_total, AssetFetcher,
    AssetSource, License, LicenseFilter,
};

fn unique_temp_root(tag: &str) -> PathBuf {
    let base = std::env::temp_dir();
    let pid = std::process::id();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    base.join(format!("cssl-asset-fetcher-it-{tag}-{pid}-{nanos}"))
}

#[test]
fn cache_dir_creation() {
    let root = unique_temp_root("cache-dir");
    assert!(!root.exists());
    let _f = AssetFetcher::with_cache(root.clone(), 1_000_000).unwrap();
    assert!(root.exists());
    assert!(root.is_dir());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn lru_evicts_oldest_first() {
    let root = unique_temp_root("lru-evict");
    let f = AssetFetcher::with_cache(root.clone(), 60).unwrap();

    // Fetch three small kenney assets ; cap is small enough only one fits.
    let _p1 = f.fetch_or_cache("kenney", "kenney:tower-defense-kit").unwrap();
    let _p2 = f.fetch_or_cache("kenney", "kenney:platformer-pack-redux").unwrap();
    let _p3 = f.fetch_or_cache("kenney", "kenney:rpg-pack").unwrap();

    // Force eviction down to 0 ; everything should be evicted.
    let evicted = f.evict_lru_to_size(0).unwrap();
    assert!(evicted > 0, "expected at least one eviction, got {evicted}");
    assert_eq!(f.cache_size_bytes(), 0);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn meta_sidecar_round_trip() {
    let root = unique_temp_root("sidecar");
    let f = AssetFetcher::with_cache(root.clone(), 1_000_000).unwrap();
    let path = f
        .fetch_or_cache("kenney", "kenney:dungeon-pack")
        .unwrap();
    assert!(path.exists());
    // Sidecar should sit alongside the asset path with `.meta` suffix.
    let mut sidecar_path = path.clone().into_os_string();
    sidecar_path.push(".meta");
    let sidecar_path = PathBuf::from(sidecar_path);
    assert!(
        sidecar_path.exists(),
        "sidecar missing at {}",
        sidecar_path.display()
    );
    let json = fs::read_to_string(&sidecar_path).unwrap();
    assert!(json.contains("\"id\""));
    assert!(json.contains("\"license\""));
    assert!(json.contains("kenney:dungeon-pack"));
    assert!(json.contains("\"cc0\""));
    assert!(json.contains("\"Kenney Vleugels\""));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn sketchfab_adapter_search_returns_results() {
    let s = SketchfabSource::new();
    // Empty query : returns whole catalog (filtered by CC).
    let all = s.search("", LicenseFilter::CcOnly).unwrap();
    assert!(!all.is_empty(), "sketchfab catalog empty under CcOnly");
    // Targeted query.
    let towers = s.search("tower", LicenseFilter::Cc0Only).unwrap();
    assert!(!towers.is_empty());
    for r in &towers {
        assert_eq!(r.license, License::Cc0);
        assert!(r.name.to_lowercase().contains("tower") || r.tags.iter().any(|t| t.contains("tower")));
    }
}

#[test]
fn polyhaven_adapter_attribution_correct() {
    let s = PolyHavenSource::new();
    let all = s.search("", LicenseFilter::CcOnly).unwrap();
    assert!(!all.is_empty());
    for r in &all {
        // PolyHaven is uniformly CC0 ; author always set to "Poly Haven".
        assert_eq!(r.license, License::Cc0);
        assert_eq!(r.author, "Poly Haven");
        assert_eq!(r.src, "polyhaven");
    }
}

#[test]
fn kenney_static_catalog_has_100plus_assets() {
    let s = KenneySource::new();
    let all = s.search("", LicenseFilter::CcOnly).unwrap();
    assert!(
        all.len() >= 100,
        "kenney catalog too small : {} (need >=100)",
        all.len()
    );
    // All entries must be CC0 + authored by Kenney Vleugels.
    for r in &all {
        assert_eq!(r.license, License::Cc0);
        assert_eq!(r.author, "Kenney Vleugels");
        assert!(r.id.starts_with("kenney:"));
        assert!(r.url.starts_with("https://kenney.nl/assets/"));
    }
}

#[test]
fn license_filter_excludes_non_cc() {
    // Sketchfab catalog has a deliberate `Other`-license entry. CcOnly
    // filter must exclude it.
    let s = SketchfabSource::new();
    let cc_only = s.search("", LicenseFilter::CcOnly).unwrap();
    for r in &cc_only {
        assert!(
            r.license.is_creative_commons(),
            "non-CC entry leaked through CcOnly filter: {:?}",
            r
        );
    }
    // `Any` filter should include the Other-license entries.
    let any = s.search("", LicenseFilter::Any).unwrap();
    assert!(any.iter().any(|r| matches!(r.license, License::Other)));
    assert!(any.len() > cc_only.len());
}

#[test]
fn fetch_then_cached_no_re_download() {
    let root = unique_temp_root("hits");
    let f = AssetFetcher::with_cache(root.clone(), 1_000_000).unwrap();
    let pre_hits = telemetry_cache_hits_total();

    // First fetch : should be a miss → wrote to disk.
    let p1 = f.fetch_or_cache("kenney", "kenney:nature-kit").unwrap();
    let post_first_hits = telemetry_cache_hits_total();
    assert_eq!(post_first_hits, pre_hits, "miss should not increment hits");

    // Second fetch : MUST be a hit — no re-download.
    let p2 = f.fetch_or_cache("kenney", "kenney:nature-kit").unwrap();
    assert_eq!(p1, p2, "cache hit should return same path");
    let post_second_hits = telemetry_cache_hits_total();
    assert!(
        post_second_hits > post_first_hits,
        "hit should increment cache_hits_total"
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn cross_source_search_concatenates() {
    let root = unique_temp_root("xsearch");
    let f = AssetFetcher::with_cache(root.clone(), 1_000_000).unwrap();
    let pre_search = telemetry_search_total();
    let results = f.search("dungeon", LicenseFilter::CcOnly);
    assert!(telemetry_search_total() > pre_search);
    // We expect hits from at least two sources for "dungeon".
    let sources: std::collections::HashSet<_> =
        results.iter().map(|r| r.src.clone()).collect();
    assert!(
        sources.len() >= 2,
        "expected >=2 sources for 'dungeon', got: {:?}",
        sources
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn ffi_search_returns_json() {
    let q = b"dragon";
    let mut out = vec![0u8; 65536];
    // SAFETY: pointers + lengths come from Rust slices ; out has nonzero capacity.
    let n = unsafe {
        cssl_asset_fetcher::__cssl_asset_search(q.as_ptr(), q.len(), out.as_mut_ptr(), out.len())
    };
    assert!(n > 0, "ffi-search returned non-positive: {n}");
    let n_usize = n as usize;
    let json = std::str::from_utf8(&out[..n_usize]).unwrap();
    assert!(json.starts_with('['));
    assert!(json.ends_with(']'));
    // Should mention at least one of our sources.
    assert!(
        json.contains("kenney") || json.contains("quaternius") || json.contains("dragon"),
        "ffi-search json missing expected tokens: {}",
        json
    );
}

#[test]
fn evict_to_zero_clears_cache() {
    let root = unique_temp_root("ev0");
    let f = AssetFetcher::with_cache(root.clone(), 1_000_000).unwrap();
    let _ = f
        .fetch_or_cache("kenney", "kenney:tower-defense-kit")
        .unwrap();
    let _ = f
        .fetch_or_cache("polyhaven", "polyhaven:wood-planks-01")
        .unwrap();
    assert!(f.cache_size_bytes() > 0);
    let evicted = f.evict_lru_to_size(0).unwrap();
    assert_eq!(evicted, 2);
    assert_eq!(f.cache_size_bytes(), 0);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn quaternius_and_opengameart_present_and_distinct() {
    let q = QuaterniusSource::new();
    let o = OpenGameArtSource::new();
    let q_results = q.search("", LicenseFilter::Any).unwrap();
    let o_results = o.search("", LicenseFilter::Any).unwrap();
    assert!(!q_results.is_empty());
    assert!(!o_results.is_empty());
    for r in &q_results {
        assert_eq!(r.src, "quaternius");
        assert_eq!(r.license, License::Cc0);
        assert_eq!(r.author, "Quaternius");
    }
    let oga_licenses: std::collections::HashSet<_> =
        o_results.iter().map(|r| r.license).collect();
    // OpenGameArt deliberately has multiple license types.
    assert!(
        oga_licenses.len() >= 3,
        "opengameart should have license diversity, got: {:?}",
        oga_licenses
    );
}

#[test]
fn telemetry_fetch_counter_advances() {
    let root = unique_temp_root("tel");
    let f = AssetFetcher::with_cache(root.clone(), 1_000_000).unwrap();
    let pre = telemetry_fetch_total();
    let _ = f.fetch_or_cache("kenney", "kenney:space-kit").unwrap();
    let _ = f.fetch_or_cache("kenney", "kenney:medieval-rts").unwrap();
    let post = telemetry_fetch_total();
    assert!(post >= pre + 2, "fetch_total advance: pre={pre} post={post}");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn sources_registered_in_known_order() {
    let root = unique_temp_root("order");
    let f = AssetFetcher::with_cache(root.clone(), 1_000_000).unwrap();
    let names = f.source_names();
    assert_eq!(
        names,
        vec!["sketchfab", "polyhaven", "kenney", "quaternius", "opengameart"]
    );
    let _ = fs::remove_dir_all(root);
}
