//! Listener — the receiver of spatialized audio.
//!
//! § DESIGN
//!   In a 3D-audio system the **listener** is the virtual ears the mixer
//!   spatializes voices toward. The listener carries position +
//!   orientation + velocity ; voices are spatialized relative to this
//!   reference frame.
//!
//!   For a single-player game there's typically one listener (matching
//!   the camera). For split-screen multiplayer the mixer instantiates
//!   one listener per player + the spatial bus computes per-listener
//!   pan independently.
//!
//! § ORIENTATION
//!   Listener orientation is encoded as a forward + up vector pair
//!   (matching OpenAL's listener convention). The mixer derives a
//!   right-handed basis at spatialize time : `right = forward × up`.
//!
//! § VELOCITY
//!   Listener velocity is used by the Doppler computation in the
//!   spatial module — a moving listener experiences shifted-frequency
//!   sources, just as a moving source does for a static listener.

use crate::voice::Vec3;

/// Listener orientation = forward + up basis vectors.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Orientation {
    /// Direction the listener is facing.
    pub forward: Vec3,
    /// "Up" relative to the listener.
    pub up: Vec3,
}

impl Orientation {
    /// Standard "looking down -Z, up = +Y" orientation (right-handed).
    pub const STANDARD: Self = Self {
        forward: Vec3::FORWARD,
        up: Vec3::UP,
    };

    /// Construct a new orientation. The vectors are normalized at
    /// construction so callers don't have to.
    #[must_use]
    pub fn new(forward: Vec3, up: Vec3) -> Self {
        Self {
            forward: forward.normalize(),
            up: up.normalize(),
        }
    }

    /// Right-vector derived from `forward × up`. Returns the normalized
    /// cross product ; `Vec3::ZERO` if forward + up are degenerate
    /// (caller should treat as no-spatialization).
    #[must_use]
    pub fn right(&self) -> Vec3 {
        self.forward.cross(self.up).normalize()
    }
}

impl Default for Orientation {
    fn default() -> Self {
        Self::STANDARD
    }
}

/// The listener — virtual ears the mixer spatializes audio toward.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Listener {
    /// World-space position.
    pub position: Vec3,
    /// Forward + up orientation.
    pub orientation: Orientation,
    /// Velocity for Doppler computation. `Vec3::ZERO` = static.
    pub velocity: Vec3,
    /// Master gain applied at spatialization time. 1.0 = unity.
    pub gain: f32,
}

impl Listener {
    /// Construct a listener at the origin with standard orientation.
    #[must_use]
    pub const fn at_origin() -> Self {
        Self {
            position: Vec3::ZERO,
            orientation: Orientation::STANDARD,
            velocity: Vec3::ZERO,
            gain: 1.0,
        }
    }

    /// Construct a listener at a given position with standard orientation.
    #[must_use]
    pub const fn at(position: Vec3) -> Self {
        Self {
            position,
            orientation: Orientation::STANDARD,
            velocity: Vec3::ZERO,
            gain: 1.0,
        }
    }

    /// Update position.
    pub const fn set_position(&mut self, position: Vec3) {
        self.position = position;
    }

    /// Update orientation.
    pub fn set_orientation(&mut self, forward: Vec3, up: Vec3) {
        self.orientation = Orientation::new(forward, up);
    }

    /// Update velocity.
    pub const fn set_velocity(&mut self, velocity: Vec3) {
        self.velocity = velocity;
    }

    /// Update gain ; clamps to non-negative.
    pub fn set_gain(&mut self, gain: f32) {
        self.gain = gain.max(0.0);
    }
}

impl Default for Listener {
    fn default() -> Self {
        Self::at_origin()
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;

    #[test]
    fn orientation_standard_constants() {
        assert_eq!(Orientation::STANDARD.forward, Vec3::FORWARD);
        assert_eq!(Orientation::STANDARD.up, Vec3::UP);
    }

    #[test]
    fn orientation_new_normalizes_forward() {
        let o = Orientation::new(Vec3::new(0.0, 0.0, -2.0), Vec3::UP);
        assert!((o.forward.length() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn orientation_new_normalizes_up() {
        let o = Orientation::new(Vec3::FORWARD, Vec3::new(0.0, 5.0, 0.0));
        assert!((o.up.length() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn orientation_right_is_cross_forward_up() {
        // Standard orientation : forward = -Z, up = +Y → right = +X.
        let o = Orientation::STANDARD;
        let right = o.right();
        // forward × up = (-Z) × Y = -X (right-handed cross).
        // Cross : (0,0,-1) × (0,1,0) = (0*0 - -1*1, -1*0 - 0*0, 0*1 - 0*0)
        //                          = (1, 0, 0) ⇒ +X.
        assert!((right.x - 1.0).abs() < 1e-6);
        assert!(right.y.abs() < 1e-6);
        assert!(right.z.abs() < 1e-6);
    }

    #[test]
    fn listener_at_origin() {
        let l = Listener::at_origin();
        assert_eq!(l.position, Vec3::ZERO);
        assert_eq!(l.gain, 1.0);
        assert_eq!(l.velocity, Vec3::ZERO);
    }

    #[test]
    fn listener_at_keeps_position() {
        let pos = Vec3::new(1.0, 2.0, 3.0);
        let l = Listener::at(pos);
        assert_eq!(l.position, pos);
    }

    #[test]
    fn listener_set_position_updates() {
        let mut l = Listener::default();
        l.set_position(Vec3::new(5.0, 0.0, 0.0));
        assert_eq!(l.position.x, 5.0);
    }

    #[test]
    fn listener_set_orientation_normalizes() {
        let mut l = Listener::default();
        l.set_orientation(Vec3::new(0.0, 0.0, -3.0), Vec3::new(0.0, 4.0, 0.0));
        assert!((l.orientation.forward.length() - 1.0).abs() < 1e-6);
        assert!((l.orientation.up.length() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn listener_set_gain_clamps_negative() {
        let mut l = Listener::default();
        l.set_gain(-1.0);
        assert_eq!(l.gain, 0.0);
    }

    #[test]
    fn listener_set_gain_accepts_above_one() {
        // We don't clamp above 1 ; mixer headroom + limiter handle it.
        let mut l = Listener::default();
        l.set_gain(2.0);
        assert_eq!(l.gain, 2.0);
    }

    #[test]
    fn listener_set_velocity_updates() {
        let mut l = Listener::default();
        l.set_velocity(Vec3::new(1.0, 0.0, 0.0));
        assert_eq!(l.velocity.x, 1.0);
    }

    #[test]
    fn listener_default_is_at_origin() {
        let l = Listener::default();
        assert_eq!(l, Listener::at_origin());
    }
}
