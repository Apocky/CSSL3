//! § T11-D260 (W-H3) — FFI integration test : stereo swapchain
//! acquire / wait / release ring.
//!
//! Confirms the `MockSwapchain` produces a valid array-2 stereo
//! swapchain config + cycles through the ring index correctly.

use cssl_host_openxr::ffi::{
    MockSwapchain, SwapchainCreateInfo, SwapchainImageReleaseInfo, SwapchainImageWaitInfo,
    SwapchainUsageFlags, XrResult,
};

#[test]
fn quest_3s_canonical_color_swapchain_is_array_2() {
    let info = SwapchainCreateInfo::quest_3s_stereo_color(2064, 2208);
    assert!(info.is_stereo());
    assert_eq!(info.array_size, 2);
    assert_eq!(info.face_count, 1);
    assert_eq!(info.mip_count, 1);
    assert_eq!(info.sample_count, 1);
    assert!(info.usage_flags.contains(SwapchainUsageFlags::COLOR_ATTACHMENT));
}

#[test]
fn quest_3s_canonical_depth_swapchain_advertises_depth() {
    let info = SwapchainCreateInfo::quest_3s_stereo_depth(2064, 2208);
    assert!(info.is_stereo());
    assert!(info
        .usage_flags
        .contains(SwapchainUsageFlags::DEPTH_STENCIL_ATTACHMENT));
}

#[test]
fn acquire_wait_release_ring_progresses_three_then_wraps() {
    let info = SwapchainCreateInfo::quest_3s_stereo_color(2064, 2208);
    let mut sc = MockSwapchain::create(info).expect("create");
    assert_eq!(sc.image_count, 3);

    let i0 = sc.acquire().expect("acq0");
    let r = sc.wait(&SwapchainImageWaitInfo::with_timeout_ms(100));
    assert_eq!(r, XrResult::SUCCESS);
    let r = sc.release(&SwapchainImageReleaseInfo::empty());
    assert_eq!(r, XrResult::SUCCESS);

    let i1 = sc.acquire().expect("acq1");
    let _ = sc.release(&SwapchainImageReleaseInfo::empty());

    let i2 = sc.acquire().expect("acq2");
    let _ = sc.release(&SwapchainImageReleaseInfo::empty());

    let i3 = sc.acquire().expect("acq3");
    let _ = sc.release(&SwapchainImageReleaseInfo::empty());

    assert_eq!(i0, 0);
    assert_eq!(i1, 1);
    assert_eq!(i2, 2);
    assert_eq!(i3, 0);
    assert_eq!(sc.acquire_calls, 4);
    assert_eq!(sc.release_calls, 4);
}

#[test]
fn double_acquire_without_release_is_call_order_invalid() {
    let info = SwapchainCreateInfo::quest_3s_stereo_color(2064, 2208);
    let mut sc = MockSwapchain::create(info).expect("create");
    let _ = sc.acquire().expect("acq0");
    let r = sc.acquire();
    assert_eq!(r.unwrap_err(), XrResult::ERROR_CALL_ORDER_INVALID);
}

#[test]
fn release_without_acquire_is_call_order_invalid() {
    let info = SwapchainCreateInfo::quest_3s_stereo_color(2064, 2208);
    let mut sc = MockSwapchain::create(info).expect("create");
    let r = sc.release(&SwapchainImageReleaseInfo::empty());
    assert_eq!(r, XrResult::ERROR_CALL_ORDER_INVALID);
}

#[test]
fn zero_extent_swapchain_create_fails() {
    let info = SwapchainCreateInfo::quest_3s_stereo_color(0, 2208);
    let r = MockSwapchain::create(info);
    assert_eq!(r.unwrap_err(), XrResult::ERROR_VALIDATION_FAILURE);
}
