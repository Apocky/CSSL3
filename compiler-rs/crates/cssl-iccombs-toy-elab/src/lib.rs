#![forbid(unsafe_code)]
#![doc = "cssl-iccombs-toy-elab — TOY graded-modal elaborator (iccombs harness).\n\n\
⚠ TOY per `specs/Upgrade/impl/IMPL_06_CORRIGENDUM.csl`. NOT a production elaborator.\n\
The production elaborator is `cssl-hir` (CST→typed-resolved-inferred HIR + integrates\n\
cssl-caps + cssl-effects + cssl-ifc). Wave U-B revised : extend cssl-hir with grades,\n\
do NOT keep this crate as the elab path. This crate exists only as the L1 input to\n\
the iccombs demo-harness (cssl-lower-iccombs + cssl-iccombs-toy-cli)."]

use cssl_cas::{cid_of_bytes, Cid};
use cssl_hgraph::{EdgeKind, HGraph, NodeId, NodeLabel, Port, TypeCid};
use std::collections::HashMap;
use thiserror::Error;

/// Toy effect-row : ordered set of effect labels accumulated during elaboration.
///
/// ⚠ TOY per IMPL_06_CORRIGENDUM. Real effect-rows live in `cssl-effects`
/// (28 built-in effects + Ω-substrate-rows + sub_effect_check + banned_composition).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EffectRow {
    pub labels: std::collections::BTreeSet<String>,
}

impl EffectRow {
    #[must_use]
    pub fn empty() -> Self { Self::default() }
    #[must_use]
    pub fn singleton(label: String) -> Self {
        let mut s = std::collections::BTreeSet::new();
        s.insert(label);
        Self { labels: s }
    }
    #[must_use]
    pub fn union(mut self, other: Self) -> Self {
        self.labels.extend(other.labels);
        self
    }
    #[must_use]
    pub fn is_pure(&self) -> bool { self.labels.is_empty() }
    #[must_use]
    pub fn contains(&self, label: &str) -> bool { self.labels.contains(label) }
}

/// Grade annotation on a binder.
///
/// Maps onto `cssl-grades::Linear` / `Affine` / `Unrestricted` semirings ; this
/// enum is the surface-syntax form. Mapping happens during elaboration when
/// usage counts are checked.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Grade {
    Linear,
    Affine,
    Unrestricted,
}

/// Surface-syntax term.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Term {
    /// Variable reference.
    Var(String),
    /// λ-abstraction with a graded parameter.
    Lam {
        param: String,
        grade: Grade,
        body: Box<Term>,
    },
    /// Function application.
    App(Box<Term>, Box<Term>),
    /// Graded let-binding.
    Let {
        name: String,
        grade: Grade,
        value: Box<Term>,
        body: Box<Term>,
    },
    /// Perform an effect operation labelled `label`.
    Op(String),
    /// Pure unit (no effects, no captures).
    Unit,
}

/// Errors raised during elaboration.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ElabError {
    /// Variable referenced but not in scope.
    #[error("unbound variable `{0}`")]
    UnboundVar(String),
    /// Linearity discipline violated (e.g. linear var used twice or zero times).
    #[error("linearity violation : `{var}` graded {grade:?} but used {used} time(s)")]
    LinearityViolation {
        var: String,
        grade: Grade,
        used: u32,
    },
}

/// Result of elaborating a `Term`.
#[derive(Clone, Debug)]
pub struct Elaborated {
    /// Hypergraph encoding of the elaborated term.
    pub hgraph: HGraph,
    /// `NodeId` of the root expression node within `hgraph`.
    pub root: NodeId,
    /// Effect-row accumulated over the term's effect operations.
    pub effects: EffectRow,
}

impl Elaborated {
    /// Content-Cid of the elaborated hypergraph.
    #[must_use]
    pub fn cid(&self) -> Cid {
        self.hgraph.cid()
    }
}

/// Elaborate a closed surface term.
pub fn elaborate(term: &Term) -> Result<Elaborated, ElabError> {
    let mut g = HGraph::new();
    let mut env = Env::default();
    let (root, effects) = elab_inner(term, &mut g, &mut env)?;
    Ok(Elaborated { hgraph: g, root, effects })
}

#[derive(Default)]
struct Env {
    /// Stack of scopes ; innermost last.
    scopes: Vec<Scope>,
}

struct Scope {
    name: String,
    grade: Grade,
    binder_node: NodeId,
    uses: u32,
}

impl Env {
    fn push(&mut self, name: String, grade: Grade, binder_node: NodeId) {
        self.scopes.push(Scope { name, grade, binder_node, uses: 0 });
    }

    fn pop_and_check(&mut self) -> Result<(), ElabError> {
        let s = self.scopes.pop().expect("scope-stack underflow : elaborator bug");
        check_grade(&s.name, s.grade, s.uses)
    }

    /// Look up a variable, incrementing its use count. Returns the binder's hgraph node.
    fn lookup(&mut self, name: &str) -> Option<NodeId> {
        for scope in self.scopes.iter_mut().rev() {
            if scope.name == name {
                scope.uses = scope.uses.saturating_add(1);
                return Some(scope.binder_node);
            }
        }
        None
    }
}

fn check_grade(name: &str, grade: Grade, uses: u32) -> Result<(), ElabError> {
    let ok = match grade {
        Grade::Linear => uses == 1,
        Grade::Affine => uses <= 1,
        Grade::Unrestricted => true,
    };
    if ok {
        Ok(())
    } else {
        Err(ElabError::LinearityViolation { var: name.into(), grade, used: uses })
    }
}

/// Single placeholder type-cid until Wave U-B (type elaboration) lands.
fn placeholder_type() -> TypeCid {
    TypeCid(cid_of_bytes(b"cssl-elab::U-A::placeholder-type"))
}

fn elab_inner(
    term: &Term,
    g: &mut HGraph,
    env: &mut Env,
) -> Result<(NodeId, EffectRow), ElabError> {
    let ty = placeholder_type();
    match term {
        Term::Unit => {
            let n = g.add_node(ty, NodeLabel::Leaf("()".into()));
            Ok((n, EffectRow::empty()))
        }
        Term::Var(name) => {
            let binder = env
                .lookup(name)
                .ok_or_else(|| ElabError::UnboundVar(name.clone()))?;
            let n = g.add_node(ty, NodeLabel::Leaf(name.clone()));
            // Edge from var-occurrence to its binder.
            g.add_edge(
                EdgeKind::DataFlow,
                &[
                    Port { node: n, idx: 0, type_cid: ty },
                    Port { node: binder, idx: 0, type_cid: ty },
                ],
            )
            .expect("ports refer to just-added nodes");
            Ok((n, EffectRow::empty()))
        }
        Term::Lam { param, grade, body } => {
            let lam_node = g.add_node(ty, NodeLabel::Lam);
            // Binder pseudo-node : a leaf labeled with the bound name + its grade.
            let binder_label = format!("bind:{param}:{}", grade_tag(*grade));
            let binder = g.add_node(ty, NodeLabel::Leaf(binder_label));
            g.add_edge(
                EdgeKind::Subterm,
                &[
                    Port { node: lam_node, idx: 0, type_cid: ty },
                    Port { node: binder, idx: 0, type_cid: ty },
                ],
            )
            .expect("ports valid");
            env.push(param.clone(), *grade, binder);
            let (body_node, body_effects) = elab_inner(body, g, env)?;
            env.pop_and_check()?;
            g.add_edge(
                EdgeKind::Subterm,
                &[
                    Port { node: lam_node, idx: 0, type_cid: ty },
                    Port { node: body_node, idx: 0, type_cid: ty },
                ],
            )
            .expect("ports valid");
            // λ suspends effects : the row of the lambda itself is empty ;
            // the latent effects live on the body and surface when applied.
            // For Wave U-A we conservatively keep the body-effects on the lambda
            // node so callers see them ; refinement (effect-suspension) is U-B.
            Ok((lam_node, body_effects))
        }
        Term::App(f, x) => {
            let (fn_node, fn_eff) = elab_inner(f, g, env)?;
            let (arg_node, arg_eff) = elab_inner(x, g, env)?;
            let app_node = g.add_node(ty, NodeLabel::App);
            g.add_edge(
                EdgeKind::Subterm,
                &[
                    Port { node: app_node, idx: 0, type_cid: ty },
                    Port { node: fn_node, idx: 0, type_cid: ty },
                    Port { node: arg_node, idx: 0, type_cid: ty },
                ],
            )
            .expect("ports valid");
            Ok((app_node, fn_eff.union(arg_eff)))
        }
        Term::Let { name, grade, value, body } => {
            let (val_node, val_eff) = elab_inner(value, g, env)?;
            let binder_label = format!("let:{name}:{}", grade_tag(*grade));
            let binder = g.add_node(ty, NodeLabel::Leaf(binder_label));
            g.add_edge(
                EdgeKind::DataFlow,
                &[
                    Port { node: binder, idx: 0, type_cid: ty },
                    Port { node: val_node, idx: 0, type_cid: ty },
                ],
            )
            .expect("ports valid");
            env.push(name.clone(), *grade, binder);
            let (body_node, body_eff) = elab_inner(body, g, env)?;
            env.pop_and_check()?;
            Ok((body_node, val_eff.union(body_eff)))
        }
        Term::Op(label) => {
            let n = g.add_node(ty, NodeLabel::Custom(format!("op:{label}")));
            let row = EffectRow::singleton(label.clone());
            // Annotate the op-node with its effect label as a hyperedge so the
            // hgraph itself is self-describing for downstream passes.
            g.add_edge(
                EdgeKind::EffectScope,
                &[Port { node: n, idx: 0, type_cid: ty }],
            )
            .expect("ports valid");
            Ok((n, row))
        }
    }
}

fn grade_tag(g: Grade) -> &'static str {
    match g {
        Grade::Linear => "L",
        Grade::Affine => "A",
        Grade::Unrestricted => "ω",
    }
}

/// Convenience : count distinct effect-labels collected during elaboration of `t`.
///
/// For tooling / quick spot-checks. Production code should pattern-match
/// `Elaborated::effects` directly.
#[must_use]
pub fn effect_label_count(t: &Term) -> usize {
    elaborate(t)
        .map(|e| e.effects.labels.len())
        .unwrap_or(0)
}

/// Helper : count occurrences of a free variable inside a term (approximate ;
/// does not respect shadowing — used for testing only).
#[doc(hidden)]
pub fn approx_free_uses(name: &str, t: &Term) -> u32 {
    fn go(name: &str, t: &Term, shadowed: bool) -> u32 {
        if shadowed {
            return 0;
        }
        match t {
            Term::Var(n) if n == name => 1,
            Term::Var(_) | Term::Op(_) | Term::Unit => 0,
            Term::Lam { param, body, .. } => go(name, body, param == name),
            Term::App(f, x) => go(name, f, false) + go(name, x, false),
            Term::Let { name: n, value, body, .. } => {
                go(name, value, false) + go(name, body, n == name)
            }
        }
    }
    go(name, t, false)
}

/// Sanity helper : check whether `name` is in the elaborator's view of free vars
/// (returns the binders-stack key set as a snapshot).
#[doc(hidden)]
#[must_use]
pub fn free_var_names(t: &Term) -> Vec<String> {
    let mut out = Vec::new();
    let mut bound = HashMap::<String, u32>::new();
    fn go(t: &Term, bound: &mut HashMap<String, u32>, out: &mut Vec<String>) {
        match t {
            Term::Var(n) => {
                if !bound.contains_key(n) && !out.contains(n) {
                    out.push(n.clone());
                }
            }
            Term::Lam { param, body, .. } => {
                *bound.entry(param.clone()).or_insert(0) += 1;
                go(body, bound, out);
                let e = bound.get_mut(param).expect("present");
                *e -= 1;
                if *e == 0 { bound.remove(param); }
            }
            Term::App(f, x) => { go(f, bound, out); go(x, bound, out); }
            Term::Let { name, value, body, .. } => {
                go(value, bound, out);
                *bound.entry(name.clone()).or_insert(0) += 1;
                go(body, bound, out);
                let e = bound.get_mut(name).expect("present");
                *e -= 1;
                if *e == 0 { bound.remove(name); }
            }
            Term::Op(_) | Term::Unit => {}
        }
    }
    go(t, &mut bound, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn var(s: &str) -> Term { Term::Var(s.into()) }
    fn lam(p: &str, g: Grade, b: Term) -> Term {
        Term::Lam { param: p.into(), grade: g, body: Box::new(b) }
    }
    fn app(f: Term, x: Term) -> Term { Term::App(Box::new(f), Box::new(x)) }
    fn let_(n: &str, g: Grade, v: Term, b: Term) -> Term {
        Term::Let { name: n.into(), grade: g, value: Box::new(v), body: Box::new(b) }
    }
    fn op(l: &str) -> Term { Term::Op(l.into()) }

    #[test]
    fn pure_unit_elaborates_to_empty_effect_row() {
        let e = elaborate(&Term::Unit).unwrap();
        assert!(e.effects.is_pure());
        assert_eq!(e.hgraph.node_count(), 1);
    }

    #[test]
    fn linear_var_used_exactly_once_succeeds() {
        // λx:L. x
        let t = lam("x", Grade::Linear, var("x"));
        elaborate(&t).expect("linear-once must elaborate");
    }

    #[test]
    fn linear_var_used_twice_fails() {
        // λx:L. (x x)
        let t = lam("x", Grade::Linear, app(var("x"), var("x")));
        let err = elaborate(&t).unwrap_err();
        assert!(matches!(err, ElabError::LinearityViolation { used: 2, .. }));
    }

    #[test]
    fn linear_var_unused_fails() {
        // λx:L. ()
        let t = lam("x", Grade::Linear, Term::Unit);
        let err = elaborate(&t).unwrap_err();
        assert!(matches!(err, ElabError::LinearityViolation { used: 0, .. }));
    }

    #[test]
    fn affine_var_unused_succeeds() {
        // λx:A. ()
        let t = lam("x", Grade::Affine, Term::Unit);
        elaborate(&t).expect("affine-zero must elaborate");
    }

    #[test]
    fn affine_var_used_twice_fails() {
        // λx:A. (x x)
        let t = lam("x", Grade::Affine, app(var("x"), var("x")));
        let err = elaborate(&t).unwrap_err();
        assert!(matches!(err, ElabError::LinearityViolation { used: 2, .. }));
    }

    #[test]
    fn unrestricted_var_used_any_number_of_times() {
        // λx:ω. (x x)
        let t = lam("x", Grade::Unrestricted, app(var("x"), var("x")));
        elaborate(&t).expect("unrestricted permits any count");
    }

    #[test]
    fn unbound_var_fails() {
        let err = elaborate(&var("x")).unwrap_err();
        assert_eq!(err, ElabError::UnboundVar("x".into()));
    }

    #[test]
    fn op_emits_effect_label_in_row() {
        let t = op("io");
        let e = elaborate(&t).unwrap();
        assert!(e.effects.contains("io"));
        assert!(!e.effects.is_pure());
    }

    #[test]
    fn app_unions_effects_of_subterms() {
        // (op io) (op state) — applies one effect-op to another.
        let t = app(op("io"), op("state"));
        let e = elaborate(&t).unwrap();
        assert!(e.effects.contains("io"));
        assert!(e.effects.contains("state"));
    }

    #[test]
    fn let_unions_value_and_body_effects() {
        // let x:ω = op io in op state
        let t = let_("x", Grade::Unrestricted, op("io"), op("state"));
        let e = elaborate(&t).unwrap();
        assert!(e.effects.contains("io"));
        assert!(e.effects.contains("state"));
    }

    #[test]
    fn elaborated_cid_is_deterministic_across_runs() {
        let t = lam("x", Grade::Linear, var("x"));
        let e1 = elaborate(&t).unwrap();
        let e2 = elaborate(&t).unwrap();
        assert_eq!(e1.cid(), e2.cid());
    }

    #[test]
    fn shadowing_does_not_leak_uses_to_outer_binder() {
        // λx:L. (λx:L. x) x
        let inner = lam("x", Grade::Linear, var("x"));
        let t = lam("x", Grade::Linear, app(inner, var("x")));
        // Outer x is used once (in `x` after the inner lam) ;
        // inner x is used once (in its body). Both linear-once = OK.
        elaborate(&t).expect("nested same-named linear binders elaborate");
    }

    #[test]
    fn nested_unbound_inside_lambda_is_reported() {
        // λx:ω. y
        let t = lam("x", Grade::Unrestricted, var("y"));
        let err = elaborate(&t).unwrap_err();
        assert_eq!(err, ElabError::UnboundVar("y".into()));
    }

    #[test]
    fn approx_free_uses_helper_counts_correctly() {
        // (x x) — x free, used twice
        let t = app(var("x"), var("x"));
        assert_eq!(approx_free_uses("x", &t), 2);
        // λx. x — x bound, zero free uses
        let t = lam("x", Grade::Unrestricted, var("x"));
        assert_eq!(approx_free_uses("x", &t), 0);
    }

    #[test]
    fn free_var_names_excludes_bound() {
        // λx. (x y)
        let t = lam("x", Grade::Unrestricted, app(var("x"), var("y")));
        let frees = free_var_names(&t);
        assert_eq!(frees, vec!["y".to_string()]);
    }
}
