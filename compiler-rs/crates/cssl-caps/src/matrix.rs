//! Pony-6 alias+deny matrix — the canonical table of per-capability rights.
//!
//! § SPEC (`specs/12_CAPABILITIES.csl` § THE SIX CAPABILITIES) — reproduced for
//! reference :
//!
//! ```text
//!   cap     alias-local   alias-global   mut-local   mut-global    meaning
//!   iso     ✗             ✗              ✓           ✓             isolated, linear
//!   trn     ✓             ✗              ✓           ✗             writable, locally-aliased
//!   ref     ✓             ✓              ✓           ✓             shared-mutable (gen-ref)
//!   val     ✓             ✓              ✗           ✗             immutable-shared
//!   box     ✓             ✓              ✗           ✗             read-only view
//!   tag     ✓             ✓              ✗           ✗             opaque-handle (no deref)
//! ```

use crate::cap::CapKind;

/// Rights a capability grants : aliasing-locally, aliasing-globally,
/// mutation-locally, mutation-globally. `send`-safety is derived from these.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AliasRights {
    pub alias_local: bool,
    pub alias_global: bool,
    pub mut_local: bool,
    pub mut_global: bool,
}

impl AliasRights {
    /// Return the rights granted by the given capability.
    #[must_use]
    pub const fn for_cap(c: CapKind) -> Self {
        match c {
            CapKind::Iso => Self {
                alias_local: false,
                alias_global: false,
                mut_local: true,
                mut_global: true,
            },
            CapKind::Trn => Self {
                alias_local: true,
                alias_global: false,
                mut_local: true,
                mut_global: false,
            },
            CapKind::Ref => Self {
                alias_local: true,
                alias_global: true,
                mut_local: true,
                mut_global: true,
            },
            CapKind::Val => Self {
                alias_local: true,
                alias_global: true,
                mut_local: false,
                mut_global: false,
            },
            CapKind::Box => Self {
                alias_local: true,
                alias_global: true,
                mut_local: false,
                mut_global: false,
            },
            CapKind::Tag => Self {
                alias_local: true,
                alias_global: true,
                mut_local: false,
                mut_global: false,
            },
        }
    }

    /// `true` iff any form of aliasing is permitted.
    #[must_use]
    pub const fn can_alias(self) -> bool {
        self.alias_local || self.alias_global
    }

    /// `true` iff any form of mutation is permitted.
    #[must_use]
    pub const fn can_mutate(self) -> bool {
        self.mut_local || self.mut_global
    }
}

/// One row of the alias+deny matrix — the rights a cap grants.
pub type AliasRow = AliasRights;

/// The full 6-entry alias-matrix. `rights[cap.index()]` returns the rights for
/// that capability.
#[derive(Debug, Clone, Copy)]
pub struct AliasMatrix {
    rights: [AliasRights; 6],
}

impl AliasMatrix {
    /// Build the canonical Pony-6 matrix.
    #[must_use]
    pub const fn pony6() -> Self {
        Self {
            rights: [
                AliasRights::for_cap(CapKind::Iso),
                AliasRights::for_cap(CapKind::Trn),
                AliasRights::for_cap(CapKind::Ref),
                AliasRights::for_cap(CapKind::Val),
                AliasRights::for_cap(CapKind::Box),
                AliasRights::for_cap(CapKind::Tag),
            ],
        }
    }

    /// Lookup rights for a given capability.
    #[must_use]
    pub const fn get(&self, c: CapKind) -> AliasRights {
        self.rights[c.index()]
    }

    /// `true` iff the `caller`'s cap admits passing through to a parameter declared
    /// with `callee_param`-cap. Defined as `caller <: callee_param` in the
    /// cap-subtyping relation (see `subtype::is_subtype`) — delegates to that check.
    #[must_use]
    pub fn can_pass_through(&self, caller: CapKind, callee_param: CapKind) -> bool {
        crate::subtype::is_subtype(caller, callee_param)
    }

    /// Iterate over all rights rows paired with their cap.
    pub fn iter(&self) -> impl Iterator<Item = (CapKind, AliasRights)> + '_ {
        CapKind::ALL.iter().map(|c| (*c, self.get(*c)))
    }
}

impl Default for AliasMatrix {
    fn default() -> Self {
        Self::pony6()
    }
}

#[cfg(test)]
mod tests {
    use super::{AliasMatrix, AliasRights};
    use crate::cap::CapKind;

    #[test]
    fn pony6_matches_spec() {
        let m = AliasMatrix::pony6();
        // iso : no aliasing, full mutation
        let iso = m.get(CapKind::Iso);
        assert!(!iso.alias_local);
        assert!(!iso.alias_global);
        assert!(iso.mut_local);
        assert!(iso.mut_global);
        // trn : local-alias, no-global-alias, local-mut, no-global-mut
        let trn = m.get(CapKind::Trn);
        assert!(trn.alias_local);
        assert!(!trn.alias_global);
        assert!(trn.mut_local);
        assert!(!trn.mut_global);
        // ref : full aliasing + mutation
        let ref_ = m.get(CapKind::Ref);
        assert!(ref_.alias_local);
        assert!(ref_.alias_global);
        assert!(ref_.mut_local);
        assert!(ref_.mut_global);
        // val : full alias, no mutation
        let val = m.get(CapKind::Val);
        assert!(val.alias_local);
        assert!(val.alias_global);
        assert!(!val.mut_local);
        assert!(!val.mut_global);
        // box : same shape as val
        let box_ = m.get(CapKind::Box);
        assert!(box_.alias_local);
        assert!(box_.alias_global);
        assert!(!box_.mut_local);
        assert!(!box_.mut_global);
        // tag : same shape as val/box
        let tag = m.get(CapKind::Tag);
        assert!(tag.alias_local);
        assert!(tag.alias_global);
        assert!(!tag.mut_local);
        assert!(!tag.mut_global);
    }

    #[test]
    fn passing_val_to_val_allowed() {
        let m = AliasMatrix::pony6();
        assert!(m.can_pass_through(CapKind::Val, CapKind::Val));
    }

    #[test]
    fn passing_iso_to_iso_allowed_linear() {
        let m = AliasMatrix::pony6();
        assert!(m.can_pass_through(CapKind::Iso, CapKind::Iso));
    }

    #[test]
    fn passing_val_to_iso_blocked() {
        let m = AliasMatrix::pony6();
        // val is aliasable, but iso-param needs exclusive access ; val can't promise that.
        assert!(!m.can_pass_through(CapKind::Val, CapKind::Iso));
    }

    #[test]
    fn passing_iso_to_val_allowed_via_freeze() {
        let m = AliasMatrix::pony6();
        // val-param accepts anything aliasable (iso can freeze).
        assert!(m.can_pass_through(CapKind::Iso, CapKind::Val));
    }

    #[test]
    fn iter_returns_six_rows() {
        let m = AliasMatrix::pony6();
        assert_eq!(m.iter().count(), 6);
    }

    #[test]
    fn alias_rights_predicates() {
        let iso = AliasRights::for_cap(CapKind::Iso);
        assert!(!iso.can_alias());
        assert!(iso.can_mutate());
        let val = AliasRights::for_cap(CapKind::Val);
        assert!(val.can_alias());
        assert!(!val.can_mutate());
    }
}
