// § pool.rs — projectile ring-pool (pre-alloc · max-256-active)
// ════════════════════════════════════════════════════════════════════
// § I> Per W13-2 brief : "Spawned-projectiles ring-pool (pre-alloc ·
//      max 256 active)". Implementation : fixed-size array, free-list
//      via index-stack. O(1) spawn/despawn ; zero heap-alloc per shot.
// § I> Sawyer-pattern : pre-alloc + index-types + reuse > GC pressure.
// § I> Step-all advances every live projectile in stable order.
// ════════════════════════════════════════════════════════════════════

use crate::damage::DamageType;
use crate::projectile::{step_projectile, sweep_collision, Projectile, ProjectileImpact, TrajectoryEnv};
use crate::hitscan::{HitscanTarget, Vec3};

/// Maximum simultaneously-alive projectiles.
pub const MAX_PROJECTILES: usize = 256;

/// Fixed-capacity projectile ring-pool with free-list reuse.
///
/// Note · NO `Serialize/Deserialize` derive — `[T; 256]` does not auto-impl
/// serde without `serde-big-array`. Pool state is per-frame transient ;
/// callers serialize via `live_iter()` if checkpointing is required.
#[derive(Debug, Clone)]
pub struct ProjectilePool {
    slots: [Projectile; MAX_PROJECTILES],
    /// Free-slot indices ; LIFO-stack semantics.
    free_stack: [u16; MAX_PROJECTILES],
    /// Number of valid entries in `free_stack`.
    free_top: u16,
    /// Monotonic id-allocator.
    next_id: u64,
}

impl Default for ProjectilePool {
    fn default() -> Self {
        Self::new()
    }
}

impl ProjectilePool {
    /// Construct empty pool with all slots free.
    #[must_use]
    pub fn new() -> Self {
        let mut free_stack = [0u16; MAX_PROJECTILES];
        // Pre-fill stack so pop yields slot 0 first, then 1, etc.
        // Reversed : top = MAX-1 means free_stack[MAX-1] = 0 (popped first).
        let mut i = 0u16;
        while (i as usize) < MAX_PROJECTILES {
            free_stack[i as usize] = (MAX_PROJECTILES as u16) - 1 - i;
            i += 1;
        }
        Self {
            slots: [Projectile::DEAD; MAX_PROJECTILES],
            free_stack,
            free_top: MAX_PROJECTILES as u16,
            next_id: 1,
        }
    }

    /// Returns count of currently-alive projectiles.
    #[must_use]
    pub fn live_count(&self) -> usize {
        MAX_PROJECTILES - (self.free_top as usize)
    }

    /// Returns capacity.
    #[must_use]
    pub const fn capacity(&self) -> usize {
        MAX_PROJECTILES
    }

    /// Try to spawn a projectile. Returns Some(id) on success ; None if pool full.
    pub fn spawn(
        &mut self,
        pos: Vec3,
        vel: Vec3,
        radius: f32,
        ttl_secs: f32,
        damage: f32,
        damage_type: DamageType,
    ) -> Option<u64> {
        if self.free_top == 0 {
            return None;
        }
        self.free_top -= 1;
        let slot_idx = self.free_stack[self.free_top as usize] as usize;
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        self.slots[slot_idx] = Projectile {
            id,
            pos,
            vel,
            radius,
            ttl_secs,
            damage,
            damage_type,
            alive: true,
        };
        Some(id)
    }

    /// Step every alive projectile by `dt_secs` and run collision-sweep
    /// against `targets`. Despawn on TTL expiry OR on impact. Append impact
    /// records to `out_impacts` (caller-provided, fixed buffer, returns N).
    pub fn step_all(
        &mut self,
        env: TrajectoryEnv,
        dt_secs: f32,
        targets: &[HitscanTarget],
        out_impacts: &mut [ProjectileImpact],
    ) -> usize {
        let mut written = 0usize;
        for slot_idx in 0..MAX_PROJECTILES {
            if !self.slots[slot_idx].alive {
                continue;
            }
            let start = self.slots[slot_idx].pos;
            let was_alive = step_projectile(&mut self.slots[slot_idx], env, dt_secs);
            let end = self.slots[slot_idx].pos;
            // Sweep using the just-traversed segment.
            let radius = self.slots[slot_idx].radius;
            if let Some((tgt_idx, impact, _dist)) = sweep_collision(start, end, radius, targets) {
                if written < out_impacts.len() {
                    out_impacts[written] = ProjectileImpact {
                        projectile_id: self.slots[slot_idx].id,
                        target_id: targets[tgt_idx].id,
                        impact_pos: impact,
                        damage: self.slots[slot_idx].damage,
                        damage_type: self.slots[slot_idx].damage_type,
                    };
                    written += 1;
                }
                // Despawn on impact.
                self.recycle_slot(slot_idx);
            } else if !was_alive {
                self.recycle_slot(slot_idx);
            }
        }
        written
    }

    fn recycle_slot(&mut self, slot_idx: usize) {
        if !self.slots[slot_idx].alive && self.slots[slot_idx].id == 0 {
            return; // already recycled (sentinel)
        }
        self.slots[slot_idx] = Projectile::DEAD;
        if (self.free_top as usize) < MAX_PROJECTILES {
            self.free_stack[self.free_top as usize] = slot_idx as u16;
            self.free_top += 1;
        }
    }

    /// Force-clear all slots (used at scene-transition).
    pub fn clear(&mut self) {
        *self = Self::new();
    }

    /// Iterate live slots (immutable) — for HUD / serialization.
    pub fn live_iter(&self) -> impl Iterator<Item = &Projectile> {
        self.slots.iter().filter(|p| p.alive)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capacity_is_256() {
        let pool = ProjectilePool::new();
        assert_eq!(pool.capacity(), 256);
    }

    #[test]
    fn spawn_then_count() {
        let mut pool = ProjectilePool::new();
        for _ in 0..10 {
            pool.spawn([0.0; 3], [1.0, 0.0, 0.0], 0.05, 5.0, 50.0, DamageType::Kinetic);
        }
        assert_eq!(pool.live_count(), 10);
    }

    #[test]
    fn pool_full_returns_none() {
        let mut pool = ProjectilePool::new();
        for _ in 0..MAX_PROJECTILES {
            assert!(pool.spawn([0.0; 3], [1.0, 0.0, 0.0], 0.05, 5.0, 50.0, DamageType::Kinetic).is_some());
        }
        assert!(pool.spawn([0.0; 3], [1.0, 0.0, 0.0], 0.05, 5.0, 50.0, DamageType::Kinetic).is_none());
    }

    #[test]
    fn pool_recycles_after_ttl_expiry() {
        let mut pool = ProjectilePool::new();
        // Spawn 100 ; let them all expire ; spawn 100 more ; pool live should be 100 after.
        for _ in 0..100 {
            pool.spawn([0.0; 3], [1.0, 0.0, 0.0], 0.05, 0.001, 50.0, DamageType::Kinetic);
        }
        assert_eq!(pool.live_count(), 100);
        let mut impacts = [ProjectileImpact {
            projectile_id: 0, target_id: 0, impact_pos: [0.0; 3], damage: 0.0, damage_type: DamageType::Kinetic,
        }; 0];
        // Step 1s : ttl = 0.001 expires immediately on first step.
        pool.step_all(TrajectoryEnv::VACUUM, 0.1, &[], &mut impacts);
        assert_eq!(pool.live_count(), 0);
        for _ in 0..100 {
            assert!(pool.spawn([0.0; 3], [1.0, 0.0, 0.0], 0.05, 5.0, 50.0, DamageType::Kinetic).is_some());
        }
        assert_eq!(pool.live_count(), 100);
    }

    #[test]
    fn sweep_records_impact_and_despawns() {
        use crate::damage::ArmorClass;
        let mut pool = ProjectilePool::new();
        pool.spawn([0.0; 3], [10.0, 0.0, 0.0], 0.05, 5.0, 50.0, DamageType::Kinetic);
        let targets = [HitscanTarget {
            id: 7, center: [3.0, 0.0, 0.0], radius: 0.5,
            armor: ArmorClass::Unarmored, is_head: false, is_weak: false,
        }];
        let mut impacts = [ProjectileImpact {
            projectile_id: 0, target_id: 0, impact_pos: [0.0; 3], damage: 0.0, damage_type: DamageType::Kinetic,
        }; 4];
        let n = pool.step_all(TrajectoryEnv::VACUUM, 1.0, &targets, &mut impacts);
        assert_eq!(n, 1);
        assert_eq!(impacts[0].target_id, 7);
        assert_eq!(pool.live_count(), 0);
    }
}
