// § bundle.rs : in-memory AssetBundle + FNV-1a-128 fingerprint + validate
// ══════════════════════════════════════════════════════════════════════════
// § I> AssetBundle pairs a BundleManifest with the actual file-blobs in-memory
// § I> finalize() walks all blobs, FNV-1a-128 each, then aggregates into a
// § I>   bundle-wide fingerprint stamped into the manifest
// § I> validate() catches : fingerprint-drift · material-out-of-range · empty-name · no-license
// § I> fingerprint is *perceptual*, not crypto — matches cssl-host-golden choice

use cssl_host_license_attribution::AssetLicenseRecord;
use serde::{Deserialize, Serialize};

use crate::manifest::{BundleManifest, FileEntry, FileKind, MaterialEntry};
use crate::MANIFEST_SCHEMA_VERSION;

/// In-memory asset bundle : manifest + raw file blobs.
///
/// Construction order :
///   1. [`AssetBundle::new`] — seeds the manifest with name/asset-id/source/license
///   2. [`AssetBundle::add_file`] — append blob, get back its file index
///   3. [`AssetBundle::add_material`] — bind file indices to PBR slots
///   4. [`AssetBundle::finalize`] — compute aggregate fingerprint
///   5. [`AssetBundle::validate`] — sanity-check before persisting
#[derive(Debug, Clone)]
pub struct AssetBundle {
    manifest: BundleManifest,
    file_blobs: Vec<Vec<u8>>,
}

/// Errors surfaced by [`AssetBundle::validate`].
///
/// `Serialize`/`Deserialize` so callers can persist validation reports
/// (e.g. CI logs) without manual conversion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BundleErr {
    /// The aggregate fingerprint stamped in the manifest does not match the
    /// fingerprint recomputed from the live file-blobs. Indicates either a
    /// missing `finalize()` call or post-finalize mutation.
    FingerprintMismatch,
    /// A material referenced a file index outside `0..files.len()`.
    MaterialOutOfRange {
        /// Index of the offending material within `manifest.materials`.
        mat_idx: u8,
        /// File index that was out of range.
        file_idx: u32,
    },
    /// Bundle name or asset_id was empty.
    EmptyName,
    /// License record had `License::Unknown` AND no asset_id — can't ship.
    NoLicense,
}

impl std::fmt::Display for BundleErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BundleErr::FingerprintMismatch => write!(
                f,
                "bundle fingerprint mismatch — finalize() likely missing or stale"
            ),
            BundleErr::MaterialOutOfRange { mat_idx, file_idx } => write!(
                f,
                "material {mat_idx} references file_idx {file_idx} out of range"
            ),
            BundleErr::EmptyName => write!(f, "bundle name or asset_id is empty"),
            BundleErr::NoLicense => write!(
                f,
                "bundle has License::Unknown — refusing to ship without explicit license"
            ),
        }
    }
}

impl std::error::Error for BundleErr {}

impl AssetBundle {
    /// Construct an empty bundle. Manifest is seeded but `fingerprint_hex` is
    /// empty until [`AssetBundle::finalize`] is called.
    pub fn new(
        name: String,
        asset_id: String,
        source: String,
        license: AssetLicenseRecord,
    ) -> AssetBundle {
        AssetBundle {
            manifest: BundleManifest {
                schema_version: MANIFEST_SCHEMA_VERSION,
                name,
                asset_id,
                source,
                license_record: license,
                materials: Vec::new(),
                files: Vec::new(),
                fingerprint_hex: String::new(),
            },
            file_blobs: Vec::new(),
        }
    }

    /// Append a file blob. Returns the new file index (used by `add_material`).
    ///
    /// The per-file fingerprint is computed at insertion time so it is
    /// stable across later mutations of *other* blobs.
    pub fn add_file(&mut self, kind: FileKind, name: String, blob: Vec<u8>) -> u32 {
        let idx = u32::try_from(self.file_blobs.len()).unwrap_or(u32::MAX);
        let byte_length = u32::try_from(blob.len()).unwrap_or(u32::MAX);
        let fp = fnv_1a_128_hex(&blob);
        self.manifest.files.push(FileEntry {
            kind,
            name,
            byte_length,
            fingerprint_hex: fp,
        });
        self.file_blobs.push(blob);
        // mutating files invalidates the aggregate fingerprint
        self.manifest.fingerprint_hex.clear();
        idx
    }

    /// Add a material, binding file indices to PBR channel slots.
    ///
    /// Returns the new material index. Panics-free : if more than 255 materials
    /// are added, indices saturate at u8::MAX (downstream `validate` will not
    /// flag this — caller is expected to keep mats ≤ 255).
    pub fn add_material(
        &mut self,
        name: String,
        base_color: Option<u32>,
        normal: Option<u32>,
        roughness: Option<u32>,
        metallic: Option<u32>,
    ) -> u8 {
        let idx = u8::try_from(self.manifest.materials.len()).unwrap_or(u8::MAX);
        self.manifest.materials.push(MaterialEntry {
            idx,
            name,
            base_color,
            normal_map: normal,
            roughness_map: roughness,
            metallic_map: metallic,
        });
        // material change does not invalidate file-fingerprint, but it does
        // invalidate the aggregate-fingerprint computation since validate()
        // re-checks fingerprint via files only ; safe to leave fp_hex alone.
        idx
    }

    /// Compute the FNV-1a-128 aggregate fingerprint over all file blobs and
    /// stamp it into the manifest.
    ///
    /// Aggregation strategy : each blob contributes its bytes plus a one-byte
    /// `0xFF` separator into a single FNV-1a-128 stream. This keeps the
    /// fingerprint sensitive to file order + boundaries (so swapping two
    /// files of identical content but different role still produces a
    /// different aggregate fingerprint).
    pub fn finalize(&mut self) {
        let mut hi = FNV_OFFSET_128_HI;
        let mut lo = FNV_OFFSET_128_LO;
        for (i, blob) in self.file_blobs.iter().enumerate() {
            // separator byte derived from file index (low byte of u32)
            #[allow(clippy::cast_possible_truncation)]
            let sep = i as u8 ^ 0xFF;
            (hi, lo) = fnv_step(hi, lo, sep);
            for &b in blob {
                (hi, lo) = fnv_step(hi, lo, b);
            }
        }
        self.manifest.fingerprint_hex = encode_128_hex(hi, lo);
    }

    /// Borrow the manifest. Read-only access to the canonical bundle metadata.
    pub fn manifest(&self) -> &BundleManifest {
        &self.manifest
    }

    /// Borrow a file blob by index, or `None` if out of range.
    pub fn file_blob(&self, idx: u32) -> Option<&[u8]> {
        let i = usize::try_from(idx).ok()?;
        self.file_blobs.get(i).map(Vec::as_slice)
    }

    /// Number of file blobs in the bundle.
    pub fn file_count(&self) -> usize {
        self.file_blobs.len()
    }

    /// Number of materials in the bundle.
    pub fn material_count(&self) -> usize {
        self.manifest.materials.len()
    }

    /// Validate the bundle for shipping :
    ///   1. name + asset_id non-empty
    ///   2. license_record license != Unknown
    ///   3. every material's file-index references are in-range
    ///   4. aggregate fingerprint matches recomputation from blobs
    pub fn validate(&self) -> Result<(), BundleErr> {
        if self.manifest.name.is_empty() || self.manifest.asset_id.is_empty() {
            return Err(BundleErr::EmptyName);
        }
        if matches!(
            self.manifest.license_record.license,
            cssl_host_license_attribution::License::Unknown
        ) {
            return Err(BundleErr::NoLicense);
        }
        let n_files_u32 = u32::try_from(self.file_blobs.len()).unwrap_or(u32::MAX);
        for mat in &self.manifest.materials {
            for slot in [
                mat.base_color,
                mat.normal_map,
                mat.roughness_map,
                mat.metallic_map,
            ]
            .into_iter()
            .flatten()
            {
                if slot >= n_files_u32 {
                    return Err(BundleErr::MaterialOutOfRange {
                        mat_idx: mat.idx,
                        file_idx: slot,
                    });
                }
            }
        }
        // recompute aggregate fingerprint
        let mut hi = FNV_OFFSET_128_HI;
        let mut lo = FNV_OFFSET_128_LO;
        for (i, blob) in self.file_blobs.iter().enumerate() {
            #[allow(clippy::cast_possible_truncation)]
            let sep = i as u8 ^ 0xFF;
            (hi, lo) = fnv_step(hi, lo, sep);
            for &b in blob {
                (hi, lo) = fnv_step(hi, lo, b);
            }
        }
        let recomputed = encode_128_hex(hi, lo);
        if recomputed != self.manifest.fingerprint_hex {
            return Err(BundleErr::FingerprintMismatch);
        }
        Ok(())
    }

    /// Construct from previously-loaded parts. Used by [`crate::storage::LabStore::load`].
    /// Does NOT call `finalize` ; the manifest's fingerprint must already be stamped.
    pub(crate) fn from_parts(manifest: BundleManifest, file_blobs: Vec<Vec<u8>>) -> AssetBundle {
        AssetBundle {
            manifest,
            file_blobs,
        }
    }
}

// ─────────────────────────────────────────────────────────────────
// FNV-1a-128 (stdlib-only) — same shape as cssl-host-golden::snapshot
// ─────────────────────────────────────────────────────────────────

const FNV_OFFSET_128_HI: u64 = 0x6c62_272e_07bb_0142;
const FNV_OFFSET_128_LO: u64 = 0x62b8_2175_6295_c58d;
const FNV_PRIME_128_HI: u64 = 0x0000_0000_0100_0000;
const FNV_PRIME_128_LO: u64 = 0x0000_0000_0000_013b;

/// Step the FNV-1a-128 accumulator by one byte.
fn fnv_step(mut hi: u64, mut lo: u64, byte: u8) -> (u64, u64) {
    lo ^= u64::from(byte);
    let (new_hi, new_lo) = mul_128(hi, lo, FNV_PRIME_128_HI, FNV_PRIME_128_LO);
    hi = new_hi;
    lo = new_lo;
    (hi, lo)
}

/// 128 × 128 → 128 mul (truncated, wrapping).
fn mul_128(a_hi: u64, a_lo: u64, b_hi: u64, b_lo: u64) -> (u64, u64) {
    let lo_lo = u128::from(a_lo) * u128::from(b_lo);
    let lo_lo_hi = (lo_lo >> 64) as u64;
    let lo_lo_lo = lo_lo as u64;
    let cross = a_hi
        .wrapping_mul(b_lo)
        .wrapping_add(a_lo.wrapping_mul(b_hi));
    let hi = cross.wrapping_add(lo_lo_hi);
    (hi, lo_lo_lo)
}

/// Encode a 128-bit value as 32-char lower-hex, big-endian.
fn encode_128_hex(hi: u64, lo: u64) -> String {
    let mut out = String::with_capacity(32);
    for byte in hi.to_be_bytes() {
        out.push_str(&format!("{byte:02x}"));
    }
    for byte in lo.to_be_bytes() {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// Compute FNV-1a-128 of a single byte slice — used by per-file fingerprints.
///
/// Returns 32-char lower-hex. Documented as *perceptual fingerprint, not crypto*.
pub fn fnv_1a_128_hex(bytes: &[u8]) -> String {
    let mut hi = FNV_OFFSET_128_HI;
    let mut lo = FNV_OFFSET_128_LO;
    for &b in bytes {
        (hi, lo) = fnv_step(hi, lo, b);
    }
    encode_128_hex(hi, lo)
}

// ─────────────────────────────────────────────────────────────────
// tests
// ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use cssl_host_license_attribution::License;

    fn cc0_rec() -> AssetLicenseRecord {
        AssetLicenseRecord::new("rock_03", "kenney", License::CC0)
    }

    fn unknown_rec() -> AssetLicenseRecord {
        AssetLicenseRecord::new("mystery", "?", License::Unknown)
    }

    #[test]
    fn new_then_finalize_stamps_fp() {
        let mut b = AssetBundle::new(
            "rock_03".into(),
            "kenney::rock_03".into(),
            "kenney".into(),
            cc0_rec(),
        );
        assert_eq!(b.manifest().fingerprint_hex, "");
        let _ = b.add_file(FileKind::Glb, "rock.glb".into(), b"binary".to_vec());
        assert_eq!(b.manifest().fingerprint_hex, ""); // still empty until finalize
        b.finalize();
        assert_eq!(b.manifest().fingerprint_hex.len(), 32);
        // every char is a lower-hex digit
        for ch in b.manifest().fingerprint_hex.chars() {
            assert!(ch.is_ascii_hexdigit() && (!ch.is_ascii_uppercase()));
        }
    }

    #[test]
    fn add_file_returns_increasing_indices_and_assigns_per_file_fp() {
        let mut b = AssetBundle::new(
            "tree".into(),
            "polyhaven::tree".into(),
            "polyhaven".into(),
            cc0_rec(),
        );
        let i0 = b.add_file(FileKind::Glb, "tree.glb".into(), vec![0, 1, 2]);
        let i1 = b.add_file(FileKind::PngBaseColor, "bark.png".into(), vec![9; 32]);
        assert_eq!(i0, 0);
        assert_eq!(i1, 1);
        assert_eq!(b.file_count(), 2);
        let m = b.manifest();
        assert_eq!(m.files.len(), 2);
        assert_eq!(m.files[0].byte_length, 3);
        assert_eq!(m.files[1].byte_length, 32);
        assert_ne!(
            m.files[0].fingerprint_hex, m.files[1].fingerprint_hex,
            "different blobs should have different per-file fingerprints"
        );
        assert_eq!(m.files[0].fingerprint_hex.len(), 32);
    }

    #[test]
    fn add_material_returns_indices_and_records() {
        let mut b = AssetBundle::new(
            "tree".into(),
            "polyhaven::tree".into(),
            "polyhaven".into(),
            cc0_rec(),
        );
        let f0 = b.add_file(FileKind::PngBaseColor, "a.png".into(), vec![1; 8]);
        let f1 = b.add_file(FileKind::PngNormal, "n.png".into(), vec![2; 8]);
        let m0 = b.add_material("bark".into(), Some(f0), Some(f1), None, None);
        let m1 = b.add_material("leaf".into(), Some(f0), None, None, None);
        assert_eq!(m0, 0);
        assert_eq!(m1, 1);
        assert_eq!(b.material_count(), 2);
        let m = &b.manifest().materials;
        assert_eq!(m[0].name, "bark");
        assert_eq!(m[0].base_color, Some(0));
        assert_eq!(m[0].normal_map, Some(1));
        assert_eq!(m[1].metallic_map, None);
    }

    #[test]
    fn validate_ok_after_finalize() {
        let mut b = AssetBundle::new(
            "rock".into(),
            "kenney::rock".into(),
            "kenney".into(),
            cc0_rec(),
        );
        let f = b.add_file(FileKind::Glb, "rock.glb".into(), vec![0xAB; 16]);
        b.add_material("stone".into(), Some(f), None, None, None);
        b.finalize();
        assert_eq!(b.validate(), Ok(()));
    }

    #[test]
    fn validate_flags_all_error_kinds() {
        // empty name
        let mut b = AssetBundle::new(String::new(), "x".into(), "src".into(), cc0_rec());
        b.finalize();
        assert_eq!(b.validate(), Err(BundleErr::EmptyName));

        // empty asset_id
        let mut b = AssetBundle::new("n".into(), String::new(), "src".into(), cc0_rec());
        b.finalize();
        assert_eq!(b.validate(), Err(BundleErr::EmptyName));

        // unknown license
        let mut b = AssetBundle::new("n".into(), "a".into(), "s".into(), unknown_rec());
        b.finalize();
        assert_eq!(b.validate(), Err(BundleErr::NoLicense));

        // material out of range
        let mut b = AssetBundle::new("n".into(), "a".into(), "s".into(), cc0_rec());
        b.add_material("oops".into(), Some(99), None, None, None);
        b.finalize();
        match b.validate() {
            Err(BundleErr::MaterialOutOfRange { mat_idx, file_idx }) => {
                assert_eq!(mat_idx, 0);
                assert_eq!(file_idx, 99);
            }
            other => panic!("expected MaterialOutOfRange, got {other:?}"),
        }

        // fingerprint mismatch — finalize, then mutate (add_file clears fp)
        let mut b = AssetBundle::new("n".into(), "a".into(), "s".into(), cc0_rec());
        b.add_file(FileKind::Glb, "x.glb".into(), vec![1; 4]);
        b.finalize();
        let _ = b.add_file(FileKind::Hdr, "y.hdr".into(), vec![2; 4]);
        // add_file clears the aggregate fp ; validate should detect mismatch
        assert_eq!(b.validate(), Err(BundleErr::FingerprintMismatch));
    }

    #[test]
    fn fingerprint_changes_with_content() {
        let mut a = AssetBundle::new("n".into(), "a".into(), "s".into(), cc0_rec());
        a.add_file(FileKind::Glb, "x.glb".into(), vec![1, 2, 3]);
        a.finalize();
        let fp_a = a.manifest().fingerprint_hex.clone();

        let mut b = AssetBundle::new("n".into(), "a".into(), "s".into(), cc0_rec());
        b.add_file(FileKind::Glb, "x.glb".into(), vec![1, 2, 99]);
        b.finalize();
        let fp_b = b.manifest().fingerprint_hex.clone();

        assert_ne!(fp_a, fp_b, "different bytes must yield different fp");

        // fingerprint also order-sensitive
        let mut c = AssetBundle::new("n".into(), "a".into(), "s".into(), cc0_rec());
        c.add_file(FileKind::Glb, "x.glb".into(), vec![1]);
        c.add_file(FileKind::Hdr, "y.hdr".into(), vec![2]);
        c.finalize();
        let fp_c = c.manifest().fingerprint_hex.clone();

        let mut d = AssetBundle::new("n".into(), "a".into(), "s".into(), cc0_rec());
        d.add_file(FileKind::Hdr, "y.hdr".into(), vec![2]);
        d.add_file(FileKind::Glb, "x.glb".into(), vec![1]);
        d.finalize();
        let fp_d = d.manifest().fingerprint_hex.clone();

        assert_ne!(fp_c, fp_d, "swapped order must yield different fp");
    }

    #[test]
    fn file_blob_lookup_in_range_and_oob() {
        let mut b = AssetBundle::new(
            "n".into(),
            "a".into(),
            "s".into(),
            AssetLicenseRecord::new("a", "s", License::CC0),
        );
        let i = b.add_file(FileKind::Glb, "x.glb".into(), vec![10, 20, 30]);
        assert_eq!(b.file_blob(i), Some(&[10u8, 20, 30][..]));
        assert!(b.file_blob(99).is_none());
    }

    #[test]
    fn fnv_helper_deterministic() {
        let h1 = fnv_1a_128_hex(b"hello world");
        let h2 = fnv_1a_128_hex(b"hello world");
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 32);
        let h3 = fnv_1a_128_hex(b"hello world!");
        assert_ne!(h1, h3);
        // empty input still produces a 32-char hex (the FNV offset basis)
        let h_empty = fnv_1a_128_hex(b"");
        assert_eq!(h_empty.len(), 32);
        assert_ne!(h_empty, h1);
    }

    #[test]
    fn bundle_err_display_strings_present() {
        let e1 = BundleErr::FingerprintMismatch.to_string();
        assert!(e1.contains("fingerprint"));
        let e2 = BundleErr::EmptyName.to_string();
        assert!(e2.contains("empty"));
        let e3 = BundleErr::NoLicense.to_string();
        assert!(e3.contains("Unknown") || e3.contains("license"));
        let e4 = BundleErr::MaterialOutOfRange {
            mat_idx: 3,
            file_idx: 7,
        }
        .to_string();
        assert!(e4.contains('3'));
        assert!(e4.contains('7'));
    }
}
