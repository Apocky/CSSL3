//! § gm_narrator — GM narrator · STAGE-0-BOOTSTRAP-SHIM for substrate-intelligence
//! ══════════════════════════════════════════════════════════════════════════════
//!
//! § T11-W17-G+N · canonical-impl : `Labyrinth of Apocalypse/systems/gm_specialist.csl`
//!                                   `Labyrinth of Apocalypse/systems/substrate_intelligence.csl`
//!
//! § APOCKY-DIRECTIVE-2026-05-02
//!
//! >  "stop deferring and taking the easy route"
//! >  "PROCEDURAL EVERYTHING"
//! >  "PROPRIETARY local-intelligence"
//! >  "describe things in text or voice and they crystalize from the substrate"
//!
//! § WHAT CHANGED FROM THE PRE-W17 VERSION
//!
//! The prior gm_narrator was a phrase-pool xorshift32 looker-upper :
//!   - 32 pools × 4 phrases = 128 hardcoded strings
//!   - per-archetype preference table (16 × 4 = 64 indices)
//!   - 32-deep anti-repeat FNV-ring
//!   - xorshift32 PRNG for deterministic seeded picks
//!
//! Apocky vetoed all of it. "Scripted dialogue" is a violation of the
//! foundational PROCEDURAL-EVERYTHING axiom. This file is now a thin
//! BOOTSTRAP-SHIM that delegates ALL phrase generation to
//! `cssl-host-substrate-intelligence` — the proprietary local composition
//! engine that procedurally synthesizes morphologically-coherent text from
//! 8 substrate-resonance axes (NO phrase-pool lookups · NO scripted-tables).
//!
//! The PUBLIC API stays stable (Archetype / Mood / PhraseTopic / TimeOfDay
//! enums + GmNarrator struct + describe_environment / generate_dialogue /
//! respond_in_persona functions) so dependent modules
//! (gm_persona / dm_director / intent_router / mcp_tools) compile-without-
//! change. Only the IMPLEMENTATION of the dialogue-generation moved.
//!
//! § DETERMINISM CONTRACT (preserved)
//!
//! Same `(camera_pos · time_of_day)` → same environment description.
//! Same `(npc_id · archetype · mood · topic)` → same dialogue-line.
//! The substrate-intelligence composer is bit-deterministic given identical
//! inputs (BLAKE3 of the input tuple drives the 8-axis vector + entropy).
//!
//! § AUTHORITATIVE DESIGN SPECS
//!
//!   `Labyrinth of Apocalypse/systems/substrate_intelligence.csl`  · the engine
//!   `Labyrinth of Apocalypse/systems/gm_specialist.csl`            · GM dispatch
//!   `Labyrinth of Apocalypse/systems/intent_translation.csl`       · text/voice
//!   `Labyrinth of Apocalypse/systems/crystallization.csl`          · ω-field
//!   `Labyrinth of Apocalypse/systems/alien_materialization.csl`    · projection
//!   `Labyrinth of Apocalypse/systems/digital_intelligence_render.csl` · pipeline
//!
//! Constant + identifier names below are preserved verbatim from the prior
//! .csl design notes for the eventual stage-1 csslc translation path.

use cssl_host_substrate_intelligence as si;
use cssl_rt::loa_startup::log_event;

// ─────────────────────────────────────────────────────────────────────────
// § VEC3 (stage-0 ; pre-glam)
// ─────────────────────────────────────────────────────────────────────────

/// Stage-0 local Vec3. Sibling slices may swap for `glam::Vec3` when the
/// workspace adopts glam ; the narrator only consumes `(x, y, z)`.
#[derive(Debug, Clone, Copy, Default)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § ARCHETYPES (16) · public surface preserved
// ─────────────────────────────────────────────────────────────────────────

/// 16 NPC archetypes. Indices preserved verbatim from `scenes/gm_narrator.cssl`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Archetype {
    Sage = 0,
    Trickster = 1,
    Warrior = 2,
    Healer = 3,
    Merchant = 4,
    Hermit = 5,
    Apprentice = 6,
    Elder = 7,
    Scout = 8,
    Bard = 9,
    Smith = 10,
    Witness = 11,
    Wanderer = 12,
    Apocalyptic = 13,
    Liminal = 14,
    Mute = 15,
}

pub const ARCHETYPE_SAGE: Archetype = Archetype::Sage;
pub const ARCHETYPE_TRICKSTER: Archetype = Archetype::Trickster;
pub const ARCHETYPE_WARRIOR: Archetype = Archetype::Warrior;
pub const ARCHETYPE_HEALER: Archetype = Archetype::Healer;
pub const ARCHETYPE_MERCHANT: Archetype = Archetype::Merchant;
pub const ARCHETYPE_HERMIT: Archetype = Archetype::Hermit;
pub const ARCHETYPE_APPRENTICE: Archetype = Archetype::Apprentice;
pub const ARCHETYPE_ELDER: Archetype = Archetype::Elder;
pub const ARCHETYPE_SCOUT: Archetype = Archetype::Scout;
pub const ARCHETYPE_BARD: Archetype = Archetype::Bard;
pub const ARCHETYPE_SMITH: Archetype = Archetype::Smith;
pub const ARCHETYPE_WITNESS: Archetype = Archetype::Witness;
pub const ARCHETYPE_WANDERER: Archetype = Archetype::Wanderer;
pub const ARCHETYPE_APOCALYPTIC: Archetype = Archetype::Apocalyptic;
pub const ARCHETYPE_LIMINAL: Archetype = Archetype::Liminal;
pub const ARCHETYPE_MUTE: Archetype = Archetype::Mute;

pub const ARCHETYPE_COUNT: usize = 16;

impl Archetype {
    pub fn from_index(i: u8) -> Option<Self> {
        match i {
            0 => Some(Archetype::Sage),
            1 => Some(Archetype::Trickster),
            2 => Some(Archetype::Warrior),
            3 => Some(Archetype::Healer),
            4 => Some(Archetype::Merchant),
            5 => Some(Archetype::Hermit),
            6 => Some(Archetype::Apprentice),
            7 => Some(Archetype::Elder),
            8 => Some(Archetype::Scout),
            9 => Some(Archetype::Bard),
            10 => Some(Archetype::Smith),
            11 => Some(Archetype::Witness),
            12 => Some(Archetype::Wanderer),
            13 => Some(Archetype::Apocalyptic),
            14 => Some(Archetype::Liminal),
            15 => Some(Archetype::Mute),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Archetype::Sage => "Sage",
            Archetype::Trickster => "Trickster",
            Archetype::Warrior => "Warrior",
            Archetype::Healer => "Healer",
            Archetype::Merchant => "Merchant",
            Archetype::Hermit => "Hermit",
            Archetype::Apprentice => "Apprentice",
            Archetype::Elder => "Elder",
            Archetype::Scout => "Scout",
            Archetype::Bard => "Bard",
            Archetype::Smith => "Smith",
            Archetype::Witness => "Witness",
            Archetype::Wanderer => "Wanderer",
            Archetype::Apocalyptic => "Apocalyptic",
            Archetype::Liminal => "Liminal",
            Archetype::Mute => "Mute",
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § PHRASE TOPICS (32) · semantic-tag input to the substrate composer
// ─────────────────────────────────────────────────────────────────────────
//
// Preserved as a public enum because dependent modules (gm_persona, dm_arc)
// pass these to influence the semantic-axis-vector. The substrate-
// intelligence composer hashes the topic discriminant into the BLAKE3 seed
// so different topics produce different procedurally-composed text — but
// no string is ever "selected from a topic-pool". Composition is full
// morphological synthesis.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PhraseTopic {
    LoreHistory = 0,
    Environment = 1,
    Weather = 2,
    Creature = 3,
    Architecture = 4,
    Memory = 5,
    Prophecy = 6,
    Warning = 7,
    Greeting = 8,
    Farewell = 9,
    Mystery = 10,
    Hope = 11,
    Despair = 12,
    Reverence = 13,
    Defiance = 14,
    Mourning = 15,
    Bargain = 16,
    Riddle = 17,
    Lullaby = 18,
    BattleCry = 19,
    Apology = 20,
    Boast = 21,
    Confession = 22,
    Question = 23,
    Direction = 24,
    Advice = 25,
    Joke = 26,
    Threat = 27,
    Promise = 28,
    Plea = 29,
    Observation = 30,
    Silence = 31,
}

pub const PHRASE_TOPIC_COUNT: usize = 32;

impl PhraseTopic {
    pub fn from_index(i: u8) -> Option<Self> {
        match i {
            0 => Some(PhraseTopic::LoreHistory),
            1 => Some(PhraseTopic::Environment),
            2 => Some(PhraseTopic::Weather),
            3 => Some(PhraseTopic::Creature),
            4 => Some(PhraseTopic::Architecture),
            5 => Some(PhraseTopic::Memory),
            6 => Some(PhraseTopic::Prophecy),
            7 => Some(PhraseTopic::Warning),
            8 => Some(PhraseTopic::Greeting),
            9 => Some(PhraseTopic::Farewell),
            10 => Some(PhraseTopic::Mystery),
            11 => Some(PhraseTopic::Hope),
            12 => Some(PhraseTopic::Despair),
            13 => Some(PhraseTopic::Reverence),
            14 => Some(PhraseTopic::Defiance),
            15 => Some(PhraseTopic::Mourning),
            16 => Some(PhraseTopic::Bargain),
            17 => Some(PhraseTopic::Riddle),
            18 => Some(PhraseTopic::Lullaby),
            19 => Some(PhraseTopic::BattleCry),
            20 => Some(PhraseTopic::Apology),
            21 => Some(PhraseTopic::Boast),
            22 => Some(PhraseTopic::Confession),
            23 => Some(PhraseTopic::Question),
            24 => Some(PhraseTopic::Direction),
            25 => Some(PhraseTopic::Advice),
            26 => Some(PhraseTopic::Joke),
            27 => Some(PhraseTopic::Threat),
            28 => Some(PhraseTopic::Promise),
            29 => Some(PhraseTopic::Plea),
            30 => Some(PhraseTopic::Observation),
            31 => Some(PhraseTopic::Silence),
            _ => None,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § TIME-OF-DAY · semantic-tag · folds into substrate-axis derivation
// ─────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TimeOfDay {
    Dawn = 0,
    Day = 1,
    Dusk = 2,
    Night = 3,
}

// ─────────────────────────────────────────────────────────────────────────
// § MOOD · semantic-tag · folds into substrate-axis derivation
// ─────────────────────────────────────────────────────────────────────────

/// Mood enum for `generate_dialogue`. Stable indices for FFI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Mood {
    Calm = 0,
    Anxious = 1,
    Hostile = 2,
    Friendly = 3,
    Sorrowful = 4,
    Reverent = 5,
    Playful = 6,
    Defiant = 7,
}

// ─────────────────────────────────────────────────────────────────────────
// § GMNarrator · public-surface preserved · STAGE-0-BOOTSTRAP-SHIM
// ─────────────────────────────────────────────────────────────────────────

/// GM narrator — procedural environment-description + dialogue generator.
///
/// All actual text composition lives in `cssl-host-substrate-intelligence`.
/// This struct is a stateless dispatcher (kept as a struct for backward-
/// compatibility with dependent modules that hold a mutable reference).
#[derive(Debug, Default)]
pub struct GmNarrator {
    /// Reserved for future per-NPC state. Stage-0 carries no state — every
    /// composition is fully derived from the inputs.
    _phantom: (),
}

impl GmNarrator {
    pub fn new() -> Self {
        log_event(
            "INFO",
            "loa-host/gm",
            "init · substrate-intelligence procedural composer · NO phrase-pools · NO scripted dialogue",
        );
        Self { _phantom: () }
    }

    /// Procedurally describe the neighborhood at `camera_pos` for the given
    /// `time_of_day`. Delegates composition to substrate-intelligence ; the
    /// (camera_pos, time_of_day) tuple folds into the BLAKE3 seed so spatial
    /// + temporal motion produce continuously-varying procedural narration.
    pub fn describe_environment(&mut self, camera_pos: Vec3, time_of_day: TimeOfDay) -> String {
        let seed = mix_pos_seed(camera_pos, time_of_day as u32);
        let s = si::compose_environment_description(seed as u64);
        let msg = format!(
            "describe-environment · pos=({:.2},{:.2},{:.2}) · tod={:?} · text={}",
            camera_pos.x, camera_pos.y, camera_pos.z, time_of_day, s,
        );
        log_event("DEBUG", "loa-host/gm", &msg);
        s
    }

    /// Generate a line of dialogue for `npc_id` (stable per-NPC id) in the
    /// given `mood`, on the given `topic`. Delegates composition to
    /// substrate-intelligence ; archetype / mood / topic fold into the seed
    /// so the same inputs reproduce the same line bit-for-bit.
    pub fn generate_dialogue(
        &mut self,
        npc_id: u32,
        archetype: Archetype,
        mood: Mood,
        topic: PhraseTopic,
    ) -> String {
        let seed = npc_id
            .wrapping_mul(0x9E37_79B9)
            .wrapping_add((mood as u32).wrapping_mul(0x85eb_ca6b))
            .wrapping_add(topic as u32);
        let s = si::compose_dialogue_line(archetype as u8, mood as u8, topic as u8, seed as u64);
        let msg = format!(
            "dialogue · npc={} · archetype={} · mood={:?} · topic={:?} · text={}",
            npc_id,
            archetype.label(),
            mood,
            topic,
            s,
        );
        log_event("DEBUG", "loa-host/gm", &msg);
        s
    }

    /// § T11-W11-GM-DM-DEEPEN · refactored for substrate-intelligence
    ///
    /// Compose a rich response by blending player-utterance + persona +
    /// arc-phase + dm-micro-phase into the substrate composer's seed, then
    /// running the existing persona-decoration pipeline on the result.
    /// The decoration pipeline (`gm_persona::decorate_with_persona`) is
    /// untouched — only the BASE PHRASE generation moved from phrase-pool
    /// lookup to substrate-procgen.
    pub fn respond_in_persona(
        &mut self,
        player_text: &str,
        persona: &crate::gm_persona::GmPersona,
        memory: &mut crate::gm_persona::GmMemory,
        arc_phase: crate::dm_arc::ArcPhase,
        dm_micro_phase: crate::dm_director::DmState,
        topic_hint: PhraseTopic,
        frame: u64,
        seed: u64,
    ) -> crate::gm_persona::ComposedResponse {
        use crate::gm_persona::{decorate_with_persona, fnv1a_64 as persona_fnv, GmMemoryEntry};

        // 1. Pick a topic — start from hint, but allow arc-phase to bias
        //    toward tension-appropriate topics. NO phrase-pool lookup ;
        //    topic just feeds into the substrate-composer's seed.
        let topic_choice = match arc_phase {
            crate::dm_arc::ArcPhase::Crisis => {
                // Crisis arc-phase favors urgent topics. Deterministic
                // round-robin off the seed (no random table).
                let crisis_topics = [
                    PhraseTopic::Warning,
                    PhraseTopic::Defiance,
                    PhraseTopic::BattleCry,
                    PhraseTopic::Threat,
                ];
                if seed.wrapping_mul(0xA5).wrapping_rem_euclid(3) == 0 {
                    crisis_topics[(seed as usize >> 8) % crisis_topics.len()]
                } else {
                    topic_hint
                }
            }
            crate::dm_arc::ArcPhase::Catharsis => {
                let cath_topics = [
                    PhraseTopic::Hope,
                    PhraseTopic::Promise,
                    PhraseTopic::Reverence,
                ];
                if seed.wrapping_mul(0xA5).wrapping_rem_euclid(3) == 0 {
                    cath_topics[(seed as usize >> 8) % cath_topics.len()]
                } else {
                    topic_hint
                }
            }
            crate::dm_arc::ArcPhase::Quiet => {
                let quiet_topics = [
                    PhraseTopic::Lullaby,
                    PhraseTopic::Memory,
                    PhraseTopic::Silence,
                ];
                if seed.wrapping_mul(0xA5).wrapping_rem_euclid(3) == 0 {
                    quiet_topics[(seed as usize >> 8) % quiet_topics.len()]
                } else {
                    topic_hint
                }
            }
            _ => topic_hint,
        };

        // 2. Avoid topic-recent if memory says it's a repeat.
        let topic = if memory.topic_recent(topic_choice as u8, 4) {
            // Round-robin to a different topic (deterministic from seed +
            // archetype-bias). NO archetype-preferences lookup table ; the
            // archetype number simply offsets the topic shift.
            let arch_byte = persona.archetype_bias as u8;
            let shift = arch_byte.wrapping_add((seed & 0xFF) as u8);
            PhraseTopic::from_index(((topic_choice as u8).wrapping_add(shift)) % 32)
                .unwrap_or(topic_choice)
        } else {
            topic_choice
        };

        // 3. Compose the BASE PHRASE through substrate-intelligence.
        //    Mix player_text hash into the seed so the base reflects
        //    what the player just said.
        let utterance_h = persona_fnv(player_text);
        let base_seed = seed
            .wrapping_mul(0x9E37_79B9_7F4A_7C15)
            .wrapping_add(utterance_h);
        let base_phrase = si::compose_dialogue_line(
            persona.archetype_bias as u8,
            (seed & 0x7) as u8, // mood-byte derived from seed
            topic as u8,
            base_seed,
        );

        // 4. Decorate with persona × dm-micro-phase (existing pipeline,
        //    untouched — only base-phrase source changed).
        let mut composed = decorate_with_persona(&base_phrase, persona, dm_micro_phase, seed);
        composed.topic = topic;

        // 5. Anti-loop: if the composed text matches a recent response
        //    hash, re-roll with a different seed. NO phrase-pool re-pick ;
        //    just a different substrate-composition seed.
        let resp_h = persona_fnv(&composed.text);
        if memory.has_recent_response(resp_h) {
            let alt_seed = base_seed.wrapping_mul(0xC2B2_AE35_8F1F_B0E1);
            let alt_phrase = si::compose_dialogue_line(
                persona.archetype_bias as u8,
                ((seed >> 3) & 0x7) as u8,
                topic as u8,
                alt_seed,
            );
            composed = decorate_with_persona(&alt_phrase, persona, dm_micro_phase, alt_seed);
            composed.topic = topic;
        }

        // 6. Push a memory entry.
        let entry = GmMemoryEntry {
            player_utterance_hash: persona_fnv(player_text),
            gm_response_hash: persona_fnv(&composed.text),
            frame,
            topic: topic as u8,
            kind: composed.kind as u8,
        };
        memory.push(entry);

        let log_msg = format!(
            "respond-in-persona · phase={:?} · arc={:?} · topic={:?} · kind={} · text={}",
            dm_micro_phase,
            arc_phase,
            topic,
            composed.kind.label(),
            composed.text,
        );
        log_event("DEBUG", "loa-host/gm", &log_msg);
        composed
    }
}

/// Mix Vec3 + time_of_day into a 32-bit seed for the substrate-composer.
/// (No PRNG state — the substrate-composer's BLAKE3 derivation handles all
/// entropy. This is just a bit-mixer to fold spatial+temporal coordinates
/// into a single u32.)
fn mix_pos_seed(p: Vec3, tod: u32) -> u32 {
    let xi = p.x.to_bits();
    let yi = p.y.to_bits();
    let zi = p.z.to_bits();
    let mut s = xi
        .wrapping_mul(0x9E37_79B9)
        .wrapping_add(yi.wrapping_mul(0x85eb_ca6b))
        .wrapping_add(zi.wrapping_mul(0xc2b2_ae35));
    s = s.wrapping_add(tod.wrapping_mul(0x27d4_eb2f));
    if s == 0 {
        0xdead_beef
    } else {
        s
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § TESTS
// ─────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gm_describe_environment_returns_non_empty() {
        let mut g = GmNarrator::new();
        let s = g.describe_environment(Vec3::new(1.0, 2.0, 3.0), TimeOfDay::Day);
        assert!(!s.is_empty());
        // Substrate composer emits at least a stem + punctuation.
        assert!(s.len() > 4);
    }

    #[test]
    fn gm_dialogue_returns_non_empty() {
        let mut g = GmNarrator::new();
        let s = g.generate_dialogue(42, Archetype::Sage, Mood::Calm, PhraseTopic::LoreHistory);
        assert!(!s.is_empty());
        assert!(s.len() > 4);
    }

    #[test]
    fn gm_dialogue_is_deterministic() {
        let mut a = GmNarrator::new();
        let mut b = GmNarrator::new();
        let s1 = a.generate_dialogue(7, Archetype::Bard, Mood::Playful, PhraseTopic::Joke);
        let s2 = b.generate_dialogue(7, Archetype::Bard, Mood::Playful, PhraseTopic::Joke);
        assert_eq!(s1, s2, "same inputs must produce same dialogue");
    }

    #[test]
    fn gm_dialogue_varies_with_npc_id() {
        let mut g = GmNarrator::new();
        let s1 = g.generate_dialogue(1, Archetype::Bard, Mood::Playful, PhraseTopic::Joke);
        let s2 = g.generate_dialogue(2, Archetype::Bard, Mood::Playful, PhraseTopic::Joke);
        assert_ne!(s1, s2, "different npc ids must produce different dialogue");
    }

    #[test]
    fn gm_environment_varies_with_time_of_day() {
        let mut g = GmNarrator::new();
        let pos = Vec3::new(0.0, 0.0, 0.0);
        let s_day = g.describe_environment(pos, TimeOfDay::Day);
        let s_night = g.describe_environment(pos, TimeOfDay::Night);
        assert_ne!(s_day, s_night, "time-of-day must shift the procedural axis");
    }

    #[test]
    fn archetype_round_trip() {
        for i in 0..16u8 {
            let a = Archetype::from_index(i).unwrap();
            assert_eq!(a as u8, i);
            assert!(!a.label().is_empty());
        }
        assert!(Archetype::from_index(99).is_none());
    }

    #[test]
    fn phrase_topic_round_trip() {
        for i in 0..32u8 {
            let t = PhraseTopic::from_index(i).unwrap();
            assert_eq!(t as u8, i);
        }
        assert!(PhraseTopic::from_index(99).is_none());
    }
}
