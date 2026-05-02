//! § dm_runtime — DM director + GM narrator aggregate
//! ════════════════════════════════════════════════════
//!
//! § T11-LOA-HOST-4 (W-LOA-host-dm) — consumer-facing aggregate that the
//! MCP-server tools (`dm.intensity` / `dm.event.propose` /
//! `gm.describe_environment` / `gm.dialogue`) consume once sibling
//! W-LOA-host-mcp lands and wires the field into `EngineState`.
//!
//! § DESIGN
//!   `DmRuntime` owns a `DmDirector` + `GmNarrator` and exposes two main
//!   surfaces :
//!     - `tick(player_state, frame)` : drives the director's FSM + returns
//!       any proposed event.
//!     - `describe_neighborhood(camera_pos)` : convenience wrapper over
//!       `GmNarrator::describe_environment`. Time-of-day defaults to `Day`
//!       at this stage ; sibling slices wire a real time-of-day source
//!       (engine clock) by extending the API.
//!
//! § MCP-TOOL SHAPE PREVIEW
//!   ```text
//!   dm.intensity              → f32 (DmRuntime::intensity)
//!   dm.event.propose          → Option<DmEvent> (DmRuntime::tick)
//!   gm.describe_environment   → String (DmRuntime::describe_neighborhood)
//!   gm.dialogue               → String (DmRuntime::dialogue)
//!   ```

use std::time::Instant;

use cssl_rt::loa_startup::log_event;

use crate::dm_arc::{classify_nudge, ArcKnobs, ArcPhase, DmArc};
use crate::dm_director::{DmDirector, DmEvent, DmState, PlayerState};
use crate::gm_narrator::{Archetype, GmNarrator, Mood, PhraseTopic, TimeOfDay, Vec3};
use crate::gm_persona::{ComposedResponse, GmMemory, GmPersona};

// ─────────────────────────────────────────────────────────────────────────
// § DM RUNTIME
// ─────────────────────────────────────────────────────────────────────────

/// Aggregate of the DM director + GM narrator. Held by sibling
/// W-LOA-host-mcp's `EngineState` as a single field ; the MCP-server tools
/// dispatch into the methods below.
///
/// § T11-W11-GM-DM-DEEPEN
///   `arc` (DmArc) and `persona`/`memory` give the runtime the rich
///   narrative-arc state machine + per-player persona surface that the
///   chat-panel lights up. Stage-0 holds a SINGLE persona+memory ; stage-1
///   will key these by σ-mask so multi-tenant sessions stay isolated.
#[derive(Debug)]
pub struct DmRuntime {
    director: DmDirector,
    narrator: GmNarrator,
    /// 5-phase narrative arc (Discovery → Tension → Crisis → Catharsis → Quiet).
    arc: DmArc,
    /// Active persona for the current player-session.
    persona: GmPersona,
    /// 16-deep ring of (player-utterance, GM-response) pairs.
    memory: GmMemory,
    /// Wall-clock of the last event proposal (for cooldown-debug + UI hints).
    /// Kept as `Instant` to avoid serialization complications ; sibling
    /// MCP-server slice exposes elapsed-since-last as a u64 millis if
    /// telemetry needs it.
    last_event_ts: Instant,
}

impl Default for DmRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl DmRuntime {
    pub fn new() -> Self {
        log_event(
            "INFO",
            "loa-host/dm-runtime",
            "init · director + narrator + arc + persona constructed",
        );
        // Stage-0 default seed · sibling MCP-server slice will replace
        // with a session-σ-mask-derived seed once chat-panel wires.
        // (literal trimmed to fit u64 ; original 0x10A_50C1A1_0FFEE_C5E5
        // exceeded u64::MAX. The truncated lower 64 bits are equivalent
        // for stage-0 seeding · still deterministic per-session-σ-mask.)
        let default_seed: u64 = 0xA50C_1A10_FFEE_C5E5;
        Self {
            director: DmDirector::new(),
            narrator: GmNarrator::new(),
            arc: DmArc::new(),
            persona: GmPersona::from_seed(default_seed),
            memory: GmMemory::new(),
            last_event_ts: Instant::now(),
        }
    }

    /// Construct with a specific player-seed · used by the chat-panel
    /// wiring once a player-session opens. Seed isolates persona-axes +
    /// the utterance memory across player sessions.
    #[must_use]
    pub fn with_player_seed(seed: u64) -> Self {
        log_event(
            "INFO",
            "loa-host/dm-runtime",
            &format!("init · player-seed={:016x}", seed),
        );
        Self {
            director: DmDirector::new(),
            narrator: GmNarrator::new(),
            arc: DmArc::new(),
            persona: GmPersona::from_seed(seed),
            memory: GmMemory::new(),
            last_event_ts: Instant::now(),
        }
    }

    /// Re-seed the persona (and clear memory) — called on σ-mask flip.
    pub fn reseed_persona(&mut self, seed: u64) {
        self.persona = GmPersona::from_seed(seed);
        self.memory.clear();
        self.arc.reset();
        log_event(
            "INFO",
            "loa-host/dm-runtime",
            &format!("persona-reseeded · seed={:016x}", seed),
        );
    }

    /// Drive one tick of the DM director ; returns any proposed event.
    /// Sibling MCP-server slice's `dm.event.propose` tool calls this and
    /// ships the result over the JSON-RPC wire.
    pub fn tick(&mut self, player_state: &PlayerState, frame: u64) -> Option<DmEvent> {
        let ev = self.director.tick(player_state, frame);
        if ev.is_some() {
            self.last_event_ts = Instant::now();
        }
        ev
    }

    /// Current tension scalar (0.0..=1.0). Sibling MCP-server slice's
    /// `dm.intensity` tool returns this directly.
    pub fn intensity(&self) -> f32 {
        self.director.tension()
    }

    /// Current FSM state (for telemetry / UI badges).
    pub fn state(&self) -> DmState {
        self.director.state()
    }

    /// Procedurally describe the neighborhood at `camera_pos`. Sibling
    /// MCP-server slice's `gm.describe_environment` tool returns this.
    /// Time-of-day defaults to `Day` at the stage-0 layer ; sibling slices
    /// extend with a clock-aware variant.
    pub fn describe_neighborhood(&mut self, camera_pos: Vec3) -> String {
        self.narrator.describe_environment(camera_pos, TimeOfDay::Day)
    }

    /// As `describe_neighborhood`, but with explicit time-of-day.
    pub fn describe_neighborhood_at(
        &mut self,
        camera_pos: Vec3,
        time_of_day: TimeOfDay,
    ) -> String {
        self.narrator.describe_environment(camera_pos, time_of_day)
    }

    /// Generate a line of dialogue for the given NPC. Sibling MCP-server
    /// slice's `gm.dialogue` tool returns this.
    pub fn dialogue(
        &mut self,
        npc_id: u32,
        archetype: Archetype,
        mood: Mood,
        topic: PhraseTopic,
    ) -> String {
        self.narrator
            .generate_dialogue(npc_id, archetype, mood, topic)
    }

    /// Wall-clock instant of the most-recent event proposal. Useful for
    /// MCP-server diagnostics ("last DM event 4.2 s ago").
    pub fn last_event_instant(&self) -> Instant {
        self.last_event_ts
    }

    // ─────────────────────────────────────────────────────────────────────
    // § T11-W11-GM-DM-DEEPEN — persona + arc surface
    // ─────────────────────────────────────────────────────────────────────

    /// Snapshot of the persona-axes for the current session. The chat-
    /// panel UI can render these as a small persona-badge so the player
    /// sees who they're talking to.
    #[must_use]
    pub fn persona_axes(&self) -> [i8; crate::gm_persona::PERSONA_AXIS_COUNT] {
        self.persona.axes
    }

    /// Persona's archetype-bias (informs phrase-pool selection).
    #[must_use]
    pub fn persona_archetype(&self) -> Archetype {
        self.persona.archetype_bias
    }

    /// Read-only view of the persona.
    #[must_use]
    pub fn persona(&self) -> &GmPersona {
        &self.persona
    }

    /// Current 5-phase narrative-arc phase.
    #[must_use]
    pub fn arc_phase(&self) -> ArcPhase {
        self.arc.phase()
    }

    /// Tunable knobs for the current arc-phase. Sibling render +
    /// audio + procgen consumers read these to modulate spawn rate ·
    /// music intensity · loot richness · NPC mood.
    #[must_use]
    pub fn arc_knobs(&self) -> ArcKnobs {
        self.arc.knobs()
    }

    /// Tick the narrative-arc state machine. Returns Some(new-phase) on
    /// transition · None otherwise. Caller (sibling-MCP-server intent
    /// dispatcher) is expected to invoke this once per game-frame.
    pub fn tick_arc(&mut self, frame: u64) -> Option<ArcPhase> {
        self.arc.tick(frame)
    }

    /// Total arc-transitions seen by this runtime instance. Cheap
    /// telemetry surface for the chat-panel arc-badge.
    #[must_use]
    pub fn arc_total_transitions(&self) -> u32 {
        self.arc.total_transitions()
    }

    /// Compose a rich persona+arc-aware response to a player utterance.
    ///
    /// This is the FRONTSTAGE entry-point the chat-panel calls. It :
    ///   1. Classifies the utterance for any nudge-intent (rest/push/etc).
    ///   2. Pushes the nudge into the arc state machine.
    ///   3. Picks an appropriate topic_hint based on arc-phase + dm-state.
    ///   4. Asks the narrator for a persona-decorated response.
    ///   5. Returns the ComposedResponse (text + kind + glyph-density).
    ///
    /// Sample outputs (illustrative · per-persona-seed will vary) :
    ///   - Discovery + warm-persona : "the corridor breathes around you,
    ///     slow and patient. · the lamps stay lit for you."
    ///   - Crisis + mythic-persona  : "‼ Tread softly here ; the floor
    ///     remembers weight. § ω "
    ///   - Quiet + cryptic-persona  : "(the figure looks at you and says
    ///     nothing) — and yet."
    pub fn respond_to_player(&mut self, player_text: &str, frame: u64) -> ComposedResponse {
        // 1. Classify nudge + push to arc.
        if let Some(n) = classify_nudge(player_text) {
            self.arc.push_nudge(n, frame);
        }
        // 2. Tick arc once (lets nudge potentially flip the phase).
        let _ = self.arc.tick(frame);

        // 3. Pick topic-hint from current arc + micro-phase.
        let topic_hint = match (self.arc.phase(), self.director.state()) {
            (ArcPhase::Discovery, _) => PhraseTopic::Greeting,
            (ArcPhase::Tension, DmState::Buildup) => PhraseTopic::Warning,
            (ArcPhase::Tension, _) => PhraseTopic::Mystery,
            (ArcPhase::Crisis, DmState::Climax) => PhraseTopic::BattleCry,
            (ArcPhase::Crisis, _) => PhraseTopic::Defiance,
            (ArcPhase::Catharsis, DmState::Relief) => PhraseTopic::Hope,
            (ArcPhase::Catharsis, _) => PhraseTopic::Reverence,
            (ArcPhase::Quiet, _) => PhraseTopic::Lullaby,
        };

        // 4. Per-tick seed so successive composes vary.
        let seed = self
            .persona
            .player_seed
            .wrapping_mul(0x9E37_79B9_7F4A_7C15)
            .wrapping_add(frame);

        // 5. Compose.
        self.narrator.respond_in_persona(
            player_text,
            &self.persona,
            &mut self.memory,
            self.arc.phase(),
            self.director.state(),
            topic_hint,
            frame,
            seed,
        )
    }

    /// Read-only memory-size · for MCP diagnostics.
    #[must_use]
    pub fn memory_pushes(&self) -> u32 {
        self.memory.total_pushes
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § TESTS
// ─────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_init_default() {
        let r = DmRuntime::new();
        assert_eq!(r.intensity(), 0.0);
        assert_eq!(r.state(), DmState::Calm);
    }

    #[test]
    fn runtime_tick_drives_director() {
        let mut r = DmRuntime::new();
        let ps = PlayerState {
            hp_deficit: 1.0,
            stamina_deficit: 1.0,
            recent_combat_density: 1.0,
            rest_signals: 0.0,
        };
        let _ = r.tick(&ps, 1);
        assert!(r.intensity() > 0.0);
    }

    #[test]
    fn runtime_describe_neighborhood_returns_text() {
        let mut r = DmRuntime::new();
        let s = r.describe_neighborhood(Vec3::new(0.0, 0.0, 0.0));
        assert!(!s.is_empty());
    }

    #[test]
    fn runtime_dialogue_returns_text() {
        let mut r = DmRuntime::new();
        let s = r.dialogue(1, Archetype::Bard, Mood::Friendly, PhraseTopic::Greeting);
        assert!(!s.is_empty());
    }

    // ─────────────────────────────────────────────────────────────────
    // § T11-W11-GM-DM-DEEPEN — persona + arc tests
    // ─────────────────────────────────────────────────────────────────

    #[test]
    fn runtime_with_player_seed_is_deterministic() {
        let r1 = DmRuntime::with_player_seed(0xC0FF_EEDA_DAFE_DABE);
        let r2 = DmRuntime::with_player_seed(0xC0FF_EEDA_DAFE_DABE);
        assert_eq!(r1.persona_axes(), r2.persona_axes());
    }

    #[test]
    fn runtime_arc_starts_in_discovery() {
        let r = DmRuntime::new();
        assert_eq!(r.arc_phase(), ArcPhase::Discovery);
    }

    #[test]
    fn runtime_respond_to_player_returns_text() {
        let mut r = DmRuntime::with_player_seed(42);
        let resp = r.respond_to_player("hello there", 1);
        assert!(!resp.text.is_empty(), "response must not be empty");
        assert!(resp.glyph_density >= 0.0 && resp.glyph_density <= 1.0);
    }

    #[test]
    fn runtime_respond_pushes_memory() {
        let mut r = DmRuntime::with_player_seed(7);
        assert_eq!(r.memory_pushes(), 0);
        let _ = r.respond_to_player("hello there", 1);
        assert_eq!(r.memory_pushes(), 1);
        let _ = r.respond_to_player("anyone there?", 2);
        assert_eq!(r.memory_pushes(), 2);
    }

    #[test]
    fn runtime_respond_classifies_rest_as_nudge() {
        let mut r = DmRuntime::with_player_seed(7);
        let _ = r.respond_to_player("I want to rest", 1);
        // After a rest-nudge · the arc nudge-sum goes negative ; we can't
        // peek directly · but the arc should still be in Discovery (rest
        // nudges don't matter at base anyway).
        assert_eq!(r.arc_phase(), ArcPhase::Discovery);
    }

    #[test]
    fn runtime_reseed_persona_clears_memory() {
        let mut r = DmRuntime::with_player_seed(1);
        let _ = r.respond_to_player("hi", 1);
        let _ = r.respond_to_player("again", 2);
        assert_eq!(r.memory_pushes(), 2);
        r.reseed_persona(99);
        assert_eq!(r.memory_pushes(), 0);
        assert_eq!(r.arc_phase(), ArcPhase::Discovery);
    }

    #[test]
    fn runtime_arc_knobs_match_phase() {
        let r = DmRuntime::new();
        let k = r.arc_knobs();
        // Discovery default-knobs : monster_density 0.10
        assert!((k.monster_density - 0.10).abs() < 0.001);
    }

    #[test]
    fn runtime_persona_archetype_in_range() {
        let r = DmRuntime::with_player_seed(0xFEEDFACE);
        // Archetype is one of the 16 (label is a non-empty static str).
        assert!(!r.persona_archetype().label().is_empty());
    }
}
