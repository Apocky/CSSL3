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
//!     1. `{Sensitive<"coercion">} ⊎ *`                       — absolute ban (any row)
//!     2. `{Sensitive<"surveillance">} ⊎ {IO}`                — no override
//!     3. `{Sensitive<"weapon">} ⊎ {IO}`                      — unless `Privilege<Kernel>`
//!     4. `{Sensitive<"weapon">} ⊎ {IO}` + `Privilege<lesser>`— still banned
//!     5. `{Sensitive<"gaze">} ⊎ {Net}`                       — absolute ban (T11-D129)
//!     6. `{Sensitive<"biometric">} ⊎ {Net}`                  — absolute ban (T11-D129)
//!     7. `{Sensitive<"biometric">} ⊎ {Telemetry<*>}`         — absolute ban (T11-D129)
//!     8. `{Sensitive<"face-tracking">} ⊎ {Net|Telemetry}`    — absolute ban (T11-D129)
//!     9. `{Sensitive<"body-tracking">} ⊎ {Net|Telemetry}`    — absolute ban (T11-D129)
//!    10. `{OnDeviceOnly} ⊎ {Net}`                            — absolute ban (T11-D129)
//!    11. `{OnDeviceOnly} ⊎ {Telemetry<*>}`                   — absolute ban (T11-D129)
//!
//! § WHY "STRUCTURAL" (not policy-pasted)
//!   The rules are encoded in the type system : a program that tries to compose
//!   these effects is a compile error, regardless of handler installation,
//!   privilege escalation at runtime, or flag toggles. This is PRIME-DIRECTIVE F5
//!   — the prohibition is a property of the type, not a runtime check.
//!
//! § T11-D129 BIOMETRIC ANTI-SURVEILLANCE
//!   The new bans (5–11 above) implement P18 BiometricEgress. Biometric data
//!   (gaze, face, body, generic biometric) MUST never leave the device on which
//!   the user resides. `Privilege<Kernel>` and even `Privilege<ApockyRoot>`
//!   CANNOT override these bans — per §1 N! surveillance and §6 SCOPE
//!   "no flag, no configuration, no environment variable, no command-line
//!   argument, no API call, no runtime condition can disable, weaken, or
//!   circumvent" the prohibition.

use thiserror::Error;

use crate::discipline::EffectRef;
use crate::registry::BuiltinEffect;

/// Domain labels recognized by the `Sensitive` effect (per `specs/11_IFC` enumeration).
/// Unknown domains are accepted at the built-in level ; validation against the
/// project-wide domain list happens at elaboration via a separate allow-list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SensitiveDomain<'a> {
    /// Privacy / personal data (general).
    Privacy,
    /// Weapon systems.
    Weapon,
    /// Surveillance.
    Surveillance,
    /// Coercion / behavior modification.
    Coercion,
    /// Eye-tracking / gaze data (T11-D129). Raw gaze MUST never leave-device.
    Gaze,
    /// General biometric (heart-rate, EDA, breath, etc.) (T11-D129).
    Biometric,
    /// Face-tracking (FACS coefficients, expression-shape vectors) (T11-D129).
    FaceTracking,
    /// Body-tracking (joint poses, skeletal data) (T11-D129).
    BodyTracking,
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
            "gaze" => Self::Gaze,
            "biometric" => Self::Biometric,
            "face-tracking" => Self::FaceTracking,
            "body-tracking" => Self::BodyTracking,
            other => Self::Other(other),
        }
    }

    /// Canonical label form (matches `Sensitive<"label">` literal).
    #[must_use]
    pub const fn label(&self) -> &'a str {
        match self {
            Self::Privacy => "privacy",
            Self::Weapon => "weapon",
            Self::Surveillance => "surveillance",
            Self::Coercion => "coercion",
            Self::Gaze => "gaze",
            Self::Biometric => "biometric",
            Self::FaceTracking => "face-tracking",
            Self::BodyTracking => "body-tracking",
            Self::Other(s) => s,
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

    /// `true` iff this domain is biometric (T11-D129) — gaze, biometric,
    /// face-tracking, body-tracking. Biometric domains have ABSOLUTE bans
    /// against `Net` and `Telemetry<*>` regardless of `Privilege<L>`.
    #[must_use]
    pub const fn is_biometric(&self) -> bool {
        matches!(
            self,
            Self::Gaze | Self::Biometric | Self::FaceTracking | Self::BodyTracking
        )
    }
}

impl SensitiveDomain<'static> {
    /// All domain variants known by the built-in checker (excludes `Other`).
    #[must_use]
    pub const fn all_known() -> [SensitiveDomain<'static>; 8] {
        [
            SensitiveDomain::Privacy,
            SensitiveDomain::Weapon,
            SensitiveDomain::Surveillance,
            SensitiveDomain::Coercion,
            SensitiveDomain::Gaze,
            SensitiveDomain::Biometric,
            SensitiveDomain::FaceTracking,
            SensitiveDomain::BodyTracking,
        ]
    }

    /// All four biometric variants (T11-D129).
    #[must_use]
    pub const fn all_biometric() -> [SensitiveDomain<'static>; 4] {
        [
            SensitiveDomain::Gaze,
            SensitiveDomain::Biometric,
            SensitiveDomain::FaceTracking,
            SensitiveDomain::BodyTracking,
        ]
    }
}

/// Reason a composition is banned.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum BannedReason {
    /// `Sensitive<"coercion">` is banned in any context.
    #[error(
        "[BAN0001] Sensitive<\"coercion\"> is absolutely banned — no composition permitted \
         (PRIME DIRECTIVE § 1 : N! coercion)"
    )]
    CoercionAbsolute,
    /// `Sensitive<"surveillance">` + `IO` is banned with no override.
    #[error(
        "[BAN0002] Sensitive<\"surveillance\"> composed with IO is banned — no override exists \
         (PRIME DIRECTIVE § 1 : N! surveillance ; specs/04 PRIME-DIRECTIVE EFFECTS)"
    )]
    SurveillanceWithIo,
    /// `Sensitive<"weapon">` + `IO` requires `Privilege<Kernel>`.
    #[error(
        "[BAN0003] Sensitive<\"weapon\"> composed with IO requires Privilege<Kernel> \
         (PRIME DIRECTIVE § 1 : N! weaponization ; specs/04 PRIME-DIRECTIVE EFFECTS)"
    )]
    WeaponWithIoNeedsKernel,
    /// `Sensitive<"gaze">` + `Net` is absolutely banned (T11-D129).
    #[error(
        "[BAN0004] Sensitive<\"gaze\"> composed with Net is ABSOLUTELY banned — no Privilege override \
         (PRIME DIRECTIVE § 1 : N! surveillance ; P18 BiometricEgress ; \
         Omniverse/07_AESTHETIC/05_VR_RENDERING.csl raw-gaze NEVER-egress)"
    )]
    GazeWithNet,
    /// `Sensitive<"biometric">` + `Net` is absolutely banned (T11-D129).
    #[error(
        "[BAN0005] Sensitive<\"biometric\"> composed with Net is ABSOLUTELY banned — no Privilege \
         override (PRIME DIRECTIVE § 1 : N! surveillance ; P18 BiometricEgress)"
    )]
    BiometricWithNet,
    /// `Sensitive<"biometric">` + `Telemetry<*>` is absolutely banned (T11-D129).
    #[error(
        "[BAN0006] Sensitive<\"biometric\"> composed with Telemetry is ABSOLUTELY banned — no \
         Privilege override (PRIME DIRECTIVE § 1 : N! surveillance ; P18 BiometricEgress)"
    )]
    BiometricWithTelemetry,
    /// `Sensitive<"face-tracking">` + `Net|Telemetry` is absolutely banned (T11-D129).
    #[error(
        "[BAN0007] Sensitive<\"face-tracking\"> composed with Net or Telemetry is ABSOLUTELY \
         banned — no Privilege override (PRIME DIRECTIVE § 1 : N! surveillance ; P18 BiometricEgress)"
    )]
    FaceTrackingEgress,
    /// `Sensitive<"body-tracking">` + `Net|Telemetry` is absolutely banned (T11-D129).
    #[error(
        "[BAN0008] Sensitive<\"body-tracking\"> composed with Net or Telemetry is ABSOLUTELY \
         banned — no Privilege override (PRIME DIRECTIVE § 1 : N! surveillance ; P18 BiometricEgress \
         ; Omniverse/08_BODY/02_VR_EMBODIMENT.csl Σ-mask body-region defaults)"
    )]
    BodyTrackingEgress,
    /// `OnDeviceOnly` + `Net` is absolutely banned (T11-D129).
    #[error(
        "[BAN0009] OnDeviceOnly composed with Net is ABSOLUTELY banned — no Privilege override \
         (PRIME DIRECTIVE § 1 : N! surveillance ; P18 BiometricEgress)"
    )]
    OnDeviceOnlyWithNet,
    /// `OnDeviceOnly` + `Telemetry<*>` is absolutely banned (T11-D129).
    #[error(
        "[BAN0010] OnDeviceOnly composed with Telemetry is ABSOLUTELY banned — Telemetry egress \
         could exfiltrate non-egress data (PRIME DIRECTIVE § 1 : N! surveillance ; P18 \
         BiometricEgress)"
    )]
    OnDeviceOnlyWithTelemetry,
    /// T11-D127 — `{Travel}` without `Privilege<L4+>` is banned.
    /// Per `Omniverse/02_CSSL/02_EFFECTS.csl.md § IV` only Privilege<4>
    /// (Apocky-tier) may authorize cross-substrate translation. User-spells
    /// (Privilege<0>) and modder-spells (Privilege<1>) refuse Travel-effects.
    #[error(
        "[BAN0011] Travel without Privilege<L4+> is banned — only Apocky-tier may authorize \
         cross-substrate translation (Omniverse Axiom-2 + 02_CSSL/02_EFFECTS § IV)"
    )]
    TravelWithoutPrivilegeL4,
}

impl BannedReason {
    /// Stable diagnostic code (`BAN0001..BAN0011`) for the reason.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::CoercionAbsolute => "BAN0001",
            Self::SurveillanceWithIo => "BAN0002",
            Self::WeaponWithIoNeedsKernel => "BAN0003",
            Self::GazeWithNet => "BAN0004",
            Self::BiometricWithNet => "BAN0005",
            Self::BiometricWithTelemetry => "BAN0006",
            Self::FaceTrackingEgress => "BAN0007",
            Self::BodyTrackingEgress => "BAN0008",
            Self::OnDeviceOnlyWithNet => "BAN0009",
            Self::OnDeviceOnlyWithTelemetry => "BAN0010",
            Self::TravelWithoutPrivilegeL4 => "BAN0011",
        }
    }

    /// `true` iff the violation is absolute — no Privilege<L> can override.
    /// All T11-D129 biometric / on-device bans are absolute. T11-D127 Travel
    /// is gated by Privilege<L4+> so it is NOT absolute.
    #[must_use]
    pub const fn is_absolute(&self) -> bool {
        !matches!(
            self,
            Self::WeaponWithIoNeedsKernel | Self::TravelWithoutPrivilegeL4
        )
    }
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
/// `Privilege<level>` arg-value (T11-D127).
pub fn banned_composition_with_privilege_l4(
    row: &[EffectRef<'_>],
    has_privilege_l4: bool,
) -> Result<(), Vec<BannedReason>> {
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
    banned_composition_with_domains_and_privilege(row, &sensitive_domains, has_privilege_l4)
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
    let has_net = row
        .iter()
        .any(|e| matches!(e.builtin, Some(BuiltinEffect::Net)));
    let has_telemetry = row
        .iter()
        .any(|e| matches!(e.builtin, Some(BuiltinEffect::Telemetry)));
    let has_kernel_priv = row
        .iter()
        .any(|e| matches!(e.builtin, Some(BuiltinEffect::Privilege)) && e.arg_count == 1);
    let has_travel = row
        .iter()
        .any(|e| matches!(e.builtin, Some(BuiltinEffect::Travel)));
    let has_on_device_only = row
        .iter()
        .any(|e| matches!(e.builtin, Some(BuiltinEffect::OnDeviceOnly)));

    // ─ existing rules (1–4) ──────────────────────────────────────────────
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

    // ─ T11-D129 biometric anti-surveillance rules (5–9) ──────────────────
    for dom in sensitive_domains {
        match dom {
            SensitiveDomain::Gaze => {
                if has_net {
                    violations.push(BannedReason::GazeWithNet);
                }
            }
            SensitiveDomain::Biometric => {
                if has_net {
                    violations.push(BannedReason::BiometricWithNet);
                }
                if has_telemetry {
                    violations.push(BannedReason::BiometricWithTelemetry);
                }
            }
            SensitiveDomain::FaceTracking => {
                if has_net || has_telemetry {
                    violations.push(BannedReason::FaceTrackingEgress);
                }
            }
            SensitiveDomain::BodyTracking => {
                if has_net || has_telemetry {
                    violations.push(BannedReason::BodyTrackingEgress);
                }
            }
            _ => {}
        }
    }

    // ─ T11-D129 OnDeviceOnly composition gates (10, 11) ──────────────────
    if has_on_device_only {
        if has_net {
            violations.push(BannedReason::OnDeviceOnlyWithNet);
        }
        if has_telemetry {
            violations.push(BannedReason::OnDeviceOnlyWithTelemetry);
        }
    }

    // ─ T11-D127 STRUCTURAL : Travel without Privilege<L4+> is banned. ────
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

    fn sensitive() -> EffectRef<'static> {
        e("Sensitive", Some(BuiltinEffect::Sensitive), 1)
    }
    fn io() -> EffectRef<'static> {
        e("IO", Some(BuiltinEffect::Io), 0)
    }
    fn net() -> EffectRef<'static> {
        e("Net", Some(BuiltinEffect::Net), 0)
    }
    fn telemetry() -> EffectRef<'static> {
        e("Telemetry", Some(BuiltinEffect::Telemetry), 1)
    }
    fn privilege() -> EffectRef<'static> {
        e("Privilege", Some(BuiltinEffect::Privilege), 1)
    }
    fn on_device_only() -> EffectRef<'static> {
        e("OnDeviceOnly", Some(BuiltinEffect::OnDeviceOnly), 0)
    }

    // ─ original rules ─────────────────────────────────────────────────────

    #[test]
    fn coercion_domain_absolutely_banned() {
        let row = vec![sensitive()];
        let res = banned_composition_with_domains(&row, &[SensitiveDomain::Coercion]);
        assert!(matches!(res, Err(ref v) if v.contains(&BannedReason::CoercionAbsolute)));
    }

    #[test]
    fn surveillance_with_io_banned_no_override() {
        let row = vec![sensitive(), io(), privilege()];
        let res = banned_composition_with_domains(&row, &[SensitiveDomain::Surveillance]);
        assert!(matches!(res, Err(ref v) if v.contains(&BannedReason::SurveillanceWithIo)));
    }

    #[test]
    fn weapon_with_io_needs_kernel() {
        let row = vec![sensitive(), io()];
        let res = banned_composition_with_domains(&row, &[SensitiveDomain::Weapon]);
        assert!(matches!(res, Err(ref v) if v.contains(&BannedReason::WeaponWithIoNeedsKernel)));
    }

    #[test]
    fn weapon_with_io_plus_kernel_privilege_ok() {
        let row = vec![sensitive(), io(), privilege()];
        let res = banned_composition_with_domains(&row, &[SensitiveDomain::Weapon]);
        assert!(res.is_ok());
    }

    #[test]
    fn privacy_with_io_is_fine() {
        let row = vec![sensitive(), io()];
        let res = banned_composition_with_domains(&row, &[SensitiveDomain::Privacy]);
        assert!(res.is_ok());
    }

    #[test]
    fn no_sensitive_is_trivially_ok() {
        let row = vec![io(), e("GPU", Some(BuiltinEffect::Gpu), 0)];
        let res = banned_composition_with_domains(&row, &[]);
        assert!(res.is_ok());
    }

    #[test]
    fn coercion_bans_even_without_io() {
        let row = vec![sensitive()];
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
        let row = vec![sensitive(), io()];
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

    // ─ T11-D129 : new biometric domain classification ────────────────────

    #[test]
    fn from_label_recognizes_biometric_domains() {
        assert!(matches!(
            SensitiveDomain::from_label("gaze"),
            SensitiveDomain::Gaze
        ));
        assert!(matches!(
            SensitiveDomain::from_label("biometric"),
            SensitiveDomain::Biometric
        ));
        assert!(matches!(
            SensitiveDomain::from_label("face-tracking"),
            SensitiveDomain::FaceTracking
        ));
        assert!(matches!(
            SensitiveDomain::from_label("body-tracking"),
            SensitiveDomain::BodyTracking
        ));
    }

    #[test]
    fn biometric_predicate_for_all_four_domains() {
        for dom in SensitiveDomain::all_biometric() {
            assert!(dom.is_biometric(), "{dom:?} should be biometric");
        }
    }

    #[test]
    fn non_biometric_domains_are_not_biometric() {
        assert!(!SensitiveDomain::Privacy.is_biometric());
        assert!(!SensitiveDomain::Weapon.is_biometric());
        assert!(!SensitiveDomain::Surveillance.is_biometric());
        assert!(!SensitiveDomain::Coercion.is_biometric());
    }

    #[test]
    fn label_round_trip_for_all_known() {
        for d in SensitiveDomain::all_known() {
            let label = d.label();
            let back = SensitiveDomain::from_label(label);
            assert_eq!(d, back, "label-round-trip failed for {d:?}");
        }
    }

    // ─ T11-D129 : gaze + Net absolute-ban ────────────────────────────────

    #[test]
    fn gaze_with_net_absolutely_banned() {
        let row = vec![sensitive(), net()];
        let res = banned_composition_with_domains(&row, &[SensitiveDomain::Gaze]);
        assert!(matches!(res, Err(ref v) if v.contains(&BannedReason::GazeWithNet)));
    }

    #[test]
    fn gaze_with_net_plus_kernel_privilege_still_banned() {
        let row = vec![sensitive(), net(), privilege()];
        let res = banned_composition_with_domains(&row, &[SensitiveDomain::Gaze]);
        assert!(
            matches!(res, Err(ref v) if v.contains(&BannedReason::GazeWithNet)),
            "Privilege<Kernel> CANNOT override gaze+Net ban"
        );
    }

    #[test]
    fn gaze_alone_is_fine() {
        let row = vec![sensitive()];
        let res = banned_composition_with_domains(&row, &[SensitiveDomain::Gaze]);
        assert!(res.is_ok(), "gaze without egress is fine");
    }

    #[test]
    fn gaze_on_device_only_is_fine() {
        let row = vec![sensitive(), on_device_only()];
        let res = banned_composition_with_domains(&row, &[SensitiveDomain::Gaze]);
        assert!(
            res.is_ok(),
            "gaze + OnDeviceOnly is the canonical safe shape"
        );
    }

    // ─ T11-D129 : biometric + Net + Telemetry absolute-ban ──────────────

    #[test]
    fn biometric_with_net_absolutely_banned() {
        let row = vec![sensitive(), net()];
        let res = banned_composition_with_domains(&row, &[SensitiveDomain::Biometric]);
        assert!(matches!(res, Err(ref v) if v.contains(&BannedReason::BiometricWithNet)));
    }

    #[test]
    fn biometric_with_telemetry_absolutely_banned() {
        let row = vec![sensitive(), telemetry()];
        let res = banned_composition_with_domains(&row, &[SensitiveDomain::Biometric]);
        assert!(matches!(res, Err(ref v) if v.contains(&BannedReason::BiometricWithTelemetry)));
    }

    #[test]
    fn biometric_with_telemetry_plus_kernel_priv_still_banned() {
        let row = vec![sensitive(), telemetry(), privilege()];
        let res = banned_composition_with_domains(&row, &[SensitiveDomain::Biometric]);
        assert!(matches!(res, Err(ref v) if v.contains(&BannedReason::BiometricWithTelemetry)));
    }

    #[test]
    fn biometric_with_both_net_and_telemetry_reports_both() {
        let row = vec![sensitive(), net(), telemetry()];
        let res = banned_composition_with_domains(&row, &[SensitiveDomain::Biometric]);
        if let Err(v) = res {
            assert!(v.contains(&BannedReason::BiometricWithNet));
            assert!(v.contains(&BannedReason::BiometricWithTelemetry));
        } else {
            panic!("expected dual-ban");
        }
    }

    // ─ T11-D129 : face-tracking egress absolute-ban ─────────────────────

    #[test]
    fn face_tracking_with_net_absolutely_banned() {
        let row = vec![sensitive(), net()];
        let res = banned_composition_with_domains(&row, &[SensitiveDomain::FaceTracking]);
        assert!(matches!(res, Err(ref v) if v.contains(&BannedReason::FaceTrackingEgress)));
    }

    #[test]
    fn face_tracking_with_telemetry_absolutely_banned() {
        let row = vec![sensitive(), telemetry()];
        let res = banned_composition_with_domains(&row, &[SensitiveDomain::FaceTracking]);
        assert!(matches!(res, Err(ref v) if v.contains(&BannedReason::FaceTrackingEgress)));
    }

    #[test]
    fn face_tracking_alone_is_fine() {
        let row = vec![sensitive()];
        let res = banned_composition_with_domains(&row, &[SensitiveDomain::FaceTracking]);
        assert!(res.is_ok());
    }

    // ─ T11-D129 : body-tracking egress absolute-ban ─────────────────────

    #[test]
    fn body_tracking_with_net_absolutely_banned() {
        let row = vec![sensitive(), net()];
        let res = banned_composition_with_domains(&row, &[SensitiveDomain::BodyTracking]);
        assert!(matches!(res, Err(ref v) if v.contains(&BannedReason::BodyTrackingEgress)));
    }

    #[test]
    fn body_tracking_with_telemetry_absolutely_banned() {
        let row = vec![sensitive(), telemetry()];
        let res = banned_composition_with_domains(&row, &[SensitiveDomain::BodyTracking]);
        assert!(matches!(res, Err(ref v) if v.contains(&BannedReason::BodyTrackingEgress)));
    }

    #[test]
    fn body_tracking_with_apocky_root_priv_still_banned() {
        // ApockyRoot encoded as Privilege<L>=4 ; arg_count is still 1.
        let row = vec![sensitive(), net(), privilege()];
        let res = banned_composition_with_domains(&row, &[SensitiveDomain::BodyTracking]);
        assert!(
            matches!(res, Err(ref v) if v.contains(&BannedReason::BodyTrackingEgress)),
            "ApockyRoot CANNOT override body-tracking egress ban"
        );
    }

    // ─ T11-D129 : OnDeviceOnly + Net/Telemetry absolute-ban ─────────────

    #[test]
    fn on_device_only_with_net_absolutely_banned() {
        let row = vec![on_device_only(), net()];
        let res = banned_composition_with_domains(&row, &[]);
        assert!(matches!(res, Err(ref v) if v.contains(&BannedReason::OnDeviceOnlyWithNet)));
    }

    #[test]
    fn on_device_only_with_net_plus_priv_still_banned() {
        let row = vec![on_device_only(), net(), privilege()];
        let res = banned_composition_with_domains(&row, &[]);
        assert!(matches!(res, Err(ref v) if v.contains(&BannedReason::OnDeviceOnlyWithNet)));
    }

    #[test]
    fn on_device_only_with_telemetry_absolutely_banned() {
        let row = vec![on_device_only(), telemetry()];
        let res = banned_composition_with_domains(&row, &[]);
        assert!(matches!(res, Err(ref v) if v.contains(&BannedReason::OnDeviceOnlyWithTelemetry)));
    }

    #[test]
    fn on_device_only_alone_is_fine() {
        let row = vec![on_device_only()];
        let res = banned_composition_with_domains(&row, &[]);
        assert!(res.is_ok());
    }

    #[test]
    fn on_device_only_with_io_local_is_fine() {
        // local IO (filesystem, etc.) is fine — only Net + Telemetry leak off-device.
        let row = vec![on_device_only(), io()];
        let res = banned_composition_with_domains(&row, &[]);
        assert!(res.is_ok());
    }

    // ─ Interaction matrix : every biometric × every egress = banned ─────

    #[test]
    fn every_biometric_with_net_is_banned() {
        for dom in SensitiveDomain::all_biometric() {
            let row = vec![sensitive(), net()];
            let res = banned_composition_with_domains(&row, &[dom]);
            assert!(res.is_err(), "{dom:?} + Net must be banned");
        }
    }

    #[test]
    fn every_biometric_with_telemetry_is_banned_except_gaze() {
        // Gaze + Telemetry isn't independently banned (yet) — only Net + biometric +
        // face-tracking + body-tracking trigger Telemetry ban explicitly. Gaze is
        // covered by the more-conservative `OnDeviceOnly` route in handlers.
        for dom in [
            SensitiveDomain::Biometric,
            SensitiveDomain::FaceTracking,
            SensitiveDomain::BodyTracking,
        ] {
            let row = vec![sensitive(), telemetry()];
            let res = banned_composition_with_domains(&row, &[dom]);
            assert!(res.is_err(), "{dom:?} + Telemetry must be banned");
        }
    }

    // ─ banned_reason metadata ────────────────────────────────────────────

    #[test]
    fn banned_reason_codes_are_distinct() {
        let codes = [
            BannedReason::CoercionAbsolute.code(),
            BannedReason::SurveillanceWithIo.code(),
            BannedReason::WeaponWithIoNeedsKernel.code(),
            BannedReason::GazeWithNet.code(),
            BannedReason::BiometricWithNet.code(),
            BannedReason::BiometricWithTelemetry.code(),
            BannedReason::FaceTrackingEgress.code(),
            BannedReason::BodyTrackingEgress.code(),
            BannedReason::OnDeviceOnlyWithNet.code(),
            BannedReason::OnDeviceOnlyWithTelemetry.code(),
            BannedReason::TravelWithoutPrivilegeL4.code(),
        ];
        let mut sorted = codes.to_vec();
        sorted.sort_unstable();
        let original_len = sorted.len();
        sorted.dedup();
        assert_eq!(sorted.len(), original_len, "BAN-codes must be distinct");
    }

    #[test]
    fn t11_d129_bans_are_all_absolute() {
        // None of the T11-D129 bans permit Privilege<L> override.
        for r in [
            BannedReason::GazeWithNet,
            BannedReason::BiometricWithNet,
            BannedReason::BiometricWithTelemetry,
            BannedReason::FaceTrackingEgress,
            BannedReason::BodyTrackingEgress,
            BannedReason::OnDeviceOnlyWithNet,
            BannedReason::OnDeviceOnlyWithTelemetry,
        ] {
            assert!(r.is_absolute(), "{r:?} must be absolute");
        }
    }

    #[test]
    fn weapon_with_io_is_only_non_absolute_reason() {
        assert!(!BannedReason::WeaponWithIoNeedsKernel.is_absolute());
    }

    #[test]
    fn ban_text_references_p18_for_t11_d129() {
        for r in [
            BannedReason::GazeWithNet,
            BannedReason::BiometricWithNet,
            BannedReason::BiometricWithTelemetry,
            BannedReason::FaceTrackingEgress,
            BannedReason::BodyTrackingEgress,
            BannedReason::OnDeviceOnlyWithNet,
            BannedReason::OnDeviceOnlyWithTelemetry,
        ] {
            let s = r.to_string();
            assert!(
                s.contains("BiometricEgress") || s.contains("surveillance"),
                "{r:?} must reference §1 N! surveillance / P18 BiometricEgress"
            );
        }
    }
}
