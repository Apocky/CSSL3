// § diff.rs : per-pixel RGBA / RGB diff with tolerance
// ══════════════════════════════════════════════════════════════════
// § I> DiffReport summarizes pixel-level deltas between two RGBA buffers
// § I> tolerance_per_channel : if max channel-delta of pixel ≤ tolerance ⇒ ¬ count
// § I> diff_rgba   ← all 4 channels
// § I> diff_rgb    ← skip alpha (channels 3, 7, 11, …)

use serde::{Deserialize, Serialize};
use std::fmt;

/// Summary report of a pairwise pixel diff.
///
/// `pixels_diff` counts pixels with *any* nonzero channel-delta ;
/// `pixels_above_tolerance` counts pixels with at least one channel-delta
/// strictly greater than `tolerance_per_channel`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DiffReport {
    pub pixels_total: u64,
    pub pixels_diff: u64,
    pub pixels_above_tolerance: u64,
    pub max_channel_delta: u8,
    pub mean_channel_delta: f32,
    pub percent_diff: f32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiffErr {
    LengthMismatch { left: usize, right: usize },
    EmptyInput,
}

impl fmt::Display for DiffErr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LengthMismatch { left, right } => {
                write!(f, "diff length mismatch : left={left} right={right}")
            }
            Self::EmptyInput => write!(f, "diff buffers must be non-empty"),
        }
    }
}

impl std::error::Error for DiffErr {}

/// Diff RGBA buffers including the alpha channel.
///
/// `tolerance_per_channel` : per-channel signed-magnitude threshold. A pixel
/// is counted in `pixels_above_tolerance` iff *any* of its 4 channel-deltas
/// exceeds this value. Set to `0` for strict bit-equality.
pub fn diff_rgba(
    left: &[u8],
    right: &[u8],
    tolerance_per_channel: u8,
) -> Result<DiffReport, DiffErr> {
    diff_inner(left, right, tolerance_per_channel, 4, true)
}

/// Diff RGBA buffers ignoring the alpha channel.
///
/// Length must still be a multiple of 4 (caller passes RGBA buffers) ; only
/// the R, G, B channels of each pixel are compared.
pub fn diff_rgb(
    left: &[u8],
    right: &[u8],
    tolerance_per_channel: u8,
) -> Result<DiffReport, DiffErr> {
    diff_inner(left, right, tolerance_per_channel, 4, false)
}

fn diff_inner(
    left: &[u8],
    right: &[u8],
    tol: u8,
    stride: usize,
    include_alpha: bool,
) -> Result<DiffReport, DiffErr> {
    if left.len() != right.len() {
        return Err(DiffErr::LengthMismatch { left: left.len(), right: right.len() });
    }
    if left.is_empty() {
        return Err(DiffErr::EmptyInput);
    }
    if left.len() % stride != 0 {
        return Err(DiffErr::LengthMismatch { left: left.len(), right: right.len() });
    }
    let pixels_total = (left.len() / stride) as u64;
    let chans_compared = if include_alpha { stride } else { stride - 1 };

    let mut pixels_diff: u64 = 0;
    let mut pixels_above_tol: u64 = 0;
    let mut max_delta: u8 = 0;
    let mut sum_delta: u64 = 0;

    for px_idx in 0..(pixels_total as usize) {
        let base = px_idx * stride;
        let mut any_diff = false;
        let mut any_above_tol = false;
        for c in 0..chans_compared {
            let l = left[base + c];
            let r = right[base + c];
            let d = if l > r { l - r } else { r - l };
            sum_delta += u64::from(d);
            if d > 0 {
                any_diff = true;
            }
            if d > tol {
                any_above_tol = true;
            }
            if d > max_delta {
                max_delta = d;
            }
        }
        if any_diff {
            pixels_diff += 1;
        }
        if any_above_tol {
            pixels_above_tol += 1;
        }
    }

    let total_chan_samples = pixels_total * (chans_compared as u64);
    let mean_channel_delta = if total_chan_samples == 0 {
        0.0
    } else {
        (sum_delta as f64 / total_chan_samples as f64) as f32
    };
    let percent_diff = if pixels_total == 0 {
        0.0
    } else {
        (pixels_above_tol as f64 / pixels_total as f64 * 100.0) as f32
    };

    Ok(DiffReport {
        pixels_total,
        pixels_diff,
        pixels_above_tolerance: pixels_above_tol,
        max_channel_delta: max_delta,
        mean_channel_delta,
        percent_diff,
    })
}

// ─────────────────────────────────────────────────────────────────
// tests
// ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_zero_diff() {
        let a = vec![10, 20, 30, 255, 40, 50, 60, 255];
        let r = diff_rgba(&a, &a, 0).unwrap();
        assert_eq!(r.pixels_total, 2);
        assert_eq!(r.pixels_diff, 0);
        assert_eq!(r.pixels_above_tolerance, 0);
        assert_eq!(r.max_channel_delta, 0);
        assert!((r.mean_channel_delta - 0.0).abs() < 1e-6);
        assert!((r.percent_diff - 0.0).abs() < 1e-6);
    }

    #[test]
    fn single_pixel_mismatch() {
        let a = vec![0, 0, 0, 255, 0, 0, 0, 255];
        let b = vec![0, 0, 0, 255, 5, 0, 0, 255];
        let r = diff_rgba(&a, &b, 0).unwrap();
        assert_eq!(r.pixels_total, 2);
        assert_eq!(r.pixels_diff, 1);
        assert_eq!(r.pixels_above_tolerance, 1);
        assert_eq!(r.max_channel_delta, 5);
    }

    #[test]
    fn all_channels_delta() {
        let a = vec![0, 0, 0, 0];
        let b = vec![10, 20, 30, 40];
        let r = diff_rgba(&a, &b, 0).unwrap();
        assert_eq!(r.pixels_total, 1);
        assert_eq!(r.pixels_diff, 1);
        assert_eq!(r.pixels_above_tolerance, 1);
        assert_eq!(r.max_channel_delta, 40);
        // mean across 4 channels = (10+20+30+40)/4 = 25
        assert!((r.mean_channel_delta - 25.0).abs() < 1e-3);
    }

    #[test]
    fn alpha_ignored_mode() {
        // alpha differs by 200 ; RGB identical → diff_rgb should report no diff
        let a = vec![10, 20, 30, 0];
        let b = vec![10, 20, 30, 200];
        let r_full = diff_rgba(&a, &b, 0).unwrap();
        let r_rgb = diff_rgb(&a, &b, 0).unwrap();
        assert_eq!(r_full.pixels_diff, 1);
        assert_eq!(r_rgb.pixels_diff, 0);
        assert_eq!(r_rgb.pixels_above_tolerance, 0);
        assert_eq!(r_rgb.max_channel_delta, 0);
    }

    #[test]
    fn length_mismatch() {
        let a = vec![0; 8];
        let b = vec![0; 12];
        let err = diff_rgba(&a, &b, 0).unwrap_err();
        matches!(err, DiffErr::LengthMismatch { .. });
        let err2 = diff_rgb(&a, &b, 0).unwrap_err();
        matches!(err2, DiffErr::LengthMismatch { .. });
    }

    #[test]
    fn tolerance_suppresses_tiny_deltas() {
        // pixel with channel-delta 3 ; tolerance 5 ⇒ pixels_diff=1 but pixels_above_tolerance=0
        let a = vec![0, 0, 0, 255, 100, 100, 100, 255];
        let b = vec![3, 0, 0, 255, 100, 100, 100, 255];
        let r = diff_rgba(&a, &b, 5).unwrap();
        assert_eq!(r.pixels_diff, 1);
        assert_eq!(r.pixels_above_tolerance, 0);
        assert_eq!(r.max_channel_delta, 3);
    }

    #[test]
    fn empty_input_rejected() {
        let a: Vec<u8> = vec![];
        let err = diff_rgba(&a, &a, 0).unwrap_err();
        assert_eq!(err, DiffErr::EmptyInput);
    }

    #[test]
    fn non_multiple_of_stride_rejected() {
        let a = vec![0u8; 5];
        let b = vec![0u8; 5];
        let err = diff_rgba(&a, &b, 0).unwrap_err();
        matches!(err, DiffErr::LengthMismatch { .. });
    }
}
