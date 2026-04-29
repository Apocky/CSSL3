//! § cssl-render::light — directional / point / spot / area light primitives
//! ═══════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Light parameters that drive the lighting passes. The renderer ships
//!   four light types :
//!   - [`Light::Directional`] — infinite-distance directional light (sun)
//!   - [`Light::Point`]       — omnidirectional point light with falloff
//!   - [`Light::Spot`]        — cone-restricted point light with inner/outer angle
//!   - [`Light::Area`]        — rectangular area light (LTC-approximated)
//!
//! § PHOTOMETRIC UNITS
//!   `intensity` is in linear candela (cd) for point/spot/area, lumens (lm)
//!   for directional. `color` is linear-space RGB. The shader multiplies
//!   color × intensity at light-evaluation time. Tonemap is applied at
//!   display-output via the RenderGraph's TonemapPass — colors here MUST
//!   be in linear space.
//!
//! § FALLOFF MODELS
//!   Point + Spot lights use **inverse-square falloff** : `1 / (1 + r²)`,
//!   modulated by `radius` for soft cutoff. Substrate canonical : real
//!   inverse-square (energy-conserving) ; the legacy `1 + d/r + d²/r²`
//!   falloff is NOT supported.
//!
//! § SHADOW CASTERS (stage-0 stub)
//!   `cast_shadow` is recorded but the actual shadow-map generation is a
//!   future slice (the RenderGraph's ShadowPass is scaffolded but not
//!   wired). `Light::shadow_bias` is preserved through the pipeline so
//!   when ShadowPass lands no struct migration is needed.

use crate::math::Vec3;

// ════════════════════════════════════════════════════════════════════════════
// § Light — discriminated union of light types
// ════════════════════════════════════════════════════════════════════════════

/// All light types share these baseline properties — extracted to avoid
/// per-variant duplication.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LightCommon {
    /// Linear-space RGB color in `[0, ∞)`. Pre-tonemap.
    pub color: Vec3,
    /// Linear photometric intensity. cd for point/spot/area, lm for
    /// directional. Applied as a scalar multiplier on `color`.
    pub intensity: f32,
    /// True if this light should cast shadows. Reserved for future
    /// ShadowPass slice — RenderGraph traverses the scene with this flag
    /// but no shadow maps are produced yet.
    pub cast_shadow: bool,
    /// Constant shadow-map bias to combat acne. Applied when ShadowPass
    /// lands. Typical range `[0.001, 0.01]`.
    pub shadow_bias: f32,
}

impl Default for LightCommon {
    fn default() -> Self {
        Self {
            color: Vec3::ONE,
            intensity: 1.0,
            cast_shadow: false,
            shadow_bias: 0.005,
        }
    }
}

/// Light primitive.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Light {
    /// Infinite-distance directional light. Common case : the sun. Has no
    /// position ; only direction. `intensity` in lumens (lm).
    Directional {
        common: LightCommon,
        /// World-space direction the light propagates *toward*. Substrate
        /// canonical : `(0, -1, 0)` is "sun directly overhead pointing down".
        /// Normalized at construction.
        direction: Vec3,
    },
    /// Point light : illuminates uniformly in all directions, falls off
    /// with inverse-square distance. `intensity` in candela (cd).
    Point {
        common: LightCommon,
        /// World-space position.
        position: Vec3,
        /// Soft cutoff radius. Beyond this distance the contribution fades
        /// to zero ; serves as a per-light culling bound. `f32::INFINITY`
        /// disables soft cutoff.
        radius: f32,
    },
    /// Spot light : point light with a cone-restricted emission. `intensity`
    /// in candela (cd).
    Spot {
        common: LightCommon,
        /// World-space position of the light origin.
        position: Vec3,
        /// World-space direction the cone points. Normalized.
        direction: Vec3,
        /// Inner cone half-angle in radians. Inside this cone the light is
        /// at full intensity ; between inner and outer it falls off.
        inner_angle_rad: f32,
        /// Outer cone half-angle in radians. Outside this cone contribution
        /// is zero. Must be `>= inner_angle_rad`.
        outer_angle_rad: f32,
        /// Soft cutoff radius (same semantics as `Point::radius`).
        radius: f32,
    },
    /// Area light : rectangular emitter. Stage-0 ships the rectangular
    /// case (the most common UI / studio-lighting shape) ; disk + sphere
    /// area lights are deferred to a follow-up.
    Area {
        common: LightCommon,
        /// World-space center of the rectangle.
        position: Vec3,
        /// World-space normal of the emitting face. Normalized.
        normal: Vec3,
        /// World-space "right" axis (rectangle width direction). Normalized.
        right: Vec3,
        /// Rectangle width along `right` axis.
        width: f32,
        /// Rectangle height along `cross(normal, right)`.
        height: f32,
    },
}

impl Default for Light {
    fn default() -> Self {
        Self::Directional {
            common: LightCommon::default(),
            direction: Vec3::new(0.0, -1.0, 0.0),
        }
    }
}

impl Light {
    /// Construct a directional light pointing along `direction` with `color`
    /// + `intensity`.
    #[must_use]
    pub fn directional(direction: Vec3, color: Vec3, intensity: f32) -> Self {
        Self::Directional {
            common: LightCommon {
                color,
                intensity,
                ..LightCommon::default()
            },
            direction: direction.normalize(),
        }
    }

    /// Construct a point light at `position` with `color` + `intensity` +
    /// `radius`.
    #[must_use]
    pub fn point(position: Vec3, color: Vec3, intensity: f32, radius: f32) -> Self {
        Self::Point {
            common: LightCommon {
                color,
                intensity,
                ..LightCommon::default()
            },
            position,
            radius,
        }
    }

    /// Borrow the common parameters regardless of variant.
    #[must_use]
    pub fn common(&self) -> &LightCommon {
        match self {
            Self::Directional { common, .. }
            | Self::Point { common, .. }
            | Self::Spot { common, .. }
            | Self::Area { common, .. } => common,
        }
    }

    /// True if the light casts shadows.
    #[must_use]
    pub fn casts_shadow(&self) -> bool {
        self.common().cast_shadow
    }

    /// World-space position, or `None` for directional lights (no position).
    #[must_use]
    pub fn position(&self) -> Option<Vec3> {
        match *self {
            Self::Directional { .. } => None,
            Self::Point { position, .. }
            | Self::Spot { position, .. }
            | Self::Area { position, .. } => Some(position),
        }
    }

    /// True if the light is a punctual / non-area light. Punctual lights
    /// can be evaluated analytically ; area lights need LTC approximation.
    #[must_use]
    pub fn is_punctual(&self) -> bool {
        !matches!(self, Self::Area { .. })
    }

    /// Soft-cutoff radius for finite-extent lights. Returns `f32::INFINITY`
    /// for directional + uncapped lights.
    #[must_use]
    pub fn influence_radius(&self) -> f32 {
        match *self {
            Self::Directional { .. } => f32::INFINITY,
            Self::Point { radius, .. } | Self::Spot { radius, .. } => radius,
            Self::Area { width, height, .. } => {
                // Area light's effective influence : diagonal of the
                // rectangle plus a 4× extension for far-field falloff.
                let diag = (width * width + height * height).sqrt();
                4.0 * diag
            }
        }
    }
}

// ════════════════════════════════════════════════════════════════════════════
// § Tests
// ════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() <= eps
    }

    #[test]
    fn light_default_is_directional_overhead() {
        match Light::default() {
            Light::Directional { direction, .. } => {
                assert!(approx_eq(direction.y, -1.0, 1e-6));
            }
            _ => panic!("default light should be directional"),
        }
    }

    #[test]
    fn light_common_default() {
        let c = LightCommon::default();
        assert_eq!(c.color, Vec3::ONE);
        assert_eq!(c.intensity, 1.0);
        assert!(!c.cast_shadow);
    }

    #[test]
    fn light_directional_constructor_normalizes() {
        let l = Light::directional(Vec3::new(0.0, -2.0, 0.0), Vec3::ONE, 5.0);
        match l {
            Light::Directional { direction, common } => {
                assert!(approx_eq(direction.length(), 1.0, 1e-6));
                assert_eq!(common.intensity, 5.0);
            }
            _ => panic!("expected directional"),
        }
    }

    #[test]
    fn light_point_constructor() {
        let l = Light::point(
            Vec3::new(1.0, 2.0, 3.0),
            Vec3::new(1.0, 0.5, 0.0),
            100.0,
            10.0,
        );
        match l {
            Light::Point {
                position,
                radius,
                common,
            } => {
                assert_eq!(position, Vec3::new(1.0, 2.0, 3.0));
                assert_eq!(radius, 10.0);
                assert_eq!(common.color, Vec3::new(1.0, 0.5, 0.0));
                assert_eq!(common.intensity, 100.0);
            }
            _ => panic!("expected point"),
        }
    }

    #[test]
    fn light_position_directional_is_none() {
        let l = Light::directional(Vec3::Y, Vec3::ONE, 1.0);
        assert!(l.position().is_none());
    }

    #[test]
    fn light_position_point_is_some() {
        let l = Light::point(Vec3::new(5.0, 0.0, 0.0), Vec3::ONE, 1.0, 1.0);
        assert_eq!(l.position(), Some(Vec3::new(5.0, 0.0, 0.0)));
    }

    #[test]
    fn light_punctual_classification() {
        let dir = Light::directional(Vec3::Y, Vec3::ONE, 1.0);
        assert!(dir.is_punctual());

        let pt = Light::point(Vec3::ZERO, Vec3::ONE, 1.0, 1.0);
        assert!(pt.is_punctual());

        let area = Light::Area {
            common: LightCommon::default(),
            position: Vec3::ZERO,
            normal: Vec3::Z,
            right: Vec3::X,
            width: 1.0,
            height: 1.0,
        };
        assert!(!area.is_punctual());
    }

    #[test]
    fn light_directional_radius_is_infinite() {
        let l = Light::directional(Vec3::Y, Vec3::ONE, 1.0);
        assert_eq!(l.influence_radius(), f32::INFINITY);
    }

    #[test]
    fn light_point_radius_is_explicit() {
        let l = Light::point(Vec3::ZERO, Vec3::ONE, 1.0, 7.0);
        assert_eq!(l.influence_radius(), 7.0);
    }

    #[test]
    fn light_spot_radius_is_explicit() {
        let l = Light::Spot {
            common: LightCommon::default(),
            position: Vec3::ZERO,
            direction: Vec3::new(0.0, -1.0, 0.0),
            inner_angle_rad: 0.3,
            outer_angle_rad: 0.5,
            radius: 12.0,
        };
        assert_eq!(l.influence_radius(), 12.0);
    }

    #[test]
    fn light_area_radius_uses_diagonal() {
        let l = Light::Area {
            common: LightCommon::default(),
            position: Vec3::ZERO,
            normal: Vec3::Z,
            right: Vec3::X,
            width: 3.0,
            height: 4.0,
        };
        // diagonal = sqrt(9 + 16) = 5 ; 4 × 5 = 20.
        assert_eq!(l.influence_radius(), 20.0);
    }

    #[test]
    fn light_shadow_default_off() {
        let l = Light::default();
        assert!(!l.casts_shadow());
    }

    #[test]
    fn light_shadow_can_be_set() {
        let mut l = Light::point(Vec3::ZERO, Vec3::ONE, 1.0, 1.0);
        if let Light::Point { common, .. } = &mut l {
            common.cast_shadow = true;
        }
        assert!(l.casts_shadow());
    }

    #[test]
    fn light_common_borrow_uniform() {
        let l = Light::point(Vec3::ZERO, Vec3::new(0.5, 0.5, 0.5), 42.0, 1.0);
        assert_eq!(l.common().intensity, 42.0);
        assert_eq!(l.common().color, Vec3::new(0.5, 0.5, 0.5));
    }
}
