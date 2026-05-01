//! § sources/sketchfab — Sketchfab adapter.
//! ══════════════════════════════════════════
//!
//! Sketchfab hosts CC0 / CC-BY / CC-BY-SA glTF assets. Public search API
//! lives at `https://api.sketchfab.com/v3/search`. Stage-0 ships a
//! representative mocked catalog ; wire-side HTTP lands when the cssl-rt
//! TLS shim ships.

use crate::{AssetFormat, AssetMeta, AssetSource, License, LicenseFilter, SourceError, SourceResult};

/// Sketchfab adapter. Stage-0 returns a curated representative-catalog.
pub struct SketchfabSource {
    catalog: Vec<AssetMeta>,
}

impl SketchfabSource {
    #[must_use]
    pub fn new() -> Self {
        Self {
            catalog: catalog(),
        }
    }
}

impl Default for SketchfabSource {
    fn default() -> Self {
        Self::new()
    }
}

impl AssetSource for SketchfabSource {
    fn name(&self) -> &str {
        "sketchfab"
    }

    fn search(&self, query: &str, lf: LicenseFilter) -> SourceResult<Vec<AssetMeta>> {
        let q = query.to_lowercase();
        let results: Vec<AssetMeta> = self
            .catalog
            .iter()
            .filter(|m| lf.permits(m.license))
            .filter(|m| {
                q.is_empty()
                    || m.name.to_lowercase().contains(&q)
                    || m.tags.iter().any(|t| t.contains(&q))
            })
            .cloned()
            .collect();
        if !q.is_empty() && results.is_empty() {
            // Empty query with no filter-exclusion is a legitimate empty
            // result ; an explicit query that filters everything out is
            // also legitimate (callers expect Vec, not Err). LicenseFilterExcluded
            // is reserved for "filter excluded the entire un-queried catalog".
        }
        Ok(results)
    }

    fn fetch(&self, asset_id: &str) -> SourceResult<Vec<u8>> {
        let entry = self
            .catalog
            .iter()
            .find(|m| m.id == asset_id)
            .ok_or_else(|| SourceError::NotFound(asset_id.to_string()))?;
        // Stage-0 : synthesize a deterministic placeholder GLB body so
        // tests + downstream consumers can exercise round-trip without
        // network access. The body is a valid (minimal) glTF JSON wrapper
        // followed by a stable byte-pattern.
        Ok(synth_glb_for(entry))
    }
}

fn synth_glb_for(meta: &AssetMeta) -> Vec<u8> {
    // Minimal valid-shaped glTF JSON ; consumers parse and route to the
    // (real) cssl-asset gltf parser. This is NOT a binary GLB ; the format
    // hint in the catalog (Glb vs Gltf) signals which path the consumer
    // should take. For Stage-0, both routes return a JSON body that the
    // downstream gltf parser accepts.
    let json = format!(
        "{{\"asset\":{{\"version\":\"2.0\",\"generator\":\"cssl-asset-fetcher stage-0 sketchfab placeholder for {}\"}},\"meta\":{{\"id\":\"{}\",\"license\":\"{}\"}},\"buffers\":[],\"meshes\":[]}}",
        meta.id,
        meta.id,
        match meta.license {
            License::Cc0 => "cc0",
            License::CcBy => "cc-by",
            License::CcBySa => "cc-by-sa",
            License::Gpl => "gpl",
            License::Other => "other",
        },
    );
    json.into_bytes()
}

fn catalog() -> Vec<AssetMeta> {
    vec![
        AssetMeta {
            id: "sketchfab:medieval-tower-cc0".to_string(),
            src: "sketchfab".to_string(),
            name: "Medieval stone tower (CC0)".to_string(),
            license: License::Cc0,
            format: AssetFormat::Glb,
            url: "https://sketchfab.com/3d-models/medieval-tower-cc0.glb".to_string(),
            author: String::new(),
            tags: vec!["medieval".into(), "tower".into(), "stone".into()],
            size_bytes: 1_500_000,
        },
        AssetMeta {
            id: "sketchfab:forest-tree-pack".to_string(),
            src: "sketchfab".to_string(),
            name: "Stylized forest tree pack".to_string(),
            license: License::CcBy,
            format: AssetFormat::Glb,
            url: "https://sketchfab.com/3d-models/forest-tree-pack.glb".to_string(),
            author: "ForestArtist".to_string(),
            tags: vec!["forest".into(), "tree".into(), "nature".into()],
            size_bytes: 4_200_000,
        },
        AssetMeta {
            id: "sketchfab:dungeon-skeleton".to_string(),
            src: "sketchfab".to_string(),
            name: "Animated dungeon skeleton".to_string(),
            license: License::CcBy,
            format: AssetFormat::Glb,
            url: "https://sketchfab.com/3d-models/dungeon-skeleton.glb".to_string(),
            author: "RiggedArt".to_string(),
            tags: vec!["dungeon".into(), "skeleton".into(), "creature".into()],
            size_bytes: 2_800_000,
        },
        AssetMeta {
            id: "sketchfab:scifi-station-cc0".to_string(),
            src: "sketchfab".to_string(),
            name: "Sci-fi space station (CC0)".to_string(),
            license: License::Cc0,
            format: AssetFormat::Glb,
            url: "https://sketchfab.com/3d-models/scifi-station-cc0.glb".to_string(),
            author: String::new(),
            tags: vec!["scifi".into(), "station".into(), "space".into()],
            size_bytes: 6_500_000,
        },
        AssetMeta {
            id: "sketchfab:cathedral-bysa".to_string(),
            src: "sketchfab".to_string(),
            name: "Gothic cathedral (CC-BY-SA)".to_string(),
            license: License::CcBySa,
            format: AssetFormat::Glb,
            url: "https://sketchfab.com/3d-models/cathedral-bysa.glb".to_string(),
            author: "GothicArchitect".to_string(),
            tags: vec!["cathedral".into(), "gothic".into(), "architecture".into()],
            size_bytes: 8_900_000,
        },
        AssetMeta {
            id: "sketchfab:proprietary-tank".to_string(),
            src: "sketchfab".to_string(),
            name: "Proprietary tank (Other license)".to_string(),
            license: License::Other,
            format: AssetFormat::Glb,
            url: "https://sketchfab.com/3d-models/proprietary-tank.glb".to_string(),
            author: "TankShop".to_string(),
            tags: vec!["tank".into(), "vehicle".into(), "military".into()],
            size_bytes: 3_300_000,
        },
    ]
}
