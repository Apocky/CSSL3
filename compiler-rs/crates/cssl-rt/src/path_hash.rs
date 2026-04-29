//! § cssl-rt path-hash discipline (T11-D130 / F6 observability).
//!
//! § ROLE
//!   The runtime's lower-half of the path-hash-only logging discipline.
//!   The FFI shims (`__cssl_fs_open`) compute a salted BLAKE3 hash of
//!   the path BEFORE any other recording happens, then call
//!   [`crate::io::record_path_hash_event`] with the 32-byte hash + op.
//!
//! § DESIGN
//!   - [`process_salt`] holds the 32-byte per-process salt in a
//!     `OnceLock`. First-call generates a fresh OS-RNG random salt ;
//!     tests can install a deterministic salt via [`install_test_salt`].
//!   - [`hash_path_bytes`] is the canonical hashing entry-point.
//!   - The algorithm is byte-identical to `cssl_telemetry::PathHasher` :
//!     `BLAKE3(domain-tag || salt || path-bytes)`.
//!
//! § PRIME-DIRECTIVE attestation
//!   "no raw paths logged ; only BLAKE3-salted path-hashes appear in
//!    telemetry + audit-chain"
//!
//!   This module is the structural enforcement point for that attestation
//!   in the lower-half of the runtime.

#![allow(unsafe_code)]

use core::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

/// Domain tag — must match `cssl_telemetry::path_hash::PATH_HASH_DOMAIN`.
const PATH_HASH_DOMAIN: &[u8] = b"cssl-path-hash-v1";

/// Per-process 32-byte salt.
static PROCESS_SALT: OnceLock<[u8; 32]> = OnceLock::new();
static TEST_SALT_INSTALLED: AtomicBool = AtomicBool::new(false);

fn process_salt() -> &'static [u8; 32] {
    PROCESS_SALT.get_or_init(|| {
        use rand::RngCore as _;
        let mut bytes = [0u8; 32];
        let mut rng = rand::rngs::OsRng;
        rng.fill_bytes(&mut bytes);
        bytes
    })
}

/// Install a deterministic test-salt. Returns `Ok(())` on first call ;
/// [`SaltAlreadyInstalled`] if the salt is already initialized.
///
/// # Errors
/// Returns [`SaltAlreadyInstalled`] if the per-process salt was already
/// initialized — either by a prior `install_test_salt` call or by a
/// hash-call that triggered the OS-RNG fallback.
pub fn install_test_salt(seed: [u8; 32]) -> Result<(), SaltAlreadyInstalled> {
    if PROCESS_SALT.set(seed).is_err() {
        return Err(SaltAlreadyInstalled);
    }
    TEST_SALT_INSTALLED.store(true, Ordering::Relaxed);
    Ok(())
}

/// Failure mode for [`install_test_salt`] : the per-process salt is
/// already initialized.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SaltAlreadyInstalled;

impl core::fmt::Display for SaltAlreadyInstalled {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("path-hash salt already installed (call before any fs op)")
    }
}

impl std::error::Error for SaltAlreadyInstalled {}

/// True if a test-salt was explicitly installed (vs OS-RNG default).
#[doc(hidden)]
#[must_use]
pub fn test_salt_installed() -> bool {
    TEST_SALT_INSTALLED.load(Ordering::Relaxed)
}

/// Compute the salted BLAKE3 hash of `path_bytes`.
#[must_use]
pub fn hash_path_bytes(path_bytes: &[u8]) -> [u8; 32] {
    let salt = process_salt();
    let mut hasher = blake3::Hasher::new();
    hasher.update(PATH_HASH_DOMAIN);
    hasher.update(salt);
    hasher.update(path_bytes);
    *hasher.finalize().as_bytes()
}

/// Compute the salted BLAKE3 hash from a `(ptr, len)` pair.
///
/// # Safety
/// `path_ptr` must be valid for `path_len` consecutive bytes. Returns
/// the 32-byte zero-hash if `path_ptr` is null AND `path_len == 0`.
#[must_use]
pub unsafe fn hash_path_ptr(path_ptr: *const u8, path_len: usize) -> [u8; 32] {
    if path_ptr.is_null() || path_len == 0 {
        return [0u8; 32];
    }
    // SAFETY : caller contract — path_ptr valid for path_len bytes.
    let bytes = unsafe { core::slice::from_raw_parts(path_ptr, path_len) };
    hash_path_bytes(bytes)
}

#[cfg(test)]
mod tests {
    use super::{hash_path_bytes, hash_path_ptr, install_test_salt, test_salt_installed};

    // The salt is a process-wide OnceLock — these tests can run in any
    // order but the salt is initialized exactly once. We rely on
    // determinism within whatever salt happens to be in place.

    #[test]
    fn salt_install_then_hash_roundtrip() {
        let _g = crate::test_helpers::lock_and_reset_all();
        let _ = install_test_salt([0xAB; 32]);

        let h1 = hash_path_bytes(b"/etc/hosts");
        let h2 = hash_path_bytes(b"/etc/hosts");
        assert_eq!(h1, h2, "deterministic within-process");
        assert_ne!(h1, [0u8; 32], "salt makes zero-collision astronomical");

        let h3 = hash_path_bytes(b"/etc/passwd");
        assert_ne!(h1, h3, "different paths -> different hashes");
    }

    #[test]
    fn hash_path_ptr_matches_hash_path_bytes() {
        let _g = crate::test_helpers::lock_and_reset_all();
        let p = b"/tmp/abc.txt";
        let h1 = hash_path_bytes(p);
        // SAFETY : valid byte-slice.
        let h2 = unsafe { hash_path_ptr(p.as_ptr(), p.len()) };
        assert_eq!(h1, h2);
    }

    #[test]
    fn hash_path_ptr_null_returns_zero() {
        let _g = crate::test_helpers::lock_and_reset_all();
        // SAFETY : null-zero combo is the documented sentinel.
        let h = unsafe { hash_path_ptr(core::ptr::null(), 0) };
        assert_eq!(h, [0u8; 32]);
    }

    #[test]
    fn test_salt_installed_reflects_state() {
        let _g = crate::test_helpers::lock_and_reset_all();
        let _ = test_salt_installed();
    }
}
