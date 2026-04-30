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
//!
//! § DefId-ATTRIBUTION (T11-D287 · W-E5-4)
//!   Plain `DefId(u32)` numbering is deterministic GIVEN iteration-order, but the numeric
//!   value alone is not enough for cross-run / cross-host fingerprint comparison because
//!   `lasso::Rodeo` Spur-values are HashMap-derived and may shift across processes. The
//!   fixed-point gate compares two compiler outputs byte-by-byte ; if any DefId reference
//!   serializes via the unstable Spur path, the gate falsely diverges.
//!
//!   Solution : every `DefId` is paired with an [`AttributionKey`] — a content-stable
//!   tuple of `(DefKind, span_start, name_offset)` recorded at allocation-time. The
//!   attribution-table ([`DefIdAttribution`]) is therefore a deterministic
//!   source-position-based identity that downstream emission can use for fingerprinting
//!   instead of the raw `u32` ordinal.
//!
//!   § INVARIANT  Same source-text → same `AttributionKey` per `DefId`, regardless of
//!   `Spur` values, allocator state, or HashMap seeds.

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

/// Coarse classification of a definition, fixed-encoded as a `u8` so that the
/// attribution-key has a stable byte-shape across runs / hosts.
///
/// § INVARIANT  The numeric encoding of every variant is part of the stable
/// fingerprint contract — variants may be added (with new `u8` values), but
/// existing values must NEVER be re-numbered.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum DefKind {
    /// `fn` item.
    Fn = 0,
    /// `struct` item.
    Struct = 1,
    /// `enum` item.
    Enum = 2,
    /// `enum`-variant.
    Variant = 3,
    /// `interface` item.
    Interface = 4,
    /// Associated type declaration (interface).
    AssocTypeDecl = 5,
    /// Associated type definition (impl).
    AssocTypeDef = 6,
    /// `effect` item.
    Effect = 7,
    /// `handler` item.
    Handler = 8,
    /// `type` alias item.
    TypeAlias = 9,
    /// `const` item.
    Const = 10,
    /// Nested `module` item.
    Module = 11,
    /// Anything else (fallback — must NEVER be used for a definition that the
    /// fixed-point gate may need to compare ; reserved for synthetic def-ids).
    Other = 255,
}

impl DefKind {
    /// Numeric encoding (stable across versions for variants below `255`).
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }
}

/// Content-stable identity for a `DefId`.
///
/// § FIELDS
///   - `kind`        : coarse definition-class (see [`DefKind`]).
///   - `span_start`  : byte-offset of the item-span START in the source file.
///                     Canonical source-position — invariant across compilation runs
///                     for unchanged source text.
///   - `span_end`    : byte-offset one-past-the-end of the item-span. Combined with
///                     `span_start` this gives the item's full byte-length — two items
///                     that begin at the same position but differ in length are
///                     distinguishable.
///   - `name_offset` : byte-offset of the item NAME token in the source file.
///   - `name_end`    : byte-offset one-past-the-end of the item NAME. Combined with
///                     `name_offset` this captures the IDENTIFIER LENGTH so two items
///                     with same-position-but-different-length names are
///                     distinguishable WITHOUT depending on `Spur` numerics.
///
/// § WHY NOT Symbol  `Symbol(Spur)` is HashMap-derived inside `lasso::Rodeo` ; its
/// numeric value is not stable across compiler runs. Source byte-offsets ARE stable
/// for unchanged source text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct AttributionKey {
    pub kind: DefKind,
    pub span_start: u32,
    pub span_end: u32,
    pub name_offset: u32,
    pub name_end: u32,
}

impl AttributionKey {
    /// Build a fresh attribution-key.
    #[must_use]
    pub const fn new(
        kind: DefKind,
        span_start: u32,
        span_end: u32,
        name_offset: u32,
        name_end: u32,
    ) -> Self {
        Self {
            kind,
            span_start,
            span_end,
            name_offset,
            name_end,
        }
    }

    /// Stable 64-bit content-hash of this key. Used by the fixed-point gate to
    /// emit a `DefId → fingerprint` mapping that does NOT depend on `lasso` Spur
    /// values or HashMap seeds.
    ///
    /// § ALGORITHM  FNV-1a 64-bit on the byte-shape (`u8` kind + 4 bytes per
    /// span/name offset, little-endian). Pure ; no allocator state ; no
    /// platform-conditional behavior.
    #[must_use]
    pub const fn content_hash(self) -> u64 {
        const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
        const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

        let mut h = FNV_OFFSET;
        let span_start = self.span_start.to_le_bytes();
        let span_end = self.span_end.to_le_bytes();
        let name_offset = self.name_offset.to_le_bytes();
        let name_end = self.name_end.to_le_bytes();
        let bytes = [
            self.kind.as_u8(),
            span_start[0],
            span_start[1],
            span_start[2],
            span_start[3],
            span_end[0],
            span_end[1],
            span_end[2],
            span_end[3],
            name_offset[0],
            name_offset[1],
            name_offset[2],
            name_offset[3],
            name_end[0],
            name_end[1],
            name_end[2],
            name_end[3],
        ];
        let mut i = 0;
        while i < bytes.len() {
            h ^= bytes[i] as u64;
            h = h.wrapping_mul(FNV_PRIME);
            i += 1;
        }
        h
    }
}

/// Sorted, contiguous mapping from `DefId` → [`AttributionKey`].
///
/// Backed by a `Vec` indexed by the `DefId.0` ordinal — lookups are `O(1)`. The
/// indexing scheme depends on `fresh_def_id` being the ONLY allocation path, which
/// is enforced by the arena.
///
/// § DETERMINISM  Iteration order = DefId-allocation order = source-traversal order
/// of the lowering pass. No HashMap involvement at any layer.
#[derive(Debug, Default, Clone)]
pub struct DefIdAttribution {
    keys: Vec<AttributionKey>,
}

impl DefIdAttribution {
    /// Build an empty attribution table.
    #[must_use]
    pub const fn new() -> Self {
        Self { keys: Vec::new() }
    }

    /// Number of recorded entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.keys.len()
    }

    /// `true` iff no entries recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    /// Record the attribution-key for the next `DefId` slot. Must be called
    /// exactly once per `fresh_def_id` allocation, in allocation-order.
    pub fn record(&mut self, key: AttributionKey) {
        self.keys.push(key);
    }

    /// Look up the attribution-key for a given `DefId`. Returns `None` for the
    /// unresolved sentinel and for any `DefId` that was not recorded (this is a
    /// bug — every allocated `DefId` should have an attribution recorded).
    #[must_use]
    pub fn get(&self, def: DefId) -> Option<AttributionKey> {
        if def.is_unresolved() {
            return None;
        }
        self.keys.get(def.0 as usize).copied()
    }

    /// Iterate all `(DefId, AttributionKey)` pairs in allocation-order.
    pub fn iter(&self) -> impl Iterator<Item = (DefId, AttributionKey)> + '_ {
        self.keys
            .iter()
            .enumerate()
            .map(|(i, k)| (DefId(i as u32), *k))
    }

    /// Return the recorded keys sorted by canonical-source-position.
    ///
    /// § PURPOSE  Downstream emitters that want a deterministic SOURCE-ORDER
    /// listing (e.g. fingerprint manifest in the fixed-point gate) call this
    /// instead of `iter()`. Ties on `span_start` are broken by `name_offset`,
    /// then by `kind.as_u8()` — ALL fields participate in the ordering, so the
    /// result is total-ordered and reproducible.
    #[must_use]
    pub fn sorted_by_source_position(&self) -> Vec<(DefId, AttributionKey)> {
        let mut entries: Vec<(DefId, AttributionKey)> = self.iter().collect();
        entries.sort_by(|a, b| {
            a.1.span_start
                .cmp(&b.1.span_start)
                .then_with(|| a.1.name_offset.cmp(&b.1.name_offset))
                .then_with(|| a.1.kind.as_u8().cmp(&b.1.kind.as_u8()))
                .then_with(|| a.0.cmp(&b.0))
        });
        entries
    }

    /// Stable 64-bit content-hash over ALL attributions.
    ///
    /// Folds each `AttributionKey::content_hash` into a running FNV-1a accumulator
    /// in `DefId` allocation-order. Suitable for a one-line fingerprint that
    /// downstream tooling (fixed-point gate, golden-file diff) can compare across
    /// runs.
    #[must_use]
    pub fn module_fingerprint(&self) -> u64 {
        const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
        const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

        let mut h = FNV_OFFSET;
        for k in &self.keys {
            let kh = k.content_hash();
            // mix the per-entry hash byte-by-byte
            for b in kh.to_le_bytes() {
                h ^= u64::from(b);
                h = h.wrapping_mul(FNV_PRIME);
            }
        }
        h
    }
}

/// Monotonic counter for `HirId` / `DefId` allocation during a single lowering pass,
/// plus the attribution-table that records a content-stable identity per `DefId`.
#[derive(Debug, Default, Clone)]
pub struct HirArena {
    hir_counter: u32,
    def_counter: u32,
    attribution: DefIdAttribution,
}

impl HirArena {
    /// Build an empty arena.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            hir_counter: 0,
            def_counter: 0,
            attribution: DefIdAttribution::new(),
        }
    }

    /// Allocate a fresh `HirId`.
    pub fn fresh_hir_id(&mut self) -> HirId {
        let id = HirId(self.hir_counter);
        self.hir_counter = self.hir_counter.saturating_add(1);
        id
    }

    /// Allocate a fresh `DefId` WITHOUT attribution. Prefer `fresh_def_id_with`
    /// — this overload is retained for legacy call-sites + tests where no source
    /// span is available (synthetic / placeholder defs).
    ///
    /// § INVARIANT  Synthetic defs allocated through this path get an
    /// [`AttributionKey`] with `kind = DefKind::Other` and `span_start =
    /// name_offset = u32::MAX`. They are still distinguishable across runs
    /// because their `DefId` ordinal is deterministic ; what they lose is the
    /// stable source-position-based fingerprint.
    pub fn fresh_def_id(&mut self) -> DefId {
        let id = DefId(self.def_counter);
        self.def_counter = self.def_counter.saturating_add(1);
        self.attribution.record(AttributionKey::new(
            DefKind::Other,
            u32::MAX,
            u32::MAX,
            u32::MAX,
            u32::MAX,
        ));
        id
    }

    /// Allocate a fresh `DefId` and record its content-stable attribution-key.
    /// Lowering call-sites should prefer this over `fresh_def_id` so that the
    /// fixed-point gate can fingerprint the module without depending on `Spur`
    /// numerics.
    pub fn fresh_def_id_with(&mut self, key: AttributionKey) -> DefId {
        let id = DefId(self.def_counter);
        self.def_counter = self.def_counter.saturating_add(1);
        self.attribution.record(key);
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

    /// Borrow the attribution-table.
    #[must_use]
    pub const fn attribution(&self) -> &DefIdAttribution {
        &self.attribution
    }
}

#[cfg(test)]
mod tests {
    use super::{AttributionKey, DefId, DefIdAttribution, DefKind, HirArena, HirId};

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

    // ─ § T11-D287 (W-E5-4) attribution stability tests ─────────────────────

    /// § same-source-produces-same-DefIds — sequencing two identical sequences of
    /// `fresh_def_id_with` calls gives bit-identical DefId-attribution tables.
    #[test]
    fn same_source_produces_same_def_ids() {
        let inputs = [
            AttributionKey::new(DefKind::Fn, 0, 16, 3, 8),
            AttributionKey::new(DefKind::Struct, 32, 64, 39, 43),
            AttributionKey::new(DefKind::Enum, 100, 130, 105, 110),
        ];

        let mut a1 = HirArena::new();
        let mut a2 = HirArena::new();
        let mut ids1 = Vec::new();
        let mut ids2 = Vec::new();
        for k in inputs {
            ids1.push(a1.fresh_def_id_with(k));
            ids2.push(a2.fresh_def_id_with(k));
        }

        assert_eq!(ids1, ids2, "DefId sequence must match across runs");
        assert_eq!(
            a1.attribution().module_fingerprint(),
            a2.attribution().module_fingerprint(),
            "module-fingerprint must be stable"
        );
        for (id1, id2) in ids1.iter().zip(ids2.iter()) {
            assert_eq!(
                a1.attribution().get(*id1),
                a2.attribution().get(*id2),
                "per-DefId attribution must match"
            );
        }
    }

    /// § sorted-iteration — `sorted_by_source_position` orders strictly by
    /// `(span_start, name_offset, kind)` regardless of allocation order.
    #[test]
    fn sorted_iteration_is_canonical() {
        let mut a = HirArena::new();
        // Alloc OUT-OF-ORDER on purpose — span_start is shuffled.
        let _d2 = a.fresh_def_id_with(AttributionKey::new(DefKind::Struct, 200, 230, 207, 213));
        let _d0 = a.fresh_def_id_with(AttributionKey::new(DefKind::Fn, 10, 28, 13, 18));
        let _d1 = a.fresh_def_id_with(AttributionKey::new(DefKind::Enum, 100, 130, 105, 110));

        let sorted = a.attribution().sorted_by_source_position();
        let span_order: Vec<u32> = sorted.iter().map(|(_, k)| k.span_start).collect();
        assert_eq!(
            span_order,
            vec![10, 100, 200],
            "sorted iteration must order by span_start regardless of alloc-order"
        );
    }

    /// § content-hash-based-id-stable — the attribution-key content-hash is the
    /// same for two arenas that allocated the same `(kind, span_start,
    /// name_offset)` triple.
    #[test]
    fn content_hash_based_id_stable() {
        let k = AttributionKey::new(DefKind::Fn, 42, 60, 45, 50);
        let h1 = k.content_hash();
        let h2 = k.content_hash();
        let h3 = AttributionKey::new(DefKind::Fn, 42, 60, 45, 50).content_hash();
        assert_eq!(h1, h2);
        assert_eq!(h1, h3);

        // Different kind → different hash (single-bit-flip discrimination).
        let h_diff = AttributionKey::new(DefKind::Struct, 42, 60, 45, 50).content_hash();
        assert_ne!(h1, h_diff, "kind must affect content-hash");

        // Different span_start → different hash.
        let h_span = AttributionKey::new(DefKind::Fn, 43, 60, 45, 50).content_hash();
        assert_ne!(h1, h_span, "span_start must affect content-hash");

        // Different name_end (identifier-length change) → different hash.
        let h_name_end = AttributionKey::new(DefKind::Fn, 42, 60, 45, 51).content_hash();
        assert_ne!(h1, h_name_end, "name_end must affect content-hash");
    }

    /// § cross-run-determinism — building two attribution tables in DIFFERENT
    /// allocation-orders but with the same SOURCE positions yields identical
    /// `module_fingerprint` IF the sorted-canonical view is what we hash. This
    /// test guards the contract : the stream-fingerprint depends on alloc-order,
    /// while the canonical-fingerprint does NOT.
    #[test]
    fn cross_run_determinism_canonical_view() {
        let inputs_a = [
            AttributionKey::new(DefKind::Fn, 0, 16, 3, 8),
            AttributionKey::new(DefKind::Struct, 32, 64, 39, 43),
            AttributionKey::new(DefKind::Enum, 100, 130, 105, 110),
        ];
        let inputs_b = [
            AttributionKey::new(DefKind::Enum, 100, 130, 105, 110),
            AttributionKey::new(DefKind::Fn, 0, 16, 3, 8),
            AttributionKey::new(DefKind::Struct, 32, 64, 39, 43),
        ];

        let mut a = DefIdAttribution::new();
        let mut b = DefIdAttribution::new();
        for k in inputs_a {
            a.record(k);
        }
        for k in inputs_b {
            b.record(k);
        }

        // Sorted-canonical-view collapses allocation-order : both arenas yield
        // the same content-stable sequence.
        let canonical_a: Vec<AttributionKey> =
            a.sorted_by_source_position().into_iter().map(|(_, k)| k).collect();
        let canonical_b: Vec<AttributionKey> =
            b.sorted_by_source_position().into_iter().map(|(_, k)| k).collect();
        assert_eq!(
            canonical_a, canonical_b,
            "canonical view must be alloc-order-invariant"
        );

        // Stream fingerprint differs across the two orderings (sanity-check —
        // proves the canonical view is doing real work).
        assert_ne!(
            a.module_fingerprint(),
            b.module_fingerprint(),
            "stream fingerprint depends on alloc-order"
        );
    }

    /// § regression — `fresh_def_id` legacy path still works (DefKind::Other +
    /// MAX sentinels) so existing call-sites that don't yet pass a key do not
    /// silently corrupt the attribution table.
    #[test]
    fn legacy_fresh_def_id_records_sentinel_attribution() {
        let mut a = HirArena::new();
        let d = a.fresh_def_id();
        let key = a.attribution().get(d).expect("attribution recorded");
        assert_eq!(key.kind, DefKind::Other);
        assert_eq!(key.span_start, u32::MAX);
        assert_eq!(key.name_offset, u32::MAX);
        assert_eq!(a.attribution().len(), 1);
    }

    /// § attribution-table-len-tracks-def-count — every fresh_def_id* call
    /// records exactly one entry, so `attribution().len() == def_count()`.
    #[test]
    fn attribution_len_tracks_def_count() {
        let mut a = HirArena::new();
        for i in 0..5u32 {
            a.fresh_def_id_with(AttributionKey::new(
                DefKind::Fn,
                i * 10,
                i * 10 + 8,
                i * 10 + 3,
                i * 10 + 7,
            ));
        }
        assert_eq!(a.attribution().len(), 5);
        assert_eq!(a.attribution().len() as u32, a.def_count());
    }

    /// § iter-yields-allocation-order — the `iter()` view returns entries in
    /// `DefId.0` order, matching how downstream emitters scan the table.
    #[test]
    fn iter_yields_allocation_order() {
        let mut a = HirArena::new();
        let d0 = a.fresh_def_id_with(AttributionKey::new(DefKind::Fn, 5, 20, 8, 13));
        let d1 = a.fresh_def_id_with(AttributionKey::new(DefKind::Struct, 2, 30, 9, 14));
        let d2 = a.fresh_def_id_with(AttributionKey::new(DefKind::Enum, 100, 130, 103, 108));

        let collected: Vec<DefId> = a.attribution().iter().map(|(id, _)| id).collect();
        assert_eq!(collected, vec![d0, d1, d2]);
    }
}
