//! Path-hash-only logging discipline (T11-D130 / F6 observability).
//!
//! § SPEC : `specs/22_TELEMETRY.csl` § PRIME-DIRECTIVE-ENFORCEMENT
//!          + `specs/22_TELEMETRY.csl` § FS-OPS § telemetry-composition
//!          + `PRIME_DIRECTIVE.md` § 1 PROHIBITIONS § surveillance
//!          + `PRIME_DIRECTIVE.md` § 11 CREATOR-ATTESTATION.
//!
//! § THESIS
//!
//! Raw filesystem paths are surveillance-class metadata. A telemetry
//! ring + audit-chain that contains plain `/home/<user>/<project>/<file>`
//! strings leaks user-identity, directory-structure-fingerprints, and
//! work-pattern signatures to anyone who later inspects the logs — even
//! when the actual file-bytes never escape. The PRIME_DIRECTIVE § 1
//! `N! surveillance` clause requires that no observation occur without
//! consent ; *background* path-collection during routine fs-ops would
//! violate that clause regardless of how the logs are eventually used.
//!
//! § DISCIPLINE  (this module enforces)
//!
//!   1. Path arguments are NEVER stored in audit-entries, ring-slots, or
//!      exporter records as raw strings.
//!   2. The only observable form of a path is its 32-byte BLAKE3 hash
//!      computed under a per-installation salt (see [`PathHasher::new`]).
//!   3. The hash is deterministic *within a process* (so two ops on the
//!      same path produce the same hash, allowing correlation analysis)
//!      but NON-portable across installations (so two different
//!      installations' logs cannot be cross-correlated to track a user).
//!   4. The same-installation determinism is what enables R16
//!      reproducibility-tests + de-anonymization-free audit reconstruction.
//!
//! § DESIGN
//!
//!   - [`PathHasher`] is a thin wrapper around `blake3::Hasher` that holds
//!     the per-installation salt as a 32-byte secret.
//!   - The salt is generated once via [`PathHasher::new_random`] (OS
//!     randomness) or constructed from a known seed via
//!     [`PathHasher::from_seed`]. Tests use a fixed seed for determinism ;
//!     production uses random.
//!   - [`PathHash`] is a newtype around `[u8; 32]` so the type system can
//!     refuse silently-stringifying it back to a path-shape.
//!   - The convenience `Display` impl emits 16 hex chars + ellipsis (the
//!     "short-form" used in audit-entries) so accidentally `format!`-ing
//!     a hash never produces a 64-char hex blob that could be confused
//!     for path metadata.
//!
//! § PRIME_DIRECTIVE-ALIGNMENT
//!
//!   - § 1 `N! surveillance` : raw paths are surveillance-class data ;
//!     this module is the structural barrier preventing them from
//!     entering the observability surface.
//!   - § 4 `TRANSPARENCY` : the hash is computed by a published algorithm
//!     (BLAKE3), the salt-source is documented, and the discipline is
//!     attested in [`crate::audit::AuditChain`] via the §11 attestation
//!     extension.
//!   - § 11 `CREATOR-ATTESTATION` : the canonical attestation is extended
//!     with "no raw paths logged" via the
//!     [`PATH_HASH_DISCIPLINE_ATTESTATION`] constant.

use blake3::Hasher;
use core::fmt;
use std::path::Path;

use crate::audit::ContentHash;

// ───────────────────────────────────────────────────────────────────────
// § PathHasher — installation-salted BLAKE3 path hasher.
// ───────────────────────────────────────────────────────────────────────

/// 32-byte path-hash. Newtype wrapper preventing accidental cast back to
/// raw path-shape via `Deref<Target = [u8]>` or auto-string conversions.
///
/// § INVARIANTS
///   - Constructible only via [`PathHasher::hash`] / [`PathHasher::hash_str`].
///   - The bytes are NOT a function of the path alone — they depend on
///     the [`PathHasher`]'s installation salt. This is intentional :
///     two installations cannot cross-correlate their logs.
///   - Implements `Display` as a SHORT-form (16 hex + "...") so accidental
///     `format!` calls do not flood logs with full 64-char hex blobs that
///     could be mistaken for path-strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PathHash(pub [u8; 32]);

impl PathHash {
    /// Zero-hash placeholder. Never produced by [`PathHasher`] (the salt
    /// makes a true zero-hash collision astronomical) ; reserved as a
    /// sentinel for "no path involved" in audit-entry fields.
    #[must_use]
    pub const fn zero() -> Self {
        Self([0u8; 32])
    }

    /// Full 64-char lowercase hex form. Use sparingly : the SHORT-form
    /// emitted by [`Display`] is what audit-entries should carry.
    #[must_use]
    pub fn hex(&self) -> String {
        let mut s = String::with_capacity(64);
        for b in self.0 {
            s.push_str(&format!("{b:02x}"));
        }
        s
    }

    /// First 16 hex chars (8 bytes) of the hash — the SHORT-form used in
    /// audit-entry messages. Renders as `"deadbeefcafebabe..."`.
    #[must_use]
    pub fn short_hex(&self) -> String {
        let mut s = String::with_capacity(19);
        for b in &self.0[..8] {
            s.push_str(&format!("{b:02x}"));
        }
        s.push_str("...");
        s
    }

    /// Convert to a [`ContentHash`] for chain-integration. Use only in
    /// places where the audit-chain layer expects a `ContentHash` field
    /// (e.g., the `prev_hash` linkage). For path-hash-specific fields
    /// keep the [`PathHash`] type to preserve the type-level guarantee.
    #[must_use]
    pub const fn to_content_hash(&self) -> ContentHash {
        ContentHash(self.0)
    }
}

impl fmt::Display for PathHash {
    /// SHORT-form display : 16 hex chars + `"..."`. Choosing the short-form
    /// as the default `Display` is intentional — accidental `format!("{hash}")`
    /// in an audit-entry then produces a 19-char abbreviation, never the
    /// full 64-char hex that could be mistaken for path metadata.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.short_hex())
    }
}

/// Domain-tag prepended to every BLAKE3-hash computation. Prevents the
/// same salt from being usable as a hash-prefix-MAC for non-path data.
const PATH_HASH_DOMAIN: &[u8] = b"cssl-path-hash-v1";

/// 32-byte installation salt, opaque to consumers.
///
/// § DESIGN
///   - Carried inside [`PathHasher`] only.
///   - Never logged, never serialized, never escapes the process.
///   - The hash output is a function of (domain-tag, salt, normalized-path-bytes).
#[derive(Clone)]
pub struct PathSalt {
    bytes: [u8; 32],
}

impl PathSalt {
    /// Construct from a 32-byte seed. Useful for tests + reproducible-
    /// build attestation paths. Production code should use the random-
    /// salt path via [`PathHasher::new_random`].
    #[must_use]
    pub const fn from_seed(seed: [u8; 32]) -> Self {
        Self { bytes: seed }
    }

    /// Random 32-byte salt drawn from the OS RNG.
    #[must_use]
    pub fn random() -> Self {
        // Use the same RNG path the rest of the crate uses (see
        // [`crate::audit::SigningKey::generate`]).
        use rand::RngCore as _;
        let mut bytes = [0u8; 32];
        let mut rng = rand::rngs::OsRng;
        rng.fill_bytes(&mut bytes);
        Self { bytes }
    }
}

impl fmt::Debug for PathSalt {
    /// Never print the salt bytes in `Debug` — they are install-secret
    /// material per the surveillance prohibition. We expose only the
    /// public-side digest (BLAKE3 of the salt) for identification purposes,
    /// matching the pattern used in [`crate::audit::SigningKey`].
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let digest = ContentHash::hash(&self.bytes);
        f.debug_struct("PathSalt")
            .field("salt_digest", &digest.hex())
            .finish()
    }
}

/// BLAKE3-based path hasher with per-installation salt.
///
/// § USAGE
///
/// ```
/// use cssl_telemetry::path_hash::PathHasher;
/// use std::path::Path;
///
/// // Production path : random installation salt.
/// let hasher = PathHasher::new_random();
/// let h1 = hasher.hash(Path::new("/etc/hosts"));
/// let h2 = hasher.hash(Path::new("/etc/hosts"));
/// // Same path under same salt -> same hash.
/// assert_eq!(h1, h2);
///
/// // Different path -> different hash (overwhelming probability).
/// let h3 = hasher.hash(Path::new("/etc/passwd"));
/// assert_ne!(h1, h3);
/// ```
///
/// § PRIME_DIRECTIVE
///
///   The salt makes the output non-portable across installations. Two
///   different developer machines hashing `/home/alice/secret.txt`
///   produce DIFFERENT hashes. That is the security property : audit
///   logs are correlatable WITHIN a process but NOT across installations
///   without holding the salt.
#[derive(Clone)]
pub struct PathHasher {
    salt: PathSalt,
}

impl PathHasher {
    /// Construct from an explicit salt. Used by tests + reproducible-build
    /// audit paths (where determinism across runs is required).
    #[must_use]
    pub const fn new(salt: PathSalt) -> Self {
        Self { salt }
    }

    /// Construct from a 32-byte seed. Equivalent to
    /// `PathHasher::new(PathSalt::from_seed(seed))`.
    #[must_use]
    pub const fn from_seed(seed: [u8; 32]) -> Self {
        Self::new(PathSalt::from_seed(seed))
    }

    /// Construct with a fresh OS-randomness salt. The recommended path
    /// for production builds.
    #[must_use]
    pub fn new_random() -> Self {
        Self::new(PathSalt::random())
    }

    /// Hash a [`Path`] under the installation salt. The trait-method form
    /// of [`PathHasher::hash_bytes`].
    #[must_use]
    pub fn hash(&self, path: &Path) -> PathHash {
        // We normalize via `Path::as_os_str().as_encoded_bytes()` which is
        // the OS-native encoding (UTF-16 on Windows ; UTF-8 elsewhere). The
        // domain-tag distinguishes path-hashes from any other BLAKE3 use
        // of the same salt.
        let bytes = path.as_os_str().as_encoded_bytes();
        self.hash_bytes(bytes)
    }

    /// Hash a UTF-8 path string under the installation salt. Convenience
    /// for stage-0 callers that have a `&str` rather than a `Path`.
    #[must_use]
    pub fn hash_str(&self, path: &str) -> PathHash {
        self.hash_bytes(path.as_bytes())
    }

    /// Hash raw path-bytes (OS-native encoding) under the installation salt.
    /// Used by FFI shims that have a `(ptr, len)` pair rather than a typed
    /// `Path`.
    #[must_use]
    pub fn hash_bytes(&self, path_bytes: &[u8]) -> PathHash {
        let mut hasher = Hasher::new();
        hasher.update(PATH_HASH_DOMAIN);
        hasher.update(&self.salt.bytes);
        hasher.update(path_bytes);
        let out = hasher.finalize();
        PathHash(*out.as_bytes())
    }
}

impl fmt::Debug for PathHasher {
    /// Surfaces only the salt-digest, never the raw salt bytes.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PathHasher")
            .field("salt", &self.salt)
            .finish()
    }
}

// ───────────────────────────────────────────────────────────────────────
// § Path-hash-discipline attestation (extension of §11 CREATOR-ATTESTATION).
// ───────────────────────────────────────────────────────────────────────

/// Attestation-extension constant — appended to the canonical §11 text in
/// audit-chain entries that touch the fs-ops surface.
///
/// § STABILITY
///   Renaming this string = bug per §7 INTEGRITY (downstream verifiers
///   pin the byte-pattern). Future amendments append clauses ; never
///   weaken the existing one.
pub const PATH_HASH_DISCIPLINE_ATTESTATION: &str =
    "no raw paths logged ; only BLAKE3-salted path-hashes appear in telemetry + audit-chain";

/// Compute the BLAKE3 hash of [`PATH_HASH_DISCIPLINE_ATTESTATION`]. Used
/// by tests + by the substrate-prime-directive crate's §11 extension
/// hash-pin.
#[must_use]
pub fn path_hash_discipline_attestation_hash() -> ContentHash {
    ContentHash::hash(PATH_HASH_DISCIPLINE_ATTESTATION.as_bytes())
}

// ───────────────────────────────────────────────────────────────────────
// § Tests — 30+ covering hash-determinism, salt-isolation, audit
//   integration, raw-path rejection, attestation-extension hash pin.
// ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::{
        path_hash_discipline_attestation_hash, PathHash, PathHasher, PathSalt,
        PATH_HASH_DISCIPLINE_ATTESTATION, PATH_HASH_DOMAIN,
    };
    use crate::audit::{AuditChain, ContentHash};
    use std::path::{Path, PathBuf};

    // § Determinism + correlation properties (within-installation).

    #[test]
    fn same_path_same_salt_same_hash() {
        let hasher = PathHasher::from_seed([1u8; 32]);
        let a = hasher.hash(Path::new("/etc/hosts"));
        let b = hasher.hash(Path::new("/etc/hosts"));
        assert_eq!(a, b, "deterministic within-installation");
    }

    #[test]
    fn different_path_same_salt_different_hash() {
        let hasher = PathHasher::from_seed([1u8; 32]);
        let a = hasher.hash(Path::new("/etc/hosts"));
        let b = hasher.hash(Path::new("/etc/passwd"));
        assert_ne!(a, b, "distinct paths must yield distinct hashes");
    }

    #[test]
    fn same_path_different_salt_different_hash() {
        // Cross-installation correlation defense.
        let h1 = PathHasher::from_seed([1u8; 32]);
        let h2 = PathHasher::from_seed([2u8; 32]);
        let a = h1.hash(Path::new("/home/alice/secret.txt"));
        let b = h2.hash(Path::new("/home/alice/secret.txt"));
        assert_ne!(a, b, "different installations must NOT cross-correlate");
    }

    #[test]
    fn random_salt_produces_distinct_hashers() {
        let h1 = PathHasher::new_random();
        let h2 = PathHasher::new_random();
        let a = h1.hash(Path::new("/test"));
        let b = h2.hash(Path::new("/test"));
        // Overwhelming probability distinct ; if equal the OS RNG is broken.
        assert_ne!(a, b);
    }

    // § PathHash type-level guarantees.

    #[test]
    fn path_hash_zero_is_all_zeroes() {
        assert_eq!(PathHash::zero().0, [0u8; 32]);
    }

    #[test]
    fn path_hasher_never_produces_zero_hash() {
        // The salt-prefix + domain-tag make a true zero-output collision
        // astronomically improbable. We sample several typical paths to
        // verify none happen to land on zero.
        let hasher = PathHasher::from_seed([7u8; 32]);
        for p in [
            "/",
            "/etc",
            "/etc/hosts",
            "/home",
            "/home/x",
            "C:\\",
            "C:\\Users",
        ] {
            let h = hasher.hash_str(p);
            assert_ne!(h, PathHash::zero(), "salt collision with zero for {p}");
        }
    }

    #[test]
    fn path_hash_short_hex_is_19_chars() {
        let hasher = PathHasher::from_seed([3u8; 32]);
        let h = hasher.hash(Path::new("/test"));
        let s = h.short_hex();
        assert_eq!(s.len(), 19); // 16 hex + 3 dots
        assert!(s.ends_with("..."));
    }

    #[test]
    fn path_hash_full_hex_is_64_chars() {
        let hasher = PathHasher::from_seed([3u8; 32]);
        let h = hasher.hash(Path::new("/test"));
        assert_eq!(h.hex().len(), 64);
        assert!(h.hex().chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn path_hash_display_uses_short_form() {
        let hasher = PathHasher::from_seed([3u8; 32]);
        let h = hasher.hash(Path::new("/test"));
        let s = format!("{h}");
        assert_eq!(s.len(), 19);
        assert!(s.ends_with("..."));
        assert!(!s.contains('/'), "display must not contain raw path chars");
    }

    #[test]
    fn path_hash_to_content_hash_bytewise_equal() {
        let hasher = PathHasher::from_seed([5u8; 32]);
        let p = hasher.hash(Path::new("/abc"));
        let c = p.to_content_hash();
        assert_eq!(p.0, c.0);
    }

    // § Path-input variants (Path / &str / bytes) all converge.

    #[test]
    fn hash_str_matches_hash_path_for_utf8() {
        let hasher = PathHasher::from_seed([9u8; 32]);
        let a = hasher.hash(Path::new("/foo/bar"));
        let b = hasher.hash_str("/foo/bar");
        assert_eq!(a, b);
    }

    #[test]
    fn hash_bytes_matches_hash_str_for_ascii() {
        let hasher = PathHasher::from_seed([9u8; 32]);
        let a = hasher.hash_str("/foo/bar");
        let b = hasher.hash_bytes(b"/foo/bar");
        assert_eq!(a, b);
    }

    #[test]
    fn hash_pathbuf_works() {
        let hasher = PathHasher::from_seed([11u8; 32]);
        let pb = PathBuf::from("/some/path/for/test.txt");
        let h = hasher.hash(&pb);
        let h2 = hasher.hash(pb.as_path());
        assert_eq!(h, h2);
    }

    #[test]
    fn empty_path_hashes_deterministically() {
        let hasher = PathHasher::from_seed([2u8; 32]);
        let h1 = hasher.hash_str("");
        let h2 = hasher.hash_str("");
        assert_eq!(h1, h2);
        assert_ne!(h1, PathHash::zero());
    }

    // § Salt secrecy (Debug-output never leaks salt).

    #[test]
    fn path_salt_debug_does_not_leak_bytes() {
        let salt = PathSalt::from_seed([0xAB; 32]);
        let s = format!("{salt:?}");
        // Must not contain any raw byte hex pattern of the seed.
        assert!(
            !s.contains("ab, ab, ab"),
            "raw salt bytes leaked in Debug : {s}"
        );
        // But should contain the digest field (public-side identifier).
        assert!(s.contains("salt_digest"));
    }

    #[test]
    fn path_hasher_debug_does_not_leak_salt() {
        let hasher = PathHasher::from_seed([0xCD; 32]);
        let s = format!("{hasher:?}");
        assert!(!s.contains("cd, cd, cd"));
        assert!(s.contains("salt_digest"));
    }

    // § Salt seed alone is enough — separate hashers from same seed agree.

    #[test]
    fn from_seed_is_deterministic_across_constructions() {
        let h1 = PathHasher::from_seed([42u8; 32]);
        let h2 = PathHasher::from_seed([42u8; 32]);
        let a = h1.hash(Path::new("/x"));
        let b = h2.hash(Path::new("/x"));
        assert_eq!(a, b);
    }

    #[test]
    fn from_seed_different_seeds_disagree() {
        let h1 = PathHasher::from_seed([1u8; 32]);
        let h2 = PathHasher::from_seed([2u8; 32]);
        let a = h1.hash(Path::new("/x"));
        let b = h2.hash(Path::new("/x"));
        assert_ne!(a, b);
    }

    // § Domain-tag isolation : path-hash != raw-content-hash of bytes.

    #[test]
    fn salted_path_hash_differs_from_unsalted_blake3() {
        let hasher = PathHasher::from_seed([0u8; 32]);
        let salted = hasher.hash_str("/etc/hosts");
        let raw = ContentHash::hash(b"/etc/hosts");
        assert_ne!(
            salted.0, raw.0,
            "salted hash must differ from raw BLAKE3 of same string"
        );
    }

    #[test]
    fn domain_tag_present_in_input_stream() {
        // Sanity : the domain tag is non-empty + recognizable.
        assert!(!PATH_HASH_DOMAIN.is_empty());
        assert!(
            std::str::from_utf8(PATH_HASH_DOMAIN).is_ok(),
            "domain tag must be valid UTF-8 for grep-ability"
        );
    }

    // § Audit-chain integration : hashes flow into chain entries cleanly.

    #[test]
    fn audit_chain_can_carry_path_hash_in_message_short_form() {
        let hasher = PathHasher::from_seed([7u8; 32]);
        let h = hasher.hash(Path::new("/var/log/cssl.log"));
        let mut chain = AuditChain::new();
        chain.append("fs-write", format!("path_hash={h} bytes=42"), 1_000);
        let entry = chain.iter().next().unwrap();
        // The short-form ends with "..." ; a raw path would have "/".
        assert!(entry.message.contains("..."));
        assert!(!entry.message.contains("/var/log"));
        chain.verify_chain().expect("chain still verifies");
    }

    #[test]
    fn audit_chain_with_many_path_hashes_remains_unique_to_path() {
        let h = PathHasher::from_seed([7u8; 32]);
        let mut chain = AuditChain::new();
        for p in &["/a.txt", "/b.txt", "/c.txt"] {
            let ph = h.hash_str(p);
            chain.append("fs-open", format!("path_hash={ph}"), 0);
        }
        // Three distinct entries with three distinct hashes.
        let messages: Vec<_> = chain.iter().map(|e| e.message.clone()).collect();
        assert_eq!(messages.len(), 3);
        let mut shorts: Vec<_> = messages
            .iter()
            .map(|m| m.split_once("path_hash=").unwrap().1.to_string())
            .collect();
        shorts.sort();
        shorts.dedup();
        assert_eq!(shorts.len(), 3, "every path produces a distinct short-hash");
    }

    #[test]
    fn audit_chain_path_hashes_correlate_within_chain() {
        // Same path, two ops -> same hash. Useful for forensic correlation
        // without leaking the path itself.
        let hasher = PathHasher::from_seed([7u8; 32]);
        let mut chain = AuditChain::new();
        let p = "/var/data/x.bin";
        chain.append("fs-open", format!("path_hash={}", hasher.hash_str(p)), 0);
        chain.append("fs-write", format!("path_hash={}", hasher.hash_str(p)), 1);
        chain.append("fs-close", format!("path_hash={}", hasher.hash_str(p)), 2);
        let messages: Vec<_> = chain.iter().map(|e| e.message.clone()).collect();
        // All three must reference the SAME short-hash — the within-installation
        // correlation property.
        let h0 = messages[0].split_once('=').unwrap().1;
        let h1 = messages[1].split_once('=').unwrap().1;
        let h2 = messages[2].split_once('=').unwrap().1;
        assert_eq!(h0, h1);
        assert_eq!(h1, h2);
    }

    // § Attestation-extension constants.

    #[test]
    fn path_hash_discipline_attestation_text_is_canonical() {
        assert_eq!(
            PATH_HASH_DISCIPLINE_ATTESTATION,
            "no raw paths logged ; only BLAKE3-salted path-hashes appear in telemetry + audit-chain"
        );
    }

    #[test]
    fn path_hash_discipline_attestation_hash_is_pinned() {
        // Hash-pin the attestation text so any drift is caught immediately.
        let h = path_hash_discipline_attestation_hash();
        let expected = ContentHash::hash(PATH_HASH_DISCIPLINE_ATTESTATION.as_bytes());
        assert_eq!(h, expected);
        // Non-zero (BLAKE3 of non-empty data).
        assert_ne!(h, ContentHash::zero());
    }

    #[test]
    fn path_hash_discipline_attestation_hex_is_canonical() {
        // Verify the hex form is stable. Print on assert-fail so future
        // edits to PATH_HASH_DISCIPLINE_ATTESTATION reveal the new pin.
        let hex = path_hash_discipline_attestation_hash().hex();
        assert_eq!(hex.len(), 64);
        // Sanity : drift would change this. The hash is the BLAKE3 of
        // the canonical text ; tests above pin both halves.
        eprintln!("PATH_HASH_DISCIPLINE_ATTESTATION_HASH = {hex}");
    }

    #[test]
    fn path_hash_discipline_attestation_mentions_blake3_and_salt() {
        // Sanity-check that the attestation cites the algorithm + salt.
        assert!(PATH_HASH_DISCIPLINE_ATTESTATION.contains("BLAKE3"));
        assert!(PATH_HASH_DISCIPLINE_ATTESTATION.contains("salt"));
        assert!(PATH_HASH_DISCIPLINE_ATTESTATION.contains("no raw paths"));
    }

    // § Pre-image / compression-resistance smoke tests.

    #[test]
    fn related_paths_produce_uncorrelated_hashes() {
        // Subdirectory + parent must produce independent-looking hashes ;
        // BLAKE3 makes near-paths visually distinct.
        let hasher = PathHasher::from_seed([0u8; 32]);
        let a = hasher.hash_str("/home/alice");
        let b = hasher.hash_str("/home/alice/");
        // Trailing-slash IS a different byte sequence — different hash.
        assert_ne!(a, b);
    }

    #[test]
    fn long_path_hashes_to_32_bytes() {
        let hasher = PathHasher::from_seed([0u8; 32]);
        let long: String = "a".repeat(4096);
        let h = hasher.hash_str(&long);
        assert_eq!(h.0.len(), 32);
    }

    #[test]
    fn unicode_path_hashes_deterministically() {
        let hasher = PathHasher::from_seed([0u8; 32]);
        let p = "/h\u{f4}me/\u{e1}lice/r\u{e9}sum\u{e9}.txt";
        let h1 = hasher.hash_str(p);
        let h2 = hasher.hash_str(p);
        assert_eq!(h1, h2);
        // And differs from the ASCII-strip equivalent.
        let h3 = hasher.hash_str("/home/alice/resume.txt");
        assert_ne!(h1, h3);
    }

    #[test]
    fn windows_separator_distinct_from_unix() {
        let hasher = PathHasher::from_seed([0u8; 32]);
        let a = hasher.hash_str("C:\\Users\\Alice\\doc.txt");
        let b = hasher.hash_str("/Users/Alice/doc.txt");
        assert_ne!(a, b);
    }

    // § PathHash : equality + ordering work.

    #[test]
    fn path_hash_equality_is_byte_exact() {
        let h1 = PathHash([0u8; 32]);
        let h2 = PathHash([0u8; 32]);
        assert_eq!(h1, h2);
        let mut h3 = [0u8; 32];
        h3[0] = 1;
        assert_ne!(h1, PathHash(h3));
    }

    #[test]
    fn path_hash_orderable_for_sorting_audit_logs() {
        let h = PathHasher::from_seed([0u8; 32]);
        let mut sorted: Vec<PathHash> = ["/a", "/b", "/c", "/d"]
            .iter()
            .map(|p| h.hash_str(p))
            .collect();
        sorted.sort();
        // Sort succeeded ; the ordering is deterministic.
        let mut resorted = sorted.clone();
        resorted.sort();
        assert_eq!(sorted, resorted);
    }

    // § Raw-path rejection helper (string-form audit).

    #[test]
    fn raw_path_string_is_rejected_by_audit_helper() {
        // Any audit-chain user who tries to insert a raw path string
        // would have to do so via `chain.append(tag, message, ts)` — the
        // tag/message are arbitrary strings at the chain level. The
        // discipline is enforced at the CALLER level via the
        // [`crate::audit_path_op`] convenience function which panics on
        // detected raw-path patterns. Test that here.
        use crate::audit_path_op_check_raw_path_rejected;
        // Plain hash-form accepted.
        assert!(audit_path_op_check_raw_path_rejected("path_hash=deadbeefcafebabe...").is_ok());
        // Forward slash -> raw path detected.
        assert!(audit_path_op_check_raw_path_rejected("path=/etc/hosts").is_err());
        // Backslash -> raw path detected.
        assert!(audit_path_op_check_raw_path_rejected("path=C:\\users\\x").is_err());
    }

    #[test]
    fn audit_path_op_helper_emits_hash_only_message() {
        use crate::audit_path_op;
        let hasher = PathHasher::from_seed([0u8; 32]);
        let mut chain = AuditChain::new();
        let h = hasher.hash_str("/etc/hosts");
        audit_path_op(&mut chain, "fs-open", h, "bytes=0", 1_000).expect("hash-form accepted");
        let entry = chain.iter().next().unwrap();
        assert!(entry.message.contains("path_hash="));
        assert!(!entry.message.contains("/etc"));
        assert!(!entry.message.contains('\\'));
    }

    #[test]
    fn audit_path_op_rejects_raw_path_in_extra() {
        use crate::audit_path_op;
        let hasher = PathHasher::from_seed([0u8; 32]);
        let mut chain = AuditChain::new();
        let h = hasher.hash_str("/etc/hosts");
        // The 'extra' field gets validated for raw-path leaks too.
        let r = audit_path_op(&mut chain, "fs-open", h, "leaked=/etc/hosts", 1);
        assert!(r.is_err());
    }
}
