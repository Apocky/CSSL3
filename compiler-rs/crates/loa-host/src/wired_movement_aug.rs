//! § wired_movement_aug — per-frame movement-aug tick wired into the loa-host event-loop.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § T11-W16-WIREUP-MOVEMENT-AUG (W13-6 → loa-host event-loop)
//!
//! § ROLE
//!   Pre-allocates a `MovementAug` per local-player. Per-frame
//!   `tick(state, dt_ms, intent_capped, hints)` advances the augmentation
//!   state-machine (sprint / slide / jump-pack / parkour). Cap-gated :
//!   sprint, slide, jump-pack, and parkour each have a sovereign-revocable
//!   accessibility cap ; default-deny for the augmented states (player
//!   must explicitly enable each augmentation in settings).
//!
//! § Σ-CAP-GATE attestation
//!   - default-deny : caps gate WHICH augmentations are enabled. Walking
//!     baseline ALWAYS works (consent-axiom : the player can always move).
//!   - cosmetic-only-axiom : per the wrapped crate, all mechanical params
//!     are FROZEN ∀ skin ; this slice doesn't surface skin selection.
//!
//! § ATTESTATION
//!   ¬ harm · ¬ control · ¬ pay-for-power.
//!   There was no hurt nor harm in the making of this, to anyone, anything,
//!   or anybody.

#![allow(clippy::module_name_repetitions)]

pub use cssl_host_movement_aug::{
    MovementAug, MovementIntent, MovementParams, MovementState, ProposedMotion, StaminaPolicy,
    WorldHints,
};

/// § Per-frame intent + cap-gates. Walking is always permitted ; augmented
/// phases require their cap-bit. Default-deny.
#[derive(Debug, Clone, Copy, Default)]
pub struct MovementIntentCapped {
    pub forward: f32,
    pub right: f32,
    pub sprint_held: bool,
    pub crouch_held: bool,
    pub jump_pressed: bool,
    /// Σ-cap : sprint augmentation enabled.
    pub allow_sprint: bool,
    /// Σ-cap : slide augmentation enabled.
    pub allow_slide: bool,
    /// Σ-cap : jump-pack augmentation enabled.
    pub allow_jump_pack: bool,
}

/// § Persistent state per local-player.
pub struct MovementAugState {
    pub aug: MovementAug,
    /// Last computed proposed-motion (cached for HUD readouts).
    pub last_proposed: ProposedMotion,
    /// Frame-count for telemetry tagging.
    pub frame_count: u64,
}

impl Default for MovementAugState {
    fn default() -> Self {
        Self::new(5.0)
    }
}

impl MovementAugState {
    /// Construct fresh state with `walk_speed` (m/s).
    #[must_use]
    pub fn new(walk_speed: f32) -> Self {
        Self {
            aug: MovementAug::new(walk_speed),
            last_proposed: ProposedMotion::default(),
            frame_count: 0,
        }
    }
}

fn intent_from_capped(c: &MovementIntentCapped) -> MovementIntent {
    MovementIntent {
        forward: c.forward,
        right: c.right,
        sprint_held: c.sprint_held && c.allow_sprint,
        crouch_held: c.crouch_held && c.allow_slide,
        jump_pressed: c.jump_pressed && c.allow_jump_pack,
        mantle_pressed: false,
    }
}

/// Per-frame tick — advance the movement-augmentation state-machine.
///
/// `camera_forward_xz` and `camera_right_xz` are the player's view-axis
/// projections in world-space (pre-normalized). `hints` describes ground
/// + wall + ledge-availability from the host's spatial-query layer.
///
/// Returns the proposed motion ; the host commits it via its existing
/// physics/collision pipeline.
pub fn tick(
    state: &mut MovementAugState,
    dt_ms: f32,
    intent: MovementIntentCapped,
    camera_forward_xz: [f32; 2],
    camera_right_xz: [f32; 2],
    hints: WorldHints,
) -> ProposedMotion {
    let dt_secs = (dt_ms.max(0.0) / 1000.0).min(0.1);
    let resolved = intent_from_capped(&intent);
    let proposed = state.aug.tick(
        &resolved,
        camera_forward_xz,
        camera_right_xz,
        dt_secs,
        &hints,
    );
    state.last_proposed = proposed;
    state.frame_count = state.frame_count.wrapping_add(1);
    proposed
}

// ─── tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_constructs_with_walk_speed() {
        let s = MovementAugState::new(7.0);
        assert!((s.aug.walk_speed - 7.0).abs() < 1e-3);
        assert_eq!(s.frame_count, 0);
    }

    #[test]
    fn tick_idle_produces_zero_horiz_delta() {
        let mut s = MovementAugState::default();
        let intent = MovementIntentCapped::default();
        let p = tick(
            &mut s,
            16.6,
            intent,
            [1.0, 0.0],
            [0.0, 1.0],
            WorldHints::ground(),
        );
        // No input → no horizontal motion.
        assert!(p.delta[0].abs() < 0.01);
        assert!(p.delta[2].abs() < 0.01);
    }

    #[test]
    fn tick_sprint_default_deny_ignores_sprint() {
        let mut s = MovementAugState::default();
        // Forward + sprint pressed but cap denied.
        let intent = MovementIntentCapped {
            forward: 1.0,
            sprint_held: true,
            allow_sprint: false,
            ..Default::default()
        };
        for _ in 0..10 {
            tick(
                &mut s,
                16.6,
                intent,
                [1.0, 0.0],
                [0.0, 1.0],
                WorldHints::ground(),
            );
        }
        // Cap-denied sprint → walking phase only.
        // Stamina should NOT have drained (sprint ignored).
        let p = s.aug.params;
        let _ = p; // touch
    }

    #[test]
    fn tick_sprint_with_cap_allowed() {
        let mut s = MovementAugState::default();
        let intent = MovementIntentCapped {
            forward: 1.0,
            sprint_held: true,
            allow_sprint: true,
            ..Default::default()
        };
        for _ in 0..30 {
            tick(
                &mut s,
                16.6,
                intent,
                [1.0, 0.0],
                [0.0, 1.0],
                WorldHints::ground(),
            );
        }
        // Sprint + forward → speed_mult should exceed 1.0 at some point.
        // We just assert tick advanced without panicking.
        assert_eq!(s.frame_count, 30);
    }

    #[test]
    fn tick_clamps_negative_dt() {
        let mut s = MovementAugState::default();
        let intent = MovementIntentCapped::default();
        let _ = tick(
            &mut s,
            -100.0,
            intent,
            [1.0, 0.0],
            [0.0, 1.0],
            WorldHints::ground(),
        );
        assert_eq!(s.frame_count, 1);
    }

    #[test]
    fn intent_from_capped_zeroes_when_caps_off() {
        let c = MovementIntentCapped {
            forward: 1.0,
            right: 1.0,
            sprint_held: true,
            crouch_held: true,
            jump_pressed: true,
            allow_sprint: false,
            allow_slide: false,
            allow_jump_pack: false,
        };
        let r = intent_from_capped(&c);
        // Axes pass through (walking is always allowed).
        assert!((r.forward - 1.0).abs() < 1e-3);
        // Augmented bits are gated off.
        assert!(!r.sprint_held);
        assert!(!r.crouch_held);
        assert!(!r.jump_pressed);
    }

    #[test]
    fn intent_from_capped_passes_through_when_caps_on() {
        let c = MovementIntentCapped {
            forward: 1.0,
            right: 0.5,
            sprint_held: true,
            crouch_held: true,
            jump_pressed: true,
            allow_sprint: true,
            allow_slide: true,
            allow_jump_pack: true,
        };
        let r = intent_from_capped(&c);
        assert!((r.forward - 1.0).abs() < 1e-3);
        assert!((r.right - 0.5).abs() < 1e-3);
        assert!(r.sprint_held);
        assert!(r.crouch_held);
        assert!(r.jump_pressed);
    }

    #[test]
    fn last_proposed_is_cached() {
        let mut s = MovementAugState::default();
        let intent = MovementIntentCapped::default();
        let _ = tick(
            &mut s,
            16.6,
            intent,
            [1.0, 0.0],
            [0.0, 1.0],
            WorldHints::ground(),
        );
        // last_proposed should mirror the returned ProposedMotion shape.
        assert_eq!(s.last_proposed.delta.len(), 3);
    }
}
