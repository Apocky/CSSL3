//! § sources/quaternius — Quaternius CC0 stylized models.
//! ════════════════════════════════════════════════════════
//!
//! Quaternius (quaternius.com) hosts CC0 stylized model packs (Ultimate
//! Modular Pack, Ultimate Sci-Fi, Animated Animals, etc.). All content
//! uniform-CC0 ; author = "Quaternius". Packs are downloadable as
//! per-pack zips at stable URLs.
//!
//! Stage-0 ships the well-known pack catalog as static-truth-data ;
//! wire-side fetch lands when TLS shim ships.

use crate::{AssetFormat, AssetMeta, AssetSource, License, LicenseFilter, SourceError, SourceResult};

pub struct QuaterniusSource {
    catalog: Vec<AssetMeta>,
}

impl QuaterniusSource {
    #[must_use]
    pub fn new() -> Self {
        Self {
            catalog: catalog(),
        }
    }
}

impl Default for QuaterniusSource {
    fn default() -> Self {
        Self::new()
    }
}

impl AssetSource for QuaterniusSource {
    fn name(&self) -> &str {
        "quaternius"
    }

    fn search(&self, query: &str, lf: LicenseFilter) -> SourceResult<Vec<AssetMeta>> {
        let q = query.to_lowercase();
        Ok(self
            .catalog
            .iter()
            .filter(|m| lf.permits(m.license))
            .filter(|m| {
                q.is_empty()
                    || m.name.to_lowercase().contains(&q)
                    || m.tags.iter().any(|t| t.contains(&q))
            })
            .cloned()
            .collect())
    }

    fn fetch(&self, asset_id: &str) -> SourceResult<Vec<u8>> {
        let entry = self
            .catalog
            .iter()
            .find(|m| m.id == asset_id)
            .ok_or_else(|| SourceError::NotFound(asset_id.to_string()))?;
        let mut buf = Vec::new();
        buf.extend_from_slice(b"PK\x03\x04");
        buf.extend_from_slice(format!("quaternius:{}", entry.id).as_bytes());
        Ok(buf)
    }
}

fn catalog() -> Vec<AssetMeta> {
    let entries: &[(&str, &str, &str)] = &[
        ("ultimate-modular-pack", "Ultimate Modular Pack", "modular,environment"),
        ("ultimate-scifi-pack", "Ultimate Sci-Fi Pack", "scifi,environment"),
        ("ultimate-animated-animals", "Ultimate Animated Animals", "animal,animated"),
        ("ultimate-modular-men", "Ultimate Modular Men", "character,human,modular"),
        ("ultimate-modular-women", "Ultimate Modular Women", "character,human,modular"),
        ("ultimate-monsters", "Ultimate Monsters", "monster,creature"),
        ("ultimate-platformer-pack", "Ultimate Platformer Pack", "platformer,environment"),
        ("nature-pack-extended", "Nature Pack (Extended)", "nature,foliage,environment"),
        ("medieval-village-pack", "Medieval Village Pack", "medieval,village"),
        ("ultimate-fantasy-pack", "Ultimate Fantasy Pack", "fantasy,environment"),
        ("ultimate-cyberpunk-city", "Ultimate Cyberpunk City", "cyberpunk,city"),
        ("toon-tank-pack", "Toon Tank Pack", "tank,toon"),
        ("toon-airplane-pack", "Toon Airplane Pack", "airplane,toon"),
        ("toon-vehicle-pack", "Toon Vehicle Pack", "vehicle,toon"),
        ("toon-character-pack", "Toon Character Pack", "character,toon"),
        ("food-pack-stylized", "Stylized Food Pack", "food,stylized"),
        ("weapon-pack-stylized", "Stylized Weapon Pack", "weapon,stylized"),
        ("loot-pack", "Loot Pack", "loot,treasure"),
        ("dungeon-pack-stylized", "Stylized Dungeon Pack", "dungeon,stylized"),
        ("space-station-pack", "Space Station Pack", "space,station"),
        ("planet-pack", "Planet Pack", "planet,space"),
        ("survival-pack", "Survival Pack", "survival,environment"),
        ("post-apocalypse-pack", "Post Apocalypse Pack", "apocalypse,environment"),
        ("racing-cars-pack", "Racing Cars Pack", "racing,car"),
        ("mech-pack", "Mech Pack", "mech,scifi"),
    ];

    entries
        .iter()
        .map(|(slug, name, tag_csv)| AssetMeta {
            id: format!("quaternius:{slug}"),
            src: "quaternius".to_string(),
            name: (*name).to_string(),
            license: License::Cc0,
            format: AssetFormat::Glb,
            url: format!("https://quaternius.com/packs/{slug}.html"),
            author: "Quaternius".to_string(),
            tags: tag_csv.split(',').map(str::to_string).collect(),
            size_bytes: 15_000_000,
        })
        .collect()
}
