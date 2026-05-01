// § T11-W8-C4 : cssl-host-akashic-shrine
// ─────────────────────────────────────────────────────────────────────────────
// § I> Cosmetic-only Akashic shrine archetypes + rune-kits + ambient-FX.
// § I> ¬ pay-for-power · cosmetic-channel-only-axiom enforced structurally
// § I> at construction (§ cosmetic_guard module).
// § I> sibling W8-C3 (cssl-host-akashic-records) ¬ merged → MOCK via traits
// § I> ImprintLike + ShardLike. attestation-emission also mocked locally.
// § I> Shrines are placed in player Home pocket-dimension (specs/grand-vision/16).
//
// § ATTESTATION (PRIME_DIRECTIVE.md § 11)
// There was no hurt nor harm in the making of this, to anyone, anything,
// or anybody.
// ─────────────────────────────────────────────────────────────────────────────

#![forbid(unsafe_code)]
#![doc = "cssl-host-akashic-shrine — cosmetic-only player-home shrines."]

pub mod archetype;
pub mod runekit;
pub mod ambient_fx;
pub mod placement;
pub mod cosmetic_guard;

pub use archetype::ShrineArchetype;
pub use runekit::{GlyphId, RuneKit, RuneKitId, ColorPalette, PRESET_RUNE_KITS};
pub use ambient_fx::AmbientFx;
pub use placement::{HomeAnchorRef, ShrineInstance};
pub use cosmetic_guard::{CosmeticOnlyError, assert_cosmetic_only};
