//! Statement + block surface façade.
//!
//! § Statement parsing lives inline in `expr.rs` (`parse_block` / let-inside-block
//! handling) because the block-trailing-expression rule is easiest expressed where the
//! expression parser lives. This module re-exports the public entry points so callers
//! can `use crate::rust_hybrid::stmt::{parse_block}` with a more descriptive path.

pub use crate::rust_hybrid::expr::parse_block;

#[cfg(test)]
mod tests {
    use super::parse_block;
    use crate::cursor::TokenCursor;
    use cssl_ast::{DiagnosticBag, SourceFile, SourceId, Surface};

    #[test]
    fn reexport_reachable() {
        let f = SourceFile::new(SourceId::first(), "<t>", "{ }", Surface::RustHybrid);
        let toks = cssl_lex::lex(&f);
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let b = parse_block(&mut c, &mut bag);
        assert!(b.stmts.is_empty());
        assert!(b.trailing.is_none());
    }
}
