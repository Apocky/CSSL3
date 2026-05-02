//! § apply — atomic bundle application + rollback.
//!
//! § DISK LAYOUT
//! ```text
//! <install_dir>/
//!     <channel-name>/
//!         current        ← payload bytes of the active version (file)
//!         current.meta   ← JSON {version, applied_at_ns, blake3_hex}
//!     .history/
//!         <channel-name>/
//!             <ver>.payload   (N-1, N-2 preserved · older pruned)
//!             <ver>.meta
//! ```
//!
//! § APPLY FLOW (atomic)
//!   1. write payload to `<dir>/<channel>/.staged.payload`
//!   2. write meta to    `<dir>/<channel>/.staged.meta`
//!   3. if `current` exists, snapshot it to `.history/<channel>/<old>.payload`
//!   4. rename `.staged.payload` → `current`     (atomic on most filesystems)
//!   5. rename `.staged.meta`    → `current.meta`
//!   6. prune `.history/<channel>/` to N-1 + N-2
//!   Return `AppliedSnapshot` carrying enough state to fully revert.
//!
//! § ROLLBACK FLOW
//!   1. read `<channel>.history/<prior_version>.payload`
//!   2. atomic-rename current → `.history/<channel>/<failed>.payload`
//!   3. atomic-rename `.history/<channel>/<prior>.payload` → current
//!   4. write new meta
//!   5. emit telemetry event `hotfix.rolled_back`
//!
//! § VERIFICATION GATE
//!   `apply_bundle` is INFALLIBLE without a `VerifyOk` token : the type
//!   signature requires you to call `verify::verify_bundle` first. This
//!   makes the no-DRM-but-cryptographically-grounded design enforceable
//!   at compile time.

use crate::bundle::Bundle;
use crate::channel::Channel;
use crate::verify::VerifyOk;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// § A snapshot of the state-before-apply, used to roll back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppliedSnapshot {
    pub channel: Channel,
    pub new_version: String,
    pub prior_version: Option<String>,
    pub applied_at_ns: u64,
    /// Path to the prior payload backup (in `.history/`). `None` if this
    /// was a fresh install with no prior version.
    pub prior_payload_path: Option<PathBuf>,
    pub install_dir: PathBuf,
}

/// On-disk meta for the active version of a channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChannelMeta {
    version: String,
    applied_at_ns: u64,
    blake3_hex: String,
}

#[derive(Debug, Error)]
pub enum ApplyError {
    #[error("io error : {0}")]
    Io(String),
    #[error("install dir does not exist : {0}")]
    NoInstallDir(PathBuf),
    #[error("serde error : {0}")]
    Serde(String),
}

#[derive(Debug, Error)]
pub enum RollbackError {
    #[error("no prior version on disk for channel {0}")]
    NoPriorVersion(&'static str),
    #[error("io error : {0}")]
    Io(String),
    #[error("serde error : {0}")]
    Serde(String),
}

impl From<io::Error> for ApplyError {
    fn from(e: io::Error) -> Self {
        Self::Io(e.to_string())
    }
}
impl From<io::Error> for RollbackError {
    fn from(e: io::Error) -> Self {
        Self::Io(e.to_string())
    }
}

const HISTORY_KEEP: usize = 2;

fn channel_dir(install_dir: &Path, ch: Channel) -> PathBuf {
    install_dir.join(ch.name())
}
fn history_dir(install_dir: &Path, ch: Channel) -> PathBuf {
    install_dir.join(".history").join(ch.name())
}
fn version_str(major: u16, minor: u16, patch: u16) -> String {
    format!("{major}.{minor}.{patch}")
}
fn hex32(b: &[u8; 32]) -> String {
    crate::hex_lower(b)
}

/// § Atomically apply a verified bundle to the install dir.
///
/// The `_verified` parameter is a type-level proof that
/// `verify::verify_bundle` returned `Ok` for this bundle. Callers cannot
/// fabricate a `VerifyOk` outside this crate (its inner is private), so
/// the only path to apply is verify-then-apply.
pub fn apply_bundle(
    bundle: &Bundle,
    install_dir: &Path,
    applied_at_ns: u64,
    _verified: VerifyOk,
) -> Result<AppliedSnapshot, ApplyError> {
    if !install_dir.exists() {
        return Err(ApplyError::NoInstallDir(install_dir.to_path_buf()));
    }

    let ch = bundle.header.channel;
    let new_version = version_str(
        bundle.header.ver_major,
        bundle.header.ver_minor,
        bundle.header.ver_patch,
    );

    let cdir = channel_dir(install_dir, ch);
    fs::create_dir_all(&cdir)?;
    let hdir = history_dir(install_dir, ch);
    fs::create_dir_all(&hdir)?;

    // Stage.
    let staged_payload = cdir.join(".staged.payload");
    let staged_meta = cdir.join(".staged.meta");
    fs::write(&staged_payload, &bundle.payload)?;
    let meta = ChannelMeta {
        version: new_version.clone(),
        applied_at_ns,
        blake3_hex: hex32(&bundle.header.payload_blake3),
    };
    let meta_json = serde_json::to_string_pretty(&meta).map_err(|e| ApplyError::Serde(e.to_string()))?;
    fs::write(&staged_meta, meta_json)?;

    // Snapshot prior, if any.
    let current_payload = cdir.join("current");
    let current_meta = cdir.join("current.meta");
    let (prior_version, prior_payload_path) = if current_payload.exists() {
        let prior_meta_str = fs::read_to_string(&current_meta).unwrap_or_default();
        let prior_ver = serde_json::from_str::<ChannelMeta>(&prior_meta_str)
            .map_or_else(|_| "unknown".to_string(), |m| m.version);
        let backup = hdir.join(format!("{prior_ver}.payload"));
        let backup_meta = hdir.join(format!("{prior_ver}.meta"));
        // Use rename for atomic preservation.
        fs::rename(&current_payload, &backup)?;
        // current.meta may not exist on legacy installs.
        if current_meta.exists() {
            fs::rename(&current_meta, &backup_meta)?;
        }
        (Some(prior_ver), Some(backup))
    } else {
        (None, None)
    };

    // Promote staged → current.
    fs::rename(&staged_payload, &current_payload)?;
    fs::rename(&staged_meta, &current_meta)?;

    // Prune history beyond N-1 + N-2.
    prune_history(&hdir, HISTORY_KEEP)?;

    Ok(AppliedSnapshot {
        channel: ch,
        new_version,
        prior_version,
        applied_at_ns,
        prior_payload_path,
        install_dir: install_dir.to_path_buf(),
    })
}

/// § Roll back the most recent apply for the given snapshot's channel.
///
/// Restores the prior-version payload from `.history/`, moves the
/// failed-apply payload aside as `<failed>.payload` for forensics, and
/// rewrites `current.meta` accordingly.
pub fn rollback(snapshot: &AppliedSnapshot, ts_ns: u64) -> Result<(), RollbackError> {
    let prior = snapshot
        .prior_payload_path
        .as_ref()
        .ok_or(RollbackError::NoPriorVersion(snapshot.channel.name()))?;
    let prior_version = snapshot
        .prior_version
        .clone()
        .ok_or(RollbackError::NoPriorVersion(snapshot.channel.name()))?;

    let cdir = channel_dir(&snapshot.install_dir, snapshot.channel);
    let hdir = history_dir(&snapshot.install_dir, snapshot.channel);
    fs::create_dir_all(&hdir)?;

    let current_payload = cdir.join("current");
    let current_meta = cdir.join("current.meta");

    // Move failed payload to history (so we can debug it later).
    if current_payload.exists() {
        let failed_path = hdir.join(format!("{}.failed.payload", snapshot.new_version));
        fs::rename(&current_payload, &failed_path)?;
    }
    if current_meta.exists() {
        let failed_meta = hdir.join(format!("{}.failed.meta", snapshot.new_version));
        fs::rename(&current_meta, &failed_meta)?;
    }

    // Restore prior payload.
    fs::rename(prior, &current_payload)?;

    // Rewrite current.meta. We don't know the prior payload blake3 by here
    // without reading bytes ; cheap : read the restored file & hash it.
    let bytes = fs::read(&current_payload)?;
    let blake = hex32(blake3::hash(&bytes).as_bytes());
    let meta = ChannelMeta {
        version: prior_version,
        applied_at_ns: ts_ns,
        blake3_hex: blake,
    };
    let meta_json = serde_json::to_string_pretty(&meta).map_err(|e| RollbackError::Serde(e.to_string()))?;
    fs::write(&current_meta, meta_json)?;

    Ok(())
}

fn prune_history(hdir: &Path, keep: usize) -> Result<(), ApplyError> {
    if !hdir.exists() {
        return Ok(());
    }
    let mut payloads: Vec<(std::time::SystemTime, PathBuf)> = vec![];
    for entry in fs::read_dir(hdir)? {
        let e = entry?;
        let p = e.path();
        // Only consider .payload files (skip .meta + .failed.*).
        if p.extension().and_then(|s| s.to_str()) == Some("payload") {
            // Skip .failed.payload — preserved for forensics.
            let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if !name.contains(".failed.") {
                let mtime = e.metadata()?.modified()?;
                payloads.push((mtime, p));
            }
        }
    }
    // Sort newest first.
    payloads.sort_by(|a, b| b.0.cmp(&a.0));
    for (_, p) in payloads.into_iter().skip(keep) {
        // Also drop the matching .meta if present.
        let meta = p.with_extension("meta");
        let _ = fs::remove_file(&meta);
        fs::remove_file(p)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bundle::{BundleHeader, BUNDLE_FORMAT_VERSION};
    use crate::cap::CapRole;
    use crate::verify::verify_ok_for_test;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_dir(tag: &str) -> PathBuf {
        let n = TMP_COUNTER.fetch_add(1, Ordering::SeqCst);
        let pid = std::process::id();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        let p = std::env::temp_dir().join(format!("cssl-hotfix-{tag}-{pid}-{nanos}-{n}"));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn make_bundle(channel: Channel, cap: CapRole, ver: (u16, u16, u16), payload: &[u8]) -> Bundle {
        let header = BundleHeader {
            format_version: BUNDLE_FORMAT_VERSION,
            channel,
            cap_role: cap,
            ver_major: ver.0,
            ver_minor: ver.1,
            ver_patch: ver.2,
            timestamp_ns: 0,
            payload_size: payload.len() as u64,
            payload_blake3: *blake3::hash(payload).as_bytes(),
        };
        Bundle {
            header,
            payload: payload.to_vec(),
            signature: [0u8; 64],
        }
    }

    #[test]
    fn apply_creates_current_payload_and_meta() {
        let dir = temp_dir("apply-create");
        let bundle = make_bundle(Channel::CsslBundle, CapRole::CapB, (1, 0, 0), b"hello");
        let snap = apply_bundle(&bundle, &dir, 1, verify_ok_for_test()).unwrap();
        let cur = dir.join(Channel::CsslBundle.name()).join("current");
        assert!(cur.exists());
        let bytes = fs::read(cur).unwrap();
        assert_eq!(bytes, b"hello");
        assert_eq!(snap.new_version, "1.0.0");
        assert!(snap.prior_version.is_none());
    }

    #[test]
    fn apply_then_replace_preserves_prior_in_history() {
        let dir = temp_dir("apply-replace");
        let v1 = make_bundle(Channel::CsslBundle, CapRole::CapB, (1, 0, 0), b"v1-data");
        let v2 = make_bundle(Channel::CsslBundle, CapRole::CapB, (1, 0, 1), b"v2-data");
        apply_bundle(&v1, &dir, 1, verify_ok_for_test()).unwrap();
        let snap = apply_bundle(&v2, &dir, 2, verify_ok_for_test()).unwrap();
        assert_eq!(snap.prior_version.as_deref(), Some("1.0.0"));
        let cur = fs::read(dir.join(Channel::CsslBundle.name()).join("current")).unwrap();
        assert_eq!(cur, b"v2-data");
        let prior = fs::read(snap.prior_payload_path.as_ref().unwrap()).unwrap();
        assert_eq!(prior, b"v1-data");
    }

    #[test]
    fn rollback_restores_prior_payload() {
        let dir = temp_dir("rollback");
        let v1 = make_bundle(Channel::CsslBundle, CapRole::CapB, (1, 0, 0), b"v1-data");
        let v2 = make_bundle(Channel::CsslBundle, CapRole::CapB, (1, 0, 1), b"v2-data");
        apply_bundle(&v1, &dir, 1, verify_ok_for_test()).unwrap();
        let snap = apply_bundle(&v2, &dir, 2, verify_ok_for_test()).unwrap();
        rollback(&snap, 3).unwrap();
        let cur = fs::read(dir.join(Channel::CsslBundle.name()).join("current")).unwrap();
        assert_eq!(cur, b"v1-data");
    }

    #[test]
    fn rollback_with_no_prior_errors() {
        let dir = temp_dir("rollback-none");
        let v1 = make_bundle(Channel::CsslBundle, CapRole::CapB, (1, 0, 0), b"v1");
        let snap = apply_bundle(&v1, &dir, 1, verify_ok_for_test()).unwrap();
        let r = rollback(&snap, 2);
        assert!(matches!(r, Err(RollbackError::NoPriorVersion(_))));
    }

    #[test]
    fn history_pruned_to_n_minus_two() {
        let dir = temp_dir("prune");
        for v in 0u16..6 {
            let b = make_bundle(
                Channel::CsslBundle,
                CapRole::CapB,
                (1, 0, v),
                &v.to_le_bytes(),
            );
            apply_bundle(&b, &dir, v as u64, verify_ok_for_test()).unwrap();
            // tiny sleep so mtime ordering is reliable on fast filesystems
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let hdir = dir.join(".history").join(Channel::CsslBundle.name());
        let payload_count = fs::read_dir(&hdir)
            .unwrap()
            .filter_map(std::result::Result::ok)
            .filter(|e| {
                e.path().extension().and_then(|s| s.to_str()) == Some("payload")
                    && !e
                        .path()
                        .file_name()
                        .and_then(|s| s.to_str())
                        .is_some_and(|s| s.contains(".failed."))
            })
            .count();
        assert!(
            payload_count <= HISTORY_KEEP,
            "history must be pruned to {HISTORY_KEEP} entries, got {payload_count}"
        );
    }

    #[test]
    fn apply_no_install_dir_errors() {
        let bundle = make_bundle(Channel::CsslBundle, CapRole::CapB, (1, 0, 0), b"x");
        let bogus = std::env::temp_dir().join("does-not-exist-xyz-cssl-hotfix");
        let _ = fs::remove_dir_all(&bogus);
        let r = apply_bundle(&bundle, &bogus, 0, verify_ok_for_test());
        assert!(matches!(r, Err(ApplyError::NoInstallDir(_))));
    }
}
