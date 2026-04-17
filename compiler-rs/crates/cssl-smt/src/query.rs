//! SMT-LIB query : logic + declarations + assertions + check-sat.

use crate::term::{Sort, Term, Theory};

/// A function / constant declaration in the query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FnDecl {
    pub name: String,
    /// Sort of each parameter. Empty for constants.
    pub params: Vec<Sort>,
    /// Result sort.
    pub result: Sort,
}

impl FnDecl {
    /// Build a fn-declaration.
    #[must_use]
    pub fn new(name: impl Into<String>, params: Vec<Sort>, result: Sort) -> Self {
        Self {
            name: name.into(),
            params,
            result,
        }
    }

    /// Render as `(declare-fun ...)`.
    #[must_use]
    pub fn render(&self) -> String {
        let params_s = self
            .params
            .iter()
            .map(Sort::render)
            .collect::<Vec<_>>()
            .join(" ");
        format!(
            "(declare-fun {} ({}) {})",
            self.name,
            params_s,
            self.result.render()
        )
    }
}

/// One assertion (`(assert <term>)`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assertion {
    pub term: Term,
    /// Optional named-assertion label for unsat-core extraction.
    pub label: Option<String>,
}

impl Assertion {
    /// Unlabeled assertion.
    #[must_use]
    pub fn new(term: Term) -> Self {
        Self { term, label: None }
    }

    /// Labeled assertion.
    #[must_use]
    pub fn named(label: impl Into<String>, term: Term) -> Self {
        Self {
            term,
            label: Some(label.into()),
        }
    }

    /// Render as `(assert ...)` or `(assert (! ... :named label))`.
    #[must_use]
    pub fn render(&self) -> String {
        self.label.as_ref().map_or_else(
            || format!("(assert {})", self.term.render()),
            |label| format!("(assert (! {} :named {}))", self.term.render(), label),
        )
    }
}

/// A complete SMT-LIB query : logic + sort-decls + fn-decls + assertions + check-sat.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Query {
    /// Logic to use (default : `ALL`).
    pub theory: Option<Theory>,
    /// Declare-sort statements (for uninterpreted sorts).
    pub sort_decls: Vec<String>,
    /// Declare-fun statements.
    pub fn_decls: Vec<FnDecl>,
    /// Assertions in order.
    pub assertions: Vec<Assertion>,
    /// Whether to emit `(get-model)` after check-sat.
    pub get_model: bool,
    /// Whether to emit `(get-unsat-core)` after check-sat.
    pub get_unsat_core: bool,
}

impl Query {
    /// Empty query.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the theory.
    #[must_use]
    pub const fn with_theory(mut self, t: Theory) -> Self {
        self.theory = Some(t);
        self
    }

    /// Declare an uninterpreted sort (arity 0).
    pub fn declare_sort(&mut self, name: impl Into<String>) {
        self.sort_decls.push(name.into());
    }

    /// Declare a fn.
    pub fn declare_fn(&mut self, decl: FnDecl) {
        self.fn_decls.push(decl);
    }

    /// Append an assertion.
    pub fn assert(&mut self, term: Term) {
        self.assertions.push(Assertion::new(term));
    }

    /// Append a labeled assertion.
    pub fn assert_named(&mut self, label: impl Into<String>, term: Term) {
        self.assertions.push(Assertion::named(label, term));
    }

    /// `true` iff the query has no declarations or assertions.
    #[must_use]
    pub fn is_trivial(&self) -> bool {
        self.sort_decls.is_empty() && self.fn_decls.is_empty() && self.assertions.is_empty()
    }
}

/// Solver verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Verdict {
    /// `sat` — assertions are satisfiable ; the obligation is *violated* (we check
    /// unsatisfiability of negation).
    Sat,
    /// `unsat` — assertions are unsatisfiable ; the obligation holds.
    Unsat,
    /// `unknown` — solver gave up ; treat as violated for CI safety.
    Unknown,
    /// Solver process failed before reporting.
    Error,
}

#[cfg(test)]
mod tests {
    use super::{Assertion, FnDecl, Query, Verdict};
    use crate::term::{Sort, Term, Theory};

    #[test]
    fn fn_decl_rendering_zero_arity() {
        let d = FnDecl::new("x", vec![], Sort::Int);
        assert_eq!(d.render(), "(declare-fun x () Int)");
    }

    #[test]
    fn fn_decl_rendering_multi_arity() {
        let d = FnDecl::new("f", vec![Sort::Int, Sort::Bool], Sort::Real);
        assert_eq!(d.render(), "(declare-fun f (Int Bool) Real)");
    }

    #[test]
    fn assertion_unlabeled_renders() {
        let a = Assertion::new(Term::bool(true));
        assert_eq!(a.render(), "(assert true)");
    }

    #[test]
    fn assertion_labeled_renders() {
        let a = Assertion::named("lbl", Term::bool(false));
        assert_eq!(a.render(), "(assert (! false :named lbl))");
    }

    #[test]
    fn query_new_is_trivial() {
        let q = Query::new();
        assert!(q.is_trivial());
    }

    #[test]
    fn query_builder_chain() {
        let q = Query::new().with_theory(Theory::LIA);
        assert_eq!(q.theory, Some(Theory::LIA));
    }

    #[test]
    fn query_declare_and_assert() {
        let mut q = Query::new();
        q.declare_fn(FnDecl::new("x", vec![], Sort::Int));
        q.assert(Term::app(">", vec![Term::var("x"), Term::int(0)]));
        assert_eq!(q.fn_decls.len(), 1);
        assert_eq!(q.assertions.len(), 1);
        assert!(!q.is_trivial());
    }

    #[test]
    fn query_labeled_assertion() {
        let mut q = Query::new();
        q.assert_named("P1", Term::bool(true));
        assert_eq!(q.assertions[0].label.as_deref(), Some("P1"));
    }

    #[test]
    fn verdict_variants() {
        // Spot-check all four verdict variants exist + are distinct.
        assert_ne!(Verdict::Sat, Verdict::Unsat);
        assert_ne!(Verdict::Unknown, Verdict::Error);
    }

    #[test]
    fn query_declare_sort_tracked() {
        let mut q = Query::new();
        q.declare_sort("Elem");
        assert_eq!(q.sort_decls.len(), 1);
    }
}
