//! SMT-LIB theory + sort + term datatypes.

use core::fmt;

/// SMT-LIB logic name (corresponds to `(set-logic <name>)`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Theory {
    /// Linear Integer Arithmetic.
    LIA,
    /// Linear Real Arithmetic.
    LRA,
    /// Non-linear Real Arithmetic.
    NRA,
    /// Fixed-size Bit-Vectors.
    BV,
    /// Uninterpreted Functions.
    UF,
    /// UF + LIA combined.
    UFLIA,
    /// ALL — most permissive combination (Z3-specific).
    ALL,
}

impl Theory {
    /// SMT-LIB logic string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LIA => "QF_LIA",
            Self::LRA => "QF_LRA",
            Self::NRA => "QF_NRA",
            Self::BV => "QF_BV",
            Self::UF => "QF_UF",
            Self::UFLIA => "QF_UFLIA",
            Self::ALL => "ALL",
        }
    }

    /// All 7 theories.
    pub const ALL_THEORIES: [Self; 7] = [
        Self::LIA,
        Self::LRA,
        Self::NRA,
        Self::BV,
        Self::UF,
        Self::UFLIA,
        Self::ALL,
    ];
}

impl fmt::Display for Theory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// SMT-LIB sort (type in SMT-land).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Sort {
    /// `Bool`.
    Bool,
    /// `Int` (unbounded mathematical integer).
    Int,
    /// `Real` (unbounded mathematical real).
    Real,
    /// `(_ BitVec N)` — N-bit bitvector.
    BitVec(u32),
    /// Uninterpreted sort declared via `(declare-sort <name> 0)`.
    Uninterp(String),
}

impl Sort {
    /// SMT-LIB textual rendering.
    #[must_use]
    pub fn render(&self) -> String {
        match self {
            Self::Bool => "Bool".into(),
            Self::Int => "Int".into(),
            Self::Real => "Real".into(),
            Self::BitVec(n) => format!("(_ BitVec {n})"),
            Self::Uninterp(name) => name.clone(),
        }
    }
}

impl fmt::Display for Sort {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.render())
    }
}

/// Literal value in a term.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Literal {
    /// `true` / `false`.
    Bool(bool),
    /// Integer literal.
    Int(i64),
    /// Rational literal `num / den` rendered as `(/ num den)`.
    Rational { num: i64, den: u64 },
    /// Bitvector literal : `(_ bvN width)`.
    BitVec { value: u64, width: u32 },
}

impl Literal {
    /// SMT-LIB textual rendering.
    #[must_use]
    pub fn render(&self) -> String {
        match self {
            Self::Bool(b) => if *b { "true" } else { "false" }.into(),
            Self::Int(n) => n.to_string(),
            Self::Rational { num, den } => format!("(/ {num} {den})"),
            Self::BitVec { value, width } => format!("(_ bv{value} {width})"),
        }
    }
}

/// SMT-LIB term tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Term {
    /// Variable reference (by name). Must be previously declared in the query.
    Var(String),
    /// Literal constant.
    Lit(Literal),
    /// Function / operator application : `(f a b c)`.
    App { head: String, args: Vec<Term> },
    /// `(forall ((x Sort) ...) body)`.
    Forall {
        binders: Vec<(String, Sort)>,
        body: Box<Term>,
    },
    /// `(exists ((x Sort) ...) body)`.
    Exists {
        binders: Vec<(String, Sort)>,
        body: Box<Term>,
    },
    /// `(let ((x t1) ...) body)`.
    Let {
        bindings: Vec<(String, Term)>,
        body: Box<Term>,
    },
}

impl Term {
    /// Build a variable-reference term.
    #[must_use]
    pub fn var(name: impl Into<String>) -> Self {
        Self::Var(name.into())
    }

    /// Build an application term.
    #[must_use]
    pub fn app(head: impl Into<String>, args: Vec<Term>) -> Self {
        Self::App {
            head: head.into(),
            args,
        }
    }

    /// Convenient constructor : integer literal.
    #[must_use]
    pub const fn int(n: i64) -> Self {
        Self::Lit(Literal::Int(n))
    }

    /// Convenient constructor : boolean literal.
    #[must_use]
    pub const fn bool(b: bool) -> Self {
        Self::Lit(Literal::Bool(b))
    }

    /// Render the term as SMT-LIB textual syntax.
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = String::new();
        self.render_into(&mut out);
        out
    }

    fn render_into(&self, out: &mut String) {
        match self {
            Self::Var(name) => out.push_str(name),
            Self::Lit(l) => out.push_str(&l.render()),
            Self::App { head, args } => {
                out.push('(');
                out.push_str(head);
                for a in args {
                    out.push(' ');
                    a.render_into(out);
                }
                out.push(')');
            }
            Self::Forall { binders, body } => {
                out.push_str("(forall (");
                for (i, (name, sort)) in binders.iter().enumerate() {
                    if i > 0 {
                        out.push(' ');
                    }
                    out.push('(');
                    out.push_str(name);
                    out.push(' ');
                    out.push_str(&sort.render());
                    out.push(')');
                }
                out.push_str(") ");
                body.render_into(out);
                out.push(')');
            }
            Self::Exists { binders, body } => {
                out.push_str("(exists (");
                for (i, (name, sort)) in binders.iter().enumerate() {
                    if i > 0 {
                        out.push(' ');
                    }
                    out.push('(');
                    out.push_str(name);
                    out.push(' ');
                    out.push_str(&sort.render());
                    out.push(')');
                }
                out.push_str(") ");
                body.render_into(out);
                out.push(')');
            }
            Self::Let { bindings, body } => {
                out.push_str("(let (");
                for (i, (name, term)) in bindings.iter().enumerate() {
                    if i > 0 {
                        out.push(' ');
                    }
                    out.push('(');
                    out.push_str(name);
                    out.push(' ');
                    term.render_into(out);
                    out.push(')');
                }
                out.push_str(") ");
                body.render_into(out);
                out.push(')');
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Literal, Sort, Term, Theory};

    #[test]
    fn theory_names() {
        assert_eq!(Theory::LIA.as_str(), "QF_LIA");
        assert_eq!(Theory::ALL.as_str(), "ALL");
    }

    #[test]
    fn seven_theories() {
        assert_eq!(Theory::ALL_THEORIES.len(), 7);
    }

    #[test]
    fn sort_rendering() {
        assert_eq!(Sort::Bool.render(), "Bool");
        assert_eq!(Sort::Int.render(), "Int");
        assert_eq!(Sort::Real.render(), "Real");
        assert_eq!(Sort::BitVec(32).render(), "(_ BitVec 32)");
        assert_eq!(Sort::Uninterp("Elem".into()).render(), "Elem");
    }

    #[test]
    fn literal_rendering() {
        assert_eq!(Literal::Bool(true).render(), "true");
        assert_eq!(Literal::Bool(false).render(), "false");
        assert_eq!(Literal::Int(42).render(), "42");
        assert_eq!(Literal::Int(-7).render(), "-7");
        assert_eq!(Literal::Rational { num: 1, den: 3 }.render(), "(/ 1 3)");
        assert_eq!(Literal::BitVec { value: 5, width: 8 }.render(), "(_ bv5 8)");
    }

    #[test]
    fn term_var_and_lit() {
        assert_eq!(Term::var("x").render(), "x");
        assert_eq!(Term::int(42).render(), "42");
        assert_eq!(Term::bool(true).render(), "true");
    }

    #[test]
    fn term_app_rendering() {
        let t = Term::app("+", vec![Term::int(1), Term::int(2)]);
        assert_eq!(t.render(), "(+ 1 2)");
    }

    #[test]
    fn term_nested_app() {
        let t = Term::app(
            ">",
            vec![
                Term::app("+", vec![Term::var("x"), Term::int(1)]),
                Term::int(0),
            ],
        );
        assert_eq!(t.render(), "(> (+ x 1) 0)");
    }

    #[test]
    fn term_forall_rendering() {
        let t = Term::Forall {
            binders: vec![("x".into(), Sort::Int)],
            body: Box::new(Term::app(">", vec![Term::var("x"), Term::int(0)])),
        };
        assert_eq!(t.render(), "(forall ((x Int)) (> x 0))");
    }

    #[test]
    fn term_exists_rendering() {
        let t = Term::Exists {
            binders: vec![("y".into(), Sort::Bool)],
            body: Box::new(Term::var("y")),
        };
        assert_eq!(t.render(), "(exists ((y Bool)) y)");
    }

    #[test]
    fn term_let_rendering() {
        let t = Term::Let {
            bindings: vec![("z".into(), Term::int(5))],
            body: Box::new(Term::var("z")),
        };
        assert_eq!(t.render(), "(let ((z 5)) z)");
    }

    #[test]
    fn multi_binder_forall() {
        let t = Term::Forall {
            binders: vec![("x".into(), Sort::Int), ("y".into(), Sort::Int)],
            body: Box::new(Term::app(">=", vec![Term::var("x"), Term::var("y")])),
        };
        assert_eq!(t.render(), "(forall ((x Int) (y Int)) (>= x y))");
    }
}
