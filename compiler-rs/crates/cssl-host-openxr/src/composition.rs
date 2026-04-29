//! Composition-layer enum + builders.
//!
//! § SPEC : `07_AESTHETIC/05_VR_RENDERING.csl` § XI.E + § IX +
//!         05_VR_RENDERING § IV.A xrEndFrame submission.
//!
//! § DESIGN
//!   `XrCompositionLayer` enum is the union over the spec's day-one
//!   composition-layer types :
//!     - Projection           (XR_TYPE_COMPOSITION_LAYER_PROJECTION) — primary stereo output
//!     - ProjectionDepth      (+ XR_KHR_composition_layer_depth)
//!     - Quad                 (XR_TYPE_COMPOSITION_LAYER_QUAD) — UI/HUD
//!     - Cylinder             (XR_KHR_composition_layer_cylinder) — wrap-around UI
//!     - Equirect             (XR_KHR_composition_layer_equirect2) — 360° media
//!     - Cube                 (XR_KHR_composition_layer_cube) — skybox
//!     - Passthrough          (XR_FB_passthrough)
//!     - PassthroughDepth     (passthrough + XR_META_environment_depth)
//!     - EnvironmentMesh      (5-yr scene-mesh anticipated)

use crate::error::XRFailure;
use crate::passthrough::PassthroughLayer;
use crate::view::ViewSet;

/// Common composition-layer flags. Bitmask matches OpenXR
/// `XrCompositionLayerFlags` semantics ; the actual values are mapped
/// at the FFI boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CompositionLayerFlags {
    /// Correct-chromatic-aberration enabled.
    pub correct_chromatic_aberration: bool,
    /// Source-alpha blend (vs. unpremultiplied).
    pub source_alpha: bool,
    /// Unpremultiplied alpha.
    pub unpremultiplied_alpha: bool,
}

impl CompositionLayerFlags {
    /// All-zero flags.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            correct_chromatic_aberration: false,
            source_alpha: false,
            unpremultiplied_alpha: false,
        }
    }

    /// Default for projection-layer (chromatic-correction on, source-alpha off).
    #[must_use]
    pub const fn projection_default() -> Self {
        Self {
            correct_chromatic_aberration: true,
            source_alpha: false,
            unpremultiplied_alpha: false,
        }
    }

    /// Default for UI/HUD layers (source-alpha + chromatic-correction).
    #[must_use]
    pub const fn ui_default() -> Self {
        Self {
            correct_chromatic_aberration: true,
            source_alpha: true,
            unpremultiplied_alpha: false,
        }
    }
}

/// Quad-layer params. § XI.E UI/HUD.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QuadLayerParams {
    /// Width in meters at 1m distance.
    pub size_m: [f32; 2],
    /// Pose (column-major mat4).
    pub pose: [f32; 16],
    /// Texture-handle.
    pub texture: u64,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

/// Cylinder-layer params. § XI.E wrap-around UI.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CylinderLayerParams {
    /// Radius in meters.
    pub radius_m: f32,
    /// Central-angle in radians.
    pub central_angle_rad: f32,
    /// Aspect-ratio (width / height).
    pub aspect_ratio: f32,
    /// Pose.
    pub pose: [f32; 16],
    /// Texture-handle.
    pub texture: u64,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

/// Equirect-layer params. 360° media.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EquirectLayerParams {
    /// Pose.
    pub pose: [f32; 16],
    /// Radius in meters (¬ infinite ; finite-radius equirect).
    pub radius_m: f32,
    /// Central-horizontal-angle in radians.
    pub central_horizontal_angle_rad: f32,
    /// Upper-vertical-angle in radians.
    pub upper_vertical_angle_rad: f32,
    /// Lower-vertical-angle in radians.
    pub lower_vertical_angle_rad: f32,
    /// Texture-handle.
    pub texture: u64,
}

/// Cube-layer params. Skybox (cubemap).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CubeLayerParams {
    /// Pose orientation (rotation only ; cube is at infinity).
    pub orientation: [f32; 4],
    /// Cubemap-texture handle.
    pub cubemap: u64,
}

/// Environment-mesh layer params (5-yr forward-compat hook).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EnvironmentMeshParams {
    /// Mesh-handle.
    pub mesh_handle: u64,
    /// Pose.
    pub pose: [f32; 16],
}

/// Composition-layer enum. § IV.A submitted to `xrEndFrame`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum XrCompositionLayer {
    /// Projection (stereo / quad-view) — primary.
    Projection {
        /// Common flags.
        flags: CompositionLayerFlags,
        /// View-count of the parent ViewSet.
        view_count: u32,
        /// Per-view color texture-handles.
        color_textures: [u64; crate::view::MAX_VIEWS],
        /// Per-view linear-depth texture-handles (for XR_KHR_composition_layer_depth).
        depth_textures: [u64; crate::view::MAX_VIEWS],
        /// `true` iff depth-attachment present (XR_KHR_composition_layer_depth in use).
        depth_present: bool,
    },
    /// Quad — UI/HUD.
    Quad {
        /// Common flags.
        flags: CompositionLayerFlags,
        /// Quad params.
        params: QuadLayerParams,
    },
    /// Cylinder — wrap-around UI.
    Cylinder {
        /// Common flags.
        flags: CompositionLayerFlags,
        /// Cylinder params.
        params: CylinderLayerParams,
    },
    /// Equirect — 360° media.
    Equirect {
        /// Common flags.
        flags: CompositionLayerFlags,
        /// Equirect params.
        params: EquirectLayerParams,
    },
    /// Cube — skybox.
    Cube {
        /// Common flags.
        flags: CompositionLayerFlags,
        /// Cube params.
        params: CubeLayerParams,
    },
    /// Passthrough.
    Passthrough(PassthroughLayer),
    /// Environment-mesh (5-yr forward-compat).
    EnvironmentMesh {
        /// Params.
        params: EnvironmentMeshParams,
    },
}

impl XrCompositionLayer {
    /// Build a Projection-layer for `view_set` with given color +
    /// depth handles.
    pub fn projection(
        view_set: &ViewSet,
        color_textures: &[u64],
        depth_textures: Option<&[u64]>,
    ) -> Result<Self, XRFailure> {
        if color_textures.len() != view_set.view_count as usize {
            return Err(XRFailure::ViewCountOutOfRange {
                got: color_textures.len() as u32,
            });
        }
        let mut color = [0u64; crate::view::MAX_VIEWS];
        for (i, c) in color_textures.iter().enumerate().take(crate::view::MAX_VIEWS) {
            color[i] = *c;
        }
        let mut depth = [0u64; crate::view::MAX_VIEWS];
        let depth_present = if let Some(d) = depth_textures {
            if d.len() != view_set.view_count as usize {
                return Err(XRFailure::ViewCountOutOfRange {
                    got: d.len() as u32,
                });
            }
            for (i, dh) in d.iter().enumerate().take(crate::view::MAX_VIEWS) {
                depth[i] = *dh;
            }
            true
        } else {
            false
        };
        Ok(Self::Projection {
            flags: CompositionLayerFlags::projection_default(),
            view_count: view_set.view_count,
            color_textures: color,
            depth_textures: depth,
            depth_present,
        })
    }

    /// Build a Quad-layer.
    pub fn quad(params: QuadLayerParams) -> Self {
        Self::Quad {
            flags: CompositionLayerFlags::ui_default(),
            params,
        }
    }

    /// Build a Cylinder-layer.
    pub fn cylinder(params: CylinderLayerParams) -> Self {
        Self::Cylinder {
            flags: CompositionLayerFlags::ui_default(),
            params,
        }
    }

    /// Build an Equirect-layer.
    pub fn equirect(params: EquirectLayerParams) -> Self {
        Self::Equirect {
            flags: CompositionLayerFlags::projection_default(),
            params,
        }
    }

    /// Build a Cube-layer (skybox).
    pub fn cube(params: CubeLayerParams) -> Self {
        Self::Cube {
            flags: CompositionLayerFlags::projection_default(),
            params,
        }
    }

    /// Layer-type discriminant string for logs / diagnostics.
    #[must_use]
    pub const fn type_str(&self) -> &'static str {
        match self {
            Self::Projection { .. } => "projection",
            Self::Quad { .. } => "quad",
            Self::Cylinder { .. } => "cylinder",
            Self::Equirect { .. } => "equirect",
            Self::Cube { .. } => "cube",
            Self::Passthrough(_) => "passthrough",
            Self::EnvironmentMesh { .. } => "environment-mesh",
        }
    }
}

/// Composition-layer stack submitted to `xrEndFrame`. Order matters :
/// front of array = back-most layer.
#[derive(Debug, Clone, Default)]
pub struct CompositionLayerStack {
    layers: Vec<XrCompositionLayer>,
}

impl CompositionLayerStack {
    /// Empty stack.
    #[must_use]
    pub fn empty() -> Self {
        Self { layers: Vec::new() }
    }

    /// Number of layers.
    #[must_use]
    pub fn len(&self) -> usize {
        self.layers.len()
    }

    /// `true` iff empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.layers.is_empty()
    }

    /// Push a layer (becomes front-most after this call).
    pub fn push(&mut self, layer: XrCompositionLayer) {
        self.layers.push(layer);
    }

    /// Iterate layers in submission order (back-most → front-most).
    pub fn iter(&self) -> impl Iterator<Item = &XrCompositionLayer> {
        self.layers.iter()
    }

    /// Validate the stack : at least one Projection or Passthrough layer
    /// must be present (per OpenXR spec, otherwise the runtime rejects).
    pub fn validate(&self) -> Result<(), XRFailure> {
        if self.layers.is_empty() {
            return Err(XRFailure::CompositionLayerRejected { index: 0, code: -100 });
        }
        let has_primary = self.layers.iter().any(|l| {
            matches!(
                l,
                XrCompositionLayer::Projection { .. } | XrCompositionLayer::Passthrough(_)
            )
        });
        if !has_primary {
            return Err(XRFailure::CompositionLayerRejected { index: 0, code: -101 });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CompositionLayerFlags, CompositionLayerStack, CubeLayerParams, CylinderLayerParams,
        EquirectLayerParams, QuadLayerParams, XrCompositionLayer,
    };
    use crate::passthrough::{PassthroughConfig, PassthroughLayer};
    use crate::view::{identity_mat4, ViewSet};

    #[test]
    fn flags_projection_default() {
        let f = CompositionLayerFlags::projection_default();
        assert!(f.correct_chromatic_aberration);
        assert!(!f.source_alpha);
    }

    #[test]
    fn flags_ui_default() {
        let f = CompositionLayerFlags::ui_default();
        assert!(f.correct_chromatic_aberration);
        assert!(f.source_alpha);
    }

    #[test]
    fn projection_layer_with_depth() {
        let vs = ViewSet::stereo_identity(64.0);
        let layer = XrCompositionLayer::projection(
            &vs,
            &[0xc010_0001, 0xc010_0002],
            Some(&[0xdeb7_0001, 0xdeb7_0002]),
        )
        .unwrap();
        assert_eq!(layer.type_str(), "projection");
        if let XrCompositionLayer::Projection {
            depth_present,
            view_count,
            ..
        } = layer
        {
            assert!(depth_present);
            assert_eq!(view_count, 2);
        } else {
            panic!("not projection");
        }
    }

    #[test]
    fn projection_layer_without_depth() {
        let vs = ViewSet::stereo_identity(64.0);
        let layer = XrCompositionLayer::projection(&vs, &[1, 2], None).unwrap();
        if let XrCompositionLayer::Projection { depth_present, .. } = layer {
            assert!(!depth_present);
        }
    }

    #[test]
    fn projection_view_count_mismatch_fails() {
        let vs = ViewSet::stereo_identity(64.0);
        // Pass 3 textures for a 2-view set.
        assert!(XrCompositionLayer::projection(&vs, &[1, 2, 3], None).is_err());
    }

    #[test]
    fn quad_layer_builds() {
        let p = QuadLayerParams {
            size_m: [1.5, 1.0],
            pose: identity_mat4(),
            texture: 0xbeef,
            width: 1024,
            height: 1024,
        };
        let layer = XrCompositionLayer::quad(p);
        assert_eq!(layer.type_str(), "quad");
    }

    #[test]
    fn cylinder_layer_builds() {
        let p = CylinderLayerParams {
            radius_m: 1.0,
            central_angle_rad: std::f32::consts::PI,
            aspect_ratio: 16.0 / 9.0,
            pose: identity_mat4(),
            texture: 0xdead,
            width: 2048,
            height: 1024,
        };
        let layer = XrCompositionLayer::cylinder(p);
        assert_eq!(layer.type_str(), "cylinder");
    }

    #[test]
    fn equirect_layer_builds() {
        let p = EquirectLayerParams {
            pose: identity_mat4(),
            radius_m: 100.0,
            central_horizontal_angle_rad: std::f32::consts::TAU,
            upper_vertical_angle_rad: std::f32::consts::FRAC_PI_2,
            lower_vertical_angle_rad: -std::f32::consts::FRAC_PI_2,
            texture: 0xfeed,
        };
        let layer = XrCompositionLayer::equirect(p);
        assert_eq!(layer.type_str(), "equirect");
    }

    #[test]
    fn cube_layer_builds() {
        let p = CubeLayerParams {
            orientation: [0.0, 0.0, 0.0, 1.0],
            cubemap: 0xc0debabe,
        };
        let layer = XrCompositionLayer::cube(p);
        assert_eq!(layer.type_str(), "cube");
    }

    #[test]
    fn passthrough_layer_classified() {
        let cfg = PassthroughConfig::quest3_default();
        let pt = PassthroughLayer::from_config(cfg).unwrap();
        let layer = XrCompositionLayer::Passthrough(pt);
        assert_eq!(layer.type_str(), "passthrough");
    }

    #[test]
    fn empty_stack_validate_fails() {
        let s = CompositionLayerStack::empty();
        assert!(s.validate().is_err());
    }

    #[test]
    fn quad_only_stack_fails_without_primary() {
        let mut s = CompositionLayerStack::empty();
        let p = QuadLayerParams {
            size_m: [1.0, 1.0],
            pose: identity_mat4(),
            texture: 1,
            width: 256,
            height: 256,
        };
        s.push(XrCompositionLayer::quad(p));
        // No primary Projection or Passthrough ⇒ runtime rejects.
        assert!(s.validate().is_err());
    }

    #[test]
    fn projection_plus_quad_stack_validates() {
        let vs = ViewSet::stereo_identity(64.0);
        let mut s = CompositionLayerStack::empty();
        s.push(XrCompositionLayer::projection(&vs, &[1, 2], None).unwrap());
        let p = QuadLayerParams {
            size_m: [1.0, 1.0],
            pose: identity_mat4(),
            texture: 1,
            width: 256,
            height: 256,
        };
        s.push(XrCompositionLayer::quad(p));
        assert_eq!(s.len(), 2);
        assert!(s.validate().is_ok());
    }

    #[test]
    fn passthrough_only_stack_validates() {
        let mut s = CompositionLayerStack::empty();
        let pt = PassthroughLayer::from_config(PassthroughConfig::quest3_default()).unwrap();
        s.push(XrCompositionLayer::Passthrough(pt));
        assert!(s.validate().is_ok());
    }
}
