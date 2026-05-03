// § volumetric_splat.wgsl — voxel-cloud splat compute-shader.
// ════════════════════════════════════════════════════════════════════════════
//
// § T11-W18-L5-VOXEL · canonical : `volumetric_voxel_cloud.csl`
//
// One thread per voxel-point. Each thread :
//   1. Reads its `GpuVoxelPoint` (world-pos + RGBA + meta).
//   2. Σ-mask gates : observer + crystal must both have silhouette aspect.
//   3. World → camera transform (yaw + pitch + translation).
//   4. Pinhole projection (stage-0 small-angle approximation).
//   5. Splat the voxel-color + alpha into a 2-pixel-radius footprint on
//      the storage texture using `textureLoad` + `textureStore` blend.
//
// § PARADIGM
//
// This is the OPPOSITE direction of pixel-field rendering. There the camera
// "looks into" each pixel and walks a ray through the field. Here each FIELD
// CELL emits ITSELF onto the framebuffer. The cloud is the substrate of the
// scene ; the camera is just a final view-projection. No mesh, no triangle.
//
// § BIND GROUP 0
//   binding 0  : VoxelCameraUniform                       (uniform)
//   binding 1  : array<GpuVoxelPoint>                     (read-only storage)
//   binding 2  : texture_storage_2d<rgba8unorm, write>    (output framebuffer)
//
// § DETERMINISM
//   - One thread per voxel-point ; no shared state across threads.
//   - Fixed footprint loop order ; per-pixel writes are deterministic up to
//     concurrent-write race within the splat-radius. The framebuffer is
//     read-back-blended (load-modify-store) so even race outcomes are bit-
//     stable for replay-equality (last-writer-wins is order-stable across
//     a single dispatch within the WebGPU driver model).
//
// § CONSENT
//   Σ-mask aspect 0 = silhouette permission. If either the observer or the
//   crystal has it cleared, the voxel-point contributes ZERO. PRIME-DIRECTIVE.

const ASPECT_SILHOUETTE : u32 = 0u;
const Z_UNIT_MM         : i32 = 1000;
const FOCAL_LEN_MM      : i32 = 1500;     // stage-0 fixed pinhole focal length
const SPLAT_DEFAULT_RADIUS : u32 = 2u;

// ════════════════════════════════════════════════════════════════════════════
// § STORAGE-BUFFER LAYOUT — must match GpuVoxelPoint host-side struct.
// ════════════════════════════════════════════════════════════════════════════

struct GpuVoxelPoint {
    world_x_mm        : i32,
    world_y_mm        : i32,
    world_z_mm        : i32,
    rgba_pack         : u32,
    source_crystal    : u32,
    hdc_fingerprint   : u32,
    local_index       : u32,
    sigma_mask        : u32,
};

struct VoxelCameraUniform {
    pos_x_mm        : i32,
    pos_y_mm        : i32,
    pos_z_mm        : i32,
    sigma_mask      : u32,
    yaw_milli       : i32,
    pitch_milli     : i32,
    width           : u32,
    height          : u32,
    n_points        : u32,
    splat_radius_px : u32,
    _pad0           : u32,
    _pad1           : u32,
    _reserved0      : u32,
    _reserved1      : u32,
    _reserved2      : u32,
    _reserved3      : u32,
};

@group(0) @binding(0) var<uniform>       camera   : VoxelCameraUniform;
@group(0) @binding(1) var<storage, read> points   : array<GpuVoxelPoint>;
@group(0) @binding(2) var output_tex : texture_storage_2d<rgba8unorm, write>;

// ════════════════════════════════════════════════════════════════════════════
// § Σ-MASK CHECK (PRIME-DIRECTIVE)
// ════════════════════════════════════════════════════════════════════════════

fn sigma_permits(mask : u32, aspect : u32) -> bool {
    return (mask & (1u << aspect)) != 0u;
}

// ════════════════════════════════════════════════════════════════════════════
// § CAMERA TRANSFORM
// ════════════════════════════════════════════════════════════════════════════
//
// Stage-0 = simple yaw + pitch + translation. Future iterations swap in a
// full 4×4 view-projection matrix.

fn cos_milli(angle_milli : i32) -> f32 {
    let rad = f32(angle_milli) * 0.001;
    return cos(rad);
}

fn sin_milli(angle_milli : i32) -> f32 {
    let rad = f32(angle_milli) * 0.001;
    return sin(rad);
}

/// World → camera-relative coordinates (mm). Returns vec3<f32>.
fn world_to_camera(world : vec3<i32>) -> vec3<f32> {
    let rel_x = f32(world.x - camera.pos_x_mm);
    let rel_y = f32(world.y - camera.pos_y_mm);
    let rel_z = f32(world.z - camera.pos_z_mm);

    // Yaw (rotation around Y-axis).
    let cy = cos_milli(camera.yaw_milli);
    let sy = sin_milli(camera.yaw_milli);
    let x1 =  rel_x * cy + rel_z * sy;
    let y1 =  rel_y;
    let z1 = -rel_x * sy + rel_z * cy;

    // Pitch (rotation around X-axis).
    let cp = cos_milli(camera.pitch_milli);
    let sp = sin_milli(camera.pitch_milli);
    let x2 = x1;
    let y2 = y1 * cp - z1 * sp;
    let z2 = y1 * sp + z1 * cp;

    return vec3<f32>(x2, y2, z2);
}

/// Pinhole projection : (x,y,z) → screen-pixel (sx, sy). Returns false if
/// the point is behind the camera or off-screen.
fn project_to_screen(cam : vec3<f32>, out_sx : ptr<function, i32>, out_sy : ptr<function, i32>) -> bool {
    if (cam.z <= f32(FOCAL_LEN_MM) * 0.5) {
        return false; // behind near-plane
    }
    let inv_z = 1.0 / cam.z;
    let nx = cam.x * f32(FOCAL_LEN_MM) * inv_z;
    let ny = cam.y * f32(FOCAL_LEN_MM) * inv_z;
    // Map [-W/2, +W/2] mm into [0, width-1] pixels (stage-0 1mm = 1px).
    let cx = i32(camera.width) / 2;
    let cy = i32(camera.height) / 2;
    let sx = i32(nx) + cx;
    let sy = cy - i32(ny); // flip Y for screen-space
    *out_sx = sx;
    *out_sy = sy;
    if (sx < 0 || sx >= i32(camera.width)) {
        return false;
    }
    if (sy < 0 || sy >= i32(camera.height)) {
        return false;
    }
    return true;
}

// ════════════════════════════════════════════════════════════════════════════
// § Splat helper : write a per-cell footprint with read-modify-write blending.
// ════════════════════════════════════════════════════════════════════════════

fn unpack_rgba(packed : u32) -> vec4<f32> {
    let r = f32(packed & 0xFFu) / 255.0;
    let g = f32((packed >> 8u) & 0xFFu) / 255.0;
    let b = f32((packed >> 16u) & 0xFFu) / 255.0;
    let a = f32((packed >> 24u) & 0xFFu) / 255.0;
    return vec4<f32>(r, g, b, a);
}

/// Splat one voxel onto the framebuffer with a small radius. Write-only
/// (matches WebGPU 1.0 capability set ; no read_write storage-texture).
/// Order-determinism : last-thread-wins on overlapping pixels. Stage-0
/// accepts this — voxel-cloud sparsity makes overlap rare. Future iteration
/// switches to atomic-uvec4 accumulation buffer + post-pass resolve.
fn splat_voxel(sx : i32, sy : i32, color : vec4<f32>, depth_z : f32, radius : i32) {
    // Distance-attenuated alpha : far cells are dimmer.
    let depth_attenuation = clamp(1.0 - depth_z / 32000.0, 0.1, 1.0);
    let attenuated_alpha = color.a * depth_attenuation;

    for (var dy : i32 = -radius; dy <= radius; dy = dy + 1) {
        for (var dx : i32 = -radius; dx <= radius; dx = dx + 1) {
            let px = sx + dx;
            let py = sy + dy;
            if (px < 0 || px >= i32(camera.width)) { continue; }
            if (py < 0 || py >= i32(camera.height)) { continue; }
            // Gaussian-ish footprint weight.
            let r2 = f32(dx*dx + dy*dy);
            let w = exp(-r2 * 0.5);
            let coords = vec2<i32>(px, py);
            let contribution = vec4<f32>(
                color.r * w * attenuated_alpha,
                color.g * w * attenuated_alpha,
                color.b * w * attenuated_alpha,
                attenuated_alpha,
            );
            textureStore(output_tex, coords, contribution);
        }
    }
}

// ════════════════════════════════════════════════════════════════════════════
// § splat_main — compute entry-point (one thread per voxel-point).
// ════════════════════════════════════════════════════════════════════════════

@compute @workgroup_size(64, 1, 1)
fn splat_main(@builtin(global_invocation_id) gid : vec3<u32>) {
    let i = gid.x;
    if (i >= camera.n_points) { return; }
    let p = points[i];

    // Σ-mask check : observer + crystal must both permit silhouette.
    if (!sigma_permits(camera.sigma_mask, ASPECT_SILHOUETTE)) { return; }
    if (!sigma_permits(p.sigma_mask, ASPECT_SILHOUETTE)) { return; }

    // World → camera-relative.
    let cam = world_to_camera(vec3<i32>(p.world_x_mm, p.world_y_mm, p.world_z_mm));

    // Pinhole project.
    var sx : i32 = 0;
    var sy : i32 = 0;
    if (!project_to_screen(cam, &sx, &sy)) { return; }

    // Splat with the camera's configured radius (default 2).
    let color = unpack_rgba(p.rgba_pack);
    let radius = i32(max(camera.splat_radius_px, 1u));
    splat_voxel(sx, sy, color, cam.z, radius);
}
