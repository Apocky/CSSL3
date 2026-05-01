//! § sources/opengameart — OpenGameArt.org adapter.
//! ══════════════════════════════════════════════════
//!
//! OpenGameArt (opengameart.org) hosts a community catalog of CC0 / CC-BY /
//! CC-BY-SA / GPL game art. Stage-0 ships a representative cross-license
//! catalog so license-filter tests are exercisable across all 4 license
//! variants. The URL pattern follows the canonical `opengameart.org/content`
//! per-content-page slug.

use crate::{AssetFormat, AssetMeta, AssetSource, License, LicenseFilter, SourceError, SourceResult};

pub struct OpenGameArtSource {
    catalog: Vec<AssetMeta>,
}

impl OpenGameArtSource {
    #[must_use]
    pub fn new() -> Self {
        Self {
            catalog: catalog(),
        }
    }
}

impl Default for OpenGameArtSource {
    fn default() -> Self {
        Self::new()
    }
}

impl AssetSource for OpenGameArtSource {
    fn name(&self) -> &str {
        "opengameart"
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
        match entry.format {
            AssetFormat::Glb => buf.extend_from_slice(b"glTF"),
            AssetFormat::Obj => buf.extend_from_slice(b"# obj\n"),
            _ => buf.extend_from_slice(b"OGA-PLACEHOLDER\n"),
        }
        buf.extend_from_slice(format!("opengameart:{}", entry.id).as_bytes());
        Ok(buf)
    }
}

fn catalog() -> Vec<AssetMeta> {
    vec![
        AssetMeta {
            id: "oga:lpc-character-base".to_string(),
            src: "opengameart".to_string(),
            name: "LPC Character Base".to_string(),
            license: License::CcBySa,
            format: AssetFormat::Other,
            url: "https://opengameart.org/content/lpc-character-base".to_string(),
            author: "Stephen Challener (Redshrike)".to_string(),
            tags: vec!["character".into(), "sprite".into(), "rpg".into()],
            size_bytes: 600_000,
        },
        AssetMeta {
            id: "oga:rpg-monster-pack".to_string(),
            src: "opengameart".to_string(),
            name: "RPG Monster Pack".to_string(),
            license: License::CcBy,
            format: AssetFormat::Other,
            url: "https://opengameart.org/content/rpg-monster-pack".to_string(),
            author: "DragonDePlatino".to_string(),
            tags: vec!["monster".into(), "rpg".into(), "sprite".into()],
            size_bytes: 1_200_000,
        },
        AssetMeta {
            id: "oga:fantasy-icons-cc0".to_string(),
            src: "opengameart".to_string(),
            name: "Fantasy Icons (CC0)".to_string(),
            license: License::Cc0,
            format: AssetFormat::Other,
            url: "https://opengameart.org/content/fantasy-icons-cc0".to_string(),
            author: String::new(),
            tags: vec!["icon".into(), "fantasy".into(), "ui".into()],
            size_bytes: 400_000,
        },
        AssetMeta {
            id: "oga:lowpoly-tree-set".to_string(),
            src: "opengameart".to_string(),
            name: "Low-poly tree set".to_string(),
            license: License::Cc0,
            format: AssetFormat::Glb,
            url: "https://opengameart.org/content/lowpoly-tree-set".to_string(),
            author: String::new(),
            tags: vec!["tree".into(), "lowpoly".into(), "nature".into()],
            size_bytes: 800_000,
        },
        AssetMeta {
            id: "oga:medieval-house-cc0".to_string(),
            src: "opengameart".to_string(),
            name: "Medieval house CC0".to_string(),
            license: License::Cc0,
            format: AssetFormat::Glb,
            url: "https://opengameart.org/content/medieval-house-cc0".to_string(),
            author: String::new(),
            tags: vec!["medieval".into(), "house".into(), "building".into()],
            size_bytes: 2_500_000,
        },
        AssetMeta {
            id: "oga:gpl-spaceship-set".to_string(),
            src: "opengameart".to_string(),
            name: "GPL Spaceship Set".to_string(),
            license: License::Gpl,
            format: AssetFormat::Obj,
            url: "https://opengameart.org/content/gpl-spaceship-set".to_string(),
            author: "Anonymous Contributor".to_string(),
            tags: vec!["spaceship".into(), "scifi".into(), "obj".into()],
            size_bytes: 1_800_000,
        },
        AssetMeta {
            id: "oga:bysa-knight-anim".to_string(),
            src: "opengameart".to_string(),
            name: "CC-BY-SA animated knight".to_string(),
            license: License::CcBySa,
            format: AssetFormat::Glb,
            url: "https://opengameart.org/content/bysa-knight-anim".to_string(),
            author: "Skorpio".to_string(),
            tags: vec!["knight".into(), "animated".into(), "rpg".into()],
            size_bytes: 3_400_000,
        },
        AssetMeta {
            id: "oga:dungeon-tileset-cc0".to_string(),
            src: "opengameart".to_string(),
            name: "Dungeon tileset CC0".to_string(),
            license: License::Cc0,
            format: AssetFormat::Other,
            url: "https://opengameart.org/content/dungeon-tileset-cc0".to_string(),
            author: String::new(),
            tags: vec!["dungeon".into(), "tileset".into(), "2d".into()],
            size_bytes: 700_000,
        },
    ]
}
