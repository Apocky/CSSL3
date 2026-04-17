//! Capability subtyping relation.
//!
//! § SPEC (`specs/12_CAPABILITIES.csl` § CAPABILITY-DIRECTED SUBTYPING) :
//!
//! ```text
//!   iso <: trn    (relax unique → writable)         [explicit consume]
//!   iso <: val    (freeze)                           [explicit freeze]
//!   iso <: tag    (hide data)                        [explicit alias-as-tag]
//!   iso <: box    (transitively readable)
//!   trn <: box    (lose write access, keep read)
//!   val <: box    (val is already immutable-readable)
//!   no auto-demotion iso → ref (aliasing must be explicit)
//! ```

use thiserror::Error;

use crate::cap::CapKind;

/// Subtype witness — tags the kind of coercion needed to go `from → to`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Subtype {
    /// Reflexive : same capability on both sides.
    Reflexive,
    /// `iso <: trn` via explicit consume.
    IsoToTrn,
    /// `iso <: val` via freeze.
    IsoToVal,
    /// `iso <: box` via transitive-read.
    IsoToBox,
    /// `iso <: tag` via hide-data.
    IsoToTag,
    /// `trn <: box` via lose-write.
    TrnToBox,
    /// `val <: box` : val is already immutable-readable.
    ValToBox,
}

/// Failure mode for `coerce` when no subtype relation exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error(
    "no cap-subtype : cannot coerce {from:?} to {to:?} (per §§ 12 CAPABILITY-DIRECTED SUBTYPING)"
)]
pub struct SubtypeError {
    pub from: CapKind,
    pub to: CapKind,
}

/// `true` iff `from <: to` per the Pony-6 subtyping table (reflexive included).
#[must_use]
pub const fn is_subtype(from: CapKind, to: CapKind) -> bool {
    coerce(from, to).is_ok()
}

/// Look up the subtype witness between two capabilities. Returns `Ok(Subtype)` if a
/// relation exists ; otherwise `Err(SubtypeError)`.
///
/// § NOTES
///   - No auto-demotion `iso → ref` — aliasing must be explicit (per spec).
///   - No demotion `ref → anything` — once-shared stays once-shared.
///   - `tag` is minimal ; only `iso <: tag` is permitted.
pub const fn coerce(from: CapKind, to: CapKind) -> Result<Subtype, SubtypeError> {
    if matches!(
        (from, to),
        (CapKind::Iso, CapKind::Iso)
            | (CapKind::Trn, CapKind::Trn)
            | (CapKind::Ref, CapKind::Ref)
            | (CapKind::Val, CapKind::Val)
            | (CapKind::Box, CapKind::Box)
            | (CapKind::Tag, CapKind::Tag)
    ) {
        return Ok(Subtype::Reflexive);
    }
    match (from, to) {
        (CapKind::Iso, CapKind::Trn) => Ok(Subtype::IsoToTrn),
        (CapKind::Iso, CapKind::Val) => Ok(Subtype::IsoToVal),
        (CapKind::Iso, CapKind::Box) => Ok(Subtype::IsoToBox),
        (CapKind::Iso, CapKind::Tag) => Ok(Subtype::IsoToTag),
        (CapKind::Trn, CapKind::Box) => Ok(Subtype::TrnToBox),
        (CapKind::Val, CapKind::Box) => Ok(Subtype::ValToBox),
        _ => Err(SubtypeError { from, to }),
    }
}

#[cfg(test)]
mod tests {
    use super::{coerce, is_subtype, Subtype};
    use crate::cap::CapKind;

    #[test]
    fn reflexive_for_all_caps() {
        for c in CapKind::ALL {
            assert_eq!(coerce(c, c).unwrap(), Subtype::Reflexive);
            assert!(is_subtype(c, c));
        }
    }

    #[test]
    fn iso_can_become_trn_val_box_tag() {
        assert_eq!(
            coerce(CapKind::Iso, CapKind::Trn).unwrap(),
            Subtype::IsoToTrn
        );
        assert_eq!(
            coerce(CapKind::Iso, CapKind::Val).unwrap(),
            Subtype::IsoToVal
        );
        assert_eq!(
            coerce(CapKind::Iso, CapKind::Box).unwrap(),
            Subtype::IsoToBox
        );
        assert_eq!(
            coerce(CapKind::Iso, CapKind::Tag).unwrap(),
            Subtype::IsoToTag
        );
    }

    #[test]
    fn trn_can_become_box() {
        assert_eq!(
            coerce(CapKind::Trn, CapKind::Box).unwrap(),
            Subtype::TrnToBox
        );
    }

    #[test]
    fn val_can_become_box() {
        assert_eq!(
            coerce(CapKind::Val, CapKind::Box).unwrap(),
            Subtype::ValToBox
        );
    }

    #[test]
    fn iso_cannot_auto_become_ref() {
        // Explicit aliasing required ; no auto-demotion.
        assert!(coerce(CapKind::Iso, CapKind::Ref).is_err());
        assert!(!is_subtype(CapKind::Iso, CapKind::Ref));
    }

    #[test]
    fn ref_cannot_demote() {
        // Once shared, always shared.
        assert!(coerce(CapKind::Ref, CapKind::Iso).is_err());
        assert!(coerce(CapKind::Ref, CapKind::Val).is_err());
        assert!(coerce(CapKind::Ref, CapKind::Box).is_err());
    }

    #[test]
    fn val_cannot_become_iso() {
        // val is aliasable ; iso is unique. No demotion.
        assert!(coerce(CapKind::Val, CapKind::Iso).is_err());
    }

    #[test]
    fn box_cannot_become_writable() {
        assert!(coerce(CapKind::Box, CapKind::Iso).is_err());
        assert!(coerce(CapKind::Box, CapKind::Trn).is_err());
        assert!(coerce(CapKind::Box, CapKind::Ref).is_err());
        assert!(coerce(CapKind::Box, CapKind::Val).is_err());
    }

    #[test]
    fn tag_only_reflexive() {
        for c in [
            CapKind::Iso,
            CapKind::Trn,
            CapKind::Ref,
            CapKind::Val,
            CapKind::Box,
        ] {
            assert!(
                coerce(CapKind::Tag, c).is_err(),
                "tag should not become {c:?}"
            );
        }
    }

    #[test]
    fn subtype_error_carries_pair() {
        let err = coerce(CapKind::Val, CapKind::Iso).unwrap_err();
        assert_eq!(err.from, CapKind::Val);
        assert_eq!(err.to, CapKind::Iso);
    }
}
