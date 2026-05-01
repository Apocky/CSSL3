// § T11-W8-C2 : cssl-host-coherence-proof
// ══════════════════════════════════════════════════════════════════════════════
// § I> Coherence-Proof = Σ-Chain consensus-validator
//   ⊑ deterministic-recompute (state · lineage · sig) → new-merkle-root
//   ⊑ 2-validator agreement → Verified
//   ⊑ disagreement → DisagreementFlag → audit-emit ; tie-break Ed25519-sig hex-asc
//   ⊑ ServerTick monotonic-counter ⇒ timestamp-trust-anchor
//   ⊑ tampering (sig-fail) → TamperDetected
// ══════════════════════════════════════════════════════════════════════════════
// § I> trait-decoupled : SigmaEventLike (sibling-crate-shape) ; AuditEmitter (audit-bus)
//   ¬ direct-deps on cssl-host-sigma-chain or cssl-host-attestation
//   mock-impls in tests + AuditEmitter::stderr fallback for dev-runs
// § I> determinism : BTreeMap-orderings ; lineage sorted (ts asc · id asc tie-break)
//   merkle-padding rule : duplicate-last-leaf when odd ; documented in `merkle` mod
// § I> tie-break-rule : Ed25519-sig (64 bytes) → lower-hex → lexicographic-asc compare
//   THE LEXICOGRAPHICALLY-LOWEST hex IS THE WINNING SIGNATURE (canonical-low-wins).
// ══════════════════════════════════════════════════════════════════════════════
#![forbid(unsafe_code)]
#![doc = "Coherence-Proof consensus-validator for Σ-Chain (W8-C2)."]

pub mod audit;
pub mod consensus;
pub mod disagreement;
pub mod event;
pub mod lineage;
pub mod merkle;
pub mod recompute;
pub mod sig_serde;
pub mod state;
pub mod tick;
pub mod tiebreak;

pub use audit::{AuditEmitter, AuditEvent, StderrAuditEmitter, VecAuditEmitter};
pub use consensus::{ConsensusReport, ConsensusValidator, ValidatorView};
pub use disagreement::{DisagreementFlag, DisagreementReason};
pub use event::{EventId, MockSigmaEvent, PubKey, SigBytes, SigmaEventLike};
pub use lineage::{Lineage, LineageError};
pub use merkle::{merkle_root_blake3, MerkleRoot};
pub use recompute::{recompute_event_effect, RecomputeError, VerificationOutcome};
pub use state::{StateSnapshot, StateSnapshotLike};
pub use tick::{ServerTick, TickError, TickStream};
pub use tiebreak::{ed25519_hex_asc_winner, hex_lower};

// § public-version-tag for diagnostics + canonical-spec-ref
/// Spec-version this validator implements.
pub const SPEC_VERSION: &str = "T11-W8-C2";

/// Canonical-low-wins documentation marker — emitted in audit-events.
pub const TIEBREAK_RULE: &str = "Ed25519-sig hex-asc lexicographic-low-wins";

// § attestation per PRIME_DIRECTIVE.md § 11
/// Attestation-string embedded in this crate.
pub const ATTESTATION: &str =
    "There was no hurt nor harm in the making of this, to anyone, anything, or anybody.";
