//! § dm_arc — narrative-arc 5-phase Markov + tunable knobs
//! ════════════════════════════════════════════════════════════
//!
//! § T11-W11-GM-DM-DEEPEN
//!   Sibling-module to the 4-state pacing FSM in `dm_director.rs`. Where
//!   the director-FSM handles MICRO-pacing (CALM ↔ BUILDUP ↔ CLIMAX ↔
//!   RELIEF over seconds-of-frames) · the arc state machine handles
//!   MACRO-pacing (Discovery → Tension → Crisis → Catharsis → Quiet over
//!   minutes-of-play within a single scene).
//!
//!   The two work in concert : the director still drives spawn-cadence ;
//!   the arc shapes ambient-music · loot-richness · NPC-mood · monster-
//!   density envelope across the full arc-cycle. Player text-input
//!   intents nudge the arc transitions — e.g. "I want to rest" eases
//!   toward `Quiet` ; "anyone left ?" pushes toward `Tension`.
//!
//! § AUTHORITATIVE DESIGN-SPEC mirror
//!   `Labyrinth of Apocalypse/systems/dm_arc.csl` is authoritative.
//!   This Rust file is a stage-0 mirror until csslc-advance compiles the
//!   .csl directly.
//!
//! § PRIME-DIRECTIVE attestation
//!   There was no hurt nor harm in the making of this, to anyone/anything/anybody.

#![allow(clippy::module_name_repetitions)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_precision_loss)]

use cssl_rt::loa_startup::log_event;

// ─────────────────────────────────────────────────────────────────────────
// § ARC PHASE (5)
// ─────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ArcPhase {
    Discovery = 0,
    Tension = 1,
    Crisis = 2,
    Catharsis = 3,
    Quiet = 4,
}

pub const ARC_PHASE_COUNT: usize = 5;

impl ArcPhase {
    pub fn label(self) -> &'static str {
        match self {
            ArcPhase::Discovery => "Discovery",
            ArcPhase::Tension => "Tension",
            ArcPhase::Crisis => "Crisis",
            ArcPhase::Catharsis => "Catharsis",
            ArcPhase::Quiet => "Quiet",
        }
    }

    pub fn from_index(i: u8) -> Option<Self> {
        match i {
            0 => Some(ArcPhase::Discovery),
            1 => Some(ArcPhase::Tension),
            2 => Some(ArcPhase::Crisis),
            3 => Some(ArcPhase::Catharsis),
            4 => Some(ArcPhase::Quiet),
            _ => None,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § ARC KNOBS — what each phase outputs to the consumer
// ─────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ArcKnobs {
    pub monster_density: f32,
    pub ambient_music_intensity: f32,
    pub loot_richness: f32,
    /// -1.0 = grim · 0.0 = neutral · +1.0 = bright.
    pub npc_mood_bias: f32,
}

/// Default-knobs-per-phase · indexed by `ArcPhase as usize`.
pub const PHASE_KNOBS: [ArcKnobs; ARC_PHASE_COUNT] = [
    // Discovery — wonder · low monsters · soft music · sparse-but-rich loot
    ArcKnobs {
        monster_density: 0.10,
        ambient_music_intensity: 0.30,
        loot_richness: 0.50,
        npc_mood_bias: 0.40,
    },
    // Tension — unease · medium monsters · rising music · normal loot
    ArcKnobs {
        monster_density: 0.40,
        ambient_music_intensity: 0.55,
        loot_richness: 0.40,
        npc_mood_bias: 0.0,
    },
    // Crisis — peril · high monsters · loud music · low loot
    ArcKnobs {
        monster_density: 0.85,
        ambient_music_intensity: 0.90,
        loot_richness: 0.20,
        npc_mood_bias: -0.50,
    },
    // Catharsis — release · low monsters · soaring music · rich-burst loot
    ArcKnobs {
        monster_density: 0.20,
        ambient_music_intensity: 0.75,
        loot_richness: 0.85,
        npc_mood_bias: 0.60,
    },
    // Quiet — lull · near-zero monsters · ambient music · trickle loot
    ArcKnobs {
        monster_density: 0.05,
        ambient_music_intensity: 0.20,
        loot_richness: 0.30,
        npc_mood_bias: 0.30,
    },
];

// ─────────────────────────────────────────────────────────────────────────
// § DURATION GATES (frames-in-phase before "ready to advance")
// ─────────────────────────────────────────────────────────────────────────

pub const MIN_DWELL_FRAMES: [u32; ARC_PHASE_COUNT] = [
    300,   // Discovery — 5s @ 60fps
    600,   // Tension   — 10s
    900,   // Crisis    — 15s
    600,   // Catharsis — 10s
    600,   // Quiet     — 10s
];

pub const SOFT_CAP_FRAMES: [u32; ARC_PHASE_COUNT] = [
    1800,  // Discovery — 30s
    3600,  // Tension   — 60s
    1800,  // Crisis    — 30s
    1200,  // Catharsis — 20s
    2400,  // Quiet     — 40s
];

// ─────────────────────────────────────────────────────────────────────────
// § NUDGE RING (8 deep)
// ─────────────────────────────────────────────────────────────────────────

pub const NUDGE_RING_LEN: usize = 8;
pub const NUDGE_THRESHOLD: i8 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum NudgeKind {
    /// "I want to rest" · "let's sit" · "make camp" → -2 (regress)
    Rest = 0,
    /// "anyone left ?" · "I'll keep going" · "I'm hungry for more" → +2
    Push = 1,
    /// "this is too much" · "I need a moment" → -1
    Caution = 2,
    /// "show me what's next" · "more" → +1
    Curiosity = 3,
    /// "I'll fight" · "engage" → +2
    Aggression = 4,
    /// "I yield" · "I retreat" → -2
    Yield = 5,
}

impl NudgeKind {
    pub fn delta(self) -> i8 {
        match self {
            NudgeKind::Rest => -2,
            NudgeKind::Push => 2,
            NudgeKind::Caution => -1,
            NudgeKind::Curiosity => 1,
            NudgeKind::Aggression => 2,
            NudgeKind::Yield => -2,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            NudgeKind::Rest => "rest",
            NudgeKind::Push => "push",
            NudgeKind::Caution => "caution",
            NudgeKind::Curiosity => "curiosity",
            NudgeKind::Aggression => "aggression",
            NudgeKind::Yield => "yield",
        }
    }
}

/// Heuristically classify a player utterance into a NudgeKind.
#[must_use]
pub fn classify_nudge(text: &str) -> Option<NudgeKind> {
    let lower = text.to_lowercase();
    if lower.contains("rest") || lower.contains("camp") || lower.contains("sit") {
        Some(NudgeKind::Rest)
    } else if lower.contains("retreat") || lower.contains("yield") || lower.contains("flee") {
        Some(NudgeKind::Yield)
    } else if lower.contains("fight") || lower.contains("engage") || lower.contains("attack") {
        Some(NudgeKind::Aggression)
    } else if lower.contains("push") || lower.contains("more") || lower.contains("keep going") {
        Some(NudgeKind::Push)
    } else if lower.contains("caution") || lower.contains("careful") || lower.contains("slow") {
        Some(NudgeKind::Caution)
    } else if lower.contains("curious") || lower.contains("explore") || lower.contains("show me") {
        Some(NudgeKind::Curiosity)
    } else {
        None
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § DM ARC STATE MACHINE
// ─────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
struct NudgeSlot {
    kind: u8, // 0xFF = empty
    frame: u64,
}

const NUDGE_SLOT_EMPTY: u8 = 0xFF;

#[derive(Debug)]
pub struct DmArc {
    phase: ArcPhase,
    frame_in_phase: u32,
    nudge_ring: [NudgeSlot; NUDGE_RING_LEN],
    nudge_write: usize,
    cached_nudge_sum: i8,
    last_transition_frame: u64,
    total_transitions: u32,
}

impl Default for DmArc {
    fn default() -> Self {
        Self::new()
    }
}

impl DmArc {
    #[must_use]
    pub fn new() -> Self {
        log_event(
            "INFO",
            "loa-host/dm-arc",
            "init · phase=Discovery · 5-phase Markov",
        );
        Self {
            phase: ArcPhase::Discovery,
            frame_in_phase: 0,
            nudge_ring: [NudgeSlot {
                kind: NUDGE_SLOT_EMPTY,
                frame: 0,
            }; NUDGE_RING_LEN],
            nudge_write: 0,
            cached_nudge_sum: 0,
            last_transition_frame: 0,
            total_transitions: 0,
        }
    }

    #[must_use]
    pub fn phase(&self) -> ArcPhase {
        self.phase
    }

    #[must_use]
    pub fn frame_in_phase(&self) -> u32 {
        self.frame_in_phase
    }

    #[must_use]
    pub fn knobs(&self) -> ArcKnobs {
        PHASE_KNOBS[self.phase as usize]
    }

    #[must_use]
    pub fn nudge_sum(&self) -> i8 {
        self.cached_nudge_sum
    }

    #[must_use]
    pub fn total_transitions(&self) -> u32 {
        self.total_transitions
    }

    #[must_use]
    pub fn last_transition_frame(&self) -> u64 {
        self.last_transition_frame
    }

    pub fn push_nudge(&mut self, kind: NudgeKind, frame: u64) {
        self.nudge_ring[self.nudge_write] = NudgeSlot {
            kind: kind as u8,
            frame,
        };
        self.nudge_write = (self.nudge_write + 1) % NUDGE_RING_LEN;
        self.recompute_nudge_sum();
    }

    fn recompute_nudge_sum(&mut self) {
        let mut sum: i32 = 0;
        for slot in &self.nudge_ring {
            if slot.kind == NUDGE_SLOT_EMPTY {
                continue;
            }
            let Some(k) = nudge_from_byte(slot.kind) else {
                continue;
            };
            sum += i32::from(k.delta());
        }
        self.cached_nudge_sum = sum.clamp(-127, 127) as i8;
    }

    pub fn tick(&mut self, frame: u64) -> Option<ArcPhase> {
        self.frame_in_phase = self.frame_in_phase.saturating_add(1);

        // Nudge-decay : nudges older than 600 frames are dropped.
        let mut changed_any = false;
        for slot in &mut self.nudge_ring {
            if slot.kind != NUDGE_SLOT_EMPTY && frame.saturating_sub(slot.frame) > 600 {
                slot.kind = NUDGE_SLOT_EMPTY;
                changed_any = true;
            }
        }
        if changed_any {
            self.recompute_nudge_sum();
        }

        // Soft-cap : auto-advance even without nudges if we've been here
        // for too long.
        let cap = SOFT_CAP_FRAMES[self.phase as usize];
        if self.frame_in_phase > cap {
            return Some(self.advance_phase(frame));
        }

        // Min-dwell : refuse transitions before this minimum.
        let min = MIN_DWELL_FRAMES[self.phase as usize];
        if self.frame_in_phase < min {
            return None;
        }

        // Threshold check.
        let sum = self.cached_nudge_sum;
        if sum >= NUDGE_THRESHOLD {
            self.cached_nudge_sum = sum.saturating_sub(NUDGE_THRESHOLD);
            return Some(self.advance_phase(frame));
        }
        if sum <= -NUDGE_THRESHOLD {
            self.cached_nudge_sum = sum.saturating_add(NUDGE_THRESHOLD);
            return Some(self.regress_phase(frame));
        }

        None
    }

    fn advance_phase(&mut self, frame: u64) -> ArcPhase {
        let next = match self.phase {
            ArcPhase::Discovery => ArcPhase::Tension,
            ArcPhase::Tension => ArcPhase::Crisis,
            ArcPhase::Crisis => ArcPhase::Catharsis,
            ArcPhase::Catharsis => ArcPhase::Quiet,
            ArcPhase::Quiet => ArcPhase::Discovery,
        };
        self.transition(next, frame, "advance");
        next
    }

    fn regress_phase(&mut self, frame: u64) -> ArcPhase {
        let prev = match self.phase {
            ArcPhase::Discovery => ArcPhase::Discovery,
            ArcPhase::Tension => ArcPhase::Discovery,
            ArcPhase::Crisis => ArcPhase::Tension,
            ArcPhase::Catharsis => ArcPhase::Crisis,
            ArcPhase::Quiet => ArcPhase::Catharsis,
        };
        self.transition(prev, frame, "regress");
        prev
    }

    fn transition(&mut self, next: ArcPhase, frame: u64, why: &str) {
        if next == self.phase {
            return;
        }
        let from = self.phase.label();
        self.phase = next;
        self.frame_in_phase = 0;
        self.last_transition_frame = frame;
        self.total_transitions = self.total_transitions.saturating_add(1);
        log_event(
            "INFO",
            "loa-host/dm-arc",
            &format!(
                "transition · {} → {} · why={} · frame={} · total={}",
                from, next.label(), why, frame, self.total_transitions,
            ),
        );
    }

    pub fn reset(&mut self) {
        self.phase = ArcPhase::Discovery;
        self.frame_in_phase = 0;
        self.nudge_ring = [NudgeSlot {
            kind: NUDGE_SLOT_EMPTY,
            frame: 0,
        }; NUDGE_RING_LEN];
        self.nudge_write = 0;
        self.cached_nudge_sum = 0;
    }
}

fn nudge_from_byte(b: u8) -> Option<NudgeKind> {
    match b {
        0 => Some(NudgeKind::Rest),
        1 => Some(NudgeKind::Push),
        2 => Some(NudgeKind::Caution),
        3 => Some(NudgeKind::Curiosity),
        4 => Some(NudgeKind::Aggression),
        5 => Some(NudgeKind::Yield),
        _ => None,
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § TESTS
// ─────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper : drive ticks until a transition fires (or `cap` ticks elapse).
    /// Refills nudges every 400 frames so MIN_DWELL > decay-window cases work.
    fn drive_until_transition(
        a: &mut DmArc,
        start_frame: u64,
        cap: u64,
        refill: NudgeKind,
    ) -> (ArcPhase, u64) {
        if a.nudge_sum().abs() < NUDGE_THRESHOLD {
            for _ in 0..3 {
                a.push_nudge(refill, start_frame);
            }
        }
        for f in (start_frame + 1)..=(start_frame + cap) {
            if (f - start_frame) % 400 == 0 {
                for _ in 0..3 {
                    a.push_nudge(refill, f);
                }
            }
            if let Some(p) = a.tick(f) {
                return (p, f);
            }
        }
        panic!("no transition within {} ticks (cap)", cap);
    }

    #[test]
    fn arc_starts_in_discovery() {
        let a = DmArc::new();
        assert_eq!(a.phase(), ArcPhase::Discovery);
        assert_eq!(a.frame_in_phase(), 0);
        assert_eq!(a.nudge_sum(), 0);
        assert_eq!(a.total_transitions(), 0);
    }

    #[test]
    fn arc_advances_through_all_5_phases_in_order() {
        let mut a = DmArc::new();
        let (p1, f1) = drive_until_transition(
            &mut a, 0,
            (MIN_DWELL_FRAMES[ArcPhase::Discovery as usize] as u64) + 100,
            NudgeKind::Push,
        );
        assert_eq!(p1, ArcPhase::Tension);

        let (p2, f2) = drive_until_transition(
            &mut a, f1,
            (MIN_DWELL_FRAMES[ArcPhase::Tension as usize] as u64) + 100,
            NudgeKind::Aggression,
        );
        assert_eq!(p2, ArcPhase::Crisis);

        let (p3, f3) = drive_until_transition(
            &mut a, f2,
            (MIN_DWELL_FRAMES[ArcPhase::Crisis as usize] as u64) + 200,
            NudgeKind::Push,
        );
        assert_eq!(p3, ArcPhase::Catharsis);

        let (p4, f4) = drive_until_transition(
            &mut a, f3,
            (MIN_DWELL_FRAMES[ArcPhase::Catharsis as usize] as u64) + 200,
            NudgeKind::Push,
        );
        assert_eq!(p4, ArcPhase::Quiet);

        let (p5, _f5) = drive_until_transition(
            &mut a, f4,
            (MIN_DWELL_FRAMES[ArcPhase::Quiet as usize] as u64) + 200,
            NudgeKind::Push,
        );
        assert_eq!(p5, ArcPhase::Discovery);

        assert!(a.total_transitions() >= 5, "should have seen ≥5 transitions");
    }

    #[test]
    fn arc_min_dwell_blocks_premature_transition() {
        let mut a = DmArc::new();
        for _ in 0..4 {
            a.push_nudge(NudgeKind::Push, 0);
        }
        for f in 1..=10 {
            let r = a.tick(f);
            assert_eq!(r, None, "min-dwell should block transition at frame {}", f);
        }
        assert_eq!(a.phase(), ArcPhase::Discovery);
    }

    #[test]
    fn arc_soft_cap_auto_advances() {
        let mut a = DmArc::new();
        let cap = SOFT_CAP_FRAMES[ArcPhase::Discovery as usize];
        let mut transitioned = false;
        for f in 1..=(cap + 10) as u64 {
            if a.tick(f).is_some() {
                transitioned = true;
                break;
            }
        }
        assert!(transitioned, "soft-cap should force advance from Discovery");
        assert_eq!(a.phase(), ArcPhase::Tension);
    }

    #[test]
    fn arc_regress_works_from_tension() {
        let mut a = DmArc::new();
        let (p1, f1) = drive_until_transition(
            &mut a, 0,
            (MIN_DWELL_FRAMES[ArcPhase::Discovery as usize] as u64) + 50,
            NudgeKind::Push,
        );
        assert_eq!(p1, ArcPhase::Tension);

        // Wait for the original Push-nudges to decay before pushing Rest.
        let mut frame = f1;
        while frame < 700 {
            frame += 1;
            let _ = a.tick(frame);
        }
        for _ in 0..3 {
            a.push_nudge(NudgeKind::Rest, frame);
        }
        for _ in 0..1 {
            a.push_nudge(NudgeKind::Yield, frame);
        }

        let cap = (MIN_DWELL_FRAMES[ArcPhase::Tension as usize] as u64) + 200;
        let (p2, _) = drive_until_transition(&mut a, frame, cap, NudgeKind::Rest);
        assert_eq!(p2, ArcPhase::Discovery);
    }

    #[test]
    fn arc_regress_clamped_at_discovery() {
        let mut a = DmArc::new();
        for _ in 0..4 {
            a.push_nudge(NudgeKind::Yield, 0);
        }
        let dwell = MIN_DWELL_FRAMES[ArcPhase::Discovery as usize];
        for f in 1..=(dwell + 5) as u64 {
            let r = a.tick(f);
            assert_ne!(r, Some(ArcPhase::Quiet), "must not wrap-regress");
        }
        assert_eq!(a.phase(), ArcPhase::Discovery);
    }

    #[test]
    fn arc_knobs_per_phase_distinct() {
        for i in 0..ARC_PHASE_COUNT {
            for j in (i + 1)..ARC_PHASE_COUNT {
                assert_ne!(
                    PHASE_KNOBS[i], PHASE_KNOBS[j],
                    "phase {} and {} share knobs",
                    i, j
                );
            }
        }
    }

    #[test]
    fn arc_knobs_in_canonical_ranges() {
        for k in &PHASE_KNOBS {
            assert!((0.0..=1.0).contains(&k.monster_density));
            assert!((0.0..=1.0).contains(&k.ambient_music_intensity));
            assert!((0.0..=1.0).contains(&k.loot_richness));
            assert!((-1.0..=1.0).contains(&k.npc_mood_bias));
        }
    }

    #[test]
    fn arc_nudge_decay_drops_old_entries() {
        let mut a = DmArc::new();
        a.push_nudge(NudgeKind::Push, 0);
        a.push_nudge(NudgeKind::Push, 0);
        assert_eq!(a.nudge_sum(), 4);
        let _ = a.tick(700);
        assert_eq!(a.nudge_sum(), 0, "nudges older than 600 frames must decay");
    }

    #[test]
    fn classify_nudge_basic_keywords() {
        assert_eq!(classify_nudge("I want to rest"), Some(NudgeKind::Rest));
        assert_eq!(classify_nudge("anyone left? push"), Some(NudgeKind::Push));
        assert_eq!(classify_nudge("attack!"), Some(NudgeKind::Aggression));
        assert_eq!(classify_nudge("flee the room"), Some(NudgeKind::Yield));
        assert_eq!(classify_nudge("what is this"), None);
    }

    #[test]
    fn arc_reset_returns_to_discovery() {
        let mut a = DmArc::new();
        for _ in 0..3 {
            a.push_nudge(NudgeKind::Push, 0);
        }
        let dwell = MIN_DWELL_FRAMES[ArcPhase::Discovery as usize];
        for f in 1..=(dwell + 5) as u64 {
            let _ = a.tick(f);
        }
        assert_eq!(a.phase(), ArcPhase::Tension);
        a.reset();
        assert_eq!(a.phase(), ArcPhase::Discovery);
        assert_eq!(a.nudge_sum(), 0);
    }
}
