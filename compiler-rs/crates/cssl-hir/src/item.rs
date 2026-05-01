//! HIR items — top-level declarations with `DefId` identity.
//!
//! Mirrors `cssl_ast::cst::Item` variants. Each def-level node carries a `DefId` so that
//! name-resolution can wire path references at the module + cross-module boundary.

use cssl_ast::Span;

use crate::arena::{DefId, HirArena, HirId};
use crate::attr::HirAttr;
use crate::expr::{HirBlock, HirExpr};
use crate::pat::HirPattern;
use crate::symbol::Symbol;
use crate::ty::{HirEffectRow, HirType};

/// A whole HIR module — the top-level container produced by lowering.
#[derive(Debug)]
pub struct HirModule {
    pub span: Span,
    /// Arena allocator (HirId/DefId counters).
    pub arena: HirArena,
    /// Inner attributes `#![…]` at file-head.
    pub inner_attrs: Vec<HirAttr>,
    /// Optional module-path declaration (e.g., `module com.apocky.loa`).
    pub module_path: Option<Vec<Symbol>>,
    /// Top-level items in source order.
    pub items: Vec<HirItem>,
}

/// A top-level or nested item.
#[derive(Debug, Clone)]
pub enum HirItem {
    Fn(HirFn),
    /// `extern fn name(params) -> ret` — body-less FFI declaration. The
    /// resolver wires `func.call` ops to this DefId so codegen marks the
    /// callee as an external linkage import.
    ExternFn(HirExternFn),
    Struct(HirStruct),
    Enum(HirEnum),
    Interface(HirInterface),
    Impl(HirImpl),
    Effect(HirEffect),
    Handler(HirHandler),
    TypeAlias(HirTypeAlias),
    Use(HirUse),
    Const(HirConst),
    Module(HirNestedModule),
}

impl HirItem {
    /// Span of the item.
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

    /// `DefId` of the item (only items carry definition identity).
    #[must_use]
    pub fn def_id(&self) -> Option<DefId> {
        match self {
            Self::Fn(i) => Some(i.def),
            Self::ExternFn(i) => Some(i.def),
            Self::Struct(i) => Some(i.def),
            Self::Enum(i) => Some(i.def),
            Self::Interface(i) => Some(i.def),
            Self::Effect(i) => Some(i.def),
            Self::Handler(i) => Some(i.def),
            Self::TypeAlias(i) => Some(i.def),
            Self::Const(i) => Some(i.def),
            Self::Module(i) => Some(i.def),
            Self::Impl(_) | Self::Use(_) => None,
        }
    }

    /// The item's declared name if it carries one. `impl` and `use` items return `None`.
    #[must_use]
    pub fn name(&self) -> Option<Symbol> {
        match self {
            Self::Fn(i) => Some(i.name),
            Self::ExternFn(i) => Some(i.name),
            Self::Struct(i) => Some(i.name),
            Self::Enum(i) => Some(i.name),
            Self::Interface(i) => Some(i.name),
            Self::Effect(i) => Some(i.name),
            Self::Handler(i) => Some(i.name),
            Self::TypeAlias(i) => Some(i.name),
            Self::Const(i) => Some(i.name),
            Self::Module(i) => Some(i.name),
            Self::Impl(_) | Self::Use(_) => None,
        }
    }
}

/// Visibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HirVisibility {
    Private,
    Public,
}

/// Generics container.
#[derive(Debug, Clone, Default)]
pub struct HirGenerics {
    pub params: Vec<HirGenericParam>,
}

/// Single generic parameter.
#[derive(Debug, Clone)]
pub struct HirGenericParam {
    pub span: Span,
    pub id: HirId,
    pub name: Symbol,
    pub kind: HirGenericParamKind,
    pub bounds: Vec<HirType>,
    pub default: Option<HirType>,
}

/// Kind of generic parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HirGenericParamKind {
    Type,
    Region,
    Const,
}

/// `where T : Bound` clause.
#[derive(Debug, Clone)]
pub struct HirWhereClause {
    pub span: Span,
    pub ty: HirType,
    pub bounds: Vec<HirType>,
}

/// Function parameter.
#[derive(Debug, Clone)]
pub struct HirFnParam {
    pub span: Span,
    pub id: HirId,
    pub attrs: Vec<HirAttr>,
    pub pat: HirPattern,
    pub ty: HirType,
    pub default: Option<HirExpr>,
}

/// `fn` item.
#[derive(Debug, Clone)]
pub struct HirFn {
    pub span: Span,
    pub def: DefId,
    pub visibility: HirVisibility,
    pub attrs: Vec<HirAttr>,
    pub name: Symbol,
    pub generics: HirGenerics,
    pub params: Vec<HirFnParam>,
    pub return_ty: Option<HirType>,
    pub effect_row: Option<HirEffectRow>,
    pub where_clauses: Vec<HirWhereClause>,
    pub body: Option<HirBlock>,
}

/// `extern fn` item — body-less FFI declaration.
///
/// Mirrors `cst::ExternFnItem`. The downstream MIR pass synthesizes a
/// signature-only `MirFunc` ; backend codegen marks the callee as an
/// external linkage import so the host loader resolves it at runtime.
#[derive(Debug, Clone)]
pub struct HirExternFn {
    pub span: Span,
    pub def: DefId,
    pub visibility: HirVisibility,
    pub attrs: Vec<HirAttr>,
    pub name: Symbol,
    pub params: Vec<HirFnParam>,
    pub return_ty: Option<HirType>,
    /// Source-form ABI string. Stage-0 always "C".
    pub abi: String,
}

/// `struct` item.
#[derive(Debug, Clone)]
pub struct HirStruct {
    pub span: Span,
    pub def: DefId,
    pub visibility: HirVisibility,
    pub attrs: Vec<HirAttr>,
    pub name: Symbol,
    pub generics: HirGenerics,
    pub body: HirStructBody,
}

/// Struct body shape.
#[derive(Debug, Clone)]
pub enum HirStructBody {
    Unit,
    Tuple(Vec<HirFieldDecl>),
    Named(Vec<HirFieldDecl>),
}

/// Field declaration.
#[derive(Debug, Clone)]
pub struct HirFieldDecl {
    pub span: Span,
    pub id: HirId,
    pub attrs: Vec<HirAttr>,
    pub visibility: HirVisibility,
    /// `None` for tuple-struct positional fields.
    pub name: Option<Symbol>,
    pub ty: HirType,
}

/// `enum` item.
#[derive(Debug, Clone)]
pub struct HirEnum {
    pub span: Span,
    pub def: DefId,
    pub visibility: HirVisibility,
    pub attrs: Vec<HirAttr>,
    pub name: Symbol,
    pub generics: HirGenerics,
    pub variants: Vec<HirEnumVariant>,
}

/// Single enum variant.
#[derive(Debug, Clone)]
pub struct HirEnumVariant {
    pub span: Span,
    pub def: DefId,
    pub attrs: Vec<HirAttr>,
    pub name: Symbol,
    pub body: HirStructBody,
}

/// `interface` item.
#[derive(Debug, Clone)]
pub struct HirInterface {
    pub span: Span,
    pub def: DefId,
    pub visibility: HirVisibility,
    pub attrs: Vec<HirAttr>,
    pub name: Symbol,
    pub generics: HirGenerics,
    pub super_bounds: Vec<HirType>,
    pub fns: Vec<HirFn>,
    pub assoc_types: Vec<HirAssocTypeDecl>,
    pub consts: Vec<HirConst>,
}

/// `associated type Name : Bounds [= Default]` inside an interface.
#[derive(Debug, Clone)]
pub struct HirAssocTypeDecl {
    pub span: Span,
    pub def: DefId,
    pub attrs: Vec<HirAttr>,
    pub name: Symbol,
    pub bounds: Vec<HirType>,
    pub default: Option<HirType>,
}

/// `impl` item.
#[derive(Debug, Clone)]
pub struct HirImpl {
    pub span: Span,
    pub attrs: Vec<HirAttr>,
    pub generics: HirGenerics,
    /// The trait implemented — `None` for inherent impls.
    pub trait_: Option<HirType>,
    pub self_ty: HirType,
    pub where_clauses: Vec<HirWhereClause>,
    pub fns: Vec<HirFn>,
    pub assoc_types: Vec<HirAssocTypeDef>,
    pub consts: Vec<HirConst>,
}

/// `associated type Name = T` inside an impl.
#[derive(Debug, Clone)]
pub struct HirAssocTypeDef {
    pub span: Span,
    pub def: DefId,
    pub attrs: Vec<HirAttr>,
    pub name: Symbol,
    pub ty: HirType,
}

/// `effect Name<G> { ops }`.
#[derive(Debug, Clone)]
pub struct HirEffect {
    pub span: Span,
    pub def: DefId,
    pub visibility: HirVisibility,
    pub attrs: Vec<HirAttr>,
    pub name: Symbol,
    pub generics: HirGenerics,
    pub ops: Vec<HirFn>,
}

/// `handler name(params) for Effect<args> -> Ret { ops + return-clause }`.
#[derive(Debug, Clone)]
pub struct HirHandler {
    pub span: Span,
    pub def: DefId,
    pub visibility: HirVisibility,
    pub attrs: Vec<HirAttr>,
    pub name: Symbol,
    pub generics: HirGenerics,
    pub params: Vec<HirFnParam>,
    pub handled_effect: HirType,
    pub return_ty: Option<HirType>,
    pub ops: Vec<HirFn>,
    pub return_clause: Option<HirBlock>,
}

/// `type Alias<G> = Ty`.
#[derive(Debug, Clone)]
pub struct HirTypeAlias {
    pub span: Span,
    pub def: DefId,
    pub visibility: HirVisibility,
    pub attrs: Vec<HirAttr>,
    pub name: Symbol,
    pub generics: HirGenerics,
    pub ty: HirType,
}

/// `use path::{a, b as c}` — the use-tree is flattened into a `Vec<HirUseBinding>`
/// so the resolver can iterate over single bindings.
#[derive(Debug, Clone)]
pub struct HirUse {
    pub span: Span,
    pub visibility: HirVisibility,
    pub attrs: Vec<HirAttr>,
    pub bindings: Vec<HirUseBinding>,
}

/// Single `use`-binding with optional alias.
#[derive(Debug, Clone)]
pub struct HirUseBinding {
    pub span: Span,
    pub path: Vec<Symbol>,
    pub alias: Option<Symbol>,
    /// `true` iff the binding is a glob form `use path::*`.
    pub is_glob: bool,
    /// Resolution target — filled by name-resolution.
    pub def: Option<DefId>,
}

/// `const NAME : T = expr`.
#[derive(Debug, Clone)]
pub struct HirConst {
    pub span: Span,
    pub def: DefId,
    pub visibility: HirVisibility,
    pub attrs: Vec<HirAttr>,
    pub name: Symbol,
    pub ty: HirType,
    pub value: HirExpr,
}

/// Nested `module Name { items }` or `module Name;`.
#[derive(Debug, Clone)]
pub struct HirNestedModule {
    pub span: Span,
    pub def: DefId,
    pub visibility: HirVisibility,
    pub attrs: Vec<HirAttr>,
    pub name: Symbol,
    /// `None` → declaration-only (source file lives separately).
    pub items: Option<Vec<HirItem>>,
}

#[cfg(test)]
mod tests {
    use super::{HirItem, HirVisibility};
    use crate::arena::{DefId, HirArena, HirId};
    use cssl_ast::{SourceId, Span};

    #[test]
    fn visibility_variants() {
        let v = [HirVisibility::Private, HirVisibility::Public];
        assert_eq!(v.len(), 2);
    }

    #[test]
    fn arena_allocates_def_ids() {
        let mut a = HirArena::new();
        let d0 = a.fresh_def_id();
        let d1 = a.fresh_def_id();
        assert_ne!(d0, d1);
    }

    #[test]
    fn item_impl_and_use_have_no_def_id() {
        use super::{HirGenerics, HirImpl, HirUse};
        use crate::ty::{HirType, HirTypeKind};
        let sp = Span::new(SourceId::first(), 0, 1);
        let placeholder_ty = HirType {
            span: sp,
            id: HirId::DUMMY,
            kind: HirTypeKind::Infer,
        };
        let imp = HirItem::Impl(HirImpl {
            span: sp,
            attrs: Vec::new(),
            generics: HirGenerics::default(),
            trait_: None,
            self_ty: placeholder_ty,
            where_clauses: Vec::new(),
            fns: Vec::new(),
            assoc_types: Vec::new(),
            consts: Vec::new(),
        });
        assert_eq!(imp.def_id(), None);
        assert_eq!(imp.name(), None);
        let use_item = HirItem::Use(HirUse {
            span: sp,
            visibility: HirVisibility::Private,
            attrs: Vec::new(),
            bindings: Vec::new(),
        });
        assert_eq!(use_item.def_id(), None);
        // Ensure the placeholder DefId stays unique.
        let _ = DefId::UNRESOLVED;
    }
}
