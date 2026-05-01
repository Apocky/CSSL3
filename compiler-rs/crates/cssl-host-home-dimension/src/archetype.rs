//! Archetype + access-mode enums per spec/grand-vision/16 § HOME-DIMENSION.
//!
//! 7 archetypes (player-choice @ first-launch ; re-changeable anytime via
//! [`crate::Home::change_archetype`]) and 5 access-modes (cap-gated, sovereign-revocable).

use serde::{Deserialize, Serialize};

/// One of the 7 Home archetypes a player may select.
///
/// Variants map 1:1 to spec/16 H1..H7 :
/// - `OrbitalShip` — Destiny-2-orbit pattern, viewport-window onto Multiverse
/// - `GroundWorkshop` — Warframe-Liset pattern, interior-room + craft-bench
/// - `CavernSanctum` — underground mycelial-burrow, root aesthetics, grow-things
/// - `CathedralHall` — grand architecture, gallery of Ascended items + NPCs
/// - `CosmicObservatory` — floating among stars, contemplation, scrying
/// - `LivingForest` — organic, room-shape evolves with aggregate-bias (opt-in)
/// - `HybridCustom` — runtime-procgen mix (cssl-host-fab-procgen blueprint)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ArchetypeId {
    /// H1 ORBITAL-SHIP : Destiny-2-orbit-pattern.
    OrbitalShip,
    /// H2 GROUND-WORKSHOP : Warframe-Liset-pattern.
    GroundWorkshop,
    /// H3 CAVERN-SANCTUM : underground-mycelial-burrow.
    CavernSanctum,
    /// H4 CATHEDRAL-HALL : grand-architecture, NPC-companions hub.
    CathedralHall,
    /// H5 COSMIC-OBSERVATORY : floating-platform-among-stars.
    CosmicObservatory,
    /// H6 LIVING-FOREST : organic-grown-by-mycelium, biome-evolves.
    LivingForest,
    /// H7 HYBRID-CUSTOM : player-composed @ runtime-procgen.
    HybridCustom,
}

impl ArchetypeId {
    /// All seven variants in canonical (spec H1..H7) order.
    ///
    /// Suitable for round-tripping through serde + driving UI carousels
    /// without ever losing or reordering variants.
    #[must_use]
    pub const fn all() -> [Self; 7] {
        [
            Self::OrbitalShip,
            Self::GroundWorkshop,
            Self::CavernSanctum,
            Self::CathedralHall,
            Self::CosmicObservatory,
            Self::LivingForest,
            Self::HybridCustom,
        ]
    }

    /// Short H-code per spec (`"H1".."H7"`).
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::OrbitalShip => "H1",
            Self::GroundWorkshop => "H2",
            Self::CavernSanctum => "H3",
            Self::CathedralHall => "H4",
            Self::CosmicObservatory => "H5",
            Self::LivingForest => "H6",
            Self::HybridCustom => "H7",
        }
    }
}

/// One of the 5 access-modes governing who may enter a Home.
///
/// Default is [`AccessMode::PrivateAlwaysOn`] — opt-in is required to widen.
/// Each mode beyond Private is **cap-gated** (see [`crate::HomeCapBits`]) and
/// can be revoked instantly by the owner ; revocation forces an audit-emit
/// and disconnects current visitors per spec/16 § Home-modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum AccessMode {
    /// M0 : default — only the owner may enter.
    PrivateAlwaysOn,
    /// M1 : whitelist of friend-pubkeys may enter.
    FriendOnly,
    /// M2 : guild-members may enter.
    GuildOpen,
    /// M3 : discoverable via Bazaar (Tier-3+).
    PublicListed,
    /// M4 : multiverse may randomly route other players during fruiting-season.
    RandomDropin,
}

impl AccessMode {
    /// All five variants in canonical (M0..M4) order.
    #[must_use]
    pub const fn all() -> [Self; 5] {
        [
            Self::PrivateAlwaysOn,
            Self::FriendOnly,
            Self::GuildOpen,
            Self::PublicListed,
            Self::RandomDropin,
        ]
    }

    /// Short M-code per spec (`"M0".."M4"`).
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::PrivateAlwaysOn => "M0",
            Self::FriendOnly => "M1",
            Self::GuildOpen => "M2",
            Self::PublicListed => "M3",
            Self::RandomDropin => "M4",
        }
    }

    /// Whether this mode is the always-on private default that requires no cap.
    #[must_use]
    pub const fn is_private(self) -> bool {
        matches!(self, Self::PrivateAlwaysOn)
    }
}

impl Default for AccessMode {
    fn default() -> Self {
        Self::PrivateAlwaysOn
    }
}
