//! § Integration tests for cssl-substrate-loa-kan
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § COVERAGE
//!   - KAN-activation-shape : ParametricActivation kind dispatch
//!   - per-cell-modulation  : LoaKanCellModulation apply + compose
//!   - ω-read-with-cap      : OmegaField + LoaKanOverlay read-cell with mask
//!   - ω-write-with-cap     : OmegaField + LoaKanOverlay set-cell with mask
//!   - IFC-violation-detected: Σ-mask refusal on cell-touch
//!   - Σ-mask-enforce       : Reconfigure-bit gate on overlay set
//!   - capability-mismatch  : Sovereign-handle mismatch on set
//!   - companion-AI-hook    : default-deny + opt-in registration
//!
//! Per § TESTS contract in T11-D261 W-S12 dispatch.

#![allow(clippy::needless_range_loop)]

use cssl_substrate_loa_kan::{
    ActivationKind, AdaptiveContentScaler, CompanionAiHook, CompanionAiKind, CompanionConsent,
    KanDetailTier, LoaKanCellModulation, LoaKanExtension, LoaKanOverlay, ParametricActivation,
    MODULATION_DIM,
};
use cssl_substrate_omega_field::{MortonKey, OmegaField};
use cssl_substrate_prime_directive::sigma::{ConsentBit, SigmaMaskPacked, SigmaPolicy};

fn permissive_with_sovereign(s: u16) -> SigmaMaskPacked {
    SigmaMaskPacked::default_mask()
        .with_consent(
            ConsentBit::Observe.bits()
                | ConsentBit::Sample.bits()
                | ConsentBit::Modify.bits()
                | ConsentBit::Reconfigure.bits(),
        )
        .with_sovereign(s)
}

// ── 1. KAN-activation-shape ─────────────────────────────────────────────

#[test]
fn kan_activation_shape_dispatch() {
    let kinds = [
        ActivationKind::Identity,
        ActivationKind::Sigmoid,
        ActivationKind::Tanh,
        ActivationKind::Gaussian,
        ActivationKind::RadialBasis,
        ActivationKind::Polynomial,
        ActivationKind::BSplineEdge,
    ];
    for k in &kinds {
        // Each kind has a distinct param_count.
        assert!(k.param_count() <= 16);
        // Canonical name is non-empty.
        assert!(!k.canonical_name().is_empty());
    }
}

// ── 2. per-cell-modulation ───────────────────────────────────────────────

#[test]
fn per_cell_modulation_apply_and_compose() {
    let m_a = LoaKanCellModulation::uniform(2.0, 7).unwrap();
    let m_b = LoaKanCellModulation::uniform(0.5, 7).unwrap();
    let composed = m_a.compose(&m_b).unwrap();
    // 2.0 * 0.5 = 1.0 ⇒ identity-coefficient
    for i in 0..MODULATION_DIM {
        assert_eq!(composed.coeffs[i], 1.0);
    }
    // Apply the composed modulation to a unit vector.
    let mut out = [3.0_f32; MODULATION_DIM];
    composed.apply_to(&mut out);
    for i in 0..MODULATION_DIM {
        assert_eq!(out[i], 3.0); // 3.0 * 1.0 = 3.0
    }
}

// ── 3. ω-read-with-cap (read-with-capability) ───────────────────────────

#[test]
fn omega_read_cell_with_capability() {
    let mut field = OmegaField::new();
    let mut overlay = LoaKanOverlay::new();
    let k = MortonKey::encode(1, 2, 3).unwrap();
    // Sovereign claims the cell with permissive mask.
    let mask = permissive_with_sovereign(42);
    field.set_sigma(k, mask);
    // Stamp an extension via bootstrap (initial scene-load).
    let act = ParametricActivation::sigmoid(1.0, 0.0);
    let modu = LoaKanCellModulation::uniform(2.0, 42).unwrap();
    let ext = LoaKanExtension::new(act, modu).unwrap();
    overlay.stamp_bootstrap(k, ext).unwrap();
    // Read returns the explicit extension.
    let read = overlay.at(k);
    assert_eq!(read.activation.kind, ActivationKind::Sigmoid);
    assert_eq!(read.sovereign_handle(), 42);
    // Sigma-mask is preserved at the field side.
    assert_eq!(field.sigma().at(k).sovereign_handle(), 42);
}

// ── 4. ω-write-with-cap (write-with-capability) ─────────────────────────

#[test]
fn omega_write_cell_with_capability() {
    let mut field = OmegaField::new();
    let mut overlay = LoaKanOverlay::new();
    let k = MortonKey::encode(0, 0, 0).unwrap();
    let mask = permissive_with_sovereign(7);
    field.set_sigma(k, mask);
    let act = ParametricActivation::tanh(1.5, 0.0);
    let modu = LoaKanCellModulation::uniform(0.8, 7).unwrap();
    let ext = LoaKanExtension::new(act, modu).unwrap();
    let prev = overlay.set(k, ext, mask).unwrap();
    assert!(prev.is_none());
    assert_eq!(overlay.cell_count(), 1);
    let read = overlay.at(k);
    assert_eq!(read.sovereign_handle(), 7);
}

// ── 5. IFC-violation-detected ───────────────────────────────────────────

#[test]
fn ifc_violation_blocks_extension_install() {
    // Default-Private mask permits Observe only — extension install must fail.
    let mut overlay = LoaKanOverlay::new();
    let k = MortonKey::encode(2, 2, 2).unwrap();
    let mask = SigmaMaskPacked::default_mask();
    let ext = LoaKanExtension::identity();
    let err = overlay.set(k, ext, mask).unwrap_err();
    // The error must be a Reconfigure-refusal — confirms the IFC pass
    // structurally rejected the cell-touch without consent.
    assert!(format!("{}", err).contains("Reconfigure"));
    assert_eq!(overlay.cell_count(), 0);
}

// ── 6. Σ-mask-enforce ───────────────────────────────────────────────────

#[test]
fn sigma_mask_enforces_reconfigure_consent() {
    let mut overlay = LoaKanOverlay::new();
    let k = MortonKey::encode(3, 3, 3).unwrap();
    // Mask permits Observe + Modify but NOT Reconfigure.
    let mask = SigmaMaskPacked::default_mask()
        .with_consent(ConsentBit::Observe.bits() | ConsentBit::Modify.bits());
    let ext = LoaKanExtension::identity();
    let err = overlay.set(k, ext, mask).unwrap_err();
    assert!(format!("{}", err).contains("Reconfigure"));
    // Now grant Reconfigure → succeeds.
    let mask_with_recon = mask.with_consent(
        ConsentBit::Observe.bits() | ConsentBit::Modify.bits() | ConsentBit::Reconfigure.bits(),
    );
    overlay.set(k, ext, mask_with_recon).unwrap();
    assert_eq!(overlay.cell_count(), 1);
}

// ── 7. capability-mismatch (Sovereign-handle) ──────────────────────────

#[test]
fn capability_mismatch_refuses_set() {
    let mut overlay = LoaKanOverlay::new();
    let k = MortonKey::encode(4, 4, 4).unwrap();
    // Cell claimed by Sovereign 99 ; extension authored by 7.
    let mask = permissive_with_sovereign(99);
    let act = ParametricActivation::sigmoid(1.0, 0.0);
    let modu = LoaKanCellModulation::uniform(2.0, 7).unwrap();
    let ext = LoaKanExtension::new(act, modu).unwrap();
    let err = overlay.set(k, ext, mask).unwrap_err();
    assert!(format!("{}", err).contains("Sovereign-handle mismatch"));
}

// ── 8. companion-AI-hook ────────────────────────────────────────────────

#[test]
fn companion_ai_hook_default_deny_then_opt_in() {
    // Default state : no companion ⇒ inactive hook.
    let none = CompanionAiHook::none();
    assert!(!none.is_active());

    // Default-deny : Refused consent must reject registration.
    let err = CompanionAiHook::register(
        CompanionAiKind::Creature,
        CompanionConsent::Refused,
        42,
        7,
        0,
    )
    .unwrap_err();
    assert!(format!("{}", err).contains("refused"));

    // Opt-in via Sovereign-granted consent : hook is active.
    let h = CompanionAiHook::register(
        CompanionAiKind::Creature,
        CompanionConsent::Granted,
        42,
        7,
        0,
    )
    .unwrap();
    assert!(h.is_active());
    assert_eq!(h.sovereign_handle, 42);
    assert!(!h.requires_audit());

    // Mutual-witness consent : hook is active AND requires audit.
    let h_witness = CompanionAiHook::register(
        CompanionAiKind::Witness,
        CompanionConsent::MutualWitness,
        99,
        12,
        0,
    )
    .unwrap();
    assert!(h_witness.is_active());
    assert!(h_witness.requires_audit());
}

// ── 9. extra : adaptive-scaler with foveal budget ──────────────────────

#[test]
fn adaptive_scaler_respects_kan_budget() {
    // Foveal tier permits up to 1.0 ; peripheral caps at 0.25.
    let foveal = AdaptiveContentScaler::new(KanDetailTier::Foveal, 7, 1.0);
    let peripheral = AdaptiveContentScaler::new(KanDetailTier::Peripheral, 7, 1.0);
    assert_eq!(foveal.effective_scale(), 1.0);
    assert_eq!(peripheral.effective_scale(), 0.25);
    assert!(!foveal.is_clipped());
    assert!(peripheral.is_clipped());
    // Without Sample consent the scaler emits identity-modulation regardless.
    let mask_no_sample = SigmaMaskPacked::default_mask();
    assert!(!mask_no_sample.can_sample());
    let m = foveal.derive(mask_no_sample).unwrap();
    assert!(m.is_identity());
    // With Sample consent the scaler emits a tier-clamped uniform modulation.
    let mask_sample = SigmaMaskPacked::from_policy(SigmaPolicy::PublicRead);
    let m = foveal.derive(mask_sample).unwrap();
    assert!(m.active);
    for i in 0..MODULATION_DIM {
        assert_eq!(m.coeffs[i], 1.0);
    }
}

// ── 10. extra : extension validates internal coherence ─────────────────

#[test]
fn extension_validates_against_corruption() {
    let act = ParametricActivation::sigmoid(1.0, 0.0);
    let modu = LoaKanCellModulation::uniform(2.0, 7).unwrap();
    let ext = LoaKanExtension::new(act, modu).unwrap();
    ext.validate().unwrap();
    // Identity-extension with non-zero Sovereign is incoherent.
    let mut bad_modu = LoaKanCellModulation::identity();
    bad_modu.sovereign_handle = 42;
    let err = LoaKanExtension::new(ParametricActivation::identity(), bad_modu).unwrap_err();
    assert!(format!("{}", err).contains("identity extension"));
}

// ── 11. extra : modulation compose with mismatched Sovereign refused ──

#[test]
fn modulation_compose_mismatched_sovereigns_refused() {
    let m_a = LoaKanCellModulation::uniform(2.0, 5).unwrap();
    let m_b = LoaKanCellModulation::uniform(0.5, 6).unwrap();
    let err = m_a.compose(&m_b).unwrap_err();
    assert!(format!("{}", err).contains("Sovereign-handle mismatch"));
}

// ── 12. extra : full extension evaluate path ───────────────────────────

#[test]
fn extension_evaluate_full_path() {
    let act = ParametricActivation::sigmoid(2.0, -1.0);
    let modu = LoaKanCellModulation::uniform(1.5, 7).unwrap();
    let ext = LoaKanExtension::new(act, modu).unwrap();
    let mut out = [0.0_f32; 8];
    ext.evaluate(0.5, &mut out);
    // σ(2*0.5 - 1) = σ(0) = 0.5 ; * 1.5 = 0.75
    for i in 0..8 {
        assert!((out[i] - 0.75).abs() < 1e-3);
    }
}
