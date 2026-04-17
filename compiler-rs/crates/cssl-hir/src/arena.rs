//! HIR identifier newtypes + simple arena storage.
//!
//! § DESIGN
//!   Every HIR node has a stable `HirId` : a dense `u32` assigned at lowering time.
//!   Item-level nodes additionally carry a `DefId` — the identity used by name-resolution
//!   and cross-item references. `DefId` ⊆ `HirId` (every definition is a HIR node) but
//!   not vice versa (expressions and patterns have `HirId`s but no `DefId`).
//!
//! § ARENA
//!   For T3.3 the arena is a `Vec` per node-category in `HirModule`. At T3.4+ we may
//!   move to a typed-arena (`typed_arena`) if compilation-time profiles show allocator
//!   pressure ; the public `HirId` API stays stable across the refactor.

use core::fmt;

/// Monotonic identifier for any HIR node. Assigned at lowering time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct HirId(pub u32);

impl HirId {
    /// Sentinel for a synthetic / placeholder node (no source position).
    pub const DUMMY: Self = Self(u32::MAX);

    /// `true` iff this is the sentinel dummy id.
    #[must_use]
    pub const fn is_dummy(self) -> bool {
        self.0 == u32::MAX
    }
}

impl fmt::Display for HirId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_dummy() {
            f.write_str("hir#<dummy>")
        } else {
            write!(f, "hir#{}", self.0)
        }
    }
}

/// Monotonic identifier for a definition-level HIR node (fn / struct / enum / …).
/// Distinct from `HirId` so that name-resolution can target only definitions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct DefId(pub u32);

impl DefId {
    /// Sentinel for an unresolved / external reference.
    pub const UNRESOLVED: Self = Self(u32::MAX);

    /// `true` iff this is the unresolved sentinel.
    #[must_use]
    pub const fn is_unresolved(self) -> bool {
        self.0 == u32::MAX
    }
}

impl fmt::Display for DefId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_unresolved() {
            f.write_str("def#<unresolved>")
        } else {
            write!(f, "def#{}", self.0)
        }
    }
}

/// Monotonic counter for `HirId` / `DefId` allocation during a single lowering pass.
#[derive(Debug, Default)]
pub struct HirArena {
    hir_counter: u32,
    def_counter: u32,
}

impl HirArena {
    /// Build an empty arena.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            hir_counter: 0,
            def_counter: 0,
        }
    }

    /// Allocate a fresh `HirId`.
    pub fn fresh_hir_id(&mut self) -> HirId {
        let id = HirId(self.hir_counter);
        self.hir_counter = self.hir_counter.saturating_add(1);
        id
    }

    /// Allocate a fresh `DefId`.
    pub fn fresh_def_id(&mut self) -> DefId {
        let id = DefId(self.def_counter);
        self.def_counter = self.def_counter.saturating_add(1);
        id
    }

    /// How many `HirId`s have been assigned so far.
    #[must_use]
    pub const fn hir_count(&self) -> u32 {
        self.hir_counter
    }

    /// How many `DefId`s have been assigned so far.
    #[must_use]
    pub const fn def_count(&self) -> u32 {
        self.def_counter
    }
}

#[cfg(test)]
mod tests {
    use super::{DefId, HirArena, HirId};

    #[test]
    fn dummy_sentinels_identify_correctly() {
        assert!(HirId::DUMMY.is_dummy());
        assert!(!HirId(0).is_dummy());
        assert!(DefId::UNRESOLVED.is_unresolved());
        assert!(!DefId(0).is_unresolved());
    }

    #[test]
    fn arena_counts_distinctly() {
        let mut a = HirArena::new();
        let h0 = a.fresh_hir_id();
        let h1 = a.fresh_hir_id();
        let d0 = a.fresh_def_id();
        assert_eq!(h0, HirId(0));
        assert_eq!(h1, HirId(1));
        assert_eq!(d0, DefId(0));
        assert_eq!(a.hir_count(), 2);
        assert_eq!(a.def_count(), 1);
    }

    #[test]
    fn display_formats_include_prefix() {
        assert_eq!(format!("{}", HirId(7)), "hir#7");
        assert_eq!(format!("{}", HirId::DUMMY), "hir#<dummy>");
        assert_eq!(format!("{}", DefId(3)), "def#3");
        assert_eq!(format!("{}", DefId::UNRESOLVED), "def#<unresolved>");
    }
}
