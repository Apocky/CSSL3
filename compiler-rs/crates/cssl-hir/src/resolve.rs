//! Basic name-resolution — scope-tree + path resolution.
//!
//! § SCOPE
//!   T3.3 scope : single-file module lookup. We build a module-level symbol table
//!   mapping `Symbol → DefId` at lowering time, then walk HIR expressions / types
//!   and fill in `Option<DefId>` slots for path-references that resolve to a known
//!   definition. Unresolved references stay `None` — the elaborator (T3.4) reports
//!   diagnostics where resolution is required.
//!
//! § DEFERRED (T3.4+)
//!   - Cross-module / cross-file resolution (honoring `use` paths + re-exports).
//!   - Method resolution (dispatching on receiver type + interface impls).
//!   - Macro expansion and hygiene.
//!   - Shadowing rules inside nested scopes (block-locals vs item-level).

use std::collections::HashMap;

use crate::arena::DefId;
use crate::symbol::Symbol;

/// A simple scope — `Symbol → DefId` map with an optional parent for nested lookup.
#[derive(Debug, Default, Clone)]
pub struct Scope {
    bindings: HashMap<Symbol, DefId>,
}

impl Scope {
    /// Construct an empty scope.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a `Symbol → DefId` binding. Returns the previous binding if any.
    pub fn insert(&mut self, name: Symbol, def: DefId) -> Option<DefId> {
        self.bindings.insert(name, def)
    }

    /// Lookup a name in this scope only.
    #[must_use]
    pub fn lookup(&self, name: Symbol) -> Option<DefId> {
        self.bindings.get(&name).copied()
    }

    /// Number of bindings in this scope.
    #[must_use]
    pub fn len(&self) -> usize {
        self.bindings.len()
    }

    /// `true` iff no bindings exist.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bindings.is_empty()
    }
}

/// A scope-map : the root (module-level) scope plus a stack of nested scopes.
///
/// The top of the stack is the innermost scope. Lookups walk inward-to-outward.
#[derive(Debug, Default)]
pub struct ScopeMap {
    /// Module-level scope (never popped).
    module: Scope,
    /// Nested scopes — pushed when entering a block / fn / impl body.
    stack: Vec<Scope>,
}

impl ScopeMap {
    /// Build an empty scope-map.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Push a fresh nested scope.
    pub fn enter(&mut self) {
        self.stack.push(Scope::new());
    }

    /// Pop the innermost nested scope. No-op if we're already at module level.
    pub fn leave(&mut self) {
        self.stack.pop();
    }

    /// Insert into the innermost nested scope (or module if stack is empty).
    pub fn insert(&mut self, name: Symbol, def: DefId) -> Option<DefId> {
        if let Some(top) = self.stack.last_mut() {
            top.insert(name, def)
        } else {
            self.module.insert(name, def)
        }
    }

    /// Insert directly into the module-level scope regardless of stack depth.
    /// Used when lowering top-level item declarations — these must be visible
    /// across the whole module even if we're inside a nested-block context when
    /// pre-registering them.
    pub fn insert_module(&mut self, name: Symbol, def: DefId) -> Option<DefId> {
        self.module.insert(name, def)
    }

    /// Look up a name — innermost-to-outermost.
    #[must_use]
    pub fn lookup(&self, name: Symbol) -> Option<DefId> {
        for scope in self.stack.iter().rev() {
            if let Some(d) = scope.lookup(name) {
                return Some(d);
            }
        }
        self.module.lookup(name)
    }

    /// Look up only at module level (skip nested-scopes).
    #[must_use]
    pub fn lookup_module(&self, name: Symbol) -> Option<DefId> {
        self.module.lookup(name)
    }

    /// Resolve a single-segment path. Multi-segment paths route through `resolve_path`
    /// which walks the module tree (T3.4 work).
    #[must_use]
    pub fn resolve_single(&self, name: Symbol) -> Option<DefId> {
        self.lookup(name)
    }

    /// Number of nested-scopes currently active (0 at module level).
    #[must_use]
    pub fn depth(&self) -> usize {
        self.stack.len()
    }
}

#[cfg(test)]
mod tests {
    use super::{Scope, ScopeMap};
    use crate::arena::DefId;
    use crate::symbol::Interner;

    #[test]
    fn scope_insert_and_lookup() {
        let interner = Interner::new();
        let name = interner.intern("foo");
        let mut s = Scope::new();
        assert_eq!(s.lookup(name), None);
        s.insert(name, DefId(7));
        assert_eq!(s.lookup(name), Some(DefId(7)));
    }

    #[test]
    fn nested_scope_shadows_outer() {
        let interner = Interner::new();
        let name = interner.intern("x");
        let mut map = ScopeMap::new();
        map.insert_module(name, DefId(1));
        map.enter();
        map.insert(name, DefId(2));
        assert_eq!(map.lookup(name), Some(DefId(2)));
        map.leave();
        assert_eq!(map.lookup(name), Some(DefId(1)));
    }

    #[test]
    fn module_scope_persists() {
        let interner = Interner::new();
        let name = interner.intern("y");
        let mut map = ScopeMap::new();
        map.insert_module(name, DefId(42));
        map.enter();
        map.enter();
        assert_eq!(map.lookup(name), Some(DefId(42)));
        map.leave();
        map.leave();
        assert_eq!(map.lookup_module(name), Some(DefId(42)));
    }

    #[test]
    fn depth_tracks_stack() {
        let mut map = ScopeMap::new();
        assert_eq!(map.depth(), 0);
        map.enter();
        assert_eq!(map.depth(), 1);
        map.enter();
        assert_eq!(map.depth(), 2);
        map.leave();
        assert_eq!(map.depth(), 1);
        map.leave();
        assert_eq!(map.depth(), 0);
    }
}
