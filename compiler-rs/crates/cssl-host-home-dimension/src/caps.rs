//! Cap-bit flags governing every state-mutation on a Home.
//!
//! Cap-bits are **subtractive** : a freshly-constructed Home holds the empty
//! set + the implicit always-on `PRIVATE` mode. The owner explicitly grants
//! caps (via runtime / cssl-edge UI / cssl-host-mycelium hotfix). Any cap may
//! be revoked instantly, which forces an audit-emit and disconnects affected
//! visitors per spec/16 § Home-modes ALWAYS-OVERRIDABLE.

use serde::{Deserialize, Serialize};

/// HM_CAP_DECORATE — owner may place / remove decorations.
pub const HM_CAP_DECORATE: u32 = 1;
/// HM_CAP_INVITE — owner may invite friends or guildmates (Friend / Guild modes).
pub const HM_CAP_INVITE: u32 = 2;
/// HM_CAP_PUBLISH — Home may be Public-Listed in the Bazaar (M3) or Drop-In (M4).
pub const HM_CAP_PUBLISH: u32 = 4;
/// HM_CAP_NPC_HIRE — owner may host befriended NPCs (Companions).
pub const HM_CAP_NPC_HIRE: u32 = 8;
/// HM_CAP_FORGE_USE — Forge-node may queue crafting recipes.
pub const HM_CAP_FORGE_USE: u32 = 16;
/// HM_CAP_HOTFIX_RECEIVE — Home may apply hotfix-deltas from cssl-host-hotfix-stream.
pub const HM_CAP_HOTFIX_RECEIVE: u32 = 32;
/// HM_CAP_MYCELIAL_EMIT — opt-in events emitted to cssl-host-mycelium.
pub const HM_CAP_MYCELIAL_EMIT: u32 = 64;
/// HM_CAP_MEMORIAL_PIN — visitors may pin spore-ascriptions to the memorial-wall.
pub const HM_CAP_MEMORIAL_PIN: u32 = 128;

/// Mask of every defined cap-bit. Bits outside this mask are reserved.
pub const HM_CAP_ALL: u32 = HM_CAP_DECORATE
    | HM_CAP_INVITE
    | HM_CAP_PUBLISH
    | HM_CAP_NPC_HIRE
    | HM_CAP_FORGE_USE
    | HM_CAP_HOTFIX_RECEIVE
    | HM_CAP_MYCELIAL_EMIT
    | HM_CAP_MEMORIAL_PIN;

/// Bag of cap-bit flags. Bitwise-composable, serde-roundtrip safe.
///
/// Construct via [`HomeCapBits::empty`] / [`HomeCapBits::full`] /
/// [`HomeCapBits::from_bits`] ; manipulate via [`HomeCapBits::grant`] /
/// [`HomeCapBits::revoke`] / [`HomeCapBits::has`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct HomeCapBits(pub u32);

impl HomeCapBits {
    /// No caps granted (the default for a freshly-constructed Home).
    #[must_use]
    pub const fn empty() -> Self {
        Self(0)
    }

    /// All defined caps granted (used in tests + as a max-sovereignty preset).
    #[must_use]
    pub const fn full() -> Self {
        Self(HM_CAP_ALL)
    }

    /// Build from raw bits, masking off any reserved bits silently.
    ///
    /// The mask discipline ensures forward-compat : if a future spec adds a
    /// new cap-bit, old serialized blobs round-trip without spurious bits.
    #[must_use]
    pub const fn from_bits(bits: u32) -> Self {
        Self(bits & HM_CAP_ALL)
    }

    /// Raw representation, suitable for FFI / serialization.
    #[must_use]
    pub const fn bits(self) -> u32 {
        self.0
    }

    /// Whether **every** bit in `cap` is set (cap may be a single flag or a mask).
    #[must_use]
    pub const fn has(self, cap: u32) -> bool {
        let cap = cap & HM_CAP_ALL;
        (self.0 & cap) == cap
    }

    /// Add `cap` to the set, returning the result.
    #[must_use]
    pub const fn grant(self, cap: u32) -> Self {
        Self((self.0 | (cap & HM_CAP_ALL)) & HM_CAP_ALL)
    }

    /// Remove `cap` from the set, returning the result.
    #[must_use]
    pub const fn revoke(self, cap: u32) -> Self {
        Self(self.0 & !(cap & HM_CAP_ALL))
    }

    /// Whether the cap-set is empty.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Count of granted bits (sovereignty footprint).
    #[must_use]
    pub const fn count(self) -> u32 {
        self.0.count_ones()
    }
}

impl Default for HomeCapBits {
    fn default() -> Self {
        Self::empty()
    }
}

impl core::ops::BitOr for HomeCapBits {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self((self.0 | rhs.0) & HM_CAP_ALL)
    }
}

impl core::ops::BitAnd for HomeCapBits {
    type Output = Self;
    fn bitand(self, rhs: Self) -> Self {
        Self(self.0 & rhs.0)
    }
}
