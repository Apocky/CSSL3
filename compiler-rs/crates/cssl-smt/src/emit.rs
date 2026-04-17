//! SMT-LIB 2.6 textual emission.

use crate::query::Query;

/// Render a complete SMT-LIB script from a `Query`.
#[must_use]
pub fn emit_smtlib(q: &Query) -> String {
    let mut out = String::new();
    if let Some(t) = q.theory {
        out.push_str(&format!("(set-logic {})\n", t.as_str()));
    }
    for sort in &q.sort_decls {
        out.push_str(&format!("(declare-sort {sort} 0)\n"));
    }
    for f in &q.fn_decls {
        out.push_str(&f.render());
        out.push('\n');
    }
    for a in &q.assertions {
        out.push_str(&a.render());
        out.push('\n');
    }
    out.push_str("(check-sat)\n");
    if q.get_model {
        out.push_str("(get-model)\n");
    }
    if q.get_unsat_core {
        out.push_str("(get-unsat-core)\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::emit_smtlib;
    use crate::query::{FnDecl, Query};
    use crate::term::{Sort, Term, Theory};

    #[test]
    fn emit_empty_query() {
        let q = Query::new();
        let s = emit_smtlib(&q);
        assert!(s.contains("(check-sat)"));
    }

    #[test]
    fn emit_with_theory() {
        let q = Query::new().with_theory(Theory::LIA);
        let s = emit_smtlib(&q);
        assert!(s.contains("(set-logic QF_LIA)"));
    }

    #[test]
    fn emit_with_declare_fn() {
        let mut q = Query::new();
        q.declare_fn(FnDecl::new("x", vec![], Sort::Int));
        q.assert(Term::app(">", vec![Term::var("x"), Term::int(0)]));
        let s = emit_smtlib(&q);
        assert!(s.contains("(declare-fun x () Int)"));
        assert!(s.contains("(assert (> x 0))"));
        assert!(s.contains("(check-sat)"));
    }

    #[test]
    fn emit_with_get_model() {
        let mut q = Query::new();
        q.get_model = true;
        let s = emit_smtlib(&q);
        assert!(s.contains("(check-sat)"));
        assert!(s.contains("(get-model)"));
    }

    #[test]
    fn emit_with_get_unsat_core() {
        let mut q = Query::new();
        q.get_unsat_core = true;
        let s = emit_smtlib(&q);
        assert!(s.contains("(get-unsat-core)"));
    }

    #[test]
    fn emit_declare_sort_renders() {
        let mut q = Query::new();
        q.declare_sort("Elem");
        let s = emit_smtlib(&q);
        assert!(s.contains("(declare-sort Elem 0)"));
    }

    #[test]
    fn emit_multi_assertion_order_preserved() {
        let mut q = Query::new();
        q.assert(Term::bool(true));
        q.assert(Term::bool(false));
        let s = emit_smtlib(&q);
        // First assert appears before second.
        let first = s.find("(assert true)").unwrap();
        let second = s.find("(assert false)").unwrap();
        assert!(first < second);
    }
}
