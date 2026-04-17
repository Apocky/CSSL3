//! The 6 Pony capabilities + common predicates.
//!
//! § SPEC : `specs/12_CAPABILITIES.csl` § THE SIX CAPABILITIES.

use core::fmt;

/// Pony-6 capability kind — the canonical set per `specs/12`.
///
/// The rows in the alias+deny matrix (`matrix.rs`) are indexed by this enum in
/// the declared order : `iso → trn → ref → val → box → tag`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum CapKind {
    /// `iso` — isolated / linear. No aliasing anywhere ; mutation allowed ; safe-to-send.
    Iso,
    /// `trn` — writable-unique-reference. Local aliasing ok ; no global aliasing ;
    /// mutable locally ; can freeze to `val`.
    Trn,
    /// `ref` — shared-mutable (Vale gen-ref). Aliasing everywhere ; mutation everywhere ;
    /// runtime deref-check required.
    Ref,
    /// `val` — deep-immutable. Aliasing everywhere ; no mutation ; safe-to-share.
    Val,
    /// `box` — read-only view. Aliasing everywhere ; no mutation ; accepts iso/trn/val.
    Box,
    /// `tag` — opaque handle. No data access ; identity-only. `Handle<T>` lowers here.
    Tag,
}

impl CapKind {
    /// All 6 capabilities in canonical order.
    pub const ALL: [Self; 6] = [
        Self::Iso,
        Self::Trn,
        Self::Ref,
        Self::Val,
        Self::Box,
        Self::Tag,
    ];

    /// Canonical source-form name (lowercase keyword as it appears in `.cssl` source).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Iso => "iso",
            Self::Trn => "trn",
            Self::Ref => "ref",
            Self::Val => "val",
            Self::Box => "box",
            Self::Tag => "tag",
        }
    }

    /// Parse a source-form name to a `CapKind`. Returns `None` on unknown input.
    #[must_use]
    pub fn from_str(s: &str) -> Option<Self> {
        Some(match s {
            "iso" => Self::Iso,
            "trn" => Self::Trn,
            "ref" => Self::Ref,
            "val" => Self::Val,
            "box" => Self::Box,
            "tag" => Self::Tag,
            _ => return None,
        })
    }

    /// Dense index in `[0..6)` matching `ALL`'s order — used by the alias-matrix.
    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::Iso => 0,
            Self::Trn => 1,
            Self::Ref => 2,
            Self::Val => 3,
            Self::Box => 4,
            Self::Tag => 5,
        }
    }

    /// `true` iff this cap is linear (must-consume-or-drop semantics).
    /// Per `specs/12` § THE SIX CAPABILITIES, only `iso` is linear.
    #[must_use]
    pub const fn is_linear(self) -> bool {
        matches!(self, Self::Iso)
    }

    /// `true` iff this cap permits mutation (locally).
    #[must_use]
    pub const fn is_mutable(self) -> bool {
        matches!(self, Self::Iso | Self::Trn | Self::Ref)
    }

    /// `true` iff this cap permits unrestricted aliasing across thread-boundaries.
    /// `iso` is safe-to-send because it denies aliasing ; `val` / `box` / `tag`
    /// are safe because they are read-only or opaque.
    #[must_use]
    pub const fn is_send_safe(self) -> bool {
        matches!(self, Self::Iso | Self::Val | Self::Box | Self::Tag)
    }

    /// `true` iff this cap requires runtime deref-check (Vale gen-ref).
    #[must_use]
    pub const fn requires_gen_check(self) -> bool {
        matches!(self, Self::Ref)
    }

    /// `true` iff this cap permits reading the wrapped data. `tag` denies reading.
    #[must_use]
    pub const fn can_read(self) -> bool {
        !matches!(self, Self::Tag)
    }
}

impl fmt::Display for CapKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Small set of capabilities — bit-packed over the 6-element universe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CapSet(u8);

impl CapSet {
    /// Empty set.
    #[must_use]
    pub const fn empty() -> Self {
        Self(0)
    }

    /// Set containing a single capability.
    #[must_use]
    pub const fn single(c: CapKind) -> Self {
        Self(1 << c.index())
    }

    /// Set containing all 6 capabilities.
    #[must_use]
    pub const fn full() -> Self {
        Self(0b0011_1111)
    }

    /// `true` iff `c` is in the set.
    #[must_use]
    pub const fn contains(self, c: CapKind) -> bool {
        (self.0 >> c.index()) & 1 != 0
    }

    /// Return a new set with `c` added.
    #[must_use]
    pub const fn with(self, c: CapKind) -> Self {
        Self(self.0 | (1 << c.index()))
    }

    /// Union of two sets.
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Intersection of two sets.
    #[must_use]
    pub const fn intersection(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }

    /// `true` iff the set contains no capabilities.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

#[cfg(test)]
mod tests {
    use super::{CapKind, CapSet};

    #[test]
    fn all_six_caps_present() {
        assert_eq!(CapKind::ALL.len(), 6);
    }

    #[test]
    fn cap_roundtrip_through_str() {
        for cap in CapKind::ALL {
            assert_eq!(CapKind::from_str(cap.as_str()), Some(cap));
        }
    }

    #[test]
    fn unknown_str_returns_none() {
        assert_eq!(CapKind::from_str("blah"), None);
    }

    #[test]
    fn index_matches_all_order() {
        for (i, c) in CapKind::ALL.iter().enumerate() {
            assert_eq!(c.index(), i);
        }
    }

    #[test]
    fn only_iso_is_linear() {
        assert!(CapKind::Iso.is_linear());
        for c in [
            CapKind::Trn,
            CapKind::Ref,
            CapKind::Val,
            CapKind::Box,
            CapKind::Tag,
        ] {
            assert!(!c.is_linear(), "{c:?} should not be linear");
        }
    }

    #[test]
    fn mutation_set() {
        assert!(CapKind::Iso.is_mutable());
        assert!(CapKind::Trn.is_mutable());
        assert!(CapKind::Ref.is_mutable());
        assert!(!CapKind::Val.is_mutable());
        assert!(!CapKind::Box.is_mutable());
        assert!(!CapKind::Tag.is_mutable());
    }

    #[test]
    fn send_safe_set() {
        assert!(CapKind::Iso.is_send_safe());
        assert!(CapKind::Val.is_send_safe());
        assert!(CapKind::Box.is_send_safe());
        assert!(CapKind::Tag.is_send_safe());
        assert!(!CapKind::Ref.is_send_safe());
        assert!(!CapKind::Trn.is_send_safe());
    }

    #[test]
    fn only_ref_requires_gen_check() {
        assert!(CapKind::Ref.requires_gen_check());
        for c in [
            CapKind::Iso,
            CapKind::Trn,
            CapKind::Val,
            CapKind::Box,
            CapKind::Tag,
        ] {
            assert!(!c.requires_gen_check());
        }
    }

    #[test]
    fn tag_cannot_read() {
        assert!(!CapKind::Tag.can_read());
        for c in [
            CapKind::Iso,
            CapKind::Trn,
            CapKind::Ref,
            CapKind::Val,
            CapKind::Box,
        ] {
            assert!(c.can_read());
        }
    }

    #[test]
    fn cap_set_operations() {
        let empty = CapSet::empty();
        assert!(empty.is_empty());
        let just_iso = CapSet::single(CapKind::Iso);
        assert!(just_iso.contains(CapKind::Iso));
        assert!(!just_iso.contains(CapKind::Ref));
        let iso_plus_val = just_iso.with(CapKind::Val);
        assert!(iso_plus_val.contains(CapKind::Iso));
        assert!(iso_plus_val.contains(CapKind::Val));
        let all = CapSet::full();
        for c in CapKind::ALL {
            assert!(all.contains(c));
        }
    }

    #[test]
    fn cap_set_union_intersection() {
        let a = CapSet::single(CapKind::Iso).with(CapKind::Val);
        let b = CapSet::single(CapKind::Val).with(CapKind::Box);
        let u = a.union(b);
        assert!(u.contains(CapKind::Iso));
        assert!(u.contains(CapKind::Val));
        assert!(u.contains(CapKind::Box));
        let i = a.intersection(b);
        assert!(!i.contains(CapKind::Iso));
        assert!(i.contains(CapKind::Val));
        assert!(!i.contains(CapKind::Box));
    }
}
