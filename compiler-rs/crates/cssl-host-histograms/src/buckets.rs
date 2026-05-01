//! # Bucket index / range mapping for HDR-style power-of-2 histograms
//!
//! ## Mapping
//!
//! - 64 buckets indexed `[0, 64)`.
//! - Bucket `i` covers values in `[2^i, 2^(i+1))` for `i ∈ [0, 63)`.
//! - Bucket `0` additionally absorbs `0` (special-case ; lower-bound = 0).
//! - Bucket `63` is the saturation bucket : it covers `[2^63, u64::MAX]` so
//!   any value at or above `2^63` µs (~292 471 years) lands here. In practice
//!   nothing reaches that range ; the saturation bucket is a safety net so
//!   `record` is total over `u64`.
//!
//! ## Coverage at a glance (in µs / human-grain)
//!
//! | bucket | lower (µs) | upper (µs) | human-grain               |
//! |-------:|-----------:|-----------:|---------------------------|
//! | 0      | 0          | 1          | sub-µs                    |
//! | 10     | 1024       | 2047       | ~1 ms                     |
//! | 14     | 16384      | 32767      | 16 ms (60-fps frame-time) |
//! | 17     | 131072     | 262143     | ~131 ms                   |
//! | 20     | 1048576    | 2097151    | ~1 s                      |
//! | 30     | ~1 G       | ~2 G       | ~17 min                   |
//! | 40     | ~1 T       | ~2 T       | ~12 days                  |
//! | 63     | 2^63       | u64::MAX   | saturation                |
//!
//! ## Why 64 ?
//!
//! `u64::leading_zeros` returns a value in `[0, 64]`, so the bucket-index
//! computation is one branchless intrinsic + one subtraction. Larger bucket
//! counts would need finer sub-bucketing per power-of-two ; 64 is the
//! "16-precision" coarse-grain that the spec calls for and is the hot-path
//! O(1) variant.

/// Total number of histogram buckets.
///
/// Covers the full `u64` µs range : sub-µs (bucket 0) through saturation
/// (bucket 63 = `[2^63, u64::MAX]`).
pub const BUCKETS: usize = 64;

/// Map a sample value (in microseconds) to a bucket index in `[0, BUCKETS)`.
///
/// - `0` falls into bucket `0` (special-case).
/// - `value ∈ [2^i, 2^(i+1))` for `i ∈ [0, 63)` falls into bucket `i`.
/// - `value ∈ [2^63, u64::MAX]` falls into bucket `63` (saturation).
///
/// Cost : one branch (zero-check) + one `leading_zeros` intrinsic + one
/// subtraction. Branchless on the hot path for non-zero inputs.
#[inline]
#[must_use]
pub fn bucket_index(value_us: u64) -> usize {
    if value_us == 0 {
        return 0;
    }
    // 64 - leading_zeros(v) - 1 = floor(log2(v)) ∈ [0, 63] for v ≥ 1
    let lz = value_us.leading_zeros() as usize;
    BUCKETS - 1 - lz
}

/// Lower bound (inclusive) of bucket `idx`, in microseconds.
///
/// - `idx = 0` → `0` (the zero-value special-case).
/// - `idx ∈ [1, 63]` → `2^idx`.
///
/// Panics in debug builds if `idx >= BUCKETS` ; saturates to bucket 63's
/// lower bound in release builds via the `min` clamp.
#[inline]
#[must_use]
pub fn bucket_lower_bound(idx: usize) -> u64 {
    debug_assert!(idx < BUCKETS, "bucket index out of range");
    let i = idx.min(BUCKETS - 1);
    if i == 0 {
        0
    } else {
        1u64 << i
    }
}

/// Upper bound (exclusive for non-saturation buckets, inclusive for bucket 63)
/// of bucket `idx`, in microseconds.
///
/// - `idx ∈ [0, 62]` → `2^(idx+1)` (next power-of-two ; exclusive bound).
/// - `idx = 63` → `u64::MAX` (saturation : no overflow possible).
///
/// Panics in debug builds if `idx >= BUCKETS` ; saturates to `u64::MAX` in
/// release builds via the clamp.
#[inline]
#[must_use]
pub fn bucket_upper_bound(idx: usize) -> u64 {
    debug_assert!(idx < BUCKETS, "bucket index out of range");
    let i = idx.min(BUCKETS - 1);
    if i >= BUCKETS - 1 {
        u64::MAX
    } else {
        1u64 << (i + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// § zero falls into bucket zero (the special-case branch).
    #[test]
    fn zero_falls_into_bucket_zero() {
        assert_eq!(bucket_index(0), 0);
    }

    /// § monotonic bucket bounds : lower(i) < lower(i+1) AND upper(i) <= upper(i+1).
    /// Validates the ordering invariant the percentile-walker relies on.
    #[test]
    fn monotonic_bucket_bounds() {
        for i in 0..BUCKETS - 1 {
            let lo_i = bucket_lower_bound(i);
            let lo_next = bucket_lower_bound(i + 1);
            let up_i = bucket_upper_bound(i);
            let up_next = bucket_upper_bound(i + 1);
            assert!(
                lo_i <= lo_next,
                "lower bound not monotonic at i={i}: {lo_i} > {lo_next}",
            );
            assert!(
                up_i <= up_next,
                "upper bound not monotonic at i={i}: {up_i} > {up_next}",
            );
        }
    }

    /// § saturation bucket absorbs the largest representable values.
    #[test]
    fn upper_saturates() {
        // u64::MAX has leading_zeros=0 → bucket 63
        assert_eq!(bucket_index(u64::MAX), BUCKETS - 1);
        // A value just above 2^63 also lands in bucket 63
        assert_eq!(bucket_index(1u64 << 63), BUCKETS - 1);
        // Saturation bucket's upper bound is u64::MAX
        assert_eq!(bucket_upper_bound(BUCKETS - 1), u64::MAX);
    }

    /// § round-trip : a value inside bucket `i` maps back to bucket `i`,
    /// and the lower-bound of `i` maps back to `i`.
    #[test]
    fn round_trip_bucket_bounds() {
        for i in 1..BUCKETS - 1 {
            let lo = bucket_lower_bound(i);
            assert_eq!(
                bucket_index(lo),
                i,
                "lower-bound round-trip failed at i={i}: lo={lo}",
            );
            // Mid-point inside bucket i : 1.5 * 2^i = 2^i + 2^(i-1)
            let mid = lo + (lo >> 1).max(1);
            // mid may overflow into next bucket if lo == 2^62 (mid would be 2^62 + 2^61),
            // still in bucket 62 ; verify in-range cases :
            if mid < bucket_upper_bound(i) {
                assert_eq!(
                    bucket_index(mid),
                    i,
                    "mid-bucket round-trip failed at i={i}: mid={mid}",
                );
            }
        }
        // Bucket 0 special-case : value 0 → bucket 0
        assert_eq!(bucket_index(0), 0);
        // Value 1 : leading_zeros=63 → bucket 0 (covers [1, 2))
        assert_eq!(bucket_index(1), 0);
        // Value 2 : leading_zeros=62 → bucket 1
        assert_eq!(bucket_index(2), 1);
    }

    /// § 16ms frame-time (60 fps) : 16384 µs is the lower-bound of bucket 14.
    /// A typical 16.7 ms frame at 60 fps lands here.
    #[test]
    fn sixteen_ms_frame_time_typical_bucket() {
        // 16 ms = 16_000 µs ∈ [16384? no — 16384 is 2^14] — so 16000 < 16384 → bucket 13
        // but 17_000 µs > 16_384 → bucket 14.
        // The "typical 60-fps frame" of 16_667 µs lands in bucket 14 ([16384, 32768)).
        let frame_60fps_us: u64 = 16_667;
        assert_eq!(bucket_index(frame_60fps_us), 14);
        assert_eq!(bucket_lower_bound(14), 16_384);
        assert_eq!(bucket_upper_bound(14), 32_768);
        // 33ms (30 fps) lands in bucket 15 ([32768, 65536))
        assert_eq!(bucket_index(33_333), 15);
    }
}
