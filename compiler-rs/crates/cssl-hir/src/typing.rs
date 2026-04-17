//! Type algebra used by the T3.4 inference engine.
//!
//! § SPEC : `specs/02_IR.csl` § HIR types + `specs/03_TYPES.csl` + `specs/04_EFFECTS.csl`
//!   effect-row operational semantics.
//!
//! § DESIGN
//!   `Ty` is the inference-level type language — a discriminated union covering
//!   primitives, nominal types via `DefId`, tuples, references, function types
//!   (parameters + return + effect-row), fresh inference variables, and skolem
//!   type parameters (used during function-body check for generics).
//!
//!   `Row` represents a Koka-style effect row : a multiset of effect instances
//!   plus an optional tail variable for row-polymorphism. Stage-0 treats all
//!   rows as closed by default — row variables appear only when an explicit
//!   effect-row tail (`μ`) is declared in a function signature.
//!
//! § STAGE-0 LIMITATIONS (tracked in `DECISIONS.md` T3-D9)
//!   - No subtyping — references unify nominally.
//!   - No higher-rank polymorphism.
//!   - No kind polymorphism (all type vars have kind `*`).
//!   - Capability + IFC + refinement annotations are collected but not propagated
//!     (T3.4-phase-2 work).

use std::collections::{BTreeMap, HashMap};

use crate::arena::DefId;
use crate::symbol::Symbol;

/// Inference-level type. This is distinct from `HirType` (which is the CST-mirror
/// structure) : a `Ty` is produced by inference and stored in the `TypeMap` side
/// table under each HIR node's `HirId`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ty {
    /// Integer primitive (`i32` by default ; elaborator refines via inference).
    Int,
    /// Float primitive (`f32` by default).
    Float,
    /// `bool`.
    Bool,
    /// `str`.
    Str,
    /// `()` — unit type.
    Unit,
    /// `!` — never/divergent type ; unifies with anything.
    Never,
    /// Nominal type reference : `DefId` + type arguments.
    Named { def: DefId, args: Vec<Ty> },
    /// Tuple type with known arity.
    Tuple(Vec<Ty>),
    /// `&T` or `&mut T`.
    Ref { mutable: bool, inner: Box<Ty> },
    /// Function type : parameters → return / effect-row.
    Fn {
        params: Vec<Ty>,
        return_ty: Box<Ty>,
        effect_row: Row,
    },
    /// Fresh inference variable.
    Var(TyVar),
    /// Skolem type parameter — an opaque, bound-by-`fn<T>` type variable treated
    /// as distinct-from-everything during body checking.
    Param(Symbol),
    /// Array `[T ; N]` — length kept symbolic for stage-0 (no const-eval yet).
    Array { elem: Box<Ty>, len: ArrayLen },
    /// Slice `[T]`.
    Slice { elem: Box<Ty> },
    /// Error-recovery placeholder — unifies with anything.
    Error,
}

/// Array-length slot. Stage-0 defers const-evaluation ; we track whether the
/// length was a literal or a free variable. Full const-evaluation is T8 scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArrayLen {
    /// Length literal `[T ; 10]` — value known at parse-time.
    Literal(u64),
    /// Length expression — stage-0 keeps it opaque.
    Opaque,
    /// Fresh inference variable.
    Var(u32),
}

/// Effect-row : Koka-style multiset of effect instances.
///
/// The row `⟨⟩ = { effects: [], tail: None }` means pure (no effects).
/// The row `⟨e1, e2 | μ⟩` has effects plus a tail variable `μ`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Row {
    /// Effects present in the row. Canonical order is sorted-by-name at equality
    /// comparison time (via `canonicalize`).
    pub effects: Vec<EffectInstance>,
    /// Tail variable — `None` for closed rows, `Some(_)` for row-polymorphic.
    pub tail: Option<RowVar>,
}

/// A single effect in a row : `Effect<args>` form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectInstance {
    /// Interned effect name (e.g., `GPU`, `NoAlloc`, `Telemetry`).
    pub name: Symbol,
    /// Effect arguments (types + exprs boxed into `Ty` for stage-0 uniformity).
    pub args: Vec<Ty>,
}

/// Inference-variable identifier for types. Allocated monotonically by `TyCtx`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct TyVar(pub u32);

/// Inference-variable identifier for effect rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct RowVar(pub u32);

impl Row {
    /// A pure (empty) row.
    #[must_use]
    pub fn pure() -> Self {
        Self::default()
    }

    /// Build a closed row from a list of effect instances.
    #[must_use]
    pub fn closed(effects: Vec<EffectInstance>) -> Self {
        Self {
            effects,
            tail: None,
        }
    }

    /// `true` iff the row has no effects and no tail variable.
    #[must_use]
    pub fn is_pure(&self) -> bool {
        self.effects.is_empty() && self.tail.is_none()
    }

    /// Return a canonicalized copy : effects sorted by (name, arg-count).
    /// Used for structural equality when comparing rows across binding positions.
    #[must_use]
    pub fn canonicalize(&self) -> Self {
        let mut effects = self.effects.clone();
        effects.sort_by(|a, b| {
            a.name
                .cmp(&b.name)
                .then_with(|| a.args.len().cmp(&b.args.len()))
        });
        Self {
            effects,
            tail: self.tail,
        }
    }
}

/// Substitution : the accumulated mapping from inference variables to concrete types
/// (or further variables). Applied to types after each unification step.
#[derive(Debug, Clone, Default)]
pub struct Subst {
    pub ty_vars: HashMap<TyVar, Ty>,
    pub row_vars: HashMap<RowVar, Row>,
}

impl Subst {
    /// Empty substitution.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Apply the substitution to a type, replacing variables with their mapped targets.
    /// Recursive : if the result is a variable that's also mapped, follows the chain.
    #[must_use]
    pub fn apply(&self, t: &Ty) -> Ty {
        match t {
            Ty::Var(v) => {
                if let Some(next) = self.ty_vars.get(v) {
                    // Avoid infinite recursion — caller should have run occurs-check.
                    self.apply(next)
                } else {
                    Ty::Var(*v)
                }
            }
            Ty::Named { def, args } => Ty::Named {
                def: *def,
                args: args.iter().map(|a| self.apply(a)).collect(),
            },
            Ty::Tuple(elems) => Ty::Tuple(elems.iter().map(|e| self.apply(e)).collect()),
            Ty::Ref { mutable, inner } => Ty::Ref {
                mutable: *mutable,
                inner: Box::new(self.apply(inner)),
            },
            Ty::Fn {
                params,
                return_ty,
                effect_row,
            } => Ty::Fn {
                params: params.iter().map(|p| self.apply(p)).collect(),
                return_ty: Box::new(self.apply(return_ty)),
                effect_row: self.apply_row(effect_row),
            },
            Ty::Array { elem, len } => Ty::Array {
                elem: Box::new(self.apply(elem)),
                len: len.clone(),
            },
            Ty::Slice { elem } => Ty::Slice {
                elem: Box::new(self.apply(elem)),
            },
            // Leaves.
            Ty::Int
            | Ty::Float
            | Ty::Bool
            | Ty::Str
            | Ty::Unit
            | Ty::Never
            | Ty::Param(_)
            | Ty::Error => t.clone(),
        }
    }

    /// Apply the substitution to a row.
    #[must_use]
    pub fn apply_row(&self, r: &Row) -> Row {
        let effects = r
            .effects
            .iter()
            .map(|e| EffectInstance {
                name: e.name,
                args: e.args.iter().map(|a| self.apply(a)).collect(),
            })
            .collect();
        let tail = r.tail.and_then(|v| self.row_vars.get(&v).cloned());
        if let Some(tail_row) = tail {
            // Inline the tail-row's effects + replace tail with inner tail.
            let mut merged = Row {
                effects,
                tail: None,
            };
            merged.effects.extend(tail_row.effects);
            merged.tail = tail_row.tail;
            self.apply_row(&merged) // follow further tail-chain if any
        } else {
            Row {
                effects,
                tail: r.tail,
            }
        }
    }

    /// Insert a `TyVar -> Ty` mapping. Caller must ensure `v` is not already mapped
    /// and that `occurs_check` has passed.
    pub fn bind_ty(&mut self, v: TyVar, t: Ty) {
        self.ty_vars.insert(v, t);
    }

    /// Insert a `RowVar -> Row` mapping.
    pub fn bind_row(&mut self, v: RowVar, r: Row) {
        self.row_vars.insert(v, r);
    }
}

/// Type-variable allocator + row-variable allocator.
#[derive(Debug, Default)]
pub struct TyCtx {
    next_ty: u32,
    next_row: u32,
}

impl TyCtx {
    /// Build an empty allocator.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            next_ty: 0,
            next_row: 0,
        }
    }

    /// Allocate a fresh type variable.
    pub fn fresh_ty(&mut self) -> Ty {
        let v = TyVar(self.next_ty);
        self.next_ty = self.next_ty.saturating_add(1);
        Ty::Var(v)
    }

    /// Allocate a fresh row variable.
    pub fn fresh_row(&mut self) -> RowVar {
        let v = RowVar(self.next_row);
        self.next_row = self.next_row.saturating_add(1);
        v
    }

    /// Current type-variable counter (for diagnostic + snapshot use).
    #[must_use]
    pub const fn ty_count(&self) -> u32 {
        self.next_ty
    }

    /// Current row-variable counter.
    #[must_use]
    pub const fn row_count(&self) -> u32 {
        self.next_row
    }
}

/// Map from a HIR node's `HirId` to its inferred type. Populated by the inference pass.
#[derive(Debug, Default, Clone)]
pub struct TypeMap {
    pub types: BTreeMap<u32, Ty>,
}

impl TypeMap {
    /// Empty map.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record the inferred type for a HIR node.
    pub fn insert(&mut self, id: crate::arena::HirId, t: Ty) {
        self.types.insert(id.0, t);
    }

    /// Lookup the inferred type of a HIR node.
    #[must_use]
    pub fn get(&self, id: crate::arena::HirId) -> Option<&Ty> {
        self.types.get(&id.0)
    }

    /// Number of recorded types.
    #[must_use]
    pub fn len(&self) -> usize {
        self.types.len()
    }

    /// `true` iff no types have been recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.types.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::{ArrayLen, EffectInstance, Row, RowVar, Subst, Ty, TyCtx, TyVar, TypeMap};
    use crate::arena::{DefId, HirId};
    use crate::symbol::Interner;

    #[test]
    fn fresh_vars_distinct() {
        let mut ctx = TyCtx::new();
        let a = ctx.fresh_ty();
        let b = ctx.fresh_ty();
        assert_ne!(a, b);
        let r1 = ctx.fresh_row();
        let r2 = ctx.fresh_row();
        assert_ne!(r1, r2);
    }

    #[test]
    fn subst_apply_resolves_single_chain() {
        let mut s = Subst::new();
        s.bind_ty(TyVar(0), Ty::Int);
        assert_eq!(s.apply(&Ty::Var(TyVar(0))), Ty::Int);
    }

    #[test]
    fn subst_apply_chains_through_variables() {
        let mut s = Subst::new();
        s.bind_ty(TyVar(0), Ty::Var(TyVar(1)));
        s.bind_ty(TyVar(1), Ty::Bool);
        assert_eq!(s.apply(&Ty::Var(TyVar(0))), Ty::Bool);
    }

    #[test]
    fn subst_apply_distributes_through_constructors() {
        let mut s = Subst::new();
        s.bind_ty(TyVar(0), Ty::Int);
        s.bind_ty(TyVar(1), Ty::Bool);
        let t = Ty::Tuple(vec![Ty::Var(TyVar(0)), Ty::Var(TyVar(1))]);
        assert_eq!(s.apply(&t), Ty::Tuple(vec![Ty::Int, Ty::Bool]));
    }

    #[test]
    fn subst_apply_through_fn_type() {
        let mut s = Subst::new();
        s.bind_ty(TyVar(0), Ty::Int);
        s.bind_ty(TyVar(1), Ty::Float);
        let t = Ty::Fn {
            params: vec![Ty::Var(TyVar(0))],
            return_ty: Box::new(Ty::Var(TyVar(1))),
            effect_row: Row::pure(),
        };
        match s.apply(&t) {
            Ty::Fn {
                params, return_ty, ..
            } => {
                assert_eq!(params, vec![Ty::Int]);
                assert_eq!(*return_ty, Ty::Float);
            }
            _ => panic!("expected Fn"),
        }
    }

    #[test]
    fn row_pure_and_closed() {
        let p = Row::pure();
        assert!(p.is_pure());
        let interner = Interner::new();
        let gpu = interner.intern("GPU");
        let c = Row::closed(vec![EffectInstance {
            name: gpu,
            args: Vec::new(),
        }]);
        assert!(!c.is_pure());
        assert_eq!(c.effects.len(), 1);
    }

    #[test]
    fn row_canonicalize_sorts_by_name() {
        let interner = Interner::new();
        let a = interner.intern("AAA");
        let b = interner.intern("BBB");
        let row = Row::closed(vec![
            EffectInstance {
                name: b,
                args: Vec::new(),
            },
            EffectInstance {
                name: a,
                args: Vec::new(),
            },
        ]);
        let canon = row.canonicalize();
        assert_eq!(canon.effects[0].name, a);
        assert_eq!(canon.effects[1].name, b);
    }

    #[test]
    fn subst_apply_row_expands_tail() {
        let mut s = Subst::new();
        let interner = Interner::new();
        let e1 = interner.intern("E1");
        let e2 = interner.intern("E2");
        s.bind_row(
            RowVar(0),
            Row::closed(vec![EffectInstance {
                name: e2,
                args: Vec::new(),
            }]),
        );
        let r = Row {
            effects: vec![EffectInstance {
                name: e1,
                args: Vec::new(),
            }],
            tail: Some(RowVar(0)),
        };
        let applied = s.apply_row(&r);
        assert_eq!(applied.effects.len(), 2);
        assert!(applied.tail.is_none());
    }

    #[test]
    fn type_map_records_and_retrieves() {
        let mut m = TypeMap::new();
        assert!(m.is_empty());
        m.insert(HirId(7), Ty::Int);
        assert_eq!(m.get(HirId(7)), Some(&Ty::Int));
        assert_eq!(m.len(), 1);
    }

    #[test]
    fn named_type_preserves_def_id() {
        let t = Ty::Named {
            def: DefId(5),
            args: vec![Ty::Int],
        };
        if let Ty::Named { def, args } = t {
            assert_eq!(def, DefId(5));
            assert_eq!(args.len(), 1);
        } else {
            panic!("expected Named");
        }
    }

    #[test]
    fn array_len_literal_vs_opaque() {
        let l1 = ArrayLen::Literal(10);
        let l2 = ArrayLen::Opaque;
        let l3 = ArrayLen::Var(3);
        assert_ne!(l1, l2);
        assert_ne!(l2, l3);
    }
}
