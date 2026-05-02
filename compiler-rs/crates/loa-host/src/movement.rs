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
    /// § AXIS-CONVENTION : yaw=0 → forward=(0,0,-1) i.e. -Z. This matches
    ///   `camera::Camera` (render-side) which uses `Mat4::look_to_rh` with
    ///   `dir=-Z` at yaw=0. Prior bug : movement-side forward was +Z which
    ///   made pressing D move toward +X while the rendered camera's screen-
    ///   right was -X, causing inverted strafe (Apocky play-test report).
    pub fn forward(&self) -> [f32; 3] {
        let cp = self.pitch.cos();
        [
            self.yaw.sin() * cp,
            self.pitch.sin(),
            -self.yaw.cos() * cp,
        ]
    }

    /// Right unit-vector. Always horizontal (y=0) — independent of pitch.
    /// § AXIS-CONVENTION : computed as `forward × world_up` so a press of D
    ///   moves the player toward the screen's right side (matches the
    ///   rendered view-matrix's right-axis). At yaw=0 right = (1, 0, 0).
    pub fn right(&self) -> [f32; 3] {
        // forward × Y_up : (sin·cp, sin·p, -cos·cp) × (0, 1, 0) =
        //   (sin·p·0 - (-cos·cp)·1, (-cos·cp)·0 - sin·cp·0, sin·cp·1 - sin·p·0)
        //   = (cos·cp, 0, sin·cp)
        // Drop the cp scaling on x/z (physics treats right as horizontal-only)
        // and use cos(yaw)/sin(yaw). At yaw=0 → (1, 0, 0).
        [self.yaw.cos(), 0.0, self.yaw.sin()]
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

// ═══════════════════════════════════════════════════════════════════════
// § T11-W13-MOVEMENT-AUG : sprint + slide + jump-pack + parkour.
// ═══════════════════════════════════════════════════════════════════════
//
// Apex/Titanfall-style fluid traversal lives in the path-DEP-FREE sister
// crate `cssl-host-movement-aug`. Here we only define light SHIM helpers
// so this module can produce the camera-basis vectors + intent struct that
// the augmentation engine ingests, WITHOUT taking a path-dep on the new
// crate (loa-host already has 30+ deps).
//
// The integration commit will add `cssl-host-movement-aug = { path = ... }`
// to Cargo.toml and a `MovementAugBridge` field on the runtime state.
// Until that integration lands, the helpers below are call-site-ready and
// match the exact signatures of `cssl_host_movement_aug::MovementAug::tick`.

/// Camera basis projected to the horizontal plane, ready to feed
/// `MovementAug::tick(.., forward_xz, right_xz, ..)`.
///
/// Returns `(forward_xz, right_xz)` where each is a 2-element [x, z] pair.
/// Y components are dropped (movement-aug operates on the floor plane).
#[must_use]
pub fn camera_basis_xz(camera: &Camera) -> ([f32; 2], [f32; 2]) {
    let f = camera.forward();
    let r = camera.right();
    let f_mag = f[0].hypot(f[2]).max(1.0e-6);
    let r_mag = r[0].hypot(r[2]).max(1.0e-6);
    (
        [f[0] / f_mag, f[2] / f_mag],
        [r[0] / r_mag, r[2] / r_mag],
    )
}

/// Light shim for the movement-aug intent struct (kept type-compatible with
/// `cssl_host_movement_aug::MovementIntent` field-by-field). Construct from
/// an `InputFrame` ; the host's main loop then passes this to the aug-engine.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct MovementAugIntent {
    pub forward: f32,
    pub right: f32,
    pub sprint_held: bool,
    pub crouch_held: bool,
    pub jump_pressed: bool,
    pub mantle_pressed: bool,
}

impl MovementAugIntent {
    /// Project an `InputFrame` into the augmentation engine's intent shape.
    #[must_use]
    pub fn from_input_frame(frame: &crate::input::InputFrame) -> Self {
        Self {
            forward: frame.forward,
            right: frame.right,
            sprint_held: frame.sprint,
            crouch_held: frame.crouch_held,
            jump_pressed: frame.jump_pressed,
            // Auto-mantle is the default ; the dedicated press is reserved
            // for an accessibility-input wave that doesn't exist yet.
            mantle_pressed: false,
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
            ..InputFrame::default()
        }
    }

    fn frame_mouse(dx: f32, dy: f32) -> InputFrame {
        InputFrame {
            yaw_delta: dx,
            pitch_delta: dy,
            ..InputFrame::default()
        }
    }

    #[test]
    fn camera_new_at_origin_facing_z() {
        let c = Camera::new();
        assert_eq!(c.pos, [0.0, 1.55, 0.0]);
        assert_eq!(c.yaw, 0.0);
        assert_eq!(c.pitch, 0.0);
        let f = c.forward();
        // yaw=0, pitch=0 → forward = (sin(0)·cos(0), sin(0), -cos(0)·cos(0)) = (0, 0, -1)
        // matches camera::Camera (render-side) which uses Mat4::look_to_rh with -Z fwd
        assert!((f[0] - 0.0).abs() < 1e-6);
        assert!((f[1] - 0.0).abs() < 1e-6);
        assert!((f[2] - (-1.0)).abs() < 1e-6);
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
        // yaw=0 → forward = -Z. dz should be -SPEED · dt = -5.0 · 0.1 = -0.5
        assert!((delta[0] - 0.0).abs() < 1e-5);
        assert!((delta[1] - 0.0).abs() < 1e-5);
        assert!((delta[2] - (-0.5)).abs() < 1e-5);
    }

    #[test]
    fn sprint_doubles_speed() {
        let c = Camera::new();
        let mut f = frame_walk_forward();
        f.sprint = true;
        let delta = c.propose_motion(&f, 0.1);
        // Sprint = -5.0 · 2.0 · 0.1 = -1.0 (forward is -Z at yaw=0)
        assert!((delta[2] - (-1.0)).abs() < 1e-5);
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

    // § T11-W13-MOVEMENT-AUG shim coverage ──────────────────────────────────

    #[test]
    fn camera_basis_xz_at_yaw_zero() {
        let c = Camera::new();
        let (fwd, right) = camera_basis_xz(&c);
        // yaw=0, pitch=0 → forward = (0, 0, -1) ; right = (1, 0, 0)
        assert!(fwd[0].abs() < 1e-6);
        assert!((fwd[1] - (-1.0)).abs() < 1e-6);
        assert!((right[0] - 1.0).abs() < 1e-6);
        assert!(right[1].abs() < 1e-6);
    }

    #[test]
    fn movement_aug_intent_propagates_input_frame_flags() {
        let mut frame = InputFrame::default();
        frame.forward = 1.0;
        frame.right = -0.5;
        frame.sprint = true;
        frame.crouch_held = true;
        frame.jump_pressed = true;
        let intent = MovementAugIntent::from_input_frame(&frame);
        assert!((intent.forward - 1.0).abs() < 1e-6);
        assert!((intent.right - (-0.5)).abs() < 1e-6);
        assert!(intent.sprint_held);
        assert!(intent.crouch_held);
        assert!(intent.jump_pressed);
        assert!(!intent.mantle_pressed);
    }

    #[test]
    fn movement_aug_intent_default_idle() {
        let intent = MovementAugIntent::from_input_frame(&InputFrame::default());
        assert_eq!(intent, MovementAugIntent::default());
    }
}
