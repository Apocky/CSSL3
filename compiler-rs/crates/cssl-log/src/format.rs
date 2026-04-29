//! Wire-format encoders : JsonLines / CslGlyph / Binary.
//!
//! § SPEC : `_drafts/phase_j/05_l0_l1_error_log_spec.md` § 2.9.
//!
//! § DETERMINISM :
//!   ALL encoders are deterministic — given the same [`crate::sink::LogRecord`]
//!   they always produce identical bytes. Spec § 2.9 "ALL conversions
//!   deterministic ⟵ replay-friendly".

use core::fmt::Write as _;

use crate::field::FieldValue;
use crate::sink::LogRecord;

/// Wire-format selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// `{"frame":..., "lvl":..., "sub":..., "msg":..., "fields":{...}}`
    /// — machine-readable.
    JsonLines,
    /// `frame=42 I [render] msg fields...` — human-readable terminal.
    CslGlyph,
    /// 64-byte ring-slot encoding ⟵ binary-protocol-tooling (Wave-Jζ).
    Binary,
}

impl Format {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::JsonLines => "json-lines",
            Self::CslGlyph => "csl-glyph",
            Self::Binary => "binary",
        }
    }
}

// ───────────────────────────────────────────────────────────────────────
// § JsonLines encoder
// ───────────────────────────────────────────────────────────────────────

/// Encode a [`LogRecord`] into a single JSON-line. Trailing `\n` included.
///
/// § SHAPE : `{"frame":N, "lvl":"info", "sub":"render", "msg":"...",
/// "fields":{"k":v,...}, "src":{"line":L, "col":C, "file_hash":"hex..."}}`
#[must_use]
pub fn encode_json_lines(record: &LogRecord) -> String {
    let mut obj = serde_json::Map::new();
    obj.insert(
        "frame".into(),
        serde_json::Value::Number(record.frame_n.into()),
    );
    obj.insert(
        "lvl".into(),
        serde_json::Value::String(record.severity.as_str().into()),
    );
    obj.insert(
        "sub".into(),
        serde_json::Value::String(record.subsystem.as_str().into()),
    );
    obj.insert(
        "msg".into(),
        serde_json::Value::String(record.message.clone()),
    );

    // Fields object.
    let mut fields_obj = serde_json::Map::new();
    for (k, v) in &record.fields {
        fields_obj.insert((*k).to_string(), field_value_to_json(v));
    }
    obj.insert("fields".into(), serde_json::Value::Object(fields_obj));

    // Source-loc as nested object.
    let mut src_obj = serde_json::Map::new();
    src_obj.insert(
        "line".into(),
        serde_json::Value::Number(record.source.line.into()),
    );
    src_obj.insert(
        "col".into(),
        serde_json::Value::Number(record.source.column.into()),
    );
    src_obj.insert(
        "file_hash".into(),
        serde_json::Value::String(format!("{}", record.source.file_path_hash)),
    );
    obj.insert("src".into(), serde_json::Value::Object(src_obj));

    let mut s = serde_json::to_string(&serde_json::Value::Object(obj))
        .unwrap_or_else(|_| String::from("{}"));
    s.push('\n');
    s
}

fn field_value_to_json(v: &FieldValue) -> serde_json::Value {
    match v {
        FieldValue::Path(h) => serde_json::Value::String(format!("{h}")),
        FieldValue::Str(s) => serde_json::Value::String((*s).to_string()),
        FieldValue::OwnedStr(s) => serde_json::Value::String(s.clone()),
        FieldValue::I64(v) => serde_json::Value::Number((*v).into()),
        FieldValue::U64(v) => serde_json::Value::Number((*v).into()),
        FieldValue::F64(v) => serde_json::Number::from_f64(*v)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        FieldValue::Bool(b) => serde_json::Value::Bool(*b),
    }
}

// ───────────────────────────────────────────────────────────────────────
// § CslGlyph encoder (human-readable line)
// ───────────────────────────────────────────────────────────────────────

/// Encode a [`LogRecord`] into a human-readable single line. Trailing `\n`
/// included. Format : `frame=N G [subsystem] msg key=val ...`.
///
/// `G` is the severity glyph from [`crate::Severity::glyph`].
#[must_use]
pub fn encode_csl_glyph(record: &LogRecord) -> String {
    let mut s = String::with_capacity(128);
    let _ = write!(
        s,
        "frame={n} {g} [{sub}] {msg}",
        n = record.frame_n,
        g = record.severity.glyph(),
        sub = record.subsystem.as_str(),
        msg = record.message
    );
    for (k, v) in &record.fields {
        let _ = write!(s, " {k}={v}");
    }
    s.push('\n');
    s
}

// ───────────────────────────────────────────────────────────────────────
// § Binary encoder (ring-slot 40-byte payload)
// ───────────────────────────────────────────────────────────────────────

/// Encode a [`LogRecord`] into the 40-byte inline ring-slot payload per
/// spec § 2.4 "[level:1][subsystem:1][field-count:1][fields:...] ⟵ binary
/// packing". Returns up to 40 bytes ; longer payloads truncate.
///
/// § LAYOUT (offset : len : meaning) :
///   0   : 1 : severity u8
///   1   : 1 : subsystem u8
///   2   : 1 : field_count u8 (truncated to count emitted in available bytes)
///   3   : 1 : reserved (zero)
///   4   : 8 : frame_n (big-endian u64)
///   12  : 4 : line (big-endian u32)
///   16  : 4 : column (big-endian u32)
///   20+ : variable : truncated message + key-prefix bytes
#[must_use]
pub fn encode_binary(record: &LogRecord) -> [u8; 40] {
    let mut out = [0u8; 40];
    out[0] = record.severity.as_u8();
    out[1] = record.subsystem.as_u8();
    let count = u8::try_from(record.fields.len()).unwrap_or(u8::MAX);
    out[2] = count;
    out[3] = 0; // reserved
    out[4..12].copy_from_slice(&record.frame_n.to_be_bytes());
    out[12..16].copy_from_slice(&record.source.line.to_be_bytes());
    out[16..20].copy_from_slice(&record.source.column.to_be_bytes());
    // Remaining 20 bytes : prefix of message.
    let msg_bytes = record.message.as_bytes();
    let copy_len = core::cmp::min(msg_bytes.len(), 20);
    out[20..20 + copy_len].copy_from_slice(&msg_bytes[..copy_len]);
    out
}

/// Decode the metadata header from a binary 40-byte payload. Returns
/// `(severity_u8, subsystem_u8, field_count, frame_n, line, col)`. Used
/// for ring-replay tooling.
#[must_use]
pub const fn decode_binary_header(buf: &[u8; 40]) -> (u8, u8, u8, u64, u32, u32) {
    let frame_n = u64::from_be_bytes([
        buf[4], buf[5], buf[6], buf[7], buf[8], buf[9], buf[10], buf[11],
    ]);
    let line = u32::from_be_bytes([buf[12], buf[13], buf[14], buf[15]]);
    let col = u32::from_be_bytes([buf[16], buf[17], buf[18], buf[19]]);
    (buf[0], buf[1], buf[2], frame_n, line, col)
}

#[cfg(test)]
mod tests {
    use super::{decode_binary_header, encode_binary, encode_csl_glyph, encode_json_lines, Format};
    use crate::field::FieldValue;
    use crate::path_hash_field::PathHashField;
    use crate::severity::{Severity, SourceLocation};
    use crate::sink::LogRecord;
    use crate::subsystem::SubsystemTag;
    use cssl_telemetry::PathHasher;

    fn fresh_record() -> LogRecord {
        let hasher = PathHasher::from_seed([0u8; 32]);
        let h = hasher.hash_str("/test/file.rs");
        LogRecord {
            frame_n: 42,
            severity: Severity::Info,
            subsystem: SubsystemTag::Render,
            source: SourceLocation::new(PathHashField::from_path_hash(h), 7, 3),
            message: String::from("frame complete"),
            fields: vec![("ms", FieldValue::I64(16))],
        }
    }

    // § Format::as_str.

    #[test]
    fn format_as_str_canonical() {
        assert_eq!(Format::JsonLines.as_str(), "json-lines");
        assert_eq!(Format::CslGlyph.as_str(), "csl-glyph");
        assert_eq!(Format::Binary.as_str(), "binary");
    }

    // § JsonLines encoder.

    #[test]
    fn json_lines_emits_valid_json() {
        let r = fresh_record();
        let line = encode_json_lines(&r);
        assert!(line.ends_with('\n'));
        let parsed: serde_json::Value = serde_json::from_str(line.trim_end()).unwrap();
        assert_eq!(parsed["frame"], 42);
        assert_eq!(parsed["lvl"], "info");
        assert_eq!(parsed["sub"], "render");
        assert_eq!(parsed["msg"], "frame complete");
    }

    #[test]
    fn json_lines_preserves_field_keys() {
        let r = fresh_record();
        let line = encode_json_lines(&r);
        let parsed: serde_json::Value = serde_json::from_str(line.trim_end()).unwrap();
        assert_eq!(parsed["fields"]["ms"], 16);
    }

    #[test]
    fn json_lines_path_field_uses_short_form() {
        let hasher = PathHasher::from_seed([0u8; 32]);
        let h = hasher.hash_str("/secret.txt");
        let mut r = fresh_record();
        r.fields.push(("path", FieldValue::Path(PathHashField::from_path_hash(h))));
        let line = encode_json_lines(&r);
        assert!(line.contains("..."));
        assert!(!line.contains("/secret"));
    }

    #[test]
    fn json_lines_deterministic_across_calls() {
        let r = fresh_record();
        let a = encode_json_lines(&r);
        let b = encode_json_lines(&r);
        assert_eq!(a, b);
    }

    #[test]
    fn json_lines_emits_source_block() {
        let r = fresh_record();
        let line = encode_json_lines(&r);
        let parsed: serde_json::Value = serde_json::from_str(line.trim_end()).unwrap();
        assert_eq!(parsed["src"]["line"], 7);
        assert_eq!(parsed["src"]["col"], 3);
        assert!(parsed["src"]["file_hash"].as_str().unwrap().ends_with("..."));
    }

    #[test]
    fn json_lines_does_not_contain_raw_path() {
        let mut r = fresh_record();
        r.fields.push(("note", FieldValue::Str("this is /etc/hosts").sanitize_for_sink("note")));
        let line = encode_json_lines(&r);
        assert!(!line.contains("/etc/hosts"));
    }

    // § CslGlyph encoder.

    #[test]
    fn csl_glyph_starts_with_frame() {
        let r = fresh_record();
        let line = encode_csl_glyph(&r);
        assert!(line.starts_with("frame=42"));
        assert!(line.ends_with('\n'));
    }

    #[test]
    fn csl_glyph_contains_severity_glyph() {
        let r = fresh_record();
        let line = encode_csl_glyph(&r);
        assert!(line.contains(" I ")); // Info glyph
    }

    #[test]
    fn csl_glyph_contains_subsystem() {
        let r = fresh_record();
        let line = encode_csl_glyph(&r);
        assert!(line.contains("[render]"));
    }

    #[test]
    fn csl_glyph_appends_fields() {
        let r = fresh_record();
        let line = encode_csl_glyph(&r);
        assert!(line.contains("ms=16"));
    }

    #[test]
    fn csl_glyph_deterministic() {
        let r = fresh_record();
        let a = encode_csl_glyph(&r);
        let b = encode_csl_glyph(&r);
        assert_eq!(a, b);
    }

    #[test]
    fn csl_glyph_severity_glyphs_distinct() {
        let mut r = fresh_record();
        let mut glyphs = std::collections::HashSet::new();
        for s in Severity::all() {
            r.severity = s;
            let line = encode_csl_glyph(&r);
            // Extract glyph char between `frame=N ` and ` [`.
            let parts: Vec<&str> = line.split(' ').collect();
            glyphs.insert(parts[1].to_string());
        }
        assert_eq!(glyphs.len(), 6);
    }

    // § Binary encoder.

    #[test]
    fn binary_encodes_severity_and_subsystem() {
        let r = fresh_record();
        let buf = encode_binary(&r);
        assert_eq!(buf[0], Severity::Info.as_u8());
        assert_eq!(buf[1], SubsystemTag::Render.as_u8());
    }

    #[test]
    fn binary_encodes_field_count() {
        let r = fresh_record();
        let buf = encode_binary(&r);
        assert_eq!(buf[2], 1); // one field (`ms`)
    }

    #[test]
    fn binary_encodes_frame_be() {
        let r = fresh_record();
        let buf = encode_binary(&r);
        let frame = u64::from_be_bytes([
            buf[4], buf[5], buf[6], buf[7], buf[8], buf[9], buf[10], buf[11],
        ]);
        assert_eq!(frame, 42);
    }

    #[test]
    fn binary_encodes_line_col_be() {
        let r = fresh_record();
        let buf = encode_binary(&r);
        let line = u32::from_be_bytes([buf[12], buf[13], buf[14], buf[15]]);
        let col = u32::from_be_bytes([buf[16], buf[17], buf[18], buf[19]]);
        assert_eq!(line, 7);
        assert_eq!(col, 3);
    }

    #[test]
    fn binary_decodes_round_trip() {
        let r = fresh_record();
        let buf = encode_binary(&r);
        let (s, sub, fc, fr, ln, c) = decode_binary_header(&buf);
        assert_eq!(s, Severity::Info.as_u8());
        assert_eq!(sub, SubsystemTag::Render.as_u8());
        assert_eq!(fc, 1);
        assert_eq!(fr, 42);
        assert_eq!(ln, 7);
        assert_eq!(c, 3);
    }

    #[test]
    fn binary_truncates_long_message() {
        let mut r = fresh_record();
        r.message = "a".repeat(64);
        let buf = encode_binary(&r);
        // Message bytes start at offset 20, max 20 bytes — all 'a'.
        assert_eq!(&buf[20..40], &[b'a'; 20]);
    }

    #[test]
    fn binary_short_message_zero_padded() {
        let mut r = fresh_record();
        r.message = String::from("hi");
        let buf = encode_binary(&r);
        assert_eq!(buf[20], b'h');
        assert_eq!(buf[21], b'i');
        assert_eq!(buf[22], 0);
        assert_eq!(buf[39], 0);
    }

    #[test]
    fn binary_field_count_saturates_at_u8_max() {
        let mut r = fresh_record();
        r.fields.clear();
        for _ in 0..300 {
            r.fields.push(("k", FieldValue::I64(0)));
        }
        let buf = encode_binary(&r);
        assert_eq!(buf[2], u8::MAX);
    }

    #[test]
    fn binary_deterministic_across_calls() {
        let r = fresh_record();
        let a = encode_binary(&r);
        let b = encode_binary(&r);
        assert_eq!(a, b);
    }

    #[test]
    fn binary_reserved_byte_zero() {
        let r = fresh_record();
        let buf = encode_binary(&r);
        assert_eq!(buf[3], 0);
    }
}
