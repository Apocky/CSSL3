// § T11-W5b-INPUT-VIRTUAL : high-level scenario generators
// ══════════════════════════════════════════════════════════════════
//! High-level scenario generators that compose the low-level keystrokes /
//! mouse-paths primitives into common test shapes.
//!
//! Three public scenarios :
//!
//! - [`navigate_test_room`] : 30-second WASD-tour with mouse-look — what a
//!   QA agent would do to sanity-check the test room renders + responds
//!   to first-person controls.
//! - [`type_intent_phrase`] : `/`-key (focus intent box) → typed phrase →
//!   `Enter` (commit intent).  Mirrors the LoA UI flow for natural-language
//!   commands.
//! - [`full_qa_session`] : draw `n_intents` phrases from a built-in 16-phrase
//!   pool and chain them with idle gaps — produces a realistic-looking
//!   multi-turn QA session for visual-regression goldens.
//!
//! All scenarios are deterministic functions of `(seed, ...)`.

use cssl_host_replay::{ReplayEvent, ReplayEventKind};

use crate::keystrokes::ascii_text_to_keystrokes;
use crate::mouse_paths::random_walk;
use crate::rng::Pcg32;

/// § Standard ASCII keycodes for WASD + special keys used in scenarios.
const KEY_W: u32 = b'w' as u32;
const KEY_A: u32 = b'a' as u32;
const KEY_S: u32 = b's' as u32;
const KEY_D: u32 = b'd' as u32;
const KEY_SLASH: u32 = b'/' as u32;
const KEY_ENTER: u32 = 0x0A;

/// § Built-in intent-phrase pool used by `full_qa_session`.
///
/// Sixteen intentionally diverse phrases : navigation, observation, action,
/// dialogue, abstract.  This is content not configuration — the test
/// harness pins on this exact pool.
const PHRASE_POOL: [&str; 16] = [
    "look at the door",
    "walk north slowly",
    "open the chest",
    "ask the guard about the keys",
    "examine the painting",
    "pick up the lantern",
    "read the inscription",
    "listen for footsteps",
    "wait until midnight",
    "follow the corridor east",
    "search the bookshelf",
    "talk to the merchant",
    "rest by the fire",
    "draw the sword",
    "study the map",
    "remember the symbol",
];

/// § Default per-character delay for typed phrases (≈ 80 WPM).
const TYPE_CHAR_DELAY_MICROS: u64 = 150_000;

/// § Default mouse-walk step for the navigate scenario.
const MOUSE_STEP: f32 = 3.5;

/// Synthesize a 30-second WASD navigation + mouse-look stream for the test room.
///
/// Emits :
/// 1. `RngSeed(seed)` at t=0 (so the replay re-seeds the engine RNG).
/// 2. Three WASD bursts at t=2s, t=10s, t=18s — each a Down/Up pair on a
///    seed-selected key.
/// 3. A continuous mouse-look stream (Gaussian-walk @ 60 Hz) over the full duration.
#[must_use]
pub fn navigate_test_room(seed: u64) -> Vec<ReplayEvent> {
    let mut rng = Pcg32::new(seed);
    let mut events = Vec::with_capacity(2048);

    // 1. RNG seed marker at t=0.
    events.push(ReplayEvent::new(0, ReplayEventKind::RngSeed(seed)));

    // 2. Three WASD bursts.
    let wasd_keys = [KEY_W, KEY_A, KEY_S, KEY_D];
    let burst_starts_micros: [u64; 3] = [2_000_000, 10_000_000, 18_000_000];
    for &start in &burst_starts_micros {
        let key_idx = rng.range_u32(0, wasd_keys.len() as u32) as usize;
        let key = wasd_keys[key_idx];
        events.push(ReplayEvent::new(start, ReplayEventKind::KeyDown(key)));
        events.push(ReplayEvent::new(
            start + 500_000,
            ReplayEventKind::KeyUp(key),
        ));
    }

    // 3. Mouse-look : Gaussian random walk over full 30s.
    //    Use a derived seed so the walk doesn't reuse the same RNG state we
    //    just consumed for the WASD-key picks.
    let walk_seed = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    let walk = random_walk(walk_seed, (640.0, 360.0), MOUSE_STEP, 30_000, 60);
    events.extend(walk);

    // Re-sort by ts_micros so the merged stream is monotonic.
    events.sort_by_key(|ev| ev.ts_micros);
    events
}

/// Synthesize an intent-phrase typing event sequence : `/` → phrase → Enter.
///
/// Emits keystrokes for the slash, the phrase characters, and the Enter key,
/// each as a (KeyDown, KeyUp) pair with `TYPE_CHAR_DELAY_MICROS` spacing.
/// Additionally emits an `IntentText(phrase)` event at the end so a test
/// harness can verify the parsed intent without re-running the input
/// pipeline.  Empty `phrase` still emits the slash + Enter framing.
#[must_use]
pub fn type_intent_phrase(seed: u64, phrase: &str) -> Vec<ReplayEvent> {
    let _ = seed; // reserved for future jitter-mode

    let mut events = Vec::with_capacity(phrase.len() * 2 + 8);

    // 1. RngSeed marker (so replay engines re-seed deterministically).
    events.push(ReplayEvent::new(0, ReplayEventKind::RngSeed(seed)));

    // 2. Slash key (focus intent box) at t=100_000.
    let mut t = 100_000_u64;
    events.push(ReplayEvent::new(t, ReplayEventKind::KeyDown(KEY_SLASH)));
    events.push(ReplayEvent::new(t + 40_000, ReplayEventKind::KeyUp(KEY_SLASH)));
    t += TYPE_CHAR_DELAY_MICROS;

    // 3. Phrase characters.
    let phrase_keys = ascii_text_to_keystrokes(phrase, t, TYPE_CHAR_DELAY_MICROS);
    let phrase_end_ts = phrase_keys.last().map_or(t, |ev| ev.ts_micros);
    events.extend(phrase_keys);
    t = phrase_end_ts + TYPE_CHAR_DELAY_MICROS;

    // 4. Enter key (commit intent).
    events.push(ReplayEvent::new(t, ReplayEventKind::KeyDown(KEY_ENTER)));
    events.push(ReplayEvent::new(t + 40_000, ReplayEventKind::KeyUp(KEY_ENTER)));

    // 5. IntentText event so harnesses can verify without reparsing.
    events.push(ReplayEvent::new(
        t + 80_000,
        ReplayEventKind::IntentText(phrase.to_string()),
    ));

    events
}

/// Synthesize a multi-intent QA session by chaining `n_intents` phrases
/// drawn from `PHRASE_POOL`.
///
/// Each intent block is offset by `INTENT_GAP_MICROS = 2_000_000` so the
/// stream "looks like" a paced human session.  Phrase selection is via
/// the seeded RNG (sampling with replacement).
#[must_use]
pub fn full_qa_session(seed: u64, n_intents: u32) -> Vec<ReplayEvent> {
    if n_intents == 0 {
        return Vec::new();
    }
    const INTENT_GAP_MICROS: u64 = 2_000_000;

    let mut rng = Pcg32::new(seed);
    let mut events = Vec::with_capacity(n_intents as usize * 64);

    for i in 0..n_intents {
        let idx = rng.range_u32(0, PHRASE_POOL.len() as u32) as usize;
        let phrase = PHRASE_POOL[idx];
        let block = type_intent_phrase(seed.wrapping_add(u64::from(i)), phrase);
        let offset = INTENT_GAP_MICROS * u64::from(i);
        for ev in block {
            events.push(ReplayEvent::new(ev.ts_micros + offset, ev.kind));
        }
    }
    events
}

// ══════════════════════════════════════════════════════════════════
// § Tests
// ══════════════════════════════════════════════════════════════════
#[cfg(test)]
mod tests {
    use super::*;

    /// § navigate_test_room emits at least one WASD keystroke.
    #[test]
    fn navigate_includes_wasd() {
        let evs = navigate_test_room(0xBEEF);
        let wasd_set = [KEY_W, KEY_A, KEY_S, KEY_D];
        let any_wasd = evs.iter().any(|ev| {
            matches!(ev.kind, ReplayEventKind::KeyDown(k) if wasd_set.contains(&k))
        });
        assert!(any_wasd, "navigate scenario must include at least one WASD key");
        // Also verify monotonic ts_micros (sort invariant).
        for w in evs.windows(2) {
            assert!(w[0].ts_micros <= w[1].ts_micros);
        }
    }

    /// § type_intent_phrase opens with `/` and closes with Enter + IntentText.
    #[test]
    fn type_includes_slash_and_enter() {
        let evs = type_intent_phrase(1, "look");
        // Find slash KeyDown and Enter KeyDown.
        let has_slash = evs.iter().any(
            |ev| matches!(ev.kind, ReplayEventKind::KeyDown(k) if k == KEY_SLASH),
        );
        let has_enter = evs.iter().any(
            |ev| matches!(ev.kind, ReplayEventKind::KeyDown(k) if k == KEY_ENTER),
        );
        let has_intent = evs.iter().any(
            |ev| matches!(&ev.kind, ReplayEventKind::IntentText(s) if s == "look"),
        );
        assert!(has_slash, "missing leading slash");
        assert!(has_enter, "missing trailing enter");
        assert!(has_intent, "missing IntentText");
    }

    /// § full_qa_session with n=3 contains ≥ 3 IntentText events.
    #[test]
    fn full_qa_multi_intent() {
        let evs = full_qa_session(42, 3);
        let n_intents = evs
            .iter()
            .filter(|ev| matches!(ev.kind, ReplayEventKind::IntentText(_)))
            .count();
        assert_eq!(n_intents, 3, "expected exactly 3 IntentText events, got {n_intents}");
        // Determinism check.
        let evs2 = full_qa_session(42, 3);
        assert_eq!(evs, evs2);
        // Zero-intent edge case.
        assert!(full_qa_session(0, 0).is_empty());
    }

    /// § PHRASE_POOL is the 16-phrase pool the directive specifies.
    #[test]
    fn phrase_pool_non_empty() {
        assert_eq!(PHRASE_POOL.len(), 16);
        for p in &PHRASE_POOL {
            assert!(!p.is_empty(), "phrase must not be empty");
            assert!(
                p.is_ascii(),
                "phrase '{p}' must be ASCII for ascii_text_to_keystrokes"
            );
        }
    }
}
