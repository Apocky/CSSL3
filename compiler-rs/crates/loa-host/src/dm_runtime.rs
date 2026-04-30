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

use crate::dm_director::{DmDirector, DmEvent, DmState, PlayerState};
use crate::gm_narrator::{Archetype, GmNarrator, Mood, PhraseTopic, TimeOfDay, Vec3};

// ─────────────────────────────────────────────────────────────────────────
// § DM RUNTIME
// ─────────────────────────────────────────────────────────────────────────

/// Aggregate of the DM director + GM narrator. Held by sibling
/// W-LOA-host-mcp's `EngineState` as a single field ; the MCP-server tools
/// dispatch into the methods below.
#[derive(Debug)]
pub struct DmRuntime {
    director: DmDirector,
    narrator: GmNarrator,
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
            "init · director + narrator constructed",
        );
        Self {
            director: DmDirector::new(),
            narrator: GmNarrator::new(),
            last_event_ts: Instant::now(),
        }
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
}
