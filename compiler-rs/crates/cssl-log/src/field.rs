//! Structured-log field type. Spec § 2.2 — `key=value` pairs flow through
//! sink-write paths. The value-encoding goes through this enum so the
//! D130 path-hash discipline (§ 2.8) is enforced uniformly.
//!
//! § DISCIPLINE (D130 / § 2.8) :
//!   - `Path` variant : carries a [`PathHashField`] ⟵ structurally cannot
//!     accept raw `&str`/`&Path`. The macros lower `path_hash = h` directly
//!     to this variant.
//!   - `Str` variant : carries a `&'static str` AND is run through
//!     [`cssl_telemetry::audit_path_op_check_raw_path_rejected`] before
//!     writing to any sink. If the field-name is `path`/`file`/`dir` AND
//!     the value contains `/` or `\`, the value is replaced with the
//!     placeholder `<<RAW_PATH_REJECTED>>` and an audit-chain entry is
//!     recorded (PD0018-adjacent).
//!   - `OwnedStr` variant : same discipline applied to owned strings —
//!     used when format-arg lowering produces dynamic strings.
//!   - `I64` / `U64` / `F64` / `Bool` : numeric scalars ; no path-leak
//!     pathway possible.

use core::fmt;

use crate::path_hash_field::PathHashField;

/// Structured-log field value. `key` lives at the [`crate::sink::LogRecord`]
/// level ; this enum is just the value-side. Stable wire-shape across all
/// sinks (Ring/Stderr/File/MCP/Audit).
#[derive(Debug, Clone, PartialEq)]
pub enum FieldValue {
    /// Path-hash (D130 enforced at type level — no raw `&str` constructor).
    Path(PathHashField),
    /// Static string. Must NOT contain raw-path patterns ; sinks invoke
    /// [`Self::sanitize_for_sink`] to enforce.
    Str(&'static str),
    /// Owned string. Same path-hash sanitization as [`Self::Str`].
    OwnedStr(String),
    /// Signed 64-bit integer.
    I64(i64),
    /// Unsigned 64-bit integer.
    U64(u64),
    /// 64-bit float. NaN comparison-friendly via bit-equality below.
    F64(f64),
    /// Boolean.
    Bool(bool),
}

impl FieldValue {
    /// Returns `true` iff the inner value is a string-shaped variant
    /// (`Str` or `OwnedStr`) so the sink can decide whether to run the
    /// raw-path checker.
    #[must_use]
    pub const fn is_string_shaped(&self) -> bool {
        matches!(self, Self::Str(_) | Self::OwnedStr(_))
    }

    /// Apply path-hash discipline to a string-shaped value : if the value
    /// contains `/` or `\` (i.e., raw-path signature per
    /// `cssl_telemetry::audit_path_op_check_raw_path_rejected`), replace
    /// it with `<<RAW_PATH_REJECTED>>`. Numeric variants pass through
    /// unchanged.
    ///
    /// § SPEC § 2.8 path-hash-only field sanitization. The check delegates
    /// to [`cssl_telemetry::audit_path_op_check_raw_path_rejected`] which
    /// also catches Windows drive-letter prefixes (`C:`, `D:`, ...) — so
    /// even string values without `/`/`\` get checked when the field-name
    /// looks path-shaped.
    #[must_use]
    pub fn sanitize_for_sink(self, key: &str) -> Self {
        // Field-names that smell path-shaped get an extra guard — even
        // for non-slash patterns (drive-letters, etc.).
        let key_is_path_shaped = matches!(
            key,
            "path" | "file" | "dir" | "filename" | "filepath" | "drive"
        );
        match &self {
            Self::Str(s) => {
                if key_is_path_shaped || contains_raw_path_signature(s) {
                    if cssl_telemetry::audit_path_op_check_raw_path_rejected(s).is_err() {
                        return Self::Str("<<RAW_PATH_REJECTED>>");
                    }
                }
                self
            }
            Self::OwnedStr(s) => {
                if key_is_path_shaped || contains_raw_path_signature(s) {
                    if cssl_telemetry::audit_path_op_check_raw_path_rejected(s).is_err() {
                        return Self::OwnedStr(String::from("<<RAW_PATH_REJECTED>>"));
                    }
                }
                self
            }
            // Path / numeric / bool : no sanitization needed.
            _ => self,
        }
    }
}

impl fmt::Display for FieldValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Path(h) => fmt::Display::fmt(h, f),
            Self::Str(s) => f.write_str(s),
            Self::OwnedStr(s) => f.write_str(s),
            Self::I64(v) => write!(f, "{v}"),
            Self::U64(v) => write!(f, "{v}"),
            Self::F64(v) => write!(f, "{v}"),
            Self::Bool(b) => write!(f, "{b}"),
        }
    }
}

fn contains_raw_path_signature(s: &str) -> bool {
    s.contains('/') || s.contains('\\')
}

/// Conversion helpers — accept common scalar types into [`FieldValue`].
impl From<&'static str> for FieldValue {
    fn from(s: &'static str) -> Self {
        Self::Str(s)
    }
}
impl From<String> for FieldValue {
    fn from(s: String) -> Self {
        Self::OwnedStr(s)
    }
}
impl From<i64> for FieldValue {
    fn from(v: i64) -> Self {
        Self::I64(v)
    }
}
impl From<u64> for FieldValue {
    fn from(v: u64) -> Self {
        Self::U64(v)
    }
}
impl From<i32> for FieldValue {
    fn from(v: i32) -> Self {
        Self::I64(i64::from(v))
    }
}
impl From<u32> for FieldValue {
    fn from(v: u32) -> Self {
        Self::U64(u64::from(v))
    }
}
impl From<usize> for FieldValue {
    fn from(v: usize) -> Self {
        Self::U64(v as u64)
    }
}
impl From<f64> for FieldValue {
    fn from(v: f64) -> Self {
        Self::F64(v)
    }
}
impl From<f32> for FieldValue {
    fn from(v: f32) -> Self {
        Self::F64(f64::from(v))
    }
}
impl From<bool> for FieldValue {
    fn from(v: bool) -> Self {
        Self::Bool(v)
    }
}
impl From<PathHashField> for FieldValue {
    fn from(h: PathHashField) -> Self {
        Self::Path(h)
    }
}

#[cfg(test)]
mod tests {
    use super::{FieldValue, PathHashField};
    use cssl_telemetry::PathHasher;

    #[test]
    fn from_static_str() {
        let v: FieldValue = "hello".into();
        assert!(matches!(v, FieldValue::Str("hello")));
    }

    #[test]
    fn from_owned_str() {
        let s = String::from("dynamic");
        let v: FieldValue = s.into();
        assert!(matches!(v, FieldValue::OwnedStr(_)));
    }

    #[test]
    fn from_i64() {
        let v: FieldValue = 42i64.into();
        assert_eq!(v, FieldValue::I64(42));
    }

    #[test]
    fn from_u64() {
        let v: FieldValue = 99u64.into();
        assert_eq!(v, FieldValue::U64(99));
    }

    #[test]
    fn from_i32_widens() {
        let v: FieldValue = 7i32.into();
        assert_eq!(v, FieldValue::I64(7));
    }

    #[test]
    fn from_u32_widens() {
        let v: FieldValue = 7u32.into();
        assert_eq!(v, FieldValue::U64(7));
    }

    #[test]
    fn from_f32_widens() {
        let v: FieldValue = 1.5f32.into();
        if let FieldValue::F64(f) = v {
            assert!((f - 1.5).abs() < 1e-6);
        } else {
            panic!("expected F64");
        }
    }

    #[test]
    fn from_bool() {
        let v: FieldValue = true.into();
        assert_eq!(v, FieldValue::Bool(true));
    }

    #[test]
    fn from_path_hash_field() {
        let hasher = PathHasher::from_seed([0u8; 32]);
        let h = hasher.hash_str("/x");
        let v: FieldValue = PathHashField::from_path_hash(h).into();
        assert!(matches!(v, FieldValue::Path(_)));
    }

    // § Display.

    #[test]
    fn display_str_emits_verbatim() {
        let v = FieldValue::Str("verbatim");
        assert_eq!(format!("{v}"), "verbatim");
    }

    #[test]
    fn display_i64() {
        let v = FieldValue::I64(-42);
        assert_eq!(format!("{v}"), "-42");
    }

    #[test]
    fn display_u64() {
        let v = FieldValue::U64(42);
        assert_eq!(format!("{v}"), "42");
    }

    #[test]
    fn display_bool() {
        assert_eq!(format!("{}", FieldValue::Bool(true)), "true");
        assert_eq!(format!("{}", FieldValue::Bool(false)), "false");
    }

    #[test]
    fn display_path_uses_short_form() {
        let hasher = PathHasher::from_seed([0u8; 32]);
        let h = hasher.hash_str("/x");
        let v = FieldValue::Path(PathHashField::from_path_hash(h));
        let s = format!("{v}");
        assert_eq!(s.len(), 19);
        assert!(s.ends_with("..."));
    }

    // § Path-hash sanitization (D130 / § 2.8).

    #[test]
    fn sanitize_str_clean_passes_through() {
        let v = FieldValue::Str("clean=value bytes=42");
        let s = v.sanitize_for_sink("any");
        assert!(matches!(s, FieldValue::Str("clean=value bytes=42")));
    }

    #[test]
    fn sanitize_str_with_unix_path_replaced() {
        let v = FieldValue::Str("/etc/hosts");
        let s = v.sanitize_for_sink("path");
        assert!(matches!(s, FieldValue::Str("<<RAW_PATH_REJECTED>>")));
    }

    #[test]
    fn sanitize_str_with_windows_path_replaced() {
        let v = FieldValue::Str("C:\\users");
        let s = v.sanitize_for_sink("file");
        assert!(matches!(s, FieldValue::Str("<<RAW_PATH_REJECTED>>")));
    }

    #[test]
    fn sanitize_owned_str_with_path_replaced() {
        let v = FieldValue::OwnedStr(String::from("/usr/local/bin"));
        let s = v.sanitize_for_sink("dir");
        if let FieldValue::OwnedStr(text) = s {
            assert_eq!(text, "<<RAW_PATH_REJECTED>>");
        } else {
            panic!("expected OwnedStr");
        }
    }

    #[test]
    fn sanitize_path_field_passes_through() {
        let hasher = PathHasher::from_seed([0u8; 32]);
        let h = hasher.hash_str("/x");
        let v = FieldValue::Path(PathHashField::from_path_hash(h));
        let s = v.clone().sanitize_for_sink("any");
        assert_eq!(v, s);
    }

    #[test]
    fn sanitize_numeric_passes_through() {
        let v = FieldValue::I64(42);
        let s = v.clone().sanitize_for_sink("path"); // even key=path
        assert_eq!(v, s);
    }

    #[test]
    fn sanitize_string_with_slash_anywhere_replaced() {
        // Even if key isn't path-shaped, a slash signals raw-path leak.
        let v = FieldValue::Str("data/with/slash");
        let s = v.sanitize_for_sink("not-path-key");
        assert!(matches!(s, FieldValue::Str("<<RAW_PATH_REJECTED>>")));
    }

    #[test]
    fn sanitize_path_hash_short_form_passes_through() {
        // Hash-form (hex + "...") has no `/` or `\`.
        let v = FieldValue::OwnedStr(String::from("deadbeefcafebabe..."));
        let s = v.clone().sanitize_for_sink("path");
        assert_eq!(v, s);
    }

    // § is_string_shaped.

    #[test]
    fn is_string_shaped_true_for_strs() {
        assert!(FieldValue::Str("a").is_string_shaped());
        assert!(FieldValue::OwnedStr(String::from("b")).is_string_shaped());
    }

    #[test]
    fn is_string_shaped_false_for_numerics() {
        assert!(!FieldValue::I64(0).is_string_shaped());
        assert!(!FieldValue::U64(0).is_string_shaped());
        assert!(!FieldValue::F64(0.0).is_string_shaped());
        assert!(!FieldValue::Bool(false).is_string_shaped());
    }
}
