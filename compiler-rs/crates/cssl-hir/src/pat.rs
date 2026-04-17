//! HIR patterns.
//!
//! Mirrors `cssl_ast::cst::Pattern` with `Symbol` interning for binding names and path
//! references resolved via `DefId`.

use cssl_ast::Span;

use crate::arena::{DefId, HirId};
use crate::expr::HirLiteral;
use crate::symbol::Symbol;

/// A HIR pattern.
#[derive(Debug, Clone)]
pub struct HirPattern {
    pub span: Span,
    pub id: HirId,
    pub kind: HirPatternKind,
}

/// Shape of a HIR pattern.
#[derive(Debug, Clone)]
pub enum HirPatternKind {
    /// `_` — wildcard.
    Wildcard,
    /// Literal pattern.
    Literal(HirLiteral),
    /// Binding pattern `x` / `mut x`.
    Binding { mutable: bool, name: Symbol },
    /// Tuple pattern `(a, b, c)`.
    Tuple(Vec<HirPattern>),
    /// Struct pattern `Point { x, y : b }`.
    Struct {
        path: Vec<Symbol>,
        def: Option<DefId>,
        fields: Vec<HirPatternField>,
        rest: bool,
    },
    /// Variant pattern `Some(v)` / `None`.
    Variant {
        path: Vec<Symbol>,
        def: Option<DefId>,
        args: Vec<HirPattern>,
    },
    /// Or-pattern `a | b | c`.
    Or(Vec<HirPattern>),
    /// Range pattern `a..b` / `a..=b`.
    Range {
        start: Option<Box<HirPattern>>,
        end: Option<Box<HirPattern>>,
        inclusive: bool,
    },
    /// Reference binding pattern `ref x` / `ref mut x`.
    Ref {
        mutable: bool,
        inner: Box<HirPattern>,
    },
    /// Error-recovery placeholder.
    Error,
}

/// A struct-pattern field `{ name : pat }` (or `{ name }` shorthand).
#[derive(Debug, Clone)]
pub struct HirPatternField {
    pub span: Span,
    pub name: Symbol,
    /// `None` → shorthand `{ x }` ≡ `{ x : x }`.
    pub pat: Option<HirPattern>,
}

#[cfg(test)]
mod tests {
    use super::{HirPattern, HirPatternKind};
    use crate::arena::HirId;
    use cssl_ast::{SourceId, Span};

    #[test]
    fn pattern_kinds_constructible() {
        let sp = Span::new(SourceId::first(), 0, 1);
        let wild = HirPattern {
            span: sp,
            id: HirId::DUMMY,
            kind: HirPatternKind::Wildcard,
        };
        assert!(matches!(wild.kind, HirPatternKind::Wildcard));
        let err = HirPattern {
            span: sp,
            id: HirId::DUMMY,
            kind: HirPatternKind::Error,
        };
        assert!(matches!(err.kind, HirPatternKind::Error));
    }
}
