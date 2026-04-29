//! § boundary — boundary-extension policies for finite-length wavelet transforms
//!
//! A length-N signal convolved against an L-tap wavelet filter needs L − 1
//! extra samples beyond the array boundary to keep the convolution defined
//! at every output position. The classical wavelet literature offers three
//! standard policies for what those extra samples should be :
//!
//!   - **Periodic** : the signal wraps around — `x[N + k] = x[k]`. This
//!     is what every fast-DWT reference implementation uses by default
//!     because the signal length is then exactly preserved (no boundary
//!     samples leak into the output). The downside is the implicit
//!     assumption that the signal is periodic ; a sharp jump from
//!     `x[N - 1]` to `x[0]` becomes a synthetic detail spike at every
//!     scale.
//!
//!   - **Symmetric** (a.k.a. "whole-point symmetric") : the signal mirrors
//!     about the boundary — `x[-k] = x[k]`, `x[N + k] = x[N - 1 - k]`.
//!     This avoids the synthetic-spike artifact at the cost of a slight
//!     length-extension at each level (handled internally so callers see
//!     length-preserved coefficients).
//!
//!   - **Zero** : the signal is zero outside `[0, N)`. The simplest
//!     policy, but introduces a "ramp-down to zero" boundary energy.
//!
//! All three modes are part of the public surface ; `Periodic` is the
//! default to match the standard reference implementation. All three
//! preserve perfect reconstruction when the inverse pass uses the same
//! boundary mode as the forward pass.

/// § Boundary-extension policy for finite-length wavelet transforms.
///
/// Pass to every `forward_*` and `inverse_*` call ; the same mode must be
/// used on the round-trip pair for perfect reconstruction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BoundaryMode {
    /// `x[N + k] = x[k mod N]` — wraps-around. The default ; matches
    /// every reference fast-DWT implementation.
    #[default]
    Periodic,
    /// `x[-k] = x[k]`, `x[N + k] = x[N - 1 - k]` — whole-point mirror.
    /// Good for natural-image / scientific signals where the
    /// periodic-wrap-around assumption is an artifact.
    Symmetric,
    /// `x[N + k] = 0` — zeroes outside `[0, N)`. Simplest ; good for
    /// strictly-bounded signals (e.g. impulse response with a known
    /// zero tail).
    Zero,
}

/// § Sample the signal at index `idx` (which may be outside `[0, n)`)
/// using the requested boundary mode. `n` is the signal length.
///
/// The function is total : every `(idx, n, mode)` combination returns a
/// finite value. For `Zero` mode and `idx ∉ [0, n)`, the returned value
/// is `0.0`. For `Periodic` and `Symmetric`, the index wraps / mirrors
/// to a valid position in `[0, n)`.
#[must_use]
pub fn sample_at(signal: &[f32], idx: isize, mode: BoundaryMode) -> f32 {
    let n = signal.len() as isize;
    if n == 0 {
        return 0.0;
    }
    if idx >= 0 && idx < n {
        return signal[idx as usize];
    }
    match mode {
        BoundaryMode::Periodic => {
            // Rust's `%` is the truncated remainder ; we want the
            // mathematical-modulo so that `(-1) mod n = n - 1`.
            let m = ((idx % n) + n) % n;
            signal[m as usize]
        }
        BoundaryMode::Symmetric => {
            // Reflect about the boundary : -1 → 0, -2 → 1, n → n-1, n+1 → n-2.
            // Use the period 2n - 2 (whole-point mirror).
            let period = 2 * n - 2;
            if period == 0 {
                return signal[0]; // n == 1 : single sample, replicate
            }
            let mut m = ((idx % period) + period) % period;
            if m >= n {
                m = period - m;
            }
            signal[m as usize]
        }
        BoundaryMode::Zero => 0.0,
    }
}

/// § Build the periodic extension of `signal` to a length-`n + pad_left + pad_right`
/// buffer. Useful when a kernel wants a padded view it can convolve over with
/// a contiguous slice.
#[must_use]
pub fn extend_periodic(signal: &[f32], pad_left: usize, pad_right: usize) -> Vec<f32> {
    let n = signal.len();
    let mut out = Vec::with_capacity(n + pad_left + pad_right);
    if n == 0 {
        out.resize(pad_left + pad_right, 0.0);
        return out;
    }
    for i in 0..pad_left {
        let src = (n - ((pad_left - i) % n)) % n;
        out.push(signal[src]);
    }
    out.extend_from_slice(signal);
    for i in 0..pad_right {
        out.push(signal[i % n]);
    }
    out
}

/// § Build the symmetric (whole-point-mirror) extension.
#[must_use]
pub fn extend_symmetric(signal: &[f32], pad_left: usize, pad_right: usize) -> Vec<f32> {
    let n = signal.len();
    let mut out = Vec::with_capacity(n + pad_left + pad_right);
    if n == 0 {
        out.resize(pad_left + pad_right, 0.0);
        return out;
    }
    for i in 0..pad_left {
        let idx = -((pad_left - i) as isize);
        out.push(sample_at(signal, idx, BoundaryMode::Symmetric));
    }
    out.extend_from_slice(signal);
    for i in 0..pad_right {
        let idx = (n + i) as isize;
        out.push(sample_at(signal, idx, BoundaryMode::Symmetric));
    }
    out
}

/// § Build the zero-extended buffer.
#[must_use]
pub fn extend_zero(signal: &[f32], pad_left: usize, pad_right: usize) -> Vec<f32> {
    let n = signal.len();
    let mut out = Vec::with_capacity(n + pad_left + pad_right);
    out.resize(pad_left, 0.0);
    out.extend_from_slice(signal);
    out.resize(out.len() + pad_right, 0.0);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn periodic_in_range() {
        let s = [1.0, 2.0, 3.0, 4.0];
        assert!((sample_at(&s, 0, BoundaryMode::Periodic) - 1.0).abs() < 1e-6);
        assert!((sample_at(&s, 3, BoundaryMode::Periodic) - 4.0).abs() < 1e-6);
    }

    #[test]
    fn periodic_negative_wraps() {
        let s = [1.0, 2.0, 3.0, 4.0];
        assert!((sample_at(&s, -1, BoundaryMode::Periodic) - 4.0).abs() < 1e-6);
        assert!((sample_at(&s, -4, BoundaryMode::Periodic) - 1.0).abs() < 1e-6);
        assert!((sample_at(&s, -5, BoundaryMode::Periodic) - 4.0).abs() < 1e-6);
    }

    #[test]
    fn periodic_overflow_wraps() {
        let s = [1.0, 2.0, 3.0, 4.0];
        assert!((sample_at(&s, 4, BoundaryMode::Periodic) - 1.0).abs() < 1e-6);
        assert!((sample_at(&s, 7, BoundaryMode::Periodic) - 4.0).abs() < 1e-6);
        assert!((sample_at(&s, 100, BoundaryMode::Periodic) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn symmetric_negative_mirrors() {
        let s = [1.0, 2.0, 3.0, 4.0];
        // x[-1] = x[1], x[-2] = x[2]
        assert!((sample_at(&s, -1, BoundaryMode::Symmetric) - 2.0).abs() < 1e-6);
        assert!((sample_at(&s, -2, BoundaryMode::Symmetric) - 3.0).abs() < 1e-6);
    }

    #[test]
    fn symmetric_overflow_mirrors() {
        let s = [1.0, 2.0, 3.0, 4.0];
        // n=4, period = 6 ; x[4] = x[2], x[5] = x[1]
        assert!((sample_at(&s, 4, BoundaryMode::Symmetric) - 3.0).abs() < 1e-6);
        assert!((sample_at(&s, 5, BoundaryMode::Symmetric) - 2.0).abs() < 1e-6);
    }

    #[test]
    fn zero_outside_range() {
        let s = [1.0, 2.0, 3.0, 4.0];
        assert!((sample_at(&s, -1, BoundaryMode::Zero)).abs() < 1e-6);
        assert!((sample_at(&s, 4, BoundaryMode::Zero)).abs() < 1e-6);
        assert!((sample_at(&s, 100, BoundaryMode::Zero)).abs() < 1e-6);
    }

    #[test]
    fn extend_periodic_pads_correctly() {
        let s = [1.0, 2.0, 3.0, 4.0];
        let ext = extend_periodic(&s, 2, 2);
        assert_eq!(ext.len(), 8);
        // Left pad = [s[2], s[3]] = [3.0, 4.0]
        assert!((ext[0] - 3.0).abs() < 1e-6);
        assert!((ext[1] - 4.0).abs() < 1e-6);
        // Center = signal
        assert!((ext[2] - 1.0).abs() < 1e-6);
        assert!((ext[5] - 4.0).abs() < 1e-6);
        // Right pad = [s[0], s[1]]
        assert!((ext[6] - 1.0).abs() < 1e-6);
        assert!((ext[7] - 2.0).abs() < 1e-6);
    }

    #[test]
    fn extend_symmetric_mirrors_at_boundaries() {
        let s = [1.0, 2.0, 3.0, 4.0];
        let ext = extend_symmetric(&s, 2, 2);
        assert_eq!(ext.len(), 8);
        // Left pad : x[-2] = x[2] = 3.0, x[-1] = x[1] = 2.0
        assert!((ext[0] - 3.0).abs() < 1e-6);
        assert!((ext[1] - 2.0).abs() < 1e-6);
        // Right pad : x[4] = x[2] = 3.0, x[5] = x[1] = 2.0
        assert!((ext[6] - 3.0).abs() < 1e-6);
        assert!((ext[7] - 2.0).abs() < 1e-6);
    }

    #[test]
    fn extend_zero_pads_with_zeros() {
        let s = [1.0, 2.0, 3.0, 4.0];
        let ext = extend_zero(&s, 2, 2);
        assert_eq!(ext.len(), 8);
        assert!(ext[0].abs() < 1e-6);
        assert!(ext[1].abs() < 1e-6);
        assert!(ext[6].abs() < 1e-6);
        assert!(ext[7].abs() < 1e-6);
    }

    #[test]
    fn boundary_mode_default_is_periodic() {
        let m: BoundaryMode = Default::default();
        assert_eq!(m, BoundaryMode::Periodic);
    }
}
