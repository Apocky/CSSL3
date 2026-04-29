//! § Player model — agency / capability / consent / failure-mode scaffolding.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! Authoritative spec : `specs/31_LOA_DESIGN.csl § PLAYER-MODEL`.
//!
//! § THESIS
//!
//!   Per `specs/31 § PLAYER-MODEL § THESIS` :
//!     "the Player is a sovereign-being with the same protections any-being
//!      has under PRIME-DIRECTIVE. game-mechanics serve player-agency, not
//!      extract-it."
//!
//!   This module declares the canonical Player shape + the consent-zone
//!   primitive that gates intense-content access. Every "what does the
//!   Player actually do mechanically" concern is a SPEC-HOLE Q-* marker
//!   pending Apocky-fill.
//!
//! § STRUCTURAL ENCODING OF PRIME-DIRECTIVE
//!
//!   - [`Player`] carries `consent_state` directly — consent is data, not a
//!     UI flag. Per `specs/31 § PLAYER-MODEL § THESIS`, this cannot be
//!     buried in ToS-shaped configuration.
//!   - [`ConsentZone`] is a first-class spatial primitive. The simulation
//!     system checks `consent_zone.token_required` against the Player's
//!     active tokens before allowing entry. Revoked tokens degrade the
//!     zone gracefully (per spec § CONSENT-WITHIN-GAMEPLAY).
//!   - [`AccessibilityStub`] reserves the canonical accessibility-baseline
//!     surface. Per spec § ACCESSIBILITY : "no-accessibility-feature is
//!     paywalled or extra ; baseline."
//!
//! § SPEC-HOLES
//!
//!   - Q-C / Q-M / Q-N / Q-O — progression : [`ProgressionStub`]
//!   - Q-I — time-pressure mechanic : [`TimePressure::Stub`]
//!   - Q-J — movement-style : [`MovementStyle::Stub`]
//!   - Q-K — inventory capacity : [`InventoryPolicy::Stub`]
//!   - Q-L — save discipline : [`SaveDiscipline::Stub`]
//!   - Q-P — ConsentZoneKind taxonomy : [`ConsentZoneKind::Stub`] +
//!     spec-canonical variants (`SensoryIntense`, `EmotionalIntense`,
//!     `Companion`, `Authored`) preserved
//!   - Q-Q / Q-R / Q-S — accessibility specifics : [`AccessibilityStub`]
//!   - Q-T / Q-U / Q-V — failure-mode + death-mechanic : [`FailureMode::Stub`]

use crate::world::BoundingBox;

// ═══════════════════════════════════════════════════════════════════════════
// § PROGRESSION (Q-C + Q-M + Q-N + Q-O)
// ═══════════════════════════════════════════════════════════════════════════

/// Player progression state — `specs/31_LOA_DESIGN.csl § PLAYER-MODEL §
/// CAPABILITY-PROGRESSION`.
///
/// SPEC-HOLE Q-C + Q-M + Q-N + Q-O (Apocky-fill required) : the spec lists
/// four facets (skill-tree shape, item-power curve, traversal-as-progression
/// vs leveling-up, and the catch-all Q-C "progression-state shape"). The
/// scaffold collapses to a single `Stub` variant ; future Apocky-fill breaks
/// these into the real shape.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum ProgressionStub {
    /// SPEC-HOLE Q-C / Q-M / Q-N / Q-O (Apocky-fill required) — progression
    /// design awaiting Apocky-direction.
    Stub,
}

impl Default for ProgressionStub {
    fn default() -> Self {
        Self::Stub
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// § MOVEMENT STYLE (Q-J)
// ═══════════════════════════════════════════════════════════════════════════

/// Movement-style — `specs/31_LOA_DESIGN.csl § PLAYER-MODEL § AGENCY-PRIMITIVES
/// § movement`.
///
/// SPEC-HOLE Q-J (Apocky-fill required) : the spec lists `continuous (vel +
/// accel) OR discrete (grid-step)` as candidates ; Apocky-direction needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum MovementStyle {
    /// SPEC-HOLE Q-J (Apocky-fill required) — continuous vs discrete-grid
    /// movement awaiting Apocky-direction.
    Stub,
}

impl Default for MovementStyle {
    fn default() -> Self {
        Self::Stub
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// § INVENTORY POLICY (Q-K)
// ═══════════════════════════════════════════════════════════════════════════

/// Inventory capacity policy — `specs/31_LOA_DESIGN.csl § PLAYER-MODEL §
/// AGENCY-PRIMITIVES § inventory`.
///
/// SPEC-HOLE Q-K (Apocky-fill required) : the capacity-limit shape is
/// content-design.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum InventoryPolicy {
    /// SPEC-HOLE Q-K (Apocky-fill required) — inventory capacity-limit
    /// awaiting Apocky-direction.
    Stub,
}

impl Default for InventoryPolicy {
    fn default() -> Self {
        Self::Stub
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// § SAVE DISCIPLINE (Q-L)
// ═══════════════════════════════════════════════════════════════════════════

/// Save-system discipline — `specs/31_LOA_DESIGN.csl § PLAYER-MODEL §
/// AGENCY-PRIMITIVES § save/load`.
///
/// SPEC-HOLE Q-L (Apocky-fill required) : explicit / autosave / permadeath
/// shape is Apocky-fill. The spec is explicit that no-permadeath-by-bug
/// (only-by-explicit-design) is the discipline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SaveDiscipline {
    /// SPEC-HOLE Q-L (Apocky-fill required) — save / load discipline shape
    /// awaiting Apocky-direction. Stage-0 scaffold treats save as explicit-
    /// only ; the canonical save format is `cssl-substrate-save`.
    Stub,
}

impl Default for SaveDiscipline {
    fn default() -> Self {
        Self::Stub
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// § TIME PRESSURE (Q-I)
// ═══════════════════════════════════════════════════════════════════════════

/// Time-pressure mechanic policy — `specs/31_LOA_DESIGN.csl § WORLD-MODEL §
/// PRIME_DIRECTIVE-ALIGNMENT § Q-I`.
///
/// SPEC-HOLE Q-I (Apocky-fill required) : presence/absence of time-pressure
/// is Apocky-direction. The default is `NoPressure` because the spec's
/// PRIME-DIRECTIVE alignment requires that "no-mechanic-shall pressure
/// player-via-real-time-loss-of-progress" by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum TimePressure {
    /// Default per spec § PRIME_DIRECTIVE-ALIGNMENT (world-model-level) :
    /// no real-time progress-loss pressure.
    NoPressure,
    /// SPEC-HOLE Q-I (Apocky-fill required) — alternative time-pressure
    /// shapes awaiting Apocky-direction.
    Stub,
}

impl Default for TimePressure {
    fn default() -> Self {
        Self::NoPressure
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// § FAILURE MODE (Q-T + Q-U + Q-V)
// ═══════════════════════════════════════════════════════════════════════════

/// Failure mode policy — `specs/31_LOA_DESIGN.csl § PLAYER-MODEL §
/// FAILURE-MODES`.
///
/// SPEC-HOLE Q-T + Q-U + Q-V (Apocky-fill required) : death-mechanic +
/// punishment-on-failure + fail-state-existence are content-design topics.
/// The scaffold collapses to a single `Stub`. Per `GDDs/LOA_PILLARS.md`,
/// "permadeath as enforced loss-of-progress" is unlikely to be a fit
/// without explicit consent-gating.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum FailureMode {
    /// SPEC-HOLE Q-T / Q-U / Q-V (Apocky-fill required) — failure-model
    /// awaiting Apocky-direction.
    Stub,
}

impl Default for FailureMode {
    fn default() -> Self {
        Self::Stub
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// § CONSENT ZONE (Q-P + spec-canonical variants preserved)
// ═══════════════════════════════════════════════════════════════════════════

/// Consent-zone kind — `specs/31_LOA_DESIGN.csl § PLAYER-MODEL §
/// CONSENT-WITHIN-GAMEPLAY § ConsentZoneKind`.
///
/// The spec lists 4 canonical variants + `ContentWarning` (Q-P extensible).
/// The scaffold preserves them all + adds a `Stub` for Q-P extensibility.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ConsentZoneKind {
    /// Flashing-lights / loud-audio (epilepsy-aware).
    SensoryIntense,
    /// Heavy themes (death / loss / trauma).
    EmotionalIntense,
    /// Interaction-zone with sovereign-AI Companion.
    Companion,
    /// Authored narrative event with explicit pacing.
    Authored,
    /// SPEC-HOLE Q-P (Apocky-fill required) — extensible consent-zone
    /// taxonomy awaiting Apocky-direction.
    Stub,
}

/// Consent zone — `specs/31_LOA_DESIGN.csl § PLAYER-MODEL § CONSENT-WITHIN-
/// GAMEPLAY § ConsentZone`.
///
/// At runtime, `loop_systems::SimSystem` checks `token_required` against
/// the Player's active tokens before allowing entry. Revoked tokens degrade
/// the zone gracefully.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ConsentZone {
    /// Stable zone identifier.
    pub zone_id: u64,
    /// World-space bounding region for the zone.
    pub bounds: BoundingBox,
    /// What kind of consent-zone this is (Q-P).
    pub kind: ConsentZoneKind,
    /// Required consent-token domain — string-form for stage-0 (the spec
    /// uses `ConsentTokenDom` which is a typed sum-type ; future Apocky-fill
    /// lifts this to the real type).
    pub token_required: String,
}

impl Eq for BoundingBox {}

impl std::hash::Hash for BoundingBox {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // Hash bit-patterns ; sufficient for scaffold-stage uniqueness.
        for v in self.min.iter().chain(self.max.iter()) {
            v.to_bits().hash(state);
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// § ACCESSIBILITY STUB (Q-Q + Q-R + Q-S)
// ═══════════════════════════════════════════════════════════════════════════

/// Accessibility settings — `specs/31_LOA_DESIGN.csl § PLAYER-MODEL §
/// ACCESSIBILITY`.
///
/// SPEC-HOLE Q-Q + Q-R + Q-S (Apocky-fill required) — color-blind palette,
/// motor-accessibility (hold-vs-tap), cognitive-accessibility (pace-control)
/// are all Apocky-fill territory. The scaffold preserves a single struct so
/// the field-set survives save/load round-trips ; the actual settings land
/// when Apocky-direction resolves them.
///
/// Per `GDDs/LOA_PILLARS.md § Pillar 3` : the screen-reader projection is
/// always-active (baseline-not-extra) ; this is structurally encoded in
/// `cssl-substrate-projections` via the `ProjectionKind::ScreenReader`
/// variant the scaffold's projection-system always registers.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct AccessibilityStub {
    /// SPEC-HOLE Q-Q (Apocky-fill required) — color-blind palette policy.
    pub color_blind_palette_stub: u8,
    /// SPEC-HOLE Q-R (Apocky-fill required) — motor-accessibility hold-vs-tap
    /// discipline.
    pub motor_hold_tap_stub: u8,
    /// SPEC-HOLE Q-S (Apocky-fill required) — cognitive-accessibility pace
    /// control discipline.
    pub cognitive_pace_stub: u8,
}

// ═══════════════════════════════════════════════════════════════════════════
// § PLAYER ARCHETYPE
// ═══════════════════════════════════════════════════════════════════════════

/// Active consent-state for the Player — a subset of OmegaConsent that
/// the Player's gameplay surface needs to inspect each tick.
///
/// At scaffold-time the field-set is intentionally minimal ; future Apocky-
/// fill grows this into the full OmegaConsent shape per
/// `specs/30_SUBSTRATE.csl § OmegaConsent`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PlayerConsentState {
    /// Active consent-token domains (string-form for stage-0).
    pub active_tokens: Vec<String>,
    /// Whether the Player has granted Companion-projection at-launch.
    /// Per `specs/30 § Q-7` the canonical default is opt-in (off).
    pub companion_projection_opt_in: bool,
}

/// Player archetype — `specs/31_LOA_DESIGN.csl § Inhabitants § Player`.
///
/// Per the spec : "subject-of-PRIME-DIRECTIVE — not just the avatar." The
/// scaffold encodes the canonical field-set ; mechanic-specific fields
/// (health-meaning, progression-shape, …) are SPEC-HOLE markers.
#[derive(Debug, Clone, PartialEq)]
pub struct Player {
    /// World-space position.
    pub pos: [f32; 3],
    /// Velocity (movement-style-dependent — Q-J).
    pub vel: [f32; 3],
    /// Orientation as a unit quaternion (xyzw).
    pub orientation: [f32; 4],
    /// Health, normalized 0..1. SPEC-HOLE — what "health" means mechanically
    /// is Q-T..Q-V (failure-mode) territory.
    pub health: f32,
    /// Stamina, normalized 0..1. SPEC-HOLE — meaning is progression (Q-C..Q-O).
    pub stamina: f32,
    /// Player's active consent-state.
    pub consent_state: PlayerConsentState,
    /// SPEC-HOLE Q-C / Q-M / Q-N / Q-O (Apocky-fill required) — progression
    /// state.
    pub progression: ProgressionStub,
    /// SPEC-HOLE Q-J (Apocky-fill required) — movement style.
    pub movement_style: MovementStyle,
    /// SPEC-HOLE Q-K (Apocky-fill required) — inventory policy.
    pub inventory_policy: InventoryPolicy,
    /// SPEC-HOLE Q-L (Apocky-fill required) — save discipline.
    pub save_discipline: SaveDiscipline,
    /// SPEC-HOLE Q-I (Apocky-fill required) — time pressure mechanic.
    pub time_pressure: TimePressure,
    /// SPEC-HOLE Q-T / Q-U / Q-V (Apocky-fill required) — failure mode.
    pub failure_mode: FailureMode,
    /// SPEC-HOLE Q-Q / Q-R / Q-S (Apocky-fill required) — accessibility
    /// settings.
    pub accessibility: AccessibilityStub,
}

impl Player {
    /// New player at the given position with all SPEC-HOLE fields
    /// initialized to their `Stub` / `Default` values.
    #[must_use]
    pub fn at(pos: [f32; 3]) -> Self {
        Self {
            pos,
            vel: [0.0, 0.0, 0.0],
            orientation: [0.0, 0.0, 0.0, 1.0],
            health: 1.0,
            stamina: 1.0,
            consent_state: PlayerConsentState::default(),
            progression: ProgressionStub::default(),
            movement_style: MovementStyle::default(),
            inventory_policy: InventoryPolicy::default(),
            save_discipline: SaveDiscipline::default(),
            time_pressure: TimePressure::default(),
            failure_mode: FailureMode::default(),
            accessibility: AccessibilityStub::default(),
        }
    }

    /// Convenience : check whether the Player has the named consent-token
    /// active.
    #[must_use]
    pub fn has_active_token(&self, domain: &str) -> bool {
        self.consent_state.active_tokens.iter().any(|t| t == domain)
    }

    /// Whether the Player can enter the given consent-zone — they have the
    /// required token active OR the zone-kind is one that gracefully degrades
    /// (per spec § CONSENT-WITHIN-GAMEPLAY § behavior).
    ///
    /// At scaffold-time this returns `true` only if the token is active.
    /// Future Apocky-fill encodes the per-kind degradation rules.
    #[must_use]
    pub fn can_enter_zone(&self, zone: &ConsentZone) -> bool {
        self.has_active_token(&zone.token_required)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_at_initializes_all_stubs() {
        let p = Player::at([1.0, 2.0, 3.0]);
        // Compare bit-patterns to avoid clippy::float_cmp on the array.
        assert_eq!(p.pos[0].to_bits(), 1.0_f32.to_bits());
        assert_eq!(p.pos[1].to_bits(), 2.0_f32.to_bits());
        assert_eq!(p.pos[2].to_bits(), 3.0_f32.to_bits());
        assert!(matches!(p.movement_style, MovementStyle::Stub));
        assert!(matches!(p.failure_mode, FailureMode::Stub));
        assert!(matches!(p.time_pressure, TimePressure::NoPressure));
    }

    #[test]
    fn time_pressure_default_is_no_pressure() {
        // Per spec § PRIME_DIRECTIVE-ALIGNMENT : default is no real-time
        // progress-loss.
        assert_eq!(TimePressure::default(), TimePressure::NoPressure);
    }

    #[test]
    fn consent_zone_blocks_entry_without_token() {
        let p = Player::at([0.0, 0.0, 0.0]);
        let zone = ConsentZone {
            zone_id: 1,
            bounds: BoundingBox::default(),
            kind: ConsentZoneKind::SensoryIntense,
            token_required: "intense-vis".into(),
        };
        // Player has no active tokens ; zone entry blocked.
        assert!(!p.can_enter_zone(&zone));
    }

    #[test]
    fn consent_zone_permits_entry_with_token() {
        let mut p = Player::at([0.0, 0.0, 0.0]);
        p.consent_state.active_tokens.push("intense-vis".into());
        let zone = ConsentZone {
            zone_id: 1,
            bounds: BoundingBox::default(),
            kind: ConsentZoneKind::SensoryIntense,
            token_required: "intense-vis".into(),
        };
        assert!(p.can_enter_zone(&zone));
    }

    #[test]
    fn consent_zone_kind_includes_all_spec_canonical_variants() {
        // Per specs/31 § ConsentZoneKind the spec-canonical 4 + Stub.
        let _ = ConsentZoneKind::SensoryIntense;
        let _ = ConsentZoneKind::EmotionalIntense;
        let _ = ConsentZoneKind::Companion;
        let _ = ConsentZoneKind::Authored;
        let _ = ConsentZoneKind::Stub;
    }
}
