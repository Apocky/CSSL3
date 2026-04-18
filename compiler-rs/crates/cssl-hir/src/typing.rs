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

// ───────────────────────────────────────────────────────────────────────────
// § Type schemes — foundation for let-generalization + rank-N polymorphism
//   (T3.4-phase-3-let-gen ; T3-D14 when integrated into infer).
// ───────────────────────────────────────────────────────────────────────────

/// A type scheme : the body type plus the list of type/row variables it
/// universally quantifies over. At a `let`-binding in Hindley-Milner, the
/// inferred type's free type variables (those not fixed by the surrounding
/// environment) are **generalized** into a scheme ; at each use-site the
/// scheme is **instantiated** with fresh variables, yielding a fresh
/// monomorphic type for that call.
///
/// Monomorphic types (no quantified vars) round-trip unchanged through
/// [`Scheme::monomorphic`] + [`Scheme::instantiate`].
///
/// § STAGE-0 SCOPE
/// - Rank-1 polymorphism only (quantified vars live at the outermost layer).
/// - Rank-N via nested `Scheme` inside `Ty` is phase-2e+ work.
/// - Constraints (e.g., `T: Differentiable`) are not yet tracked — quantified
///   vars are unconstrained.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scheme {
    /// Type variables quantified by this scheme.
    pub ty_vars: Vec<TyVar>,
    /// Row variables quantified by this scheme.
    pub row_vars: Vec<RowVar>,
    /// Body type (may reference any of `ty_vars` / `row_vars` ; may also
    /// reference free vars fixed by the surrounding environment).
    pub body: Ty,
}

impl Scheme {
    /// Build a monomorphic scheme (no quantified vars) wrapping `ty`.
    #[must_use]
    pub const fn monomorphic(ty: Ty) -> Self {
        Self {
            ty_vars: Vec::new(),
            row_vars: Vec::new(),
            body: ty,
        }
    }

    /// `true` iff the scheme quantifies over no variables.
    #[must_use]
    pub fn is_monomorphic(&self) -> bool {
        self.ty_vars.is_empty() && self.row_vars.is_empty()
    }

    /// Number of universally-quantified type variables.
    #[must_use]
    pub fn rank(&self) -> usize {
        self.ty_vars.len() + self.row_vars.len()
    }

    /// Instantiate the scheme at a use-site : replace each quantified var with
    /// a **fresh** inference variable, yielding a monomorphic `Ty` ready for
    /// unification. Each call produces independent fresh vars — multiple
    /// instantiations of the same scheme do not share state.
    ///
    /// § INVARIANT
    /// The caller must pass a [`TyCtx`] whose `next_ty` counter is strictly
    /// greater than the highest [`TyVar`] in `self.ty_vars` (and similarly for
    /// `row_vars`). In real inference this is automatic — generalization
    /// happens after the scheme's bound vars have been allocated by the same
    /// ctx. Hand-built test fixtures should advance the counter explicitly
    /// via `let _ = ctx.fresh_ty();`.
    ///
    /// # Example
    /// ```text
    /// let id : ∀a. a → a = |x| x
    /// id(1)       // instantiate { a ↦ τ₀ }  → τ₀ → τ₀ , unify τ₀ = Int
    /// id(true)    // instantiate { a ↦ τ₁ }  → τ₁ → τ₁ , unify τ₁ = Bool
    /// ```
    pub fn instantiate(&self, ctx: &mut TyCtx) -> Ty {
        if self.is_monomorphic() {
            return self.body.clone();
        }
        let mut subst = Subst::new();
        for v in &self.ty_vars {
            let fresh = ctx.fresh_ty();
            subst.ty_vars.insert(*v, fresh);
        }
        for rv in &self.row_vars {
            let fresh_rv = ctx.fresh_row();
            subst.row_vars.insert(
                *rv,
                Row {
                    effects: Vec::new(),
                    tail: Some(fresh_rv),
                },
            );
        }
        subst.apply(&self.body)
    }

    /// Collect the type variables bound by this scheme (ty_vars field).
    /// Useful when verifying schemes at API boundaries.
    #[must_use]
    pub fn bound_ty_vars(&self) -> &[TyVar] {
        &self.ty_vars
    }

    /// Collect the row variables bound by this scheme (row_vars field).
    #[must_use]
    pub fn bound_row_vars(&self) -> &[RowVar] {
        &self.row_vars
    }
}

/// Collect every free `TyVar` reachable from `ty`. Used by
/// [`generalize`] to find the candidates for quantification.
#[must_use]
pub fn free_ty_vars(ty: &Ty) -> Vec<TyVar> {
    let mut out = Vec::new();
    collect_ty_vars(ty, &mut out);
    out.sort_by_key(|v| v.0);
    out.dedup();
    out
}

fn collect_ty_vars(ty: &Ty, out: &mut Vec<TyVar>) {
    match ty {
        Ty::Int
        | Ty::Float
        | Ty::Bool
        | Ty::Str
        | Ty::Unit
        | Ty::Never
        | Ty::Param(_)
        | Ty::Error => {}
        Ty::Var(v) => out.push(*v),
        Ty::Named { args, .. } => {
            for a in args {
                collect_ty_vars(a, out);
            }
        }
        Ty::Tuple(elems) => {
            for e in elems {
                collect_ty_vars(e, out);
            }
        }
        Ty::Ref { inner, .. } | Ty::Slice { elem: inner } => collect_ty_vars(inner, out),
        Ty::Fn {
            params, return_ty, ..
        } => {
            for p in params {
                collect_ty_vars(p, out);
            }
            collect_ty_vars(return_ty, out);
        }
        Ty::Array { elem, .. } => collect_ty_vars(elem, out),
    }
}

/// Collect every free `RowVar` reachable from `ty`. Walks through
/// `Ty::Fn { effect_row, .. }` + nested type arguments.
#[must_use]
pub fn free_row_vars(ty: &Ty) -> Vec<RowVar> {
    let mut out = Vec::new();
    collect_row_vars(ty, &mut out);
    out.sort_by_key(|v| v.0);
    out.dedup();
    out
}

fn collect_row_vars(ty: &Ty, out: &mut Vec<RowVar>) {
    match ty {
        Ty::Int
        | Ty::Float
        | Ty::Bool
        | Ty::Str
        | Ty::Unit
        | Ty::Never
        | Ty::Param(_)
        | Ty::Var(_)
        | Ty::Error => {}
        Ty::Named { args, .. } => {
            for a in args {
                collect_row_vars(a, out);
            }
        }
        Ty::Tuple(elems) => {
            for e in elems {
                collect_row_vars(e, out);
            }
        }
        Ty::Ref { inner, .. } | Ty::Slice { elem: inner } => collect_row_vars(inner, out),
        Ty::Fn {
            params,
            return_ty,
            effect_row,
        } => {
            for p in params {
                collect_row_vars(p, out);
            }
            collect_row_vars(return_ty, out);
            if let Some(tail) = effect_row.tail {
                out.push(tail);
            }
        }
        Ty::Array { elem, .. } => collect_row_vars(elem, out),
    }
}

/// Generalize `ty` with respect to the set of free vars fixed by the
/// surrounding environment. Any free `TyVar` in `ty` not present in
/// `env_free_ty` becomes a quantified scheme-variable ; same for row-vars.
///
/// This is the "gen" half of Hindley-Milner's `let`-binding rule :
///   `Γ ⊢ e : τ       α̅ = ftv(τ) − ftv(Γ)
///     ───────────────────────────────────
///     Γ ⊢ let x = e in ... : [x ↦ ∀α̅. τ]`
///
/// Callers typically build `env_free_ty` by walking the current `TypingEnv`
/// and collecting every `TyVar` referenced by any binding in scope.
#[must_use]
pub fn generalize<S: std::hash::BuildHasher, T: std::hash::BuildHasher>(
    env_free_ty: &std::collections::HashSet<TyVar, S>,
    env_free_row: &std::collections::HashSet<RowVar, T>,
    ty: Ty,
) -> Scheme {
    let ty_vars: Vec<TyVar> = free_ty_vars(&ty)
        .into_iter()
        .filter(|v| !env_free_ty.contains(v))
        .collect();
    let row_vars: Vec<RowVar> = free_row_vars(&ty)
        .into_iter()
        .filter(|v| !env_free_row.contains(v))
        .collect();
    Scheme {
        ty_vars,
        row_vars,
        body: ty,
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

    // ─────────────────────────────────────────────────────────────────────
    // § Scheme + generalize + instantiate — foundation for let-gen
    // ─────────────────────────────────────────────────────────────────────

    use super::{free_row_vars, free_ty_vars, generalize, Scheme};
    use std::collections::HashSet;

    #[test]
    fn free_ty_vars_of_primitives_is_empty() {
        assert!(free_ty_vars(&Ty::Int).is_empty());
        assert!(free_ty_vars(&Ty::Bool).is_empty());
        assert!(free_ty_vars(&Ty::Unit).is_empty());
    }

    #[test]
    fn free_ty_vars_of_var_is_self() {
        let v = Ty::Var(TyVar(3));
        let free = free_ty_vars(&v);
        assert_eq!(free, vec![TyVar(3)]);
    }

    #[test]
    fn free_ty_vars_of_tuple_collects_all() {
        let t = Ty::Tuple(vec![Ty::Var(TyVar(1)), Ty::Var(TyVar(2)), Ty::Int]);
        let free = free_ty_vars(&t);
        assert_eq!(free, vec![TyVar(1), TyVar(2)]);
    }

    #[test]
    fn free_ty_vars_of_fn_collects_params_return_and_dedupes() {
        let t = Ty::Fn {
            params: vec![Ty::Var(TyVar(0)), Ty::Var(TyVar(1))],
            return_ty: Box::new(Ty::Var(TyVar(0))),
            effect_row: Row::pure(),
        };
        let free = free_ty_vars(&t);
        // Dedupe : TyVar(0) appears twice but only once in free-set.
        assert_eq!(free, vec![TyVar(0), TyVar(1)]);
    }

    #[test]
    fn free_row_vars_of_fn_collects_tail() {
        let t = Ty::Fn {
            params: vec![Ty::Int],
            return_ty: Box::new(Ty::Int),
            effect_row: Row {
                effects: Vec::new(),
                tail: Some(RowVar(5)),
            },
        };
        let free = free_row_vars(&t);
        assert_eq!(free, vec![RowVar(5)]);
    }

    #[test]
    fn free_row_vars_of_pure_row_is_empty() {
        let t = Ty::Fn {
            params: vec![Ty::Int],
            return_ty: Box::new(Ty::Int),
            effect_row: Row::pure(),
        };
        assert!(free_row_vars(&t).is_empty());
    }

    #[test]
    fn monomorphic_scheme_has_no_quantified_vars() {
        let s = Scheme::monomorphic(Ty::Int);
        assert!(s.is_monomorphic());
        assert_eq!(s.rank(), 0);
    }

    #[test]
    fn monomorphic_scheme_instantiates_to_identical_body() {
        let mut ctx = TyCtx::new();
        let s = Scheme::monomorphic(Ty::Tuple(vec![Ty::Int, Ty::Bool]));
        let inst = s.instantiate(&mut ctx);
        // Monomorphic → unchanged + no fresh-var allocation.
        assert_eq!(inst, Ty::Tuple(vec![Ty::Int, Ty::Bool]));
        assert_eq!(ctx.ty_count(), 0);
    }

    #[test]
    fn generalize_identity_fn_binds_single_var() {
        // f : τ₀ → τ₀ (the untyped identity fn)
        let ty = Ty::Fn {
            params: vec![Ty::Var(TyVar(0))],
            return_ty: Box::new(Ty::Var(TyVar(0))),
            effect_row: Row::pure(),
        };
        let env_ty: HashSet<TyVar> = HashSet::new();
        let env_row: HashSet<RowVar> = HashSet::new();
        let s = generalize(&env_ty, &env_row, ty);
        // TyVar(0) is free in ty + not in env → generalized.
        assert_eq!(s.ty_vars, vec![TyVar(0)]);
        assert_eq!(s.rank(), 1);
    }

    #[test]
    fn generalize_env_fixed_vars_are_not_quantified() {
        // f : τ₀ → τ₁ where τ₀ is environment-fixed (e.g., bound by an outer
        // scope) and τ₁ is free. Only τ₁ should be generalized.
        let ty = Ty::Fn {
            params: vec![Ty::Var(TyVar(0))],
            return_ty: Box::new(Ty::Var(TyVar(1))),
            effect_row: Row::pure(),
        };
        let mut env_ty: HashSet<TyVar> = HashSet::new();
        env_ty.insert(TyVar(0));
        let env_row: HashSet<RowVar> = HashSet::new();
        let s = generalize(&env_ty, &env_row, ty);
        assert_eq!(s.ty_vars, vec![TyVar(1)]);
    }

    #[test]
    fn instantiate_uses_fresh_variables() {
        // Production invariant : the TyCtx counter must be ≥ max(bound_vars)+1
        // so freshly-allocated vars don't collide with the scheme's quantified
        // vars. In real inference this is automatic (quantified vars are
        // already-allocated by the time generalize runs) ; tests manually
        // advance the counter.
        let mut ctx = TyCtx::new();
        let _ = ctx.fresh_ty(); // allocate TyVar(0) so next fresh is TyVar(1)
        let s = Scheme {
            ty_vars: vec![TyVar(0)],
            row_vars: Vec::new(),
            body: Ty::Fn {
                params: vec![Ty::Var(TyVar(0))],
                return_ty: Box::new(Ty::Var(TyVar(0))),
                effect_row: Row::pure(),
            },
        };
        let inst = s.instantiate(&mut ctx);
        // Fresh ty-var allocated ; body rewritten with the fresh.
        if let Ty::Fn {
            params, return_ty, ..
        } = inst
        {
            // Both param + return must reference the same fresh var.
            assert_eq!(params.len(), 1);
            assert_eq!(&params[0], &*return_ty);
            // The fresh var is NOT the original TyVar(0).
            assert_ne!(&params[0], &Ty::Var(TyVar(0)));
        } else {
            panic!("expected Fn after instantiate");
        }
    }

    #[test]
    fn two_instantiations_produce_distinct_fresh_vars() {
        let mut ctx = TyCtx::new();
        let _ = ctx.fresh_ty(); // advance past the scheme's bound TyVar(0)
        let s = Scheme {
            ty_vars: vec![TyVar(0)],
            row_vars: Vec::new(),
            body: Ty::Var(TyVar(0)),
        };
        let a = s.instantiate(&mut ctx);
        let b = s.instantiate(&mut ctx);
        // Each instantiation gets its own fresh var.
        assert_ne!(a, b);
    }

    #[test]
    fn generalize_then_instantiate_roundtrips_monomorphic_values() {
        // A monomorphic type (no free vars) round-trips : generalize yields
        // rank-0 scheme, instantiate returns the same concrete type.
        let ty = Ty::Int;
        let env_ty: HashSet<TyVar> = HashSet::new();
        let env_row: HashSet<RowVar> = HashSet::new();
        let s = generalize(&env_ty, &env_row, ty.clone());
        assert!(s.is_monomorphic());
        let mut ctx = TyCtx::new();
        let inst = s.instantiate(&mut ctx);
        assert_eq!(inst, ty);
    }

    #[test]
    fn scheme_bound_accessors_return_ref_to_fields() {
        let s = Scheme {
            ty_vars: vec![TyVar(0), TyVar(1)],
            row_vars: vec![RowVar(7)],
            body: Ty::Unit,
        };
        assert_eq!(s.bound_ty_vars(), &[TyVar(0), TyVar(1)]);
        assert_eq!(s.bound_row_vars(), &[RowVar(7)]);
    }
}
