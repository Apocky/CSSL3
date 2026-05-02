// § T11-W11-SIGMA-CHAIN : cssl-substrate-sigma-chain — bootstrap distributed-ledger primitive
// §§ spec : specs/28_SIGMA_CHAIN_BOOTSTRAP.csl
// §§ thesis : "blockchain but better" via substrate-primitives ; substrate-tier ledger ;
//     append-only · BLAKE3-Merkle-rooted · Ed25519-signed · monotonic-gapless seq_no ;
//     Coherence-Proof = deterministic-replay-from-genesis-or-checkpoint ;
//     NO PoW · NO PoS · NO gas · sovereign-rollback-via-checkpoint-rewind.
//
// §§ disjoint-from cssl-host-sigma-chain :
//     - host-sigma-chain : event-emitter (privacy-tiers · per-event sign · BTreeMap ledger)
//     - this crate       : substrate-tier append-only-log (incremental Merkle · seq_no ·
//                          checkpoints · concurrent-append · federation skeleton)
//     siblings can layer one on top of the other ; this crate is the lower-level primitive.
//
// §§ siblings that will anchor through this in next-wave :
//     - cssl-hotfix-stream            : record_hotfix_bundle()
//     - cssl-substrate-prime-directive: record_cap_grant() / record_cap_revoke()
//     - cssl-mycelium-chat-sync       : record_mycelium_pattern()
//     - cssl-substrate-knowledge      : record_knowledge_ingest()
//     - cssl-substrate-omega-field    : record_cell_emission()
//
// §§ ATTESTATION (PRIME_DIRECTIVE.md § 11) :
//     There was no hurt nor harm in the making of this, to anyone, anything, or anybody.

#![forbid(unsafe_code)]
//! # cssl-substrate-sigma-chain
//!
//! Bootstrap distributed-ledger primitive for the Σ-Chain.
//!
//! Implements `specs/28_SIGMA_CHAIN_BOOTSTRAP.csl` per `specs/grand-vision/14_SIGMA_CHAIN.csl`
//! and `specs/grand-vision/15_UNIFIED_SUBSTRATE.csl`.
//!
//! ## Quick start
//!
//! ```
//! use cssl_substrate_sigma_chain::{SigmaChain, record_cap_grant};
//! use ed25519_dalek::SigningKey;
//! use rand::rngs::OsRng;
//!
//! let chain = SigmaChain::new();
//! let signer = SigningKey::generate(&mut OsRng);
//! let result = record_cap_grant(&chain, &signer, b"cap=read:fs:/home/user", 1_700_000_000)
//!     .expect("record");
//! assert_eq!(result.seq_no, 1);
//! assert!(result.root != [0u8; 32]);
//! ```
//!
//! ## Axioms (t∞)
//!
//! - **A-1** ¬ Proof-of-Work
//! - **A-2** ¬ Proof-of-Stake
//! - **A-3** ¬ gas / transaction-fees
//! - **A-4** ¬ majority-override-sovereignty (sovereign-rollback always available)
//! - **A-5** ✓ append-only log (monotonic gapless seq_no)
//! - **A-6** ✓ Ed25519-signed entries (forge-resistant)
//! - **A-7** ✓ BLAKE3-Merkle-rooted (tamper-evident)
//! - **A-8** ✓ Coherence-Proof = deterministic replay from genesis OR checkpoint
//! - **A-9** ✓ stage-0 single-node-mode default ; federated-mode skeleton-only

pub mod attest;
pub mod chain;
pub mod coherence;
pub mod entry;
pub mod federation;
pub mod merkle;

pub use attest::{
    hash_payload, record_attestation, record_cap_grant, record_cap_revoke, record_cell_emission,
    record_hotfix_bundle, record_knowledge_ingest, record_mycelium_pattern, Attestation,
};
pub use chain::{
    AppendResult, ChainError, ChainTail, Checkpoint, SigmaChain, CHECKPOINT_INTERVAL,
};
pub use coherence::{
    checkpoint_is_self_consistent, count_checkpoint_marks, verify_coherence_from_checkpoint,
    verify_coherence_from_genesis, CoherenceOutcome, IncoherenceReason,
};
pub use entry::{EntryKind, LedgerEntry, ENTRY_DOMAIN, ENTRY_WIRE_SIZE};
pub use federation::{
    CheckpointPublication, Federation, Mode as FederationMode, PeerId,
};
pub use merkle::{
    hash_leaf, hash_node, verify_inclusion, IncrementalMerkle, InclusionProof, SiblingSide,
    ZERO_HASH,
};

/// Crate-wide tag for transparency/audit-stream identification.
pub const SIGMA_CHAIN_BOOTSTRAP_CRATE_TAG: &str = "cssl-substrate-sigma-chain/0.1.0";
