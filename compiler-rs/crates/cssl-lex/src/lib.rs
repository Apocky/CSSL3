//! CSSLv3 stage0 — CSLv3-native + Rust-hybrid lexer dispatch.
//!
//! Authoritative design : `specs/09_SYNTAX.csl` + `specs/16_DUAL_SURFACE.csl`.
//! Cross-crate decisions : `DECISIONS.md` (T1-D2 : Rust-native CSLv3 port).
//!
//! § STATUS : T2 scaffold — lexer modules pending.
//! § SURFACES
//!   - Rust-hybrid  : `logos`-derived, per `specs/09_SYNTAX.csl` lexical section.
//!   - CSLv3-native : hand-rolled Rust port from `CSLv3/specs/12_TOKENIZER.csl`
//!                    (74-glyph master alias table) + `CSLv3/specs/13_GRAMMAR_SELF.csl`.

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]

/// Crate version, exposes `CARGO_PKG_VERSION`.
pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod scaffold_tests {
    #[test]
    fn scaffold_version_present() {
        assert!(!super::STAGE0_SCAFFOLD.is_empty());
    }
}
