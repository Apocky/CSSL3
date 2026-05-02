// § cosmetic_dps_parity.rs — exhaustive proof of cosmetic-only-axiom
// ════════════════════════════════════════════════════════════════════
// § Required by W13-2 brief : "DPS-parity-across-skins · cosmetic-only-axiom".
//   For every (kind, tier) pair, swapping cosmetic-skin MUST preserve :
//     • dps_signature()
//     • base_dps()
//     • per_shot()
//     • compute_damage() output for matched mechanic-inputs
// ════════════════════════════════════════════════════════════════════

use cssl_host_weapons::{
    compute_damage, ArmorClass, DamageType, HitZone, WeaponBuild, WeaponCosmetic, WeaponKind,
    WeaponTier,
};

fn cosmetic_palette() -> [WeaponCosmetic; 5] {
    [
        WeaponCosmetic::DEFAULT,
        WeaponCosmetic::gold_tracer(),
        WeaponCosmetic::neon_blue(),
        WeaponCosmetic {
            tracer_rgb: 0xFF_00_FF,
            muzzle_flash_id: 99,
            impact_particle_id: 99,
            fire_sound_id: 99,
            idle_anim_id: 99,
            skin_id: 99,
        },
        WeaponCosmetic {
            tracer_rgb: 0x00_FF_00,
            muzzle_flash_id: 7,
            impact_particle_id: 13,
            fire_sound_id: 21,
            idle_anim_id: 34,
            skin_id: 55,
        },
    ]
}

#[test]
fn dps_signature_invariant_for_every_kind_tier_cosmetic() {
    let cosmetics = cosmetic_palette();
    for k in WeaponKind::all() {
        for t in WeaponTier::ALL {
            let baseline = WeaponBuild::new(k, t, cosmetics[0]).dps_signature();
            for c in cosmetics {
                let sig = WeaponBuild::new(k, t, c).dps_signature();
                assert_eq!(sig, baseline, "DPS-signature LEAK at {k:?}/{t:?} cosmetic={c:?}");
            }
        }
    }
}

#[test]
fn base_dps_invariant_under_cosmetic_swap() {
    let cosmetics = cosmetic_palette();
    for k in WeaponKind::all() {
        for t in WeaponTier::ALL {
            let baseline = WeaponBuild::new(k, t, cosmetics[0]).base_dps();
            for c in cosmetics {
                let d = WeaponBuild::new(k, t, c).base_dps();
                assert_eq!(d.to_bits(), baseline.to_bits(), "DPS LEAK at {k:?}/{t:?}");
            }
        }
    }
}

#[test]
fn per_shot_invariant_under_cosmetic_swap() {
    let cosmetics = cosmetic_palette();
    for k in WeaponKind::all() {
        for t in WeaponTier::ALL {
            let baseline = WeaponBuild::new(k, t, cosmetics[0]).per_shot();
            for c in cosmetics {
                let d = WeaponBuild::new(k, t, c).per_shot();
                assert_eq!(d.to_bits(), baseline.to_bits(), "per-shot LEAK at {k:?}/{t:?}");
            }
        }
    }
}

#[test]
fn compute_damage_invariant_for_matched_mechanic_inputs() {
    // Same per-shot ⇒ same final damage, regardless of cosmetic.
    let cosmetics = cosmetic_palette();
    for k in WeaponKind::all() {
        for t in WeaponTier::ALL {
            let baseline = WeaponBuild::new(k, t, cosmetics[0]).per_shot();
            for c in cosmetics {
                let per_shot = WeaponBuild::new(k, t, c).per_shot();
                let a = compute_damage(per_shot, HitZone::Body, DamageType::Kinetic, ArmorClass::Plate, false);
                let b = compute_damage(baseline, HitZone::Body, DamageType::Kinetic, ArmorClass::Plate, false);
                assert_eq!(a.final_dmg.to_bits(), b.final_dmg.to_bits());
            }
        }
    }
}
