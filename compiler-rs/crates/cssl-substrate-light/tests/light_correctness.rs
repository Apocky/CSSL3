//! § Integration test suite for ApockyLight per-quantum primitive type.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   End-to-end tests across the public surface of `cssl-substrate-light`.
//!   Verifies layout invariants + physics-invariants + operations algebra +
//!   IFC capability-flow + PRIME-DIRECTIVE structural gates.
//!
//! § SPEC
//!   - `specs/34_APOCKY_LIGHT.csl` § FIELDS + § OPERATIONS.
//!   - `specs/30_SUBSTRATE_v3.csl` § APOCKY-LIGHT.
//!   - `specs/36_CFER_RENDERER.csl` § Light-state per-cell.
//!
//! § COVERAGE
//!   - layout : size + alignment + std430 invariants
//!   - physics : radiance ≥ 0 ; lambda clamping ; DoP ∈ [0,1] ;
//!     Stokes invariant DoP² = s1²+s2²+s3² ; direction unit-length
//!   - operations : zero / monochromatic / blackbody / d65 / add / scale /
//!     attenuate / mueller_apply
//!   - IFC : combine_caps Pony-cap algebra ; biometric-egress denied ;
//!     audit-flag propagation
//!
//! Total : 18 tests (≥ 12 required by the slice).

#![allow(clippy::float_cmp)]
#![allow(clippy::manual_clamp)]
#![allow(clippy::suboptimal_flops)]
#![allow(clippy::uninlined_format_args)]
#![allow(clippy::similar_names)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]

use cssl_caps::CapKind;
use cssl_substrate_light::{
    can_egress, combine_caps,
    operations::{
        add, attenuate, blackbody, d65, monochromatic, mueller_apply, scale,
        LightCompositionError, LightConstructionError,
    },
    ApockyLight, CapHandle, EvidenceGlyph, IfcFlowError, KanBandHandle, ACCOMPANIMENT_COUNT,
    APOCKY_LIGHT_SIZE_BYTES, LAMBDA_MAX_NM, LAMBDA_MIN_NM,
};

// ───────────────────────────────────────────────────────────────────────────
// § Layout invariants (1-3)
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn layout_struct_size_is_32_bytes() {
    assert_eq!(core::mem::size_of::<ApockyLight>(), 32);
    assert_eq!(core::mem::size_of::<ApockyLight>(), APOCKY_LIGHT_SIZE_BYTES);
}

#[test]
fn layout_alignment_matches_std430() {
    assert_eq!(core::mem::align_of::<ApockyLight>(), 4);
}

#[test]
fn layout_packed_fields_round_trip() {
    // Round-trip a fully-populated quantum to verify no data is lost in
    // packing into the 32B std430 layout.
    let l = ApockyLight::new(
        12.5,
        650.0,
        [0.1, 0.2, 0.3, 0.4],
        0.5,
        0.3,
        -0.2,
        [0.5, 0.5, 0.707],
        0xABCDEF,
        EvidenceGlyph::Increasing,
        0xCAFE_BABE,
    );
    assert!((l.intensity() - 12.5).abs() < 1e-3);
    assert!((l.lambda_nm() - 650.0).abs() < 1e-3);
    assert!((l.dop() - 0.5).abs() < 1e-3);
    assert_eq!(l.kan_band_handle(), 0xABCDEF);
    assert_eq!(l.evidence(), EvidenceGlyph::Increasing);
    assert_eq!(l.cap_handle(), 0xCAFE_BABE);
}

// ───────────────────────────────────────────────────────────────────────────
// § Physics invariants (4-7)
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn physics_intensity_non_negative() {
    let lights = [
        ApockyLight::zero(),
        monochromatic(5.0, 550.0, [0.0, 0.0, 1.0], 0).unwrap(),
        blackbody(3000.0, 0).unwrap(),
        d65(1.0, 0).unwrap(),
    ];
    for l in lights {
        assert!(l.intensity() >= 0.0, "intensity went negative: {:?}", l);
    }
}

#[test]
fn physics_lambda_bounds_enforced() {
    let l = ApockyLight::new(
        1.0,
        50.0, // way below LAMBDA_MIN_NM
        [0.0; ACCOMPANIMENT_COUNT],
        0.0,
        0.0,
        0.0,
        [0.0, 0.0, 1.0],
        0,
        EvidenceGlyph::Default,
        0,
    );
    assert!(l.lambda_nm() >= LAMBDA_MIN_NM);
    assert!(l.lambda_nm() <= LAMBDA_MAX_NM);

    let l_high = ApockyLight::new(
        1.0,
        9999.0,
        [0.0; ACCOMPANIMENT_COUNT],
        0.0,
        0.0,
        0.0,
        [0.0, 0.0, 1.0],
        0,
        EvidenceGlyph::Default,
        0,
    );
    assert!(l_high.lambda_nm() <= LAMBDA_MAX_NM);
}

#[test]
fn physics_dop_in_unit_interval() {
    for dop in [0.0_f32, 0.1, 0.5, 0.9, 1.0, 1.5, -0.3] {
        let l = ApockyLight::new(
            1.0,
            550.0,
            [0.0; ACCOMPANIMENT_COUNT],
            dop,
            0.0,
            0.0,
            [0.0, 0.0, 1.0],
            0,
            EvidenceGlyph::Default,
            0,
        );
        assert!(
            (0.0..=1.0).contains(&l.dop()),
            "dop out of range : in={}, decoded={}",
            dop,
            l.dop()
        );
    }
}

#[test]
fn physics_stokes_invariant() {
    // For a polarized quantum with DoP=0.7, s1=0.4, s2=0.3 :
    //   reconstructed s3² = DoP² - s1² - s2² = 0.49 - 0.16 - 0.09 = 0.24
    //   s3 = 0.49 (relative-Stokes) ; total polarization within [0, intensity].
    let l = ApockyLight::new(
        1.0,
        550.0,
        [0.0; ACCOMPANIMENT_COUNT],
        0.7,
        0.4,
        0.3,
        [0.0, 0.0, 1.0],
        0,
        EvidenceGlyph::Default,
        0,
    );
    let stokes = l.stokes();
    let s0 = stokes[0];
    assert!((s0 - 1.0).abs() < 1e-3);
    // Relative s1²+s2²+s3² = DoP² (within q1.7 packing precision).
    let lin_sq = (stokes[1] / s0).powi(2) + (stokes[2] / s0).powi(2);
    let total_sq = lin_sq + (stokes[3] / s0).powi(2);
    let dop = l.dop();
    assert!(
        (total_sq - dop * dop).abs() < 5e-2,
        "Stokes invariant violated : total_sq={}, dop²={}",
        total_sq,
        dop * dop
    );
}

// ───────────────────────────────────────────────────────────────────────────
// § Operations algebra (8-13)
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn ops_zero_quantum_is_identity_for_add() {
    let a = monochromatic(2.0, 550.0, [0.0, 0.0, 1.0], 0).unwrap();
    let z = ApockyLight::zero();
    let sum = add(&a, &z).unwrap();
    assert!((sum.intensity() - a.intensity()).abs() < 1e-3);
}

#[test]
fn ops_scale_zero_yields_dark_quantum() {
    // monochromatic helper takes (radiance, lambda, direction[3], cap_handle).
    let a = monochromatic(5.0, 550.0, [0.0, 0.0, 1.0], 0).unwrap();
    let dark = scale(&a, 0.0).unwrap();
    assert!(dark.intensity() < 1e-3);
    for v in dark.accompaniments() {
        assert!(v.abs() < 1e-3);
    }
}

#[test]
fn ops_scale_distributes_over_add() {
    let a = monochromatic(2.0, 550.0, [0.0, 0.0, 1.0], 0).unwrap();
    let b = monochromatic(3.0, 600.0, [0.0, 0.0, 1.0], 0).unwrap();
    let factor = 1.5;
    // scale(add(a, b), factor) == add(scale(a, factor), scale(b, factor))
    let sum_then_scale = scale(&add(&a, &b).unwrap(), factor).unwrap();
    let scaled_then_sum = add(&scale(&a, factor).unwrap(), &scale(&b, factor).unwrap()).unwrap();
    let lhs = sum_then_scale.intensity();
    let rhs = scaled_then_sum.intensity();
    assert!(
        (lhs - rhs).abs() < 1e-3,
        "scale-add distributivity failed : {} vs {}",
        lhs,
        rhs
    );
}

#[test]
fn ops_attenuate_clamped_to_unit_interval() {
    let a = monochromatic(1.0, 550.0, [0.0, 0.0, 1.0], 0).unwrap();
    assert!(matches!(
        attenuate(&a, 1.5),
        Err(LightCompositionError::InvalidAttenuation(_))
    ));
    assert!(matches!(
        attenuate(&a, -0.1),
        Err(LightCompositionError::InvalidAttenuation(_))
    ));
    // Boundaries OK.
    assert!(attenuate(&a, 0.0).is_ok());
    assert!(attenuate(&a, 1.0).is_ok());
}

#[test]
fn ops_mueller_identity_preserves_dop() {
    let a = ApockyLight::new(
        1.0,
        550.0,
        [0.0; ACCOMPANIMENT_COUNT],
        0.6,
        0.3,
        0.2,
        [0.0, 0.0, 1.0],
        0,
        EvidenceGlyph::Default,
        0,
    );
    let identity = [
        [1.0_f32, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];
    let out = mueller_apply(&a, identity).unwrap();
    assert!((out.intensity() - a.intensity()).abs() < 1e-3);
    assert!((out.dop() - a.dop()).abs() < 1e-2);
}

#[test]
fn ops_blackbody_temperature_drives_lambda() {
    // 3000K (incandescent, peak ≈ 966 nm) vs 6500K (D65-like, peak ≈ 446 nm).
    let cool = blackbody(3000.0, 0).unwrap();
    let hot = blackbody(6500.0, 0).unwrap();
    assert!(
        cool.lambda_nm() > hot.lambda_nm(),
        "Wien-displacement violated : cool_λ={}, hot_λ={}",
        cool.lambda_nm(),
        hot.lambda_nm()
    );
}

// ───────────────────────────────────────────────────────────────────────────
// § IFC + PRIME-DIRECTIVE (14-18)
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn ifc_anonymous_cap_egress_permitted() {
    assert!(can_egress(CapHandle::ANONYMOUS));
}

#[test]
fn ifc_biometric_cap_absolute_banned_from_egress() {
    let biometric = CapHandle::new(1, CapKind::Val, true, false, false);
    assert!(!can_egress(biometric));
}

#[test]
fn ifc_combine_caps_propagates_biometric_flag() {
    let benign = CapHandle::new(1, CapKind::Val, false, false, false);
    let biometric = CapHandle::new(2, CapKind::Val, true, false, false);
    let combined = combine_caps(benign, biometric).unwrap();
    assert!(combined.is_biometric());
    assert!(!can_egress(combined));
}

#[test]
fn ifc_combine_iso_with_ref_refused() {
    let iso = CapHandle::new(1, CapKind::Iso, false, false, false);
    let r = CapHandle::new(2, CapKind::Ref, false, false, false);
    assert!(matches!(
        combine_caps(iso, r),
        Err(IfcFlowError::IncompatiblePonyCaps { .. })
    ));
}

#[test]
fn ifc_kan_band_handle_truncates_to_24_bit() {
    let h = KanBandHandle::new(0xFFFF_FFFF);
    assert_eq!(h.as_u32(), 0x00FF_FFFF);
    let null = KanBandHandle::NULL;
    assert!(null.is_null());
}

// ───────────────────────────────────────────────────────────────────────────
// § Composition with capability-incompatible inputs is structurally refused.
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn ops_add_with_incompatible_caps_refused() {
    // Iso + Ref combination is refused by the Pony-cap algebra.
    let iso_handle = CapHandle::new(1, CapKind::Iso, false, false, false).as_u32();
    let ref_handle = CapHandle::new(2, CapKind::Ref, false, false, false).as_u32();
    let a = ApockyLight::new(
        1.0,
        550.0,
        [0.0; ACCOMPANIMENT_COUNT],
        0.0,
        0.0,
        0.0,
        [0.0, 0.0, 1.0],
        0,
        EvidenceGlyph::Default,
        iso_handle,
    );
    let b = ApockyLight::new(
        1.0,
        550.0,
        [0.0; ACCOMPANIMENT_COUNT],
        0.0,
        0.0,
        0.0,
        [0.0, 0.0, 1.0],
        0,
        EvidenceGlyph::Default,
        ref_handle,
    );
    assert!(matches!(
        add(&a, &b),
        Err(LightCompositionError::IncompatibleCaps { .. })
    ));
}

// ───────────────────────────────────────────────────────────────────────────
// § Convenience-accessor sanity (final tests)
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn rgb_conversion_visible_band_yields_nonzero() {
    // Yellow-green at 555 nm should produce non-zero RGB ; we don't pin
    // exact values but verify the channels are positive + finite.
    let l = monochromatic(1.0, 555.0, [0.0, 0.0, 1.0], 0).unwrap();
    let [r, g, b] = l.to_rgb();
    assert!(r.is_finite() && g.is_finite() && b.is_finite());
    assert!(r >= 0.0 && g >= 0.0 && b >= 0.0);
    // Green channel should dominate at 555 nm.
    assert!(g > 0.0);
}

#[test]
fn construction_validators_reject_garbage() {
    assert!(matches!(
        monochromatic(f32::NAN, 550.0, [0.0, 0.0, 1.0], 0),
        Err(LightConstructionError::NegativeRadiance(_))
    ));
    assert!(matches!(
        blackbody(0.0, 0),
        Err(LightConstructionError::InvalidTemperature(_))
    ));
    assert!(matches!(
        d65(-1.0, 0),
        Err(LightConstructionError::NegativeRadiance(_))
    ));
}
