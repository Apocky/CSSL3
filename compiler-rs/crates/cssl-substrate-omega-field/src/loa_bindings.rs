//! § loa_bindings — Rust-side glue for the LoA-side `stdlib/omega.cssl` surface.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   This module is the **canonical Rust-host adapter** that the
//!   `stdlib/omega.cssl` surface lowers to. CSSL scene-authors write code
//!   like :
//!
//!   ```cssl
//!   omega::loa_read_cell(field_handle, coord)
//!   omega::loa_write_cell(field_handle, coord, cell, sovereign_cap)
//!   omega::loa_attach_extension(field_handle, coord, extension)
//!   ```
//!
//!   ...and each call lowers (per `cssl_mir::body_lower`) to a
//!   monomorph-mangled MirFunc whose ABI matches one of the entry-points
//!   below. The stdlib/omega.cssl shim translates per-rank surface fns
//!   into the canonical Σ-checked OmegaField mutation gate.
//!
//! § CAPABILITY-GATE DESIGN
//!   Every entry-point that mutates the OmegaField REQUIRES :
//!     1. A non-zero `sovereign_cap` u16 handle (the CSSL-side
//!        capability-token equivalent).
//!     2. A `coord` that decodes to a valid MortonKey.
//!     3. The cell's Σ-mask permits the requested op-class.
//!
//!   The READ-side path (`loa_read_cell`) requires Observe consent ; the
//!   WRITE-side path requires Modify (or Reconfigure for extension-set).
//!
//! § PRIME-DIRECTIVE-ALIGNMENT
//!   - sovereign_cap=0 ⇒ unauthorized actor ; refuses with
//!     [`LoaBindingError::SovereignCapNull`].
//!   - Reading a Sigma-claimed cell requires sovereign_cap to match if the
//!     mask carries a non-zero sovereign_handle (the
//!     'Sovereign-only-reads-claimed-cell' rule for sensitive cell-classes).
//!   - All entry-points are MUTATION-FREE on the underlying field on
//!     refusal — failed checks leave the field bit-identical.
//!
//! § SPEC
//!   - `specs/30_SUBSTRATE_v2.csl` § Σ-MASK-PER-CELL (consent-bit semantics).
//!   - `specs/33_F1_F6_LANGUAGE_FEATURES.csl` § F5.4 (EnforcesΣAtCellTouches).
//!   - `stdlib/omega.cssl` § LOA-BINDINGS (CSSL-side surface).

use crate::field_cell::FieldCell;
use crate::morton::{MortonError, MortonKey};
use crate::omega_field::{MutationError, OmegaField};
use cssl_substrate_prime_directive::sigma::SigmaMaskPacked;

/// § Marker constant : the canonical "no-Sovereign" sentinel for the
///   CSSL-side capability handle. Any actor presenting this handle is
///   refused at the gate.
pub const LOA_SOVEREIGN_NULL_CAP: u16 = 0;

/// § Per-call-site IFC-violation-classification. Used by the CSSL-side
///   recognizer to emit the canonical SIG0001..SIG0010 diagnostic codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum IfcViolation {
    /// § Read attempted without Observe consent.
    NoObserveConsent = 1,
    /// § Write attempted without Modify consent.
    NoModifyConsent = 2,
    /// § Sample/derive-out attempted without Sample consent.
    NoSampleConsent = 3,
    /// § Sovereign-handle mismatch on a claimed-cell mutation.
    SovereignMismatch = 4,
    /// § Capacity-floor erosion would result.
    CapacityFloorErosion = 5,
    /// § Reversibility-scope widening would result.
    ReversibilityWidening = 6,
    /// § Travel-row composed without Translate consent.
    TravelWithoutTranslate = 7,
    /// § Crystallize-row composed without Recrystallize consent.
    CrystallizeWithoutRecrystallize = 8,
    /// § Destroy attempted on a Frozen cell.
    DestroyOnFrozen = 9,
    /// § Reserved-tail bits set non-zero.
    ReservedNonZero = 10,
}

impl IfcViolation {
    /// § Stable diagnostic code matching specs/30_SUBSTRATE_v2 § ENFORCESΣATCELLTOUCHES-PASS.
    #[must_use]
    pub const fn diagnostic_code(self) -> &'static str {
        match self {
            Self::NoObserveConsent => "SIG0001",
            Self::NoModifyConsent => "SIG0001",
            Self::NoSampleConsent => "SIG0001",
            Self::SovereignMismatch => "SIG0004",
            Self::CapacityFloorErosion => "SIG0005",
            Self::ReversibilityWidening => "SIG0006",
            Self::TravelWithoutTranslate => "SIG0007",
            Self::CrystallizeWithoutRecrystallize => "SIG0008",
            Self::DestroyOnFrozen => "SIG0009",
            Self::ReservedNonZero => "SIG0010",
        }
    }
}

/// § Failure modes for the LoA-binding entry-points. These are the Rust-
///   host equivalents of the CSSL-side `Result<T, IfcViolation>` surface ;
///   the recognizer translates between them.
#[derive(Debug, thiserror::Error)]
pub enum LoaBindingError {
    /// § The presented capability is the NULL sentinel.
    #[error("LBND001 — sovereign_cap is NULL ; refusing capability-gated op")]
    SovereignCapNull,

    /// § The presented coordinate fails Morton-validity.
    #[error("LBND002 — invalid coordinate : {0}")]
    InvalidCoord(#[from] MortonError),

    /// § The requested op-class is refused by the cell's Σ-mask.
    #[error(
        "LBND003 — IFC violation : {violation:?} ({code} ; consent_bits=0x{consent_bits:08x})"
    )]
    IfcViolation {
        violation: IfcViolation,
        code: &'static str,
        consent_bits: u32,
    },

    /// § Underlying OmegaField mutation refusal (e.g. Sovereign mismatch).
    #[error("LBND004 — OmegaField mutation refused : {0}")]
    Field(#[from] MutationError),
}

/// § Read a cell from the OmegaField, capability-gated.
///
/// § CONTRACT
///   - `sovereign_cap` MUST be non-NULL.
///   - The cell's Σ-mask MUST permit Observe.
///   - On a Σ-claimed cell, `sovereign_cap` MUST match the cell's
///     sovereign_handle UNLESS the mask permits Sample (which is the
///     "public-read" affordance per SigmaPolicy::PublicRead).
///
/// § DESIGN
///   This is a value-by-copy read — FieldCell is Copy + 72B which is
///   small enough to return by-value without an indirection.
pub fn loa_read_cell(
    field: &OmegaField,
    coord: (u64, u64, u64),
    sovereign_cap: u16,
) -> Result<FieldCell, LoaBindingError> {
    if sovereign_cap == LOA_SOVEREIGN_NULL_CAP {
        return Err(LoaBindingError::SovereignCapNull);
    }
    let (x, y, z) = coord;
    let key = MortonKey::encode(x, y, z)?;
    let mask = field.sigma().at(key);
    if !mask.can_observe() {
        return Err(LoaBindingError::IfcViolation {
            violation: IfcViolation::NoObserveConsent,
            code: IfcViolation::NoObserveConsent.diagnostic_code(),
            consent_bits: mask.consent_bits(),
        });
    }
    // Sovereign-claimed-cell read : require either matching cap OR Sample
    // consent (the public-read affordance).
    if mask.is_sovereign() && mask.sovereign_handle() != sovereign_cap && !mask.can_sample() {
        return Err(LoaBindingError::IfcViolation {
            violation: IfcViolation::SovereignMismatch,
            code: IfcViolation::SovereignMismatch.diagnostic_code(),
            consent_bits: mask.consent_bits(),
        });
    }
    Ok(field.cell(key))
}

/// § Write a cell to the OmegaField, capability-gated.
///
/// § CONTRACT
///   - `sovereign_cap` MUST be non-NULL.
///   - The cell's Σ-mask MUST permit Modify.
///   - On a Σ-claimed cell, `sovereign_cap` MUST match the cell's
///     sovereign_handle (no Sample-bypass on writes).
///
/// § PRIME-DIRECTIVE
///   This entry-point goes through OmegaField::set_cell which advances
///   the audit-chain epoch. The write-path is thus first-class auditable
///   even when invoked from CSSL-side code.
pub fn loa_write_cell(
    field: &mut OmegaField,
    coord: (u64, u64, u64),
    cell: FieldCell,
    sovereign_cap: u16,
) -> Result<(), LoaBindingError> {
    if sovereign_cap == LOA_SOVEREIGN_NULL_CAP {
        return Err(LoaBindingError::SovereignCapNull);
    }
    let (x, y, z) = coord;
    let key = MortonKey::encode(x, y, z)?;
    let mask = field.sigma().at(key);
    if !mask.can_modify() {
        return Err(LoaBindingError::IfcViolation {
            violation: IfcViolation::NoModifyConsent,
            code: IfcViolation::NoModifyConsent.diagnostic_code(),
            consent_bits: mask.consent_bits(),
        });
    }
    if mask.is_sovereign() && mask.sovereign_handle() != sovereign_cap {
        return Err(LoaBindingError::IfcViolation {
            violation: IfcViolation::SovereignMismatch,
            code: IfcViolation::SovereignMismatch.diagnostic_code(),
            consent_bits: mask.consent_bits(),
        });
    }
    // Forward to the canonical OmegaField mutation gate (which re-checks
    // Σ-mask + advances the audit-chain epoch).
    field.set_cell(key, cell)?;
    Ok(())
}

/// § Sample-style derivative read — copies a cell out for derivative
///   computation. Requires Sample consent (NOT just Observe).
pub fn loa_sample_cell(
    field: &OmegaField,
    coord: (u64, u64, u64),
    sovereign_cap: u16,
) -> Result<FieldCell, LoaBindingError> {
    if sovereign_cap == LOA_SOVEREIGN_NULL_CAP {
        return Err(LoaBindingError::SovereignCapNull);
    }
    let (x, y, z) = coord;
    let key = MortonKey::encode(x, y, z)?;
    let mask = field.sigma().at(key);
    if !mask.can_sample() {
        return Err(LoaBindingError::IfcViolation {
            violation: IfcViolation::NoSampleConsent,
            code: IfcViolation::NoSampleConsent.diagnostic_code(),
            consent_bits: mask.consent_bits(),
        });
    }
    Ok(field.cell(key))
}

/// § Set the Σ-mask on a cell (Sovereign-action). Used to GRANT consent
///   on a previously-default-Private cell. Requires the new mask to
///   declare a non-zero sovereign_handle that matches `sovereign_cap`.
pub fn loa_grant_sovereign(
    field: &mut OmegaField,
    coord: (u64, u64, u64),
    new_mask: SigmaMaskPacked,
    sovereign_cap: u16,
) -> Result<(), LoaBindingError> {
    if sovereign_cap == LOA_SOVEREIGN_NULL_CAP {
        return Err(LoaBindingError::SovereignCapNull);
    }
    if new_mask.sovereign_handle() != sovereign_cap {
        return Err(LoaBindingError::IfcViolation {
            violation: IfcViolation::SovereignMismatch,
            code: IfcViolation::SovereignMismatch.diagnostic_code(),
            consent_bits: new_mask.consent_bits(),
        });
    }
    let (x, y, z) = coord;
    let key = MortonKey::encode(x, y, z)?;
    // If the cell is already-claimed by a different Sovereign, refuse.
    let existing = field.sigma().at(key);
    if existing.is_sovereign() && existing.sovereign_handle() != sovereign_cap {
        return Err(LoaBindingError::IfcViolation {
            violation: IfcViolation::SovereignMismatch,
            code: IfcViolation::SovereignMismatch.diagnostic_code(),
            consent_bits: existing.consent_bits(),
        });
    }
    field.set_sigma(key, new_mask);
    Ok(())
}

/// § Probe whether a cell is Σ-readable for the given capability (without
///   actually reading). Used by CSSL-side recognizers to emit pre-flight
///   diagnostics rather than fail-at-call-site.
#[must_use]
pub fn loa_can_read(field: &OmegaField, coord: (u64, u64, u64), sovereign_cap: u16) -> bool {
    if sovereign_cap == LOA_SOVEREIGN_NULL_CAP {
        return false;
    }
    let (x, y, z) = coord;
    let Ok(key) = MortonKey::encode(x, y, z) else {
        return false;
    };
    let mask = field.sigma().at(key);
    if !mask.can_observe() {
        return false;
    }
    if mask.is_sovereign() && mask.sovereign_handle() != sovereign_cap && !mask.can_sample() {
        return false;
    }
    true
}

/// § Probe whether a cell is Σ-writable for the given capability.
#[must_use]
pub fn loa_can_write(field: &OmegaField, coord: (u64, u64, u64), sovereign_cap: u16) -> bool {
    if sovereign_cap == LOA_SOVEREIGN_NULL_CAP {
        return false;
    }
    let (x, y, z) = coord;
    let Ok(key) = MortonKey::encode(x, y, z) else {
        return false;
    };
    let mask = field.sigma().at(key);
    if !mask.can_modify() {
        return false;
    }
    if mask.is_sovereign() && mask.sovereign_handle() != sovereign_cap {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use cssl_substrate_prime_directive::sigma::{ConsentBit, SigmaPolicy};

    fn permissive_for(s: u16) -> SigmaMaskPacked {
        SigmaMaskPacked::default_mask()
            .with_consent(
                ConsentBit::Observe.bits()
                    | ConsentBit::Sample.bits()
                    | ConsentBit::Modify.bits()
                    | ConsentBit::Reconfigure.bits(),
            )
            .with_sovereign(s)
    }

    // ── ω-read with capability ─────────────────────────────────────

    #[test]
    fn read_with_null_cap_refused() {
        let field = OmegaField::new();
        let err = loa_read_cell(&field, (1, 2, 3), LOA_SOVEREIGN_NULL_CAP).unwrap_err();
        assert!(matches!(err, LoaBindingError::SovereignCapNull));
    }

    #[test]
    fn read_with_observe_consent_succeeds() {
        let field = OmegaField::new();
        // Default mask permits Observe ; non-Sovereign cell ; any non-NULL
        // capability passes through.
        let cell = loa_read_cell(&field, (1, 2, 3), 7).unwrap();
        assert_eq!(cell, FieldCell::default());
    }

    #[test]
    fn read_sovereign_claimed_with_matching_cap_succeeds() {
        let mut field = OmegaField::new();
        let key = MortonKey::encode(0, 0, 0).unwrap();
        let mask = SigmaMaskPacked::default_mask().with_sovereign(42);
        field.set_sigma(key, mask);
        loa_read_cell(&field, (0, 0, 0), 42).unwrap();
    }

    #[test]
    fn read_sovereign_claimed_with_mismatch_no_sample_refused() {
        let mut field = OmegaField::new();
        let key = MortonKey::encode(0, 0, 0).unwrap();
        let mask = SigmaMaskPacked::default_mask().with_sovereign(42);
        field.set_sigma(key, mask);
        let err = loa_read_cell(&field, (0, 0, 0), 99).unwrap_err();
        match err {
            LoaBindingError::IfcViolation { violation, .. } => {
                assert_eq!(violation, IfcViolation::SovereignMismatch);
            }
            _ => panic!("expected IfcViolation::SovereignMismatch"),
        }
    }

    #[test]
    fn read_sovereign_claimed_with_mismatch_but_sample_succeeds() {
        let mut field = OmegaField::new();
        let key = MortonKey::encode(0, 0, 0).unwrap();
        // Sovereign claims the cell but ALSO publishes Sample consent.
        let mask = SigmaMaskPacked::from_policy(SigmaPolicy::PublicRead).with_sovereign(42);
        field.set_sigma(key, mask);
        // Different cap — but Sample-bit lets the read through.
        loa_read_cell(&field, (0, 0, 0), 99).unwrap();
    }

    // ── ω-write with capability ────────────────────────────────────

    #[test]
    fn write_without_modify_consent_refused() {
        let mut field = OmegaField::new();
        let cell = FieldCell::default();
        let err = loa_write_cell(&mut field, (1, 2, 3), cell, 7).unwrap_err();
        // Default-Private mask does NOT permit Modify.
        match err {
            LoaBindingError::IfcViolation { violation, .. } => {
                assert_eq!(violation, IfcViolation::NoModifyConsent);
            }
            _ => panic!("expected IfcViolation::NoModifyConsent"),
        }
    }

    #[test]
    fn write_with_modify_consent_succeeds() {
        let mut field = OmegaField::new();
        let key = MortonKey::encode(0, 0, 0).unwrap();
        let mask = permissive_for(7);
        field.set_sigma(key, mask);
        let mut cell = FieldCell::default();
        cell.density = 1.5;
        loa_write_cell(&mut field, (0, 0, 0), cell, 7).unwrap();
        // After write the audit-chain epoch advanced.
        assert_eq!(field.epoch(), 1);
    }

    #[test]
    fn write_sovereign_claimed_with_mismatch_refused() {
        let mut field = OmegaField::new();
        let key = MortonKey::encode(0, 0, 0).unwrap();
        let mask = permissive_for(42);
        field.set_sigma(key, mask);
        let cell = FieldCell::default();
        let err = loa_write_cell(&mut field, (0, 0, 0), cell, 99).unwrap_err();
        match err {
            LoaBindingError::IfcViolation { violation, .. } => {
                assert_eq!(violation, IfcViolation::SovereignMismatch);
            }
            _ => panic!("expected IfcViolation::SovereignMismatch"),
        }
        // Field unchanged on refusal.
        assert_eq!(field.epoch(), 0);
    }

    // ── Sample (derivative-out) ────────────────────────────────────

    #[test]
    fn sample_without_sample_consent_refused() {
        let field = OmegaField::new();
        let err = loa_sample_cell(&field, (1, 2, 3), 7).unwrap_err();
        match err {
            LoaBindingError::IfcViolation { violation, .. } => {
                assert_eq!(violation, IfcViolation::NoSampleConsent);
            }
            _ => panic!("expected NoSampleConsent"),
        }
    }

    #[test]
    fn sample_with_sample_consent_succeeds() {
        let mut field = OmegaField::new();
        let key = MortonKey::encode(0, 0, 0).unwrap();
        let mask = SigmaMaskPacked::from_policy(SigmaPolicy::PublicRead);
        field.set_sigma(key, mask);
        loa_sample_cell(&field, (0, 0, 0), 7).unwrap();
    }

    // ── Sovereign-grant ────────────────────────────────────────────

    #[test]
    fn grant_sovereign_with_matching_cap_succeeds() {
        let mut field = OmegaField::new();
        let mask = permissive_for(42);
        loa_grant_sovereign(&mut field, (0, 0, 0), mask, 42).unwrap();
        let key = MortonKey::encode(0, 0, 0).unwrap();
        assert_eq!(field.sigma().at(key).sovereign_handle(), 42);
    }

    #[test]
    fn grant_sovereign_mismatch_refused() {
        let mut field = OmegaField::new();
        let mask = permissive_for(42);
        let err = loa_grant_sovereign(&mut field, (0, 0, 0), mask, 99).unwrap_err();
        assert!(matches!(err, LoaBindingError::IfcViolation { .. }));
    }

    #[test]
    fn grant_sovereign_already_claimed_by_other_refused() {
        let mut field = OmegaField::new();
        let key = MortonKey::encode(0, 0, 0).unwrap();
        let prior_mask = permissive_for(42);
        field.set_sigma(key, prior_mask);
        let new_mask = permissive_for(99);
        let err = loa_grant_sovereign(&mut field, (0, 0, 0), new_mask, 99).unwrap_err();
        assert!(matches!(err, LoaBindingError::IfcViolation { .. }));
    }

    // ── Probes ─────────────────────────────────────────────────────

    #[test]
    fn can_read_default_mask_permits_observe() {
        let field = OmegaField::new();
        assert!(loa_can_read(&field, (1, 2, 3), 7));
    }

    #[test]
    fn can_read_null_cap_false() {
        let field = OmegaField::new();
        assert!(!loa_can_read(&field, (1, 2, 3), LOA_SOVEREIGN_NULL_CAP));
    }

    #[test]
    fn can_write_default_mask_no() {
        let field = OmegaField::new();
        assert!(!loa_can_write(&field, (1, 2, 3), 7));
    }

    #[test]
    fn can_write_with_modify_consent_yes() {
        let mut field = OmegaField::new();
        let key = MortonKey::encode(0, 0, 0).unwrap();
        let mask = permissive_for(7);
        field.set_sigma(key, mask);
        assert!(loa_can_write(&field, (0, 0, 0), 7));
    }

    // ── IFC violation diagnostic codes ─────────────────────────────

    #[test]
    fn ifc_violation_codes_match_spec() {
        assert_eq!(IfcViolation::NoObserveConsent.diagnostic_code(), "SIG0001");
        assert_eq!(IfcViolation::SovereignMismatch.diagnostic_code(), "SIG0004");
        assert_eq!(
            IfcViolation::CapacityFloorErosion.diagnostic_code(),
            "SIG0005"
        );
        assert_eq!(
            IfcViolation::ReversibilityWidening.diagnostic_code(),
            "SIG0006"
        );
        assert_eq!(
            IfcViolation::TravelWithoutTranslate.diagnostic_code(),
            "SIG0007"
        );
        assert_eq!(
            IfcViolation::CrystallizeWithoutRecrystallize.diagnostic_code(),
            "SIG0008"
        );
        assert_eq!(IfcViolation::DestroyOnFrozen.diagnostic_code(), "SIG0009");
        assert_eq!(IfcViolation::ReservedNonZero.diagnostic_code(), "SIG0010");
    }
}
