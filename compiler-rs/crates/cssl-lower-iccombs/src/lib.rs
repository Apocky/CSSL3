#![forbid(unsafe_code)]
#![doc = "cssl-lower-iccombs — Lafont/Mackie symmetric-interaction-combinator encoding\n\
of the LINEAR fragment of cssl-elab's surface λ-calculus.\n\n\
Encoding (λ_lin → SIC) :\n\
  • Lam(x, body)   ↦ Con node ; principal=output ; aux1=variable-port (env[x]) ;\n\
                       aux2 ← linked to body's output\n\
  • App(f, arg)    ↦ Con node ; principal ← linked to f's output ;\n\
                       aux1 ← linked to arg's output ; aux2=result port (returned)\n\
  • Var(x)         ↦ env[x] (the binder's aux1 port) — exactly-once linear use\n\
  • Unit           ↦ Era node ; return its principal port\n\
  • Let(x,v,b)     ↦ desugars to App(Lam(x, b), v)\n\n\
Out of scope (returns `LowerError::UnsupportedGrade` / `UnsupportedTerm`) :\n\
  • Affine / Unrestricted binders → would require Era for unused, Dup-tree for\n\
    multi-use ; deferred to a follow-up pass that performs erasure-Era / share-Dup\n\
    insertion based on use-counts.\n\
  • `Op(_)` effectful primitives → no SIC encoding ; effects must be discharged\n\
    via cssl-pd-check at HIR-level before lowering, or thunked into a runtime-call\n\
    combinator (deferred).\n\n\
Spec : `specs/Upgrade/impl/IMPL_01_PLAN.csl` § Wave U-D ; `IMPL_02_FOUNDATION.csl`\n\
§ cssl-iccombs."]

use cssl_elab::{Grade, Term};
use cssl_iccombs::{AgentKind, Net, PortRef};
use std::collections::HashMap;

/// Lowering error.
#[derive(Debug, thiserror::Error)]
pub enum LowerError {
    /// Variable referenced but not in scope (would be caught by elab too).
    #[error("unbound variable `{0}`")]
    UnboundVar(String),
    /// Linear fragment only ; affine / unrestricted require Dup/Era insertion.
    #[error("unsupported grade {0:?} (linear fragment only ; Affine/Unrestricted deferred)")]
    UnsupportedGrade(Grade),
    /// Effectful primitive ; must be discharged before lowering or wrapped in a runtime combinator.
    #[error("unsupported term : `Op({0})` is effectful and has no pure-SIC encoding")]
    UnsupportedTerm(String),
}

/// Result of lowering : the net + the principal output port of the term.
#[derive(Debug)]
pub struct Lowered {
    pub net: Net,
    pub root: PortRef,
}

/// Lower a `Term` to an interaction net (linear fragment only).
///
/// The returned `root` port is the term's "output" — typically the lambda's
/// principal port or the application's result-port.
///
/// # Errors
///
/// Returns `LowerError::UnsupportedGrade` for non-Linear binders, or
/// `LowerError::UnsupportedTerm` for `Op(_)` primitives.
pub fn lower(term: &Term) -> Result<Lowered, LowerError> {
    let mut net = Net::new();
    let mut env: HashMap<String, PortRef> = HashMap::new();
    let root = lower_inner(&mut net, &mut env, term)?;
    Ok(Lowered { net, root })
}

fn lower_inner(
    net: &mut Net,
    env: &mut HashMap<String, PortRef>,
    term: &Term,
) -> Result<PortRef, LowerError> {
    match term {
        Term::Unit => {
            let era = net.add_agent(AgentKind::Era);
            Ok(PortRef::Port(era, 0))
        }
        Term::Var(name) => env
            .get(name)
            .copied()
            .ok_or_else(|| LowerError::UnboundVar(name.clone())),
        Term::Lam { param, grade, body } => {
            if *grade != Grade::Linear {
                return Err(LowerError::UnsupportedGrade(*grade));
            }
            let con = net.add_agent(AgentKind::Con);
            let var_port = PortRef::Port(con, 1);
            let principal = PortRef::Port(con, 0);
            let body_port_2 = PortRef::Port(con, 2);
            let prev = env.insert(param.clone(), var_port);
            let body_out = lower_inner(net, env, body)?;
            // Restore shadowed binding (or remove ours).
            match prev {
                Some(p) => { env.insert(param.clone(), p); }
                None    => { env.remove(param); }
            }
            net.link(body_out, body_port_2);
            Ok(principal)
        }
        Term::App(f, x) => {
            let con = net.add_agent(AgentKind::Con);
            let principal = PortRef::Port(con, 0);
            let arg_port = PortRef::Port(con, 1);
            let result   = PortRef::Port(con, 2);
            let f_out = lower_inner(net, env, f)?;
            net.link(f_out, principal);
            let x_out = lower_inner(net, env, x)?;
            net.link(x_out, arg_port);
            Ok(result)
        }
        Term::Let { name, grade, value, body } => {
            // Desugar : (let x g v b) ≡ (app (lam x g b) v)
            let lam = Term::Lam {
                param: name.clone(),
                grade: *grade,
                body: body.clone(),
            };
            let desugared = Term::App(Box::new(lam), value.clone());
            lower_inner(net, env, &desugared)
        }
        Term::Op(label) => Err(LowerError::UnsupportedTerm(label.clone())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cssl_iccombs::ReduceResult;

    fn lin_lam(param: &str, body: Term) -> Term {
        Term::Lam { param: param.into(), grade: Grade::Linear, body: Box::new(body) }
    }

    #[test]
    fn lower_unit_yields_one_era() {
        let l = lower(&Term::Unit).unwrap();
        assert_eq!(l.net.agent_count(), 1);
        assert!(matches!(l.root, PortRef::Port(_, 0)));
    }

    #[test]
    fn lower_linear_identity() {
        // λx.x : a single Con whose aux1 = body output (= var-port itself) linked to aux2.
        let id = lin_lam("x", Term::Var("x".into()));
        let l = lower(&id).unwrap();
        assert_eq!(l.net.agent_count(), 1);
        assert!(matches!(l.root, PortRef::Port(_, 0)));
    }

    #[test]
    fn lower_application_creates_two_cons() {
        // ((λx.x) ()) : 1 Con (lambda) + 1 Con (app) + 1 Era (unit) = 3 agents
        let id = lin_lam("x", Term::Var("x".into()));
        let app = Term::App(Box::new(id), Box::new(Term::Unit));
        let l = lower(&app).unwrap();
        assert_eq!(l.net.agent_count(), 3);
    }

    #[test]
    fn lower_identity_applied_to_unit_reduces_to_normal_form() {
        // ((λx.x) ()) : the Con-Con annihilation between λ and app fires ; then Con-Era erasure ;
        // any further reductions until normal form. The exact final form depends on how we
        // expose the result — here we just verify it terminates.
        let id = lin_lam("x", Term::Var("x".into()));
        let app = Term::App(Box::new(id), Box::new(Term::Unit));
        let mut l = lower(&app).unwrap();
        let r = l.net.reduce_to_normal_form(64);
        assert_eq!(r, ReduceResult::NormalForm);
    }

    #[test]
    fn lower_let_desugars_to_application() {
        // (let x L () x)  ≡  ((λx.x) ())  → 3 agents
        let t = Term::Let {
            name: "x".into(),
            grade: Grade::Linear,
            value: Box::new(Term::Unit),
            body: Box::new(Term::Var("x".into())),
        };
        let l = lower(&t).unwrap();
        assert_eq!(l.net.agent_count(), 3);
    }

    #[test]
    fn lower_rejects_affine_binder() {
        let t = Term::Lam {
            param: "x".into(),
            grade: Grade::Affine,
            body: Box::new(Term::Unit),
        };
        let err = lower(&t).unwrap_err();
        assert!(matches!(err, LowerError::UnsupportedGrade(Grade::Affine)));
    }

    #[test]
    fn lower_rejects_unrestricted_binder() {
        let t = Term::Lam {
            param: "x".into(),
            grade: Grade::Unrestricted,
            body: Box::new(Term::Unit),
        };
        let err = lower(&t).unwrap_err();
        assert!(matches!(err, LowerError::UnsupportedGrade(Grade::Unrestricted)));
    }

    #[test]
    fn lower_rejects_op_primitive() {
        let err = lower(&Term::Op("io".into())).unwrap_err();
        assert!(matches!(err, LowerError::UnsupportedTerm(_)));
    }

    #[test]
    fn lower_rejects_unbound_var() {
        let err = lower(&Term::Var("oops".into())).unwrap_err();
        assert!(matches!(err, LowerError::UnboundVar(_)));
    }

    #[test]
    fn lower_nested_linear_lambdas() {
        // λx.λy.x  is NOT linear (y is unused) — but for THIS test we want a use of both,
        // so : λx.λy.(y x)  ≈  swap. Both vars used exactly once.
        let inner = Term::App(
            Box::new(Term::Var("y".into())),
            Box::new(Term::Var("x".into())),
        );
        let lam_y = lin_lam("y", inner);
        let lam_x = lin_lam("x", lam_y);
        let l = lower(&lam_x).unwrap();
        // 2 lambdas + 1 application = 3 Con agents, no Eras
        assert_eq!(l.net.agent_count(), 3);
    }
}
