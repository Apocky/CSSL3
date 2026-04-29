//! Path-hash field sanitization integration tests (D130 / spec § 2.8).

use cssl_log::{
    emit_structured, init_default_policy, install_sink_chain, install_source_hasher,
    set_current_frame, set_replay_strict, Context, FieldValue, LogRecord, LogSink, PathHashField,
    Severity, SinkChain, SinkError, SourceLocation, SubsystemTag,
};
use cssl_telemetry::PathHasher;
use std::sync::{Arc, Mutex};

static TEST_LOCK: Mutex<()> = Mutex::new(());

struct CapSink(Arc<Mutex<Vec<LogRecord>>>);

impl LogSink for CapSink {
    fn write(&self, r: &LogRecord) -> Result<(), SinkError> {
        self.0.lock().unwrap().push(r.clone());
        Ok(())
    }
}

fn lock_and_setup(seed: u8, base_frame: u64) -> (std::sync::MutexGuard<'static, ()>, Arc<Mutex<Vec<LogRecord>>>) {
    let g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    set_replay_strict(false);
    cssl_log::force_reset_to_default();
    install_source_hasher(PathHasher::from_seed([seed; 32]));
    set_current_frame(base_frame);
    let captured: Arc<Mutex<Vec<LogRecord>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::new(CapSink(captured.clone()));
    let chain = Arc::new(SinkChain::new().with_sink(sink));
    install_sink_chain(chain);
    (g, captured)
}

fn fresh_ctx(seed: u8, frame: u64) -> Context {
    let hasher = PathHasher::from_seed([seed; 32]);
    let h = PathHashField::from_path_hash(hasher.hash_str("/file.rs"));
    Context::new(
        Severity::Info,
        SubsystemTag::Render,
        SourceLocation::new(h, 1, 1),
        frame,
    )
}

#[test]
fn raw_unix_path_in_field_replaced_at_emit() {
    let (_g, captured) = lock_and_setup(40, 13_000_000);
    let ctx = fresh_ctx(40, 13_000_000);
    emit_structured(
        &ctx,
        "msg".to_string(),
        vec![("path", FieldValue::Str("/etc/hosts"))],
    );
    let recs = captured.lock().unwrap();
    let r = &recs[0];
    let v = &r.fields[0].1;
    assert!(matches!(v, FieldValue::Str("<<RAW_PATH_REJECTED>>")));
}

#[test]
fn raw_windows_path_in_field_replaced_at_emit() {
    let (_g, captured) = lock_and_setup(41, 13_001_000);
    let ctx = fresh_ctx(41, 13_001_000);
    emit_structured(
        &ctx,
        "msg".to_string(),
        vec![("file", FieldValue::Str("C:\\users\\x"))],
    );
    let recs = captured.lock().unwrap();
    assert!(matches!(
        &recs[0].fields[0].1,
        FieldValue::Str("<<RAW_PATH_REJECTED>>")
    ));
}

#[test]
fn path_hash_field_passes_through_clean() {
    let (_g, captured) = lock_and_setup(42, 13_002_000);
    let ctx = fresh_ctx(42, 13_002_000);
    let hasher = PathHasher::from_seed([42u8; 32]);
    let h = PathHashField::from_path_hash(hasher.hash_str("/etc/hosts"));
    emit_structured(
        &ctx,
        "msg".to_string(),
        vec![("path", FieldValue::Path(h))],
    );
    let recs = captured.lock().unwrap();
    assert!(matches!(&recs[0].fields[0].1, FieldValue::Path(_)));
}

#[test]
fn numeric_field_passes_through() {
    let (_g, captured) = lock_and_setup(43, 13_003_000);
    let ctx = fresh_ctx(43, 13_003_000);
    emit_structured(
        &ctx,
        "msg".to_string(),
        vec![
            ("count", FieldValue::I64(42)),
            ("ratio", FieldValue::F64(0.5)),
            ("ok", FieldValue::Bool(true)),
        ],
    );
    let recs = captured.lock().unwrap();
    let r = &recs[0];
    assert_eq!(r.fields[0].1, FieldValue::I64(42));
    assert!(matches!(r.fields[1].1, FieldValue::F64(_)));
    assert_eq!(r.fields[2].1, FieldValue::Bool(true));
}

#[test]
fn clean_string_field_passes_through() {
    let (_g, captured) = lock_and_setup(44, 13_004_000);
    let ctx = fresh_ctx(44, 13_004_000);
    emit_structured(
        &ctx,
        "msg".to_string(),
        vec![("note", FieldValue::Str("clean text no slashes"))],
    );
    let recs = captured.lock().unwrap();
    assert!(matches!(
        &recs[0].fields[0].1,
        FieldValue::Str("clean text no slashes")
    ));
}

#[test]
fn json_lines_output_no_raw_path_chars() {
    let (_g, captured) = lock_and_setup(45, 13_005_000);
    let ctx = fresh_ctx(45, 13_005_000);
    emit_structured(
        &ctx,
        "msg".to_string(),
        vec![("path", FieldValue::Str("/secret/key.txt"))],
    );
    let recs = captured.lock().unwrap();
    let line = recs[0].encode_line(cssl_log::Format::JsonLines);
    assert!(!line.contains("/secret"));
    assert!(line.contains("RAW_PATH_REJECTED"));
}

#[test]
fn csl_glyph_output_no_raw_path_chars() {
    let (_g, captured) = lock_and_setup(46, 13_006_000);
    let ctx = fresh_ctx(46, 13_006_000);
    emit_structured(
        &ctx,
        "msg".to_string(),
        vec![("file", FieldValue::Str("/secret/key.txt"))],
    );
    let recs = captured.lock().unwrap();
    let line = recs[0].encode_line(cssl_log::Format::CslGlyph);
    assert!(!line.contains("/secret"));
}

#[test]
fn owned_string_with_path_replaced() {
    let (_g, captured) = lock_and_setup(47, 13_007_000);
    let ctx = fresh_ctx(47, 13_007_000);
    let owned = String::from("/usr/bin/x");
    emit_structured(
        &ctx,
        "msg".to_string(),
        vec![("path", FieldValue::OwnedStr(owned))],
    );
    let recs = captured.lock().unwrap();
    if let FieldValue::OwnedStr(s) = &recs[0].fields[0].1 {
        assert_eq!(s, "<<RAW_PATH_REJECTED>>");
    } else {
        panic!("expected OwnedStr");
    }
}

#[test]
fn drive_letter_alone_replaced() {
    let (_g, captured) = lock_and_setup(48, 13_008_000);
    let ctx = fresh_ctx(48, 13_008_000);
    emit_structured(
        &ctx,
        "msg".to_string(),
        vec![("drive", FieldValue::Str("D:"))],
    );
    // "D:" alone IS a drive-letter prefix per audit_path_op_check ; gets replaced.
    let recs = captured.lock().unwrap();
    assert!(matches!(
        &recs[0].fields[0].1,
        FieldValue::Str("<<RAW_PATH_REJECTED>>")
    ));
}

#[test]
fn message_text_unaffected_by_field_sanitization() {
    // Spec : ONLY field values are sanitized at the cssl-log boundary.
    // The message string itself is NOT touched (it's user-format-args).
    // The audit-sink does its own check on the message before audit-append.
    let (_g, captured) = lock_and_setup(49, 13_009_000);
    let ctx = fresh_ctx(49, 13_009_000);
    emit_structured(
        &ctx,
        "loaded data from /etc/hosts".to_string(),
        Vec::new(),
    );
    let recs = captured.lock().unwrap();
    // Message preserved verbatim ; audit-sink would reject this if asked
    // to append, but the ring/stderr/file/mcp sinks do not.
    assert!(recs[0].message.contains("/etc/hosts"));
}
