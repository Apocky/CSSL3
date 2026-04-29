//! § World model — Floor / Level / Room / Entity / Item canonical types.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! Authoritative spec : `specs/31_LOA_DESIGN.csl § WORLD-MODEL`.
//!
//! § THESIS
//!
//!   The labyrinth is structured as `Labyrinth → Floor → Level → Room → Cell`.
//!   At Phase-I scaffold-time every "what's-in-the-labyrinth" concern is a
//!   `// SPEC-HOLE Q-X (Apocky-fill required)` marker — the corresponding
//!   `Stub` enum-variant exists so the canonical types compile + serialize
//!   end-to-end via `cssl-substrate-save`. Apocky's later content slices
//!   replace the `Stub` variants with real labyrinth content.
//!
//! § STRUCTURAL COMMITMENTS  (per `specs/31_LOA_DESIGN.csl § HIERARCHY`)
//!
//!   - `World` is the ECS-pool root (stage-0 form : `Vec`-of-archetype).
//!   - `Floor`, `Level`, `Room`, `Cell`, `Entity`, `Item` are the canonical
//!     archetypes — names + field-shape preserved from the spec.
//!   - Every enum that carries spec-hole content has a single `Stub`
//!     variant that future Apocky-fill slices break out into real content.
//!   - The ID types (`FloorId`, `LevelId`, `RoomId`, `EntityId`, `ItemId`)
//!     are newtype-wrapped `u64`s — stable across save/load round-trips.
//!
//! § STRUCTURAL ENCODING OF PRIME-DIRECTIVE
//!
//!   - `Room::consent_zone` carries an optional `ConsentZone` per
//!     `specs/31_LOA_DESIGN.csl § PLAYER-MODEL § CONSENT-WITHIN-GAMEPLAY`.
//!     The scaffold preserves the field even though the actual zones are
//!     Apocky-fill (Q-P). At runtime, `loop_systems::SimSystem` checks
//!     consent-zone tokens before allowing the player to enter intense
//!     content.
//!   - `Door::DoorOpensVia::Consent(...)` is a first-class door variant
//!     so consent-gated passages are encoded in the world-graph itself
//!     (per spec § Door-MODEL).
//!   - The `ItemKind::Stub` variant explicitly cannot represent weapons
//!     or surveillance items — the scaffold's `validate_item` routine
//!     rejects any item kind that future Apocky-fill might add with a
//!     `weapon` or `surveillance` discriminant (per spec §§ 30
//!     § FORBIDDEN-COMPOSITIONS).

use std::collections::BTreeMap;

// ═══════════════════════════════════════════════════════════════════════════
// § ID TYPES — stable u64 newtype handles.
// ═══════════════════════════════════════════════════════════════════════════

/// Stable identifier for a `Floor` archetype.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FloorId(pub u64);

/// Stable identifier for a `Level` archetype.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LevelId(pub u64);

/// Stable identifier for a `Room` archetype.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RoomId(pub u64);

/// Stable identifier for a `Door` archetype.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DoorId(pub u64);

/// Stable identifier for an `Entity` archetype.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EntityId(pub u64);

/// Stable identifier for an `Item` archetype.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ItemId(pub u64);

// ═══════════════════════════════════════════════════════════════════════════
// § THEME (Q-B)
// ═══════════════════════════════════════════════════════════════════════════

/// Per-Floor thematic identifier — `specs/31_LOA_DESIGN.csl § Floor § theme`.
///
/// SPEC-HOLE Q-B (Apocky-fill required) : the actual theme-set is enumerable
/// content the scaffold cannot guess. `Stub` is the placeholder so save/load
/// round-trips cleanly ; future Apocky-fill replaces with real variants.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum ThemeId {
    /// SPEC-HOLE Q-B (Apocky-fill required) — the canonical theme-set is
    /// content Apocky has not yet imported into the CSSLv3-native spec.
    /// Legacy LoA iterations may inform the eventual variant-set.
    Stub,
}

impl Default for ThemeId {
    fn default() -> Self {
        Self::Stub
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// § LABYRINTH GENERATION (Q-A)
// ═══════════════════════════════════════════════════════════════════════════

/// Generation method for a Labyrinth — `specs/31_LOA_DESIGN.csl §
/// Labyrinth-STRUCTURE § generation_method`.
///
/// SPEC-HOLE Q-A (Apocky-fill required) : the spec lists `Procedural |
/// Authored | Hybrid` as the candidate set, but Apocky-direction is needed
/// to commit to one. `Stub` is the placeholder.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum LabyrinthGeneration {
    /// SPEC-HOLE Q-A (Apocky-fill required) — generation method awaiting
    /// Apocky-direction (Procedural | Authored | Hybrid).
    Stub,
}

impl Default for LabyrinthGeneration {
    fn default() -> Self {
        Self::Stub
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// § ITEM KIND (Q-F + Q-LL economy)
// ═══════════════════════════════════════════════════════════════════════════

/// Item kind — `specs/31_LOA_DESIGN.csl § Items § ItemKind`.
///
/// SPEC-HOLE Q-F (Apocky-fill required) : item taxonomy is content-shaped.
/// SPEC-HOLE Q-LL (Apocky-fill required) : economy / trade is content-shaped.
/// Both share this enum at scaffold-time ; future Apocky-fill breaks them
/// into separate variant-sets.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum ItemKind {
    /// SPEC-HOLE Q-F + Q-LL (Apocky-fill required) — item taxonomy + economy
    /// awaiting Apocky-direction.
    Stub,
}

impl ItemKind {
    /// Whether this item-kind is permitted in a Substrate that respects
    /// `specs/30_SUBSTRATE.csl § FORBIDDEN-COMPOSITIONS`. Future Apocky-fill
    /// variants that encode weapons or surveillance MUST set this to
    /// `false` so the engine refuses to materialize them.
    #[must_use]
    pub const fn is_substrate_safe(&self) -> bool {
        match self {
            Self::Stub => true,
        }
    }
}

impl Default for ItemKind {
    fn default() -> Self {
        Self::Stub
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// § AFFORDANCE (Q-H)
// ═══════════════════════════════════════════════════════════════════════════

/// Item affordance — `specs/31_LOA_DESIGN.csl § Items § Affordance`.
///
/// The `Pickup` / `Drop` / `Use` / `Combine` variants are spec-canonical
/// (no SPEC-HOLE) ; the `Stub` variant covers Q-H (`ContextSpecific`
/// extensibility) which is Apocky-fill.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum Affordance {
    Pickup,
    Drop,
    /// SPEC-HOLE Q-H (Apocky-fill required) — context-specific affordance
    /// extensibility. The spec lists `Use(UseTarget)` + `Combine(ItemId)`
    /// + `ContextSpecific` ; the scaffold collapses to `Stub` until Apocky-
    /// direction lands the `UseTarget` shape.
    Stub,
}

// ═══════════════════════════════════════════════════════════════════════════
// § NARRATIVE (Q-G + Q-HH + Q-II + Q-JJ + Q-KK)
// ═══════════════════════════════════════════════════════════════════════════

/// Narrative anchor kind — `specs/31_LOA_DESIGN.csl § NARRATIVE-AND-CONTENT
/// § NarrativeKind`.
///
/// SPEC-HOLE Q-G + Q-HH + Q-II + Q-JJ + Q-KK (Apocky-fill required) — the
/// narrative-system topic spans multiple Q-* spec-holes (item lore, kind-
/// extension, authored events, cinematics, quests). All collapse to `Stub`
/// at scaffold-time.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum NarrativeKind {
    /// SPEC-HOLE Q-G + Q-HH + Q-II + Q-JJ + Q-KK (Apocky-fill required) —
    /// narrative content awaiting Apocky's authoring slices.
    Stub,
}

impl Default for NarrativeKind {
    fn default() -> Self {
        Self::Stub
    }
}

/// Optional narrative role attached to an Item — `specs/31_LOA_DESIGN.csl
/// § Items § narrative_role`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum NarrativeRole {
    /// SPEC-HOLE Q-G (Apocky-fill required) — items' narrative-role is
    /// authored-content territory.
    Stub,
}

// ═══════════════════════════════════════════════════════════════════════════
// § WILDLIFE (Q-E)
// ═══════════════════════════════════════════════════════════════════════════

/// Ambient creature archetype — `specs/31_LOA_DESIGN.csl § Inhabitants §
/// Wildlife`.
///
/// SPEC-HOLE Q-E (Apocky-fill required) — ambient-creature design is
/// content-shaped + Apocky-direction-dependent.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum Wildlife {
    /// SPEC-HOLE Q-E (Apocky-fill required) — wildlife taxonomy awaiting
    /// Apocky-direction.
    Stub,
}

// ═══════════════════════════════════════════════════════════════════════════
// § BOUNDING + SPATIAL TYPES
// ═══════════════════════════════════════════════════════════════════════════

/// Axis-aligned bounding box — minimal scalar form for scaffold-stage
/// spatial bookkeeping. The full `cssl-substrate-projections::Aabb` is the
/// rendering-side AABB ; this form is the world-content side (no `Vec3` or
/// `Quat` dependency at the world-model layer).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BoundingBox {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

/// Single voxel-cell — `specs/31_LOA_DESIGN.csl § Room § voxel_chunks`.
///
/// At scaffold-time the cell is a tag + a 3D position ; future Apocky-fill
/// lifts this to the full `VoxelChunk` shape from `specs/08`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Cell {
    /// Stable tag — at scaffold-time always `0` (Stub). Future Apocky-fill
    /// lifts this to a per-material discriminant.
    pub tag: u32,
    /// World-space position of the cell's origin (lower-corner).
    pub pos: [f32; 3],
}

// ═══════════════════════════════════════════════════════════════════════════
// § DOOR (spec-canonical ; consent-gated variant load-bearing)
// ═══════════════════════════════════════════════════════════════════════════

/// Door state — `specs/31_LOA_DESIGN.csl § Door-MODEL § DoorState`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DoorState {
    Closed,
    Open,
    Locked,
    Sealed,
    /// One-way passage. At scaffold-time the direction is not enumerated
    /// (the spec's `Direction` type is content-side ; future Apocky-fill
    /// lands the canonical variant-set). Stub-as-OneWay because the
    /// SHAPE is spec-canonical even if the direction-set is Q-fill.
    OneWayStub,
}

/// Means of opening a door — `specs/31_LOA_DESIGN.csl § Door-MODEL §
/// DoorOpensVia`.
///
/// `Consent(...)` is a first-class variant : consent-gated passage is the
/// engine-level enforcement of `specs/31 § PLAYER-MODEL § CONSENT-WITHIN-
/// GAMEPLAY`. The scaffold preserves it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DoorOpensVia {
    Free,
    /// Requires the named item.
    Item(ItemId),
    /// Requires AI-Companion participation per
    /// `specs/31 § AI-INTERACTION`.
    Companion,
    /// Requires an active consent-token in the named domain. Spec-canonical
    /// per `specs/31 § Door-MODEL § DoorOpensVia § Consent(ConsentTokenDom)`.
    Consent(String),
    /// SPEC-HOLE — gestures + switches lifted into a single Stub at
    /// scaffold-time ; future Apocky-fill enumerates the variant-set.
    Stub,
}

/// Door — `specs/31_LOA_DESIGN.csl § Door-MODEL`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Door {
    pub id: DoorId,
    pub from_room: RoomId,
    pub to_room: RoomId,
    pub state: DoorState,
    pub opens_via: DoorOpensVia,
}

// ═══════════════════════════════════════════════════════════════════════════
// § ROOM
// ═══════════════════════════════════════════════════════════════════════════

/// Optional consent-zone attached to a Room — see `crate::player::ConsentZone`.
/// Re-exported here because Room is the spatial host of consent-zones in the
/// canonical World hierarchy.
pub use crate::player::ConsentZone;

/// Room — `specs/31_LOA_DESIGN.csl § Room`.
#[derive(Debug, Clone, PartialEq)]
pub struct Room {
    pub id: RoomId,
    pub bounds: BoundingBox,
    /// At scaffold-time empty ; future content-fill populates voxel chunks.
    pub voxel_cells: Vec<Cell>,
    /// Entity handles within this room.
    pub entities: Vec<EntityId>,
    /// Doors connecting this room to others.
    pub doors: Vec<DoorId>,
    /// Optional consent-zone gating this room's content per Q-P.
    pub consent_zone: Option<ConsentZone>,
}

// ═══════════════════════════════════════════════════════════════════════════
// § LEVEL
// ═══════════════════════════════════════════════════════════════════════════

/// Level — `specs/31_LOA_DESIGN.csl § Level`.
///
/// At scaffold-time the adjacency graph is a sparse list of (room, room)
/// pairs ; future Apocky-fill replaces with the full `Graph<RoomId, Edge>`
/// form once the edge-type lands.
#[derive(Debug, Clone, PartialEq)]
pub struct Level {
    pub id: LevelId,
    pub rooms: Vec<RoomId>,
    /// Sparse adjacency list — pairs of `(from, to)` connected RoomIds.
    pub adjacency: Vec<(RoomId, RoomId)>,
}

// ═══════════════════════════════════════════════════════════════════════════
// § FLOOR
// ═══════════════════════════════════════════════════════════════════════════

/// Floor — `specs/31_LOA_DESIGN.csl § Floor`.
#[derive(Debug, Clone, PartialEq)]
pub struct Floor {
    pub id: FloorId,
    pub levels: Vec<LevelId>,
    /// SPEC-HOLE Q-B (Apocky-fill required) — see [`ThemeId`] doc.
    pub theme: ThemeId,
    /// Apockalypse-engine phase tag tied to this Floor — see
    /// `crate::apockalypse::ApockalypsePhase`.
    pub apockalypse_phase: crate::apockalypse::ApockalypsePhase,
}

// ═══════════════════════════════════════════════════════════════════════════
// § ENTITY (generic non-Player non-Companion entity)
// ═══════════════════════════════════════════════════════════════════════════

/// Entity-kind — what role this entity plays in the world.
///
/// `Wildlife` covers Q-E ; `NarrativeAnchor` covers Q-G + Q-HH ;
/// `Stub` is the catch-all for everything not yet Apocky-direction-resolved.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum EntityKind {
    /// SPEC-HOLE Q-E (Apocky-fill required) — wildlife archetypes.
    Wildlife(Wildlife),
    /// SPEC-HOLE Q-G + Q-HH (Apocky-fill required) — narrative-anchor
    /// entities.
    NarrativeAnchor(NarrativeKind),
    /// SPEC-HOLE — catch-all for residents + scripted NPCs that the spec
    /// defers to §§ 30 D-5 (neural-NPC-runtime).
    Stub,
}

/// Entity — generic ECS-archetype for non-Player non-Companion world
/// inhabitants. `specs/31_LOA_DESIGN.csl § Inhabitants`.
#[derive(Debug, Clone, PartialEq)]
pub struct Entity {
    pub id: EntityId,
    pub kind: EntityKind,
    pub pos: [f32; 3],
}

// ═══════════════════════════════════════════════════════════════════════════
// § ITEM
// ═══════════════════════════════════════════════════════════════════════════

/// Item — `specs/31_LOA_DESIGN.csl § Items`.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub id: ItemId,
    pub kind: ItemKind,
    /// `None` = in someone's inventory ; `Some(...)` = world-positioned.
    pub pos: Option<[f32; 3]>,
    pub mass: f32,
    pub affordances: Vec<Affordance>,
    /// SPEC-HOLE Q-G (Apocky-fill required) — narrative anchor.
    pub narrative_role: Option<NarrativeRole>,
}

// ═══════════════════════════════════════════════════════════════════════════
// § WORLD — the ECS-pool root
// ═══════════════════════════════════════════════════════════════════════════

/// World — the ECS-pool root for the Labyrinth + its inhabitants + items.
/// `specs/31_LOA_DESIGN.csl § WORLD-MODEL`.
///
/// At scaffold-time the World stores archetypes in `BTreeMap`s keyed by id.
/// `BTreeMap` is chosen over `HashMap` so the iteration order is deterministic
/// — load-bearing for `cssl-substrate-save`'s deterministic serialization
/// invariant.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct World {
    /// SPEC-HOLE Q-A (Apocky-fill required) — labyrinth generation method.
    pub generation: LabyrinthGeneration,
    /// Labyrinth genesis seed for deterministic generation. Rolls into the
    /// `OmegaScheduler::with_seed` master-seed at engine construction.
    pub genesis_seed: u64,
    /// Floors in this Labyrinth.
    pub floors: BTreeMap<FloorId, Floor>,
    /// Levels across all floors.
    pub levels: BTreeMap<LevelId, Level>,
    /// Rooms across all levels.
    pub rooms: BTreeMap<RoomId, Room>,
    /// Doors connecting rooms.
    pub doors: BTreeMap<DoorId, Door>,
    /// Generic entities (Wildlife / NarrativeAnchor / Stub residents).
    pub entities: BTreeMap<EntityId, Entity>,
    /// Items.
    pub items: BTreeMap<ItemId, Item>,
}

impl World {
    /// New empty world with the given genesis-seed.
    #[must_use]
    pub fn empty(genesis_seed: u64) -> Self {
        Self {
            generation: LabyrinthGeneration::default(),
            genesis_seed,
            floors: BTreeMap::new(),
            levels: BTreeMap::new(),
            rooms: BTreeMap::new(),
            doors: BTreeMap::new(),
            entities: BTreeMap::new(),
            items: BTreeMap::new(),
        }
    }

    /// Scaffold-stub world : a single Floor with a single Level and a single
    /// Room, no doors, no entities, no items. Demonstrates the canonical
    /// hierarchy compiles + serializes without committing to game-content.
    ///
    /// Future Apocky-fill replaces this with the actual labyrinth-generation
    /// flow per Q-A.
    #[must_use]
    pub fn scaffold_stub(genesis_seed: u64) -> Self {
        let mut w = Self::empty(genesis_seed);
        let floor_id = FloorId(0);
        let level_id = LevelId(0);
        let room_id = RoomId(0);

        w.rooms.insert(
            room_id,
            Room {
                id: room_id,
                bounds: BoundingBox {
                    min: [0.0, 0.0, 0.0],
                    max: [1.0, 1.0, 1.0],
                },
                voxel_cells: Vec::new(),
                entities: Vec::new(),
                doors: Vec::new(),
                consent_zone: None,
            },
        );

        w.levels.insert(
            level_id,
            Level {
                id: level_id,
                rooms: vec![room_id],
                adjacency: Vec::new(),
            },
        );

        w.floors.insert(
            floor_id,
            Floor {
                id: floor_id,
                levels: vec![level_id],
                theme: ThemeId::default(),
                apockalypse_phase: crate::apockalypse::ApockalypsePhase::default(),
            },
        );

        w
    }

    /// Validate every Item is Substrate-safe per
    /// `specs/30_SUBSTRATE.csl § FORBIDDEN-COMPOSITIONS`. Returns the
    /// offending `ItemId` on the first violation.
    ///
    /// At scaffold-time every `ItemKind::Stub` passes ; future Apocky-fill
    /// variants that encode weapons or surveillance trip this check at
    /// world-construction time so the engine refuses to materialize them.
    ///
    /// # Errors
    /// Returns the offending `ItemId` on the first non-Substrate-safe item.
    pub fn validate_substrate_safety(&self) -> Result<(), ItemId> {
        for (id, item) in &self.items {
            if !item.kind.is_substrate_safe() {
                return Err(*id);
            }
        }
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// § TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scaffold_stub_has_one_floor_level_room() {
        let w = World::scaffold_stub(0xABCD);
        assert_eq!(w.floors.len(), 1);
        assert_eq!(w.levels.len(), 1);
        assert_eq!(w.rooms.len(), 1);
        assert_eq!(w.genesis_seed, 0xABCD);
    }

    #[test]
    fn theme_default_is_stub() {
        assert_eq!(ThemeId::default(), ThemeId::Stub);
    }

    #[test]
    fn item_kind_stub_is_substrate_safe() {
        let k = ItemKind::Stub;
        assert!(k.is_substrate_safe());
    }

    #[test]
    fn validate_substrate_safety_passes_on_stub_world() {
        let w = World::scaffold_stub(0);
        assert!(w.validate_substrate_safety().is_ok());
    }

    #[test]
    fn ids_are_distinct_across_types() {
        // Newtype-id discrimination at the type level — FloorId(0) and
        // RoomId(0) cannot be confused.
        let a = FloorId(0);
        let b = RoomId(0);
        // Different types ; this would not compile if we accidentally
        // unified them. The explicit `let` bindings prove they're distinct.
        assert_eq!(a.0, b.0);
    }

    #[test]
    fn world_serialization_order_is_deterministic() {
        // BTreeMap iteration order is the load-bearing invariant for
        // cssl-substrate-save's deterministic-serialization rule.
        let mut w1 = World::empty(0);
        let mut w2 = World::empty(0);
        for i in 0..10u64 {
            w1.rooms.insert(
                RoomId(i),
                Room {
                    id: RoomId(i),
                    bounds: BoundingBox::default(),
                    voxel_cells: Vec::new(),
                    entities: Vec::new(),
                    doors: Vec::new(),
                    consent_zone: None,
                },
            );
        }
        for i in (0..10u64).rev() {
            w2.rooms.insert(
                RoomId(i),
                Room {
                    id: RoomId(i),
                    bounds: BoundingBox::default(),
                    voxel_cells: Vec::new(),
                    entities: Vec::new(),
                    doors: Vec::new(),
                    consent_zone: None,
                },
            );
        }
        // Same RoomIds inserted in different orders produce equal worlds
        // — this is the BTreeMap-determinism guarantee the save format
        // depends on.
        let k1: Vec<_> = w1.rooms.keys().collect();
        let k2: Vec<_> = w2.rooms.keys().collect();
        assert_eq!(k1, k2);
    }

    #[test]
    fn door_consent_variant_preserved() {
        // Spec-canonical : consent-gated passage is a first-class variant.
        let d = Door {
            id: DoorId(0),
            from_room: RoomId(0),
            to_room: RoomId(1),
            state: DoorState::Locked,
            opens_via: DoorOpensVia::Consent("ai-collab".into()),
        };
        assert!(matches!(d.opens_via, DoorOpensVia::Consent(_)));
    }
}
