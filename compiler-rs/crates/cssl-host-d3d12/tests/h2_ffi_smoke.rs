//! § W-H2 (T11-D259) — integration tests for own-FFI surface.
//!
//! § COVERAGE PER GOAL-SPEC ≥ 4 :
//!   1. device-create-mock          (loader probe + Loader::fully_loaded)
//!   2. swapchain-create-mock        (SwapChain::mock + present + resize)
//!   3. cmd-record-mock              (CmdRecorder + submit_mock)
//!   4. pipeline-from-dxil-mock      (DxilBytecode + create_*_pipeline_mock)
//!
//! These are integration tests (cross-module, cross-public-API) ; the
//! per-module unit tests live alongside their module under `mod tests`.

use cssl_host_d3d12::{
    CmdQueueDesc, CmdRecorder, ComputePipelineDesc, D3d12Error, DxgiFormat, DxilBytecode,
    GraphicsPipelineDesc, Hwnd, Loader, PipelineKind, PresentMode, SwapChain, SwapChainConfig,
    SwapEffect, create_compute_pipeline_mock, create_graphics_pipeline_mock, submit_mock,
    synth_dxil_fixture,
};

// ─── Test 1 : device-create-mock ──────────────────────────────────────────

#[test]
fn h2_test_1_device_create_mock_loader_probe() {
    // The loader probe should either succeed (Windows w/ DLLs) or return
    // a LoaderMissing error (non-Windows or stripped target). It must
    // never panic.
    let r = Loader::probe();
    match r {
        Ok(loader) => {
            // On a real Windows host with the GPU stack present, at least
            // ONE of the four canonical entry-points should resolve.
            assert!(
                loader.d3d12_create_device.is_some()
                    || loader.create_dxgi_factory2.is_some()
                    || loader.d3d12_get_debug_interface.is_some()
                    || loader.d3d12_serialize_root_signature.is_some(),
                "Loader probe succeeded but no entry-point resolved — sentinel for stale DLLs"
            );
        }
        Err(e) => {
            assert!(
                e.is_loader_missing(),
                "Loader probe failed but the error wasn't LoaderMissing : {e:?}"
            );
        }
    }

    // Synthetic fully-loaded check : a fully-populated Loader { Some, Some,
    // Some, Some } must report fully_loaded=true.
    let synth = Loader {
        d3d12_create_device: Some(0xdead_0001),
        create_dxgi_factory2: Some(0xdead_0002),
        d3d12_get_debug_interface: Some(0xdead_0003),
        d3d12_serialize_root_signature: Some(0xdead_0004),
    };
    assert!(synth.fully_loaded());
}

// ─── Test 2 : swapchain-create-mock ───────────────────────────────────────

#[test]
fn h2_test_2_swapchain_create_mock_full_lifecycle() {
    // Build a SwapChain in mock mode at 1080p triple-buffer + VSync.
    let cfg = SwapChainConfig::default_1080p();
    assert_eq!(cfg.format as u32, DxgiFormat::R8g8b8a8Unorm as u32);
    assert!(matches!(cfg.swap_effect, SwapEffect::FlipDiscard));
    assert!(matches!(cfg.present_mode, PresentMode::Vsync));

    let mut sc = SwapChain::mock(cfg).expect("mock swapchain create");
    assert!(sc.is_mock());
    assert_eq!(sc.extent(), (1920, 1080));
    assert_eq!(sc.buffer_count(), 3);
    assert_eq!(sc.frame_count(), 0);

    // Cycle through one full set of back buffers.
    for f in 0..sc.buffer_count() {
        assert_eq!(sc.current_back_buffer_index(), f);
        sc.present().unwrap();
    }
    assert_eq!(sc.current_back_buffer_index(), 0);
    assert_eq!(sc.frame_count(), u64::from(sc.buffer_count()));

    // Resize.
    sc.resize(2560, 1440).unwrap();
    assert_eq!(sc.extent(), (2560, 1440));

    // Real-FFI path with null HWND must reject.
    let synth_loader = Loader {
        d3d12_create_device: Some(1),
        create_dxgi_factory2: Some(1),
        d3d12_get_debug_interface: Some(1),
        d3d12_serialize_root_signature: Some(1),
    };
    let err = SwapChain::create_for_hwnd(
        &synth_loader,
        Hwnd::null(),
        SwapChainConfig::default_1080p(),
    )
    .unwrap_err();
    assert!(matches!(err, D3d12Error::InvalidArgument { .. }));
}

// ─── Test 3 : cmd-record-mock ─────────────────────────────────────────────

#[test]
fn h2_test_3_cmd_record_mock_three_queue_types() {
    // Direct queue : draw + barrier + dispatch all valid.
    let mut direct = CmdRecorder::new(CmdQueueDesc::direct());
    direct.record_set_pipeline_state(0).unwrap();
    direct.record_resource_barrier(0, 0x1, 0x4).unwrap();
    direct.record_draw_indexed(36, 1).unwrap();
    direct.record_dispatch(8, 8, 1).unwrap();
    let s = submit_mock(&mut direct, 1).expect("submit direct");
    assert_eq!(s.fence_value, 1);
    assert_eq!(s.op_count, 4);

    // Compute queue : dispatch + barrier valid ; draw rejected.
    let mut compute = CmdRecorder::new(CmdQueueDesc::compute());
    compute.record_dispatch(64, 1, 1).unwrap();
    compute.record_resource_barrier(0, 0x4, 0x80).unwrap();
    let draw_err = compute.record_draw_indexed(36, 1);
    assert!(matches!(draw_err, Err(D3d12Error::InvalidArgument { .. })));
    let s = submit_mock(&mut compute, 2).expect("submit compute");
    assert_eq!(s.fence_value, 2);
    assert_eq!(s.op_count, 2);

    // Copy queue : dispatch rejected ; copy + barrier OK.
    let mut copy = CmdRecorder::new(CmdQueueDesc::copy());
    let dispatch_err = copy.record_dispatch(1, 1, 1);
    assert!(matches!(dispatch_err, Err(D3d12Error::InvalidArgument { .. })));
    copy.record_resource_barrier(0, 0x1, 0x400).unwrap();
    copy.record(cssl_host_d3d12::CmdOp::CopyResource {
        dst_index: 0,
        src_index: 1,
    })
    .unwrap();
    let s = submit_mock(&mut copy, 3).expect("submit copy");
    assert_eq!(s.fence_value, 3);
    assert_eq!(s.op_count, 2);

    // Empty submit must reject.
    let mut empty = CmdRecorder::new(CmdQueueDesc::direct());
    assert!(submit_mock(&mut empty, 99).is_err());
}

// ─── Test 4 : pipeline-from-dxil-mock ─────────────────────────────────────

#[test]
fn h2_test_4_pipeline_from_dxil_mock_compute_and_graphics() {
    // Build a minimal-magic DXIL fixture and wrap it.
    let cs_blob = DxilBytecode::from_bytes(synth_dxil_fixture(256))
        .expect("compute DXIL must validate");
    let compute = ComputePipelineDesc {
        cs: cs_blob,
        root_signature_index: 0,
        node_mask: 0,
    };
    compute.validate(1).unwrap();
    let cpso = create_compute_pipeline_mock(&compute, 1, 7).unwrap();
    assert!(cpso.is_mock());
    assert!(matches!(cpso.kind(), PipelineKind::Compute));
    assert_eq!(cpso.mock_index(), 7);

    // Graphics : VS+PS magic-blobs, valid sample count.
    let vs_blob = DxilBytecode::from_bytes(synth_dxil_fixture(128)).unwrap();
    let ps_blob = DxilBytecode::from_bytes(synth_dxil_fixture(128)).unwrap();
    let gfx = GraphicsPipelineDesc {
        vs: vs_blob,
        ps: ps_blob,
        root_signature_index: 0,
        rtv_format: DxgiFormat::R8g8b8a8Unorm as u32,
        dsv_format: 0,
        sample_count: 4,
        node_mask: 0,
    };
    gfx.validate(1).unwrap();
    let gpso = create_graphics_pipeline_mock(&gfx, 1, 33).unwrap();
    assert!(gpso.is_mock());
    assert!(matches!(gpso.kind(), PipelineKind::Graphics));

    // Bad bytecode — no DXBC magic — must be rejected at wrap time.
    let bad = DxilBytecode::from_bytes(vec![b'M', b'Z', 0, 0]);
    assert!(matches!(bad, Err(D3d12Error::InvalidArgument { .. })));

    // Out-of-range root sig index must be rejected at descriptor validate.
    let cs_blob2 = DxilBytecode::from_bytes(synth_dxil_fixture(64)).unwrap();
    let bad_root = ComputePipelineDesc {
        cs: cs_blob2,
        root_signature_index: 99,
        node_mask: 0,
    };
    assert!(bad_root.validate(1).is_err());
}

// ─── Test 5 : cross-module wire-through (recorder × pipeline × swapchain) ─

#[test]
fn h2_test_5_render_loop_skeleton_mock() {
    // Build all three pieces : swap-chain, pipelines, recorder. Drive a
    // 4-frame "render loop" in mock mode.
    let mut sc = SwapChain::mock(SwapChainConfig::default_1080p()).unwrap();

    let cs_blob = DxilBytecode::from_bytes(synth_dxil_fixture(64)).unwrap();
    let compute_pso = create_compute_pipeline_mock(
        &ComputePipelineDesc {
            cs: cs_blob,
            root_signature_index: 0,
            node_mask: 0,
        },
        1,
        100,
    )
    .unwrap();

    let mut next_fence = 1_u64;
    for _frame in 0..4 {
        let mut rec = CmdRecorder::new(CmdQueueDesc::compute());
        rec.record_set_pipeline_state(compute_pso.mock_index()).unwrap();
        rec.record_dispatch(120, 68, 1).unwrap();
        let submission = submit_mock(&mut rec, next_fence).unwrap();
        assert_eq!(submission.fence_value, next_fence);
        next_fence += 1;
        sc.present().unwrap();
    }

    assert_eq!(sc.frame_count(), 4);
    // After 4 presents on a 3-buffer chain, idx is 1.
    assert_eq!(sc.current_back_buffer_index(), 1);
    // Fence-counter advanced as expected.
    assert_eq!(next_fence, 5);
}
