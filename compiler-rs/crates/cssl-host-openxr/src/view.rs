//! `ViewSet` primitive : the canonical render-graph root that every
//! render-pass takes per `07_AESTHETIC/05_VR_RENDERING.csl` § III.
//!
//! § SPEC § III :
//!   - viewCount ∈ {1, 2, 4, N} day-one ; N ≤ MAX_VIEWS = 16
//!   - flat-monitor = degenerate viewCount = 1
//!   - stereo = viewCount = 2
//!   - quad-view foveated = viewCount = 4 (XR_VARJO_quad_views)
//!   - light-field = viewCount = N (5-yr forward-compat hook)
//!
//! § DESIGN
//!   `ViewSet` carries `SmallVec<View, MAX_VIEWS>` : day-one stack-allocated
//!   for the typical 1/2/4 case ; spills to heap only at viewCount > 8.
//!   The `view_count: u32` is redundant with `views.len()` but kept for
//!   shader-uniform compatibility (see § III shader-uniform contract).
//!
//! § ZERO-COST INVARIANT
//!   `View` matrices are bare `[f32; 16]` arrays (column-major, std430-
//!   compatible). No nalgebra / glam dependency : the engine's PGA crate
//!   (`cssl-pga`) supplies `Motor → mat4` lowering ; this crate only
//!   stores the result.

use smallvec::SmallVec;

use crate::error::XRFailure;

/// Maximum view-count supported day-one. § XIV.B forward-compat hook :
/// shader-uniform-arrays bind to MAX_VIEWS = 16 today ; light-field
/// 8/12/16-view configs land via the same `ViewSet` surface.
pub const MAX_VIEWS: usize = 16;

/// Topology of the views in a `ViewSet`. § III enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ViewTopology {
    /// `viewCount = 1` — flat-monitor degenerate case.
    Flat,
    /// `viewCount = 2` — stereo-pair (left, right).
    StereoPair,
    /// `viewCount = 4` — quad-view foveated
    /// (left-context, right-context, left-focus, right-focus). § V.C.
    QuadViewFoveated,
    /// `viewCount = N` (N ∈ {6, 8, 10, 12, 16}) — light-field display.
    /// 5-yr forward-compat hook ; today no-op render-path.
    LightFieldN,
}

impl ViewTopology {
    /// Default-topology for a given view-count.
    #[must_use]
    pub const fn from_view_count(view_count: u32) -> Self {
        match view_count {
            1 => Self::Flat,
            2 => Self::StereoPair,
            4 => Self::QuadViewFoveated,
            _ => Self::LightFieldN,
        }
    }

    /// `true` iff this topology is one of the day-one rendered paths
    /// (Flat / StereoPair / QuadViewFoveated). LightFieldN is a hook
    /// today.
    #[must_use]
    pub const fn is_day_one(self) -> bool {
        matches!(self, Self::Flat | Self::StereoPair | Self::QuadViewFoveated)
    }

    /// Display-name (stable diagnostic + serialization).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Flat => "flat",
            Self::StereoPair => "stereo-pair",
            Self::QuadViewFoveated => "quad-view-foveated",
            Self::LightFieldN => "light-field-N",
        }
    }
}

/// Field-of-view for a single view, in radians, stored as the canonical
/// asymmetric `(left, right, up, down)` quadruple. § III.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fov {
    /// Left half-FOV (radians, negative).
    pub left: f32,
    /// Right half-FOV (radians, positive).
    pub right: f32,
    /// Up half-FOV (radians, positive).
    pub up: f32,
    /// Down half-FOV (radians, negative).
    pub down: f32,
}

impl Fov {
    /// Symmetric FOV from a single horizontal half-angle + vertical
    /// half-angle. Convenience for the flat-monitor / Flat-topology case.
    #[must_use]
    pub const fn symmetric(half_h: f32, half_v: f32) -> Self {
        Self {
            left: -half_h,
            right: half_h,
            up: half_v,
            down: -half_v,
        }
    }

    /// Total horizontal FOV in radians.
    #[must_use]
    pub fn horizontal(self) -> f32 {
        self.right - self.left
    }

    /// Total vertical FOV in radians.
    #[must_use]
    pub fn vertical(self) -> f32 {
        self.up - self.down
    }

    /// `true` iff this FOV is the symmetric case (left == -right, up == -down).
    #[must_use]
    pub fn is_symmetric(self) -> bool {
        (self.left + self.right).abs() < f32::EPSILON && (self.up + self.down).abs() < f32::EPSILON
    }

    /// Build the canonical Quest-3 per-eye FOV (canted-display, asymmetric).
    /// Source : Meta Quest 3 documented per-eye FOV ~106° H × ~96° V combined,
    /// per-eye ~96° H × ~96° V with ~10° canted toward nose. These are
    /// representative defaults ; real values come from `xrLocateViews`.
    #[must_use]
    pub fn quest3_left() -> Self {
        Self {
            left: -0.95, // ~54° to the left
            right: 0.78, // ~45° to the right (canted)
            up: 0.84,    // ~48° up
            down: -0.84, // ~48° down
        }
    }

    /// Quest-3 right-eye FOV (mirror of left).
    #[must_use]
    pub fn quest3_right() -> Self {
        let l = Self::quest3_left();
        Self {
            left: -l.right,
            right: -l.left,
            up: l.up,
            down: l.down,
        }
    }
}

/// A single view in a `ViewSet`. § III layout-std430.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct View {
    /// World → eye, column-major. Derived from `eye_pose` at locate-time.
    pub view_matrix: [f32; 16],
    /// Asymmetric perspective projection. Derived from `fov`.
    pub proj_matrix: [f32; 16],
    /// FOV (asymmetric) in radians.
    pub fov: Fov,
    /// Eye-position relative to `hmd_pose`, column-major (translation in
    /// `[12..15]`).
    pub eye_pose: [f32; 16],
    /// TAA jitter per-eye (independent across eyes for de-correlated
    /// sampling). § III.
    pub jitter_ndc: [f32; 2],
    /// Previous-frame view-matrix (for motion-vector + AppSW).
    pub prev_view_matrix: [f32; 16],
    /// Previous-frame proj-matrix.
    pub prev_proj_matrix: [f32; 16],
    /// Index into the parent `ViewSet.views` (0..view_count-1).
    pub view_index: u32,
}

impl View {
    /// Identity-view : view = identity, proj = symmetric 90°, no jitter.
    /// Used by the flat-monitor degenerate-case + tests.
    #[must_use]
    pub fn identity() -> Self {
        let id = identity_mat4();
        Self {
            view_matrix: id,
            proj_matrix: id,
            fov: Fov::symmetric(0.785, 0.785), // 45° each side
            eye_pose: id,
            jitter_ndc: [0.0, 0.0],
            prev_view_matrix: id,
            prev_proj_matrix: id,
            view_index: 0,
        }
    }
}

/// `ViewSet` primitive : flows-through every render-pass per § III.
#[derive(Debug, Clone, PartialEq)]
pub struct ViewSet {
    /// View-count ∈ 1..=MAX_VIEWS. `views.len() == view_count as usize`.
    pub view_count: u32,
    /// Per-view data. Length-equals `view_count`.
    pub views: SmallVec<[View; 8]>,
    /// Topology category.
    pub topology: ViewTopology,
    /// HMD pose at predicted-display-time, column-major.
    /// (PGA `Motor` → mat4 lowering happens in `cssl-pga` ; this crate
    /// stores the result.)
    pub hmd_pose: [f32; 16],
    /// Inter-pupillary distance in millimeters. Range 50.0..=80.0
    /// per § III bound.
    pub ipd_mm: f32,
    /// OpenXR predicted-display-time in nanoseconds.
    pub display_time_ns: u64,
}

impl ViewSet {
    /// Build the canonical flat-monitor degenerate `ViewSet`
    /// (viewCount = 1).
    #[must_use]
    pub fn flat_monitor() -> Self {
        let mut views = SmallVec::<[View; 8]>::new();
        views.push(View::identity());
        Self {
            view_count: 1,
            views,
            topology: ViewTopology::Flat,
            hmd_pose: identity_mat4(),
            ipd_mm: 64.0,
            display_time_ns: 0,
        }
    }

    /// Build the canonical stereo `ViewSet` (viewCount = 2). Both views
    /// at identity-pose ; tests + scaffold-callers fill in real poses.
    #[must_use]
    pub fn stereo_identity(ipd_mm: f32) -> Self {
        let mut views = SmallVec::<[View; 8]>::new();
        let mut left = View::identity();
        left.view_index = 0;
        let mut right = View::identity();
        right.view_index = 1;
        views.push(left);
        views.push(right);
        Self {
            view_count: 2,
            views,
            topology: ViewTopology::StereoPair,
            hmd_pose: identity_mat4(),
            ipd_mm,
            display_time_ns: 0,
        }
    }

    /// Build the canonical quad-view foveated `ViewSet` (viewCount = 4).
    /// Order : left-context, right-context, left-focus, right-focus.
    /// § V.C.
    #[must_use]
    pub fn quad_view_foveated(ipd_mm: f32) -> Self {
        let mut views = SmallVec::<[View; 8]>::new();
        for i in 0..4u32 {
            let mut v = View::identity();
            v.view_index = i;
            views.push(v);
        }
        Self {
            view_count: 4,
            views,
            topology: ViewTopology::QuadViewFoveated,
            hmd_pose: identity_mat4(),
            ipd_mm,
            display_time_ns: 0,
        }
    }

    /// Build a `ViewSet` from an arbitrary view-count. Returns
    /// `XRFailure::ViewCountOutOfRange` if `view_count == 0` or
    /// `view_count > MAX_VIEWS`. Returns `XRFailure::IpdOutOfRange` if
    /// `ipd_mm` is outside `50.0..=80.0`.
    pub fn try_new(view_count: u32, ipd_mm: f32, display_time_ns: u64) -> Result<Self, XRFailure> {
        if view_count == 0 || view_count as usize > MAX_VIEWS {
            return Err(XRFailure::ViewCountOutOfRange { got: view_count });
        }
        if !(50.0..=80.0).contains(&ipd_mm) {
            return Err(XRFailure::IpdOutOfRange { got: ipd_mm });
        }
        let mut views = SmallVec::<[View; 8]>::new();
        for i in 0..view_count {
            let mut v = View::identity();
            v.view_index = i;
            views.push(v);
        }
        let topology = ViewTopology::from_view_count(view_count);
        Ok(Self {
            view_count,
            views,
            topology,
            hmd_pose: identity_mat4(),
            ipd_mm,
            display_time_ns,
        })
    }

    /// `true` iff this is the flat-monitor degenerate case.
    #[must_use]
    pub fn is_flat(&self) -> bool {
        matches!(self.topology, ViewTopology::Flat) && self.view_count == 1
    }

    /// `true` iff this is the stereo-pair case.
    #[must_use]
    pub fn is_stereo(&self) -> bool {
        matches!(self.topology, ViewTopology::StereoPair) && self.view_count == 2
    }

    /// `true` iff this is the quad-view-foveated case.
    #[must_use]
    pub fn is_quad_view(&self) -> bool {
        matches!(self.topology, ViewTopology::QuadViewFoveated) && self.view_count == 4
    }

    /// `true` iff this is a light-field configuration (viewCount > 4).
    #[must_use]
    pub fn is_light_field(&self) -> bool {
        matches!(self.topology, ViewTopology::LightFieldN) && self.view_count > 4
    }

    /// Validate `view_count == views.len() as u32` + topology
    /// consistent with view_count + ipd_mm in-range.
    pub fn validate(&self) -> Result<(), XRFailure> {
        if self.view_count == 0 || self.view_count as usize > MAX_VIEWS {
            return Err(XRFailure::ViewCountOutOfRange {
                got: self.view_count,
            });
        }
        if self.views.len() != self.view_count as usize {
            return Err(XRFailure::ViewCountOutOfRange {
                got: self.view_count,
            });
        }
        if !(50.0..=80.0).contains(&self.ipd_mm) {
            return Err(XRFailure::IpdOutOfRange { got: self.ipd_mm });
        }
        Ok(())
    }
}

/// 4×4 identity in column-major std430 form.
#[must_use]
pub const fn identity_mat4() -> [f32; 16] {
    [
        1.0, 0.0, 0.0, 0.0, //
        0.0, 1.0, 0.0, 0.0, //
        0.0, 0.0, 1.0, 0.0, //
        0.0, 0.0, 0.0, 1.0, //
    ]
}

#[cfg(test)]
mod tests {
    use super::{Fov, View, ViewSet, ViewTopology, MAX_VIEWS};
    use crate::error::XRFailure;

    #[test]
    fn flat_monitor_is_view_count_1() {
        let v = ViewSet::flat_monitor();
        assert_eq!(v.view_count, 1);
        assert_eq!(v.views.len(), 1);
        assert!(v.is_flat());
        assert!(!v.is_stereo());
    }

    #[test]
    fn stereo_identity_is_view_count_2() {
        let v = ViewSet::stereo_identity(64.0);
        assert_eq!(v.view_count, 2);
        assert!(v.is_stereo());
        assert_eq!(v.views[0].view_index, 0);
        assert_eq!(v.views[1].view_index, 1);
    }

    #[test]
    fn quad_view_is_view_count_4() {
        let v = ViewSet::quad_view_foveated(64.0);
        assert_eq!(v.view_count, 4);
        assert!(v.is_quad_view());
        for (i, view) in v.views.iter().enumerate() {
            assert_eq!(view.view_index, i as u32);
        }
    }

    #[test]
    fn try_new_rejects_zero_views() {
        let err = ViewSet::try_new(0, 64.0, 0).unwrap_err();
        assert!(matches!(err, XRFailure::ViewCountOutOfRange { got: 0 }));
    }

    #[test]
    fn try_new_rejects_too_many_views() {
        let err = ViewSet::try_new((MAX_VIEWS + 1) as u32, 64.0, 0).unwrap_err();
        assert!(matches!(err, XRFailure::ViewCountOutOfRange { .. }));
    }

    #[test]
    fn try_new_rejects_ipd_out_of_range_low() {
        let err = ViewSet::try_new(2, 30.0, 0).unwrap_err();
        assert!(matches!(err, XRFailure::IpdOutOfRange { got } if got == 30.0));
    }

    #[test]
    fn try_new_rejects_ipd_out_of_range_high() {
        let err = ViewSet::try_new(2, 100.0, 0).unwrap_err();
        assert!(matches!(err, XRFailure::IpdOutOfRange { got } if got == 100.0));
    }

    #[test]
    fn try_new_accepts_max_views() {
        let v = ViewSet::try_new(MAX_VIEWS as u32, 64.0, 0).unwrap();
        assert_eq!(v.view_count, MAX_VIEWS as u32);
        assert!(v.is_light_field());
    }

    #[test]
    fn topology_from_view_count() {
        assert!(matches!(
            ViewTopology::from_view_count(1),
            ViewTopology::Flat
        ));
        assert!(matches!(
            ViewTopology::from_view_count(2),
            ViewTopology::StereoPair
        ));
        assert!(matches!(
            ViewTopology::from_view_count(4),
            ViewTopology::QuadViewFoveated
        ));
        assert!(matches!(
            ViewTopology::from_view_count(8),
            ViewTopology::LightFieldN
        ));
        assert!(matches!(
            ViewTopology::from_view_count(16),
            ViewTopology::LightFieldN
        ));
    }

    #[test]
    fn topology_is_day_one() {
        assert!(ViewTopology::Flat.is_day_one());
        assert!(ViewTopology::StereoPair.is_day_one());
        assert!(ViewTopology::QuadViewFoveated.is_day_one());
        assert!(!ViewTopology::LightFieldN.is_day_one());
    }

    #[test]
    fn fov_quest3_is_asymmetric() {
        let l = Fov::quest3_left();
        let r = Fov::quest3_right();
        assert!(!l.is_symmetric());
        assert!(!r.is_symmetric());
        // mirror-symmetry between left + right
        assert!((l.left + r.right).abs() < 1e-6);
        assert!((l.right + r.left).abs() < 1e-6);
    }

    #[test]
    fn fov_horizontal_vertical() {
        let f = Fov::symmetric(0.5, 0.7);
        assert!((f.horizontal() - 1.0).abs() < 1e-6);
        assert!((f.vertical() - 1.4).abs() < 1e-6);
    }

    #[test]
    fn validate_catches_view_count_mismatch() {
        let mut v = ViewSet::stereo_identity(64.0);
        v.view_count = 4; // mismatch with views.len() == 2
        assert!(v.validate().is_err());
    }

    #[test]
    fn view_identity_construction() {
        let v = View::identity();
        // identity diagonal
        assert!((v.view_matrix[0] - 1.0).abs() < 1e-6);
        assert!((v.view_matrix[5] - 1.0).abs() < 1e-6);
        assert!((v.view_matrix[10] - 1.0).abs() < 1e-6);
        assert!((v.view_matrix[15] - 1.0).abs() < 1e-6);
    }
}
