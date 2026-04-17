//! CSSLv3 macros — Racket-lineage hygienic macros + proc-macro tier-3.
//!
//! § SPEC : `specs/13_MACROS.csl`.
//!
//! § TIERS (per `specs/13` § TIER-HIERARCHY)
//!   - Tier-1 : `@attr`-macros — compile-time annotations that transform items.
//!   - Tier-2 : declarative macros — pattern-directed syntactic rewrite.
//!   - Tier-3 : `#run` proc-macros — sandboxed comptime code with full stdlib access.
//!
//! § HYGIENE (Racket / Flatt et al. lineage)
//!   Every `SyntaxObject` carries a `HygieneMark` — a set of "scopes" under which
//!   a binding is in scope. Two identifiers compare equal iff their spelling AND
//!   hygiene-mark agree. This prevents "unhygienic" name capture where a macro-
//!   introduced binding accidentally shadows a user-binding.
//!
//! § SCOPE (T8-phase-1 / this commit)
//!   Data model + hygiene primitives. Actual expansion (tier-2 pattern-match + tier-3
//!   `#run` eval) lands in T8-phase-2 alongside the staging pass.

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]

use std::collections::BTreeSet;

use thiserror::Error;

/// Which tier a macro belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MacroTier {
    /// Tier-1 : `@attr`-macro.
    AttrMacro,
    /// Tier-2 : declarative (pattern-rewrite) macro.
    Declarative,
    /// Tier-3 : `#run` proc-macro.
    Procedural,
}

impl MacroTier {
    /// Canonical source-form label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::AttrMacro => "tier-1-attr",
            Self::Declarative => "tier-2-declarative",
            Self::Procedural => "tier-3-proc",
        }
    }

    /// All 3 tiers.
    pub const ALL: [Self; 3] = [Self::AttrMacro, Self::Declarative, Self::Procedural];
}

/// Opaque scope-identifier — a numeric label for a hygiene scope. Expanding a
/// macro introduces a fresh scope ; identifiers created by the expansion carry
/// that scope in their `HygieneMark`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct ScopeId(pub u32);

/// Set-of-scopes under which a syntax object's binding is considered in-scope.
///
/// Two identifiers with the same spelling compare equal iff their `HygieneMark`
/// sets agree. Flipping scopes (`flip`) implements the standard Racket hygiene
/// algorithm.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HygieneMark {
    scopes: BTreeSet<ScopeId>,
}

impl HygieneMark {
    /// Empty mark (no scopes) — for tokens freshly minted by the parser.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a scope to the mark.
    pub fn add(&mut self, s: ScopeId) {
        self.scopes.insert(s);
    }

    /// Remove a scope (idempotent).
    pub fn remove(&mut self, s: ScopeId) {
        self.scopes.remove(&s);
    }

    /// `true` iff `s` is in the mark.
    #[must_use]
    pub fn contains(&self, s: ScopeId) -> bool {
        self.scopes.contains(&s)
    }

    /// "Flip" a scope per Racket rules : if present, remove ; if absent, add.
    pub fn flip(&mut self, s: ScopeId) {
        if self.scopes.contains(&s) {
            self.scopes.remove(&s);
        } else {
            self.scopes.insert(s);
        }
    }

    /// Union of two marks.
    #[must_use]
    pub fn union(&self, other: &Self) -> Self {
        let mut out = self.clone();
        for s in &other.scopes {
            out.scopes.insert(*s);
        }
        out
    }

    /// Number of scopes in the mark.
    #[must_use]
    pub fn len(&self) -> usize {
        self.scopes.len()
    }

    /// `true` iff no scopes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.scopes.is_empty()
    }
}

/// Syntax object = source text + hygiene mark.
/// Identifiers compare equal iff text *and* mark agree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxObject {
    pub text: String,
    pub mark: HygieneMark,
}

impl SyntaxObject {
    /// Build a syntax object with an empty mark (parser-provided identifier).
    #[must_use]
    pub fn fresh(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            mark: HygieneMark::new(),
        }
    }

    /// Build a syntax object with a given mark.
    #[must_use]
    pub fn with_mark(text: impl Into<String>, mark: HygieneMark) -> Self {
        Self {
            text: text.into(),
            mark,
        }
    }

    /// Apply a scope-flip to the mark.
    pub fn flip_scope(&mut self, s: ScopeId) {
        self.mark.flip(s);
    }
}

/// Fresh-scope allocator.
#[derive(Debug, Default)]
pub struct ScopeAllocator {
    next: u32,
}

impl ScopeAllocator {
    /// Build an empty allocator.
    #[must_use]
    pub const fn new() -> Self {
        Self { next: 0 }
    }

    /// Allocate a fresh scope-id.
    pub fn fresh(&mut self) -> ScopeId {
        let id = ScopeId(self.next);
        self.next = self.next.saturating_add(1);
        id
    }

    /// Count of allocated scopes so far.
    #[must_use]
    pub const fn count(&self) -> u32 {
        self.next
    }
}

/// Declaration of a registered macro.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacroDecl {
    pub name: String,
    pub tier: MacroTier,
}

/// Registry of known macros (stage-0 : simple name → tier map).
#[derive(Debug, Default, Clone)]
pub struct MacroRegistry {
    macros: Vec<MacroDecl>,
}

impl MacroRegistry {
    /// Empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a macro.
    pub fn register(&mut self, decl: MacroDecl) {
        self.macros.push(decl);
    }

    /// Lookup by name.
    #[must_use]
    pub fn lookup(&self, name: &str) -> Option<&MacroDecl> {
        self.macros.iter().find(|m| m.name == name)
    }

    /// Number of registered macros.
    #[must_use]
    pub fn len(&self) -> usize {
        self.macros.len()
    }

    /// `true` iff empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.macros.is_empty()
    }
}

/// Failure modes during macro expansion.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum MacroError {
    /// Macro invoked but not registered.
    #[error("unknown macro : {name}")]
    UnknownMacro { name: String },
    /// Pattern-match failed in a tier-2 macro.
    #[error("macro pattern-match failed : {message}")]
    PatternMismatch { message: String },
    /// `#run` proc-macro escaped its sandbox.
    #[error("proc-macro sandbox violation : {op}")]
    SandboxViolation { op: String },
}

/// Crate version exposed for scaffold verification.
pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::{
        HygieneMark, MacroDecl, MacroRegistry, MacroTier, ScopeAllocator, ScopeId, SyntaxObject,
        STAGE0_SCAFFOLD,
    };

    #[test]
    fn scaffold_version_present() {
        assert!(!STAGE0_SCAFFOLD.is_empty());
    }

    #[test]
    fn three_tiers_enumerated() {
        assert_eq!(MacroTier::ALL.len(), 3);
    }

    #[test]
    fn tier_labels_unique() {
        let labels: Vec<&str> = MacroTier::ALL.iter().map(|t| t.label()).collect();
        let mut sorted = labels.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), labels.len());
    }

    #[test]
    fn hygiene_mark_add_and_contains() {
        let mut m = HygieneMark::new();
        assert!(m.is_empty());
        m.add(ScopeId(5));
        assert!(m.contains(ScopeId(5)));
        assert_eq!(m.len(), 1);
    }

    #[test]
    fn hygiene_flip_is_xor() {
        let mut m = HygieneMark::new();
        m.flip(ScopeId(3));
        assert!(m.contains(ScopeId(3)));
        m.flip(ScopeId(3));
        assert!(!m.contains(ScopeId(3)));
    }

    #[test]
    fn hygiene_union_merges() {
        let mut a = HygieneMark::new();
        a.add(ScopeId(1));
        let mut b = HygieneMark::new();
        b.add(ScopeId(2));
        let u = a.union(&b);
        assert!(u.contains(ScopeId(1)));
        assert!(u.contains(ScopeId(2)));
        assert_eq!(u.len(), 2);
    }

    #[test]
    fn syntax_object_equality_respects_mark() {
        let a = SyntaxObject::fresh("x");
        let mut b = SyntaxObject::fresh("x");
        b.flip_scope(ScopeId(1));
        assert_ne!(a, b, "marks differ ; not equal");
    }

    #[test]
    fn syntax_object_same_text_same_mark_equal() {
        let a = SyntaxObject::fresh("x");
        let b = SyntaxObject::fresh("x");
        assert_eq!(a, b);
    }

    #[test]
    fn scope_allocator_fresh_unique() {
        let mut a = ScopeAllocator::new();
        let s0 = a.fresh();
        let s1 = a.fresh();
        assert_ne!(s0, s1);
        assert_eq!(a.count(), 2);
    }

    #[test]
    fn macro_registry_roundtrip() {
        let mut r = MacroRegistry::new();
        assert!(r.is_empty());
        r.register(MacroDecl {
            name: "println".into(),
            tier: MacroTier::Declarative,
        });
        let m = r.lookup("println").unwrap();
        assert_eq!(m.tier, MacroTier::Declarative);
    }

    #[test]
    fn macro_registry_unknown_returns_none() {
        let r = MacroRegistry::new();
        assert!(r.lookup("missing").is_none());
    }
}
