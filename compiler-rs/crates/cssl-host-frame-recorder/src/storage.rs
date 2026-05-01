//! § storage.rs — disk-backed LFRC recordings.
//! ══════════════════════════════════════════════════════════════════
//! § LfrcStore = directory of `<label>.lfrc` files. Operations :
//!   • `save(label, &recorder)` — encode + write `dir/<label>.lfrc`
//!   • `load(label)` — read + decode `dir/<label>.lfrc`
//!   • `list()` — directory scan for `*.lfrc` files (returns labels)
//!
//! § design notes
//!   • labels are sanitized : alphanumeric + `_-.` only ; everything
//!     else is replaced with `_` so callers can't escape the dir.
//!   • errors are `io::Error` — decode-time `LfrcErr` is folded into
//!     `io::ErrorKind::InvalidData` for a uniform store-side surface.

use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::lfrc::{decode_from_bytes, encode_to_bytes};
use crate::recorder::FrameRecorder;

/// § disk-backed LFRC recording store.
#[derive(Debug, Clone)]
pub struct LfrcStore {
    dir: PathBuf,
}

impl LfrcStore {
    /// § create a new store rooted at `dir`. Directory is NOT created
    /// eagerly — `save` calls `create_dir_all` lazily.
    #[must_use]
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// § encode `recorder` to LFRC + write `dir/<label>.lfrc`. Returns
    /// the absolute path of the written file. Label is sanitized to
    /// prevent directory escape.
    pub fn save(&self, label: &str, recorder: &FrameRecorder) -> io::Result<PathBuf> {
        let safe = sanitize_label(label);
        if safe.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "label must contain at least one alphanumeric character",
            ));
        }
        fs::create_dir_all(&self.dir)?;
        let path = self.dir.join(format!("{safe}.lfrc"));
        let bytes = encode_to_bytes(recorder);
        fs::write(&path, &bytes)?;
        Ok(path)
    }

    /// § read `dir/<label>.lfrc` + decode back into a recorder.
    pub fn load(&self, label: &str) -> io::Result<FrameRecorder> {
        let safe = sanitize_label(label);
        let path = self.dir.join(format!("{safe}.lfrc"));
        let bytes = fs::read(&path)?;
        decode_from_bytes(&bytes).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("LFRC decode failed for {}: {e}", path.display()),
            )
        })
    }

    /// § list all `*.lfrc` labels (filename stems) in the store
    /// directory. Returns an empty Vec if the directory does not yet
    /// exist (matches `save`'s lazy-create semantics).
    pub fn list(&self) -> io::Result<Vec<String>> {
        if !self.dir.exists() {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        for entry in fs::read_dir(&self.dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(OsStr::to_str) != Some("lfrc") {
                continue;
            }
            if let Some(stem) = path.file_stem().and_then(OsStr::to_str) {
                out.push(stem.to_string());
            }
        }
        out.sort();
        Ok(out)
    }

    /// § store directory.
    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }
}

fn sanitize_label(label: &str) -> String {
    label
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::{Frame, FrameKind};

    fn temp_dir(suffix: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        let pid = std::process::id();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        p.push(format!("cssl-host-frame-recorder-{pid}-{nanos}-{suffix}"));
        p
    }

    fn mk_recorder(n: u64) -> FrameRecorder {
        let mut r = FrameRecorder::new(16);
        for i in 0..n {
            let frame = Frame {
                width: 2,
                height: 2,
                ts_micros: i * 100,
                kind: FrameKind::KeyFrame,
                rgba: vec![(i as u8).wrapping_mul(13); 16],
            };
            r.push(frame);
        }
        r
    }

    #[test]
    fn save_then_load_roundtrip() {
        let dir = temp_dir("roundtrip");
        let store = LfrcStore::new(&dir);
        let r = mk_recorder(3);
        let path = store.save("clip-a", &r).expect("save");
        assert!(path.exists());
        let back = store.load("clip-a").expect("load");
        assert_eq!(back.frame_count(), 3);
        for (i, f) in back.snapshot().iter().enumerate() {
            assert_eq!(f.ts_micros, (i as u64) * 100);
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn list_returns_multiple_labels_sorted() {
        let dir = temp_dir("list-multi");
        let store = LfrcStore::new(&dir);
        let r = mk_recorder(1);
        store.save("zulu", &r).unwrap();
        store.save("alpha", &r).unwrap();
        store.save("mike", &r).unwrap();
        let labels = store.list().expect("list");
        assert_eq!(labels, vec!["alpha", "mike", "zulu"]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_missing_errors() {
        let dir = temp_dir("missing");
        fs::create_dir_all(&dir).unwrap();
        let store = LfrcStore::new(&dir);
        let err = store.load("does-not-exist").expect_err("must error");
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn list_empty_dir_returns_empty() {
        let dir = temp_dir("empty-listing");
        let store = LfrcStore::new(&dir);
        // dir doesn't exist yet — list must still succeed
        let labels = store.list().expect("list");
        assert!(labels.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn label_sanitization_blocks_path_escape() {
        let dir = temp_dir("sanitize");
        let store = LfrcStore::new(&dir);
        let r = mk_recorder(1);
        // attempted escape : "../evil/clip"
        let path = store.save("../evil/clip", &r).expect("save sanitized");
        // resulting filename should be flat under dir, no escape.
        assert_eq!(path.parent().unwrap(), dir.as_path());
        assert_eq!(
            path.extension().and_then(OsStr::to_str),
            Some("lfrc"),
            "sanitized save must produce a .lfrc file"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_label_rejected() {
        let dir = temp_dir("empty-label");
        let store = LfrcStore::new(&dir);
        let r = mk_recorder(1);
        let err = store.save("", &r).expect_err("empty must reject");
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_file_surfaces_invalid_data() {
        let dir = temp_dir("corrupt");
        fs::create_dir_all(&dir).unwrap();
        let store = LfrcStore::new(&dir);
        // write garbage bytes under the right name + extension
        fs::write(dir.join("trash.lfrc"), b"NOTLFRC").unwrap();
        let err = store.load("trash").expect_err("garbage must reject");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        let _ = fs::remove_dir_all(&dir);
    }
}
