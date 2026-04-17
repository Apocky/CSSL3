//! HIR type expressions.
//!
//! Mirrors `cssl_ast::cst::Type` with paths resolved to interned symbol sequences and
//! a `DefId` slot for name-resolved references. Inference-derived annotations (IFC label,
//! capability, fully-resolved type constructors) remain placeholders here and are filled
//! in by the T3.4 inference engine.

use cssl_ast::Span;

use crate::arena::{DefId, HirId};
use crate::expr::HirExpr;
use crate::symbol::Symbol;

/// A HIR type expression.
#[derive(Debug, Clone)]
pub struct HirType {
    pub span: Span,
    pub id: HirId,
    pub kind: HirTypeKind,
}

/// Shape of a HIR type.
#[derive(Debug, Clone)]
pub enum HirTypeKind {
    /// Resolved path : `module::T<A, B>`.
    Path {
        /// Path as a sequence of interned segments.
        path: Vec<Symbol>,
        /// Optional resolved definition (filled by name-resolution pass).
        def: Option<DefId>,
        /// Type arguments (for generics).
        type_args: Vec<HirType>,
    },
    /// Tuple type `(T, U, V)` — arity 0 → unit.
    Tuple { elems: Vec<HirType> },
    /// Array `[T ; N]`.
    Array {
        elem: Box<HirType>,
        len: Box<HirExpr>,
    },
    /// Slice `[T]`.
    Slice { elem: Box<HirType> },
    /// Reference `&T` / `&mut T`.
    Reference { mutable: bool, inner: Box<HirType> },
    /// Capability wrapper `iso<T>` / `trn<T>` / `ref<T>` / `val<T>` / `box<T>` / `tag<T>`.
    Capability {
        cap: HirCapKind,
        inner: Box<HirType>,
    },
    /// Function type `fn(T1, …) -> U / ε`.
    Function {
        params: Vec<HirType>,
        return_ty: Box<HirType>,
        effect_row: Option<HirEffectRow>,
    },
    /// Refined type (T'tag | predicate-form | Lipschitz).
    Refined {
        base: Box<HirType>,
        kind: HirRefinementKind,
    },
    /// `_` — inferred type placeholder (filled at T3.4).
    Infer,
    /// Error-recovery placeholder.
    Error,
}

/// Capability-kind wrapping a type (Pony-6 set).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HirCapKind {
    Iso,
    Trn,
    Ref,
    Val,
    Box,
    Tag,
}

/// Refinement shape attached to a base type.
#[derive(Debug, Clone)]
pub enum HirRefinementKind {
    /// `T'tagname` — tag resolved by elaboration via refinement dictionary.
    Tag { name: Symbol },
    /// `{v : T | P(v)}` — explicit predicate.
    Predicate {
        binder: Symbol,
        predicate: Box<HirExpr>,
    },
    /// `SDF'L<k>` — Lipschitz bound.
    Lipschitz { bound: Box<HirExpr> },
}

/// Effect-row `/ {e1, e2<arg>, … | μ}` attached to a function type.
#[derive(Debug, Clone)]
pub struct HirEffectRow {
    pub span: Span,
    pub effects: Vec<HirEffectAnnotation>,
    /// Polymorphic tail variable name (e.g., `μ`).
    pub tail: Option<Symbol>,
}

/// A single effect-row entry.
#[derive(Debug, Clone)]
pub struct HirEffectAnnotation {
    pub span: Span,
    pub name: Vec<Symbol>,
    pub args: Vec<HirEffectArg>,
}

/// Effect-row argument — a type, an expression, or a nested effect.
#[derive(Debug, Clone)]
pub enum HirEffectArg {
    Type(HirType),
    Expr(HirExpr),
}

#[cfg(test)]
mod tests {
    use super::{HirCapKind, HirType, HirTypeKind};
    use crate::arena::HirId;
    use cssl_ast::{SourceId, Span};

    fn sp() -> Span {
        Span::new(SourceId::first(), 0, 1)
    }

    #[test]
    fn type_kind_variants_constructible() {
        let infer = HirType {
            span: sp(),
            id: HirId::DUMMY,
            kind: HirTypeKind::Infer,
        };
        assert!(matches!(infer.kind, HirTypeKind::Infer));
        let tuple = HirType {
            span: sp(),
            id: HirId::DUMMY,
            kind: HirTypeKind::Tuple { elems: Vec::new() },
        };
        assert!(matches!(tuple.kind, HirTypeKind::Tuple { .. }));
        let err = HirType {
            span: sp(),
            id: HirId::DUMMY,
            kind: HirTypeKind::Error,
        };
        assert!(matches!(err.kind, HirTypeKind::Error));
    }

    #[test]
    fn cap_kinds_enumerated() {
        let caps = [
            HirCapKind::Iso,
            HirCapKind::Trn,
            HirCapKind::Ref,
            HirCapKind::Val,
            HirCapKind::Box,
            HirCapKind::Tag,
        ];
        assert_eq!(caps.len(), 6);
    }
}
