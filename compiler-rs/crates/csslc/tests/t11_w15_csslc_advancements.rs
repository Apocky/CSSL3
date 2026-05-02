//! § T11-W15-CSSLC integration tests — soft-keyword bindings + #[test] outer-attr
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § COVERAGE
//!   Each gate validates one of the W15 parser-advancements landed in this wave
//!   by running the full csslc-check pipeline on the fixture under
//!   `tests/fixtures/`. A test is "green" when the pipeline returns
//!   `ExitCode::SUCCESS` ; this is the strongest signal that the advancement
//!   actually unblocks pure-CSSL authoring (lex → parse → HIR-lower all clean
//!   for the targeted shape).
//!
//! § REFERENCE
//!   `specs/csslc/T11-W15-kwbind-and-test-attr.csl` — design + gap-audit
//!   `specs/csslc/_BACKLOG.csl`                      — deferred-gap roadmap

use std::path::Path;
use std::process::ExitCode;

use csslc::commands::check;
use csslc::exit_code;

fn fixture_path(name: &str) -> std::path::PathBuf {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.join("tests").join("fixtures").join(name)
}

fn assert_check_succeeds(name: &str) {
    let path = fixture_path(name);
    let source = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("fixture '{name}' read-error: {e}"));
    let code = check::run_with_source(&path, &source);
    let ok: ExitCode = ExitCode::from(exit_code::SUCCESS);
    assert_eq!(
        format!("{code:?}"),
        format!("{ok:?}"),
        "fixture '{name}' must pass check ; got non-success exit"
    );
}

#[test]
fn t11_w15_kwbind_soft_keywords_as_binding_idents() {
    // § GAP-W15-A : soft-keywords (Tag/Ref/Val/Box/Iso/Trn/Type/Module/Where/
    //   Comptime/In/As) usable as binding-idents in let-bindings, fn-params,
    //   struct-fields, field-access RHS, struct-constructor field names, and
    //   named-args. Pre-W15 `let tag : T = ...` failed @ pat::parse_pattern.
    assert_check_succeeds("soft_keyword_bindings.csl");
}

#[test]
fn t11_w15_testattr_hash_bracket_outer_attribute() {
    // § GAP-W15-B : `#[test]` / `#[derive(...)]` / `#[doc = "..."]` outer-
    //   attribute form parses identically to the existing `@name(...)` form.
    //   Pre-W15 `#[test]` produced "expected an item" because the dispatch
    //   only-recognized `@`.
    assert_check_succeeds("test_attr_outer.csl");
}

#[test]
fn t11_w15_kwbind_does_not_accept_hard_keywords() {
    // § Negative : hard keywords like `fn` / `let` / `if` MUST still be rejected
    //   in pattern position — they collide with grammar boundaries and are
    //   excluded from the soft-keyword set.
    let src = "module com.test\nfn f() -> i32 {\n  let fn: i32 = 0i32 ;\n  fn\n}\n";
    let path = std::path::PathBuf::from("synthetic_negative_hard_kw.csl");
    let code = check::run_with_source(&path, src);
    let err: ExitCode = ExitCode::from(exit_code::USER_ERROR);
    assert_eq!(
        format!("{code:?}"),
        format!("{err:?}"),
        "hard-keyword `fn` MUST NOT be accepted as binding-name"
    );
}

#[test]
fn t11_w15_kwbind_field_access_with_soft_kw_name() {
    // § Field access on a struct whose field-name is a soft-keyword.
    let src = "module com.test\n\
               struct Foo { tag: u32, val: u32 }\n\
               fn read_tag(f: Foo) -> u32 {\n\
                   f.tag\n\
               }\n\
               fn read_val(f: Foo) -> u32 {\n\
                   f.val\n\
               }\n";
    let path = std::path::PathBuf::from("synthetic_field_access.csl");
    let code = check::run_with_source(&path, src);
    let ok: ExitCode = ExitCode::from(exit_code::SUCCESS);
    assert_eq!(
        format!("{code:?}"),
        format!("{ok:?}"),
        "field-access on soft-keyword-named field MUST parse + lower clean"
    );
}

#[test]
fn t11_w15_testattr_hash_test_followed_by_at_attr() {
    // § Mixed-form : both `#[..]` and `@..` attrs may stack on the same item.
    let src = "module com.test\n\
               @vertex\n\
               #[test]\n\
               fn mixed_attrs() -> u32 { 0u32 }\n";
    let path = std::path::PathBuf::from("synthetic_mixed_attrs.csl");
    let code = check::run_with_source(&path, src);
    let ok: ExitCode = ExitCode::from(exit_code::SUCCESS);
    assert_eq!(
        format!("{code:?}"),
        format!("{ok:?}"),
        "mixed @-attr + #[..]-attr stack MUST parse"
    );
}

#[test]
fn t11_w15_badhex_namespace_prefix_recovery() {
    // § GAP-W15-C : the lexer's strict-hex regex consumed the longest valid-
    //   hex prefix and emitted `BRASSMARu64` as a separate Ident, breaking
    //   call-args parse. Post-W15 a recovery regex variant lexes the entire
    //   `0x...BRASSMARu64` as a single IntLiteral so the parser doesn't
    //   cascade.
    assert_check_succeeds("badhex_recovery.csl");
}

#[test]
fn t11_w15_testattr_does_not_break_inner_attrs() {
    // § Negative : the `#![..]` inner-attr form MUST still parse correctly
    //   (different from `#[..]` outer by the `!` between `#` and `[`).
    let src = "#![surface = \"rust-hybrid\"]\n\
               module com.test\n\
               fn f() -> u32 { 0u32 }\n";
    let path = std::path::PathBuf::from("synthetic_inner_attr.csl");
    let code = check::run_with_source(&path, src);
    let ok: ExitCode = ExitCode::from(exit_code::SUCCESS);
    assert_eq!(
        format!("{code:?}"),
        format!("{ok:?}"),
        "inner-attr `#![..]` MUST still parse"
    );
}
