//! Integration tests exercising the full lex pipeline over realistic fixtures.
//!
//! Fixtures live under `tests/fixtures/` and represent small but representative
//! samples of each surface. Assertions focus on *coverage* (which token kinds appear)
//! rather than exact sequences, so fixture edits don't cascade.

use cssl_ast::{SourceFile, SourceId, Surface};
use cssl_lex::{lex, TokenKind};

fn load_fixture(name: &str, surface: Surface) -> SourceFile {
    let path = format!("tests/fixtures/{name}");
    let contents =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("fixture {path} read: {e}"));
    SourceFile::new(SourceId::first(), name, contents, surface)
}

/// Full token-stream of a fixture, driven by the public `cssl_lex::lex` dispatcher.
fn tokenize(name: &str, surface: Surface) -> Vec<TokenKind> {
    lex(&load_fixture(name, surface))
        .into_iter()
        .map(|t| t.kind)
        .collect()
}

// ════════════════════════════════════════════════════════════════════════════
// § Rust-hybrid fixture
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn rust_hybrid_fixture_tokenizes_without_errors() {
    let toks = tokenize("rust_hybrid_basic.cssl-rust", Surface::RustHybrid);
    assert!(
        !toks.contains(&TokenKind::Error),
        "lex produced Error tokens"
    );
    assert_eq!(toks.last(), Some(&TokenKind::Eof));
}

#[test]
fn rust_hybrid_fixture_has_expected_kinds() {
    let toks = tokenize("rust_hybrid_basic.cssl-rust", Surface::RustHybrid);
    assert!(
        toks.iter().any(|k| matches!(k, TokenKind::Keyword(_))),
        "expected at least one keyword (fn / module / etc)",
    );
    assert!(toks.contains(&TokenKind::Arrow), "expected `->` arrow");
    assert!(
        toks.contains(&TokenKind::At),
        "expected `@` attribute prefix"
    );
    assert!(
        toks.contains(&TokenKind::Slash),
        "expected `/` effect-row separator"
    );
    assert!(toks.iter().any(|k| matches!(k, TokenKind::LineComment)));
}

// ════════════════════════════════════════════════════════════════════════════
// § CSLv3-native fixture
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn csl_native_fixture_tokenizes_without_errors() {
    let file = load_fixture("csl_native_basic.cssl-csl", Surface::CslNative);
    let toks = lex(&file);
    let mut errors = Vec::new();
    for t in &toks {
        if t.kind == TokenKind::Error {
            let slice = file
                .slice(t.span.start, t.span.end)
                .unwrap_or("<bad-slice>");
            let pos = file.position_of(t.span.start);
            errors.push(format!("{pos}: {slice:?}"));
        }
    }
    assert!(errors.is_empty(), "lex produced Error tokens: {errors:#?}");
    assert_eq!(toks.last().map(|t| t.kind), Some(TokenKind::Eof));
}

#[test]
fn csl_native_fixture_has_expected_kinds() {
    let toks = tokenize("csl_native_basic.cssl-csl", Surface::CslNative);
    assert!(toks.contains(&TokenKind::Section), "expected `§` marker");
    assert!(
        toks.iter().any(|k| matches!(k, TokenKind::Modal(_))),
        "expected at least one modal (W!/I>/etc)",
    );
    assert!(
        toks.iter().any(|k| matches!(k, TokenKind::Evidence(_))),
        "expected at least one evidence mark",
    );
    assert!(toks.contains(&TokenKind::ElemOf), "expected `∈` element-of");
    assert!(toks.contains(&TokenKind::Indent), "expected Indent tokens");
    assert!(toks.contains(&TokenKind::Dedent), "expected Dedent tokens");
    assert!(toks.contains(&TokenKind::Arrow), "expected `→` arrow");
    assert!(toks.contains(&TokenKind::Qed), "expected `∎` terminator");
}

// ════════════════════════════════════════════════════════════════════════════
// § Surface dispatch via Auto
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn auto_surface_dispatch_csl_via_extension() {
    let f = load_fixture("csl_native_basic.cssl-csl", Surface::Auto);
    assert!(lex(&f).into_iter().any(|t| t.kind == TokenKind::Section));
}

#[test]
fn auto_surface_dispatch_rust_via_extension() {
    let f = load_fixture("rust_hybrid_basic.cssl-rust", Surface::Auto);
    assert!(lex(&f)
        .into_iter()
        .any(|t| matches!(t.kind, TokenKind::Keyword(cssl_lex::Keyword::Fn))));
}

// ════════════════════════════════════════════════════════════════════════════
// § Differential-oracle precondition (T6 ship-gate scaffolding)
// ════════════════════════════════════════════════════════════════════════════

/// Preflight check : both fixtures lex cleanly on *both* lexers without crashes.
/// The `@differential` oracle (enabled at T10+ when `csslc tokens --json` is wired) will
/// compare these fixtures against `parser.exe --tokens` on matching inputs in the
/// `CSLv3` repository. For T2 scaffold we assert only that both paths terminate.
#[test]
fn differential_oracle_preflight_both_surfaces_terminate() {
    let r = tokenize("rust_hybrid_basic.cssl-rust", Surface::RustHybrid);
    let c = tokenize("csl_native_basic.cssl-csl", Surface::CslNative);
    assert_eq!(r.last(), Some(&TokenKind::Eof));
    assert_eq!(c.last(), Some(&TokenKind::Eof));
}
