//! Conservation-law + cosmology effect-row discipline.
//!
//! § SPEC :
//!   - `Omniverse/02_CSSL/00_LANGUAGE_CONTRACT.csl.md § V` — Ω-required effect-row
//!     vocabulary that CSSLv3 must provide.
//!   - `Omniverse/02_CSSL/02_EFFECTS.csl.md` — composition rules + REFUSED rules.
//!   - `Omniverse/01_AXIOMS/03_OMEGA_FIELD.csl.md § IV CONSERVATION-LAWS` —
//!     1..6 base laws + 7..9 (Φ-integrity, Σ-monotonicity, capacity-floor).
//!   - `Omniverse/01_AXIOMS/04_AGENCY_INVARIANT.csl.md` — agency-triple
//!     {consent, sovereignty, reversibility} ; Σ-facet at cell-level.
//!   - `Omniverse/01_AXIOMS/05_OBSERVATION_COLLAPSE.csl.md` — observation-collapse
//!     primitive ; oracle-mode generators ; re-collapse-determinism.
//!   - `Omniverse/01_AXIOMS/07_COHOMOLOGY_NARRATIVE.csl.md` — H¹-classes as
//!     story-events ; persistent-homology lifecycle.
//!   - `Omniverse/01_AXIOMS/09_ENTROPY_RG_FLOW.csl.md` — σ-as-currency ;
//!     conservation @ each tick.
//!
//! § THESIS
//!   The Substrate-row layer (`substrate.rs`) covers the six engine-plumbing
//!   labels {Render, Sim, Audio, Net, Save, Telemetry}. The BuiltinEffect layer
//!   (`registry.rs`) covers the broad CSSLv3 vocabulary (NoAlloc, Deadline,
//!   Privilege<l>, Audit<dom>, ...). This module covers the **structural
//!   conservation laws** that the Omniverse-substrate enforces on top of both —
//!   σ-bookkeeping, Φ-integrity, agency-triple verification, observation-collapse
//!   determinism, and cohomology-class lifecycle.
//!
//!   Each rule is a compile-time property. None of them can be disabled by
//!   `cfg`, env-var, or runtime flag (per PRIME_DIRECTIVE.md F5 — F5 = the
//!   structural-encoding form of protections).
//!
//! § STABLE BLOCK : EFR0019..EFR0030 allocated in T11-D128 (W3β-03). Reordering
//!   any code is a major-version-bump event ; new codes go in EFR0031+ in a
//!   future slice. The block follows the same discipline as substrate.rs's
//!   EFR0001..EFR0010 block + (reserved) EFR0011..EFR0018 (W3β-02 / T11-D127).
//!
//! § ENCODING SUMMARY (the six row-checks)
//!
//!   ```text
//!     EFR0019  EntropyDriftExceeded                  σ ¬ within ε_f
//!     EFR0020  EntropyBalancedRequiresAudit          σ-bookkeep R! audit-companion
//!     EFR0021  PatternIntegrityWithoutSovereign      Φ-preserve R! Sovereign-context
//!     EFR0022  PatternIntegrityViolatedByMutation    Φ ¬ rewriteable inline
//!     EFR0023  AgencyVerifiedRequiresTriple          {consent ∧ sovereignty ∧ reversibility}
//!     EFR0024  AgencyVerifiedWithoutPrivilege        Privilege<≥2> R!
//!     EFR0025  AgencyVerifiedSovereignTouchNeedsAudit Audit-companion R! @ Sovereign-touch
//!     EFR0026  RegionCollapseRequiresDetRng          re-collapse-determinism (Axiom 5)
//!     EFR0027  RegionCollapseWithoutCohomology       advisory : H¹-class preservation
//!     EFR0028  RegionCollapseDoubleCollapseForbidden already-collapsed R! prune-first
//!     EFR0029  CohomologyRequiresAuditSpan           class-lifecycle R! audit
//!     EFR0030  CohomologyClassMismatchOnTransform    H<id> wrong @ class-transform
//!   ```
//!
//! § DESIGN NOTES
//!   The conservation rules **complement** the existing Substrate-row checker —
//!   a fn-row may pass `try_compose` (substrate-axis) yet still trip a rule here.
//!   This is intentional : the two checkers cover orthogonal axes (engine-plumbing
//!   vs. structural-conservation). Both must pass before the HIR layer accepts
//!   the row.

// `ConservationContext` carries 11 independent witness/companion bits that map
// 1:1 to the spec's gating dimensions (Axiom 4 agency-triple ; Axiom 5 collapse-
// preconditions ; Axiom 7 cohomology-class identity). Refactoring into a state
// machine or two-variant enums obscures the spec→impl correspondence, the same
// rationale `substrate.rs` cites for its 4-bool `RowContext`. The float_cmp +
// redundant_closure lints fire on test-time σ-edge equality + closure-deref of
// a method ; both are intentional and idiomatic for the test-shape used here.
#![allow(
    clippy::similar_names,
    clippy::struct_excessive_bools,
    clippy::float_cmp,
    clippy::redundant_closure_for_method_calls,
    clippy::trivially_copy_pass_by_ref
)]

use thiserror::Error;

use crate::discipline::EffectRef;
use crate::registry::BuiltinEffect;

// ─ ConservationContext ─────────────────────────────────────────────────────

/// Caller-context bits used by the conservation-rule checks.
///
/// § RATIONALE
///   Several rules check companion-row presence (e.g., AgencyVerified requires
///   Audit<dom>). The `EffectRef`-list passed to each checker is the single
///   source-of-truth for these checks ; the `ConservationContext` holds the
///   handful of side-channel bits the checker needs (e.g., is the operation
///   touching a Sovereign-target ? — which the row-list alone can't tell us).
///
/// § DEFAULT
///   `ConservationContext::default()` represents a pure / non-Sovereign-touching
///   op with no privilege escalation — the strictest interpretation. Builders
///   add bits as the HIR layer learns more about the op-shape.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ConservationContext {
    /// `true` iff the op's target is a Sovereign-acting-or-acted-upon entity.
    /// Maps to `Op<S, T, ε, π, υ>` having `T` resolve through Σ-mask. Per
    /// Axiom 4 § VII, every Sovereign-touching op must carry `Audit<dom>`.
    pub touches_sovereign: bool,
    /// `true` iff the row's privilege-tier is `Privilege<≥ Engine>` (≥ 2).
    /// `AgencyVerified` requires this as the well-formedness check is encoded
    /// at the engine-stdlib level.
    pub privilege_at_or_above_engine: bool,
    /// `true` iff the row already declares a `DetRNG` companion. Required for
    /// `RegionCollapse` per Axiom 5 § IV (re-collapse-determinism).
    pub has_det_rng: bool,
    /// `true` iff the row already declares an `Audit<dom>` companion. Required
    /// for σ-bookkeeping (EFR0020), agency-verified-Sovereign-touch (EFR0025),
    /// and cohomology-class lifecycle (EFR0029).
    pub has_audit: bool,
    /// `true` iff the caller declares this op may rewrite a Φ-Pattern (i.e.,
    /// the call-site is inside a `RECRYSTALLIZE`-permitted block). Without
    /// this flag, `PatternIntegrity` is non-mutating and an inline mutation
    /// is `EFR0022`.
    pub allows_pattern_rewrite: bool,
    /// `true` iff the caller declares the consent-witness for the op
    /// (consent leg of the agency-triple).
    pub has_consent_witness: bool,
    /// `true` iff the caller declares the sovereignty-witness (capacity-floor
    /// preserved across causal-cone) — sovereignty leg of the triple.
    pub has_sovereignty_witness: bool,
    /// `true` iff the caller declares the reversibility-witness (constructive
    /// proof or declared-irreversibility-justification) — reversibility leg.
    pub has_reversibility_witness: bool,
    /// `true` iff the region targeted by `RegionCollapse` is already collapsed
    /// (per Axiom 5 § II — already-collapsed regions stay stable). Set by HIR
    /// layer when it can prove the region was previously observed.
    pub region_already_collapsed: bool,
    /// `true` iff the row carries `Cohomology<H>` companion — used by
    /// `RegionCollapse` advisory check (EFR0027).
    pub has_cohomology_companion: bool,
    /// Optional class-id expected by a cohomology-class transform. Set when
    /// the HIR layer knows the source class-id ; mismatch with the row's
    /// `Cohomology<H>` arg surfaces `EFR0030`.
    pub expected_cohomology_class_id: Option<u64>,
    /// The class-id actually carried by `Cohomology<H>` in the row, if known.
    /// HIR layer fills this from the type-arg.
    pub row_cohomology_class_id: Option<u64>,
}

impl ConservationContext {
    /// Builder : the op touches a Sovereign-target.
    #[must_use]
    pub const fn with_sovereign_touch(mut self) -> Self {
        self.touches_sovereign = true;
        self
    }
    /// Builder : the op declares Privilege<≥ Engine> (tier ≥ 2).
    #[must_use]
    pub const fn with_privilege_engine(mut self) -> Self {
        self.privilege_at_or_above_engine = true;
        self
    }
    /// Builder : the row carries DetRNG.
    #[must_use]
    pub const fn with_det_rng(mut self) -> Self {
        self.has_det_rng = true;
        self
    }
    /// Builder : the row carries Audit<dom>.
    #[must_use]
    pub const fn with_audit(mut self) -> Self {
        self.has_audit = true;
        self
    }
    /// Builder : the call-site allows Φ-Pattern rewrite (RECRYSTALLIZE).
    #[must_use]
    pub const fn with_pattern_rewrite(mut self) -> Self {
        self.allows_pattern_rewrite = true;
        self
    }
    /// Builder : the consent leg of the agency-triple is witnessed.
    #[must_use]
    pub const fn with_consent_witness(mut self) -> Self {
        self.has_consent_witness = true;
        self
    }
    /// Builder : the sovereignty leg is witnessed.
    #[must_use]
    pub const fn with_sovereignty_witness(mut self) -> Self {
        self.has_sovereignty_witness = true;
        self
    }
    /// Builder : the reversibility leg is witnessed.
    #[must_use]
    pub const fn with_reversibility_witness(mut self) -> Self {
        self.has_reversibility_witness = true;
        self
    }
    /// Builder : witness all three legs of the agency-triple (helper).
    #[must_use]
    pub const fn with_agency_triple(self) -> Self {
        self.with_consent_witness()
            .with_sovereignty_witness()
            .with_reversibility_witness()
    }
    /// Builder : the targeted region is already-collapsed.
    #[must_use]
    pub const fn with_region_already_collapsed(mut self) -> Self {
        self.region_already_collapsed = true;
        self
    }
    /// Builder : the row carries Cohomology<H> companion.
    #[must_use]
    pub const fn with_cohomology_companion(mut self) -> Self {
        self.has_cohomology_companion = true;
        self
    }
    /// Builder : set both the expected and actual class-ids for transform-check.
    #[must_use]
    pub const fn with_cohomology_class_ids(mut self, expected: u64, actual: u64) -> Self {
        self.expected_cohomology_class_id = Some(expected);
        self.row_cohomology_class_id = Some(actual);
        self
    }
}

// ─ EntropyEdge ─────────────────────────────────────────────────────────────

/// One σ-edge (debit or credit) in the entropy-bookkeeping ledger.
///
/// § RATIONALE
///   `EntropyBalanced` encodes the conservation-law @ row-level ; the actual
///   σ-balance per Axiom 9 § I is implemented at runtime by `entropy_book`.
///   This stage-0 layer surfaces *drift* (the |sum(edges)| > ε_f case) when
///   the HIR layer can deliver a static edge-list, and otherwise records
///   "balance-must-be-asserted-at-runtime" via `DischargeTiming::
///   CompileAndRuntimeAssert`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EntropyEdge {
    /// The σ-delta this op contributes (positive = source, negative = sink).
    pub delta: f64,
    /// Region-id this edge is local-to (for diagnostic locality).
    pub region_id: u64,
}

impl EntropyEdge {
    /// Build a debit-edge (σ ↓ here ⊗ σ ↑ elsewhere).
    #[must_use]
    pub const fn debit(amount: f64, region_id: u64) -> Self {
        Self {
            delta: -amount,
            region_id,
        }
    }
    /// Build a credit-edge.
    #[must_use]
    pub const fn credit(amount: f64, region_id: u64) -> Self {
        Self {
            delta: amount,
            region_id,
        }
    }
}

/// Sum a slice of `EntropyEdge`s. Returns the total drift.
#[must_use]
pub fn sum_entropy_edges(edges: &[EntropyEdge]) -> f64 {
    edges.iter().map(|e| e.delta).sum()
}

/// Default tolerance for σ-imbalance per Axiom 9 § VI ("no drift > ε_f"). The
/// concrete value is project-tunable ; stage-0 picks a conservative 1e-6.
pub const ENTROPY_EPSILON: f64 = 1e-6;

// ─ ConservationViolation / EFR codes ───────────────────────────────────────

/// Stable diagnostic codes for conservation-law + cosmology-effect-row
/// composition conflicts.
///
/// § STABLE BLOCK : EFR0019..EFR0030 allocated in T11-D128 (W3β-03). The codes
/// EFR0011..EFR0018 are reserved for W3β-02 (T11-D127, in flight) ; this block
/// starts at EFR0019 to leave room.
///
/// § ACTIONABLE-MESSAGE-CONVENTION : every variant carries enough context to
/// produce a full diagnostic of the shape :
///
/// ```text
/// error[EFR0019]: σ-balance drift |sum(edges)| = 1.23e-3 > ε_f = 1e-6
///   = note: see Omniverse/01_AXIOMS/09_ENTROPY_RG_FLOW § VI ACCEPTANCE
///   = help: ensure every op produces both a debit and a matching credit
/// ```
#[derive(Debug, Clone, PartialEq, Error)]
pub enum ConservationViolation {
    /// EFR0019 — σ-balance drift exceeds ε_f. Per Axiom 9 § VI, no drift > ε_f
    /// is permitted ; the op-row would corrupt the entropy ledger.
    #[error(
        "[EFR0019] σ-balance drift |sum(edges)| = {drift:.3e} > ε_f = {epsilon:.3e} \
         (Axiom 9 § I + § VI ACCEPTANCE — entropy_book passes balance-test, \
         no drift > ε_f) ; help: ensure every debit has a matching credit, \
         or split the op into two rows where each balances locally"
    )]
    EntropyDriftExceeded {
        /// The actual drift (signed sum of edge-deltas).
        drift: f64,
        /// The tolerance that was exceeded.
        epsilon: f64,
    },

    /// EFR0020 — `EntropyBalanced` row missing `Audit<dom>` companion.
    /// Rationale : per Axiom 3 § IV CONSERVATION + Axiom 9 § V CSSL ENCODING,
    /// the `entropy_book` fn carries `/{Audit<'entropy>, Pure}`. Without an
    /// Audit-companion, the σ-ledger has no append-only record.
    #[error(
        "[EFR0020] effect `{{EntropyBalanced}}` requires an `Audit<dom>` companion \
         (Axiom 3 § IV + Axiom 9 § V CSSL ENCODING — entropy_book carries \
         /{{Audit<'entropy>, Pure}}) ; \
         help: add `Audit<\"entropy\">` (or a project-specific σ-domain) to the row"
    )]
    EntropyBalancedRequiresAudit,

    /// EFR0021 — `PatternIntegrity` row absent from a Sovereign-touching context.
    /// Rationale : per Axiom 4 § VII (substrate-relativity-interaction), every
    /// op that touches a Sovereign must preserve Φ-Pattern integrity ; without
    /// the row, the op is allowed to launder the Pattern.
    #[error(
        "[EFR0021] Sovereign-touching op missing `{{PatternIntegrity}}` row \
         (Axiom 2 + Axiom 4 § VII — Φ-fingerprint must be preserved across \
         the op or the Sovereign-identity is broken) ; \
         help: add `PatternIntegrity` to the row, or refactor to non-Sovereign-touching"
    )]
    PatternIntegrityWithoutSovereign,

    /// EFR0022 — `PatternIntegrity` row composed with an inline Φ-mutation
    /// without a `RECRYSTALLIZE` ConsentBit / privileged-rewrite block.
    /// Rationale : per Axiom 4 § II Σ-Mask ConsentBit table, RECRYSTALLIZE is
    /// "Pattern-rewrite permitted (rare ; Sovereign-only)" — inline mutation
    /// without that bit violates Axiom 2 Pattern-integrity.
    #[error(
        "[EFR0022] `{{PatternIntegrity}}` violated by inline Φ-rewrite without \
         RECRYSTALLIZE ConsentBit \
         (Axiom 2 + Axiom 4 § II Σ.consent_bits — Pattern-rewrite is rare and \
         Sovereign-only) ; \
         help: wrap the rewrite in an `unsafe_recrystallize {{ ... }}` block + \
         ensure Sovereign-acknowledgement signature, or replace with a non-mutating op"
    )]
    PatternIntegrityViolatedByMutation,

    /// EFR0023 — `AgencyVerified` row missing one or more legs of the
    /// agency-triple {consent, sovereignty, reversibility}.
    /// Rationale : per Axiom 4 § I.1-3, the triple is the well-formedness
    /// definition. Missing any leg = op may not compile.
    #[error(
        "[EFR0023] `{{AgencyVerified}}` requires all three legs of the agency-triple \
         (Axiom 4 § I — well-formed(op) ⟺ consent ∧ sovereignty ∧ reversibility ; \
         current state : consent={consent}, sovereignty={sovereignty}, \
         reversibility={reversibility}) ; \
         help: ensure the call-site provides {{✶-token, capacity-floor-witness, undo-witness}}"
    )]
    AgencyVerifiedRequiresTriple {
        /// Whether the consent leg was witnessed.
        consent: bool,
        /// Whether the sovereignty leg was witnessed.
        sovereignty: bool,
        /// Whether the reversibility leg was witnessed.
        reversibility: bool,
    },

    /// EFR0024 — `AgencyVerified` row without `Privilege<≥ Engine>` (tier ≥ 2).
    /// Rationale : per Omniverse/02_CSSL/02_EFFECTS § IV PRIVILEGE TIERS, the
    /// well-formedness checker runs at engine-stdlib privilege ; user-spells
    /// at Privilege<0> cannot certify their own well-formedness.
    #[error(
        "[EFR0024] `{{AgencyVerified}}` requires `Privilege<≥ Engine>` (tier ≥ 2) \
         (Omniverse/02_CSSL/02_EFFECTS § IV — Privilege<0/1> cannot self-certify \
         well-formedness ; only engine-stdlib + above) ; \
         help: elevate to Privilege<2> or higher, or remove `AgencyVerified` and \
         have the engine-shell verify the op externally"
    )]
    AgencyVerifiedWithoutPrivilege,

    /// EFR0025 — `AgencyVerified` + Sovereign-touch without `Audit<dom>`.
    /// Rationale : per Axiom 4 § VII ACCEPTANCE bullet "effect-row {Audit,
    /// Privilege<l>} required at every Sovereign-touching op", missing-audit
    /// fails the spec.
    #[error(
        "[EFR0025] `{{AgencyVerified}}` + Sovereign-touching op requires `Audit<dom>` \
         (Axiom 4 § VII ACCEPTANCE — effect-row {{Audit, Privilege<l>}} required \
         at every Sovereign-touching op) ; \
         help: add `Audit<\"<sovereign-dom>\">` to the row"
    )]
    AgencyVerifiedSovereignTouchNeedsAudit,

    /// EFR0026 — `RegionCollapse` row without `DetRNG` companion.
    /// Rationale : per Axiom 5 § IV RECOLLAPSE-AND-MEMORY, "re-collapse ⊗
    /// deterministic ⊗ given DetRNG-seed-from-Φ" ; without DetRNG, replay-mode
    /// is broken.
    #[error(
        "[EFR0026] `{{RegionCollapse}}` requires `{{DetRNG}}` companion \
         (Axiom 5 § IV — re-collapse must be deterministic given DetRNG-seed) ; \
         help: add `DetRNG` to the row, or move non-deterministic sampling \
         to a separate fiber"
    )]
    RegionCollapseRequiresDetRng,

    /// EFR0027 — Advisory : `RegionCollapse` without `Cohomology<_>` companion.
    /// Rationale : per Axiom 5 § IV consistency-with-prior-state + Axiom 7 § III
    /// persistent-homology, every collapse should record/preserve H¹-classes.
    /// Stage-0 surfaces this as advisory ; the cohomology-DB enforces the
    /// actual preservation.
    #[error(
        "[EFR0027] `{{RegionCollapse}}` without `{{Cohomology<_>}}` companion — \
         H¹-class preservation cannot be verified at compile-time \
         (Axiom 5 § IV + Axiom 7 § III — re-collapse must preserve cohomology-classes \
         to avoid amnesia) ; \
         help: add `Cohomology<_>` to record the class-set affected by this collapse"
    )]
    RegionCollapseWithoutCohomology,

    /// EFR0028 — `RegionCollapse` invoked on already-collapsed region without
    /// prune-first.
    /// Rationale : per Axiom 5 § II, "already-collapsed regions stay stable" ;
    /// re-collapsing without first pruning the prior summary is double-collapse,
    /// which violates the conservation law.
    #[error(
        "[EFR0028] `{{RegionCollapse}}` invoked on already-collapsed region \
         (Axiom 5 § II ORACLE-SAMPLE — already-collapsed regions stay stable ; \
         re-collapse requires first pruning detail at MERA-layer-N per § IV) ; \
         help: precede with a prune-call, or check is_collapsed before calling"
    )]
    RegionCollapseDoubleCollapseForbidden,

    /// EFR0029 — `Cohomology<H>` row without `Audit<dom>` companion.
    /// Rationale : per Axiom 7 § VI CSSL ENCODING — `cohomology_detect` carries
    /// `/{Audit<'cohom>, Realtime<60Hz>}`. Without Audit, the class-lifecycle
    /// (birth/persist/transform/kill) is unrecorded — cannot audit narrative.
    #[error(
        "[EFR0029] `{{Cohomology<H>}}` requires `Audit<dom>` companion for \
         class-lifecycle recording \
         (Axiom 7 § VI CSSL ENCODING — cohomology_detect carries /{{Audit<'cohom>}}) ; \
         help: add `Audit<\"cohom\">` (or class-specific domain) to the row"
    )]
    CohomologyRequiresAuditSpan,

    /// EFR0030 — `Cohomology<H>` class-id mismatch on transform.
    /// Rationale : per Axiom 7 § III, transforms ({appear, persist, transform,
    /// die}) preserve class-id linkage ; a mismatched H<id> at transform-site
    /// orphans the new class from its lineage.
    #[error(
        "[EFR0030] `{{Cohomology<H>}}` class-id mismatch — expected H<{expected:#x}>, \
         row carries H<{actual:#x}> \
         (Axiom 7 § III persistent-homology — match-new-classes against existing \
         via cocycle-similarity ; mismatch orphans the lineage) ; \
         help: verify the source class-id at the transform-site, or use \
         `cohomology_birth(...)` instead of `cohomology_transform(...)`"
    )]
    CohomologyClassMismatchOnTransform {
        /// The expected class-id (from the transform-site's source).
        expected: u64,
        /// The class-id actually carried by `Cohomology<H>` in the row.
        actual: u64,
    },
}

impl ConservationViolation {
    /// Stable diagnostic-code as a `&'static str` (e.g., `"EFR0019"`).
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::EntropyDriftExceeded { .. } => "EFR0019",
            Self::EntropyBalancedRequiresAudit => "EFR0020",
            Self::PatternIntegrityWithoutSovereign => "EFR0021",
            Self::PatternIntegrityViolatedByMutation => "EFR0022",
            Self::AgencyVerifiedRequiresTriple { .. } => "EFR0023",
            Self::AgencyVerifiedWithoutPrivilege => "EFR0024",
            Self::AgencyVerifiedSovereignTouchNeedsAudit => "EFR0025",
            Self::RegionCollapseRequiresDetRng => "EFR0026",
            Self::RegionCollapseWithoutCohomology => "EFR0027",
            Self::RegionCollapseDoubleCollapseForbidden => "EFR0028",
            Self::CohomologyRequiresAuditSpan => "EFR0029",
            Self::CohomologyClassMismatchOnTransform { .. } => "EFR0030",
        }
    }

    /// `true` iff this is a hard compile-error (vs an advisory).
    ///
    /// § STAGE-0 CLASSIFICATION
    ///   Hard errors : 19, 21, 22, 23, 24, 25, 26, 28, 30
    ///   Advisories  : 20 (companion-required), 27 (cohomology-advisory),
    ///                 29 (audit-companion-required)
    #[must_use]
    pub const fn is_hard_error(&self) -> bool {
        matches!(
            self,
            Self::EntropyDriftExceeded { .. }
                | Self::PatternIntegrityWithoutSovereign
                | Self::PatternIntegrityViolatedByMutation
                | Self::AgencyVerifiedRequiresTriple { .. }
                | Self::AgencyVerifiedWithoutPrivilege
                | Self::AgencyVerifiedSovereignTouchNeedsAudit
                | Self::RegionCollapseRequiresDetRng
                | Self::RegionCollapseDoubleCollapseForbidden
                | Self::CohomologyClassMismatchOnTransform { .. }
        )
    }
}

// ─ Row-level checkers ──────────────────────────────────────────────────────

/// `true` iff `row` contains a `BuiltinEffect` matching `target`.
fn has_effect(row: &[EffectRef<'_>], target: BuiltinEffect) -> bool {
    row.iter().any(|e| e.builtin == Some(target))
}

/// Check σ-balance against `ENTROPY_EPSILON`. Returns `EFR0019` if the drift
/// exceeds tolerance.
///
/// § USE-CASE
///   The HIR layer collects `EntropyEdge`s declared by the op (e.g., from
///   `cast-spell` in Axiom 9 § I). When the row carries `EntropyBalanced`, the
///   HIR layer calls this check to verify the static-time edge-sum is within ε.
pub fn check_entropy_balance(edges: &[EntropyEdge]) -> Result<(), ConservationViolation> {
    let drift = sum_entropy_edges(edges);
    if drift.abs() > ENTROPY_EPSILON {
        Err(ConservationViolation::EntropyDriftExceeded {
            drift,
            epsilon: ENTROPY_EPSILON,
        })
    } else {
        Ok(())
    }
}

/// Like `check_entropy_balance` but with caller-provided ε (for project-tuning).
pub fn check_entropy_balance_with_epsilon(
    edges: &[EntropyEdge],
    epsilon: f64,
) -> Result<(), ConservationViolation> {
    let drift = sum_entropy_edges(edges);
    if drift.abs() > epsilon {
        Err(ConservationViolation::EntropyDriftExceeded { drift, epsilon })
    } else {
        Ok(())
    }
}

/// Check `EntropyBalanced` companion-discipline (EFR0020).
fn check_entropy_balanced_audit(
    row: &[EffectRef<'_>],
    ctx: &ConservationContext,
) -> Vec<ConservationViolation> {
    let mut violations = Vec::new();
    if has_effect(row, BuiltinEffect::EntropyBalanced) && !ctx.has_audit {
        violations.push(ConservationViolation::EntropyBalancedRequiresAudit);
    }
    violations
}

/// Check `PatternIntegrity` discipline (EFR0021 + EFR0022).
fn check_pattern_integrity(
    row: &[EffectRef<'_>],
    ctx: &ConservationContext,
) -> Vec<ConservationViolation> {
    let mut violations = Vec::new();
    let has_pattern = has_effect(row, BuiltinEffect::PatternIntegrity);

    // EFR0021 : Sovereign-touch without PatternIntegrity row
    if ctx.touches_sovereign && !has_pattern {
        violations.push(ConservationViolation::PatternIntegrityWithoutSovereign);
    }

    // EFR0022 : PatternIntegrity present + Pattern rewrite attempted but no
    // RECRYSTALLIZE-permission. The HIR layer signals "Pattern-rewrite
    // attempted" via `allows_pattern_rewrite=false` while still requiring
    // PatternIntegrity in the row. Stage-0 inverts : if PatternIntegrity is
    // present **and** the row contains `State<Phi>` (a HIR proxy for Pattern-
    // mutating-state), the `allows_pattern_rewrite` flag must be true.
    if has_pattern && row_has_pattern_mutator(row) && !ctx.allows_pattern_rewrite {
        violations.push(ConservationViolation::PatternIntegrityViolatedByMutation);
    }

    violations
}

/// Detect a `State<Phi>` row-element that signals an inline Pattern-mutator.
/// Stage-0 heuristic : a `State<...>` effect whose name string contains "Phi"
/// or "Pattern" (HIR layer uses canonical names ; user code can't fake this).
fn row_has_pattern_mutator(row: &[EffectRef<'_>]) -> bool {
    row.iter()
        .any(|e| matches!(e.builtin, Some(BuiltinEffect::State)) && is_pattern_state(e.name))
}

/// Heuristic : does this `State<T>` use-site mutate a Pattern?
fn is_pattern_state(state_name: &str) -> bool {
    state_name.contains("Phi") || state_name.contains("Pattern") || state_name == "Pattern"
}

/// Check `AgencyVerified` discipline (EFR0023 + EFR0024 + EFR0025).
fn check_agency_verified(
    row: &[EffectRef<'_>],
    ctx: &ConservationContext,
) -> Vec<ConservationViolation> {
    let mut violations = Vec::new();
    if !has_effect(row, BuiltinEffect::AgencyVerified) {
        return violations;
    }

    // EFR0023 : missing-leg in agency-triple
    if !(ctx.has_consent_witness && ctx.has_sovereignty_witness && ctx.has_reversibility_witness) {
        violations.push(ConservationViolation::AgencyVerifiedRequiresTriple {
            consent: ctx.has_consent_witness,
            sovereignty: ctx.has_sovereignty_witness,
            reversibility: ctx.has_reversibility_witness,
        });
    }

    // EFR0024 : Privilege<≥ Engine> not declared
    if !ctx.privilege_at_or_above_engine {
        violations.push(ConservationViolation::AgencyVerifiedWithoutPrivilege);
    }

    // EFR0025 : Sovereign-touch without Audit-companion
    if ctx.touches_sovereign && !ctx.has_audit {
        violations.push(ConservationViolation::AgencyVerifiedSovereignTouchNeedsAudit);
    }

    violations
}

/// Check `RegionCollapse` discipline (EFR0026 + EFR0027 + EFR0028).
fn check_region_collapse(
    row: &[EffectRef<'_>],
    ctx: &ConservationContext,
) -> Vec<ConservationViolation> {
    let mut violations = Vec::new();
    if !has_effect(row, BuiltinEffect::RegionCollapse) {
        return violations;
    }

    // EFR0026 : DetRNG required (Axiom 5 § IV)
    if !ctx.has_det_rng && !has_effect(row, BuiltinEffect::DetRng) {
        violations.push(ConservationViolation::RegionCollapseRequiresDetRng);
    }

    // EFR0027 advisory : Cohomology<_> companion missing
    if !ctx.has_cohomology_companion && !has_effect(row, BuiltinEffect::Cohomology) {
        violations.push(ConservationViolation::RegionCollapseWithoutCohomology);
    }

    // EFR0028 : double-collapse on already-collapsed region
    if ctx.region_already_collapsed {
        violations.push(ConservationViolation::RegionCollapseDoubleCollapseForbidden);
    }

    violations
}

/// Check `Cohomology<H>` discipline (EFR0029 + EFR0030).
fn check_cohomology(
    row: &[EffectRef<'_>],
    ctx: &ConservationContext,
) -> Vec<ConservationViolation> {
    let mut violations = Vec::new();
    if !has_effect(row, BuiltinEffect::Cohomology) {
        return violations;
    }

    // EFR0029 : Audit<dom> companion required
    if !ctx.has_audit {
        violations.push(ConservationViolation::CohomologyRequiresAuditSpan);
    }

    // EFR0030 : class-id mismatch on transform
    if let (Some(expected), Some(actual)) = (
        ctx.expected_cohomology_class_id,
        ctx.row_cohomology_class_id,
    ) {
        if expected != actual {
            violations.push(ConservationViolation::CohomologyClassMismatchOnTransform {
                expected,
                actual,
            });
        }
    }

    violations
}

/// The full conservation-discipline check. Combines all six row-checkers.
///
/// § ALGORITHM
///   1. Run each row-level checker against `row` + `ctx` ;
///   2. Concatenate all violations ;
///   3. Return `Ok(())` if empty, `Err(Vec<...>)` otherwise.
///
/// § ORDERING
///   The returned vec orders violations by EFR-code ascending. Tooling that
///   greps for the first error gets a deterministic "earliest-rule-violated"
///   diagnostic.
///
/// § USE-CASE
///   The HIR layer calls this **after** `try_compose` (substrate-axis) +
///   `banned_composition` (PRIME-DIRECTIVE-axis). All three checkers must
///   pass before the HIR layer accepts the row.
pub fn check_conservation(
    row: &[EffectRef<'_>],
    ctx: &ConservationContext,
) -> Result<(), Vec<ConservationViolation>> {
    let mut violations = Vec::new();
    violations.extend(check_entropy_balanced_audit(row, ctx));
    violations.extend(check_pattern_integrity(row, ctx));
    violations.extend(check_agency_verified(row, ctx));
    violations.extend(check_region_collapse(row, ctx));
    violations.extend(check_cohomology(row, ctx));
    if violations.is_empty() {
        Ok(())
    } else {
        Err(violations)
    }
}

// ─ Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::EffectRegistry;

    fn e(name: &'static str, builtin: Option<BuiltinEffect>, arity: usize) -> EffectRef<'static> {
        EffectRef {
            name,
            builtin,
            arg_count: arity,
        }
    }

    // ── BuiltinEffect registration smoke-tests ─────────────────────────

    #[test]
    fn entropy_balanced_registered() {
        let r = EffectRegistry::with_builtins();
        let m = r
            .lookup("EntropyBalanced")
            .expect("EntropyBalanced present");
        assert_eq!(m.effect, BuiltinEffect::EntropyBalanced);
    }

    #[test]
    fn pattern_integrity_registered() {
        let r = EffectRegistry::with_builtins();
        let m = r
            .lookup("PatternIntegrity")
            .expect("PatternIntegrity present");
        assert_eq!(m.effect, BuiltinEffect::PatternIntegrity);
    }

    #[test]
    fn agency_verified_registered() {
        let r = EffectRegistry::with_builtins();
        let m = r.lookup("AgencyVerified").expect("AgencyVerified present");
        assert_eq!(m.effect, BuiltinEffect::AgencyVerified);
    }

    #[test]
    fn region_collapse_registered() {
        let r = EffectRegistry::with_builtins();
        let m = r.lookup("RegionCollapse").expect("RegionCollapse present");
        assert_eq!(m.effect, BuiltinEffect::RegionCollapse);
    }

    #[test]
    fn cohomology_registered() {
        let r = EffectRegistry::with_builtins();
        let m = r.lookup("Cohomology").expect("Cohomology present");
        assert_eq!(m.effect, BuiltinEffect::Cohomology);
    }

    #[test]
    fn region_still_registered() {
        // Region was already in the registry ; verify T11-D128 didn't break it.
        let r = EffectRegistry::with_builtins();
        let m = r.lookup("Region").expect("Region present");
        assert_eq!(m.effect, BuiltinEffect::Region);
    }

    #[test]
    fn registry_size_grew_by_five() {
        // Pre-D128 had 32 entries (28 base + Yield + State + Region + Resume +
        // others ; see registry's own count test). Post-D128 added 5 (Entropy +
        // Pattern + Agency + RegionCollapse + Cohomology).
        let r = EffectRegistry::with_builtins();
        assert!(
            r.len() >= 33,
            "expected at least 33 effects after T11-D128 ; found {}",
            r.len()
        );
    }

    // ── ConservationContext builder tests ──────────────────────────────

    #[test]
    fn default_context_is_strictest() {
        let c = ConservationContext::default();
        assert!(!c.touches_sovereign);
        assert!(!c.privilege_at_or_above_engine);
        assert!(!c.has_det_rng);
        assert!(!c.has_audit);
        assert!(!c.allows_pattern_rewrite);
        assert!(!c.has_consent_witness);
        assert!(!c.has_sovereignty_witness);
        assert!(!c.has_reversibility_witness);
    }

    #[test]
    fn agency_triple_builder_sets_three_legs() {
        let c = ConservationContext::default().with_agency_triple();
        assert!(c.has_consent_witness);
        assert!(c.has_sovereignty_witness);
        assert!(c.has_reversibility_witness);
    }

    #[test]
    fn cohomology_class_ids_builder() {
        let c = ConservationContext::default().with_cohomology_class_ids(0xDEAD, 0xBEEF);
        assert_eq!(c.expected_cohomology_class_id, Some(0xDEAD));
        assert_eq!(c.row_cohomology_class_id, Some(0xBEEF));
    }

    // ── EntropyEdge tests ──────────────────────────────────────────────

    #[test]
    fn entropy_debit_negates() {
        let e = EntropyEdge::debit(5.0, 0);
        assert_eq!(e.delta, -5.0);
    }

    #[test]
    fn entropy_credit_positive() {
        let e = EntropyEdge::credit(5.0, 0);
        assert_eq!(e.delta, 5.0);
    }

    #[test]
    fn balanced_pair_sums_to_zero() {
        let edges = [EntropyEdge::debit(3.0, 0), EntropyEdge::credit(3.0, 1)];
        assert_eq!(sum_entropy_edges(&edges), 0.0);
    }

    // ── EFR0019 EntropyDriftExceeded ───────────────────────────────────

    #[test]
    fn balanced_edges_pass() {
        let edges = [EntropyEdge::debit(7.0, 0), EntropyEdge::credit(7.0, 1)];
        assert!(check_entropy_balance(&edges).is_ok());
    }

    #[test]
    fn drifted_edges_fail_efr0019() {
        let edges = [EntropyEdge::debit(7.0, 0), EntropyEdge::credit(6.0, 1)];
        let res = check_entropy_balance(&edges);
        assert!(matches!(
            res,
            Err(ConservationViolation::EntropyDriftExceeded { .. })
        ));
        assert_eq!(res.unwrap_err().code(), "EFR0019");
    }

    #[test]
    fn drift_within_epsilon_passes() {
        // Drift = 1e-9 < ENTROPY_EPSILON = 1e-6
        let edges = [
            EntropyEdge::debit(1.0, 0),
            EntropyEdge::credit(1.0 + 1e-9, 1),
        ];
        assert!(check_entropy_balance(&edges).is_ok());
    }

    #[test]
    fn custom_epsilon_tightens() {
        // Drift = 1e-7 > custom-ε = 1e-9 should fail
        let edges = [
            EntropyEdge::debit(1.0, 0),
            EntropyEdge::credit(1.0 + 1e-7, 1),
        ];
        assert!(check_entropy_balance_with_epsilon(&edges, 1e-9).is_err());
    }

    // ── EFR0020 EntropyBalancedRequiresAudit ───────────────────────────

    #[test]
    fn entropy_balanced_without_audit_efr0020() {
        let row = vec![e(
            "EntropyBalanced",
            Some(BuiltinEffect::EntropyBalanced),
            0,
        )];
        let ctx = ConservationContext::default();
        let res = check_conservation(&row, &ctx);
        assert!(res.is_err());
        let v = res.unwrap_err();
        assert!(v.iter().any(|x| x.code() == "EFR0020"));
    }

    #[test]
    fn entropy_balanced_with_audit_clean() {
        let row = vec![e(
            "EntropyBalanced",
            Some(BuiltinEffect::EntropyBalanced),
            0,
        )];
        let ctx = ConservationContext::default().with_audit();
        assert!(check_conservation(&row, &ctx).is_ok());
    }

    // ── EFR0021 PatternIntegrityWithoutSovereign ───────────────────────

    #[test]
    fn sovereign_touch_without_pattern_integrity_efr0021() {
        let row: Vec<EffectRef<'_>> = vec![];
        let ctx = ConservationContext::default().with_sovereign_touch();
        let res = check_conservation(&row, &ctx);
        assert!(res.is_err());
        let v = res.unwrap_err();
        assert!(v.iter().any(|x| x.code() == "EFR0021"));
    }

    #[test]
    fn sovereign_touch_with_pattern_integrity_no_efr0021() {
        let row = vec![e(
            "PatternIntegrity",
            Some(BuiltinEffect::PatternIntegrity),
            0,
        )];
        let ctx = ConservationContext::default().with_sovereign_touch();
        let res = check_conservation(&row, &ctx);
        let v = res.err().unwrap_or_default();
        assert!(!v.iter().any(|x| x.code() == "EFR0021"));
    }

    // ── EFR0022 PatternIntegrityViolatedByMutation ─────────────────────

    #[test]
    fn pattern_integrity_with_phi_mutator_no_consent_efr0022() {
        let row = vec![
            e("PatternIntegrity", Some(BuiltinEffect::PatternIntegrity), 0),
            e("State<Phi>", Some(BuiltinEffect::State), 1),
        ];
        let ctx = ConservationContext::default().with_sovereign_touch();
        let res = check_conservation(&row, &ctx);
        let v = res.unwrap_err();
        assert!(v.iter().any(|x| x.code() == "EFR0022"));
    }

    #[test]
    fn pattern_integrity_with_phi_mutator_consent_clean() {
        let row = vec![
            e("PatternIntegrity", Some(BuiltinEffect::PatternIntegrity), 0),
            e("State<Phi>", Some(BuiltinEffect::State), 1),
        ];
        let ctx = ConservationContext::default()
            .with_sovereign_touch()
            .with_pattern_rewrite();
        let res = check_conservation(&row, &ctx);
        // EFR0022 must not surface ; other rules may still fire (e.g., EFR0021).
        let v = res.err().unwrap_or_default();
        assert!(!v.iter().any(|x| x.code() == "EFR0022"));
    }

    #[test]
    fn pattern_integrity_with_state_pattern_canonical() {
        // is_pattern_state covers the literal "Pattern" name too.
        let row = vec![
            e("PatternIntegrity", Some(BuiltinEffect::PatternIntegrity), 0),
            e("Pattern", Some(BuiltinEffect::State), 1),
        ];
        let ctx = ConservationContext::default().with_sovereign_touch();
        let res = check_conservation(&row, &ctx);
        let v = res.unwrap_err();
        assert!(v.iter().any(|x| x.code() == "EFR0022"));
    }

    // ── EFR0023 AgencyVerifiedRequiresTriple ───────────────────────────

    #[test]
    fn agency_verified_no_legs_efr0023() {
        let row = vec![e("AgencyVerified", Some(BuiltinEffect::AgencyVerified), 0)];
        let ctx = ConservationContext::default()
            .with_privilege_engine()
            .with_audit();
        let res = check_conservation(&row, &ctx);
        let v = res.unwrap_err();
        assert!(v.iter().any(|x| x.code() == "EFR0023"));
    }

    #[test]
    fn agency_verified_partial_legs_efr0023() {
        let row = vec![e("AgencyVerified", Some(BuiltinEffect::AgencyVerified), 0)];
        let ctx = ConservationContext::default()
            .with_privilege_engine()
            .with_audit()
            .with_consent_witness()
            .with_sovereignty_witness();
        // Missing reversibility-witness → EFR0023
        let res = check_conservation(&row, &ctx);
        let v = res.unwrap_err();
        assert!(v.iter().any(|x| x.code() == "EFR0023"));
        // Diagnostic carries the per-leg state
        let triple = v
            .iter()
            .find(|x| {
                matches!(
                    x,
                    ConservationViolation::AgencyVerifiedRequiresTriple { .. }
                )
            })
            .unwrap();
        if let ConservationViolation::AgencyVerifiedRequiresTriple {
            consent,
            sovereignty,
            reversibility,
        } = triple
        {
            assert!(consent);
            assert!(sovereignty);
            assert!(!reversibility);
        }
    }

    #[test]
    fn agency_verified_full_triple_no_efr0023() {
        let row = vec![e("AgencyVerified", Some(BuiltinEffect::AgencyVerified), 0)];
        let ctx = ConservationContext::default()
            .with_privilege_engine()
            .with_audit()
            .with_agency_triple();
        let res = check_conservation(&row, &ctx);
        // No errors at all ; the row is well-formed.
        assert!(res.is_ok(), "got {res:?}");
    }

    // ── EFR0024 AgencyVerifiedWithoutPrivilege ─────────────────────────

    #[test]
    fn agency_verified_user_privilege_efr0024() {
        let row = vec![e("AgencyVerified", Some(BuiltinEffect::AgencyVerified), 0)];
        let ctx = ConservationContext::default()
            .with_audit()
            .with_agency_triple();
        // No privilege_engine → EFR0024
        let v = check_conservation(&row, &ctx).unwrap_err();
        assert!(v.iter().any(|x| x.code() == "EFR0024"));
    }

    // ── EFR0025 AgencyVerifiedSovereignTouchNeedsAudit ─────────────────

    #[test]
    fn agency_verified_sovereign_touch_no_audit_efr0025() {
        let row = vec![
            e("AgencyVerified", Some(BuiltinEffect::AgencyVerified), 0),
            // PatternIntegrity to avoid EFR0021 confounding.
            e("PatternIntegrity", Some(BuiltinEffect::PatternIntegrity), 0),
        ];
        let ctx = ConservationContext::default()
            .with_privilege_engine()
            .with_agency_triple()
            .with_sovereign_touch();
        let v = check_conservation(&row, &ctx).unwrap_err();
        assert!(v.iter().any(|x| x.code() == "EFR0025"));
    }

    #[test]
    fn agency_verified_sovereign_touch_with_audit_clean() {
        let row = vec![
            e("AgencyVerified", Some(BuiltinEffect::AgencyVerified), 0),
            e("PatternIntegrity", Some(BuiltinEffect::PatternIntegrity), 0),
        ];
        let ctx = ConservationContext::default()
            .with_privilege_engine()
            .with_agency_triple()
            .with_sovereign_touch()
            .with_audit();
        let res = check_conservation(&row, &ctx);
        assert!(res.is_ok(), "got {res:?}");
    }

    // ── EFR0026 RegionCollapseRequiresDetRng ───────────────────────────

    #[test]
    fn region_collapse_no_detrng_efr0026() {
        let row = vec![e("RegionCollapse", Some(BuiltinEffect::RegionCollapse), 0)];
        let ctx = ConservationContext::default().with_cohomology_companion();
        let v = check_conservation(&row, &ctx).unwrap_err();
        assert!(v.iter().any(|x| x.code() == "EFR0026"));
    }

    #[test]
    fn region_collapse_via_detrng_in_row_no_efr0026() {
        let row = vec![
            e("RegionCollapse", Some(BuiltinEffect::RegionCollapse), 0),
            e("DetRNG", Some(BuiltinEffect::DetRng), 0),
            e("Cohomology<_>", Some(BuiltinEffect::Cohomology), 1),
        ];
        let ctx = ConservationContext::default();
        let res = check_conservation(&row, &ctx);
        // Cohomology is in row → EFR0027 cleared. DetRNG in row → EFR0026 cleared.
        // Cohomology<_> in row triggers EFR0029 (no audit) ; that's the only one.
        let v = res.err().unwrap_or_default();
        assert!(!v.iter().any(|x| x.code() == "EFR0026"));
        assert!(!v.iter().any(|x| x.code() == "EFR0027"));
    }

    #[test]
    fn region_collapse_via_ctx_detrng_no_efr0026() {
        let row = vec![
            e("RegionCollapse", Some(BuiltinEffect::RegionCollapse), 0),
            e("Cohomology<_>", Some(BuiltinEffect::Cohomology), 1),
        ];
        let ctx = ConservationContext::default().with_det_rng().with_audit();
        let res = check_conservation(&row, &ctx);
        // No EFR0026 because ctx says DetRNG ; no EFR0027 because Cohomology in row ;
        // no EFR0029 because audit is set.
        assert!(res.is_ok(), "got {res:?}");
    }

    // ── EFR0027 RegionCollapseWithoutCohomology ────────────────────────

    #[test]
    fn region_collapse_no_cohom_efr0027() {
        let row = vec![
            e("RegionCollapse", Some(BuiltinEffect::RegionCollapse), 0),
            e("DetRNG", Some(BuiltinEffect::DetRng), 0),
        ];
        let ctx = ConservationContext::default();
        let v = check_conservation(&row, &ctx).unwrap_err();
        assert!(v.iter().any(|x| x.code() == "EFR0027"));
    }

    // ── EFR0028 RegionCollapseDoubleCollapseForbidden ──────────────────

    #[test]
    fn double_collapse_efr0028() {
        let row = vec![
            e("RegionCollapse", Some(BuiltinEffect::RegionCollapse), 0),
            e("DetRNG", Some(BuiltinEffect::DetRng), 0),
            e("Cohomology<_>", Some(BuiltinEffect::Cohomology), 1),
        ];
        let ctx = ConservationContext::default()
            .with_audit()
            .with_region_already_collapsed();
        let v = check_conservation(&row, &ctx).unwrap_err();
        assert!(v.iter().any(|x| x.code() == "EFR0028"));
    }

    // ── EFR0029 CohomologyRequiresAuditSpan ────────────────────────────

    #[test]
    fn cohomology_no_audit_efr0029() {
        let row = vec![e("Cohomology<_>", Some(BuiltinEffect::Cohomology), 1)];
        let ctx = ConservationContext::default();
        let v = check_conservation(&row, &ctx).unwrap_err();
        assert!(v.iter().any(|x| x.code() == "EFR0029"));
    }

    #[test]
    fn cohomology_with_audit_no_efr0029() {
        let row = vec![e("Cohomology<_>", Some(BuiltinEffect::Cohomology), 1)];
        let ctx = ConservationContext::default().with_audit();
        let res = check_conservation(&row, &ctx);
        let v = res.err().unwrap_or_default();
        assert!(!v.iter().any(|x| x.code() == "EFR0029"));
    }

    // ── EFR0030 CohomologyClassMismatchOnTransform ─────────────────────

    #[test]
    fn cohomology_class_mismatch_efr0030() {
        let row = vec![e("Cohomology<H>", Some(BuiltinEffect::Cohomology), 1)];
        let ctx = ConservationContext::default()
            .with_audit()
            .with_cohomology_class_ids(0xAAAA_AAAA, 0xBBBB_BBBB);
        let v = check_conservation(&row, &ctx).unwrap_err();
        assert!(v.iter().any(|x| x.code() == "EFR0030"));
        let mismatch = v
            .iter()
            .find(|x| {
                matches!(
                    x,
                    ConservationViolation::CohomologyClassMismatchOnTransform { .. }
                )
            })
            .unwrap();
        if let ConservationViolation::CohomologyClassMismatchOnTransform { expected, actual } =
            mismatch
        {
            assert_eq!(*expected, 0xAAAA_AAAA);
            assert_eq!(*actual, 0xBBBB_BBBB);
        }
    }

    #[test]
    fn cohomology_class_match_no_efr0030() {
        let row = vec![e("Cohomology<H>", Some(BuiltinEffect::Cohomology), 1)];
        let ctx = ConservationContext::default()
            .with_audit()
            .with_cohomology_class_ids(0xCAFE, 0xCAFE);
        assert!(check_conservation(&row, &ctx).is_ok());
    }

    // ── Code-discipline tests ──────────────────────────────────────────

    #[test]
    fn all_efr_codes_distinct_and_stable() {
        // T11-D128 block : EFR0019..EFR0030 (12 codes).
        let reasons = [
            ConservationViolation::EntropyDriftExceeded {
                drift: 1.0,
                epsilon: 1e-6,
            },
            ConservationViolation::EntropyBalancedRequiresAudit,
            ConservationViolation::PatternIntegrityWithoutSovereign,
            ConservationViolation::PatternIntegrityViolatedByMutation,
            ConservationViolation::AgencyVerifiedRequiresTriple {
                consent: false,
                sovereignty: false,
                reversibility: false,
            },
            ConservationViolation::AgencyVerifiedWithoutPrivilege,
            ConservationViolation::AgencyVerifiedSovereignTouchNeedsAudit,
            ConservationViolation::RegionCollapseRequiresDetRng,
            ConservationViolation::RegionCollapseWithoutCohomology,
            ConservationViolation::RegionCollapseDoubleCollapseForbidden,
            ConservationViolation::CohomologyRequiresAuditSpan,
            ConservationViolation::CohomologyClassMismatchOnTransform {
                expected: 0,
                actual: 0,
            },
        ];
        let codes: Vec<&str> = reasons.iter().map(|r| r.code()).collect();
        assert_eq!(codes.len(), 12);
        assert_eq!(
            codes,
            vec![
                "EFR0019", "EFR0020", "EFR0021", "EFR0022", "EFR0023", "EFR0024", "EFR0025",
                "EFR0026", "EFR0027", "EFR0028", "EFR0029", "EFR0030",
            ]
        );
    }

    #[test]
    fn hard_vs_advisory_classification() {
        // Hard
        assert!(ConservationViolation::EntropyDriftExceeded {
            drift: 1.0,
            epsilon: 1e-6,
        }
        .is_hard_error());
        assert!(ConservationViolation::PatternIntegrityWithoutSovereign.is_hard_error());
        assert!(ConservationViolation::PatternIntegrityViolatedByMutation.is_hard_error());
        assert!(ConservationViolation::AgencyVerifiedRequiresTriple {
            consent: false,
            sovereignty: false,
            reversibility: false,
        }
        .is_hard_error());
        assert!(ConservationViolation::AgencyVerifiedWithoutPrivilege.is_hard_error());
        assert!(ConservationViolation::AgencyVerifiedSovereignTouchNeedsAudit.is_hard_error());
        assert!(ConservationViolation::RegionCollapseRequiresDetRng.is_hard_error());
        assert!(ConservationViolation::RegionCollapseDoubleCollapseForbidden.is_hard_error());
        assert!(ConservationViolation::CohomologyClassMismatchOnTransform {
            expected: 0,
            actual: 0,
        }
        .is_hard_error());
        // Advisory
        assert!(!ConservationViolation::EntropyBalancedRequiresAudit.is_hard_error());
        assert!(!ConservationViolation::RegionCollapseWithoutCohomology.is_hard_error());
        assert!(!ConservationViolation::CohomologyRequiresAuditSpan.is_hard_error());
    }

    #[test]
    fn empty_row_clean() {
        let row: Vec<EffectRef<'_>> = vec![];
        let ctx = ConservationContext::default();
        assert!(check_conservation(&row, &ctx).is_ok());
    }

    // ── Integration tests : the canonical Ω-omega-step row ─────────────

    #[test]
    fn canonical_omega_step_row_clean() {
        // Per Omniverse/02_CSSL/00_LANGUAGE_CONTRACT § V :
        // fn omega_step(...) / { Realtime<60Hz>, Deadline<16ms>, DetRNG, Audit<'tick>,
        //                        EntropyBalanced, PatternIntegrity, AgencyVerified }
        let row = vec![
            e("Realtime", Some(BuiltinEffect::Realtime), 1),
            e("Deadline", Some(BuiltinEffect::Deadline), 1),
            e("DetRNG", Some(BuiltinEffect::DetRng), 0),
            e("Audit<'tick>", Some(BuiltinEffect::Audit), 1),
            e("EntropyBalanced", Some(BuiltinEffect::EntropyBalanced), 0),
            e("PatternIntegrity", Some(BuiltinEffect::PatternIntegrity), 0),
            e("AgencyVerified", Some(BuiltinEffect::AgencyVerified), 0),
        ];
        let ctx = ConservationContext::default()
            .with_audit()
            .with_privilege_engine()
            .with_agency_triple()
            .with_sovereign_touch();
        let res = check_conservation(&row, &ctx);
        assert!(
            res.is_ok(),
            "canonical omega_step row should be clean ; got {res:?}"
        );
    }

    #[test]
    fn canonical_oracle_sample_row_clean() {
        // Per Omniverse/02_CSSL/02_EFFECTS § II :
        // fn oracle_sample(...) / { DetRNG, RegionCollapse, Crystallize,
        //                            Region<'collapse>, Cohomology<_> }
        // Note : Crystallize isn't a CSSLv3-stage0 BuiltinEffect (it's an
        // Omniverse layer-2 add-on per § I taxonomy table). For this stage-0
        // test we drop Crystallize and verify the conservation-axis is clean.
        let row = vec![
            e("DetRNG", Some(BuiltinEffect::DetRng), 0),
            e("RegionCollapse", Some(BuiltinEffect::RegionCollapse), 0),
            e("Region", Some(BuiltinEffect::Region), 1),
            e("Cohomology<_>", Some(BuiltinEffect::Cohomology), 1),
            e("Audit<'collapse>", Some(BuiltinEffect::Audit), 1),
        ];
        let ctx = ConservationContext::default().with_audit();
        let res = check_conservation(&row, &ctx);
        assert!(
            res.is_ok(),
            "canonical oracle_sample row should be clean ; got {res:?}"
        );
    }

    // ── Multiple-violation aggregation tests ───────────────────────────

    #[test]
    fn multiple_violations_reported() {
        // RegionCollapse without DetRNG nor Cohomology + already-collapsed →
        // EFR0026 + EFR0027 + EFR0028 simultaneously.
        let row = vec![e("RegionCollapse", Some(BuiltinEffect::RegionCollapse), 0)];
        let ctx = ConservationContext::default().with_region_already_collapsed();
        let v = check_conservation(&row, &ctx).unwrap_err();
        let codes: std::collections::HashSet<_> = v.iter().map(|x| x.code()).collect();
        assert!(codes.contains("EFR0026"));
        assert!(codes.contains("EFR0027"));
        assert!(codes.contains("EFR0028"));
    }

    #[test]
    fn agency_verified_full_violation_aggregation() {
        // AgencyVerified + Sovereign-touch ; no privilege, no audit, no triple.
        let row = vec![e("AgencyVerified", Some(BuiltinEffect::AgencyVerified), 0)];
        let ctx = ConservationContext::default().with_sovereign_touch();
        let v = check_conservation(&row, &ctx).unwrap_err();
        let codes: std::collections::HashSet<_> = v.iter().map(|x| x.code()).collect();
        // EFR0021 (Sovereign-touch, no PatternIntegrity)
        assert!(codes.contains("EFR0021"));
        // EFR0023 (missing-triple)
        assert!(codes.contains("EFR0023"));
        // EFR0024 (no privilege)
        assert!(codes.contains("EFR0024"));
        // EFR0025 (Sovereign-touch, no Audit)
        assert!(codes.contains("EFR0025"));
    }

    // ── Diagnostic message tests ───────────────────────────────────────

    #[test]
    fn diagnostic_message_efr0019_actionable() {
        let v = ConservationViolation::EntropyDriftExceeded {
            drift: 1.234e-3,
            epsilon: 1e-6,
        };
        let msg = format!("{v}");
        assert!(msg.contains("EFR0019"));
        assert!(msg.contains("σ-balance drift"));
        assert!(msg.contains("Axiom 9"));
        assert!(msg.contains("help:"));
    }

    #[test]
    fn diagnostic_message_efr0023_carries_legs() {
        let v = ConservationViolation::AgencyVerifiedRequiresTriple {
            consent: true,
            sovereignty: false,
            reversibility: false,
        };
        let msg = format!("{v}");
        assert!(msg.contains("EFR0023"));
        assert!(msg.contains("consent=true"));
        assert!(msg.contains("sovereignty=false"));
        assert!(msg.contains("reversibility=false"));
        assert!(msg.contains("Axiom 4"));
    }

    #[test]
    fn diagnostic_message_efr0030_carries_class_ids() {
        let v = ConservationViolation::CohomologyClassMismatchOnTransform {
            expected: 0xCAFE,
            actual: 0xBABE,
        };
        let msg = format!("{v}");
        assert!(msg.contains("EFR0030"));
        assert!(msg.contains("0xcafe"));
        assert!(msg.contains("0xbabe"));
        assert!(msg.contains("Axiom 7"));
    }

    // ── is_pattern_state heuristic tests ───────────────────────────────

    #[test]
    fn is_pattern_state_recognizes_phi() {
        assert!(is_pattern_state("State<Phi>"));
        assert!(is_pattern_state("Phi"));
    }

    #[test]
    fn is_pattern_state_recognizes_pattern() {
        assert!(is_pattern_state("Pattern"));
        assert!(is_pattern_state("State<Pattern>"));
    }

    #[test]
    fn is_pattern_state_rejects_unrelated() {
        assert!(!is_pattern_state("State<Counter>"));
        assert!(!is_pattern_state("u32"));
    }
}
