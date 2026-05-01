// § mana.rs — mana-pool : capacity + current + regen + channel-bool
// ════════════════════════════════════════════════════════════════════
// § I> per GDD § MANA-ECONOMY § REGEN
//   idle       : 2 / sec   (default)
//   channeling : 0 / sec   (no-regen mid-channel)
// § I> saturating semantics : current never exceeds capacity ; never < 0.
// § I> tactical-resource invariant : ALWAYS-bound ¬ infinite-cast.
// ════════════════════════════════════════════════════════════════════

use serde::{Deserialize, Serialize};

/// Mana-pool with saturating arithmetic.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ManaPool {
    pub capacity: f32,
    pub current: f32,
    pub regen_per_sec: f32,
    pub channeling: bool,
}

impl ManaPool {
    /// New full-capacity pool with default regen (2/sec) and no channel.
    #[must_use]
    pub fn new(capacity: f32) -> Self {
        Self {
            capacity,
            current: capacity,
            regen_per_sec: 2.0,
            channeling: false,
        }
    }

    /// Advance the pool by `dt` seconds. No regen while channeling.
    /// Saturates at capacity ; never exceeds.
    pub fn tick(&mut self, dt: f32) {
        if self.channeling {
            return;
        }
        let next = self.regen_per_sec.mul_add(dt, self.current);
        self.current = next.clamp(0.0, self.capacity);
    }

    /// Try to consume `cost` mana ; returns true on success.
    /// Saturating-on-fail : leaves current unchanged on insufficient.
    pub fn try_consume(&mut self, cost: f32) -> bool {
        if cost.is_nan() || cost < 0.0 {
            return false;
        }
        if self.current + 1e-6 < cost {
            return false;
        }
        self.current = (self.current - cost).max(0.0);
        true
    }

    /// Begin channeling (suppresses regen until cleared).
    pub fn begin_channel(&mut self) {
        self.channeling = true;
    }

    /// End channeling.
    pub fn end_channel(&mut self) {
        self.channeling = false;
    }

    /// Catalyst-restore : clamped to capacity.
    pub fn restore(&mut self, amount: f32) {
        if amount.is_nan() || amount < 0.0 {
            return;
        }
        self.current = (self.current + amount).min(self.capacity);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_pool_full() {
        let p = ManaPool::new(100.0);
        assert!((p.current - 100.0).abs() < 1e-3);
    }

    #[test]
    fn try_consume_succeeds_when_sufficient() {
        let mut p = ManaPool::new(100.0);
        assert!(p.try_consume(30.0));
        assert!((p.current - 70.0).abs() < 1e-3);
    }

    #[test]
    fn try_consume_fails_when_underflow() {
        let mut p = ManaPool::new(20.0);
        assert!(!p.try_consume(30.0));
        assert!((p.current - 20.0).abs() < 1e-3);
    }

    #[test]
    fn channel_blocks_regen() {
        let mut p = ManaPool::new(100.0);
        p.current = 50.0;
        p.begin_channel();
        p.tick(10.0);
        assert!((p.current - 50.0).abs() < 1e-3);
        p.end_channel();
        p.tick(5.0);
        assert!((p.current - 60.0).abs() < 1e-3);
    }
}
