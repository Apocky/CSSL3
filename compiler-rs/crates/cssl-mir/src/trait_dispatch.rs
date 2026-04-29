//! T11-D99 — Trait dispatch + impl resolution.
//!
//! § PURPOSE
//!
//! Until this slice CSSLv3 method calls were a SPEC-HOLE : `obj.method(args)`
//! either fell through to a syntactic recognizer (`Box::new`, `Some`, etc.)
//! or hit an opaque `cssl.field` op that no codegen path consumed. Drop /
//! Display / Debug / operator overloading / generic-bounded methods all
//! lived as deferred TODOs. This slice closes the loop : it lifts
//! `HirImpl` blocks (both inherent and trait-impl) into a queryable
//! [`TraitImplTable`], wires `body_lower` so any `obj.method(args)` /
//! `Trait::method(args)` HIR call resolves through that table to a
//! mangled impl-fn name, and registers `Drop` impls for use by the
//! scope-exit `drop` injector (see [`crate::drop_inject`]).
//!
//! § DISPATCH STRATEGY : MONOMORPHIZATION
//!
//! Per the slice landmines, this implementation follows the existing
//! T11-D38..D50 monomorph-quartet precedent : every trait-method call
//! resolves at compile time to a concrete mangled function ; no vtable
//! indirection. This means :
//!
//!   - `impl Drop for File { fn drop(&mut self) }` produces a MirFunc
//!     called `File__drop` (one underscore-pair to mark the impl-method
//!     boundary).
//!   - `impl<T> Drop for Vec<T> { fn drop(&mut self) }` produces a
//!     mangled `Vec_<T-mangle>__drop` per concrete substitution.
//!   - `obj.method(args)` resolves the impl by looking up `self_ty`
//!     in the table ; if `self_ty` is generic, substitution flows
//!     through [`crate::monomorph::specialize_generic_impl`] which is
//!     already in place from T11-D49.
//!
//! § DESIGN
//!
//! [`TraitImplTable`] is built by walking the [`HirModule`] once. Each
//! `HirItem::Impl` (whether `trait_: Some(_)` or `None`) becomes one
//! [`TraitImplEntry`]. The entry preserves the source-form `self_ty` +
//! the trait-name (or `None` for inherent) + the per-method mangled
//! names. Lookup keys are :
//!
//!   - `(self_ty_name, method_name)` → mangled-impl-fn-name
//!   - `("Drop", self_ty_name)` → drop-fn-name (the dedicated
//!     fast-path used by the scope-exit injector — same lookup
//!     under a different key shape).
//!
//! § FAST-PATHS PRESERVED
//!
//! Per the slice landmines `Box::new` / `Some` / `None` / `Ok` / `Err` /
//! `format(...)` / `fs::*` / `net::*` continue to be matched
//! SYNTACTICALLY in `body_lower::lower_call` BEFORE any trait-dispatch
//! lookup. Trait-dispatch is the slow-path that fires when the syntactic
//! recognizers all decline, exactly mirroring how Rust's prelude works
//! (the prelude shadows the compiler's intrinsic recognizers). User-
//! defined traits + impls land on the slow-path ; the recognizer fast-
//! paths remain authoritative for stdlib builtins.
//!
//! § SPEC : `specs/03_TYPES.csl` § GENERIC-COLLECTIONS + § GENERICS +
//!         INTERFACES + new § TRAIT-DISPATCH (added in this slice).
//!
//! § OUT-OF-SCOPE
//!
//!   - Vtable / dyn-Trait runtime dispatch (deferred — mono only at
//!     stage-0 per the `monomorphized @ HIR → MIR ≡ zero-cost` rule
//!     in `specs/03_TYPES.csl § GENERICS + INTERFACES`).
//!   - Trait coherence (orphan rule) — relaxed at stage-0 since the
//!     workspace is closed-world ; documented in DECISIONS.
//!   - Associated-type projection (`<T as Trait>::Assoc`) — the table
//!     records assoc-type defs but the resolver returns the HIR type
//!     unchanged ; substitution + projection is a follow-up slice.
//!   - Default methods on traits — interfaces declare fn-signatures only
//!     at stage-0 ; default-bodies on the trait itself are deferred.

use std::collections::HashMap;

use cssl_hir::{HirImpl, HirInterface, HirItem, HirModule, HirType, HirTypeKind, Interner, Symbol};

use crate::monomorph::TypeSubst;

/// One entry in the [`TraitImplTable`] : a single source-form `impl` block
/// (either trait-impl `impl Foo for Bar { ... }` or inherent `impl Bar { ... }`).
#[derive(Debug, Clone)]
pub struct TraitImplEntry {
    /// `Some(name)` for `impl Trait for Self`, `None` for inherent impls.
    pub trait_name: Option<Symbol>,
    /// The self-type's leading-segment name (e.g., `Vec`, `Box`, `File`).
    /// Multi-segment self-types are rejected at table-build time at stage-0.
    pub self_ty_name: Symbol,
    /// `true` iff the impl block declared generic parameters (`impl<T> ...`).
    pub is_generic: bool,
    /// Method-name → mangled-impl-fn-name map.
    ///
    /// Stage-0 mangling : `<self-ty-frag>__<method>` for inherent impls,
    /// or `<self-ty-frag>__<trait>__<method>` for trait impls. The dunder
    /// pair separates self-ty from the impl's discriminator ; trait_-name
    /// is sandwiched in between for trait-impls so two distinct trait-impls
    /// of the same self-type don't collide on a method-name clash.
    pub method_mangled: HashMap<Symbol, String>,
}

impl TraitImplEntry {
    /// `true` iff this entry implements the `Drop` trait.
    #[must_use]
    pub fn is_drop_impl(&self, interner: &Interner) -> bool {
        match self.trait_name {
            Some(s) => interner.resolve(s) == "Drop",
            None => false,
        }
    }
}

/// The trait-impl resolution table — one entry per source-form `impl` block.
///
/// Built by [`build_trait_impl_table`] from a [`HirModule`] in a single
/// pass. Lookup is by `(self-ty-name, method-name)` for inherent impls
/// and `(trait-name, self-ty-name)` for trait-impl Drop dispatch.
#[derive(Debug, Clone, Default)]
pub struct TraitImplTable {
    entries: Vec<TraitImplEntry>,
    /// Index : self-ty-name → indices of entries where it appears.
    by_self_ty: HashMap<Symbol, Vec<usize>>,
    /// Index : trait-name → indices of entries implementing it.
    by_trait: HashMap<Symbol, Vec<usize>>,
}

impl TraitImplTable {
    /// Empty table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of impl entries indexed.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// `true` iff the table is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Iterate entries in registration order.
    pub fn entries(&self) -> impl Iterator<Item = &TraitImplEntry> + '_ {
        self.entries.iter()
    }

    /// Push a new impl entry. Returns the inserted index.
    pub fn push(&mut self, entry: TraitImplEntry) -> usize {
        let idx = self.entries.len();
        self.by_self_ty
            .entry(entry.self_ty_name)
            .or_default()
            .push(idx);
        if let Some(t) = entry.trait_name {
            self.by_trait.entry(t).or_default().push(idx);
        }
        self.entries.push(entry);
        idx
    }

    /// Resolve `obj.method(args)` : given the self-type leading-segment
    /// name + the method-name, returns the mangled impl-fn name (the first
    /// matching entry — coherence is single-impl-per-method-name at stage-0).
    ///
    /// ‼ Inherent impls are checked before trait impls so a directly-
    ///   declared inherent method shadows trait impls of the same name.
    ///   This matches Rust + spec § GENERICS + INTERFACES.
    #[must_use]
    pub fn resolve_method(&self, self_ty: Symbol, method: Symbol) -> Option<&str> {
        let indices = self.by_self_ty.get(&self_ty)?;
        // Two-pass : prefer inherent, then trait.
        for pass_inherent_first in [true, false] {
            for &idx in indices {
                let entry = &self.entries[idx];
                let is_inherent = entry.trait_name.is_none();
                if is_inherent != pass_inherent_first {
                    continue;
                }
                if let Some(name) = entry.method_mangled.get(&method) {
                    return Some(name);
                }
            }
        }
        None
    }

    /// Look up the `Drop` impl for `self_ty` (if any). Returns the mangled
    /// drop-fn name. Used by [`crate::drop_inject`] for scope-exit cleanup.
    #[must_use]
    pub fn drop_for(&self, interner: &Interner, self_ty: Symbol) -> Option<&str> {
        let indices = self.by_self_ty.get(&self_ty)?;
        for &idx in indices {
            let entry = &self.entries[idx];
            if !entry.is_drop_impl(interner) {
                continue;
            }
            // The Drop trait has a single method `drop` — find it via the
            // method-mangled map keyed by the interner's `drop` symbol.
            // Callers that have the symbol cached can use [`drop_for_with_sym`]
            // to skip the resolve call.
            for (m, mangled) in &entry.method_mangled {
                if interner.resolve(*m) == "drop" {
                    return Some(mangled);
                }
            }
        }
        None
    }

    /// Same as [`drop_for`] but with the `drop`-symbol provided up front to
    /// skip an inner resolve loop on hot paths (the drop-injector calls this
    /// once per scoped binding).
    #[must_use]
    pub fn drop_for_with_sym(&self, self_ty: Symbol, drop_sym: Symbol) -> Option<&str> {
        let indices = self.by_self_ty.get(&self_ty)?;
        for &idx in indices {
            let entry = &self.entries[idx];
            // Inherent impls do NOT participate in Drop dispatch — only
            // trait-impl Drop entries register. We can't check trait-name
            // by-string here (no interner), so we encode this via
            // `trait_name == Some(_)` AND the method-name match. A future
            // slice can plumb a precomputed `drop-trait-symbol` for tighter
            // discrimination ; at stage-0 no other trait declares a `drop`
            // method so the match is unambiguous.
            if entry.trait_name.is_none() {
                continue;
            }
            if let Some(mangled) = entry.method_mangled.get(&drop_sym) {
                return Some(mangled);
            }
        }
        None
    }

    /// Iterate every impl entry whose `trait_name` is the given symbol.
    pub fn impls_of_trait(&self, trait_sym: Symbol) -> impl Iterator<Item = &TraitImplEntry> + '_ {
        let indices = self.by_trait.get(&trait_sym).cloned().unwrap_or_default();
        indices.into_iter().map(move |idx| &self.entries[idx])
    }

    /// `true` iff the table records `impl Trait for SelfTy { ... }` for the
    /// given (trait, self-type) pair. Used by the trait-bound checker to
    /// validate `T : Trait` constraints at call-sites.
    #[must_use]
    pub fn has_impl(&self, trait_name: Symbol, self_ty: Symbol) -> bool {
        let Some(indices) = self.by_self_ty.get(&self_ty) else {
            return false;
        };
        indices
            .iter()
            .any(|&i| self.entries[i].trait_name == Some(trait_name))
    }
}

/// Build the trait-impl table from a HIR module.
///
/// § ALGORITHM
///   1. Walk every `HirItem::Impl` in the module (top-level + nested-module).
///   2. For each, extract :
///      - `trait_name` from `trait_: Option<HirType>` (single-segment Path).
///      - `self_ty_name` from `self_ty: HirType` (single-segment Path).
///      - per-method mangled name via [`mangle_method_name`].
///   3. Push the entry.
///
/// § SCOPE
///   - Single-segment trait + self-ty paths only at stage-0 ; multi-segment
///     `mod::Trait for mod::Foo` is a follow-up slice.
///   - Generic impls (`impl<T> Foo<T>`) are recorded but their per-method
///     mangled names use the GENERIC base form ; concrete monomorphic
///     mangled names are produced at call-site time by
///     [`crate::monomorph::specialize_generic_impl`] +
///     [`crate::auto_monomorph::auto_monomorphize_impls`].
#[must_use]
pub fn build_trait_impl_table(module: &HirModule, interner: &Interner) -> TraitImplTable {
    let mut table = TraitImplTable::new();
    walk_items(&module.items, interner, &mut table);
    table
}

fn walk_items(items: &[HirItem], interner: &Interner, table: &mut TraitImplTable) {
    for item in items {
        match item {
            HirItem::Impl(i) => {
                if let Some(entry) = build_entry(i, interner) {
                    table.push(entry);
                }
            }
            HirItem::Module(m) => {
                if let Some(inner) = &m.items {
                    walk_items(inner, interner, table);
                }
            }
            _ => {}
        }
    }
}

fn build_entry(i: &HirImpl, interner: &Interner) -> Option<TraitImplEntry> {
    let self_ty_name = leading_path_symbol(&i.self_ty)?;
    let trait_name = match &i.trait_ {
        Some(t) => Some(leading_path_symbol(t)?),
        None => None,
    };
    let is_generic = !i.generics.params.is_empty();
    let mut method_mangled = HashMap::new();
    for f in &i.fns {
        let mangled = mangle_method_name(self_ty_name, trait_name, f.name, interner);
        method_mangled.insert(f.name, mangled);
    }
    Some(TraitImplEntry {
        trait_name,
        self_ty_name,
        is_generic,
        method_mangled,
    })
}

/// Extract the single-segment leading symbol from a path-form `HirType`.
/// Returns `None` for non-path types or empty paths.
#[must_use]
pub fn leading_path_symbol(t: &HirType) -> Option<Symbol> {
    match &t.kind {
        HirTypeKind::Path { path, .. } => path.last().copied(),
        _ => None,
    }
}

/// Stage-0 method-name mangling.
///
/// § SHAPE
///   - Inherent impl  : `<self>__<method>`     (e.g., `Box__value`)
///   - Trait impl     : `<self>__<trait>__<method>`  (e.g., `File__Drop__drop`)
///
/// The dunder pair (`__`) separates the impl-discriminator from the method-
/// name ; the trait-name slot disappears for inherent impls so the
/// inherent + trait-impl mangle-spaces are disjoint.
#[must_use]
pub fn mangle_method_name(
    self_ty: Symbol,
    trait_name: Option<Symbol>,
    method: Symbol,
    interner: &Interner,
) -> String {
    let s = interner.resolve(self_ty);
    let m = interner.resolve(method);
    match trait_name {
        Some(t) => {
            let t_str = interner.resolve(t);
            format!("{s}__{t_str}__{m}")
        }
        None => format!("{s}__{m}"),
    }
}

/// Mangle the per-monomorphization concrete impl method-name. Used by
/// [`crate::auto_monomorph::auto_monomorphize_impls`] to produce the same
/// concrete name when generic impls get specialized at call-site.
///
/// § SHAPE
///   - Inherent generic impl : `<self>_<subst-frag>__<method>`
///   - Trait generic impl    : `<self>_<subst-frag>__<trait>__<method>`
///
/// where `<subst-frag>` follows the same convention as
/// [`crate::monomorph::mangle_specialization_name`].
#[must_use]
pub fn mangle_concrete_method_name(
    self_ty_with_subst: &str,
    trait_name: Option<&str>,
    method: &str,
) -> String {
    match trait_name {
        Some(t) => format!("{self_ty_with_subst}__{t}__{method}"),
        None => format!("{self_ty_with_subst}__{method}"),
    }
}

// ═════════════════════════════════════════════════════════════════════════
// § TRAIT-BOUND CHECKING
// ═════════════════════════════════════════════════════════════════════════

/// Verify that a generic-fn's type-args satisfy each declared trait-bound.
///
/// Given the bounds list (per generic param) and the concrete substitution,
/// this walks each `T : Trait` bound and confirms the substituted type has a
/// recorded `impl Trait for <ConcreteT>` in `table`. Returns the list of
/// unsatisfied (param-name, missing-trait-name, concrete-self-ty) tuples ;
/// an empty vec means all bounds are satisfied.
///
/// § EXAMPLE
///   `fn map<T : Display>(x : T) -> String { x.to_string() }`
///
///   Call-site `map::<i32>(5)` :
///     - bounds : `[(T, [Display])]`
///     - subst  : `{T ↦ i32}`
///     - check  : `table.has_impl(Display, i32)` ⇒ if false, error.
#[must_use]
pub fn check_trait_bounds(
    table: &TraitImplTable,
    bounds: &[(Symbol, Vec<Symbol>)],
    subst: &TypeSubst,
    interner: &Interner,
) -> Vec<TraitBoundViolation> {
    let mut out = Vec::new();
    for (param_name, traits) in bounds {
        let Some(concrete) = subst.get(param_name) else {
            // No substitution for this param — caller error, but we don't
            // fail bound-checking on it (the monomorphization pass would
            // surface the missing-subst as a separate diagnostic).
            continue;
        };
        let Some(self_sym) = leading_path_symbol(concrete) else {
            continue;
        };
        for &trait_sym in traits {
            if !table.has_impl(trait_sym, self_sym) {
                out.push(TraitBoundViolation {
                    param_name: *param_name,
                    trait_name: trait_sym,
                    concrete_self_ty: self_sym,
                    diagnostic: format!(
                        "trait bound `{} : {}` not satisfied — no `impl {} for {}` in scope",
                        interner.resolve(*param_name),
                        interner.resolve(trait_sym),
                        interner.resolve(trait_sym),
                        interner.resolve(self_sym)
                    ),
                });
            }
        }
    }
    out
}

/// One unsatisfied trait-bound diagnostic returned by [`check_trait_bounds`].
#[derive(Debug, Clone)]
pub struct TraitBoundViolation {
    pub param_name: Symbol,
    pub trait_name: Symbol,
    pub concrete_self_ty: Symbol,
    pub diagnostic: String,
}

// ═════════════════════════════════════════════════════════════════════════
// § INTERFACE / TRAIT METADATA — recorded at table-build time so the
//   trait-bound checker can see which trait-method names exist (for
//   `T.method()` dispatch when `T` is a generic-bound parameter).
// ═════════════════════════════════════════════════════════════════════════

/// Per-trait metadata : trait-name → set of method-names it declares.
///
/// Built once at table construction time so the bound-checker (and the
/// dispatch resolver, when the receiver is a generic-bound parameter)
/// can answer "does trait Foo declare a method `bar`?" without re-walking
/// the HirInterface list. At stage-0 we don't track signatures here —
/// only names — because the body_lower trait-dispatch path mangles by
/// name and the per-impl `method_mangled` map carries the concrete
/// signature reference back to MIR.
#[derive(Debug, Clone, Default)]
pub struct TraitInterfaceTable {
    /// trait-name → method-names
    methods: HashMap<Symbol, Vec<Symbol>>,
}

impl TraitInterfaceTable {
    /// Empty.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register an interface : every method-symbol becomes queryable.
    pub fn register(&mut self, iface: &HirInterface) {
        let entry = self.methods.entry(iface.name).or_default();
        for f in &iface.fns {
            if !entry.contains(&f.name) {
                entry.push(f.name);
            }
        }
    }

    /// `true` iff trait `trait_sym` declares a method named `method`.
    #[must_use]
    pub fn has_method(&self, trait_sym: Symbol, method: Symbol) -> bool {
        match self.methods.get(&trait_sym) {
            Some(v) => v.contains(&method),
            None => false,
        }
    }

    /// Iterate methods of a trait.
    pub fn methods_of(&self, trait_sym: Symbol) -> impl Iterator<Item = Symbol> + '_ {
        self.methods
            .get(&trait_sym)
            .cloned()
            .unwrap_or_default()
            .into_iter()
    }

    /// Number of trait registrations (one per interface).
    #[must_use]
    pub fn len(&self) -> usize {
        self.methods.len()
    }

    /// `true` iff empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.methods.is_empty()
    }

    /// Number of methods declared by `trait_sym`. Returns 0 if not registered.
    #[must_use]
    pub fn method_count(&self, trait_sym: Symbol) -> usize {
        self.methods.get(&trait_sym).map_or(0, Vec::len)
    }
}

/// Build the trait-interface metadata table from a HirModule.
///
/// Walks every `HirItem::Interface` and registers each. Nested-module
/// interfaces participate (matching the trait-impl table's reach).
#[must_use]
pub fn build_trait_interface_table(module: &HirModule) -> TraitInterfaceTable {
    let mut table = TraitInterfaceTable::new();
    walk_interfaces(&module.items, &mut table);
    table
}

// ═════════════════════════════════════════════════════════════════════════
// § MODULE-LEVEL TRAIT-BOUND VALIDATION
// ═════════════════════════════════════════════════════════════════════════

/// One module-level violation : (call-site span, source-form fn-name,
/// per-bound diagnostics).
#[derive(Debug, Clone)]
pub struct ModuleBoundViolation {
    /// Source span of the offending call-site.
    pub span: cssl_ast::Span,
    /// Source-form callee fn name.
    pub callee: String,
    /// Per-bound violation details (one entry per failing bound).
    pub failures: Vec<TraitBoundViolation>,
}

/// Walk every turbofish / type-args call-site in `module` and confirm every
/// bound declared on the callee's generic params is satisfied by the call-
/// site's concrete substitution.
///
/// This is the module-level analog of [`check_trait_bounds`] : it does the
/// indexing + walking so callers can dispatch a single check. Returns the
/// list of violations ; an empty vec means all bounds are satisfied.
///
/// § ALGORITHM
///   1. Index generic fns by name (matching `auto_monomorph::auto_monomorphize`).
///   2. For each turbofish call-site, build a [`TypeSubst`] from the callee's
///      generics + the supplied type-args.
///   3. Build the bounds list `[(param_name, [trait_sym...]) ; …]` from each
///      generic-param's `bounds` field (filter to single-segment Path).
///   4. Invoke [`check_trait_bounds`] ; collect non-empty results.
#[must_use]
pub fn validate_trait_bounds_in_module(
    module: &HirModule,
    interner: &Interner,
    table: &TraitImplTable,
) -> Vec<ModuleBoundViolation> {
    use cssl_hir::{HirFn, HirItem};

    let mut fn_index: HashMap<Symbol, &HirFn> = HashMap::new();
    for item in &module.items {
        if let HirItem::Fn(f) = item {
            if !f.generics.params.is_empty() {
                fn_index.insert(f.name, f);
            }
        }
    }

    let mut violations = Vec::new();
    for item in &module.items {
        if let HirItem::Fn(f) = item {
            if let Some(body) = &f.body {
                walk_block_for_call_sites(body, interner, &fn_index, table, &mut violations);
            }
        }
    }
    violations
}

fn walk_block_for_call_sites(
    block: &cssl_hir::HirBlock,
    interner: &Interner,
    fn_index: &HashMap<Symbol, &cssl_hir::HirFn>,
    table: &TraitImplTable,
    violations: &mut Vec<ModuleBoundViolation>,
) {
    for stmt in &block.stmts {
        match &stmt.kind {
            cssl_hir::HirStmtKind::Let { value, .. } => {
                if let Some(v) = value {
                    walk_expr_for_call_sites(v, interner, fn_index, table, violations);
                }
            }
            cssl_hir::HirStmtKind::Expr(e) => {
                walk_expr_for_call_sites(e, interner, fn_index, table, violations);
            }
            cssl_hir::HirStmtKind::Item(_) => {}
        }
    }
    if let Some(t) = &block.trailing {
        walk_expr_for_call_sites(t, interner, fn_index, table, violations);
    }
}

fn walk_expr_for_call_sites(
    expr: &cssl_hir::HirExpr,
    interner: &Interner,
    fn_index: &HashMap<Symbol, &cssl_hir::HirFn>,
    table: &TraitImplTable,
    violations: &mut Vec<ModuleBoundViolation>,
) {
    use crate::monomorph::TypeSubst;
    use cssl_hir::{HirArrayExpr, HirCallArg, HirExprKind};

    match &expr.kind {
        HirExprKind::Call {
            callee,
            args,
            type_args,
        } => {
            // Validate this call-site's bounds (if it's a turbofish).
            if !type_args.is_empty() {
                if let HirExprKind::Path { segments, .. } = &callee.kind {
                    if segments.len() == 1 {
                        if let Some(fn_decl) = fn_index.get(&segments[0]) {
                            // Build subst.
                            let mut subst = TypeSubst::new();
                            for (param, ty) in fn_decl.generics.params.iter().zip(type_args.iter())
                            {
                                subst.bind(param.name, ty.clone());
                            }
                            // Build bounds from each generic param's bounds field.
                            let mut bounds: Vec<(Symbol, Vec<Symbol>)> = Vec::new();
                            for p in &fn_decl.generics.params {
                                let trait_syms: Vec<Symbol> =
                                    p.bounds.iter().filter_map(leading_path_symbol).collect();
                                if !trait_syms.is_empty() {
                                    bounds.push((p.name, trait_syms));
                                }
                            }
                            if !bounds.is_empty() {
                                let failures = check_trait_bounds(table, &bounds, &subst, interner);
                                if !failures.is_empty() {
                                    violations.push(ModuleBoundViolation {
                                        span: expr.span,
                                        callee: interner.resolve(segments[0]),
                                        failures,
                                    });
                                }
                            }
                        }
                    }
                }
            }
            // Recurse into callee + args.
            walk_expr_for_call_sites(callee, interner, fn_index, table, violations);
            for a in args {
                let e = match a {
                    HirCallArg::Positional(e) | HirCallArg::Named { value: e, .. } => e,
                };
                walk_expr_for_call_sites(e, interner, fn_index, table, violations);
            }
        }
        HirExprKind::Block(b) => {
            walk_block_for_call_sites(b, interner, fn_index, table, violations);
        }
        HirExprKind::If {
            cond,
            then_branch,
            else_branch,
        } => {
            walk_expr_for_call_sites(cond, interner, fn_index, table, violations);
            walk_block_for_call_sites(then_branch, interner, fn_index, table, violations);
            if let Some(e) = else_branch {
                walk_expr_for_call_sites(e, interner, fn_index, table, violations);
            }
        }
        HirExprKind::Match { scrutinee, arms } => {
            walk_expr_for_call_sites(scrutinee, interner, fn_index, table, violations);
            for arm in arms {
                walk_expr_for_call_sites(&arm.body, interner, fn_index, table, violations);
            }
        }
        HirExprKind::For { iter, body, .. } => {
            walk_expr_for_call_sites(iter, interner, fn_index, table, violations);
            walk_block_for_call_sites(body, interner, fn_index, table, violations);
        }
        HirExprKind::While { cond, body } => {
            walk_expr_for_call_sites(cond, interner, fn_index, table, violations);
            walk_block_for_call_sites(body, interner, fn_index, table, violations);
        }
        HirExprKind::Loop { body }
        | HirExprKind::Region { body, .. }
        | HirExprKind::With { body, .. } => {
            walk_block_for_call_sites(body, interner, fn_index, table, violations);
        }
        HirExprKind::Field { obj, .. } | HirExprKind::Paren(obj) => {
            walk_expr_for_call_sites(obj, interner, fn_index, table, violations);
        }
        HirExprKind::Index { obj, index } => {
            walk_expr_for_call_sites(obj, interner, fn_index, table, violations);
            walk_expr_for_call_sites(index, interner, fn_index, table, violations);
        }
        HirExprKind::Binary { lhs, rhs, .. }
        | HirExprKind::Assign { lhs, rhs, .. }
        | HirExprKind::Pipeline { lhs, rhs }
        | HirExprKind::Compound { lhs, rhs, .. } => {
            walk_expr_for_call_sites(lhs, interner, fn_index, table, violations);
            walk_expr_for_call_sites(rhs, interner, fn_index, table, violations);
        }
        HirExprKind::Unary { operand, .. } => {
            walk_expr_for_call_sites(operand, interner, fn_index, table, violations);
        }
        HirExprKind::Cast { expr, .. } | HirExprKind::Run { expr } => {
            walk_expr_for_call_sites(expr, interner, fn_index, table, violations);
        }
        HirExprKind::Range { lo, hi, .. } => {
            if let Some(l) = lo {
                walk_expr_for_call_sites(l, interner, fn_index, table, violations);
            }
            if let Some(h) = hi {
                walk_expr_for_call_sites(h, interner, fn_index, table, violations);
            }
        }
        HirExprKind::TryDefault { expr, default } => {
            walk_expr_for_call_sites(expr, interner, fn_index, table, violations);
            walk_expr_for_call_sites(default, interner, fn_index, table, violations);
        }
        HirExprKind::Try { expr } => {
            walk_expr_for_call_sites(expr, interner, fn_index, table, violations);
        }
        HirExprKind::Return { value } | HirExprKind::Break { value, .. } => {
            if let Some(v) = value {
                walk_expr_for_call_sites(v, interner, fn_index, table, violations);
            }
        }
        HirExprKind::Lambda { body, .. } => {
            walk_expr_for_call_sites(body, interner, fn_index, table, violations);
        }
        HirExprKind::Tuple(elems) => {
            for e in elems {
                walk_expr_for_call_sites(e, interner, fn_index, table, violations);
            }
        }
        HirExprKind::Array(arr) => match arr {
            HirArrayExpr::List(es) => {
                for e in es {
                    walk_expr_for_call_sites(e, interner, fn_index, table, violations);
                }
            }
            HirArrayExpr::Repeat { elem, len } => {
                walk_expr_for_call_sites(elem, interner, fn_index, table, violations);
                walk_expr_for_call_sites(len, interner, fn_index, table, violations);
            }
        },
        HirExprKind::Struct { fields, spread, .. } => {
            for fld in fields {
                if let Some(v) = &fld.value {
                    walk_expr_for_call_sites(v, interner, fn_index, table, violations);
                }
            }
            if let Some(s) = spread {
                walk_expr_for_call_sites(s, interner, fn_index, table, violations);
            }
        }
        HirExprKind::Perform { args, .. } => {
            for a in args {
                let e = match a {
                    HirCallArg::Positional(e) | HirCallArg::Named { value: e, .. } => e,
                };
                walk_expr_for_call_sites(e, interner, fn_index, table, violations);
            }
        }
        HirExprKind::Continue { .. }
        | HirExprKind::Path { .. }
        | HirExprKind::Literal(_)
        | HirExprKind::SectionRef { .. }
        | HirExprKind::Error => {}
    }
}

fn walk_interfaces(items: &[HirItem], table: &mut TraitInterfaceTable) {
    for item in items {
        match item {
            HirItem::Interface(i) => table.register(i),
            HirItem::Module(m) => {
                if let Some(inner) = &m.items {
                    walk_interfaces(inner, table);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cssl_ast::{SourceFile, SourceId, Surface};
    use cssl_hir::lower_module;

    fn lower(src: &str) -> (HirModule, Interner, SourceFile) {
        let f = SourceFile::new(SourceId::first(), "<t>", src, Surface::RustHybrid);
        let toks = cssl_lex::lex(&f);
        let (cst, _bag) = cssl_parse::parse(&f, &toks);
        let (hir, interner, _) = lower_module(&f, &cst);
        (hir, interner, f)
    }

    #[test]
    fn empty_module_yields_empty_table() {
        let (m, i, _) = lower("");
        let t = build_trait_impl_table(&m, &i);
        assert!(t.is_empty());
        assert_eq!(t.len(), 0);
    }

    #[test]
    fn inherent_impl_lands_in_table() {
        let src = r"
            struct Foo { x : i32 }
            impl Foo {
                fn bar(self : Foo) -> i32 { 1 }
            }
        ";
        let (m, i, _) = lower(src);
        let t = build_trait_impl_table(&m, &i);
        assert_eq!(t.len(), 1);
        let foo_sym = i.intern("Foo");
        let bar_sym = i.intern("bar");
        let mangled = t.resolve_method(foo_sym, bar_sym).expect("resolve bar");
        assert_eq!(mangled, "Foo__bar");
    }

    #[test]
    fn trait_impl_uses_three_segment_mangle() {
        let src = r"
            interface Greeter { fn greet(self : Foo) -> i32 ; }
            struct Foo { x : i32 }
            impl Greeter for Foo {
                fn greet(self : Foo) -> i32 { self.x }
            }
        ";
        let (m, i, _) = lower(src);
        let t = build_trait_impl_table(&m, &i);
        // 1 trait-impl entry. (Interface itself doesn't enter the table.)
        assert_eq!(t.len(), 1);
        let foo_sym = i.intern("Foo");
        let greet_sym = i.intern("greet");
        let mangled = t.resolve_method(foo_sym, greet_sym).expect("resolve greet");
        assert_eq!(mangled, "Foo__Greeter__greet");
    }

    #[test]
    fn drop_impl_resolves_via_drop_for() {
        let src = r"
            interface Drop { fn drop(self : Foo) ; }
            struct Foo { x : i32 }
            impl Drop for Foo {
                fn drop(self : Foo) {  }
            }
        ";
        let (m, i, _) = lower(src);
        let t = build_trait_impl_table(&m, &i);
        let foo_sym = i.intern("Foo");
        let mangled = t.drop_for(&i, foo_sym).expect("Drop fn");
        assert_eq!(mangled, "Foo__Drop__drop");
    }

    #[test]
    fn inherent_shadows_trait_when_method_collides() {
        // Both an inherent `bar` and a trait-impl `bar` ; resolver must
        // pick the inherent.
        let src = r"
            interface BarTrait { fn bar(self : Foo) -> i32 ; }
            struct Foo { x : i32 }
            impl Foo {
                fn bar(self : Foo) -> i32 { 1 }
            }
            impl BarTrait for Foo {
                fn bar(self : Foo) -> i32 { 2 }
            }
        ";
        let (m, i, _) = lower(src);
        let t = build_trait_impl_table(&m, &i);
        let foo_sym = i.intern("Foo");
        let bar_sym = i.intern("bar");
        let mangled = t.resolve_method(foo_sym, bar_sym).expect("resolve bar");
        // Inherent must win.
        assert_eq!(mangled, "Foo__bar");
    }

    #[test]
    #[allow(clippy::min_ident_chars, clippy::many_single_char_names)]
    fn has_impl_records_trait_self_pair() {
        let src = r"
            interface Greeter { fn greet(self : Foo) -> i32 ; }
            struct Foo { x : i32 }
            impl Greeter for Foo {
                fn greet(self : Foo) -> i32 { 1 }
            }
        ";
        let (m, i, _) = lower(src);
        let t = build_trait_impl_table(&m, &i);
        let g = i.intern("Greeter");
        let f = i.intern("Foo");
        let bogus = i.intern("Nope");
        assert!(t.has_impl(g, f));
        assert!(!t.has_impl(g, bogus));
    }

    #[test]
    fn missing_method_returns_none() {
        let src = r"
            struct Foo { x : i32 }
            impl Foo {
                fn bar(self : Foo) -> i32 { 1 }
            }
        ";
        let (m, i, _) = lower(src);
        let t = build_trait_impl_table(&m, &i);
        let foo_sym = i.intern("Foo");
        let nope_sym = i.intern("nope");
        assert!(t.resolve_method(foo_sym, nope_sym).is_none());
    }

    #[test]
    fn missing_self_ty_returns_none() {
        let src = r"
            struct Foo { x : i32 }
            impl Foo {
                fn bar(self : Foo) -> i32 { 1 }
            }
        ";
        let (m, i, _) = lower(src);
        let t = build_trait_impl_table(&m, &i);
        let bogus_sym = i.intern("Bogus");
        let bar_sym = i.intern("bar");
        assert!(t.resolve_method(bogus_sym, bar_sym).is_none());
    }

    #[test]
    fn generic_impl_marked_is_generic() {
        let src = r"
            struct Vec<T> { data : i64 }
            impl<T> Vec<T> {
                fn len(self : Vec<T>) -> i64 { 0 }
            }
        ";
        let (m, i, _) = lower(src);
        let t = build_trait_impl_table(&m, &i);
        assert_eq!(t.len(), 1);
        let entry = t.entries().next().unwrap();
        assert!(entry.is_generic);
    }

    #[test]
    fn impls_of_trait_iter_returns_all_self_types() {
        let src = r"
            interface Display { fn display(self : Foo) -> i32 ; }
            struct Foo { x : i32 }
            struct Bar { y : i32 }
            impl Display for Foo { fn display(self : Foo) -> i32 { self.x } }
            impl Display for Bar { fn display(self : Bar) -> i32 { self.y } }
        ";
        let (m, i, _) = lower(src);
        let t = build_trait_impl_table(&m, &i);
        let display = i.intern("Display");
        let names: Vec<String> = t
            .impls_of_trait(display)
            .map(|e| i.resolve(e.self_ty_name))
            .collect();
        assert_eq!(names.len(), 2);
        assert!(names.iter().any(|n| n == "Foo"));
        assert!(names.iter().any(|n| n == "Bar"));
    }

    #[test]
    fn interface_table_records_method_names() {
        let src = r"
            interface Display {
                fn display(self : Foo) -> i32 ;
                fn debug(self : Foo) -> i32 ;
            }
            struct Foo { x : i32 }
        ";
        let (m, i, _) = lower(src);
        let _t = build_trait_impl_table(&m, &i);
        let it = build_trait_interface_table(&m);
        let d = i.intern("Display");
        let display = i.intern("display");
        let debug = i.intern("debug");
        let nope = i.intern("nope");
        assert!(it.has_method(d, display));
        assert!(it.has_method(d, debug));
        assert!(!it.has_method(d, nope));
        assert_eq!(it.method_count(d), 2);
    }

    #[test]
    fn check_trait_bounds_passes_when_impl_exists() {
        use crate::monomorph::TypeSubst;
        let src = r"
            interface Display { fn display(self : Foo) -> i32 ; }
            struct Foo { x : i32 }
            impl Display for Foo { fn display(self : Foo) -> i32 { self.x } }
        ";
        let (m, i, _) = lower(src);
        let t = build_trait_impl_table(&m, &i);
        let t_param = i.intern("T");
        let display = i.intern("Display");
        let foo_ty = cssl_hir::HirType {
            span: cssl_ast::Span::new(SourceId::first(), 0, 1),
            id: cssl_hir::HirId::DUMMY,
            kind: cssl_hir::HirTypeKind::Path {
                path: vec![i.intern("Foo")],
                def: None,
                type_args: Vec::new(),
            },
        };
        let mut subst = TypeSubst::new();
        subst.bind(t_param, foo_ty);
        let bounds = vec![(t_param, vec![display])];
        let v = check_trait_bounds(&t, &bounds, &subst, &i);
        assert!(v.is_empty(), "expected no violations, got {v:?}");
    }

    #[test]
    fn check_trait_bounds_fails_when_impl_missing() {
        use crate::monomorph::TypeSubst;
        let src = r"
            interface Display { fn display(self : Foo) -> i32 ; }
            struct Foo { x : i32 }
            // No `impl Display for Foo` !
        ";
        let (m, i, _) = lower(src);
        let t = build_trait_impl_table(&m, &i);
        let t_param = i.intern("T");
        let display = i.intern("Display");
        let foo_ty = cssl_hir::HirType {
            span: cssl_ast::Span::new(SourceId::first(), 0, 1),
            id: cssl_hir::HirId::DUMMY,
            kind: cssl_hir::HirTypeKind::Path {
                path: vec![i.intern("Foo")],
                def: None,
                type_args: Vec::new(),
            },
        };
        let mut subst = TypeSubst::new();
        subst.bind(t_param, foo_ty);
        let bounds = vec![(t_param, vec![display])];
        let v = check_trait_bounds(&t, &bounds, &subst, &i);
        assert_eq!(v.len(), 1);
        assert!(v[0].diagnostic.contains("Display"));
        assert!(v[0].diagnostic.contains("Foo"));
    }

    #[test]
    fn mangle_concrete_method_name_inherent_shape() {
        let s = mangle_concrete_method_name("Box_i32", None, "value");
        assert_eq!(s, "Box_i32__value");
    }

    #[test]
    fn mangle_concrete_method_name_trait_shape() {
        let s = mangle_concrete_method_name("Vec_f32", Some("Drop"), "drop");
        assert_eq!(s, "Vec_f32__Drop__drop");
    }

    #[test]
    fn module_bound_check_reports_violation_for_unsatisfied_call_site() {
        // `fn map<T : Display>(x : T) -> i32 { 0 }`
        // Call : `map::<Foo>(some_foo)` where Foo has NO Display impl ⇒ violation.
        let src = r"
            interface Display { fn display(self : Foo) -> i32 ; }
            struct Foo { x : i32 }
            // No `impl Display for Foo` ⇒ map::<Foo> is unsatisfied.
            fn map<T : Display>(x : T) -> i32 { 0 }
            fn caller() -> i32 {
                let f : Foo = Foo { x : 1 };
                map::<Foo>(f)
            }
        ";
        let (m, i, _) = lower(src);
        let t = build_trait_impl_table(&m, &i);
        let violations = validate_trait_bounds_in_module(&m, &i, &t);
        assert!(!violations.is_empty(), "expected at least one violation");
        let v = &violations[0];
        assert_eq!(v.callee, "map");
        assert!(v.failures[0].diagnostic.contains("Display"));
        assert!(v.failures[0].diagnostic.contains("Foo"));
    }

    #[test]
    fn module_bound_check_passes_when_impl_exists() {
        let src = r"
            interface Display { fn display(self : Foo) -> i32 ; }
            struct Foo { x : i32 }
            impl Display for Foo { fn display(self : Foo) -> i32 { self.x } }
            fn map<T : Display>(x : T) -> i32 { 0 }
            fn caller() -> i32 {
                let f : Foo = Foo { x : 1 };
                map::<Foo>(f)
            }
        ";
        let (m, i, _) = lower(src);
        let t = build_trait_impl_table(&m, &i);
        let violations = validate_trait_bounds_in_module(&m, &i, &t);
        assert!(
            violations.is_empty(),
            "expected zero violations, got {violations:?}"
        );
    }

    #[test]
    fn module_bound_check_skips_unbounded_generics() {
        // `fn id<T>(x : T) -> T { x }` — no bounds, no violations.
        let src = r"
            fn id<T>(x : T) -> T { x }
            fn caller() -> i32 {
                id::<i32>(5)
            }
        ";
        let (m, i, _) = lower(src);
        let t = build_trait_impl_table(&m, &i);
        let violations = validate_trait_bounds_in_module(&m, &i, &t);
        assert!(violations.is_empty());
    }

    #[test]
    fn module_bound_check_recurses_into_nested_blocks() {
        // Bound violation deep inside an if-then arm.
        let src = r"
            interface Display { fn display(self : Foo) -> i32 ; }
            struct Foo { x : i32 }
            fn map<T : Display>(x : T) -> i32 { 0 }
            fn caller() -> i32 {
                if 1 == 1 {
                    let f : Foo = Foo { x : 1 };
                    map::<Foo>(f)
                } else {
                    0
                }
            }
        ";
        let (m, i, _) = lower(src);
        let t = build_trait_impl_table(&m, &i);
        let violations = validate_trait_bounds_in_module(&m, &i, &t);
        assert!(!violations.is_empty());
    }
}
