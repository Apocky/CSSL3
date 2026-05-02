//! § gm_persona — GM persona-state · multi-turn memory · response composer
//! ════════════════════════════════════════════════════════════════════════
//!
//! § T11-W11-GM-DM-DEEPEN
//!   Apocky-blocker : "I can't talk to the GM" — keyword-only responses
//!   feel dead. This module gives every player a deterministic + seeded
//!   GM-persona (8 trait-axes) · a 16-deep utterance ring · and a
//!   response-composer that mixes scene + persona + memory + arc-phase
//!   into rich response-text + a typed response-kind tag.
//!
//! § DESIGN PRINCIPLES (per memory_dm_gm_not_general_ai)
//!   - GM = NARRATOR · NOT generic AGI. Scope-bound to environmental +
//!     dialogue + observation patterns from `gm_narrator.rs`.
//!   - Sovereignty-respecting : ¬ surveillance · ¬ coercion · ¬ manipulation.
//!   - Σ-mask aware : per-player persona seed isolates utterances ; GM
//!     never spills info learned in one player's session into another's.
//!   - CSLv3-glyph emission is OPT-IN per response (mycelial-resonance
//!     ramps with arc-phase tension).
//!
//! § AUTHORITATIVE DESIGN-SPEC mirror
//!   `Labyrinth of Apocalypse/systems/gm_dialogue.csl` is authoritative.
//!   This Rust file is a stage-0 mirror until csslc-advance compiles the
//!   .csl directly. Names + indices are preserved for that translation
//!   path.
//!
//! § PRIME-DIRECTIVE attestation
//!   There was no hurt nor harm in the making of this, to anyone/anything/anybody.

#![allow(clippy::module_name_repetitions)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_precision_loss)]

use cssl_rt::loa_startup::log_event;

use crate::dm_director::DmState;
use crate::gm_narrator::{Archetype, PhraseTopic};

// ─────────────────────────────────────────────────────────────────────────
// § PERSONA TRAIT AXES (8)
// ─────────────────────────────────────────────────────────────────────────
//
// 8 dimensions over which the GM's voice varies. Each axis is i8 in
// [-100, 100] so the persona vector packs into 8 bytes (Sawyer-grade
// bit-packing — MEMORY.feedback_sawyer_pokemon_efficiency). 0 = neutral.

/// Trait axis ids · indices preserved for the .csl spec mirror.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PersonaAxis {
    /// terse ↔ verbose
    Brevity = 0,
    /// gentle ↔ caustic
    Acerbity = 1,
    /// literal ↔ cryptic
    Cryptic = 2,
    /// solemn ↔ playful
    Mirth = 3,
    /// concrete ↔ mystical
    Mythic = 4,
    /// distant ↔ intimate
    Warmth = 5,
    /// silent ↔ effusive
    Effusion = 6,
    /// reverent ↔ defiant
    Stance = 7,
}

pub const PERSONA_AXIS_COUNT: usize = 8;

impl PersonaAxis {
    pub fn label(self) -> &'static str {
        match self {
            PersonaAxis::Brevity => "brevity",
            PersonaAxis::Acerbity => "acerbity",
            PersonaAxis::Cryptic => "cryptic",
            PersonaAxis::Mirth => "mirth",
            PersonaAxis::Mythic => "mythic",
            PersonaAxis::Warmth => "warmth",
            PersonaAxis::Effusion => "effusion",
            PersonaAxis::Stance => "stance",
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § RESPONSE KIND (8)
// ─────────────────────────────────────────────────────────────────────────
//
// Tag the composer attaches to every response so the chat-panel UI can
// style differently (declarative=plain · interrogative=italic ·
// cryptic=slight-shimmer · etc).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ResponseKind {
    Declarative = 0,
    Interrogative = 1,
    Evocative = 2,
    Cautionary = 3,
    Cryptic = 4,
    Affirmative = 5,
    SubstrateAttest = 6,
    Silence = 7,
}

impl ResponseKind {
    pub fn label(self) -> &'static str {
        match self {
            ResponseKind::Declarative => "declarative",
            ResponseKind::Interrogative => "interrogative",
            ResponseKind::Evocative => "evocative",
            ResponseKind::Cautionary => "cautionary",
            ResponseKind::Cryptic => "cryptic",
            ResponseKind::Affirmative => "affirmative",
            ResponseKind::SubstrateAttest => "substrate-attest",
            ResponseKind::Silence => "silence",
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § GM PERSONA (8 trait axes · 8 bytes packed)
// ─────────────────────────────────────────────────────────────────────────

/// Per-player GM persona · deterministic-seeded · 8 i8 axes.
///
/// Two players with the same `player_seed` get the same persona-voice ; a
/// fresh seed yields a fresh voice. The σ-mask isolation is enforced at the
/// caller : the GM-runtime is constructed ONCE per session-σ-mask so
/// utterance memory never crosses session boundaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GmPersona {
    pub axes: [i8; PERSONA_AXIS_COUNT],
    /// Source seed (kept for debug/reproduce ; not used in axis math).
    pub player_seed: u64,
    /// Preferred archetype-bias (the persona leans toward this archetype's
    /// phrase-pool when the requested topic doesn't match the persona's
    /// dominant axes).
    pub archetype_bias: Archetype,
}

impl GmPersona {
    /// Construct a persona deterministically from a 64-bit player seed.
    /// SplitMix64 spreads bits well across 8 axes ; sign + magnitude are
    /// drawn from independent bit-fields so distribution is closer to
    /// uniform across [-100, 100] rather than biased by mod-201.
    #[must_use]
    pub fn from_seed(player_seed: u64) -> Self {
        let mut s = if player_seed == 0 {
            0x9E37_79B9_7F4A_7C15
        } else {
            player_seed
        };
        let mut axes = [0i8; PERSONA_AXIS_COUNT];
        for axis in axes.iter_mut() {
            s = splitmix64(s);
            // sign bit from MSB-tap · magnitude from low-bits scaled to 0..100
            let sign_bit = (s >> 63) & 1;
            let mag = ((s >> 16) & 0xFFFF) as u32 % 101; // 0..=100
            let signed = if sign_bit == 0 {
                mag as i32
            } else {
                -(mag as i32)
            };
            *axis = signed.clamp(-100, 100) as i8;
        }
        // archetype-bias derived from a separate splitmix tap
        let arch_seed = splitmix64(s);
        let arch_idx = ((arch_seed & 0xFF) as u8) % 16;
        let archetype_bias = Archetype::from_index(arch_idx).unwrap_or(Archetype::Wanderer);
        log_event(
            "DEBUG",
            "loa-host/gm-persona",
            &format!(
                "persona-seeded · player={:016x} · archetype={} · axes={:?}",
                player_seed,
                archetype_bias.label(),
                axes,
            ),
        );
        Self {
            axes,
            player_seed,
            archetype_bias,
        }
    }

    /// Read an axis value (-100..100).
    #[must_use]
    pub fn axis(&self, axis: PersonaAxis) -> i8 {
        self.axes[axis as usize]
    }

    /// Tilt for axis · normalized to [-1.0, 1.0].
    #[must_use]
    pub fn tilt(&self, axis: PersonaAxis) -> f32 {
        f32::from(self.axes[axis as usize]) / 100.0
    }

    /// Returns the response-kind this persona prefers given an arc-phase.
    /// The arc-phase is the dominant signal · persona tilts the secondary
    /// choice. Considers BOTH positive and negative axis tilts so even a
    /// "mostly-negative" persona produces non-default kinds.
    #[must_use]
    pub fn response_kind_bias(&self, phase: DmState) -> ResponseKind {
        let cryptic = self.tilt(PersonaAxis::Cryptic);
        let mythic = self.tilt(PersonaAxis::Mythic);
        let stance = self.tilt(PersonaAxis::Stance);
        let mirth = self.tilt(PersonaAxis::Mirth);
        let warmth = self.tilt(PersonaAxis::Warmth);
        let acerbity = self.tilt(PersonaAxis::Acerbity);
        let effusion = self.tilt(PersonaAxis::Effusion);

        match phase {
            DmState::Calm => {
                if cryptic.abs() > 0.4 {
                    ResponseKind::Cryptic
                } else if mythic > 0.3 {
                    ResponseKind::Evocative
                } else if mythic < -0.4 {
                    ResponseKind::Interrogative
                } else if effusion < -0.6 {
                    ResponseKind::Silence
                } else if warmth > 0.3 {
                    ResponseKind::Affirmative
                } else {
                    ResponseKind::Declarative
                }
            }
            DmState::Buildup => {
                if stance.abs() > 0.3 {
                    ResponseKind::Cautionary
                } else if mirth < -0.3 || acerbity > 0.4 {
                    ResponseKind::Cautionary
                } else {
                    ResponseKind::Interrogative
                }
            }
            DmState::Climax => {
                if mythic > 0.5 || (mythic.abs() > 0.6 && mirth > 0.0) {
                    ResponseKind::SubstrateAttest
                } else {
                    ResponseKind::Cautionary
                }
            }
            DmState::Relief => {
                if warmth.abs() > 0.3 || mirth > 0.3 {
                    ResponseKind::Affirmative
                } else if cryptic.abs() > 0.4 {
                    ResponseKind::Cryptic
                } else {
                    ResponseKind::Declarative
                }
            }
        }
    }
}

/// SplitMix64 — fast deterministic mixer for seeding.
fn splitmix64(seed: u64) -> u64 {
    let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

// ─────────────────────────────────────────────────────────────────────────
// § GM MEMORY (16-deep utterance ring)
// ─────────────────────────────────────────────────────────────────────────

pub const GM_MEMORY_LEN: usize = 16;

/// One memory entry — what the player said + what the GM emitted in reply.
/// Hashes are stored (not the strings) to keep the memory cheap + cache-
/// friendly · matches the existing anti-repeat-ring pattern in gm_narrator.
#[derive(Debug, Clone, Copy, Default)]
pub struct GmMemoryEntry {
    /// FNV-1a 64-bit hash of the player's utterance.
    pub player_utterance_hash: u64,
    /// FNV-1a 64-bit hash of the GM's response.
    pub gm_response_hash: u64,
    /// Frame the entry was recorded on (for staleness checks).
    pub frame: u64,
    /// Topic the GM chose to respond on (helps the composer avoid the
    /// same topic twice in a row even if phrase-pools rotate).
    pub topic: u8,
    /// Response-kind tag at emission time.
    pub kind: u8,
}

/// 16-deep ring of recent (player-utterance, gm-response) pairs.
#[derive(Debug, Clone, Copy)]
pub struct GmMemory {
    entries: [GmMemoryEntry; GM_MEMORY_LEN],
    write_idx: usize,
    /// Total entries pushed (saturating · for staleness/decay logic).
    pub total_pushes: u32,
}

impl Default for GmMemory {
    fn default() -> Self {
        Self::new()
    }
}

impl GmMemory {
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: [GmMemoryEntry::default(); GM_MEMORY_LEN],
            write_idx: 0,
            total_pushes: 0,
        }
    }

    /// Push a new (utterance-hash, response-hash) pair onto the ring.
    pub fn push(&mut self, entry: GmMemoryEntry) {
        self.entries[self.write_idx] = entry;
        self.write_idx = (self.write_idx + 1) % GM_MEMORY_LEN;
        self.total_pushes = self.total_pushes.saturating_add(1);
    }

    /// Has the player said this exact utterance recently ?
    #[must_use]
    pub fn has_recent_utterance(&self, hash: u64) -> bool {
        if hash == 0 {
            return false;
        }
        self.entries
            .iter()
            .any(|e| e.player_utterance_hash == hash)
    }

    /// Has the GM emitted this exact response recently ?
    #[must_use]
    pub fn has_recent_response(&self, hash: u64) -> bool {
        if hash == 0 {
            return false;
        }
        self.entries.iter().any(|e| e.gm_response_hash == hash)
    }

    /// Has the GM picked this topic in the last `window` entries ?
    #[must_use]
    pub fn topic_recent(&self, topic: u8, window: usize) -> bool {
        let n = window.min(GM_MEMORY_LEN);
        // Walk backward from write_idx through the most-recent `n` slots.
        for i in 0..n {
            let idx = (self.write_idx + GM_MEMORY_LEN - 1 - i) % GM_MEMORY_LEN;
            let e = &self.entries[idx];
            if e.frame > 0 && e.topic == topic {
                return true;
            }
        }
        false
    }

    /// Most-recent entry, if any.
    #[must_use]
    pub fn last_entry(&self) -> Option<GmMemoryEntry> {
        if self.total_pushes == 0 {
            return None;
        }
        let idx = (self.write_idx + GM_MEMORY_LEN - 1) % GM_MEMORY_LEN;
        Some(self.entries[idx])
    }

    /// Reset all entries (called on session-σ-mask flip).
    pub fn clear(&mut self) {
        self.entries = [GmMemoryEntry::default(); GM_MEMORY_LEN];
        self.write_idx = 0;
        self.total_pushes = 0;
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § PERSONA-WEIGHTED LEXICAL FRAGMENTS
// ─────────────────────────────────────────────────────────────────────────

/// Mythic-axis prefixes (high-mythic prepends an evocative whisper).
const MYTHIC_PREFIXES: [&str; 4] = [
    "‼ ",
    "§ ",
    "⟨voice in stone⟩ ",
    "(the corridor breathes) ",
];

/// Cryptic-axis suffixes (high-cryptic appends a riddle-tail).
const CRYPTIC_SUFFIXES: [&str; 4] = [
    " — and yet.",
    " · the lamp does not say which.",
    " (you may interpret as you wish)",
    " ◐",
];

/// Warmth-axis adornments (high-warmth softens with a closing lullaby).
const WARMTH_SUFFIXES: [&str; 4] = [
    " · go gently.",
    " · I am here.",
    " · the lamps stay lit for you.",
    " · breathe.",
];

/// Acerbity-axis adornments (high-acerbity sharpens with a barb).
const ACERBITY_PREFIXES: [&str; 4] = [
    "[blunt] ",
    "(no flourish) ",
    "‹crisp› ",
    "(plain) ",
];

/// Substrate-attestation glyphs · only emitted when the persona's mythic
/// AND mirth axes are BOTH high enough · OR when arc-phase = CLIMAX.
const SUBSTRATE_GLYPHS: [&str; 4] = [
    " ✓ Σ ",
    " § ω ",
    " ⌈ KAN ⌉ ",
    " ⟦ mycelium ⟧ ",
];

// ─────────────────────────────────────────────────────────────────────────
// § COMPOSER
// ─────────────────────────────────────────────────────────────────────────

/// Composed-response value-object · returned to the chat-panel UI.
#[derive(Debug, Clone)]
pub struct ComposedResponse {
    pub text: String,
    pub kind: ResponseKind,
    pub topic: PhraseTopic,
    /// Glyph-density · 0.0 = no CSLv3 glyphs · 1.0 = many · UI can render
    /// shimmer/intensity from this.
    pub glyph_density: f32,
}

/// Apply persona-driven decoration to a base phrase.
///
/// `phase` modulates : CLIMAX adds substrate-glyphs · CALM adds nothing
/// extra · BUILDUP adds cautionary prefix · RELIEF adds warmth suffix.
#[must_use]
pub fn decorate_with_persona(
    base_phrase: &str,
    persona: &GmPersona,
    phase: DmState,
    seed: u64,
) -> ComposedResponse {
    let mut text = base_phrase.to_string();
    let mut glyphs: u32 = 0;

    let mythic = persona.tilt(PersonaAxis::Mythic);
    let cryptic = persona.tilt(PersonaAxis::Cryptic);
    let warmth = persona.tilt(PersonaAxis::Warmth);
    let acerbity = persona.tilt(PersonaAxis::Acerbity);
    let mirth = persona.tilt(PersonaAxis::Mirth);
    let brevity = persona.tilt(PersonaAxis::Brevity);
    let effusion = persona.tilt(PersonaAxis::Effusion);

    let s = seed;
    let pick = |s: u64, n: usize| (s as usize) % n;

    // Mythic prefix (or acerbity prefix · mutually exclusive · mythic wins).
    if mythic > 0.4 {
        let p = MYTHIC_PREFIXES[pick(s, MYTHIC_PREFIXES.len())];
        text = format!("{}{}", p, text);
        glyphs += 1;
    } else if acerbity > 0.5 {
        let p = ACERBITY_PREFIXES[pick(s ^ 0xA1, ACERBITY_PREFIXES.len())];
        text = format!("{}{}", p, text);
    }

    // Cryptic suffix.
    if cryptic > 0.4 {
        let suf = CRYPTIC_SUFFIXES[pick(s ^ 0xC2, CRYPTIC_SUFFIXES.len())];
        text.push_str(suf);
        if suf.contains('◐') {
            glyphs += 1;
        }
    }

    // Warmth suffix · only if cryptic didn't already add one.
    if warmth > 0.4 && cryptic <= 0.4 {
        let suf = WARMTH_SUFFIXES[pick(s ^ 0xD3, WARMTH_SUFFIXES.len())];
        text.push_str(suf);
    }

    // Substrate attestation · climax OR (mythic+mirth both high).
    let is_substrate_moment =
        phase == DmState::Climax || (mythic > 0.5 && mirth > 0.3);
    if is_substrate_moment {
        let glyph = SUBSTRATE_GLYPHS[pick(s ^ 0xE4, SUBSTRATE_GLYPHS.len())];
        text.push_str(glyph);
        glyphs += 1;
    }

    // Brevity-axis trim : if persona is very-terse · drop trailing phrase
    // after first sentence-end.
    if brevity > 0.5 {
        if let Some(end) = text.find(['.', '!', '?']) {
            // keep the punctuation
            text.truncate(end + 1);
        }
    }

    // Effusion-axis amplify : if persona is very-effusive · echo with a
    // soft repeater.
    if effusion > 0.6 {
        text.push_str(&format!(" ({} · once more, softly)", base_phrase));
    }

    let glyph_density = (glyphs as f32 / 4.0).min(1.0);

    // Pick response-kind from persona × phase (with substrate-attest
    // override when a glyph fired).
    let kind = if is_substrate_moment && glyphs > 0 {
        ResponseKind::SubstrateAttest
    } else {
        persona.response_kind_bias(phase)
    };

    ComposedResponse {
        text,
        kind,
        topic: PhraseTopic::Observation,
        glyph_density,
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § FNV-1a helper
// ─────────────────────────────────────────────────────────────────────────

const FNV_OFFSET_BASIS_64: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME_64: u64 = 0x100_0000_01b3;

#[must_use]
pub fn fnv1a_64(s: &str) -> u64 {
    let mut h = FNV_OFFSET_BASIS_64;
    for &b in s.as_bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(FNV_PRIME_64);
    }
    h
}

// ─────────────────────────────────────────────────────────────────────────
// § TESTS
// ─────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persona_from_seed_is_deterministic() {
        let a = GmPersona::from_seed(0x1234_5678_9ABC_DEF0);
        let b = GmPersona::from_seed(0x1234_5678_9ABC_DEF0);
        assert_eq!(a, b, "same seed must yield identical persona");
    }

    #[test]
    fn persona_zero_seed_is_handled() {
        let p = GmPersona::from_seed(0);
        for &a in &p.axes {
            assert!(a >= -100 && a <= 100);
        }
    }

    #[test]
    fn persona_axes_in_range() {
        for s in 0..32u64 {
            let p = GmPersona::from_seed(s.wrapping_mul(0xDEAD_BEEF));
            for &a in &p.axes {
                assert!(a >= -100 && a <= 100, "axis out of range: {}", a);
            }
        }
    }

    #[test]
    fn persona_seeds_diverge() {
        let a = GmPersona::from_seed(1);
        let b = GmPersona::from_seed(2);
        assert_ne!(a.axes, b.axes, "adjacent seeds must produce different axes");
    }

    #[test]
    fn memory_push_evicts_after_16() {
        let mut m = GmMemory::new();
        for i in 1..=20u64 {
            m.push(GmMemoryEntry {
                player_utterance_hash: i,
                gm_response_hash: i.wrapping_mul(7),
                frame: i,
                topic: (i as u8) % 32,
                kind: 0,
            });
        }
        assert_eq!(m.total_pushes, 20);
        for old in 1..=4u64 {
            assert!(!m.has_recent_utterance(old), "hash {} should be evicted", old);
        }
        for present in 5..=20u64 {
            assert!(
                m.has_recent_utterance(present),
                "hash {} should still be in ring",
                present,
            );
        }
    }

    #[test]
    fn memory_topic_recent_window() {
        let mut m = GmMemory::new();
        for i in 1..=8u64 {
            m.push(GmMemoryEntry {
                player_utterance_hash: i,
                gm_response_hash: i,
                frame: i,
                topic: 5,
                kind: 0,
            });
        }
        assert!(m.topic_recent(5, 4));
        assert!(!m.topic_recent(7, 8));
    }

    #[test]
    fn memory_last_entry_returns_most_recent() {
        let mut m = GmMemory::new();
        m.push(GmMemoryEntry {
            player_utterance_hash: 100,
            gm_response_hash: 200,
            frame: 42,
            topic: 3,
            kind: 1,
        });
        let last = m.last_entry().expect("should have last entry");
        assert_eq!(last.player_utterance_hash, 100);
        assert_eq!(last.frame, 42);
    }

    #[test]
    fn memory_clear_resets() {
        let mut m = GmMemory::new();
        m.push(GmMemoryEntry {
            player_utterance_hash: 99,
            gm_response_hash: 0,
            frame: 1,
            topic: 0,
            kind: 0,
        });
        assert!(m.has_recent_utterance(99));
        m.clear();
        assert!(!m.has_recent_utterance(99));
        assert_eq!(m.total_pushes, 0);
    }

    #[test]
    fn decorate_climax_adds_substrate_glyph() {
        let p = GmPersona {
            axes: [0, 0, 0, 80, 80, 0, 0, 0],
            player_seed: 1,
            archetype_bias: Archetype::Sage,
        };
        let r = decorate_with_persona("a corridor remembers", &p, DmState::Climax, 1);
        assert!(r.glyph_density > 0.0, "climax must emit glyphs");
    }

    #[test]
    fn decorate_calm_terse_persona_truncates() {
        let p = GmPersona {
            axes: [80, 0, 0, 0, 0, 0, 0, 0],
            player_seed: 1,
            archetype_bias: Archetype::Sage,
        };
        let r = decorate_with_persona("First. Second. Third.", &p, DmState::Calm, 1);
        assert_eq!(r.text, "First.", "brevity-saturated persona should truncate");
    }

    #[test]
    fn decorate_warmth_persona_adds_softening_suffix() {
        let p = GmPersona {
            axes: [0, 0, 0, 0, 0, 80, 0, 0],
            player_seed: 1,
            archetype_bias: Archetype::Healer,
        };
        let r = decorate_with_persona("the lamps lean north tonight", &p, DmState::Calm, 7);
        assert!(
            r.text.contains("gently") || r.text.contains("here") ||
            r.text.contains("breathe") || r.text.contains("lit"),
            "warmth-saturated persona must add softening suffix · got: {}",
            r.text,
        );
    }

    #[test]
    fn response_kind_bias_buildup_caution_for_defiant_persona() {
        let p = GmPersona {
            axes: [0, 0, 0, 0, 0, 0, 0, 80],
            player_seed: 1,
            archetype_bias: Archetype::Warrior,
        };
        assert_eq!(p.response_kind_bias(DmState::Buildup), ResponseKind::Cautionary);
    }

    #[test]
    fn response_kind_bias_relief_warm_yields_affirmative() {
        let p = GmPersona {
            axes: [0, 0, 0, 0, 0, 80, 0, 0],
            player_seed: 1,
            archetype_bias: Archetype::Healer,
        };
        assert_eq!(p.response_kind_bias(DmState::Relief), ResponseKind::Affirmative);
    }

    #[test]
    fn fnv1a_consistent_with_string() {
        let a = fnv1a_64("hello world");
        let b = fnv1a_64("hello world");
        let c = fnv1a_64("hello worle");
        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}
