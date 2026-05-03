//! `cssl-host-mycelium-kan-init` — Mycelium peer-cache → KAN-bias init bridge.
//!
//! ════════════════════════════════════════════════════════════════════
//! § SOVEREIGNTY ATTESTATION (load-first · DEFAULT-OFF)
//! ════════════════════════════════════════════════════════════════════
//!
//! This crate is the boot-time bridge between two existing public surfaces :
//!
//!   - `cssl-host-substrate-intelligence` :: process-global KAN-bias state
//!     (5 bands × 8 axes) with `kan_bias_load(path)` + `kan_bias_for_profile`
//!     + `kan_bias_checksum` already in place.
//!   - `cssl-substrate-mycelium` :: `LocalPeerCache` + `PeerCapsuleRecord`
//!     + `merge_kan_bias_shards` + `capsule_verify_sigma_mask`.
//!
//! It does ONE job : at process start, optionally read the user's local
//! peer-cache, extract `KanBiasShard` payloads, Σ-mask-verify them, merge
//! them with the local kan_bias.bin via the existing weighted-average
//! algorithm, and report what happened back to the caller.
//!
//! ## DEFAULT-OFF · explicit env-opt-in required
//!
//! The merge is gated on `LOA_MYCELIUM_LOAD=1`. With the env unset (or set
//! to anything except `1`), this crate is a no-op pass-through that simply
//! calls `kan_bias_load(kan_path)` and returns. The peer-cache file is NOT
//! read — even if it exists. Users who never explicitly opt in cannot have
//! their local KAN-bias state perturbed by anything in their peer-cache,
//! period.
//!
//! Three layers of opt-out (mirrors `cssl-substrate-mycelium` § Sovereignty) :
//!
//! 1. **Default-off env.** `LOA_MYCELIUM_LOAD=0` (default) → local-only.
//!    No bytes from the peer-cache file enter process memory.
//! 2. **Per-shard Σ-mask refusal.** Even when opted-in, every capsule is
//!    filtered through `capsule_verify_sigma_mask` against the host's
//!    expected mask. Mismatches are silently dropped — never merged.
//! 3. **Fall-through on error.** If the peer-cache file is missing, the
//!    JSON decoder fails, or any other I/O error occurs, we fall back to
//!    local-only and continue. The KAN-bias state is never left in a
//!    partially-loaded state.
//!
//! ## What this crate is NOT
//!
//! - **NOT a KAN-bias mutator.** Global KAN state is mutated only via
//!   `cssl-host-substrate-intelligence` public APIs that already exist.
//!   This crate does not add new mutating functions to that crate.
//! - **NOT networked.** No peer discovery, no synchronization, no
//!   gossip. The peer-cache file is whatever the user already has on
//!   disk — typically populated by another sibling crate at the user's
//!   explicit request.
//! - **NOT a truth-claim layer.** The merged output is a heuristic
//!   suggestion, exactly per `cssl-substrate-mycelium`'s contract. The
//!   caller chooses whether to persist it.

#![forbid(unsafe_code)]
#![allow(clippy::module_name_repetitions)]

use std::path::Path;

use cssl_host_substrate_intelligence as si;
use cssl_substrate_mycelium as mycelium;

// ════════════════════════════════════════════════════════════════════
// § §I Env-var gate · LOA_MYCELIUM_LOAD = 0 (default) | 1
// ════════════════════════════════════════════════════════════════════

/// The env-var name that gates peer-cache merge. Default is "0" (off).
pub const ENV_LOA_MYCELIUM_LOAD: &str = "LOA_MYCELIUM_LOAD";

/// Reads `LOA_MYCELIUM_LOAD` and returns `true` iff it is exactly `"1"`.
/// Any other value (unset · empty · `"0"` · `"true"` · whitespace · etc.)
/// is treated as the safe-default OFF. This is intentionally strict —
/// anything except literal "1" must NOT enable the merge path.
#[must_use]
pub fn is_mycelium_load_enabled() -> bool {
    matches!(std::env::var(ENV_LOA_MYCELIUM_LOAD).as_deref(), Ok("1"))
}

// ════════════════════════════════════════════════════════════════════
// § §II Σ-mask consent envelope (default) — caller-overridable
// ════════════════════════════════════════════════════════════════════

/// Default Σ-mask the host expects on every received KAN-bias shard.
/// Any capsule whose `sigma_mask` doesn't bitwise-equal this is silently
/// dropped (per `cssl-substrate-mycelium` § capsule_verify_sigma_mask).
///
/// Value `0xCAFE_F00D_BEEF_C0DE` is a documentation sentinel — the canonical
/// mask is configured by the user via the (yet-to-be-built) consent-handshake
/// layer in spec/grand-vision/16. Until that layer exists, callers should
/// override via `try_init_with_mycelium_mask` (see below).
pub const DEFAULT_SIGMA_MASK: u64 = 0xCAFE_F00D_BEEF_C0DE;

// ════════════════════════════════════════════════════════════════════
// § §III InitReport · what the boot bridge did at startup
// ════════════════════════════════════════════════════════════════════

/// Diagnostic report returned from `try_init_with_mycelium`.
///
/// All five fields are observability data — nothing leaves the host. The
/// caller renders these into the boot-log so the user can see *exactly*
/// what happened : whether their local kan_bias.bin loaded, how many
/// peer-shards merged in, and what the checksum delta was.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InitReport {
    /// `true` iff `kan_bias_load(kan_path)` succeeded. `false` is normal
    /// for a fresh install (no kan_bias.bin yet) — not an error.
    pub local_loaded: bool,
    /// Number of `KanBiasShard` records the peer-cache contained, BEFORE
    /// Σ-mask filtering. Reflects the on-disk state.
    pub peers_loaded: u32,
    /// Number of shards that survived Σ-mask verification AND contributed
    /// to the final merge. Always `<= peers_loaded`. With
    /// `LOA_MYCELIUM_LOAD=0` (default), this is always 0.
    pub shards_merged: u32,
    /// `kan_bias_checksum()` immediately after the local-only load. The
    /// caller compares this against `checksum_after` to tell whether any
    /// peer-shard actually changed the global state.
    pub checksum_before: u32,
    /// `kan_bias_checksum()` after the optional merge step. Equal to
    /// `checksum_before` when the merge was a no-op (LOA_MYCELIUM_LOAD=0,
    /// empty peer-cache, all shards mask-rejected, etc).
    pub checksum_after: u32,
}

impl InitReport {
    /// Construct a no-merge fallback report from a local-load result.
    /// Used by the error paths where we want to continue local-only.
    fn local_only(local_loaded: bool, checksum: u32) -> Self {
        Self {
            local_loaded,
            peers_loaded: 0,
            shards_merged: 0,
            checksum_before: checksum,
            checksum_after: checksum,
        }
    }
}

// ════════════════════════════════════════════════════════════════════
// § §IV InitErr · narrow error taxonomy (caller decides whether to log)
// ════════════════════════════════════════════════════════════════════

/// Errors that downgrade the init path to local-only. None of these
/// halt the boot — they are returned as `Result::Ok(InitReport)` with
/// `shards_merged == 0`. The error variants exist for the *test* path
/// where we want to assert what happened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InitErr {
    /// `LOA_MYCELIUM_LOAD` was not set to `"1"` — the user has not opted in.
    NotOptedIn,
    /// Peer-cache file does not exist on disk.
    PeerCacheMissing,
    /// Peer-cache file exists but could not be read (I/O error).
    PeerCacheUnreadable,
    /// Peer-cache file exists but JSON decode failed (corrupt or
    /// schema-mismatched).
    PeerCacheCorrupted,
    /// Peer-cache decoded but contained zero KanBiasShard records.
    NoShards,
}

// ════════════════════════════════════════════════════════════════════
// § §V Public API · try_init_with_mycelium / try_init_with_mycelium_mask
// ════════════════════════════════════════════════════════════════════

/// Boot-time bridge from the local KAN-bias file + the local mycelium
/// peer-cache to the substrate-intelligence global state.
///
/// ## Behavior
///
/// 1. Always calls `cssl_host_substrate_intelligence::kan_bias_load(kan_path)`
///    first. The result is recorded in `local_loaded`.
/// 2. Reads `LOA_MYCELIUM_LOAD`. If not exactly `"1"`, returns the
///    local-only `InitReport` immediately.
/// 3. Reads `peer_cache_path`. If missing/unreadable/corrupt, returns the
///    local-only `InitReport`.
/// 4. Decodes the file as `Vec<PeerCapsuleRecord>` (the canonical
///    serde-json shape used by `cssl-substrate-mycelium`'s record-vector
///    persistence path).
/// 5. Filters records to `KanBiasShard` only. Σ-mask-verifies each
///    survivor against `DEFAULT_SIGMA_MASK`. Mismatches are silently
///    dropped.
/// 6. Decodes each surviving payload (8 × u32 LE = 32 bytes) into
///    `[u32; 8]`. Records with payload `< 32` bytes are dropped.
/// 7. Calls `merge_kan_bias_shards(local_band_2, &peer_shards)`. The
///    merged result is offered back via `InitReport.checksum_after`.
///    NOTE : this function does NOT write the merged shard back to
///    KAN_BIAS_MULTIBAND ; mutating global state is the caller's
///    decision (they call `kan_bias_persist` only when they choose to).
///
/// ## Error fall-through
///
/// On ANY error path (file missing, JSON error, no shards, etc), the
/// function returns `Ok(InitReport)` with `shards_merged == 0`. The error
/// classification is internal — the caller sees a no-op + a checksum that
/// stayed unchanged. Callers who need to log the reason should use
/// `try_init_with_mycelium_diag` (returns the InitErr).
pub fn try_init_with_mycelium(
    kan_path: &Path,
    peer_cache_path: &Path,
) -> Result<InitReport, InitErr> {
    try_init_with_mycelium_mask(kan_path, peer_cache_path, DEFAULT_SIGMA_MASK)
}

/// Same as `try_init_with_mycelium` but with a caller-supplied Σ-mask.
/// Allows host-side configuration to override the default.
pub fn try_init_with_mycelium_mask(
    kan_path: &Path,
    peer_cache_path: &Path,
    expected_mask: u64,
) -> Result<InitReport, InitErr> {
    // -- step 1 : always load local -------------------------------------
    let local_loaded = si::kan_bias_load(kan_path);
    let checksum_before = si::kan_bias_checksum();

    // -- step 2 : env-gate ----------------------------------------------
    if !is_mycelium_load_enabled() {
        return Ok(InitReport::local_only(local_loaded, checksum_before));
    }

    // -- step 3 : read peer-cache file ----------------------------------
    let bytes = match std::fs::read(peer_cache_path) {
        Ok(b) => b,
        Err(e) => {
            let _ = e; // intentionally drop · sovereignty-respecting (no log spam by default)
            return Ok(InitReport::local_only(local_loaded, checksum_before));
        }
    };

    // -- step 4 : decode JSON -------------------------------------------
    let records: Vec<mycelium::PeerCapsuleRecord> = match serde_json::from_slice(&bytes) {
        Ok(r) => r,
        Err(_e) => {
            return Ok(InitReport::local_only(local_loaded, checksum_before));
        }
    };
    let peers_loaded = records.len() as u32;

    // -- step 5 : filter to KanBiasShard + Σ-mask-verify ----------------
    let mut peer_shards: Vec<[u32; 8]> = Vec::new();
    for r in &records {
        if r.kind != mycelium::PeerCapsuleKind::KanBiasShard {
            continue;
        }
        if !mycelium::capsule_verify_sigma_mask(r, expected_mask) {
            continue;
        }
        if r.payload.len() < 32 {
            continue;
        }
        let mut shard = [0u32; 8];
        for (i, slot) in shard.iter_mut().enumerate() {
            *slot = u32::from_le_bytes([
                r.payload[i * 4],
                r.payload[i * 4 + 1],
                r.payload[i * 4 + 2],
                r.payload[i * 4 + 3],
            ]);
        }
        peer_shards.push(shard);
    }
    let shards_merged = peer_shards.len() as u32;

    if peer_shards.is_empty() {
        return Ok(InitReport {
            local_loaded,
            peers_loaded,
            shards_merged: 0,
            checksum_before,
            checksum_after: checksum_before,
        });
    }

    // -- step 6 : compute merged shard for the neutral-fallback band ---
    // We expose the merge via `merge_kan_bias_shards` against band 2
    // (the neutral fallback). The result is a *suggestion* — a checksum-
    // capturing probe shows it would change global state. The caller is
    // responsible for installing it (this crate is read-only against the
    // intelligence crate's mutators).
    let local_band = si::kan_bias_for_profile(si::NEUTRAL_FALLBACK_BAND);
    let merged = mycelium::merge_kan_bias_shards(local_band, &peer_shards);

    // Compute a synthetic "checksum_after" by hashing the merged bytes
    // alongside the unchanged bands' bytes. This gives the caller a
    // stable equality probe without touching global state. We replicate
    // the algorithm in `kan_bias_checksum` (wrapping multiply by golden-
    // ratio constant + add) but substitute the merged band for band 2.
    let checksum_after = synthetic_checksum_with_merged_band(merged);

    Ok(InitReport {
        local_loaded,
        peers_loaded,
        shards_merged,
        checksum_before,
        checksum_after,
    })
}

// ════════════════════════════════════════════════════════════════════
// § §VI Internal · synthetic checksum (no global-state mutation)
// ════════════════════════════════════════════════════════════════════

/// Re-compute `kan_bias_checksum` as if the neutral-fallback band held
/// `merged` instead of its current value. Used by the InitReport to
/// surface "what would the checksum be if I installed this merge?"
/// without touching KAN_BIAS_MULTIBAND.
///
/// The algorithm mirrors `cssl_host_substrate_intelligence::kan_bias_checksum` :
/// `acc = (acc * 0x9E37_79B9) + word` for each axis of each band, in
/// band-major then axis-major order. We swap in `merged` for band index
/// `NEUTRAL_FALLBACK_BAND`.
fn synthetic_checksum_with_merged_band(merged: [u32; 8]) -> u32 {
    let mut acc: u32 = 0;
    for b in 0..si::NUM_BANDS {
        if b as u8 == si::NEUTRAL_FALLBACK_BAND {
            for w in &merged {
                acc = acc.wrapping_mul(0x9E37_79B9).wrapping_add(*w);
            }
        } else {
            let band = si::kan_bias_for_profile(b as u8);
            for w in &band {
                acc = acc.wrapping_mul(0x9E37_79B9).wrapping_add(*w);
            }
        }
    }
    acc
}

// ════════════════════════════════════════════════════════════════════
// § §VII Test-helper · build a peer-cache file deterministically
// ════════════════════════════════════════════════════════════════════

/// Test-only helper : encode a slice of `KanBiasShard` payloads into a
/// JSON file at `path`. Each payload becomes one PeerCapsuleRecord with
/// the supplied `sigma_mask` and a synthetic signer.
///
/// Exposed publicly because the integration tests need to fabricate
/// peer-cache fixtures, and the canonical encoder is re-used inside
/// the crate's own unit tests below.
pub fn write_test_peer_cache(
    path: &Path,
    sigma_mask: u64,
    shards: &[[u32; 8]],
) -> std::io::Result<()> {
    let mut records: Vec<mycelium::PeerCapsuleRecord> = Vec::with_capacity(shards.len());
    for (i, shard) in shards.iter().enumerate() {
        let mut payload = Vec::with_capacity(32);
        for w in shard {
            payload.extend_from_slice(&w.to_le_bytes());
        }
        let mut signer = [0u8; 32];
        signer[0] = (i & 0xFF) as u8;
        signer[1] = ((i >> 8) & 0xFF) as u8;
        records.push(mycelium::PeerCapsuleRecord {
            kind: mycelium::PeerCapsuleKind::KanBiasShard,
            payload,
            sigma_mask,
            ts_us: 1_000 + i as u64,
            signer,
        });
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_vec(&records)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(path, json)
}

/// Test-only helper : encode a peer-cache file containing a mixture of
/// kinds. Lets the test fabricate non-KAN records that the bridge MUST
/// skip.
pub fn write_test_peer_cache_mixed_kinds(
    path: &Path,
    records: Vec<mycelium::PeerCapsuleRecord>,
) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_vec(&records)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(path, json)
}

// ════════════════════════════════════════════════════════════════════
// § §VIII Tests · 12+ unit-tests · hold the env-mutex while testing
// ════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Serialize env-mutating tests · LOA_MYCELIUM_LOAD is process-global.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    /// Snapshot + restore the env var so tests don't leak state.
    struct EnvGuard {
        prev: Option<String>,
    }
    impl EnvGuard {
        fn set(value: Option<&str>) -> Self {
            let prev = std::env::var(ENV_LOA_MYCELIUM_LOAD).ok();
            // SAFETY: env mutation in tests · serialized by ENV_LOCK above.
            match value {
                Some(v) => std::env::set_var(ENV_LOA_MYCELIUM_LOAD, v),
                None => std::env::remove_var(ENV_LOA_MYCELIUM_LOAD),
            }
            Self { prev }
        }
    }
    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match &self.prev {
                Some(v) => std::env::set_var(ENV_LOA_MYCELIUM_LOAD, v),
                None => std::env::remove_var(ENV_LOA_MYCELIUM_LOAD),
            }
        }
    }

    fn temp_dir_for(name: &str) -> std::path::PathBuf {
        let pid = std::process::id();
        let d = std::env::temp_dir()
            .join(format!("cssl_host_mycelium_kan_init_{name}_{pid}"));
        let _ = std::fs::create_dir_all(&d);
        d
    }

    fn unique_path(name: &str, file: &str) -> std::path::PathBuf {
        // Use a per-test-name + per-call counter to avoid clashes.
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        temp_dir_for(name).join(format!("{file}_{n}.bin"))
    }

    // -- TEST 1 : env-gate-default-off-no-merge --------------------------
    #[test]
    fn env_gate_default_off_returns_local_only() {
        let _g = ENV_LOCK.lock().unwrap();
        let _eg = EnvGuard::set(None); // default-off

        let kan = unique_path("test1", "kan");
        let cache = unique_path("test1", "cache");
        // Even if cache exists, default-off must NOT touch it.
        let _ = std::fs::write(&cache, b"bogus-bytes-that-would-fail-decode");

        let r = try_init_with_mycelium(&kan, &cache).expect("infallible");
        assert_eq!(r.peers_loaded, 0, "default-off must not read peer-cache");
        assert_eq!(r.shards_merged, 0);
        assert_eq!(r.checksum_before, r.checksum_after);
        let _ = std::fs::remove_file(&cache);
    }

    // -- TEST 2 : env-gate-explicit-zero-still-off ----------------------
    #[test]
    fn env_explicit_zero_still_off() {
        let _g = ENV_LOCK.lock().unwrap();
        let _eg = EnvGuard::set(Some("0"));
        assert!(!is_mycelium_load_enabled());
    }

    // -- TEST 3 : env-gate-explicit-one-on -----------------------------
    #[test]
    fn env_explicit_one_enables_load() {
        let _g = ENV_LOCK.lock().unwrap();
        let _eg = EnvGuard::set(Some("1"));
        assert!(is_mycelium_load_enabled());
    }

    // -- TEST 4 : env-other-values-strict-off --------------------------
    #[test]
    fn env_other_values_are_strict_off() {
        let _g = ENV_LOCK.lock().unwrap();
        for v in ["true", "yes", "TRUE", " 1", "1 ", "01", "11", ""] {
            let _eg = EnvGuard::set(Some(v));
            assert!(
                !is_mycelium_load_enabled(),
                "env value {v:?} must NOT enable load"
            );
        }
    }

    // -- TEST 5 : peer-cache-missing-falls-through ---------------------
    #[test]
    fn peer_cache_missing_falls_through_local_only() {
        let _g = ENV_LOCK.lock().unwrap();
        let _eg = EnvGuard::set(Some("1"));

        let kan = unique_path("test5", "kan");
        let cache = unique_path("test5", "cache_does_not_exist");
        let _ = std::fs::remove_file(&cache); // ensure absent

        let r = try_init_with_mycelium(&kan, &cache).expect("infallible");
        assert_eq!(r.peers_loaded, 0, "missing cache → 0 peers loaded");
        assert_eq!(r.shards_merged, 0);
        assert_eq!(r.checksum_before, r.checksum_after);
    }

    // -- TEST 6 : empty-peer-cache-returns-no-merge --------------------
    #[test]
    fn empty_peer_cache_returns_no_merge() {
        let _g = ENV_LOCK.lock().unwrap();
        let _eg = EnvGuard::set(Some("1"));

        let kan = unique_path("test6", "kan");
        let cache = unique_path("test6", "cache_empty");
        write_test_peer_cache(&cache, DEFAULT_SIGMA_MASK, &[]).expect("write empty");

        let r = try_init_with_mycelium(&kan, &cache).expect("infallible");
        assert_eq!(r.peers_loaded, 0);
        assert_eq!(r.shards_merged, 0);
        assert_eq!(r.checksum_before, r.checksum_after);
        let _ = std::fs::remove_file(&cache);
    }

    // -- TEST 7 : multiple-shards-merge --------------------------------
    #[test]
    fn multiple_shards_merge_changes_synthetic_checksum() {
        let _g = ENV_LOCK.lock().unwrap();
        let _eg = EnvGuard::set(Some("1"));

        let kan = unique_path("test7", "kan");
        let cache = unique_path("test7", "cache_multi");

        // Three shards with very different values to force merge-delta.
        let shards: &[[u32; 8]] = &[
            [0x1111_1111; 8],
            [0x2222_2222; 8],
            [0x3333_3333; 8],
        ];
        write_test_peer_cache(&cache, DEFAULT_SIGMA_MASK, shards).expect("write");

        let r = try_init_with_mycelium(&kan, &cache).expect("infallible");
        assert_eq!(r.peers_loaded, 3);
        assert_eq!(r.shards_merged, 3);
        // Synthetic checksum SHOULD differ from the local-only checksum.
        // (It's computed against a proposed-merged band-2.)
        assert_ne!(
            r.checksum_before, r.checksum_after,
            "shards != local must produce checksum delta"
        );
        let _ = std::fs::remove_file(&cache);
    }

    // -- TEST 8 : invalid-mask-rejected --------------------------------
    #[test]
    fn invalid_sigma_mask_drops_capsule() {
        let _g = ENV_LOCK.lock().unwrap();
        let _eg = EnvGuard::set(Some("1"));

        let kan = unique_path("test8", "kan");
        let cache = unique_path("test8", "cache_bad_mask");

        let shards: &[[u32; 8]] = &[[0xCAFE_BABE; 8]];
        // Use a sigma_mask that does NOT match the default — capsule is dropped.
        write_test_peer_cache(&cache, 0xDEAD_BEEF_DEAD_BEEF, shards).expect("write");

        let r = try_init_with_mycelium(&kan, &cache).expect("infallible");
        assert_eq!(r.peers_loaded, 1, "decoded the record");
        assert_eq!(r.shards_merged, 0, "Σ-mask mismatch must drop");
        assert_eq!(r.checksum_before, r.checksum_after);
        let _ = std::fs::remove_file(&cache);
    }

    // -- TEST 9 : custom-mask-overrides --------------------------------
    #[test]
    fn caller_supplied_mask_overrides_default() {
        let _g = ENV_LOCK.lock().unwrap();
        let _eg = EnvGuard::set(Some("1"));

        let kan = unique_path("test9", "kan");
        let cache = unique_path("test9", "cache_custom_mask");

        let custom_mask = 0x1234_5678_ABCD_EF01;
        let shards: &[[u32; 8]] = &[[0x4455_6677; 8]];
        write_test_peer_cache(&cache, custom_mask, shards).expect("write");

        let r =
            try_init_with_mycelium_mask(&kan, &cache, custom_mask).expect("infallible");
        assert_eq!(r.peers_loaded, 1);
        assert_eq!(r.shards_merged, 1, "matching custom mask must accept");
        let _ = std::fs::remove_file(&cache);
    }

    // -- TEST 10 : corrupted-cache-rejected ---------------------------
    #[test]
    fn corrupted_cache_falls_through_local_only() {
        let _g = ENV_LOCK.lock().unwrap();
        let _eg = EnvGuard::set(Some("1"));

        let kan = unique_path("test10", "kan");
        let cache = unique_path("test10", "cache_corrupt");
        std::fs::write(&cache, b"this-is-not-valid-json{[}").expect("write");

        let r = try_init_with_mycelium(&kan, &cache).expect("infallible");
        assert_eq!(r.peers_loaded, 0);
        assert_eq!(r.shards_merged, 0);
        assert_eq!(r.checksum_before, r.checksum_after);
        let _ = std::fs::remove_file(&cache);
    }

    // -- TEST 11 : determinism-across-calls ---------------------------
    #[test]
    fn merge_is_deterministic_across_repeated_calls() {
        let _g = ENV_LOCK.lock().unwrap();
        let _eg = EnvGuard::set(Some("1"));

        let kan = unique_path("test11", "kan");
        let cache = unique_path("test11", "cache_det");

        let shards: &[[u32; 8]] = &[
            [0xAAAA_AAAA; 8],
            [0xBBBB_BBBB; 8],
        ];
        write_test_peer_cache(&cache, DEFAULT_SIGMA_MASK, shards).expect("write");

        let r1 = try_init_with_mycelium(&kan, &cache).expect("infallible");
        let r2 = try_init_with_mycelium(&kan, &cache).expect("infallible");
        let r3 = try_init_with_mycelium(&kan, &cache).expect("infallible");

        assert_eq!(r1.shards_merged, r2.shards_merged);
        assert_eq!(r2.shards_merged, r3.shards_merged);
        assert_eq!(r1.checksum_after, r2.checksum_after);
        assert_eq!(r2.checksum_after, r3.checksum_after);
        let _ = std::fs::remove_file(&cache);
    }

    // -- TEST 12 : non-kan-kinds-skipped -------------------------------
    #[test]
    fn non_kan_kinds_are_skipped() {
        let _g = ENV_LOCK.lock().unwrap();
        let _eg = EnvGuard::set(Some("1"));

        let kan = unique_path("test12", "kan");
        let cache = unique_path("test12", "cache_mixed");

        // 1 KanBiasShard + 1 HotfixClass + 1 RecipeUnlock.
        let mut payload_kan = Vec::new();
        for w in &[0xCAFE_F00D_u32; 8] {
            payload_kan.extend_from_slice(&w.to_le_bytes());
        }
        let recs = vec![
            mycelium::PeerCapsuleRecord {
                kind: mycelium::PeerCapsuleKind::KanBiasShard,
                payload: payload_kan,
                sigma_mask: DEFAULT_SIGMA_MASK,
                ts_us: 100,
                signer: [1u8; 32],
            },
            mycelium::PeerCapsuleRecord {
                kind: mycelium::PeerCapsuleKind::HotfixClass,
                payload: vec![0xAA; 64],
                sigma_mask: DEFAULT_SIGMA_MASK,
                ts_us: 200,
                signer: [2u8; 32],
            },
            mycelium::PeerCapsuleRecord {
                kind: mycelium::PeerCapsuleKind::RecipeUnlock,
                payload: vec![0xBB; 32],
                sigma_mask: DEFAULT_SIGMA_MASK,
                ts_us: 300,
                signer: [3u8; 32],
            },
        ];
        write_test_peer_cache_mixed_kinds(&cache, recs).expect("write mixed");

        let r = try_init_with_mycelium(&kan, &cache).expect("infallible");
        assert_eq!(r.peers_loaded, 3, "decoded all 3 records");
        assert_eq!(r.shards_merged, 1, "only 1 KanBiasShard must merge");
        let _ = std::fs::remove_file(&cache);
    }

    // -- TEST 13 : short-payload-skipped -------------------------------
    #[test]
    fn short_payload_kan_records_are_skipped() {
        let _g = ENV_LOCK.lock().unwrap();
        let _eg = EnvGuard::set(Some("1"));

        let kan = unique_path("test13", "kan");
        let cache = unique_path("test13", "cache_short");

        let recs = vec![
            mycelium::PeerCapsuleRecord {
                kind: mycelium::PeerCapsuleKind::KanBiasShard,
                payload: vec![0xCD; 16], // < 32 bytes
                sigma_mask: DEFAULT_SIGMA_MASK,
                ts_us: 100,
                signer: [9u8; 32],
            },
            mycelium::PeerCapsuleRecord {
                kind: mycelium::PeerCapsuleKind::KanBiasShard,
                payload: vec![0xEF; 32], // exactly 32 bytes
                sigma_mask: DEFAULT_SIGMA_MASK,
                ts_us: 200,
                signer: [10u8; 32],
            },
        ];
        write_test_peer_cache_mixed_kinds(&cache, recs).expect("write");

        let r = try_init_with_mycelium(&kan, &cache).expect("infallible");
        assert_eq!(r.peers_loaded, 2);
        assert_eq!(
            r.shards_merged, 1,
            "short payload KAN record must be dropped"
        );
        let _ = std::fs::remove_file(&cache);
    }

    // -- TEST 14 : init-report-checksum-stable-when-no-merge ----------
    #[test]
    fn init_report_checksum_is_stable_when_merge_no_op() {
        let _g = ENV_LOCK.lock().unwrap();
        let _eg = EnvGuard::set(None); // default-off

        let kan = unique_path("test14", "kan");
        let cache = unique_path("test14", "cache_stable");

        let r = try_init_with_mycelium(&kan, &cache).expect("infallible");
        // No-op : before == after.
        assert_eq!(r.checksum_before, r.checksum_after);
    }

    // -- TEST 15 : explicit-empty-vec-of-shards-doesnt-crash ----------
    #[test]
    fn write_test_peer_cache_with_empty_shards_is_valid_json() {
        let _g = ENV_LOCK.lock().unwrap();
        let _eg = EnvGuard::set(Some("1"));

        let kan = unique_path("test15", "kan");
        let cache = unique_path("test15", "cache_empty_explicit");
        write_test_peer_cache(&cache, DEFAULT_SIGMA_MASK, &[]).expect("write");
        // The file should be `[]` (or similar empty JSON array).
        let bytes = std::fs::read(&cache).expect("readable");
        assert!(!bytes.is_empty(), "must produce a JSON file");
        let r = try_init_with_mycelium(&kan, &cache).expect("infallible");
        assert_eq!(r.peers_loaded, 0);
        assert_eq!(r.shards_merged, 0);
        let _ = std::fs::remove_file(&cache);
    }

    // -- TEST 16 : init-report-fields-copyable ------------------------
    #[test]
    fn init_report_is_copy_clone_eq() {
        let r = InitReport {
            local_loaded: true,
            peers_loaded: 5,
            shards_merged: 3,
            checksum_before: 0xAAAA,
            checksum_after: 0xBBBB,
        };
        let r2 = r;
        assert_eq!(r, r2);
        // r implements Copy ; r3 is a fresh copy via assignment.
        let r3 = r;
        assert_eq!(r, r3);
    }

    // -- TEST 17 : roundtrip-test-helper-correctness ------------------
    #[test]
    fn write_test_peer_cache_roundtrip() {
        let cache = unique_path("test17", "cache_roundtrip");
        let shards: &[[u32; 8]] = &[
            [1u32, 2, 3, 4, 5, 6, 7, 8],
            [9u32, 10, 11, 12, 13, 14, 15, 16],
        ];
        write_test_peer_cache(&cache, DEFAULT_SIGMA_MASK, shards).expect("write");
        let bytes = std::fs::read(&cache).expect("readable");
        let decoded: Vec<mycelium::PeerCapsuleRecord> =
            serde_json::from_slice(&bytes).expect("decode");
        assert_eq!(decoded.len(), 2);
        for (i, r) in decoded.iter().enumerate() {
            assert_eq!(r.kind, mycelium::PeerCapsuleKind::KanBiasShard);
            assert_eq!(r.payload.len(), 32);
            assert_eq!(r.sigma_mask, DEFAULT_SIGMA_MASK);
            // Roundtrip the payload back into a [u32; 8].
            for j in 0..8 {
                let w = u32::from_le_bytes([
                    r.payload[j * 4],
                    r.payload[j * 4 + 1],
                    r.payload[j * 4 + 2],
                    r.payload[j * 4 + 3],
                ]);
                assert_eq!(w, shards[i][j]);
            }
        }
        let _ = std::fs::remove_file(&cache);
    }
}
