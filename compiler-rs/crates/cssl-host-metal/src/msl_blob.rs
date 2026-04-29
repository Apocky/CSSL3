//! Hand-written placeholder MSL kernel-set used by smoke tests.
//!
//! § Until S6-D3 lands the real CSSLv3 MSL emitter, we ship a no-op kernel
//!   trio (compute + vertex + fragment) so the loader machinery can be
//!   exercised end-to-end on Apple hosts via `MTLLibrary newWithSource`.
//!
//! § The MSL source is intentionally minimal :
//!   - no resource bindings (so the apple-side library compile is
//!     deterministic across Metal versions) ;
//!   - no platform-version pragmas (compiles on macOS 10.13+, iOS 11+) ;
//!   - explicit `kernel` / `vertex` / `fragment` markers so each entry
//!     point can be looked up by name.

/// Compute kernel — copies one buffer to another (no-op binding-free shape).
pub const MSL_COMPUTE_PLACEHOLDER: &str = "#include <metal_stdlib>
using namespace metal;

kernel void cssl_placeholder_compute(uint tid [[thread_position_in_grid]]) {
    // intentionally empty - D3 emitter replaces this with real kernel bodies.
    (void) tid;
}
";

/// Vertex shader — passes through normalized-device-coordinates as-is.
pub const MSL_VERTEX_PLACEHOLDER: &str = "#include <metal_stdlib>
using namespace metal;

struct VertexIn {
    float3 pos [[attribute(0)]];
};

struct VertexOut {
    float4 pos [[position]];
};

vertex VertexOut cssl_placeholder_vertex(VertexIn in [[stage_in]]) {
    VertexOut out;
    out.pos = float4(in.pos, 1.0);
    return out;
}
";

/// Fragment shader — returns opaque white.
pub const MSL_FRAGMENT_PLACEHOLDER: &str = "#include <metal_stdlib>
using namespace metal;

fragment float4 cssl_placeholder_fragment() {
    return float4(1.0, 1.0, 1.0, 1.0);
}
";

/// Combined placeholder shader-set covering compute + vertex + fragment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MslShaderSet {
    /// Compute kernel source.
    pub compute_source: &'static str,
    /// Compute entry-point name.
    pub compute_entry: &'static str,
    /// Vertex shader source.
    pub vertex_source: &'static str,
    /// Vertex entry-point name.
    pub vertex_entry: &'static str,
    /// Fragment shader source.
    pub fragment_source: &'static str,
    /// Fragment entry-point name.
    pub fragment_entry: &'static str,
}

impl MslShaderSet {
    /// Hand-written placeholder set covering all three stages.
    #[must_use]
    pub const fn placeholder() -> Self {
        Self {
            compute_source: MSL_COMPUTE_PLACEHOLDER,
            compute_entry: "cssl_placeholder_compute",
            vertex_source: MSL_VERTEX_PLACEHOLDER,
            vertex_entry: "cssl_placeholder_vertex",
            fragment_source: MSL_FRAGMENT_PLACEHOLDER,
            fragment_entry: "cssl_placeholder_fragment",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MslShaderSet, MSL_COMPUTE_PLACEHOLDER, MSL_FRAGMENT_PLACEHOLDER, MSL_VERTEX_PLACEHOLDER,
    };

    #[test]
    fn compute_placeholder_includes_metal_stdlib() {
        assert!(MSL_COMPUTE_PLACEHOLDER.contains("metal_stdlib"));
        assert!(MSL_COMPUTE_PLACEHOLDER.contains("kernel void"));
        assert!(MSL_COMPUTE_PLACEHOLDER.contains("cssl_placeholder_compute"));
    }

    #[test]
    fn vertex_placeholder_has_attribute_and_position() {
        assert!(MSL_VERTEX_PLACEHOLDER.contains("[[attribute(0)]]"));
        assert!(MSL_VERTEX_PLACEHOLDER.contains("[[position]]"));
        assert!(MSL_VERTEX_PLACEHOLDER.contains("vertex VertexOut"));
    }

    #[test]
    fn fragment_placeholder_returns_float4() {
        assert!(MSL_FRAGMENT_PLACEHOLDER.contains("fragment float4"));
        assert!(MSL_FRAGMENT_PLACEHOLDER.contains("cssl_placeholder_fragment"));
    }

    #[test]
    fn shader_set_placeholder_all_three_stages() {
        let s = MslShaderSet::placeholder();
        assert_eq!(s.compute_entry, "cssl_placeholder_compute");
        assert_eq!(s.vertex_entry, "cssl_placeholder_vertex");
        assert_eq!(s.fragment_entry, "cssl_placeholder_fragment");
        assert!(!s.compute_source.is_empty());
        assert!(!s.vertex_source.is_empty());
        assert!(!s.fragment_source.is_empty());
    }
}
