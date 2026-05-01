//! § policy — per-tier apply decision + sovereign-cap bit-flag check.
//!
//! Tier-policy rules (§ 16) :
//!   - `Cosmetic` → `AutoApply` (silent ; audit-emit on apply).
//!   - `Balance`  → `PromptUser` (apply only on user-confirm ; armed
//!     30-second revert window starts at apply).
//!   - `Security` → `RequireSovereign` (apply only if sovereign-cap
//!     `SOV_HOTFIX_APPLY` is set ; otherwise `Reject`).

use crate::class::{HotfixClass, HotfixTier};
use serde::{Deserialize, Serialize};

/// § Sovereign-cap bit `SOV_HOTFIX_APPLY` per brief landmine.
///
/// The host's cap-system (cssl-host-cap-system) integrates by passing
/// the live cap-bitmask into `decide_apply` ; we accept the raw u32
/// to avoid a dep on that crate at this stage.
pub const SOV_HOTFIX_APPLY: u32 = 0x100;

/// § Tagged sovereign-cap bitmask. Newtype prevents accidental mix
/// with arbitrary integer flags at call-sites.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SovereignCaps(pub u32);

impl SovereignCaps {
    #[must_use]
    pub const fn empty() -> Self {
        Self(0)
    }

    #[must_use]
    pub const fn with_hotfix_apply() -> Self {
        Self(SOV_HOTFIX_APPLY)
    }

    #[must_use]
    pub const fn has_hotfix_apply(self) -> bool {
        (self.0 & SOV_HOTFIX_APPLY) == SOV_HOTFIX_APPLY
    }
}

/// § The decision the policy module renders per (class, caps) pair.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PolicyDecision {
    /// Silent auto-apply (Cosmetic tier).
    AutoApply,
    /// Stage and prompt user ; on confirm, apply + arm revert window.
    PromptUser,
    /// Apply if sovereign-cap-set ; else `Reject`.
    RequireSovereign,
    /// Refuse outright (returned when Security-tier lacks sovereign cap).
    Reject,
}

/// § Per-tier policy decision. Pure function ; deterministic.
#[must_use]
pub fn decide_apply(class: HotfixClass, caps: SovereignCaps) -> PolicyDecision {
    match class.tier() {
        HotfixTier::Cosmetic => PolicyDecision::AutoApply,
        HotfixTier::Balance => PolicyDecision::PromptUser,
        HotfixTier::Security => {
            if caps.has_hotfix_apply() {
                PolicyDecision::RequireSovereign
            } else {
                PolicyDecision::Reject
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// per-tier policy-decision (3 tier-cases batched into 3 fns).
    #[test]
    fn cosmetic_classes_auto_apply_regardless_of_caps() {
        let cosmetic = [
            HotfixClass::KanWeightUpdate,
            HotfixClass::NewRecipeUnlock,
            HotfixClass::NemesisArchetypeEvolve,
            HotfixClass::NarrativeStoryletAdd,
            HotfixClass::RenderPipelineParam,
        ];
        for c in cosmetic {
            assert_eq!(
                decide_apply(c, SovereignCaps::empty()),
                PolicyDecision::AutoApply
            );
            assert_eq!(
                decide_apply(c, SovereignCaps::with_hotfix_apply()),
                PolicyDecision::AutoApply
            );
        }
    }

    #[test]
    fn balance_classes_prompt_user() {
        for c in [HotfixClass::ProcgenBiasNudge, HotfixClass::BalanceConstantAdjust] {
            assert_eq!(
                decide_apply(c, SovereignCaps::empty()),
                PolicyDecision::PromptUser
            );
        }
    }

    #[test]
    fn security_class_rejects_without_sovereign_cap() {
        assert_eq!(
            decide_apply(HotfixClass::SovereignCapPolicyFix, SovereignCaps::empty()),
            PolicyDecision::Reject
        );
        assert_eq!(
            decide_apply(
                HotfixClass::SovereignCapPolicyFix,
                SovereignCaps::with_hotfix_apply()
            ),
            PolicyDecision::RequireSovereign
        );
    }

    #[test]
    fn caps_bitmask_only_recognizes_correct_bit() {
        assert!(!SovereignCaps(0x001).has_hotfix_apply());
        assert!(!SovereignCaps(0x0FF).has_hotfix_apply());
        assert!(SovereignCaps(SOV_HOTFIX_APPLY).has_hotfix_apply());
        assert!(SovereignCaps(SOV_HOTFIX_APPLY | 0x001).has_hotfix_apply());
    }
}
