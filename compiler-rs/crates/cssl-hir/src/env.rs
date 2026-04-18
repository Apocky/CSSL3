//! Typing environment — a scope-stack mapping `Symbol` to [`Scheme`], plus item
//! signatures indexed by `DefId`.
//!
//! § SCOPE-STACK (expression-level bindings)
//!   A stack of per-block maps. When entering a fn body / block / lambda / match-arm,
//!   push a new scope ; when leaving, pop. Lookups walk inward-to-outward.
//!
//! § ITEM-SIGNATURES (module-level definitions)
//!   A flat map from `DefId → Ty` populated during an initial pass over items
//!   (see `infer::collect_item_signatures`). Item types can reference other items
//!   freely — cross-referencing is OK because all item-types are registered before
//!   any body is type-checked.
//!
//! § LET-GENERALIZATION (T3-D15)
//!   Scope-storage uses [`Scheme`] so `let x = e` can generalize `e`'s inferred
//!   type into a rank-1 polymorphic scheme. Use-sites (in path-resolution)
//!   instantiate the scheme with fresh inference-vars. Monomorphic insertions
//!   auto-wrap via [`Scheme::monomorphic`] — the legacy `insert(name, ty)` API
//!   continues to work for callers that don't care about generalization.

use std::collections::HashMap;

use crate::arena::DefId;
use crate::symbol::Symbol;
use crate::typing::{Scheme, Ty};

/// A single lexical scope — `Symbol → Scheme` map.
///
/// Monomorphic callers can continue using [`Self::insert`] + [`Self::lookup`]
/// — these auto-wrap/unwrap through [`Scheme::monomorphic`] + reading `.body`.
/// Polymorphic callers (let-generalization) use [`Self::insert_scheme`] +
/// [`Self::lookup_scheme`].
#[derive(Debug, Default, Clone)]
pub struct TypeScope {
    bindings: HashMap<Symbol, Scheme>,
}

impl TypeScope {
    /// Build an empty scope.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a monomorphic binding (auto-wraps the `Ty` in a rank-0 scheme).
    /// Returns the previous `Ty` (read from the previous scheme's body) if the
    /// name was already in scope.
    pub fn insert(&mut self, name: Symbol, t: Ty) -> Option<Ty> {
        self.bindings
            .insert(name, Scheme::monomorphic(t))
            .map(|s| s.body)
    }

    /// Insert a full polymorphic scheme. Returns the previous scheme if any.
    pub fn insert_scheme(&mut self, name: Symbol, scheme: Scheme) -> Option<Scheme> {
        self.bindings.insert(name, scheme)
    }

    /// Lookup a name in this scope only — returns the `Ty` body of the stored
    /// scheme. Equivalent to `lookup_scheme(name).map(|s| &s.body)` and
    /// semantically correct for monomorphic schemes ; polymorphic callers
    /// should use [`Self::lookup_scheme`] + [`Scheme::instantiate`].
    #[must_use]
    pub fn lookup(&self, name: Symbol) -> Option<&Ty> {
        self.bindings.get(&name).map(|s| &s.body)
    }

    /// Lookup the full scheme (preserves quantified vars).
    #[must_use]
    pub fn lookup_scheme(&self, name: Symbol) -> Option<&Scheme> {
        self.bindings.get(&name)
    }

    /// Iterate over all schemes in this scope (stable order not guaranteed).
    pub fn schemes(&self) -> impl Iterator<Item = (&Symbol, &Scheme)> {
        self.bindings.iter()
    }

    /// Number of bindings.
    #[must_use]
    pub fn len(&self) -> usize {
        self.bindings.len()
    }

    /// `true` iff no bindings.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bindings.is_empty()
    }
}

/// Typing environment : a stack of `TypeScope`s + a flat item-signature table.
///
/// § ITEM-SIG STORAGE (post-T3-D17)
///   Item signatures are stored as [`Scheme`], not raw [`Ty`]. Monomorphic
///   items (e.g., structs without generics) auto-wrap via
///   [`Scheme::monomorphic`]. Generic fn items carry real rank-1 schemes
///   with quantified ty-vars ; each call-site instantiates with fresh vars
///   via [`Scheme::instantiate`] to avoid var-sharing across calls.
#[derive(Debug, Default)]
pub struct TypingEnv {
    stack: Vec<TypeScope>,
    item_sigs: HashMap<DefId, Scheme>,
    item_names: HashMap<Symbol, DefId>,
}

impl TypingEnv {
    /// Build an empty environment.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Enter a fresh nested scope.
    pub fn enter(&mut self) {
        self.stack.push(TypeScope::new());
    }

    /// Leave the innermost scope.
    pub fn leave(&mut self) {
        self.stack.pop();
    }

    /// Current nested-scope depth.
    #[must_use]
    pub fn depth(&self) -> usize {
        self.stack.len()
    }

    /// Insert a monomorphic binding into the innermost scope. No-op if no
    /// scope is active (callers are expected to `enter()` before binding
    /// locals).
    pub fn insert_local(&mut self, name: Symbol, t: Ty) -> Option<Ty> {
        self.stack.last_mut().and_then(|s| s.insert(name, t))
    }

    /// Insert a polymorphic scheme into the innermost scope. Used by
    /// let-generalization at let-boundaries (`let x = e` where `e : τ` is
    /// generalized to `∀α̅. τ` where α̅ = ftv(τ) − ftv(Γ)).
    pub fn insert_local_scheme(&mut self, name: Symbol, scheme: Scheme) -> Option<Scheme> {
        self.stack
            .last_mut()
            .and_then(|s| s.insert_scheme(name, scheme))
    }

    /// Lookup a local binding (innermost-to-outermost scope). Returns the
    /// stored scheme's body `Ty` — use [`Self::lookup_local_scheme`] for the
    /// full quantified form.
    #[must_use]
    pub fn lookup_local(&self, name: Symbol) -> Option<&Ty> {
        for scope in self.stack.iter().rev() {
            if let Some(t) = scope.lookup(name) {
                return Some(t);
            }
        }
        None
    }

    /// Lookup a local scheme — returns the full quantified form so callers
    /// can instantiate with fresh vars.
    #[must_use]
    pub fn lookup_local_scheme(&self, name: Symbol) -> Option<&Scheme> {
        for scope in self.stack.iter().rev() {
            if let Some(s) = scope.lookup_scheme(name) {
                return Some(s);
            }
        }
        None
    }

    /// Collect the set of free type-variables across every binding in every
    /// active scope + every item-signature. Used by let-generalization to
    /// determine which type-vars are "fixed by the environment" and therefore
    /// must NOT be generalized at the let-boundary.
    ///
    /// § COMPUTATION
    ///   free_env = ⋃ scope ∈ stack : ⋃ (n, σ) ∈ scope : ftv(σ.body) − σ.ty_vars
    ///            + ⋃ (d, τ) ∈ item_sigs : ftv(τ)
    ///
    /// Vars that are already-quantified inside a scheme don't count as
    /// environment-fixed — they're already bound.
    #[must_use]
    pub fn free_ty_vars(&self) -> std::collections::HashSet<crate::typing::TyVar> {
        use std::collections::HashSet;
        let mut out: HashSet<crate::typing::TyVar> = HashSet::new();
        for scope in &self.stack {
            for (_, scheme) in scope.schemes() {
                let bound: HashSet<_> = scheme.ty_vars.iter().copied().collect();
                for v in crate::typing::free_ty_vars(&scheme.body) {
                    if !bound.contains(&v) {
                        out.insert(v);
                    }
                }
            }
        }
        for scheme in self.item_sigs.values() {
            let bound: HashSet<_> = scheme.ty_vars.iter().copied().collect();
            for v in crate::typing::free_ty_vars(&scheme.body) {
                if !bound.contains(&v) {
                    out.insert(v);
                }
            }
        }
        out
    }

    /// Parallel collector for free row-variables. Mirrors [`Self::free_ty_vars`].
    #[must_use]
    pub fn free_row_vars(&self) -> std::collections::HashSet<crate::typing::RowVar> {
        use std::collections::HashSet;
        let mut out: HashSet<crate::typing::RowVar> = HashSet::new();
        for scope in &self.stack {
            for (_, scheme) in scope.schemes() {
                let bound: HashSet<_> = scheme.row_vars.iter().copied().collect();
                for v in crate::typing::free_row_vars(&scheme.body) {
                    if !bound.contains(&v) {
                        out.insert(v);
                    }
                }
            }
        }
        for scheme in self.item_sigs.values() {
            let bound: HashSet<_> = scheme.row_vars.iter().copied().collect();
            for v in crate::typing::free_row_vars(&scheme.body) {
                if !bound.contains(&v) {
                    out.insert(v);
                }
            }
        }
        out
    }

    /// Register an item signature with a monomorphic wrap.
    pub fn register_item(&mut self, name: Symbol, def: DefId, t: Ty) {
        self.item_sigs.insert(def, Scheme::monomorphic(t));
        self.item_names.insert(name, def);
    }

    /// Register an item with a full polymorphic scheme (e.g., generic fn with
    /// quantified ty-vars).
    pub fn register_item_scheme(&mut self, name: Symbol, def: DefId, scheme: Scheme) {
        self.item_sigs.insert(def, scheme);
        self.item_names.insert(name, def);
    }

    /// Lookup the `Ty` body of an item signature by `DefId`. Equivalent to
    /// `item_scheme(def).map(|s| &s.body)`. Semantically correct for
    /// monomorphic items ; polymorphic callers should use
    /// [`Self::item_scheme`] + [`Scheme::instantiate`] for fresh-var
    /// independence per use-site.
    #[must_use]
    pub fn item_sig(&self, def: DefId) -> Option<&Ty> {
        self.item_sigs.get(&def).map(|s| &s.body)
    }

    /// Lookup the full polymorphic scheme for an item.
    #[must_use]
    pub fn item_scheme(&self, def: DefId) -> Option<&Scheme> {
        self.item_sigs.get(&def)
    }

    /// Lookup `DefId` for a top-level name.
    #[must_use]
    pub fn item_def(&self, name: Symbol) -> Option<DefId> {
        self.item_names.get(&name).copied()
    }

    /// Resolve a name — prefer local binding, fall back to item signature.
    /// Returns the `Ty` body ; polymorphic callers should use
    /// [`Self::item_scheme`] on the resolved `DefId` + instantiate.
    #[must_use]
    pub fn lookup(&self, name: Symbol) -> Option<&Ty> {
        if let Some(t) = self.lookup_local(name) {
            return Some(t);
        }
        let def = self.item_names.get(&name)?;
        self.item_sigs.get(def).map(|s| &s.body)
    }

    /// Iterate over all registered item signatures (stable order not guaranteed).
    /// Returns `(DefId, &Ty)` pairs reading the scheme body — polymorphic
    /// callers should use [`Self::item_schemes`] instead.
    pub fn item_sigs(&self) -> impl Iterator<Item = (&DefId, &Ty)> {
        self.item_sigs.iter().map(|(d, s)| (d, &s.body))
    }

    /// Iterate over registered item schemes (full polymorphic form).
    pub fn item_schemes(&self) -> impl Iterator<Item = (&DefId, &Scheme)> {
        self.item_sigs.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::{TypeScope, TypingEnv};
    use crate::arena::DefId;
    use crate::symbol::Interner;
    use crate::typing::Ty;

    #[test]
    fn scope_insert_and_lookup() {
        let interner = Interner::new();
        let x = interner.intern("x");
        let mut s = TypeScope::new();
        s.insert(x, Ty::Int);
        assert_eq!(s.lookup(x), Some(&Ty::Int));
    }

    #[test]
    fn env_depth_tracking() {
        let mut env = TypingEnv::new();
        assert_eq!(env.depth(), 0);
        env.enter();
        assert_eq!(env.depth(), 1);
        env.enter();
        assert_eq!(env.depth(), 2);
        env.leave();
        assert_eq!(env.depth(), 1);
    }

    #[test]
    fn env_innermost_shadows_outer() {
        let interner = Interner::new();
        let x = interner.intern("x");
        let mut env = TypingEnv::new();
        env.enter();
        env.insert_local(x, Ty::Int);
        env.enter();
        env.insert_local(x, Ty::Bool);
        assert_eq!(env.lookup_local(x), Some(&Ty::Bool));
        env.leave();
        assert_eq!(env.lookup_local(x), Some(&Ty::Int));
    }

    #[test]
    fn item_sig_registration_and_lookup() {
        let interner = Interner::new();
        let foo = interner.intern("foo");
        let mut env = TypingEnv::new();
        let foo_ty = Ty::Fn {
            params: vec![Ty::Int],
            return_ty: Box::new(Ty::Bool),
            effect_row: crate::typing::Row::pure(),
        };
        env.register_item(foo, DefId(0), foo_ty.clone());
        assert_eq!(env.item_sig(DefId(0)), Some(&foo_ty));
        assert_eq!(env.item_def(foo), Some(DefId(0)));
        assert_eq!(env.lookup(foo), Some(&foo_ty));
    }

    #[test]
    fn local_takes_precedence_over_item() {
        let interner = Interner::new();
        let x = interner.intern("x");
        let mut env = TypingEnv::new();
        env.register_item(x, DefId(0), Ty::Int);
        env.enter();
        env.insert_local(x, Ty::Bool);
        assert_eq!(env.lookup(x), Some(&Ty::Bool));
    }
}
