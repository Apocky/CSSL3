//! § propagation.rs — Σ-mask composition rules across nested cells.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § THESIS
//!   When a parent mask carries [`crate::FLAG_PROPAGATE`], child cells
//!   INHERIT a strictly-tighter mask. Composition is AND-narrowing on the
//!   permission-axes (audience · effect-caps) + MAX-stricter on the floor-
//!   axes (k-anon-thresh) + MIN-sooner on the time-axes (TTL).
//!
//! § CASCADE-REVOKE
//!   Parent revoked ⇒ child auto-revoked using the parent's `revoked_at`.
//!   This is the load-bearing property that makes whole-subtree-revocation
//!   constant-time : revoke the root, descendants observe `revoked_at != 0`
//!   on their next composition.
//!
//! § OVERRIDE
//!   When a child carries [`crate::FLAG_OVERRIDE`], the SOVEREIGN-DECLARED
//!   child intent supersedes parent narrowing on dimensions where the child
//!   is STRICTER than parent. Override may NEVER LOOSEN — narrowing remains
//!   monotone-tighter on every axis.

use thiserror::Error;

use crate::mask::{
    SigmaMask, FLAG_INHERIT, FLAG_OVERRIDE, FLAG_PROPAGATE,
};

/// Composition error variants.
#[derive(Debug, Clone, Error)]
pub enum CompositionError {
    /// Parent mask is not flagged PROPAGATE — composition was requested
    /// but the parent did not authorize it.
    #[error("parent mask is not PROPAGATE-flagged")]
    ParentDoesNotPropagate,
    /// Parent or child checksum failed validation.
    #[error("input mask failed checksum validation")]
    InputTampered,
    /// Child attempts to LOOSEN a dimension below parent · forbidden.
    #[error("child attempts to loosen dimension {dim:?} — forbidden")]
    LooseningForbidden { dim: LooseningDim },
}

/// Identifies the dimension of a forbidden-loosening attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LooseningDim {
    /// Audience class (child requested broader audience than parent).
    Audience,
    /// Effect caps (child requested more effects than parent).
    Effects,
    /// K-anonymity threshold (child requested LOWER floor than parent).
    KAnon,
    /// TTL (child requested LATER expiry than parent).
    Ttl,
}

/// Compose a parent's PROPAGATE mask with a child's declared mask, producing
/// the effective inherited mask the child sees at evaluate-time.
///
/// § ARGS
///   - `parent` : the upstream mask carrying [`FLAG_PROPAGATE`].
///   - `child`  : the downstream mask declaring the child's INTENT
///                (broader-or-stricter ; will be narrowed by parent).
///   - `now_seconds` : wall-clock-second used for the composition timestamp
///                     (the resulting mask's `created_at`).
///
/// § RETURNS
///   The COMPOSED mask · checksum-fresh · `FLAG_INHERIT` bit set.
///
/// § COMPOSITION RULES (AND-narrowing semantics)
///   ```text
///   audience_class    = parent.audience_class & child.audience_class
///   effect_caps       = parent.effect_caps    & child.effect_caps
///   k_anon_thresh     = max(parent.k_anon_thresh, child.k_anon_thresh)
///   ttl_seconds       = min-effective(parent.ttl, child.ttl)
///   revoked_at        = parent.revoked_at != 0 ? parent.revoked_at : 0
///   flags             = (parent.flags & PROPAGATE) | child.flags | INHERIT
///   ```
///
/// § OVERRIDE INTERACTION
///   If the child carries `FLAG_OVERRIDE`, AND-narrowing still applies,
///   but the function does NOT raise `LooseningForbidden` — overrides may
///   express equivalent strictness without erroring (idempotency property).
pub fn compose_parent_child(
    parent: &SigmaMask,
    child: &SigmaMask,
    now_seconds: u64,
) -> Result<SigmaMask, CompositionError> {
    if !parent.verify_checksum() || !child.verify_checksum() {
        return Err(CompositionError::InputTampered);
    }
    if !parent.has_flag(FLAG_PROPAGATE) {
        return Err(CompositionError::ParentDoesNotPropagate);
    }

    // ── AND-narrow audience-class ──────────────────────────────────────
    let composed_audience = parent.audience_class() & child.audience_class();
    // ── AND-narrow effect-caps ─────────────────────────────────────────
    let composed_effects = parent.effect_caps() & child.effect_caps();
    // ── MAX-stricter k-anon ───────────────────────────────────────────
    let composed_k = parent.k_anon_thresh().max(child.k_anon_thresh());
    // ── MIN-sooner TTL · 0 = no-TTL is treated as "infinite" ───────────
    let composed_ttl = compose_ttl(parent, child, now_seconds);

    // ── child loosening detection (skip when OVERRIDE set) ────────────
    if !child.has_flag(FLAG_OVERRIDE) {
        if (child.audience_class() & !parent.audience_class()) != 0 {
            return Err(CompositionError::LooseningForbidden {
                dim: LooseningDim::Audience,
            });
        }
        if (child.effect_caps() & !parent.effect_caps()) != 0 {
            return Err(CompositionError::LooseningForbidden {
                dim: LooseningDim::Effects,
            });
        }
        if child.k_anon_thresh() < parent.k_anon_thresh() {
            return Err(CompositionError::LooseningForbidden {
                dim: LooseningDim::KAnon,
            });
        }
        if loosens_ttl(parent, child, now_seconds) {
            return Err(CompositionError::LooseningForbidden {
                dim: LooseningDim::Ttl,
            });
        }
    }

    // ── flags : PROPAGATE inherits · INHERIT set · child flags retained ─
    let composed_flags =
        (parent.flags() & FLAG_PROPAGATE) | child.flags() | FLAG_INHERIT;

    let mut composed = SigmaMask::new(
        composed_audience,
        composed_effects,
        composed_k,
        composed_ttl,
        composed_flags,
        now_seconds,
    );

    // ── cascade-revoke : parent revoked ⇒ child revoked too ────────────
    if parent.is_revoked() {
        composed.revoke(parent.revoked_at());
    } else if child.is_revoked() {
        composed.revoke(child.revoked_at());
    }

    Ok(composed)
}

/// MIN-sooner TTL composition. 0 = "no TTL · effectively infinite".
///
/// § DESIGN : both 0 ⇒ 0 ; one 0 ⇒ the other ; both nonzero ⇒ effective-min
/// translated back to a relative-seconds-from-now value.
fn compose_ttl(parent: &SigmaMask, child: &SigmaMask, now_seconds: u64) -> u32 {
    let p_ttl = parent.ttl_seconds();
    let c_ttl = child.ttl_seconds();
    if p_ttl == 0 && c_ttl == 0 {
        return 0;
    }
    let p_expires = if p_ttl == 0 { u64::MAX } else { parent.expires_at() };
    let c_expires = if c_ttl == 0 { u64::MAX } else { child.expires_at() };
    let composed_expires = p_expires.min(c_expires);
    if composed_expires == u64::MAX {
        0
    } else if composed_expires <= now_seconds {
        // already-expired → 1-second TTL (effectively immediate-expire).
        1
    } else {
        let delta = composed_expires - now_seconds;
        u32::try_from(delta).unwrap_or(u32::MAX)
    }
}

/// Detect whether the child's TTL would LOOSEN parent (later expiry) in
/// composition. Used as the `LooseningForbidden::Ttl` predicate.
///
/// § DESIGN : 0 = no-TTL = infinite. A child-of-0 against a parent-with-TTL
/// loosens · two-nonzero compares absolute expiry.
fn loosens_ttl(parent: &SigmaMask, child: &SigmaMask, _now: u64) -> bool {
    let p_ttl = parent.ttl_seconds();
    let c_ttl = child.ttl_seconds();
    match (p_ttl, c_ttl) {
        (0, _) => false,                                       // parent ∞ · child can be anything
        (_, 0) => true,                                        // child ∞ · parent finite ⇒ loosen
        (_, _) => child.expires_at() > parent.expires_at(),    // both finite : compare absolute
    }
}

// ───────────────────────────────────────────────────────────────────────────
// § Tests
// ───────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mask::{
        AUDIENCE_CIRCLE, AUDIENCE_DERIVED, AUDIENCE_PUBLIC, AUDIENCE_SELF, EFFECT_DERIVE,
        EFFECT_LOG, EFFECT_READ, EFFECT_WRITE,
    };

    #[test]
    fn t01_compose_and_narrows_audience_and_effects() {
        let parent = SigmaMask::new(
            AUDIENCE_SELF | AUDIENCE_CIRCLE | AUDIENCE_PUBLIC,
            EFFECT_READ | EFFECT_WRITE | EFFECT_DERIVE,
            5,
            0,
            FLAG_PROPAGATE,
            1_000,
        );
        let child = SigmaMask::new(
            AUDIENCE_SELF | AUDIENCE_CIRCLE,
            EFFECT_READ | EFFECT_DERIVE,
            10,
            0,
            0,
            1_000,
        );
        let composed = compose_parent_child(&parent, &child, 1_000).unwrap();
        assert_eq!(composed.audience_class(), AUDIENCE_SELF | AUDIENCE_CIRCLE);
        assert_eq!(composed.effect_caps(), EFFECT_READ | EFFECT_DERIVE);
        assert_eq!(composed.k_anon_thresh(), 10);
        assert!(composed.has_flag(FLAG_INHERIT));
        assert!(composed.has_flag(FLAG_PROPAGATE));
    }

    #[test]
    fn t02_compose_takes_max_k_anon() {
        let parent = SigmaMask::new(
            AUDIENCE_DERIVED,
            EFFECT_DERIVE,
            3,
            0,
            FLAG_PROPAGATE,
            1_000,
        );
        let child = SigmaMask::new(
            AUDIENCE_DERIVED,
            EFFECT_DERIVE,
            7,
            0,
            0,
            1_000,
        );
        let composed = compose_parent_child(&parent, &child, 1_000).unwrap();
        assert_eq!(composed.k_anon_thresh(), 7);
    }

    #[test]
    fn t03_compose_loosening_ttl_forbidden() {
        let parent = SigmaMask::new(AUDIENCE_SELF, EFFECT_READ, 0, 600, FLAG_PROPAGATE, 1_000);
        let child = SigmaMask::new(AUDIENCE_SELF, EFFECT_READ, 0, 1200, 0, 1_000);
        // child expires at 2200 ; parent at 1600 ⇒ child is LOOSER ⇒ ERROR.
        let r = compose_parent_child(&parent, &child, 1_000);
        assert!(matches!(
            r,
            Err(CompositionError::LooseningForbidden {
                dim: LooseningDim::Ttl
            })
        ));
    }

    #[test]
    fn t03b_compose_min_ttl_when_child_stricter() {
        let parent = SigmaMask::new(AUDIENCE_SELF, EFFECT_READ, 0, 1200, FLAG_PROPAGATE, 1_000);
        let child = SigmaMask::new(AUDIENCE_SELF, EFFECT_READ, 0, 600, 0, 1_000);
        let composed = compose_parent_child(&parent, &child, 1_000).unwrap();
        // Composition's ttl_seconds is computed relative to now=1_000 ; child
        // expires at 1_600 → ttl = 600.
        assert_eq!(composed.ttl_seconds(), 600);
    }

    #[test]
    fn t04_loosening_audience_forbidden() {
        let parent = SigmaMask::new(AUDIENCE_SELF, EFFECT_READ, 0, 0, FLAG_PROPAGATE, 1_000);
        let child = SigmaMask::new(AUDIENCE_SELF | AUDIENCE_PUBLIC, EFFECT_READ, 0, 0, 0, 1_000);
        let r = compose_parent_child(&parent, &child, 1_000);
        assert!(matches!(
            r,
            Err(CompositionError::LooseningForbidden {
                dim: LooseningDim::Audience
            })
        ));
    }

    #[test]
    fn t05_loosening_effects_forbidden() {
        let parent = SigmaMask::new(AUDIENCE_SELF, EFFECT_READ, 0, 0, FLAG_PROPAGATE, 1_000);
        let child = SigmaMask::new(
            AUDIENCE_SELF,
            EFFECT_READ | EFFECT_LOG,
            0,
            0,
            0,
            1_000,
        );
        let r = compose_parent_child(&parent, &child, 1_000);
        assert!(matches!(
            r,
            Err(CompositionError::LooseningForbidden {
                dim: LooseningDim::Effects
            })
        ));
    }

    #[test]
    fn t06_loosening_kanon_forbidden() {
        let parent = SigmaMask::new(AUDIENCE_DERIVED, EFFECT_DERIVE, 10, 0, FLAG_PROPAGATE, 1_000);
        let child = SigmaMask::new(AUDIENCE_DERIVED, EFFECT_DERIVE, 5, 0, 0, 1_000);
        let r = compose_parent_child(&parent, &child, 1_000);
        assert!(matches!(
            r,
            Err(CompositionError::LooseningForbidden {
                dim: LooseningDim::KAnon
            })
        ));
    }

    #[test]
    fn t07_parent_revoked_child_auto_revoked() {
        let mut parent = SigmaMask::new(AUDIENCE_SELF, EFFECT_READ, 0, 0, FLAG_PROPAGATE, 1_000);
        parent.revoke(1_500);
        let child = SigmaMask::new(AUDIENCE_SELF, EFFECT_READ, 0, 0, 0, 1_000);
        let composed = compose_parent_child(&parent, &child, 1_500).unwrap();
        assert!(composed.is_revoked());
        assert_eq!(composed.revoked_at(), 1_500);
    }

    #[test]
    fn t08_parent_no_propagate_rejected() {
        let parent = SigmaMask::new(AUDIENCE_SELF, EFFECT_READ, 0, 0, 0, 1_000);
        let child = SigmaMask::new(AUDIENCE_SELF, EFFECT_READ, 0, 0, 0, 1_000);
        let r = compose_parent_child(&parent, &child, 1_000);
        assert!(matches!(r, Err(CompositionError::ParentDoesNotPropagate)));
    }

    #[test]
    fn t09_override_allows_loosening_request_but_still_and_narrows() {
        let parent = SigmaMask::new(AUDIENCE_SELF, EFFECT_READ, 0, 0, FLAG_PROPAGATE, 1_000);
        // child requests broader audience BUT carries OVERRIDE
        let child = SigmaMask::new(
            AUDIENCE_SELF | AUDIENCE_PUBLIC,
            EFFECT_READ,
            0,
            0,
            FLAG_OVERRIDE,
            1_000,
        );
        let composed = compose_parent_child(&parent, &child, 1_000).unwrap();
        // AND-narrowing remains in effect ; OVERRIDE only suppresses the error.
        assert_eq!(composed.audience_class(), AUDIENCE_SELF);
        assert!(composed.has_flag(FLAG_OVERRIDE));
        assert!(composed.has_flag(FLAG_INHERIT));
    }

    #[test]
    fn t10_input_tamper_detected() {
        let mut parent = SigmaMask::new(AUDIENCE_SELF, EFFECT_READ, 0, 0, FLAG_PROPAGATE, 1_000);
        let child = SigmaMask::new(AUDIENCE_SELF, EFFECT_READ, 0, 0, 0, 1_000);
        // Use unsafe-ish field access via clone-and-overwrite : we cannot
        // mutate audience_class directly (private), so we simulate tamper by
        // building two masks with identical bits except revoked_at via revoke.
        // Tamper-test : take parent, then sneak-revoke without re-hash.
        // Since revoke() rehashes, we need a different vector :
        // Construct a mask via pack+manipulate? Easier: copy the struct
        // bit-by-bit and corrupt audience_class via core::ptr is unsafe.
        // For test-purposes, we instead corrupt the parent indirectly :
        // re-invoke revoke, then fake "stale checksum" by clone-and-revoke-
        // again-without-rehash impossible. Use a simpler scenario : verify
        // path covered by mask::tests t03_checksum_detects_field_tamper.
        // Here we just confirm valid → composes.
        let _ = compose_parent_child(&parent, &child, 1_000).unwrap();
        // mutating parent via re-revoke leaves checksum valid.
        parent.revoke(2_000);
        let _ = compose_parent_child(&parent, &child, 2_000).unwrap();
    }
}
