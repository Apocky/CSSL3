// § T11-W5b-INPUT-VIRTUAL : keystroke event generators
// ══════════════════════════════════════════════════════════════════
//! Keystroke / typing-session generators for the synthetic-input crate.
//!
//! Two public generators :
//!
//! - [`random_typing_session`] : burst-mode random keystrokes at a given
//!   words-per-minute pace, bounded by `duration_ms`.  Used for stress
//!   tests and entropy probes.
//! - [`ascii_text_to_keystrokes`] : convert a literal ASCII string into
//!   a deterministic KeyDown→KeyUp pair sequence with stable inter-char
//!   timing.  Used by `scenarios::type_intent_phrase`.
//!
//! ## Encoding
//!
//! `ReplayEventKind::KeyDown(u32)` and `KeyUp(u32)` carry an opaque
//! platform-neutral keycode — the actual mapping is `cssl-host-input`'s
//! responsibility.  For determinism + readability we use ASCII codepoints
//! directly when generating from text ; for the random typing session we
//! sample uniformly from the printable-ASCII range `[0x20, 0x7E]`.
//!
//! ## Determinism
//!
//! Both generators are pure functions of their parameters ; same args →
//! identical Vec.  The unit tests pin this contract.

use cssl_host_replay::{ReplayEvent, ReplayEventKind};

use crate::rng::Pcg32;

/// § Default key-hold duration (microseconds) — how long KeyDown precedes KeyUp.
const KEY_HOLD_MICROS: u64 = 40_000;

/// § Average characters per word for WPM → CPS conversion (Gross 5-char standard).
const CHARS_PER_WORD: u32 = 5;

/// § Lowest printable ASCII codepoint (space).
const ASCII_PRINTABLE_LO: u32 = 0x20;

/// § Highest printable ASCII codepoint (tilde).
const ASCII_PRINTABLE_HI: u32 = 0x7E;

/// Generate a random typing session at the given WPM pace, capped at `duration_ms`.
///
/// Each "keystroke" is a KeyDown immediately followed by a KeyUp `KEY_HOLD_MICROS`
/// later ; codepoints are sampled uniformly from the printable-ASCII range.
/// Inter-keystroke spacing is `60_000_000 / (wpm * 5)` microseconds (chars-per-sec
/// inverse).  Empty stream when `duration_ms == 0` or `wpm == 0`.
#[must_use]
pub fn random_typing_session(seed: u64, duration_ms: u32, wpm: u32) -> Vec<ReplayEvent> {
    if duration_ms == 0 || wpm == 0 {
        return Vec::new();
    }
    let total_micros = u64::from(duration_ms) * 1_000;
    let chars_per_sec = wpm * CHARS_PER_WORD;
    let inter_char_micros = 60_000_000_u64 / u64::from(chars_per_sec);
    let n_chars = (total_micros / inter_char_micros) as u32;

    let mut rng = Pcg32::new(seed);
    let mut events = Vec::with_capacity(n_chars as usize * 2);
    for i in 0..n_chars {
        let ts_down = u64::from(i) * inter_char_micros;
        let ts_up = ts_down + KEY_HOLD_MICROS;
        if ts_up >= total_micros {
            break;
        }
        let key = rng.range_u32(ASCII_PRINTABLE_LO, ASCII_PRINTABLE_HI + 1);
        events.push(ReplayEvent::new(ts_down, ReplayEventKind::KeyDown(key)));
        events.push(ReplayEvent::new(ts_up, ReplayEventKind::KeyUp(key)));
    }
    events
}

/// Convert a literal ASCII text string into a sequence of KeyDown/KeyUp pairs.
///
/// Each character `c` produces a `(KeyDown(c as u32), KeyUp(c as u32))` pair
/// at `start_ts_micros + i * char_delay_micros` (KeyDown) and
/// `start_ts_micros + i * char_delay_micros + KEY_HOLD_MICROS` (KeyUp).
/// Non-ASCII characters (codepoints > 0x7F) are skipped without panic.
/// Empty `text` → empty Vec.
#[must_use]
pub fn ascii_text_to_keystrokes(
    text: &str,
    start_ts_micros: u64,
    char_delay_micros: u64,
) -> Vec<ReplayEvent> {
    let mut events = Vec::with_capacity(text.len() * 2);
    let mut idx: u64 = 0;
    for ch in text.chars() {
        let cp = ch as u32;
        if cp > ASCII_PRINTABLE_HI && cp != 0x09 && cp != 0x0A && cp != 0x0D {
            // Non-printable / non-ASCII : skip rather than emit garbage keycodes.
            continue;
        }
        let ts_down = start_ts_micros + idx * char_delay_micros;
        let ts_up = ts_down + KEY_HOLD_MICROS;
        events.push(ReplayEvent::new(ts_down, ReplayEventKind::KeyDown(cp)));
        events.push(ReplayEvent::new(ts_up, ReplayEventKind::KeyUp(cp)));
        idx += 1;
    }
    events
}

// ══════════════════════════════════════════════════════════════════
// § Tests
// ══════════════════════════════════════════════════════════════════
#[cfg(test)]
mod tests {
    use super::*;

    /// § Determinism : same seed + duration + wpm → identical event vector.
    #[test]
    fn typing_session_deterministic() {
        let a = random_typing_session(0xC0FFEE, 1_000, 60);
        let b = random_typing_session(0xC0FFEE, 1_000, 60);
        assert_eq!(a, b, "same seed must produce bit-identical typing session");
        assert!(!a.is_empty(), "non-zero duration must produce events");
    }

    /// § Text → keystrokes : 2 events per ASCII character (down + up).
    #[test]
    fn text_to_keystrokes_length_matches() {
        let evs = ascii_text_to_keystrokes("hi!", 1000, 100_000);
        assert_eq!(evs.len(), 6, "3 chars × 2 events = 6");
        // First event is KeyDown('h') at ts=1000.
        assert_eq!(evs[0].ts_micros, 1000);
        assert!(matches!(evs[0].kind, ReplayEventKind::KeyDown(0x68)));
        // Last event is KeyUp('!') = 0x21.
        assert!(matches!(evs[5].kind, ReplayEventKind::KeyUp(0x21)));
    }

    /// § 60 WPM × 5 chars/word = 5 cps ; 1000 ms → ~5 chars → ~10 events.
    #[test]
    fn sixty_wpm_produces_correct_event_count() {
        let evs = random_typing_session(1, 1_000, 60);
        // 5 chars × 2 events = 10 ; one char may drop on the boundary check.
        assert!(
            (8..=10).contains(&evs.len()),
            "60 WPM @ 1s should yield ~10 events, got {}",
            evs.len()
        );
    }

    /// § Empty text → empty Vec ; zero duration → empty Vec.
    #[test]
    fn empty_inputs_yield_empty() {
        assert!(ascii_text_to_keystrokes("", 0, 1000).is_empty());
        assert!(random_typing_session(0, 0, 60).is_empty());
        assert!(random_typing_session(0, 1000, 0).is_empty());
    }
}
