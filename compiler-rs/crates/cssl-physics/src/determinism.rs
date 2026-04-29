//! Float-determinism helpers.
//!
//! § THESIS
//!   `specs/30_SUBSTRATE.csl § OMEGA-STEP § DETERMINISTIC-REPLAY-INVARIANTS`
//!   requires that physics simulation be bit-equal across runs given identical
//!   inputs. The three traditional sources of float non-determinism are :
//!     1. Denormal-handling (FTZ/DAZ flags)
//!     2. Fused-multiply-add (FMA) instructions reordering operations
//!     3. Compiler-issued fast-math reassociations
//!
//!   This module supplies probes for (1) and (2), and the build-config note
//!   for (3). The H2 omega-step crate has its own probes ; we re-implement
//!   here so the physics crate is testable in isolation.
//!
//! § FTZ/DAZ
//!   Flush-to-zero / denormals-are-zero. When enabled, denormal floats (very
//!   tiny non-zero values) are treated as zero for arithmetic. This matters
//!   for physics because denormals appear at the extremes of contact-resolution
//!   (near-zero relative velocities, almost-resting bodies) and their handling
//!   varies by CPU generation. We require the caller to enable FTZ/DAZ for
//!   deterministic replay ; we PROBE whether they're enabled at world-construction
//!   time and report via the return value of `flush_denormals_to_zero`.
//!
//!   Stage-0 form : we don't actually flip the MXCSR register here (that
//!   requires `unsafe` + platform-specific intrinsics ; cssl-physics has
//!   `#![forbid(unsafe_code)]`). Instead, we DOCUMENT the requirement +
//!   provide a test-helper that detects denormal-creation. The H2 scheduler's
//!   `denormal_flush_probe` does the actual flag-check via x86_64 intrinsics
//!   in a separate crate that has the unsafe-allow.
//!
//! § FMA
//!   Fused-multiply-add issues `a*b+c` as a single instruction with one
//!   rounding step instead of two. Result : different rounding from the
//!   non-fused sequence. This matters when the same expression is compiled
//!   with FMA enabled on one machine and disabled on another — bit-equal
//!   guarantee breaks. We document that physics builds MUST be compiled
//!   with `-Ctarget-feature=-fma` ; the `fmadd_disabled` probe verifies
//!   this at the source-code level (not at the codegen level — that's a
//!   build-system concern, not a runtime concern).

/// Probe whether denormal floats are flushed to zero. Returns `true` if
/// denormals appear to be flushed (i.e., `1e-310 / 1.0 == 0.0`), `false`
/// otherwise.
///
/// § STAGE-0 NOTE
///   This is an indicator probe, not an enforcer. Real physics determinism
///   requires the caller to set MXCSR.FTZ + MXCSR.DAZ via the H2 scheduler's
///   `denormal_flush_probe`. This function lets the physics crate test
///   denormal-behavior in isolation.
#[must_use]
pub fn flush_denormals_to_zero() -> bool {
    // Construct a value that's a denormal in f64 — smallest-positive denormal
    // is ~5e-324. Multiplying by 0.5 should yield 0 if FTZ is on, else a
    // smaller denormal.
    let smallest_normal: f64 = f64::MIN_POSITIVE;
    let denormal = smallest_normal * 0.5;
    // If FTZ is on, denormal arithmetic flushes to zero ;
    // otherwise we get a denormal value.
    denormal == 0.0
}

/// Verify that the current build was compiled WITHOUT FMA contraction.
///
/// § TECHNIQUE
///   Construct a calculation where FMA-vs-non-FMA produces a different
///   bit-pattern. On most x86_64 platforms, the canonical example is :
///     `let r = (a * b) + c ;`
///   with `a = 1.0 + 2^-53`, `b = 1.0 + 2^-53`, `c = -(1.0 + 2^-52)`.
///   Without FMA, the multiply rounds to `1.0 + 2^-52`, then adds `c` to get
///   exactly `0.0`. With FMA, the operation is done with the full intermediate
///   precision, yielding a tiny non-zero residual.
///
/// § STAGE-0 NOTE
///   Returns `true` if the calculation matches the non-FMA expectation
///   (i.e., `r == 0.0`), `false` if a small residual appears (FMA active).
///   This is an indicator probe, not an enforcer ; physics builds MUST set
///   `RUSTFLAGS="-Ctarget-feature=-fma"` for guaranteed FMA-disabled codegen.
#[must_use]
pub fn fmadd_disabled() -> bool {
    let a: f64 = 1.0 + f64::EPSILON;
    let b: f64 = 1.0 + f64::EPSILON;
    let c: f64 = -(1.0 + 2.0 * f64::EPSILON);
    // Use std::hint::black_box to prevent compiler from constant-folding.
    let a = std::hint::black_box(a);
    let b = std::hint::black_box(b);
    let c = std::hint::black_box(c);
    let result = (a * b) + c;
    // If FMA was active, result is non-zero (small residual).
    // If FMA was inactive, result is exactly zero.
    result == 0.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ftz_probe_returns_bool() {
        // Just verify the probe doesn't panic + returns SOME bool.
        let _ = flush_denormals_to_zero();
    }

    #[test]
    fn fma_probe_returns_bool() {
        let _ = fmadd_disabled();
    }
}
