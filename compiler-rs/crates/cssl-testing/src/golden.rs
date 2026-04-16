//! Golden-image oracle (`@golden`) — pixel + SSIM + FLIP.
//!
//! § SPEC    : `specs/23_TESTING.csl` § golden-image-framework.
//! § LOCATION: reference frames under `compiler-rs/tests/golden/<bench-id>/*.png` (or HDR).
//! § METRICS :
//!   - pixel-diff : tolerance per-percentile, configurable.
//!   - SSIM       : structural-similarity > 0.99 default threshold.
//!   - FLIP       : NVIDIA perceptual-diff metric for human-aligned comparison.
//! § UPDATE  : `csslc test --update-golden` (T11+) after verified-change.
//! § STATUS  : T10+ stub — implementation pending.

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

#[cfg(test)]
mod tests {
    use super::{Config, Dispatcher, Outcome, Stage0Stub};

    #[test]
    fn stub_returns_unimplemented() {
        assert_eq!(
            Stage0Stub.run(&Config::default()),
            Outcome::Stage0Unimplemented
        );
    }
}
