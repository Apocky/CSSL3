// § ambient_fx : exact 16 cosmetic ambient-FX variants.

use serde::{Deserialize, Serialize};

/// § AmbientFx — exactly 16 cosmetic ambient-FX. Variants are visual/audio
/// only (per cosmetic-channel-only-axiom).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum AmbientFx {
    Mist,
    Embers,
    MotesGold,
    MotesSilver,
    Wind,
    WhisperLoop,
    Halo,
    Aura,
    Pulse,
    Shimmer,
    Glow,
    Cascade,
    Spiral,
    Bloom,
    DustfallLight,
    DustfallDark,
}

impl AmbientFx {
    /// Stable declaration-order list. MUST contain exactly 16 elements
    /// per spec § T11-W8-C4.
    pub const ALL: [AmbientFx; 16] = [
        AmbientFx::Mist,
        AmbientFx::Embers,
        AmbientFx::MotesGold,
        AmbientFx::MotesSilver,
        AmbientFx::Wind,
        AmbientFx::WhisperLoop,
        AmbientFx::Halo,
        AmbientFx::Aura,
        AmbientFx::Pulse,
        AmbientFx::Shimmer,
        AmbientFx::Glow,
        AmbientFx::Cascade,
        AmbientFx::Spiral,
        AmbientFx::Bloom,
        AmbientFx::DustfallLight,
        AmbientFx::DustfallDark,
    ];

    pub fn tag(self) -> &'static str {
        match self {
            AmbientFx::Mist           => "mist",
            AmbientFx::Embers         => "embers",
            AmbientFx::MotesGold      => "motes_gold",
            AmbientFx::MotesSilver    => "motes_silver",
            AmbientFx::Wind           => "wind",
            AmbientFx::WhisperLoop    => "whisper_loop",
            AmbientFx::Halo           => "halo",
            AmbientFx::Aura           => "aura",
            AmbientFx::Pulse          => "pulse",
            AmbientFx::Shimmer        => "shimmer",
            AmbientFx::Glow           => "glow",
            AmbientFx::Cascade        => "cascade",
            AmbientFx::Spiral         => "spiral",
            AmbientFx::Bloom          => "bloom",
            AmbientFx::DustfallLight  => "dustfall_light",
            AmbientFx::DustfallDark   => "dustfall_dark",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exactly_sixteen_fx() {
        assert_eq!(AmbientFx::ALL.len(), 16, "spec mandates exactly 16 ambient-FX");
    }

    #[test]
    fn all_distinct_tags() {
        let mut seen = std::collections::BTreeSet::new();
        for f in AmbientFx::ALL {
            assert!(seen.insert(f.tag()), "duplicate fx tag {}", f.tag());
        }
        assert_eq!(seen.len(), 16);
    }

    #[test]
    fn round_trip_all_fx() {
        for f in AmbientFx::ALL {
            let s = serde_json::to_string(&f).unwrap();
            let back: AmbientFx = serde_json::from_str(&s).unwrap();
            assert_eq!(f, back);
        }
    }

    #[test]
    fn tags_are_lowercase_snake_case() {
        for f in AmbientFx::ALL {
            assert!(
                f.tag().chars().all(|c| c.is_ascii_lowercase() || c == '_'),
                "fx tag {} not snake_case",
                f.tag()
            );
        }
    }
}
