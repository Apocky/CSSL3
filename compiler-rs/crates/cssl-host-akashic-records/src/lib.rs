// § T11-W8-C3 : cssl-host-akashic-records · imprint state-machine · 5-tier fidelity
// § cosmetic-channel-only-axiom (¬ pay-for-power) · spec/grand-vision/18_AKASHIC_RECORDS.csl
// § Aetheric-Shards balance · BLAKE3 content-hash · ETERNAL one-time-permanent
// § PRIME-DIRECTIVE : ✓ consent-as-OS · ✓ sovereignty · ✓ free-tier-always · ¬ exploitation
//
// § module-tree :
//   imprint     ← Imprint + ImprintId + ImprintState
//   fidelity    ← FidelityTier + cost-config + 16-band-flag
//   attribution ← author-pubkey permanence + AttributionLedger
//   shards      ← AethericShards balance + checked-arithmetic + audit-emit
//   purchase    ← PurchaseOutcome + AkashicLedger purchase-flow
//   browse      ← BrowseQuery + browse-by-{scene,author,fidelity-min}
//
// § G1-integration-hooks (sibling-crates · W8 fanout) :
//   - cssl-host-asset-bundle (B22 LAB-v1) : consume HighFidelity scene-snapshot
//     + audio-loop + BLAKE3-hash · stub-flag-only here · sibling owns binary-blob
//   - cssl-host-attestation : audit_trail() consumer · sibling emits W6+ feed
//   - cssl-host-historical-reconstructor (B24) : calls our TtlToken issuance
//     when player initiates 30-min-tour · respects HistoricalReconstructionTour
//     fidelity tier · TTL-tracking lives here, deterministic-recompute in B24
//   - cssl-host-akashic-shrine (B23) : queries our `browse()` for shrine-display
//     + uses Imprint cosmetic-only fields for shrine-state · gameplay-isolated
//   - cssl-edge `/api/akashic/*` (W8-D1) : Stripe-purchase → ledger.credit() →
//     ledger.imprint() · NEVER calls Stripe directly here per landmine
//   - Σ-Chain (spec/14) : every imprint maps-to a TIER-3 multiversal-stream
//     event · `kind="akashic.imprint"` · BLAKE3-merkle-root @ tick
//
// § hard-caps :
//   - #![forbid(unsafe_code)] · BTreeMap (¬ HashMap)
//   - cosmetic-channel-only structural-guard
//   - eternal-attribution NEVER-revoked
//
// § ATTESTATION (PRIME_DIRECTIVE.md § 11) :
//   There was no hurt nor harm in the making of this, to anyone, anything,
//   or anybody.

#![forbid(unsafe_code)]
#![deny(rust_2018_idioms)]

pub mod attribution;
pub mod browse;
pub mod fidelity;
pub mod imprint;
pub mod purchase;
pub mod shards;

pub use attribution::{AttributionLedger, AuthorPubkey};
pub use browse::{BrowseQuery, BrowseResult};
pub use fidelity::{FidelityTier, ShardCostConfig};
pub use imprint::{
    AkashicError, Imprint, ImprintId, ImprintState, RevokedReason, SceneMeta, TtlToken,
};
pub use purchase::{AkashicLedger, AuditEvent, PurchaseOutcome, PurchaseRequest};
pub use shards::AethericShards;

/// Cosmetic-channel-only axiom · structural-guard (per spec/13 + spec/18 A-4).
///
/// Returns `Ok(())` iff `imprint` carries no gameplay-affecting fields.
/// All `SceneMeta` fields are cosmetic by-construction (compile-enforced); this
/// runtime check additionally validates lengths + characters to defend against
/// mis-decoded externally-crafted payloads.
///
/// # Errors
/// Returns [`AkashicError::CosmeticAxiomViolation`] if any field smuggles
/// gameplay state (e.g. an excessively-long opaque blob, or non-printable bytes
/// that could carry a serialized stat-block).
pub fn assert_cosmetic_only(imprint: &Imprint) -> Result<(), AkashicError> {
    imprint.assert_cosmetic_only()
}
