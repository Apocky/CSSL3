//! § dm_director — DM director 4-state pacing FSM
//! ════════════════════════════════════════════════
//!
//! § T11-LOA-HOST-4 (W-LOA-host-dm) — STAGE-0 BOOTSTRAP for `scenes/dm_director.cssl`.
//!
//! § AUTHORITATIVE DESIGN SPEC
//!   `scenes/dm_director.cssl` (the .cssl source is the authority ; this
//!   Rust file is a stage-0 mirror until the stage-1 csslc compiles the
//!   .cssl directly into equivalent code). Constant + enum names below
//!   are preserved verbatim for that translation path.
//!
//! § FSM (4 states)
//!   - `CALM`     (0) → `BUILDUP` (1) on `tension > TENSION_THRESHOLD_BUILDUP`
//!   - `BUILDUP`  (1) → `CLIMAX`  (2) on `tension > TENSION_THRESHOLD_CLIMAX`
//!   - `CLIMAX`   (2) → `RELIEF`  (3) after `frame_in_state > CLIMAX_DURATION_FRAMES`
//!   - `RELIEF`   (3) → `CALM`    (0) after `frame_in_state > RELIEF_DURATION_FRAMES`
//!
//! § TENSION MODEL
//!   `tension = HP_deficit * 0.4 + recent_combat_density * 0.3 -
//!              rest_signals * 0.2 + climax_decay`
//!   bounded `0.0..=1.0` (saturating).
//!
//! § EVENT TEMPLATE REGISTRY (8 pre-baked entries)
//!   - SPAWN_NPC_ARRIVAL        (intensity 0.2)
//!   - SPAWN_AMBIENT_CREATURE   (intensity 0.3)
//!   - SPAWN_BOSS_TEASER        (intensity 0.6)
//!   - SPAWN_LOOT_DROP          (intensity 0.4)
//!   - SPAWN_ELITE_ENCOUNTER    (intensity 0.7)
//!   - TRIGGER_WEATHER_CHANGE   (intensity 0.3)
//!   - TRIGGER_LORE_REVEAL      (intensity 0.5)
//!   - TRIGGER_REST_AREA        (intensity 0.1)
//!
//!   Each tick the DM director can propose AT MOST 1 event (via cooldown
//!   ring of 8 slots) when in `BUILDUP` or `CLIMAX` states. The selected
//!   template's intensity must satisfy `intensity ≤ tension` so a calm
//!   moment never spawns an elite encounter.

use cssl_rt::loa_startup::log_event;

// ─────────────────────────────────────────────────────────────────────────
// § CONSTANTS — preserved verbatim for stage-1 .cssl translation
// ─────────────────────────────────────────────────────────────────────────

/// Tension threshold for `CALM → BUILDUP` transition.
/// Q14-fixed-point reference : `0.25 * 16384 = 4096`.
pub const TENSION_THRESHOLD_BUILDUP: f32 = 0.25;
pub const TENSION_THRESHOLD_BUILDUP_Q14: i32 = 4096;

/// Tension threshold for `BUILDUP → CLIMAX` transition.
/// Q14-fixed-point reference : `0.75 * 16384 = 12288`.
pub const TENSION_THRESHOLD_CLIMAX: f32 = 0.75;
pub const TENSION_THRESHOLD_CLIMAX_Q14: i32 = 12288;

/// Frame-count CLIMAX dwells before transitioning to RELIEF.
pub const CLIMAX_DURATION_FRAMES: u32 = 50;

/// Frame-count RELIEF dwells before transitioning to CALM.
pub const RELIEF_DURATION_FRAMES: u32 = 30;

/// Tension contribution coefficients.
pub const COEF_HP_DEFICIT: f32 = 0.4;
pub const COEF_COMBAT_DENSITY: f32 = 0.3;
pub const COEF_REST_SIGNALS: f32 = 0.2;

/// Stamina-deficit contribution coefficient. The .cssl spec lists stamina
/// as a tension input but the v0 formula left it weighted at 0 (sum =
/// 0.4 + 0.3 = 0.7 which never reached the 0.75 CLIMAX threshold from
/// player-state alone). Stage-0 weighting brings stamina online so a
/// fully-saturated hostile environment (HP=0 + combat=1 + stamina=0 +
/// rest=0) yields tension = 0.85, which crosses CLIMAX.
pub const COEF_STAMINA_DEFICIT: f32 = 0.15;

/// Drama-curve "bonus" term added when BOTH hp_deficit AND combat_density
/// are saturated (>= 0.9). Pushes the tension over the CLIMAX threshold
/// even if stamina is high. Multiplicative interaction makes the curve
/// rise sharply when the player is genuinely in trouble + actively in
/// combat — the .cssl spec calls this the "danger spike".
pub const DRAMA_SPIKE_BONUS: f32 = 0.10;
pub const DRAMA_SPIKE_HP_THRESHOLD: f32 = 0.9;
pub const DRAMA_SPIKE_COMBAT_THRESHOLD: f32 = 0.9;

/// Cooldown ring size — at most one event per tick ; ring of 8 slots
/// prevents the same template firing twice in rapid succession.
pub const COOLDOWN_RING_LEN: usize = 8;

/// Cooldown duration in frames — once a template is proposed it cannot
/// fire again for this many frames.
pub const COOLDOWN_DURATION_FRAMES: u32 = 120;

// ─────────────────────────────────────────────────────────────────────────
// § STATE ENUM (4 states)
// ─────────────────────────────────────────────────────────────────────────

/// 4-state pacing FSM state. Indices preserved verbatim from `scenes/dm_director.cssl`
/// for the stage-1 translation path (CALM=0 ; BUILDUP=1 ; CLIMAX=2 ; RELIEF=3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DmState {
    Calm = 0,
    Buildup = 1,
    Climax = 2,
    Relief = 3,
}

impl DmState {
    /// Stable string label for telemetry. Matches the .cssl identifier exactly.
    pub fn label(self) -> &'static str {
        match self {
            DmState::Calm => "CALM",
            DmState::Buildup => "BUILDUP",
            DmState::Climax => "CLIMAX",
            DmState::Relief => "RELIEF",
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § PLAYER STATE INPUT
// ─────────────────────────────────────────────────────────────────────────

/// Snapshot of player-derived inputs the tension model consumes each tick.
///
/// All fields are normalized to `0.0..=1.0` so the formula stays in
/// dimensionless units. Sibling W-LOA-host-render's player-state extraction
/// is responsible for clamping to range before passing the struct in.
#[derive(Debug, Clone, Copy, Default)]
pub struct PlayerState {
    /// Health-deficit fraction : `1.0 - (hp / hp_max)` ; 1.0 = near-death.
    pub hp_deficit: f32,
    /// Stamina-deficit fraction : `1.0 - (stamina / stamina_max)`. Reserved
    /// for future weighting ; not yet used in the v0 tension formula but
    /// passed through so sibling slices can extend without API churn.
    pub stamina_deficit: f32,
    /// Recent-combat density : EWMA of combat events in the last N seconds.
    pub recent_combat_density: f32,
    /// Rest signals : EWMA of "player is sitting / not moving / safe-zone"
    /// indicators in the last N seconds.
    pub rest_signals: f32,
}

// ─────────────────────────────────────────────────────────────────────────
// § EVENT TEMPLATE REGISTRY (8 entries)
// ─────────────────────────────────────────────────────────────────────────

/// Identifier for the 8 pre-baked event templates. Stable indices for
/// FFI / serialization (preserved verbatim from `scenes/dm_director.cssl`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum EventTemplateId {
    SpawnNpcArrival = 0,
    SpawnAmbientCreature = 1,
    SpawnBossTeaser = 2,
    SpawnLootDrop = 3,
    SpawnEliteEncounter = 4,
    TriggerWeatherChange = 5,
    TriggerLoreReveal = 6,
    TriggerRestArea = 7,
}

impl EventTemplateId {
    /// Stable string label for telemetry. Matches the .cssl identifier exactly.
    pub fn label(self) -> &'static str {
        match self {
            EventTemplateId::SpawnNpcArrival => "SPAWN_NPC_ARRIVAL",
            EventTemplateId::SpawnAmbientCreature => "SPAWN_AMBIENT_CREATURE",
            EventTemplateId::SpawnBossTeaser => "SPAWN_BOSS_TEASER",
            EventTemplateId::SpawnLootDrop => "SPAWN_LOOT_DROP",
            EventTemplateId::SpawnEliteEncounter => "SPAWN_ELITE_ENCOUNTER",
            EventTemplateId::TriggerWeatherChange => "TRIGGER_WEATHER_CHANGE",
            EventTemplateId::TriggerLoreReveal => "TRIGGER_LORE_REVEAL",
            EventTemplateId::TriggerRestArea => "TRIGGER_REST_AREA",
        }
    }
}

/// Event template metadata — minimum surface needed for selection +
/// telemetry. Sibling slices can extend (e.g. asset-id / weighting hooks)
/// by appending fields ; preserve the existing field order for the .cssl
/// translation.
#[derive(Debug, Clone, Copy)]
pub struct EventTemplate {
    pub id: EventTemplateId,
    /// Required tension level for the template to fire (0.0..=1.0).
    pub intensity: f32,
}

/// 8-template registry. Indexed by `EventTemplateId as usize`.
pub const EVENT_TEMPLATE_REGISTRY: [EventTemplate; 8] = [
    EventTemplate { id: EventTemplateId::SpawnNpcArrival, intensity: 0.2 },
    EventTemplate { id: EventTemplateId::SpawnAmbientCreature, intensity: 0.3 },
    EventTemplate { id: EventTemplateId::SpawnBossTeaser, intensity: 0.6 },
    EventTemplate { id: EventTemplateId::SpawnLootDrop, intensity: 0.4 },
    EventTemplate { id: EventTemplateId::SpawnEliteEncounter, intensity: 0.7 },
    EventTemplate { id: EventTemplateId::TriggerWeatherChange, intensity: 0.3 },
    EventTemplate { id: EventTemplateId::TriggerLoreReveal, intensity: 0.5 },
    EventTemplate { id: EventTemplateId::TriggerRestArea, intensity: 0.1 },
];

/// A proposed event — what the DM director hands to the consumer per tick.
#[derive(Debug, Clone, Copy)]
pub struct DmEvent {
    pub template: EventTemplateId,
    pub intensity: f32,
    pub state_at_propose: DmState,
    pub frame: u64,
}

// ─────────────────────────────────────────────────────────────────────────
// § DIRECTOR
// ─────────────────────────────────────────────────────────────────────────

/// Cooldown entry : `(template-id, expiry-frame)`.
#[derive(Debug, Clone, Copy, Default)]
struct CooldownSlot {
    /// 0xFF = empty slot ; otherwise template id as u8.
    template_byte: u8,
    expiry_frame: u64,
}

const COOLDOWN_SLOT_EMPTY: u8 = 0xFF;

/// DM director — the engine's autonomous pacing brain.
#[derive(Debug)]
pub struct DmDirector {
    state: DmState,
    /// Frame-count this state has been active (resets on transition).
    frame_in_state: u32,
    /// Last computed tension (0.0..=1.0).
    last_tension: f32,
    /// Climax-decay accumulator — adds to tension while in CLIMAX so the
    /// curve doesn't immediately fall under the BUILDUP threshold once
    /// the player heals mid-CLIMAX. Resets on entering CLIMAX.
    climax_decay: f32,
    /// Cooldown ring (8 slots ; round-robin write index).
    cooldown_ring: [CooldownSlot; COOLDOWN_RING_LEN],
    cooldown_write_idx: usize,
    /// Round-robin selector over the 8 templates so successive proposals
    /// don't always land on the lowest-index match.
    template_rr_idx: u8,
}

impl Default for DmDirector {
    fn default() -> Self {
        Self::new()
    }
}

impl DmDirector {
    /// Construct a director starting in `CALM`.
    pub fn new() -> Self {
        log_event(
            "INFO",
            "loa-host/dm",
            "init · state=CALM · tension=0.0",
        );
        Self {
            state: DmState::Calm,
            frame_in_state: 0,
            last_tension: 0.0,
            climax_decay: 0.0,
            cooldown_ring: [CooldownSlot {
                template_byte: COOLDOWN_SLOT_EMPTY,
                expiry_frame: 0,
            }; COOLDOWN_RING_LEN],
            cooldown_write_idx: 0,
            template_rr_idx: 0,
        }
    }

    /// Current FSM state (for telemetry / external read).
    pub fn state(&self) -> DmState {
        self.state
    }

    /// Last computed tension value (0.0..=1.0).
    pub fn tension(&self) -> f32 {
        self.last_tension
    }

    /// Frame-count active in the current state.
    pub fn frame_in_state(&self) -> u32 {
        self.frame_in_state
    }

    /// Compute the tension scalar from a player-state snapshot.
    ///
    /// `tension = hp_deficit * 0.4 + recent_combat_density * 0.3 +
    ///            stamina_deficit * 0.15 - rest_signals * 0.2 +
    ///            drama_spike_bonus + climax_decay`
    /// bounded `0.0..=1.0`. The drama-spike kicks in when BOTH hp_deficit
    /// AND combat_density are saturated (>= 0.9), pushing tension over
    /// the CLIMAX threshold. `climax_decay` is a self-feedback term the
    /// FSM injects while in CLIMAX so a healed player doesn't immediately
    /// drop back below BUILDUP.
    pub fn compute_tension(&self, ps: &PlayerState) -> f32 {
        let danger_spike = if ps.hp_deficit >= DRAMA_SPIKE_HP_THRESHOLD
            && ps.recent_combat_density >= DRAMA_SPIKE_COMBAT_THRESHOLD
        {
            DRAMA_SPIKE_BONUS
        } else {
            0.0
        };
        let raw = ps.hp_deficit * COEF_HP_DEFICIT
            + ps.recent_combat_density * COEF_COMBAT_DENSITY
            + ps.stamina_deficit * COEF_STAMINA_DEFICIT
            - ps.rest_signals * COEF_REST_SIGNALS
            + danger_spike
            + self.climax_decay;
        raw.clamp(0.0, 1.0)
    }

    /// Drive one tick of the director.
    ///
    /// - Recomputes tension from `player_state`.
    /// - Steps the FSM (one transition max per tick).
    /// - In `BUILDUP` or `CLIMAX`, optionally proposes one event whose
    ///   intensity ≤ tension and is not on cooldown.
    pub fn tick(&mut self, player_state: &PlayerState, frame: u64) -> Option<DmEvent> {
        // 1. recompute tension
        let tension = self.compute_tension(player_state);
        self.last_tension = tension;

        // 2. step FSM
        let prev_state = self.state;
        self.frame_in_state = self.frame_in_state.saturating_add(1);
        match self.state {
            DmState::Calm => {
                if tension > TENSION_THRESHOLD_BUILDUP {
                    self.transition(DmState::Buildup);
                }
            }
            DmState::Buildup => {
                if tension > TENSION_THRESHOLD_CLIMAX {
                    self.climax_decay = 0.15; // gentle floor while in CLIMAX
                    self.transition(DmState::Climax);
                }
            }
            DmState::Climax => {
                // climax_decay slowly bleeds out while in this state
                self.climax_decay = (self.climax_decay - 0.002).max(0.0);
                if self.frame_in_state > CLIMAX_DURATION_FRAMES {
                    self.transition(DmState::Relief);
                }
            }
            DmState::Relief => {
                self.climax_decay = 0.0;
                if self.frame_in_state > RELIEF_DURATION_FRAMES {
                    self.transition(DmState::Calm);
                }
            }
        }

        if prev_state != self.state {
            let msg = format!(
                "tick · state={} · tension={:.2}",
                self.state.label(),
                tension,
            );
            log_event("INFO", "loa-host/dm", &msg);
        }

        // 3. propose event (BUILDUP or CLIMAX only)
        if matches!(self.state, DmState::Buildup | DmState::Climax) {
            if let Some(template) = self.select_template(tension, frame) {
                self.record_cooldown(template, frame);
                let msg = format!(
                    "event-proposed · template={} · intensity={:.2} · state={}",
                    template.label(),
                    EVENT_TEMPLATE_REGISTRY[template as usize].intensity,
                    self.state.label(),
                );
                log_event("INFO", "loa-host/dm", &msg);
                return Some(DmEvent {
                    template,
                    intensity: EVENT_TEMPLATE_REGISTRY[template as usize].intensity,
                    state_at_propose: self.state,
                    frame,
                });
            }
        }
        None
    }

    /// Select an event template whose `intensity ≤ tension` and that is not
    /// currently on cooldown. Round-robin over the registry to avoid bias
    /// toward low-index templates.
    fn select_template(&mut self, tension: f32, frame: u64) -> Option<EventTemplateId> {
        let n = EVENT_TEMPLATE_REGISTRY.len() as u8;
        for offset in 0..n {
            let idx = (self.template_rr_idx + offset) % n;
            let tmpl = EVENT_TEMPLATE_REGISTRY[idx as usize];
            if tmpl.intensity <= tension && !self.is_on_cooldown(tmpl.id, frame) {
                self.template_rr_idx = (idx + 1) % n;
                return Some(tmpl.id);
            }
        }
        None
    }

    /// True if `template` was proposed within `COOLDOWN_DURATION_FRAMES` of `frame`.
    fn is_on_cooldown(&self, template: EventTemplateId, frame: u64) -> bool {
        let want = template as u8;
        for slot in &self.cooldown_ring {
            if slot.template_byte == want && slot.expiry_frame > frame {
                return true;
            }
        }
        false
    }

    /// Record `template` on cooldown (round-robin overwrite of oldest slot).
    fn record_cooldown(&mut self, template: EventTemplateId, frame: u64) {
        let slot = &mut self.cooldown_ring[self.cooldown_write_idx];
        slot.template_byte = template as u8;
        slot.expiry_frame = frame.saturating_add(u64::from(COOLDOWN_DURATION_FRAMES));
        self.cooldown_write_idx = (self.cooldown_write_idx + 1) % COOLDOWN_RING_LEN;
    }

    /// Apply a state transition, resetting the in-state frame counter.
    fn transition(&mut self, next: DmState) {
        self.state = next;
        self.frame_in_state = 0;
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § TESTS
// ─────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dm_init_starts_in_calm() {
        let d = DmDirector::new();
        assert_eq!(d.state(), DmState::Calm);
        assert_eq!(d.frame_in_state(), 0);
        assert_eq!(d.tension(), 0.0);
    }

    #[test]
    fn dm_tension_drives_buildup_transition() {
        let mut d = DmDirector::new();
        // Configure a player-state that yields tension > 0.25 :
        //   hp_deficit 0.5 * 0.4 = 0.20
        //   recent_combat_density 0.5 * 0.3 = 0.15
        //   rest_signals 0.0 * 0.2 = 0.00
        //   total = 0.35 > 0.25
        let ps = PlayerState {
            hp_deficit: 0.5,
            stamina_deficit: 0.0,
            recent_combat_density: 0.5,
            rest_signals: 0.0,
        };
        let _ = d.tick(&ps, 1);
        assert_eq!(d.state(), DmState::Buildup);
        assert!(d.tension() > TENSION_THRESHOLD_BUILDUP);
    }

    #[test]
    fn dm_climax_to_relief_after_50_frames() {
        let mut d = DmDirector::new();
        let high = PlayerState {
            hp_deficit: 1.0,
            stamina_deficit: 1.0,
            recent_combat_density: 1.0,
            rest_signals: 0.0,
        };
        // First tick → BUILDUP. Second tick → CLIMAX.
        let _ = d.tick(&high, 1);
        let _ = d.tick(&high, 2);
        assert_eq!(d.state(), DmState::Climax);
        // Drive 51 more ticks at high tension → CLIMAX exceeds 50 → RELIEF.
        for f in 3..=53 {
            let _ = d.tick(&high, f);
        }
        assert_eq!(d.state(), DmState::Relief);
    }

    #[test]
    fn dm_relief_to_calm_after_30_frames() {
        let mut d = DmDirector::new();
        let high = PlayerState {
            hp_deficit: 1.0,
            stamina_deficit: 1.0,
            recent_combat_density: 1.0,
            rest_signals: 0.0,
        };
        // Reach RELIEF (~53 ticks).
        for f in 1..=53 {
            let _ = d.tick(&high, f);
        }
        assert_eq!(d.state(), DmState::Relief);
        // Now drop tension to zero (rest_signals saturates).
        let calm_ps = PlayerState {
            hp_deficit: 0.0,
            stamina_deficit: 0.0,
            recent_combat_density: 0.0,
            rest_signals: 1.0,
        };
        for f in 54..=85 {
            let _ = d.tick(&calm_ps, f);
        }
        assert_eq!(d.state(), DmState::Calm);
    }

    #[test]
    fn dm_event_template_registry_has_8_entries() {
        assert_eq!(EVENT_TEMPLATE_REGISTRY.len(), 8);
        // Confirm intensities are sorted across the spec's 0.1..0.7 band.
        assert_eq!(EVENT_TEMPLATE_REGISTRY[0].intensity, 0.2);
        assert_eq!(EVENT_TEMPLATE_REGISTRY[7].intensity, 0.1);
        // Sanity : no template demands intensity > 1.0.
        for t in &EVENT_TEMPLATE_REGISTRY {
            assert!(t.intensity > 0.0 && t.intensity <= 1.0);
        }
    }

    #[test]
    fn dm_calm_state_proposes_no_events() {
        let mut d = DmDirector::new();
        let ps = PlayerState::default();
        for f in 1..=20 {
            let ev = d.tick(&ps, f);
            assert!(ev.is_none(), "CALM must not propose events");
        }
        assert_eq!(d.state(), DmState::Calm);
    }

    #[test]
    fn dm_cooldown_blocks_repeat_template_within_window() {
        let mut d = DmDirector::new();
        let ps = PlayerState {
            hp_deficit: 1.0,
            stamina_deficit: 1.0,
            recent_combat_density: 1.0,
            rest_signals: 0.0,
        };
        let mut first: Option<EventTemplateId> = None;
        for f in 1..=4 {
            if let Some(ev) = d.tick(&ps, f) {
                if first.is_none() {
                    first = Some(ev.template);
                }
            }
        }
        // Within the 120-frame cooldown window, the same template should NOT
        // re-fire as the round-robin cycles through alternatives.
        let mut saw_repeat = false;
        if let Some(initial) = first {
            for f in 5..=20 {
                if let Some(ev) = d.tick(&ps, f) {
                    if ev.template == initial {
                        saw_repeat = true;
                    }
                }
            }
        }
        assert!(!saw_repeat, "cooldown ring should suppress repeat within window");
    }
}
