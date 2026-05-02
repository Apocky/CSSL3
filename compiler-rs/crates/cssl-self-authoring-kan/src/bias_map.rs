//! § bias_map.rs — TemplateBiasMap × Archetype tensor + Q14 weight-arithmetic.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § TemplateBiasMap
//!   Per-template-id × per-archetype bias-vector. The map drives procgen
//!   template-selection via [`TemplateBiasMap::priority_shift`] which returns
//!   the (saturating-clamped) Q14 bias for a (template, archetype) pair.
//!
//! § Q14 fixed-point semantics
//!   - Range : `i16` in `[-16383, +16383]` (≈ `[-1.0, +1.0]`).
//!   - One : 16384 (used as scaling factor in mul → div pattern).
//!   - All updates are saturating-clamped : runaway-amplification ¬ possible.
//!
//! § DESIGN (per memory_sawyer_pokemon_efficiency)
//!   - Sparse storage : `BTreeMap<(TemplateId, ArchetypeId), i16>`. Most
//!     templates × archetype cells are 0 (unbiased) ; storing only non-zero
//!     entries keeps the map compact even with 10^4+ templates.
//!   - Bit-pack archetype : `ArchetypeId(u8)` indexes into 8-slot LUT.
//!   - Differential updates : `apply_delta` is sign-aware saturating-add.

use std::collections::BTreeMap;

use thiserror::Error;

// ───────────────────────────────────────────────────────────────────────────
// § Q14 constants
// ───────────────────────────────────────────────────────────────────────────

/// Maximum positive Q14 bias (≈ +1.0). Saturating-clamp ceiling.
pub const BIAS_Q14_MAX: i16 = 16383;
/// Minimum negative Q14 bias (≈ -1.0). Saturating-clamp floor.
pub const BIAS_Q14_MIN: i16 = -16383;
/// Q14 unity (1.0). Used as scaling-factor in mul → div pattern.
pub const BIAS_Q14_ONE: i32 = 16384;

// ───────────────────────────────────────────────────────────────────────────
// § Index types
// ───────────────────────────────────────────────────────────────────────────

/// Template-id index (32-bit ; supports 4G distinct templates).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TemplateId(pub u32);

/// Archetype-id index (8-bit ; 8 canonical archetypes per ARCHETYPE_NAMES).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ArchetypeId(pub u8);

/// Number of canonical archetypes (matches ARCHETYPE_NAMES len).
pub const ARCHETYPE_COUNT: usize = 8;

/// LUT : archetype-id → name. Index = ArchetypeId.0 as usize.
///
/// § STABILITY : positions FROZEN. Adding an archetype = spec-amendment + new
/// slot at the next-index ; never renumber.
pub const ARCHETYPE_NAMES: [&str; ARCHETYPE_COUNT] = [
    "warrior",      // 0 · melee-focused · prefers proximity-combat templates
    "mage",         // 1 · ranged-magic · prefers spell-rich templates
    "rogue",        // 2 · stealth · prefers ambush + utility templates
    "ranger",       // 3 · ranged-physical · prefers exploration + bow templates
    "support",      // 4 · healing/buff · prefers party-oriented templates
    "explorer",     // 5 · puzzle/lore · prefers narrative + secret templates
    "diplomat",     // 6 · social · prefers dialogue + faction templates
    "engineer",     // 7 · craft/automation · prefers recipe + machine templates
];

impl ArchetypeId {
    /// O(1) name-lookup (returns "unknown" for out-of-range).
    pub fn name(self) -> &'static str {
        let i = self.0 as usize;
        if i < ARCHETYPE_COUNT {
            ARCHETYPE_NAMES[i]
        } else {
            "unknown"
        }
    }

    /// True iff the id is in canonical range.
    pub const fn is_canonical(self) -> bool {
        (self.0 as usize) < ARCHETYPE_COUNT
    }
}

// ───────────────────────────────────────────────────────────────────────────
// § KanBiasUpdate — signal × prompt-embedding → KAN-weight-delta.
// ───────────────────────────────────────────────────────────────────────────

/// Single bias-update record. Applied via [`TemplateBiasMap::apply_update`]
/// after k-anon-floor verification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KanBiasUpdate {
    pub template_id: TemplateId,
    pub archetype_id: ArchetypeId,
    /// Q14 bias-delta (signed).
    pub delta: i16,
    /// Audit-ring sequence-ref of the k-anon-floor-verification audit-entry.
    pub audit_ref: u64,
}

impl KanBiasUpdate {
    pub const fn new(
        template_id: TemplateId,
        archetype_id: ArchetypeId,
        delta: i16,
        audit_ref: u64,
    ) -> Self {
        Self {
            template_id,
            archetype_id,
            delta,
            audit_ref,
        }
    }
}

// ───────────────────────────────────────────────────────────────────────────
// § Errors
// ───────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum BiasUpdateError {
    /// Archetype-id was out of canonical [0..ARCHETYPE_COUNT) range.
    #[error("archetype-id {0} out of canonical range [0..{ARCHETYPE_COUNT})")]
    ArchetypeOutOfRange(u8),
    /// Q14 delta was outside the canonical [BIAS_Q14_MIN, BIAS_Q14_MAX] range.
    #[error("Q14 delta {0} outside [{BIAS_Q14_MIN}..{BIAS_Q14_MAX}]")]
    DeltaOutOfRange(i16),
}

// ───────────────────────────────────────────────────────────────────────────
// § TemplateBiasMap
// ───────────────────────────────────────────────────────────────────────────

/// Sparse (TemplateId · ArchetypeId) → Q14-bias map.
#[derive(Debug, Clone, Default)]
pub struct TemplateBiasMap {
    /// Sparse storage : only non-zero entries are present.
    cells: BTreeMap<(TemplateId, ArchetypeId), i16>,
    /// Monotone update-counter (used by Σ-Chain anchor cadence).
    update_count: u64,
}

impl TemplateBiasMap {
    pub fn new() -> Self {
        Self::default()
    }

    /// Total number of non-zero (template · archetype) cells.
    pub fn cell_count(&self) -> usize {
        self.cells.len()
    }

    /// Cumulative update-counter (monotone increment).
    pub const fn update_count(&self) -> u64 {
        self.update_count
    }

    /// Read the Q14 bias for a cell (zero if absent).
    pub fn priority_shift(&self, template: TemplateId, archetype: ArchetypeId) -> i16 {
        self.cells
            .get(&(template, archetype))
            .copied()
            .unwrap_or(0)
    }

    /// Apply a single bias-update with saturating-clamp arithmetic.
    ///
    /// § Algorithm
    ///   1. Validate archetype + delta range.
    ///   2. Saturating-add the delta to the existing cell value.
    ///   3. If result is 0 (within ±1 epsilon), prune the entry to keep the
    ///      sparse-map compact.
    ///   4. Bump update_count.
    pub fn apply_update(&mut self, update: KanBiasUpdate) -> Result<(), BiasUpdateError> {
        if !update.archetype_id.is_canonical() {
            return Err(BiasUpdateError::ArchetypeOutOfRange(update.archetype_id.0));
        }
        if !(BIAS_Q14_MIN..=BIAS_Q14_MAX).contains(&update.delta) {
            return Err(BiasUpdateError::DeltaOutOfRange(update.delta));
        }
        let key = (update.template_id, update.archetype_id);
        let prev = self.cells.get(&key).copied().unwrap_or(0);
        let next = saturating_add_q14(prev, update.delta);
        if next == 0 {
            self.cells.remove(&key);
        } else {
            self.cells.insert(key, next);
        }
        self.update_count = self.update_count.saturating_add(1);
        Ok(())
    }

    /// Iterate all non-zero cells in deterministic ascending order.
    pub fn iter_cells(&self) -> impl Iterator<Item = (TemplateId, ArchetypeId, i16)> + '_ {
        self.cells.iter().map(|(&(t, a), &v)| (t, a, v))
    }

    /// Reset the map back to empty (used by sovereign-revoke recompute).
    /// The update_count is preserved : revoke-recompute counts as a continuation
    /// of the loop's lifetime, not a fresh start.
    pub fn reset_cells(&mut self) {
        self.cells.clear();
    }

    /// Top-N templates for a given archetype, ranked by descending bias.
    /// Returns at most `top_n` entries ; ties resolve by ascending template-id.
    pub fn top_for_archetype(
        &self,
        archetype: ArchetypeId,
        top_n: usize,
    ) -> Vec<(TemplateId, i16)> {
        if top_n == 0 {
            return Vec::new();
        }
        let mut filtered: Vec<(TemplateId, i16)> = self
            .cells
            .iter()
            .filter_map(|(&(t, a), &v)| if a == archetype { Some((t, v)) } else { None })
            .collect();
        // Descending by bias ; ties → ascending template-id.
        filtered.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        filtered.truncate(top_n);
        filtered
    }

    /// BLAKE3-128 hash of the canonical-byte-form of all cells, in ascending
    /// (template · archetype) order. Used by Σ-Chain anchor.
    pub fn anchor_hash(&self) -> [u8; 16] {
        let mut hasher = blake3::Hasher::new();
        // include update_count so two maps with identical-cells but different
        // update-history hash distinctly.
        hasher.update(&self.update_count.to_le_bytes());
        for (&(t, a), &v) in &self.cells {
            hasher.update(&t.0.to_le_bytes());
            hasher.update(&[a.0]);
            hasher.update(&v.to_le_bytes());
        }
        let h = hasher.finalize();
        let mut out = [0u8; 16];
        out.copy_from_slice(&h.as_bytes()[..16]);
        out
    }
}

// ───────────────────────────────────────────────────────────────────────────
// § Helpers
// ───────────────────────────────────────────────────────────────────────────

/// Saturating-add on Q14-clamped i16 (clamped to ±BIAS_Q14_MAX).
pub fn saturating_add_q14(a: i16, b: i16) -> i16 {
    let s = i32::from(a).saturating_add(i32::from(b));
    s.clamp(i32::from(BIAS_Q14_MIN), i32::from(BIAS_Q14_MAX)) as i16
}

// ───────────────────────────────────────────────────────────────────────────
// § Tests
// ───────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archetype_names_unique_and_canonical() {
        let mut names: Vec<&str> = ARCHETYPE_NAMES.to_vec();
        names.sort_unstable();
        let prev = names.len();
        names.dedup();
        assert_eq!(names.len(), prev, "archetype names must be unique");
        assert_eq!(names.len(), ARCHETYPE_COUNT);
        for i in 0..ARCHETYPE_COUNT as u8 {
            assert!(ArchetypeId(i).is_canonical());
            assert_ne!(ArchetypeId(i).name(), "unknown");
        }
        assert_eq!(ArchetypeId(99).name(), "unknown");
        assert!(!ArchetypeId(99).is_canonical());
    }

    #[test]
    fn saturating_add_q14_clamps_at_bounds() {
        assert_eq!(saturating_add_q14(BIAS_Q14_MAX, 1000), BIAS_Q14_MAX);
        assert_eq!(saturating_add_q14(BIAS_Q14_MIN, -1000), BIAS_Q14_MIN);
        assert_eq!(saturating_add_q14(100, 200), 300);
        assert_eq!(saturating_add_q14(-5000, 3000), -2000);
        // exact-boundary cases
        assert_eq!(saturating_add_q14(BIAS_Q14_MAX, 0), BIAS_Q14_MAX);
        assert_eq!(saturating_add_q14(BIAS_Q14_MIN, 0), BIAS_Q14_MIN);
    }

    #[test]
    fn apply_update_basic_arithmetic() {
        let mut m = TemplateBiasMap::new();
        let upd1 = KanBiasUpdate::new(TemplateId(7), ArchetypeId(2), 500, 1);
        m.apply_update(upd1).unwrap();
        assert_eq!(m.priority_shift(TemplateId(7), ArchetypeId(2)), 500);
        assert_eq!(m.update_count(), 1);

        let upd2 = KanBiasUpdate::new(TemplateId(7), ArchetypeId(2), -200, 2);
        m.apply_update(upd2).unwrap();
        assert_eq!(m.priority_shift(TemplateId(7), ArchetypeId(2)), 300);
        assert_eq!(m.update_count(), 2);

        // -300 should prune to zero ⇒ entry removed.
        let upd3 = KanBiasUpdate::new(TemplateId(7), ArchetypeId(2), -300, 3);
        m.apply_update(upd3).unwrap();
        assert_eq!(m.priority_shift(TemplateId(7), ArchetypeId(2)), 0);
        assert_eq!(m.cell_count(), 0);
    }

    #[test]
    fn apply_update_rejects_out_of_range() {
        let mut m = TemplateBiasMap::new();
        let bad_arch = KanBiasUpdate::new(TemplateId(0), ArchetypeId(99), 100, 0);
        assert!(matches!(
            m.apply_update(bad_arch),
            Err(BiasUpdateError::ArchetypeOutOfRange(99))
        ));
        // delta i16::MAX is > BIAS_Q14_MAX
        let bad_delta = KanBiasUpdate::new(TemplateId(0), ArchetypeId(0), i16::MAX, 0);
        assert!(matches!(
            m.apply_update(bad_delta),
            Err(BiasUpdateError::DeltaOutOfRange(_))
        ));
    }

    #[test]
    fn top_for_archetype_returns_descending() {
        let mut m = TemplateBiasMap::new();
        for (t, v) in [(TemplateId(1), 1000), (TemplateId(2), 5000), (TemplateId(3), 3000)] {
            m.apply_update(KanBiasUpdate::new(t, ArchetypeId(0), v, 0))
                .unwrap();
        }
        let top = m.top_for_archetype(ArchetypeId(0), 3);
        assert_eq!(top, vec![(TemplateId(2), 5000), (TemplateId(3), 3000), (TemplateId(1), 1000)]);
        let top1 = m.top_for_archetype(ArchetypeId(0), 1);
        assert_eq!(top1, vec![(TemplateId(2), 5000)]);
        let top0 = m.top_for_archetype(ArchetypeId(0), 0);
        assert!(top0.is_empty());
    }

    #[test]
    fn anchor_hash_deterministic_and_state_dependent() {
        let mut m = TemplateBiasMap::new();
        m.apply_update(KanBiasUpdate::new(TemplateId(1), ArchetypeId(0), 500, 0))
            .unwrap();
        let h1 = m.anchor_hash();
        // Idempotent : same map ⇒ same hash.
        let h2 = m.anchor_hash();
        assert_eq!(h1, h2);
        // Different state ⇒ different hash.
        m.apply_update(KanBiasUpdate::new(TemplateId(2), ArchetypeId(1), 100, 1))
            .unwrap();
        let h3 = m.anchor_hash();
        assert_ne!(h1, h3);
    }

    #[test]
    fn reset_cells_preserves_update_count() {
        let mut m = TemplateBiasMap::new();
        m.apply_update(KanBiasUpdate::new(TemplateId(0), ArchetypeId(0), 1000, 0)).unwrap();
        assert_eq!(m.update_count(), 1);
        m.reset_cells();
        assert_eq!(m.cell_count(), 0);
        assert_eq!(m.update_count(), 1, "update_count preserved across reset for replay-determinism");
    }
}
