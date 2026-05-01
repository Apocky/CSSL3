// § storage.rs : LabStore — directory-backed save/load/list of bundles
// ══════════════════════════════════════════════════════════════════════════
// § I> save : <label>.json (manifest, pretty) + <label>.blob.<idx> (raw bytes)
// § I> load : reads manifest first, then each .blob.<idx> in order
// § I> list : enumerates *.json in dir, returns label set
// § I> JSON-only ; LAB binary container deliberately deferred (out of scope)

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::bundle::AssetBundle;
use crate::manifest::BundleManifest;

/// Directory-backed store of asset bundles.
///
/// On-disk layout for label `"foo"` :
///   * `<dir>/foo.json`        — pretty-printed [`BundleManifest`]
///   * `<dir>/foo.blob.0`      — raw bytes of file index 0
///   * `<dir>/foo.blob.1`      — raw bytes of file index 1
///   * ...
///
/// The manifest's per-file `byte_length` + `fingerprint_hex` are the source-
/// of-truth for blob count + ordering ; load() respects them strictly.
#[derive(Clone, Debug)]
pub struct LabStore {
    /// Directory in which bundles are stored. Created on first save.
    pub dir: PathBuf,
}

impl LabStore {
    /// Construct a store rooted at `dir`. Directory is created on first save.
    pub fn new(dir: impl Into<PathBuf>) -> LabStore {
        LabStore { dir: dir.into() }
    }

    /// Persist a bundle under `label`.
    ///
    /// Returns the manifest path so callers can log + sanity-check.
    pub fn save(&self, label: &str, bundle: &AssetBundle) -> io::Result<PathBuf> {
        fs::create_dir_all(&self.dir)?;
        let manifest_path = self.dir.join(format!("{label}.json"));
        let json = serde_json::to_string_pretty(bundle.manifest())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        fs::write(&manifest_path, json)?;

        // write each blob to its own file
        for i in 0..bundle.file_count() {
            let idx = u32::try_from(i).unwrap_or(u32::MAX);
            let blob_path = self.dir.join(format!("{label}.blob.{i}"));
            let blob = bundle.file_blob(idx).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("file_blob({idx}) out-of-range while saving"),
                )
            })?;
            fs::write(&blob_path, blob)?;
        }

        Ok(manifest_path)
    }

    /// Load a previously-saved bundle by `label`.
    ///
    /// Reads `<label>.json` for the manifest, then each `<label>.blob.<i>`
    /// file in order from 0 .. manifest.files.len(). Returns an
    /// `io::Error::InvalidData` if any blob file is missing or its byte-
    /// length disagrees with the manifest entry.
    pub fn load(&self, label: &str) -> io::Result<AssetBundle> {
        let manifest_path = self.dir.join(format!("{label}.json"));
        let json = fs::read_to_string(&manifest_path)?;
        let manifest: BundleManifest = serde_json::from_str(&json)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        if !manifest.schema_compatible() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "manifest schema_version {} unsupported by build {}",
                    manifest.schema_version,
                    BundleManifest::expected_schema_version()
                ),
            ));
        }
        let mut blobs: Vec<Vec<u8>> = Vec::with_capacity(manifest.files.len());
        for (i, f_entry) in manifest.files.iter().enumerate() {
            let blob_path = self.dir.join(format!("{label}.blob.{i}"));
            let bytes = fs::read(&blob_path)?;
            let actual_len = u32::try_from(bytes.len()).unwrap_or(u32::MAX);
            if actual_len != f_entry.byte_length {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "blob {i} byte_length mismatch : manifest={} actual={}",
                        f_entry.byte_length, actual_len
                    ),
                ));
            }
            blobs.push(bytes);
        }
        Ok(AssetBundle::from_parts(manifest, blobs))
    }

    /// Enumerate labels of all bundles in the store.
    ///
    /// Looks for `*.json` files in the directory and strips the suffix.
    /// Returns an empty list if the directory does not yet exist.
    pub fn list(&self) -> io::Result<Vec<String>> {
        if !Path::new(&self.dir).exists() {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        for entry in fs::read_dir(&self.dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    out.push(stem.to_string());
                }
            }
        }
        out.sort();
        Ok(out)
    }
}

// ─────────────────────────────────────────────────────────────────
// tests
// ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::FileKind;
    use cssl_host_license_attribution::{AssetLicenseRecord, License};
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn fresh_tempdir() -> PathBuf {
        let id = TEST_DIR_COUNTER.fetch_add(1, Ordering::SeqCst);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("cssl-host-asset-bundle-test-{nanos}-{id}"));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn make_bundle(label_seed: &str) -> AssetBundle {
        let lic = AssetLicenseRecord::new(label_seed, "kenney", License::CC0);
        let mut b = AssetBundle::new(
            label_seed.to_string(),
            format!("kenney::{label_seed}"),
            "kenney".to_string(),
            lic,
        );
        let f0 = b.add_file(FileKind::Glb, "rock.glb".into(), vec![0xAB; 24]);
        let f1 = b.add_file(FileKind::PngBaseColor, "rock_bc.png".into(), vec![0xCD; 96]);
        b.add_material("stone".into(), Some(f0), Some(f1), None, None);
        b.finalize();
        b
    }

    #[test]
    fn save_then_load_roundtrip() {
        let dir = fresh_tempdir();
        let store = LabStore::new(&dir);
        let bundle = make_bundle("rock_03");
        let path = store.save("rock_03", &bundle).expect("save ok");
        assert!(path.exists());

        let loaded = store.load("rock_03").expect("load ok");
        // manifests must be byte-identical (incl. fingerprint)
        assert_eq!(loaded.manifest(), bundle.manifest());
        // blobs must match by index
        assert_eq!(loaded.file_count(), bundle.file_count());
        for i in 0..bundle.file_count() {
            let idx = u32::try_from(i).unwrap();
            assert_eq!(loaded.file_blob(idx), bundle.file_blob(idx));
        }
        // and the loaded bundle still validates against its embedded fingerprint
        assert_eq!(loaded.validate(), Ok(()));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn list_returns_saved_labels_sorted() {
        let dir = fresh_tempdir();
        let store = LabStore::new(&dir);
        // empty store on missing dir
        assert!(store.list().expect("empty list ok").is_empty());

        store.save("zeta", &make_bundle("zeta")).unwrap();
        store.save("alpha", &make_bundle("alpha")).unwrap();
        store.save("middle", &make_bundle("middle")).unwrap();

        let labels = store.list().expect("list ok");
        assert_eq!(labels, vec!["alpha".to_string(), "middle".to_string(), "zeta".to_string()]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_detects_blob_size_drift_and_missing_files() {
        let dir = fresh_tempdir();
        let store = LabStore::new(&dir);
        let bundle = make_bundle("drift");
        store.save("drift", &bundle).unwrap();

        // Tamper : truncate blob.0 — manifest byte_length disagrees with disk
        let blob0_path = dir.join("drift.blob.0");
        fs::write(&blob0_path, b"shorter").unwrap();

        let err = store.load("drift").expect_err("expected size-mismatch err");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        let msg = err.to_string();
        assert!(
            msg.contains("byte_length") || msg.contains("mismatch"),
            "err msg should mention size drift : {msg}"
        );

        // Missing file : delete blob.1 entirely
        fs::remove_file(dir.join("drift.blob.1")).unwrap();
        // restore blob.0 to satisfy that check first
        fs::write(&blob0_path, vec![0xAB; 24]).unwrap();
        let err2 = store.load("drift").expect_err("expected missing-file err");
        assert_eq!(err2.kind(), io::ErrorKind::NotFound);

        let _ = fs::remove_dir_all(&dir);
    }
}
