//! § affix — COSMETIC-ONLY affix sum-type
//!
//! [`LootAffix`] is a closed-set enum with **only four cosmetic categories**.
//! There is no `StatBuff` variant ; the absence of the variant is the structural
//! enforcement of the COSMETIC-ONLY-AXIOM.
//!
//! Every variant carries small bounded data (color / sound-id / particle-id /
//! attribution-string) — **no fields that change combat balance**.

use serde::{Deserialize, Serialize};

// ───────────────────────────────────────────────────────────────────────
// § AffixCategory — top-level classification (4 categories per spec)
// ───────────────────────────────────────────────────────────────────────

/// Cosmetic-affix classification. Closed set — adding a stat-affecting category
/// would require a spec-update **and** would fail [`crate::attest_no_pay_for_power`]
/// review (which is keyed off this enum's exhaustive match).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum AffixCategory {
    /// Visual : tracer-color · muzzle-flash · impact-particle · weapon-skin-pattern.
    Visual,
    /// Audio : fire-sound · reload-clink · idle-hum.
    Audio,
    /// Particle : casing-eject · trail · holster-effect.
    Particle,
    /// Attribution : creator-name · season-tag · biome-origin.
    Attribution,
}

impl AffixCategory {
    /// Stable name for audit + Σ-Chain payload.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            AffixCategory::Visual => "visual",
            AffixCategory::Audio => "audio",
            AffixCategory::Particle => "particle",
            AffixCategory::Attribution => "attribution",
        }
    }

    /// All four categories in canonical order. Stable-iteration for tests.
    #[must_use]
    pub const fn all() -> [AffixCategory; 4] {
        [
            AffixCategory::Visual,
            AffixCategory::Audio,
            AffixCategory::Particle,
            AffixCategory::Attribution,
        ]
    }
}

// ───────────────────────────────────────────────────────────────────────
// § VisualAffix
// ───────────────────────────────────────────────────────────────────────

/// Visual cosmetic — color / pattern / particle-id. **Never** affects damage,
/// hit-detection, or projectile-physics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum VisualAffix {
    /// 24-bit packed RGB tracer-color (e.g. `0x00_FF_00` for green).
    TracerColor(u32),
    /// Muzzle-flash variant id (lookup-table only ; balance-neutral).
    MuzzleFlash(u16),
    /// Impact-particle variant id.
    ImpactParticle(u16),
    /// Weapon-skin pattern id.
    SkinPattern(u16),
}

// ───────────────────────────────────────────────────────────────────────
// § AudioAffix
// ───────────────────────────────────────────────────────────────────────

/// Audio cosmetic — sound-id / ambient-loop. Balance-neutral.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AudioAffix {
    /// Fire-sound variant id.
    FireSound(u16),
    /// Reload-clink id.
    ReloadClink(u16),
    /// Idle-hum loop id.
    IdleHum(u16),
}

// ───────────────────────────────────────────────────────────────────────
// § ParticleAffix
// ───────────────────────────────────────────────────────────────────────

/// Particle cosmetic — emitter-id only. Balance-neutral.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ParticleAffix {
    /// Casing-eject style id.
    CasingEject(u16),
    /// Bullet-trail emitter id.
    Trail(u16),
    /// Holster-effect emitter id.
    HolsterEffect(u16),
}

// ───────────────────────────────────────────────────────────────────────
// § AttributionAffix
// ───────────────────────────────────────────────────────────────────────

/// Attribution cosmetic — creator-credit string + season-tag + biome-origin.
/// Balance-neutral. Used by Akashic-Records to reconstruct provenance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AttributionAffix {
    /// Creator-name (intentionally bounded ≤ 64 bytes ; truncated if longer).
    CreatorName(String),
    /// Season-tag (e.g. "S0" ... "S99").
    SeasonTag(u8),
    /// Biome-origin id (matches `cssl_host_gear_archetype::Biome` discriminant).
    BiomeOrigin(u8),
}

// ───────────────────────────────────────────────────────────────────────
// § LootAffix — top-level sum
// ───────────────────────────────────────────────────────────────────────

/// Cosmetic affix — sum-type over the four categories.
///
/// **There is no `StatBuff` variant.** The COSMETIC-ONLY-AXIOM is enforced by
/// the absence of stat-modifying variants in this enum. [`crate::attest_no_pay_for_power`]
/// performs an exhaustive match and returns `true` because every variant lives
/// in the cosmetic categories.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LootAffix {
    /// Visual cosmetic — color / pattern / particle-id.
    Visual(VisualAffix),
    /// Audio cosmetic — sound-id / loop-id.
    Audio(AudioAffix),
    /// Particle cosmetic — emitter-id.
    Particle(ParticleAffix),
    /// Attribution cosmetic — creator / season / biome-origin.
    Attribution(AttributionAffix),
}

impl LootAffix {
    /// Returns the category of this affix.
    #[must_use]
    pub fn category(&self) -> AffixCategory {
        match self {
            LootAffix::Visual(_) => AffixCategory::Visual,
            LootAffix::Audio(_) => AffixCategory::Audio,
            LootAffix::Particle(_) => AffixCategory::Particle,
            LootAffix::Attribution(_) => AffixCategory::Attribution,
        }
    }

    /// Canonical bytes for Σ-Chain payload — stable across runs.
    /// Length-prefixed category-tag + serde-json of the variant data.
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(64);
        let tag = self.category().name().as_bytes();
        let len = u32::try_from(tag.len()).unwrap_or(u32::MAX);
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(tag);
        // Serde-json is canonical-enough for our purposes (BTreeMap-sorted).
        let payload = serde_json::to_vec(self).unwrap_or_default();
        let plen = u32::try_from(payload.len()).unwrap_or(u32::MAX);
        out.extend_from_slice(&plen.to_le_bytes());
        out.extend_from_slice(&payload);
        out
    }
}
