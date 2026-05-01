// § decorators.rs — BT decorator-kinds (modify-child-result)
// ════════════════════════════════════════════════════════════════════
// § I> per GDD § BEHAVIOR-TREE-NODES § DECORATORS (≥4 canonical)
// § I> wrap one child-node ; transform its BtStatus
// ════════════════════════════════════════════════════════════════════

use serde::{Deserialize, Serialize};

/// Decorator variants for BT Decorator nodes.
///
/// § I> ≥ 4 per GDD : Repeat · Invert · Limiter · Cooldown
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DecoratorKind {
    /// Repeat child up to `n` times (collected on Success ; abort on Failure).
    Repeat { n: u32 },
    /// Invert child's Success ↔ Failure (Running passes through).
    Invert,
    /// Cap number of ticks-per-frame this subtree may consume.
    Limiter { max_per_tick: u32 },
    /// Suppress evaluation for `cooldown_ms` after last Success ; returns Failure during cooldown.
    Cooldown { cooldown_ms: u32 },
}

impl DecoratorKind {
    /// Stable tag-string for audit-attribs.
    #[must_use]
    pub fn tag(&self) -> &'static str {
        match self {
            DecoratorKind::Repeat { .. } => "Repeat",
            DecoratorKind::Invert => "Invert",
            DecoratorKind::Limiter { .. } => "Limiter",
            DecoratorKind::Cooldown { .. } => "Cooldown",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn four_distinct_tags() {
        let ds = [
            DecoratorKind::Repeat { n: 1 },
            DecoratorKind::Invert,
            DecoratorKind::Limiter { max_per_tick: 1 },
            DecoratorKind::Cooldown { cooldown_ms: 1 },
        ];
        let mut tags: Vec<_> = ds.iter().map(DecoratorKind::tag).collect();
        tags.sort_unstable();
        tags.dedup();
        assert_eq!(tags.len(), 4);
    }
}
