// § integration.rs — top-level integration coverage for W13-6.
// ════════════════════════════════════════════════════════════════════
// § I> Cross-module scenarios : intent → genre → aug → state-machine.
// § I> Genre-shift round-trip · cosmetic-only-axiom · pay-for-power N!
// ════════════════════════════════════════════════════════════════════

#![allow(clippy::many_single_char_names)] // `a, i, f, r, m` are conventional in tests
#![allow(clippy::float_cmp)]

use cssl_host_movement_aug::{
    BoostAffix, BoostSkinId, CameraGenre, GenreTranslator, LocomotionPhase, MovementAug,
    MovementIntent, MovementParams, ProposedMotion, StaminaPolicy, WorldHints,
};

fn fwd_xz() -> ([f32; 2], [f32; 2]) {
    ([0.0, -1.0], [1.0, 0.0])
}

#[test]
fn full_apex_loop_sprint_slide_jump_works() {
    let mut a = MovementAug::default();
    let (f, r) = fwd_xz();
    let mut i = MovementIntent {
        forward: 1.0,
        sprint_held: true,
        ..Default::default()
    };
    // Sprint for 0.5s.
    for _ in 0..50 {
        let m = a.tick(&i, f, r, 0.01, &WorldHints::ground());
        assert!(m.boost_emit || a.state.phase == LocomotionPhase::Sprinting);
    }
    // Engage slide.
    i.crouch_held = true;
    let m = a.tick(&i, f, r, 0.05, &WorldHints::ground());
    assert_eq!(a.state.phase, LocomotionPhase::Sliding);
    assert!(m.boost_emit, "slide-entry should emit boost");
    // Mid-slide jump.
    i.crouch_held = false;
    i.jump_pressed = true;
    a.tick(&i, f, r, 0.016, &WorldHints::ground());
    // Should now be airborne with positive vy.
    assert!(a.state.vy > 0.0);
}

#[test]
fn genre_shift_roundtrip_iso_back_to_fps_preserves_mechanics() {
    let mut a_fps = MovementAug::default();
    let mut a_iso = MovementAug::default();
    let (f, r) = fwd_xz();
    let mut i = MovementIntent {
        forward: 1.0,
        sprint_held: true,
        ..Default::default()
    };

    // Same input under FPS.
    let trans_fps = GenreTranslator::new(CameraGenre::Fps);
    let i_fps = trans_fps.translate(&i);
    let m_fps = a_fps.tick(&i_fps, f, r, 0.1, &WorldHints::ground());

    // Same input under Iso (snapped to grid : 1.0 forward, 0 right ; same
    // vector when forward=1, right=0 already).
    let trans_iso = GenreTranslator::new(CameraGenre::Iso);
    let i_iso = trans_iso.translate(&i);
    let m_iso = a_iso.tick(&i_iso, f, r, 0.1, &WorldHints::ground());

    // For pure-forward input (1, 0) the snap returns (1, 0) — delta should match.
    assert!((m_fps.delta[2] - m_iso.delta[2]).abs() < 1e-4);
    assert_eq!(a_fps.state.phase, a_iso.state.phase);

    // Now take a fractional input ; iso snaps differently.
    i.forward = 0.7;
    i.right = 0.3;
    let i_iso2 = trans_iso.translate(&i);
    assert!((i_iso2.forward - 1.0).abs() < 1e-6);
    assert!((i_iso2.right - 0.0).abs() < 1e-6);

    // Round-trip back to FPS preserves stamina-budget semantics.
    let s_before = a_iso.state.stamina;
    let trans_back = GenreTranslator::new(CameraGenre::Fps);
    let i_back = trans_back.translate(&i);
    a_iso.tick(&i_back, f, r, 0.1, &WorldHints::ground());
    // Stamina should monotonically change ; not jump.
    assert!((a_iso.state.stamina - s_before).abs() < 0.05);
}

#[test]
fn no_pay_for_power_skin_swap_yields_identical_distance() {
    // Apocky's promise : ANY skin-swap MUST NOT alter distance traveled.
    let mut a = MovementAug::default();
    let (f, r) = fwd_xz();
    let mut i = MovementIntent {
        forward: 1.0,
        sprint_held: true,
        ..Default::default()
    };

    let mut total_x = 0.0;
    let mut total_z = 0.0;
    let skins = [
        BoostAffix::baseline(),
        BoostAffix {
            skin: BoostSkinId(1),
            trail_hue: 1.5,
            audio_pack_id: 4,
            vfx_density: 1.8,
            slide_spark_rgb: [255, 0, 0],
            emit_wall_run_particles: false,
        },
        BoostAffix {
            skin: BoostSkinId(2),
            trail_hue: 3.0,
            audio_pack_id: 9,
            vfx_density: 0.5,
            slide_spark_rgb: [0, 255, 0],
            emit_wall_run_particles: true,
        },
    ];

    // The crucial pattern : `MovementAug::tick` does NOT accept any skin. The
    // skin loop is purely render-channel ; the engine integrates without any
    // skin reference.
    for skin in &skins {
        let _ = skin.validate();
        let m: ProposedMotion = a.tick(&i, f, r, 0.05, &WorldHints::ground());
        total_x += m.delta[0];
        total_z += m.delta[2];
    }
    // 3 ticks of 0.05s sprinting forward = 3 * 5.0 * 1.6 * 0.05 = 1.2 along -Z.
    assert!((total_z - (-1.2)).abs() < 0.05, "got {total_z}");
    assert!(total_x.abs() < 0.05);
    // Suppress "unused mut" if reorder happens.
    i.sprint_held = false;
}

#[test]
fn movement_params_are_canonical_published_constants() {
    let p = MovementParams::CANONICAL;
    let d = MovementParams::default();
    assert_eq!(p, d);
    // Cross-check the spec values.
    assert!((p.sprint_mult - 1.6).abs() < 1e-6);
    assert_eq!(p.max_jumps_in_air, 2);
    assert!((p.wall_run_max_secs - 2.0).abs() < 1e-6);
    assert!((p.air_control - 0.30).abs() < 1e-6);
}

#[test]
fn sovereign_stamina_policy_doesnt_cap_speed() {
    let mut a = MovementAug::default();
    a.set_stamina_policy(StaminaPolicy::Sovereign);
    let (f, r) = fwd_xz();
    let mut i = MovementIntent {
        forward: 1.0,
        sprint_held: true,
        ..Default::default()
    };
    // After 10 seconds of sprint, stamina is still full.
    for _ in 0..1000 {
        a.tick(&i, f, r, 0.01, &WorldHints::ground());
    }
    assert!(a.state.stamina > 0.99);
    let m = a.tick(&i, f, r, 0.01, &WorldHints::ground());
    assert!((m.speed_mult - 1.6).abs() < 1e-3);
    i.sprint_held = false; // just to silence dead-store
}
