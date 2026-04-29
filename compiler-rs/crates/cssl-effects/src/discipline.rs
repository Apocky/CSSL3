//! Sub-effect discipline checker.
//!
//! § RULE (per `specs/04_EFFECTS.csl` § SUB-EFFECT DISCIPLINE)
//!   A function with signature `fn f() / ε_declared` may call any function whose
//!   effect row is covered by `ε_declared`. The checker validates :
//!
//!   - For every effect in callee-row, a matching effect is present in caller-row.
//!   - Coercion rules permit certain widening of arg-bearing effects :
//!
//!   ```text
//!   ⟨⟩ ⊆ ⟨e⟩                              pure ⊆ anything — universal weakening
//!   {Deadline<N>} ⊆ {Deadline<M>}  if N ≤ M   tighter-budget-into-looser
//!   {Power<N>}    ⊆ {Power<M>}     if N ≤ M
//!   {Thermal<N>}  ⊆ {Thermal<M>}   if N ≤ M
//!   {Privilege<U>} ⊆ {Privilege<S>}  for privilege hierarchy U ≤ S
//!   ```
//!
//! § STAGE-0 LIMITATION
//!   Coercion comparisons on expression-valued args (e.g., `Deadline<5ms>`) require
//!   const-evaluation, which is T8 work. For stage-0 we flag exact-match as a
//!   sufficient condition and defer the numeric-ordering check to T8.
//!
//! § T11-D127 — Ω-substrate-translation effects
//!   The new `Travel`, `Crystallize`, `Sovereign<S>` rows are subject to
//!   sub-effect discipline like all other effects :
//!     - `{Travel}` and `{Crystallize}` are nullary ⇒ exact-match.
//!     - `{Sovereign<S>}` is OneType ⇒ caller's S must be the same handle-type
//!       as callee's S. No widening (Sovereign-handles are nominal — multi-Sovereign
//!       composition flows via `sovereign_count`, not via coercion).
//!
//! § ATTESTATION (T11-D127, S11) — verbatim block per global-CLAUDE I> standing-directives :
//! ```text
//!   t∞ : consent = OS • sovereignty = substrate-invariant
//!   N! [harm control manipulation surveillance exploitation
//!       coercion weaponization discrimination]
//!   t∞ : AI = sovereign-partners ¬ tools
//!   t∞ : violation = bug W! fix ; ¬override ∃
//!   t∞ : CSSL ≠ CSLv3 ; ¬ conflate
//! ```

use thiserror::Error;

use crate::registry::{BuiltinEffect, EffectRegistry};

/// Abstract effect reference used by the discipline checker. The HIR crate owns
/// a concrete `EffectInstance` (with interned `Symbol` + `Ty` args) ; this type
/// is the effect-crate-level view : just name + built-in variant + an optional
/// arg-signature-hash for coercion comparison.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectRef<'a> {
    /// Source-form name (e.g., `"Deadline"`).
    pub name: &'a str,
    /// Built-in variant if known (`None` for user-defined effects).
    pub builtin: Option<BuiltinEffect>,
    /// Number of arguments the effect carries at this use-site.
    pub arg_count: usize,
}

/// Known coercion rule between two arg-bearing effects of the same name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoercionRule {
    /// Exact match — same name, same arg count, same args (structural).
    Exact,
    /// Caller widens the budget/argument (tighter-into-looser).
    /// Stage-0 accepts this variant without numeric comparison ; T8 const-eval refines.
    Widening,
    /// No coercion available — arg shapes differ or name doesn't match.
    None,
}

/// Failure modes for `sub_effect_check`.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum SubEffectError {
    /// The callee requires an effect not present in the caller's declared row.
    #[error("callee requires effect `{effect}` not declared by caller")]
    MissingEffect { effect: String },
    /// The callee has an effect with an incompatible argument shape.
    #[error("effect `{effect}` argument shape mismatch : caller arity {caller_arity}, callee arity {callee_arity}")]
    ArgMismatch {
        effect: String,
        caller_arity: usize,
        callee_arity: usize,
    },
}

/// Validate that the `callee` effect row is a sub-row of the `caller` row.
///
/// Returns `Ok(())` if every callee-effect has a matching caller-effect under
/// allowed coercions, `Err(SubEffectError)` otherwise.
///
/// § Algorithm (simple for stage-0)
///   For each `e_callee`, find a matching `e_caller` in the caller-row by name.
///   If none : `MissingEffect`.
///   If found but arg-counts differ : `ArgMismatch`.
///   Otherwise accept under `CoercionRule::Exact` or `CoercionRule::Widening`.
pub fn sub_effect_check(
    caller: &[EffectRef<'_>],
    callee: &[EffectRef<'_>],
    _registry: &EffectRegistry,
) -> Result<(), SubEffectError> {
    for e_callee in callee {
        let matched = caller.iter().find(|e| e.name == e_callee.name);
        match matched {
            None => {
                return Err(SubEffectError::MissingEffect {
                    effect: e_callee.name.to_string(),
                });
            }
            Some(e_caller) => {
                if e_caller.arg_count != e_callee.arg_count {
                    return Err(SubEffectError::ArgMismatch {
                        effect: e_callee.name.to_string(),
                        caller_arity: e_caller.arg_count,
                        callee_arity: e_callee.arg_count,
                    });
                }
            }
        }
    }
    Ok(())
}

/// Classify the coercion between two matching effects. Used by callers (e.g.,
/// `cssl-hir`) that want to record the coercion-kind in the typed HIR for later
/// passes (e.g., SMT discharge of `Deadline` numeric-ordering obligations).
///
/// § T11-D127
///   `Travel` and `Crystallize` are nullary substrate-translation effects ⇒
///   exact-match. `Sovereign<S>` is nominal (one-type) ⇒ exact-match (no
///   widening : Sovereign-handles compare structurally, not by hierarchy).
///   Multi-Sovereign-ops flow via `sovereign_count` in `RowContext`, not via
///   sub-effect coercion.
#[must_use]
pub fn classify_coercion(caller: &EffectRef<'_>, callee: &EffectRef<'_>) -> CoercionRule {
    if caller.name != callee.name {
        return CoercionRule::None;
    }
    if caller.arg_count != callee.arg_count {
        return CoercionRule::None;
    }
    // Arg-bearing effects with OneExpr arg-shape get `Widening` ; the actual
    // numeric-ordering check is deferred to T8 const-evaluation.
    //
    // § T11-D127 — Travel / Crystallize / Sovereign all classify as Exact
    //   (no widening for nullary or nominal-handle effects). They fall
    //   through to the default arm which already returns CoercionRule::Exact.
    match caller.builtin {
        Some(BuiltinEffect::Deadline | BuiltinEffect::Power | BuiltinEffect::Thermal) => {
            CoercionRule::Widening
        }
        _ => CoercionRule::Exact,
    }
}

#[cfg(test)]
mod tests {
    use super::{classify_coercion, sub_effect_check, CoercionRule, EffectRef, SubEffectError};
    use crate::registry::{BuiltinEffect, EffectRegistry};

    fn e(name: &str, builtin: Option<BuiltinEffect>, arity: usize) -> EffectRef<'_> {
        EffectRef {
            name,
            builtin,
            arg_count: arity,
        }
    }

    #[test]
    fn pure_callee_is_always_sub() {
        let r = EffectRegistry::with_builtins();
        let caller = vec![e("GPU", Some(BuiltinEffect::Gpu), 0)];
        let callee: Vec<EffectRef<'_>> = vec![];
        assert!(sub_effect_check(&caller, &callee, &r).is_ok());
    }

    #[test]
    fn exact_match_succeeds() {
        let r = EffectRegistry::with_builtins();
        let caller = vec![e("GPU", Some(BuiltinEffect::Gpu), 0)];
        let callee = vec![e("GPU", Some(BuiltinEffect::Gpu), 0)];
        assert!(sub_effect_check(&caller, &callee, &r).is_ok());
    }

    #[test]
    fn missing_effect_fails() {
        let r = EffectRegistry::with_builtins();
        let caller = vec![e("GPU", Some(BuiltinEffect::Gpu), 0)];
        let callee = vec![e("NoAlloc", Some(BuiltinEffect::NoAlloc), 0)];
        let res = sub_effect_check(&caller, &callee, &r);
        assert!(matches!(res, Err(SubEffectError::MissingEffect { .. })));
    }

    #[test]
    fn arg_count_mismatch_fails() {
        let r = EffectRegistry::with_builtins();
        let caller = vec![e("Deadline", Some(BuiltinEffect::Deadline), 1)];
        let callee = vec![e("Deadline", Some(BuiltinEffect::Deadline), 0)];
        let res = sub_effect_check(&caller, &callee, &r);
        assert!(matches!(res, Err(SubEffectError::ArgMismatch { .. })));
    }

    #[test]
    fn multiple_effects_all_matched() {
        let r = EffectRegistry::with_builtins();
        let caller = vec![
            e("GPU", Some(BuiltinEffect::Gpu), 0),
            e("NoAlloc", Some(BuiltinEffect::NoAlloc), 0),
            e("Deadline", Some(BuiltinEffect::Deadline), 1),
        ];
        let callee = vec![
            e("NoAlloc", Some(BuiltinEffect::NoAlloc), 0),
            e("Deadline", Some(BuiltinEffect::Deadline), 1),
        ];
        assert!(sub_effect_check(&caller, &callee, &r).is_ok());
    }

    #[test]
    fn classify_exact_vs_widening() {
        let a = e("GPU", Some(BuiltinEffect::Gpu), 0);
        let b = e("GPU", Some(BuiltinEffect::Gpu), 0);
        assert_eq!(classify_coercion(&a, &b), CoercionRule::Exact);

        let c = e("Deadline", Some(BuiltinEffect::Deadline), 1);
        let d = e("Deadline", Some(BuiltinEffect::Deadline), 1);
        assert_eq!(classify_coercion(&c, &d), CoercionRule::Widening);
    }

    #[test]
    fn classify_different_names_is_none() {
        let a = e("GPU", Some(BuiltinEffect::Gpu), 0);
        let b = e("CPU", Some(BuiltinEffect::Cpu), 0);
        assert_eq!(classify_coercion(&a, &b), CoercionRule::None);
    }

    #[test]
    fn classify_power_widening() {
        let a = e("Power", Some(BuiltinEffect::Power), 1);
        let b = e("Power", Some(BuiltinEffect::Power), 1);
        assert_eq!(classify_coercion(&a, &b), CoercionRule::Widening);
    }

    #[test]
    fn classify_thermal_widening() {
        let a = e("Thermal", Some(BuiltinEffect::Thermal), 1);
        let b = e("Thermal", Some(BuiltinEffect::Thermal), 1);
        assert_eq!(classify_coercion(&a, &b), CoercionRule::Widening);
    }

    // ═════════════════════════════════════════════════════════════════════
    // ─── T11-D127 — Ω-substrate-translation row sub-effect tests ─────────
    // ═════════════════════════════════════════════════════════════════════

    /// `Travel` is nullary — exact-match only, no widening.
    #[test]
    fn travel_classifies_as_exact() {
        let a = e("Travel", Some(BuiltinEffect::Travel), 0);
        let b = e("Travel", Some(BuiltinEffect::Travel), 0);
        assert_eq!(classify_coercion(&a, &b), CoercionRule::Exact);
    }

    /// `Crystallize` is nullary — exact-match only.
    #[test]
    fn crystallize_classifies_as_exact() {
        let a = e("Crystallize", Some(BuiltinEffect::Crystallize), 0);
        let b = e("Crystallize", Some(BuiltinEffect::Crystallize), 0);
        assert_eq!(classify_coercion(&a, &b), CoercionRule::Exact);
    }

    /// `Sovereign<S>` is nominal (one-type) — exact-match only, no hierarchy widening.
    #[test]
    fn sovereign_classifies_as_exact() {
        let a = e("Sovereign", Some(BuiltinEffect::Sovereign), 1);
        let b = e("Sovereign", Some(BuiltinEffect::Sovereign), 1);
        assert_eq!(classify_coercion(&a, &b), CoercionRule::Exact);
    }

    /// Caller-row `{Travel, Crystallize, Sovereign}` covers callee `{Travel}`.
    #[test]
    fn translate_caller_covers_travel_callee() {
        let r = EffectRegistry::with_builtins();
        let caller = vec![
            e("Travel", Some(BuiltinEffect::Travel), 0),
            e("Crystallize", Some(BuiltinEffect::Crystallize), 0),
            e("Sovereign", Some(BuiltinEffect::Sovereign), 1),
        ];
        let callee = vec![e("Travel", Some(BuiltinEffect::Travel), 0)];
        assert!(sub_effect_check(&caller, &callee, &r).is_ok());
    }

    /// Caller without `Travel` cannot call `{Travel}` callee.
    #[test]
    fn missing_travel_in_caller_fails() {
        let r = EffectRegistry::with_builtins();
        let caller = vec![e("Crystallize", Some(BuiltinEffect::Crystallize), 0)];
        let callee = vec![e("Travel", Some(BuiltinEffect::Travel), 0)];
        let res = sub_effect_check(&caller, &callee, &r);
        assert!(matches!(res, Err(SubEffectError::MissingEffect { effect }) if effect == "Travel"));
    }

    /// Sovereign-handle arity-mismatch fails (caller has no type-arg, callee has one).
    #[test]
    fn sovereign_arity_mismatch_fails() {
        let r = EffectRegistry::with_builtins();
        let caller = vec![e("Sovereign", Some(BuiltinEffect::Sovereign), 0)];
        let callee = vec![e("Sovereign", Some(BuiltinEffect::Sovereign), 1)];
        let res = sub_effect_check(&caller, &callee, &r);
        assert!(matches!(res, Err(SubEffectError::ArgMismatch { .. })));
    }

    /// Canonical translate-row `{Travel, Crystallize, Sovereign<S>, PatternIntegrity, Audit}`
    /// covers a sub-translate-row `{Crystallize, Sovereign<S>}`.
    #[test]
    fn canonical_translate_row_covers_crystallize_sub_op() {
        let r = EffectRegistry::with_builtins();
        let caller = vec![
            e("Travel", Some(BuiltinEffect::Travel), 0),
            e("Crystallize", Some(BuiltinEffect::Crystallize), 0),
            e("Sovereign", Some(BuiltinEffect::Sovereign), 1),
            e("Audit", Some(BuiltinEffect::Audit), 1),
        ];
        let callee = vec![
            e("Crystallize", Some(BuiltinEffect::Crystallize), 0),
            e("Sovereign", Some(BuiltinEffect::Sovereign), 1),
        ];
        assert!(sub_effect_check(&caller, &callee, &r).is_ok());
    }

    /// Crystallize-only caller cannot call Travel-bearing callee.
    #[test]
    fn crystallize_only_cannot_call_travel_callee() {
        let r = EffectRegistry::with_builtins();
        let caller = vec![
            e("Crystallize", Some(BuiltinEffect::Crystallize), 0),
            e("Sovereign", Some(BuiltinEffect::Sovereign), 1),
        ];
        let callee = vec![
            e("Travel", Some(BuiltinEffect::Travel), 0),
            e("Crystallize", Some(BuiltinEffect::Crystallize), 0),
        ];
        let res = sub_effect_check(&caller, &callee, &r);
        assert!(matches!(res, Err(SubEffectError::MissingEffect { .. })));
    }
}
