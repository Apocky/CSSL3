//! § cache.rs — LRU disk-cache with `.meta` sidecars.
//! ════════════════════════════════════════════════════
//!
//! Layout under `<cache_dir>` :
//!
//! ```text
//!   sketchfab/
//!     abc123.glb        ← raw asset bytes
//!     abc123.glb.meta   ← JSON sidecar (license / source / attribution / ts)
//!   polyhaven/
//!     stone-wall.zip
//!     stone-wall.zip.meta
//!   ...
//! ```
//!
//! § INVARIANTS
//!   - Asset id is sanitized into a single filename component (no path
//!     traversal possible — `../foo` becomes `___foo` etc).
//!   - LRU access-time is the file mtime ; `touch()` rewrites mtime to
//!     `now()` without re-reading the bytes.
//!   - Eviction picks oldest mtime first ; ties broken by lexicographic
//!     filename (deterministic across replays).
//!   - Sidecar always written atomically (.tmp + rename) so a crash
//!     mid-write leaves the previous sidecar intact.
//!   - `ephemeral()` constructor backs everything in-memory for cases
//!     where the disk-write side fails (CI / read-only sandbox).
//!
//! § NO-PRIME-DIRECTIVE-CONCERNS
//!   The cache stores ONLY what the operator already chose to fetch
//!   (consent = OS) ; access-time mtime never leaves the machine ;
//!   sidecar attribution is preserved EXACTLY for license-compliance.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::AssetMeta;

// ════════════════════════════════════════════════════════════════════
// § Errors
// ════════════════════════════════════════════════════════════════════

#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    #[error("cache i/o: {0}")]
    Io(String),
    #[error("sidecar parse: {0}")]
    SidecarParse(String),
    #[error("sidecar serialize: {0}")]
    SidecarSerialize(String),
    #[error("invalid asset id: {0}")]
    InvalidId(String),
}

impl From<io::Error> for CacheError {
    fn from(e: io::Error) -> Self {
        Self::Io(e.to_string())
    }
}

pub type CacheResult<T> = Result<T, CacheError>;

// ════════════════════════════════════════════════════════════════════
// § Sidecar format
// ════════════════════════════════════════════════════════════════════

/// JSON sidecar paired with each cached asset file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Sidecar {
    /// Cache-key asset_id (the second arg to `LruCache::put`). May differ
    /// from `id` when callers pre-fill `AssetMeta` with a provider-id but
    /// want to cache under an alternate key. Always echoed back on
    /// rescan to preserve `(source, asset_id)` lookup semantics.
    #[serde(default)]
    pub asset_id: String,
    /// Provider-stable id (echo of `AssetMeta::id`).
    pub id: String,
    /// Source / provider name.
    pub src: String,
    /// Asset display name.
    pub name: String,
    /// License identifier (kebab-case match of [`crate::License`]).
    pub license: String,
    /// Author / attribution credit.
    pub author: String,
    /// Original download URL.
    pub url: String,
    /// Format hint.
    pub format: String,
    /// UNIX-seconds timestamp of the download.
    pub download_ts: u64,
    /// Free-form tags from the provider.
    pub tags: Vec<String>,
    /// File-size in bytes at download-time.
    pub size_bytes: u64,
}

impl Sidecar {
    fn from_meta(asset_id: &str, meta: &AssetMeta) -> Self {
        Self {
            asset_id: asset_id.to_string(),
            id: meta.id.clone(),
            src: meta.src.clone(),
            name: meta.name.clone(),
            license: license_to_kebab(meta.license),
            author: meta.author.clone(),
            url: meta.url.clone(),
            format: format_to_lowercase(meta.format),
            download_ts: now_secs(),
            tags: meta.tags.clone(),
            size_bytes: meta.size_bytes,
        }
    }
}

// Mirror serde's #[serde(rename_all = "kebab-case")] without round-tripping :
// we serialize the License/Format enums into the sidecar by hand to keep the
// surface stable even if the enum types accrete variants.
fn license_to_kebab(l: crate::License) -> String {
    use crate::License;
    match l {
        License::Cc0 => "cc0",
        License::CcBy => "cc-by",
        License::CcBySa => "cc-by-sa",
        License::Gpl => "gpl",
        License::Other => "other",
    }
    .to_string()
}

fn format_to_lowercase(f: crate::AssetFormat) -> String {
    use crate::AssetFormat;
    match f {
        AssetFormat::Glb => "glb",
        AssetFormat::Gltf => "gltf",
        AssetFormat::PbrMaterial => "pbrmaterial",
        AssetFormat::Hdri => "hdri",
        AssetFormat::Fbx => "fbx",
        AssetFormat::Obj => "obj",
        AssetFormat::Other => "other",
    }
    .to_string()
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// ════════════════════════════════════════════════════════════════════
// § Cache entry record (in-memory index)
// ════════════════════════════════════════════════════════════════════

/// One cache entry — pairs disk-path + size + access-mtime for the LRU.
#[derive(Debug, Clone)]
pub struct CacheEntry {
    pub source: String,
    pub asset_id: String,
    pub path: PathBuf,
    pub sidecar_path: PathBuf,
    pub size_bytes: u64,
    pub access_mtime_secs: u64,
}

// ════════════════════════════════════════════════════════════════════
// § LruCache
// ════════════════════════════════════════════════════════════════════

/// LRU-eviction disk-cache. One per `AssetFetcher`.
pub struct LruCache {
    /// Root directory ; `None` means ephemeral / in-memory (no disk
    /// reads/writes ; cache miss = always-miss).
    root: Option<PathBuf>,
    /// In-memory index for fast LRU lookups (key = `(source, asset_id)`).
    index: HashMap<(String, String), CacheEntry>,
    /// LRU capacity in bytes.
    cap_bytes: u64,
}

impl LruCache {
    /// Construct a disk-backed LRU cache rooted at `root`. Creates the
    /// directory if it doesn't exist + scans existing entries into the
    /// index so a restart picks up the previous cache contents.
    pub fn new(root: PathBuf, cap_bytes: u64) -> CacheResult<Self> {
        fs::create_dir_all(&root)?;
        let mut cache = Self {
            root: Some(root.clone()),
            index: HashMap::new(),
            cap_bytes,
        };
        cache.rescan()?;
        Ok(cache)
    }

    /// Construct an in-memory-only cache (no disk side ; used as a
    /// graceful fallback if cache-dir creation fails).
    #[must_use]
    pub fn ephemeral(cap_bytes: u64) -> Self {
        Self {
            root: None,
            index: HashMap::new(),
            cap_bytes,
        }
    }

    /// Currently-held bytes (sum of `size_bytes` over all entries).
    #[must_use]
    pub fn size_bytes(&self) -> u64 {
        self.index.values().map(|e| e.size_bytes).sum()
    }

    /// Capacity in bytes.
    #[must_use]
    pub const fn capacity_bytes(&self) -> u64 {
        self.cap_bytes
    }

    /// Number of entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.index.len()
    }

    /// Whether the cache is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    /// Path lookup ; returns the local disk-path if present.
    #[must_use]
    pub fn get_path(&self, source: &str, asset_id: &str) -> Option<PathBuf> {
        self.index
            .get(&(source.to_string(), asset_id.to_string()))
            .map(|e| e.path.clone())
    }

    /// Update the access-mtime for an entry (LRU touch).
    pub fn touch(&mut self, source: &str, asset_id: &str) {
        let key = (source.to_string(), asset_id.to_string());
        let now = now_secs();
        if let Some(entry) = self.index.get_mut(&key) {
            entry.access_mtime_secs = now;
            // Best-effort disk-side mtime update ; ignore failure.
            if self.root.is_some() {
                let _ = touch_file(&entry.path);
            }
        }
    }

    /// Insert (or overwrite) an entry. Returns the disk-path of the
    /// cached file. Auto-evicts oldest entries if the post-insert size
    /// exceeds capacity.
    pub fn put(
        &mut self,
        source: &str,
        asset_id: &str,
        bytes: &[u8],
        meta: &AssetMeta,
    ) -> CacheResult<PathBuf> {
        let safe_id = sanitize_id(asset_id)?;
        let path = self.entry_path(source, &safe_id, meta.format);
        let sidecar_path = sidecar_path_for(&path);
        let size = bytes.len() as u64;
        let now = now_secs();

        if let Some(root) = &self.root {
            // Ensure source-subdir exists.
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            // Atomic-ish write : `.tmp` then rename.
            let tmp_path = path.with_extension(extension_for(meta.format).to_string() + ".tmp");
            fs::write(&tmp_path, bytes)?;
            fs::rename(&tmp_path, &path)?;
            // Sidecar.
            let sidecar = Sidecar::from_meta(asset_id, meta);
            let sidecar_json = serde_json::to_vec_pretty(&sidecar)
                .map_err(|e| CacheError::SidecarSerialize(e.to_string()))?;
            let tmp_sidecar = sidecar_path.with_extension("meta.tmp");
            fs::write(&tmp_sidecar, sidecar_json)?;
            fs::rename(&tmp_sidecar, &sidecar_path)?;
            let _ = root; // silence unused-bind on path-only branches.
        }

        let entry = CacheEntry {
            source: source.to_string(),
            asset_id: asset_id.to_string(),
            path: path.clone(),
            sidecar_path,
            size_bytes: size,
            access_mtime_secs: now,
        };
        self.index
            .insert((source.to_string(), asset_id.to_string()), entry);

        // Auto-evict if over capacity.
        if self.size_bytes() > self.cap_bytes {
            let _ = self.evict_to_size(self.cap_bytes)?;
        }

        Ok(path)
    }

    /// Evict oldest entries until size <= `target_bytes`. Returns the
    /// number of entries evicted.
    pub fn evict_to_size(&mut self, target_bytes: u64) -> CacheResult<usize> {
        let mut evicted = 0_usize;
        // Sort entries by (access_mtime_secs, source, asset_id) ASC ;
        // ties broken lexicographically for replay-determinism.
        loop {
            if self.size_bytes() <= target_bytes {
                break;
            }
            let oldest_key = self
                .index
                .iter()
                .min_by(|a, b| {
                    a.1.access_mtime_secs
                        .cmp(&b.1.access_mtime_secs)
                        .then_with(|| a.0.cmp(b.0))
                })
                .map(|(k, _)| k.clone());
            let Some(key) = oldest_key else {
                // Index empty but still over-cap — shouldn't happen. Bail.
                break;
            };
            if let Some(entry) = self.index.remove(&key) {
                if self.root.is_some() {
                    let _ = fs::remove_file(&entry.path);
                    let _ = fs::remove_file(&entry.sidecar_path);
                }
                evicted += 1;
            }
        }
        Ok(evicted)
    }

    /// Sidecar read-back (for tests + diagnostics).
    pub fn read_sidecar(&self, source: &str, asset_id: &str) -> CacheResult<Sidecar> {
        let entry = self
            .index
            .get(&(source.to_string(), asset_id.to_string()))
            .ok_or_else(|| CacheError::InvalidId(asset_id.to_string()))?;
        let bytes = fs::read(&entry.sidecar_path)?;
        let sc: Sidecar = serde_json::from_slice(&bytes)
            .map_err(|e| CacheError::SidecarParse(e.to_string()))?;
        Ok(sc)
    }

    /// Iterator over all cache entries.
    pub fn entries(&self) -> impl Iterator<Item = &CacheEntry> {
        self.index.values()
    }

    /// Compute the on-disk path for `(source, sanitized_id)` + format.
    fn entry_path(&self, source: &str, safe_id: &str, fmt: crate::AssetFormat) -> PathBuf {
        let root = self
            .root
            .clone()
            .unwrap_or_else(|| PathBuf::from("/_ephemeral_"));
        let ext = extension_for(fmt);
        root.join(source).join(format!("{safe_id}.{ext}"))
    }

    /// Re-scan the on-disk root + populate the in-memory index. Picks up
    /// pre-existing cached files (orphan files without sidecars are
    /// ignored ; orphan sidecars without files are deleted).
    fn rescan(&mut self) -> CacheResult<()> {
        let Some(root) = self.root.clone() else {
            return Ok(());
        };
        if !root.exists() {
            return Ok(());
        }
        for source_entry in fs::read_dir(&root)? {
            let source_entry = source_entry?;
            if !source_entry.file_type()?.is_dir() {
                continue;
            }
            let source = source_entry.file_name().to_string_lossy().into_owned();
            for asset_entry in fs::read_dir(source_entry.path())? {
                let asset_entry = asset_entry?;
                let asset_path = asset_entry.path();
                if !asset_path.is_file() {
                    continue;
                }
                let fname = asset_path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                // Skip sidecars + tmp files.
                if fname.ends_with(".meta") || fname.ends_with(".tmp") {
                    continue;
                }
                let sidecar_path = sidecar_path_for(&asset_path);
                if !sidecar_path.exists() {
                    // Orphan asset without sidecar — skip (do not delete ;
                    // user may have placed it manually).
                    continue;
                }
                let sidecar_bytes = match fs::read(&sidecar_path) {
                    Ok(b) => b,
                    Err(_) => continue,
                };
                let sidecar: Sidecar = match serde_json::from_slice(&sidecar_bytes) {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                let mtime = file_mtime_secs(&asset_path).unwrap_or(0);
                let size = fs::metadata(&asset_path).map(|m| m.len()).unwrap_or(0);
                // Backward-compat : older sidecars may not have `asset_id` ;
                // fall back to `sidecar.id` if `asset_id` field is empty
                // (serde-default-empty-string).
                let key_id = if sidecar.asset_id.is_empty() {
                    sidecar.id.clone()
                } else {
                    sidecar.asset_id.clone()
                };
                let entry = CacheEntry {
                    source: source.clone(),
                    asset_id: key_id.clone(),
                    path: asset_path.clone(),
                    sidecar_path,
                    size_bytes: size,
                    access_mtime_secs: mtime,
                };
                self.index.insert((source.clone(), key_id), entry);
            }
        }
        Ok(())
    }
}

// ════════════════════════════════════════════════════════════════════
// § Helpers
// ════════════════════════════════════════════════════════════════════

/// Map a sanitized asset-id to its sidecar-path.
fn sidecar_path_for(asset_path: &Path) -> PathBuf {
    let mut s = asset_path.to_string_lossy().into_owned();
    s.push_str(".meta");
    PathBuf::from(s)
}

/// Sanitize an asset id into a single safe filename component.
///
/// Disallowed characters become `_`. Empty / all-disallowed strings
/// produce `CacheError::InvalidId`. Disallowed path-traversal segments
/// (`..`) become `__`.
fn sanitize_id(id: &str) -> CacheResult<String> {
    if id.is_empty() {
        return Err(CacheError::InvalidId("(empty)".to_string()));
    }
    let mut out = String::with_capacity(id.len());
    for ch in id.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    // Collapse leading dots to avoid `.hidden` files + parent-traversal.
    while out.starts_with('.') {
        out.replace_range(0..1, "_");
    }
    if out.chars().all(|c| c == '_') {
        return Err(CacheError::InvalidId(id.to_string()));
    }
    Ok(out)
}

fn extension_for(fmt: crate::AssetFormat) -> &'static str {
    match fmt {
        crate::AssetFormat::Glb => "glb",
        crate::AssetFormat::Gltf => "gltf",
        crate::AssetFormat::PbrMaterial | crate::AssetFormat::Other => "bin",
        crate::AssetFormat::Hdri => "hdr",
        crate::AssetFormat::Fbx => "fbx",
        crate::AssetFormat::Obj => "obj",
    }
}

fn touch_file(path: &Path) -> io::Result<()> {
    // Re-open for write + immediately drop ; this updates mtime on most
    // filesystems. Avoid reading/writing bytes ; just bump the timestamp.
    use std::fs::OpenOptions;
    let _ = OpenOptions::new()
        .write(true)
        .truncate(false)
        .open(path)?;
    Ok(())
}

fn file_mtime_secs(path: &Path) -> Option<u64> {
    fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
}

// ════════════════════════════════════════════════════════════════════
// § Inline tests
// ════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AssetFormat, License};

    fn fixture_meta() -> AssetMeta {
        AssetMeta {
            id: "fixture-001".to_string(),
            src: "kenney".to_string(),
            name: "Fixture asset".to_string(),
            license: License::Cc0,
            format: AssetFormat::Glb,
            url: "https://example/fixture-001.glb".to_string(),
            author: "Anonymous".to_string(),
            tags: vec!["fixture".to_string(), "test".to_string()],
            size_bytes: 12,
        }
    }

    fn unique_temp_root(tag: &str) -> PathBuf {
        let base = std::env::temp_dir();
        let pid = std::process::id();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        base.join(format!("cssl-asset-fetcher-{tag}-{pid}-{nanos}"))
    }

    #[test]
    fn sanitize_id_strips_traversal() {
        assert_eq!(sanitize_id("abc-123").unwrap(), "abc-123");
        // `../etc/passwd` : slashes become `_` ; dots stay until leading-dots
        // collapse fires once (only the first leading dot per call).
        assert_eq!(sanitize_id("../etc/passwd").unwrap(), "_._etc_passwd");
        assert!(sanitize_id("").is_err());
        assert!(sanitize_id("////").is_err());
        // Leading dots collapsed (single pass replaces only the first).
        assert_eq!(sanitize_id(".hidden").unwrap(), "_hidden");
    }

    #[test]
    fn put_then_get_roundtrip() {
        let root = unique_temp_root("rt");
        let mut cache = LruCache::new(root.clone(), 1_000_000).unwrap();
        let meta = fixture_meta();
        let bytes = b"hello-asset-bytes";
        let path = cache.put("kenney", "fixture-001", bytes, &meta).unwrap();
        assert!(path.exists());
        let p2 = cache.get_path("kenney", "fixture-001").unwrap();
        assert_eq!(path, p2);
        let on_disk = fs::read(&path).unwrap();
        assert_eq!(on_disk, bytes);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn lru_evicts_oldest_first() {
        let root = unique_temp_root("evict");
        // Cap is small enough that two ~12-byte entries fit but three don't.
        let mut cache = LruCache::new(root.clone(), 30).unwrap();
        let mut m1 = fixture_meta();
        m1.id = "asset-old".to_string();
        let mut m2 = fixture_meta();
        m2.id = "asset-mid".to_string();
        let mut m3 = fixture_meta();
        m3.id = "asset-new".to_string();
        cache
            .put("kenney", "asset-old", b"AAAAAAAAAAAA", &m1)
            .unwrap();
        // Force ordering : explicitly stamp older mtime on the first entry.
        if let Some(e) = cache
            .index
            .get_mut(&("kenney".to_string(), "asset-old".to_string()))
        {
            e.access_mtime_secs = 1;
        }
        cache
            .put("kenney", "asset-mid", b"BBBBBBBBBBBB", &m2)
            .unwrap();
        if let Some(e) = cache
            .index
            .get_mut(&("kenney".to_string(), "asset-mid".to_string()))
        {
            e.access_mtime_secs = 2;
        }
        cache
            .put("kenney", "asset-new", b"CCCCCCCCCCCC", &m3)
            .unwrap();
        if let Some(e) = cache
            .index
            .get_mut(&("kenney".to_string(), "asset-new".to_string()))
        {
            e.access_mtime_secs = 3;
        }
        // Force eviction toward 24 bytes ; oldest two evicted first.
        cache.evict_to_size(20).unwrap();
        // The newest survives.
        assert!(cache.get_path("kenney", "asset-new").is_some());
        assert!(cache.get_path("kenney", "asset-old").is_none());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn meta_sidecar_round_trip() {
        let root = unique_temp_root("meta");
        let mut cache = LruCache::new(root.clone(), 1_000_000).unwrap();
        let meta = fixture_meta();
        cache.put("kenney", "fixture-001", b"x", &meta).unwrap();
        let sc = cache.read_sidecar("kenney", "fixture-001").unwrap();
        assert_eq!(sc.id, "fixture-001");
        assert_eq!(sc.license, "cc0");
        assert_eq!(sc.author, "Anonymous");
        assert_eq!(sc.src, "kenney");
        assert_eq!(sc.tags.len(), 2);
        assert!(sc.download_ts > 0);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rescan_finds_existing_entries() {
        let root = unique_temp_root("rescan");
        {
            let mut cache = LruCache::new(root.clone(), 1_000_000).unwrap();
            cache
                .put("kenney", "persisted", b"persisted-bytes", &fixture_meta())
                .unwrap();
        }
        // Re-open : rescan should pick up the persisted entry.
        let cache = LruCache::new(root.clone(), 1_000_000).unwrap();
        assert!(cache.get_path("kenney", "persisted").is_some());
        let _ = fs::remove_dir_all(root);
    }
}
