//! § store — disk-backed AttestationStore for SessionAttestation artifacts.
//! ════════════════════════════════════════════════════════════════════
//!
//! [`AttestationStore`] persists [`SessionAttestation`] values under a
//! caller-provided directory. Each `save` writes three artifacts :
//!
//! ```text
//!     <dir>/<session_id>.attestation.json   ← canonical machine-readable
//!     <dir>/<session_id>.attestation.csl    ← CSLv3-glyph human report
//!     <dir>/<session_id>.attestation.txt    ← plain-text human report
//! ```
//!
//! The `.json` artifact is the canonical store ; `load` only reads that
//! file. The `.csl` and `.txt` siblings are derivable but written
//! eagerly so consumers (auditors, CI checks) can grep them without
//! pulling the JSON parser into their toolchain.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::attestation::SessionAttestation;
use crate::report::{render_csl_native, render_json_pretty, render_text};

/// Disk-backed attestation store.
///
/// One `AttestationStore` is constructed per output directory.
/// The directory is created on the first `save` call ; existence is
/// not asserted at construction time so callers can pre-build a
/// store under a path that doesn't exist yet.
#[derive(Debug, Clone)]
pub struct AttestationStore {
    dir: PathBuf,
}

impl AttestationStore {
    /// Construct a store rooted at `dir`. The directory is NOT created
    /// here ; it is created lazily on the first `save` call.
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// Borrow the root directory.
    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Serialize `att` to disk under `<dir>/<session_id>.attestation.{json,csl,txt}`.
    /// Returns the path of the canonical `.json` artifact.
    ///
    /// The `session_id` is required ; saving an attestation with an
    /// empty `session_id` returns `io::ErrorKind::InvalidInput`.
    pub fn save(&self, att: &SessionAttestation) -> io::Result<PathBuf> {
        if att.session_id.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "session_id must not be empty",
            ));
        }
        fs::create_dir_all(&self.dir)?;
        let json_path = self.path_for(&att.session_id, "json");
        let csl_path = self.path_for(&att.session_id, "csl");
        let txt_path = self.path_for(&att.session_id, "txt");

        fs::write(&json_path, render_json_pretty(att))?;
        fs::write(&csl_path, render_csl_native(att))?;
        fs::write(&txt_path, render_text(att))?;

        Ok(json_path)
    }

    /// Load `<dir>/<session_id>.attestation.json` and parse to a
    /// [`SessionAttestation`].
    pub fn load(&self, session_id: &str) -> io::Result<SessionAttestation> {
        let path = self.path_for(session_id, "json");
        let bytes = fs::read(&path)?;
        serde_json::from_slice(&bytes).map_err(|e| {
            io::Error::new(io::ErrorKind::InvalidData, format!("parse {path:?}: {e}"))
        })
    }

    /// List all session identifiers visible in the store directory by
    /// inspecting `*.attestation.json` filenames. Returns an empty
    /// `Vec` when the directory does not exist. Output order is
    /// filesystem-dependent ; callers needing stable order should
    /// sort.
    pub fn list_sessions(&self) -> io::Result<Vec<String>> {
        let mut out = Vec::new();
        let read_dir = match fs::read_dir(&self.dir) {
            Ok(rd) => rd,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(out),
            Err(e) => return Err(e),
        };
        for entry in read_dir {
            let entry = entry?;
            if let Some(name) = entry.file_name().to_str() {
                if let Some(prefix) = name.strip_suffix(".attestation.json") {
                    out.push(prefix.to_string());
                }
            }
        }
        Ok(out)
    }

    fn path_for(&self, session_id: &str, ext: &str) -> PathBuf {
        self.dir.join(format!("{session_id}.attestation.{ext}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attestation::{aggregate, AttestationVerdict};
    use cssl_host_audit::{AuditLevel, AuditRow, AuditSource};

    fn make_session(id: &str) -> SessionAttestation {
        let r = AuditRow {
            ts_iso: "2026-04-30T01:00:00Z".into(),
            ts_micros: 1,
            source: AuditSource::Runtime,
            level: AuditLevel::Info,
            kind: "frame.tick".into(),
            message: String::new(),
            sovereign_cap_used: false,
            kv: vec![],
        };
        aggregate(&[r]).with_session_id(id)
    }

    fn unique_dir(tag: &str) -> PathBuf {
        // Use std::time + thread-id to keep multi-thread cargo-test runs
        // from colliding ; the temp dir is namespaced under the OS temp
        // root to avoid touching the worktree.
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.subsec_nanos());
        let tid = format!("{:?}", std::thread::current().id());
        let tid_clean: String = tid.chars().filter(char::is_ascii_alphanumeric).collect();
        std::env::temp_dir().join(format!("cssl-host-attestation-{tag}-{tid_clean}-{nanos}"))
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = unique_dir("save-load");
        let store = AttestationStore::new(&dir);
        let att = make_session("sess-A");
        let json_path = store.save(&att).expect("save");
        assert!(json_path.exists());
        // Sibling files exist too.
        assert!(dir.join("sess-A.attestation.csl").exists());
        assert!(dir.join("sess-A.attestation.txt").exists());
        let back = store.load("sess-A").expect("load");
        assert_eq!(back, att);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn list_sessions_returns_all_ids() {
        let dir = unique_dir("list-multi");
        let store = AttestationStore::new(&dir);
        store.save(&make_session("sess-A")).expect("save A");
        store.save(&make_session("sess-B")).expect("save B");
        store.save(&make_session("sess-C")).expect("save C");
        let mut ids = store.list_sessions().expect("list");
        ids.sort();
        assert_eq!(ids, vec!["sess-A", "sess-B", "sess-C"]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_load_returns_io_error() {
        let dir = unique_dir("missing-load");
        let store = AttestationStore::new(&dir);
        let err = store.load("does-not-exist").unwrap_err();
        assert!(matches!(
            err.kind(),
            io::ErrorKind::NotFound | io::ErrorKind::InvalidData
        ));
    }

    #[test]
    fn round_trip_via_disk_preserves_verdict() {
        let dir = unique_dir("verdict");
        let store = AttestationStore::new(&dir);
        let att = make_session("sess-V");
        assert_eq!(att.verdict, AttestationVerdict::Clean);
        store.save(&att).expect("save");
        let back = store.load("sess-V").expect("load");
        assert_eq!(back.verdict, AttestationVerdict::Clean);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_session_id_save_errors() {
        let dir = unique_dir("empty-id");
        let store = AttestationStore::new(&dir);
        let mut att = make_session("temp");
        att.session_id = String::new();
        let err = store.save(&att).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
    }
}
