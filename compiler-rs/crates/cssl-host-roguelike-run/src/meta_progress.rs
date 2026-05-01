// § meta-progression ← GDDs/ROGUELIKE_LOOP.csl §META-PROGRESSION
// ════════════════════════════════════════════════════════════════════
// § I> Hub-currency = Echoes ← drops + boss-kills · NOT-lost-on-death
// § I> spend-on : permanent-stat-nodes · classes · cosmetics
// § I> rule : meta-progression NEVER gates content-access ; only-tunes power-curve
// ════════════════════════════════════════════════════════════════════

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// § Meta-progression : persistent across runs · NOT lost on death.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetaProgress {
    /// Total Echoes-currency banked (post-soft-perma-carryover).
    pub echoes_total: u64,
    /// Class-IDs the player has unlocked (from base-class-tree).
    pub classes_unlocked: BTreeSet<u32>,
    /// Perk / meta-node string-keys unlocked (e.g. "deep-1", "verdant", "iron").
    pub perks_unlocked: BTreeSet<String>,
    /// Cumulative class-XP per class-id (capped per soft-perma rule).
    pub class_xp: BTreeSet<(u32, u64)>,
}

impl Default for MetaProgress {
    fn default() -> Self {
        Self::new()
    }
}

/// § Meta-progression operation errors.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MetaErr {
    /// Insufficient echoes for the requested spend.
    InsufficientEchoes { have: u64, need: u64 },
    /// Class already unlocked (idempotency-guard for caller-side detection).
    ClassAlreadyUnlocked(u32),
    /// Perk-key empty (must be non-empty meta-node identifier).
    EmptyPerkKey,
}

impl MetaProgress {
    /// Fresh meta-progress at run-1-genesis (zero echoes, no unlocks).
    pub fn new() -> Self {
        Self {
            echoes_total: 0,
            classes_unlocked: BTreeSet::new(),
            perks_unlocked: BTreeSet::new(),
            class_xp: BTreeSet::new(),
        }
    }

    /// Deposit echoes (e.g. from soft-perma-carryover or in-Hub-grants).
    pub fn deposit_echoes(&mut self, amount: u64) {
        self.echoes_total = self.echoes_total.saturating_add(amount);
    }

    /// Spend echoes ; fails if insufficient.
    pub fn spend_echoes(&mut self, amount: u64) -> Result<u64, MetaErr> {
        if self.echoes_total < amount {
            return Err(MetaErr::InsufficientEchoes {
                have: self.echoes_total,
                need: amount,
            });
        }
        self.echoes_total -= amount;
        Ok(self.echoes_total)
    }

    /// Unlock a class-ID. Idempotent : returns ClassAlreadyUnlocked if dup.
    pub fn unlock_class(&mut self, class_id: u32) -> Result<(), MetaErr> {
        if !self.classes_unlocked.insert(class_id) {
            return Err(MetaErr::ClassAlreadyUnlocked(class_id));
        }
        Ok(())
    }

    /// Unlock a perk-key (meta-node bridge for biome-DAG edges).
    pub fn unlock_perk(&mut self, key: impl Into<String>) -> Result<(), MetaErr> {
        let k: String = key.into();
        if k.is_empty() {
            return Err(MetaErr::EmptyPerkKey);
        }
        self.perks_unlocked.insert(k);
        Ok(())
    }

    /// Has the player unlocked a given perk ?
    pub fn has_perk(&self, key: &str) -> bool {
        self.perks_unlocked.contains(key)
    }

    /// Add class-XP (capped per soft-perma rule ; cap = 1_000_000 per class).
    pub fn grant_class_xp(&mut self, class_id: u32, xp: u64) {
        const CAP: u64 = 1_000_000;
        // Drop existing entry for this class, recompute capped.
        let prev: u64 = self
            .class_xp
            .iter()
            .find(|(c, _)| *c == class_id)
            .map_or(0, |(_, v)| *v);
        self.class_xp.retain(|(c, _)| *c != class_id);
        let next = prev.saturating_add(xp).min(CAP);
        self.class_xp.insert((class_id, next));
    }

    /// Read class-XP for a given class.
    pub fn class_xp_for(&self, class_id: u32) -> u64 {
        self.class_xp
            .iter()
            .find(|(c, _)| *c == class_id)
            .map_or(0, |(_, v)| *v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_meta_is_empty() {
        let m = MetaProgress::new();
        assert_eq!(m.echoes_total, 0);
        assert!(m.classes_unlocked.is_empty());
        assert!(m.perks_unlocked.is_empty());
    }

    #[test]
    fn spend_underflow_fails() {
        let mut m = MetaProgress::new();
        m.deposit_echoes(50);
        let err = m.spend_echoes(100).unwrap_err();
        assert!(matches!(err, MetaErr::InsufficientEchoes { have: 50, need: 100 }));
    }
}
