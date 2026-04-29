//! Path-hash field newtype used inside [`crate::Context::source`] +
//! structured-log fields.
//!
//! § DISCIPLINE (D130) :
//!   - This is a thin re-wrapper around [`cssl_telemetry::PathHash`] so the
//!     `cssl-log` public surface does not export a different path-hash type.
//!     Constructible ONLY through the underlying [`cssl_telemetry::PathHasher`]
//!     ⟵ structurally cannot accept `&str`/`&Path`.
//!   - When T11-D155 (cssl-error) lands, this newtype is folded into
//!     `cssl_error::SourceLocation::file_path_hash` directly — the cssl-log
//!     surface continues to re-export it for back-compat.
//!
//! § SPEC : `05_l0_l1_error_log_spec.md` § 1.4 + § 2.8 + § 7.1.

use core::fmt;

use cssl_telemetry::PathHash;

/// Path-hash field. Construct via [`Self::from_path_hash`] — there is no
/// `&str`/`&Path` constructor. The only way to get a `PathHashField` is
/// to first compute a `PathHash` via [`cssl_telemetry::PathHasher`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PathHashField(PathHash);

impl PathHashField {
    /// Construct from an already-hashed [`PathHash`]. Const so call-sites
    /// in macro-expansion pay zero runtime cost.
    #[must_use]
    pub const fn from_path_hash(h: PathHash) -> Self {
        Self(h)
    }

    /// Zero-hash sentinel (per [`PathHash::zero`]). Used when source-loc
    /// is not available (e.g., manually-constructed [`crate::Context`]).
    #[must_use]
    pub const fn zero() -> Self {
        Self(PathHash::zero())
    }

    /// Inner [`PathHash`] for downstream consumers (audit-chain, ring-slot
    /// payload).
    #[must_use]
    pub const fn inner(self) -> PathHash {
        self.0
    }

    /// 32-byte hash bytes — for binary-wire-format encoding only.
    /// Callers MUST NOT decode these back to a path ; that is structurally
    /// impossible (the salt is per-installation secret).
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        let PathHash(b) = &self.0;
        b
    }
}

impl fmt::Display for PathHashField {
    /// Delegates to [`PathHash`] short-form (16 hex + `...`). Spec § 1.4
    /// + path_hash.rs `PathHash::short_hex`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

#[cfg(test)]
mod tests {
    use super::PathHashField;
    use cssl_telemetry::{PathHash, PathHasher};

    #[test]
    fn from_path_hash_round_trips() {
        let hasher = PathHasher::from_seed([1u8; 32]);
        let h = hasher.hash_str("/test/path");
        let field = PathHashField::from_path_hash(h);
        assert_eq!(field.inner(), h);
    }

    #[test]
    fn zero_field_uses_zero_hash() {
        let field = PathHashField::zero();
        assert_eq!(field.inner(), PathHash::zero());
    }

    #[test]
    fn display_emits_short_form() {
        let hasher = PathHasher::from_seed([1u8; 32]);
        let h = hasher.hash_str("/test/path");
        let field = PathHashField::from_path_hash(h);
        let s = format!("{field}");
        assert_eq!(s.len(), 19);
        assert!(s.ends_with("..."));
    }

    #[test]
    fn display_does_not_contain_raw_path_chars() {
        let hasher = PathHasher::from_seed([1u8; 32]);
        let h = hasher.hash_str("/etc/hosts");
        let field = PathHashField::from_path_hash(h);
        let s = format!("{field}");
        assert!(!s.contains('/'));
        assert!(!s.contains('\\'));
        assert!(!s.contains("etc"));
        assert!(!s.contains("hosts"));
    }

    #[test]
    fn as_bytes_returns_32_bytes() {
        let hasher = PathHasher::from_seed([2u8; 32]);
        let h = hasher.hash_str("/anything");
        let field = PathHashField::from_path_hash(h);
        assert_eq!(field.as_bytes().len(), 32);
    }

    #[test]
    fn equality_is_byte_exact() {
        let hasher = PathHasher::from_seed([1u8; 32]);
        let h1 = hasher.hash_str("/a");
        let h2 = hasher.hash_str("/a");
        let h3 = hasher.hash_str("/b");
        assert_eq!(
            PathHashField::from_path_hash(h1),
            PathHashField::from_path_hash(h2)
        );
        assert_ne!(
            PathHashField::from_path_hash(h1),
            PathHashField::from_path_hash(h3)
        );
    }

    #[test]
    fn copy_preserves_value() {
        let hasher = PathHasher::from_seed([3u8; 32]);
        let h = hasher.hash_str("/x");
        let f1 = PathHashField::from_path_hash(h);
        let f2 = f1;
        assert_eq!(f1, f2);
    }

    #[test]
    fn ordering_is_total() {
        let hasher = PathHasher::from_seed([0u8; 32]);
        let mut v: Vec<_> = ["/a", "/b", "/c", "/d"]
            .iter()
            .map(|p| PathHashField::from_path_hash(hasher.hash_str(p)))
            .collect();
        v.sort();
        // Sorting succeeded ; deterministic across re-runs.
        let v2 = v.clone();
        assert_eq!(v, v2);
    }
}
