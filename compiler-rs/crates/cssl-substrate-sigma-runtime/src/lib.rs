//! § cssl-substrate-sigma-runtime — the canonical runtime Σ-mask evaluator.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   This crate is the runtime gate-fn that every-other-crate routes
//!   READ / WRITE / EMIT through. It is the load-bearing surface of the
//!   sovereignty-model :
//!
//! ```text
//!     caller-crate ──evaluate(mask, cap, audience, effect, k)──→ AccessDecision
//!                                       │
//!                                       └→ audit-ring-buffer (zero-alloc)
//! ```
//!
//!   Where the existing `cssl-substrate-prime-directive::sigma::SigmaMaskPacked`
//!   is the 16-byte STD430-aligned PER-CELL representation embedded in the
//!   72-byte Ω-FieldCell, THIS crate provides the 19-byte RUNTIME mask used
//!   by host-side aggregator / hotfix / akashic / chat-sync crates that gate
//!   on AUDIENCE-CLASS + EFFECT-CAP + K-ANONYMITY + TTL semantics rather than
//!   per-cell op-class consent. The two are complementary :
//!
//! ```text
//!     - SigmaMaskPacked (cell-level)  — 16 B std430 — per-cell consent +
//!       sovereignty-handle + reversibility-scope + agency-state.
//!     - SigmaMask       (runtime)     — 19 B packed — audience-class +
//!       effect-caps + k-anon-floor + TTL + revocation + checksum.
//! ```
//!
//! § SPEC
//!   - `specs/27_SIGMA_MASK_RUNTIME.csl`            (canonical for this crate).
//!   - `specs/grand-vision/15_UNIFIED_SUBSTRATE.csl` § Σ-Chain primitives.
//!   - `specs/grand-vision/14_SIGMA_CHAIN.csl`       § Coherence-Proof consensus.
//!   - `PRIME_DIRECTIVE.md`                          § 0 consent = OS + § 5
//!     revocability + § 7 INTEGRITY (audit append-only).
//!
//! § PRIME-DIRECTIVE alignment
//!   - **§ 0 consent = OS** : every [`evaluate`] call consults a Σ-mask. No
//!     code path bypasses the gate ; there is no "trusted-caller" exception.
//!   - **§ 5 revocability** : [`SigmaMask::revoke`] sets `revoked_at` and the
//!     evaluator returns [`AccessDecision::Revoked`] thereafter — past data
//!     emissions stand recorded in the audit-ring, future ones are denied.
//!   - **§ 7 INTEGRITY** : every grant / revoke / evaluate emits an entry into
//!     the [`audit::AuditRing`] which is APPEND-ONLY ; truncation = panic.
//!   - **structurally-encoded** : `SigmaMask` embeds a BLAKE3-128 truncated
//!     checksum. Tampering in-memory ⇒ checksum-mismatch ⇒ `DenyTampered`.
//!
//! § CONCURRENCY
//!   The hot-path [`evaluate`] is lock-free + zero-allocation. The audit-ring
//!   uses a `parking_lot::Mutex` (via std::sync::Mutex here to avoid a workspace
//!   dep on parking_lot for this crate) wrapping a fixed-size pre-allocated
//!   ring of 8192 entries. Drain is also zero-allocation : it copies into a
//!   caller-provided slice.
//!
//! § ATTESTATION
//!   See [`ATTESTATION`] — recorded verbatim per `PRIME_DIRECTIVE § 11`.

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
// Per-crate clippy noise suppression matches workspace baseline + per-crate
// site-specific allowances : ring-buffer index-loops + format-args inlining +
// items-after-statements (test-fixture style).
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::needless_range_loop)]
#![allow(clippy::uninlined_format_args)]
#![allow(clippy::items_after_statements)]
#![allow(clippy::map_unwrap_or)]
#![allow(clippy::similar_names)]
#![allow(clippy::cast_possible_truncation)]

pub mod audit;
pub mod cap;
pub mod evaluator;
pub mod mask;
pub mod propagation;

// ── canonical re-exports ────────────────────────────────────────────────────
pub use audit::{AuditEntry, AuditRing, DecisionTag, AUDIT_RING_DEFAULT_CAPACITY};
pub use cap::{CapError, SovereignCap};
pub use evaluator::{evaluate, evaluate_with_ring, now_seconds, AccessDecision, DenyReason};
pub use mask::{
    AudienceBit, EffectCap, MaskFlag, SigmaMask, AUDIENCE_ADMIN, AUDIENCE_CIRCLE, AUDIENCE_DERIVED,
    AUDIENCE_PUBLIC, AUDIENCE_SELF, AUDIENCE_SYSTEM, EFFECT_BROADCAST, EFFECT_DERIVE, EFFECT_LOG,
    EFFECT_PURGE, EFFECT_READ, EFFECT_WRITE, FLAG_ATTESTED, FLAG_INHERIT, FLAG_OVERRIDE,
    FLAG_PROPAGATE, MASK_PACKED_BYTES,
};
pub use propagation::{compose_parent_child, CompositionError};

// ───────────────────────────────────────────────────────────────────────────
// § ATTESTATION (verbatim per PRIME_DIRECTIVE § 11)
// ───────────────────────────────────────────────────────────────────────────

/// Canonical attestation recorded into the audit-ring on first
/// [`AuditRing::new`] construction.
///
/// § PRIME_DIRECTIVE § 11 : every substrate-primitive crate ships an
/// attestation-string declaring its canonical alignment + spec citation.
pub const ATTESTATION: &str = "\
§ cssl-substrate-sigma-runtime ‼ ATTESTATION (PRIME_DIRECTIVE § 11)\n\
   t∞: every-evaluate consults Σ-mask · ¬ trusted-caller-bypass\n\
   t∞: every-grant + every-revoke + every-evaluate ⇒ audit-ring-emit\n\
   t∞: BLAKE3-128 checksum tamper-detect · Ed25519 cap-signature verify\n\
   t∞: revocation = sovereign-revocable · TTL = sovereign-time-bound\n\
   t∞: composition AND-narrows audience + effects (parent ⊇ child)\n\
   t∞: k-anonymity = aggregation-floor · per-record evaluate refused\n\
   spec : specs/27_SIGMA_MASK_RUNTIME.csl\n\
   ¬-conflate : SigmaMask (runtime 19 B) ≠ SigmaMaskPacked (cell 16 B)\n";
