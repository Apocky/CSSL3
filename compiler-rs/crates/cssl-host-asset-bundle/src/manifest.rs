// § manifest.rs : BundleManifest schema + MaterialEntry + FileEntry + FileKind
// ══════════════════════════════════════════════════════════════════════════
// § I> manifest = JSON-serializable summary of an AssetBundle ; pairs the
// § I>   license-record with material + file metadata + a content fingerprint
// § I> FileKind covers the canonical free-3D-asset payload shapes
// § I>   (GLTF/GLB + PNG-channel-roles + HDR + KTX) plus a Custom escape-hatch
// § I> material indices are u8 (≤ 255 mats per bundle) ; file indices are u32

use cssl_host_license_attribution::AssetLicenseRecord;
use serde::{Deserialize, Serialize};

use crate::MANIFEST_SCHEMA_VERSION;

/// Top-level bundle manifest.
///
/// Holds everything you need to describe a fetched-asset payload *without*
/// the bytes themselves — perfect for a side-channel JSON file alongside the
/// raw `.blob.<idx>` files written by [`crate::storage::LabStore::save`].
///
/// `schema_version` is emitted by [`crate::bundle::AssetBundle::finalize`]
/// from the [`crate::MANIFEST_SCHEMA_VERSION`] constant ; loaders check
/// against [`BundleManifest::expected_schema_version`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleManifest {
    /// Schema version — bumped when on-disk JSON layout changes non-additively.
    #[serde(default = "BundleManifest::expected_schema_version")]
    pub schema_version: u32,
    /// Human-readable bundle name (e.g. "tree_oak_01").
    pub name: String,
    /// Stable asset-id within LoA's content addressing.
    pub asset_id: String,
    /// Origin host or platform (e.g. "polyhaven", "kenney").
    pub source: String,
    /// License record — provenance + attribution + LoA-policy compatibility.
    pub license_record: AssetLicenseRecord,
    /// Materials referenced by the asset's submeshes (≤ 255).
    #[serde(default)]
    pub materials: Vec<MaterialEntry>,
    /// Files (geometry + textures + HDRs) carried by the bundle.
    #[serde(default)]
    pub files: Vec<FileEntry>,
    /// Aggregate FNV-1a-128 fingerprint over all file blobs (32 lower-hex chars).
    /// Empty until [`crate::bundle::AssetBundle::finalize`] is called.
    #[serde(default)]
    pub fingerprint_hex: String,
}

impl BundleManifest {
    /// Schema version this build of the crate emits + accepts.
    pub fn expected_schema_version() -> u32 {
        MANIFEST_SCHEMA_VERSION
    }

    /// True iff `schema_version` matches what this build expects.
    pub fn schema_compatible(&self) -> bool {
        self.schema_version == Self::expected_schema_version()
    }
}

/// One material entry — channel-indexed PBR slots referencing files by index.
///
/// Each `Option<u32>` is an index into [`BundleManifest::files`] (or `None`
/// when the channel is absent). u32 chosen over u8 so future bundles can
/// carry > 255 textures (decals, atlases, etc.) without manifest churn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaterialEntry {
    /// Material index within this bundle (0-based, ≤ 255).
    pub idx: u8,
    /// Material name (e.g. "trunk_bark", "leaf_canopy_01").
    pub name: String,
    /// File index of the base-color (albedo) texture.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_color: Option<u32>,
    /// File index of the normal-map texture.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub normal_map: Option<u32>,
    /// File index of the roughness-map texture.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub roughness_map: Option<u32>,
    /// File index of the metallic-map texture.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metallic_map: Option<u32>,
}

/// One file entry — name + size + per-file FNV-1a-128 fingerprint.
///
/// Per-file fingerprints are computed at `add_file` time so loaders can
/// resume / verify partial loads without re-reading the entire bundle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileEntry {
    /// Semantic role of the file.
    pub kind: FileKind,
    /// File name (e.g. "tree.glb", "trunk_basecolor.png").
    pub name: String,
    /// Length of the blob in bytes.
    pub byte_length: u32,
    /// Per-file FNV-1a-128 fingerprint, lower-hex (32 chars).
    pub fingerprint_hex: String,
}

/// Semantic role of a file blob within an asset bundle.
///
/// Tagged with `kind` discriminator + optional `text` payload (used by `Custom`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "text")]
pub enum FileKind {
    /// glTF text (.gltf) — JSON scene description.
    #[serde(rename = "gltf")]
    Gltf,
    /// glTF binary (.glb) — single-file packed.
    #[serde(rename = "glb")]
    Glb,
    /// PNG base-color (albedo) texture.
    #[serde(rename = "png_base_color")]
    PngBaseColor,
    /// PNG normal-map texture (tangent-space, OpenGL-Y or DirectX-Y per source).
    #[serde(rename = "png_normal")]
    PngNormal,
    /// PNG roughness texture (single-channel, packed in red).
    #[serde(rename = "png_roughness")]
    PngRoughness,
    /// PNG metallic texture (single-channel, packed in red).
    #[serde(rename = "png_metallic")]
    PngMetallic,
    /// PNG emission texture.
    #[serde(rename = "png_emission")]
    PngEmission,
    /// HDR equirectangular environment map (.hdr / .exr — payload is raw bytes).
    #[serde(rename = "hdr")]
    Hdr,
    /// KTX or KTX2 GPU-compressed texture container.
    #[serde(rename = "ktx_compressed")]
    KtxCompressed,
    /// Custom file role — payload carries the semantic label.
    #[serde(rename = "custom")]
    Custom(String),
}

impl FileKind {
    /// Short label suitable for logs / inspection.
    pub fn label(&self) -> &str {
        match self {
            FileKind::Gltf => "gltf",
            FileKind::Glb => "glb",
            FileKind::PngBaseColor => "png_base_color",
            FileKind::PngNormal => "png_normal",
            FileKind::PngRoughness => "png_roughness",
            FileKind::PngMetallic => "png_metallic",
            FileKind::PngEmission => "png_emission",
            FileKind::Hdr => "hdr",
            FileKind::KtxCompressed => "ktx_compressed",
            FileKind::Custom(s) => s.as_str(),
        }
    }

    /// True iff the file kind is a PNG channel-role (used by material slot binding).
    pub fn is_png_channel(&self) -> bool {
        matches!(
            self,
            FileKind::PngBaseColor
                | FileKind::PngNormal
                | FileKind::PngRoughness
                | FileKind::PngMetallic
                | FileKind::PngEmission
        )
    }
}

// ─────────────────────────────────────────────────────────────────
// tests
// ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use cssl_host_license_attribution::License;

    fn cc0_record() -> AssetLicenseRecord {
        AssetLicenseRecord::new("rock_03", "kenney", License::CC0)
    }

    #[test]
    fn schema_version_constant_is_one() {
        assert_eq!(BundleManifest::expected_schema_version(), 1);
    }

    #[test]
    fn manifest_roundtrip_serde() {
        let manifest = BundleManifest {
            schema_version: BundleManifest::expected_schema_version(),
            name: "tree_oak_01".into(),
            asset_id: "polyhaven::tree_oak_01".into(),
            source: "polyhaven".into(),
            license_record: cc0_record(),
            materials: vec![MaterialEntry {
                idx: 0,
                name: "bark".into(),
                base_color: Some(0),
                normal_map: Some(1),
                roughness_map: None,
                metallic_map: None,
            }],
            files: vec![FileEntry {
                kind: FileKind::PngBaseColor,
                name: "bark_basecolor.png".into(),
                byte_length: 4096,
                fingerprint_hex: "00".repeat(16),
            }],
            fingerprint_hex: "ff".repeat(16),
        };
        let json = serde_json::to_string(&manifest).expect("ser");
        let back: BundleManifest = serde_json::from_str(&json).expect("de");
        assert_eq!(manifest, back);
        assert!(back.schema_compatible());
    }

    #[test]
    fn file_kind_serde_tag_form() {
        // canonical kinds round-trip
        let glb = FileKind::Glb;
        let json = serde_json::to_string(&glb).unwrap();
        assert!(json.contains("\"kind\""));
        assert!(json.contains("\"glb\""));
        let back: FileKind = serde_json::from_str(&json).unwrap();
        assert_eq!(back, glb);

        // custom carries text payload
        let custom = FileKind::Custom("my_special_format_v2".into());
        let json2 = serde_json::to_string(&custom).unwrap();
        assert!(json2.contains("my_special_format_v2"));
        let back2: FileKind = serde_json::from_str(&json2).unwrap();
        assert_eq!(back2, custom);
    }

    #[test]
    fn file_kind_label_and_png_channel() {
        assert_eq!(FileKind::Gltf.label(), "gltf");
        assert_eq!(FileKind::Glb.label(), "glb");
        assert_eq!(FileKind::PngBaseColor.label(), "png_base_color");
        assert_eq!(FileKind::Hdr.label(), "hdr");
        assert_eq!(FileKind::KtxCompressed.label(), "ktx_compressed");
        assert_eq!(FileKind::Custom("z".into()).label(), "z");

        for k in [
            FileKind::PngBaseColor,
            FileKind::PngNormal,
            FileKind::PngRoughness,
            FileKind::PngMetallic,
            FileKind::PngEmission,
        ] {
            assert!(k.is_png_channel(), "expected PNG-channel : {}", k.label());
        }
        for k in [
            FileKind::Gltf,
            FileKind::Glb,
            FileKind::Hdr,
            FileKind::KtxCompressed,
            FileKind::Custom("x".into()),
        ] {
            assert!(!k.is_png_channel(), "expected non-PNG : {}", k.label());
        }
    }

    #[test]
    fn material_entry_serde_skips_none() {
        let mat = MaterialEntry {
            idx: 7,
            name: "leaf".into(),
            base_color: Some(2),
            normal_map: None,
            roughness_map: None,
            metallic_map: None,
        };
        let json = serde_json::to_string(&mat).unwrap();
        // None fields are skipped in serialization
        assert!(json.contains("\"base_color\":2"));
        assert!(!json.contains("normal_map"));
        assert!(!json.contains("roughness_map"));
        assert!(!json.contains("metallic_map"));
        // and round-trip back faithfully
        let back: MaterialEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(back, mat);
    }

    #[test]
    fn schema_compat_check() {
        let mut m = BundleManifest {
            schema_version: BundleManifest::expected_schema_version(),
            name: "n".into(),
            asset_id: "a".into(),
            source: "s".into(),
            license_record: cc0_record(),
            materials: vec![],
            files: vec![],
            fingerprint_hex: String::new(),
        };
        assert!(m.schema_compatible());
        m.schema_version = 9999;
        assert!(!m.schema_compatible());
    }
}
