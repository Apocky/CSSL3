//! Navigation-hub : portal/door/launch-bay registration.
//!
//! Portals are the cap-gated egress points to other places (Multiverse,
//! Bazaar, Run-start, Friends' Homes ; spec/16 § Home-features
//! NAVIGATION-HUB). Registering or enabling a portal that requires a cap
//! the Home does not hold returns an error from
//! [`crate::Home::register_portal`].

use serde::{Deserialize, Serialize};

/// Where a portal leads.
///
/// Variants are kept open (`Other(String)`) so consumers can add more
/// destinations without bumping the home schema.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PortalDest {
    /// Multiverse hub.
    Multiverse,
    /// Bazaar marketplace.
    Bazaar,
    /// Start a fresh run.
    RunStart,
    /// Visit a specific friend's Home (their pubkey is the destination).
    FriendHome([u8; 32]),
    /// Some other named destination (consumer-defined string).
    Other(String),
}

/// One registered portal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Portal {
    /// Caller-allocated id (map-key in the Home).
    pub id: u32,
    /// Destination.
    pub dest: PortalDest,
    /// Cap-bit-mask required for visitors to traverse.
    pub cap_required: u32,
    /// Whether the portal is currently enabled.
    pub enabled: bool,
}

impl Portal {
    /// Build a fresh portal — defaults to enabled.
    #[must_use]
    pub fn new(id: u32, dest: PortalDest, cap_required: u32) -> Self {
        Self {
            id,
            dest,
            cap_required,
            enabled: true,
        }
    }
}
