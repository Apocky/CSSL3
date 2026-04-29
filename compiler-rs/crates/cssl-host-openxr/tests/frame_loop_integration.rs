//! End-to-end frame-loop integration test : drives multiple frames
//! through the canonical xrWaitFrame → xrBeginFrame → xrLocateViews →
//! xrEndFrame state-machine.
//!
//! § SPEC : `07_AESTHETIC/05_VR_RENDERING.csl` § IV.A.

use cssl_host_openxr::{
    comfort::JudderDetector,
    composition::{CompositionLayerStack, XrCompositionLayer},
    foveation::FFRFoveator,
    instance::MockInstance,
    session::{GraphicsBinding, MockSession},
    space_warp::{AppSwMode, AppSwScheduler, HYSTERESIS_FRAMES},
    view::ViewSet,
    FrameLoop,
};

#[test]
fn quest3_full_lifecycle_render_100_frames() {
    let inst = MockInstance::quest3_default().unwrap();
    let mut s = MockSession::create(&inst, GraphicsBinding::Vulkan).unwrap();
    s.run_to_focused();
    let mut appsw = AppSwScheduler::quest3_default();
    let mut judder = JudderDetector::quest3_default();
    let mut fov = FFRFoveator::default_high();
    let mut fl = FrameLoop::new(&mut s, &mut appsw, &mut judder, &mut fov);

    let vs = ViewSet::stereo_identity(64.0);
    let mut layers = CompositionLayerStack::empty();
    layers.push(XrCompositionLayer::projection(&vs, &[1, 2], None).unwrap());

    for i in 0..100u64 {
        let r = fl.drive_one_frame(64.0, &layers, 8_000_000).unwrap();
        assert_eq!(r.frame_index, i);
        assert!(r.rendered);
    }
    assert_eq!(fl.frame_index(), 100);
}

#[test]
fn quest3_appsw_engages_after_budget_violations() {
    let inst = MockInstance::quest3_default().unwrap();
    let mut s = MockSession::create(&inst, GraphicsBinding::Vulkan).unwrap();
    s.run_to_focused();
    let mut appsw = AppSwScheduler::quest3_default();
    let mut judder = JudderDetector::quest3_default();
    let mut fov = FFRFoveator::default_high();
    let mut fl = FrameLoop::new(&mut s, &mut appsw, &mut judder, &mut fov);

    let vs = ViewSet::stereo_identity(64.0);
    let mut layers = CompositionLayerStack::empty();
    layers.push(XrCompositionLayer::projection(&vs, &[1, 2], None).unwrap());

    // Drive HYSTERESIS_FRAMES + 1 budget-violations.
    for _ in 0..(HYSTERESIS_FRAMES + 5) {
        // Frame-time well over 11.111 ms budget.
        fl.drive_one_frame(64.0, &layers, 20_000_000).unwrap();
    }
    // AppSW should have engaged by now.
    assert_eq!(appsw.mode(), AppSwMode::EveryOtherFrame);
}

#[test]
fn pimax_quad_view_lifecycle() {
    let inst = MockInstance::pimax_crystal_super_default().unwrap();
    let mut s = MockSession::create(&inst, GraphicsBinding::D3D12).unwrap();
    s.run_to_focused();
    let mut appsw = AppSwScheduler::pimax_default();
    let mut judder = JudderDetector::for_display_hz(90.0);
    let mut fov = FFRFoveator::default_high();
    let mut fl = FrameLoop::new(&mut s, &mut appsw, &mut judder, &mut fov)
        .with_topology(cssl_host_openxr::ViewTopology::QuadViewFoveated);

    let vs = ViewSet::quad_view_foveated(64.0);
    let mut layers = CompositionLayerStack::empty();
    layers.push(XrCompositionLayer::projection(&vs, &[1, 2, 3, 4], None).unwrap());

    for _ in 0..30 {
        let r = fl.drive_one_frame(64.0, &layers, 9_000_000).unwrap();
        assert_eq!(r.view_set.view_count, 4);
    }
}

#[test]
fn vision_pro_compositor_services_session_no_vulkan() {
    let inst = MockInstance::vision_pro_default().unwrap();
    // visionOS demands Compositor-Services binding.
    let s = MockSession::create(&inst, GraphicsBinding::CompositorServices);
    assert!(s.is_ok());
}

#[test]
fn flat_monitor_degenerate_view_count_one() {
    let inst = MockInstance::flat_monitor_default().unwrap();
    let mut s = MockSession::create(&inst, GraphicsBinding::Headless).unwrap();
    s.run_to_focused();
    let mut appsw = AppSwScheduler::for_display_hz(60.0);
    let mut judder = JudderDetector::for_display_hz(60.0);
    let mut fov = FFRFoveator::default_high();
    let mut fl = FrameLoop::new(&mut s, &mut appsw, &mut judder, &mut fov)
        .with_topology(cssl_host_openxr::ViewTopology::Flat);
    let vs = ViewSet::flat_monitor();
    let mut layers = CompositionLayerStack::empty();
    layers.push(XrCompositionLayer::projection(&vs, &[1], None).unwrap());
    let r = fl.drive_one_frame(64.0, &layers, 14_000_000).unwrap();
    assert!(r.view_set.is_flat());
    assert_eq!(r.view_set.view_count, 1);
}

#[test]
fn judder_recovers_quality_after_stable_frames() {
    let inst = MockInstance::quest3_default().unwrap();
    let mut s = MockSession::create(&inst, GraphicsBinding::Vulkan).unwrap();
    s.run_to_focused();
    let mut appsw = AppSwScheduler::quest3_default();
    let mut judder = JudderDetector::quest3_default();
    let mut fov = FFRFoveator::default_high();

    // Force a degraded state to start.
    judder.force_quality(cssl_host_openxr::QualityLevel::DegradeFoveation);
    {
        let mut fl = FrameLoop::new(&mut s, &mut appsw, &mut judder, &mut fov);
        let vs = ViewSet::stereo_identity(64.0);
        let mut layers = CompositionLayerStack::empty();
        layers.push(XrCompositionLayer::projection(&vs, &[1, 2], None).unwrap());
        for _ in 0..(cssl_host_openxr::STABLE_FRAMES_TO_RECOVER + 10) {
            fl.drive_one_frame(64.0, &layers, 4_000_000).unwrap(); // way under budget
        }
    }
    assert_eq!(judder.quality(), cssl_host_openxr::QualityLevel::Full);
}

#[test]
fn wait_frame_in_idle_session_fails() {
    let inst = MockInstance::quest3_default().unwrap();
    let mut s = MockSession::create(&inst, GraphicsBinding::Vulkan).unwrap();
    // Don't transition to focused.
    let mut appsw = AppSwScheduler::quest3_default();
    let mut judder = JudderDetector::quest3_default();
    let mut fov = FFRFoveator::default_high();
    let mut fl = FrameLoop::new(&mut s, &mut appsw, &mut judder, &mut fov);
    assert!(fl.wait_frame().is_err());
}
