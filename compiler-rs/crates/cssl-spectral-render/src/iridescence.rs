//! § IridescenceModel — thin-film interference + dispersion
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Per `07_AES/03 § IV` :
//!     "iridescence + thin-film effects :
//!       @ M-coord axis-15 (anisotropy) > threshold ⊗ KAN-derived thin-film stack
//!       @ angle-dependent reflection-spectrum"
//!
//!   And per `07_AES/07 § VIII` iridescence variant :
//!     "when m_embed.axis_15 > τ_aniso (anisotropy threshold)
//!       ⇒ activate KAN_IRIDESCENCE_STACK (33D input incl. cosθ)
//!       ⇒ angle-dependent reflectance-spectrum
//!       ⇒ thin-film-stack visible (oil-slick, butterfly-wing tests)"
//!
//!   This module owns the thin-film physics + dispersion composition. It
//!   evaluates the 33-D input (32-D `m_embed` + 1 cosθ) against the
//!   iridescence-variant KAN, but the per-band reflectance is computed via
//!   a parametric thin-film formula (Newton-rings + Fresnel for the n-layer
//!   stack), since the canonical KAN-runtime is deferred.
//!
//!   Reference physics : the airy reflectance for an N-layer thin-film stack
//!   is well-defined ; we use the simplest single-layer parameterized form
//!   (one film + one substrate) and derive `R(λ, cosθ)` from the optical
//!   path difference Δ = 2·n·d·cosθ. The interference factor is `cos²(2π·Δ/λ)`.

use crate::band::{BandTable, BAND_COUNT};
use crate::radiance::SpectralRadiance;
use cssl_substrate_kan::kan_material::EMBEDDING_DIM;

/// § The anisotropy threshold above which iridescence is active. Per
///   `07_AES/03 § IV` referenced as `axis-15 > threshold`. We pick 0.55 as
///   the canonical threshold ; this is the value tagged in the cohort
///   tests.
pub const ANISOTROPY_THRESHOLD: f32 = 0.55;

/// § The maximum number of thin-film layers in the stack. The full Optics-
///   2024 reference supports up to 6 ; we use 4 as the runtime cap to fit
///   the Quest-3 budget.
pub const THIN_FILM_LAYERS_MAX: usize = 4;

/// § A single thin-film layer in the stack — physical thickness + index of
///   refraction. The IOR is per-band so dispersion is naturally captured.
#[derive(Debug, Clone, Copy)]
pub struct ThinFilmLayer {
    /// § Physical thickness in nm.
    pub thickness_nm: f32,
    /// § Per-band index of refraction. The 16 entries align with the
    ///   `BAND_TABLE`.
    pub ior_per_band: [f32; BAND_COUNT],
}

impl ThinFilmLayer {
    /// § Default layer : ~300 nm soap-film with a flat IOR of 1.33 (water-
    ///   like). Useful as a smoke-test fixture.
    #[must_use]
    pub fn soap_film() -> Self {
        Self {
            thickness_nm: 300.0,
            ior_per_band: [1.33; BAND_COUNT],
        }
    }

    /// § Construct a layer with a constant IOR.
    #[must_use]
    pub fn flat(thickness_nm: f32, ior: f32) -> Self {
        Self {
            thickness_nm,
            ior_per_band: [ior; BAND_COUNT],
        }
    }
}

/// § A stack of thin-film layers. Per `07_AES/03 § IV` the stack height
///   maps to the M-coord embedding's "iridescence" axes ; we expose the
///   stack count + per-layer parameters so callers can author exotic
///   surfaces (peacock-feather = 2 layers, oil-on-water = 1, butterfly-
///   wing = 3+).
#[derive(Debug, Clone)]
pub struct ThinFilmStack {
    /// § The number of valid layers in `layers`. (0..=THIN_FILM_LAYERS_MAX).
    pub layer_count: u8,
    /// § Per-layer parameters.
    pub layers: [ThinFilmLayer; THIN_FILM_LAYERS_MAX],
    /// § Substrate IOR (the medium below all layers, e.g. water under oil).
    pub substrate_ior: f32,
}

impl ThinFilmStack {
    /// § Construct an empty (no layers) stack.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            layer_count: 0,
            layers: [ThinFilmLayer::soap_film(); THIN_FILM_LAYERS_MAX],
            substrate_ior: 1.0,
        }
    }

    /// § Single-layer oil-on-water stack — the canonical "rainbow puddle"
    ///   demo. ~600 nm oil layer over water substrate.
    #[must_use]
    pub fn oil_on_water() -> Self {
        let mut s = Self::empty();
        s.layers[0] = ThinFilmLayer::flat(600.0, 1.45);
        s.layer_count = 1;
        s.substrate_ior = 1.33;
        s
    }

    /// § Two-layer peacock-feather stack — alternating melanin + keratin,
    ///   each ~70 nm. Produces the green-blue iridescence band.
    #[must_use]
    pub fn peacock_feather() -> Self {
        let mut s = Self::empty();
        s.layers[0] = ThinFilmLayer::flat(70.0, 1.85);
        s.layers[1] = ThinFilmLayer::flat(70.0, 1.55);
        s.layer_count = 2;
        s.substrate_ior = 1.45;
        s
    }

    /// § Three-layer butterfly-wing stack — chitin layers at ~150 nm each.
    #[must_use]
    pub fn butterfly_wing() -> Self {
        let mut s = Self::empty();
        s.layers[0] = ThinFilmLayer::flat(150.0, 1.56);
        s.layers[1] = ThinFilmLayer::flat(150.0, 1.45);
        s.layers[2] = ThinFilmLayer::flat(150.0, 1.56);
        s.layer_count = 3;
        s.substrate_ior = 1.40;
        s
    }

    /// § Number of valid layers in the stack.
    #[must_use]
    pub fn count(&self) -> usize {
        (self.layer_count as usize).min(THIN_FILM_LAYERS_MAX)
    }
}

impl Default for ThinFilmStack {
    fn default() -> Self {
        Self::empty()
    }
}

/// § The iridescence model. Owns no state ; each call takes the embedding +
///   stack + cosθ + base reflectance.
#[derive(Debug, Clone, Copy, Default)]
pub struct IridescenceModel;

impl IridescenceModel {
    /// § Construct an iridescence model.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// § True iff the given material's anisotropy axis (axis 15 per the spec)
    ///   crosses the threshold for iridescence activation.
    #[must_use]
    pub fn is_active(&self, m_embed: &[f32; EMBEDDING_DIM]) -> bool {
        m_embed[15].abs() >= ANISOTROPY_THRESHOLD
    }

    /// § Modulate the base spectral reflectance by the thin-film
    ///   interference factor for each band. The result is the angle- and
    ///   wavelength-dependent reflectance of the stack.
    ///
    ///   `cos_theta` is the angle between the surface normal and the view
    ///   direction (clamped to `[0, 1]`).
    pub fn modulate(
        &self,
        base: &mut [f32; BAND_COUNT],
        stack: &ThinFilmStack,
        cos_theta: f32,
        table: &BandTable,
    ) {
        let cos_t = cos_theta.max(0.05).min(1.0);
        let count = stack.count();
        if count == 0 {
            return;
        }
        // § Per-band : compute the airy reflectance of the stack via the
        //   single-layer Fresnel + interference approximation. For an
        //   N-layer stack we apply the multiplicative composition of each
        //   layer's interference factor (a first-order approximation that
        //   matches the perceptual goal without needing the full transfer-
        //   matrix method).
        for i in 0..BAND_COUNT {
            let lambda = table.band(i).center_nm;
            let mut interference = 1.0_f32;
            let mut prev_ior = 1.0_f32; // ambient air IOR
            for k in 0..count {
                let layer = &stack.layers[k];
                let n = layer.ior_per_band[i];
                let cos_t_in_layer = self.refracted_cos(cos_t, prev_ior, n);
                // Optical path difference : 2 · n · d · cos(θ).
                let path = 2.0 * n * layer.thickness_nm * cos_t_in_layer;
                // Phase = 2π · path / λ.
                let phase = std::f32::consts::TAU * path / lambda;
                // Interference factor = cos²(phase / 2). This produces the
                // classic Newton-rings spectrum.
                let f = (0.5 * phase).cos();
                interference *= 0.6 + 0.4 * f * f;
                prev_ior = n;
            }
            // § Substrate fresnel.
            let r_sub = self.fresnel_schlick(cos_t, prev_ior, stack.substrate_ior);
            base[i] *= interference * (0.5 + 0.5 * r_sub);
        }
    }

    /// § Modulate a SpectralRadiance in-place (convenience wrapper).
    pub fn modulate_radiance(
        &self,
        r: &mut SpectralRadiance,
        stack: &ThinFilmStack,
        cos_theta: f32,
        table: &BandTable,
    ) {
        self.modulate(&mut r.bands, stack, cos_theta, table);
        // After per-band modulation the hero+accompaniment view is stale.
        // Rebuild it from the band buffer.
        r.accumulate_from_bands(4, table);
    }

    /// § Snell's law for the cosine of the refracted angle in the next
    ///   medium. Returns 1.0 (TIR) if total internal reflection.
    fn refracted_cos(&self, cos_in: f32, n_in: f32, n_out: f32) -> f32 {
        let sin_in_sq = (1.0 - cos_in * cos_in).max(0.0);
        let ratio = n_in / n_out.max(1e-6);
        let sin_out_sq = sin_in_sq * ratio * ratio;
        if sin_out_sq >= 1.0 {
            return 1.0; // TIR
        }
        (1.0 - sin_out_sq).max(0.0).sqrt()
    }

    /// § Schlick approximation of the Fresnel reflectance at a dielectric
    ///   interface.
    fn fresnel_schlick(&self, cos_t: f32, n_a: f32, n_b: f32) -> f32 {
        let r0_root = (n_a - n_b) / (n_a + n_b).max(1e-6);
        let r0 = r0_root * r0_root;
        let one_minus_cos = (1.0 - cos_t).max(0.0);
        let m = one_minus_cos * one_minus_cos;
        r0 + (1.0 - r0) * m * m * one_minus_cos
    }

    /// § Construct a per-fragment 33-D KAN input for the iridescence-stack
    ///   variant per `07_AES/07 § VIII`. The first 32 entries are the
    ///   m_embed, the 33rd is `cos(θ)`. This packing exists so a future
    ///   slice can wire the iridescence-variant KAN dispatch directly.
    #[must_use]
    pub fn pack_input_33d(
        &self,
        m_embed: &[f32; EMBEDDING_DIM],
        cos_theta: f32,
    ) -> [f32; EMBEDDING_DIM + 1] {
        let mut out = [0.0_f32; EMBEDDING_DIM + 1];
        out[..EMBEDDING_DIM].copy_from_slice(m_embed);
        out[EMBEDDING_DIM] = cos_theta.max(0.0).min(1.0);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// § Anisotropy threshold matches spec.
    #[test]
    fn anisotropy_threshold_value() {
        assert!((ANISOTROPY_THRESHOLD - 0.55).abs() < 1e-6);
    }

    /// § is_active triggers above threshold.
    #[test]
    fn is_active_above_threshold() {
        let mut e = [0.0_f32; EMBEDDING_DIM];
        e[15] = 0.7;
        assert!(IridescenceModel::new().is_active(&e));
    }

    /// § is_active false below threshold.
    #[test]
    fn is_active_below_threshold() {
        let mut e = [0.0_f32; EMBEDDING_DIM];
        e[15] = 0.3;
        assert!(!IridescenceModel::new().is_active(&e));
    }

    /// § oil_on_water has 1 layer.
    #[test]
    fn oil_on_water_one_layer() {
        let s = ThinFilmStack::oil_on_water();
        assert_eq!(s.count(), 1);
    }

    /// § peacock_feather has 2 layers.
    #[test]
    fn peacock_two_layers() {
        let s = ThinFilmStack::peacock_feather();
        assert_eq!(s.count(), 2);
    }

    /// § butterfly_wing has 3 layers.
    #[test]
    fn butterfly_three_layers() {
        let s = ThinFilmStack::butterfly_wing();
        assert_eq!(s.count(), 3);
    }

    /// § empty stack produces no modulation.
    #[test]
    fn empty_stack_no_modulation() {
        let mut bands = [0.5_f32; BAND_COUNT];
        let pre = bands;
        let t = BandTable::d65();
        IridescenceModel::new().modulate(&mut bands, &ThinFilmStack::empty(), 1.0, &t);
        assert_eq!(bands, pre);
    }

    /// § Single-layer modulation produces band-dependent variation.
    #[test]
    fn single_layer_modulates_bands_differently() {
        let mut bands = [0.5_f32; BAND_COUNT];
        let t = BandTable::d65();
        IridescenceModel::new().modulate(&mut bands, &ThinFilmStack::oil_on_water(), 0.7, &t);
        let any_diff = bands.windows(2).any(|w| (w[0] - w[1]).abs() > 1e-3);
        assert!(any_diff, "all bands equal after modulation : {:?}", bands);
    }

    /// § refracted_cos : flat IOR returns input.
    #[test]
    fn refracted_flat_ior() {
        let m = IridescenceModel::new();
        let cos_out = m.refracted_cos(0.5, 1.0, 1.0);
        assert!((cos_out - 0.5).abs() < 1e-6);
    }

    /// § refracted_cos : large IOR-step into denser medium reduces angle
    ///   (cos increases toward 1).
    #[test]
    fn refracted_into_denser_increases_cos() {
        let m = IridescenceModel::new();
        let cos_in = 0.5;
        let cos_out = m.refracted_cos(cos_in, 1.0, 1.5);
        assert!(cos_out >= cos_in);
    }

    /// § fresnel_schlick increases at grazing (cos_t → 0).
    #[test]
    fn fresnel_increases_at_grazing() {
        let m = IridescenceModel::new();
        let f_normal = m.fresnel_schlick(1.0, 1.0, 1.5);
        let f_grazing = m.fresnel_schlick(0.05, 1.0, 1.5);
        assert!(f_grazing > f_normal);
    }

    /// § pack_input_33d copies m_embed + sets cos_theta.
    #[test]
    fn pack_input_33d_layout() {
        let mut e = [0.0_f32; EMBEDDING_DIM];
        for i in 0..EMBEDDING_DIM {
            e[i] = 0.1 * i as f32;
        }
        let inp = IridescenceModel::new().pack_input_33d(&e, 0.5);
        for i in 0..EMBEDDING_DIM {
            assert!((inp[i] - e[i]).abs() < 1e-6);
        }
        assert!((inp[EMBEDDING_DIM] - 0.5).abs() < 1e-6);
    }

    /// § modulate_radiance refreshes hero/accompaniment.
    #[test]
    fn modulate_radiance_refreshes_hero() {
        let t = BandTable::d65();
        let mut r = SpectralRadiance::black();
        for i in 0..BAND_COUNT {
            r.bands[i] = 0.5;
        }
        IridescenceModel::new().modulate_radiance(&mut r, &ThinFilmStack::oil_on_water(), 0.7, &t);
        // Hero intensity must be non-zero post-modulation.
        assert!(r.hero_intensity > 0.0);
    }
}
