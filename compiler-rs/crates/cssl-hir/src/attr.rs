//! HIR attributes — outer `@name(args)` + inner `#![name = …]`.
//!
//! Mirrors `cssl_ast::cst::Attr` but with the path resolved to a `Symbol` sequence and
//! argument expressions elaborated to `HirExpr`.

use cssl_ast::Span;

use crate::expr::HirExpr;
use crate::symbol::Symbol;

/// Kind of attribute application (mirrors `cst::AttrKind`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HirAttrKind {
    /// `@name(…)` placed before an item.
    Outer,
    /// `#![name = "…"]` placed at file-top or block-top.
    Inner,
}

/// A resolved attribute application.
#[derive(Debug, Clone)]
pub struct HirAttr {
    pub span: Span,
    pub kind: HirAttrKind,
    /// Dotted path as a sequence of interned symbols (e.g., `["lipschitz"]`).
    pub path: Vec<Symbol>,
    /// Attribute arguments.
    pub args: Vec<HirAttrArg>,
}

/// A single attribute argument.
#[derive(Debug, Clone)]
pub enum HirAttrArg {
    /// `@attr(expr)` — positional expression.
    Positional(HirExpr),
    /// `@attr(name = expr)` — named key-value.
    Named { name: Symbol, value: HirExpr },
}

impl HirAttr {
    /// `true` iff the outer attribute's path matches a single-segment name.
    #[must_use]
    pub fn is_simple(&self, target: Symbol) -> bool {
        self.path.len() == 1 && self.path[0] == target
    }
}

#[cfg(test)]
mod tests {
    use super::{HirAttr, HirAttrArg, HirAttrKind};
    use crate::symbol::Interner;
    use cssl_ast::{SourceId, Span};

    fn sp(start: u32, end: u32) -> Span {
        Span::new(SourceId::first(), start, end)
    }

    #[test]
    fn attr_is_simple_matches_single_segment() {
        let interner = Interner::new();
        let name = interner.intern("differentiable");
        let attr = HirAttr {
            span: sp(0, 15),
            kind: HirAttrKind::Outer,
            path: vec![name],
            args: Vec::new(),
        };
        assert!(attr.is_simple(name));
    }

    #[test]
    fn attr_is_simple_rejects_multi_segment() {
        let interner = Interner::new();
        let a = interner.intern("a");
        let b = interner.intern("b");
        let attr = HirAttr {
            span: sp(0, 3),
            kind: HirAttrKind::Outer,
            path: vec![a, b],
            args: Vec::new(),
        };
        assert!(!attr.is_simple(a));
    }

    #[test]
    fn attr_arg_both_shapes_constructible() {
        let interner = Interner::new();
        let key = interner.intern("k");
        let dummy_expr = crate::expr::HirExpr {
            span: Span::DUMMY,
            id: crate::arena::HirId::DUMMY,
            attrs: Vec::new(),
            kind: crate::expr::HirExprKind::Error,
        };
        let pos = HirAttrArg::Positional(dummy_expr.clone());
        assert!(matches!(pos, HirAttrArg::Positional(_)));
        let named = HirAttrArg::Named {
            name: key,
            value: dummy_expr,
        };
        assert!(matches!(named, HirAttrArg::Named { .. }));
    }
}
