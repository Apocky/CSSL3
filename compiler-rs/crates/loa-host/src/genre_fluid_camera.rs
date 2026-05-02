//! § genre-fluid-camera — 4-mode perspective-fluid camera system
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § T11-W13-CAMERA (W13-4) : genre-fluid camera + perspective-switch
//!
//! § ROLE
//!   Provides 4 camera-modes layered atop the primitive `Camera` (from
//!   `camera.rs`). The base `Camera` exposes eye-state + matrices ; this
//!   module adds MODE + smooth-transition between modes WITHOUT touching
//!   the world-state. Mode-switch only re-poses the camera ; the ω-field
//!   + ECS + render targets stay invariant.
//!
//! § AXIOMS  (inherit Labyrinth of Apocalypse/systems/genre_fluid_camera.csl)
//!   t∞: 4 camera-modes · {FpsLocked, ThirdPersonOverShoulder, Isometric, TopDown}
//!   t∞: transition = cubic-ease · 300ms-default · player-tunable
//!   t∞: same-world-state ← only-camera-pose mutates · ω-field invariant
//!   t∞: sovereign-revocable ← player-cap toggle ALWAYS available
//!   t∞: ¬ forced-against-consent · DM/scene-prompt → player-confirms
//!   t∞: ALWAYS player-revertable ← prior-mode preserved on cap-revoke
//!
//! § PRIME-DIRECTIVE COMPLIANCE
//!   ✓ sovereign-revoke ← `revoke_sovereign()` restores prior mode
//!   ✓ ¬ forced-cap ← every transition records consent-source
//!   ✓ same-world-state ← attestation-flag returned per-tick
//!
//! § INTEGRATION
//!   - W13-1 (render) : reads `current_camera()` + `effective_fov()`
//!   - W13-5 (ADS-recoil) : reads `mode() == FpsLocked` to gate ADS
//!   - W13-6 (movement) : reads `mode()` to scale move-speed per-pose
//!   - W13-11 (input) : calls `request_mode_switch()` on player-keybind

#![allow(clippy::module_name_repetitions)]

use crate::camera::Camera;
use glam::{Mat4, Vec3};

// ──────────────────────────────────────────────────────────────────────────
// § CONSTANTS  (mirror systems/genre_fluid_camera.csl)
// ──────────────────────────────────────────────────────────────────────────

/// Default smooth-transition duration in milliseconds.
pub const DEFAULT_TRANSITION_MS: u32 = 300;

/// Per-mode default vertical FOV in radians.
pub const FOV_FPS_DEG: f32 = 90.0;
pub const FOV_THIRD_DEG: f32 = 80.0;

/// Per-mode shoulder/elevation offsets (world-space, relative to player-feet).
pub const FPS_EYE_HEIGHT: f32 = 1.7;
pub const THIRD_SHOULDER_BACK: f32 = 3.5;
pub const THIRD_SHOULDER_RIGHT: f32 = 0.7;
pub const THIRD_SHOULDER_UP: f32 = 1.9;
pub const ISO_HEIGHT: f32 = 12.0;
pub const ISO_BACK: f32 = 12.0;
pub const ISO_PITCH_DEG: f32 = -35.264; // arctan(1/√2) ≈ true-iso
pub const ISO_YAW_DEG: f32 = 45.0;
pub const ISO_ORTHO_HALF_HEIGHT: f32 = 8.0;
pub const TOPDOWN_HEIGHT: f32 = 18.0;
pub const TOPDOWN_ORTHO_HALF_HEIGHT: f32 = 10.0;

// ──────────────────────────────────────────────────────────────────────────
// § ENUMS
// ──────────────────────────────────────────────────────────────────────────

/// 4 mutually-exclusive camera modes. Layout matches genre_fluid_camera.csl.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CameraMode {
    /// First-person locked-to-head. FOV 90°.
    FpsLocked,
    /// Third-person over-the-shoulder. FOV 80°.
    ThirdPersonOverShoulder,
    /// Fixed-orthographic 45°-yaw 35°-pitch isometric.
    Isometric,
    /// Pure-overhead orthographic projection.
    TopDown,
}

impl CameraMode {
    /// Per-mode default FOV (radians). Ortho-modes return 0 (unused).
    #[must_use]
    pub fn fov_y(self) -> f32 {
        match self {
            Self::FpsLocked => FOV_FPS_DEG.to_radians(),
            Self::ThirdPersonOverShoulder => FOV_THIRD_DEG.to_radians(),
            Self::Isometric | Self::TopDown => 0.0,
        }
    }

    /// Whether this mode uses orthographic projection.
    #[must_use]
    pub fn is_orthographic(self) -> bool {
        matches!(self, Self::Isometric | Self::TopDown)
    }

    /// Stable u8 discriminant for serialization / Σ-Chain attestation.
    #[must_use]
    pub fn as_u8(self) -> u8 {
        match self {
            Self::FpsLocked => 0,
            Self::ThirdPersonOverShoulder => 1,
            Self::Isometric => 2,
            Self::TopDown => 3,
        }
    }
}

/// Source-of-truth for who initiated a mode-transition. Audit-trail input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionSource {
    /// Player keybind / explicit toggle. ALWAYS-honored, ¬ confirm-required.
    PlayerToggle,
    /// DM-orchestrator arc-phase-shift (cinematic). Player can revert.
    DmOrchestrated,
    /// In-scene narrative-prompt. Requires player-confirm BEFORE shift.
    ScenePromptConfirmed,
    /// Sovereign-revoke restoring prior-mode. Bypasses normal lerp.
    SovereignRevoke,
}

// ──────────────────────────────────────────────────────────────────────────
// § STATE
// ──────────────────────────────────────────────────────────────────────────

/// Genre-fluid camera state. Wraps the primitive `Camera` and adds
/// mode + transition + sovereign-cap state.
#[derive(Debug, Clone, Copy)]
pub struct GenreFluidCamera {
    /// Current active camera-mode.
    mode: CameraMode,
    /// Mode being transitioned-toward. None ⇔ no transition active.
    target_mode: Option<CameraMode>,
    /// Last "stable" mode prior to current — used for sovereign-revoke.
    prior_mode: CameraMode,
    /// Transition progress ∈ [0, 1]. 0 = at-source, 1 = at-target.
    progress: f32,
    /// Configured transition duration (ms). Player-tunable.
    duration_ms: u32,
    /// Player-tracked focal-point (e.g. avatar-head world-position).
    pub player_focus: Vec3,
    /// Player yaw (drives 3rd-person/iso orbit). Radians.
    pub player_yaw: f32,
    /// Player pitch (FPS only — 3rd/iso/top-down clamp pitch).
    pub player_pitch: f32,
    /// Whether the genre-fluid system is currently sovereign-enabled.
    /// `false` ⇔ player has revoked the cap → mode locked to FpsLocked.
    sovereign_enabled: bool,
    /// Audit-source of the most-recent transition.
    last_source: Option<TransitionSource>,
    /// Same-world-state attestation : flips false ONLY if a bug allows
    /// world mutation during a mode-switch. Tested in unit-tests.
    world_state_invariant: bool,
}

impl Default for GenreFluidCamera {
    fn default() -> Self {
        Self {
            mode: CameraMode::FpsLocked,
            target_mode: None,
            prior_mode: CameraMode::FpsLocked,
            progress: 1.0,
            duration_ms: DEFAULT_TRANSITION_MS,
            player_focus: Vec3::new(0.0, FPS_EYE_HEIGHT, 0.0),
            player_yaw: 0.0,
            player_pitch: 0.0,
            sovereign_enabled: true,
            last_source: None,
            world_state_invariant: true,
        }
    }
}

impl GenreFluidCamera {
    /// Construct a new genre-fluid camera at default-pose.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Currently-active mode (or transition-source if a transition is in-flight).
    #[must_use]
    pub fn mode(&self) -> CameraMode {
        self.mode
    }

    /// In-flight target-mode (None ⇔ no transition).
    #[must_use]
    pub fn target_mode(&self) -> Option<CameraMode> {
        self.target_mode
    }

    /// True ⇔ a transition is currently animating.
    #[must_use]
    pub fn is_transitioning(&self) -> bool {
        self.target_mode.is_some() && self.progress < 1.0
    }

    /// Prior-mode used by sovereign-revoke.
    #[must_use]
    pub fn prior_mode(&self) -> CameraMode {
        self.prior_mode
    }

    /// Sovereign-cap state. `false` ⇔ player has revoked → locked-FPS.
    #[must_use]
    pub fn sovereign_enabled(&self) -> bool {
        self.sovereign_enabled
    }

    /// Same-world-state attestation flag (always-true unless bug detected).
    #[must_use]
    pub fn world_state_invariant(&self) -> bool {
        self.world_state_invariant
    }

    /// Audit-trail : source of the last-honored transition.
    #[must_use]
    pub fn last_transition_source(&self) -> Option<TransitionSource> {
        self.last_source
    }

    /// Configure transition duration. 0 ⇔ instant-snap.
    pub fn set_transition_duration_ms(&mut self, ms: u32) {
        self.duration_ms = ms.clamp(0, 5_000);
    }

    /// Currently-effective FOV (radians). Cubic-ease blend during transition.
    /// Returns 0 for orthographic modes ; use `effective_ortho_half_height`.
    #[must_use]
    pub fn effective_fov(&self) -> f32 {
        match (self.mode, self.target_mode) {
            (m, None) => m.fov_y(),
            (a, Some(b)) => {
                if a.is_orthographic() && b.is_orthographic() {
                    0.0
                } else if a.is_orthographic() {
                    // ortho → persp : ramp-in target FOV
                    b.fov_y() * cubic_ease(self.progress)
                } else if b.is_orthographic() {
                    // persp → ortho : ramp-out source FOV
                    a.fov_y() * (1.0 - cubic_ease(self.progress))
                } else {
                    // persp → persp : direct lerp
                    lerp(a.fov_y(), b.fov_y(), cubic_ease(self.progress))
                }
            }
        }
    }

    /// Effective orthographic-half-height (camera-units). 0 for perspective.
    #[must_use]
    pub fn effective_ortho_half_height(&self) -> f32 {
        let src = ortho_half_for(self.mode);
        match self.target_mode {
            None => src,
            Some(t) => {
                let dst = ortho_half_for(t);
                lerp(src, dst, cubic_ease(self.progress))
            }
        }
    }

    /// Compute the effective camera-pose for the current frame. Returns
    /// the primitive `Camera` ready for view/proj-matrix derivation.
    #[must_use]
    pub fn current_camera(&self) -> Camera {
        let src_pose = pose_for(self.mode, self.player_focus, self.player_yaw, self.player_pitch);
        let pose = match self.target_mode {
            None => src_pose,
            Some(t) => {
                let dst_pose = pose_for(t, self.player_focus, self.player_yaw, self.player_pitch);
                lerp_pose(&src_pose, &dst_pose, cubic_ease(self.progress))
            }
        };
        let fov = if self.effective_fov() > 0.0 {
            self.effective_fov()
        } else {
            // Orthographic modes still need a non-zero perspective fov for
            // projection-matrix fallback ; render-side may swap to ortho.
            FOV_FPS_DEG.to_radians()
        };
        Camera {
            position: pose.position,
            yaw: pose.yaw,
            pitch: pose.pitch,
            fov_y: fov,
            znear: 0.1,
            zfar: 200.0,
        }
    }

    /// Build the projection matrix appropriate for the current mode.
    /// Perspective for FPS/3rd-person ; orthographic for iso/top-down.
    #[must_use]
    pub fn effective_proj(&self, aspect: f32) -> Mat4 {
        let h = self.effective_ortho_half_height();
        let blend = cubic_ease(self.progress);
        let pure_persp = !self.mode.is_orthographic()
            && self.target_mode.is_none_or(|t| !t.is_orthographic());
        let pure_ortho = self.mode.is_orthographic()
            && self.target_mode.is_none_or(|t| t.is_orthographic());
        if pure_persp {
            self.current_camera().proj(aspect)
        } else if pure_ortho {
            ortho(h, aspect)
        } else {
            // Mid-transition between persp ↔ ortho : commit to the target
            // projection past the half-way point ; before that, use source.
            // Renderer can additionally blend FOV/ortho-half if desired.
            let switching_to_ortho = self.target_mode.is_some_and(|t| t.is_orthographic());
            let past_midpoint = blend > 0.5;
            // XOR : ortho-target past-mid = ortho ; persp-target past-mid = persp ;
            //       inverse pairs hold for pre-mid (still in source projection).
            if switching_to_ortho == past_midpoint {
                ortho(h, aspect)
            } else {
                self.current_camera().proj(aspect)
            }
        }
    }

    // ──────────────────────────────────────────────────────────────────
    // § TRANSITION CONTROL
    // ──────────────────────────────────────────────────────────────────

    /// Request a mode-switch from the given source. Returns `true` if
    /// the switch was accepted (sovereign-cap allows it AND the source
    /// is honored). `ScenePromptConfirmed` requires explicit confirm
    /// (call `confirm_scene_prompt` separately). `DmOrchestrated` is
    /// always-accepted but player may revert via `revoke_sovereign`.
    pub fn request_mode_switch(&mut self, target: CameraMode, source: TransitionSource) -> bool {
        // ¬ forced-cap : if sovereign-revoked, ONLY PlayerToggle/SovereignRevoke
        // can change mode (player has explicitly locked the system).
        if !self.sovereign_enabled
            && !matches!(
                source,
                TransitionSource::PlayerToggle | TransitionSource::SovereignRevoke
            )
        {
            return false;
        }
        if target == self.mode && self.target_mode.is_none() {
            // No-op : already in target mode.
            return true;
        }
        // Promote the current-mode to prior-mode UNLESS this is a revoke
        // (revoke restores prior, doesn't re-record it).
        if !matches!(source, TransitionSource::SovereignRevoke) {
            self.prior_mode = self.mode;
        }
        self.target_mode = Some(target);
        self.progress = 0.0;
        self.last_source = Some(source);
        // Sovereign-revoke is always-instant ; other sources lerp.
        if matches!(source, TransitionSource::SovereignRevoke) || self.duration_ms == 0 {
            self.progress = 1.0;
            self.commit_transition();
        }
        true
    }

    /// Step the transition forward by `dt_ms` milliseconds. Call once-per-frame.
    pub fn tick(&mut self, dt_ms: u32) {
        if self.target_mode.is_none() {
            return;
        }
        if self.duration_ms == 0 {
            self.progress = 1.0;
        } else {
            self.progress += (dt_ms as f32) / (self.duration_ms as f32);
        }
        if self.progress >= 1.0 {
            self.progress = 1.0;
            self.commit_transition();
        }
    }

    /// Finalize transition : promote target → mode, clear target.
    fn commit_transition(&mut self) {
        if let Some(t) = self.target_mode {
            self.mode = t;
            self.target_mode = None;
        }
    }

    // ──────────────────────────────────────────────────────────────────
    // § SOVEREIGN-CAP CONTROL
    // ──────────────────────────────────────────────────────────────────

    /// Player explicitly revokes the genre-fluid cap. Camera snaps back
    /// to `prior_mode` (the last stable-mode) and locks until re-granted.
    /// PRIME-DIRECTIVE compliance : ALWAYS-honored, ¬ override exists.
    pub fn revoke_sovereign(&mut self) {
        let restore = self.prior_mode;
        self.sovereign_enabled = false;
        self.request_mode_switch(restore, TransitionSource::SovereignRevoke);
    }

    /// Player re-grants the cap. Mode stays where it was when revoked.
    pub fn grant_sovereign(&mut self) {
        self.sovereign_enabled = true;
    }

    /// Confirm a pending scene-prompt transition (player-press-confirm).
    /// Promotes a `ScenePromptConfirmed` request from "queued" → "active".
    /// Stage-0 is no-op since `request_mode_switch` already activates ;
    /// reserved for stage-1 prompt-queue UI.
    pub fn confirm_scene_prompt(&mut self) {}

    // ──────────────────────────────────────────────────────────────────
    // § PLAYER-FOCUS UPDATES
    // ──────────────────────────────────────────────────────────────────

    /// Update the player's tracked focus-point (called per-frame from
    /// movement / physics). All non-FPS modes orbit this point.
    pub fn set_player_focus(&mut self, focus: Vec3) {
        self.player_focus = focus;
    }

    /// Update player yaw/pitch (mouse-look). FPS reads both ; 3rd-person
    /// reads yaw + clamps pitch ; iso/top-down ignore both.
    pub fn set_player_orient(&mut self, yaw: f32, pitch: f32) {
        self.player_yaw = yaw;
        self.player_pitch = pitch.clamp(
            -std::f32::consts::FRAC_PI_2 + 0.01,
            std::f32::consts::FRAC_PI_2 - 0.01,
        );
    }

    // ──────────────────────────────────────────────────────────────────
    // § SAME-WORLD-STATE ATTESTATION
    // ──────────────────────────────────────────────────────────────────

    /// Attestation hook : called by the world-mutation harness BEFORE
    /// any ω-field write. If `is_transitioning()` returns true AND the
    /// caller is the camera-system itself, the world-state invariant is
    /// broken (camera should NEVER mutate world during a switch).
    /// For unit-test use ; production uses the static cssl-host attestor.
    pub fn record_world_mutation_attempted(&mut self, mutator_is_camera: bool) {
        if mutator_is_camera && self.is_transitioning() {
            self.world_state_invariant = false;
        }
    }

    /// Reset attestation flag (e.g. start-of-frame).
    pub fn reset_world_state_flag(&mut self) {
        self.world_state_invariant = true;
    }
}

// ──────────────────────────────────────────────────────────────────────────
// § HELPERS — Pose Math
// ──────────────────────────────────────────────────────────────────────────

/// Camera-pose snapshot used by transition-blending.
#[derive(Debug, Clone, Copy)]
struct Pose {
    position: Vec3,
    yaw: f32,
    pitch: f32,
}

/// Compute the canonical camera-pose for a given mode + player-state.
fn pose_for(mode: CameraMode, focus: Vec3, yaw: f32, pitch: f32) -> Pose {
    match mode {
        CameraMode::FpsLocked => Pose {
            position: focus,
            yaw,
            pitch,
        },
        CameraMode::ThirdPersonOverShoulder => {
            // Place camera back + above + right-of player, looking-at focus.
            let back_dir = Vec3::new(-yaw.sin(), 0.0, yaw.cos()).normalize();
            let right_dir = Vec3::new(yaw.cos(), 0.0, yaw.sin()).normalize();
            let pos = focus
                + back_dir * THIRD_SHOULDER_BACK
                + right_dir * THIRD_SHOULDER_RIGHT
                + Vec3::new(0.0, THIRD_SHOULDER_UP - FPS_EYE_HEIGHT, 0.0);
            Pose {
                position: pos,
                yaw,
                pitch: pitch.clamp(-0.6, 0.4),
            }
        }
        CameraMode::Isometric => {
            let iso_yaw = ISO_YAW_DEG.to_radians();
            let iso_pitch = ISO_PITCH_DEG.to_radians();
            let back_dir = Vec3::new(-iso_yaw.sin(), 0.0, iso_yaw.cos()).normalize();
            let pos = focus + back_dir * ISO_BACK + Vec3::new(0.0, ISO_HEIGHT, 0.0);
            Pose {
                position: pos,
                yaw: iso_yaw,
                pitch: iso_pitch,
            }
        }
        CameraMode::TopDown => Pose {
            position: focus + Vec3::new(0.0, TOPDOWN_HEIGHT, 0.0),
            yaw: 0.0,
            pitch: -std::f32::consts::FRAC_PI_2 + 0.001,
        },
    }
}

fn ortho_half_for(mode: CameraMode) -> f32 {
    match mode {
        CameraMode::FpsLocked | CameraMode::ThirdPersonOverShoulder => 0.0,
        CameraMode::Isometric => ISO_ORTHO_HALF_HEIGHT,
        CameraMode::TopDown => TOPDOWN_ORTHO_HALF_HEIGHT,
    }
}

fn ortho(half_height: f32, aspect: f32) -> Mat4 {
    let h = half_height.max(0.5);
    let w = h * aspect;
    Mat4::orthographic_rh(-w, w, -h, h, 0.1, 200.0)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn lerp_vec3(a: Vec3, b: Vec3, t: f32) -> Vec3 {
    a + (b - a) * t
}

fn lerp_pose(a: &Pose, b: &Pose, t: f32) -> Pose {
    Pose {
        position: lerp_vec3(a.position, b.position, t),
        yaw: lerp_angle(a.yaw, b.yaw, t),
        pitch: lerp(a.pitch, b.pitch, t),
    }
}

/// Angular-lerp picking the shortest-arc (handles ±π wrap).
fn lerp_angle(a: f32, b: f32, t: f32) -> f32 {
    let two_pi = std::f32::consts::TAU;
    let mut d = (b - a) % two_pi;
    if d > std::f32::consts::PI {
        d -= two_pi;
    } else if d < -std::f32::consts::PI {
        d += two_pi;
    }
    a + d * t
}

/// Cubic-ease : smooth-step `3t²-2t³`.  C¹-continuous endpoints.
fn cubic_ease(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

// ──────────────────────────────────────────────────────────────────────────
// § TESTS
// ──────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() < eps
    }

    #[test]
    fn default_mode_is_fps_locked() {
        let g = GenreFluidCamera::new();
        assert_eq!(g.mode(), CameraMode::FpsLocked);
        assert_eq!(g.target_mode(), None);
        assert!(g.sovereign_enabled());
    }

    #[test]
    fn four_mode_roundtrip_fps_to_third_to_iso_to_top_to_fps() {
        let mut g = GenreFluidCamera::new();
        g.set_transition_duration_ms(0); // instant-snap for test determinism

        for target in [
            CameraMode::ThirdPersonOverShoulder,
            CameraMode::Isometric,
            CameraMode::TopDown,
            CameraMode::FpsLocked,
        ] {
            assert!(g.request_mode_switch(target, TransitionSource::PlayerToggle));
            g.tick(1);
            assert_eq!(g.mode(), target);
            assert!(!g.is_transitioning());
        }
    }

    #[test]
    fn transition_completes_at_default_300ms_duration() {
        let mut g = GenreFluidCamera::new();
        g.set_transition_duration_ms(DEFAULT_TRANSITION_MS);
        assert!(g.request_mode_switch(
            CameraMode::ThirdPersonOverShoulder,
            TransitionSource::PlayerToggle
        ));
        // mid-transition
        g.tick(150);
        assert!(g.is_transitioning());
        assert!(g.mode() == CameraMode::FpsLocked);
        // complete
        g.tick(150);
        assert!(!g.is_transitioning());
        assert_eq!(g.mode(), CameraMode::ThirdPersonOverShoulder);
    }

    #[test]
    fn fov_lerps_between_modes_during_transition() {
        let mut g = GenreFluidCamera::new();
        g.set_transition_duration_ms(DEFAULT_TRANSITION_MS);
        let fps_fov = g.effective_fov();
        assert!(approx_eq(fps_fov, FOV_FPS_DEG.to_radians(), 1e-5));
        g.request_mode_switch(
            CameraMode::ThirdPersonOverShoulder,
            TransitionSource::PlayerToggle,
        );
        g.tick(150); // halfway
        let mid = g.effective_fov();
        // halfway with cubic-ease-at-0.5 = 0.5
        let expected = lerp(
            FOV_FPS_DEG.to_radians(),
            FOV_THIRD_DEG.to_radians(),
            cubic_ease(0.5),
        );
        assert!(approx_eq(mid, expected, 1e-4));
    }

    #[test]
    fn world_state_invariant_holds_during_normal_transition() {
        let mut g = GenreFluidCamera::new();
        g.set_transition_duration_ms(100);
        g.request_mode_switch(CameraMode::Isometric, TransitionSource::PlayerToggle);
        // Simulate camera-internals NOT trying to mutate world.
        g.record_world_mutation_attempted(false);
        g.tick(50);
        g.record_world_mutation_attempted(false);
        g.tick(50);
        assert!(g.world_state_invariant());
    }

    #[test]
    fn world_state_invariant_breaks_if_camera_mutates_during_transition() {
        let mut g = GenreFluidCamera::new();
        g.set_transition_duration_ms(100);
        g.request_mode_switch(CameraMode::TopDown, TransitionSource::PlayerToggle);
        g.record_world_mutation_attempted(true); // bug-simulation
        assert!(!g.world_state_invariant());
    }

    #[test]
    fn sovereign_revoke_restores_prior_mode() {
        let mut g = GenreFluidCamera::new();
        g.set_transition_duration_ms(0);
        // Move FPS → ThirdPerson
        g.request_mode_switch(
            CameraMode::ThirdPersonOverShoulder,
            TransitionSource::PlayerToggle,
        );
        g.tick(1);
        assert_eq!(g.mode(), CameraMode::ThirdPersonOverShoulder);
        // Move ThirdPerson → Isometric
        g.request_mode_switch(CameraMode::Isometric, TransitionSource::PlayerToggle);
        g.tick(1);
        assert_eq!(g.mode(), CameraMode::Isometric);
        assert_eq!(g.prior_mode(), CameraMode::ThirdPersonOverShoulder);
        // Player revokes ; should restore prior (ThirdPerson)
        g.revoke_sovereign();
        assert_eq!(g.mode(), CameraMode::ThirdPersonOverShoulder);
        assert!(!g.sovereign_enabled());
    }

    #[test]
    fn dm_orchestrated_trigger_accepted_when_sovereign_enabled() {
        let mut g = GenreFluidCamera::new();
        g.set_transition_duration_ms(0);
        let ok = g.request_mode_switch(
            CameraMode::TopDown,
            TransitionSource::DmOrchestrated,
        );
        assert!(ok);
        g.tick(1);
        assert_eq!(g.mode(), CameraMode::TopDown);
        assert_eq!(
            g.last_transition_source(),
            Some(TransitionSource::DmOrchestrated)
        );
    }

    #[test]
    fn dm_trigger_rejected_when_sovereign_revoked() {
        let mut g = GenreFluidCamera::new();
        g.set_transition_duration_ms(0);
        g.revoke_sovereign();
        let ok = g.request_mode_switch(
            CameraMode::Isometric,
            TransitionSource::DmOrchestrated,
        );
        assert!(!ok);
        // Should still be FpsLocked (post-revoke restore-target).
        assert_eq!(g.mode(), CameraMode::FpsLocked);
    }

    #[test]
    fn scene_prompt_rejected_when_sovereign_revoked() {
        let mut g = GenreFluidCamera::new();
        g.set_transition_duration_ms(0);
        g.revoke_sovereign();
        let ok = g.request_mode_switch(
            CameraMode::Isometric,
            TransitionSource::ScenePromptConfirmed,
        );
        assert!(!ok);
    }

    #[test]
    fn player_toggle_always_accepted_even_after_revoke() {
        // Sovereign-revoke locks DM/scene paths but PLAYER explicit-toggle
        // is the ONLY thing that overrides — that's the non-forced spec.
        let mut g = GenreFluidCamera::new();
        g.set_transition_duration_ms(0);
        g.revoke_sovereign();
        let ok = g.request_mode_switch(
            CameraMode::TopDown,
            TransitionSource::PlayerToggle,
        );
        assert!(ok);
        g.tick(1);
        assert_eq!(g.mode(), CameraMode::TopDown);
    }

    #[test]
    fn ortho_modes_use_orthographic_projection() {
        let mut g = GenreFluidCamera::new();
        g.set_transition_duration_ms(0);
        g.request_mode_switch(CameraMode::Isometric, TransitionSource::PlayerToggle);
        g.tick(1);
        // Iso should be ortho (FOV = 0).
        assert!(g.mode().is_orthographic());
        assert_eq!(g.effective_fov(), 0.0);
        assert!(g.effective_ortho_half_height() > 0.0);

        g.request_mode_switch(CameraMode::TopDown, TransitionSource::PlayerToggle);
        g.tick(1);
        assert!(g.mode().is_orthographic());
    }

    #[test]
    fn camera_mode_u8_discriminants_stable() {
        assert_eq!(CameraMode::FpsLocked.as_u8(), 0);
        assert_eq!(CameraMode::ThirdPersonOverShoulder.as_u8(), 1);
        assert_eq!(CameraMode::Isometric.as_u8(), 2);
        assert_eq!(CameraMode::TopDown.as_u8(), 3);
    }

    #[test]
    fn cubic_ease_endpoints_clamped() {
        assert!(approx_eq(cubic_ease(0.0), 0.0, 1e-6));
        assert!(approx_eq(cubic_ease(1.0), 1.0, 1e-6));
        assert!(approx_eq(cubic_ease(0.5), 0.5, 1e-6));
        // Out-of-range clamps.
        assert!(approx_eq(cubic_ease(-0.5), 0.0, 1e-6));
        assert!(approx_eq(cubic_ease(1.5), 1.0, 1e-6));
    }

    #[test]
    fn third_person_camera_is_behind_and_above_player() {
        let mut g = GenreFluidCamera::new();
        g.set_transition_duration_ms(0);
        g.set_player_focus(Vec3::new(0.0, FPS_EYE_HEIGHT, 0.0));
        g.set_player_orient(0.0, 0.0); // facing -Z
        g.request_mode_switch(
            CameraMode::ThirdPersonOverShoulder,
            TransitionSource::PlayerToggle,
        );
        g.tick(1);
        let cam = g.current_camera();
        // Camera should be BEHIND player (negative -Z facing means cam is +Z).
        assert!(cam.position.z > 0.0, "3rd-person should be behind player");
        assert!(cam.position.y > FPS_EYE_HEIGHT, "should be above eye-height");
    }

    #[test]
    fn instant_snap_when_duration_zero() {
        let mut g = GenreFluidCamera::new();
        g.set_transition_duration_ms(0);
        g.request_mode_switch(CameraMode::TopDown, TransitionSource::PlayerToggle);
        // No tick required ; instant.
        assert_eq!(g.mode(), CameraMode::TopDown);
        assert!(!g.is_transitioning());
    }

    #[test]
    fn current_camera_at_player_focus_in_fps_mode() {
        let mut g = GenreFluidCamera::new();
        let focus = Vec3::new(5.0, 1.7, -3.0);
        g.set_player_focus(focus);
        g.set_player_orient(0.5, 0.1);
        let cam = g.current_camera();
        // FPS = camera AT focus.
        assert!(approx_eq(cam.position.x, focus.x, 1e-5));
        assert!(approx_eq(cam.position.y, focus.y, 1e-5));
        assert!(approx_eq(cam.position.z, focus.z, 1e-5));
        assert!(approx_eq(cam.yaw, 0.5, 1e-5));
        assert!(approx_eq(cam.pitch, 0.1, 1e-5));
    }

    #[test]
    fn grant_sovereign_restores_normal_operation() {
        let mut g = GenreFluidCamera::new();
        g.set_transition_duration_ms(0);
        g.revoke_sovereign();
        assert!(!g.sovereign_enabled());
        g.grant_sovereign();
        assert!(g.sovereign_enabled());
        // DM-orchestrated should now work again.
        let ok = g.request_mode_switch(
            CameraMode::Isometric,
            TransitionSource::DmOrchestrated,
        );
        assert!(ok);
    }
}
