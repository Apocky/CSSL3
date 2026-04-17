//! Solver dispatch : Z3 / CVC5 CLI sub-process wrappers + obligation discharge.
//!
//! § STRATEGY (per T9-phase-1)
//!   Instead of linking `z3-sys` / `cvc5-sys` (both require C++ build + MSVC on
//!   Windows per T1-D7), we invoke the `z3` / `cvc5` CLI binaries as sub-processes
//!   and pipe the SMT-LIB query text through stdin. The verdict is parsed from
//!   the first line of stdout : `sat` / `unsat` / `unknown`.
//!
//!   If the binary is not present on PATH, the solver returns `Verdict::Error` ;
//!   CI is configured to install Z3 via `apt` / `brew` / `choco` at bootstrap.

use std::io::Write;
use std::process::{Command, Stdio};

use thiserror::Error;

use crate::emit::emit_smtlib;
use crate::query::{Query, Verdict};

/// Which solver to dispatch to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SolverKind {
    /// Z3 (Microsoft Research) — primary solver.
    Z3,
    /// CVC5 (Stanford / Iowa) — fallback + used for independent-proof mode.
    Cvc5,
}

impl SolverKind {
    /// Canonical binary-name (must be on PATH).
    #[must_use]
    pub const fn binary(self) -> &'static str {
        match self {
            Self::Z3 => "z3",
            Self::Cvc5 => "cvc5",
        }
    }
}

/// Solver-dispatch trait.
pub trait Solver {
    /// Kind of solver this is.
    fn kind(&self) -> SolverKind;
    /// Run the given `Query` and return a verdict.
    fn check(&self, q: &Query) -> Result<Verdict, SolverError>;
}

/// Failure modes for solver dispatch.
#[derive(Debug, Error)]
pub enum SolverError {
    /// Solver binary not found on PATH.
    #[error("solver binary `{binary}` not found on PATH")]
    BinaryMissing { binary: &'static str },
    /// Solver process exited with non-zero status and no verdict.
    #[error("solver `{binary}` exited with status {status}")]
    NonZeroExit { binary: &'static str, status: i32 },
    /// Solver stdout was empty or unparseable.
    #[error("solver `{binary}` produced unparseable output : {output}")]
    UnparseableOutput {
        binary: &'static str,
        output: String,
    },
    /// IO error during sub-process management.
    #[error("solver IO error : {0}")]
    Io(#[from] std::io::Error),
}

/// Z3 CLI adapter. Invokes `z3 -in` sub-process.
#[derive(Debug, Default, Clone)]
pub struct Z3CliSolver {
    /// Extra command-line args (e.g., `-smt2`, `-T:10`).
    pub extra_args: Vec<String>,
}

impl Solver for Z3CliSolver {
    fn kind(&self) -> SolverKind {
        SolverKind::Z3
    }

    fn check(&self, q: &Query) -> Result<Verdict, SolverError> {
        run_cli(SolverKind::Z3, q, &default_z3_args(&self.extra_args))
    }
}

/// CVC5 CLI adapter.
#[derive(Debug, Default, Clone)]
pub struct Cvc5CliSolver {
    pub extra_args: Vec<String>,
}

impl Solver for Cvc5CliSolver {
    fn kind(&self) -> SolverKind {
        SolverKind::Cvc5
    }

    fn check(&self, q: &Query) -> Result<Verdict, SolverError> {
        run_cli(SolverKind::Cvc5, q, &default_cvc5_args(&self.extra_args))
    }
}

fn default_z3_args(extra: &[String]) -> Vec<String> {
    let mut args = vec!["-in".to_string(), "-smt2".to_string()];
    args.extend(extra.iter().cloned());
    args
}

fn default_cvc5_args(extra: &[String]) -> Vec<String> {
    let mut args = vec!["--lang=smt2".to_string(), "-".to_string()];
    args.extend(extra.iter().cloned());
    args
}

fn run_cli(kind: SolverKind, q: &Query, args: &[String]) -> Result<Verdict, SolverError> {
    let binary = kind.binary();
    let smtlib = emit_smtlib(q);
    let mut child = match Command::new(binary)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(SolverError::BinaryMissing { binary });
        }
        Err(e) => return Err(SolverError::Io(e)),
    };
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(smtlib.as_bytes())?;
    }
    let output = child.wait_with_output()?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let first_line = stdout.lines().next().map_or("", str::trim).to_string();
    match first_line.as_str() {
        "sat" => Ok(Verdict::Sat),
        "unsat" => Ok(Verdict::Unsat),
        "unknown" => Ok(Verdict::Unknown),
        _ if !output.status.success() => Err(SolverError::NonZeroExit {
            binary,
            status: output.status.code().unwrap_or(-1),
        }),
        _ => Err(SolverError::UnparseableOutput {
            binary,
            output: first_line,
        }),
    }
}

/// Discharge a list of refinement obligations by emitting SMT queries + running a solver.
/// Each obligation produces one `(ObligationId, Verdict)` pair.
///
/// § STAGE-0 SHAPE
///   The stage-0 stub converts each obligation into a trivial query (just `(check-sat)`)
///   since the full HIR-expression → SMT-term translation is T9-phase-2 work. This
///   exercises the emit + solver-dispatch pipeline without yet producing meaningful
///   correctness checks — the integration point lands first, the semantics follow.
pub fn discharge<S: Solver>(
    obligations: &cssl_hir::ObligationBag,
    solver: &S,
) -> Vec<(cssl_hir::ObligationId, Result<Verdict, SolverError>)> {
    let mut out = Vec::new();
    for o in obligations.iter() {
        let q = build_stub_query(o);
        let verdict = solver.check(&q);
        out.push((o.id, verdict));
    }
    out
}

/// Stage-0 stub : every obligation becomes a trivially-true query.
fn build_stub_query(_o: &cssl_hir::RefinementObligation) -> Query {
    let mut q = Query::new().with_theory(crate::term::Theory::ALL);
    q.assert(crate::term::Term::bool(true));
    q
}

#[cfg(test)]
mod tests {
    use super::{SolverError, SolverKind, Z3CliSolver};
    use crate::query::Query;

    #[test]
    fn solver_kinds_have_binaries() {
        assert_eq!(SolverKind::Z3.binary(), "z3");
        assert_eq!(SolverKind::Cvc5.binary(), "cvc5");
    }

    #[test]
    fn z3_solver_default_extra_args_empty() {
        let s = Z3CliSolver::default();
        assert!(s.extra_args.is_empty());
    }

    #[test]
    fn default_args_include_in_flag_for_z3() {
        let args = super::default_z3_args(&[]);
        assert!(args.iter().any(|a| a == "-in"));
        assert!(args.iter().any(|a| a == "-smt2"));
    }

    #[test]
    fn default_args_include_lang_flag_for_cvc5() {
        let args = super::default_cvc5_args(&[]);
        assert!(args.iter().any(|a| a == "--lang=smt2"));
    }

    #[test]
    fn build_stub_query_is_trivially_true() {
        use crate::term::{Literal, Term};
        let interner = cssl_hir::Interner::new();
        let _ = interner.intern("pos");
        let o = cssl_hir::RefinementObligation {
            id: cssl_hir::ObligationId(0),
            origin: cssl_hir::HirId::DUMMY,
            span: cssl_ast::Span::DUMMY,
            enclosing_def: None,
            kind: cssl_hir::ObligationKind::Tag {
                name: interner.intern("pos"),
            },
            base_type_text: "f32".into(),
        };
        let q = super::build_stub_query(&o);
        // The stub asserts `true`.
        assert_eq!(q.assertions.len(), 1);
        assert!(matches!(
            q.assertions[0].term,
            Term::Lit(Literal::Bool(true))
        ));
        // Trivial predicate-satisfiability is `sat`, not `unsat` — this is just a shape
        // test, not a correctness test. Real obligation discharge is T9-phase-2.
        let _q: &Query = &q;
    }

    #[test]
    fn solver_error_display_shapes() {
        let e = SolverError::BinaryMissing { binary: "z3" };
        assert!(format!("{e}").contains("z3"));
        let e = SolverError::NonZeroExit {
            binary: "z3",
            status: 42,
        };
        assert!(format!("{e}").contains("42"));
    }

    // Note : we intentionally do NOT test actual solver invocation here — it would
    // require the `z3` binary on the test runner's PATH. CI has a separate job
    // that installs solvers ; unit tests exercise only the dispatch / emit path.
}
