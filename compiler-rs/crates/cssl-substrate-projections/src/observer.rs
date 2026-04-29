//! Viewport rect + observer-frame composition + LoD selector.
//!
//! § SPEC ANCHOR : `specs/30_SUBSTRATE.csl § PROJECTIONS § ObserverFrame +
//!   § LoD-SCHEMA + § PROJECTION-TARGET`. Stage-0 surfaces a slimmer subset
//!   focused on what cssl-host-vulkan / cssl-host-d3d12 need to render :
//!   pose + viewport rect + LoD policy + projection-target tag.
//!
//! § COMPOSITION
//!   ```text
//!   ObserverFrame ::= { camera : Camera ; viewport : Viewport ;
//!                       lod : LodPolicy ; target : ProjectionTarget ;
//!                       caps : CapsToken }
//!   ```
//!   Multiple `ObserverFrame` instances coexist in `Vec<ObserverFrame>` for
//!   split-screen, stereo, mini-map, debug-introspect, etc. The `caps` field
//!   gates substrate-state access on a per-projection basis (a debug-cam
//!   has full access ; the player's main camera has only consent-token
//!   access ; a companion AI's view has only what the AI consented to see).

use crate::camera::Camera;
use crate::caps::CapsToken;

/// Integer-pixel viewport rectangle in render-target coordinate space.
/// Origin is top-left ; width / height are non-negative pixel counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Viewport {
    /// X coordinate of the top-left corner, in pixels.
    pub x: u32,
    /// Y coordinate of the top-left corner, in pixels.
    pub y: u32,
    /// Viewport width, in pixels. Zero means a degenerate / disabled viewport.
    pub width: u32,
    /// Viewport height, in pixels. Zero means a degenerate / disabled viewport.
    pub height: u32,
}

impl Viewport {
    /// Construct from explicit components.
    #[must_use]
    pub const fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Aspect ratio (width / height). Returns `1.0` for degenerate viewports
    /// (zero height) — substrate totality.
    #[must_use]
    pub fn aspect(self) -> f32 {
        if self.height == 0 {
            1.0
        } else {
            self.width as f32 / self.height as f32
        }
    }

    /// `true` if both dimensions are nonzero.
    #[must_use]
    pub const fn is_valid(self) -> bool {
        self.width > 0 && self.height > 0
    }
}

/// Distance-based LoD selection policy. Stage-0 surfaces a flat slice of
/// distance thresholds — the spec's `LoDSchema` adds hysteresis +
/// detail-mask + screen-pixel-target which are deferred to a follow-up.
///
/// § SEMANTICS
///   - `thresholds[i]` is the maximum view-space distance at which LoD
///     level `i` is selected. A monotonically-non-decreasing sequence is
///     required (caller validates ; `LodPolicy::validate`).
///   - When `distance > thresholds.last()`, the LAST level is used (highest
///     LoD index = lowest detail = farthest fallback).
///   - The level returned is always `< thresholds.len()` ; an empty
///     thresholds slice produces `0` for any distance.
#[derive(Debug, Clone, PartialEq)]
pub struct LodPolicy {
    /// Distance thresholds in increasing order — `thresholds[i]` is the
    /// upper bound for level `i` (level `i+1` is selected for distances
    /// above this value).
    pub thresholds: Vec<f32>,
}

impl Default for LodPolicy {
    fn default() -> Self {
        Self::SINGLE_LEVEL
    }
}

impl LodPolicy {
    /// Single-level policy — every distance maps to LoD 0. The lowest-cost
    /// default for projections that don't need LoD switching.
    pub const SINGLE_LEVEL: Self = Self {
        thresholds: Vec::new(),
    };

    /// Construct from a slice of thresholds. The slice is copied into an
    /// owned `Vec` ; caller-owned data is not borrowed.
    #[must_use]
    pub fn from_thresholds(thresholds: &[f32]) -> Self {
        Self {
            thresholds: thresholds.to_vec(),
        }
    }

    /// Validate the policy : thresholds must be finite, non-negative, and
    /// monotonically non-decreasing.
    ///
    /// # Errors
    /// Returns `Err(LodError::*)` if any threshold is non-finite, negative,
    /// or violates monotonicity. The first violation determines the error
    /// variant.
    pub fn validate(&self) -> Result<(), LodError> {
        let mut prev = f32::NEG_INFINITY;
        for (i, &t) in self.thresholds.iter().enumerate() {
            if !t.is_finite() {
                return Err(LodError::NonFiniteThreshold { index: i, value: t });
            }
            if t < 0.0 {
                return Err(LodError::NegativeThreshold { index: i, value: t });
            }
            if t < prev {
                return Err(LodError::NonMonotonic {
                    index: i,
                    value: t,
                    previous: prev,
                });
            }
            prev = t;
        }
        Ok(())
    }

    /// Number of LoD levels — `thresholds.len() + 1` (one past every threshold).
    /// A SINGLE_LEVEL policy returns `1`.
    #[must_use]
    pub fn level_count(&self) -> usize {
        self.thresholds.len() + 1
    }
}

/// LoD-policy validation error.
#[derive(Debug, Clone, Copy, PartialEq, thiserror::Error)]
pub enum LodError {
    /// A threshold is NaN or infinity.
    #[error("LoD threshold at index {index} is non-finite : {value}")]
    NonFiniteThreshold {
        /// Index of the offending threshold.
        index: usize,
        /// Value at that index.
        value: f32,
    },
    /// A threshold is negative.
    #[error("LoD threshold at index {index} is negative : {value}")]
    NegativeThreshold {
        /// Index of the offending threshold.
        index: usize,
        /// Value at that index.
        value: f32,
    },
    /// Thresholds violate the non-decreasing monotonicity invariant.
    #[error("LoD threshold at index {index} ({value}) is below previous ({previous})")]
    NonMonotonic {
        /// Index of the offending threshold.
        index: usize,
        /// Value at that index.
        value: f32,
        /// Value at index - 1.
        previous: f32,
    },
}

/// Distance-based LoD selection. Returns the level index `[0, lod_levels.len()]`
/// such that `distance <= lod_levels[level - 1]` for the smallest `level` where
/// the inequality holds, or `lod_levels.len()` if no threshold is exceeded.
///
/// # Invariants
/// - `lod_levels` is assumed monotonically non-decreasing. If this invariant
///   is violated, the result is well-defined (a linear scan returns the first
///   threshold the distance does not exceed) but may not match the
///   user's intent ; call [`LodPolicy::validate`] to catch this.
/// - `distance` is treated as `0.0` if NaN — substrate totality.
#[must_use]
pub fn select_lod(distance: f32, lod_levels: &[f32]) -> usize {
    let d = if distance.is_nan() { 0.0 } else { distance };
    for (i, &t) in lod_levels.iter().enumerate() {
        if d <= t {
            return i;
        }
    }
    lod_levels.len()
}

/// What the projection's rendered output goes to. Mirrors the spec's
/// `ProjectionTarget` enum at the host-runtime granularity needed by
/// cssl-host-vulkan / cssl-host-d3d12.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ProjectionTarget {
    /// OS window — the substrate emits to a swapchain image.
    Window,
    /// Off-screen render-to-texture. Caller manages the texture handle ;
    /// this enum carries only the kind tag.
    Texture,
    /// Render-to-disk (file). Per spec, requires `ConsentToken<"fs">`
    /// + `Audit<"projection-record">` ; the audit binding is enforced at
    /// the cssl-rt fs surface, not here.
    File,
    /// Cull-only ; produces visibility metadata but no rendered output.
    /// Used for AI-companion projections + audio-listener surfaces.
    #[default]
    Null,
}

/// Stable handle for a single projection. Opaque `u64` ; allocation policy
/// is the host's responsibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ProjectionId(pub u64);

/// What a single observer "sees". Combines a [`Camera`] (intrinsics +
/// pose) with a viewport rect, LoD policy, target tag, and capability
/// token. Multiple `ObserverFrame` instances live in a `Vec` for
/// split-screen / stereo / mini-map / debug rendering.
///
/// § ASPECT-CONSISTENCY
///   The `Camera::aspect` field and `viewport.aspect()` SHOULD match.
///   `ObserverFrame::sync_aspect` mutates the contained camera to match
///   the viewport — call after viewport-resize.
#[derive(Debug, Clone, PartialEq)]
pub struct ObserverFrame {
    /// Stable handle for telemetry / debugger correlation.
    pub id: ProjectionId,
    /// Camera intrinsics + pose.
    pub camera: Camera,
    /// Render-target rectangle in pixel space.
    pub viewport: Viewport,
    /// LoD policy used by per-archetype detail selection.
    pub lod: LodPolicy,
    /// Where the rendered output goes.
    pub target: ProjectionTarget,
    /// Capability token gating substrate-state access.
    pub caps: CapsToken,
}

impl ObserverFrame {
    /// Construct an observer-frame. The camera's `aspect` field is left
    /// as-is — call [`Self::sync_aspect`] if the viewport differs from
    /// the camera's stored aspect.
    #[must_use]
    pub fn new(
        id: ProjectionId,
        camera: Camera,
        viewport: Viewport,
        lod: LodPolicy,
        target: ProjectionTarget,
        caps: CapsToken,
    ) -> Self {
        Self {
            id,
            camera,
            viewport,
            lod,
            target,
            caps,
        }
    }

    /// Update the contained camera's aspect ratio to match the viewport.
    /// Call after a viewport-resize event so projection matrices stay
    /// consistent with the render-target rect.
    pub fn sync_aspect(&mut self) {
        if self.viewport.is_valid() {
            self.camera.aspect = self.viewport.aspect();
        }
    }

    /// Select a LoD level for an object at the given world-space distance
    /// from the camera. Pure delegation to [`select_lod`] using this
    /// observer-frame's policy.
    #[must_use]
    pub fn pick_lod(&self, distance: f32) -> usize {
        select_lod(distance, &self.lod.thresholds)
    }

    /// Convenience : update the viewport rect and re-sync the camera's
    /// aspect. The canonical path for "the OS told me the window resized".
    pub fn resize(&mut self, width: u32, height: u32) {
        self.viewport.width = width;
        self.viewport.height = height;
        self.sync_aspect();
    }
}

#[cfg(test)]
mod tests {
    use super::{
        select_lod, LodError, LodPolicy, ObserverFrame, ProjectionId, ProjectionTarget, Viewport,
    };
    use crate::camera::Camera;
    use crate::caps::CapsToken;

    fn approx_eq(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() <= eps
    }

    #[test]
    fn viewport_aspect_basic() {
        let v = Viewport::new(0, 0, 1920, 1080);
        assert!(approx_eq(v.aspect(), 16.0 / 9.0, 1e-5));
        assert!(v.is_valid());
    }

    #[test]
    fn viewport_zero_height_returns_unit_aspect() {
        // Substrate totality : zero-height viewport must not divide-by-zero.
        let v = Viewport::new(0, 0, 100, 0);
        assert_eq!(v.aspect(), 1.0);
        assert!(!v.is_valid());
    }

    #[test]
    fn select_lod_below_first_threshold_returns_zero() {
        let thresholds = [10.0, 50.0, 200.0];
        assert_eq!(select_lod(5.0, &thresholds), 0);
        assert_eq!(select_lod(10.0, &thresholds), 0);
    }

    #[test]
    fn select_lod_in_middle_returns_correct_index() {
        let thresholds = [10.0, 50.0, 200.0];
        assert_eq!(select_lod(15.0, &thresholds), 1);
        assert_eq!(select_lod(50.0, &thresholds), 1);
        assert_eq!(select_lod(150.0, &thresholds), 2);
    }

    #[test]
    fn select_lod_beyond_last_returns_levels_count() {
        let thresholds = [10.0, 50.0, 200.0];
        assert_eq!(select_lod(500.0, &thresholds), 3);
    }

    #[test]
    fn select_lod_empty_returns_zero() {
        let thresholds: [f32; 0] = [];
        assert_eq!(select_lod(0.0, &thresholds), 0);
        assert_eq!(select_lod(1000.0, &thresholds), 0);
    }

    #[test]
    fn select_lod_nan_distance_treated_as_zero() {
        let thresholds = [10.0, 50.0];
        assert_eq!(select_lod(f32::NAN, &thresholds), 0);
    }

    #[test]
    fn lod_policy_validates_monotone() {
        let p = LodPolicy::from_thresholds(&[10.0, 50.0, 200.0]);
        p.validate().expect("monotone policy");
    }

    #[test]
    fn lod_policy_rejects_non_monotone() {
        let p = LodPolicy::from_thresholds(&[10.0, 5.0]);
        assert!(matches!(p.validate(), Err(LodError::NonMonotonic { .. })));
    }

    #[test]
    fn lod_policy_rejects_negative() {
        let p = LodPolicy::from_thresholds(&[-1.0]);
        assert!(matches!(
            p.validate(),
            Err(LodError::NegativeThreshold { .. })
        ));
    }

    #[test]
    fn lod_policy_rejects_nan() {
        let p = LodPolicy::from_thresholds(&[f32::NAN]);
        assert!(matches!(
            p.validate(),
            Err(LodError::NonFiniteThreshold { .. })
        ));
    }

    #[test]
    fn lod_policy_level_count_matches_thresholds_plus_one() {
        assert_eq!(LodPolicy::SINGLE_LEVEL.level_count(), 1);
        assert_eq!(LodPolicy::from_thresholds(&[10.0]).level_count(), 2);
        assert_eq!(LodPolicy::from_thresholds(&[10.0, 50.0]).level_count(), 3);
    }

    #[test]
    fn observer_frame_resize_updates_aspect() {
        let mut frame = ObserverFrame::new(
            ProjectionId(1),
            Camera::DEFAULT,
            Viewport::new(0, 0, 800, 600),
            LodPolicy::SINGLE_LEVEL,
            ProjectionTarget::Window,
            CapsToken::EMPTY,
        );
        frame.resize(1920, 1080);
        assert_eq!(frame.viewport.width, 1920);
        assert_eq!(frame.viewport.height, 1080);
        assert!(approx_eq(frame.camera.aspect, 16.0 / 9.0, 1e-5));
    }

    #[test]
    fn observer_frame_pick_lod_uses_policy() {
        let lod = LodPolicy::from_thresholds(&[5.0, 25.0]);
        let frame = ObserverFrame::new(
            ProjectionId(0),
            Camera::DEFAULT,
            Viewport::new(0, 0, 1920, 1080),
            lod,
            ProjectionTarget::Window,
            CapsToken::EMPTY,
        );
        assert_eq!(frame.pick_lod(2.0), 0);
        assert_eq!(frame.pick_lod(10.0), 1);
        assert_eq!(frame.pick_lod(50.0), 2);
    }

    #[test]
    fn projection_target_default_is_null() {
        // Cull-only is the safe default — no rendering, no consent escalation.
        assert_eq!(ProjectionTarget::default(), ProjectionTarget::Null);
    }

    #[test]
    fn multi_observer_layout_works() {
        // Split-screen with two observers : left + right halves of a 1920x1080.
        let cam = Camera::DEFAULT;
        let mut left = ObserverFrame::new(
            ProjectionId(1),
            cam,
            Viewport::new(0, 0, 960, 1080),
            LodPolicy::SINGLE_LEVEL,
            ProjectionTarget::Window,
            CapsToken::EMPTY,
        );
        let mut right = ObserverFrame::new(
            ProjectionId(2),
            cam,
            Viewport::new(960, 0, 960, 1080),
            LodPolicy::SINGLE_LEVEL,
            ProjectionTarget::Window,
            CapsToken::EMPTY,
        );
        left.sync_aspect();
        right.sync_aspect();
        // Both halves have identical aspect = 960/1080.
        let want = 960.0_f32 / 1080.0_f32;
        assert!(approx_eq(left.camera.aspect, want, 1e-5));
        assert!(approx_eq(right.camera.aspect, want, 1e-5));
        // IDs are distinct ; viewports don't overlap.
        assert_ne!(left.id, right.id);
        assert_eq!(left.viewport.x + left.viewport.width, right.viewport.x);
    }
}
