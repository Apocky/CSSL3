//! Concrete syntax tree (CST) — source-preserving node types shared by both surfaces.
//!
//! § SPEC SOURCES
//!   - `specs/02_IR.csl` § HIR (shape at elaboration time)
//!   - `specs/03_TYPES.csl` (type grammar)
//!   - `specs/09_SYNTAX.csl` § RUST-HYBRID SURFACE (item / expression / type grammar)
//!   - `CSLv3/specs/13_GRAMMAR_SELF.csl` (compound formation + slot-template + morpheme)
//! § DECISIONS : `DECISIONS.md` T3-D1..D4.
//!
//! § DESIGN
//!   Both lexer surfaces (Rust-hybrid + CSLv3-native) parse into this same CST. Downstream
//!   elaboration (in `cssl-hir`) interns strings + runs name resolution + type inference.
//!   Every node carries a `Span`. No strings live in the CST — identifier text is re-sliced
//!   from `SourceFile` by `span` when needed (see T3-D2).
//!
//! § NON-GOAL (for T3 scope)
//!   Trivia preservation (comments + whitespace) — the CST drops trivia. The formatter path
//!   (at stage1) will layer a trivia-preserving wrapper on top; for now, comments only survive
//!   through token-stream formatting, not CST.

#![allow(clippy::large_enum_variant)]

use crate::span::Span;

// ════════════════════════════════════════════════════════════════════════════
// § TOP-LEVEL : Module + ModulePath + Ident
// ════════════════════════════════════════════════════════════════════════════

/// A complete compilation unit (one source file).
#[derive(Debug, Clone)]
pub struct Module {
    pub span: Span,
    /// Inner attributes (e.g., `#![surface = "csl"]`, `#![forbid(unsafe_code)]`).
    pub inner_attrs: Vec<Attr>,
    /// Optional explicit module path declaration (first non-comment line).
    pub path: Option<ModulePath>,
    /// Top-level items in source order.
    pub items: Vec<Item>,
}

/// Dotted path of identifiers used for `use X::Y::Z`, `module foo.bar.baz`, attribute names.
#[derive(Debug, Clone)]
pub struct ModulePath {
    pub span: Span,
    /// Non-empty sequence of segments.
    pub segments: Vec<Ident>,
}

/// Identifier reference — text is re-sliced from source via `span`.
#[derive(Debug, Clone, Copy)]
pub struct Ident {
    pub span: Span,
}

/// Visibility marker on items.
#[derive(Debug, Clone, Copy)]
pub struct Visibility {
    pub span: Span,
    pub kind: VisibilityKind,
}

/// Visibility kinds recognized by the `pub` keyword.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisibilityKind {
    /// No `pub` marker — default module-private.
    Private,
    /// Plain `pub` — public to all consumers.
    Public,
}

// ════════════════════════════════════════════════════════════════════════════
// § ATTRIBUTES
// ════════════════════════════════════════════════════════════════════════════

/// An attribute application like `@differentiable` or `@lipschitz(k = 1.0)`.
///
/// Inner attributes use `#![…]` in Rust-hybrid; outer attributes use `@name(…)`.
#[derive(Debug, Clone)]
pub struct Attr {
    pub span: Span,
    /// Kind of attribute application.
    pub kind: AttrKind,
    /// Dotted path to the attribute name (e.g., `differentiable`, `Target`, `lipschitz`).
    pub path: ModulePath,
    /// Optional argument list.
    pub args: Vec<AttrArg>,
}

/// Whether the attribute is an outer form (`@name`) or inner form (`#![name]`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttrKind {
    /// `@name(…)` placed before an item.
    Outer,
    /// `#![name = "…"]` placed at file-top or block-top.
    Inner,
}

/// A single attribute argument — either positional (`@attr(arg)`) or named (`@attr(k = v)`).
#[derive(Debug, Clone)]
pub enum AttrArg {
    Positional(Expr),
    Named { name: Ident, value: Expr },
}

// ════════════════════════════════════════════════════════════════════════════
// § ITEMS — top-level declarations
// ════════════════════════════════════════════════════════════════════════════

/// A top-level or block-level item.
#[derive(Debug, Clone)]
pub enum Item {
    Fn(FnItem),
    /// `extern fn name(params) -> ret` — body-less FFI declaration. Stage-0
    /// implicit ABI is "C". The lowered MIR fn is signature-only ; downstream
    /// codegen registers it as an external import the linker resolves.
    ExternFn(ExternFnItem),
    Struct(StructItem),
    Enum(EnumItem),
    Interface(InterfaceItem),
    Impl(ImplItem),
    Effect(EffectItem),
    Handler(HandlerItem),
    TypeAlias(TypeAliasItem),
    Use(UseItem),
    Const(ConstItem),
    Module(ModuleItem),
}

impl Item {
    #[must_use]
    pub fn span(&self) -> Span {
        match self {
            Self::Fn(i) => i.span,
            Self::ExternFn(i) => i.span,
            Self::Struct(i) => i.span,
            Self::Enum(i) => i.span,
            Self::Interface(i) => i.span,
            Self::Impl(i) => i.span,
            Self::Effect(i) => i.span,
            Self::Handler(i) => i.span,
            Self::TypeAlias(i) => i.span,
            Self::Use(i) => i.span,
            Self::Const(i) => i.span,
            Self::Module(i) => i.span,
        }
    }
}

/// `fn name<G>(params) -> ret / effects where-clauses { body }`
#[derive(Debug, Clone)]
pub struct FnItem {
    pub span: Span,
    pub attrs: Vec<Attr>,
    pub visibility: Visibility,
    pub name: Ident,
    pub generics: Generics,
    pub params: Vec<Param>,
    pub return_ty: Option<Type>,
    pub effect_row: Option<EffectRow>,
    pub where_clauses: Vec<WhereClause>,
    /// `None` for interface-method signatures (no body); `Some(block)` for concrete fns.
    pub body: Option<Block>,
}

/// `extern fn name(params) -> ret` — body-less FFI declaration.
///
/// § PURPOSE
/// Allows CSSL source to forward-declare host symbols (e.g., `__cssl_*`
/// runtime calls + LoA scene-action thunks) so the resolver can wire
/// `func.call` ops to external imports. Has no body, no generics, no
/// effect-row (effects on host symbols flow through MIR-level cap checks
/// instead of CSSL effect rows), and no where-clauses.
///
/// § ABI
/// Stage-0 implicit ABI is "C". The grammar accepts an optional ABI
/// string-literal between `extern` and `fn` (e.g. `extern "C" fn foo(…)`,
/// `extern "system" fn bar(…)`). When the literal is present, its span is
/// preserved in `abi_span` so downstream passes (HIR lowering) can resolve
/// the verbatim ABI text via `SourceFile::slice` ; the unquoted text is
/// stored in `abi` at lower time. The parser itself only records the span
/// (the parser has no `SourceFile` access), defaulting `abi` to "C" until
/// resolution.
#[derive(Debug, Clone)]
pub struct ExternFnItem {
    pub span: Span,
    pub attrs: Vec<Attr>,
    pub visibility: Visibility,
    pub name: Ident,
    pub params: Vec<Param>,
    pub return_ty: Option<Type>,
    /// Source-form ABI string. Stage-0 default "C". When `abi_span` is
    /// `Some(_)`, lowering replaces this with the unquoted literal text
    /// sliced from the originating `SourceFile`.
    pub abi: String,
    /// Span of the optional ABI string-literal token (quotes included).
    /// `None` when the declaration omits an explicit ABI tag — implicit "C".
    pub abi_span: Option<Span>,
}

/// `struct Name<G> { fields }`  or tuple-struct / unit-struct variants.
#[derive(Debug, Clone)]
pub struct StructItem {
    pub span: Span,
    pub attrs: Vec<Attr>,
    pub visibility: Visibility,
    pub name: Ident,
    pub generics: Generics,
    pub body: StructBody,
}

/// Struct body shape.
#[derive(Debug, Clone)]
pub enum StructBody {
    /// `struct Unit ;`
    Unit,
    /// `struct Point(T, U) ;`
    Tuple(Vec<FieldDecl>),
    /// `struct Point { x : f32, y : f32 }`
    Named(Vec<FieldDecl>),
}

/// `enum Name<G> { variants }`
#[derive(Debug, Clone)]
pub struct EnumItem {
    pub span: Span,
    pub attrs: Vec<Attr>,
    pub visibility: Visibility,
    pub name: Ident,
    pub generics: Generics,
    pub variants: Vec<EnumVariant>,
}

/// A single enum variant — unit, tuple, or struct form.
///
/// `discriminant` is `Some(_)` when an explicit value was given via `Variant = expr`
/// (C-style enum), otherwise `None`. The expression is held un-evaluated; the elaborator
/// resolves it to a constant integer.
#[derive(Debug, Clone)]
pub struct EnumVariant {
    pub span: Span,
    pub attrs: Vec<Attr>,
    pub name: Ident,
    pub body: StructBody,
    pub discriminant: Option<Expr>,
}

/// `interface Name<G> { associated-items }`
#[derive(Debug, Clone)]
pub struct InterfaceItem {
    pub span: Span,
    pub attrs: Vec<Attr>,
    pub visibility: Visibility,
    pub name: Ident,
    pub generics: Generics,
    pub super_bounds: Vec<Type>,
    pub items: Vec<InterfaceAssocItem>,
}

/// Items that may appear inside an interface body.
#[derive(Debug, Clone)]
pub enum InterfaceAssocItem {
    Fn(FnItem),
    AssociatedType(AssocTypeDecl),
    Const(ConstItem),
}

/// `associated type Name : Bounds`
#[derive(Debug, Clone)]
pub struct AssocTypeDecl {
    pub span: Span,
    pub attrs: Vec<Attr>,
    pub name: Ident,
    pub bounds: Vec<Type>,
    pub default: Option<Type>,
}

/// `impl<G> Trait for Ty where … { items }`  or inherent `impl<G> Ty { items }`.
#[derive(Debug, Clone)]
pub struct ImplItem {
    pub span: Span,
    pub attrs: Vec<Attr>,
    pub generics: Generics,
    /// The trait being implemented, if any; `None` means inherent impl.
    pub trait_: Option<Type>,
    /// The receiver type.
    pub self_ty: Type,
    pub where_clauses: Vec<WhereClause>,
    pub items: Vec<ImplAssocItem>,
}

/// Items that may appear inside an `impl` body.
#[derive(Debug, Clone)]
pub enum ImplAssocItem {
    Fn(FnItem),
    AssociatedType(AssocTypeDef),
    Const(ConstItem),
}

/// `associated type Name = T` (concrete, inside impl).
#[derive(Debug, Clone)]
pub struct AssocTypeDef {
    pub span: Span,
    pub attrs: Vec<Attr>,
    pub name: Ident,
    pub ty: Type,
}

/// `effect Name<G> { ops }`
#[derive(Debug, Clone)]
pub struct EffectItem {
    pub span: Span,
    pub attrs: Vec<Attr>,
    pub visibility: Visibility,
    pub name: Ident,
    pub generics: Generics,
    /// Effect operations — each is a fn-signature without a body.
    pub ops: Vec<FnItem>,
}

/// `handler name(params) for Effect -> Ret { ops + return-clause }`
#[derive(Debug, Clone)]
pub struct HandlerItem {
    pub span: Span,
    pub attrs: Vec<Attr>,
    pub visibility: Visibility,
    pub name: Ident,
    pub generics: Generics,
    pub params: Vec<Param>,
    pub handled_effect: Type,
    pub return_ty: Option<Type>,
    pub ops: Vec<FnItem>,
    /// Optional `return` clause transforming handler's answer.
    pub return_clause: Option<Block>,
}

/// `type Alias<G> = Ty`
#[derive(Debug, Clone)]
pub struct TypeAliasItem {
    pub span: Span,
    pub attrs: Vec<Attr>,
    pub visibility: Visibility,
    pub name: Ident,
    pub generics: Generics,
    pub ty: Type,
}

/// `use Path::{a, b as c, …}`
#[derive(Debug, Clone)]
pub struct UseItem {
    pub span: Span,
    pub attrs: Vec<Attr>,
    pub visibility: Visibility,
    pub tree: UseTree,
}

/// A nested-use tree.
#[derive(Debug, Clone)]
pub enum UseTree {
    /// `use X::Y` — terminal path.
    Path {
        path: ModulePath,
        alias: Option<Ident>,
    },
    /// `use X::*`
    Glob { path: ModulePath },
    /// `use X::{a, b::c}`
    Group {
        prefix: ModulePath,
        trees: Vec<UseTree>,
    },
}

/// `const NAME : T = expr`
#[derive(Debug, Clone)]
pub struct ConstItem {
    pub span: Span,
    pub attrs: Vec<Attr>,
    pub visibility: Visibility,
    pub name: Ident,
    pub ty: Type,
    pub value: Expr,
}

/// Nested `module Name { items }` (rare in Rust-hybrid; possible at file-level).
#[derive(Debug, Clone)]
pub struct ModuleItem {
    pub span: Span,
    pub attrs: Vec<Attr>,
    pub visibility: Visibility,
    pub name: Ident,
    /// `None` → declaration only (`module foo;` referring to separate file).
    /// `Some(items)` → inline module with body.
    pub items: Option<Vec<Item>>,
}

// ════════════════════════════════════════════════════════════════════════════
// § GENERICS + PARAMS
// ════════════════════════════════════════════════════════════════════════════

/// `<T : Bound, U : Bound>` on an item.
#[derive(Debug, Clone, Default)]
pub struct Generics {
    pub span: Option<Span>,
    pub params: Vec<GenericParam>,
}

/// A single generic parameter declaration.
#[derive(Debug, Clone)]
pub struct GenericParam {
    pub span: Span,
    pub name: Ident,
    pub kind: GenericParamKind,
    pub bounds: Vec<Type>,
    pub default: Option<Type>,
}

/// Kind of generic parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenericParamKind {
    /// Type parameter : `T : Bound`.
    Type,
    /// Lifetime / region parameter : `'r`.
    Region,
    /// Const parameter : `const N : usize`.
    Const,
}

/// A `where` clause bound: `T : Bound1 + Bound2`.
#[derive(Debug, Clone)]
pub struct WhereClause {
    pub span: Span,
    pub ty: Type,
    pub bounds: Vec<Type>,
}

/// A fn-parameter declaration.
#[derive(Debug, Clone)]
pub struct Param {
    pub span: Span,
    pub attrs: Vec<Attr>,
    /// The binding pattern (for `(x, y) : (f32, f32)` destructuring).
    pub pat: Pattern,
    pub ty: Type,
    pub default: Option<Expr>,
}

/// A struct / tuple-struct field.
#[derive(Debug, Clone)]
pub struct FieldDecl {
    pub span: Span,
    pub attrs: Vec<Attr>,
    pub visibility: Visibility,
    /// `None` for tuple-struct positional fields.
    pub name: Option<Ident>,
    pub ty: Type,
}

// ════════════════════════════════════════════════════════════════════════════
// § TYPES
// ════════════════════════════════════════════════════════════════════════════

/// A type expression.
#[derive(Debug, Clone)]
pub struct Type {
    pub span: Span,
    pub kind: TypeKind,
}

/// Shapes of type expressions. Capability wrappers (`iso<T>`, `val<T>`, etc) are represented
/// as `Capability { ... }`; refinement tags (`T'pos`, `{v : T | P}`) as `Refined`.
#[derive(Debug, Clone)]
pub enum TypeKind {
    /// `T` or `module::T` — named path with optional type-args.
    Path {
        path: ModulePath,
        type_args: Vec<Type>,
    },
    /// `[T ; N]`
    Array { elem: Box<Type>, len: Box<Expr> },
    /// `[T]`
    Slice { elem: Box<Type> },
    /// `(T, U, V)` — arity 0 → unit.
    Tuple { elems: Vec<Type> },
    /// `&T` / `&mut T`
    Reference { mutable: bool, inner: Box<Type> },
    /// `*const T` / `*mut T` — C-style raw pointer (FFI surface). Distinct
    /// from `Reference` because raw pointers carry no aliasing/lifetime/cap
    /// guarantee — they exist solely so `extern fn` signatures can declare
    /// the host symbol's exact ABI shape. Stage-0 lowering treats them as
    /// equivalent to `Reference` for type-check purposes ; cap-checks etc
    /// skip them.
    RawPointer { mutable: bool, inner: Box<Type> },
    /// `iso<T>` / `trn<T>` / `ref<T>` / `val<T>` / `box<T>` / `tag<T>`.
    Capability { cap: CapKind, inner: Box<Type> },
    /// `fn(T1, …, Tn) -> U / ε`
    Function {
        params: Vec<Type>,
        return_ty: Box<Type>,
        effect_row: Option<EffectRow>,
    },
    /// `T'tag` (surface-sugar) or `{ v : T | P(v) }` (full form).
    Refined {
        base: Box<Type>,
        kind: RefinementKind,
    },
    /// `_` — inferred type.
    Infer,
}

/// Capability-kind wrapping a type (Pony-6 set).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapKind {
    Iso,
    Trn,
    Ref,
    Val,
    Box,
    Tag,
}

/// Refinement-shape attached to a base type.
#[derive(Debug, Clone)]
pub enum RefinementKind {
    /// `T'tagname` — identifier tag (resolved via refinement dictionary at elaboration).
    Tag { name: Ident },
    /// `{ v : T | P(v) }` — explicit predicate.
    Predicate { binder: Ident, predicate: Box<Expr> },
    /// `SDF'L<k>` — Lipschitz-bound form (one of the built-in refinement shapes).
    Lipschitz { bound: Box<Expr> },
}

/// An effect-row `/ { e1, e2<arg>, … }` attached to a function type.
#[derive(Debug, Clone)]
pub struct EffectRow {
    pub span: Span,
    pub effects: Vec<EffectAnnotation>,
    /// If present, the row is extended by a polymorphic tail `μ`.
    pub tail: Option<Ident>,
}

/// One entry in an effect row — name + optional type/expr arguments.
#[derive(Debug, Clone)]
pub struct EffectAnnotation {
    pub span: Span,
    pub name: ModulePath,
    pub args: Vec<EffectArg>,
}

/// Effect-row argument — a type, an expression (for literal parameters), or a nested effect.
#[derive(Debug, Clone)]
pub enum EffectArg {
    Type(Type),
    Expr(Expr),
}

// ════════════════════════════════════════════════════════════════════════════
// § PATTERNS
// ════════════════════════════════════════════════════════════════════════════

/// A pattern used in `let`, `match` arms, fn-params, destructuring.
#[derive(Debug, Clone)]
pub struct Pattern {
    pub span: Span,
    pub kind: PatternKind,
}

/// Shapes of patterns.
#[derive(Debug, Clone)]
pub enum PatternKind {
    /// `_` — wildcard.
    Wildcard,
    /// `5`, `"hi"`, `true` — literal.
    Literal(Literal),
    /// `x` or `mut x` — binding pattern.
    Binding { mutable: bool, name: Ident },
    /// `(a, b, c)`
    Tuple(Vec<Pattern>),
    /// `Point { x, y }` or `Point { x : a, y : b }`
    Struct {
        path: ModulePath,
        fields: Vec<PatternField>,
        rest: bool,
    },
    /// `Some(a)` — enum-variant pattern with positional arguments.
    Variant {
        path: ModulePath,
        args: Vec<Pattern>,
    },
    /// `a | b | c`
    Or(Vec<Pattern>),
    /// `a..b`  / `a..=b`
    Range {
        start: Option<Box<Pattern>>,
        end: Option<Box<Pattern>>,
        inclusive: bool,
    },
    /// `ref x` — explicit reference binding.
    Ref { mutable: bool, inner: Box<Pattern> },
}

/// `{ name : pat }` field in a struct pattern.
#[derive(Debug, Clone)]
pub struct PatternField {
    pub span: Span,
    pub name: Ident,
    /// `None` means shorthand (`{ x }` ≡ `{ x : x }`).
    pub pat: Option<Pattern>,
}

// ════════════════════════════════════════════════════════════════════════════
// § STATEMENTS + BLOCKS
// ════════════════════════════════════════════════════════════════════════════

/// A block `{ stmt* expr? }` — the optional trailing expression is the block's value.
#[derive(Debug, Clone)]
pub struct Block {
    pub span: Span,
    pub stmts: Vec<Stmt>,
    /// Trailing expression that produces the block's value.
    pub trailing: Option<Box<Expr>>,
}

/// A single statement inside a block.
#[derive(Debug, Clone)]
pub struct Stmt {
    pub span: Span,
    pub kind: StmtKind,
}

/// Shape of a statement.
#[derive(Debug, Clone)]
pub enum StmtKind {
    /// `let pat : T = expr`
    Let {
        attrs: Vec<Attr>,
        pat: Pattern,
        ty: Option<Type>,
        value: Option<Expr>,
    },
    /// `expr ;` — expression evaluated for side effects.
    Expr(Expr),
    /// Item declared inside a block (rare).
    Item(Item),
}

// ════════════════════════════════════════════════════════════════════════════
// § EXPRESSIONS
// ════════════════════════════════════════════════════════════════════════════

/// An expression node.
#[derive(Debug, Clone)]
pub struct Expr {
    pub span: Span,
    /// Attributes applied to this expression (e.g., `@metamorphic(law="commute")`).
    pub attrs: Vec<Attr>,
    pub kind: ExprKind,
}

/// Shapes of expressions. Precedence is encoded by the parser; this enum is flat.
#[derive(Debug, Clone)]
pub enum ExprKind {
    /// Literal value.
    Literal(Literal),
    /// `x` / `foo::bar` — path reference (any length ≥ 1).
    Path(ModulePath),
    /// `f(arg1, arg2)`  or  `f::<T, U>(arg1)` — application. `type_args` carries
    /// the turbofish explicit type-arguments (empty `Vec` when no turbofish was
    /// written) ; populated by the parser at the call-site and propagated through
    /// HIR-lowering to the monomorphization pass per T11-D39.
    Call {
        callee: Box<Expr>,
        args: Vec<CallArg>,
        type_args: Vec<Type>,
    },
    /// `obj.field` / `obj.method(args)`
    Field { obj: Box<Expr>, name: Ident },
    /// `arr[idx]`
    Index { obj: Box<Expr>, index: Box<Expr> },
    /// Binary operator application.
    Binary {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    /// Unary operator application.
    Unary { op: UnOp, operand: Box<Expr> },
    /// Block expression.
    Block(Block),
    /// `if cond { a } else { b }`  or `else-if` chain.
    If {
        cond: Box<Expr>,
        then_branch: Block,
        else_branch: Option<Box<Expr>>,
    },
    /// `match scrutinee { arm* }`
    Match {
        scrutinee: Box<Expr>,
        arms: Vec<MatchArm>,
    },
    /// `for pat in iter { body }`
    For {
        pat: Pattern,
        iter: Box<Expr>,
        body: Block,
    },
    /// `while cond { body }`
    While { cond: Box<Expr>, body: Block },
    /// `loop { body }` — infinite loop.
    Loop { body: Block },
    /// `return expr ?`
    Return { value: Option<Box<Expr>> },
    /// `break [label] [expr] ?`
    Break {
        label: Option<Ident>,
        value: Option<Box<Expr>>,
    },
    /// `continue [label] ?`
    Continue { label: Option<Ident> },
    /// `|x : T| -> U { body }`
    Lambda {
        params: Vec<Param>,
        return_ty: Option<Type>,
        body: Box<Expr>,
    },
    /// `a = b`  or  `a += b` / `a -= b` / …
    Assign {
        op: Option<BinOp>, // None → plain `=`; Some(Add) → `+=`; etc.
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    /// `expr as Ty`
    Cast { expr: Box<Expr>, ty: Type },
    /// `lo..hi` / `lo..=hi` / `lo..` / `..hi`
    Range {
        lo: Option<Box<Expr>>,
        hi: Option<Box<Expr>>,
        inclusive: bool,
    },
    /// `expr |> f` — pipeline forward.
    Pipeline { lhs: Box<Expr>, rhs: Box<Expr> },
    /// `expr ? : default` — early-return default operator.
    TryDefault { expr: Box<Expr>, default: Box<Expr> },
    /// `expr ?` — `Try` / early-return propagation.
    Try { expr: Box<Expr> },
    /// `perform Effect::op(args)` — handler invocation.
    Perform {
        path: ModulePath,
        args: Vec<CallArg>,
    },
    /// `with handler-expr { body }` — handler installation.
    With { handler: Box<Expr>, body: Block },
    /// `region 'r { body }`
    Region { label: Option<Ident>, body: Block },
    /// `(a, b, c)` — tuple constructor (arity 0 → unit).
    Tuple(Vec<Expr>),
    /// `[a, b, c]` / `[elem ; N]`
    Array(ArrayExpr),
    /// `Point { x : 1, y : 2 }` — struct constructor.
    Struct {
        path: ModulePath,
        fields: Vec<StructFieldInit>,
        /// `..base_value`
        spread: Option<Box<Expr>>,
    },
    /// `#run expr` — compile-time evaluation marker.
    Run { expr: Box<Expr> },
    /// CSLv3-native compound expression `A op B` per §§ 13 compound-formation.
    Compound {
        op: CompoundOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    /// `§§ path` — reference to another section (CSLv3-native only).
    SectionRef { path: ModulePath },
    /// Parenthesized grouping — preserved so formatter can round-trip.
    Paren(Box<Expr>),
    /// Error recovery placeholder — parser produced a `Diagnostic`.
    Error,
}

/// An array-expression form.
#[derive(Debug, Clone)]
pub enum ArrayExpr {
    /// `[a, b, c]`
    List(Vec<Expr>),
    /// `[elem ; len]`
    Repeat { elem: Box<Expr>, len: Box<Expr> },
}

/// A match arm `pat [if guard] => body`.
#[derive(Debug, Clone)]
pub struct MatchArm {
    pub span: Span,
    pub attrs: Vec<Attr>,
    pub pat: Pattern,
    pub guard: Option<Expr>,
    pub body: Expr,
}

/// `name : value` initializer in a struct-constructor expression.
#[derive(Debug, Clone)]
pub struct StructFieldInit {
    pub span: Span,
    pub name: Ident,
    /// `None` → shorthand (`{ x }` ≡ `{ x : x }`).
    pub value: Option<Expr>,
}

/// A function-call argument — positional or named (`name = value`).
#[derive(Debug, Clone)]
pub enum CallArg {
    Positional(Expr),
    Named { name: Ident, value: Expr },
}

/// Literal values.
#[derive(Debug, Clone)]
pub struct Literal {
    pub span: Span,
    pub kind: LiteralKind,
}

/// Literal shapes. Actual value re-parsed from source via `span`; the kind hints type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiteralKind {
    /// `42`, `0x10`, `0b101` — integer with optional type suffix.
    Int,
    /// `3.14`, `1e10` — float with optional type suffix.
    Float,
    /// `"…"` / `r"…"` — string.
    Str,
    /// `'c'`
    Char,
    /// `true` / `false`.
    Bool(bool),
    /// `()` — unit literal.
    Unit,
}

/// Binary operators. Precedence handled by the parser, not by this enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
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
    Implies, // `⇒` / `=>` — used in propositions / match-arms (context-dependent)
    Entails, // `⊢`
}

/// Unary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Not,    // `!` / `¬`
    Neg,    // `-`
    BitNot, // `~`
    Ref,    // `&`
    RefMut, // `&mut`
    Deref,  // `*`
}

/// CSLv3-native compound-formation operators per `CSLv3/specs/13_GRAMMAR_SELF.csl`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompoundOp {
    /// `.` — tatpuruṣa : B-of-A (field-access shares syntax; parser disambiguates by context).
    Tp,
    /// `+` — dvandva : co-equal conjunction.
    Dv,
    /// `-` — karmadhāraya : B-that-is-A.
    Kd,
    /// `⊗` — bahuvrīhi : thing-having-A+B.
    Bv,
    /// `@` — avyayībhāva : at/per/in-scope-of X.
    Av,
}

// ════════════════════════════════════════════════════════════════════════════
// § TESTS (structural / coverage)
// ════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::{
        Attr, AttrArg, AttrKind, BinOp, Block, CapKind, CompoundOp, Expr, ExprKind, FnItem,
        Generics, Ident, Item, Literal, LiteralKind, Module, ModulePath, Param, Pattern,
        PatternKind, Stmt, StmtKind, Type, TypeKind, UnOp, Visibility, VisibilityKind,
    };
    use crate::{SourceId, Span};

    fn sp(start: u32, end: u32) -> Span {
        Span::new(SourceId(1), start, end)
    }

    fn ident(start: u32, end: u32) -> Ident {
        Ident {
            span: sp(start, end),
        }
    }

    fn path(segments: Vec<Ident>) -> ModulePath {
        let span = match (segments.first(), segments.last()) {
            (Some(a), Some(b)) => Span::new(a.span.source, a.span.start, b.span.end),
            _ => Span::DUMMY,
        };
        ModulePath { span, segments }
    }

    #[test]
    fn module_holds_items_and_attrs() {
        let m = Module {
            span: sp(0, 100),
            inner_attrs: vec![Attr {
                span: sp(0, 20),
                kind: AttrKind::Inner,
                path: path(vec![ident(4, 11)]),
                args: vec![],
            }],
            path: Some(path(vec![ident(21, 24)])),
            items: vec![],
        };
        assert_eq!(m.inner_attrs.len(), 1);
        assert!(m.path.is_some());
    }

    #[test]
    fn fn_item_with_generics_and_effect_row() {
        let f = FnItem {
            span: sp(0, 50),
            attrs: vec![],
            visibility: Visibility {
                span: sp(0, 3),
                kind: VisibilityKind::Public,
            },
            name: ident(7, 10),
            generics: Generics::default(),
            params: vec![Param {
                span: sp(11, 20),
                attrs: vec![],
                pat: Pattern {
                    span: sp(11, 12),
                    kind: PatternKind::Binding {
                        mutable: false,
                        name: ident(11, 12),
                    },
                },
                ty: Type {
                    span: sp(15, 20),
                    kind: TypeKind::Infer,
                },
                default: None,
            }],
            return_ty: None,
            effect_row: None,
            where_clauses: vec![],
            body: Some(Block {
                span: sp(22, 50),
                stmts: vec![],
                trailing: None,
            }),
        };
        assert_eq!(f.params.len(), 1);
        assert!(f.body.is_some());
    }

    #[test]
    fn expr_kinds_coverable() {
        // Spot-check that every expr kind can be constructed.
        let lit = Expr {
            span: sp(0, 2),
            attrs: vec![],
            kind: ExprKind::Literal(Literal {
                span: sp(0, 2),
                kind: LiteralKind::Int,
            }),
        };
        let bin = Expr {
            span: sp(0, 5),
            attrs: vec![],
            kind: ExprKind::Binary {
                op: BinOp::Add,
                lhs: Box::new(lit.clone()),
                rhs: Box::new(lit.clone()),
            },
        };
        let un = Expr {
            span: sp(0, 3),
            attrs: vec![],
            kind: ExprKind::Unary {
                op: UnOp::Neg,
                operand: Box::new(lit.clone()),
            },
        };
        let compound = Expr {
            span: sp(0, 5),
            attrs: vec![],
            kind: ExprKind::Compound {
                op: CompoundOp::Tp,
                lhs: Box::new(lit.clone()),
                rhs: Box::new(lit.clone()),
            },
        };
        let _ = (lit, bin, un, compound);
    }

    #[test]
    fn stmt_let_with_pattern_and_value() {
        let s = Stmt {
            span: sp(0, 20),
            kind: StmtKind::Let {
                attrs: vec![],
                pat: Pattern {
                    span: sp(4, 5),
                    kind: PatternKind::Binding {
                        mutable: false,
                        name: ident(4, 5),
                    },
                },
                ty: Some(Type {
                    span: sp(8, 11),
                    kind: TypeKind::Infer,
                }),
                value: None,
            },
        };
        match s.kind {
            StmtKind::Let { pat, .. } => {
                assert!(matches!(pat.kind, PatternKind::Binding { .. }));
            }
            _ => panic!("expected Let"),
        }
    }

    #[test]
    fn cap_kinds_enumerated() {
        let caps = [
            CapKind::Iso,
            CapKind::Trn,
            CapKind::Ref,
            CapKind::Val,
            CapKind::Box,
            CapKind::Tag,
        ];
        assert_eq!(caps.len(), 6);
    }

    #[test]
    fn compound_ops_enumerated() {
        let ops = [
            CompoundOp::Tp,
            CompoundOp::Dv,
            CompoundOp::Kd,
            CompoundOp::Bv,
            CompoundOp::Av,
        ];
        assert_eq!(ops.len(), 5);
    }

    #[test]
    fn item_span_dispatch() {
        let fi = Item::Fn(FnItem {
            span: sp(10, 20),
            attrs: vec![],
            visibility: Visibility {
                span: sp(10, 13),
                kind: VisibilityKind::Private,
            },
            name: ident(14, 17),
            generics: Generics::default(),
            params: vec![],
            return_ty: None,
            effect_row: None,
            where_clauses: vec![],
            body: None,
        });
        assert_eq!(fi.span(), sp(10, 20));
    }

    #[test]
    fn attr_arg_both_shapes() {
        let _pos = AttrArg::Positional(Expr {
            span: sp(0, 2),
            attrs: vec![],
            kind: ExprKind::Literal(Literal {
                span: sp(0, 2),
                kind: LiteralKind::Int,
            }),
        });
        let _named = AttrArg::Named {
            name: ident(0, 1),
            value: Expr {
                span: sp(2, 5),
                attrs: vec![],
                kind: ExprKind::Literal(Literal {
                    span: sp(2, 5),
                    kind: LiteralKind::Float,
                }),
            },
        };
    }
}
