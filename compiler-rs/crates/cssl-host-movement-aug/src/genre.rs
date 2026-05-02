// § genre.rs — camera-mode driven input translation.
// ════════════════════════════════════════════════════════════════════
// § I> Sibling W13-4 surfaces a CameraMode enum ; we accept a mirror here
//      so we don't take a path-dep on the camera crate.
// § I> FPS = direct WASD · ThirdPerson = momentum-arrow · Iso/TopDown = grid-snap.
// § I> Genre-shift round-trip is bit-equal in the mechanical channel — only
//      input-translation changes.
// ════════════════════════════════════════════════════════════════════

use serde::{Deserialize, Serialize};

use crate::intent::MovementIntent;

/// Genre / camera-mode discriminant. Mirrors the W13-4 sibling enum to avoid
/// a cyclic path-dep ; the loa-host integration commit will round-trip
/// these via a `From` impl.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CameraGenre {
    /// First-person — direct WASD → camera-relative motion.
    Fps,
    /// Third-person — same WASD math but momentum-arrow is render-visible.
    ThirdPerson,
    /// Isometric — 45°-rotated grid-snap (Diablo-feel).
    Iso,
    /// Top-down — orthographic ; same grid-snap logic as Iso.
    TopDown,
}

impl Default for CameraGenre {
    fn default() -> Self {
        Self::Fps
    }
}

/// Translates an incoming `MovementIntent` based on camera-genre.
///
/// FPS / ThirdPerson : passthrough (axes stay analog ; downstream camera-
/// basis transform handles direction).
///
/// Iso / TopDown : snaps to 1m grid steps (or 0 if input below threshold).
/// Stamina-budget still applies regardless — the SAME `MovementAug::tick`
/// state-machine consumes the translated intent.
#[derive(Debug, Clone, Copy)]
pub struct GenreTranslator {
    pub mode: CameraGenre,
}

impl GenreTranslator {
    pub const fn new(mode: CameraGenre) -> Self {
        Self { mode }
    }

    /// Apply genre-specific input rewriting.
    pub fn translate(&self, intent: &MovementIntent) -> MovementIntent {
        match self.mode {
            CameraGenre::Fps | CameraGenre::ThirdPerson => *intent,
            CameraGenre::Iso | CameraGenre::TopDown => {
                let snap = |x: f32| -> f32 {
                    if x > 0.5 {
                        1.0
                    } else if x < -0.5 {
                        -1.0
                    } else {
                        0.0
                    }
                };
                MovementIntent {
                    forward: snap(intent.forward),
                    right: snap(intent.right),
                    ..*intent
                }
            }
        }
    }

    /// True if the renderer should display a momentum-arrow overlay.
    /// FPS hides it (you ARE the arrow) ; everything else shows it.
    pub fn shows_momentum_arrow(&self) -> bool {
        !matches!(self.mode, CameraGenre::Fps)
    }
}

impl Default for GenreTranslator {
    fn default() -> Self {
        Self::new(CameraGenre::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fps_passthrough() {
        let t = GenreTranslator::new(CameraGenre::Fps);
        let inp = MovementIntent {
            forward: 0.7,
            right: 0.3,
            ..Default::default()
        };
        let out = t.translate(&inp);
        assert_eq!(out, inp);
    }

    #[test]
    fn iso_grid_snaps_to_unit() {
        let t = GenreTranslator::new(CameraGenre::Iso);
        let inp = MovementIntent {
            forward: 0.7,
            right: 0.3,
            ..Default::default()
        };
        let out = t.translate(&inp);
        assert!((out.forward - 1.0).abs() < 1e-6);
        assert!((out.right - 0.0).abs() < 1e-6);
    }

    #[test]
    fn topdown_negative_snap() {
        let t = GenreTranslator::new(CameraGenre::TopDown);
        let inp = MovementIntent {
            forward: -0.8,
            right: -0.6,
            ..Default::default()
        };
        let out = t.translate(&inp);
        assert!((out.forward - (-1.0)).abs() < 1e-6);
        assert!((out.right - (-1.0)).abs() < 1e-6);
    }

    #[test]
    fn momentum_arrow_visibility_by_genre() {
        assert!(!GenreTranslator::new(CameraGenre::Fps).shows_momentum_arrow());
        assert!(GenreTranslator::new(CameraGenre::ThirdPerson).shows_momentum_arrow());
        assert!(GenreTranslator::new(CameraGenre::Iso).shows_momentum_arrow());
        assert!(GenreTranslator::new(CameraGenre::TopDown).shows_momentum_arrow());
    }

    #[test]
    fn genre_roundtrip_preserves_buttons() {
        let t = GenreTranslator::new(CameraGenre::Iso);
        let inp = MovementIntent {
            forward: 1.0,
            right: 0.0,
            sprint_held: true,
            crouch_held: false,
            jump_pressed: true,
            mantle_pressed: false,
        };
        let out = t.translate(&inp);
        assert_eq!(out.sprint_held, inp.sprint_held);
        assert_eq!(out.jump_pressed, inp.jump_pressed);
        assert_eq!(out.crouch_held, inp.crouch_held);
    }
}
