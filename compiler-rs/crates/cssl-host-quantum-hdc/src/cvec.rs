//! § cvec — `CHdcVec` 256-component complex hypervector with phase-coherent
//! bind / bundle / permute / coherence / interfere primitives.
//!
//! § STORAGE-DECISION
//!   Polar form `(amp[256], phase[256])` is the canonical persistent
//!   representation because (a) the substrate spec talks in
//!   "(amplitude, phase)" pairs at the spec-axiom level, (b) phase is the
//!   load-bearing physics-metaphor for interference/coherence, and (c) the
//!   storage is a flat `[f32; 512]` per vector → 2 KiB, which fits in L1
//!   alongside several vectors. Bind / bundle / interfere convert to
//!   cartesian internally for the actual arithmetic, then convert back.
//!
//! § DETERMINISM
//!   No thread-local state. No system clock. Every operation is a pure
//!   function of the input vectors plus an explicit `seed` / `n` parameter.
//!   Same inputs ⇒ same outputs across hosts modulo IEEE-754 single-
//!   precision (tests use ε-tolerance).

use crate::complex::{wrap_phase, C32};

/// § Vector width — 256 complex components. Matches stage-0 binary-HDC
///   width @ `cssl-host-crystallization::hdc::HdcVec256` so a future
///   conversion lane can map binary→complex by setting `amp = 1.0` and
///   `phase = 0` or `phase = π` per bit.
pub const CHDC_DIM: usize = 256;

/// § 256-component complex hypervector in polar form.
///
/// Each of the 256 components is an independent `(amp[i], phase[i])`
/// where `amp[i] ∈ [0, ∞)` and `phase[i] ∈ [-π, π]`. Newly-derived
/// vectors have `amp[i] ∈ [0, 1]`. `interfere()` can produce amplitudes
/// > 1 (constructive) or near-zero (destructive) — those are intentional
/// and observable to consumers.
#[derive(Debug, Clone)]
pub struct CHdcVec {
    pub amp: [f32; CHDC_DIM],
    pub phase: [f32; CHDC_DIM],
}

impl CHdcVec {
    /// § Zero vector — all amplitudes 0 (decoherent / silent).
    pub const ZERO: Self = Self {
        amp: [0.0; CHDC_DIM],
        phase: [0.0; CHDC_DIM],
    };

    /// § Unit-amplitude zero-phase vector — analogous to "all 1s" in binary HDC.
    pub fn ones() -> Self {
        Self {
            amp: [1.0; CHDC_DIM],
            phase: [0.0; CHDC_DIM],
        }
    }

    /// § Direct constructor.
    pub const fn new(amp: [f32; CHDC_DIM], phase: [f32; CHDC_DIM]) -> Self {
        Self { amp, phase }
    }

    /// § Derive a deterministic complex hypervector from a 32-byte seed.
    ///
    /// Uses BLAKE3's extendable-output-function (XOF) to stream 2048 bytes
    /// (8 bytes per component × 256 components). For each component, the
    /// first u32 → amplitude in `[0, 1]`, the second u32 → phase mapped
    /// to `[-π, π]` via `(u/u32::MAX - 0.5) · 2π`.
    ///
    /// Same seed ⇒ same vector ∀ hosts. No host-fingerprinting, no time
    /// entropy.
    pub fn derive_from_blake3(seed: &[u8; 32]) -> Self {
        use core::f32::consts::PI;

        let mut h = blake3::Hasher::new();
        h.update(b"chdc-derive-v1");
        h.update(seed);

        // 256 components × 8 bytes each = 2048 bytes.
        let mut bytes = [0u8; CHDC_DIM * 8];
        let mut xof = h.finalize_xof();
        xof.fill(&mut bytes);

        let mut amp = [0.0f32; CHDC_DIM];
        let mut phase = [0.0f32; CHDC_DIM];
        let two_pi = 2.0 * PI;
        let max_u32 = u32::MAX as f32;

        for i in 0..CHDC_DIM {
            let off = i * 8;
            let u0 = u32::from_le_bytes([
                bytes[off],
                bytes[off + 1],
                bytes[off + 2],
                bytes[off + 3],
            ]);
            let u1 = u32::from_le_bytes([
                bytes[off + 4],
                bytes[off + 5],
                bytes[off + 6],
                bytes[off + 7],
            ]);
            amp[i] = (u0 as f32) / max_u32; // ∈ [0, 1]
            // (u1/MAX - 0.5) · 2π → ∈ [-π, π).
            phase[i] = ((u1 as f32) / max_u32 - 0.5) * two_pi;
        }

        Self { amp, phase }
    }

    /// § Per-component complex multiplication — the canonical `bind` op.
    ///
    /// `(a·b)ᵢ.amp   = aᵢ.amp · bᵢ.amp`
    /// `(a·b)ᵢ.phase = wrap(aᵢ.phase + bᵢ.phase)`
    ///
    /// Commutative + associative + has-identity (`Self::ones()`).
    pub fn bind(&self, other: &Self) -> Self {
        let mut amp = [0.0f32; CHDC_DIM];
        let mut phase = [0.0f32; CHDC_DIM];
        for i in 0..CHDC_DIM {
            amp[i] = self.amp[i] * other.amp[i];
            phase[i] = wrap_phase(self.phase[i] + other.phase[i]);
        }
        Self { amp, phase }
    }

    /// § Phase-rotation permute — rotate every component's phase by
    /// `n · π / CHDC_DIM`. Cyclic group of order `2 · CHDC_DIM`.
    ///
    /// This is the ℂ-HDC analog of binary-HDC's bit-rotation. It encodes
    /// position / sequence without altering amplitudes (so coherence with
    /// the original is preserved up to phase-only divergence).
    pub fn permute(&self, n: u32) -> Self {
        use core::f32::consts::PI;
        // Modulo 2·CHDC_DIM — anything beyond is a full revolution.
        let n_mod = (n as i64).rem_euclid(2 * CHDC_DIM as i64) as f32;
        let delta = n_mod * (PI / CHDC_DIM as f32);
        let mut amp = self.amp;
        let mut phase = [0.0f32; CHDC_DIM];
        for i in 0..CHDC_DIM {
            phase[i] = wrap_phase(self.phase[i] + delta);
        }
        // Defensive — `let mut amp = self.amp` already copies.
        for i in 0..CHDC_DIM {
            amp[i] = self.amp[i];
        }
        Self { amp, phase }
    }

    /// § Coherence — normalized inner-product magnitude `∈ [0, 1]`.
    ///
    /// `1.0` ⟺ vectors share both amplitude pattern and phase pattern.
    /// `0.0` ⟺ amplitudes nonzero but phases distributed such that
    ///        sum-of-products cancels (decoherent).
    ///
    /// Formula : `‖∑ᵢ aᵢ · b̄ᵢ‖ / ∑ᵢ ‖aᵢ‖·‖bᵢ‖`. The denominator
    /// normalizes against amplitude-magnitude so the score is
    /// phase-coherence, not energy.
    pub fn coherence(&self, other: &Self) -> f32 {
        let mut sum_re = 0.0f32;
        let mut sum_im = 0.0f32;
        let mut amp_norm = 0.0f32;
        for i in 0..CHDC_DIM {
            // a · b̄ in polar form : amp_a·amp_b · exp(i·(φa - φb)).
            let amp_prod = self.amp[i] * other.amp[i];
            let dphi = self.phase[i] - other.phase[i];
            sum_re += amp_prod * dphi.cos();
            sum_im += amp_prod * dphi.sin();
            amp_norm += amp_prod;
        }
        if amp_norm <= f32::EPSILON {
            return 0.0;
        }
        let mag = (sum_re * sum_re + sum_im * sum_im).sqrt();
        (mag / amp_norm).clamp(0.0, 1.0)
    }

    /// § Renormalize amplitudes to peak = 1.0. Pure when peak > 0.
    pub fn renormalize(&self) -> Self {
        let mut peak = 0.0f32;
        for &a in &self.amp {
            if a > peak {
                peak = a;
            }
        }
        if peak <= f32::EPSILON {
            return self.clone();
        }
        let inv = 1.0 / peak;
        let mut amp = [0.0f32; CHDC_DIM];
        for i in 0..CHDC_DIM {
            amp[i] = self.amp[i] * inv;
        }
        Self {
            amp,
            phase: self.phase,
        }
    }
}

/// § Bundle — vector-sum then renormalize. Preserves phase information
/// (unlike binary-HDC majority-vote which is phase-blind).
///
/// Each component is summed in cartesian form so that constructive /
/// destructive interference between bundled vectors produces the
/// expected result. The bundle of 1 vector is itself (modulo
/// renormalization).
pub fn bundle(vecs: &[CHdcVec]) -> CHdcVec {
    if vecs.is_empty() {
        return CHdcVec::ZERO;
    }

    let mut sum_re = [0.0f32; CHDC_DIM];
    let mut sum_im = [0.0f32; CHDC_DIM];

    for v in vecs {
        for i in 0..CHDC_DIM {
            let c = C32::from_polar(v.amp[i], v.phase[i]);
            sum_re[i] += c.re;
            sum_im[i] += c.im;
        }
    }

    let mut amp = [0.0f32; CHDC_DIM];
    let mut phase = [0.0f32; CHDC_DIM];
    for i in 0..CHDC_DIM {
        let c = C32::new(sum_re[i], sum_im[i]);
        let (a, p) = c.to_polar();
        amp[i] = a;
        phase[i] = p;
    }

    CHdcVec { amp, phase }.renormalize()
}

/// § Interfere — explicit interference operation. Sum WITHOUT
/// renormalization so that constructive (>1) and destructive (≈0)
/// amplitudes are observable to the caller.
///
/// This is the operation that exposes the "fringe pattern" — passing two
/// derived vectors with similar phases gives mostly-constructive output;
/// passing a vector and its phase-shifted copy at `permute(CHDC_DIM)`
/// (which is `+π` per component) gives near-zero amplitude
/// (destructive).
pub fn interfere(a: &CHdcVec, b: &CHdcVec) -> CHdcVec {
    let mut amp = [0.0f32; CHDC_DIM];
    let mut phase = [0.0f32; CHDC_DIM];
    for i in 0..CHDC_DIM {
        let ca = C32::from_polar(a.amp[i], a.phase[i]);
        let cb = C32::from_polar(b.amp[i], b.phase[i]);
        let cs = ca.add(cb);
        let (am, ph) = cs.to_polar();
        amp[i] = am;
        phase[i] = ph;
    }
    CHdcVec { amp, phase }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::f32::consts::PI;

    fn seed(byte: u8) -> [u8; 32] {
        [byte; 32]
    }

    #[test]
    fn derive_is_deterministic() {
        let a = CHdcVec::derive_from_blake3(&seed(7));
        let b = CHdcVec::derive_from_blake3(&seed(7));
        for i in 0..CHDC_DIM {
            assert!((a.amp[i] - b.amp[i]).abs() < 1e-6);
            assert!((a.phase[i] - b.phase[i]).abs() < 1e-6);
        }
    }

    #[test]
    fn derive_amplitudes_in_unit_range() {
        let v = CHdcVec::derive_from_blake3(&seed(13));
        for &a in &v.amp {
            assert!((0.0..=1.0001).contains(&a));
        }
        for &p in &v.phase {
            assert!(p >= -PI - 1e-4 && p <= PI + 1e-4);
        }
    }

    #[test]
    fn bind_is_commutative_within_eps() {
        let a = CHdcVec::derive_from_blake3(&seed(1));
        let b = CHdcVec::derive_from_blake3(&seed(2));
        let ab = a.bind(&b);
        let ba = b.bind(&a);
        for i in 0..CHDC_DIM {
            assert!((ab.amp[i] - ba.amp[i]).abs() < 1e-5);
            // Phase wraps may differ by 2π — compare via cos/sin.
            let cos_ab = ab.phase[i].cos();
            let cos_ba = ba.phase[i].cos();
            let sin_ab = ab.phase[i].sin();
            let sin_ba = ba.phase[i].sin();
            assert!((cos_ab - cos_ba).abs() < 1e-4);
            assert!((sin_ab - sin_ba).abs() < 1e-4);
        }
    }

    #[test]
    fn coherence_self_is_one() {
        let a = CHdcVec::derive_from_blake3(&seed(3));
        let c = a.coherence(&a);
        assert!((c - 1.0).abs() < 1e-4, "self-coherence = {c}");
    }

    #[test]
    fn coherence_bounded_zero_to_one() {
        for byte_a in [1u8, 5, 17, 99] {
            for byte_b in [2u8, 11, 64, 200] {
                let a = CHdcVec::derive_from_blake3(&seed(byte_a));
                let b = CHdcVec::derive_from_blake3(&seed(byte_b));
                let c = a.coherence(&b);
                assert!(
                    (0.0..=1.0).contains(&c),
                    "coherence out of bounds : {c}"
                );
            }
        }
    }

    #[test]
    fn coherence_decreases_under_random_phase() {
        // Different seeds → randomized phases → coherence < self-coherence.
        let a = CHdcVec::derive_from_blake3(&seed(31));
        let b = CHdcVec::derive_from_blake3(&seed(73));
        let c_self = a.coherence(&a);
        let c_other = a.coherence(&b);
        assert!(c_other < c_self);
        // Statistically, two random ℂ vectors of width 256 average ≈ 1/√256.
        assert!(c_other < 0.5);
    }

    #[test]
    fn permute_is_phase_only() {
        // Permute preserves amplitudes exactly — only phases change.
        let a = CHdcVec::derive_from_blake3(&seed(42));
        let p = a.permute(7);
        for i in 0..CHDC_DIM {
            assert!((a.amp[i] - p.amp[i]).abs() < 1e-7);
        }
        // Phase changed by π·7/256 per component (modulo 2π).
        let expected_delta = 7.0 * PI / CHDC_DIM as f32;
        let dphi = wrap_phase(p.phase[0] - a.phase[0]);
        assert!((dphi - expected_delta).abs() < 1e-4);
    }

    #[test]
    fn permute_full_revolution_is_identity_on_amp() {
        let a = CHdcVec::derive_from_blake3(&seed(11));
        // Full revolution = 2 · CHDC_DIM permute steps.
        let p = a.permute(2 * CHDC_DIM as u32);
        for i in 0..CHDC_DIM {
            assert!((a.amp[i] - p.amp[i]).abs() < 1e-7);
            // Phase identical modulo wrap.
            let dphi = wrap_phase(p.phase[i] - a.phase[i]);
            assert!(dphi.abs() < 1e-3);
        }
    }

    #[test]
    fn bundle_of_one_returns_renormalized_input() {
        let a = CHdcVec::derive_from_blake3(&seed(5));
        let b = bundle(&[a.clone()]);
        let a_norm = a.renormalize();
        for i in 0..CHDC_DIM {
            assert!((b.amp[i] - a_norm.amp[i]).abs() < 1e-4);
        }
    }

    #[test]
    fn bundle_preserves_coherence_with_member() {
        // Bundle a vector with itself — coherence with the bundle should
        // be perfect (after renormalization, phase-pattern unchanged).
        let a = CHdcVec::derive_from_blake3(&seed(91));
        let bundled = bundle(&[a.clone(), a.clone(), a.clone()]);
        let c = a.coherence(&bundled);
        assert!(c > 0.99, "self-bundle coherence = {c}");
    }

    #[test]
    fn interfere_constructive_doubles_amplitude() {
        // Identical vectors interfered → amplitudes 2× original.
        let a = CHdcVec::derive_from_blake3(&seed(17));
        let c = interfere(&a, &a);
        for i in 0..CHDC_DIM {
            // Skip near-zero components to avoid noise.
            if a.amp[i] > 0.1 {
                let expected = 2.0 * a.amp[i];
                assert!(
                    (c.amp[i] - expected).abs() < expected * 0.01 + 1e-4,
                    "interfere(a,a)[{i}] = {} expected ≈ {}",
                    c.amp[i],
                    expected
                );
            }
        }
    }

    #[test]
    fn interfere_destructive_cancels_amplitude() {
        // a + (a phase-shifted by +π per component) → ≈ 0 amplitude.
        let a = CHdcVec::derive_from_blake3(&seed(23));
        // Permute by CHDC_DIM = +π per component.
        let pi_shifted = a.permute(CHDC_DIM as u32);
        let c = interfere(&a, &pi_shifted);
        // Sum of |c.amp[i]| should be tiny vs sum of |a.amp[i]|.
        let sum_a: f32 = a.amp.iter().sum();
        let sum_c: f32 = c.amp.iter().sum();
        assert!(
            sum_c < sum_a * 0.05,
            "destructive interference failed : sum_c = {sum_c} vs sum_a = {sum_a}"
        );
    }

    #[test]
    fn interfere_shows_fringes_on_phase_offset() {
        // Permute by a phase that is neither identical nor π → mixed.
        let a = CHdcVec::derive_from_blake3(&seed(29));
        let half_shifted = a.permute(CHDC_DIM as u32 / 2); // +π/2.
        let c = interfere(&a, &half_shifted);
        // Some components constructive, some destructive — variance > 0.
        let mean: f32 = c.amp.iter().sum::<f32>() / CHDC_DIM as f32;
        let var: f32 = c.amp.iter().map(|x| (x - mean).powi(2)).sum::<f32>()
            / CHDC_DIM as f32;
        assert!(var > 1e-4, "no fringe variance : var = {var}");
    }

    #[test]
    fn bind_with_ones_is_identity() {
        let a = CHdcVec::derive_from_blake3(&seed(37));
        let one = CHdcVec::ones();
        let b = a.bind(&one);
        for i in 0..CHDC_DIM {
            assert!((b.amp[i] - a.amp[i]).abs() < 1e-5);
            let dphi = wrap_phase(b.phase[i] - a.phase[i]);
            assert!(dphi.abs() < 1e-4);
        }
    }
}
