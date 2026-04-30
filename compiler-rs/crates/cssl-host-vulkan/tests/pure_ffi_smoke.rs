//! § T11-D258 (W-H1) : `pure_ffi` integration-smoke tests.
//!
//! § ROLE
//!   Cross-module exercises that the unit-tests can't easily verify in
//!   isolation. Every test uses [`MockLoader`] / [`StubLoader`] and never
//!   touches a real Vulkan loader — these tests are platform-independent
//!   and run under CI without a GPU.
//!
//! § TESTS
//!   1. `instance_create_mock`     — `InstanceBuilder` → `MockLoader` resolves `vkCreateInstance`.
//!   2. `device_enumerate_mock`    — `pick_queue_family` + `DeviceBuilder` → `MockLoader` resolves `vkCreateDevice`.
//!   3. `swapchain_acquire_mock`   — `SwapchainBuilder` + acquire-shape → `MockLoader` resolves `vkCreateSwapchainKHR` + `vkAcquireNextImageKHR`.
//!   4. `pipeline_compile_mock`    — `ComputePipelineCompile` → `MockLoader` resolves the 3-symbol pipeline-create chain.
//!   5. `cmd_record_submit_mock`   — `CommandRecorder` end-to-end with `MockLoader`.
//!   6. `null_handle_constants`    — sentinel-constant invariants across submodules.

#![allow(clippy::cast_possible_truncation)]

use cssl_host_vulkan::pure_ffi::{
    cmd::{CmdError, CommandRecorder, RecordState},
    device::{
        pick_queue_family, DeviceBuildError, DeviceBuilder, VkPhysicalDeviceType,
        VkQueueFamilyProperties, VkQueueFlag,
    },
    instance::{InstanceBuildError, InstanceBuilder, VK_API_VERSION_1_4},
    pipeline::{ComputePipelineCompile, PipelineCompileError, SPIRV_MAGIC},
    swapchain::{
        acquire_next_image_with_loader, queue_present_with_loader, SwapchainBuilder,
        SwapchainError, VkColorSpaceKHR, VkFormat, VkPresentModeKHR,
    },
    MockLoader, StubLoader, VulkanLoader, VK_NULL_HANDLE_NDISP,
};

fn fake_spirv() -> Vec<u8> {
    let mut v = Vec::with_capacity(20);
    v.extend_from_slice(&SPIRV_MAGIC.to_ne_bytes());
    v.extend_from_slice(&0x0001_0000_u32.to_ne_bytes());
    v.extend_from_slice(&0u32.to_ne_bytes());
    v.extend_from_slice(&1u32.to_ne_bytes());
    v.extend_from_slice(&0u32.to_ne_bytes());
    v
}

#[test]
fn instance_create_mock() {
    let l = MockLoader::new();
    let r = InstanceBuilder::new()
        .with_application_name("cssl-smoke")
        .with_engine_name("cssl-engine")
        .with_api_version(VK_API_VERSION_1_4)
        .with_layer("VK_LAYER_KHRONOS_validation")
        .with_extension("VK_KHR_surface")
        .with_extension("VK_EXT_debug_utils")
        .build_with_loader(&l);
    assert!(matches!(r, Err(InstanceBuildError::StubLoaderUnsupported)));
    assert_eq!(l.resolve_count(), 1);
    assert_eq!(l.resolved_names()[0], "vkCreateInstance");
}

#[test]
fn device_enumerate_mock() {
    // Pick a graphics+compute family from a synthetic property-list.
    let families = vec![
        VkQueueFamilyProperties {
            queue_flags: VkQueueFlag::Transfer as u32,
            queue_count: 1,
            ..Default::default()
        },
        VkQueueFamilyProperties {
            queue_flags: (VkQueueFlag::Graphics as u32) | (VkQueueFlag::Compute as u32),
            queue_count: 4,
            ..Default::default()
        },
    ];
    let pick = pick_queue_family(&families, VkQueueFlag::Graphics as u32).expect("pick");
    assert_eq!(pick.index, 1);

    // Now try device-build via mock loader.
    let l = MockLoader::new();
    let r = DeviceBuilder::new()
        .with_queue_family(pick.index)
        .with_queue_priorities(vec![1.0])
        .with_extension("VK_KHR_swapchain")
        .build_with_loader(&l);
    assert!(matches!(r, Err(DeviceBuildError::StubLoaderUnsupported)));
    assert_eq!(l.resolve_count(), 1);
    assert_eq!(l.resolved_names()[0], "vkCreateDevice");

    // PhysicalDeviceType round-trip.
    assert_eq!(
        VkPhysicalDeviceType::from_raw(2),
        VkPhysicalDeviceType::DiscreteGpu
    );
    assert_eq!(
        VkPhysicalDeviceType::from_raw(99),
        VkPhysicalDeviceType::Other
    );
}

#[test]
fn swapchain_acquire_mock() {
    let l = MockLoader::new();
    let surface = 0xDEAD_BEEF_u64;
    // Build the swapchain-create-info shape.
    let r = SwapchainBuilder::new()
        .with_surface(surface)
        .with_extent(1280, 720)
        .with_format(VkFormat::B8g8r8a8Srgb)
        .with_color_space(VkColorSpaceKHR::SrgbNonlinear)
        .with_present_mode(VkPresentModeKHR::Mailbox)
        .with_min_image_count(3)
        .create_with_loader(&l);
    assert!(matches!(r, Err(SwapchainError::StubLoaderUnsupported)));

    // Acquire-call-shape : non-zero timeout + mock loader → stub-unsupported.
    let acq = acquire_next_image_with_loader(&l, VK_NULL_HANDLE_NDISP, 1_000_000);
    assert!(matches!(acq, Err(SwapchainError::StubLoaderUnsupported)));

    // Present-call-shape : mock loader → stub-unsupported.
    let prs = queue_present_with_loader(&l, core::ptr::null_mut(), VK_NULL_HANDLE_NDISP, 0);
    assert!(matches!(prs, Err(SwapchainError::StubLoaderUnsupported)));

    // Three resolves : create + acquire + present (in order).
    assert_eq!(l.resolve_count(), 3);
    let names = l.resolved_names();
    assert_eq!(names[0], "vkCreateSwapchainKHR");
    assert_eq!(names[1], "vkAcquireNextImageKHR");
    assert_eq!(names[2], "vkQueuePresentKHR");
}

#[test]
fn pipeline_compile_mock() {
    let l = MockLoader::new();
    let c = ComputePipelineCompile::new(fake_spirv(), VK_NULL_HANDLE_NDISP)
        .with_entry_point("main");
    let r = c.compile_with_loader(&l);
    assert!(matches!(r, Err(PipelineCompileError::StubLoaderUnsupported)));
    assert_eq!(l.resolve_count(), 3);
    let names = l.resolved_names();
    assert_eq!(names[0], "vkCreateShaderModule");
    assert_eq!(names[1], "vkCreatePipelineLayout");
    assert_eq!(names[2], "vkCreateComputePipelines");
}

#[test]
fn cmd_record_submit_mock() {
    let mut r = CommandRecorder::new();
    assert_eq!(r.state(), RecordState::Idle);
    r.begin().expect("begin");
    r.cmd_bind_compute_pipeline(0xCAFE_BABE).expect("bind");
    r.cmd_dispatch(64, 1, 1);
    r.cmd_dispatch(128, 1, 1);
    r.cmd_dispatch(256, 1, 1);
    r.end();
    assert_eq!(r.state(), RecordState::Recorded);
    assert_eq!(r.dispatch_count(), 3);
    assert_eq!(r.bound_pipeline(), 0xCAFE_BABE);

    let l = MockLoader::new();
    let res = r.submit_with_loader(&l);
    assert!(matches!(res, Err(CmdError::StubLoaderUnsupported)));
    assert_eq!(r.state(), RecordState::Submitted);
    assert_eq!(l.resolve_count(), 1);
    assert_eq!(l.resolved_names()[0], "vkQueueSubmit");
}

#[test]
fn null_handle_constants() {
    // VK_NULL_HANDLE for non-dispatchable handles is 0 across submodules.
    assert_eq!(VK_NULL_HANDLE_NDISP, 0);
    // SPIR-V magic constant matches Khronos spec.
    assert_eq!(SPIRV_MAGIC, 0x0723_0203);
    // Stub loader is not real ; mock loader is also not real (both are
    // test-only ; a real Vulkan loader hasn't been wired yet).
    let s = StubLoader;
    let m = MockLoader::new();
    assert!(!s.is_real());
    assert!(!m.is_real());
}

#[test]
fn loader_resolve_chain_with_stub_short_circuits() {
    // The StubLoader returns None for every resolve, so the very first
    // symbol-lookup short-circuits with LoaderMissingSymbol.
    let l = StubLoader;
    let r = InstanceBuilder::new().build_with_loader(&l);
    assert!(matches!(r, Err(InstanceBuildError::LoaderMissingSymbol(ref n)) if n == "vkCreateInstance"));

    let r = DeviceBuilder::new().build_with_loader(&l);
    assert!(matches!(r, Err(DeviceBuildError::LoaderMissingSymbol(ref n)) if n == "vkCreateDevice"));

    let r = SwapchainBuilder::new()
        .with_surface(0x1234)
        .create_with_loader(&l);
    assert!(matches!(r, Err(SwapchainError::LoaderMissingSymbol(ref n)) if n == "vkCreateSwapchainKHR"));

    let c = ComputePipelineCompile::new(fake_spirv(), VK_NULL_HANDLE_NDISP);
    let r = c.compile_with_loader(&l);
    assert!(matches!(r, Err(PipelineCompileError::LoaderMissingSymbol(ref n)) if n == "vkCreateShaderModule"));
}
