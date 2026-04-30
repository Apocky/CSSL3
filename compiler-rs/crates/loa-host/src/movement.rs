// § T11-LOA-HOST-2 (W-LOA-host-input) · movement.rs ───────────────────────
// Camera state-update from per-frame `InputFrame`. WASD → translation along
// camera-space basis ; mouse-deltas → yaw + pitch (pitch clamped ±89°).
//
// § design-notes ───────────────────────────────────────────────────────
// The `Camera` struct is the SHIM canonical definition (see lib.rs comment).
// Render-sibling W-LOA-host-render is expected to use this same struct
// shape. If the render-sibling lands a different shape, the integration
// commit reconciles by promoting THIS struct (it is the input-and-physics
// authoritative form) — render code is read-only on Camera.
//
// § movement model ────────────────────────────────────────────────────
// Camera basis (world-space, y-up) :
//
//     forward = (sin(yaw)·cos(pitch),  sin(pitch),  cos(yaw)·cos(pitch))
//     right   = (cos(yaw),             0,          -sin(yaw))
//     up      = (0, 1, 0)  ← world-up · NOT camera-up · simplifies vertical-strafe
//
// We use WORLD-up for vertical strafe (Space/LCtrl) so looking-down doesn't
// shove us into the floor. Forward/Right derive from the camera's pitch+yaw
// (so looking up + W moves us up-and-forward — typical FPS feel).
//
// § speed ─────────────────────────────────────────────────────────────
//   walk   : 5.0 m/s  ← SPEED_M_PER_S const
//   sprint : 10.0 m/s ← SHIFT held
// The speed is APPLIED to a NORMALIZED direction (when forward+right both
// held, diagonal magnitude = 1.0 not √2 — no diagonal-cheese).
//
// § PRIME-DIRECTIVE ────────────────────────────────────────────────────
// Movement applies to the LOCAL camera state only ; physics.rs is the next
// layer that validates against world-collision before committing the delta.
// We do NOT call physics here — `propose_motion()` returns a candidate-delta
// for the host to validate and the host then calls `commit_motion()` with
// the (possibly-clamped) delta.

use cssl_rt::loa_startup::log_event;

use crate::input::InputFrame;

/// Walk speed in m/s. Per scenes/player_physics.cssl design.
pub const SPEED_M_PER_S: f32 = 5.0;
/// Sprint multiplier. Hold LShift.
pub const SPRINT_MULT: f32 = 2.0;
/// Mouse-look sensitivity (radians per pixel). Tune-target ; users can
/// override via env var in a future slice. 0.0025 ≈ 360° per ~50cm at
/// typical 400-CPI mouse — feels right for FPS on 1920×1080.
pub const MOUSE_SENSITIVITY: f32 = 0.0025;
/// Pitch clamp (radians). 89° = 1.5533 rad. Hard-clamp prevents look-up-and-
/// over which inverts the world (gimbal-lock-adjacent).
pub const PITCH_CLAMP_RAD: f32 = 1.553_343; // 89° in rad

/// Player's eye-camera. Position + yaw + pitch. World-space, y-up.
///
/// The `pos` field is the EYE-position (camera target = pos + forward). The
/// player capsule's CENTER is `pos - (0, 0.7, 0)` (eye is 0.7m above center
/// for a 1.7m-tall capsule with center at 0.85m and eye at 1.55m above feet).
/// The capsule-center calculation lives in physics.rs ; movement.rs only
/// tracks the eye.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    pub pos: [f32; 3],
    pub yaw: f32,
    pub pitch: f32,
}

impl Camera {
    /// Construct at world-origin facing +Z. Eye height 1.55m for a 1.7m capsule.
    pub fn new() -> Self {
        Self {
            pos: [0.0, 1.55, 0.0],
            yaw: 0.0,
            pitch: 0.0,
        }
    }

    /// Spawn-with-position (e.g. for scene-load).
    pub fn at(pos: [f32; 3]) -> Self {
        Self { pos, yaw: 0.0, pitch: 0.0 }
    }

    /// Forward unit-vector in world-space. Used by physics + render.
    pub fn forward(&self) -> [f32; 3] {
        let cp = self.pitch.cos();
        [
            self.yaw.sin() * cp,
            self.pitch.sin(),
            self.yaw.cos() * cp,
        ]
    }

    /// Right unit-vector. Always horizontal (y=0) — independent of pitch.
    pub fn right(&self) -> [f32; 3] {
        [self.yaw.cos(), 0.0, -self.yaw.sin()]
    }

    /// Apply mouse-look from input-frame. Pitch is clamped to ±89°.
    pub fn apply_look(&mut self, frame: &InputFrame) {
        self.yaw += frame.yaw_delta * MOUSE_SENSITIVITY;
        self.pitch -= frame.pitch_delta * MOUSE_SENSITIVITY; // mouse-up = look-up
        // Wrap yaw to keep numerically stable across long sessions. Use modulo
        // to handle arbitrarily-large deltas in O(1) rather than a while-loop.
        let tau = std::f32::consts::TAU;
        self.yaw = self.yaw.rem_euclid(tau);
        if self.yaw > tau * 0.5 {
            self.yaw -= tau;
        }
        // Hard-clamp pitch.
        self.pitch = self.pitch.clamp(-PITCH_CLAMP_RAD, PITCH_CLAMP_RAD);
    }

    /// Compute a CANDIDATE position delta from input-frame + dt. Does NOT
    /// apply it — physics.rs validates and commits. Returns the world-space
    /// delta vector.
    pub fn propose_motion(&self, frame: &InputFrame, dt_secs: f32) -> [f32; 3] {
        let speed = if frame.sprint {
            SPEED_M_PER_S * SPRINT_MULT
        } else {
            SPEED_M_PER_S
        };
        // Normalize horizontal input so diagonal isn't faster.
        let horiz_mag = frame.forward.hypot(frame.right);
        let inv_mag = if horiz_mag > 1.0e-3 { 1.0 / horiz_mag } else { 1.0 };
        let f = self.forward();
        let r = self.right();
        // Horizontal motion uses the camera's forward (which has y-component
        // when looking up/down — gives natural "fly-forward" if pitched) but
        // we PROJECT it to horizontal-plane for floor-walking. The render
        // sibling can opt-in to fly-mode by skipping the projection.
        let f_horiz_mag = f[0].hypot(f[2]);
        let f_horiz = if f_horiz_mag > 1.0e-3 {
            [f[0] / f_horiz_mag, 0.0, f[2] / f_horiz_mag]
        } else {
            [0.0, 0.0, 1.0]
        };
        // mul_add for accuracy : x·a + y·b = a.mul_add(x, y·b)
        let scale = speed * dt_secs * inv_mag;
        let dx = f_horiz[0].mul_add(frame.forward, r[0] * frame.right) * scale;
        let dz = f_horiz[2].mul_add(frame.forward, r[2] * frame.right) * scale;
        let dy = frame.up * speed * dt_secs;
        [dx, dy, dz]
    }

    /// Commit a (possibly-clamped) world-space delta. Physics.rs returns the
    /// validated delta after axis-slide ; the host then calls this.
    pub fn commit_motion(&mut self, delta: [f32; 3]) {
        self.pos[0] += delta[0];
        self.pos[1] += delta[1];
        self.pos[2] += delta[2];
        if std::env::var("CSSL_LOG_VERBOSE").is_ok() {
            log_event(
                "DEBUG",
                "loa-host/movement",
                &format!(
                    "frame · pos=({:.3},{:.3},{:.3}) yaw={:.3} pitch={:.3}",
                    self.pos[0], self.pos[1], self.pos[2], self.yaw, self.pitch
                ),
            );
        }
    }
}

impl Default for Camera {
    fn default() -> Self {
        Self::new()
    }
}

/// Tunable knobs collected for documentation discoverability. Not used
/// directly by Camera (which references the consts) but exported for
/// renderer + DM/GM + tests.
#[derive(Debug, Clone, Copy)]
pub struct MovementParams {
    pub walk_speed: f32,
    pub sprint_mult: f32,
    pub mouse_sensitivity: f32,
    pub pitch_clamp_rad: f32,
}

impl Default for MovementParams {
    fn default() -> Self {
        Self {
            walk_speed: SPEED_M_PER_S,
            sprint_mult: SPRINT_MULT,
            mouse_sensitivity: MOUSE_SENSITIVITY,
            pitch_clamp_rad: PITCH_CLAMP_RAD,
        }
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp, clippy::suboptimal_flops, clippy::imprecise_flops)]
mod tests {
    use super::*;
    use crate::input::InputFrame;

    fn frame_walk_forward() -> InputFrame {
        InputFrame {
            forward: 1.0,
            right: 0.0,
            up: 0.0,
            yaw_delta: 0.0,
            pitch_delta: 0.0,
            sprint: false,
            render_mode: 0,
            paused: false,
            debug_overlay: false,
            quit_requested: false,
        }
    }

    fn frame_mouse(dx: f32, dy: f32) -> InputFrame {
        InputFrame {
            forward: 0.0,
            right: 0.0,
            up: 0.0,
            yaw_delta: dx,
            pitch_delta: dy,
            sprint: false,
            render_mode: 0,
            paused: false,
            debug_overlay: false,
            quit_requested: false,
        }
    }

    #[test]
    fn camera_new_at_origin_facing_z() {
        let c = Camera::new();
        assert_eq!(c.pos, [0.0, 1.55, 0.0]);
        assert_eq!(c.yaw, 0.0);
        assert_eq!(c.pitch, 0.0);
        let f = c.forward();
        // yaw=0, pitch=0 → forward = (sin(0)·cos(0), sin(0), cos(0)·cos(0)) = (0, 0, 1)
        assert!((f[0] - 0.0).abs() < 1e-6);
        assert!((f[1] - 0.0).abs() < 1e-6);
        assert!((f[2] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn pitch_clamp_at_89_degrees() {
        let mut c = Camera::new();
        // Push pitch way down via mouse. Mouse-down in screen-space → looking-up
        // (negative pitch_delta inverts to positive pitch via apply_look → pitch -= ...).
        // To force pitch positive (look UP) we need NEGATIVE pitch_delta input.
        c.apply_look(&frame_mouse(0.0, -1_000_000.0));
        assert!((c.pitch - PITCH_CLAMP_RAD).abs() < 1e-3);
        // And the other direction.
        c.apply_look(&frame_mouse(0.0, 2_000_000.0));
        assert!((c.pitch + PITCH_CLAMP_RAD).abs() < 1e-3);
    }

    #[test]
    fn propose_motion_walk_forward() {
        let c = Camera::new();
        let dt = 0.1; // 100ms
        let delta = c.propose_motion(&frame_walk_forward(), dt);
        // yaw=0 → forward = +Z. dz should be SPEED · dt = 5.0 · 0.1 = 0.5
        assert!((delta[0] - 0.0).abs() < 1e-5);
        assert!((delta[1] - 0.0).abs() < 1e-5);
        assert!((delta[2] - 0.5).abs() < 1e-5);
    }

    #[test]
    fn sprint_doubles_speed() {
        let c = Camera::new();
        let mut f = frame_walk_forward();
        f.sprint = true;
        let delta = c.propose_motion(&f, 0.1);
        // Sprint = 5.0 · 2.0 · 0.1 = 1.0
        assert!((delta[2] - 1.0).abs() < 1e-5);
    }

    #[test]
    fn diagonal_normalized() {
        let c = Camera::new();
        let mut f = frame_walk_forward();
        f.right = 1.0;
        let delta = c.propose_motion(&f, 1.0); // dt=1 to read magnitudes directly
        let mag = (delta[0] * delta[0] + delta[2] * delta[2]).sqrt();
        // Should equal SPEED · dt = 5.0, NOT 5√2.
        assert!((mag - SPEED_M_PER_S).abs() < 1e-4);
    }

    #[test]
    fn commit_motion_advances_position() {
        let mut c = Camera::new();
        c.commit_motion([1.0, 0.0, 2.0]);
        assert!((c.pos[0] - 1.0).abs() < 1e-6);
        assert!((c.pos[2] - 2.0).abs() < 1e-6);
    }

    #[test]
    fn yaw_wraps_at_tau() {
        let mut c = Camera::new();
        // 200000 px·sensitivity = 200000 · 0.0025 = 500 rad → many tau-wraps.
        c.apply_look(&frame_mouse(200_000.0, 0.0));
        assert!(c.yaw.abs() < std::f32::consts::TAU + 1e-3);
    }
}
