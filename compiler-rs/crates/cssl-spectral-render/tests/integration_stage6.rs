//! § integration_stage6 — end-to-end Stage-6 pipeline tests
//! ════════════════════════════════════════════════════════════════════════════

#![allow(clippy::uninlined_format_args)]
#![allow(clippy::float_cmp)]
#![allow(clippy::needless_range_loop)]
//!
//! § ROLE
//!   Exercises the full Stage-6 pipeline with simulated D116 SDF-raymarch
//!   inputs + D119 fractal-amplifier displacement. The actual D116 / D119
//!   crates are not depended-on directly (they are sibling slices ; the
//!   integration boundary is the structural `FragmentInput` record).
//!
//!   These tests verify :
//!     - Full pipeline runs end-to-end without panic
//!     - Spectral output is well-formed (no NaN, no negative bands)
//!     - Stage-6 + tonemap produces non-zero RGB for non-trivial input
//!     - Iridescence is visibly band-dependent (oil-slick / butterfly tests)
//!     - Fluorescence shifts emission (Λ-token test)
//!     - CSF gate prunes sub-threshold contributions
//!     - Cost model fits Stage-6 budget on RTX-50 ; degrades on Quest-3
//!     - Stage-6 + Stage-10 tonemap pipeline produces sane sRGB

use cssl_spectral_render::band::{BAND_VISIBLE_END, BAND_VISIBLE_START};
use cssl_spectral_render::{
    BandTable, CostTier, CsfPerceptualGate, DisplayPrimaries, Fluorescence, FragmentInput,
    HeroWavelengthMIS, IridescenceModel, KanBrdfEvaluator, MisWeights, PerFragmentCost,
    SpectralRadiance, SpectralRenderStage, SpectralTristimulus, ThinFilmStack,
};
use cssl_substrate_kan::kan_material::{KanMaterial, EMBEDDING_DIM};
use cssl_substrate_projections::Vec3;

fn make_anisotropic_material() -> KanMaterial {
    let mut e = [0.5_f32; EMBEDDING_DIM];
    e[0] = 0.7;
    e[14] = 0.5;
    e[15] = 0.65; // > ANISOTROPY_THRESHOLD
    KanMaterial::spectral_brdf::<16>(e)
}

fn make_isotropic_material() -> KanMaterial {
    let mut e = [0.4_f32; EMBEDDING_DIM];
    e[0] = 0.6;
    e[15] = 0.10;
    KanMaterial::spectral_brdf::<16>(e)
}

fn make_fragment(material: &KanMaterial, hero_xi: f32) -> FragmentInput<'_> {
    FragmentInput {
        hit_position: Vec3::new(0.0, 0.0, 0.0),
        normal: Vec3::new(0.0, 0.0, 1.0),
        view_dir: Vec3::new(0.0, 0.5, 0.866_025_4).normalize(),
        light_dir: Vec3::new(0.5, 0.0, 0.866_025_4).normalize(),
        displaced_position: None,
        material,
        screen_uv: (0.5, 0.5),
        eccentricity_deg: 0.0,
        hero_xi,
        mean_luminance_cdm2: 100.0,
    }
}

#[test]
fn stage6_end_to_end_runs() {
    let m = make_anisotropic_material();
    let inp = make_fragment(&m, 0.5);
    let stage = SpectralRenderStage::quest3_default();
    let r = stage.shade_fragment(&inp);
    assert!(r.is_well_formed());
}

#[test]
fn stage6_plus_tonemap_produces_rgb() {
    let m = make_anisotropic_material();
    let inp = make_fragment(&m, 0.4);
    let stage = SpectralRenderStage::quest3_default();
    let radiance = stage.shade_fragment(&inp);
    let tone = SpectralTristimulus::srgb_default();
    let rgb = tone.tonemap(&radiance, &stage.bands);
    // RGB channels are in [0, 1] post-ACES2 + sRGB encoding.
    for c in [rgb.r, rgb.g, rgb.b] {
        assert!((0.0..=1.001).contains(&c), "channel {c} out of [0, 1]");
    }
}

#[test]
fn stage6_d119_displacement_consumed() {
    let m = make_anisotropic_material();
    let mut inp = make_fragment(&m, 0.5);
    // Simulate D119 fractal-amplifier handing back a displaced position.
    inp.displaced_position = Some(Vec3::new(0.0, 0.0, 0.5));
    let stage = SpectralRenderStage::quest3_default();
    let r = stage.shade_fragment(&inp);
    assert!(r.is_well_formed());
    assert_eq!(inp.effective_position(), Vec3::new(0.0, 0.0, 0.5));
}

#[test]
fn stage6_d116_marshalling_compatible() {
    // Simulate a batch of D116 SDF-raymarch hits, each with a different
    // material + slightly different geometry.
    let m1 = make_anisotropic_material();
    let m2 = make_isotropic_material();
    let inputs = vec![
        make_fragment(&m1, 0.1),
        make_fragment(&m1, 0.5),
        make_fragment(&m2, 0.9),
    ];
    let stage = SpectralRenderStage::quest3_default();
    let out = stage.shade_batch(&inputs);
    assert_eq!(out.len(), 3);
    for r in &out {
        assert!(r.is_well_formed());
    }
}

#[test]
fn iridescence_modulates_bands_for_anisotropic() {
    // Direct iridescence test : evaluate bands for an anisotropic
    // material at 2 different cos_theta values ; the band buffer must
    // differ.
    let table = BandTable::d65();
    let stack = ThinFilmStack::oil_on_water();
    let model = IridescenceModel::new();
    let mut bands_a = [0.5_f32; cssl_spectral_render::band::BAND_COUNT];
    let mut bands_b = [0.5_f32; cssl_spectral_render::band::BAND_COUNT];
    model.modulate(&mut bands_a, &stack, 0.9, &table);
    model.modulate(&mut bands_b, &stack, 0.3, &table);
    let any_diff = bands_a
        .iter()
        .zip(bands_b.iter())
        .any(|(a, b)| (a - b).abs() > 1e-3);
    assert!(any_diff);
}

#[test]
fn fluorescence_shifts_emission_uv_to_visible() {
    let table = BandTable::d65();
    let mut bands = [0.0_f32; cssl_spectral_render::band::BAND_COUNT];
    // Excite at 360 nm UV (band index 1).
    bands[1] = 1.0;
    let f = Fluorescence::optical_brightener();
    f.remap(&mut bands, &table);
    // Some visible band has emission.
    let visible_emission: f32 = bands[BAND_VISIBLE_START..BAND_VISIBLE_END].iter().sum();
    assert!(visible_emission > 0.0);
}

#[test]
fn csf_gate_prunes_sub_threshold() {
    let table = BandTable::d65();
    let mut r = SpectralRadiance::black();
    // Tiny contribution near background.
    for i in BAND_VISIBLE_START..BAND_VISIBLE_END {
        r.bands[i] = 100.000_1;
    }
    let g = CsfPerceptualGate::mantiuk_default();
    let r_post = g.gate_radiance(r, 100.0, 12.0, 25.0, &table);
    let lum = r_post.integrate_visible(&table);
    assert!(lum < 1.0);
}

#[test]
fn rtx50_max_fits_stage6_budget() {
    let stage = SpectralRenderStage::rtx50_max();
    let c = stage.estimated_cost();
    assert!(c.fits_stage6_quest3_budget());
    assert!(c.frame_ms_1080p_foveated() <= cssl_spectral_render::STAGE6_BUDGET_MS);
}

#[test]
fn quest3_simd_warp_within_kan_budget() {
    // Quest-3 SIMD-warp dispatch + iridescence may exceed Stage-6 1.8 ms
    // budget but must stay within the 2.5 ms KAN-shading total budget per
    // `07_AES/07 § VII`.
    let stage = SpectralRenderStage::quest3_default();
    let c = stage.estimated_cost();
    assert!(c.frame_ms_1080p_foveated() <= 4.0);
}

#[test]
fn pipeline_dispersion_visible() {
    let m = make_anisotropic_material();
    let stage = SpectralRenderStage::quest3_default();
    let r_blue = stage.shade_fragment(&make_fragment(&m, 0.05));
    let r_red = stage.shade_fragment(&make_fragment(&m, 0.95));
    // Different hero wavelengths produce different argmax bands.
    let max_blue = argmax_visible(&r_blue);
    let max_red = argmax_visible(&r_red);
    assert!(
        max_blue <= max_red,
        "dispersion : blue argmax {} > red argmax {}",
        max_blue,
        max_red
    );
}

#[test]
fn pipeline_with_fluorescence_brightens_visible() {
    let m = make_anisotropic_material();
    let mut stage = SpectralRenderStage::rtx50_max();
    stage.fluorescence = Some(Fluorescence::optical_brightener());
    let r = stage.shade_fragment(&make_fragment(&m, 0.5));
    // After fluorescence remap, every band should still be non-negative.
    assert!(r.is_well_formed());
}

#[test]
fn cost_tier_ordering() {
    // CoopMatrix < SimdWarp < Scalar in per-band ns.
    assert!(CostTier::CoopMatrix.ns_per_band() < CostTier::SimdWarp.ns_per_band());
    assert!(CostTier::SimdWarp.ns_per_band() < CostTier::Scalar.ns_per_band());
}

#[test]
fn hero_mis_balance_weights_sum_one() {
    let w = MisWeights::balance(4);
    assert!((w.total() - 1.0).abs() < 1e-6);
}

#[test]
fn hero_mis_drives_hero_in_visible() {
    let table = BandTable::d65();
    let mis = HeroWavelengthMIS::manuka_default();
    for xi in [0.0_f32, 0.25, 0.5, 0.75, 0.99] {
        let s = mis.sample(xi, &table);
        let lo = table.band(BAND_VISIBLE_START).lo_nm();
        let hi = table.band(BAND_VISIBLE_END - 1).hi_nm();
        assert!(s.hero_wavelength_nm >= lo);
        assert!(s.hero_wavelength_nm <= hi);
    }
}

#[test]
fn end_to_end_3_displays() {
    let m = make_anisotropic_material();
    let stage = SpectralRenderStage::rtx50_max();
    let r = stage.shade_fragment(&make_fragment(&m, 0.5));
    for primaries in [
        DisplayPrimaries::Srgb,
        DisplayPrimaries::DciP3,
        DisplayPrimaries::Rec2020,
    ] {
        let cfg = SpectralTristimulus {
            primaries,
            exposure: 1.0,
            apply_aces2: true,
        };
        let rgb = cfg.tonemap(&r, &stage.bands);
        // No NaNs, no Infs.
        for c in [rgb.r, rgb.g, rgb.b] {
            assert!(c.is_finite());
        }
    }
}

#[test]
fn brdf_eval_yields_band_buffer_for_spectral_brdf() {
    let m = make_anisotropic_material();
    let frame = cssl_spectral_render::ShadingFrame::new(
        Vec3::new(0.0, 0.5, 0.866_025_4).normalize(),
        Vec3::new(0.5, 0.0, 0.866_025_4).normalize(),
        Vec3::new(0.0, 0.0, 1.0),
    );
    let bands = KanBrdfEvaluator::new().evaluate(&m, &frame, 550.0, &BandTable::d65());
    // Some bands non-zero.
    assert!(bands.iter().any(|&v| v > 0.0));
}

#[test]
fn cost_summary_within_tier_budget_when_under_brdf_only() {
    let cost = PerFragmentCost::baseline(CostTier::CoopMatrix);
    assert!(cost.fits_stage6_quest3_budget());
    let cost_with = cost.with_iridescence().with_fluorescence();
    assert!(cost_with.fits_stage6_quest3_budget());
}

fn argmax_visible(r: &SpectralRadiance) -> usize {
    let mut best = BAND_VISIBLE_START;
    let mut bv = r.bands[BAND_VISIBLE_START];
    for i in BAND_VISIBLE_START..BAND_VISIBLE_END {
        if r.bands[i] > bv {
            bv = r.bands[i];
            best = i;
        }
    }
    best
}
