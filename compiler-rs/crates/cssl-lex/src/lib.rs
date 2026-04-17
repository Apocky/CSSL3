//! CSSLv3 stage0 — CSLv3-native + Rust-hybrid lexer dispatch.
//!
//! Authoritative design : `specs/09_SYNTAX.csl` + `specs/16_DUAL_SURFACE.csl`.
//! Cross-crate decisions : `DECISIONS.md` (T1-D2 : Rust-native CSLv3 port).
//!
//! § STATUS : T2 in-progress
//!   - `token` module : unified `Token` + `TokenKind` covering both surfaces  ← Turn-2 (here)
//!   - `rust_hybrid` module : `logos`-derived lexer per §§ 09 lexical         ← Turn-3
//!   - `csl_native` module : hand-rolled port of CSLv3/specs/12_TOKENIZER     ← Turn-4
//!   - `mode` module : auto-detect + pragma + extension dispatch              ← Turn-5
//!
//! § SURFACES
//!   - Rust-hybrid  : `logos`-derived (Turn-3); §§ 09_SYNTAX lexical tables.
//!   - CSLv3-native : hand-rolled Rust port from `CSLv3/specs/12_TOKENIZER.csl`
//!                    (74-glyph master alias table) + `CSLv3/specs/13_GRAMMAR_SELF.csl`.
//! § POLICY : any divergence between the Rust-native CSLv3 port and `parser.exe --tokens`
//!            on fixture corpus is a spec-ambiguity, filed against CSLv3 (not CSSLv3).

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]

pub mod rust_hybrid;
pub mod token;

pub use token::{
    BracketKind, BracketSide, CompoundOp, Determinative, EvidenceMark, Keyword, ModalOp,
    StringFlavor, Token, TokenKind, TypeSuffix,
};

/// Crate version, exposes `CARGO_PKG_VERSION`.
pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod scaffold_tests {
    use super::STAGE0_SCAFFOLD;

    #[test]
    fn scaffold_version_present() {
        assert!(!STAGE0_SCAFFOLD.is_empty());
    }
}
