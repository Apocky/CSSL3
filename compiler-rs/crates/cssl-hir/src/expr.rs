//! HIR expressions, blocks, literals, and binary/unary operators.
//!
//! Mirrors `cssl_ast::cst::Expr` / `cst::Block` / `cst::Literal` / `cst::BinOp` / `cst::UnOp`
//! but with :
//!   - Identifiers interned as `Symbol`.
//!   - Path references carrying an `Option<DefId>` filled by name-resolution.
//!   - Every node tagged with a fresh `HirId`.
//!   - Type-annotation slot left `Infer` for T3.4 inference to fill.

use cssl_ast::Span;

use crate::arena::{DefId, HirId};
use crate::attr::HirAttr;
use crate::pat::HirPattern;
use crate::stmt::HirStmt;
use crate::symbol::Symbol;
use crate::ty::HirType;

/// A HIR expression node.
#[derive(Debug, Clone)]
pub struct HirExpr {
    pub span: Span,
    pub id: HirId,
    pub attrs: Vec<HirAttr>,
    pub kind: HirExprKind,
}

/// Shape of a HIR expression. Flat enum — precedence is encoded in the tree shape.
#[derive(Debug, Clone)]
pub enum HirExprKind {
    /// Literal value.
    Literal(HirLiteral),
    /// Resolved path expression.
    Path {
        segments: Vec<Symbol>,
        def: Option<DefId>,
    },
    /// Function application : `f(arg1, arg2)`.
    Call {
        callee: Box<HirExpr>,
        args: Vec<HirCallArg>,
    },
    /// Field / method access : `obj.field`.
    Field { obj: Box<HirExpr>, name: Symbol },
    /// Indexing : `arr[idx]`.
    Index {
        obj: Box<HirExpr>,
        index: Box<HirExpr>,
    },
    /// Binary operator application.
    Binary {
        op: HirBinOp,
        lhs: Box<HirExpr>,
        rhs: Box<HirExpr>,
    },
    /// Unary operator application.
    Unary { op: HirUnOp, operand: Box<HirExpr> },
    /// Block expression.
    Block(HirBlock),
    /// `if cond { then } else { else }`.
    If {
        cond: Box<HirExpr>,
        then_branch: HirBlock,
        else_branch: Option<Box<HirExpr>>,
    },
    /// `match scrutinee { arm* }`.
    Match {
        scrutinee: Box<HirExpr>,
        arms: Vec<HirMatchArm>,
    },
    /// `for pat in iter { body }`.
    For {
        pat: HirPattern,
        iter: Box<HirExpr>,
        body: HirBlock,
    },
    /// `while cond { body }`.
    While { cond: Box<HirExpr>, body: HirBlock },
    /// `loop { body }`.
    Loop { body: HirBlock },
    /// `return [expr] ?`.
    Return { value: Option<Box<HirExpr>> },
    /// `break [label] [value] ?`.
    Break {
        label: Option<Symbol>,
        value: Option<Box<HirExpr>>,
    },
    /// `continue [label] ?`.
    Continue { label: Option<Symbol> },
    /// `|params| -> return { body }`.
    Lambda {
        params: Vec<HirLambdaParam>,
        return_ty: Option<HirType>,
        body: Box<HirExpr>,
    },
    /// Assignment `a = b` / compound-assign `a += b`.
    Assign {
        op: Option<HirBinOp>,
        lhs: Box<HirExpr>,
        rhs: Box<HirExpr>,
    },
    /// `expr as Ty`.
    Cast { expr: Box<HirExpr>, ty: HirType },
    /// `lo..hi` / `lo..=hi` / `lo..` / `..hi`.
    Range {
        lo: Option<Box<HirExpr>>,
        hi: Option<Box<HirExpr>>,
        inclusive: bool,
    },
    /// `expr |> f`.
    Pipeline {
        lhs: Box<HirExpr>,
        rhs: Box<HirExpr>,
    },
    /// `expr ? : default` — early-return default.
    TryDefault {
        expr: Box<HirExpr>,
        default: Box<HirExpr>,
    },
    /// `expr ?` — `Try` propagation.
    Try { expr: Box<HirExpr> },
    /// `perform Effect::op(args)`.
    Perform {
        path: Vec<Symbol>,
        def: Option<DefId>,
        args: Vec<HirCallArg>,
    },
    /// `with handler { body }`.
    With {
        handler: Box<HirExpr>,
        body: HirBlock,
    },
    /// `region 'r { body }`.
    Region {
        label: Option<Symbol>,
        body: HirBlock,
    },
    /// Tuple constructor.
    Tuple(Vec<HirExpr>),
    /// Array constructor.
    Array(HirArrayExpr),
    /// Struct constructor `Point { x : 1, y : 2 [, ..base] }`.
    Struct {
        path: Vec<Symbol>,
        def: Option<DefId>,
        fields: Vec<HirStructFieldInit>,
        spread: Option<Box<HirExpr>>,
    },
    /// `#run expr` — compile-time eval marker.
    Run { expr: Box<HirExpr> },
    /// CSLv3-native compound formation `A op B`.
    Compound {
        op: HirCompoundOp,
        lhs: Box<HirExpr>,
        rhs: Box<HirExpr>,
    },
    /// `§§ path` — section reference.
    SectionRef { path: Vec<Symbol> },
    /// Parenthesized grouping — preserved for round-trip.
    Paren(Box<HirExpr>),
    /// Error-recovery placeholder.
    Error,
}

/// Lambda parameter — simplified form for T3.3 ; full inference happens at T3.4.
#[derive(Debug, Clone)]
pub struct HirLambdaParam {
    pub span: Span,
    pub pat: HirPattern,
    /// `None` → `_` (to be inferred by T3.4).
    pub ty: Option<HirType>,
}

/// Function-call argument — positional or named.
#[derive(Debug, Clone)]
pub enum HirCallArg {
    Positional(HirExpr),
    Named { name: Symbol, value: HirExpr },
}

/// Struct-field initializer `name : value` or shorthand `name`.
#[derive(Debug, Clone)]
pub struct HirStructFieldInit {
    pub span: Span,
    pub name: Symbol,
    /// `None` → shorthand.
    pub value: Option<HirExpr>,
}

/// Array-expression form.
#[derive(Debug, Clone)]
pub enum HirArrayExpr {
    /// `[a, b, c]`.
    List(Vec<HirExpr>),
    /// `[elem ; len]`.
    Repeat {
        elem: Box<HirExpr>,
        len: Box<HirExpr>,
    },
}

/// Match arm `pat [if guard] => body`.
#[derive(Debug, Clone)]
pub struct HirMatchArm {
    pub span: Span,
    pub attrs: Vec<HirAttr>,
    pub pat: HirPattern,
    pub guard: Option<HirExpr>,
    pub body: HirExpr,
}

/// A block `{ stmts … trailing? }`.
#[derive(Debug, Clone)]
pub struct HirBlock {
    pub span: Span,
    pub id: HirId,
    pub stmts: Vec<HirStmt>,
    pub trailing: Option<Box<HirExpr>>,
}

/// Literal value.
#[derive(Debug, Clone)]
pub struct HirLiteral {
    pub span: Span,
    pub kind: HirLiteralKind,
}

/// Literal shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HirLiteralKind {
    Int,
    Float,
    Str,
    Char,
    Bool(bool),
    Unit,
}

/// Binary operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HirBinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
    Implies,
    Entails,
}

/// Unary operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HirUnOp {
    Not,
    Neg,
    BitNot,
    Ref,
    RefMut,
    Deref,
}

/// CSLv3-native compound-formation operator (per §§ 13).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HirCompoundOp {
    /// `.` — tatpuruṣa : B-of-A.
    Tp,
    /// `+` — dvandva : co-equal conjunction.
    Dv,
    /// `-` — karmadhāraya : B-that-is-A.
    Kd,
    /// `⊗` — bahuvrīhi : thing-having-A+B.
    Bv,
    /// `@` — avyayībhāva : at/per/in-scope-of.
    Av,
}

#[cfg(test)]
mod tests {
    use super::{HirBinOp, HirCompoundOp, HirExpr, HirExprKind, HirLiteral, HirLiteralKind};
    use crate::arena::HirId;
    use cssl_ast::{SourceId, Span};

    fn sp() -> Span {
        Span::new(SourceId::first(), 0, 1)
    }

    #[test]
    fn literal_constructs() {
        let _e = HirExpr {
            span: sp(),
            id: HirId::DUMMY,
            attrs: Vec::new(),
            kind: HirExprKind::Literal(HirLiteral {
                span: sp(),
                kind: HirLiteralKind::Int,
            }),
        };
    }

    #[test]
    fn binop_variants_count() {
        let ops = [
            HirBinOp::Add,
            HirBinOp::Sub,
            HirBinOp::Mul,
            HirBinOp::Div,
            HirBinOp::Rem,
            HirBinOp::Eq,
            HirBinOp::Ne,
            HirBinOp::Lt,
            HirBinOp::Le,
            HirBinOp::Gt,
            HirBinOp::Ge,
            HirBinOp::And,
            HirBinOp::Or,
            HirBinOp::BitAnd,
            HirBinOp::BitOr,
            HirBinOp::BitXor,
            HirBinOp::Shl,
            HirBinOp::Shr,
            HirBinOp::Implies,
            HirBinOp::Entails,
        ];
        assert_eq!(ops.len(), 20);
    }

    #[test]
    fn compound_ops_5_variants() {
        let ops = [
            HirCompoundOp::Tp,
            HirCompoundOp::Dv,
            HirCompoundOp::Kd,
            HirCompoundOp::Bv,
            HirCompoundOp::Av,
        ];
        assert_eq!(ops.len(), 5);
    }
}
