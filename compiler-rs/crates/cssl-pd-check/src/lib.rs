#![forbid(unsafe_code)]
#![doc = "cssl-pd-check — Wave U-G prelude : PD-binding checker.\n\n\
Spec : `specs/Upgrade/impl/IMPL_01_PLAN.csl` § Wave-U-G. Takes an `Elaborated` \
program (from `cssl-elab`) plus a `PolicySet` of granted capability tokens \
(from `cssl-ocap`) and a `Consent` lattice level (from `cssl-consent`), and \
verifies that EVERY effect-label in the program's `EffectRow` has :\n\n\
  (1) a `CapToken` granted in the `PolicySet`, AND\n\
  (2) the token's MAC verifies under the `PolicySet` grantor's key, AND\n\
  (3) the consent level granted for the operation `permits` the level the \
      operation requires.\n\n\
This is the first end-to-end demonstration that the foundation crates compose \
into a load-bearing PD-binding pass : NO effect can be discharged without an \
unforgeable capability AND a sufficient consent grant."]

use cssl_consent::Consent;
use cssl_effects_row::{EffectLabel, EffectRow};
use cssl_elab::Elaborated;
use cssl_ocap::{CapToken, Grantor};
use std::collections::HashMap;
use thiserror::Error;

/// A single grant : `(cap_token, required_consent_level)`.
#[derive(Clone, Debug)]
pub struct Grant {
    pub token: CapToken,
    pub required_consent: Consent,
}

/// Policy set held by the PD-checker : `EffectLabel` → `Grant` plus the
/// grantor whose key minted the tokens.
#[derive(Clone, Debug)]
pub struct PolicySet {
    grantor: Grantor,
    grants: HashMap<EffectLabel, Grant>,
    /// Consent level the running program holds (e.g. user's current grant).
    pub current_consent: Consent,
}

impl PolicySet {
    /// Construct an empty policy set with the given grantor and consent baseline.
    #[must_use]
    pub fn new(grantor: Grantor, current_consent: Consent) -> Self {
        Self { grantor, grants: HashMap::new(), current_consent }
    }

    /// Register a grant for a given effect label.
    pub fn grant(&mut self, label: EffectLabel, grant: Grant) {
        self.grants.insert(label, grant);
    }

    /// Number of distinct effect labels covered.
    #[must_use]
    pub fn len(&self) -> usize { self.grants.len() }

    /// `true` iff no grants are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool { self.grants.is_empty() }
}

/// PD-binding check failure modes.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum PdViolation {
    /// An effect label was emitted by the program but no grant exists for it.
    #[error("effect `{0}` has no capability grant in PolicySet")]
    UngrantedEffect(EffectLabel),
    /// A grant exists but its MAC does not verify under the grantor's key.
    #[error("capability for effect `{0}` failed MAC verification (forged or wrong-grantor)")]
    ForgedCapability(EffectLabel),
    /// A grant exists and verifies, but the held consent level does not satisfy the
    /// required level for that operation.
    #[error("effect `{effect}` requires consent {required:?} but program holds {held:?}")]
    InsufficientConsent {
        effect: EffectLabel,
        required: Consent,
        held: Consent,
    },
}

/// Re-exported convenience name to make the type-rename below explicit.
type PdResult = Result<PdReport, PdViolation>;

/// Successful PD-check report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PdReport {
    /// Number of distinct effects checked.
    pub effects_checked: usize,
    /// `true` iff the program's effect row is empty (pure).
    pub pure: bool,
}

/// Run the PD-binding check over an elaborated program.
///
/// Returns `Ok(PdReport)` if every effect is covered ; otherwise the FIRST
/// violation in label-iteration-order (deterministic since `EffectRow.labels`
/// is a `BTreeSet`).
pub fn check(elab: &Elaborated, policy: &PolicySet) -> PdResult {
    check_row(&elab.effects, policy)
}

/// Run the PD-binding check over a raw effect-row (e.g. for unit-testing
/// without going through elaboration).
pub fn check_row(row: &EffectRow, policy: &PolicySet) -> PdResult {
    let mut count = 0usize;
    for label in &row.labels {
        let grant = policy
            .grants
            .get(label)
            .ok_or_else(|| PdViolation::UngrantedEffect(label.clone()))?;
        if !policy.grantor.verify(&grant.token) {
            return Err(PdViolation::ForgedCapability(label.clone()));
        }
        if !policy.current_consent.permits(grant.required_consent) {
            return Err(PdViolation::InsufficientConsent {
                effect: label.clone(),
                required: grant.required_consent,
                held: policy.current_consent,
            });
        }
        count += 1;
    }
    Ok(PdReport { effects_checked: count, pure: row.is_pure() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use cssl_cas::cid_of_bytes;
    use cssl_elab::{elaborate, Grade, Term};
    use cssl_ocap::CapType;

    fn grantor() -> Grantor {
        let mut k = [0u8; 32]; k[0] = 7; k[1] = 42;
        Grantor::new(k)
    }

    fn other_grantor() -> Grantor {
        let mut k = [0u8; 32]; k[0] = 99;
        Grantor::new(k)
    }

    fn cap(seed: u8) -> CapType { CapType(cid_of_bytes(&[seed])) }

    fn grant_for(g: &Grantor, label: &str, required: Consent) -> (EffectLabel, Grant) {
        let mut rng = rand::thread_rng();
        let token = g.mint(cap(label.as_bytes()[0]), &mut rng);
        (label.into(), Grant { token, required_consent: required })
    }

    #[test]
    fn pure_program_passes_check_with_empty_policy() {
        let elab = elaborate(&Term::Unit).unwrap();
        let policy = PolicySet::new(grantor(), Consent::Implicit);
        let r = check(&elab, &policy).unwrap();
        assert!(r.pure);
        assert_eq!(r.effects_checked, 0);
    }

    #[test]
    fn ungranted_effect_is_rejected() {
        let elab = elaborate(&Term::Op("io".into())).unwrap();
        let policy = PolicySet::new(grantor(), Consent::Explicit);
        let err = check(&elab, &policy).unwrap_err();
        assert_eq!(err, PdViolation::UngrantedEffect("io".into()));
    }

    #[test]
    fn granted_effect_with_sufficient_consent_passes() {
        let elab = elaborate(&Term::Op("io".into())).unwrap();
        let g = grantor();
        let mut policy = PolicySet::new(g.clone(), Consent::Explicit);
        let (l, gr) = grant_for(&g, "io", Consent::Implicit);
        policy.grant(l, gr);
        let r = check(&elab, &policy).unwrap();
        assert_eq!(r.effects_checked, 1);
        assert!(!r.pure);
    }

    #[test]
    fn granted_effect_with_insufficient_consent_is_rejected() {
        let elab = elaborate(&Term::Op("read_user".into())).unwrap();
        let g = grantor();
        let mut policy = PolicySet::new(g.clone(), Consent::Implicit);
        let (l, gr) = grant_for(&g, "read_user", Consent::Explicit);
        policy.grant(l, gr);
        let err = check(&elab, &policy).unwrap_err();
        assert!(matches!(
            err,
            PdViolation::InsufficientConsent {
                required: Consent::Explicit,
                held: Consent::Implicit,
                ..
            }
        ));
    }

    #[test]
    fn token_minted_by_other_grantor_fails_verification() {
        let elab = elaborate(&Term::Op("io".into())).unwrap();
        let g_real = grantor();
        let g_fake = other_grantor();
        let mut policy = PolicySet::new(g_real, Consent::Explicit);
        // Fake grantor mints a token claiming the same cap-type.
        let (l, gr) = grant_for(&g_fake, "io", Consent::Implicit);
        policy.grant(l, gr);
        let err = check(&elab, &policy).unwrap_err();
        assert_eq!(err, PdViolation::ForgedCapability("io".into()));
    }

    #[test]
    fn revoked_consent_blocks_all_non_trivial_effects() {
        let elab = elaborate(&Term::Op("io".into())).unwrap();
        let g = grantor();
        let mut policy = PolicySet::new(g.clone(), Consent::Revoked);
        let (l, gr) = grant_for(&g, "io", Consent::Implicit);
        policy.grant(l, gr);
        let err = check(&elab, &policy).unwrap_err();
        assert!(matches!(err, PdViolation::InsufficientConsent { .. }),
            "Revoked consent permits no operations regardless of grant");
    }

    #[test]
    fn multi_effect_program_requires_grant_per_label() {
        // Term : (op io) (op state)
        let t = Term::App(
            Box::new(Term::Op("io".into())),
            Box::new(Term::Op("state".into())),
        );
        let elab = elaborate(&t).unwrap();
        let g = grantor();
        let mut policy = PolicySet::new(g.clone(), Consent::Explicit);
        let (l1, gr1) = grant_for(&g, "io", Consent::Implicit);
        policy.grant(l1, gr1);
        // Forget to grant "state" — must fail.
        let err = check(&elab, &policy).unwrap_err();
        assert_eq!(err, PdViolation::UngrantedEffect("state".into()));

        // Now grant it too — must pass.
        let (l2, gr2) = grant_for(&g, "state", Consent::Implicit);
        policy.grant(l2, gr2);
        let r = check(&elab, &policy).unwrap();
        assert_eq!(r.effects_checked, 2);
    }

    #[test]
    fn graded_lambda_with_effect_in_body_carries_effect_to_callsite() {
        // λx:ω. op io
        let t = Term::Lam {
            param: "x".into(),
            grade: Grade::Unrestricted,
            body: Box::new(Term::Op("io".into())),
        };
        let elab = elaborate(&t).unwrap();
        // Per Wave U-B doc : lambda preserves body-effects (suspension is U-C).
        // PD-check therefore must reject without an io grant.
        let policy = PolicySet::new(grantor(), Consent::Explicit);
        let err = check(&elab, &policy).unwrap_err();
        assert_eq!(err, PdViolation::UngrantedEffect("io".into()));
    }
}
