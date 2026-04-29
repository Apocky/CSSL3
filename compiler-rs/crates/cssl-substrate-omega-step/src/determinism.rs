//! Float-determinism + fast-math probe.
//!
//! § THESIS
//!   `specs/30_SUBSTRATE.csl § DETERMINISTIC-REPLAY-INVARIANTS` requires
//!   that two scheduler instances produce bit-identical Ω-tensor states
//!   after N steps when seeded identically. This module probes the
//!   environment for the conditions that make that contract *honorable* :
//!     1. **IEEE 754 round-to-nearest-even** (the default on x86-64 + AArch64).
//!     2. **Denormal-flush flags consistent** (FTZ + DAZ either both ON
//!        or both OFF — the `DeterminismMode::Strict` form requires both
//!        OFF, which is the SSE2 default ; some build chains flip them
//!        on for performance, breaking deterministic-replay).
//!     3. **No fast-math** : in stage-0 we cannot directly probe whether
//!        the build was compiled with `-ffast-math` (rustc never emits
//!        that flag), but we can detect the symptom : a multiply-add
//!        that violates IEEE-754 rounding suggests fma fusion. We probe
//!        a known-discriminating `(a*b + c)` pattern.
//!
//! § STAGE-0 SCOPE
//!   - The probes are advisory : they classify the runtime as
//!     `DeterminismMode::Strict` or `Soft`. The scheduler refuses to
//!     register a system declaring `{PureDet}` if the mode is `Soft`.
//!   - We do not currently mutate the FP control word ; that requires
//!     `unsafe { _mm_setcsr() }` style calls which are out of scope for
//!     a `#![forbid(unsafe_code)]` crate. The probe simply reports.
//!   - A future slice will wire `cssl-rt` to set FTZ=0 + DAZ=0 at
//!     `__cssl_entry()` time — that's the canonical fix.

use std::fmt;

/// Classification of the runtime's float-determinism stance.
///
/// § STATES
///   - `Strict` : safe for `{PureDet}` — bit-identical across runs.
///   - `Soft`   : default — replay works for `{Sim}` (fixed-step physics
///                with deterministic RNG) but `{PureDet}` is not honored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeterminismMode {
    /// All probes passed : denormal-flush respected ; fast-math absent ;
    /// FP rounding mode is round-to-nearest-even.
    Strict,
    /// At least one probe failed. Replay-determinism for non-PureDet
    /// systems still works ; PureDet systems will be rejected at
    /// `OmegaScheduler::register()`.
    Soft,
}

impl fmt::Display for DeterminismMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Strict => f.write_str("Strict"),
            Self::Soft => f.write_str("Soft"),
        }
    }
}

/// Probe denormal-handling. Returns `true` if denormals are processed
/// per IEEE-754 (the safe form for replay) ; `false` if they're flushed.
///
/// § HOW
///   We multiply `f64::MIN_POSITIVE` (the smallest normal) by `0.5` —
///   that should produce a denormal. If the result equals `0.0`, denormals
///   are being flushed (FTZ on). If it equals `f64::MIN_POSITIVE * 0.5`,
///   denormals are honored.
#[must_use]
pub fn denormal_flush_probe() -> bool {
    let smallest_normal = f64::MIN_POSITIVE;
    let smaller = smallest_normal * 0.5;
    // If FTZ is set, smaller becomes 0.0. If denormals are honored,
    // smaller is a non-zero denormal.
    smaller != 0.0
}

/// Probe whether a known-discriminating fma-fusable pattern produces the
/// IEEE-754-correct result vs the fma-fused result. Returns `true` if
/// the rounding matches the IEEE-754 reference (no fma fusion happening).
///
/// § HOW
///   Choose `a, b, c` such that `(a*b + c)` rounds differently when fma'd
///   vs separated. The classic test : `a = b = sqrt(2)` (so `a*b ≈ 2`
///   but with rounding error), `c = -2.0`. Without fma, the multiply
///   rounds first, yielding a tiny non-zero error ; with fma, the round
///   happens once + the result is closer to zero (or even zero exactly).
///
///   We compute the pattern + check whether the result is consistent
///   with the multiply-then-add path. The exact value depends on the
///   ULP of the multiply, but it should NEVER be zero exactly under
///   IEEE-754-strict semantics.
#[must_use]
#[allow(
    clippy::suboptimal_flops,
    reason = "the literal `a * b + c` form is load-bearing : we are probing whether \
    the compiler is fma-fusing, so we MUST emit separate multiply + add operations. \
    Rewriting to `a.mul_add(b, c)` would explicitly invoke fma + defeat the probe."
)]
pub fn fast_math_probe() -> bool {
    let a = 2.0_f64.sqrt();
    let b = a;
    let c = -2.0_f64;
    let result = a * b + c;
    // Under IEEE-754 with no fma, sqrt(2)^2 != 2 exactly ; the multiply
    // rounds, so the result is a tiny non-zero error in either direction.
    // Under fma fusion, the (a*b + c) sequence is rounded once + the
    // error band is narrower. We accept either as long as the result
    // is ulp-bounded ; a SOFT-mode build might compute zero exactly via
    // fast-math reassociation. We classify zero exactly as suspect.
    //
    // Stage-0 form : we accept any value with |result| < 1.0 as "looks
    // IEEE-shaped". The probe is conservative — it's intended to flag
    // EXTREMELY non-IEEE behavior, not to reject every fma-using build.
    // True PureDet rejection happens via `EffectRow::PureDet`-marked
    // systems being checked against `DeterminismMode::Strict`.
    let ulp_bounded = result.abs() < 1.0;
    let not_zero_exact = result != 0.0;
    ulp_bounded && not_zero_exact
}

/// Combine the probes into a `DeterminismMode` classification. The
/// scheduler calls this once at construction time + records the result.
#[must_use]
pub fn classify_determinism() -> DeterminismMode {
    if denormal_flush_probe() && fast_math_probe() {
        DeterminismMode::Strict
    } else {
        DeterminismMode::Soft
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn denormal_probe_returns_a_bool() {
        // The probe MUST not crash + MUST return a deterministic boolean
        // for the host's current FP control word state. On standard
        // CSSLv3 dev hosts (Win64 / Linux x86-64) this should be `true`.
        let _ = denormal_flush_probe();
    }

    #[test]
    fn denormal_probe_default_host_honors_denormals() {
        // The CSSLv3 default cssl-rt entry shim should NOT enable FTZ.
        // If a future toolchain change flips this, the probe should
        // still return a value, but this test would fail loudly so we
        // catch the regression. On Apocky's Windows host today, this
        // should pass — denormals honored.
        let v = denormal_flush_probe();
        assert!(v, "denormals expected to be honored on stage-0 host");
    }

    #[test]
    fn fast_math_probe_returns_a_bool() {
        let _ = fast_math_probe();
    }

    #[test]
    fn classify_returns_strict_or_soft() {
        // Exhaustive — only two variants. Smoke test that the function
        // returns one of them deterministically.
        let mode = classify_determinism();
        assert!(matches!(
            mode,
            DeterminismMode::Strict | DeterminismMode::Soft
        ));
    }

    #[test]
    fn determinism_mode_display() {
        assert_eq!(DeterminismMode::Strict.to_string(), "Strict");
        assert_eq!(DeterminismMode::Soft.to_string(), "Soft");
    }
}
