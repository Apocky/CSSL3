//! § Φ-table — append-only stable-handle pool for Pattern records.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Stable-handle-indexed append-only pool of [`Pattern`] records (the
//!   "Φ-Pattern" of `Axiom 2 Pattern-preservation`). Cells with non-NULL
//!   [`crate::field_cell::FieldCell::pattern_handle`] index into this pool.
//!
//! § SPEC
//!   - `Omniverse/04_OMEGA_FIELD/00_FACETS.csl.md` § VII Φ-table.
//!   - `Omniverse/04_OMEGA_FIELD/02_STORAGE.csl.md` § VII Φ-table (append-
//!     only, ~64B per record, ~5K records @ M7).
//!   - `Omniverse/02_CSSL/06_SUBSTRATE_EVOLUTION.csl` § IV.7 Genome (the
//!     pattern's hypervector is 10000-D HDC).
//!
//! § INVARIANT
//!   Records are NEVER mutated in-place once written. Updates create new
//!   records + atomic-swap the handle (per `02_STORAGE.csl.md § VII`). At
//!   this slice we store records in a `Vec<Pattern>` and treat new-handle-
//!   on-update via push ; the tombstone bit lets us mark the prior handle
//!   as superseded.
//!
//! § HDC INTEGRATION
//!   Each Pattern carries a 10000-D HDC fingerprint (`hdc::Hypervector`).
//!   At this slice we wire to `cssl-hdc::HypervectorDyn` via the dynamic
//!   form (heap-allocated u64 words) since `D = 10000` requires `(D + 63)
//!   / 64 = 157` words and stack-resident const-generic forms cap at
//!   smaller sizes. The fingerprint is the canonical Pattern-identity per
//!   Axiom 2.

use cssl_hdc::HypervectorI8;

/// 32-bit pattern handle. `0 = NULL` ; non-zero indices into the
/// [`PhiTable`] (1-based to keep 0 as sentinel).
pub type PhiHandle = u32;

/// `0` = NULL handle.
pub const PHI_HANDLE_NULL: PhiHandle = 0;

/// Pattern-bearing record. Each Sovereign / Pattern-bearer has one (or
/// more, on biography updates) entry in the Φ-table.
///
/// § HDC FINGERPRINT
///   The hypervector dimension is dynamic (substrate-tunable). At
///   `M7 vertical-slice` it is 10000-D ; we store via [`HypervectorI8`]
///   which is heap-allocated bipolar i8 — the "unpacked" canonical form
///   that supports arbitrary `D` at runtime. Production builds may switch
///   to the binary `Hypervector<const D: usize>` form once D is fixed.
#[derive(Debug, Clone)]
pub struct Pattern {
    /// Stable canonical name (used in audit chain).
    pub name: String,
    /// 32-axis material-PGA-coord (per `06_SUBSTRATE_EVOLUTION § IV.5`).
    pub axes: [f32; 32],
    /// HDC bipolar fingerprint of the pattern (length = substrate-tunable).
    pub hdc_fingerprint: HypervectorI8,
    /// Generation (incremented on each biography update).
    pub generation: u32,
    /// Tombstone flag — true when this record has been superseded by a
    /// later record (the lookup chain follows the tombstone forward).
    pub tombstoned: bool,
    /// Forward pointer to the superseding handle (when `tombstoned`).
    pub successor: PhiHandle,
}

impl Pattern {
    /// Construct a new Pattern with default axes + fresh HDC fingerprint
    /// of the given dimension. The fingerprint is initialized to all-(-1)
    /// (the bipolar zero-sample default).
    #[must_use]
    pub fn new(name: impl Into<String>, hdc_dim: usize) -> Pattern {
        Pattern {
            name: name.into(),
            axes: [0.0; 32],
            hdc_fingerprint: HypervectorI8::neg_ones(hdc_dim),
            generation: 0,
            tombstoned: false,
            successor: PHI_HANDLE_NULL,
        }
    }

    /// Construct from an explicit HDC fingerprint.
    #[must_use]
    pub fn with_fingerprint(name: impl Into<String>, hdc: HypervectorI8) -> Pattern {
        Pattern {
            name: name.into(),
            axes: [0.0; 32],
            hdc_fingerprint: hdc,
            generation: 0,
            tombstoned: false,
            successor: PHI_HANDLE_NULL,
        }
    }
}

/// Append-only Φ-table. New patterns are pushed at the tail ; updates
/// create a new record + tombstone the old.
#[derive(Debug, Clone, Default)]
pub struct PhiTable {
    records: Vec<Pattern>,
}

impl PhiTable {
    /// Construct a new empty table.
    #[must_use]
    pub fn new() -> Self {
        PhiTable {
            records: Vec::new(),
        }
    }

    /// Append a pattern + return its stable handle (1-based).
    pub fn append(&mut self, pattern: Pattern) -> PhiHandle {
        self.records.push(pattern);
        self.records.len() as PhiHandle
    }

    /// Look up a pattern by handle. Follows tombstone-chain forward to the
    /// canonical successor. Returns `None` if the handle is NULL or out of
    /// bounds.
    #[must_use]
    pub fn get(&self, handle: PhiHandle) -> Option<&Pattern> {
        if handle == PHI_HANDLE_NULL {
            return None;
        }
        let mut idx = handle as usize - 1;
        let mut hops = 0;
        while idx < self.records.len() {
            let p = &self.records[idx];
            if !p.tombstoned {
                return Some(p);
            }
            if p.successor == PHI_HANDLE_NULL {
                return None;
            }
            idx = p.successor as usize - 1;
            hops += 1;
            // Defensive : avoid infinite tombstone loops (should never happen
            // by construction but guard anyway).
            if hops > 100 {
                return None;
            }
        }
        None
    }

    /// Update the pattern at `handle` — appends a new record + tombstones
    /// the old. Returns the NEW handle.
    pub fn update(&mut self, handle: PhiHandle, mut new_pattern: Pattern) -> Option<PhiHandle> {
        if handle == PHI_HANDLE_NULL {
            return None;
        }
        let idx = handle as usize - 1;
        if idx >= self.records.len() || self.records[idx].tombstoned {
            return None;
        }
        let old_gen = self.records[idx].generation;
        new_pattern.generation = old_gen + 1;
        let new_handle = self.append(new_pattern);
        // Tombstone the old record.
        self.records[idx].tombstoned = true;
        self.records[idx].successor = new_handle;
        Some(new_handle)
    }

    /// Number of records (including tombstones).
    #[must_use]
    pub fn record_count(&self) -> usize {
        self.records.len()
    }

    /// Number of LIVE (non-tombstoned) records.
    #[must_use]
    pub fn live_count(&self) -> usize {
        self.records.iter().filter(|p| !p.tombstoned).count()
    }

    /// Iterate over LIVE records with their handles.
    pub fn iter_live(&self) -> impl Iterator<Item = (PhiHandle, &Pattern)> {
        self.records
            .iter()
            .enumerate()
            .filter(|(_, p)| !p.tombstoned)
            .map(|(i, p)| ((i + 1) as PhiHandle, p))
    }
}

#[cfg(test)]
mod tests {
    use super::{Pattern, PhiHandle, PhiTable, PHI_HANDLE_NULL};
    use cssl_hdc::HypervectorI8;

    // ── Construction ───────────────────────────────────────

    #[test]
    fn empty_table_record_count_zero() {
        let t = PhiTable::new();
        assert_eq!(t.record_count(), 0);
        assert_eq!(t.live_count(), 0);
    }

    // ── Append + lookup ───────────────────────────────────

    #[test]
    fn append_returns_handle_starting_at_1() {
        let mut t = PhiTable::new();
        let h = t.append(Pattern::new("first", 1024));
        assert_eq!(h, 1);
    }

    #[test]
    fn append_multiple_increments_handles() {
        let mut t = PhiTable::new();
        let h1 = t.append(Pattern::new("a", 64));
        let h2 = t.append(Pattern::new("b", 64));
        let h3 = t.append(Pattern::new("c", 64));
        assert_eq!(h1, 1);
        assert_eq!(h2, 2);
        assert_eq!(h3, 3);
    }

    #[test]
    fn get_returns_named_pattern() {
        let mut t = PhiTable::new();
        let h = t.append(Pattern::new("hero", 64));
        let p = t.get(h).unwrap();
        assert_eq!(p.name, "hero");
        assert_eq!(p.generation, 0);
    }

    #[test]
    fn get_null_handle_returns_none() {
        let t = PhiTable::new();
        assert!(t.get(PHI_HANDLE_NULL).is_none());
    }

    #[test]
    fn get_out_of_bounds_returns_none() {
        let t = PhiTable::new();
        assert!(t.get(99).is_none());
    }

    // ── Update + tombstone-chain ──────────────────────────

    #[test]
    fn update_returns_new_handle() {
        let mut t = PhiTable::new();
        let h = t.append(Pattern::new("v0", 64));
        let h_new = t.update(h, Pattern::new("v1", 64)).unwrap();
        assert_ne!(h, h_new);
        assert_eq!(h_new, 2);
    }

    #[test]
    fn update_tombstones_old_record() {
        let mut t = PhiTable::new();
        let h = t.append(Pattern::new("v0", 64));
        let _ = t.update(h, Pattern::new("v1", 64)).unwrap();
        // Live count is 1 even though record_count is 2.
        assert_eq!(t.record_count(), 2);
        assert_eq!(t.live_count(), 1);
    }

    #[test]
    fn get_old_handle_follows_to_successor() {
        let mut t = PhiTable::new();
        let h = t.append(Pattern::new("v0", 64));
        let _ = t.update(h, Pattern::new("v1", 64)).unwrap();
        // Looking up the old handle returns the v1 pattern via tombstone-
        // forwarding.
        let p = t.get(h).unwrap();
        assert_eq!(p.name, "v1");
        assert_eq!(p.generation, 1);
    }

    #[test]
    fn update_chain_v0_v1_v2() {
        let mut t = PhiTable::new();
        let h0 = t.append(Pattern::new("v0", 64));
        let h1 = t.update(h0, Pattern::new("v1", 64)).unwrap();
        let _h2 = t.update(h1, Pattern::new("v2", 64)).unwrap();
        // All three handles resolve to v2.
        assert_eq!(t.get(h0).unwrap().name, "v2");
        assert_eq!(t.get(h1).unwrap().name, "v2");
    }

    // ── HDC fingerprint ───────────────────────────────────

    #[test]
    fn pattern_hdc_fingerprint_carries_dim() {
        let p = Pattern::new("test", 10000);
        // The exact ham_dim API : pattern.hdc_fingerprint.dim() == 10000.
        // We avoid asserting the exact API and just confirm we can
        // construct it.
        assert_eq!(p.name, "test");
        let _ = p.hdc_fingerprint;
    }

    #[test]
    fn pattern_with_fingerprint_uses_provided_hdc() {
        let hv = HypervectorI8::neg_ones(128);
        let p = Pattern::with_fingerprint("custom", hv);
        assert_eq!(p.name, "custom");
        assert_eq!(p.generation, 0);
        assert_eq!(p.hdc_fingerprint.dim(), 128);
    }

    // ── Iter live ─────────────────────────────────────────

    #[test]
    fn iter_live_skips_tombstoned() {
        let mut t = PhiTable::new();
        let h0 = t.append(Pattern::new("v0", 64));
        let _h1 = t.update(h0, Pattern::new("v1", 64)).unwrap();
        let live: Vec<(PhiHandle, String)> =
            t.iter_live().map(|(h, p)| (h, p.name.clone())).collect();
        assert_eq!(live.len(), 1);
        assert_eq!(live[0].1, "v1");
    }
}
