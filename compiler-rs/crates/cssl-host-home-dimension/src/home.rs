//! `Home` — top-level pocket-dimension state-machine.
//!
//! Owns every subsystem map (decorations / trophies / companions / portals /
//! forge-queue / memorial / mycelial-terminal) plus access-mode + cap-bits +
//! audit-log. All state-mutating APIs are cap-gated and produce
//! [`crate::AuditEvent`]s.
//!
//! ## Determinism
//!
//! Every collection is a `BTreeMap` (or insertion-ordered `Vec` where order
//! is semantic) so the serde representation is canonical. There is no
//! `HashMap` and no `RandomState`. Tests round-trip JSON to assert this.
//!
//! ## Sovereignty
//!
//! Every cap-bit can be revoked instantly. [`Home::revoke_cap`] forces
//! recomputing the access-mode (any visitors connected through a now-revoked
//! cap are ejected) and emits an audit-event for transparency.

use crate::archetype::{AccessMode, ArchetypeId};
use crate::caps::{
    HomeCapBits, HM_CAP_DECORATE, HM_CAP_FORGE_USE, HM_CAP_INVITE, HM_CAP_MEMORIAL_PIN,
    HM_CAP_NPC_HIRE, HM_CAP_PUBLISH,
};
use crate::companion::Companion;
use crate::decoration::{DecorationSlot, SlotTransform};
use crate::forge::{ForgeQueueItem, ForgeRecipeId};
use crate::ids::{HomeId, Pubkey, Timestamp};
use crate::memorial::{MemorialAscription, MemorialEntry};
use crate::portal::{Portal, PortalDest};
use crate::terminal::MycelialTerminal;
use crate::trophy::Trophy;
use crate::{
    asset_ref::OpaqueAsset,
    audit::{AuditEvent, AuditKind, AuditLog},
    HOME_SCHEMA_VERSION,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Serde adapter : `BTreeMap<Pubkey, Companion>` ↔ `Vec<(Pubkey, Companion)>`.
///
/// JSON requires map-keys to be strings ; `Pubkey` is a `[u8; 32]` newtype,
/// so a direct `BTreeMap<Pubkey, _>` serialization fails. We canonicalize via
/// `BTreeMap::iter` (already ordered) and round-trip through a Vec of pairs.
mod pubkey_companion_map_as_vec {
    use super::{Companion, Pubkey};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::collections::BTreeMap;

    pub(super) fn serialize<S: Serializer>(
        m: &BTreeMap<Pubkey, Companion>,
        ser: S,
    ) -> Result<S::Ok, S::Error> {
        let v: Vec<(&Pubkey, &Companion)> = m.iter().collect();
        v.serialize(ser)
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(
        de: D,
    ) -> Result<BTreeMap<Pubkey, Companion>, D::Error> {
        let v: Vec<(Pubkey, Companion)> = Vec::deserialize(de)?;
        Ok(v.into_iter().collect())
    }
}

/// Serde adapter : `BTreeSet<Pubkey>` ↔ `Vec<Pubkey>` (same JSON-key reason).
mod pubkey_set_as_vec {
    use super::Pubkey;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::collections::BTreeSet;

    pub(super) fn serialize<S: Serializer>(s: &BTreeSet<Pubkey>, ser: S) -> Result<S::Ok, S::Error> {
        let v: Vec<&Pubkey> = s.iter().collect();
        v.serialize(ser)
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(de: D) -> Result<BTreeSet<Pubkey>, D::Error> {
        let v: Vec<Pubkey> = Vec::deserialize(de)?;
        Ok(v.into_iter().collect())
    }
}

/// Errors returnable from a `Home` mutation.
///
/// Variants are stable + serde-roundtrip safe so cssl-edge can echo them
/// to the player UI without translation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HomeError {
    /// The required cap-bit(s) are not granted on this Home.
    MissingCap {
        /// Bitmask of cap-bits required.
        required: u32,
        /// Bitmask of cap-bits actually granted.
        granted: u32,
    },
    /// A visitor was rejected because the access-mode does not allow them.
    AccessDenied {
        /// Current access-mode.
        mode: AccessMode,
    },
    /// Slot / id not found.
    NotFound,
    /// Duplicate id : caller tried to insert the same id twice without removal.
    DuplicateId,
    /// The transition is not allowed in the current state-machine state.
    InvalidTransition,
}

impl core::fmt::Display for HomeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::MissingCap { required, granted } => write!(
                f,
                "missing cap-bits : required=0x{required:08x} granted=0x{granted:08x}"
            ),
            Self::AccessDenied { mode } => {
                write!(f, "access denied : current mode is {}", mode.code())
            }
            Self::NotFound => f.write_str("not found"),
            Self::DuplicateId => f.write_str("duplicate id"),
            Self::InvalidTransition => f.write_str("invalid transition"),
        }
    }
}

impl std::error::Error for HomeError {}

/// Inner, serializable state of a Home.
///
/// Public so callers can snapshot / persist via `serde_json` without going
/// through the [`Home`] facade. Mutations should still go through `Home`
/// methods to keep the audit-log consistent.
///
/// Not `Eq` because [`DecorationSlot::transform`] holds `f32` values.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HomeState {
    /// Schema version.
    pub schema_version: u32,
    /// Stable id.
    pub id: HomeId,
    /// Owner pubkey.
    pub owner: Pubkey,
    /// Current archetype.
    pub archetype: ArchetypeId,
    /// Current access-mode.
    pub mode: AccessMode,
    /// Granted cap-bits.
    pub caps: HomeCapBits,
    /// Friend allowlist (M1 FriendOnly). Serialized as Vec — see
    /// [`pubkey_companion_map_as_vec`] for the JSON-key rationale.
    #[serde(with = "pubkey_set_as_vec")]
    pub friends: BTreeSet<Pubkey>,
    /// Guild allowlist (M2 GuildOpen).
    #[serde(with = "pubkey_set_as_vec")]
    pub guild: BTreeSet<Pubkey>,
    /// Currently-connected visitors.
    #[serde(with = "pubkey_set_as_vec")]
    pub visitors: BTreeSet<Pubkey>,
    /// Decoration slots.
    pub decorations: BTreeMap<u32, DecorationSlot>,
    /// Trophies.
    pub trophies: BTreeMap<u64, Trophy>,
    /// Companions, keyed by pubkey. JSON-serialized as a Vec of pairs because
    /// `Pubkey` is a 32-byte array, not a string — so a `BTreeMap` with that
    /// key would otherwise fail JSON serialization. The Vec is canonicalized
    /// via `BTreeMap::iter` so order is deterministic.
    #[serde(with = "pubkey_companion_map_as_vec")]
    pub companions: BTreeMap<Pubkey, Companion>,
    /// Portals.
    pub portals: BTreeMap<u32, Portal>,
    /// Forge queue.
    pub forge_queue: BTreeMap<u64, ForgeQueueItem>,
    /// Memorial-wall entries.
    pub memorials: BTreeMap<u64, MemorialEntry>,
    /// Mycelial-terminal opt-state.
    pub terminal: MycelialTerminal,
    /// Append-only audit-log.
    pub audit: AuditLog,
}

/// Cap-bits required for each access-mode beyond Private.
const fn cap_for_mode(mode: AccessMode) -> u32 {
    match mode {
        AccessMode::PrivateAlwaysOn => 0,
        AccessMode::FriendOnly | AccessMode::GuildOpen => HM_CAP_INVITE,
        AccessMode::PublicListed | AccessMode::RandomDropin => HM_CAP_PUBLISH,
    }
}

/// Top-level Home facade. Wraps [`HomeState`] with cap-gated mutations.
///
/// Not `Eq` because the inner [`HomeState`] embeds `f32`-bearing decoration
/// transforms — see [`HomeState`] for details.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Home {
    state: HomeState,
}

impl Home {
    /// Construct a new Home with default access-mode and no caps granted.
    ///
    /// The owner is implicitly the only person able to enter (M0 Private).
    /// The audit-log is seeded with one [`AuditKind::Created`] event.
    #[must_use]
    pub fn new(id: HomeId, owner: Pubkey, archetype: ArchetypeId, at: Timestamp) -> Self {
        let mut audit = AuditLog::new();
        audit.push(AuditEvent {
            kind: AuditKind::Created,
            at,
            actor: owner,
            note: format!("archetype={}", archetype.code()),
        });
        Self {
            state: HomeState {
                schema_version: HOME_SCHEMA_VERSION,
                id,
                owner,
                archetype,
                mode: AccessMode::default(),
                caps: HomeCapBits::empty(),
                friends: BTreeSet::new(),
                guild: BTreeSet::new(),
                visitors: BTreeSet::new(),
                decorations: BTreeMap::new(),
                trophies: BTreeMap::new(),
                companions: BTreeMap::new(),
                portals: BTreeMap::new(),
                forge_queue: BTreeMap::new(),
                memorials: BTreeMap::new(),
                terminal: MycelialTerminal::new(),
                audit,
            },
        }
    }

    /// Read-only access to inner state (for serde / inspection).
    #[must_use]
    pub fn state(&self) -> &HomeState {
        &self.state
    }

    /// Stable id.
    #[must_use]
    pub fn id(&self) -> HomeId {
        self.state.id
    }

    /// Owner pubkey.
    #[must_use]
    pub fn owner(&self) -> Pubkey {
        self.state.owner
    }

    /// Current access-mode.
    #[must_use]
    pub fn mode(&self) -> AccessMode {
        self.state.mode
    }

    /// Current granted cap-bits.
    #[must_use]
    pub fn caps(&self) -> HomeCapBits {
        self.state.caps
    }

    /// Current archetype.
    #[must_use]
    pub fn archetype(&self) -> ArchetypeId {
        self.state.archetype
    }

    /// Borrow the audit-log (read-only).
    #[must_use]
    pub fn audit(&self) -> &AuditLog {
        &self.state.audit
    }

    /// Grant cap-bits.
    pub fn grant_cap(&mut self, cap: u32, at: Timestamp) {
        let new_caps = self.state.caps.grant(cap);
        if new_caps == self.state.caps {
            return;
        }
        self.state.caps = new_caps;
        self.state.audit.push(AuditEvent {
            kind: AuditKind::CapGranted,
            at,
            actor: self.state.owner,
            note: format!("cap=0x{cap:08x}"),
        });
    }

    /// Revoke cap-bits. If the current access-mode now lacks its required
    /// cap, the mode is forced back to [`AccessMode::PrivateAlwaysOn`] and
    /// every connected visitor is ejected. All state-changes emit audit.
    pub fn revoke_cap(&mut self, cap: u32, at: Timestamp) {
        let new_caps = self.state.caps.revoke(cap);
        if new_caps == self.state.caps {
            return;
        }
        self.state.caps = new_caps;
        self.state.audit.push(AuditEvent {
            kind: AuditKind::CapRevoked,
            at,
            actor: self.state.owner,
            note: format!("cap=0x{cap:08x}"),
        });
        // sovereignty discipline : if mode no-longer-satisfies, force-private + eject
        let need = cap_for_mode(self.state.mode);
        if need != 0 && !self.state.caps.has(need) {
            let prev_mode = self.state.mode;
            self.state.mode = AccessMode::PrivateAlwaysOn;
            self.state.audit.push(AuditEvent {
                kind: AuditKind::ModeChanged,
                at,
                actor: self.state.owner,
                note: format!(
                    "auto-revert {} → {} (cap-revoked)",
                    prev_mode.code(),
                    AccessMode::PrivateAlwaysOn.code()
                ),
            });
            self.eject_all_visitors(at);
        }
    }

    /// Change the access-mode. Cap-gated by `HM_CAP_INVITE` (Friend / Guild)
    /// or `HM_CAP_PUBLISH` (Public / Dropin). Setting back to Private is
    /// always permitted.
    pub fn set_mode(&mut self, mode: AccessMode, at: Timestamp) -> Result<(), HomeError> {
        let need = cap_for_mode(mode);
        if need != 0 && !self.state.caps.has(need) {
            return Err(HomeError::MissingCap {
                required: need,
                granted: self.state.caps.bits(),
            });
        }
        if mode == self.state.mode {
            return Ok(());
        }
        let prev = self.state.mode;
        self.state.mode = mode;
        self.state.audit.push(AuditEvent {
            kind: AuditKind::ModeChanged,
            at,
            actor: self.state.owner,
            note: format!("{} → {}", prev.code(), mode.code()),
        });
        // narrowing the mode (e.g. Public → Friend) ejects any visitor no-longer-allowed.
        // Setting to Private ejects all.
        if matches!(mode, AccessMode::PrivateAlwaysOn) {
            self.eject_all_visitors(at);
        } else {
            self.recompute_visitor_eligibility(at);
        }
        Ok(())
    }

    /// Change archetype.
    pub fn change_archetype(&mut self, archetype: ArchetypeId, at: Timestamp) {
        if archetype == self.state.archetype {
            return;
        }
        let prev = self.state.archetype;
        self.state.archetype = archetype;
        self.state.audit.push(AuditEvent {
            kind: AuditKind::ArchetypeChanged,
            at,
            actor: self.state.owner,
            note: format!("{} → {}", prev.code(), archetype.code()),
        });
    }

    /// Add a friend to the M1 allowlist.
    pub fn add_friend(&mut self, friend: Pubkey) {
        self.state.friends.insert(friend);
    }

    /// Remove a friend from the M1 allowlist.
    pub fn remove_friend(&mut self, friend: &Pubkey, at: Timestamp) {
        self.state.friends.remove(friend);
        // narrow visitors
        if matches!(self.state.mode, AccessMode::FriendOnly) && self.state.visitors.remove(friend) {
            self.state.audit.push(AuditEvent {
                kind: AuditKind::VisitorEjected,
                at,
                actor: self.state.owner,
                note: "friend removed".into(),
            });
        }
    }

    /// Add a guildmate to the M2 allowlist.
    pub fn add_guild(&mut self, member: Pubkey) {
        self.state.guild.insert(member);
    }

    /// Try to admit a visitor. Cap-gated by access-mode allowlist.
    pub fn accept_visitor(&mut self, visitor: Pubkey, at: Timestamp) -> Result<(), HomeError> {
        if visitor == self.state.owner {
            // owner re-entry is always allowed and not audited as a visit
            return Ok(());
        }
        let allowed = match self.state.mode {
            AccessMode::PrivateAlwaysOn => false,
            AccessMode::FriendOnly => self.state.friends.contains(&visitor),
            AccessMode::GuildOpen => self.state.guild.contains(&visitor),
            AccessMode::PublicListed | AccessMode::RandomDropin => true,
        };
        if !allowed {
            return Err(HomeError::AccessDenied {
                mode: self.state.mode,
            });
        }
        if !self.state.visitors.insert(visitor) {
            return Ok(()); // already present
        }
        self.state.audit.push(AuditEvent {
            kind: AuditKind::VisitorEntered,
            at,
            actor: visitor,
            note: format!("mode={}", self.state.mode.code()),
        });
        Ok(())
    }

    /// Eject one specific visitor.
    pub fn eject_visitor(&mut self, visitor: &Pubkey, at: Timestamp) -> bool {
        if self.state.visitors.remove(visitor) {
            self.state.audit.push(AuditEvent {
                kind: AuditKind::VisitorEjected,
                at,
                actor: *visitor,
                note: "owner-eject".into(),
            });
            true
        } else {
            false
        }
    }

    fn eject_all_visitors(&mut self, at: Timestamp) {
        let drained: Vec<Pubkey> = self.state.visitors.iter().copied().collect();
        for v in drained {
            self.state.visitors.remove(&v);
            self.state.audit.push(AuditEvent {
                kind: AuditKind::VisitorEjected,
                at,
                actor: v,
                note: "auto-eject (mode-narrowed)".into(),
            });
        }
    }

    fn recompute_visitor_eligibility(&mut self, at: Timestamp) {
        let removable: Vec<Pubkey> = self
            .state
            .visitors
            .iter()
            .copied()
            .filter(|v| match self.state.mode {
                AccessMode::PrivateAlwaysOn => true,
                AccessMode::FriendOnly => !self.state.friends.contains(v),
                AccessMode::GuildOpen => !self.state.guild.contains(v),
                AccessMode::PublicListed | AccessMode::RandomDropin => false,
            })
            .collect();
        for v in removable {
            self.state.visitors.remove(&v);
            self.state.audit.push(AuditEvent {
                kind: AuditKind::VisitorEjected,
                at,
                actor: v,
                note: "auto-eject (mode-narrowed)".into(),
            });
        }
    }

    // ─── DECORATIONS ────────────────────────────────────────────────

    /// Place a decoration. Requires `HM_CAP_DECORATE`.
    pub fn decorate(
        &mut self,
        slot_id: u32,
        asset: OpaqueAsset,
        transform: SlotTransform,
        at: Timestamp,
    ) -> Result<(), HomeError> {
        self.require(HM_CAP_DECORATE)?;
        let entry = DecorationSlot::new(slot_id, asset, transform);
        self.state.decorations.insert(slot_id, entry);
        self.state.audit.push(AuditEvent {
            kind: AuditKind::DecorationPlaced,
            at,
            actor: self.state.owner,
            note: format!("slot={slot_id}"),
        });
        Ok(())
    }

    /// Remove a decoration. Requires `HM_CAP_DECORATE`.
    pub fn remove_decoration(&mut self, slot_id: u32, at: Timestamp) -> Result<(), HomeError> {
        self.require(HM_CAP_DECORATE)?;
        if self.state.decorations.remove(&slot_id).is_none() {
            return Err(HomeError::NotFound);
        }
        self.state.audit.push(AuditEvent {
            kind: AuditKind::DecorationRemoved,
            at,
            actor: self.state.owner,
            note: format!("slot={slot_id}"),
        });
        Ok(())
    }

    /// Iterate decorations in slot-id order.
    pub fn list_decorations(&self) -> impl Iterator<Item = &DecorationSlot> {
        self.state.decorations.values()
    }

    // ─── TROPHIES ───────────────────────────────────────────────────

    /// Pin a trophy to the trophy-case.
    pub fn pin_trophy(&mut self, t: Trophy, at: Timestamp) -> Result<(), HomeError> {
        if self.state.trophies.contains_key(&t.id) {
            return Err(HomeError::DuplicateId);
        }
        let id = t.id;
        self.state.trophies.insert(id, t);
        self.state.audit.push(AuditEvent {
            kind: AuditKind::TrophyPinned,
            at,
            actor: self.state.owner,
            note: format!("trophy={id}"),
        });
        Ok(())
    }

    /// Unpin a trophy.
    pub fn unpin_trophy(&mut self, id: u64, at: Timestamp) -> Result<(), HomeError> {
        if self.state.trophies.remove(&id).is_none() {
            return Err(HomeError::NotFound);
        }
        self.state.audit.push(AuditEvent {
            kind: AuditKind::TrophyUnpinned,
            at,
            actor: self.state.owner,
            note: format!("trophy={id}"),
        });
        Ok(())
    }

    /// Iterate trophies in id order.
    pub fn list_trophies(&self) -> impl Iterator<Item = &Trophy> {
        self.state.trophies.values()
    }

    // ─── COMPANIONS ─────────────────────────────────────────────────

    /// Add a companion. Requires `HM_CAP_NPC_HIRE`.
    pub fn add_companion(&mut self, c: Companion, at: Timestamp) -> Result<(), HomeError> {
        self.require(HM_CAP_NPC_HIRE)?;
        if self.state.companions.contains_key(&c.id) {
            return Err(HomeError::DuplicateId);
        }
        let id = c.id;
        self.state.companions.insert(id, c);
        self.state.audit.push(AuditEvent {
            kind: AuditKind::CompanionAdded,
            at,
            actor: self.state.owner,
            note: "companion".into(),
        });
        Ok(())
    }

    /// Dismiss a companion. Requires `HM_CAP_NPC_HIRE`.
    pub fn dismiss_companion(&mut self, id: &Pubkey, at: Timestamp) -> Result<(), HomeError> {
        self.require(HM_CAP_NPC_HIRE)?;
        if self.state.companions.remove(id).is_none() {
            return Err(HomeError::NotFound);
        }
        self.state.audit.push(AuditEvent {
            kind: AuditKind::CompanionDismissed,
            at,
            actor: self.state.owner,
            note: "companion".into(),
        });
        Ok(())
    }

    /// Mutable access to a companion (for `converse`).
    pub fn companion_mut(&mut self, id: &Pubkey) -> Option<&mut Companion> {
        self.state.companions.get_mut(id)
    }

    /// Iterate companions in pubkey order.
    pub fn list_companions(&self) -> impl Iterator<Item = &Companion> {
        self.state.companions.values()
    }

    // ─── PORTALS ────────────────────────────────────────────────────

    /// Register a portal. The portal's own `cap_required` must be subset of
    /// the home's caps OR zero (for owner-only portals like RunStart).
    pub fn register_portal(
        &mut self,
        id: u32,
        dest: PortalDest,
        cap_required: u32,
        at: Timestamp,
    ) -> Result<(), HomeError> {
        if cap_required != 0 && !self.state.caps.has(cap_required) {
            return Err(HomeError::MissingCap {
                required: cap_required,
                granted: self.state.caps.bits(),
            });
        }
        if self.state.portals.contains_key(&id) {
            return Err(HomeError::DuplicateId);
        }
        self.state
            .portals
            .insert(id, Portal::new(id, dest, cap_required));
        self.state.audit.push(AuditEvent {
            kind: AuditKind::PortalRegistered,
            at,
            actor: self.state.owner,
            note: format!("portal={id}"),
        });
        Ok(())
    }

    /// Disable a portal. Idempotent.
    pub fn disable_portal(&mut self, id: u32, at: Timestamp) -> Result<(), HomeError> {
        let p = self.state.portals.get_mut(&id).ok_or(HomeError::NotFound)?;
        if !p.enabled {
            return Ok(());
        }
        p.enabled = false;
        self.state.audit.push(AuditEvent {
            kind: AuditKind::PortalDisabled,
            at,
            actor: self.state.owner,
            note: format!("portal={id}"),
        });
        Ok(())
    }

    /// Iterate portals in id order.
    pub fn list_portals(&self) -> impl Iterator<Item = &Portal> {
        self.state.portals.values()
    }

    // ─── FORGE ──────────────────────────────────────────────────────

    /// Queue a craft. Requires `HM_CAP_FORGE_USE`.
    pub fn forge_queue(
        &mut self,
        queue_id: u64,
        recipe: ForgeRecipeId,
        at: Timestamp,
    ) -> Result<(), HomeError> {
        self.require(HM_CAP_FORGE_USE)?;
        if self.state.forge_queue.contains_key(&queue_id) {
            return Err(HomeError::DuplicateId);
        }
        self.state
            .forge_queue
            .insert(queue_id, ForgeQueueItem::new(queue_id, recipe, at));
        self.state.audit.push(AuditEvent {
            kind: AuditKind::ForgeQueued,
            at,
            actor: self.state.owner,
            note: format!("queue={queue_id} recipe={}", recipe.0),
        });
        Ok(())
    }

    /// Cancel a queued craft. Requires `HM_CAP_FORGE_USE`.
    pub fn forge_cancel(&mut self, queue_id: u64, at: Timestamp) -> Result<(), HomeError> {
        self.require(HM_CAP_FORGE_USE)?;
        let item = self
            .state
            .forge_queue
            .get_mut(&queue_id)
            .ok_or(HomeError::NotFound)?;
        if !item.pending {
            return Ok(());
        }
        item.pending = false;
        self.state.audit.push(AuditEvent {
            kind: AuditKind::ForgeCancelled,
            at,
            actor: self.state.owner,
            note: format!("queue={queue_id}"),
        });
        Ok(())
    }

    /// Iterate forge-queue items in queue-id order.
    pub fn forge_iter(&self) -> impl Iterator<Item = &ForgeQueueItem> {
        self.state.forge_queue.values()
    }

    // ─── MEMORIAL ───────────────────────────────────────────────────

    /// Post a memorial entry (no ascriptions yet). Requires `HM_CAP_MEMORIAL_PIN`.
    pub fn post_memorial(&mut self, m: MemorialEntry, at: Timestamp) -> Result<(), HomeError> {
        self.require(HM_CAP_MEMORIAL_PIN)?;
        if self.state.memorials.contains_key(&m.id) {
            return Err(HomeError::DuplicateId);
        }
        let id = m.id;
        self.state.memorials.insert(id, m);
        self.state.audit.push(AuditEvent {
            kind: AuditKind::MemorialPosted,
            at,
            actor: self.state.owner,
            note: format!("memorial={id}"),
        });
        Ok(())
    }

    /// Append an ascription to a memorial entry.
    pub fn ascribe_memorial(
        &mut self,
        memorial_id: u64,
        ascription: MemorialAscription,
    ) -> Result<(), HomeError> {
        let entry = self
            .state
            .memorials
            .get_mut(&memorial_id)
            .ok_or(HomeError::NotFound)?;
        entry.ascriptions.push(ascription);
        Ok(())
    }

    /// Iterate memorial entries in id order.
    pub fn list_memorials(&self) -> impl Iterator<Item = &MemorialEntry> {
        self.state.memorials.values()
    }

    // ─── MYCELIAL TERMINAL ──────────────────────────────────────────

    /// Toggle the mycelial-terminal opt-in flag.
    pub fn toggle_mycelial(&mut self, at: Timestamp) {
        self.state.terminal.toggle(at);
        self.state.audit.push(AuditEvent {
            kind: AuditKind::MycelialOptToggled,
            at,
            actor: self.state.owner,
            note: format!("opted_in={}", self.state.terminal.opted_in),
        });
    }

    /// Read mycelial-terminal opt-state.
    #[must_use]
    pub fn terminal(&self) -> &MycelialTerminal {
        &self.state.terminal
    }

    // ─── INTERNAL HELPERS ───────────────────────────────────────────

    fn require(&self, cap: u32) -> Result<(), HomeError> {
        if self.state.caps.has(cap) {
            Ok(())
        } else {
            Err(HomeError::MissingCap {
                required: cap,
                granted: self.state.caps.bits(),
            })
        }
    }
}
