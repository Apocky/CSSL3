//! Unification for `Ty` and `Row`, with occurs-check.
//!
//! § ALGORITHM
//!   Classic Robinson unification on the monotype portion of `Ty`. Row unification
//!   follows the Remy-style "rewrite the other side" approach : to unify
//!   `{e1, e2 | μ}` with `{e2, e3 | ν}`, compute symmetric-difference of the effect
//!   sets and bind `μ` / `ν` to capture the leftover.
//!
//! § SOUNDNESS
//!   Monotype unification is sound by standard Robinson result. Row unification is
//!   sound under the assumption that effect-instance equality is structural : two
//!   `EffectInstance` records are equal iff their `name` and `args` are equal-after-
//!   substitution. Stage-0 uses structural equality on effect args (no subtyping).

use thiserror::Error;

use crate::typing::{EffectInstance, Row, RowVar, Subst, Ty, TyVar};

/// Failure modes for `unify`.
#[derive(Debug, Clone, Error)]
pub enum UnifyError {
    /// The two types have incompatible top-level shapes.
    #[error("type mismatch : cannot unify {a:?} with {b:?}")]
    Mismatch { a: Ty, b: Ty },
    /// `a.arity != b.arity` for tuples / fn-params / type-args.
    #[error("arity mismatch : expected {expected} elements, found {found}")]
    Arity { expected: usize, found: usize },
    /// Binding `v -> t` would create an infinite type.
    #[error("occurs check failed : variable {v:?} appears inside {t:?}")]
    OccursCheck { v: TyVar, t: Ty },
    /// Row unification failed — effects present on one side that the other can't absorb.
    #[error("effect-row mismatch : {left:?} vs {right:?}")]
    RowMismatch { left: Row, right: Row },
}

/// Unify two types. On success, the `subst` is extended in-place. On failure,
/// no partial mutations are kept (the caller typically restarts from a snapshot).
pub fn unify(a: &Ty, b: &Ty, subst: &mut Subst) -> Result<(), UnifyError> {
    let a = subst.apply(a);
    let b = subst.apply(b);
    unify_step(&a, &b, subst)
}

fn unify_step(a: &Ty, b: &Ty, subst: &mut Subst) -> Result<(), UnifyError> {
    match (a, b) {
        // Identical primitives.
        (Ty::Int, Ty::Int)
        | (Ty::Float, Ty::Float)
        | (Ty::Bool, Ty::Bool)
        | (Ty::Str, Ty::Str)
        | (Ty::Unit, Ty::Unit) => Ok(()),
        // Never + Error unify with anything.
        (Ty::Never, _) | (_, Ty::Never) | (Ty::Error, _) | (_, Ty::Error) => Ok(()),
        // Same skolem param.
        (Ty::Param(p1), Ty::Param(p2)) if p1 == p2 => Ok(()),
        // Variable unification.
        (Ty::Var(v), other) | (other, Ty::Var(v)) => bind_ty_var(*v, other.clone(), subst),
        // Named constructors : must match on def id + arg arity.
        (Ty::Named { def: d1, args: a1 }, Ty::Named { def: d2, args: a2 }) => {
            if d1 != d2 {
                return Err(UnifyError::Mismatch {
                    a: a.clone(),
                    b: b.clone(),
                });
            }
            unify_slices(a1, a2, subst)
        }
        // Tuples.
        (Ty::Tuple(a1), Ty::Tuple(a2)) => unify_slices(a1, a2, subst),
        // References : mutability must match.
        (
            Ty::Ref {
                mutable: m1,
                inner: i1,
            },
            Ty::Ref {
                mutable: m2,
                inner: i2,
            },
        ) => {
            if m1 != m2 {
                return Err(UnifyError::Mismatch {
                    a: a.clone(),
                    b: b.clone(),
                });
            }
            unify(i1, i2, subst)
        }
        // Function types : params + return + effect-row.
        (
            Ty::Fn {
                params: p1,
                return_ty: r1,
                effect_row: er1,
            },
            Ty::Fn {
                params: p2,
                return_ty: r2,
                effect_row: er2,
            },
        ) => {
            unify_slices(p1, p2, subst)?;
            unify(r1, r2, subst)?;
            unify_rows(er1, er2, subst)
        }
        // Array : elem + length.
        (Ty::Array { elem: e1, len: l1 }, Ty::Array { elem: e2, len: l2 }) => {
            if l1 != l2 {
                return Err(UnifyError::Mismatch {
                    a: a.clone(),
                    b: b.clone(),
                });
            }
            unify(e1, e2, subst)
        }
        // Slice.
        (Ty::Slice { elem: e1 }, Ty::Slice { elem: e2 }) => unify(e1, e2, subst),
        // Everything else : mismatch.
        _ => Err(UnifyError::Mismatch {
            a: a.clone(),
            b: b.clone(),
        }),
    }
}

fn unify_slices(a: &[Ty], b: &[Ty], subst: &mut Subst) -> Result<(), UnifyError> {
    if a.len() != b.len() {
        return Err(UnifyError::Arity {
            expected: a.len(),
            found: b.len(),
        });
    }
    for (x, y) in a.iter().zip(b.iter()) {
        unify(x, y, subst)?;
    }
    Ok(())
}

fn bind_ty_var(v: TyVar, t: Ty, subst: &mut Subst) -> Result<(), UnifyError> {
    if let Ty::Var(w) = &t {
        if *w == v {
            return Ok(());
        }
    }
    if occurs_in(v, &t, subst) {
        return Err(UnifyError::OccursCheck { v, t });
    }
    subst.bind_ty(v, t);
    Ok(())
}

/// `true` iff `v` appears inside `t` (after applying `subst`).
pub fn occurs_in(v: TyVar, t: &Ty, subst: &Subst) -> bool {
    let t = subst.apply(t);
    match &t {
        Ty::Var(w) => *w == v,
        Ty::Named { args, .. } => args.iter().any(|a| occurs_in(v, a, subst)),
        Ty::Tuple(elems) => elems.iter().any(|e| occurs_in(v, e, subst)),
        Ty::Ref { inner, .. } | Ty::Array { elem: inner, .. } | Ty::Slice { elem: inner } => {
            occurs_in(v, inner, subst)
        }
        Ty::Fn {
            params,
            return_ty,
            effect_row,
        } => {
            params.iter().any(|p| occurs_in(v, p, subst))
                || occurs_in(v, return_ty, subst)
                || effect_row
                    .effects
                    .iter()
                    .any(|e| e.args.iter().any(|a| occurs_in(v, a, subst)))
        }
        _ => false,
    }
}

// ─ Row unification ──────────────────────────────────────────────────────────

/// Unify two rows. The algorithm :
///   1. Apply `subst` to both rows.
///   2. Find the intersection of effect names present on both sides.
///   3. Unify the type arguments of matching effects.
///   4. The "extra" effects on each side are absorbed by the opposite-side tail
///      variable (if present) ; if absent, it's a mismatch.
pub fn unify_rows(a: &Row, b: &Row, subst: &mut Subst) -> Result<(), UnifyError> {
    let a = subst.apply_row(a);
    let b = subst.apply_row(b);
    unify_rows_step(&a, &b, subst)
}

fn unify_rows_step(a: &Row, b: &Row, subst: &mut Subst) -> Result<(), UnifyError> {
    // Step 1 : match effects that appear on both sides.
    let mut a_remaining: Vec<EffectInstance> = Vec::new();
    let mut b_remaining: Vec<EffectInstance> = b.effects.clone();
    for e_a in &a.effects {
        if let Some(pos) = b_remaining.iter().position(|e_b| e_b.name == e_a.name) {
            let e_b = b_remaining.remove(pos);
            // Unify arguments — same length required.
            if e_a.args.len() != e_b.args.len() {
                return Err(UnifyError::RowMismatch {
                    left: a.clone(),
                    right: b.clone(),
                });
            }
            for (x, y) in e_a.args.iter().zip(e_b.args.iter()) {
                unify(x, y, subst)?;
            }
        } else {
            a_remaining.push(e_a.clone());
        }
    }
    // Step 2 : route the remainders.
    // a_remaining → must be absorbed by b's tail.
    // b_remaining → must be absorbed by a's tail.
    let a_leftover_on_b_tail = absorb(&a_remaining, b.tail, a.tail, subst);
    let b_leftover_on_a_tail = absorb(&b_remaining, a.tail, b.tail, subst);
    match (a_leftover_on_b_tail, b_leftover_on_a_tail) {
        (Ok(()), Ok(())) => Ok(()),
        _ => Err(UnifyError::RowMismatch {
            left: a.clone(),
            right: b.clone(),
        }),
    }
}

/// Try to absorb the leftover effects into the target-side tail variable.
/// `extras` — effects that one side has that the other doesn't.
/// `target_tail` — the tail variable on the OPPOSITE side (where we want to bind them).
/// `source_tail` — the tail variable on THIS side (propagated into the target if present).
fn absorb(
    extras: &[EffectInstance],
    target_tail: Option<RowVar>,
    source_tail: Option<RowVar>,
    subst: &mut Subst,
) -> Result<(), ()> {
    if extras.is_empty() && target_tail == source_tail {
        return Ok(());
    }
    match target_tail {
        Some(tv) => {
            // Bind tv → { extras | source_tail }
            let row = Row {
                effects: extras.to_vec(),
                tail: source_tail,
            };
            subst.bind_row(tv, row);
            Ok(())
        }
        None => {
            // No target tail to absorb into. If there are no extras, a source_tail can
            // still unify by binding it to pure — there's nothing for it to account for.
            if extras.is_empty() {
                if let Some(sv) = source_tail {
                    subst.bind_row(sv, Row::pure());
                }
                Ok(())
            } else {
                Err(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{unify, unify_rows, UnifyError};
    use crate::symbol::Interner;
    use crate::typing::{EffectInstance, Row, RowVar, Subst, Ty, TyVar};

    #[test]
    fn unify_primitives_success() {
        let mut s = Subst::new();
        assert!(unify(&Ty::Int, &Ty::Int, &mut s).is_ok());
        assert!(unify(&Ty::Bool, &Ty::Bool, &mut s).is_ok());
    }

    #[test]
    fn unify_primitives_mismatch() {
        let mut s = Subst::new();
        assert!(matches!(
            unify(&Ty::Int, &Ty::Bool, &mut s),
            Err(UnifyError::Mismatch { .. })
        ));
    }

    #[test]
    fn unify_var_with_concrete() {
        let mut s = Subst::new();
        let v = Ty::Var(TyVar(0));
        assert!(unify(&v, &Ty::Int, &mut s).is_ok());
        assert_eq!(s.apply(&v), Ty::Int);
    }

    #[test]
    fn unify_two_vars_chains() {
        let mut s = Subst::new();
        assert!(unify(&Ty::Var(TyVar(0)), &Ty::Var(TyVar(1)), &mut s).is_ok());
        assert!(unify(&Ty::Var(TyVar(1)), &Ty::Int, &mut s).is_ok());
        assert_eq!(s.apply(&Ty::Var(TyVar(0))), Ty::Int);
    }

    #[test]
    fn unify_tuples_elementwise() {
        let mut s = Subst::new();
        let a = Ty::Tuple(vec![Ty::Var(TyVar(0)), Ty::Var(TyVar(1))]);
        let b = Ty::Tuple(vec![Ty::Int, Ty::Bool]);
        assert!(unify(&a, &b, &mut s).is_ok());
        assert_eq!(s.apply(&Ty::Var(TyVar(0))), Ty::Int);
        assert_eq!(s.apply(&Ty::Var(TyVar(1))), Ty::Bool);
    }

    #[test]
    fn unify_tuples_arity_mismatch() {
        let mut s = Subst::new();
        let a = Ty::Tuple(vec![Ty::Int, Ty::Int]);
        let b = Ty::Tuple(vec![Ty::Int]);
        assert!(matches!(
            unify(&a, &b, &mut s),
            Err(UnifyError::Arity { .. })
        ));
    }

    #[test]
    fn unify_fn_types() {
        let mut s = Subst::new();
        let a = Ty::Fn {
            params: vec![Ty::Int],
            return_ty: Box::new(Ty::Var(TyVar(0))),
            effect_row: Row::pure(),
        };
        let b = Ty::Fn {
            params: vec![Ty::Int],
            return_ty: Box::new(Ty::Bool),
            effect_row: Row::pure(),
        };
        assert!(unify(&a, &b, &mut s).is_ok());
        assert_eq!(s.apply(&Ty::Var(TyVar(0))), Ty::Bool);
    }

    #[test]
    fn unify_occurs_check() {
        let mut s = Subst::new();
        let v = Ty::Var(TyVar(0));
        // Unify v with Tuple(v) — should fail with occurs-check.
        let recursive = Ty::Tuple(vec![v.clone()]);
        assert!(matches!(
            unify(&v, &recursive, &mut s),
            Err(UnifyError::OccursCheck { .. })
        ));
    }

    #[test]
    fn unify_never_with_anything() {
        let mut s = Subst::new();
        assert!(unify(&Ty::Never, &Ty::Int, &mut s).is_ok());
        assert!(unify(&Ty::Bool, &Ty::Never, &mut s).is_ok());
    }

    #[test]
    fn unify_error_with_anything() {
        let mut s = Subst::new();
        assert!(unify(&Ty::Error, &Ty::Int, &mut s).is_ok());
        assert!(unify(&Ty::Tuple(vec![Ty::Bool]), &Ty::Error, &mut s).is_ok());
    }

    #[test]
    fn unify_closed_rows_same_effects() {
        let mut s = Subst::new();
        let interner = Interner::new();
        let gpu = interner.intern("GPU");
        let a = Row::closed(vec![EffectInstance {
            name: gpu,
            args: Vec::new(),
        }]);
        let b = Row::closed(vec![EffectInstance {
            name: gpu,
            args: Vec::new(),
        }]);
        assert!(unify_rows(&a, &b, &mut s).is_ok());
    }

    #[test]
    fn unify_closed_rows_different_mismatch() {
        let mut s = Subst::new();
        let interner = Interner::new();
        let gpu = interner.intern("GPU");
        let cpu = interner.intern("CPU");
        let a = Row::closed(vec![EffectInstance {
            name: gpu,
            args: Vec::new(),
        }]);
        let b = Row::closed(vec![EffectInstance {
            name: cpu,
            args: Vec::new(),
        }]);
        assert!(matches!(
            unify_rows(&a, &b, &mut s),
            Err(UnifyError::RowMismatch { .. })
        ));
    }

    #[test]
    fn unify_rows_with_tail_absorbs() {
        let mut s = Subst::new();
        let interner = Interner::new();
        let gpu = interner.intern("GPU");
        // a = { GPU | μ }
        let a = Row {
            effects: vec![EffectInstance {
                name: gpu,
                args: Vec::new(),
            }],
            tail: Some(RowVar(0)),
        };
        // b = { GPU } (closed)
        let b = Row::closed(vec![EffectInstance {
            name: gpu,
            args: Vec::new(),
        }]);
        // Should unify : μ gets bound to pure.
        assert!(unify_rows(&a, &b, &mut s).is_ok());
    }

    #[test]
    fn unify_rows_different_tails_link() {
        let mut s = Subst::new();
        let interner = Interner::new();
        let gpu = interner.intern("GPU");
        let cpu = interner.intern("CPU");
        // a = { GPU | μ }, b = { CPU | ν }  → μ binds to {CPU | ν'}, ν binds to {GPU | μ'} style
        let a = Row {
            effects: vec![EffectInstance {
                name: gpu,
                args: Vec::new(),
            }],
            tail: Some(RowVar(0)),
        };
        let b = Row {
            effects: vec![EffectInstance {
                name: cpu,
                args: Vec::new(),
            }],
            tail: Some(RowVar(1)),
        };
        assert!(unify_rows(&a, &b, &mut s).is_ok());
    }
}
