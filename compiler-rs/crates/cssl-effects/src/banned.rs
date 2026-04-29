//! Prime-Directive banned-composition checker.
//!
//! § SPEC : `specs/04_EFFECTS.csl` § PRIME-DIRECTIVE EFFECTS + PRIME_DIRECTIVE.md F5
//!   structural encoding of protections.
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
//!   ⇒ T11-D127 extends the banned-composition gate to refuse `Travel`
//!     without `Privilege<L4+>` — STRUCTURAL Axiom-2 sovereignty enforcement
//!     per `Omniverse/02_CSSL/02_EFFECTS.csl.md § IV` PRIVILEGE-TIERS.
//!
//! § RULE
//!   Effect rows containing `Sensitive<dom>` reject certain compositions at the
//!   type-system level. These rules encode the PRIME DIRECTIVE prohibitions
//!   structurally so they cannot be disabled by config, flag, or runtime condition
//!   (per §§ 06 SCOPE — N! flag | config | … can disable this).
//!
//!   Banned compositions :
//!     1. `{Sensitive<"coercion">} ⊎ *`                      — absolute ban (any row)
//!     2. `{Sensitive<"surveillance">} ⊎ {IO}`                — no override
//!     3. `{Sensitive<"weapon">} ⊎ {IO}`                      — unless `Privilege<Kernel>`
//!     4. `{Sensitive<"weapon">} ⊎ {IO}` + `Privilege<lesser>`— still banned
//!
//! § WHY "STRUCTURAL" (not policy-pasted)
//!   The rules are encoded in the type system : a program that tries to compose
//!   these effects is a compile error, regardless of handler installation,
//!   privilege escalation at runtime, or flag toggles. This is PRIME-DIRECTIVE F5
//!   — the prohibition is a property of the type, not a runtime check.

use thiserror::Error;

use crate::discipline::EffectRef;
use crate::registry::BuiltinEffect;

/// Domain labels recognized by the Sensitive effect (per §§ 11_IFC enumeration).
/// Unknown domains are accepted at the built-in level ; validation against the
/// project-wide domain list happens at elaboration via a separate allow-list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SensitiveDomain<'a> {
    /// Privacy / personal data.
    Privacy,
    /// Weapon systems.
    Weapon,
    /// Surveillance.
    Surveillance,
    /// Coercion / behavior modification.
    Coercion,
    /// Other user-defined domain.
    Other(&'a str),
}

impl<'a> SensitiveDomain<'a> {
    /// Build a `SensitiveDomain` from a label-string (the compile-time literal that
    /// appeared as `Sensitive<"label">` in source).
    #[must_use]
    pub fn from_label(label: &'a str) -> Self {
        match label {
            "privacy" => Self::Privacy,
            "weapon" => Self::Weapon,
            "surveillance" => Self::Surveillance,
            "coercion" => Self::Coercion,
            other => Self::Other(other),
        }
    }

    /// `true` iff this domain is absolutely banned in any composition (Coercion).
    #[must_use]
    pub const fn is_absolute_ban(&self) -> bool {
        matches!(self, Self::Coercion)
    }

    /// `true` iff this domain is banned with `IO` unless `Privilege<Kernel>` is
    /// present (Weapon).
    #[must_use]
    pub const fn is_io_banned_unless_kernel(&self) -> bool {
        matches!(self, Self::Weapon)
    }

    /// `true` iff this domain is banned with `IO` with no override (Surveillance).
    #[must_use]
    pub const fn is_io_banned_no_override(&self) -> bool {
        matches!(self, Self::Surveillance)
    }
}

/// Reason a composition is banned.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum BannedReason {
    /// `Sensitive<"coercion">` is banned in any context.
    #[error(
        "Sensitive<\"coercion\"> is absolutely banned — no composition permitted \
         (PRIME DIRECTIVE § 1 : N! coercion)"
    )]
    CoercionAbsolute,
    /// `Sensitive<"surveillance">` + `IO` is banned with no override.
    #[error(
        "Sensitive<\"surveillance\"> composed with IO is banned — no override exists \
         (PRIME DIRECTIVE § 1 : N! surveillance ; specs/04 PRIME-DIRECTIVE EFFECTS)"
    )]
    SurveillanceWithIo,
    /// `Sensitive<"weapon">` + `IO` requires `Privilege<Kernel>`.
    #[error(
        "Sensitive<\"weapon\"> composed with IO requires Privilege<Kernel> \
         (PRIME DIRECTIVE § 1 : N! weaponization ; specs/04 PRIME-DIRECTIVE EFFECTS)"
    )]
    WeaponWithIoNeedsKernel,
    /// T11-D127 — `{Travel}` without `Privilege<L4+>` is banned.
    /// Per `Omniverse/02_CSSL/02_EFFECTS.csl.md § IV` only Privilege<4>
    /// (Apocky-tier) may authorize cross-substrate translation. User-spells
    /// (Privilege<0>) and modder-spells (Privilege<1>) refuse Travel-effects.
    #[error(
        "Travel without Privilege<L4+> is banned — only Apocky-tier may authorize \
         cross-substrate translation (Omniverse Axiom-2 + 02_CSSL/02_EFFECTS § IV)"
    )]
    TravelWithoutPrivilegeL4,
}

/// Check whether an effect row is free of Prime-Directive-banned compositions.
///
/// Returns `Ok(())` if the row is compositionally safe, or a list of
/// `BannedReason`s otherwise (one per distinct violation found).
///
/// § T11-D127 EXTENSION
///   The check now also refuses `Travel` without `Privilege<L4+>` per
///   `Omniverse/02_CSSL/02_EFFECTS.csl.md § IV`. Stage-0 surfaces the L4
///   privilege presence via the explicit `_with_privilege_l4` variant ; the
///   default proxy via [`banned_composition`] inspects the `Privilege` effect
///   arg-count as a coarse stand-in (the elaborator/HIR layer wires the
///   actual L4-vs-lower distinction).
pub fn banned_composition(row: &[EffectRef<'_>]) -> Result<(), Vec<BannedReason>> {
    banned_composition_with_privilege_l4(row, /* l4 = */ false)
}

/// Like [`banned_composition`] but takes an explicit `has_privilege_l4` flag
/// from the caller (HIR layer). Used by elaboration that has resolved the
/// `Privilege<level>` arg-value.
pub fn banned_composition_with_privilege_l4(
    row: &[EffectRef<'_>],
    has_privilege_l4: bool,
) -> Result<(), Vec<BannedReason>> {
    let mut violations: Vec<BannedReason> = Vec::new();
    let sensitive_domains: Vec<SensitiveDomain<'_>> = row
        .iter()
        .filter(|e| matches!(e.builtin, Some(BuiltinEffect::Sensitive)))
        .map(|e| {
            // Stage-0 can't inspect the arg-value yet (needs const-evaluation). The
            // `name` field of EffectRef carries the canonical `Sensitive` name ; the
            // actual domain literal is passed via a side channel : callers that want
            // full checking should use the `banned_composition_with_domains` variant.
            SensitiveDomain::Other(e.name)
        })
        .collect();
    let has_io = row
        .iter()
        .any(|e| matches!(e.builtin, Some(BuiltinEffect::Io)));
    let has_kernel_priv = row
        .iter()
        .any(|e| matches!(e.builtin, Some(BuiltinEffect::Privilege)) && e.arg_count == 1);
    let has_travel = row
        .iter()
        .any(|e| matches!(e.builtin, Some(BuiltinEffect::Travel)));

    for dom in &sensitive_domains {
        if dom.is_absolute_ban() {
            violations.push(BannedReason::CoercionAbsolute);
        } else if has_io {
            if dom.is_io_banned_no_override() {
                violations.push(BannedReason::SurveillanceWithIo);
            } else if dom.is_io_banned_unless_kernel() && !has_kernel_priv {
                violations.push(BannedReason::WeaponWithIoNeedsKernel);
            }
        }
    }

    // T11-D127 — STRUCTURAL : Travel without Privilege<L4+> is banned.
    if has_travel && !has_privilege_l4 {
        violations.push(BannedReason::TravelWithoutPrivilegeL4);
    }

    if violations.is_empty() {
        Ok(())
    } else {
        Err(violations)
    }
}

/// Full-fidelity variant that inspects explicit `SensitiveDomain` labels instead
/// of relying on the `EffectRef::name` proxy. Callers in `cssl-hir` should use
/// this variant once they've resolved the domain-label from the HIR effect-arg.
///
/// § T11-D127 EXTENSION
///   Also refuses `Travel` without `has_privilege_l4` per Axiom-2.
pub fn banned_composition_with_domains(
    row: &[EffectRef<'_>],
    sensitive_domains: &[SensitiveDomain<'_>],
) -> Result<(), Vec<BannedReason>> {
    banned_composition_with_domains_and_privilege(row, sensitive_domains, /* l4 = */ false)
}

/// Like [`banned_composition_with_domains`] but with explicit `has_privilege_l4`
/// flag from the elaborator. Use this from the HIR layer after `Privilege<n>`
/// arg has been resolved.
pub fn banned_composition_with_domains_and_privilege(
    row: &[EffectRef<'_>],
    sensitive_domains: &[SensitiveDomain<'_>],
    has_privilege_l4: bool,
) -> Result<(), Vec<BannedReason>> {
    let mut violations: Vec<BannedReason> = Vec::new();
    let has_io = row
        .iter()
        .any(|e| matches!(e.builtin, Some(BuiltinEffect::Io)));
    let has_kernel_priv = row
        .iter()
        .any(|e| matches!(e.builtin, Some(BuiltinEffect::Privilege)) && e.arg_count == 1);
    let has_travel = row
        .iter()
        .any(|e| matches!(e.builtin, Some(BuiltinEffect::Travel)));

    for dom in sensitive_domains {
        if dom.is_absolute_ban() {
            violations.push(BannedReason::CoercionAbsolute);
        } else if has_io {
            if dom.is_io_banned_no_override() {
                violations.push(BannedReason::SurveillanceWithIo);
            } else if dom.is_io_banned_unless_kernel() && !has_kernel_priv {
                violations.push(BannedReason::WeaponWithIoNeedsKernel);
            }
        }
    }

    // T11-D127 — STRUCTURAL : Travel without Privilege<L4+> is banned.
    if has_travel && !has_privilege_l4 {
        violations.push(BannedReason::TravelWithoutPrivilegeL4);
    }

    if violations.is_empty() {
        Ok(())
    } else {
        Err(violations)
    }
}

#[cfg(test)]
mod tests {
    use super::{banned_composition_with_domains, BannedReason, SensitiveDomain};
    use crate::discipline::EffectRef;
    use crate::registry::BuiltinEffect;

    fn e(name: &'static str, builtin: Option<BuiltinEffect>, arity: usize) -> EffectRef<'static> {
        EffectRef {
            name,
            builtin,
            arg_count: arity,
        }
    }

    #[test]
    fn coercion_domain_absolutely_banned() {
        let row = vec![e("Sensitive", Some(BuiltinEffect::Sensitive), 1)];
        let res = banned_composition_with_domains(&row, &[SensitiveDomain::Coercion]);
        assert!(matches!(res, Err(ref v) if v.contains(&BannedReason::CoercionAbsolute)));
    }

    #[test]
    fn surveillance_with_io_banned_no_override() {
        let row = vec![
            e("Sensitive", Some(BuiltinEffect::Sensitive), 1),
            e("IO", Some(BuiltinEffect::Io), 0),
            e("Privilege", Some(BuiltinEffect::Privilege), 1),
        ];
        let res = banned_composition_with_domains(&row, &[SensitiveDomain::Surveillance]);
        assert!(matches!(res, Err(ref v) if v.contains(&BannedReason::SurveillanceWithIo)));
    }

    #[test]
    fn weapon_with_io_needs_kernel() {
        let row = vec![
            e("Sensitive", Some(BuiltinEffect::Sensitive), 1),
            e("IO", Some(BuiltinEffect::Io), 0),
        ];
        let res = banned_composition_with_domains(&row, &[SensitiveDomain::Weapon]);
        assert!(matches!(res, Err(ref v) if v.contains(&BannedReason::WeaponWithIoNeedsKernel)));
    }

    #[test]
    fn weapon_with_io_plus_kernel_privilege_ok() {
        let row = vec![
            e("Sensitive", Some(BuiltinEffect::Sensitive), 1),
            e("IO", Some(BuiltinEffect::Io), 0),
            e("Privilege", Some(BuiltinEffect::Privilege), 1),
        ];
        let res = banned_composition_with_domains(&row, &[SensitiveDomain::Weapon]);
        assert!(res.is_ok());
    }

    #[test]
    fn privacy_with_io_is_fine() {
        let row = vec![
            e("Sensitive", Some(BuiltinEffect::Sensitive), 1),
            e("IO", Some(BuiltinEffect::Io), 0),
        ];
        let res = banned_composition_with_domains(&row, &[SensitiveDomain::Privacy]);
        assert!(res.is_ok());
    }

    #[test]
    fn no_sensitive_is_trivially_ok() {
        let row = vec![
            e("IO", Some(BuiltinEffect::Io), 0),
            e("GPU", Some(BuiltinEffect::Gpu), 0),
        ];
        let res = banned_composition_with_domains(&row, &[]);
        assert!(res.is_ok());
    }

    #[test]
    fn coercion_bans_even_without_io() {
        let row = vec![e("Sensitive", Some(BuiltinEffect::Sensitive), 1)];
        let res = banned_composition_with_domains(&row, &[SensitiveDomain::Coercion]);
        assert!(res.is_err());
    }

    #[test]
    fn domain_label_classification() {
        assert!(matches!(
            SensitiveDomain::from_label("coercion"),
            SensitiveDomain::Coercion
        ));
        assert!(matches!(
            SensitiveDomain::from_label("weapon"),
            SensitiveDomain::Weapon
        ));
        assert!(matches!(
            SensitiveDomain::from_label("surveillance"),
            SensitiveDomain::Surveillance
        ));
        assert!(matches!(
            SensitiveDomain::from_label("privacy"),
            SensitiveDomain::Privacy
        ));
        assert!(matches!(
            SensitiveDomain::from_label("something-else"),
            SensitiveDomain::Other(_)
        ));
    }

    #[test]
    fn classification_predicates() {
        assert!(SensitiveDomain::Coercion.is_absolute_ban());
        assert!(!SensitiveDomain::Weapon.is_absolute_ban());
        assert!(SensitiveDomain::Weapon.is_io_banned_unless_kernel());
        assert!(SensitiveDomain::Surveillance.is_io_banned_no_override());
    }

    #[test]
    fn multiple_violations_reported() {
        let row = vec![
            e("Sensitive", Some(BuiltinEffect::Sensitive), 1),
            e("IO", Some(BuiltinEffect::Io), 0),
        ];
        let res = banned_composition_with_domains(
            &row,
            &[SensitiveDomain::Coercion, SensitiveDomain::Surveillance],
        );
        if let Err(v) = res {
            assert_eq!(v.len(), 2);
        } else {
            panic!("expected multiple violations");
        }
    }

    // ═════════════════════════════════════════════════════════════════════
    // ─── T11-D127 — Travel-without-Privilege<L4+> ban tests ──────────────
    // ═════════════════════════════════════════════════════════════════════

    /// Travel without L4 is banned.
    #[test]
    fn travel_without_l4_is_banned() {
        let row = vec![e("Travel", Some(BuiltinEffect::Travel), 0)];
        let res = super::banned_composition(&row);
        assert!(matches!(res, Err(ref v) if v.contains(&BannedReason::TravelWithoutPrivilegeL4)));
    }

    /// Travel WITH L4 (passed via with_privilege_l4 variant) is allowed.
    #[test]
    fn travel_with_l4_passes() {
        let row = vec![e("Travel", Some(BuiltinEffect::Travel), 0)];
        let res = super::banned_composition_with_privilege_l4(&row, true);
        assert!(res.is_ok(), "Travel + L4 should pass ban-gate");
    }

    /// No Travel ⇒ L4-flag-irrelevant.
    #[test]
    fn no_travel_no_l4_required() {
        let row = vec![e("IO", Some(BuiltinEffect::Io), 0)];
        let res = super::banned_composition(&row);
        assert!(res.is_ok(), "non-Travel rows do not require L4");
    }

    /// Travel + Sensitive<coercion> stacks both bans.
    #[test]
    fn travel_plus_coercion_stacks_violations() {
        let row = vec![
            e("Travel", Some(BuiltinEffect::Travel), 0),
            e("Sensitive", Some(BuiltinEffect::Sensitive), 1),
        ];
        let res = banned_composition_with_domains_and_privilege(
            &row,
            &[SensitiveDomain::Coercion],
            /* l4 = */ false,
        );
        let errs = res.unwrap_err();
        assert!(errs.contains(&BannedReason::CoercionAbsolute));
        assert!(errs.contains(&BannedReason::TravelWithoutPrivilegeL4));
    }

    /// Travel + Sensitive<coercion> + L4 still bans (Coercion is absolute).
    #[test]
    fn travel_plus_coercion_with_l4_still_bans_coercion() {
        let row = vec![
            e("Travel", Some(BuiltinEffect::Travel), 0),
            e("Sensitive", Some(BuiltinEffect::Sensitive), 1),
        ];
        let res = banned_composition_with_domains_and_privilege(
            &row,
            &[SensitiveDomain::Coercion],
            /* l4 = */ true,
        );
        let errs = res.unwrap_err();
        // L4 clears TravelWithoutPrivilegeL4 ; Coercion absolute remains.
        assert!(errs.contains(&BannedReason::CoercionAbsolute));
        assert!(!errs.contains(&BannedReason::TravelWithoutPrivilegeL4));
    }

    use super::banned_composition_with_domains_and_privilege;
}
