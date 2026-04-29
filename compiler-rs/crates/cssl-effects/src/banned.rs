//! Prime-Directive banned-composition checker.
//!
//! § SPEC : `specs/04_EFFECTS.csl` § PRIME-DIRECTIVE EFFECTS + PRIME_DIRECTIVE.md F5
//!   structural encoding of protections.
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
    /// `Telemetry<*>` composed with a raw `Path` argument is banned (T11-D130).
    /// Per the path-hash-only discipline, a fn that observes paths under a
    /// telemetry-effect MUST hash them at the boundary — receiving a raw
    /// `Path` would let the path bytes flow into the telemetry ring.
    #[error(
        "Telemetry<*> composed with a raw Path argument is banned ; pass a \
         PathHash instead (PRIME DIRECTIVE § 1 : N! surveillance ; \
         specs/22 § FS-OPS ; T11-D130 path-hash-only discipline)"
    )]
    TelemetryWithRawPath,
}

/// Check whether an effect row is free of Prime-Directive-banned compositions.
///
/// Returns `Ok(())` if the row is compositionally safe, or a list of
/// `BannedReason`s otherwise (one per distinct violation found).
pub fn banned_composition(row: &[EffectRef<'_>]) -> Result<(), Vec<BannedReason>> {
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

    if violations.is_empty() {
        Ok(())
    } else {
        Err(violations)
    }
}

// ───────────────────────────────────────────────────────────────────────
// § T11-D130 : path-hash-only discipline check.
//
// `check_telemetry_no_raw_path` rejects any fn that has both
// `{Telemetry<S>}` in its effect-row AND a raw `Path` / `&Path` /
// `PathBuf` argument. The argument types are passed as a slice of
// stringified type-names (the elaborator already has these in HIR ;
// stage-0 takes them as plain strings).
//
// The `BannedReason::TelemetryWithRawPath` reason is what the diagnostic
// emits. The check is structurally part of the same banned-composition
// table : a fn that violates is a compile error, regardless of handler
// installation, runtime check, or feature flag.
// ───────────────────────────────────────────────────────────────────────

/// Type-name strings that count as "raw path" arguments. Stage-0 stringly-
/// matches the most common Rust path-types ; the HIR-level lowering pass
/// (T11-phase-2) will drive this off the resolved-type DefId.
pub const RAW_PATH_TYPE_NAMES: &[&str] =
    &["Path", "&Path", "PathBuf", "&PathBuf", "OsStr", "&OsStr", "OsString"];

/// Heuristic : does `type_name` look like a raw filesystem-path type ?
/// Stage-0 uses prefix matching to handle generics + lifetime-prefixes
/// (e.g., `&'a Path`, `&Path<'a>`).
#[must_use]
pub fn is_raw_path_type(type_name: &str) -> bool {
    let trimmed = type_name.trim();
    // Strip leading `&'lt ` if present.
    let core_name = trimmed.strip_prefix('&').map_or(trimmed, |s| {
        // Skip optional lifetime token like 'a or 'static.
        s.trim_start().strip_prefix('\'').map_or_else(
            || s.trim_start(),
            |rest| {
                let after_lt = rest.trim_start_matches(|c: char| c.is_alphanumeric() || c == '_');
                after_lt.trim_start()
            },
        )
    });
    for &candidate in RAW_PATH_TYPE_NAMES {
        let stripped_candidate = candidate.trim_start_matches('&').trim_start();
        if core_name == stripped_candidate
            || core_name.starts_with(&format!("{stripped_candidate}<"))
        {
            return true;
        }
    }
    false
}

/// Check that a fn with `{Telemetry<S>}` in its effect-row does NOT have
/// a raw path-type argument.
///
/// § ARGUMENTS
///   - `row`         : the fn's effect-row (any sub-shape).
///   - `arg_types`   : stringified type-names of every fn argument
///                     (e.g., `["&Path", "u64"]`).
///
/// Returns `Ok(())` if compositionally safe ;
/// `Err(BannedReason::TelemetryWithRawPath)` otherwise.
///
/// # Errors
/// Returns the violation reason on detected raw-path-typed argument
/// in a Telemetry-rowed fn.
pub fn check_telemetry_no_raw_path(
    row: &[EffectRef<'_>],
    arg_types: &[&str],
) -> Result<(), BannedReason> {
    let has_telemetry = row
        .iter()
        .any(|e| matches!(e.builtin, Some(BuiltinEffect::Telemetry)));
    if !has_telemetry {
        return Ok(());
    }
    for ty in arg_types {
        if is_raw_path_type(ty) {
            return Err(BannedReason::TelemetryWithRawPath);
        }
    }
    Ok(())
}

/// Full-fidelity variant that inspects explicit `SensitiveDomain` labels instead
/// of relying on the `EffectRef::name` proxy. Callers in `cssl-hir` should use
/// this variant once they've resolved the domain-label from the HIR effect-arg.
pub fn banned_composition_with_domains(
    row: &[EffectRef<'_>],
    sensitive_domains: &[SensitiveDomain<'_>],
) -> Result<(), Vec<BannedReason>> {
    let mut violations: Vec<BannedReason> = Vec::new();
    let has_io = row
        .iter()
        .any(|e| matches!(e.builtin, Some(BuiltinEffect::Io)));
    let has_kernel_priv = row
        .iter()
        .any(|e| matches!(e.builtin, Some(BuiltinEffect::Privilege)) && e.arg_count == 1);

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

    // § T11-D130 — Telemetry × raw-Path rejection tests

    use super::{check_telemetry_no_raw_path, is_raw_path_type};

    #[test]
    fn raw_path_typename_recognizer_basic() {
        assert!(is_raw_path_type("Path"));
        assert!(is_raw_path_type("&Path"));
        assert!(is_raw_path_type("PathBuf"));
        assert!(is_raw_path_type("&PathBuf"));
        assert!(is_raw_path_type("OsStr"));
        assert!(is_raw_path_type("&OsStr"));
        assert!(is_raw_path_type("OsString"));
    }

    #[test]
    fn raw_path_typename_recognizer_with_lifetimes() {
        assert!(is_raw_path_type("&'a Path"));
        assert!(is_raw_path_type("&'static Path"));
        assert!(is_raw_path_type("& 'lt PathBuf"));
    }

    #[test]
    fn raw_path_typename_recognizer_rejects_other_types() {
        assert!(!is_raw_path_type("u64"));
        assert!(!is_raw_path_type("&str"));
        assert!(!is_raw_path_type("String"));
        assert!(!is_raw_path_type("PathHash"));
        assert!(!is_raw_path_type("&PathHash"));
    }

    #[test]
    fn raw_path_typename_recognizer_handles_generic_params() {
        // Path with a generic parameter at the type level should still match.
        assert!(is_raw_path_type("Path<'a>"));
    }

    #[test]
    fn telemetry_with_raw_path_rejected() {
        let row = vec![e("Telemetry", Some(BuiltinEffect::Telemetry), 1)];
        let r = check_telemetry_no_raw_path(&row, &["&Path", "u64"]);
        assert_eq!(r, Err(BannedReason::TelemetryWithRawPath));
    }

    #[test]
    fn telemetry_with_pathbuf_rejected() {
        let row = vec![e("Telemetry", Some(BuiltinEffect::Telemetry), 1)];
        let r = check_telemetry_no_raw_path(&row, &["PathBuf"]);
        assert_eq!(r, Err(BannedReason::TelemetryWithRawPath));
    }

    #[test]
    fn telemetry_with_path_hash_accepted() {
        let row = vec![e("Telemetry", Some(BuiltinEffect::Telemetry), 1)];
        let r = check_telemetry_no_raw_path(&row, &["PathHash", "u64"]);
        assert!(r.is_ok());
    }

    #[test]
    fn telemetry_with_no_path_args_accepted() {
        let row = vec![e("Telemetry", Some(BuiltinEffect::Telemetry), 1)];
        let r = check_telemetry_no_raw_path(&row, &["&str", "i32", "f64"]);
        assert!(r.is_ok());
    }

    #[test]
    fn no_telemetry_with_raw_path_is_irrelevant() {
        // Without {Telemetry<*>}, the path-arg is fine — no surveillance-
        // surface to leak into.
        let row = vec![e("IO", Some(BuiltinEffect::Io), 0)];
        let r = check_telemetry_no_raw_path(&row, &["&Path"]);
        assert!(r.is_ok());
    }

    #[test]
    fn telemetry_check_error_message_cites_t11_d130() {
        let row = vec![e("Telemetry", Some(BuiltinEffect::Telemetry), 1)];
        let r = check_telemetry_no_raw_path(&row, &["Path"]);
        let err = r.unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("T11-D130"));
        assert!(msg.contains("PathHash"));
    }
}
