// § runekit : 30+ rune-kit presets via const-array (per landmine guidance).
// § cosmetic-only · NO stat fields · color + glyph + particle-density only.

use serde::{Deserialize, Serialize};

/// § GlyphId — opaque cosmetic glyph ID (renders to atlas slice).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GlyphId(pub u32);

/// § ColorPalette — 4-stop ARGB palette. Pure cosmetic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ColorPalette(pub [u32; 4]);

/// § RuneKitId — opaque preset selector (BLAKE3-derived).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RuneKitId(pub [u8; 16]);

impl RuneKitId {
    /// Derive a deterministic ID from preset-name (cosmetic).
    pub fn from_name(name: &str) -> Self {
        let h = blake3::hash(name.as_bytes());
        let bytes = h.as_bytes();
        let mut id = [0u8; 16];
        id.copy_from_slice(&bytes[..16]);
        RuneKitId(id)
    }
}

/// § RuneKit — bundle of cosmetic glyphs + palette + particle-density.
/// `particle_density` is u16 (0-65535) clamped at construction.
///
/// NOTE : `RuneKit` itself does NOT derive `Serialize`/`Deserialize` —
/// presets are `const` `&'static [GlyphId]` references (ZST-friendly).
/// Persisted artifacts use `RuneKitId` (the BLAKE3-derived stable ID).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuneKit {
    pub id: RuneKitId,
    pub name: &'static str,
    pub glyphs: &'static [GlyphId],
    pub color_palette: ColorPalette,
    pub particle_density: u16,
}

// ─────────────────────────────────────────────────────────────────────────────
// § Preset glyph slices.
// ─────────────────────────────────────────────────────────────────────────────

const G_FLAME:  &[GlyphId] = &[GlyphId(0x10), GlyphId(0x11), GlyphId(0x12)];
const G_LEAF:   &[GlyphId] = &[GlyphId(0x20), GlyphId(0x21)];
const G_STAR:   &[GlyphId] = &[GlyphId(0x30), GlyphId(0x31), GlyphId(0x32), GlyphId(0x33)];
const G_TIDE:   &[GlyphId] = &[GlyphId(0x40), GlyphId(0x41)];
const G_STONE:  &[GlyphId] = &[GlyphId(0x50)];
const G_RUNE:   &[GlyphId] = &[GlyphId(0x60), GlyphId(0x61), GlyphId(0x62)];
const G_LOOM:   &[GlyphId] = &[GlyphId(0x70), GlyphId(0x71)];
const G_SHARD:  &[GlyphId] = &[GlyphId(0x80), GlyphId(0x81)];
const G_VEIN:   &[GlyphId] = &[GlyphId(0x90), GlyphId(0x91), GlyphId(0x92)];
const G_HALO:   &[GlyphId] = &[GlyphId(0xA0)];

// ─────────────────────────────────────────────────────────────────────────────
// § PRESET_RUNE_KITS : 32 cosmetic presets (≥ 30 required by spec).
// IDs are zero-initialised at compile-time (¬ const-fn-friendly hash).
// Use `RuneKit::ensure_id` lookup at runtime for hashed-id, or call
// `RuneKitId::from_name` ad-hoc.
// ─────────────────────────────────────────────────────────────────────────────

const fn pal(a: u32, b: u32, c: u32, d: u32) -> ColorPalette {
    ColorPalette([a, b, c, d])
}
const ZERO_ID: RuneKitId = RuneKitId([0u8; 16]);

pub const PRESET_RUNE_KITS: &[RuneKit] = &[
    RuneKit { id: ZERO_ID, name: "flame_amber",   glyphs: G_FLAME, color_palette: pal(0xFFFF8800, 0xFFFFC833, 0xFF885500, 0xFFFFFFFF), particle_density: 4096 },
    RuneKit { id: ZERO_ID, name: "flame_azure",   glyphs: G_FLAME, color_palette: pal(0xFF1144FF, 0xFF77AAFF, 0xFF002288, 0xFFFFFFFF), particle_density: 4096 },
    RuneKit { id: ZERO_ID, name: "flame_violet",  glyphs: G_FLAME, color_palette: pal(0xFF8822FF, 0xFFCC88FF, 0xFF441188, 0xFFFFFFFF), particle_density: 4096 },
    RuneKit { id: ZERO_ID, name: "leaf_spring",   glyphs: G_LEAF,  color_palette: pal(0xFF44CC44, 0xFFAAFF88, 0xFF226622, 0xFFFFFFFF), particle_density: 1024 },
    RuneKit { id: ZERO_ID, name: "leaf_autumn",   glyphs: G_LEAF,  color_palette: pal(0xFFCC8822, 0xFFFFAA44, 0xFF663311, 0xFFFFFFFF), particle_density: 1024 },
    RuneKit { id: ZERO_ID, name: "star_silver",   glyphs: G_STAR,  color_palette: pal(0xFFCCCCCC, 0xFFFFFFFF, 0xFF888888, 0xFF000000), particle_density: 8192 },
    RuneKit { id: ZERO_ID, name: "star_gold",     glyphs: G_STAR,  color_palette: pal(0xFFFFCC22, 0xFFFFEE88, 0xFF885500, 0xFF000000), particle_density: 8192 },
    RuneKit { id: ZERO_ID, name: "star_void",     glyphs: G_STAR,  color_palette: pal(0xFF221144, 0xFF442288, 0xFF110022, 0xFF000000), particle_density: 8192 },
    RuneKit { id: ZERO_ID, name: "tide_shore",    glyphs: G_TIDE,  color_palette: pal(0xFF44AAEE, 0xFF88CCFF, 0xFF226699, 0xFFFFFFFF), particle_density: 2048 },
    RuneKit { id: ZERO_ID, name: "tide_deep",     glyphs: G_TIDE,  color_palette: pal(0xFF112266, 0xFF335599, 0xFF000033, 0xFFFFFFFF), particle_density: 2048 },
    RuneKit { id: ZERO_ID, name: "stone_basalt",  glyphs: G_STONE, color_palette: pal(0xFF222222, 0xFF555555, 0xFF111111, 0xFFFFFFFF), particle_density: 256  },
    RuneKit { id: ZERO_ID, name: "stone_marble",  glyphs: G_STONE, color_palette: pal(0xFFEEEEEE, 0xFFFFFFFF, 0xFFCCCCCC, 0xFF888888), particle_density: 256  },
    RuneKit { id: ZERO_ID, name: "stone_granite", glyphs: G_STONE, color_palette: pal(0xFF888888, 0xFFAAAAAA, 0xFF555555, 0xFFFFFFFF), particle_density: 256  },
    RuneKit { id: ZERO_ID, name: "rune_elder",    glyphs: G_RUNE,  color_palette: pal(0xFF99CC44, 0xFFCCFF88, 0xFF446622, 0xFFFFFFFF), particle_density: 1536 },
    RuneKit { id: ZERO_ID, name: "rune_arcane",   glyphs: G_RUNE,  color_palette: pal(0xFF6644CC, 0xFFAA88FF, 0xFF332266, 0xFFFFFFFF), particle_density: 1536 },
    RuneKit { id: ZERO_ID, name: "rune_blood",    glyphs: G_RUNE,  color_palette: pal(0xFFCC2222, 0xFFFF6666, 0xFF661111, 0xFFFFFFFF), particle_density: 1536 },
    RuneKit { id: ZERO_ID, name: "loom_dawn",     glyphs: G_LOOM,  color_palette: pal(0xFFFFCCAA, 0xFFFFEEDD, 0xFFCC6644, 0xFFFFFFFF), particle_density: 768  },
    RuneKit { id: ZERO_ID, name: "loom_dusk",     glyphs: G_LOOM,  color_palette: pal(0xFF884466, 0xFFCC88AA, 0xFF442233, 0xFFFFFFFF), particle_density: 768  },
    RuneKit { id: ZERO_ID, name: "shard_quartz",  glyphs: G_SHARD, color_palette: pal(0xFFEEEEFF, 0xFFFFFFFF, 0xFFCCCCEE, 0xFF000000), particle_density: 3072 },
    RuneKit { id: ZERO_ID, name: "shard_obsidian",glyphs: G_SHARD, color_palette: pal(0xFF111122, 0xFF333344, 0xFF000011, 0xFFFFFFFF), particle_density: 3072 },
    RuneKit { id: ZERO_ID, name: "shard_emerald", glyphs: G_SHARD, color_palette: pal(0xFF22AA66, 0xFF44CC88, 0xFF115533, 0xFFFFFFFF), particle_density: 3072 },
    RuneKit { id: ZERO_ID, name: "vein_copper",   glyphs: G_VEIN,  color_palette: pal(0xFFCC7733, 0xFFFFAA66, 0xFF663311, 0xFFFFFFFF), particle_density: 512  },
    RuneKit { id: ZERO_ID, name: "vein_silver",   glyphs: G_VEIN,  color_palette: pal(0xFFCCCCCC, 0xFFFFFFFF, 0xFF888888, 0xFFFFFFFF), particle_density: 512  },
    RuneKit { id: ZERO_ID, name: "vein_gold",     glyphs: G_VEIN,  color_palette: pal(0xFFFFCC44, 0xFFFFEE99, 0xFF886611, 0xFFFFFFFF), particle_density: 512  },
    RuneKit { id: ZERO_ID, name: "halo_pale",     glyphs: G_HALO,  color_palette: pal(0xFFFFFFFF, 0xFFEEEEFF, 0xFFCCCCEE, 0xFF000000), particle_density: 5120 },
    RuneKit { id: ZERO_ID, name: "halo_solar",    glyphs: G_HALO,  color_palette: pal(0xFFFFEE88, 0xFFFFFFCC, 0xFFFFAA22, 0xFF000000), particle_density: 5120 },
    RuneKit { id: ZERO_ID, name: "halo_lunar",    glyphs: G_HALO,  color_palette: pal(0xFFCCCCEE, 0xFFEEEEFF, 0xFF8888AA, 0xFF000000), particle_density: 5120 },
    RuneKit { id: ZERO_ID, name: "tide_storm",    glyphs: G_TIDE,  color_palette: pal(0xFF334455, 0xFF6688AA, 0xFF112233, 0xFFFFFFFF), particle_density: 2048 },
    RuneKit { id: ZERO_ID, name: "leaf_winter",   glyphs: G_LEAF,  color_palette: pal(0xFFAACCEE, 0xFFFFFFFF, 0xFF6688AA, 0xFFFFFFFF), particle_density: 1024 },
    RuneKit { id: ZERO_ID, name: "flame_pale",    glyphs: G_FLAME, color_palette: pal(0xFFCCEEFF, 0xFFEEFFFF, 0xFF8899CC, 0xFFFFFFFF), particle_density: 4096 },
    RuneKit { id: ZERO_ID, name: "rune_void",     glyphs: G_RUNE,  color_palette: pal(0xFF000011, 0xFF221133, 0xFF000000, 0xFFFFFFFF), particle_density: 1536 },
    RuneKit { id: ZERO_ID, name: "halo_aurora",   glyphs: G_HALO,  color_palette: pal(0xFF44FFCC, 0xFFCCFFEE, 0xFF22AA88, 0xFF000000), particle_density: 5120 },
];

/// § resolved-id lookup : derive a hashed `RuneKitId` from preset-name.
pub fn resolved_id(kit: &RuneKit) -> RuneKitId {
    RuneKitId::from_name(kit.name)
}

/// Look up a preset by canonical name. Linear over ~32 entries.
pub fn preset_by_name(name: &str) -> Option<&'static RuneKit> {
    PRESET_RUNE_KITS.iter().find(|k| k.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn at_least_thirty_presets() {
        assert!(PRESET_RUNE_KITS.len() >= 30, "spec requires ≥ 30 rune-kit presets");
    }

    #[test]
    fn preset_names_unique() {
        let mut seen = std::collections::BTreeSet::new();
        for k in PRESET_RUNE_KITS {
            assert!(seen.insert(k.name), "duplicate preset name {}", k.name);
        }
    }

    #[test]
    fn resolved_ids_unique() {
        let mut seen = std::collections::BTreeSet::new();
        for k in PRESET_RUNE_KITS {
            let id = resolved_id(k);
            assert!(seen.insert(id), "collision on preset {}", k.name);
        }
    }

    #[test]
    fn preset_by_name_finds_each() {
        for k in PRESET_RUNE_KITS {
            assert!(preset_by_name(k.name).is_some());
        }
        assert!(preset_by_name("nonexistent_kit").is_none());
    }

    #[test]
    fn id_from_name_deterministic() {
        let a = RuneKitId::from_name("flame_amber");
        let b = RuneKitId::from_name("flame_amber");
        let c = RuneKitId::from_name("flame_azure");
        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}
