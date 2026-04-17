//! CSSLv3 stage0 — CSLv3-native + Rust-hybrid lexer dispatch.
//!
//! Authoritative design : `specs/09_SYNTAX.csl` + `specs/16_DUAL_SURFACE.csl`.
//! Cross-crate decisions : `DECISIONS.md` (T1-D2 : Rust-native CSLv3 port).
//!
//! § STATUS : T2 in-progress
//!   - `token` module : unified `Token` + `TokenKind` covering both surfaces
//!   - `rust_hybrid` module : `logos`-derived lexer per §§ 09 lexical
//!   - `csl_native` module : hand-rolled port of CSLv3/specs/12_TOKENIZER + 13_GRAMMAR_SELF
//!   - `mode` module : auto-detect + pragma + extension dispatch
//!
//! § SURFACE-DISPATCH
//!   The top-level [`lex`] function consults the `SourceFile::surface` field :
//!     - `Surface::RustHybrid` → `rust_hybrid::lex`
//!     - `Surface::CslNative`  → `csl_native::lex`
//!     - `Surface::Auto`       → run `mode::detect` then dispatch
//!
//! § POLICY : any divergence between the Rust-native CSLv3 port and `parser.exe --tokens`
//!            on fixture corpus is a spec-ambiguity, filed against CSLv3 (not CSSLv3).

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]

pub mod csl_native;
pub mod mode;
pub mod rust_hybrid;
pub mod token;

pub use mode::{detect, Detection, Reason};
pub use token::{
    BracketKind, BracketSide, CompoundOp, Determinative, EvidenceMark, Keyword, ModalOp,
    StringFlavor, Token, TokenKind, TypeSuffix,
};

use cssl_ast::{SourceFile, Surface};

/// Lex a `SourceFile` into a vector of tokens, dispatching on `source.surface`.
///
/// If `source.surface == Surface::Auto`, runs [`mode::detect`] on the contents to pick the
/// concrete surface. The detection result is not recorded back into `source` (caller may do
/// so separately via a fresh `SourceFile` if needed).
#[must_use]
pub fn lex(source: &SourceFile) -> Vec<Token> {
    match source.surface {
        Surface::RustHybrid => rust_hybrid::lex(source),
        Surface::CslNative => csl_native::lex(source),
        Surface::Auto => {
            let detected = mode::detect(&source.path, &source.contents);
            match detected.surface {
                Surface::CslNative => csl_native::lex(source),
                // default and RustHybrid both go through the Rust-hybrid lexer
                Surface::RustHybrid | Surface::Auto => rust_hybrid::lex(source),
            }
        }
    }
}

/// Crate version, exposes `CARGO_PKG_VERSION`.
pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod dispatch_tests {
    use super::{lex, STAGE0_SCAFFOLD};
    use crate::token::TokenKind;
    use cssl_ast::{SourceFile, SourceId, Surface};

    fn mk(path: &str, src: &str, surface: Surface) -> SourceFile {
        SourceFile::new(SourceId::first(), path, src, surface)
    }

    #[test]
    fn scaffold_version_present() {
        assert!(!STAGE0_SCAFFOLD.is_empty());
    }

    #[test]
    fn dispatch_rust_hybrid_explicit() {
        let f = mk("foo.cssl-rust", "fn x() {}", Surface::RustHybrid);
        let toks = lex(&f);
        // Rust-hybrid surface emits `Keyword(Fn)` for `fn`
        assert!(toks
            .iter()
            .any(|t| matches!(t.kind, TokenKind::Keyword(crate::token::Keyword::Fn))));
    }

    #[test]
    fn dispatch_csl_native_explicit() {
        let f = mk("foo.cssl-csl", "§ foo", Surface::CslNative);
        let toks = lex(&f);
        assert!(toks.iter().any(|t| t.kind == TokenKind::Section));
    }

    #[test]
    fn dispatch_auto_detects_csl_from_section_glyph() {
        let f = mk("foo.cssl", "§ prose\n", Surface::Auto);
        let toks = lex(&f);
        assert!(toks.iter().any(|t| t.kind == TokenKind::Section));
    }

    #[test]
    fn dispatch_auto_detects_rust_from_fn_keyword() {
        let f = mk("foo.cssl", "fn bar() {}\n", Surface::Auto);
        let toks = lex(&f);
        assert!(toks
            .iter()
            .any(|t| matches!(t.kind, TokenKind::Keyword(crate::token::Keyword::Fn))));
    }

    #[test]
    fn dispatch_auto_extension_csl() {
        let f = mk("x.cssl-csl", "§ x", Surface::Auto);
        let toks = lex(&f);
        assert!(toks.iter().any(|t| t.kind == TokenKind::Section));
    }
}
