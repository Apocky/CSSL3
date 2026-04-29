//! § PhiTable — typed wrapper around `AppendOnlyPool<Pattern>`
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   The substrate-spec literal `02_CSSL/06_SUBSTRATE_EVOLUTION.csl § 3` :
//!
//!   ```cssl
//!   @layout(soa)
//!   type PhiTable = AppendOnlyPool<Phi'Pattern>;     // stable-handle-indexed
//!   ```
//!
//!   plus the spec-mandated stamp-and-resolve helpers that combine
//!   `Pattern::stamp(genome, weights, tag, epoch)` with the pool's
//!   `push` to give a single one-shot stamping path.
//!
//! § STAMP / RESOLVE / EPOCH
//!   The `PhiTable` owns a monotonic stamp epoch counter that advances by
//!   one on every successful `stamp`. This guarantees uniqueness of the
//!   stamped Pattern's fingerprint even when the same (genome, weights,
//!   tag) tuple is stamped twice in the same field — because
//!   `Pattern::stamp` mixes the epoch into the fingerprint.
//!
//! § INTEGRATION POINT — FieldCell.pattern_handle
//!   The substrate spec `§ 1` field literal :
//!     `pattern_handle: Handle<Phi'Pattern>`
//!   is exactly `Handle<Pattern>` from this crate. A FieldCell records the
//!   handle ; resolving it requires a `&PhiTable` reference. The upcoming
//!   `cssl-substrate-omega-field` crate threads `field.phi_table` through
//!   the read paths to dereference cells' Φ-handles.

use crate::handle::{Handle, HandleResolveError};
use crate::kan_genome_weights::KanGenomeWeights;
use crate::pattern::{Pattern, PatternStampError, SubstrateClassTag};
use crate::pool::{AppendOnlyPool, PoolError};
use cssl_hdc::genome::Genome;

/// § PhiTable — substrate-spec `§ 3` `type PhiTable = AppendOnlyPool<Phi'Pattern>`.
///
/// Owns a monotonic stamp epoch counter so successive stamps of the same
/// (genome, weights, tag) tuple produce distinct fingerprints.
#[derive(Debug, Clone, Default)]
pub struct PhiTable {
    pool: AppendOnlyPool<Pattern>,
    /// § Monotonic stamp epoch — advances on every successful stamp.
    ///   Starts at `1` so the first stamp uses epoch `1` (epoch `0`
    ///   is reserved per the `Pattern::stamp` degenerate-input refusal).
    next_epoch: u64,
}

/// § Combined error type for [`PhiTable::stamp`] paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhiStampError {
    /// § The pool is at capacity ([`crate::MAX_PATTERNS_PER_POOL`]).
    Pool(PoolError),
    /// § The Pattern stamp itself was refused (degenerate inputs).
    Stamp(PatternStampError),
}

impl From<PoolError> for PhiStampError {
    fn from(e: PoolError) -> Self {
        Self::Pool(e)
    }
}

impl From<PatternStampError> for PhiStampError {
    fn from(e: PatternStampError) -> Self {
        Self::Stamp(e)
    }
}

impl core::fmt::Display for PhiStampError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Pool(e) => write!(f, "PhiTable.stamp pool error : {e}"),
            Self::Stamp(e) => write!(f, "PhiTable.stamp pattern error : {e}"),
        }
    }
}

impl PhiTable {
    /// § Construct an empty PhiTable. Initial epoch is `1`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            pool: AppendOnlyPool::new(),
            next_epoch: 1,
        }
    }

    /// § Construct with a hint for the underlying pool capacity.
    #[must_use]
    pub fn with_capacity(n: usize) -> Self {
        Self {
            pool: AppendOnlyPool::with_capacity(n),
            next_epoch: 1,
        }
    }

    /// § Number of patterns currently in the table.
    #[must_use]
    pub fn len(&self) -> usize {
        self.pool.len()
    }

    /// § True if the table is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pool.is_empty()
    }

    /// § Read access to the underlying pool. Useful when the caller wants
    ///   to iterate every Pattern (e.g. save-file emission).
    #[must_use]
    pub fn pool(&self) -> &AppendOnlyPool<Pattern> {
        &self.pool
    }

    /// § Current stamp epoch counter (the next epoch that would be used).
    #[must_use]
    pub fn current_epoch(&self) -> u64 {
        self.next_epoch
    }

    /// § Stamp a new Pattern from the canonical inputs and append it to
    ///   the pool. Returns the handle pointing at the newly-stamped
    ///   Pattern.
    ///
    ///   The stamp epoch is consumed from `self.next_epoch` and advanced
    ///   by one on success. On error, `next_epoch` is NOT advanced.
    pub fn stamp(
        &mut self,
        genome: &Genome,
        weights: &KanGenomeWeights,
        substrate_class_tag: SubstrateClassTag,
    ) -> Result<Handle<Pattern>, PhiStampError> {
        let epoch = self.next_epoch;
        let pattern = Pattern::stamp(genome, weights, substrate_class_tag, epoch)?;
        let handle = self.pool.push(pattern)?;
        self.next_epoch = self.next_epoch.saturating_add(1);
        Ok(handle)
    }

    /// § Resolve a handle to a borrowed Pattern reference. Returns
    ///   [`HandleResolveError`] for NULL / OOB / generation mismatch.
    pub fn resolve(&self, handle: Handle<Pattern>) -> Result<&Pattern, HandleResolveError> {
        self.pool.resolve(handle)
    }

    /// § True if the handle resolves to a live slot.
    #[must_use]
    pub fn contains(&self, handle: Handle<Pattern>) -> bool {
        self.pool.contains(handle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// § new() table is empty.
    #[test]
    fn new_table_is_empty() {
        let t = PhiTable::new();
        assert!(t.is_empty());
        assert_eq!(t.len(), 0);
        assert_eq!(t.current_epoch(), 1);
    }

    /// § stamp returns a non-NULL handle.
    #[test]
    fn stamp_returns_handle() {
        let mut t = PhiTable::new();
        let g = Genome::from_seed(1);
        let w = KanGenomeWeights::new_untrained();
        let h = t.stamp(&g, &w, SubstrateClassTag::Universal).unwrap();
        assert!(h.is_some());
    }

    /// § stamp advances the epoch.
    #[test]
    fn stamp_advances_epoch() {
        let mut t = PhiTable::new();
        let g = Genome::from_seed(1);
        let w = KanGenomeWeights::new_untrained();
        let _ = t.stamp(&g, &w, SubstrateClassTag::Universal).unwrap();
        assert_eq!(t.current_epoch(), 2);
        let _ = t.stamp(&g, &w, SubstrateClassTag::Universal).unwrap();
        assert_eq!(t.current_epoch(), 3);
    }

    /// § Two stamps of the same (genome, weights, tag) produce
    ///   different handles because the epoch differs.
    #[test]
    fn two_stamps_different_handles() {
        let mut t = PhiTable::new();
        let g = Genome::from_seed(1);
        let w = KanGenomeWeights::new_untrained();
        let h1 = t.stamp(&g, &w, SubstrateClassTag::Universal).unwrap();
        let h2 = t.stamp(&g, &w, SubstrateClassTag::Universal).unwrap();
        assert_ne!(h1, h2);
    }

    /// § Two stamps of the same tuple have different fingerprints.
    #[test]
    fn two_stamps_different_fingerprints() {
        let mut t = PhiTable::new();
        let g = Genome::from_seed(1);
        let w = KanGenomeWeights::new_untrained();
        let h1 = t.stamp(&g, &w, SubstrateClassTag::Universal).unwrap();
        let h2 = t.stamp(&g, &w, SubstrateClassTag::Universal).unwrap();
        let fp1 = t.resolve(h1).unwrap().fingerprint;
        let fp2 = t.resolve(h2).unwrap().fingerprint;
        assert_ne!(fp1, fp2);
    }

    /// § resolve roundtrip — stamp then resolve gives identical fingerprint.
    #[test]
    fn resolve_roundtrip() {
        let mut t = PhiTable::new();
        let g = Genome::from_seed(7);
        let w = KanGenomeWeights::new_untrained();
        let h = t.stamp(&g, &w, SubstrateClassTag::Classical).unwrap();
        let p = t.resolve(h).unwrap();
        assert!(!p.fingerprint.is_null());
        assert_eq!(p.substrate_class_tag, SubstrateClassTag::Classical);
    }

    /// § resolve of NULL handle errors.
    #[test]
    fn resolve_null_errors() {
        let t = PhiTable::new();
        let n = Handle::NULL;
        assert!(t.resolve(n).is_err());
    }

    /// § contains agrees with resolve.
    #[test]
    fn contains_agrees() {
        let mut t = PhiTable::new();
        let g = Genome::from_seed(1);
        let w = KanGenomeWeights::new_untrained();
        let h = t.stamp(&g, &w, SubstrateClassTag::Universal).unwrap();
        assert!(t.contains(h));
        assert!(!t.contains(Handle::NULL));
    }

    /// § PhiStampError From conversions.
    #[test]
    fn phi_stamp_error_from_pool() {
        let e: PhiStampError = PoolError::AtCapacity { len: 1, cap: 1 }.into();
        assert!(matches!(e, PhiStampError::Pool(_)));
    }

    /// § PhiStampError From PatternStampError.
    #[test]
    fn phi_stamp_error_from_pattern() {
        let e: PhiStampError = PatternStampError::DegenerateInputs.into();
        assert!(matches!(e, PhiStampError::Stamp(_)));
    }

    /// § Display for PhiStampError.
    #[test]
    fn phi_stamp_error_display() {
        let e: PhiStampError = PhiStampError::Pool(PoolError::AtCapacity { len: 1, cap: 1 });
        assert!(format!("{e}").contains("pool"));
        let e: PhiStampError = PhiStampError::Stamp(PatternStampError::DegenerateInputs);
        assert!(format!("{e}").contains("pattern"));
    }

    /// § with_capacity preserves the empty-table invariants.
    #[test]
    fn with_capacity_empty_invariant() {
        let t = PhiTable::with_capacity(100);
        assert!(t.is_empty());
        assert_eq!(t.current_epoch(), 1);
    }

    /// § Many stamps : handle indices monotonic.
    #[test]
    fn many_stamps_monotonic_indices() {
        let mut t = PhiTable::new();
        let g = Genome::from_seed(1);
        let w = KanGenomeWeights::new_untrained();
        for i in 0..200 {
            let h = t.stamp(&g, &w, SubstrateClassTag::Universal).unwrap();
            assert_eq!(h.index(), i);
        }
        assert_eq!(t.len(), 200);
    }

    /// § Pool accessor returns the underlying pool.
    #[test]
    fn pool_accessor_returns_pool() {
        let mut t = PhiTable::new();
        let g = Genome::from_seed(1);
        let w = KanGenomeWeights::new_untrained();
        let _ = t.stamp(&g, &w, SubstrateClassTag::Universal).unwrap();
        assert_eq!(t.pool().len(), 1);
    }

    /// § Stamping then iterating yields all stamped patterns.
    #[test]
    fn iterate_all_patterns() {
        let mut t = PhiTable::new();
        let w = KanGenomeWeights::new_untrained();
        for s in 1..=5u64 {
            let g = Genome::from_seed(s);
            let _ = t.stamp(&g, &w, SubstrateClassTag::Universal).unwrap();
        }
        assert_eq!(t.pool().iter().count(), 5);
    }
}
