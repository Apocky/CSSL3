//! § T11-W11-CSSLC integration tests — parser-advancement gates
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § COVERAGE
//!   Each gate validates one of the three parser-advancements landed in the
//!   T11-W11 wave by running the full csslc-check pipeline on the fixture
//!   under `tests/fixtures/`. A test is "green" when the pipeline returns
//!   `ExitCode::SUCCESS` ; this is the strongest signal that the
//!   advancement actually unblocks pure-CSSL authoring (lex → parse → HIR-
//!   lower all clean for the targeted shape).
//!
//! § REFERENCE
//!   `specs/csslc/T11-W11-parser-advancements.csl` — design + gap-audit
//!   `specs/csslc/_BACKLOG.csl` — deferred-gap roadmap

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
fn t11_w11_modkw_keyword_segment_in_module_path() {
    // § Pre-W11 : `loa.systems.run` failed parse because `run` is
    // `Keyword::Run` (used as `#run` macro-prefix). Post-W11 the path-segment
    // accepts Keyword-tokens after a separator.
    assert_check_succeeds("module_path_with_keyword_segment.csl");
}

#[test]
fn t11_w11_nostruct_if_expr_struct_disambiguation() {
    // § Pre-W11 : `if x > a { a } else { x }` parsed wrong because
    // `looks_like_struct_body` peek-ahead saw `Ident `}`` shape and accepted
    // `a { a }` as a struct-constructor. Post-W11 the parser pushes
    // `Restriction::NoStructLiteral` for cond-parse + the struct-constructor
    // dispatch consults it via `in_context_forbidding_struct_brace`.
    assert_check_succeeds("if_expr_struct_disambiguation.csl");
}

#[test]
fn t11_w11_conststmt_const_stmt_in_fn_body() {
    // § Pre-W11 : `const NAME : ty = expr ;` inside a fn body produced
    // "expected an expression" because `parse_block` only-recognized
    // `let` as a binding-stmt keyword. Post-W11 `parse_block` also routes
    // `Keyword::Const` to `parse_const_stmt` which lowers identically to
    // `let : ty = expr ;` (mutable forced-false).
    assert_check_succeeds("const_stmt_in_fn_body.csl");
}

#[test]
fn t11_w11_modkw_does_not_accept_keyword_as_first_segment() {
    // § Negative : `module fn` MUST still be rejected — the first segment
    // is parsed via `parse_ident` which strict-requires `TokenKind::Ident`.
    let src = "module fn\n";
    let path = std::path::PathBuf::from("synthetic_negative_modkw.csl");
    let code = check::run_with_source(&path, src);
    let err: ExitCode = ExitCode::from(exit_code::USER_ERROR);
    assert_eq!(
        format!("{code:?}"),
        format!("{err:?}"),
        "first-segment-as-keyword MUST NOT pass check ; got success"
    );
}

#[test]
fn t11_w11_nostruct_does_not_break_valid_struct_constructors() {
    // § Negative : the NoStructLiteral restriction is scoped to cond-parse
    // ONLY ; struct-constructors at body-position must still parse. This
    // gate ensures the push/pop pair correctly-restores the prior bits.
    let src = "module com.test\n\
               struct Point { x: u32, y: u32 }\n\
               fn make_point(x: u32, y: u32) -> Point {\n\
                   Point { x: x, y: y }\n\
               }\n";
    let path = std::path::PathBuf::from("synthetic_struct_at_body.csl");
    let code = check::run_with_source(&path, src);
    let ok: ExitCode = ExitCode::from(exit_code::SUCCESS);
    assert_eq!(
        format!("{code:?}"),
        format!("{ok:?}"),
        "struct-constructor at body-position MUST still parse ; got error"
    );
}

#[test]
fn t11_w11_conststmt_const_with_inferred_type_works() {
    // § Const-stmt without an explicit type-annotation infers from the RHS.
    // The shape mirrors `let X = expr ;` plus `mutable: false`.
    let src = "module com.test\n\
               fn f() -> i32 {\n\
                   const TWELVE = 12 ;\n\
                   TWELVE\n\
               }\n";
    let path = std::path::PathBuf::from("synthetic_const_no_ty.csl");
    let code = check::run_with_source(&path, src);
    let ok: ExitCode = ExitCode::from(exit_code::SUCCESS);
    assert_eq!(
        format!("{code:?}"),
        format!("{ok:?}"),
        "const-stmt without type-annotation should parse + lower-clean"
    );
}
