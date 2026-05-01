// ══════════════════════════════════════════════════════════════════════════════
// § feedback.rs · player-feedback events + implicit-loss extraction
// ══════════════════════════════════════════════════════════════════════════════
// § Spec : paradigm-6 § "player feedback (thumbs-up · thumbs-down · scalar
//   score) → loss". The implicit-loss convention :
//     thumbs-up    → -1.0   (we WANT more of this · negative loss = reward)
//     thumbs-down  → +1.0   (we want LESS of this · positive loss = penalty)
//     scalar(s)    → -s     (higher score = lower loss)
//     comment(_)   → None   (no quantitative signal · skipped by optimizer)
//   The optimizer multiplies by the linear-loss surrogate to derive the gradient.

use serde::{Deserialize, Serialize};

/// Discriminant + payload of a player-feedback signal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FeedbackKind {
    /// 👍 — player wants more like this.
    ThumbsUp,
    /// 👎 — player wants less like this.
    ThumbsDown,
    /// Continuous score · domain conventionally `[-1.0, 1.0]` but unconstrained.
    ScalarScore(f32),
    /// Free-text comment · carries no quantitative signal.
    Comment(String),
}

/// A single player-feedback event timestamped + linked to the scene-features
/// the optimizer should associate with the signal.
///
/// The optimizer keeps an append-only history of these for replay /
/// checkpoint-and-restore.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FeedbackEvent {
    /// Microseconds since UNIX epoch (caller-supplied · monotonic ordering
    /// is the optimizer's responsibility, not ours).
    pub ts_micros: u64,
    /// Discriminant + payload.
    pub kind: FeedbackKind,
    /// Caller-readable label of what the player reacted to ("scene:forest_clearing",
    /// "creature:companion_v3", etc.). Used for human-readable telemetry only.
    pub target_label: String,
    /// Feature-vector φ(scene) the optimizer should associate with this signal.
    /// Length MUST equal the optimizer's bias-vector dimension at observe-time.
    pub scene_features: Vec<f32>,
}

impl FeedbackEvent {
    /// Convert the payload into a numeric loss signal, if quantitative.
    ///
    /// Returns `None` for `Comment(_)` (no scalar signal).
    /// NaN scalar-scores collapse to `None` to avoid propagating non-finite
    /// values into the gradient.
    #[must_use]
    pub fn implicit_loss(&self) -> Option<f32> {
        match &self.kind {
            FeedbackKind::ThumbsUp => Some(-1.0),
            FeedbackKind::ThumbsDown => Some(1.0),
            FeedbackKind::ScalarScore(s) => {
                if s.is_finite() {
                    Some(-*s)
                } else {
                    None
                }
            }
            FeedbackKind::Comment(_) => None,
        }
    }

    /// Convenience constructor for thumbs-up.
    #[must_use]
    pub fn thumbs_up(ts_micros: u64, target_label: impl Into<String>, features: Vec<f32>) -> Self {
        Self {
            ts_micros,
            kind: FeedbackKind::ThumbsUp,
            target_label: target_label.into(),
            scene_features: features,
        }
    }

    /// Convenience constructor for thumbs-down.
    #[must_use]
    pub fn thumbs_down(ts_micros: u64, target_label: impl Into<String>, features: Vec<f32>) -> Self {
        Self {
            ts_micros,
            kind: FeedbackKind::ThumbsDown,
            target_label: target_label.into(),
            scene_features: features,
        }
    }

    /// Convenience constructor for scalar-score feedback.
    #[must_use]
    pub fn scalar(ts_micros: u64, score: f32, target_label: impl Into<String>, features: Vec<f32>) -> Self {
        Self {
            ts_micros,
            kind: FeedbackKind::ScalarScore(score),
            target_label: target_label.into(),
            scene_features: features,
        }
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// § tests
// ══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    /// 👍 → loss = -1.0 (reward signal).
    #[test]
    fn thumbs_up_loss_negative() {
        let e = FeedbackEvent::thumbs_up(0, "scene:forest", vec![0.0; 4]);
        assert_eq!(e.implicit_loss(), Some(-1.0));
    }

    /// 👎 → loss = +1.0 (penalty signal).
    #[test]
    fn thumbs_down_loss_positive() {
        let e = FeedbackEvent::thumbs_down(0, "scene:forest", vec![0.0; 4]);
        assert_eq!(e.implicit_loss(), Some(1.0));
    }

    /// scalar(s) → loss = -s ; verify both signs.
    #[test]
    fn scalar_loss_negated() {
        let e = FeedbackEvent::scalar(0, 0.7, "x", vec![0.0; 4]);
        assert_eq!(e.implicit_loss(), Some(-0.7));

        let f = FeedbackEvent::scalar(0, -0.4, "x", vec![0.0; 4]);
        assert_eq!(f.implicit_loss(), Some(0.4));
    }

    /// scalar(NaN) collapses to None (NaN guard).
    #[test]
    fn scalar_nan_no_loss() {
        let e = FeedbackEvent::scalar(0, f32::NAN, "x", vec![0.0; 4]);
        assert_eq!(e.implicit_loss(), None);
    }

    /// comments carry no quantitative signal.
    #[test]
    fn comment_no_loss() {
        let e = FeedbackEvent {
            ts_micros: 0,
            kind: FeedbackKind::Comment("nice vibe".into()),
            target_label: "scene:forest".into(),
            scene_features: vec![0.0; 4],
        };
        assert_eq!(e.implicit_loss(), None);
    }

    /// serde roundtrip preserves the event verbatim.
    #[test]
    fn serde_roundtrip() {
        let e = FeedbackEvent {
            ts_micros: 1_700_000_000_000_000,
            kind: FeedbackKind::ScalarScore(0.42),
            target_label: "creature:companion_v3".into(),
            scene_features: vec![0.1, -0.2, 0.3, 0.0],
        };
        let s = serde_json::to_string(&e).unwrap();
        let r: FeedbackEvent = serde_json::from_str(&s).unwrap();
        assert_eq!(e, r);
    }
}
