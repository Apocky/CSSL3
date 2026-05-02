//! § client — the HotfixClient orchestrator.
//!
//! § ENTRY POINTS
//!   • `HotfixClient::new` — construct from config + sources + sink.
//!   • `HotfixClient::poll_once` — single sync poll cycle (no internal sleeping).
//!   • engine integrators wrap this in their own thread / tick-loop.
//!
//! § FLOW (per `poll_once`)
//!   1. fetch_manifest → verify_manifest → emit Checked.
//!   2. for each Channel where SigmaPolicy.allows_download :
//!      a. read installed-version meta from disk (None if fresh install).
//!      b. if manifest's version > installed → schedule update.
//!      c. if installed-version is on manifest's revocation-list →
//!         schedule rollback.
//!   3. for each scheduled update : fetch bundle bytes, parse, verify,
//!      apply (or skip/prompt per consent).
//!   4. emit cumulative PollReport.

use crate::sources::{BundleSource, ManifestSource};
use crate::telemetry::{HotfixEvent, TelemetrySink};
use cssl_hotfix::apply::{apply_bundle, rollback, AppliedSnapshot};
use cssl_hotfix::bundle::Bundle;
use cssl_hotfix::cap::CapKey;
use cssl_hotfix::channel::{Channel, CHANNELS};
use cssl_hotfix::manifest::Manifest;
use cssl_hotfix::sigma::{SigmaPolicy, UpdateConsent};
use cssl_hotfix::verify::{check_not_revoked, verify_bundle, verify_manifest};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// § Client config.
#[derive(Debug, Clone)]
pub struct HotfixClientConfig {
    pub install_dir: PathBuf,
    pub poll_interval_ms: u64,
    /// Compiled-in cap-key public-keys.
    pub cap_keys: Vec<CapKey>,
}

/// § Outcome for one channel in one poll cycle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PollOutcome {
    NoUpdate,
    Skipped { reason: String },
    Updated { from: Option<String>, to: String },
    PromptPending { version: String },
    RolledBack { from: String, to: Option<String> },
    Failed { error: String },
}

/// § Per-poll-cycle report. The engine integrator may surface this in UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PollReport {
    pub ts_ns: u64,
    pub manifest_generated_at_ns: u64,
    pub per_channel: BTreeMap<Channel, PollOutcome>,
}

/// § The orchestrator.
pub struct HotfixClient {
    cfg: HotfixClientConfig,
    manifest_src: Arc<dyn ManifestSource>,
    bundle_src: Arc<dyn BundleSource>,
    sink: Arc<dyn TelemetrySink>,
    sigma: Arc<Mutex<SigmaPolicy>>,
}

impl HotfixClient {
    pub fn new(
        cfg: HotfixClientConfig,
        manifest_src: Arc<dyn ManifestSource>,
        bundle_src: Arc<dyn BundleSource>,
        sink: Arc<dyn TelemetrySink>,
    ) -> Self {
        Self {
            cfg,
            manifest_src,
            bundle_src,
            sink,
            sigma: Arc::new(Mutex::new(SigmaPolicy::default())),
        }
    }

    /// Replace the Σ-mask policy (e.g. on player-consent-UI commit).
    pub fn set_sigma_policy(&self, p: SigmaPolicy) {
        *self.sigma.lock().unwrap() = p;
    }

    pub fn sigma_policy(&self) -> SigmaPolicy {
        self.sigma.lock().unwrap().clone()
    }

    /// One poll cycle. Production loop : the engine integrator calls this
    /// every `cfg.poll_interval_ms` ms from a worker thread.
    pub fn poll_once(&self, now_ns: u64) -> PollReport {
        // 1. fetch + verify manifest.
        let manifest = match self.manifest_src.fetch_manifest() {
            Ok(m) => m,
            Err(e) => {
                self.sink.emit(HotfixEvent::Skipped {
                    channel: "<all>".to_string(),
                    reason: format!("manifest-fetch-failed : {e}"),
                    ts_ns: now_ns,
                });
                return PollReport {
                    ts_ns: now_ns,
                    manifest_generated_at_ns: 0,
                    per_channel: BTreeMap::new(),
                };
            }
        };
        if let Err(e) = verify_manifest(&manifest, &self.cfg.cap_keys) {
            self.sink.emit(HotfixEvent::Skipped {
                channel: "<all>".to_string(),
                reason: format!("manifest-verify-failed : {e}"),
                ts_ns: now_ns,
            });
            return PollReport {
                ts_ns: now_ns,
                manifest_generated_at_ns: manifest.generated_at_ns,
                per_channel: BTreeMap::new(),
            };
        }
        self.sink.emit(HotfixEvent::Checked { ts_ns: now_ns });

        let policy = self.sigma_policy();
        let mut per_channel: BTreeMap<Channel, PollOutcome> = BTreeMap::new();

        for ch in CHANNELS {
            let outcome = self.process_channel(ch, &manifest, &policy, now_ns);
            per_channel.insert(ch, outcome);
        }

        PollReport {
            ts_ns: now_ns,
            manifest_generated_at_ns: manifest.generated_at_ns,
            per_channel,
        }
    }

    fn process_channel(
        &self,
        ch: Channel,
        manifest: &Manifest,
        policy: &SigmaPolicy,
        now_ns: u64,
    ) -> PollOutcome {
        // Σ-mask gate.
        let consent = policy.get(ch);
        if consent == UpdateConsent::Off {
            self.sink.emit(HotfixEvent::Skipped {
                channel: ch.name().to_string(),
                reason: "consent-off".to_string(),
                ts_ns: now_ns,
            });
            return PollOutcome::Skipped {
                reason: "consent-off".to_string(),
            };
        }
        if consent == UpdateConsent::PinnedNoUpdates {
            self.sink.emit(HotfixEvent::Skipped {
                channel: ch.name().to_string(),
                reason: "pinned".to_string(),
                ts_ns: now_ns,
            });
            return PollOutcome::Skipped {
                reason: "pinned".to_string(),
            };
        }

        let Some(entry) = manifest.entry(ch) else {
            return PollOutcome::NoUpdate;
        };

        // Read installed version from `<dir>/<channel>/current.meta`.
        let installed = read_installed_version(&self.cfg.install_dir, ch);

        // Revocation : if installed-version is revoked, schedule rollback.
        if let Some(ref iv) = installed {
            if check_not_revoked(manifest, ch, iv).is_err() {
                self.sink.emit(HotfixEvent::Revoked {
                    channel: ch.name().to_string(),
                    version: iv.clone(),
                    ts_ns: now_ns,
                });
                return PollOutcome::RolledBack {
                    from: iv.clone(),
                    to: None,
                };
            }
        }

        // Up-to-date check.
        if installed.as_deref() == Some(entry.current_version.as_str()) {
            return PollOutcome::NoUpdate;
        }

        // Fetch bundle.
        let bytes = match self
            .bundle_src
            .fetch_bundle(ch.name(), &entry.current_version)
        {
            Ok(b) => b,
            Err(e) => {
                self.sink.emit(HotfixEvent::ApplyFailed {
                    channel: ch.name().to_string(),
                    version: entry.current_version.clone(),
                    error: format!("fetch-failed : {e}"),
                    ts_ns: now_ns,
                });
                return PollOutcome::Failed {
                    error: format!("fetch-failed : {e}"),
                };
            }
        };

        // Parse + verify bundle.
        let bundle = match Bundle::from_bytes(&bytes) {
            Ok(b) => b,
            Err(e) => {
                self.sink.emit(HotfixEvent::ApplyFailed {
                    channel: ch.name().to_string(),
                    version: entry.current_version.clone(),
                    error: format!("bundle-parse : {e}"),
                    ts_ns: now_ns,
                });
                return PollOutcome::Failed {
                    error: format!("bundle-parse : {e}"),
                };
            }
        };
        let verify_ok = match verify_bundle(&bundle, &self.cfg.cap_keys) {
            Ok(v) => v,
            Err(e) => {
                self.sink.emit(HotfixEvent::ApplyFailed {
                    channel: ch.name().to_string(),
                    version: entry.current_version.clone(),
                    error: format!("verify : {e}"),
                    ts_ns: now_ns,
                });
                return PollOutcome::Failed {
                    error: format!("verify : {e}"),
                };
            }
        };
        self.sink.emit(HotfixEvent::Downloaded {
            channel: ch.name().to_string(),
            version: entry.current_version.clone(),
            size_bytes: entry.size_bytes,
            ts_ns: now_ns,
        });

        // Prompt-required ?
        if consent == UpdateConsent::PromptBeforeApply {
            self.sink.emit(HotfixEvent::Skipped {
                channel: ch.name().to_string(),
                reason: "prompt-pending".to_string(),
                ts_ns: now_ns,
            });
            return PollOutcome::PromptPending {
                version: entry.current_version.clone(),
            };
        }

        // Apply.
        let snap = match apply_bundle(&bundle, &self.cfg.install_dir, now_ns, verify_ok) {
            Ok(s) => s,
            Err(e) => {
                self.sink.emit(HotfixEvent::ApplyFailed {
                    channel: ch.name().to_string(),
                    version: entry.current_version.clone(),
                    error: format!("apply : {e}"),
                    ts_ns: now_ns,
                });
                return PollOutcome::Failed {
                    error: format!("apply : {e}"),
                };
            }
        };

        self.sink.emit(HotfixEvent::Applied {
            channel: ch.name().to_string(),
            version: snap.new_version.clone(),
            ts_ns: now_ns,
        });

        store_snapshot(&self.cfg.install_dir, ch, &snap);

        PollOutcome::Updated {
            from: snap.prior_version,
            to: snap.new_version,
        }
    }

    /// Manually roll back a channel using its most-recent snapshot, if any.
    pub fn rollback_channel(&self, ch: Channel, now_ns: u64) -> Result<(), String> {
        let snap =
            load_snapshot(&self.cfg.install_dir, ch).ok_or_else(|| "no-snapshot".to_string())?;
        match rollback(&snap, now_ns) {
            Ok(()) => {
                self.sink.emit(HotfixEvent::RolledBack {
                    channel: ch.name().to_string(),
                    from_version: snap.new_version.clone(),
                    to_version: snap.prior_version.clone().unwrap_or_else(|| "?".to_string()),
                    reason: "manual".to_string(),
                    ts_ns: now_ns,
                });
                Ok(())
            }
            Err(e) => Err(e.to_string()),
        }
    }
}

// ──────────────────────────────────────────────────────────────────────
// Disk helpers
// ──────────────────────────────────────────────────────────────────────

fn read_installed_version(install_dir: &std::path::Path, ch: Channel) -> Option<String> {
    let meta_path = install_dir.join(ch.name()).join("current.meta");
    let s = std::fs::read_to_string(meta_path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&s).ok()?;
    v.get("version").and_then(|x| x.as_str()).map(String::from)
}

/// Snapshot index lives at `<install_dir>/.snapshots/<channel>.json`.
/// Errors here are non-fatal.
fn store_snapshot(install_dir: &std::path::Path, ch: Channel, snap: &AppliedSnapshot) {
    let dir = install_dir.join(".snapshots");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join(format!("{}.json", ch.name()));
    if let Ok(s) = serde_json::to_string_pretty(snap) {
        let _ = std::fs::write(path, s);
    }
}

fn load_snapshot(install_dir: &std::path::Path, ch: Channel) -> Option<AppliedSnapshot> {
    let path = install_dir
        .join(".snapshots")
        .join(format!("{}.json", ch.name()));
    let s = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&s).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sources::{MockBundleSource, MockManifestSource};
    use crate::telemetry::MockTelemetrySink;
    use cssl_hotfix::bundle::{BundleHeader, BUNDLE_FORMAT_VERSION};
    use cssl_hotfix::cap::CapRole;
    use cssl_hotfix::manifest::ChannelEntry;
    use cssl_hotfix::sign::{sign_bundle, sign_manifest};
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_dir(tag: &str) -> PathBuf {
        let n = TMP_COUNTER.fetch_add(1, Ordering::SeqCst);
        let pid = std::process::id();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        let p = std::env::temp_dir().join(format!("cssl-hotfix-client-{tag}-{pid}-{nanos}-{n}"));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn make_cap_key(role: CapRole) -> (SigningKey, CapKey) {
        let key = SigningKey::generate(&mut OsRng);
        let pubk = key.verifying_key().to_bytes();
        (key, CapKey { role, pubkey: pubk })
    }

    fn make_signed_bundle(
        ch: Channel,
        cap: CapRole,
        ver: (u16, u16, u16),
        payload: &[u8],
        signing: &SigningKey,
    ) -> Bundle {
        let header = BundleHeader {
            format_version: BUNDLE_FORMAT_VERSION,
            channel: ch,
            cap_role: cap,
            ver_major: ver.0,
            ver_minor: ver.1,
            ver_patch: ver.2,
            timestamp_ns: 0,
            payload_size: 0,
            payload_blake3: [0u8; 32],
        };
        sign_bundle(header, payload.to_vec(), signing).unwrap()
    }

    fn make_signed_manifest(
        ch: Channel,
        ver: &str,
        bundle_bytes: &[u8],
        cap: CapRole,
        signing: &SigningKey,
    ) -> Manifest {
        let mut channels = std::collections::BTreeMap::new();
        channels.insert(
            ch,
            ChannelEntry {
                current_version: ver.to_string(),
                bundle_sha256: hex(blake3::hash(bundle_bytes).as_bytes()),
                effective_from_ns: 0,
                download_path: format!("{}/{ver}.csslfix", ch.name()),
                size_bytes: bundle_bytes.len() as u64,
            },
        );
        let m = Manifest {
            schema_version: 1,
            generated_at_ns: 1,
            signed_by: cap,
            channels,
            revocations: vec![],
            signature: [0u8; 64],
        };
        sign_manifest(m, signing).unwrap()
    }

    fn hex(b: &[u8; 32]) -> String {
        b.iter()
            .fold(String::with_capacity(64), |mut acc, x| {
                use std::fmt::Write as _;
                let _ = write!(&mut acc, "{x:02x}");
                acc
            })
    }

    #[test]
    fn poll_with_security_default_consent_applies() {
        let (sk_d, ck_d) = make_cap_key(CapRole::CapD);
        let install = temp_dir("apply");

        let bundle = make_signed_bundle(
            Channel::SecurityPatch,
            CapRole::CapD,
            (1, 0, 0),
            b"patch-bytes",
            &sk_d,
        );
        let bundle_bytes = bundle.to_bytes();
        let manifest = make_signed_manifest(
            Channel::SecurityPatch,
            "1.0.0",
            &bundle_bytes,
            CapRole::CapD,
            &sk_d,
        );

        let m_src = Arc::new(MockManifestSource::new(manifest));
        let b_src = Arc::new(MockBundleSource::new());
        b_src.put("security.patch", "1.0.0", bundle_bytes);
        let sink = Arc::new(MockTelemetrySink::new());

        let cfg = HotfixClientConfig {
            install_dir: install.clone(),
            poll_interval_ms: 1000,
            cap_keys: vec![ck_d],
        };

        let client = HotfixClient::new(cfg, m_src, b_src, sink.clone());
        let report = client.poll_once(123);

        let outcome = report.per_channel.get(&Channel::SecurityPatch).unwrap();
        assert!(matches!(outcome, PollOutcome::Updated { .. }));
        let cur = std::fs::read(install.join("security.patch").join("current")).unwrap();
        assert_eq!(cur, b"patch-bytes");
        let snap = sink.snapshot();
        assert!(snap.iter().any(|e| matches!(e, HotfixEvent::Checked { .. })));
        assert!(snap.iter().any(|e| matches!(e, HotfixEvent::Applied { .. })));
    }

    #[test]
    fn poll_skips_off_channels() {
        let (sk_b, ck_b) = make_cap_key(CapRole::CapB);
        let (_, ck_d) = make_cap_key(CapRole::CapD);
        let install = temp_dir("skip");

        let bundle = make_signed_bundle(
            Channel::CsslBundle,
            CapRole::CapB,
            (1, 0, 0),
            b"data",
            &sk_b,
        );
        let bytes = bundle.to_bytes();
        let manifest =
            make_signed_manifest(Channel::CsslBundle, "1.0.0", &bytes, CapRole::CapB, &sk_b);

        let m_src = Arc::new(MockManifestSource::new(manifest));
        let b_src = Arc::new(MockBundleSource::new());
        b_src.put("cssl.bundle", "1.0.0", bytes);
        let sink = Arc::new(MockTelemetrySink::new());

        let cfg = HotfixClientConfig {
            install_dir: install,
            poll_interval_ms: 1000,
            cap_keys: vec![ck_b, ck_d],
        };
        let client = HotfixClient::new(cfg, m_src, b_src, sink);
        let r = client.poll_once(7);
        let outcome = r.per_channel.get(&Channel::CsslBundle).unwrap();
        assert!(matches!(outcome, PollOutcome::Skipped { .. }));
    }

    #[test]
    fn poll_prompt_pending_when_consent_is_prompt() {
        let (sk_b, ck_b) = make_cap_key(CapRole::CapB);
        let install = temp_dir("prompt");

        let bundle = make_signed_bundle(
            Channel::CsslBundle,
            CapRole::CapB,
            (2, 0, 0),
            b"new-data",
            &sk_b,
        );
        let bytes = bundle.to_bytes();
        let manifest =
            make_signed_manifest(Channel::CsslBundle, "2.0.0", &bytes, CapRole::CapB, &sk_b);

        let m_src = Arc::new(MockManifestSource::new(manifest));
        let b_src = Arc::new(MockBundleSource::new());
        b_src.put("cssl.bundle", "2.0.0", bytes);
        let sink = Arc::new(MockTelemetrySink::new());

        let cfg = HotfixClientConfig {
            install_dir: install,
            poll_interval_ms: 1000,
            cap_keys: vec![ck_b],
        };
        let client = HotfixClient::new(cfg, m_src, b_src, sink);
        let mut p = SigmaPolicy::default();
        p.set(Channel::CsslBundle, UpdateConsent::PromptBeforeApply);
        client.set_sigma_policy(p);

        let r = client.poll_once(99);
        let outcome = r.per_channel.get(&Channel::CsslBundle).unwrap();
        assert!(matches!(outcome, PollOutcome::PromptPending { .. }));
    }

    #[test]
    fn poll_no_update_when_already_current() {
        let (sk_d, ck_d) = make_cap_key(CapRole::CapD);
        let install = temp_dir("noupdate");

        let bundle = make_signed_bundle(
            Channel::SecurityPatch,
            CapRole::CapD,
            (1, 0, 0),
            b"x",
            &sk_d,
        );
        let bytes = bundle.to_bytes();
        let manifest = make_signed_manifest(
            Channel::SecurityPatch,
            "1.0.0",
            &bytes,
            CapRole::CapD,
            &sk_d,
        );

        let m_src = Arc::new(MockManifestSource::new(manifest));
        let b_src = Arc::new(MockBundleSource::new());
        b_src.put("security.patch", "1.0.0", bytes);
        let sink = Arc::new(MockTelemetrySink::new());

        let cfg = HotfixClientConfig {
            install_dir: install,
            poll_interval_ms: 1000,
            cap_keys: vec![ck_d],
        };
        let client = HotfixClient::new(cfg, m_src, b_src, sink);
        let _ = client.poll_once(1);
        let r2 = client.poll_once(2);
        let o = r2.per_channel.get(&Channel::SecurityPatch).unwrap();
        assert!(matches!(o, PollOutcome::NoUpdate));
    }

    #[test]
    fn poll_revoked_version_marks_rolled_back() {
        let (sk_d, ck_d) = make_cap_key(CapRole::CapD);
        let install = temp_dir("revoked");

        let bundle = make_signed_bundle(
            Channel::SecurityPatch,
            CapRole::CapD,
            (1, 0, 0),
            b"original",
            &sk_d,
        );
        let bytes = bundle.to_bytes();
        let manifest = make_signed_manifest(
            Channel::SecurityPatch,
            "1.0.0",
            &bytes,
            CapRole::CapD,
            &sk_d,
        );
        let m_src = Arc::new(MockManifestSource::new(manifest));
        let b_src = Arc::new(MockBundleSource::new());
        b_src.put("security.patch", "1.0.0", bytes);
        let sink = Arc::new(MockTelemetrySink::new());
        let cfg = HotfixClientConfig {
            install_dir: install,
            poll_interval_ms: 1000,
            cap_keys: vec![ck_d],
        };
        let client = HotfixClient::new(cfg, m_src.clone(), b_src, sink);
        let _ = client.poll_once(1);

        let mut new_manifest = m_src.fetch_manifest().unwrap();
        new_manifest
            .revocations
            .push(cssl_hotfix::manifest::RevocationEntry {
                channel: Channel::SecurityPatch,
                version: "1.0.0".to_string(),
                ts_ns: 2,
                reason: "exploit-found".to_string(),
            });
        new_manifest = sign_manifest(new_manifest, &sk_d).unwrap();
        m_src.set(new_manifest);

        let r = client.poll_once(3);
        let o = r.per_channel.get(&Channel::SecurityPatch).unwrap();
        assert!(matches!(o, PollOutcome::RolledBack { .. }));
    }

    #[test]
    fn manifest_fetch_error_returns_empty_report() {
        let (_, ck_d) = make_cap_key(CapRole::CapD);
        let install = temp_dir("nofetch");

        let m_src = Arc::new(MockManifestSource::new(Manifest {
            schema_version: 1,
            generated_at_ns: 0,
            signed_by: CapRole::CapD,
            channels: Default::default(),
            revocations: vec![],
            signature: [0u8; 64],
        }));
        m_src.set_error(crate::sources::SourceError::Network("offline".into()));
        let b_src = Arc::new(MockBundleSource::new());
        let sink = Arc::new(MockTelemetrySink::new());
        let cfg = HotfixClientConfig {
            install_dir: install,
            poll_interval_ms: 1000,
            cap_keys: vec![ck_d],
        };
        let client = HotfixClient::new(cfg, m_src, b_src, sink.clone());
        let r = client.poll_once(0);
        assert!(r.per_channel.is_empty());
        assert!(sink
            .snapshot()
            .iter()
            .any(|e| matches!(e, HotfixEvent::Skipped { .. })));
    }
}
