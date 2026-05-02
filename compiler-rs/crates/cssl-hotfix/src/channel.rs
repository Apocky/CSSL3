//! § channel — the 9 update channels.
//!
//! Each channel is bound to a specific `CapKey` (see `cap.rs`) which
//! defines the set of cap-roles authorized to sign bundles for it.
//! A misclassified bundle (channel ≠ cap-role-of-signer) is rejected
//! by `verify::verify_bundle` even if the Ed25519 signature is valid,
//! defending against escalation forgeries.

use crate::cap::CapRole;
use serde::{Deserialize, Serialize};

/// § The 9 update channels.
///
/// `repr(u8)` gives a stable wire-byte for the bundle header. Variants
/// renumbered → BUNDLE FORMAT BREAKING CHANGE — do NOT renumber casually.
#[derive(
    Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
#[repr(u8)]
pub enum Channel {
    /// (1) full LoA.exe replacement · BSDIFF delta-patch optional · cap-A.
    LoaBinary = 1,
    /// (2) compiled .cssl modules · hot-reload · function-table-swap · cap-B.
    CsslBundle = 2,
    /// (3) KAN classifier weights per swap-point · hot-reload · cap-C.
    KanWeights = 3,
    /// (4) recipes · drop-rates · NPC-stats · cooldowns · cap-C.
    BalanceConfig = 4,
    /// (5) crafting recipes · alchemy formulas · cap-E.
    RecipeBook = 5,
    /// (6) recurring-enemy persistence + behavior-tree updates · cap-E.
    NemesisBestiary = 6,
    /// (7) HIGH-PRIORITY · pushed-immediate · cap-D · the only default-on channel.
    SecurityPatch = 7,
    /// (8) narrative arcs · NPC dialogue · localization · cap-E.
    StoryletContent = 8,
    /// (9) shader bytecode · render-pipeline config · cosmetic shader-packs · cap-E.
    RenderPipeline = 9,
}

/// All 9 channels in stable order, used for default-policy initialisation
/// and round-trip tests.
pub const CHANNELS: [Channel; 9] = [
    Channel::LoaBinary,
    Channel::CsslBundle,
    Channel::KanWeights,
    Channel::BalanceConfig,
    Channel::RecipeBook,
    Channel::NemesisBestiary,
    Channel::SecurityPatch,
    Channel::StoryletContent,
    Channel::RenderPipeline,
];

/// § Quiescent-apply class (drives `apply.rs` boundary scheduler).
#[derive(
    Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
pub enum ChannelClass {
    /// Requires full process-restart (loa.binary).
    RestartRequired,
    /// Hot-reload on next-frame boundary (cssl.bundle).
    NextFrameBoundary,
    /// Atomic hot-swap (kan.weights).
    AtomicHotSwap,
    /// Hot-swap on scene-transition (balance/recipe/storylet/render/nemesis).
    SceneTransition,
    /// Apply immediately (security.patch).
    Immediate,
}

impl Channel {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::LoaBinary => "loa.binary",
            Self::CsslBundle => "cssl.bundle",
            Self::KanWeights => "kan.weights",
            Self::BalanceConfig => "balance.config",
            Self::RecipeBook => "recipe.book",
            Self::NemesisBestiary => "nemesis.bestiary",
            Self::SecurityPatch => "security.patch",
            Self::StoryletContent => "storylet.content",
            Self::RenderPipeline => "render.pipeline",
        }
    }

    #[must_use]
    pub fn from_name(s: &str) -> Option<Self> {
        match s {
            "loa.binary" => Some(Self::LoaBinary),
            "cssl.bundle" => Some(Self::CsslBundle),
            "kan.weights" => Some(Self::KanWeights),
            "balance.config" => Some(Self::BalanceConfig),
            "recipe.book" => Some(Self::RecipeBook),
            "nemesis.bestiary" => Some(Self::NemesisBestiary),
            "security.patch" => Some(Self::SecurityPatch),
            "storylet.content" => Some(Self::StoryletContent),
            "render.pipeline" => Some(Self::RenderPipeline),
            _ => None,
        }
    }

    /// Which cap-role is authorised to sign bundles for this channel.
    /// Single source of truth for sign + verify pipelines.
    #[must_use]
    pub const fn required_cap(self) -> CapRole {
        match self {
            Self::LoaBinary => CapRole::CapA,
            Self::CsslBundle => CapRole::CapB,
            Self::KanWeights | Self::BalanceConfig => CapRole::CapC,
            Self::SecurityPatch => CapRole::CapD,
            Self::RecipeBook
            | Self::NemesisBestiary
            | Self::StoryletContent
            | Self::RenderPipeline => CapRole::CapE,
        }
    }

    /// Quiescent-apply boundary class, drives the apply scheduler.
    #[must_use]
    pub const fn class(self) -> ChannelClass {
        match self {
            Self::LoaBinary => ChannelClass::RestartRequired,
            Self::CsslBundle => ChannelClass::NextFrameBoundary,
            Self::KanWeights => ChannelClass::AtomicHotSwap,
            Self::BalanceConfig
            | Self::RecipeBook
            | Self::NemesisBestiary
            | Self::StoryletContent
            | Self::RenderPipeline => ChannelClass::SceneTransition,
            Self::SecurityPatch => ChannelClass::Immediate,
        }
    }

    /// Default Σ-mask consent : only `SecurityPatch` is on by default ;
    /// every other channel is opt-in.
    #[must_use]
    pub const fn default_consent_on(self) -> bool {
        matches!(self, Self::SecurityPatch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channels_const_is_nine_distinct() {
        assert_eq!(CHANNELS.len(), 9);
        for (i, c) in CHANNELS.iter().enumerate() {
            assert_eq!(*c as u8, (i as u8) + 1);
        }
    }

    #[test]
    fn name_roundtrip() {
        for c in CHANNELS {
            assert_eq!(Channel::from_name(c.name()), Some(c));
        }
    }

    #[test]
    fn from_name_unknown_is_none() {
        assert!(Channel::from_name("not-a-channel").is_none());
        assert!(Channel::from_name("").is_none());
    }

    #[test]
    fn cap_role_mapping_is_total_and_distinct_per_band() {
        // The cap-A..cap-E roles cover all 9 channels.
        assert_eq!(Channel::LoaBinary.required_cap(), CapRole::CapA);
        assert_eq!(Channel::CsslBundle.required_cap(), CapRole::CapB);
        assert_eq!(Channel::KanWeights.required_cap(), CapRole::CapC);
        assert_eq!(Channel::BalanceConfig.required_cap(), CapRole::CapC);
        assert_eq!(Channel::SecurityPatch.required_cap(), CapRole::CapD);
        for c in [
            Channel::RecipeBook,
            Channel::NemesisBestiary,
            Channel::StoryletContent,
            Channel::RenderPipeline,
        ] {
            assert_eq!(c.required_cap(), CapRole::CapE);
        }
    }

    #[test]
    fn class_mapping_total() {
        assert_eq!(Channel::LoaBinary.class(), ChannelClass::RestartRequired);
        assert_eq!(Channel::CsslBundle.class(), ChannelClass::NextFrameBoundary);
        assert_eq!(Channel::KanWeights.class(), ChannelClass::AtomicHotSwap);
        assert_eq!(Channel::SecurityPatch.class(), ChannelClass::Immediate);
        assert_eq!(
            Channel::BalanceConfig.class(),
            ChannelClass::SceneTransition
        );
    }

    #[test]
    fn default_consent_only_security() {
        assert!(Channel::SecurityPatch.default_consent_on());
        for c in CHANNELS {
            if c != Channel::SecurityPatch {
                assert!(
                    !c.default_consent_on(),
                    "{} must be opt-in by default",
                    c.name()
                );
            }
        }
    }

    #[test]
    fn channel_serde_roundtrip() {
        for c in CHANNELS {
            let s = serde_json::to_string(&c).unwrap();
            let back: Channel = serde_json::from_str(&s).unwrap();
            assert_eq!(c, back);
        }
    }
}
