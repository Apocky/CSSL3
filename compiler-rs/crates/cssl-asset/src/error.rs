//! Central error type for the CSSLv3 asset pipeline.
//!
//! § DESIGN
//!   `AssetError` mirrors the cssl-host-audio / cssl-host-net error
//!   precedent — a single sum-type that flattens parser-specific failures
//!   into a small set of variants discriminating "what went wrong".
//!   Each parser (PNG / WAV / TTF / GLTF) lifts its own internal errors
//!   into the canonical surface so consumers can match on a single enum.
//!
//! § STRUCTURED FAILURE
//!   - `Io`              — underlying I/O failed (file missing / permission /
//!                         short read)
//!   - `Truncated`       — buffer ended mid-record (corrupt or partial file)
//!   - `BadMagic`        — file signature did not match expected
//!   - `UnsupportedKind` — well-formed but uses a feature stage-0 doesn't
//!                         support (e.g. PNG paletted+alpha, WAV ADPCM)
//!   - `BadChecksum`     — CRC / Adler / per-format integrity failed
//!   - `InvalidValue`    — field out-of-range or self-inconsistent
//!   - `BudgetExceeded`  — load would exceed an active `AssetBudget`
//!   - `Watcher`         — hot-reload watcher surface failure
//!   - `Encode`          — round-trip encode failure
//!
//! § PRIME-DIRECTIVE
//!   Every error variant carries enough context for a caller to surface
//!   a clear message. No silent failures, no swallowed errors. Errors
//!   from untrusted asset files are bounded — the parser caps allocation
//!   at the buffer's reported size and refuses pathological dimensions
//!   before allocating, so a malicious asset file cannot trigger an OOM.

use thiserror::Error;

/// Errors returned by the asset pipeline.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum AssetError {
    /// Underlying I/O failed (file missing / permission denied / short read).
    #[error("asset I/O failed for `{path}`: {reason}")]
    Io {
        /// Path or logical source identifier.
        path: String,
        /// Free-form description of the underlying failure.
        reason: String,
    },

    /// Buffer ended before a record could be fully parsed.
    #[error("asset truncated at `{site}` (expected {expected} bytes, got {actual})")]
    Truncated {
        /// Where in the parser the cut-off was detected.
        site: String,
        /// Bytes the parser needed.
        expected: usize,
        /// Bytes available.
        actual: usize,
    },

    /// File signature / magic bytes did not match the expected format.
    #[error("asset bad-magic for `{format}` (saw {observed:?})")]
    BadMagic {
        /// Which format the parser was expecting.
        format: String,
        /// What the parser actually saw (first 8 bytes hex-encoded).
        observed: String,
    },

    /// Well-formed file uses a feature this parser does not support.
    #[error("asset unsupported feature in `{format}`: {detail}")]
    UnsupportedKind {
        /// Which format reported it.
        format: String,
        /// Free-form description.
        detail: String,
    },

    /// CRC / Adler / per-format integrity check failed.
    #[error("asset checksum mismatch in `{format}` at `{site}` (expected 0x{expected:08x}, got 0x{actual:08x})")]
    BadChecksum {
        /// Which format reported it.
        format: String,
        /// Where in the parser.
        site: String,
        /// Stored checksum.
        expected: u32,
        /// Computed checksum.
        actual: u32,
    },

    /// A field has an out-of-range or self-inconsistent value.
    #[error("asset invalid value in `{format}` at `{site}`: {detail}")]
    InvalidValue {
        /// Which format reported it.
        format: String,
        /// Where in the parser.
        site: String,
        /// Why it was rejected.
        detail: String,
    },

    /// Loading this asset would exceed an active `AssetBudget`.
    #[error("asset would exceed budget `{class}` (would use {would_use} bytes, cap {cap})")]
    BudgetExceeded {
        /// The budget class that overflowed.
        class: String,
        /// What the load would push usage to.
        would_use: u64,
        /// Configured cap.
        cap: u64,
    },

    /// Hot-reload watcher surface failure.
    #[error("asset watcher `{site}`: {detail}")]
    Watcher {
        /// Where the failure was detected.
        site: String,
        /// Free-form description.
        detail: String,
    },

    /// Round-trip encode (e.g. PNG re-encode) failed.
    #[error("asset encode failed for `{format}`: {detail}")]
    Encode {
        /// Which format.
        format: String,
        /// Why it failed.
        detail: String,
    },
}

impl AssetError {
    /// Build an `Io` variant.
    #[must_use]
    pub fn io(path: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::Io {
            path: path.into(),
            reason: reason.into(),
        }
    }

    /// Build a `Truncated` variant.
    #[must_use]
    pub fn truncated(site: impl Into<String>, expected: usize, actual: usize) -> Self {
        Self::Truncated {
            site: site.into(),
            expected,
            actual,
        }
    }

    /// Build a `BadMagic` variant. Renders the observed magic as hex.
    #[must_use]
    pub fn bad_magic(format: impl Into<String>, observed: &[u8]) -> Self {
        let mut s = String::with_capacity(observed.len() * 2);
        for b in observed.iter().take(8) {
            s.push_str(&format!("{b:02x}"));
        }
        Self::BadMagic {
            format: format.into(),
            observed: s,
        }
    }

    /// Build an `UnsupportedKind` variant.
    #[must_use]
    pub fn unsupported(format: impl Into<String>, detail: impl Into<String>) -> Self {
        Self::UnsupportedKind {
            format: format.into(),
            detail: detail.into(),
        }
    }

    /// Build a `BadChecksum` variant.
    #[must_use]
    pub fn bad_checksum(
        format: impl Into<String>,
        site: impl Into<String>,
        expected: u32,
        actual: u32,
    ) -> Self {
        Self::BadChecksum {
            format: format.into(),
            site: site.into(),
            expected,
            actual,
        }
    }

    /// Build an `InvalidValue` variant.
    #[must_use]
    pub fn invalid(
        format: impl Into<String>,
        site: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self::InvalidValue {
            format: format.into(),
            site: site.into(),
            detail: detail.into(),
        }
    }

    /// Build a `BudgetExceeded` variant.
    #[must_use]
    pub const fn budget(class_name: String, would_use: u64, cap: u64) -> Self {
        Self::BudgetExceeded {
            class: class_name,
            would_use,
            cap,
        }
    }

    /// Build a `Watcher` variant.
    #[must_use]
    pub fn watcher(site: impl Into<String>, detail: impl Into<String>) -> Self {
        Self::Watcher {
            site: site.into(),
            detail: detail.into(),
        }
    }

    /// Build an `Encode` variant.
    #[must_use]
    pub fn encode(format: impl Into<String>, detail: impl Into<String>) -> Self {
        Self::Encode {
            format: format.into(),
            detail: detail.into(),
        }
    }

    /// Is this error a "missing or unreadable source" (caller may want to
    /// retry / fall back to a default asset) ?
    #[must_use]
    pub const fn is_io(&self) -> bool {
        matches!(self, Self::Io { .. })
    }

    /// Is this a parser-side rejection (corrupt / unsupported / inconsistent)
    /// rather than I/O ?
    #[must_use]
    pub const fn is_parse(&self) -> bool {
        matches!(
            self,
            Self::Truncated { .. }
                | Self::BadMagic { .. }
                | Self::UnsupportedKind { .. }
                | Self::BadChecksum { .. }
                | Self::InvalidValue { .. }
        )
    }
}

/// Crate-wide `Result` alias.
pub type Result<T> = core::result::Result<T, AssetError>;

#[cfg(test)]
mod tests {
    use super::AssetError;

    #[test]
    fn io_constructor_carries_path() {
        let e = AssetError::io("level.png", "ENOENT");
        match &e {
            AssetError::Io { path, reason } => {
                assert_eq!(path, "level.png");
                assert!(reason.contains("ENOENT"));
            }
            other => panic!("wrong variant: {other:?}"),
        }
        assert!(format!("{e}").contains("level.png"));
    }

    #[test]
    fn truncated_constructor_carries_site_expected_actual() {
        let e = AssetError::truncated("PNG/IHDR", 13, 7);
        let s = format!("{e}");
        assert!(s.contains("PNG/IHDR"));
        assert!(s.contains("13"));
        assert!(s.contains("7"));
    }

    #[test]
    fn bad_magic_constructor_renders_hex() {
        let e = AssetError::bad_magic("PNG", &[0x89, 0xff, 0x00]);
        let s = format!("{e}");
        assert!(s.contains("89ff00"));
    }

    #[test]
    fn bad_magic_caps_at_8_bytes() {
        let bytes = [0u8; 32];
        let e = AssetError::bad_magic("X", &bytes);
        match e {
            AssetError::BadMagic { observed, .. } => assert_eq!(observed.len(), 16),
            _ => unreachable!(),
        }
    }

    #[test]
    fn bad_checksum_constructor_renders_hex_pair() {
        let e = AssetError::bad_checksum("PNG", "IDAT", 0xdead_beef, 0xcafe_babe);
        let s = format!("{e}");
        assert!(s.contains("0xdeadbeef"));
        assert!(s.contains("0xcafebabe"));
    }

    #[test]
    fn invalid_constructor_carries_format_and_detail() {
        let e = AssetError::invalid("WAV", "fmt chunk", "channels=0");
        let s = format!("{e}");
        assert!(s.contains("WAV"));
        assert!(s.contains("channels=0"));
    }

    #[test]
    fn budget_constructor_renders_numbers() {
        let e = AssetError::budget("texture".to_string(), 4096, 1024);
        let s = format!("{e}");
        assert!(s.contains("texture"));
        assert!(s.contains("4096"));
        assert!(s.contains("1024"));
    }

    #[test]
    fn unsupported_constructor() {
        let e = AssetError::unsupported("PNG", "16-bit channels");
        assert!(format!("{e}").contains("16-bit"));
    }

    #[test]
    fn watcher_constructor() {
        let e = AssetError::watcher("watch_path", "no os support");
        assert!(format!("{e}").contains("watch_path"));
    }

    #[test]
    fn encode_constructor() {
        let e = AssetError::encode("PNG", "color-type round-trip mismatch");
        assert!(format!("{e}").contains("round-trip"));
    }

    #[test]
    fn is_io_classification() {
        assert!(AssetError::io("x", "y").is_io());
        assert!(!AssetError::truncated("z", 1, 0).is_io());
        assert!(!AssetError::bad_magic("PNG", &[]).is_io());
    }

    #[test]
    fn is_parse_classification() {
        assert!(AssetError::truncated("z", 1, 0).is_parse());
        assert!(AssetError::bad_magic("PNG", &[]).is_parse());
        assert!(AssetError::unsupported("PNG", "x").is_parse());
        assert!(AssetError::bad_checksum("PNG", "x", 0, 1).is_parse());
        assert!(AssetError::invalid("WAV", "x", "y").is_parse());
        assert!(!AssetError::io("x", "y").is_parse());
        assert!(!AssetError::watcher("x", "y").is_parse());
    }
}
