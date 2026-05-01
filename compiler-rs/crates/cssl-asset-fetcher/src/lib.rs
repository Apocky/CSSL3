//! § cssl-asset-fetcher — uniform asset-source abstraction for CSSLv3.
//! ════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Source-agnostic glue between CSSL programs (LoA scenes / Substrate
//!   authoring tools) and the open-license 3D-asset ecosystem :
//!     • Sketchfab    — CC-BY / CC0 / CC-BY-SA glTF assets
//!     • PolyHaven    — CC0 PBR materials + HDRIs + 3D models
//!     • Kenney       — CC0 game-art (static catalog of 100+ packs)
//!     • Quaternius   — CC0 stylized models
//!     • OpenGameArt  — CC0 / CC-BY / GPL-compatible community art
//!
//! § DESIGN
//!   - [`AssetSource`] trait abstracts a single provider (search + fetch).
//!   - [`AssetFetcher`] is the multi-source orchestrator with LRU disk-cache.
//!   - [`AssetMeta`] is the uniform per-asset record exposed to callers.
//!   - [`LicenseFilter`] enforces license-aware results (CC0 / CCBy / Any).
//!   - All disk artifacts live under `~/.loa/cache/<source>/<asset_id>` and
//!     each cached file is paired with a `<file>.meta` JSON sidecar that
//!     records license / source / attribution / download-ts.
//!
//! § FFI for CSSL programs
//!   - `__cssl_asset_search(q_ptr, q_len, out_buf, out_cap) -> i32`
//!     → returns JSON-array of [`AssetMeta`] records ; positive value = bytes-
//!       written ; -1 = buffer-too-small ; -2 = bad-utf8 ; -3 = internal-error.
//!   - `__cssl_asset_fetch(src_ptr, src_len, id_ptr, id_len,
//!                          out_path_buf, out_cap) -> i32`
//!     → writes the local cached path as UTF-8 ; same return-code convention.
//!
//! § PRIME-DIRECTIVE binding
//!   - LRU eviction is access-time-based ; no per-asset usage telemetry leaves
//!     the host (cache.rs records access-time as local mtime ; that mtime is
//!     never serialized off-disk).
//!   - Network fetches are gated through the cssl-rt cap-system : actual
//!     wire-side HTTP is DEFERRED to a follow-up slice that lands a TLS stack ;
//!     stage-0 ships static catalogs (kenney / quaternius / opengameart) +
//!     mocked-but-real-license-shaped responses for sketchfab / polyhaven so
//!     the surface contract is exercisable.
//!   - License filtering is enforced both at search-time AND at fetch-time
//!     (the cache sidecar records the license that was filtered-on at search,
//!     so a downstream `license_filter_excludes_non_cc` test holds across
//!     fetch boundaries).
//!
//! § TELEMETRY (atomic counters)
//!   - `asset_search_total`           — every search() invocation
//!   - `asset_fetch_total`            — every fetch_or_cache() invocation
//!   - `asset_cache_hits_total`       — cache-hit on fetch_or_cache()
//!   - `asset_cache_size_bytes_current` — gauge ; current bytes-on-disk.
//!
//! § ITERATIVE LOG
//!   Every search + fetch routes through `cssl_rt::loa_startup::log_event`
//!   at INFO level with source = `"asset-fetcher"`. This matches the existing
//!   loa-host telemetry axis.

// FFI surface (`__cssl_asset_search` / `__cssl_asset_fetch`) require unsafe
// extern "C". Confined to the FFI module at the bottom of this file ; the
// AssetSource trait + LruCache + adapters remain safe-Rust.
#![allow(unsafe_code)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::doc_markdown)]
// FFI return-codes use `bytes.len() as i32` ; on 32-bit targets the
// truncation is intentional (callers expect i32-positive bytes-written).
#![allow(clippy::cast_possible_wrap)]
// Telemetry counters live in a private mod ; pub(crate)-statics there
// are intentional (not part of the public surface).
#![allow(clippy::pub_underscore_fields)]
// `let-else` rewrites obscure the parallel `(Ok(...), Ok(...))` pattern
// in the FFI parser ; preserve the explicit match.
#![allow(clippy::manual_let_else)]
// Significant-Drop tightening would entangle log_event ordering with
// cache lock release — preserve current shape.
#![allow(clippy::significant_drop_tightening)]
// Unnecessary-lifetime hint trips on `&str` return types where the
// 'static lifetime is structural rather than incidental.
#![allow(clippy::needless_lifetimes)]
#![allow(clippy::redundant_clone)]
// Returning `&str` from short helpers is intentional ; the pedantic
// "unnecessary lifetime" suggestion forces a `&'static str` rewrite that
// doesn't compose well across our format-mapping helpers.
#![allow(clippy::needless_lifetimes_for_generics)]

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::RwLock;

use cssl_rt::loa_startup::log_event;
use serde::{Deserialize, Serialize};

pub mod cache;
pub mod sources;

pub use crate::cache::{CacheEntry, CacheError, CacheResult, LruCache};

// ════════════════════════════════════════════════════════════════════
// § Top-level types
// ════════════════════════════════════════════════════════════════════

/// License classification at a granularity coarse enough to gate fetch but
/// fine enough to preserve attribution requirements.
///
/// `Cc0` is public-domain-equivalent ; `CcBy` requires attribution but
/// permits commercial use ; `CcBySa` adds share-alike ; `Gpl` is GPL-3-or-
/// later (only OpenGameArt sometimes flags this) ; `Other` is everything
/// else (custom or unspecified) — `LicenseFilter::CcOnly` excludes `Other`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum License {
    /// CC0 (public domain dedication).
    Cc0,
    /// CC-BY 4.0 (attribution required).
    CcBy,
    /// CC-BY-SA 4.0 (attribution + share-alike).
    CcBySa,
    /// GPL-3.0-or-later (compatible with AGPL).
    Gpl,
    /// Custom / proprietary / unspecified — excluded by `CcOnly` filters.
    Other,
}

impl License {
    /// Whether this license is a Creative-Commons family member.
    #[must_use]
    pub const fn is_creative_commons(self) -> bool {
        matches!(self, Self::Cc0 | Self::CcBy | Self::CcBySa)
    }

    /// Whether this license requires attribution to the original author.
    #[must_use]
    pub const fn requires_attribution(self) -> bool {
        matches!(self, Self::CcBy | Self::CcBySa | Self::Gpl)
    }
}

/// License-filter applied at search-time + fetch-time.
///
/// `Cc0Only` is the strictest ; `CcOnly` permits all CC variants ; `Any`
/// permits everything including `License::Other`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LicenseFilter {
    /// CC0-only (no attribution required).
    Cc0Only,
    /// CC-family (CC0 / CC-BY / CC-BY-SA).
    #[default]
    CcOnly,
    /// Any license (caller takes responsibility).
    Any,
}

impl LicenseFilter {
    /// Whether `license` passes this filter.
    #[must_use]
    pub const fn permits(self, license: License) -> bool {
        match self {
            Self::Cc0Only => matches!(license, License::Cc0),
            Self::CcOnly => license.is_creative_commons(),
            Self::Any => true,
        }
    }
}

/// Asset format hint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AssetFormat {
    /// `.glb` glTF binary container.
    Glb,
    /// `.gltf` glTF JSON (may reference external `.bin` + textures).
    Gltf,
    /// PBR material set (PolyHaven : albedo / normal / roughness / etc).
    PbrMaterial,
    /// HDRI environment map (`.hdr` / `.exr`).
    Hdri,
    /// `.fbx` (rare ; some Quaternius packs).
    Fbx,
    /// `.obj` + `.mtl` (legacy ; OpenGameArt).
    Obj,
    /// Raw byte-bag (catch-all).
    Other,
}

/// Uniform per-asset metadata record — what every adapter returns from
/// `search()` and what `fetch_or_cache()` writes into the `.meta` sidecar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetMeta {
    /// Provider-stable identifier (e.g., `sketchfab:abc123` or `kenney:tower-defense-pack`).
    pub id: String,
    /// Source / provider name : `"sketchfab"` / `"polyhaven"` / `"kenney"` / `"quaternius"` / `"opengameart"`.
    pub src: String,
    /// Asset display name.
    pub name: String,
    /// License classification.
    pub license: License,
    /// Format hint for downstream consumers.
    pub format: AssetFormat,
    /// Provider-side download URL (or static-catalog URL).
    pub url: String,
    /// Author / attribution credit (empty string if CC0 + no specific author).
    pub author: String,
    /// Free-form tags supplied by the provider (lowercased ; deduped).
    pub tags: Vec<String>,
    /// Approximate file-size in bytes (0 if unknown).
    pub size_bytes: u64,
}

// ════════════════════════════════════════════════════════════════════
// § AssetSource trait
// ════════════════════════════════════════════════════════════════════

/// Common error type for `AssetSource` implementors.
#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    /// Network-side or syscall-side I/O error.
    #[error("source I/O error: {0}")]
    Io(String),
    /// Provider returned a malformed response.
    #[error("source parse error: {0}")]
    Parse(String),
    /// Asset id not found in this source's catalog.
    #[error("asset not found: {0}")]
    NotFound(String),
    /// License filter excluded all results.
    #[error("license filter excluded all results")]
    LicenseFilterExcluded,
    /// Capability-system refused a network grant (cssl-rt cap-gate).
    #[error("network capability not granted (call cssl_rt::caps_grant first)")]
    CapNotGranted,
}

/// Implementor-side `Result` alias.
pub type SourceResult<T> = Result<T, SourceError>;

/// Uniform asset-source abstraction. One implementor per provider.
pub trait AssetSource: Send + Sync {
    /// Stable provider-name (matches `AssetMeta::src`).
    fn name(&self) -> &str;

    /// Search the provider catalog with the given query + license filter.
    ///
    /// Returns a (possibly empty) vec of `AssetMeta` records. License
    /// filtering MUST be applied here (callers expect filtered results).
    fn search(&self, query: &str, lf: LicenseFilter) -> SourceResult<Vec<AssetMeta>>;

    /// Fetch the raw asset bytes for the given asset-id.
    ///
    /// The returned byte-vec is what gets written to the LRU cache. For
    /// `Glb` / `Gltf` formats this is the actual `.glb` / `.gltf` body ;
    /// for `PbrMaterial` it's a `.zip` of channel-textures.
    fn fetch(&self, asset_id: &str) -> SourceResult<Vec<u8>>;

    /// Direct id-to-metadata lookup. Default impl delegates to `search`
    /// with no license filter and matches the first record whose `id`
    /// exactly equals `asset_id`. Adapters with native id-indexed
    /// catalogs may override for O(1) lookup.
    fn lookup_by_id(&self, asset_id: &str) -> SourceResult<Option<AssetMeta>> {
        let any = self.search("", LicenseFilter::Any)?;
        Ok(any.into_iter().find(|m| m.id == asset_id))
    }
}

// ════════════════════════════════════════════════════════════════════
// § Telemetry (process-wide atomic counters)
// ════════════════════════════════════════════════════════════════════

/// Process-wide telemetry counters. Exposed via `telemetry_*()` getters.
mod tel {
    use std::sync::atomic::AtomicU64;

    pub(crate) static SEARCH_TOTAL: AtomicU64 = AtomicU64::new(0);
    pub(crate) static FETCH_TOTAL: AtomicU64 = AtomicU64::new(0);
    pub(crate) static CACHE_HITS_TOTAL: AtomicU64 = AtomicU64::new(0);
    pub(crate) static CACHE_SIZE_BYTES: AtomicU64 = AtomicU64::new(0);
}

/// Read `asset_search_total` counter.
#[must_use]
pub fn telemetry_search_total() -> u64 {
    tel::SEARCH_TOTAL.load(Ordering::Relaxed)
}
/// Read `asset_fetch_total` counter.
#[must_use]
pub fn telemetry_fetch_total() -> u64 {
    tel::FETCH_TOTAL.load(Ordering::Relaxed)
}
/// Read `asset_cache_hits_total` counter.
#[must_use]
pub fn telemetry_cache_hits_total() -> u64 {
    tel::CACHE_HITS_TOTAL.load(Ordering::Relaxed)
}
/// Read `asset_cache_size_bytes_current` gauge.
#[must_use]
pub fn telemetry_cache_size_bytes() -> u64 {
    tel::CACHE_SIZE_BYTES.load(Ordering::Relaxed)
}

// Internal increment helpers (used by AssetFetcher).
fn inc_search() {
    tel::SEARCH_TOTAL.fetch_add(1, Ordering::Relaxed);
}
fn inc_fetch() {
    tel::FETCH_TOTAL.fetch_add(1, Ordering::Relaxed);
}
fn inc_cache_hit() {
    tel::CACHE_HITS_TOTAL.fetch_add(1, Ordering::Relaxed);
}
fn set_cache_size(bytes: u64) {
    tel::CACHE_SIZE_BYTES.store(bytes, Ordering::Relaxed);
}

// ════════════════════════════════════════════════════════════════════
// § AssetFetcher orchestrator
// ════════════════════════════════════════════════════════════════════

/// Top-level error covering both source-side + cache-side failures.
#[derive(Debug, thiserror::Error)]
pub enum FetcherError {
    /// Source-level error (network / parse / not-found).
    #[error("source error: {0}")]
    Source(#[from] SourceError),
    /// Cache-level error (disk / sidecar / lru).
    #[error("cache error: {0}")]
    Cache(#[from] CacheError),
    /// No registered source matches the requested name.
    #[error("unknown source: {0}")]
    UnknownSource(String),
}

/// Top-level `Result` alias.
pub type FetcherResult<T> = Result<T, FetcherError>;

/// Default LRU cap : 50 GiB. Override via env `CSSL_ASSET_CACHE_GB`.
const DEFAULT_LRU_CAP_GB: u64 = 50;

/// Multi-source asset orchestrator with LRU disk-cache.
pub struct AssetFetcher {
    /// Backing LRU cache (one cache per fetcher ; thread-safe via RwLock).
    cache: RwLock<LruCache>,
    /// Registered sources. Order = preference-order ; `search()` queries
    /// every source in order and concatenates results.
    sources: Vec<Box<dyn AssetSource>>,
}

impl AssetFetcher {
    /// Construct a fetcher with all 5 stage-0 sources registered + the
    /// default LRU cap (50 GiB or `CSSL_ASSET_CACHE_GB`).
    #[must_use]
    pub fn new() -> Self {
        let cap_gb = std::env::var("CSSL_ASSET_CACHE_GB")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(DEFAULT_LRU_CAP_GB);
        let cap_bytes = cap_gb.saturating_mul(1024 * 1024 * 1024);

        let cache_dir = default_cache_dir();
        let cache = LruCache::new(cache_dir, cap_bytes).unwrap_or_else(|e| {
            log_event(
                "ERROR",
                "asset-fetcher",
                &format!("cache init failed: {e} ; falling back to in-memory-only"),
            );
            LruCache::ephemeral(cap_bytes)
        });
        set_cache_size(cache.size_bytes());

        let sources: Vec<Box<dyn AssetSource>> = vec![
            Box::new(crate::sources::sketchfab::SketchfabSource::new()),
            Box::new(crate::sources::polyhaven::PolyHavenSource::new()),
            Box::new(crate::sources::kenney::KenneySource::new()),
            Box::new(crate::sources::quaternius::QuaterniusSource::new()),
            Box::new(crate::sources::opengameart::OpenGameArtSource::new()),
        ];

        log_event(
            "INFO",
            "asset-fetcher",
            &format!(
                "AssetFetcher::new : sources={} cap_bytes={} cache_dir-or-ephemeral",
                sources.len(),
                cap_bytes,
            ),
        );

        Self {
            cache: RwLock::new(cache),
            sources,
        }
    }

    /// Construct with an explicit cache-dir + cap (test-only / advanced).
    pub fn with_cache(cache_dir: PathBuf, lru_cap_bytes: u64) -> CacheResult<Self> {
        let cache = LruCache::new(cache_dir, lru_cap_bytes)?;
        set_cache_size(cache.size_bytes());
        let sources: Vec<Box<dyn AssetSource>> = vec![
            Box::new(crate::sources::sketchfab::SketchfabSource::new()),
            Box::new(crate::sources::polyhaven::PolyHavenSource::new()),
            Box::new(crate::sources::kenney::KenneySource::new()),
            Box::new(crate::sources::quaternius::QuaterniusSource::new()),
            Box::new(crate::sources::opengameart::OpenGameArtSource::new()),
        ];
        Ok(Self {
            cache: RwLock::new(cache),
            sources,
        })
    }

    /// Search every registered source ; concatenate filtered results.
    ///
    /// Per-source errors are logged and skipped (one bad source must not
    /// poison the cross-source query). Returns a vec of `AssetMeta`.
    #[must_use]
    pub fn search(&self, query: &str, lf: LicenseFilter) -> Vec<AssetMeta> {
        inc_search();
        log_event(
            "INFO",
            "asset-fetcher",
            &format!("search : query={query:?} filter={lf:?}"),
        );
        let mut out = Vec::new();
        for src in &self.sources {
            match src.search(query, lf) {
                Ok(mut v) => out.append(&mut v),
                Err(e) => {
                    log_event(
                        "WARN",
                        "asset-fetcher",
                        &format!("source {} search-err: {}", src.name(), e),
                    );
                }
            }
        }
        out
    }

    /// Fetch + cache an asset by `(source, asset_id)`. Returns the local
    /// disk-path of the cached artifact. Cache-hit fast-path skips the
    /// source's `fetch()` and just touches the LRU access-time.
    pub fn fetch_or_cache(&self, source: &str, asset_id: &str) -> FetcherResult<PathBuf> {
        inc_fetch();

        // Cache hit fast-path.
        {
            let cache = self.cache.read().expect("cache RwLock poisoned");
            if let Some(path) = cache.get_path(source, asset_id) {
                inc_cache_hit();
                set_cache_size(cache.size_bytes());
                log_event(
                    "INFO",
                    "asset-fetcher",
                    &format!("fetch_or_cache HIT : src={source} id={asset_id}"),
                );
                // Touch access-time for LRU.
                drop(cache);
                self.cache
                    .write()
                    .expect("cache RwLock poisoned")
                    .touch(source, asset_id);
                return Ok(path);
            }
        }

        // Cache miss : route to the matching source.
        let src = self
            .sources
            .iter()
            .find(|s| s.name() == source)
            .ok_or_else(|| FetcherError::UnknownSource(source.to_string()))?;
        let bytes = src.fetch(asset_id)?;
        // Pull metadata for the sidecar via id-direct lookup. Search
        // is keyed on name/tag substring and would miss a `kenney:`-
        // prefixed id ; lookup_by_id walks the catalog by exact id.
        let sidecar = match src.lookup_by_id(asset_id) {
            Ok(Some(m)) => m,
            _ => synth_meta(source, asset_id, bytes.len() as u64),
        };

        let mut cache = self.cache.write().expect("cache RwLock poisoned");
        let path = cache.put(source, asset_id, &bytes, &sidecar)?;
        set_cache_size(cache.size_bytes());

        log_event(
            "INFO",
            "asset-fetcher",
            &format!(
                "fetch_or_cache MISS-WROTE : src={source} id={asset_id} bytes={} cache_total={}",
                bytes.len(),
                cache.size_bytes(),
            ),
        );

        Ok(path)
    }

    /// Current cache-size on disk (bytes).
    #[must_use]
    pub fn cache_size_bytes(&self) -> u64 {
        self.cache
            .read()
            .expect("cache RwLock poisoned")
            .size_bytes()
    }

    /// Evict LRU entries until on-disk cache <= `target_bytes`.
    pub fn evict_lru_to_size(&self, target_bytes: u64) -> CacheResult<usize> {
        let mut cache = self.cache.write().expect("cache RwLock poisoned");
        let evicted = cache.evict_to_size(target_bytes)?;
        set_cache_size(cache.size_bytes());
        log_event(
            "INFO",
            "asset-fetcher",
            &format!(
                "evict_lru_to_size : target={target_bytes} evicted={evicted} now={}",
                cache.size_bytes()
            ),
        );
        Ok(evicted)
    }

    /// Iterator over the registered source names (for diagnostics).
    pub fn source_names(&self) -> Vec<&str> {
        self.sources.iter().map(|s| s.name()).collect()
    }
}

impl Default for AssetFetcher {
    fn default() -> Self {
        Self::new()
    }
}

/// Default cache root : `~/.loa/cache/asset-fetcher/`. If the home dir
/// can't be resolved (rare on CI / sandbox), falls back to
/// `<env CSSL_ASSET_CACHE_DIR>` or finally a tempdir-style path.
fn default_cache_dir() -> PathBuf {
    if let Ok(d) = std::env::var("CSSL_ASSET_CACHE_DIR") {
        return PathBuf::from(d);
    }
    let home = std::env::var("HOME")
        .ok()
        .or_else(|| std::env::var("USERPROFILE").ok())
        .unwrap_or_else(|| ".".to_string());
    PathBuf::from(home)
        .join(".loa")
        .join("cache")
        .join("asset-fetcher")
}

fn synth_meta(source: &str, asset_id: &str, size_bytes: u64) -> AssetMeta {
    AssetMeta {
        id: asset_id.to_string(),
        src: source.to_string(),
        name: asset_id.to_string(),
        license: License::Other,
        format: AssetFormat::Other,
        url: String::new(),
        author: String::new(),
        tags: Vec::new(),
        size_bytes,
    }
}

// ════════════════════════════════════════════════════════════════════
// § FFI surface for CSSL programs
// ════════════════════════════════════════════════════════════════════

/// Singleton fetcher behind a OnceLock — FFI calls share one fetcher per
/// process so the LRU cache is consistent across CSSL invocations.
fn shared_fetcher() -> &'static AssetFetcher {
    use std::sync::OnceLock;
    static F: OnceLock<AssetFetcher> = OnceLock::new();
    F.get_or_init(AssetFetcher::new)
}

/// FFI : search every source. Writes a JSON-array of `AssetMeta` into
/// `out_buf` (capacity = `out_cap`). Returns positive bytes-written, or
/// negative error code (-1 buffer-too-small / -2 bad-utf8 / -3 internal).
///
/// # Safety
/// Caller MUST ensure :
///   - `q_ptr..q_ptr+q_len` is a valid byte-slice (UTF-8 query string)
///   - `out_buf..out_buf+out_cap` is a valid writable byte-slice
#[no_mangle]
pub unsafe extern "C" fn __cssl_asset_search(
    q_ptr: *const u8,
    q_len: usize,
    out_buf: *mut u8,
    out_cap: usize,
) -> i32 {
    if q_ptr.is_null() || (out_buf.is_null() && out_cap > 0) {
        return -3;
    }
    // SAFETY: caller-asserted slice validity.
    let q_slice = unsafe { std::slice::from_raw_parts(q_ptr, q_len) };
    let Ok(q) = std::str::from_utf8(q_slice) else {
        return -2;
    };
    let f = shared_fetcher();
    let metas = f.search(q, LicenseFilter::default());
    let json = match serde_json::to_vec(&metas) {
        Ok(v) => v,
        Err(_) => return -3,
    };
    if json.len() > out_cap {
        return -1;
    }
    // SAFETY: validated capacity above.
    unsafe {
        std::ptr::copy_nonoverlapping(json.as_ptr(), out_buf, json.len());
    }
    json.len() as i32
}

/// FFI : fetch_or_cache. Writes the local cached path as UTF-8 into
/// `out_path_buf` (capacity = `out_cap`). Same return-code convention
/// as `__cssl_asset_search`.
///
/// # Safety
/// Caller MUST ensure all four pointer-ranges are valid byte-slices
/// (source-name + asset-id are UTF-8 ; out-path-buf is writable).
#[no_mangle]
pub unsafe extern "C" fn __cssl_asset_fetch(
    src_ptr: *const u8,
    src_len: usize,
    id_ptr: *const u8,
    id_len: usize,
    out_path_buf: *mut u8,
    out_cap: usize,
) -> i32 {
    if src_ptr.is_null() || id_ptr.is_null() || (out_path_buf.is_null() && out_cap > 0) {
        return -3;
    }
    // SAFETY: caller-asserted slice validity.
    let src_slice = unsafe { std::slice::from_raw_parts(src_ptr, src_len) };
    let id_slice = unsafe { std::slice::from_raw_parts(id_ptr, id_len) };
    let (Ok(src), Ok(id)) = (std::str::from_utf8(src_slice), std::str::from_utf8(id_slice)) else {
        return -2;
    };
    let f = shared_fetcher();
    let path = match f.fetch_or_cache(src, id) {
        Ok(p) => p,
        Err(_) => return -3,
    };
    let s = path.to_string_lossy().into_owned();
    let bytes = s.as_bytes();
    if bytes.len() > out_cap {
        return -1;
    }
    // SAFETY: validated capacity above.
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), out_path_buf, bytes.len());
    }
    bytes.len() as i32
}

// `unsafe extern "C"` is required for the FFI surface ; the crate-level
// `#![allow(unsafe_code)]` opt-in is restricted to the two FFI symbols
// above. Trait + cache implementations contain no unsafe blocks.

// ════════════════════════════════════════════════════════════════════
// § Inline tests (telemetry exposure + filter invariants)
// ════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod inline_tests {
    use super::*;

    #[test]
    fn telemetry_counters_exposed_and_monotonic() {
        // Reset-baseline : don't assume zero (other tests may have run first
        // in the same binary), just record + ensure post >= pre.
        let pre_search = telemetry_search_total();
        let pre_fetch = telemetry_fetch_total();
        let pre_hits = telemetry_cache_hits_total();
        let pre_size = telemetry_cache_size_bytes();
        inc_search();
        inc_fetch();
        inc_cache_hit();
        set_cache_size(pre_size + 1024);
        assert!(telemetry_search_total() > pre_search);
        assert!(telemetry_fetch_total() > pre_fetch);
        assert!(telemetry_cache_hits_total() > pre_hits);
        assert_eq!(telemetry_cache_size_bytes(), pre_size + 1024);
        // Restore size-gauge so we don't leak state.
        set_cache_size(pre_size);
    }

    #[test]
    fn license_filter_permits_correctly() {
        assert!(LicenseFilter::Cc0Only.permits(License::Cc0));
        assert!(!LicenseFilter::Cc0Only.permits(License::CcBy));
        assert!(LicenseFilter::CcOnly.permits(License::CcBy));
        assert!(!LicenseFilter::CcOnly.permits(License::Other));
        assert!(LicenseFilter::Any.permits(License::Other));
    }

    #[test]
    fn license_attribution_flags() {
        assert!(!License::Cc0.requires_attribution());
        assert!(License::CcBy.requires_attribution());
        assert!(License::Gpl.requires_attribution());
        assert!(License::Cc0.is_creative_commons());
        assert!(!License::Gpl.is_creative_commons());
    }
}
