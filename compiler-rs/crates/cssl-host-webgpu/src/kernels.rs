//! Hand-written placeholder WGSL kernels.
//!
//! § §§ 07_CODEGEN § WGSL-PATH note : real CSSLv3-MIR → WGSL emission lands at
//!   S6-D4. Until that slice ships, the host-WebGPU backend exercises its
//!   pipeline-creation + dispatch + readback code-paths against these
//!   hand-written kernels. They cover the same op-shapes the MIR emitter will
//!   produce so the host integration tests stay valid post-D4.
//!
//! § INVARIANT
//!   These kernels are spec-current WGSL ; all `@group(N) @binding(M)` slots
//!   match what `WebGpuComputePipeline` declares for them. Any divergence
//!   surfaces as `WebGpuError::ComputePipeline` at create-time, which is the
//!   exact failure mode the integration tests guard against.

/// Trivial copy-kernel : reads one element from input, writes the same
/// element to output. Workgroup-size 1 keeps the smoke-test minimal.
///
/// ```wgsl
/// @group(0) @binding(0) var<storage, read>       in_buf  : array<u32>;
/// @group(0) @binding(1) var<storage, read_write> out_buf : array<u32>;
///
/// @compute @workgroup_size(1)
/// fn main(@builtin(global_invocation_id) gid : vec3<u32>) {
///   out_buf[gid.x] = in_buf[gid.x];
/// }
/// ```
pub const COPY_KERNEL_WGSL: &str = r"
@group(0) @binding(0) var<storage, read>       in_buf  : array<u32>;
@group(0) @binding(1) var<storage, read_write> out_buf : array<u32>;

@compute @workgroup_size(1)
fn main(@builtin(global_invocation_id) gid : vec3<u32>) {
  out_buf[gid.x] = in_buf[gid.x];
}
";

/// Add-by-constant kernel : `out[i] = in[i] + 42`.
/// Used to verify that the kernel's logic actually executes (vs. a no-op
/// false-positive on a buffer-readback test).
pub const ADD_42_KERNEL_WGSL: &str = r"
@group(0) @binding(0) var<storage, read>       in_buf  : array<u32>;
@group(0) @binding(1) var<storage, read_write> out_buf : array<u32>;

@compute @workgroup_size(1)
fn main(@builtin(global_invocation_id) gid : vec3<u32>) {
  out_buf[gid.x] = in_buf[gid.x] + 42u;
}
";

/// Trivial vertex / fragment pair for the render-pipeline smoke-test.
/// Renders a single full-screen triangle with vertex-color interpolation.
/// Output target = `Rgba8Unorm`.
pub const FULLSCREEN_TRI_WGSL: &str = r"
struct VsOut {
  @builtin(position) pos : vec4<f32>,
  @location(0)       col : vec3<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vi : u32) -> VsOut {
  // 3-vertex full-screen triangle (no vertex-buffer needed).
  var positions : array<vec2<f32>, 3> = array<vec2<f32>, 3>(
    vec2<f32>(-1.0, -1.0),
    vec2<f32>( 3.0, -1.0),
    vec2<f32>(-1.0,  3.0),
  );
  var colors : array<vec3<f32>, 3> = array<vec3<f32>, 3>(
    vec3<f32>(1.0, 0.0, 0.0),
    vec3<f32>(0.0, 1.0, 0.0),
    vec3<f32>(0.0, 0.0, 1.0),
  );
  var o : VsOut;
  o.pos = vec4<f32>(positions[vi], 0.0, 1.0);
  o.col = colors[vi];
  return o;
}

@fragment
fn fs_main(in : VsOut) -> @location(0) vec4<f32> {
  return vec4<f32>(in.col, 1.0);
}
";

#[cfg(test)]
mod tests {
    use super::{ADD_42_KERNEL_WGSL, COPY_KERNEL_WGSL, FULLSCREEN_TRI_WGSL};

    #[test]
    fn copy_kernel_is_non_empty() {
        assert!(COPY_KERNEL_WGSL.contains("@compute"));
        assert!(COPY_KERNEL_WGSL.contains("@workgroup_size(1)"));
        assert!(COPY_KERNEL_WGSL.contains("array<u32>"));
    }

    #[test]
    fn add_42_kernel_carries_constant() {
        assert!(ADD_42_KERNEL_WGSL.contains("42u"));
        assert!(ADD_42_KERNEL_WGSL.contains("@compute"));
    }

    #[test]
    fn fullscreen_tri_has_vertex_and_fragment_entries() {
        assert!(FULLSCREEN_TRI_WGSL.contains("@vertex"));
        assert!(FULLSCREEN_TRI_WGSL.contains("@fragment"));
        assert!(FULLSCREEN_TRI_WGSL.contains("vs_main"));
        assert!(FULLSCREEN_TRI_WGSL.contains("fs_main"));
    }

    #[test]
    fn kernels_use_correct_bind_group_layout() {
        // bind-group 0, binding 0 = storage read ; binding 1 = storage read_write.
        // This invariant is verified by the host code at pipeline-creation time.
        assert!(COPY_KERNEL_WGSL.contains("@group(0) @binding(0)"));
        assert!(COPY_KERNEL_WGSL.contains("@group(0) @binding(1)"));
        assert!(ADD_42_KERNEL_WGSL.contains("@group(0) @binding(0)"));
        assert!(ADD_42_KERNEL_WGSL.contains("@group(0) @binding(1)"));
    }
}
