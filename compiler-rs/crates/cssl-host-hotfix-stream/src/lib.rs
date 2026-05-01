//! § cssl-host-hotfix-stream — Σ-Chain-fed live-hotfix pipeline.
//! ════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Polls a Σ-Chain feed (TIER-3 multiversal-stream) for hotfix
//!   payloads signed by the Apocky-master-key, verifies cryptography
//!   and policy, stages payloads in-memory, prompts the player when
//!   required, applies via class-specific handlers, and supports
//!   rollback to a prior snapshot.
//!
//!   Per `specs/grand-vision/16_MYCELIAL_NETWORK.csl` § "LIVE HOTFIXES
//!   & IMPROVEMENTS", eight hotfix classes are recognized :
//!
//!   ```text
//!   HF-1 KAN-WEIGHT-UPDATE          (cosmetic-tier · auto-apply)
//!   HF-2 PROCGEN-BIAS-NUDGE         (balance-tier  · prompt + 30s-revert)
//!   HF-3 BALANCE-CONSTANT-ADJUST    (balance-tier  · prompt + 30s-revert)
//!   HF-4 NEW-RECIPE-UNLOCK          (cosmetic-tier · auto-apply)
//!   HF-5 NEMESIS-ARCHETYPE-EVOLVE   (cosmetic-tier · auto-apply)
//!   HF-6 SOVEREIGN-CAP-POLICY-FIX   (security-tier · sovereign-cap-required)
//!   HF-7 NARRATIVE-STORYLET-ADD     (cosmetic-tier · auto-apply)
//!   HF-8 RENDER-PIPELINE-PARAM      (cosmetic-tier · auto-apply)
//!   ```
//!
//! § DESIGN
//!   - All collections are `BTreeMap` for deterministic iteration and
//!     stable serde output.
//!   - `#![forbid(unsafe_code)]` ; no FFI ; no async.
//!   - `SigmaChainPoll` is a local trait : sibling W8-C1 crate
//!     `cssl-host-sigma-chain` is not yet merged. Tests use an
//!     in-memory mock implementing the trait. Production wiring
//!     swaps in the real adapter.
//!   - Audit-emission goes through the `AuditSink` trait. Tests use
//!     `MockAuditSink` ; production wires to `cssl-host-attestation`.
//!   - Time is injected via the `Clock` trait so 30-second
//!     revert-window tests are deterministic.
//!   - The `master_pubkey` is *injected at construction* — never
//!     hardcoded — per the brief landmine.

#![forbid(unsafe_code)]
#![doc(html_root_url = "https://docs.rs/cssl-host-hotfix-stream/0.1.0")]

pub mod apply;
pub mod class;
pub mod policy;
pub mod rollback;
pub mod stage;
pub mod stream;
pub mod verify;

pub use apply::{ApplyHandler, ApplyOutcome, ApplyRegistry, NoopApplyHandler};
pub use class::{Hotfix, HotfixClass, HotfixId, HotfixState, HotfixTier};
pub use policy::{PolicyDecision, SovereignCaps, SOV_HOTFIX_APPLY};
pub use rollback::{RollbackError, RollbackOutcome};
pub use stage::{StageError, StagedHotfix, StagingArea};
pub use stream::{
    AuditSink, Clock, HotfixError, HotfixStream, MockAuditSink, MockClock, MockSigmaChain,
    SigmaChainPoll, SystemClock,
};
pub use verify::{VerifyError, VerifyResult};

/// § ATTESTATION (PRIME_DIRECTIVE.md § 11) — encoded structurally :
/// every hotfix-application path emits an audit event, the apply
/// pipeline refuses to mutate state without verified Ed25519 signature
/// and matching BLAKE3 payload digest, and Security-tier hotfixes
/// require sovereign-cap presence. There was no hurt nor harm in the
/// making of this, to anyone, anything, or anybody.
pub const ATTESTATION: &str =
    "no-harm · consent-by-prompt · sovereign-cap-for-security · audit-emit-every";
