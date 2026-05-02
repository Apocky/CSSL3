// § state.rs — locomotion phase + stamina/jump-count state.
// ════════════════════════════════════════════════════════════════════
// § I> Pure-data ; mutated only by `aug.rs`.
// § I> All fields Copy + serializable for replay-bit-equal snapshots.
// ════════════════════════════════════════════════════════════════════

use serde::{Deserialize, Serialize};

/// Locomotion phase — drives the `MovementAug::tick` state-machine.
///
/// Transitions :
///   Walking ↔ Sprinting ↔ Sliding ↔ Airborne ↔ WallRunning ↔ Mantling
///
/// See `aug.rs` for the canonical transition-table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LocomotionPhase {
    /// Idle / walking / running on the ground (default).
    Walking,
    /// Holding sprint button, on-ground, with stamina available.
    Sprinting,
    /// Triggered by crouch-while-sprinting ; momentum-decay over time.
    Sliding,
    /// In the air — single-jumped or double-jumped.
    Airborne,
    /// Running along a side-wall (left or right) — capped at `wall_run_max_secs`.
    WallRunning,
    /// Auto-vaulting over a low ledge. Animation duration is fixed-step.
    Mantling,
}

impl Default for LocomotionPhase {
    fn default() -> Self {
        Self::Walking
    }
}

/// Wall-run side discriminator. None if not wall-running.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WallSide {
    Left,
    Right,
}

/// Mutable per-player movement-augmentation state. Owned by `MovementAug`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MovementState {
    pub phase: LocomotionPhase,
    /// Stamina ∈ [0, 1] — depletes during sprint, regens otherwise.
    pub stamina: f32,
    /// Time-in-current-phase in seconds (resets on phase-change).
    pub phase_time: f32,
    /// Air-jumps consumed (resets on grounding). 0 = ground · 1 = first-jump
    /// in air · 2 = double-jump consumed.
    pub air_jumps_used: u8,
    /// Vertical velocity (m/s ; positive = up).
    pub vy: f32,
    /// Horizontal momentum (m/s, world-space). Used by slide friction-decel
    /// and slide-jump combo boost.
    pub momentum_xz: [f32; 2],
    /// Wall-run side (None if not WallRunning).
    pub wall_side: Option<WallSide>,
}

impl MovementState {
    /// Fresh state — full stamina, on-ground walking, zero momentum.
    pub const fn new() -> Self {
        Self {
            phase: LocomotionPhase::Walking,
            stamina: 1.0,
            phase_time: 0.0,
            air_jumps_used: 0,
            vy: 0.0,
            momentum_xz: [0.0, 0.0],
            wall_side: None,
        }
    }

    /// Advance phase-timer by dt. Used internally by `MovementAug::tick`.
    pub fn advance_phase_time(&mut self, dt: f32) {
        self.phase_time += dt;
    }

    /// Switch to a new phase, resetting `phase_time`.
    pub fn enter_phase(&mut self, p: LocomotionPhase) {
        if self.phase != p {
            self.phase = p;
            self.phase_time = 0.0;
        }
    }

    /// Resets jump-count + clears wall-side. Called on grounding events.
    pub fn on_ground(&mut self) {
        self.air_jumps_used = 0;
        self.wall_side = None;
    }

    /// Bit-equal snapshot for replay manifests.
    pub fn snapshot_bytes(&self) -> [u8; 24] {
        let mut out = [0u8; 24];
        out[0] = match self.phase {
            LocomotionPhase::Walking => 0,
            LocomotionPhase::Sprinting => 1,
            LocomotionPhase::Sliding => 2,
            LocomotionPhase::Airborne => 3,
            LocomotionPhase::WallRunning => 4,
            LocomotionPhase::Mantling => 5,
        };
        out[1] = self.air_jumps_used;
        out[2] = match self.wall_side {
            None => 0,
            Some(WallSide::Left) => 1,
            Some(WallSide::Right) => 2,
        };
        // 3 — pad
        out[4..8].copy_from_slice(&self.stamina.to_le_bytes());
        out[8..12].copy_from_slice(&self.phase_time.to_le_bytes());
        out[12..16].copy_from_slice(&self.vy.to_le_bytes());
        out[16..20].copy_from_slice(&self.momentum_xz[0].to_le_bytes());
        out[20..24].copy_from_slice(&self.momentum_xz[1].to_le_bytes());
        out
    }
}

impl Default for MovementState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_state_walking_full_stamina() {
        let s = MovementState::new();
        assert_eq!(s.phase, LocomotionPhase::Walking);
        assert!((s.stamina - 1.0).abs() < 1e-6);
        assert_eq!(s.air_jumps_used, 0);
    }

    #[test]
    fn enter_phase_resets_timer() {
        let mut s = MovementState::new();
        s.advance_phase_time(0.5);
        s.enter_phase(LocomotionPhase::Sprinting);
        assert!((s.phase_time - 0.0).abs() < 1e-6);
        assert_eq!(s.phase, LocomotionPhase::Sprinting);
    }

    #[test]
    fn snapshot_bit_equal_for_equal_state() {
        let s1 = MovementState::new();
        let mut s2 = MovementState::new();
        s2.advance_phase_time(0.0); // no-op
        assert_eq!(s1.snapshot_bytes(), s2.snapshot_bytes());
    }

    #[test]
    fn snapshot_diverges_after_phase_change() {
        let s1 = MovementState::new();
        let mut s2 = MovementState::new();
        s2.enter_phase(LocomotionPhase::Sliding);
        assert_ne!(s1.snapshot_bytes(), s2.snapshot_bytes());
    }
}
