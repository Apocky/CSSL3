//! Metric tag-set : `(TagKey, TagVal)` pairs with biometric + raw-path refusal.
//!
//! § SPEC : `_drafts/phase_j/06_l2_telemetry_spec.md` § II.1 banner +
//!         T11-D130 path-hash-only discipline + T11-D132 biometric-compile-refuse.
//!
//! § DISCIPLINE
//!   - tag-keys are `&'static str` so common key-names interned at the
//!     literal-pool ; this also makes biometric-key checking a string-match
//!     against a frozen list ([`BIOMETRIC_TAG_KEYS`]).
//!   - tag-values are `TagVal` enum variants (Static / Hashed / U64 / I64 /
//!     Bool) that carve out a deliberately-narrow surface : raw `&str` from
//!     untrusted callers cannot become a tag-value without going through
//!     [`TagVal::hashed`] which delegates to a salted hash.
//!   - the tag-list is a `SmallVec<[(TagKey, TagVal); 4]>` ; spill is treated
//!     as a discipline violation per § II.1 (LM-8 in the spec landmines table).
//!
//! § PRIME-DIRECTIVE BINDING (§1)
//!   The biometric-tag-key list mirrors `cssl-ifc::TelemetryEgress` BiometricKind
//!   — adding-a-key requires a DECISIONS-pin so the language-of-refusal stays
//!   stable across cssl-* crates. Keys NOT in the list are still rejected if
//!   their text matches `face`/`iris`/`gaze_dir`/`blink`/`heart`/`skin`/etc
//!   substrings (defense-in-depth ; runtime + compile-time both).

use core::fmt;

use smallvec::SmallVec;

use crate::error::{MetricError, MetricResult};

/// Tag-key (a `&'static str`).
///
/// The `'static` bound prevents owned-String-from-untrusted-source paths and
/// keeps tag-key memory at the literal-pool. Construction is free ; refusal
/// happens at the point a key is used to build a [`TagSet`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TagKey(pub &'static str);

impl TagKey {
    /// Construct without runtime check (literal-only).
    #[must_use]
    pub const fn new(s: &'static str) -> Self {
        Self(s)
    }

    /// The underlying str slice.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.0
    }

    /// True iff the key matches the biometric refuse-list (PRIME-DIRECTIVE §1).
    ///
    /// § DISCIPLINE
    ///   The check is :
    ///     1. exact-match against [`BIOMETRIC_TAG_KEYS`] (the canonical list)
    ///     2. substring-match against [`BIOMETRIC_TAG_SUBSTRINGS`] (defense-in-depth)
    #[must_use]
    pub fn is_biometric(self) -> bool {
        let s = self.0;
        if BIOMETRIC_TAG_KEYS.iter().any(|k| *k == s) {
            return true;
        }
        BIOMETRIC_TAG_SUBSTRINGS
            .iter()
            .any(|sub| s.contains(*sub))
    }
}

impl fmt::Display for TagKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

/// Canonical biometric-tag-key list — the per-spec banned set.
///
/// § DISCIPLINE : adding to this list is a DECISIONS-pin event ; removing
/// requires PRIME-DIRECTIVE §1 review (effectively never).
pub const BIOMETRIC_TAG_KEYS: &[&str] = &[
    "face_id",
    "iris_id",
    "fingerprint",
    "voiceprint",
    "gait",
    "retina",
    "ear_shape",
    "skull_shape",
    "skin_color",
    "skin_tone",
    "hair_color",
    "eye_color",
    "ethnicity",
    "age",
    "gender",
    "gaze_direction",
    "gaze_dir",
    "blink_pattern",
    "blink_rate",
    "eye_openness",
    "pupil_size",
    "pupil_dilation",
    "heart_rate",
    "hrv",
    "skin_conductance",
    "respiration_rate",
    "body_temp",
    "stress_level",
    "emotion_label",
    "mood",
];

/// Substrings that imply biometric/PII content even outside the canonical list
/// (defense-in-depth against future keys not yet pinned).
pub const BIOMETRIC_TAG_SUBSTRINGS: &[&str] = &[
    "biometric",
    "biom_",
    "_biom",
    "pii_",
    "_pii",
];

/// Tag-value : a deliberately-narrow surface preventing raw `&str` tags.
///
/// § DESIGN
///   - `Static(&'static str)` is the common case (e.g., `"60"` / `"L"` / `"compose"`).
///   - `Hashed(u64)` is a pre-computed hash of an opaque identifier (per T11-D130).
///   - `U64`/`I64`/`Bool` are scalar tag-values for numeric tags (e.g., a
///     pipeline-stage index, a cohort-bucket).
///
/// Note : the `Static` variant is `&'static str` so a literal-only origin is
/// guaranteed. Path-detection lives in [`TagVal::check_no_raw_path`] for
/// values that come from string-builders (e.g., format-strings).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TagVal {
    /// Compile-time string literal.
    Static(&'static str),
    /// Salted hash of an identifier (T11-D130).
    Hashed(u64),
    /// Unsigned scalar.
    U64(u64),
    /// Signed scalar.
    I64(i64),
    /// Boolean flag.
    Bool(bool),
}

impl TagVal {
    /// Construct a hashed-id tag-value.
    ///
    /// § DISCIPLINE : callers should derive `hash` from a salted-blake3
    /// (T11-D130) ; this constructor takes the hash directly so the salt
    /// management stays in cssl-telemetry / cssl-persist where it belongs.
    #[must_use]
    pub const fn hashed(hash: u64) -> Self {
        Self::Hashed(hash)
    }

    /// True iff the variant is `Static` AND the string contains a raw-path
    /// pattern (`/` outside hex+dots ; `\` ; or a Windows drive-letter prefix).
    #[must_use]
    pub fn appears_to_be_raw_path(&self) -> bool {
        let s = match self {
            Self::Static(s) => *s,
            _ => return false,
        };
        if s.contains('/') || s.contains('\\') {
            return true;
        }
        let bytes = s.as_bytes();
        if bytes.len() >= 2
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && (bytes.len() == 2 || bytes[2] == b'/' || bytes[2] == b'\\')
        {
            return true;
        }
        false
    }

    /// Returns `Err(MetricError::RawPathTagValue)` if [`Self::appears_to_be_raw_path`].
    ///
    /// # Errors
    /// Returns [`MetricError::RawPathTagValue`] when `metric_name`'s tag-set
    /// contains a `Static` value with a path-like pattern.
    pub fn check_no_raw_path(&self, metric_name: &'static str) -> MetricResult<()> {
        if self.appears_to_be_raw_path() {
            return Err(MetricError::RawPathTagValue { name: metric_name });
        }
        Ok(())
    }
}

impl fmt::Display for TagVal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Static(s) => f.write_str(s),
            Self::Hashed(h) => write!(f, "h={h:016x}"),
            Self::U64(v) => write!(f, "{v}"),
            Self::I64(v) => write!(f, "{v}"),
            Self::Bool(v) => write!(f, "{v}"),
        }
    }
}

/// Inline-only tag-list (≤ 4 pairs). Spill = discipline violation.
pub type TagSet = SmallVec<[(TagKey, TagVal); 4]>;

/// Validate a `(key, val)` tag-pair against PRIME-DIRECTIVE §1 + T11-D130.
///
/// § FAILURE MODES (in evaluation-order)
///   1. `key.is_biometric()` ⇒ [`MetricError::BiometricTagKey`]
///   2. `val.appears_to_be_raw_path()` ⇒ [`MetricError::RawPathTagValue`]
///
/// # Errors
/// Returns [`MetricError::BiometricTagKey`] or
/// [`MetricError::RawPathTagValue`] per § FAILURE-MODES.
pub fn validate_pair(
    metric_name: &'static str,
    key: TagKey,
    val: TagVal,
) -> MetricResult<()> {
    if key.is_biometric() {
        return Err(MetricError::BiometricTagKey {
            name: metric_name,
            key: key.0,
        });
    }
    val.check_no_raw_path(metric_name)?;
    Ok(())
}

/// Validate an entire tag-list ; aggregates per-pair refusals + spill-detection.
///
/// § DISCIPLINE
///   - len > 4 ⇒ [`MetricError::TagOverflow`]
///   - per-pair : [`validate_pair`]
///
/// # Errors
/// Returns the first refusal in iteration-order.
pub fn validate_tag_list(metric_name: &'static str, tags: &[(TagKey, TagVal)]) -> MetricResult<()> {
    if tags.len() > 4 {
        return Err(MetricError::TagOverflow {
            name: metric_name,
            len: tags.len(),
        });
    }
    for (k, v) in tags {
        validate_pair(metric_name, *k, *v)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        validate_pair, validate_tag_list, TagKey, TagVal, BIOMETRIC_TAG_KEYS,
        BIOMETRIC_TAG_SUBSTRINGS,
    };
    use crate::error::MetricError;

    #[test]
    fn tag_key_new_const() {
        const K: TagKey = TagKey::new("mode");
        assert_eq!(K.as_str(), "mode");
    }

    #[test]
    fn tag_key_display_writes_str() {
        let k = TagKey::new("phase");
        assert_eq!(format!("{k}"), "phase");
    }

    #[test]
    fn biometric_canonical_list_nonempty() {
        assert!(!BIOMETRIC_TAG_KEYS.is_empty());
    }

    #[test]
    fn biometric_substrings_nonempty() {
        assert!(!BIOMETRIC_TAG_SUBSTRINGS.is_empty());
    }

    #[test]
    fn biometric_face_id_refused() {
        assert!(TagKey::new("face_id").is_biometric());
    }

    #[test]
    fn biometric_iris_id_refused() {
        assert!(TagKey::new("iris_id").is_biometric());
    }

    #[test]
    fn biometric_heart_rate_refused() {
        assert!(TagKey::new("heart_rate").is_biometric());
    }

    #[test]
    fn biometric_gaze_direction_refused() {
        assert!(TagKey::new("gaze_direction").is_biometric());
    }

    #[test]
    fn biometric_substring_pii_refused() {
        assert!(TagKey::new("user_pii_id").is_biometric());
    }

    #[test]
    fn biometric_substring_biom_refused() {
        assert!(TagKey::new("biom_eye").is_biometric());
    }

    #[test]
    fn non_biometric_mode_accepted() {
        assert!(!TagKey::new("mode").is_biometric());
    }

    #[test]
    fn non_biometric_phase_accepted() {
        assert!(!TagKey::new("phase").is_biometric());
    }

    #[test]
    fn non_biometric_stage_accepted() {
        assert!(!TagKey::new("stage").is_biometric());
    }

    #[test]
    fn tag_val_static_unix_path_detected() {
        let v = TagVal::Static("/etc/hosts");
        assert!(v.appears_to_be_raw_path());
    }

    #[test]
    fn tag_val_static_windows_path_detected() {
        let v = TagVal::Static("C:\\Users");
        assert!(v.appears_to_be_raw_path());
    }

    #[test]
    fn tag_val_static_drive_letter_alone_detected() {
        let v = TagVal::Static("D:");
        assert!(v.appears_to_be_raw_path());
    }

    #[test]
    fn tag_val_hashed_never_detected_as_path() {
        let v = TagVal::Hashed(0xdead_beef_cafe_babe);
        assert!(!v.appears_to_be_raw_path());
    }

    #[test]
    fn tag_val_u64_never_detected_as_path() {
        let v = TagVal::U64(42);
        assert!(!v.appears_to_be_raw_path());
    }

    #[test]
    fn tag_val_static_legitimate_value() {
        let v = TagVal::Static("compose");
        assert!(!v.appears_to_be_raw_path());
    }

    #[test]
    fn tag_val_check_no_raw_path_accepts_clean() {
        let v = TagVal::Static("compose");
        assert!(v.check_no_raw_path("metric.x").is_ok());
    }

    #[test]
    fn tag_val_check_no_raw_path_refuses_unix() {
        let v = TagVal::Static("/etc/passwd");
        let r = v.check_no_raw_path("metric.x");
        assert!(matches!(r, Err(MetricError::RawPathTagValue { .. })));
    }

    #[test]
    fn tag_val_display_static() {
        assert_eq!(format!("{}", TagVal::Static("foo")), "foo");
    }

    #[test]
    fn tag_val_display_hashed_hex() {
        let s = format!("{}", TagVal::Hashed(0xdead_beef));
        assert!(s.starts_with("h="));
        assert!(s.contains("deadbeef"));
    }

    #[test]
    fn tag_val_display_u64() {
        assert_eq!(format!("{}", TagVal::U64(42)), "42");
    }

    #[test]
    fn tag_val_display_i64() {
        assert_eq!(format!("{}", TagVal::I64(-3)), "-3");
    }

    #[test]
    fn tag_val_display_bool() {
        assert_eq!(format!("{}", TagVal::Bool(true)), "true");
    }

    #[test]
    fn validate_pair_accepts_clean() {
        let r = validate_pair(
            "engine.frame_n",
            TagKey::new("mode"),
            TagVal::Static("60"),
        );
        assert!(r.is_ok());
    }

    #[test]
    fn validate_pair_refuses_biometric_key() {
        let r = validate_pair("m", TagKey::new("face_id"), TagVal::U64(0));
        assert!(matches!(r, Err(MetricError::BiometricTagKey { .. })));
    }

    #[test]
    fn validate_pair_refuses_raw_path_value() {
        let r = validate_pair("m", TagKey::new("path"), TagVal::Static("/etc/hosts"));
        assert!(matches!(r, Err(MetricError::RawPathTagValue { .. })));
    }

    #[test]
    fn validate_tag_list_at_inline_cap_ok() {
        let tags = [
            (TagKey::new("a"), TagVal::U64(1)),
            (TagKey::new("b"), TagVal::U64(2)),
            (TagKey::new("c"), TagVal::U64(3)),
            (TagKey::new("d"), TagVal::U64(4)),
        ];
        assert!(validate_tag_list("m", &tags).is_ok());
    }

    #[test]
    fn validate_tag_list_overflow_refused() {
        let tags = [
            (TagKey::new("a"), TagVal::U64(1)),
            (TagKey::new("b"), TagVal::U64(2)),
            (TagKey::new("c"), TagVal::U64(3)),
            (TagKey::new("d"), TagVal::U64(4)),
            (TagKey::new("e"), TagVal::U64(5)),
        ];
        let r = validate_tag_list("m", &tags);
        assert!(matches!(r, Err(MetricError::TagOverflow { .. })));
    }

    #[test]
    fn validate_tag_list_partial_biometric_refused() {
        let tags = [
            (TagKey::new("mode"), TagVal::Static("60")),
            (TagKey::new("face_id"), TagVal::U64(7)),
        ];
        let r = validate_tag_list("m", &tags);
        assert!(matches!(r, Err(MetricError::BiometricTagKey { .. })));
    }
}
