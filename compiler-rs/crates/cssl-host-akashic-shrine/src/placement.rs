// § placement : ShrineInstance + HomeAnchorRef + per-shrine cap-bit.
// § Construction validates archetype + rune-kit + ambient-FX + cosmetic-only-tags.

use serde::{Deserialize, Serialize};

use crate::archetype::ShrineArchetype;
use crate::ambient_fx::AmbientFx;
use crate::runekit::{RuneKitId, PRESET_RUNE_KITS, resolved_id};
use crate::cosmetic_guard::{assert_cosmetic_only, CosmeticOnlyError};

/// § HomeAnchorRef — opaque pointer into the player Home pocket-dimension
/// scenegraph. The host crate (cssl-host-home-dimension) issues these.
/// We store id + slot-tag only ; placement coordinates are hidden.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct HomeAnchorRef {
    pub home_id: [u8; 16],
    pub slot_tag: u32,
}

/// § ShrineInstance — placed cosmetic shrine in player Home.
/// Construction is fallible : cosmetic-only-axiom enforced + rune-kit must
/// resolve to a known preset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShrineInstance {
    pub archetype: ShrineArchetype,
    pub rune_kit_id: RuneKitId,
    pub ambient_fx: AmbientFx,
    pub placement: HomeAnchorRef,
    /// BLAKE3 over (archetype-tag · rune-kit-id · fx-tag · placement-bytes).
    pub blake3_signature: [u8; 32],
    /// § cap-bit : per-shrine capability. Bit-0 = "may emit attestation".
    /// All other bits MUST be zero (¬ gameplay-power).
    pub cap_bits: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShrineConstructError {
    UnknownRuneKit,
    Cosmetic(CosmeticOnlyError),
    /// Cap-bits encoded gameplay-power.
    NonCosmeticCapBit(u32),
}

impl core::fmt::Display for ShrineConstructError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnknownRuneKit         => write!(f, "rune-kit id does not resolve to a preset"),
            Self::Cosmetic(e)            => write!(f, "{e}"),
            Self::NonCosmeticCapBit(bits) => write!(f, "cap-bits {bits:#x} include non-cosmetic flags"),
        }
    }
}
impl std::error::Error for ShrineConstructError {}

const COSMETIC_CAP_MASK: u32 = 0b1; // only bit-0 allowed

impl ShrineInstance {
    /// § Construct + validate a shrine. `effect_tags` audited as cosmetic-only.
    pub fn new(
        archetype: ShrineArchetype,
        rune_kit_id: RuneKitId,
        ambient_fx: AmbientFx,
        placement: HomeAnchorRef,
        effect_tags: &[&'static str],
        cap_bits: u32,
    ) -> Result<Self, ShrineConstructError> {
        // 1. cosmetic-only audit
        assert_cosmetic_only(effect_tags).map_err(ShrineConstructError::Cosmetic)?;

        // 2. cap-bits must be cosmetic-only subset
        if cap_bits & !COSMETIC_CAP_MASK != 0 {
            return Err(ShrineConstructError::NonCosmeticCapBit(cap_bits));
        }

        // 3. rune-kit must resolve to a known preset
        let known = PRESET_RUNE_KITS.iter().any(|k| resolved_id(k) == rune_kit_id);
        if !known {
            return Err(ShrineConstructError::UnknownRuneKit);
        }

        // 4. compute BLAKE3 signature
        let mut h = blake3::Hasher::new();
        h.update(archetype.tag().as_bytes());
        h.update(&rune_kit_id.0);
        h.update(ambient_fx.tag().as_bytes());
        h.update(&placement.home_id);
        h.update(&placement.slot_tag.to_le_bytes());
        h.update(&cap_bits.to_le_bytes());
        let sig = *h.finalize().as_bytes();

        Ok(Self {
            archetype,
            rune_kit_id,
            ambient_fx,
            placement,
            blake3_signature: sig,
            cap_bits,
        })
    }

    /// True iff bit-0 (attestation) is set.
    pub fn may_emit_attestation(&self) -> bool {
        self.cap_bits & 0b1 != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runekit::RuneKitId;

    fn anchor() -> HomeAnchorRef {
        HomeAnchorRef { home_id: [7u8; 16], slot_tag: 42 }
    }

    fn known_kit_id() -> RuneKitId {
        resolved_id(&PRESET_RUNE_KITS[0])
    }

    #[test]
    fn construct_pillar_with_known_kit_ok() {
        let s = ShrineInstance::new(
            ShrineArchetype::Pillar,
            known_kit_id(),
            AmbientFx::Mist,
            anchor(),
            &["visual.glow", "audio.whisper"],
            0b0,
        ).unwrap();
        assert_eq!(s.archetype, ShrineArchetype::Pillar);
    }

    #[test]
    fn construct_each_archetype() {
        for a in ShrineArchetype::ALL {
            let r = ShrineInstance::new(a, known_kit_id(), AmbientFx::Halo, anchor(), &[], 0);
            assert!(r.is_ok(), "archetype {} failed", a.tag());
        }
    }

    #[test]
    fn unknown_rune_kit_rejected() {
        let bad = RuneKitId([0xFFu8; 16]);
        let err = ShrineInstance::new(ShrineArchetype::Altar, bad, AmbientFx::Aura, anchor(), &[], 0).unwrap_err();
        assert_eq!(err, ShrineConstructError::UnknownRuneKit);
    }

    #[test]
    fn non_cosmetic_tag_rejected() {
        let err = ShrineInstance::new(
            ShrineArchetype::Tree,
            known_kit_id(),
            AmbientFx::Bloom,
            anchor(),
            &["stat.damage"],
            0,
        ).unwrap_err();
        assert!(matches!(err, ShrineConstructError::Cosmetic(_)));
    }

    #[test]
    fn non_cosmetic_cap_bit_rejected() {
        let err = ShrineInstance::new(
            ShrineArchetype::Brazier,
            known_kit_id(),
            AmbientFx::Embers,
            anchor(),
            &[],
            0b10,
        ).unwrap_err();
        assert!(matches!(err, ShrineConstructError::NonCosmeticCapBit(_)));
    }

    #[test]
    fn cap_bit_zero_attestation_off() {
        let s = ShrineInstance::new(ShrineArchetype::Obelisk, known_kit_id(), AmbientFx::Pulse, anchor(), &[], 0).unwrap();
        assert!(!s.may_emit_attestation());
    }

    #[test]
    fn cap_bit_one_attestation_on() {
        let s = ShrineInstance::new(ShrineArchetype::Mandala, known_kit_id(), AmbientFx::Spiral, anchor(), &[], 0b1).unwrap();
        assert!(s.may_emit_attestation());
    }

    #[test]
    fn placement_in_home_round_trip() {
        let a = anchor();
        let s = ShrineInstance::new(ShrineArchetype::Reliquary, known_kit_id(), AmbientFx::Glow, a, &[], 0).unwrap();
        assert_eq!(s.placement.home_id, [7u8; 16]);
        assert_eq!(s.placement.slot_tag, 42);
    }

    #[test]
    fn placement_distinct_anchors_distinct_signatures() {
        let a1 = HomeAnchorRef { home_id: [1u8; 16], slot_tag: 1 };
        let a2 = HomeAnchorRef { home_id: [1u8; 16], slot_tag: 2 };
        let s1 = ShrineInstance::new(ShrineArchetype::Tree, known_kit_id(), AmbientFx::Wind, a1, &[], 0).unwrap();
        let s2 = ShrineInstance::new(ShrineArchetype::Tree, known_kit_id(), AmbientFx::Wind, a2, &[], 0).unwrap();
        assert_ne!(s1.blake3_signature, s2.blake3_signature);
    }

    #[test]
    fn signature_stable_across_constructions() {
        let s1 = ShrineInstance::new(ShrineArchetype::Pillar, known_kit_id(), AmbientFx::Mist, anchor(), &[], 0).unwrap();
        let s2 = ShrineInstance::new(ShrineArchetype::Pillar, known_kit_id(), AmbientFx::Mist, anchor(), &[], 0).unwrap();
        assert_eq!(s1.blake3_signature, s2.blake3_signature);
    }

    #[test]
    fn signature_changes_with_archetype() {
        let s1 = ShrineInstance::new(ShrineArchetype::Pillar, known_kit_id(), AmbientFx::Mist, anchor(), &[], 0).unwrap();
        let s2 = ShrineInstance::new(ShrineArchetype::Altar,  known_kit_id(), AmbientFx::Mist, anchor(), &[], 0).unwrap();
        assert_ne!(s1.blake3_signature, s2.blake3_signature);
    }

    #[test]
    fn serde_round_trip() {
        let s = ShrineInstance::new(ShrineArchetype::Brazier, known_kit_id(), AmbientFx::Embers, anchor(), &[], 0b1).unwrap();
        let j = serde_json::to_string(&s).unwrap();
        let back: ShrineInstance = serde_json::from_str(&j).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn serde_round_trip_all_fx() {
        for fx in AmbientFx::ALL {
            let s = ShrineInstance::new(ShrineArchetype::Custom, known_kit_id(), fx, anchor(), &[], 0).unwrap();
            let j = serde_json::to_string(&s).unwrap();
            let back: ShrineInstance = serde_json::from_str(&j).unwrap();
            assert_eq!(s, back);
        }
    }
}
