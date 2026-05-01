//! § pacing.rs — pacing-policy trait + stage-0 + stage-1-stub impls.
//!
//! § ROLE
//!   Given a tension trajectory, a scene-cursor, and a player-fatigue
//!   estimate, the pacing-policy emits a [`PacingHint`] that tells the
//!   GM how aggressively to rise / hold / fall. The GM converts the
//!   hint into a [`crate::types::PacingMarkEvent`] when emit-time
//!   arrives.
//!
//! § STAGE-0 ALGORITHM
//!   - low player_fatigue + high tension_target → `PacingKind::Rise`
//!     · narrows beat_spacing.
//!   - high player_fatigue → `PacingKind::Fall`
//!     · widens beat_spacing + raises idle_allow.
//!   - default → `PacingKind::Hold` · neutral spacing.
//!
//! § STAGE-1 STUB
//!   [`Stage1KanStubPacingPolicy`] reserves the trait-object swap-point
//!   for a KAN-driven pacing model that reads the ω-field. Today it
//!   delegates to [`Stage0PacingPolicy`].

use serde::{Deserialize, Serialize};

use crate::types::PacingKind;

/// Hint returned by a pacing-policy.
///
/// The GM consumes this to pick a [`crate::types::PacingMarkEvent::kind`]
/// + magnitude, and downstream renderers consume `beat_spacing_ms` to
/// time camera + music + dialog beats.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PacingHint {
    pub beat_spacing_ms: u32,
    pub tension_target: f32,
    pub idle_allow: bool,
    pub kind: PacingKind,
}

impl PacingHint {
    /// Construct the canonical "hold" hint (neutral spacing, idle-on).
    #[must_use]
    pub fn hold(tension_target: f32) -> Self {
        Self {
            beat_spacing_ms: 1500,
            tension_target,
            idle_allow: true,
            kind: PacingKind::Hold,
        }
    }
}

/// Pacing-policy trait — object-safe so the GM holds `Box<dyn …>` and
/// stage-0 / stage-1 implementations swap at runtime.
pub trait PacingPolicy: Send + Sync {
    /// Stable backend identifier (e.g. `"stage0"`, `"stage1-kan-stub"`).
    fn name(&self) -> &'static str;

    /// Compute a pacing-hint from the tension trajectory + cursor +
    /// fatigue.
    ///
    /// `tension_vec` : the recent tension samples (most-recent last) in
    /// `[0.0..1.0]`. Implementations MUST tolerate empty vectors.
    /// `scene_cursor` : an opaque monotonic counter for replay-stability.
    /// `player_fatigue` : `[0.0..1.0]` ; high = player tired.
    fn compute_pacing(
        &self,
        tension_vec: &[f32],
        scene_cursor: u32,
        player_fatigue: f32,
    ) -> PacingHint;
}

/// Stage-0 keyword-rule-style pacing-policy.
#[derive(Debug, Default, Clone, Copy)]
pub struct Stage0PacingPolicy;

impl PacingPolicy for Stage0PacingPolicy {
    fn name(&self) -> &'static str {
        "stage0"
    }

    fn compute_pacing(
        &self,
        tension_vec: &[f32],
        _scene_cursor: u32,
        player_fatigue: f32,
    ) -> PacingHint {
        let last = tension_vec.last().copied().unwrap_or(0.5);
        let fatigue = player_fatigue.clamp(0.0, 1.0);
        if fatigue > 0.7 {
            PacingHint {
                beat_spacing_ms: 2400,
                tension_target: (last - 0.15).clamp(0.0, 1.0),
                idle_allow: true,
                kind: PacingKind::Fall,
            }
        } else if fatigue < 0.3 && last > 0.6 {
            PacingHint {
                beat_spacing_ms: 900,
                tension_target: (last + 0.10).clamp(0.0, 1.0),
                idle_allow: false,
                kind: PacingKind::Rise,
            }
        } else {
            PacingHint::hold(last)
        }
    }
}

/// Stage-1 KAN-stub pacing-policy.
///
/// Carries an opaque `kan_handle` (identifier-only ; the real KAN
/// handle lands alongside `cssl-substrate-kan` integration in stage-2).
/// When `kan_handle` is `Some`, returns a canned mocked hint ; when
/// `None`, delegates to `fallback`.
pub struct Stage1KanStubPacingPolicy {
    pub fallback: Box<dyn PacingPolicy>,
    pub kan_handle: Option<String>,
}

impl Stage1KanStubPacingPolicy {
    /// Construct with a fallback + no KAN handle (always delegates).
    #[must_use]
    pub fn new(fallback: Box<dyn PacingPolicy>) -> Self {
        Self {
            fallback,
            kan_handle: None,
        }
    }

    /// Construct with a fallback + a mock KAN handle.
    #[must_use]
    pub fn with_handle(fallback: Box<dyn PacingPolicy>, handle: String) -> Self {
        Self {
            fallback,
            kan_handle: Some(handle),
        }
    }
}

impl PacingPolicy for Stage1KanStubPacingPolicy {
    fn name(&self) -> &'static str {
        "stage1-kan-stub"
    }

    fn compute_pacing(
        &self,
        tension_vec: &[f32],
        scene_cursor: u32,
        player_fatigue: f32,
    ) -> PacingHint {
        if self.kan_handle.is_some() {
            // Mocked KAN response : pick "Rise" with tightened spacing.
            // Stage-2 replaces this with a real KAN forward-pass.
            let last = tension_vec.last().copied().unwrap_or(0.5);
            PacingHint {
                beat_spacing_ms: 1100,
                tension_target: last,
                idle_allow: false,
                kind: PacingKind::Rise,
            }
        } else {
            self.fallback
                .compute_pacing(tension_vec, scene_cursor, player_fatigue)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage0_high_fatigue_falls() {
        let p = Stage0PacingPolicy;
        let h = p.compute_pacing(&[0.8], 0, 0.9);
        assert_eq!(h.kind, PacingKind::Fall);
        assert!(h.beat_spacing_ms >= 2000);
        assert!(h.idle_allow);
    }

    #[test]
    fn stage0_low_fatigue_high_tension_rises() {
        let p = Stage0PacingPolicy;
        let h = p.compute_pacing(&[0.8], 0, 0.1);
        assert_eq!(h.kind, PacingKind::Rise);
        assert!(h.beat_spacing_ms <= 1000);
        assert!(!h.idle_allow);
    }

    #[test]
    fn stage0_default_holds() {
        let p = Stage0PacingPolicy;
        let h = p.compute_pacing(&[0.5], 0, 0.5);
        assert_eq!(h.kind, PacingKind::Hold);
    }

    #[test]
    fn stage1_no_handle_delegates() {
        let s1 = Stage1KanStubPacingPolicy::new(Box::new(Stage0PacingPolicy));
        let h = s1.compute_pacing(&[0.5], 0, 0.5);
        assert_eq!(h.kind, PacingKind::Hold);
    }
}
