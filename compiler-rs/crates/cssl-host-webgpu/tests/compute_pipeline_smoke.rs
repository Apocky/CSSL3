//! End-to-end smoke test : real wgpu compute pipeline runs on the host.
//!
//! § §§ 14_BACKEND § HOST-SUBMIT BACKENDS § per-backend adapter-layer.
//!
//! § DESIGN
//!   This integration test runs only when the `wgpu-runtime` feature is
//!   enabled. It requests a wgpu adapter ; if no adapter is available
//!   (no GPU, headless CI, etc.), the test reports a permissive-skip
//!   rather than failing — the same convention as `hello_world_gate`
//!   (T11-D56) for hosts without a working linker.
//!
//! § COVERAGE
//!   - WebGpuInstance::new + request_adapter
//!   - WebGpuDevice::from_adapter
//!   - WebGpuBuffer::allocate_initialized + WebGpuBuffer::allocate
//!   - WebGpuComputePipeline::create with a real WGSL kernel
//!   - WebGpuCommandEncoder::dispatch_compute + copy_buffer_to_buffer
//!   - submit_and_block + read_buffer_sync
//!
//! § INVARIANT
//!   The add-42 kernel is the canary : if `out[i] = in[i] + 42` doesn't
//!   produce 142 from input 100, the host pipeline is broken.

#![cfg(feature = "wgpu-runtime")]

use cssl_host_webgpu::{
    submit_and_block, BackendHint, WebGpuBuffer, WebGpuBufferConfig, WebGpuCommandEncoder,
    WebGpuComputePipeline, WebGpuComputePipelineConfig, WebGpuDevice, WebGpuDeviceConfig,
    WebGpuInstance, WebGpuInstanceConfig,
};

/// Try to acquire a wgpu device on the host. Returns `None` (with a printed
/// diagnostic) if no compatible adapter is found. This permissive-skip
/// pattern lets the test pass on CI runners without a GPU.
fn try_acquire_device() -> Option<WebGpuDevice> {
    let cfg = WebGpuInstanceConfig {
        backends: BackendHint::Default,
        power_pref: wgpu::PowerPreference::HighPerformance,
        force_fallback: false,
    };
    let inst = WebGpuInstance::new(cfg);
    let adapter = match inst.request_adapter_sync() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("[S6-E4 smoke] permissive-skip : no adapter ({e})");
            return None;
        }
    };
    let info = adapter.get_info();
    eprintln!(
        "[S6-E4 smoke] adapter : {} (vendor 0x{:04x}, device 0x{:04x}, backend {:?})",
        info.name, info.vendor, info.device, info.backend
    );
    let dev_cfg = WebGpuDeviceConfig::default();
    match WebGpuDevice::from_adapter(adapter, &dev_cfg) {
        Ok(d) => Some(d),
        Err(e) => {
            eprintln!("[S6-E4 smoke] permissive-skip : device-request failed ({e})");
            None
        }
    }
}

/// Run the add-42 compute kernel against an input buffer of 4 u32 values
/// and verify each output is `input + 42`.
#[test]
fn add_42_kernel_executes_on_real_gpu() {
    let Some(device) = try_acquire_device() else {
        return; // permissive-skip
    };

    eprintln!("[S6-E4 smoke] negotiated backend : {:?}", device.backend());

    // Input data : 4 × u32.
    let input_data: Vec<u32> = vec![100, 200, 300, 400];
    let byte_len = (input_data.len() * std::mem::size_of::<u32>()) as u64;
    let input_bytes: Vec<u8> = input_data.iter().flat_map(|x| x.to_le_bytes()).collect();

    let in_buf = WebGpuBuffer::allocate_initialized(
        &device,
        &WebGpuBufferConfig::storage(byte_len, "in_buf"),
        &input_bytes,
    )
    .expect("allocate input buffer");

    let out_buf =
        WebGpuBuffer::allocate(&device, &WebGpuBufferConfig::storage(byte_len, "out_buf"))
            .expect("allocate output buffer");

    let staging = WebGpuBuffer::allocate(
        &device,
        &WebGpuBufferConfig::staging_readback(byte_len, "staging"),
    )
    .expect("allocate staging buffer");

    let pipeline =
        WebGpuComputePipeline::create(&device, &WebGpuComputePipelineConfig::add_42_kernel())
            .expect("create compute pipeline");

    let mut encoder = WebGpuCommandEncoder::new(&device, "add-42-encoder");
    encoder
        .dispatch_compute(
            &device,
            &pipeline,
            &in_buf,
            &out_buf,
            input_data.len() as u32,
        )
        .expect("dispatch");
    encoder
        .copy_buffer_to_buffer(&out_buf, &staging, byte_len)
        .expect("copy out -> staging");
    let cmd = encoder.finish();

    submit_and_block(&device, cmd).expect("submit + block");

    let raw = cssl_host_webgpu::read_buffer_sync(&device, &staging).expect("readback");
    assert_eq!(raw.len(), byte_len as usize);
    let outs: Vec<u32> = raw
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();
    eprintln!("[S6-E4 smoke] kernel output : {outs:?}");
    assert_eq!(outs.len(), input_data.len());
    for (i, (&inp, &out)) in input_data.iter().zip(outs.iter()).enumerate() {
        assert_eq!(out, inp + 42, "mismatch at index {i}");
    }
    eprintln!("[S6-E4 smoke] PASS : add-42 kernel executed on real GPU.");
}

/// Verify the copy kernel : out[i] = in[i].
#[test]
fn copy_kernel_executes_on_real_gpu() {
    let Some(device) = try_acquire_device() else {
        return;
    };

    let input_data: Vec<u32> = vec![1, 2, 3, 4, 5, 6, 7, 8];
    let byte_len = (input_data.len() * std::mem::size_of::<u32>()) as u64;
    let input_bytes: Vec<u8> = input_data.iter().flat_map(|x| x.to_le_bytes()).collect();

    let in_buf = WebGpuBuffer::allocate_initialized(
        &device,
        &WebGpuBufferConfig::storage(byte_len, "in_buf"),
        &input_bytes,
    )
    .expect("allocate input");

    let out_buf =
        WebGpuBuffer::allocate(&device, &WebGpuBufferConfig::storage(byte_len, "out_buf"))
            .expect("allocate output");

    let staging = WebGpuBuffer::allocate(
        &device,
        &WebGpuBufferConfig::staging_readback(byte_len, "staging"),
    )
    .expect("allocate staging");

    let pipeline =
        WebGpuComputePipeline::create(&device, &WebGpuComputePipelineConfig::copy_kernel())
            .expect("create copy pipeline");

    let mut encoder = WebGpuCommandEncoder::new(&device, "copy-encoder");
    encoder
        .dispatch_compute(
            &device,
            &pipeline,
            &in_buf,
            &out_buf,
            input_data.len() as u32,
        )
        .expect("dispatch");
    encoder
        .copy_buffer_to_buffer(&out_buf, &staging, byte_len)
        .expect("copy");
    let cmd = encoder.finish();
    submit_and_block(&device, cmd).expect("submit");

    let raw = cssl_host_webgpu::read_buffer_sync(&device, &staging).expect("readback");
    let outs: Vec<u32> = raw
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();
    assert_eq!(outs, input_data, "copy kernel must round-trip exactly");
    eprintln!("[S6-E4 smoke] PASS : copy kernel round-trip exact.");
}

/// Verify pipeline-creation rejects malformed WGSL with a typed error.
#[test]
fn malformed_wgsl_returns_typed_error() {
    let Some(device) = try_acquire_device() else {
        return;
    };

    let bad_cfg = WebGpuComputePipelineConfig {
        label: Some("bad".into()),
        wgsl_source: "this is not WGSL at all".into(),
        entry_point: "main".into(),
    };

    let result = WebGpuComputePipeline::create(&device, &bad_cfg);
    match result {
        Ok(_) => panic!("expected pipeline-create to fail on malformed WGSL"),
        Err(e) => {
            eprintln!("[S6-E4 smoke] malformed WGSL rejected : {e}");
            // Either ShaderModule (parse) or ComputePipeline (link) is OK.
            let s = e.to_string();
            assert!(
                s.to_lowercase().contains("shader") || s.to_lowercase().contains("pipeline"),
                "want shader/pipeline tag in error : {s}"
            );
        }
    }
}

/// Verify backend-negotiation reports a sensible backend on the host.
#[test]
fn backend_negotiation_reports_known_backend() {
    let inst = WebGpuInstance::new_default();
    match inst.negotiate_backend() {
        Ok(b) => {
            eprintln!("[S6-E4 smoke] negotiated backend : {b:?}");
            assert!(matches!(
                b,
                wgpu::Backend::Vulkan
                    | wgpu::Backend::Dx12
                    | wgpu::Backend::Metal
                    | wgpu::Backend::Gl
                    | wgpu::Backend::BrowserWebGpu
            ));
        }
        Err(e) => {
            eprintln!("[S6-E4 smoke] permissive-skip : {e}");
        }
    }
}
