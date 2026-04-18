//! Golden-image oracle (`@golden`) — pixel + SSIM + FLIP.
//!
//! § SPEC    : `specs/23_TESTING.csl` § golden-image-framework.
//! § LOCATION: reference frames under `compiler-rs/tests/golden/<bench-id>/*.png` (or HDR).
//! § METRICS :
//!   - pixel-diff : tolerance per-percentile, configurable.
//!   - SSIM       : structural-similarity > 0.99 default threshold.
//!   - FLIP       : NVIDIA perceptual-diff metric for human-aligned comparison.
//! § UPDATE  : `csslc test --update-golden` (T11+) after verified-change.
//! § STATUS  : T11-phase-2b live (byte-exact + raw-pixel-pct mode) ; SSIM + FLIP
//!            perceptual metrics deferred to T11-phase-2c (require image-decode deps).

use std::path::Path;

/// Config for the `@golden` oracle.
#[derive(Debug, Clone)]
pub struct Config {
    /// Relative path to golden fixture under `tests/golden/`.
    pub path: String,
    /// SSIM threshold (default 0.99 per §§ 23).
    pub ssim_threshold: f32,
    /// FLIP-metric threshold (default 0.05 — lower means more-similar).
    pub flip_threshold: f32,
    /// Pixel-diff tolerance percentile (default 0.001 = 0.1% of pixels may differ).
    pub pixel_tolerance_pct: f32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            path: String::new(),
            ssim_threshold: 0.99,
            flip_threshold: 0.05,
            pixel_tolerance_pct: 0.001,
        }
    }
}

/// Metric values measured against a golden image.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Metrics {
    /// Structural similarity; 1.0 = identical.
    pub ssim: f32,
    /// FLIP perceptual difference; 0.0 = identical.
    pub flip: f32,
    /// Fraction of pixels differing beyond tolerance.
    pub pixel_diff_pct: f32,
}

/// Outcome of running the `@golden` oracle.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// Stage0 stub — body populated at T10+.
    Stage0Unimplemented,
    /// Generated image matches reference within all thresholds.
    Ok { metrics: Metrics },
    /// One or more thresholds exceeded.
    ThresholdExceeded {
        metrics: Metrics,
        breached: &'static str,
    },
    /// Reference image missing under `tests/golden/<path>`.
    NoReference { path: String },
}

/// Dispatcher trait for `@golden` oracle.
pub trait Dispatcher {
    fn run(&self, config: &Config) -> Outcome;
}

/// Stage0 stub dispatcher — always returns `Stage0Unimplemented`.
#[derive(Debug, Default, Clone, Copy)]
pub struct Stage0Stub;

impl Dispatcher for Stage0Stub {
    fn run(&self, _config: &Config) -> Outcome {
        Outcome::Stage0Unimplemented
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § Live byte-exact + raw-pixel-pct runner. SSIM + FLIP deferred to T11-phase-2c.
// ─────────────────────────────────────────────────────────────────────────

/// Compare `actual` bytes against the golden file at `golden_path`. Byte-exact
/// mode : any byte-mismatch counts as a differing byte, and the percentage is
/// reported through `Metrics::pixel_diff_pct` (repurposed as byte-diff-pct).
///
/// This is the foundation oracle that works for any binary format — image,
/// shader-bytecode, IR-dump, log-file. SSIM + FLIP layers build on top of it
/// once PNG/HDR decode is available.
///
/// Returns :
/// - `Ok { metrics }`  if bytes are within `config.pixel_tolerance_pct`
/// - `ThresholdExceeded { metrics, breached: "byte-diff" }` above threshold
/// - `NoReference { path }` if the golden file can't be opened
pub fn compare_bytes_to_golden(config: &Config, actual: &[u8]) -> Outcome {
    let Ok(expected) = std::fs::read(&config.path) else {
        return Outcome::NoReference {
            path: config.path.clone(),
        };
    };
    compare_bytes_against(config, actual, &expected)
}

/// Pure-data comparison helper (no filesystem access). Useful for tests that
/// want to exercise the metric-computation without touching disk.
#[must_use]
pub fn compare_bytes_against(config: &Config, actual: &[u8], expected: &[u8]) -> Outcome {
    let metrics = compute_byte_metrics(actual, expected);
    if metrics.pixel_diff_pct <= config.pixel_tolerance_pct {
        Outcome::Ok { metrics }
    } else {
        Outcome::ThresholdExceeded {
            metrics,
            breached: "byte-diff",
        }
    }
}

/// Compute the byte-diff metrics between `actual` + `expected`.
/// `ssim` + `flip` are zero-filled (computed by perceptual-metric layers).
///
/// Handling of length-mismatch : the diff-count includes every byte in the
/// larger buffer that has no counterpart in the smaller. `pixel_diff_pct` is
/// normalized by the longer length.
#[must_use]
#[allow(clippy::cast_precision_loss)] // len ≤ 2^32-1 in practice ; the loss is at the far tail
pub fn compute_byte_metrics(actual: &[u8], expected: &[u8]) -> Metrics {
    if actual.is_empty() && expected.is_empty() {
        return Metrics {
            ssim: 1.0,
            flip: 0.0,
            pixel_diff_pct: 0.0,
        };
    }
    let mut diff_count: u64 = 0;
    let min_len = actual.len().min(expected.len());
    for i in 0..min_len {
        if actual[i] != expected[i] {
            diff_count += 1;
        }
    }
    let max_len = actual.len().max(expected.len());
    diff_count += (max_len - min_len) as u64;
    let pct = diff_count as f32 / max_len as f32;
    Metrics {
        ssim: 0.0, // deferred : real SSIM requires image-decode
        flip: 0.0, // deferred : real FLIP requires image-decode
        pixel_diff_pct: pct,
    }
}

/// Write `bytes` to the golden path, creating parent dirs as needed. Used by
/// `csslc test --update-golden` to refresh fixtures after a verified change.
/// Returns `Err` if IO fails — callers should surface this to the operator.
pub fn update_golden(path: &str, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = Path::new(path).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(path, bytes)
}

#[cfg(test)]
mod tests {
    use super::{
        compare_bytes_against, compare_bytes_to_golden, compute_byte_metrics, update_golden,
        Config, Dispatcher, Metrics, Outcome, Stage0Stub,
    };

    #[test]
    fn stub_returns_unimplemented() {
        assert_eq!(
            Stage0Stub.run(&Config::default()),
            Outcome::Stage0Unimplemented
        );
    }

    #[test]
    fn empty_buffers_are_identical() {
        let m = compute_byte_metrics(&[], &[]);
        assert!(m.pixel_diff_pct.abs() < 1e-6);
        assert!((m.ssim - 1.0).abs() < 1e-6);
    }

    #[test]
    fn identical_buffers_report_zero_diff() {
        let data = b"hello, world";
        let m = compute_byte_metrics(data, data);
        assert!(m.pixel_diff_pct.abs() < 1e-6);
    }

    #[test]
    fn one_byte_differs_out_of_ten_reports_ten_percent() {
        let a = b"abcdefghij";
        let b = b"abcdefghiK"; // differs in last byte only
        let m = compute_byte_metrics(a, b);
        assert!((m.pixel_diff_pct - 0.1).abs() < 1e-6);
    }

    #[test]
    fn length_mismatch_counts_toward_diff() {
        let a = b"abc";
        let b = b"abcdef"; // 3 extra bytes
        let m = compute_byte_metrics(a, b);
        // 3 missing bytes / 6 total = 50%.
        assert!((m.pixel_diff_pct - 0.5).abs() < 1e-6);
    }

    #[test]
    fn within_tolerance_reports_ok() {
        let a = b"abcdefghij";
        let b = b"abcdefghij";
        let config = Config {
            path: String::new(),
            ssim_threshold: 0.99,
            flip_threshold: 0.05,
            pixel_tolerance_pct: 0.001,
        };
        let outcome = compare_bytes_against(&config, a, b);
        assert!(matches!(outcome, Outcome::Ok { .. }));
    }

    #[test]
    fn above_tolerance_reports_breach() {
        let a = b"abcdefghij";
        let b = b"ABCDEFGHIJ"; // every byte differs
        let config = Config {
            path: String::new(),
            ssim_threshold: 0.99,
            flip_threshold: 0.05,
            pixel_tolerance_pct: 0.001,
        };
        let outcome = compare_bytes_against(&config, a, b);
        match outcome {
            Outcome::ThresholdExceeded { metrics, breached } => {
                assert_eq!(breached, "byte-diff");
                assert!(metrics.pixel_diff_pct > 0.9);
            }
            other => panic!("expected ThresholdExceeded, got {other:?}"),
        }
    }

    #[test]
    fn missing_reference_reports_no_reference() {
        let config = Config {
            path: String::from("/this/path/absolutely-does-not-exist.bin"),
            ssim_threshold: 0.99,
            flip_threshold: 0.05,
            pixel_tolerance_pct: 0.001,
        };
        let outcome = compare_bytes_to_golden(&config, b"anything");
        match outcome {
            Outcome::NoReference { path } => {
                assert!(path.contains("does-not-exist"));
            }
            other => panic!("expected NoReference, got {other:?}"),
        }
    }

    #[test]
    fn update_golden_roundtrip() {
        let dir = std::env::temp_dir().join("cssl-golden-roundtrip");
        let path = dir.join("fixture.bin");
        let _ = std::fs::remove_file(&path);
        let data = b"\x00\x01\x02\x03";
        update_golden(path.to_str().unwrap(), data).expect("write golden");

        let config = Config {
            path: path.to_string_lossy().to_string(),
            ssim_threshold: 0.99,
            flip_threshold: 0.05,
            pixel_tolerance_pct: 0.001,
        };
        let outcome = compare_bytes_to_golden(&config, data);
        match outcome {
            Outcome::Ok { metrics } => assert!(metrics.pixel_diff_pct.abs() < 1e-6),
            other => panic!("expected Ok, got {other:?}"),
        }
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn metrics_default_is_all_zero() {
        let m = Metrics::default();
        assert!(m.ssim.abs() < 1e-6);
        assert!(m.flip.abs() < 1e-6);
        assert!(m.pixel_diff_pct.abs() < 1e-6);
    }
}
