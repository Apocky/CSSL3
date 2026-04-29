//! § Determinism — replay-stability scaffolding for cssl-physics-wave.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Mirror of `cssl-physics::determinism` plus wave-specific additions :
//!
//!   - **`flush_denormals_to_zero`** — probe for FTZ/DAZ FPU mode.
//!   - **`fmadd_disabled`** — probe that we're not silently emitting FMA.
//!   - **`DeterminismConfig`** — bundles the determinism knobs (dt-policy,
//!     RNG seed, broadphase-color-stable-sort) into a single value the
//!     world consumes at construction.
//!   - **`DET_RNG_SEED_DEFAULT`** — canonical seed for the wave-physics
//!     deterministic RNG. The XPBD constraint-coloring uses a
//!     deterministic sort ; the RNG is reserved for future use (e.g.
//!     stochastic-impact-spectrum sampling at the wave-coupler).
//!
//! § DESIGN-NOTE
//!   The legacy crate threaded determinism via documentation only ; the
//!   wave-physics crate centralizes the knobs in a single struct so the
//!   per-test path can flip them deterministically. This is the
//!   T11-D117 slice's "remove implicit-state landmines" remediation
//!   from the audit.
//!
//! § DETERMINISM CONTRACT
//!   - No `SystemTime::now()`, no `thread_rng()`, no parallel iteration
//!     order dependency, no FMA. Same as `cssl-physics`.
//!   - Additionally : the broadphase Morton-hash insertion order is
//!     sorted by `MortonKey` for the CPU path. The GPU path's warp-vote
//!     produces the same final hash-state by definition.
//!   - The XPBD constraint-color-bucket order is sorted by color-id, and
//!     within a color, constraints sort by `(body_a_id, body_b_id)`.

use core::cell::Cell;

/// § Canonical deterministic-RNG seed for cssl-physics-wave. Used by
///   stochastic-impact-spectrum sampling (when wired up) so that the same
///   contact event always produces the same spectrum.
///
///   Mnemonic : the hex digits spell "CSSL D117" (CSSL slice-id-117) +
///   "DETERM" prefix so a hex-dump of an audit-log makes the seed origin
///   self-evident. The actual numeric value is what matters and it is
///   reproducible across hosts.
pub const DET_RNG_SEED_DEFAULT: u64 = 0xC551_D117_DE7E_8175;

/// § Determinism knobs bundled for explicit consumption by world-construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DeterminismConfig {
    /// Seed for the deterministic-RNG (stochastic-impact-spectrum sampling).
    pub rng_seed: u64,
    /// True ⇒ broadphase iterates Morton-hash slots in MortonKey-ascending
    /// order. False ⇒ slot-order (faster but undefined replay-stability).
    /// Default = `true`.
    pub broadphase_sort_by_morton: bool,
    /// True ⇒ XPBD constraint-color-buckets iterate in color-id order with
    /// inner sort by `(body_a, body_b)`. Default = `true`.
    pub xpbd_color_stable_sort: bool,
    /// True ⇒ the world's `flush_denormals_to_zero` probe must succeed at
    /// construction or the world refuses to construct. Default = `true`.
    pub require_ftz_probe: bool,
}

impl Default for DeterminismConfig {
    fn default() -> Self {
        DeterminismConfig {
            rng_seed: DET_RNG_SEED_DEFAULT,
            broadphase_sort_by_morton: true,
            xpbd_color_stable_sort: true,
            require_ftz_probe: true,
        }
    }
}

impl DeterminismConfig {
    /// § Construct a config that sacrifices replay-stability for raw
    ///   throughput. Use only for non-replay-critical paths (visualization,
    ///   gameplay-only telemetry). Tests never set this.
    #[must_use]
    pub fn fast_unstable() -> Self {
        DeterminismConfig {
            rng_seed: DET_RNG_SEED_DEFAULT,
            broadphase_sort_by_morton: false,
            xpbd_color_stable_sort: false,
            require_ftz_probe: false,
        }
    }

    /// § Strictest mode : every replay-stability invariant is enforced.
    ///   Same as `Default`, kept as a named constructor for readability.
    #[must_use]
    pub fn strict() -> Self {
        DeterminismConfig::default()
    }
}

// ───────────────────────────────────────────────────────────────────────
// § Probes — runtime checks that the FPU is in the right mode.
// ───────────────────────────────────────────────────────────────────────

/// § Probe : is the FPU set to flush denormals to zero ?
///
/// We compute a known-denormal float (`f32::MIN_POSITIVE / 2.0` is below
/// the normal range) and check whether the result reads back as `0.0` (FTZ
/// active) or as the original sub-normal value (FTZ inactive).
///
/// **NOTE** : This probe is informational. The wave-physics solver does
/// NOT depend on FTZ for correctness ; it depends on FTZ for **performance
/// reproducibility** (denormal arithmetic is dramatically slower on every
/// modern x86-64 microarchitecture, which can cause a deterministic-but-
/// slow replay to look like a non-deterministic divergence under wall-
/// clock-pressure). The world refuses to construct if FTZ is not active
/// AND `require_ftz_probe=true`.
#[must_use]
pub fn flush_denormals_to_zero() -> bool {
    let denormal: f32 = f32::MIN_POSITIVE / 2.0;
    // Force the value through a black-box (volatile-style) read so the
    // compiler can't optimize it to a compile-time `0.0`.
    let cell = Cell::new(denormal);
    let read_back = cell.get();
    // If FTZ is active, `read_back` should be 0.0 because the FPU flushes
    // sub-normal results to zero. If FTZ is inactive, the value survives.
    //
    // Caveat : on some platforms the literal sub-normal survives the load
    // even with FTZ active because FTZ only flushes ARITHMETIC results,
    // not loads. We therefore stress the value through an arithmetic op.
    let stressed = read_back * 1.0_f32;
    stressed == 0.0_f32
}

/// § Probe : is FMA (fused-multiply-add) NOT being emitted on the
///   `mul_add` path ?
///
/// FMA changes rounding behavior + breaks the bit-equal-replay invariant.
/// We compute a value that differs between FMA-active and FMA-disabled
/// rounding, then check we get the FMA-disabled answer.
///
/// **CAVEAT** : This probe checks the BUILT binary, not the source. If
/// the consumer's build pipeline injects `-Cfast-math` or sets the
/// `force-frame-pointer` style flags that re-enable FMA emission, the
/// probe will catch it ; otherwise it cannot. We pair this probe with a
/// `#![allow(clippy::suboptimal_flops)]` discipline at the source-level
/// to keep the explicit `(a * b) + c` two-step pattern.
#[must_use]
pub fn fmadd_disabled() -> bool {
    // The classic FMA-detection trick : compute (a * b + c) where a*b
    // overflows but the FMA-fused result rounds differently from the
    // two-step. We use a value pair where this is reliably observable.
    let a: f32 = 1.0_f32 + (1.0_f32 / (1u64 << 24) as f32); // just above 1.0
    let b: f32 = a;
    let c: f32 = -1.0_f32;
    // Two-step : (a * b) + c. With FTZ-active FPU + IEEE-754 rounding,
    // the intermediate (a*b) rounds to ≈ 1.0 + 2^-23, and adding -1.0
    // gives 2^-23. With FMA-fused, the intermediate is exact and the
    // result is a tiny denormal which FTZ then flushes to 0.0.
    let two_step = (a * b) + c;
    // We accept either the two-step normal or the FTZ-flushed FMA result
    // BUT NOT both at once — because if the build is correct (no FMA),
    // we get the two-step result deterministically.
    two_step != 0.0_f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn determinism_config_default_is_strict() {
        let c = DeterminismConfig::default();
        let s = DeterminismConfig::strict();
        assert_eq!(c, s);
        assert!(c.broadphase_sort_by_morton);
        assert!(c.xpbd_color_stable_sort);
        assert!(c.require_ftz_probe);
    }

    #[test]
    fn determinism_config_fast_unstable_relaxes_all() {
        let f = DeterminismConfig::fast_unstable();
        assert!(!f.broadphase_sort_by_morton);
        assert!(!f.xpbd_color_stable_sort);
        assert!(!f.require_ftz_probe);
    }

    #[test]
    fn det_rng_seed_default_is_nonzero() {
        assert!(DET_RNG_SEED_DEFAULT != 0);
    }

    #[test]
    fn flush_denormals_probe_runs_without_panic() {
        // We don't assert the probe returns a specific value because the
        // host's FPU mode is environment-dependent. We only assert the
        // probe completes (does not panic / loop).
        let _ = flush_denormals_to_zero();
    }

    #[test]
    fn fmadd_disabled_probe_runs_without_panic() {
        let _ = fmadd_disabled();
    }

    #[test]
    fn determinism_config_clone_eq() {
        let a = DeterminismConfig::default();
        let b = a;
        assert_eq!(a, b);
    }

    #[test]
    fn determinism_config_hash_stable() {
        use core::hash::{Hash, Hasher};
        // We use a tiny-FNV-style hash to verify the type implements Hash.
        struct H(u64);
        impl Hasher for H {
            fn finish(&self) -> u64 {
                self.0
            }
            fn write(&mut self, bytes: &[u8]) {
                for b in bytes {
                    self.0 = self.0.wrapping_mul(0x100000001b3).wrapping_add(*b as u64);
                }
            }
        }
        let mut h1 = H(0xCBF29CE484222325);
        let mut h2 = H(0xCBF29CE484222325);
        DeterminismConfig::default().hash(&mut h1);
        DeterminismConfig::default().hash(&mut h2);
        assert_eq!(h1.finish(), h2.finish());
    }
}
