// cap.rs — CoderCap bitset + SovereignBit
// ══════════════════════════════════════════════════════════════════
// § CoderCap : capability-bits guarding the Coder API surface
// § SovereignBit : separate, one-bit gate for substrate / schema / spec edits
// §   ALL high-impact kinds require SovereignBit::Held OR explicit-per-action consent
// § cap-system MOCK if cssl-host-cap-system is absent (it is, in this slice)
// ══════════════════════════════════════════════════════════════════

use serde::{Deserialize, Serialize};

/// Capability-bits gating Coder API surface.
///
/// Bitset semantics : multiple caps may be held simultaneously.
/// Default is empty ; minimum cap for [`crate::CoderRuntime::submit_edit`] is [`Self::AST_EDIT`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoderCap(pub u32);

impl CoderCap {
    /// No caps held.
    pub const NONE: Self = Self(0);
    /// Submit ordinary AST/balance/cosmetic edits.
    pub const AST_EDIT: Self = Self(1);
    /// Hot-reload edits while a session is live.
    pub const HOT_RELOAD: Self = Self(2);
    /// Schema-evolution edits (sovereign-required at runtime-policy level).
    pub const SCHEMA_EVOLVE: Self = Self(4);

    /// `true` if every bit in `other` is also set in `self`.
    pub const fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    /// Union.
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Intersection.
    #[must_use]
    pub const fn intersection(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }
}

impl Default for CoderCap {
    fn default() -> Self {
        Self::NONE
    }
}

/// Sovereign-bit (per-player). Held => high-impact substrate/schema/spec kinds allowed.
/// MUST come from a sovereign-cap system (mocked here as a simple newtype).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SovereignBit {
    /// Sovereign-bit not held — high-impact kinds rejected at hard-cap.
    NotHeld,
    /// Sovereign-bit held for this session/action.
    Held,
}

impl SovereignBit {
    /// Returns `true` iff held.
    pub const fn is_held(self) -> bool {
        matches!(self, Self::Held)
    }
}
