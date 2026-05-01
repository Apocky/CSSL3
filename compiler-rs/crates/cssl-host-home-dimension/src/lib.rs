// § cssl-host-home-dimension — per-player Home pocket-dimension state-machine.
// I> spec : ../../../specs/grand-vision/16_MYCELIAL_NETWORK.csl § HOME-DIMENSION
// I> 7 archetypes × 5 access-modes × 8 cap-bits × 7 subsystems
// I> sovereign : player-can-revoke-any-cap-instantly · all-cap-changes audit-emit
// I> deterministic : BTreeMap (¬ HashMap) for canonical iteration & serde-stability
// I> Sensitive<*> structurally-banned : NO biometric/gaze/face/body fields ever
//
// modules :
//   archetype    : ArchetypeId (7 variants) + AccessMode (5 variants)
//   caps         : HomeCapBits + 8 HM_CAP_* flags + bitwise ops
//   ids          : HomeId(u64) + Pubkey([u8;32]) + Timestamp(u64) newtypes
//   decoration   : DecorationSlot + place / remove / list
//   trophy       : Trophy + pin / unpin / list
//   companion    : Companion + add / dismiss / converse-stub
//   portal       : Portal + register / disable / list (cap-gated)
//   forge        : ForgeRecipe + queue / cancel
//   terminal     : MycelialTerminal opt-in-flag (stub for cssl-host-mycelium FFI)
//   memorial     : MemorialEntry + post / list
//   home         : HomeState + Home::new / set_mode / accept_visitor / decorate
//   audit        : AuditEvent + recorded transitions for cssl-host-attestation FFI
//   asset_ref    : AssetRef trait (¬ block on cssl-host-asset-bundle availability)

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![doc = "Per-player Home pocket-dimension : 7 archetypes × 5 access-modes × cap-gated subsystems."]

pub mod archetype;
/// Asset-reference trait (decouples from cssl-host-asset-bundle for fail-safe build).
pub mod asset_ref;
/// Audit-event log : every cap-mutation + visitor-action is recorded for attestation FFI.
pub mod audit;
/// Cap-bit flags : 8 HM_CAP_* permissions gating subsystem mutations.
pub mod caps;
/// Companion : add / dismiss / converse-stub for befriended NPCs.
pub mod companion;
/// Decoration : place / remove / list of asset-refs at slot-keyed transforms.
pub mod decoration;
/// Forge-node : crafting-queue stub (cssl-host-craft-graph FFI hook).
pub mod forge;
/// Home : top-level pocket-dimension state-machine.
pub mod home;
/// Newtype wrappers : HomeId / Pubkey / Timestamp.
pub mod ids;
/// Memorial-wall : visitor-spore-ascriptions on public homes.
pub mod memorial;
/// Navigation portal : cap-gated doors to Multiverse / Bazaar / Friends' Homes.
pub mod portal;
/// Mycelial-terminal : opt-in flag + stub for cssl-host-mycelium aggregate-view.
pub mod terminal;
/// Trophy-case : pin / unpin / list of Ascended-items + Coherence-Score shrines.
pub mod trophy;

pub use archetype::{AccessMode, ArchetypeId};
pub use asset_ref::{AssetRef, OpaqueAsset};
pub use audit::{AuditEvent, AuditKind, AuditLog};
pub use caps::{
    HomeCapBits, HM_CAP_DECORATE, HM_CAP_FORGE_USE, HM_CAP_HOTFIX_RECEIVE, HM_CAP_INVITE,
    HM_CAP_MEMORIAL_PIN, HM_CAP_MYCELIAL_EMIT, HM_CAP_NPC_HIRE, HM_CAP_PUBLISH,
};
pub use companion::{Companion, CompanionDisposition};
pub use decoration::{DecorationSlot, SlotTransform};
pub use forge::{ForgeQueueItem, ForgeRecipeId};
pub use home::{Home, HomeError, HomeState};
pub use ids::{HomeId, Pubkey, Timestamp};
pub use memorial::{MemorialAscription, MemorialEntry};
pub use portal::{Portal, PortalDest};
pub use terminal::MycelialTerminal;
pub use trophy::{Trophy, TrophyKind};

/// Schema version of the on-disk Home serialization.
///
/// Bumped only on non-additive layout changes ; additive fields use `serde(default)`.
pub const HOME_SCHEMA_VERSION: u32 = 1;

#[cfg(test)]
mod tests;
