//! Canonical OpenXR frame-loop.
//!
//! § SPEC : `07_AESTHETIC/05_VR_RENDERING.csl` § IV.A.
//!
//! ```text
//! fn vr_frame() / { GPU, Realtime<90Hz>, Deadline<11.111ms>, Pure } {
//!     1. xrWaitFrame
//!     2. xrBeginFrame
//!     3. xrLocateViews
//!     4. (optional) gaze-poll
//!     5. (optional) AppSW ½-rate decision
//!     6. assemble-SDF (once per-frame, shared-across-eyes)
//!     7. cascade-build (once per-frame, shared-across-eyes)
//!     8. per-eye march + shade (multiview-amplified)
//!     9. spectral-tonemap per-eye
//!    10. xrEndFrame (composition layers : color + depth + passthrough)
//! }
//! ```
//!
//! § STAGE-0 SCOPE
//!   This module ships the **engine-side abstract loop** : `FrameLoop`
//!   walks the steps + records timestamps + produces the composition-
//!   layer-stack to submit. Steps 6/7/8/9 are **delegated** to
//!   `cssl-render` ; the FFI follow-up slice wires the actual
//!   `xrWaitFrame` / `xrBeginFrame` / `xrLocateViews` / `xrEndFrame`
//!   calls.

use crate::comfort::{JudderDetector, QualityLevel};
use crate::composition::CompositionLayerStack;
use crate::error::XRFailure;
use crate::foveation::{FoveationConfig, Foveator, GazePrediction};
use crate::per_eye::PerEyeOutputArray;
use crate::session::{MockSession, XrSessionState};
use crate::space_warp::{AppSwMode, AppSwScheduler};
use crate::ifc_shim::LabeledValue;
use crate::view::ViewSet;

/// Frame-loop state. Tracks one frame-cycle.
#[derive(Debug)]
pub struct FrameLoop<'a> {
    /// Session reference.
    session: &'a mut MockSession,
    /// AppSW scheduler.
    appsw: &'a mut AppSwScheduler,
    /// Judder detector.
    judder: &'a mut JudderDetector,
    /// Foveator (FFR / DFR / ML).
    foveator: &'a mut dyn Foveator,
    /// Monotonic frame index.
    frame_index: u64,
    /// Predicted display-time-ns from xrWaitFrame.
    predicted_display_time_ns: u64,
    /// `true` iff the frame-loop is currently in the open frame
    /// (between begin_frame + end_frame).
    frame_open: bool,
    /// Topology to use when synthesizing the per-frame `ViewSet` in
    /// stage-0 (no real `xrLocateViews`). Defaults to `StereoPair`.
    topology: crate::view::ViewTopology,
}

/// Result of a single frame-loop iteration.
#[derive(Debug, Clone)]
pub struct FrameResult {
    /// `true` iff this frame was rendered (vs. AppSW-reprojected).
    pub rendered: bool,
    /// View-set used for the frame.
    pub view_set: ViewSet,
    /// Foveation-config applied this frame.
    pub foveation: FoveationConfig,
    /// AppSW mode in effect.
    pub appsw_mode: AppSwMode,
    /// Quality-level in effect.
    pub quality: QualityLevel,
    /// Frame-time-ns observed.
    pub frame_time_ns: u64,
    /// Frame-index that produced this result.
    pub frame_index: u64,
}

impl<'a> FrameLoop<'a> {
    /// New frame-loop driver. Defaults to `StereoPair` topology ;
    /// callers pin to flat / quad-view / light-field via [`Self::with_topology`].
    pub fn new(
        session: &'a mut MockSession,
        appsw: &'a mut AppSwScheduler,
        judder: &'a mut JudderDetector,
        foveator: &'a mut dyn Foveator,
    ) -> Self {
        Self {
            session,
            appsw,
            judder,
            foveator,
            frame_index: 0,
            predicted_display_time_ns: 0,
            frame_open: false,
            topology: crate::view::ViewTopology::StereoPair,
        }
    }

    /// Pin the topology used by `locate_views` (stage-0 stand-in for
    /// the runtime's per-frame topology decision).
    #[must_use]
    pub fn with_topology(mut self, topology: crate::view::ViewTopology) -> Self {
        self.topology = topology;
        self
    }

    /// Step 1 : `xrWaitFrame`. Returns the predicted display-time-ns.
    /// In stage-0 this is computed from the AppSW budget ; the FFI
    /// follow-up slice replaces with real `xrWaitFrame`.
    pub fn wait_frame(&mut self) -> Result<u64, XRFailure> {
        if !self.session.state().allows_render() {
            return Err(XRFailure::FrameWait { code: -200 });
        }
        // Predict 1 frame ahead.
        self.predicted_display_time_ns =
            self.predicted_display_time_ns.saturating_add(self.appsw.budget_ns);
        Ok(self.predicted_display_time_ns)
    }

    /// Step 2 : `xrBeginFrame`. Opens the frame for rendering.
    pub fn begin_frame(&mut self) -> Result<(), XRFailure> {
        if !self.session.state().allows_render() {
            return Err(XRFailure::FrameBoundary { code: -201 });
        }
        if self.frame_open {
            return Err(XRFailure::FrameBoundary { code: -202 });
        }
        self.frame_open = true;
        Ok(())
    }

    /// Step 3 : `xrLocateViews`. Returns a `ViewSet` populated with
    /// per-view poses. Stage-0 returns the canonical identity view-set
    /// for the configured topology, scaled by IPD.
    pub fn locate_views(&self, ipd_mm: f32) -> Result<ViewSet, XRFailure> {
        use crate::view::ViewTopology;
        let mut vs = match self.topology {
            ViewTopology::Flat => ViewSet::flat_monitor(),
            ViewTopology::StereoPair => ViewSet::stereo_identity(ipd_mm),
            ViewTopology::QuadViewFoveated => ViewSet::quad_view_foveated(ipd_mm),
            ViewTopology::LightFieldN => ViewSet::try_new(8, ipd_mm, 0)?, // 8-view light-field default
        };
        vs.ipd_mm = if vs.is_flat() { 64.0 } else { ipd_mm };
        vs.display_time_ns = self.predicted_display_time_ns;
        Ok(vs)
    }

    /// Step 4 : optional gaze-poll. Stage-0 returns identity gaze.
    /// In a real run this calls `xrLocateSpace` on the eye-gaze action ;
    /// the result is `LabeledValue<GazePrediction>` with `Sensitive<Gaze>`.
    pub fn locate_gaze(&self) -> Result<LabeledValue<GazePrediction>, XRFailure> {
        Ok(GazePrediction::identity().into_labeled())
    }

    /// Step 5 : AppSW decision. Returns `true` iff the engine must
    /// render this frame.
    pub fn appsw_should_render(&self) -> bool {
        self.appsw.should_render(self.frame_index)
    }

    /// Steps 6-9 : delegate-out. The caller (engine `cssl-render`) does
    /// SDF assembly + cascade-build + per-eye march + tonemap. This
    /// method just records the foveation-config that was applied.
    pub fn apply_foveation(
        &mut self,
        view_set: &ViewSet,
        gaze: Option<&LabeledValue<GazePrediction>>,
    ) -> FoveationConfig {
        self.foveator.config_for_frame(view_set, gaze)
    }

    /// Step 10 : `xrEndFrame` with composition-layers. Validates the
    /// stack + closes the frame.
    pub fn end_frame(&mut self, layers: &CompositionLayerStack) -> Result<(), XRFailure> {
        if !self.frame_open {
            return Err(XRFailure::FrameBoundary { code: -203 });
        }
        layers.validate()?;
        self.session.tick_frame()?;
        self.frame_open = false;
        self.frame_index = self.frame_index.wrapping_add(1);
        Ok(())
    }

    /// Record a frame-time observation for AppSW + judder feedback.
    pub fn record_frame_time(&mut self, frame_time_ns: u64) {
        self.appsw.record_frame_time(frame_time_ns);
        self.judder.record(frame_time_ns);
    }

    /// Drive a complete frame-cycle (steps 1-10 sequentially) using
    /// stage-0 placeholders for the render-side. Returns a `FrameResult`
    /// describing the iteration.
    pub fn drive_one_frame(
        &mut self,
        ipd_mm: f32,
        layers: &CompositionLayerStack,
        observed_frame_time_ns: u64,
    ) -> Result<FrameResult, XRFailure> {
        self.wait_frame()?;
        self.begin_frame()?;
        let view_set = self.locate_views(ipd_mm)?;
        let gaze = self.locate_gaze()?;
        let rendered = self.appsw_should_render();
        let foveation = self.apply_foveation(&view_set, Some(&gaze));
        let appsw_mode = self.appsw.mode();
        let quality = self.judder.quality();
        let frame_index = self.frame_index;
        self.end_frame(layers)?;
        self.record_frame_time(observed_frame_time_ns);
        Ok(FrameResult {
            rendered,
            view_set,
            foveation,
            appsw_mode,
            quality,
            frame_time_ns: observed_frame_time_ns,
            frame_index,
        })
    }

    /// Current frame index (for tests).
    #[must_use]
    pub const fn frame_index(&self) -> u64 {
        self.frame_index
    }

    /// Whether a frame is currently open.
    #[must_use]
    pub const fn frame_open(&self) -> bool {
        self.frame_open
    }

    /// The session this loop is driving.
    pub fn session_state(&self) -> XrSessionState {
        self.session.state()
    }

    /// Construct the engine-side `PerEyeOutputArray` placeholder for
    /// this frame's view-set + a target dimension.
    #[must_use]
    pub fn per_eye_placeholder(view_set: &ViewSet, width: u32, height: u32) -> PerEyeOutputArray {
        PerEyeOutputArray::placeholder_for(view_set, width, height)
    }
}

#[cfg(test)]
mod tests {
    use super::FrameLoop;
    use crate::comfort::JudderDetector;
    use crate::composition::{CompositionLayerStack, XrCompositionLayer};
    use crate::foveation::FFRFoveator;
    use crate::instance::MockInstance;
    use crate::session::{GraphicsBinding, MockSession};
    use crate::space_warp::AppSwScheduler;
    use crate::view::ViewSet;

    fn setup() -> (MockSession, AppSwScheduler, JudderDetector, FFRFoveator) {
        let inst = MockInstance::quest3_default().unwrap();
        let mut s = MockSession::create(&inst, GraphicsBinding::Vulkan).unwrap();
        s.run_to_focused();
        let appsw = AppSwScheduler::quest3_default();
        let judder = JudderDetector::quest3_default();
        let fov = FFRFoveator::default_high();
        (s, appsw, judder, fov)
    }

    #[test]
    fn frame_loop_open_and_close_one_frame() {
        let (mut s, mut appsw, mut judder, mut fov) = setup();
        let mut fl = FrameLoop::new(&mut s, &mut appsw, &mut judder, &mut fov);
        fl.wait_frame().unwrap();
        fl.begin_frame().unwrap();
        assert!(fl.frame_open());
        let vs = ViewSet::stereo_identity(64.0);
        let mut layers = CompositionLayerStack::empty();
        layers.push(XrCompositionLayer::projection(&vs, &[1, 2], None).unwrap());
        fl.end_frame(&layers).unwrap();
        assert!(!fl.frame_open());
    }

    #[test]
    fn frame_loop_double_begin_fails() {
        let (mut s, mut appsw, mut judder, mut fov) = setup();
        let mut fl = FrameLoop::new(&mut s, &mut appsw, &mut judder, &mut fov);
        fl.wait_frame().unwrap();
        fl.begin_frame().unwrap();
        assert!(fl.begin_frame().is_err());
    }

    #[test]
    fn frame_loop_end_without_begin_fails() {
        let (mut s, mut appsw, mut judder, mut fov) = setup();
        let mut fl = FrameLoop::new(&mut s, &mut appsw, &mut judder, &mut fov);
        let layers = CompositionLayerStack::empty();
        assert!(fl.end_frame(&layers).is_err());
    }

    #[test]
    fn frame_loop_locate_views_returns_stereo() {
        let (mut s, mut appsw, mut judder, mut fov) = setup();
        let fl = FrameLoop::new(&mut s, &mut appsw, &mut judder, &mut fov);
        let vs = fl.locate_views(64.0).unwrap();
        assert!(vs.is_stereo());
    }

    #[test]
    fn frame_loop_locate_gaze_returns_labeled() {
        let (mut s, mut appsw, mut judder, mut fov) = setup();
        let fl = FrameLoop::new(&mut s, &mut appsw, &mut judder, &mut fov);
        let g = fl.locate_gaze().unwrap();
        assert!(g.is_biometric());
    }

    #[test]
    fn drive_one_frame_increments_frame_index() {
        let (mut s, mut appsw, mut judder, mut fov) = setup();
        let mut fl = FrameLoop::new(&mut s, &mut appsw, &mut judder, &mut fov);
        let vs = ViewSet::stereo_identity(64.0);
        let mut layers = CompositionLayerStack::empty();
        layers.push(XrCompositionLayer::projection(&vs, &[1, 2], None).unwrap());
        let r = fl.drive_one_frame(64.0, &layers, 8_000_000).unwrap();
        assert_eq!(r.frame_index, 0);
        assert_eq!(fl.frame_index(), 1);
    }

    #[test]
    fn drive_many_frames_records_history() {
        let (mut s, mut appsw, mut judder, mut fov) = setup();
        let mut fl = FrameLoop::new(&mut s, &mut appsw, &mut judder, &mut fov);
        let vs = ViewSet::stereo_identity(64.0);
        let mut layers = CompositionLayerStack::empty();
        layers.push(XrCompositionLayer::projection(&vs, &[1, 2], None).unwrap());
        for i in 0..10 {
            let r = fl.drive_one_frame(64.0, &layers, 8_000_000).unwrap();
            assert_eq!(r.frame_index, i);
        }
        assert_eq!(fl.frame_index(), 10);
    }

    #[test]
    fn appsw_should_render_alternates_in_every_other_frame() {
        let (mut s, mut appsw, mut judder, mut fov) = setup();
        appsw.force_mode(crate::space_warp::AppSwMode::EveryOtherFrame);
        let mut fl = FrameLoop::new(&mut s, &mut appsw, &mut judder, &mut fov);
        let vs = ViewSet::stereo_identity(64.0);
        let mut layers = CompositionLayerStack::empty();
        layers.push(XrCompositionLayer::projection(&vs, &[1, 2], None).unwrap());
        let r0 = fl.drive_one_frame(64.0, &layers, 8_000_000).unwrap();
        let r1 = fl.drive_one_frame(64.0, &layers, 8_000_000).unwrap();
        let r2 = fl.drive_one_frame(64.0, &layers, 8_000_000).unwrap();
        assert!(r0.rendered);
        assert!(!r1.rendered);
        assert!(r2.rendered);
    }

    #[test]
    fn per_eye_placeholder_factory() {
        let vs = ViewSet::stereo_identity(64.0);
        let arr = FrameLoop::per_eye_placeholder(&vs, 1024, 1024);
        assert_eq!(arr.outputs.len(), 2);
    }

    #[test]
    fn frame_loop_in_idle_state_refuses() {
        let inst = MockInstance::quest3_default().unwrap();
        let mut s = MockSession::create(&inst, GraphicsBinding::Vulkan).unwrap();
        // Don't run_to_focused : session in Idle.
        let mut appsw = AppSwScheduler::quest3_default();
        let mut judder = JudderDetector::quest3_default();
        let mut fov = FFRFoveator::default_high();
        let mut fl = FrameLoop::new(&mut s, &mut appsw, &mut judder, &mut fov);
        assert!(fl.wait_frame().is_err());
        assert!(fl.begin_frame().is_err());
    }
}
