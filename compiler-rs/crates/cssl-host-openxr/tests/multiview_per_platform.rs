//! Multiview per-platform shader-emit verification.
//!
//! § SPEC : `07_AESTHETIC/05_VR_RENDERING.csl` § VII.

use cssl_host_openxr::{
    multiview::{MultiviewConfig, MultiviewMode},
    view::ViewSet,
};

#[test]
fn vulkan_multiview_canonical_for_quest3() {
    // Quest 3 uses VK_KHR_multiview ⊗ shader emits gl_ViewIndex.
    let vs = ViewSet::stereo_identity(64.0);
    let cfg = MultiviewConfig::recommended(MultiviewMode::VulkanMultiview, &vs).unwrap();
    assert_eq!(cfg.mode, MultiviewMode::VulkanMultiview);
    assert_eq!(cfg.view_count, 2);
    assert_eq!(cfg.vulkan_view_mask, 0b11);
}

#[test]
fn d3d12_view_instancing_canonical_for_pimax() {
    // Pimax uses D3D12 view-instancing ⊗ shader emits SV_ViewID.
    let vs = ViewSet::quad_view_foveated(64.0);
    let cfg = MultiviewConfig::recommended(MultiviewMode::D3D12ViewInstancing, &vs).unwrap();
    assert_eq!(cfg.mode, MultiviewMode::D3D12ViewInstancing);
    assert_eq!(cfg.d3d12_view_instance_count, 4);
}

#[test]
fn metal_vertex_amplification_canonical_for_visionos() {
    // visionOS uses Metal vertex-amplification ⊗ shader emits [[amplification_id]].
    let vs = ViewSet::stereo_identity(64.0);
    let cfg = MultiviewConfig::recommended(MultiviewMode::MetalVertexAmplification, &vs).unwrap();
    assert_eq!(cfg.mode, MultiviewMode::MetalVertexAmplification);
    assert_eq!(cfg.metal_amplification_count, 2);
}

#[test]
fn webgpu_emulation_for_flat_web_xr_target() {
    // WebGPU multiview not yet shipping native @ 2026-04 ; fallback emulation.
    let vs = ViewSet::stereo_identity(64.0);
    let cfg = MultiviewConfig::recommended(MultiviewMode::WebGpuEmulation, &vs).unwrap();
    assert_eq!(cfg.mode, MultiviewMode::WebGpuEmulation);
    assert!(!cfg.mode.is_single_draw());
}

#[test]
fn serial_per_eye_for_flat_or_legacy() {
    let vs = ViewSet::flat_monitor();
    let cfg = MultiviewConfig::recommended(MultiviewMode::SerialPerEye, &vs).unwrap();
    assert_eq!(cfg.view_count, 1);
}

#[test]
fn vulkan_view_mask_for_view_count_8_light_field() {
    // 5-yr light-field viewCount = 8.
    let cfg = MultiviewConfig::vulkan(8).unwrap();
    assert_eq!(cfg.vulkan_view_mask, 0xFF);
}

#[test]
fn vulkan_view_mask_for_view_count_16_max_light_field() {
    let cfg = MultiviewConfig::vulkan(16).unwrap();
    assert_eq!(cfg.vulkan_view_mask, 0xFFFF);
}

#[test]
fn all_single_draw_modes_classified() {
    assert!(MultiviewMode::VulkanMultiview.is_single_draw());
    assert!(MultiviewMode::D3D12ViewInstancing.is_single_draw());
    assert!(MultiviewMode::MetalVertexAmplification.is_single_draw());
    // Emulation + serial are not.
    assert!(!MultiviewMode::WebGpuEmulation.is_single_draw());
    assert!(!MultiviewMode::SerialPerEye.is_single_draw());
}

#[test]
fn multiview_default_per_platform() {
    // Vulkan default
    assert!(MultiviewMode::VulkanMultiview.is_default_for_vulkan());
    assert!(!MultiviewMode::D3D12ViewInstancing.is_default_for_vulkan());
    // D3D12 default
    assert!(MultiviewMode::D3D12ViewInstancing.is_default_for_d3d12());
    // visionOS default
    assert!(MultiviewMode::MetalVertexAmplification.is_default_for_visionos());
}
