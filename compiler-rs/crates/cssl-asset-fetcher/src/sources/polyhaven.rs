//! § sources/polyhaven — PolyHaven adapter.
//! ══════════════════════════════════════════
//!
//! PolyHaven (polyhaven.com) hosts CC0 PBR materials, HDRIs, and 3D models.
//! Public API : `https://api.polyhaven.com/assets`. Stage-0 ships a
//! representative catalog ; wire-side fetch lands when the cssl-rt TLS
//! shim ships. PolyHaven is uniformly CC0 ; attribution is appreciated
//! but not required (we set `author = "Poly Haven"` for traceability).

use crate::{AssetFormat, AssetMeta, AssetSource, License, LicenseFilter, SourceError, SourceResult};

pub struct PolyHavenSource {
    catalog: Vec<AssetMeta>,
}

impl PolyHavenSource {
    #[must_use]
    pub fn new() -> Self {
        Self {
            catalog: catalog(),
        }
    }
}

impl Default for PolyHavenSource {
    fn default() -> Self {
        Self::new()
    }
}

impl AssetSource for PolyHavenSource {
    fn name(&self) -> &str {
        "polyhaven"
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
        Ok(synth_payload_for(entry))
    }
}

fn synth_payload_for(meta: &AssetMeta) -> Vec<u8> {
    // Stage-0 placeholder. The format hint determines the synthesized shape :
    //   PbrMaterial → minimal "TEXSET" header + per-channel placeholder bytes
    //   Hdri        → minimal "RGBE" stub (single 1x1 pixel)
    //   Glb/Gltf    → minimal glTF JSON
    match meta.format {
        AssetFormat::PbrMaterial => {
            let mut buf = b"CSSLv3-PBR-PLACEHOLDER\n".to_vec();
            buf.extend_from_slice(format!("id={}\n", meta.id).as_bytes());
            buf.extend_from_slice(b"channels=albedo,normal,roughness,metallic\n");
            buf
        }
        AssetFormat::Hdri => {
            // Minimal Radiance-format-shaped stub.
            let mut buf = b"#?RADIANCE\nFORMAT=32-bit_rle_rgbe\n\n-Y 1 +X 1\n".to_vec();
            buf.extend_from_slice(&[0x80, 0x80, 0x80, 0x80]); // 1x1 black pixel
            buf
        }
        _ => {
            let json = format!(
                "{{\"asset\":{{\"version\":\"2.0\",\"generator\":\"cssl-asset-fetcher polyhaven stage-0 placeholder for {}\"}},\"meta\":{{\"id\":\"{}\"}}}}",
                meta.id, meta.id
            );
            json.into_bytes()
        }
    }
}

fn catalog() -> Vec<AssetMeta> {
    let mut v = Vec::new();
    // PBR materials
    for (id, name, tags) in [
        ("polyhaven:wood-planks-01", "Wood planks 01", "wood,planks,floor"),
        ("polyhaven:stone-wall-04", "Stone wall 04", "stone,wall,medieval"),
        ("polyhaven:metal-rust-02", "Rusted metal 02", "metal,rust,industrial"),
        ("polyhaven:fabric-leather-03", "Leather fabric 03", "fabric,leather,armor"),
        ("polyhaven:concrete-cracked-01", "Cracked concrete 01", "concrete,cracked,urban"),
        ("polyhaven:moss-forest-02", "Forest moss 02", "moss,forest,organic"),
        ("polyhaven:brick-modular-05", "Modular brick 05", "brick,modular,wall"),
        ("polyhaven:dirt-path-01", "Dirt path 01", "dirt,path,ground"),
    ] {
        v.push(AssetMeta {
            id: id.to_string(),
            src: "polyhaven".to_string(),
            name: name.to_string(),
            license: License::Cc0,
            format: AssetFormat::PbrMaterial,
            url: format!("https://polyhaven.com/a/{}", id.replace("polyhaven:", "")),
            author: "Poly Haven".to_string(),
            tags: tags.split(',').map(str::to_string).collect(),
            size_bytes: 12_000_000, // typical 4k PBR set zipped
        });
    }
    // HDRIs
    for (id, name, tags) in [
        ("polyhaven:studio-small-09", "Studio small 09", "studio,indoor,light"),
        ("polyhaven:rural-asphalt", "Rural asphalt road", "outdoor,road,sky"),
        ("polyhaven:cave-wall-glow", "Cave wall glow", "cave,glow,fantasy"),
        ("polyhaven:sunset-mountain", "Sunset mountain", "sunset,mountain,outdoor"),
    ] {
        v.push(AssetMeta {
            id: id.to_string(),
            src: "polyhaven".to_string(),
            name: name.to_string(),
            license: License::Cc0,
            format: AssetFormat::Hdri,
            url: format!("https://polyhaven.com/a/{}", id.replace("polyhaven:", "")),
            author: "Poly Haven".to_string(),
            tags: tags.split(',').map(str::to_string).collect(),
            size_bytes: 25_000_000, // 4k EXR HDR
        });
    }
    // glTF models
    for (id, name, tags) in [
        ("polyhaven:chair-modern-01", "Modern chair 01", "chair,furniture,modern"),
        ("polyhaven:bookshelf-01", "Bookshelf 01", "bookshelf,furniture,interior"),
        ("polyhaven:lantern-old-02", "Old lantern 02", "lantern,light,medieval"),
    ] {
        v.push(AssetMeta {
            id: id.to_string(),
            src: "polyhaven".to_string(),
            name: name.to_string(),
            license: License::Cc0,
            format: AssetFormat::Glb,
            url: format!("https://polyhaven.com/a/{}", id.replace("polyhaven:", "")),
            author: "Poly Haven".to_string(),
            tags: tags.split(',').map(str::to_string).collect(),
            size_bytes: 5_000_000,
        });
    }
    v
}
