// § T11-W8-C1 : cssl-host-sigma-chain — Σ-Chain TIER-2/3 emit + Coherence-Proof basis
// §§ spec : specs/grand-vision/14_SIGMA_CHAIN.csl (POD-2 CRATE-B15)
// §§ thesis : "blockchain but better" via substrate-primitives
// §§ axioms (t∞) :
//     A-1 ¬ PoW · A-2 ¬ PoS · A-3 ¬ public-by-default · A-4 ¬ speculation-tokens
//     A-5 ¬ smart-contract-runtime-bugs · A-6 ¬ gas · A-7 ¬ majority-override-sovereignty
//     A-8 ✓ deterministic-Coherence-Proof · A-9 ✓ Σ-mask-consent-gated · A-10 ✓ player-local-first
//
// §§ scope this crate :
//     - 4 PrivacyTier { LocalOnly · Anonymized · Pseudonymous · Public }
//     - SigmaEvent canonical encoding (BLAKE3-payload + Ed25519-sig)
//     - Ledger : BTreeMap<EventId,SigmaEvent> + merkle-root + per-event merkle-path
//     - CoherenceProof : sig + merkle-path + deterministic-recompute basis
//     - LocalOnly never-egress structural-guard (egress_check)
//     - Sensitive<biometric|gaze|face|body> structurally-stripped @ emit
//
// §§ ATTESTATION (PRIME_DIRECTIVE.md § 11) :
//     There was no hurt nor harm in the making of this, to anyone, anything, or anybody.

#![forbid(unsafe_code)]
//! # cssl-host-sigma-chain
//!
//! Σ-Chain TIER-2/3 emitter & verifier — substrate-native attestation chain.
//!
//! Implements POD-2 CRATE-B15 of `specs/grand-vision/14_SIGMA_CHAIN.csl`.
//!
//! ## Quick-start
//!
//! ```no_run
//! use cssl_host_sigma_chain::{
//!     EventKind, PrivacyTier, SigmaLedger, SigmaPayload, sign_event,
//! };
//! use ed25519_dalek::SigningKey;
//! use rand::rngs::OsRng;
//!
//! let signer = SigningKey::generate(&mut OsRng);
//! let payload = SigmaPayload::new(b"loot-id=42".to_vec());
//! let event = sign_event(
//!     &signer,
//!     EventKind::LootDrop,
//!     1234,
//!     None,
//!     &payload,
//!     PrivacyTier::Pseudonymous,
//! );
//! let mut ledger = SigmaLedger::new();
//! ledger.insert(event).unwrap();
//! let _root = ledger.merkle_root();
//! ```

pub mod event;
pub mod privacy;
pub mod sign;
pub mod merkle;
pub mod ledger;
pub mod verify;

pub use event::{EventId, EventKind, SigmaEvent, SigmaPayload};
pub use ledger::{CoherenceProof, LedgerSnapshot, SigmaLedger};
pub use merkle::{merkle_path_verify, merkle_root_of};
pub use privacy::{egress_check, sanitize_for_egress, PrivacyTier};
pub use sign::{
    canonical_bytes, payload_blake3, sign_event, KeyPairBytes, PUBKEY_LEN, SIG_LEN,
};
pub use verify::{verify_event, VerifyError, VerifyOutcome};

/// Crate-wide tag for transparency/audit-stream identification.
pub const SIGMA_CHAIN_CRATE_TAG: &str = "cssl-host-sigma-chain/0.1.0";
