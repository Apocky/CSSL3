// § golden.rs : on-disk golden store + verdict pipeline
// ══════════════════════════════════════════════════════════════════
// § I> GoldenStore wraps a directory of <label>.rgba + <label>.meta.json
// § I> compare returns GoldenCompare with verdict ∈ {Match, MinorDrift, MajorRegression, NoGolden, DimensionMismatch}
// § I> verdict thresholds : MinorDrift @ ≤ 1% pixels_above_tolerance ; else MajorRegression

use crate::diff::{diff_rgba, DiffReport};
use crate::snapshot::Snapshot;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::PathBuf;

const MINOR_DRIFT_PERCENT: f32 = 1.0;

/// Directory-backed store of golden snapshots.
#[derive(Clone, Debug)]
pub struct GoldenStore {
    pub dir: PathBuf,
}

impl GoldenStore {
    /// Construct a store rooted at `dir`. Directory is created on first save.
    pub fn new(dir: impl Into<PathBuf>) -> GoldenStore {
        GoldenStore { dir: dir.into() }
    }

    /// Persist a snapshot as the golden for `label`.
    ///
    /// Writes `<dir>/<label>.rgba` (raw bytes) and `<dir>/<label>.meta.json`
    /// (snapshot metadata sans pixel buffer).
    pub fn save_golden(&self, label: &str, snap: &Snapshot) -> io::Result<()> {
        fs::create_dir_all(&self.dir)?;
        let rgba_path = self.dir.join(format!("{label}.rgba"));
        let meta_path = self.dir.join(format!("{label}.meta.json"));
        fs::write(&rgba_path, &snap.rgba)?;
        let meta = SnapshotMeta {
            width: snap.width,
            height: snap.height,
            sha256_hex: snap.sha256_hex.clone(),
            ts_iso: snap.ts_iso.clone(),
            label: snap.label.clone(),
        };
        let json = serde_json::to_string_pretty(&meta)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        fs::write(&meta_path, json)?;
        Ok(())
    }

    /// Load the golden snapshot for `label` from disk.
    pub fn load_golden(&self, label: &str) -> io::Result<Snapshot> {
        let rgba_path = self.dir.join(format!("{label}.rgba"));
        let meta_path = self.dir.join(format!("{label}.meta.json"));
        let rgba = fs::read(&rgba_path)?;
        let meta_json = fs::read_to_string(&meta_path)?;
        let meta: SnapshotMeta = serde_json::from_str(&meta_json)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        Ok(Snapshot {
            width: meta.width,
            height: meta.height,
            rgba,
            sha256_hex: meta.sha256_hex,
            ts_iso: meta.ts_iso,
            label: meta.label,
        })
    }

    /// Compare `candidate` against the stored golden for `label`.
    ///
    /// Verdict resolution order :
    ///   1. golden absent → `NoGolden`
    ///   2. dims differ → `DimensionMismatch`
    ///   3. fingerprints match → `Match` (skip pixel diff)
    ///   4. percent_diff ≤ MINOR_DRIFT_PERCENT → `MinorDrift`
    ///   5. else → `MajorRegression`
    pub fn compare(
        &self,
        label: &str,
        candidate: &Snapshot,
        tolerance: u8,
    ) -> io::Result<GoldenCompare> {
        let golden = match self.load_golden(label) {
            Ok(g) => g,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Ok(GoldenCompare {
                    golden_present: false,
                    dimensions_match: false,
                    fingerprint_match: false,
                    diff: None,
                    verdict: GoldenVerdict::NoGolden,
                });
            }
            Err(e) => return Err(e),
        };

        let dims_ok = golden.width == candidate.width && golden.height == candidate.height;
        if !dims_ok {
            return Ok(GoldenCompare {
                golden_present: true,
                dimensions_match: false,
                fingerprint_match: false,
                diff: None,
                verdict: GoldenVerdict::DimensionMismatch,
            });
        }

        let fp_match = golden.sha256_hex == candidate.sha256_hex;
        if fp_match {
            return Ok(GoldenCompare {
                golden_present: true,
                dimensions_match: true,
                fingerprint_match: true,
                diff: None,
                verdict: GoldenVerdict::Match,
            });
        }

        let diff = diff_rgba(&golden.rgba, &candidate.rgba, tolerance)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        let verdict = classify(&diff);
        Ok(GoldenCompare {
            golden_present: true,
            dimensions_match: true,
            fingerprint_match: false,
            diff: Some(diff),
            verdict,
        })
    }
}

/// Classify a diff report into a verdict.
fn classify(d: &DiffReport) -> GoldenVerdict {
    if d.pixels_above_tolerance == 0 {
        // pixels-diff exists but all under tolerance ⇒ effectively a match
        GoldenVerdict::Match
    } else if d.percent_diff <= MINOR_DRIFT_PERCENT {
        GoldenVerdict::MinorDrift
    } else {
        GoldenVerdict::MajorRegression
    }
}

/// Outcome of comparing a candidate against its golden.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GoldenCompare {
    pub golden_present: bool,
    pub dimensions_match: bool,
    pub fingerprint_match: bool,
    pub diff: Option<DiffReport>,
    pub verdict: GoldenVerdict,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GoldenVerdict {
    Match,
    MinorDrift,
    MajorRegression,
    NoGolden,
    DimensionMismatch,
}

#[derive(Serialize, Deserialize)]
struct SnapshotMeta {
    width: u32,
    height: u32,
    sha256_hex: String,
    ts_iso: String,
    label: String,
}

// ─────────────────────────────────────────────────────────────────
// tests
// ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::from_rgba;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn fresh_tempdir() -> PathBuf {
        let id = TEST_DIR_COUNTER.fetch_add(1, Ordering::SeqCst);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir()
            .join(format!("cssl-host-golden-test-{nanos}-{id}"));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = fresh_tempdir();
        let store = GoldenStore::new(&dir);
        let rgba = (0..16).map(|i: u8| i * 4).collect::<Vec<_>>();
        let snap = from_rgba("scene-A".to_string(), rgba, 2, 2).unwrap();
        store.save_golden("scene-A", &snap).expect("save ok");
        let loaded = store.load_golden("scene-A").expect("load ok");
        assert_eq!(loaded.width, snap.width);
        assert_eq!(loaded.height, snap.height);
        assert_eq!(loaded.rgba, snap.rgba);
        assert_eq!(loaded.sha256_hex, snap.sha256_hex);
        assert_eq!(loaded.label, snap.label);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn compare_identical_yields_match() {
        let dir = fresh_tempdir();
        let store = GoldenStore::new(&dir);
        let rgba = vec![42u8; 4 * 4 * 4];
        let snap = from_rgba("scene-B".to_string(), rgba.clone(), 4, 4).unwrap();
        store.save_golden("scene-B", &snap).unwrap();
        let candidate = from_rgba("scene-B".to_string(), rgba, 4, 4).unwrap();
        let cmp = store.compare("scene-B", &candidate, 0).unwrap();
        assert_eq!(cmp.verdict, GoldenVerdict::Match);
        assert!(cmp.golden_present);
        assert!(cmp.dimensions_match);
        assert!(cmp.fingerprint_match);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn compare_mismatch_yields_regression() {
        let dir = fresh_tempdir();
        let store = GoldenStore::new(&dir);
        let golden_rgba = vec![0u8; 4 * 4 * 4];
        let snap = from_rgba("scene-C".to_string(), golden_rgba, 4, 4).unwrap();
        store.save_golden("scene-C", &snap).unwrap();
        // candidate : every pixel maximally different
        let cand_rgba = vec![255u8; 4 * 4 * 4];
        let candidate = from_rgba("scene-C".to_string(), cand_rgba, 4, 4).unwrap();
        let cmp = store.compare("scene-C", &candidate, 0).unwrap();
        assert_eq!(cmp.verdict, GoldenVerdict::MajorRegression);
        assert!(!cmp.fingerprint_match);
        assert!(cmp.diff.is_some());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn no_golden_yields_no_golden() {
        let dir = fresh_tempdir();
        let store = GoldenStore::new(&dir);
        let candidate = from_rgba("missing".to_string(), vec![0u8; 4], 1, 1).unwrap();
        let cmp = store.compare("missing", &candidate, 0).unwrap();
        assert_eq!(cmp.verdict, GoldenVerdict::NoGolden);
        assert!(!cmp.golden_present);
    }

    #[test]
    fn dim_mismatch_detected() {
        let dir = fresh_tempdir();
        let store = GoldenStore::new(&dir);
        let golden = from_rgba("dim".to_string(), vec![0u8; 16], 2, 2).unwrap();
        store.save_golden("dim", &golden).unwrap();
        let cand = from_rgba("dim".to_string(), vec![0u8; 4 * 4 * 4], 4, 4).unwrap();
        let cmp = store.compare("dim", &cand, 0).unwrap();
        assert_eq!(cmp.verdict, GoldenVerdict::DimensionMismatch);
        assert!(!cmp.dimensions_match);
        let _ = fs::remove_dir_all(&dir);
    }
}
