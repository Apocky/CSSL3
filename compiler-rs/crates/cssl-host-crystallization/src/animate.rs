//! § animate — per-frame animated aspect-state evaluation.
//!
//! § T11-W18-F-ANIMATE · canonical-impl : `Labyrinth of Apocalypse/systems/crystallization.csl`
//!
//! § THESIS
//!
//! Crystals are NOT static. Per the .csl spec § ASPECT-CURVES, the MOTION ·
//! SOUND · ECHO · BLOOM aspects are *animated* — they evolve with phase
//! parameter `t_milli`. Per spec § ASPECT-CURVE evaluation, alien_materialization
//! drives per-frame queries. This module provides those queries in a single
//! batched call : `animate_crystal(c, t_milli) -> AnimatedAspectState`.
//!
//! Bit-deterministic per (crystal · t_milli). Σ-mask-gated : if an aspect is
//! denied, its slot in the output is zeroed.
//!
//! § OUTPUT-SHAPE
//!
//!   motion_pose    : [i32; 3]   xyz pose-delta in micrometers
//!   sound_spectrum : [u16; 16]  16-band audio-spectrum amplitudes
//!   echo_phase     : u32        compact prior-state-recall fingerprint
//!   bloom_phase    : u32        compact future-tendency fingerprint
//!
//! § DETERMINISM CONTRACT
//!
//! `animate_crystal(c, t)` produces the same bytes for the same `(c, t)` on
//! every call · every machine. The result depends ONLY on (curves, seed,
//! sigma_mask, t_milli). No globals · no rng · no clock-read inside.
//!
//! § ATTESTATION
//! There was no hurt nor harm in the making of this, to anyone, anything,
//! or anybody. Every read is Σ-mask-honored.

use crate::aspect::{aspect_idx, AspectCurves};
use crate::Crystal;

// ══════════════════════════════════════════════════════════════════════════
// § AnimatedAspectState — per-frame output
// ══════════════════════════════════════════════════════════════════════════

/// Per-frame animated state for the 4 phase-driven aspects of one crystal.
/// 76 bytes total : (3·4) + (16·2) + 4 + 4 = 12 + 32 + 8 = 52 + Echo/Bloom
/// glue. Plain-old-data · `Copy` · `Default` · stable layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(C)]
pub struct AnimatedAspectState {
    /// MOTION aspect : signed micrometer pose-delta on world (x, y, z).
    pub motion_pose: [i32; 3],
    /// SOUND aspect : 16-band amplitude spectrum (linear u16).
    pub sound_spectrum: [u16; 16],
    /// ECHO aspect : compact fingerprint of recalled prior state.
    pub echo_phase: u32,
    /// BLOOM aspect : compact fingerprint of anticipated future state.
    pub bloom_phase: u32,
}

impl AnimatedAspectState {
    /// All-zero state · used when every relevant aspect is Σ-mask-denied.
    pub const ZERO: Self = Self {
        motion_pose: [0; 3],
        sound_spectrum: [0; 16],
        echo_phase: 0,
        bloom_phase: 0,
    };
}

// ══════════════════════════════════════════════════════════════════════════
// § axis-weight derivation
// ══════════════════════════════════════════════════════════════════════════

/// Derive a 4-axis weight tuple from `(seed, aspect_idx, sub_idx)`. Stable ·
/// deterministic · bias-noted : axis-0 is biased toward >= 32 to ensure a
/// nonzero baseline contribution per evaluation (avoids degenerate weight
/// sums when other axes happen to be small).
///
/// `sub_idx` is for callers that need multiple distinct weight-tuples for
/// the same aspect (e.g., MOTION needs 3 — one per spatial axis).
#[inline]
fn axis_weights_for(seed: u64, aspect_i: u8, sub_idx: u8) -> [u8; 4] {
    // Splatter (seed · aspect · sub) into 4 bytes via cheap BLAKE3.
    // BLAKE3-32 is overkill ; one 32-byte hash splits into 8 distinct
    // 4-byte axis-tuples — we slice by sub_idx (0..8).
    let mut h = blake3::Hasher::new();
    h.update(b"animate-axis-v1");
    h.update(&seed.to_le_bytes());
    h.update(&[aspect_i, sub_idx]);
    let d: [u8; 32] = h.finalize().into();

    let off = ((sub_idx as usize) & 0x07) * 4;
    let mut w = [d[off], d[off + 1], d[off + 2], d[off + 3]];
    // Floor axis-0 ≥ 32 to prevent degenerate weighted-sum near zero.
    if w[0] < 32 {
        w[0] = 32;
    }
    w
}

/// Map (seed, aspect_idx) → a phase-period in milliseconds. Different
/// aspects loop on different cadences so the crystal feels organic — not
/// every aspect ticks on the same beat.
#[inline]
fn period_for(seed: u64, aspect_i: u8) -> u64 {
    // Deterministic period : 600..=2400 ms · biased per (seed, aspect).
    let mix = seed
        .rotate_left(((aspect_i as u32) * 7) & 63)
        ^ (0x9E37_79B9_7F4A_7C15u64.wrapping_mul(aspect_i as u64 + 1));
    let span = 1800u64; // 600 + 0..=1800
    600 + (mix % (span + 1))
}

/// Wrap absolute `t_milli` into spline-domain `0..=1000` given a per-aspect
/// `period_ms`. Period > 0 invariant guarded.
#[inline]
fn phase_in_period(t_milli: u64, period_ms: u64) -> u32 {
    let p = period_ms.max(1);
    // t_milli mod period → 0..period, then scale to 0..=1000.
    let local = t_milli % p;
    ((local * 1000) / p) as u32
}

// ══════════════════════════════════════════════════════════════════════════
// § per-aspect evaluators
// ══════════════════════════════════════════════════════════════════════════

/// MOTION : evaluate spline 3 times with axis-weight-tuples (sub=0,1,2),
/// each at a slightly phase-shifted `t` so xyz are decorrelated. Output
/// is signed micrometer pose-delta — clamped to ±32_000_000 (32 m).
#[inline]
fn eval_motion(curves: &AspectCurves, seed: u64, t_milli: u64) -> [i32; 3] {
    let s = curves.spline(aspect_idx::MOTION);
    let period = period_for(seed, aspect_idx::MOTION);
    // Sub-axis phase-shifts : 0, +period/3, +2·period/3 → phase-decorrelation.
    let third = period / 3;
    let mut out = [0i32; 3];
    for axis in 0..3u8 {
        let shift = (axis as u64).wrapping_mul(third);
        let phase = phase_in_period(t_milli.wrapping_add(shift), period);
        let w = axis_weights_for(seed, aspect_idx::MOTION, axis);
        let raw = s.eval(phase, w);
        // Clamp to ±32 m (±32_000_000 µm) for sane physics.
        out[axis as usize] = raw.clamp(-32_000_000, 32_000_000);
    }
    out
}

/// SOUND : evaluate spline at 16 distinct phase-points spread across the
/// aspect's period. Each evaluation produces one band-amplitude. Output
/// clamped to u16.
#[inline]
fn eval_sound(curves: &AspectCurves, seed: u64, t_milli: u64) -> [u16; 16] {
    let s = curves.spline(aspect_idx::SOUND);
    let period = period_for(seed, aspect_idx::SOUND);
    let mut spectrum = [0u16; 16];
    for band in 0..16u8 {
        // Each band gets its own phase-offset = band/16 of period.
        let band_offset = (period * band as u64) / 16;
        let phase = phase_in_period(t_milli.wrapping_add(band_offset), period);
        let w = axis_weights_for(seed, aspect_idx::SOUND, band);
        let raw = s.eval(phase, w);
        // Take absolute value, clamp to u16 range.
        let abs = raw.unsigned_abs();
        spectrum[band as usize] = abs.min(u16::MAX as u32) as u16;
    }
    spectrum
}

/// ECHO : recall prior-state by evaluating spline at `t_milli - lookback`.
/// Lookback is deterministic per seed (50..=750 ms). Output is a packed
/// u32 fingerprint of the recalled state — stable per (seed, t).
#[inline]
fn eval_echo(curves: &AspectCurves, seed: u64, t_milli: u64) -> u32 {
    let s = curves.spline(aspect_idx::ECHO);
    let period = period_for(seed, aspect_idx::ECHO);
    // Lookback in 50..=750 ms — deterministic from seed.
    let lookback = 50 + ((seed >> 16) % 701);
    // Saturating-sub avoids underflow at game-start (t_milli < lookback).
    let t_back = t_milli.saturating_sub(lookback);
    let phase = phase_in_period(t_back, period);
    let w = axis_weights_for(seed, aspect_idx::ECHO, 0);
    let raw = s.eval(phase, w);

    // Pack (raw, t_back, seed-mix) into a u32 fingerprint.
    let mut h = blake3::Hasher::new();
    h.update(b"animate-echo-v1");
    h.update(&seed.to_le_bytes());
    h.update(&t_back.to_le_bytes());
    h.update(&raw.to_le_bytes());
    let d: [u8; 32] = h.finalize().into();
    u32::from_le_bytes([d[0], d[1], d[2], d[3]])
}

/// BLOOM : anticipate future-state by evaluating spline at `t_milli + lookahead`.
/// Lookahead is deterministic per seed (100..=900 ms). Output is a packed
/// u32 fingerprint of the anticipated state.
#[inline]
fn eval_bloom(curves: &AspectCurves, seed: u64, t_milli: u64) -> u32 {
    let s = curves.spline(aspect_idx::BLOOM);
    let period = period_for(seed, aspect_idx::BLOOM);
    // Lookahead 100..=900 ms — deterministic from seed.
    let lookahead = 100 + ((seed >> 24) % 801);
    let t_fwd = t_milli.saturating_add(lookahead);
    let phase = phase_in_period(t_fwd, period);
    let w = axis_weights_for(seed, aspect_idx::BLOOM, 0);
    let raw = s.eval(phase, w);

    let mut h = blake3::Hasher::new();
    h.update(b"animate-bloom-v1");
    h.update(&seed.to_le_bytes());
    h.update(&t_fwd.to_le_bytes());
    h.update(&raw.to_le_bytes());
    let d: [u8; 32] = h.finalize().into();
    u32::from_le_bytes([d[0], d[1], d[2], d[3]])
}

// ══════════════════════════════════════════════════════════════════════════
// § public surface
// ══════════════════════════════════════════════════════════════════════════

/// Evaluate the 4 phase-driven aspects (MOTION · SOUND · ECHO · BLOOM) at
/// time `t_milli` for one crystal. Σ-mask is honored : denied aspects come
/// back zeroed.
///
/// `t_milli` is the absolute scene-time in milliseconds. The function
/// internally wraps it into per-aspect periods (600..=2400 ms) derived
/// from the crystal's seed, so a crystal feels organic rather than
/// uniformly metronomic.
///
/// Bit-deterministic per (crystal · t_milli).
pub fn animate_crystal(crystal: &Crystal, t_milli: u64) -> AnimatedAspectState {
    let seed = crystal.seed;

    let motion_pose = if crystal.aspect_permitted(aspect_idx::MOTION) {
        eval_motion(&crystal.curves, seed, t_milli)
    } else {
        [0i32; 3]
    };

    let sound_spectrum = if crystal.aspect_permitted(aspect_idx::SOUND) {
        eval_sound(&crystal.curves, seed, t_milli)
    } else {
        [0u16; 16]
    };

    let echo_phase = if crystal.aspect_permitted(aspect_idx::ECHO) {
        eval_echo(&crystal.curves, seed, t_milli)
    } else {
        0u32
    };

    let bloom_phase = if crystal.aspect_permitted(aspect_idx::BLOOM) {
        eval_bloom(&crystal.curves, seed, t_milli)
    } else {
        0u32
    };

    AnimatedAspectState {
        motion_pose,
        sound_spectrum,
        echo_phase,
        bloom_phase,
    }
}

/// Batch-evaluate `animate_crystal` over a slice of crystals at the same
/// `t_milli`. Pre-allocates the output Vec to crystals.len() · so a single
/// allocation regardless of count. Stage-0 is plain-iteration · SoA-vector
/// optimization can come later when profiling demands.
pub fn substrate_animate_field(crystals: &[Crystal], t_milli: u64) -> Vec<AnimatedAspectState> {
    let mut out = Vec::with_capacity(crystals.len());
    for c in crystals {
        out.push(animate_crystal(c, t_milli));
    }
    out
}

// ══════════════════════════════════════════════════════════════════════════
// § tests
// ══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CrystalClass, WorldPos};

    fn alloc(seed: u64) -> Crystal {
        Crystal::allocate(CrystalClass::Entity, seed, WorldPos::new(0, 0, 0))
    }

    #[test]
    fn determinism_per_seed_and_t() {
        let c = alloc(42);
        let a = animate_crystal(&c, 1234);
        let b = animate_crystal(&c, 1234);
        assert_eq!(a, b, "animate_crystal must be deterministic per (c, t)");
    }

    #[test]
    fn motion_aspect_varies_with_t() {
        let c = alloc(7);
        let mut samples = Vec::new();
        for t in (0..2000).step_by(50) {
            samples.push(animate_crystal(&c, t).motion_pose);
        }
        // At least 6 distinct pose-tuples observed across 40 samples
        // (motion is animated · not constant).
        let mut uniq = samples.clone();
        uniq.sort();
        uniq.dedup();
        assert!(
            uniq.len() >= 6,
            "expected ≥6 distinct motion poses across 40 samples · got {}",
            uniq.len()
        );
    }

    #[test]
    fn sound_aspect_non_zero() {
        let c = alloc(99);
        // Sample sound across a few t · expect at least one band non-zero
        // in at least one sample (spline should not be entirely flat).
        let mut any_nonzero = false;
        for t in (0..3000).step_by(123) {
            let s = animate_crystal(&c, t);
            if s.sound_spectrum.iter().any(|&b| b > 0) {
                any_nonzero = true;
                break;
            }
        }
        assert!(any_nonzero, "sound spectrum should be non-zero somewhere");
    }

    #[test]
    fn echo_retrieves_prior_state() {
        let c = alloc(123);
        // Echo at t=2000 references state from t-lookback. Across t in
        // a wide range, echo_phase should change · because t_back changes.
        let e1 = animate_crystal(&c, 2000).echo_phase;
        let e2 = animate_crystal(&c, 2500).echo_phase;
        let e3 = animate_crystal(&c, 3000).echo_phase;
        // At least one pair should differ — echo evolves with t.
        assert!(
            e1 != e2 || e2 != e3 || e1 != e3,
            "echo_phase should evolve · all three were equal"
        );
    }

    #[test]
    fn bloom_anticipates_future() {
        let c = alloc(555);
        // Bloom at t=0 anticipates t+lookahead. Different t ⇒ different
        // anticipated states.
        let b1 = animate_crystal(&c, 0).bloom_phase;
        let b2 = animate_crystal(&c, 500).bloom_phase;
        let b3 = animate_crystal(&c, 1500).bloom_phase;
        assert!(
            b1 != b2 || b2 != b3 || b1 != b3,
            "bloom_phase should evolve as t advances · all three were equal"
        );
    }

    #[test]
    fn sigma_mask_gates_aspects() {
        let mut c = alloc(11);
        // With full mask · expect non-trivial state somewhere.
        let full = animate_crystal(&c, 1500);

        // Revoke MOTION + SOUND + ECHO + BLOOM aspects (3..=7).
        for ai in [
            aspect_idx::MOTION,
            aspect_idx::SOUND,
            aspect_idx::ECHO,
            aspect_idx::BLOOM,
        ] {
            c.revoke_aspect(ai);
        }
        let denied = animate_crystal(&c, 1500);
        assert_eq!(
            denied,
            AnimatedAspectState::ZERO,
            "all-revoked crystal must yield ZERO state"
        );

        // Sanity : full crystal at the same t should NOT be all-zero.
        // (Probabilistically near-impossible · but guard with explicit check.)
        let any_motion = full.motion_pose.iter().any(|&v| v != 0);
        let any_sound = full.sound_spectrum.iter().any(|&v| v != 0);
        let any_echo = full.echo_phase != 0;
        let any_bloom = full.bloom_phase != 0;
        assert!(
            any_motion || any_sound || any_echo || any_bloom,
            "non-revoked crystal at t=1500 was unexpectedly all-zero"
        );
    }

    #[test]
    fn batch_eval_matches_per_crystal_eval() {
        let crystals: Vec<Crystal> = (0..16u64).map(alloc).collect();
        let t = 777;
        let batch = substrate_animate_field(&crystals, t);
        assert_eq!(batch.len(), crystals.len());
        for (i, c) in crystals.iter().enumerate() {
            let solo = animate_crystal(c, t);
            assert_eq!(batch[i], solo, "batch[{i}] != solo eval");
        }
    }

    #[test]
    fn motion_pose_is_clamped() {
        // Exhaustive bounds-check across 50 seeds · 50 time-points.
        for seed in 0..50u64 {
            let c = alloc(seed);
            for t in (0..5000u64).step_by(100) {
                let s = animate_crystal(&c, t);
                for axis in 0..3 {
                    assert!(
                        s.motion_pose[axis].abs() <= 32_000_000,
                        "motion_pose[{axis}] = {} out of bounds @ seed {seed} t {t}",
                        s.motion_pose[axis]
                    );
                }
            }
        }
    }

    #[test]
    fn batch_empty_yields_empty_vec() {
        let v = substrate_animate_field(&[], 100);
        assert!(v.is_empty());
    }

    #[test]
    fn period_in_organic_range() {
        // Per-aspect period must land in 600..=2400 ms for every (seed, aspect).
        for seed in [0u64, 1, 42, 999, u64::MAX, 0x9E37_79B9_7F4A_7C15] {
            for ai in 0..8u8 {
                let p = period_for(seed, ai);
                assert!(
                    (600..=2400).contains(&p),
                    "period_for({seed}, {ai}) = {p} out of [600, 2400]"
                );
            }
        }
    }
}
