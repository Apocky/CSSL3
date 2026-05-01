//! § wired_golden — thin loa-host wrapper around `cssl-host-golden`.
//!
//! § T11-W5c-LOA-HOST-WIRE
//!   Re-exports the visual-regression snapshot + diff + golden-store types
//!   so MCP tools can list golden labels stored in a directory without
//!   reaching into the path-dep at every call-site.
//!
//! § wrapped surface
//!   - [`Snapshot`] / [`from_rgba`] / [`SnapErr`] — pixel-buffer wrappers.
//!   - [`GoldenStore`] / [`GoldenCompare`] / [`GoldenVerdict`] — disk store
//!     + comparison results.
//!   - [`diff_rgba`] / [`diff_rgb`] / [`DiffReport`] — pixel diff math.
//!
//! § ATTESTATION ¬ harm — wrapper is a re-export shim ; reads-only over disk.

pub use cssl_host_golden::{
    diff_rgb, diff_rgba, from_rgba, render_text, CampaignReport, DiffErr, DiffReport,
    GoldenCompare, GoldenStore, GoldenVerdict, RunResult, SnapErr, Snapshot,
};

/// Convenience : list the snapshot labels (file-stems) under a golden dir.
/// Reads each `*.png` / `*.snapshot.json` filename and strips the extension ;
/// missing-dir returns an empty vec rather than erroring (matches the
/// "no panics in library code" contract).
pub fn list_labels(dir: impl AsRef<std::path::Path>) -> Vec<String> {
    let dir = dir.as_ref();
    let mut out: Vec<String> = Vec::new();
    let Ok(rd) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in rd.flatten() {
        let p = entry.path();
        if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
            // Strip well-known double-extension `.snapshot` so a label
            // appears once per asset (rather than once per side-file).
            let label = stem.trim_end_matches(".snapshot").to_string();
            if !out.contains(&label) {
                out.push(label);
            }
        }
    }
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_labels_missing_dir_returns_empty() {
        let labels = list_labels("non-existent-loa-host-wired-golden-path-xyz");
        assert!(labels.is_empty());
    }

    #[test]
    fn re_exports_compile() {
        // Construct types to confirm re-exports are usable downstream.
        let _store = GoldenStore::new(std::env::temp_dir());
        // Tiny 1×1 RGBA snapshot.
        let snap = from_rgba(
            "probe".to_string(),
            vec![255, 0, 0, 255],
            1,
            1,
        )
        .expect("snapshot ok");
        assert_eq!(snap.width, 1);
    }
}
