// § input.rs : tick-tagged player-input frame
//
// `InputFrame` is the wire-payload from client-prediction to server. Inputs
// are PURE — given an `InputFrame` and a prior `SimState`, deterministic
// simulation produces the next `SimState`. This is the foundation that
// makes rollback / reconciliation possible.
//
// We model FPS-relevant inputs : 2D move-axis, 2D look-axis (delta-pitch /
// delta-yaw), action-bitmask (jump/crouch/fire/ADS/reload/use/melee/sprint).
// Producer-crates upstream (W13-11 input) populate this from raw HID events.

use serde::{Deserialize, Serialize};

use crate::tick::TickId;

/// Action bitmask bits ; OR together for combos.
pub mod action {
    pub const JUMP: u32 = 1 << 0;
    pub const CROUCH: u32 = 1 << 1;
    pub const FIRE: u32 = 1 << 2;
    pub const ADS: u32 = 1 << 3;
    pub const RELOAD: u32 = 1 << 4;
    pub const USE_INTERACT: u32 = 1 << 5;
    pub const MELEE: u32 = 1 << 6;
    pub const SPRINT: u32 = 1 << 7;
    pub const SLIDE: u32 = 1 << 8;
    pub const MANTLE: u32 = 1 << 9;
    pub const PRONE: u32 = 1 << 10;
    pub const LEAN_LEFT: u32 = 1 << 11;
    pub const LEAN_RIGHT: u32 = 1 << 12;
    pub const SWITCH_WEAPON: u32 = 1 << 13;
    pub const GRENADE: u32 = 1 << 14;
    pub const KNIFE: u32 = 1 << 15;
}

/// Tick-tagged player-input frame. Q16.16 fixed-point for axes ;
/// determinism-preserving across float-divergent platforms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InputFrame {
    /// Authoritative tick this input applies to.
    pub tick: TickId,
    /// Sequence number (monotonic per-client) for duplicate-detection.
    pub seq: u32,
    /// 2-D move axis ; Q16.16 ; range ≈ [-65536, +65536].
    pub move_x: i32,
    pub move_y: i32,
    /// 2-D look-delta ; Q16.16 ; per-tick angular delta.
    pub look_dpitch: i32,
    pub look_dyaw: i32,
    /// Bitmask of `action::*` bits pressed this tick.
    pub actions: u32,
}

impl InputFrame {
    /// Empty (no-op) input at a tick.
    #[must_use]
    pub fn empty(tick: TickId, seq: u32) -> Self {
        Self {
            tick,
            seq,
            move_x: 0,
            move_y: 0,
            look_dpitch: 0,
            look_dyaw: 0,
            actions: 0,
        }
    }

    #[must_use]
    pub fn has_action(&self, bit: u32) -> bool {
        (self.actions & bit) != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_bits_dont_overlap() {
        // sanity : every bit-constant is a single-bit power-of-two
        let bits = [
            action::JUMP,
            action::CROUCH,
            action::FIRE,
            action::ADS,
            action::RELOAD,
            action::USE_INTERACT,
            action::MELEE,
            action::SPRINT,
            action::SLIDE,
            action::MANTLE,
            action::PRONE,
            action::LEAN_LEFT,
            action::LEAN_RIGHT,
            action::SWITCH_WEAPON,
            action::GRENADE,
            action::KNIFE,
        ];
        for b in bits {
            assert!(b.is_power_of_two(), "{b:#b} must be a single bit");
        }
        // all distinct
        let mut set = std::collections::BTreeSet::new();
        for b in bits {
            assert!(set.insert(b), "duplicate bit {b:#b}");
        }
    }

    #[test]
    fn has_action_or_combos() {
        let f = InputFrame {
            tick: TickId(1),
            seq: 1,
            move_x: 0,
            move_y: 0,
            look_dpitch: 0,
            look_dyaw: 0,
            actions: action::FIRE | action::ADS,
        };
        assert!(f.has_action(action::FIRE));
        assert!(f.has_action(action::ADS));
        assert!(!f.has_action(action::JUMP));
    }
}
