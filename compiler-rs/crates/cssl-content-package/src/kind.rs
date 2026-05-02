//! § kind — the 8 content-kinds carried by a `.ccpkg`.
//!
//! § DESIGN
//!   `repr(u8)` gives a stable wire-byte for the bundle header. Variants
//!   renumbered → BUNDLE FORMAT BREAKING CHANGE — do NOT renumber casually.
//!   The 8th variant (`Bundle`) is a composite-of-N : its TARLITE archive
//!   contains nested `.ccpkg` files, each kind-validated recursively.

use serde::{Deserialize, Serialize};

/// § The 8 content-kinds a `.ccpkg` can carry.
#[derive(
    Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
#[serde(rename_all = "kebab-case")]
#[repr(u8)]
pub enum ContentKind {
    /// (1) `.cssl` scene source + optional GLTF/asset payloads.
    Scene = 1,
    /// (2) NPC schema · AI-behavior tree · stat-block.
    Npc = 2,
    /// (3) Crafting recipe · alchemy formula · drop-table fragment.
    Recipe = 3,
    /// (4) Lore-text · NPC dialogue · localization-bundle.
    Lore = 4,
    /// (5) System rules · gameplay-mechanic mod (e.g. weather · physics-tweak).
    System = 5,
    /// (6) Render-shader-pack : WGSL/HLSL/MSL bytecode + parameter bindings.
    ShaderPack = 6,
    /// (7) Audio-pack : SFX · music stems · spatial-audio tags.
    AudioPack = 7,
    /// (8) Composite : payload contains N nested `.ccpkg` files of any kind.
    Bundle = 8,
}

/// All 8 content-kinds in stable order.
pub const CONTENT_KINDS: [ContentKind; 8] = [
    ContentKind::Scene,
    ContentKind::Npc,
    ContentKind::Recipe,
    ContentKind::Lore,
    ContentKind::System,
    ContentKind::ShaderPack,
    ContentKind::AudioPack,
    ContentKind::Bundle,
];

impl ContentKind {
    /// Stable `kebab-case` name for serde / discovery / UI.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Scene => "scene",
            Self::Npc => "npc",
            Self::Recipe => "recipe",
            Self::Lore => "lore",
            Self::System => "system",
            Self::ShaderPack => "shader-pack",
            Self::AudioPack => "audio-pack",
            Self::Bundle => "bundle",
        }
    }

    /// Parse from `kebab-case` name.
    #[must_use]
    pub fn from_name(s: &str) -> Option<Self> {
        match s {
            "scene" => Some(Self::Scene),
            "npc" => Some(Self::Npc),
            "recipe" => Some(Self::Recipe),
            "lore" => Some(Self::Lore),
            "system" => Some(Self::System),
            "shader-pack" => Some(Self::ShaderPack),
            "audio-pack" => Some(Self::AudioPack),
            "bundle" => Some(Self::Bundle),
            _ => None,
        }
    }

    /// Is the kind a composite (recursive validation required) ?
    #[must_use]
    pub const fn is_composite(self) -> bool {
        matches!(self, Self::Bundle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_const_is_eight_distinct() {
        assert_eq!(CONTENT_KINDS.len(), 8);
        for (i, k) in CONTENT_KINDS.iter().enumerate() {
            assert_eq!(*k as u8, (i as u8) + 1);
        }
    }

    #[test]
    fn name_roundtrip() {
        for k in CONTENT_KINDS {
            assert_eq!(ContentKind::from_name(k.name()), Some(k));
        }
    }

    #[test]
    fn from_name_unknown_is_none() {
        assert!(ContentKind::from_name("not-a-kind").is_none());
        assert!(ContentKind::from_name("").is_none());
    }

    #[test]
    fn only_bundle_is_composite() {
        for k in CONTENT_KINDS {
            assert_eq!(k.is_composite(), k == ContentKind::Bundle);
        }
    }

    #[test]
    fn kind_serde_roundtrip() {
        for k in CONTENT_KINDS {
            let s = serde_json::to_string(&k).unwrap();
            let back: ContentKind = serde_json::from_str(&s).unwrap();
            assert_eq!(k, back);
        }
    }
}
