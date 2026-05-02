// § recoil.rs — pattern emitted as event-stream (W13-5 ADS-recoil consumes)
// ════════════════════════════════════════════════════════════════════
// § I> Per W13-2 brief : "Recoil-pattern emitted as event-stream
//      (W13-5 consumes)". This crate does NOT render or apply screen-space
//      kick — that's the W13-5 ADS-recoil module's job. We expose pure-data
//      events ; the consumer integrates them into camera/UI.
// § I> Pattern table : per-WeaponKind sequence of (yaw_rad, pitch_rad)
//      offsets ; deterministic + repeatable ; recovers via recoil-decay.
// § I> NaN-safe ; ring-tolerant ; no allocs in hot-path (event push uses
//      caller-provided ring).
// ════════════════════════════════════════════════════════════════════

use serde::{Deserialize, Serialize};

use crate::weapon_kind::WeaponKind;

/// One recoil-event emitted at the moment of fire.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RecoilEvent {
    pub weapon_kind: u32,
    /// Step index in the fire-sequence (0 = first shot ; resets after recovery).
    pub shot_index: u32,
    /// Yaw kick (radians, signed ; +ve = right).
    pub yaw_rad: f32,
    /// Pitch kick (radians, signed ; +ve = up).
    pub pitch_rad: f32,
    /// Suggested decay-half-life for the consumer (seconds).
    pub decay_half_life_secs: f32,
}

/// Deterministic per-(kind, shot-index) recoil offset.
///
/// Pattern : pitch grows then plateaus ; yaw alternates left/right with
/// damped amplitude. Cosmetic-skin-agnostic by construction.
#[must_use]
pub fn recoil_for(kind: WeaponKind, shot_index: u32) -> RecoilEvent {
    let i = shot_index as f32;
    let (pitch_base, yaw_amp, half_life) = match kind {
        WeaponKind::Pistol           => (0.010, 0.005, 0.20),
        WeaponKind::Rifle            => (0.015, 0.008, 0.18),
        WeaponKind::ShotgunSpread    => (0.040, 0.015, 0.40),
        WeaponKind::ShotgunSlug      => (0.045, 0.012, 0.45),
        WeaponKind::SniperHitscan    => (0.060, 0.005, 0.60),
        WeaponKind::SniperProjectile => (0.060, 0.005, 0.60),
        WeaponKind::Smg              => (0.012, 0.012, 0.16),
        WeaponKind::Lmg              => (0.020, 0.020, 0.22),
        WeaponKind::Bow              => (0.025, 0.000, 0.30),
        WeaponKind::Crossbow         => (0.025, 0.000, 0.30),
        WeaponKind::LaserBeam        => (0.002, 0.001, 0.10),
        WeaponKind::PlasmaArc        => (0.005, 0.003, 0.12),
        WeaponKind::Grenade          => (0.030, 0.000, 0.50),
        WeaponKind::Explosive        => (0.080, 0.020, 0.80),
        WeaponKind::Melee            => (0.000, 0.000, 0.10),
        WeaponKind::Throwable        => (0.020, 0.000, 0.30),
    };
    // Pitch grows ∝ √index + plateau (asymptote to 8x base).
    let pitch_rad = pitch_base * (1.0 + i.sqrt()).min(8.0);
    // Yaw alternates : sign by parity ; amplitude lightly damped.
    let sign = if (shot_index & 1) == 0 { 1.0 } else { -1.0 };
    let damp = 1.0 / (1.0 + i * 0.05);
    let yaw_rad = sign * yaw_amp * damp;
    RecoilEvent {
        weapon_kind: kind.as_u32(),
        shot_index,
        yaw_rad,
        pitch_rad,
        decay_half_life_secs: half_life,
    }
}

/// Push an event into a caller-provided ring-buffer ; oldest-overwrite.
pub fn push_event(ring: &mut [RecoilEvent], head: &mut usize, ev: RecoilEvent) {
    if ring.is_empty() {
        return;
    }
    ring[*head % ring.len()] = ev;
    *head = head.wrapping_add(1);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pattern_deterministic_for_same_index() {
        let a = recoil_for(WeaponKind::Rifle, 5);
        let b = recoil_for(WeaponKind::Rifle, 5);
        assert_eq!(a, b);
    }

    #[test]
    fn pitch_grows_with_shot_index() {
        let a = recoil_for(WeaponKind::Rifle, 0);
        let b = recoil_for(WeaponKind::Rifle, 5);
        assert!(b.pitch_rad >= a.pitch_rad);
    }

    #[test]
    fn melee_no_yaw_no_pitch() {
        let r = recoil_for(WeaponKind::Melee, 5);
        assert!(r.pitch_rad.abs() < f32::EPSILON);
        assert!(r.yaw_rad.abs() < f32::EPSILON);
    }

    #[test]
    fn ring_push_and_overwrite() {
        let mut ring = [recoil_for(WeaponKind::Pistol, 0); 4];
        let mut head = 0usize;
        for i in 0..10 {
            push_event(&mut ring, &mut head, recoil_for(WeaponKind::Pistol, i));
        }
        assert_eq!(head, 10);
        // ring[0] should be the most recent overwriter (shot 8 mod 4 = 0)
        assert_eq!(ring[0 % 4].shot_index, 8);
    }
}
