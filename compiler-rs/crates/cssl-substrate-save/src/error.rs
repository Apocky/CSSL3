//! § cssl-substrate-save — typed save / load failure modes.
//!
//! Every failure surfaces through [`SaveError`] / [`LoadError`] rather than a
//! silent panic. Per the PRIME-DIRECTIVE attestation block in [`crate`]'s
//! root doc-block, attestation-mismatch is a HARD-FAIL — we never silently
//! corrupt state.
//!
//! § STABILITY
//!   The error variants are STABLE from S8-H5 forward. Renaming a variant
//!   requires a major-version bump (mirrors the cssl-rt `IoError` invariant
//!   from T11-D76).

use thiserror::Error;

/// Failure modes for [`crate::save`].
#[derive(Debug, Error)]
pub enum SaveError {
    /// The host file-system rejected the save : open / write / close error.
    /// Carries the kernel error message for human diagnosis. Per the
    /// path-hash-only logging discipline, the error message NEVER includes
    /// the cleartext path ; only the BLAKE3-prefix is included for
    /// correlation with [`crate::path_hash`] in the host audit-sink.
    #[error("filesystem error during save (path-hash {path_hash_prefix:.16}…) : {source}")]
    FsError {
        /// Hex-prefix (first 16 hex chars = 8 bytes) of the BLAKE3 path-hash.
        path_hash_prefix: String,
        /// Underlying `std::io::Error`.
        #[source]
        source: std::io::Error,
    },

    /// The serialized blob was longer than `u64::MAX − header overhead`. In
    /// practice this is unreachable on any current platform but is encoded
    /// so the trailer-offset arithmetic is total.
    #[error("save blob too large for the format ({omega_len} + {log_len} bytes)")]
    BlobTooLarge {
        /// Ω-tensor blob byte-length.
        omega_len: u64,
        /// Replay-log blob byte-length.
        log_len: u64,
    },
}

/// Failure modes for [`crate::load`].
#[derive(Debug, Error)]
pub enum LoadError {
    /// The save-file was shorter than the minimum-viable header (magic +
    /// version + length-headers + trailer-offset).
    #[error("save-file truncated : {0} bytes < minimum {1}")]
    Truncated(u64, u64),

    /// The first 8 bytes did not match `b"CSSLSAVE"`.
    #[error("not a CSSLSAVE file (magic mismatch)")]
    BadMagic,

    /// The version field is not a version this build understands. Per
    /// `specs/30_SUBSTRATE.csl § DEFERRED`, version-migration is deferred ;
    /// S8-H5 only handles the current [`crate::FORMAT_VERSION`].
    #[error(
        "unsupported save-format version : got {got}, expected {expected} \
         (migration is deferred to a later slice)"
    )]
    UnsupportedVersion { got: u32, expected: u32 },

    /// The Ω-tensor blob length exceeds the file's remaining bytes — the
    /// header was truncated or hand-edited.
    #[error("Ω-tensor blob length {claimed} exceeds remaining file bytes {remaining}")]
    OmegaBlobOverflow { claimed: u64, remaining: u64 },

    /// The replay-log blob length exceeds the file's remaining bytes.
    #[error("replay-log blob length {claimed} exceeds remaining file bytes {remaining}")]
    ReplayBlobOverflow { claimed: u64, remaining: u64 },

    /// The trailer-offset doesn't point to the right place — the file is
    /// internally inconsistent.
    #[error("trailer-offset mismatch : got {got}, expected {expected}")]
    TrailerOffsetMismatch { got: u64, expected: u64 },

    /// The stored attestation hash does not match the freshly-computed
    /// BLAKE3 over the payload. **HARD-FAIL** per
    /// `specs/30_SUBSTRATE.csl § Ω-TENSOR-LEVEL` — a load that proceeded
    /// despite this would silently corrupt state.
    #[error("attestation mismatch (load REFUSED to silently corrupt state)")]
    AttestationMismatch,

    /// The Ω-tensor blob has a malformed type-tag (not in the
    /// canonical OMEGA_TYPE_TAG_* set).
    #[error("Ω-tensor blob has unknown type-tag {0}")]
    UnknownTypeTag(u8),

    /// The Ω-tensor blob announces a rank that, after parsing, does not
    /// match the number of shape dims actually present.
    #[error("Ω-tensor blob rank/shape mismatch : header says {claimed_rank} dims, body has {actual_dims}")]
    RankShapeMismatch { claimed_rank: u32, actual_dims: u32 },

    /// The Ω-tensor blob's data section is shorter than `product(shape) * dtype_size`.
    #[error("Ω-tensor blob data underflow : claimed {claimed_bytes}, actual {actual_bytes}")]
    OmegaDataUnderflow {
        claimed_bytes: u64,
        actual_bytes: u64,
    },

    /// The replay-log blob has an unknown event-tag byte.
    #[error("replay-log blob has unknown event-tag {0}")]
    UnknownEventTag(u8),

    /// Reading the file from disk failed.
    #[error("filesystem error during load (path-hash {path_hash_prefix:.16}…) : {source}")]
    FsError {
        /// Hex-prefix of the BLAKE3 path-hash.
        path_hash_prefix: String,
        /// Underlying `std::io::Error`.
        #[source]
        source: std::io::Error,
    },
}

// `std::io::Error` is not `PartialEq` so we can't auto-derive `PartialEq` on
// `LoadError`. Hand-rolled equality compares pure-value variants by-value
// and treats `FsError` as never-equal-to-anything (incl. itself) — callers
// that need to distinguish FsError variants should match-on-source instead
// of `==`. This keeps `assert_eq!(load_err, LoadError::BadMagic)` ergonomic
// for the pure-value cases that dominate testing.
impl PartialEq for LoadError {
    fn eq(&self, other: &Self) -> bool {
        use LoadError::{
            AttestationMismatch, BadMagic, OmegaBlobOverflow, OmegaDataUnderflow,
            RankShapeMismatch, ReplayBlobOverflow, TrailerOffsetMismatch, Truncated,
            UnknownEventTag, UnknownTypeTag, UnsupportedVersion,
        };
        match (self, other) {
            (BadMagic, BadMagic) | (AttestationMismatch, AttestationMismatch) => true,
            (Truncated(a1, a2), Truncated(b1, b2)) => a1 == b1 && a2 == b2,
            (
                UnsupportedVersion {
                    got: g1,
                    expected: e1,
                },
                UnsupportedVersion {
                    got: g2,
                    expected: e2,
                },
            ) => g1 == g2 && e1 == e2,
            (
                OmegaBlobOverflow {
                    claimed: c1,
                    remaining: r1,
                },
                OmegaBlobOverflow {
                    claimed: c2,
                    remaining: r2,
                },
            )
            | (
                ReplayBlobOverflow {
                    claimed: c1,
                    remaining: r1,
                },
                ReplayBlobOverflow {
                    claimed: c2,
                    remaining: r2,
                },
            ) => c1 == c2 && r1 == r2,
            (
                TrailerOffsetMismatch {
                    got: g1,
                    expected: e1,
                },
                TrailerOffsetMismatch {
                    got: g2,
                    expected: e2,
                },
            ) => g1 == g2 && e1 == e2,
            (UnknownTypeTag(a), UnknownTypeTag(b)) | (UnknownEventTag(a), UnknownEventTag(b)) => {
                a == b
            }
            (
                RankShapeMismatch {
                    claimed_rank: cr1,
                    actual_dims: ad1,
                },
                RankShapeMismatch {
                    claimed_rank: cr2,
                    actual_dims: ad2,
                },
            ) => cr1 == cr2 && ad1 == ad2,
            (
                OmegaDataUnderflow {
                    claimed_bytes: c1,
                    actual_bytes: a1,
                },
                OmegaDataUnderflow {
                    claimed_bytes: c2,
                    actual_bytes: a2,
                },
            ) => c1 == c2 && a1 == a2,
            // FsError never compares equal — match-on-source instead.
            _ => false,
        }
    }
}

impl Eq for LoadError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_error_displays_path_hash_prefix_not_path() {
        // Construct a SaveError::FsError manually ; verify `Display` does NOT
        // contain the string "secret-path-the-user-shouldnt-see".
        let secret_phrase = "secret-path-the-user-shouldnt-see";
        let err = SaveError::FsError {
            path_hash_prefix: "deadbeefcafe1234".to_string(),
            source: std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied"),
        };
        let s = format!("{err}");
        assert!(!s.contains(secret_phrase));
        assert!(s.contains("deadbeefcafe1234"));
    }

    #[test]
    fn load_error_attestation_mismatch_is_distinct_from_bad_magic() {
        // Sanity : these two errors are not accidentally aliased.
        let a = LoadError::AttestationMismatch;
        let b = LoadError::BadMagic;
        assert_ne!(a, b);
    }

    #[test]
    fn load_error_displays_the_hard_fail_message_for_attestation_mismatch() {
        // The PRIME-DIRECTIVE alignment requires that this error's message
        // makes it clear we REFUSED rather than silently-degrading.
        let s = format!("{}", LoadError::AttestationMismatch);
        assert!(s.contains("REFUSED"));
        assert!(s.contains("silently corrupt"));
    }

    #[test]
    fn load_error_unsupported_version_is_self_explanatory() {
        let s = format!(
            "{}",
            LoadError::UnsupportedVersion {
                got: 99,
                expected: 1
            }
        );
        assert!(s.contains("99"));
        assert!(s.contains('1'));
        assert!(s.contains("migration"));
    }

    #[test]
    fn load_error_truncated_includes_byte_counts() {
        let s = format!("{}", LoadError::Truncated(4, 32));
        assert!(s.contains('4'));
        assert!(s.contains("32"));
    }

    #[test]
    fn load_error_partial_eq_works() {
        // PartialEq derivation succeeded ; FsError variant excluded by std::io::Error.
        assert_eq!(LoadError::BadMagic, LoadError::BadMagic);
        assert_ne!(LoadError::BadMagic, LoadError::AttestationMismatch);
    }
}
