//! Typing environment — a scope-stack mapping `Symbol` to `Ty`, plus item
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

use std::collections::HashMap;

use crate::arena::DefId;
use crate::symbol::Symbol;
use crate::typing::Ty;

/// A single lexical scope — `Symbol → Ty` map.
#[derive(Debug, Default, Clone)]
pub struct TypeScope {
    bindings: HashMap<Symbol, Ty>,
}

impl TypeScope {
    /// Build an empty scope.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a binding ; returns the previous type if the name was already in scope.
    pub fn insert(&mut self, name: Symbol, t: Ty) -> Option<Ty> {
        self.bindings.insert(name, t)
    }

    /// Lookup a name in this scope only.
    #[must_use]
    pub fn lookup(&self, name: Symbol) -> Option<&Ty> {
        self.bindings.get(&name)
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
#[derive(Debug, Default)]
pub struct TypingEnv {
    stack: Vec<TypeScope>,
    item_sigs: HashMap<DefId, Ty>,
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

    /// Insert a binding into the innermost scope. No-op if no scope is active (callers
    /// are expected to `enter()` before binding locals).
    pub fn insert_local(&mut self, name: Symbol, t: Ty) -> Option<Ty> {
        self.stack.last_mut().and_then(|s| s.insert(name, t))
    }

    /// Lookup a local binding (innermost-to-outermost scope).
    #[must_use]
    pub fn lookup_local(&self, name: Symbol) -> Option<&Ty> {
        for scope in self.stack.iter().rev() {
            if let Some(t) = scope.lookup(name) {
                return Some(t);
            }
        }
        None
    }

    /// Register an item signature.
    pub fn register_item(&mut self, name: Symbol, def: DefId, t: Ty) {
        self.item_sigs.insert(def, t);
        self.item_names.insert(name, def);
    }

    /// Lookup an item signature by `DefId`.
    #[must_use]
    pub fn item_sig(&self, def: DefId) -> Option<&Ty> {
        self.item_sigs.get(&def)
    }

    /// Lookup `DefId` for a top-level name.
    #[must_use]
    pub fn item_def(&self, name: Symbol) -> Option<DefId> {
        self.item_names.get(&name).copied()
    }

    /// Resolve a name — prefer local binding, fall back to item signature.
    #[must_use]
    pub fn lookup(&self, name: Symbol) -> Option<&Ty> {
        if let Some(t) = self.lookup_local(name) {
            return Some(t);
        }
        let def = self.item_names.get(&name)?;
        self.item_sigs.get(def)
    }

    /// Iterate over all registered item signatures (stable order not guaranteed).
    pub fn item_sigs(&self) -> impl Iterator<Item = (&DefId, &Ty)> {
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
