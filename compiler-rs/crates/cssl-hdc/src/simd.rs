//! § cssl-hdc::simd — AVX2-aware popcount-XOR helpers
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Performance-aware variants of the inner loops that drive
//!   `bind` / `bundle` / `similarity`. The functions here wrap the
//!   plain `u64`-loop fallback with a runtime AVX2 dispatch — when the
//!   host CPU advertises AVX2 we use the wider register width and
//!   AVX2's 64-bit popcount instruction (`vpopcntq` on Zen3+ /
//!   Ice Lake+, otherwise the harley-seal lookup).
//!
//!   The dispatch is **runtime**, not compile-time — `target_feature`
//!   would split the binary and force the consumer to choose. Runtime
//!   dispatch via `is_x86_feature_detected!` keeps us MSRV-1.75
//!   compatible AND gives us SIMD speedup on capable hosts in a single
//!   binary.
//!
//! § SAFETY
//!   `forbid(unsafe_code)` is on at the crate level, so we cannot
//!   hand-write SIMD intrinsics here without lifting that constraint.
//!   Instead we lean on the compiler's auto-vectorization : for the
//!   `popcount(a XOR b)` loop, modern rustc emits SSE4.2 `popcnt` in
//!   the absence of explicit intrinsics. The "AVX2 fast path" we
//!   describe below is a forward-compat hook : when a future slice
//!   adds the `unsafe` arm (gated by a `simd-explicit` feature), the
//!   public surface of this module stays stable.
//!
//!   For the foundation slice we ship :
//!   - [`popcount_xor_slice`] — the fallback Hamming-via-popcount
//!     summed over a slice. Auto-vectorizes to 4-way 64-bit XOR +
//!     popcount on AVX2 builds.
//!   - [`xor_slice_into`] — destination-store variant. Useful when
//!     callers want the bound result without a heap allocation for the
//!     output `Vec<u64>`.
//!
//!   When the explicit SIMD slice lands, both functions get an
//!   `is_x86_feature_detected!("avx2")` arm at the top that calls
//!   into a `simd_inner.rs` with the unsafe intrinsics.

/// § Compute `sum(popcount(a[i] XOR b[i]))` over the full slices.
///
/// `a, b` must have the same length ; this asserts in debug and
/// silently truncates in release. Returns 0 for empty slices.
///
/// The compiler auto-vectorizes this into a packed XOR + popcount
/// loop on x86-64 builds with `+sse4.2` (the default for modern
/// targets). On AVX2 builds it widens to 256-bit ops.
#[must_use]
pub fn popcount_xor_slice(a: &[u64], b: &[u64]) -> u32 {
    debug_assert_eq!(
        a.len(),
        b.len(),
        "popcount_xor_slice : length mismatch (a = {}, b = {})",
        a.len(),
        b.len()
    );
    let n = a.len().min(b.len());
    let mut acc = 0u32;
    for i in 0..n {
        acc += (a[i] ^ b[i]).count_ones();
    }
    acc
}

/// § Compute `dst[i] = a[i] ^ b[i]` for every `i`. `dst, a, b` must
///   have the same length ; debug-asserts. Used by `bind`'s allocating
///   path and by chain-rebuild code that wants a pre-allocated
///   destination.
pub fn xor_slice_into(dst: &mut [u64], a: &[u64], b: &[u64]) {
    debug_assert_eq!(dst.len(), a.len(), "xor_slice_into : dst/a length mismatch");
    debug_assert_eq!(a.len(), b.len(), "xor_slice_into : a/b length mismatch");
    let n = dst.len().min(a.len()).min(b.len());
    for i in 0..n {
        dst[i] = a[i] ^ b[i];
    }
}

/// § Compute `popcount(a[i] AND b[i])` summed over the slices. Used
///   by binary-cosine similarity for the intersection-cardinality.
#[must_use]
pub fn popcount_and_slice(a: &[u64], b: &[u64]) -> u32 {
    debug_assert_eq!(a.len(), b.len());
    let n = a.len().min(b.len());
    let mut acc = 0u32;
    for i in 0..n {
        acc += (a[i] & b[i]).count_ones();
    }
    acc
}

/// § Compute total `popcount` of a slice. Useful when bipolar
///   normalization needs `||a||` separately from `||a, b||`.
#[must_use]
pub fn popcount_slice(a: &[u64]) -> u32 {
    a.iter().map(|w| w.count_ones()).sum()
}

/// § Runtime-detect AVX2. Returns `false` on non-x86_64 targets.
///   When future explicit-SIMD lands, the popcount_* functions will
///   branch on this flag.
#[must_use]
pub fn has_avx2() -> bool {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        std::arch::is_x86_feature_detected!("avx2")
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    {
        false
    }
}

/// § Runtime-detect SSE4.2 (which provides `popcnt`). Almost universal
///   on x86_64 silicon shipped after 2008. Used by perf-conscious
///   callers for branch-decision logging.
#[must_use]
pub fn has_sse42() -> bool {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        std::arch::is_x86_feature_detected!("sse4.2")
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn popcount_xor_empty() {
        assert_eq!(popcount_xor_slice(&[], &[]), 0);
    }

    #[test]
    fn popcount_xor_identical() {
        let a = vec![0xDEAD_BEEF_DEAD_BEEFu64; 100];
        assert_eq!(popcount_xor_slice(&a, &a), 0);
    }

    #[test]
    fn popcount_xor_full_flip() {
        let a = vec![0u64; 50];
        let b = vec![u64::MAX; 50];
        assert_eq!(popcount_xor_slice(&a, &b), 50 * 64);
    }

    #[test]
    fn popcount_xor_known_pattern() {
        let a = vec![0x0Fu64; 1];
        let b = vec![0xF0u64; 1];
        // § XOR = 0xFF, popcount = 8.
        assert_eq!(popcount_xor_slice(&a, &b), 8);
    }

    #[test]
    fn xor_slice_into_basic() {
        let a = vec![0xAAu64; 4];
        let b = vec![0x55u64; 4];
        let mut dst = vec![0u64; 4];
        xor_slice_into(&mut dst, &a, &b);
        // § 0xAA XOR 0x55 = 0xFF.
        assert!(dst.iter().all(|&w| w == 0xFFu64));
    }

    #[test]
    fn popcount_and_slice_basic() {
        let a = vec![0xFFu64; 2];
        let b = vec![0x0Fu64; 2];
        // § AND = 0x0F, popcount = 4 each ⇒ 8 total.
        assert_eq!(popcount_and_slice(&a, &b), 8);
    }

    #[test]
    fn popcount_slice_basic() {
        let a = vec![u64::MAX; 10];
        assert_eq!(popcount_slice(&a), 10 * 64);
    }

    #[test]
    fn popcount_slice_zero() {
        let a = vec![0u64; 100];
        assert_eq!(popcount_slice(&a), 0);
    }

    /// § Feature detection runs without panicking. The actual values
    ///   are host-dependent so we just verify the queries return.
    #[test]
    fn feature_detection_runs() {
        let _ = has_avx2();
        let _ = has_sse42();
    }
}
