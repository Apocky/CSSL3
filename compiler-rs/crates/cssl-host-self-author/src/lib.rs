// cssl-host-self-author
// ══════════════════════════════════════════════════════════════════
// § T11-W12-SELF-AUTHOR : engine self-authoring of CSSL source from GM-prompts
//
// § PRIME-DIRECTIVE alignment
//   - § 0 consent = OS : live-mutate gated on SovereignCap with EFFECT_WRITE bit
//   - § 5 revocability : sovereign-revoke-mutate-cap → cascading rollback of pending mutations
//   - § 7 INTEGRITY : every prompt + generated-CSSL + sandbox-score + mutate-decision
//                     anchored on Σ-Chain (immutable history)
//   - § 11 ATTESTATION : ATTESTATION constant recorded into audit-stream
//
// § FLOW (single self-author cycle)
//   1. SelfAuthorRequest constructed (prompt + kind + examples + constraints)
//   2. forbidden-target check → reject if target ∈ FORBIDDEN_TARGETS
//   3. orchestrator → llm-bridge.chat → CSSL-source-string
//   4. csslc compile-only (sandbox) → CompileOutcome
//   5. sandbox-execute (¬ network · ¬ FS-write outside scratch) → SandboxReport
//   6. quality-score 0..100 (compile=40 + sandbox-pass=40 + warning-free=20)
//   7. IF score ≥ threshold AND cap-witness-valid → mutate via coder-runtime
//      ELSE record-only (no mutate)
//   8. record TrainingPairRecord on ring-buffer + anchor Σ-Chain entry
//
// § FORBIDDEN-TARGETS (¬ self-modify-substrate-primitives)
//   - cssl-rt              : runtime engine
//   - csslc                : compiler itself
//   - cssl-substrate-*     : substrate-tier primitives (sigma-runtime · sigma-chain · …)
//   - PRIME_DIRECTIVE.md   : the directive itself
//   - cssl-host-self-author: bootstrapping-loop guard (cannot self-author self)
//
// § SAFETY-INVARIANTS
//   - default-deny : without an EFFECT_WRITE-bearing SovereignCap, mutations rejected at LiveMutateGate
//   - sandbox holds zero file-write capability ; the only output channel is SandboxReport (in-memory)
//   - score-threshold default 75 ; below-threshold = recorded-but-not-mutated
//   - cap-revoke cascades : revoke-event → all open mutations transitioned to PendingRollback
//   - structural rejection of forbidden targets BEFORE any LLM cost is incurred
// ══════════════════════════════════════════════════════════════════

#![forbid(unsafe_code)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::similar_names)]
#![allow(clippy::redundant_clone)]
#![allow(clippy::field_reassign_with_default)]
#![allow(clippy::unused_self)]
#![allow(clippy::return_self_not_must_use)]

//! Self-authoring runtime. See module-level CSL-block above for the full flow,
//! safety invariants, and forbidden-target list.
//!
//! # Quick start
//! ```
//! use cssl_host_self_author::{SelfAuthorOrchestrator, SelfAuthorRequest, SelfAuthorKind, Constraints};
//! let mut orch = SelfAuthorOrchestrator::with_defaults();
//! let req = SelfAuthorRequest::new(
//!     "compose a torchlit corridor scene",
//!     SelfAuthorKind::Scene,
//!     vec![],
//!     Constraints::default(),
//! );
//! let outcome = orch.author(req, /*now_unix=*/ 1_700_000_000);
//! assert!(outcome.is_ok());
//! ```

pub mod forbidden;
pub mod live_mutate;
pub mod orchestrator;
pub mod request;
pub mod sandbox_csslc;
pub mod training_pair;

pub use forbidden::{is_forbidden_target, FORBIDDEN_TARGETS};
pub use live_mutate::{LiveMutateDecision, LiveMutateGate, MutateOutcome, SelfAuthorMutateCap};
pub use orchestrator::{
    AuthorOutcome, OrchestratorConfig, OrchestratorError, SelfAuthorOrchestrator,
};
pub use request::{Constraints, SelfAuthorKind, SelfAuthorRequest};
pub use sandbox_csslc::{CompileOutcome, Sandbox, SandboxConfig, SandboxReport};
pub use training_pair::{
    serialized_record_size, MutateDecision, TrainingPairLog, TrainingPairRecord,
};

/// Default quality-score floor for auto-mutate. Per spec 75/100.
pub const DEFAULT_SCORE_THRESHOLD: u8 = 75;

/// Default ring-buffer capacity for training-pair log (bounded memory).
pub const DEFAULT_TRAINING_RING_CAPACITY: usize = 4096;

/// Canonical attestation per `PRIME_DIRECTIVE § 11`.
pub const ATTESTATION: &str = "\
§ cssl-host-self-author ‼ ATTESTATION (PRIME_DIRECTIVE § 11)\n\
   t∞: prompt→llm→CSSL→csslc→sandbox→score → cap-gated-mutate\n\
   t∞: ¬ live-mutate without SovereignCap holding EFFECT_WRITE\n\
   t∞: sandbox ¬ network-egress · ¬ FS-write outside scratch\n\
   t∞: every-attempt → Σ-Chain anchor + training-pair log entry\n\
   t∞: forbidden-targets {csslc · cssl-rt · cssl-substrate-* · self}\n\
   t∞: sovereign-revoke-cap → cascading rollback of pending mutations\n\
   spec : Labyrinth of Apocalypse/systems/self_authoring.csl\n";
