//! § types.rs — public output types for the GM narrator.
//!
//! All types are `Serialize + Deserialize + Debug + Clone + PartialEq`
//! so they round-trip stably through the audit-stream + replay buffer.

use serde::{Deserialize, Serialize};

/// One prose frame emitted by the GM.
///
/// `utf8_text` is user-facing English. Per Apocky's CSL3-native-default
/// directive, English-prose is allowed in user-facing chat — the GM is
/// exactly that surface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NarrativeTextFrame {
    pub utf8_text: String,
    pub tone: ToneAxis,
    pub ts_micros: u64,
}

/// Rise / Hold / Fall pacing-mark axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PacingKind {
    Rise,
    Hold,
    Fall,
}

/// One pacing-mark event consumed by downstream renderers (camera /
/// music / beat-spacing).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PacingMarkEvent {
    pub kind: PacingKind,
    pub magnitude: f32,
    pub ts_micros: u64,
}

/// A small list of free-form prompt strings the player can pick from.
///
/// `max_select` caps how many the UI is allowed to display ; the GM
/// always populates `items` honoring `items.len() <= max_select`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PromptSuggestion {
    pub items: Vec<String>,
    pub max_select: u8,
}

/// Tone axis : three independent f32 channels, each in `[0.0, 1.0]`.
///
/// - `warm` : friendliness / approachability.
/// - `terse` : sentence-shortness preference.
/// - `poetic` : metaphor / lyricism preference.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ToneAxis {
    pub warm: f32,
    pub terse: f32,
    pub poetic: f32,
}

impl ToneAxis {
    /// Neutral tone — all axes 0.5.
    #[must_use]
    pub fn neutral() -> Self {
        Self {
            warm: 0.5,
            terse: 0.5,
            poetic: 0.5,
        }
    }

    /// Construct + clamp every axis into `[0.0, 1.0]`.
    #[must_use]
    pub fn clamped(warm: f32, terse: f32, poetic: f32) -> Self {
        Self {
            warm: warm.clamp(0.0, 1.0),
            terse: terse.clamp(0.0, 1.0),
            poetic: poetic.clamp(0.0, 1.0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tone_axis_neutral_is_half() {
        let t = ToneAxis::neutral();
        assert_eq!(t.warm, 0.5);
        assert_eq!(t.terse, 0.5);
        assert_eq!(t.poetic, 0.5);
    }

    #[test]
    fn tone_axis_clamps_above_one() {
        let t = ToneAxis::clamped(1.5, -0.3, 0.7);
        assert_eq!(t.warm, 1.0);
        assert_eq!(t.terse, 0.0);
        assert_eq!(t.poetic, 0.7);
    }

    #[test]
    fn pacing_kind_serde_round_trip() {
        let kinds = [PacingKind::Rise, PacingKind::Hold, PacingKind::Fall];
        for k in kinds {
            let s = serde_json::to_string(&k).unwrap();
            let r: PacingKind = serde_json::from_str(&s).unwrap();
            assert_eq!(k, r);
        }
    }

    #[test]
    fn narrative_frame_serde_round_trip() {
        let f = NarrativeTextFrame {
            utf8_text: String::from("you see the altar"),
            tone: ToneAxis::neutral(),
            ts_micros: 42,
        };
        let s = serde_json::to_string(&f).unwrap();
        let r: NarrativeTextFrame = serde_json::from_str(&s).unwrap();
        assert_eq!(f, r);
    }
}
