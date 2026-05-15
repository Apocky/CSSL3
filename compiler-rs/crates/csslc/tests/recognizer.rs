// § recognizer.rs · spec-70 § item-02 · A02.2 canonical regression corpus
//
// For each .cssl file in `tests/recognizer/`, run the frontend pipeline
// (lex → parse → cssl_hir::lower_module) and assert that the resulting
// `DiagnosticBag`'s contents match the expectation declared in the file's
// leading comment header.
//
// Negative cases (mismatch should fire) → assert at least one error
// diagnostic mentions both the expected and actual primitive name (or
// the expected arity, for tuple cases).
//
// Positive cases (`*_passes.cssl`) → assert ZERO errors. Distinguishes
// "the gate fires only on real mismatches" from "the gate over-triggers".

use std::path::PathBuf;

use cssl_ast::{Severity, SourceFile, SourceId, Surface};

fn corpus_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("recognizer")
}

fn lower(name: &str) -> cssl_ast::DiagnosticBag {
    let path = corpus_dir().join(name);
    let src = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let file = SourceFile::new(SourceId::first(), path.display().to_string(), &src, Surface::RustHybrid);
    let tokens = cssl_lex::lex(&file);
    let (cst, parse_bag) = cssl_parse::parse(&file, &tokens);
    assert_eq!(parse_bag.error_count(), 0, "parse errors in {name}: {:?}", parse_bag);
    let (_hir, _interner, bag) = cssl_hir::lower_module(&file, &cst);
    bag
}

fn assert_has_error_with(bag: &cssl_ast::DiagnosticBag, needles: &[&str], file: &str) {
    let errors: Vec<_> = bag
        .iter()
        .filter(|d| matches!(d.severity, Severity::Error))
        .collect();
    assert!(
        !errors.is_empty(),
        "{file}: expected ≥1 error, got bag = {bag:?}"
    );
    let any_match = errors.iter().any(|d| {
        let msg = &d.message;
        needles.iter().all(|n| msg.contains(n)) && d.span.is_some()
    });
    assert!(
        any_match,
        "{file}: no error matches {needles:?} ; got = {:?}",
        errors.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn a02_2_u32_to_i64_emits_diagnostic() {
    let bag = lower("u32_to_i64.cssl");
    assert_has_error_with(&bag, &["expected i64", "u32"], "u32_to_i64.cssl");
}

#[test]
fn a02_2_i32_to_u32_emits_diagnostic() {
    let bag = lower("i32_to_u32.cssl");
    assert_has_error_with(&bag, &["expected u32", "i32"], "i32_to_u32.cssl");
}

#[test]
fn a02_2_f32_to_i32_emits_diagnostic() {
    let bag = lower("f32_to_i32.cssl");
    assert_has_error_with(&bag, &["expected i32", "f32"], "f32_to_i32.cssl");
}

#[test]
fn a02_2_f32_to_f64_emits_diagnostic() {
    let bag = lower("f32_to_f64.cssl");
    assert_has_error_with(&bag, &["expected f64", "f32"], "f32_to_f64.cssl");
}

#[test]
fn a02_2_explicit_cast_passes() {
    let bag = lower("explicit_cast_passes.cssl");
    let errors: Vec<_> = bag
        .iter()
        .filter(|d| matches!(d.severity, Severity::Error))
        .collect();
    assert!(
        errors.is_empty(),
        "explicit_cast_passes.cssl should NOT trigger ; got = {:?}",
        errors.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn a02_2_tuple_arity_emits_diagnostic() {
    let bag = lower("tuple_arity.cssl");
    assert_has_error_with(&bag, &["arity 2", "arity 3"], "tuple_arity.cssl");
}

#[test]
fn a02_2_diagnostic_carries_span_pointing_at_offending_expr() {
    // Span must pin the offending expression, not the function header.
    // This is what makes the diagnostic actionable in editors.
    let bag = lower("u32_to_i64.cssl");
    let errors: Vec<_> = bag
        .iter()
        .filter(|d| matches!(d.severity, Severity::Error))
        .collect();
    let prim = errors
        .iter()
        .find(|d| d.message.contains("expected i64"))
        .expect("primary mismatch diagnostic missing");
    let span = prim.span.expect("diagnostic must carry a span");
    // The trailing-expr `x` in `fn f(x : u32) -> i64 { x }` sits well past byte 20.
    assert!(span.start > 20, "span looks too early : {:?}", span);
}

#[test]
fn a02_2_help_suggests_explicit_cast_for_numeric_pair() {
    // The diagnostic must include a help/note suggesting `... as i64`
    // (FM.3 + item-89 § A89.2 overlap).
    let bag = lower("u32_to_i64.cssl");
    let errors: Vec<_> = bag
        .iter()
        .filter(|d| matches!(d.severity, Severity::Error))
        .collect();
    let prim = errors
        .iter()
        .find(|d| d.message.contains("expected i64"))
        .expect("primary mismatch diagnostic missing");
    assert!(
        prim.notes.iter().any(|n| n.message.contains("as i64")),
        "missing `as i64` help-note ; notes = {:?}",
        prim.notes
    );
}
