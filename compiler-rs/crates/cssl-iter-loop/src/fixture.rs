//! Runtime test-fixture extraction-from-runtime.
//!
//! Mirrors `_drafts/phase_j/wave_ji_iteration_loop_docs.md` § 2 — bug-encountered
//! becomes a test-fixture via `record_replay → save → into_regression_test`.
//! The capture is a minimal snapshot of the interesting state (frame_n + seed
//! + Ω-tensor stub + creature stub + trigger event) plus a BLAKE3 fingerprint.
//!
//! § Σ-discipline (§ 2.4)
//!   - biometric-marked snapshots ARE REFUSED at extract-time
//!   - sovereign-private cells SHOULD be stripped before reaching this layer
//!     (the cssl-inspect SigmaOverlay::observe_cell already handles that)
//!   - the BLAKE3 hash binds frame_n + seed + ω + creature + trigger ; any
//!     post-extract mutation is detectable
//!
//! § Replay-format compatibility (§ 2 + landmine-3 :)
//!   The fixture serializes via `serde_json` because the workspace pins
//!   serde_json and the format is human-readable for test-debugging. A
//!   downstream slice can swap to bincode or to cssl-replay-validator's
//!   ReplayLog format once the on-disk fixture-corpus stabilizes ; see
//!   integration-point D233/05 below.
//!
//! § INTEGRATION-POINT D233/03 — `OmegaSnapshotStub` swaps to real
//!   `cssl_substrate_omega_field::OmegaFieldSnapshot` once that crate's
//!   public API stabilizes. The stub captures the canonical "ω-field at
//!   frame N" shape (cell count + bounded-region marker + content hash).
//!
//! § INTEGRATION-POINT D233/04 — `CreatureSnapshotStub` swaps to real
//!   `cssl_creature_behavior::CreatureSnapshot` when the creature crate
//!   lands an inspectable type with stable serde semantics.
//!
//! § INTEGRATION-POINT D233/05 — fixture-format unifies with cssl-replay-
//!   validator's ReplayLogSnapshot (bincode + magic-bytes header) when the
//!   on-disk fixture corpus stabilizes — Wave-Jθ-9 amendment.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// Path-hash carrier — BLAKE3 of the canonical UTF-8 path string + the
/// installation-salt. Mirrors the D130 path-hash discipline so callers
/// never serialize raw paths into the fixture-blob.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PathHash(pub [u8; 32]);

impl PathHash {
    /// Hash a raw path with the canonical installation-salt. Tests use a
    /// fixed salt so the output is reproducible ; production callers
    /// should bind to the cssl-log path-hash registry's salt.
    pub fn hash_with_salt(path: &Path, salt: &[u8]) -> Self {
        let s = path.to_string_lossy();
        let mut h = blake3::Hasher::new();
        h.update(salt);
        h.update(s.as_bytes());
        Self(*h.finalize().as_bytes())
    }

    /// Convenience hash with the canonical iter-loop salt. Production code
    /// should prefer `hash_with_salt` with the workspace-wide salt.
    pub fn hash(path: &Path) -> Self {
        Self::hash_with_salt(path, b"cssl-iter-loop-default-salt")
    }
}

/// § INTEGRATION-POINT D233/03 — stub for the real Ω-field snapshot.
///
/// Captures only the metadata + content-hash. Real snapshot will carry
/// per-band ψ-amplitudes + Σ-mask labels per cell ; callers should never
/// embed those raw amplitudes into a regression-test fixture (use the
/// real snapshot's `redact_biometric_layers()` helper when it lands).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OmegaSnapshotStub {
    pub frame_n: u64,
    pub cell_count: u64,
    pub region_morton_lo: u64,
    pub region_morton_hi: u64,
    pub content_hash: [u8; 32],
    /// True when this snapshot was post-stripped of biometric-Σ-marked cells.
    pub biometric_stripped: bool,
}

impl OmegaSnapshotStub {
    pub fn new(frame_n: u64, cell_count: u64) -> Self {
        let mut h = blake3::Hasher::new();
        h.update(&frame_n.to_le_bytes());
        h.update(&cell_count.to_le_bytes());
        Self {
            frame_n,
            cell_count,
            region_morton_lo: 0,
            region_morton_hi: u64::MAX,
            content_hash: *h.finalize().as_bytes(),
            biometric_stripped: true,
        }
    }
}

/// § INTEGRATION-POINT D233/04 — stub for the real creature snapshot.
///
/// Carries the creature's id + count of behavior-relevant scalars + a
/// content-hash. Full behavior-state will arrive once cssl-creature-behavior
/// lands a serde-stable `CreatureSnapshot`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreatureSnapshotStub {
    pub creature_id: u64,
    pub kan_layer_count: u32,
    pub agency_active: bool,
    pub content_hash: [u8; 32],
}

impl CreatureSnapshotStub {
    pub fn new(creature_id: u64, kan_layer_count: u32) -> Self {
        let mut h = blake3::Hasher::new();
        h.update(&creature_id.to_le_bytes());
        h.update(&kan_layer_count.to_le_bytes());
        Self {
            creature_id,
            kan_layer_count,
            agency_active: true,
            content_hash: *h.finalize().as_bytes(),
        }
    }
}

/// What kind of event triggered the fixture-extract. Mirrors the
/// "bug-encountered" → "fixture" pipeline from `wave_ji_iteration_loop_docs.md`
/// § 2.1 ; supports the four canonical extraction shapes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TriggerEvent {
    /// An invariant flagged a violation at frame N.
    InvariantViolation {
        invariant_name: String,
        observed: f64,
        expected_max_dev: f64,
    },
    /// An error-class telemetry event was emitted.
    ErrorEvent {
        subsystem: String,
        kind_id: u32,
        message: String,
    },
    /// The user-pressed-key path : Apocky-PM explicitly snapshotted state.
    ManualSnapshot { reason: String },
    /// Automatic perf-regression-detected snapshot.
    PerfRegression {
        metric_name: String,
        ratio_x1000: u32,
    },
}

/// Errors emitted during fixture extraction / serialization / load.
#[derive(Debug, thiserror::Error)]
pub enum FixtureError {
    /// The Ω-snapshot still has biometric-Σ-marked cells. Refuse to extract.
    #[error("biometric-refused : Ω-snapshot was not biometric-stripped")]
    BiometricRefused,

    /// Serialization to disk failed.
    #[error("serialize error : {detail}")]
    Serialize { detail: String },

    /// Deserialization from disk failed.
    #[error("deserialize error : {detail}")]
    Deserialize { detail: String },

    /// I/O error during read or write.
    #[error("io error : {detail}")]
    Io { detail: String },

    /// On-disk fixture's content-hash does not match its declared blake3_hash.
    #[error("integrity error : content-hash mismatch (expected {expected}, got {actual})")]
    IntegrityMismatch { expected: String, actual: String },

    /// The expected fixture-format-version does not match.
    #[error("version mismatch : expected v{expected}, got v{actual}")]
    VersionMismatch { expected: u32, actual: u32 },
}

/// Canonical fixture format-version. Bump on any byte-layout change.
pub const FIXTURE_FORMAT_VERSION: u32 = 1;

/// A captured runtime-fixture suitable for regression-testing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeFixture {
    pub format_version: u32,
    pub frame_n: u64,
    pub seed: u64,
    pub omega_field_snapshot: OmegaSnapshotStub,
    pub creature_state_snapshot: CreatureSnapshotStub,
    pub trigger_event: TriggerEvent,
    pub blake3_hash: [u8; 32],
}

impl RuntimeFixture {
    /// Build a fixture from runtime inputs. Computes the canonical BLAKE3
    /// fingerprint over the fixture's stable byte-form.
    ///
    /// REFUSES extraction if the Ω-snapshot is not biometric-stripped — this
    /// is the §1 SURVEILLANCE protection at fixture-extract.
    pub fn extract_from_runtime(
        frame_n: u64,
        seed: u64,
        omega: OmegaSnapshotStub,
        creature: CreatureSnapshotStub,
        trigger: TriggerEvent,
    ) -> Result<Self, FixtureError> {
        if !omega.biometric_stripped {
            return Err(FixtureError::BiometricRefused);
        }
        let blake3_hash = compute_fingerprint(frame_n, seed, &omega, &creature, &trigger);
        Ok(Self {
            format_version: FIXTURE_FORMAT_VERSION,
            frame_n,
            seed,
            omega_field_snapshot: omega,
            creature_state_snapshot: creature,
            trigger_event: trigger,
            blake3_hash,
        })
    }

    /// Verify the BLAKE3 fingerprint matches the fixture's declared content.
    /// Used at load-from-disk + at into-regression-test to detect tampering.
    pub fn verify_integrity(&self) -> Result<(), FixtureError> {
        let computed = compute_fingerprint(
            self.frame_n,
            self.seed,
            &self.omega_field_snapshot,
            &self.creature_state_snapshot,
            &self.trigger_event,
        );
        if computed == self.blake3_hash {
            Ok(())
        } else {
            Err(FixtureError::IntegrityMismatch {
                expected: hex32(&self.blake3_hash),
                actual: hex32(&computed),
            })
        }
    }

    /// Serialize the fixture to disk at `path`. The path-hash must come from
    /// a higher-layer registry (D130) — this method takes a real Path so the
    /// caller can bridge ; the on-disk record itself never carries the path.
    pub fn serialize_to_disk(&self, path: &Path) -> Result<(), FixtureError> {
        let bytes = serde_json::to_vec(self).map_err(|e| FixtureError::Serialize {
            detail: format!("serde_json : {e}"),
        })?;
        std::fs::write(path, bytes).map_err(|e| FixtureError::Io {
            detail: format!("write {} : {e}", path.display()),
        })
    }

    /// Load a fixture from disk + verify its integrity + format-version.
    pub fn load_from_disk(path: &Path) -> Result<Self, FixtureError> {
        let bytes = std::fs::read(path).map_err(|e| FixtureError::Io {
            detail: format!("read {} : {e}", path.display()),
        })?;
        let fixture: RuntimeFixture =
            serde_json::from_slice(&bytes).map_err(|e| FixtureError::Deserialize {
                detail: format!("serde_json : {e}"),
            })?;
        if fixture.format_version != FIXTURE_FORMAT_VERSION {
            return Err(FixtureError::VersionMismatch {
                expected: FIXTURE_FORMAT_VERSION,
                actual: fixture.format_version,
            });
        }
        fixture.verify_integrity()?;
        Ok(fixture)
    }

    /// Convert this fixture into a regression-test case. The result is a
    /// data-only carrier ; downstream test-runners turn it into a `#[test]`
    /// function or an `cargo nextest` parametrized case.
    pub fn into_regression_test(self) -> RegressionTestCase {
        let test_name = match &self.trigger_event {
            TriggerEvent::InvariantViolation { invariant_name, .. } => {
                format!("regression_inv_{invariant_name}_frame_{}", self.frame_n)
            }
            TriggerEvent::ErrorEvent {
                subsystem, kind_id, ..
            } => {
                format!(
                    "regression_err_{subsystem}_{kind_id}_frame_{}",
                    self.frame_n
                )
            }
            TriggerEvent::ManualSnapshot { reason } => {
                format!(
                    "regression_manual_{}_frame_{}",
                    slugify(reason),
                    self.frame_n
                )
            }
            TriggerEvent::PerfRegression { metric_name, .. } => {
                format!(
                    "regression_perf_{}_frame_{}",
                    slugify(metric_name),
                    self.frame_n
                )
            }
        };
        RegressionTestCase {
            test_name,
            fixture: self,
        }
    }
}

/// A regression-test case packaged from a runtime-fixture. Downstream
/// test-harness macros turn this into a real `#[test]` or a parametrized
/// case. The `test_name` is slug-safe (lowercase / underscores / digits).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegressionTestCase {
    pub test_name: String,
    pub fixture: RuntimeFixture,
}

impl RegressionTestCase {
    pub fn name(&self) -> &str {
        &self.test_name
    }
}

fn compute_fingerprint(
    frame_n: u64,
    seed: u64,
    omega: &OmegaSnapshotStub,
    creature: &CreatureSnapshotStub,
    trigger: &TriggerEvent,
) -> [u8; 32] {
    let mut h = blake3::Hasher::new();
    h.update(b"cssl-iter-loop-fixture-v1");
    h.update(&frame_n.to_le_bytes());
    h.update(&seed.to_le_bytes());
    h.update(&omega.content_hash);
    h.update(&creature.content_hash);
    let trig_bytes = serde_json::to_vec(trigger).unwrap_or_default();
    h.update(&trig_bytes);
    *h.finalize().as_bytes()
}

fn hex32(bytes: &[u8; 32]) -> String {
    let mut s = String::with_capacity(64);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

fn slugify(input: &str) -> String {
    input
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_inputs() -> (
        u64,
        u64,
        OmegaSnapshotStub,
        CreatureSnapshotStub,
        TriggerEvent,
    ) {
        let omega = OmegaSnapshotStub::new(12_000, 4096);
        let creature = CreatureSnapshotStub::new(0xABCD, 3);
        let trigger = TriggerEvent::InvariantViolation {
            invariant_name: "wave_solver_psi_norm".into(),
            observed: 1.003,
            expected_max_dev: 0.001,
        };
        (12_000, 0xDEADBEEF, omega, creature, trigger)
    }

    #[test]
    fn extract_from_runtime_succeeds_on_clean_omega() {
        let (frame, seed, omega, creature, trigger) = fixture_inputs();
        let f =
            RuntimeFixture::extract_from_runtime(frame, seed, omega, creature, trigger).unwrap();
        assert_eq!(f.frame_n, frame);
        assert_eq!(f.seed, seed);
        assert_eq!(f.format_version, FIXTURE_FORMAT_VERSION);
    }

    #[test]
    fn extract_refuses_biometric_unstripped_omega() {
        let (frame, seed, mut omega, creature, trigger) = fixture_inputs();
        omega.biometric_stripped = false;
        let r = RuntimeFixture::extract_from_runtime(frame, seed, omega, creature, trigger);
        assert!(matches!(r, Err(FixtureError::BiometricRefused)));
    }

    #[test]
    fn fixture_round_trip_through_disk() {
        let (frame, seed, omega, creature, trigger) = fixture_inputs();
        let f =
            RuntimeFixture::extract_from_runtime(frame, seed, omega, creature, trigger).unwrap();
        let dir = std::env::temp_dir().join("cssl-iter-loop-tests");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("fixture-roundtrip.json");
        f.serialize_to_disk(&path).unwrap();
        let loaded = RuntimeFixture::load_from_disk(&path).unwrap();
        assert_eq!(f, loaded);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn fixture_integrity_check_detects_tamper() {
        let (frame, seed, omega, creature, trigger) = fixture_inputs();
        let mut f =
            RuntimeFixture::extract_from_runtime(frame, seed, omega, creature, trigger).unwrap();
        f.frame_n = 99_999; // tamper post-fingerprint
        let r = f.verify_integrity();
        assert!(matches!(r, Err(FixtureError::IntegrityMismatch { .. })));
    }

    #[test]
    fn fixture_into_regression_test_invariant_name() {
        let (frame, seed, omega, creature, trigger) = fixture_inputs();
        let f =
            RuntimeFixture::extract_from_runtime(frame, seed, omega, creature, trigger).unwrap();
        let case = f.into_regression_test();
        assert!(case.test_name.contains("regression_inv"));
        assert!(case.test_name.contains("12000"));
    }

    #[test]
    fn path_hash_deterministic_with_same_salt() {
        let p = Path::new("/etc/passwd");
        let a = PathHash::hash_with_salt(p, b"salt");
        let b = PathHash::hash_with_salt(p, b"salt");
        assert_eq!(a, b);
        let c = PathHash::hash_with_salt(p, b"diff");
        assert_ne!(a, c);
    }

    #[test]
    fn fixture_version_mismatch_rejected() {
        let (frame, seed, omega, creature, trigger) = fixture_inputs();
        let mut f =
            RuntimeFixture::extract_from_runtime(frame, seed, omega, creature, trigger).unwrap();
        f.format_version = 99;
        let dir = std::env::temp_dir().join("cssl-iter-loop-tests");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("fixture-versionmismatch.json");
        // Bypass extract path : serialize directly with bad version + integrity.
        f.blake3_hash = compute_fingerprint(
            f.frame_n,
            f.seed,
            &f.omega_field_snapshot,
            &f.creature_state_snapshot,
            &f.trigger_event,
        );
        std::fs::write(&path, serde_json::to_vec(&f).unwrap()).unwrap();
        let r = RuntimeFixture::load_from_disk(&path);
        assert!(matches!(r, Err(FixtureError::VersionMismatch { .. })));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn trigger_event_round_trips_all_variants() {
        let v1 = TriggerEvent::InvariantViolation {
            invariant_name: "x".into(),
            observed: 1.0,
            expected_max_dev: 0.5,
        };
        let v2 = TriggerEvent::ErrorEvent {
            subsystem: "renderer".into(),
            kind_id: 42,
            message: "msg".into(),
        };
        let v3 = TriggerEvent::ManualSnapshot {
            reason: "needed snap".into(),
        };
        let v4 = TriggerEvent::PerfRegression {
            metric_name: "frame.tick_us".into(),
            ratio_x1000: 1100,
        };
        for v in [v1, v2, v3, v4] {
            let bytes = serde_json::to_vec(&v).unwrap();
            let back: TriggerEvent = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(v, back);
        }
    }
}
