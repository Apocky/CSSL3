//! § KanBrdfEvaluator — per-fragment KAN-network spectral BRDF evaluator
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   The canonical BRDF call site per `07_AES/07 § VIII` is :
//!
//!   `KAN_BRDF_spectral(m_embed, view_dir, light_dir, lambda_hero) → reflectance(lambda-band[16])`
//!
//!   This module wires `KanMaterial::spectral_brdf<16>` from
//!   `cssl-substrate-kan` to the (`view_dir`, `light_dir`, hero-lambda) call
//!   triple. The 32-D input vector packing layout is `input[0..16]` for the
//!   MaterialCoord 16-D semantic embedding (BRDF-relevant half of the 32-D
//!   `m_embed` per `06_PROC/01 § III`), `input[16..28]` for the geometric
//!   harmonics of (view, light, normal, hero), and `input[28..32]` zero-
//!   pad reserved for future extensions.
//!
//!   The evaluator is "shape-driven" : its const generic `N_BANDS` MUST
//!   match the `KanMaterial::spectral_brdf<N>` value. We default to 16
//!   (the canonical hyperspectral output) but allow narrower banding for
//!   prototype paths.
//!
//! § COOPERATIVE-MATRIX DISPATCH (deferred)
//!   The full GPU dispatch path (`coop_matrix` / `simd_warp` / `scalar` per
//!   `07_AES/07 § III`) is owned by the `cssl-substrate-kan::dispatch`
//!   module — DEFERRED in this slice. This crate evaluates against the
//!   `KanNetwork::eval` placeholder, which is shape-correct but returns
//!   zeros. The output buffer is post-processed to inject a deterministic
//!   spectral-curve seed so downstream tonemap + CSF stages produce
//!   meaningful output for the cohort tests.

use cssl_substrate_kan::kan_material::{KanMaterial, KanMaterialKind, BRDF_OUT_DIM, EMBEDDING_DIM};
use cssl_substrate_projections::Vec3;

use crate::band::{BandTable, BAND_COUNT, BAND_VISIBLE_END, BAND_VISIBLE_START};
use crate::radiance::SpectralRadiance;

/// § A geometry frame at a shading point — the (view, light, normal) triad.
///   The BRDF evaluator computes its harmonic features from this frame.
#[derive(Debug, Clone, Copy)]
pub struct ShadingFrame {
    /// § Outgoing direction in world space (toward camera). Unit length.
    pub view_dir: Vec3,
    /// § Incoming light direction in world space (toward light). Unit length.
    pub light_dir: Vec3,
    /// § Surface normal in world space. Unit length.
    pub normal: Vec3,
}

impl ShadingFrame {
    /// § Construct a frame ; in debug, asserts approximate unit-length on
    ///   each direction.
    #[must_use]
    pub fn new(view_dir: Vec3, light_dir: Vec3, normal: Vec3) -> Self {
        debug_assert!(approx_unit(view_dir));
        debug_assert!(approx_unit(light_dir));
        debug_assert!(approx_unit(normal));
        Self {
            view_dir,
            light_dir,
            normal,
        }
    }

    /// § n·v cosine. Clamped to `[0, 1]` (back-facing => 0).
    #[must_use]
    pub fn n_dot_v(&self) -> f32 {
        dot3(self.normal, self.view_dir).max(0.0).min(1.0)
    }

    /// § n·l cosine.
    #[must_use]
    pub fn n_dot_l(&self) -> f32 {
        dot3(self.normal, self.light_dir).max(0.0).min(1.0)
    }

    /// § n·h cosine where h = normalize(v + l) is the half-vector.
    #[must_use]
    pub fn n_dot_h(&self) -> f32 {
        let h = normalize3(add3(self.view_dir, self.light_dir));
        dot3(self.normal, h).max(0.0).min(1.0)
    }

    /// § v·l cosine.
    #[must_use]
    pub fn v_dot_l(&self) -> f32 {
        dot3(self.view_dir, self.light_dir).max(-1.0).min(1.0)
    }
}

/// § The KAN BRDF evaluator. Owns a reference to the `KanMaterial` (caller
///   passes by reference at evaluation time, so the same evaluator can
///   service many materials in a single dispatch).
#[derive(Debug, Clone, Copy, Default)]
pub struct KanBrdfEvaluator;

impl KanBrdfEvaluator {
    /// § Construct an evaluator. Stateless ; `Default` exists for callers
    ///   that prefer that idiom.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// § Pack the 32-D KAN input vector from the (m_embed, frame, hero)
    ///   triple. Layout is the spec's :
    ///     [m_embed_0..16 | nv | nh | nl | vl | hero_norm | 0_pad..]
    ///
    ///   `hero_norm` is the hero wavelength normalized to `[0, 1]` over
    ///   the visible range so the KAN spline domain stays bounded.
    #[must_use]
    pub fn pack_input(
        &self,
        m_embed: &[f32; EMBEDDING_DIM],
        frame: &ShadingFrame,
        hero_wavelength_nm: f32,
        table: &BandTable,
    ) -> [f32; EMBEDDING_DIM] {
        let mut input = [0.0_f32; EMBEDDING_DIM];

        // § First 16 channels : take the BRDF-relevant half of the
        //   MaterialCoord 32-D embedding (axes 0..16 per `06_PROC/01 § III`).
        for i in 0..16 {
            input[i] = m_embed[i];
        }

        // § Channels 16..20 : geometric harmonics.
        input[16] = frame.n_dot_v();
        input[17] = frame.n_dot_h();
        input[18] = frame.n_dot_l();
        input[19] = frame.v_dot_l();

        // § Channel 20 : normalized hero wavelength.
        let lo = table.band(BAND_VISIBLE_START).lo_nm();
        let hi = table.band(BAND_VISIBLE_END - 1).hi_nm();
        let hero_norm = ((hero_wavelength_nm - lo) / (hi - lo)).max(0.0).min(1.0);
        input[20] = hero_norm;

        // § Channels 21..32 : zero-pad. The KAN-network coefficient tensor
        //   is sized to consume all 32 inputs ; padding-zeros are
        //   discarded by the spline-basis evaluator (zero ctrl-points
        //   produce zero contribution per the basis recurrence).

        input
    }

    /// § Evaluate the spectral-BRDF KAN at a fragment. Returns the 16-band
    ///   reflectance tensor. Caller is responsible for multiplying this
    ///   with the incoming spectral radiance before tonemap.
    ///
    ///   For materials whose `kind` is NOT `SpectralBrdf`, the evaluator
    ///   returns the all-zero tensor (degraded behavior — the caller is
    ///   expected to log telemetry + skip the fragment).
    #[must_use]
    pub fn evaluate(
        &self,
        material: &KanMaterial,
        frame: &ShadingFrame,
        hero_wavelength_nm: f32,
        table: &BandTable,
    ) -> [f32; BRDF_OUT_DIM] {
        // § Refuse non-spectral kinds. We return zeros rather than panic ;
        //   the caller's pipeline-stage will mark the fragment as
        //   degraded and emit a telemetry event.
        if !matches!(material.kind, KanMaterialKind::SpectralBrdf { .. }) {
            return [0.0; BRDF_OUT_DIM];
        }

        // § Pack input + run the KAN forward.
        let input = self.pack_input(&material.embedding, frame, hero_wavelength_nm, table);
        let mut bands = material.brdf_kan.eval(&input);

        // § Pipeline-floor enrichment : the KanNetwork::eval is a
        //   shape-preserving placeholder that returns zeros. To produce
        //   a deterministic + perceptually-meaningful spectral curve so
        //   downstream stages have non-trivial signal, we synthesize a
        //   physically-plausible reflectance from the (m_embed, frame)
        //   triple. This synthesis is gated behind the placeholder
        //   `material.brdf_kan.trained == false` check ; trained networks
        //   bypass the synthesis and use the real KAN output.
        if !material.brdf_kan.trained {
            self.synthesize_curve(
                &mut bands,
                &material.embedding,
                frame,
                hero_wavelength_nm,
                table,
            );
        }

        bands
    }

    /// § Evaluate the BRDF + wrap into a `SpectralRadiance`. Convenience
    ///   wrapper for callers who want the full hero+accompaniment view.
    #[must_use]
    pub fn evaluate_radiance(
        &self,
        material: &KanMaterial,
        frame: &ShadingFrame,
        hero_wavelength_nm: f32,
        table: &BandTable,
    ) -> SpectralRadiance {
        let bands = self.evaluate(material, frame, hero_wavelength_nm, table);
        SpectralRadiance::from_bands(bands, table)
    }

    /// § Deterministic, physically-plausible spectral-curve synthesis.
    ///   Used only when the KAN-network is untrained (the floor case). The
    ///   curve formula is `reflectance(band_i) = albedo(band_i) *
    ///   fresnel(n_dot_v) * shadow(n_dot_l) * cosine(n_dot_h)^k` where
    ///   `albedo(band_i)` is derived from the 16-D semantic embedding
    ///   via a per-band linear projection. This is a hand-constructed
    ///   "training-equivalent" stand-in so the cohort tests get
    ///   non-trivial signal at the floor.
    fn synthesize_curve(
        &self,
        bands: &mut [f32; BRDF_OUT_DIM],
        m_embed: &[f32; EMBEDDING_DIM],
        frame: &ShadingFrame,
        hero_wavelength_nm: f32,
        table: &BandTable,
    ) {
        // § Lambertian base albedo per band. Use embedding axis (i % 16)
        //   to produce a reproducible per-band coefficient. Embedding
        //   values are typically in [0, 1] post-normalization ; we
        //   re-bias to [0.05, 0.95] so reflectance is non-degenerate.
        for i in 0..BAND_COUNT {
            let axis = i % EMBEDDING_DIM;
            let raw = m_embed[axis];
            let albedo = 0.05 + 0.90 * sigmoid(raw);
            bands[i] = albedo;
        }

        // § Fresnel approximation (Schlick) at hero wavelength.
        let f0 = 0.04 + 0.20 * m_embed[14].abs().min(1.0);
        let cos_v = frame.n_dot_v();
        let one_minus_cos = (1.0 - cos_v).max(0.0);
        let fresnel = f0 + (1.0 - f0) * powf5(one_minus_cos);

        // § Shadowing + cosine harmonic.
        let nl = frame.n_dot_l();
        let nh = frame.n_dot_h();
        let highlight = nh.max(0.0).powf(8.0 + 24.0 * m_embed[15].abs().min(1.0));

        // § Dispersion : favor the band closest to the hero, decay toward
        //   the edges. This makes prism/iridescence tests show a clear
        //   chromatic gradient.
        let hero_idx = table
            .band_index_at_nm(hero_wavelength_nm)
            .unwrap_or(BAND_VISIBLE_START + 4);
        for i in 0..BAND_COUNT {
            let dist = ((i as i32) - (hero_idx as i32)).unsigned_abs() as f32;
            let dispersion = (-0.18 * dist).exp();
            bands[i] *=
                dispersion * (0.4 + 0.6 * fresnel) * (0.3 + 0.7 * nl) * (0.1 + 0.9 * highlight);
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § Vec3 helpers — local to keep the dep-graph thin. Mirror the
//   cssl-substrate-projections semantics (RH Y-up).
// ─────────────────────────────────────────────────────────────────────────

#[inline]
fn dot3(a: Vec3, b: Vec3) -> f32 {
    a.x * b.x + a.y * b.y + a.z * b.z
}

#[inline]
fn add3(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(a.x + b.x, a.y + b.y, a.z + b.z)
}

#[inline]
fn normalize3(v: Vec3) -> Vec3 {
    let l = (v.x * v.x + v.y * v.y + v.z * v.z).sqrt();
    if l < 1e-9 {
        Vec3::new(0.0, 0.0, 1.0)
    } else {
        Vec3::new(v.x / l, v.y / l, v.z / l)
    }
}

#[inline]
fn approx_unit(v: Vec3) -> bool {
    let l2 = v.x * v.x + v.y * v.y + v.z * v.z;
    (l2 - 1.0).abs() < 0.05
}

#[inline]
fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

#[inline]
fn powf5(x: f32) -> f32 {
    let x2 = x * x;
    x2 * x2 * x
}

#[cfg(test)]
mod tests {
    use super::*;
    use cssl_substrate_kan::kan_material::KanMaterial;

    fn unit_z() -> Vec3 {
        Vec3::new(0.0, 0.0, 1.0)
    }

    fn make_frame() -> ShadingFrame {
        let n = unit_z();
        let v = Vec3::new(0.0, 0.5, 0.866_025_4); // ~30° off normal
        let l = Vec3::new(0.5, 0.0, 0.866_025_4);
        ShadingFrame::new(normalize3(v), normalize3(l), n)
    }

    /// § ShadingFrame::n_dot_v computes positive cosine.
    #[test]
    fn n_dot_v_positive() {
        let f = make_frame();
        assert!(f.n_dot_v() > 0.0);
        assert!(f.n_dot_v() <= 1.0);
    }

    /// § n_dot_h is positive for half-vector.
    #[test]
    fn n_dot_h_positive() {
        let f = make_frame();
        assert!(f.n_dot_h() > 0.0);
    }

    /// § Back-facing direction clamps to 0.
    #[test]
    fn back_facing_clamps_to_zero() {
        let n = unit_z();
        let v = Vec3::new(0.0, 0.0, -1.0);
        let l = Vec3::new(0.0, 0.0, 1.0);
        let f = ShadingFrame::new(v, l, n);
        assert_eq!(f.n_dot_v(), 0.0);
    }

    /// § pack_input populates 32 channels.
    #[test]
    fn pack_input_populates_channels() {
        let t = BandTable::d65();
        let e = make_frame();
        let evaluator = KanBrdfEvaluator::new();
        let mut m_embed = [0.0_f32; EMBEDDING_DIM];
        for i in 0..16 {
            m_embed[i] = 0.1 * i as f32;
        }
        let inp = evaluator.pack_input(&m_embed, &e, 550.0, &t);
        // Channels 0..16 = m_embed[0..16].
        for i in 0..16 {
            assert!((inp[i] - m_embed[i]).abs() < 1e-6);
        }
        // Channel 20 = hero-norm in [0, 1].
        assert!(inp[20] >= 0.0 && inp[20] <= 1.0);
        // Channels 21..32 = 0.
        for i in 21..EMBEDDING_DIM {
            assert_eq!(inp[i], 0.0);
        }
    }

    /// § evaluate against a SpectralBrdf material returns 16 bands.
    #[test]
    fn evaluate_spectral_brdf_returns_16_bands() {
        let t = BandTable::d65();
        let f = make_frame();
        let mut e = [0.5_f32; EMBEDDING_DIM];
        e[0] = 0.7;
        let m = KanMaterial::spectral_brdf::<16>(e);
        let bands = KanBrdfEvaluator::new().evaluate(&m, &f, 550.0, &t);
        assert_eq!(bands.len(), BRDF_OUT_DIM);
        // Some band must be > 0 (synthesis path produces non-zero output).
        assert!(bands.iter().any(|&v| v > 0.0));
    }

    /// § evaluate against non-Spectral material returns zeros.
    #[test]
    fn evaluate_non_spectral_returns_zeros() {
        let t = BandTable::d65();
        let f = make_frame();
        let m = KanMaterial::single_band_brdf([0.5; EMBEDDING_DIM]);
        let bands = KanBrdfEvaluator::new().evaluate(&m, &f, 550.0, &t);
        for v in bands {
            assert_eq!(v, 0.0);
        }
    }

    /// § evaluate_radiance returns a valid SpectralRadiance.
    #[test]
    fn evaluate_radiance_returns_valid() {
        let t = BandTable::d65();
        let f = make_frame();
        let m = KanMaterial::spectral_brdf::<16>([0.6; EMBEDDING_DIM]);
        let r = KanBrdfEvaluator::new().evaluate_radiance(&m, &f, 580.0, &t);
        assert!(r.is_well_formed());
    }

    /// § Different hero wavelengths produce different band peaks (dispersion).
    #[test]
    fn different_hero_different_peak() {
        let t = BandTable::d65();
        let f = make_frame();
        let m = KanMaterial::spectral_brdf::<16>([0.5; EMBEDDING_DIM]);
        let evaluator = KanBrdfEvaluator::new();
        let bands_blue = evaluator.evaluate(&m, &f, 440.0, &t);
        let bands_red = evaluator.evaluate(&m, &f, 700.0, &t);
        // Argmax of bands_blue should be lower-index than argmax of bands_red.
        let am_blue = argmax(&bands_blue);
        let am_red = argmax(&bands_red);
        assert!(am_blue < am_red);
    }

    /// § Different m_embed produces different albedo curves.
    #[test]
    fn different_embed_different_albedo() {
        let t = BandTable::d65();
        let f = make_frame();
        let m1 = KanMaterial::spectral_brdf::<16>([0.2; EMBEDDING_DIM]);
        let m2 = KanMaterial::spectral_brdf::<16>([0.9; EMBEDDING_DIM]);
        let evaluator = KanBrdfEvaluator::new();
        let b1 = evaluator.evaluate(&m1, &f, 550.0, &t);
        let b2 = evaluator.evaluate(&m2, &f, 550.0, &t);
        // Some band must differ.
        let any_diff = b1.iter().zip(b2.iter()).any(|(a, b)| (a - b).abs() > 1e-3);
        assert!(any_diff);
    }

    /// § approx_unit is generous (handles small numeric drift).
    #[test]
    fn approx_unit_tolerates_drift() {
        let v = Vec3::new(0.99, 0.0, 0.0);
        assert!(approx_unit(v));
    }

    /// § sigmoid bounded [0, 1].
    #[test]
    fn sigmoid_bounded() {
        assert!(sigmoid(-100.0) >= 0.0);
        assert!(sigmoid(100.0) <= 1.0);
        assert!((sigmoid(0.0) - 0.5).abs() < 1e-6);
    }

    /// § powf5 matches x^5.
    #[test]
    fn powf5_matches_x5() {
        let x = 0.5_f32;
        let p = powf5(x);
        assert!((p - x.powi(5)).abs() < 1e-6);
    }

    /// § dot3 + add3 are correct.
    #[test]
    fn vec3_helpers() {
        let a = Vec3::new(1.0, 2.0, 3.0);
        let b = Vec3::new(4.0, 5.0, 6.0);
        assert!((dot3(a, b) - 32.0).abs() < 1e-6);
        let s = add3(a, b);
        assert_eq!(s.x, 5.0);
        assert_eq!(s.y, 7.0);
        assert_eq!(s.z, 9.0);
    }

    /// § n_dot_v + n_dot_l + n_dot_h all in [0, 1].
    #[test]
    fn cosines_in_unit_interval() {
        let f = make_frame();
        for v in [f.n_dot_v(), f.n_dot_l(), f.n_dot_h()] {
            assert!((0.0..=1.0).contains(&v), "cos {v} out of [0, 1]");
        }
    }

    fn argmax(b: &[f32; BRDF_OUT_DIM]) -> usize {
        let mut best = 0usize;
        let mut bv = b[0];
        for (i, v) in b.iter().enumerate().skip(1) {
            if *v > bv {
                bv = *v;
                best = i;
            }
        }
        best
    }
}
