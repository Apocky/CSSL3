//! § ifc_flow — IFC capability-flow + cap-combination + egress checks.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   The IFC + Pony-cap surface bound to [`crate::light::ApockyLight`].
//!   ApockyLight stores a `cap_handle` (u32 index into a per-process
//!   CapTable) ; this module provides the structural-gate primitives that
//!   composition operators use to refuse non-compatible combinations and
//!   that telemetry / readback paths use to refuse forbidden egress.
//!
//! § DESIGN
//!   The CapTable lives elsewhere (in the renderer host) ; this crate
//!   provides only the algebraic operations on cap-handles + the
//!   structural-gate that the operations layer calls. Concrete cap-binding
//!   semantics (Pony-cap subset, IFC label, principal set) are expressed
//!   via the [`CapHandle`] newtype which packs metadata into the upper
//!   bits of the u32. The lower 24 bits form the actual table-index ;
//!   the upper 8 bits form the cap-kind tag, identifying the binding type.
//!
//! § CAP-HANDLE LAYOUT (32 bits)
//!
//!   ```text
//!   bit  | width | field
//!   -----+-------+--------------------------------------
//!   0-23 |  24   | table_index (u24, 0..16M slots)
//!   24-26|   3   | pony_cap (CapKind tag, 0..=5)
//!   27   |   1   | ifc_biometric_flag (1=biometric-banned)
//!   28   |   1   | ifc_sensitive_flag (1=sensitive-domain)
//!   29   |   1   | ifc_audit_flag (1=audit-required)
//!   30-31|   2   | reserved (must be 0)
//!   ```
//!
//!   `table_index = 0` is the canonical anonymous-cap (val + bottom-label).
//!
//! § SPEC
//!   - `specs/12_CAPABILITIES.csl` § THE SIX CAPABILITIES — Pony-cap algebra.
//!   - `specs/11_IFC.csl` § LABEL ALGEBRA + § PRIME-DIRECTIVE ENCODING.
//!   - `PRIME_DIRECTIVE.md §1` — biometric absolute-banned-from-egress.
//!
//! § PRIME-DIRECTIVE
//!   Biometric-tagged cap-handles cannot egress via [`can_egress`] ; no
//!   Privilege override exists. The check is structural — composition with
//!   a biometric-tagged handle propagates the flag through `combine_caps`.

use cssl_caps::CapKind;
use cssl_ifc::SensitiveDomain;
use thiserror::Error;

use crate::operations::LightCompositionError;

// ───────────────────────────────────────────────────────────────────────────
// § Cap-handle bit-layout constants
// ───────────────────────────────────────────────────────────────────────────

/// § Mask for the 24-bit table-index portion of the cap-handle.
pub const CAP_HANDLE_INDEX_MASK: u32 = 0x00FF_FFFF;

/// § Bit position of the Pony-cap-kind tag (3 bits).
pub const CAP_HANDLE_KIND_SHIFT: u32 = 24;

/// § Mask of the Pony-cap-kind tag bits (3 bits).
pub const CAP_HANDLE_KIND_MASK: u32 = 0x0700_0000;

/// § IFC biometric flag bit position (bit 27).
pub const CAP_HANDLE_BIOMETRIC_BIT: u32 = 1 << 27;

/// § IFC sensitive-domain flag bit position (bit 28).
pub const CAP_HANDLE_SENSITIVE_BIT: u32 = 1 << 28;

/// § IFC audit-required flag bit position (bit 29).
pub const CAP_HANDLE_AUDIT_BIT: u32 = 1 << 29;

/// § Anonymous cap-handle — the canonical default (val + bottom-label).
pub const CAP_HANDLE_ANONYMOUS: u32 = 0;

// ───────────────────────────────────────────────────────────────────────────
// § Newtype wrappers
// ───────────────────────────────────────────────────────────────────────────

/// § Capability handle bound to a light-quantum.
///
/// Encodes the Pony-cap subset + IFC flags + table index in 32 bits.
/// See module-doc § CAP-HANDLE LAYOUT for the bit-layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CapHandle(pub u32);

impl CapHandle {
    /// § The anonymous cap-handle (val + bottom-label).
    pub const ANONYMOUS: Self = Self(CAP_HANDLE_ANONYMOUS);

    /// § Construct a fresh cap-handle from individual fields.
    ///
    /// `table_index` is masked to 24 bits ; over-large indices are
    /// silently truncated. `kind` is encoded as the 3-bit Pony-cap tag.
    #[must_use]
    pub const fn new(
        table_index: u32,
        kind: CapKind,
        biometric: bool,
        sensitive: bool,
        audit: bool,
    ) -> Self {
        let kind_tag = match kind {
            CapKind::Iso => 0,
            CapKind::Trn => 1,
            CapKind::Ref => 2,
            CapKind::Val => 3,
            CapKind::Box => 4,
            CapKind::Tag => 5,
        };
        let mut bits = table_index & CAP_HANDLE_INDEX_MASK;
        bits |= kind_tag << CAP_HANDLE_KIND_SHIFT;
        if biometric {
            bits |= CAP_HANDLE_BIOMETRIC_BIT;
        }
        if sensitive {
            bits |= CAP_HANDLE_SENSITIVE_BIT;
        }
        if audit {
            bits |= CAP_HANDLE_AUDIT_BIT;
        }
        Self(bits)
    }

    /// § The 24-bit table-index portion.
    #[must_use]
    pub const fn table_index(self) -> u32 {
        self.0 & CAP_HANDLE_INDEX_MASK
    }

    /// § The Pony-cap-kind associated with this handle.
    #[must_use]
    pub const fn pony_cap(self) -> CapKind {
        let tag = (self.0 & CAP_HANDLE_KIND_MASK) >> CAP_HANDLE_KIND_SHIFT;
        match tag {
            0 => CapKind::Iso,
            1 => CapKind::Trn,
            2 => CapKind::Ref,
            3 => CapKind::Val,
            4 => CapKind::Box,
            5 => CapKind::Tag,
            _ => CapKind::Tag,
        }
    }

    /// § True iff the IFC biometric-flag is set.
    #[must_use]
    pub const fn is_biometric(self) -> bool {
        self.0 & CAP_HANDLE_BIOMETRIC_BIT != 0
    }

    /// § True iff the IFC sensitive-domain flag is set.
    #[must_use]
    pub const fn is_sensitive(self) -> bool {
        self.0 & CAP_HANDLE_SENSITIVE_BIT != 0
    }

    /// § True iff the IFC audit-required flag is set.
    #[must_use]
    pub const fn is_audit_required(self) -> bool {
        self.0 & CAP_HANDLE_AUDIT_BIT != 0
    }

    /// § Raw u32 representation for std430 storage.
    #[must_use]
    pub const fn as_u32(self) -> u32 {
        self.0
    }
}

impl Default for CapHandle {
    fn default() -> Self {
        Self::ANONYMOUS
    }
}

/// § KAN-band table index handle. Newtype around u32 for type-safety.
///
/// 24-bit handle stored in the [`crate::light::ApockyLight::kan_and_evidence`]
/// field. Over-large values (>16M) are truncated by [`Self::new`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct KanBandHandle(pub u32);

impl KanBandHandle {
    /// § The null KAN-band handle (no compressed-band associated).
    pub const NULL: Self = Self(0);

    /// § Construct a KAN-band handle ; truncates to 24 bits.
    #[must_use]
    pub const fn new(idx: u32) -> Self {
        Self(idx & 0x00FF_FFFF)
    }

    /// § Raw u32 representation for embedding in `kan_and_evidence`.
    #[must_use]
    pub const fn as_u32(self) -> u32 {
        self.0
    }

    /// § True iff this is the null handle.
    #[must_use]
    pub const fn is_null(self) -> bool {
        self.0 == 0
    }
}

// ───────────────────────────────────────────────────────────────────────────
// § Error types
// ───────────────────────────────────────────────────────────────────────────

/// § Errors raised by IFC-flow validation.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum IfcFlowError {
    /// § Egress denied for biometric-tagged quantum (PRIME-DIRECTIVE §1).
    #[error("IFC-egress REFUSED for biometric-tagged cap-handle (PRIME-DIRECTIVE §1)")]
    BiometricEgressDenied,

    /// § Egress denied for sensitive-domain quantum without explicit consent.
    #[error("IFC-egress REFUSED for sensitive-domain {0:?} cap-handle ; consent gate not satisfied")]
    SensitiveEgressDenied(SensitiveDomain),

    /// § Two cap-handles are incompatible for combination
    ///   (e.g. iso + trn cannot share aliasing).
    #[error("incompatible Pony-cap kinds : {a:?} cannot combine with {b:?}")]
    IncompatiblePonyCaps {
        /// First cap-kind.
        a: CapKind,
        /// Second cap-kind.
        b: CapKind,
    },

    /// § Audit-flag mismatch — one quantum requires audit, other does not.
    #[error("audit-flag mismatch : audit-required quantum cannot combine with non-audited")]
    AuditFlagMismatch,
}

// ───────────────────────────────────────────────────────────────────────────
// § Egress check
// ───────────────────────────────────────────────────────────────────────────

/// § True iff a quantum with the given cap-handle is permitted to egress
///   to telemetry / readback / external surface.
///
/// Refuses biometric-tagged + sensitive-without-consent unconditionally
/// per PRIME-DIRECTIVE §1.
#[must_use]
pub fn can_egress(cap: CapHandle) -> bool {
    if cap.is_biometric() {
        return false;
    }
    // Sensitive-without-consent denied unless audit-flag set
    // (audit indicates the egress was reviewed via the §11 audit chain).
    if cap.is_sensitive() && !cap.is_audit_required() {
        return false;
    }
    true
}

/// § Validate egress + return typed error for use by composition operators.
pub fn validate_egress(cap: CapHandle) -> Result<(), IfcFlowError> {
    if cap.is_biometric() {
        return Err(IfcFlowError::BiometricEgressDenied);
    }
    if cap.is_sensitive() && !cap.is_audit_required() {
        // We don't have a specific SensitiveDomain in the cap-handle bits
        // (the table-side has the full label) — surface Privacy as the
        // canonical-fallback for the structural gate.
        return Err(IfcFlowError::SensitiveEgressDenied(SensitiveDomain::Privacy));
    }
    Ok(())
}

// ───────────────────────────────────────────────────────────────────────────
// § Cap-combination
// ───────────────────────────────────────────────────────────────────────────

/// § Combine two [`CapHandle`]s per the Pony-cap algebra + IFC label-join.
///
/// IFC flags are joined by OR : if either input is biometric, the output
/// is biometric ; same for sensitive + audit. Pony-cap-kind combination
/// uses the lattice :
///
///   - `iso + iso` → `iso` (linear, no aliasing across both)
///   - `iso + trn` → ERROR (iso refuses any aliasing)
///   - `iso + val` → `val` (iso freezes-to-val for sharing)
///   - `iso + ref/box/tag` → ERROR
///   - `val + val` → `val`
///   - `val + box` → `box`
///   - `box + box` → `box`
///   - `ref + ref` → `ref`
///   - `tag + tag` → `tag`
///   - `tag + anything` → `tag` (opaque-handle wins)
///   - mismatched + non-special → ERROR
///
/// `table_index` of the output is the higher of the two inputs (caller's
/// CapTable is responsible for ensuring the higher-index handle is a valid
/// merged-cap entry).
pub fn combine_caps(a: CapHandle, b: CapHandle) -> Result<CapHandle, IfcFlowError> {
    let pony_out = combine_pony_caps(a.pony_cap(), b.pony_cap())?;

    let biometric = a.is_biometric() || b.is_biometric();
    let sensitive = a.is_sensitive() || b.is_sensitive();
    let audit = a.is_audit_required() || b.is_audit_required();

    let idx = a.table_index().max(b.table_index());
    Ok(CapHandle::new(idx, pony_out, biometric, sensitive, audit))
}

/// § Raw-u32 cap-combination : the operations.rs layer calls this directly
///   without round-tripping through CapHandle. Errors are surfaced as
///   [`LightCompositionError::IncompatibleCaps`] for ergonomic reporting.
pub fn combine_caps_raw(a: u32, b: u32) -> Result<u32, LightCompositionError> {
    let ah = CapHandle(a);
    let bh = CapHandle(b);
    match combine_caps(ah, bh) {
        Ok(out) => Ok(out.as_u32()),
        Err(IfcFlowError::IncompatiblePonyCaps { .. }) => {
            Err(LightCompositionError::IncompatibleCaps { a, b })
        }
        Err(e) => Err(LightCompositionError::IfcFlow(e)),
    }
}

/// § Combine two Pony-cap kinds per the algebra documented in
///   [`combine_caps`].
fn combine_pony_caps(a: CapKind, b: CapKind) -> Result<CapKind, IfcFlowError> {
    use CapKind::{Box, Iso, Ref, Tag, Trn, Val};
    Ok(match (a, b) {
        // Identity cases.
        (Iso, Iso) => Iso,
        (Trn, Trn) => Trn,
        (Ref, Ref) => Ref,
        (Val, Val) => Val,
        (Box, Box) => Box,
        (Tag, Tag) => Tag,

        // Iso freezes to val on any val-aliasing.
        (Iso, Val) | (Val, Iso) => Val,

        // Iso refuses other aliasing.
        (Iso, _) | (_, Iso) => return Err(IfcFlowError::IncompatiblePonyCaps { a, b }),

        // Trn box-promotes to box.
        (Trn, Box) | (Box, Trn) => Box,

        // Val + box = box (read-only view of immutable).
        (Val, Box) | (Box, Val) => Box,

        // Tag-anything = tag (opaque-handle dominates).
        (Tag, _) | (_, Tag) => Tag,

        // Other combinations refused by Pony-6 algebra.
        _ => return Err(IfcFlowError::IncompatiblePonyCaps { a, b }),
    })
}

// ───────────────────────────────────────────────────────────────────────────
// § Tests
// ───────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anonymous_cap_egress_permitted() {
        assert!(can_egress(CapHandle::ANONYMOUS));
        assert!(validate_egress(CapHandle::ANONYMOUS).is_ok());
    }

    #[test]
    fn biometric_cap_egress_denied() {
        let biometric_cap = CapHandle::new(1, CapKind::Val, true, false, false);
        assert!(!can_egress(biometric_cap));
        assert_eq!(
            validate_egress(biometric_cap),
            Err(IfcFlowError::BiometricEgressDenied)
        );
    }

    #[test]
    fn sensitive_cap_without_audit_denied() {
        let sensitive_cap = CapHandle::new(1, CapKind::Val, false, true, false);
        assert!(!can_egress(sensitive_cap));
        assert!(matches!(
            validate_egress(sensitive_cap),
            Err(IfcFlowError::SensitiveEgressDenied(_))
        ));
    }

    #[test]
    fn sensitive_cap_with_audit_permitted() {
        let sensitive_audited = CapHandle::new(1, CapKind::Val, false, true, true);
        assert!(can_egress(sensitive_audited));
    }

    #[test]
    fn cap_handle_round_trip() {
        let h = CapHandle::new(0x12_3456, CapKind::Iso, true, false, true);
        assert_eq!(h.table_index(), 0x12_3456);
        assert_eq!(h.pony_cap(), CapKind::Iso);
        assert!(h.is_biometric());
        assert!(!h.is_sensitive());
        assert!(h.is_audit_required());
    }

    #[test]
    fn combine_iso_with_iso_yields_iso() {
        let a = CapHandle::new(1, CapKind::Iso, false, false, false);
        let b = CapHandle::new(2, CapKind::Iso, false, false, false);
        let c = combine_caps(a, b).unwrap();
        assert_eq!(c.pony_cap(), CapKind::Iso);
    }

    #[test]
    fn combine_iso_with_ref_refused() {
        let a = CapHandle::new(1, CapKind::Iso, false, false, false);
        let b = CapHandle::new(2, CapKind::Ref, false, false, false);
        assert!(matches!(
            combine_caps(a, b),
            Err(IfcFlowError::IncompatiblePonyCaps { .. })
        ));
    }

    #[test]
    fn combine_iso_with_val_freezes_to_val() {
        let a = CapHandle::new(1, CapKind::Iso, false, false, false);
        let b = CapHandle::new(2, CapKind::Val, false, false, false);
        let c = combine_caps(a, b).unwrap();
        assert_eq!(c.pony_cap(), CapKind::Val);
    }

    #[test]
    fn combine_propagates_biometric_flag() {
        let a = CapHandle::new(1, CapKind::Val, true, false, false);
        let b = CapHandle::new(2, CapKind::Val, false, false, false);
        let c = combine_caps(a, b).unwrap();
        assert!(c.is_biometric());
        // Biometric-tagged combination cannot egress.
        assert!(!can_egress(c));
    }

    #[test]
    fn combine_caps_raw_anonymous_with_anonymous() {
        let result = combine_caps_raw(0, 0);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);
    }

    #[test]
    fn kan_band_handle_truncates_to_24_bit() {
        let h = KanBandHandle::new(0xFFFF_FFFF);
        assert_eq!(h.as_u32(), 0x00FF_FFFF);
        assert!(!h.is_null());

        let null = KanBandHandle::NULL;
        assert!(null.is_null());
    }
}
