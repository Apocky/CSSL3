// § intent.rs — input → MovementIntent translation.
// ════════════════════════════════════════════════════════════════════
// § I> Decouples loa-host's `InputFrame` from the augmentation engine.
// § I> Pure-Copy struct ; bit-equal serializable for replay.
// ════════════════════════════════════════════════════════════════════

use serde::{Deserialize, Serialize};

/// Per-frame movement-intent ingested by `MovementAug::tick`.
///
/// `forward`/`right` are normalized analog axes in [-1, 1] (or 0/±1 for KB).
/// All edge-flags are `true` only on the frame the action begins ; the
/// state-machine handles continuation.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct MovementIntent {
    /// Forward (+) / backward (-) axis.
    pub forward: f32,
    /// Right (+) / left (-) axis.
    pub right: f32,
    /// Sprint button held (sustained ; not edge).
    pub sprint_held: bool,
    /// Crouch button held — slides if sprinting + grounded ; otherwise crouches.
    pub crouch_held: bool,
    /// Jump pressed (edge — true on the frame the press happens).
    pub jump_pressed: bool,
    /// Dedicated mantle-confirm for assist-mode players ; auto-mantle still
    /// triggers via spatial query when this is false.
    pub mantle_pressed: bool,
}

impl MovementIntent {
    /// Construct from raw axes + button-flags. Convenience for hosts that
    /// don't carry a winit `InputFrame`.
    pub fn new(
        forward: f32,
        right: f32,
        sprint_held: bool,
        crouch_held: bool,
        jump_pressed: bool,
    ) -> Self {
        Self {
            forward,
            right,
            sprint_held,
            crouch_held,
            jump_pressed,
            mantle_pressed: false,
        }
    }

    /// Returns true if there's any horizontal-input ; used by stamina-recover
    /// gating and by the slide entry-condition.
    pub fn has_horiz_input(&self) -> bool {
        self.forward.abs() > 0.05 || self.right.abs() > 0.05
    }

    /// Magnitude of the horizontal-input vector (for stamina-cost shaping).
    pub fn horiz_magnitude(&self) -> f32 {
        self.forward.hypot(self.right).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_idle() {
        let i = MovementIntent::default();
        assert!(!i.has_horiz_input());
        assert!(!i.sprint_held);
        assert!(!i.jump_pressed);
    }

    #[test]
    fn horiz_magnitude_clamps_to_one() {
        let i = MovementIntent {
            forward: 1.0,
            right: 1.0,
            ..Default::default()
        };
        assert!((i.horiz_magnitude() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn small_input_below_threshold() {
        let i = MovementIntent {
            forward: 0.02,
            ..Default::default()
        };
        assert!(!i.has_horiz_input());
    }
}
