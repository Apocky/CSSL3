// § projectile.rs — spline-trajectory + per-frame collision-sweep
// ════════════════════════════════════════════════════════════════════
// § I> Spline-trajectory : pos(t) = pos₀ + v·t + 0.5·(g + wind - drag·v)·t²
//      Discretized via per-frame Verlet step ; deterministic.
// § I> Collision-sweep per-step : continuous sphere-vs-sphere across the
//      sub-step ray to avoid tunneling at high velocity.
// § I> NaN-safe ; saturating ; pure-fn (state mutated via `&mut`).
// ════════════════════════════════════════════════════════════════════

use serde::{Deserialize, Serialize};

use crate::damage::DamageType;
use crate::hitscan::{Vec3, HitscanTarget};

/// Live projectile entry in the ring-pool.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Projectile {
    pub id: u64,
    pub pos: Vec3,
    pub vel: Vec3,
    pub radius: f32,
    /// Time-to-live in seconds ; saturates to 0 (despawn).
    pub ttl_secs: f32,
    pub damage: f32,
    pub damage_type: DamageType,
    pub alive: bool,
}

impl Projectile {
    pub const DEAD: Self = Self {
        id: 0,
        pos: [0.0; 3],
        vel: [0.0; 3],
        radius: 0.0,
        ttl_secs: 0.0,
        damage: 0.0,
        damage_type: DamageType::Kinetic,
        alive: false,
    };
}

/// Spline-trajectory environment.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TrajectoryEnv {
    pub gravity: Vec3,
    pub wind: Vec3,
    /// Drag coefficient (1/s) ; v ← v · (1 - drag·dt).
    pub drag: f32,
}

impl TrajectoryEnv {
    pub const EARTHLIKE: Self = Self {
        gravity: [0.0, -9.81, 0.0],
        wind: [0.0, 0.0, 0.0],
        drag: 0.05,
    };
    pub const VACUUM: Self = Self {
        gravity: [0.0, 0.0, 0.0],
        wind: [0.0, 0.0, 0.0],
        drag: 0.0,
    };
}

/// One-step collision result.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ProjectileImpact {
    pub projectile_id: u64,
    pub target_id: u64,
    pub impact_pos: Vec3,
    pub damage: f32,
    pub damage_type: DamageType,
}

/// Advance one projectile by `dt_secs` against the trajectory environment.
/// Returns true if the projectile remains alive after the step.
pub fn step_projectile(p: &mut Projectile, env: TrajectoryEnv, dt_secs: f32) -> bool {
    if !p.alive {
        return false;
    }
    let dt = if dt_secs.is_finite() && dt_secs >= 0.0 {
        dt_secs
    } else {
        0.0
    };
    // velocity update : v += (g + wind - drag·v) · dt
    let drag_factor = (1.0 - env.drag * dt).clamp(0.0, 1.0);
    p.vel[0] = p.vel[0] * drag_factor + (env.gravity[0] + env.wind[0]) * dt;
    p.vel[1] = p.vel[1] * drag_factor + (env.gravity[1] + env.wind[1]) * dt;
    p.vel[2] = p.vel[2] * drag_factor + (env.gravity[2] + env.wind[2]) * dt;
    // position step
    p.pos[0] += p.vel[0] * dt;
    p.pos[1] += p.vel[1] * dt;
    p.pos[2] += p.vel[2] * dt;
    // ttl
    p.ttl_secs = (p.ttl_secs - dt).max(0.0);
    if p.ttl_secs <= 0.0 {
        p.alive = false;
    }
    p.alive
}

/// Continuous sphere-vs-sphere collision sweep for one projectile-step.
/// Tests the line-segment from `start_pos` → `end_pos` (both inclusive)
/// against all sphere-targets ; returns earliest hit (or None).
#[must_use]
pub fn sweep_collision(
    start_pos: Vec3,
    end_pos: Vec3,
    proj_radius: f32,
    targets: &[HitscanTarget],
) -> Option<(usize, Vec3, f32)> {
    let dx = end_pos[0] - start_pos[0];
    let dy = end_pos[1] - start_pos[1];
    let dz = end_pos[2] - start_pos[2];
    let seg_len_sq = dx * dx + dy * dy + dz * dz;
    if !seg_len_sq.is_finite() {
        return None;
    }

    let mut best: Option<(usize, Vec3, f32)> = None;
    let mut best_t = f32::INFINITY;

    for (i, t) in targets.iter().enumerate() {
        let ox = start_pos[0] - t.center[0];
        let oy = start_pos[1] - t.center[1];
        let oz = start_pos[2] - t.center[2];
        let total_r = proj_radius + t.radius;
        let a = seg_len_sq;
        let b = 2.0 * (ox * dx + oy * dy + oz * dz);
        let c = ox * ox + oy * oy + oz * oz - total_r * total_r;
        if a == 0.0 {
            if c <= 0.0 {
                // start already inside
                if 0.0 < best_t {
                    best_t = 0.0;
                    best = Some((i, start_pos, 0.0));
                }
            }
            continue;
        }
        let disc = b * b - 4.0 * a * c;
        if !disc.is_finite() || disc < 0.0 {
            continue;
        }
        let sq = disc.sqrt();
        let t0 = (-b - sq) / (2.0 * a);
        let t_hit = if (0.0..=1.0).contains(&t0) {
            Some(t0)
        } else {
            let t1 = (-b + sq) / (2.0 * a);
            if (0.0..=1.0).contains(&t1) {
                Some(t1)
            } else {
                None
            }
        };
        if let Some(th) = t_hit {
            if th < best_t {
                best_t = th;
                let impact = [
                    start_pos[0] + dx * th,
                    start_pos[1] + dy * th,
                    start_pos[2] + dz * th,
                ];
                let dist = (seg_len_sq * th * th).sqrt();
                best = Some((i, impact, dist));
            }
        }
    }

    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::damage::ArmorClass;

    fn target(id: u64, x: f32) -> HitscanTarget {
        HitscanTarget {
            id,
            center: [x, 0.0, 0.0],
            radius: 0.5,
            armor: ArmorClass::Unarmored,
            is_head: false,
            is_weak: false,
        }
    }

    #[test]
    fn projectile_falls_under_gravity() {
        let mut p = Projectile {
            id: 1, pos: [0.0; 3], vel: [10.0, 0.0, 0.0],
            radius: 0.05, ttl_secs: 5.0, damage: 50.0, damage_type: DamageType::Kinetic, alive: true,
        };
        for _ in 0..60 {
            step_projectile(&mut p, TrajectoryEnv::EARTHLIKE, 1.0 / 60.0);
        }
        assert!(p.pos[1] < 0.0); // fell
        assert!(p.pos[0] > 0.0); // moved forward
    }

    #[test]
    fn projectile_dies_after_ttl() {
        let mut p = Projectile {
            id: 1, pos: [0.0; 3], vel: [10.0, 0.0, 0.0],
            radius: 0.05, ttl_secs: 0.5, damage: 50.0, damage_type: DamageType::Kinetic, alive: true,
        };
        for _ in 0..60 {
            step_projectile(&mut p, TrajectoryEnv::VACUUM, 1.0 / 60.0);
        }
        assert!(!p.alive);
    }

    #[test]
    fn sweep_finds_first_hit() {
        let ts = [target(1, 5.0), target(2, 3.0)];
        let hit = sweep_collision([0.0; 3], [10.0, 0.0, 0.0], 0.05, &ts);
        let (idx, _, _) = hit.expect("hit expected");
        assert_eq!(idx, 1); // target at x=3 is closer
    }

    #[test]
    fn sweep_misses_when_offset() {
        let ts = [target(1, 5.0)];
        let hit = sweep_collision([0.0, 5.0, 0.0], [10.0, 5.0, 0.0], 0.05, &ts);
        assert!(hit.is_none());
    }
}
