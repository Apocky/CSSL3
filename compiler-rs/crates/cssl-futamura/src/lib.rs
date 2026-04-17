//! CSSLv3 Futamura projections — P1 / P2 / P3 partial-evaluation infrastructure.
//!
//! § SPEC : `specs/19_FUTAMURA3.csl` + `specs/06_STAGING.csl`.
//!
//! § PROJECTIONS (per Futamura 1971)
//!   - P1 : `spec(int, src) → compiled-prog`        — "specialize an interpreter for
//!     a given source, producing a compiled program." Baseline via `@staged`.
//!   - P2 : `spec(spec, int) → compiler`            — "specialize the specializer to
//!     an interpreter, producing a standalone compiler." Realized via stdlib builtin
//!     specializer reference at T8-phase-2.
//!   - P3 : `spec(spec, spec) → compiler-generator` — "specialize the specializer to
//!     itself, producing a compiler generator." Enables self-bootstrap at stage-1+.
//!
//! § FIXED-POINT
//!   T16 (per `specs/THEOREMS.csl` OG5) CI-gate : `generation-N ≡ generation-N+1`
//!   bit-exact. The `cssl-futamura` crate records fixed-point hashes + checksums ;
//!   full fixed-point verification requires a running stage-1 compiler (deferred).
//!
//! § SCOPE (T8-phase-1 / this commit)
//!   Data model + projection-level enum + fixed-point hash records. Actual
//!   specialization happens via `cssl-staging` + `cssl-macros` which the projection
//!   orchestrates.

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]

use thiserror::Error;

/// Futamura projection level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum FutamuraLevel {
    /// P1 : specialize interpreter for a source → compiled program.
    P1,
    /// P2 : specialize specializer to interpreter → standalone compiler.
    P2,
    /// P3 : specialize specializer to itself → compiler-generator.
    P3,
}

impl FutamuraLevel {
    /// Label for diagnostics.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::P1 => "futamura-P1",
            Self::P2 => "futamura-P2",
            Self::P3 => "futamura-P3",
        }
    }

    /// All three levels.
    pub const ALL: [Self; 3] = [Self::P1, Self::P2, Self::P3];

    /// Monotonic ordering — P1 < P2 < P3.
    #[must_use]
    pub const fn order(self) -> u32 {
        match self {
            Self::P1 => 1,
            Self::P2 => 2,
            Self::P3 => 3,
        }
    }
}

/// A projection record : input program + projection-level + produced-artifact-hash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Projection {
    /// Source-program identifier (e.g., module path hash).
    pub source: String,
    /// Which projection-level this record describes.
    pub level: FutamuraLevel,
    /// Content-hash of the produced specialized artifact (BLAKE3-compatible hex digest).
    pub artifact_hash: String,
}

impl Projection {
    /// Build a projection record.
    #[must_use]
    pub fn new(
        source: impl Into<String>,
        level: FutamuraLevel,
        artifact_hash: impl Into<String>,
    ) -> Self {
        Self {
            source: source.into(),
            level,
            artifact_hash: artifact_hash.into(),
        }
    }
}

/// Fixed-point record : two consecutive generation hashes that must agree for
/// P3 self-bootstrap to converge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixedPointRecord {
    pub generation: u32,
    pub hash_n: String,
    pub hash_n_plus_1: String,
}

impl FixedPointRecord {
    /// Build a record for generation `N`.
    #[must_use]
    pub fn new(
        generation: u32,
        hash_n: impl Into<String>,
        hash_n_plus_1: impl Into<String>,
    ) -> Self {
        Self {
            generation,
            hash_n: hash_n.into(),
            hash_n_plus_1: hash_n_plus_1.into(),
        }
    }

    /// `true` iff the two hashes match (fixed-point reached).
    #[must_use]
    pub fn converged(&self) -> bool {
        self.hash_n == self.hash_n_plus_1
    }
}

/// Failure modes for Futamura-projection orchestration.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum FutamuraError {
    /// Fixed-point check failed : generation-N+1 ≠ generation-N after max iterations.
    #[error("fixed-point not reached after {max_gen} generations : {last_hash} → {next_hash}")]
    FixedPointDiverged {
        max_gen: u32,
        last_hash: String,
        next_hash: String,
    },
    /// P2 requires a known specializer ; none registered.
    #[error("P2 requires a registered specializer")]
    SpecializerMissing,
    /// P3 requires a P2-compatible specializer.
    #[error("P3 requires a P2-compatible specializer ; found level {found:?}")]
    WrongLevel { found: FutamuraLevel },
}

/// Orchestrator for a sequence of projections.
#[derive(Debug, Default, Clone)]
pub struct Orchestrator {
    projections: Vec<Projection>,
    fixed_points: Vec<FixedPointRecord>,
}

impl Orchestrator {
    /// Empty orchestrator.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a projection.
    pub fn record(&mut self, p: Projection) {
        self.projections.push(p);
    }

    /// Record a fixed-point check.
    pub fn record_fixed_point(&mut self, fp: FixedPointRecord) {
        self.fixed_points.push(fp);
    }

    /// Most recent projection, if any.
    #[must_use]
    pub fn latest(&self) -> Option<&Projection> {
        self.projections.last()
    }

    /// All projections at a given level.
    pub fn projections_at(&self, level: FutamuraLevel) -> impl Iterator<Item = &Projection> {
        self.projections.iter().filter(move |p| p.level == level)
    }

    /// `true` iff every fixed-point check converged.
    #[must_use]
    pub fn all_converged(&self) -> bool {
        self.fixed_points.iter().all(FixedPointRecord::converged)
    }

    /// Number of recorded projections.
    #[must_use]
    pub fn projection_count(&self) -> usize {
        self.projections.len()
    }

    /// Number of fixed-point records.
    #[must_use]
    pub fn fixed_point_count(&self) -> usize {
        self.fixed_points.len()
    }
}

/// Crate version exposed for scaffold verification.
pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::{FixedPointRecord, FutamuraLevel, Orchestrator, Projection, STAGE0_SCAFFOLD};

    #[test]
    fn scaffold_version_present() {
        assert!(!STAGE0_SCAFFOLD.is_empty());
    }

    #[test]
    fn three_levels_enumerated() {
        assert_eq!(FutamuraLevel::ALL.len(), 3);
    }

    #[test]
    fn levels_ordered_p1_p2_p3() {
        assert_eq!(FutamuraLevel::P1.order(), 1);
        assert_eq!(FutamuraLevel::P2.order(), 2);
        assert_eq!(FutamuraLevel::P3.order(), 3);
        assert!(FutamuraLevel::P1 < FutamuraLevel::P2);
        assert!(FutamuraLevel::P2 < FutamuraLevel::P3);
    }

    #[test]
    fn fixed_point_converges_when_hashes_match() {
        let fp = FixedPointRecord::new(3, "abc", "abc");
        assert!(fp.converged());
    }

    #[test]
    fn fixed_point_diverges_when_hashes_differ() {
        let fp = FixedPointRecord::new(3, "abc", "def");
        assert!(!fp.converged());
    }

    #[test]
    fn orchestrator_records_projections() {
        let mut o = Orchestrator::new();
        assert_eq!(o.projection_count(), 0);
        o.record(Projection::new("mod.a", FutamuraLevel::P1, "h1"));
        o.record(Projection::new("mod.a", FutamuraLevel::P2, "h2"));
        assert_eq!(o.projection_count(), 2);
        assert_eq!(o.latest().unwrap().level, FutamuraLevel::P2);
    }

    #[test]
    fn orchestrator_filters_by_level() {
        let mut o = Orchestrator::new();
        o.record(Projection::new("m", FutamuraLevel::P1, "h1"));
        o.record(Projection::new("m", FutamuraLevel::P2, "h2"));
        o.record(Projection::new("m", FutamuraLevel::P1, "h3"));
        assert_eq!(o.projections_at(FutamuraLevel::P1).count(), 2);
    }

    #[test]
    fn orchestrator_all_converged_when_empty() {
        let o = Orchestrator::new();
        assert!(o.all_converged());
    }

    #[test]
    fn orchestrator_all_converged_requires_every_match() {
        let mut o = Orchestrator::new();
        o.record_fixed_point(FixedPointRecord::new(1, "a", "a"));
        assert!(o.all_converged());
        o.record_fixed_point(FixedPointRecord::new(2, "a", "b"));
        assert!(!o.all_converged());
    }

    #[test]
    fn level_labels_unique() {
        let labels: Vec<&str> = FutamuraLevel::ALL.iter().map(|l| l.label()).collect();
        let mut sorted = labels.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), labels.len());
    }

    #[test]
    fn projection_roundtrips() {
        let p = Projection::new("com.apocky.loa", FutamuraLevel::P3, "dead-beef");
        assert_eq!(p.level, FutamuraLevel::P3);
        assert_eq!(p.source, "com.apocky.loa");
        assert_eq!(p.artifact_hash, "dead-beef");
    }
}
