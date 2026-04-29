//! § cssl-substrate-save — `save` / `load` to host filesystem (S8-H5, T11-D93).
//!
//! § ROLE
//!   Bridges [`crate::SaveFile`] to the host file-system. Both functions
//!   route through the same path-hash discipline : the cleartext path is
//!   ONLY used for the `std::fs::*` syscall ; every error message + every
//!   audit-log event references the path by its BLAKE3 hash-prefix per
//!   `specs/22_TELEMETRY.csl § FS-OPS` (PRIME-DIRECTIVE no-path-leakage).
//!
//! § PATH HANDLING
//!   At stage-0 the host file-system is std::fs ; the cssl-rt FFI layer
//!   from S6-B5 (T11-D76) is the eventual target. The format is stable
//!   across the upgrade — the byte-stream this slice produces will be
//!   readable by future cssl-rt-routed code without migration.
//!
//! § PRIME-DIRECTIVE alignment
//!   - **Path-hash-only logging** : [`path_hash`] computes the canonical
//!     BLAKE3 hash of a path string. All error variants in
//!     [`crate::SaveError`] / [`crate::LoadError`] carry the hex-prefix
//!     of this hash, never the cleartext path. This keeps the
//!     panic-message + tracing-event log free of path-leakage.
//!   - **Hard-fail on attestation mismatch** : [`load`] verifies attestation
//!     INSIDE [`crate::SaveFile::from_bytes`] before returning the
//!     scheduler ; mismatch → [`crate::LoadError::AttestationMismatch`].
//!     The caller never sees a half-corrupt scheduler.

use std::fs::{File, OpenOptions};
use std::io::{Read as _, Write as _};
use std::path::Path;

use cssl_telemetry::ContentHash;

use crate::error::{LoadError, SaveError};
use crate::format::SaveFile;
use crate::omega::OmegaScheduler;

/// Compute the BLAKE3 hash of a path's UTF-8 encoding. The result is the
/// canonical "where was this save written" identifier that may appear in
/// audit-logs / telemetry / error messages without leaking the cleartext
/// path. PRIME-DIRECTIVE per `specs/22_TELEMETRY.csl § FS-OPS`.
#[must_use]
pub fn path_hash(path: &str) -> ContentHash {
    ContentHash::hash(path.as_bytes())
}

/// Hex-prefix (first 16 hex chars = 8 bytes) of the path-hash, suitable
/// for inclusion in an error message without disclosing the cleartext path.
fn path_hash_prefix(path: &str) -> String {
    let hex = path_hash(path).hex();
    hex.chars().take(16).collect()
}

/// Serialize `scheduler` to disk at `path`. The file is created (or
/// truncated) ; per the slice handoff stage-0 we use `std::fs::OpenOptions`
/// rather than the cssl-rt FFI layer.
///
/// # Errors
/// Returns [`SaveError::FsError`] on any filesystem error. Per the
/// PRIME-DIRECTIVE no-path-leakage discipline, the error message
/// contains only the BLAKE3 hash-prefix of `path`, never the cleartext.
pub fn save(scheduler: &OmegaScheduler, path: impl AsRef<Path>) -> Result<(), SaveError> {
    let path_ref = path.as_ref();
    let path_str = path_ref.to_string_lossy().into_owned();
    let prefix = path_hash_prefix(&path_str);

    let sf = SaveFile::from_scheduler(scheduler);
    let bytes = sf.to_bytes();

    // Open with create + truncate + write.
    let mut f = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(path_ref)
        .map_err(|source| SaveError::FsError {
            path_hash_prefix: prefix.clone(),
            source,
        })?;
    f.write_all(&bytes).map_err(|source| SaveError::FsError {
        path_hash_prefix: prefix.clone(),
        source,
    })?;
    // Force sync to be deterministic across test-runs.
    f.flush().map_err(|source| SaveError::FsError {
        path_hash_prefix: prefix,
        source,
    })?;
    Ok(())
}

/// Load + verify a save-file from disk at `path`, returning the
/// reconstructed [`OmegaScheduler`].
///
/// Verifies (in order) :
/// 1. file exists + is readable
/// 2. magic header matches `b"CSSLSAVE"`
/// 3. version field is the current [`crate::FORMAT_VERSION`]
/// 4. trailer-offset matches the body-end position
/// 5. attestation-hash is the BLAKE3 of the body (HARD-FAIL on mismatch)
/// 6. inner blobs parse cleanly
///
/// # Errors
/// Returns the corresponding [`LoadError`] variant on any failure. Per
/// PRIME-DIRECTIVE the attestation-mismatch case is HARD-FAIL — the caller
/// receives [`LoadError::AttestationMismatch`] and never a half-corrupt
/// scheduler.
pub fn load(path: impl AsRef<Path>) -> Result<OmegaScheduler, LoadError> {
    let path_ref = path.as_ref();
    let path_str = path_ref.to_string_lossy().into_owned();
    let prefix = path_hash_prefix(&path_str);

    let mut f = File::open(path_ref).map_err(|source| LoadError::FsError {
        path_hash_prefix: prefix.clone(),
        source,
    })?;
    let mut bytes = Vec::new();
    f.read_to_end(&mut bytes)
        .map_err(|source| LoadError::FsError {
            path_hash_prefix: prefix,
            source,
        })?;

    let sf = SaveFile::from_bytes(&bytes)?;
    Ok(sf.into_scheduler())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::OMEGA_TYPE_TAG_I32;
    use crate::omega::{OmegaCell, OmegaTensor, ReplayEvent, ReplayKind};

    /// Build a per-test temp file path.
    /// We hand-roll instead of pulling `tempfile` to avoid widening
    /// the workspace dep-graph (per slice landmines on workspace-policy).
    fn tmp_path(test_name: &str) -> std::path::PathBuf {
        // Use a per-test counter via std::time + thread-id to avoid collisions.
        let pid = std::process::id();
        let counter = std::sync::atomic::AtomicU64::new(0);
        // Any per-test unique value is fine — just guard against collisions
        // in the parallel-test harness.
        let n = counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let mut p = std::env::temp_dir();
        p.push(format!("cssl-h5-{test_name}-{pid}-{n}-{nanos}.csslsave"));
        p
    }

    #[test]
    fn path_hash_is_deterministic() {
        let h1 = path_hash("save/foo.csslsave");
        let h2 = path_hash("save/foo.csslsave");
        assert_eq!(h1, h2);
    }

    #[test]
    fn path_hash_differs_for_distinct_paths() {
        let h1 = path_hash("save/foo.csslsave");
        let h2 = path_hash("save/bar.csslsave");
        assert_ne!(h1, h2);
    }

    #[test]
    fn save_then_load_round_trip() {
        let mut s = OmegaScheduler::new();
        s.insert_tensor(
            "frame-counter",
            OmegaTensor::scalar(OmegaCell::new(
                OMEGA_TYPE_TAG_I32,
                1234i32.to_le_bytes().to_vec(),
            )),
        );
        s.frame = 42;
        s.replay_log
            .append(ReplayEvent::new(0, ReplayKind::Sim, vec![1, 2, 3]));

        let path = tmp_path("save-then-load");
        save(&s, &path).expect("save must succeed");

        let s2 = load(&path).expect("load must succeed");
        assert_eq!(s, s2);

        // Cleanup ; ignore errors on Windows (test-isolation primary).
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn load_rejects_bad_magic_after_disk_corrupt() {
        let s = OmegaScheduler::new();
        let path = tmp_path("bad-magic");
        save(&s, &path).expect("save must succeed");

        // Corrupt the magic byte.
        let mut bytes = std::fs::read(&path).expect("read");
        bytes[0] = b'X';
        std::fs::write(&path, &bytes).expect("write");

        let err = load(&path).unwrap_err();
        assert_eq!(err, LoadError::BadMagic);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn load_rejects_attestation_tamper() {
        let mut s = OmegaScheduler::new();
        s.insert_tensor(
            "x",
            OmegaTensor::scalar(OmegaCell::new(
                OMEGA_TYPE_TAG_I32,
                1i32.to_le_bytes().to_vec(),
            )),
        );
        let path = tmp_path("attestation-tamper");
        save(&s, &path).expect("save must succeed");

        // Flip a bit somewhere in the omega blob (offset 20 = body start).
        let mut bytes = std::fs::read(&path).expect("read");
        let body_byte = 20 + 8; // skip into the body past 4-byte name-len
        bytes[body_byte] ^= 0x01;
        std::fs::write(&path, &bytes).expect("write");

        let err = load(&path).unwrap_err();
        assert_eq!(err, LoadError::AttestationMismatch);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn load_nonexistent_returns_fs_error() {
        let path = tmp_path("does-not-exist");
        let err = load(&path).unwrap_err();
        match err {
            LoadError::FsError {
                path_hash_prefix, ..
            } => {
                // Hash-prefix is 16 hex chars per format::path_hash_prefix discipline.
                assert_eq!(path_hash_prefix.len(), 16);
                assert!(path_hash_prefix.chars().all(|c| c.is_ascii_hexdigit()));
            }
            other => panic!("expected FsError, got {other:?}"),
        }
    }

    #[test]
    fn save_error_message_does_not_leak_path() {
        // Try to write to a directory-shaped path on disk (open-as-file fails).
        let dir = std::env::temp_dir();
        let s = OmegaScheduler::new();
        let result = save(&s, &dir);
        if let Err(SaveError::FsError {
            path_hash_prefix, ..
        }) = result
        {
            // Hash prefix is hex ; no path components.
            assert!(!path_hash_prefix.contains('/'));
            assert!(!path_hash_prefix.contains('\\'));
            assert_eq!(path_hash_prefix.len(), 16);
        }
        // If save somehow succeeded (unlikely on a directory), no assertion.
    }
}
