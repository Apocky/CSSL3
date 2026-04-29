//! § sdf — analytic SDF primitives + composition.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Analytic Signed-Distance-Field primitives derived from PGA. Each primitive
//!   carries a known Lipschitz-bound (`L ≤ 1`) so composition under smooth-min /
//!   union / intersection / subtraction preserves the bound. The composition
//!   tree is the "world-as-SDF" surface walked by [`crate::raymarch::SdfRaymarchPass`].
//!
//! § SPEC
//!   - `Omniverse/07_AESTHETIC/01_SDF_NATIVE_RENDER.csl.md` § III SDF assembly :
//!     analytic + voxel-overlay smooth-min into single SDF, Lipschitz-bound
//!     preserved through compose.
//!   - `Omniverse/07_AESTHETIC/01_SDF_NATIVE_RENDER.csl.md` § IV ray marching :
//!     SDF must be `SDF'L<1.0>` typed for sphere-tracing soundness.
//!   - `Omniverse/01_AXIOMS/10_OPUS_MATH.csl § I` — PGA as SDF foundation.
//!
//! § PRIMITIVES (all Lipschitz-bound L ≤ 1)
//!   - **Sphere**             : `‖p - center‖ - radius`
//!   - **Box**                : `max(|p_i - center_i| - half_extent_i)` (linfty form,
//!                              with rounded `q.length() + min(max(q),0)` body)
//!   - **Plane (PGA)**        : `n · p + d` (PGA grade-1 vector, unit-normalized)
//!   - **Capsule**            : `‖p - segment_closest‖ - radius`
//!   - **Torus**              : `‖(‖p_xy‖ - R, p_z)‖ - r`
//!   - **Cylinder**           : `‖(‖p_xy‖ - r, p_z - h/2)‖ for half-height case`
//!
//! § COMPOSITION
//!   - **Union (hard-min)**       : `min(a, b)`
//!   - **Smooth-union**           : log-sum-exp form `-(1/k) ln(e^{-k a} + e^{-k b})`
//!   - **Intersection (hard-max)** : `max(a, b)`
//!   - **Subtraction**            : `max(a, -b)`
//!   - **Xor**                    : `max(min(a, b), -max(a, b))`
//!
//! § DETERMINISM
//!   All primitives are pure `f32` arithmetic with no `std::arch::*` or
//!   non-IEEE-754 paths. The smooth-min `k` parameter is taken at construction
//!   time so the composition tree's behavior is replay-deterministic.

use cssl_pga::Plane;

/// Lipschitz-bound on a SDF — `L = sup|∇f|` over the domain.
///
/// Sphere-tracing convergence depends on `L ≤ 1.0`. Composition operations in
/// this module preserve the bound by construction ; primitives below all return
/// `LipschitzBound::ONE`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LipschitzBound(pub f32);

impl LipschitzBound {
    /// The canonical L=1 bound — true sphere-tracing-safe SDF.
    pub const ONE: Self = LipschitzBound(1.0);
    /// Constant for queries that may not need the bound.
    pub const UNKNOWN: Self = LipschitzBound(f32::NAN);

    /// Combine two bounds under a max-form composition (intersection /
    /// subtraction / smooth-min). The result is the max of the inputs.
    #[must_use]
    pub fn combine_max(a: Self, b: Self) -> Self {
        LipschitzBound(a.0.max(b.0))
    }

    /// Returns `true` if the bound is finite + ≤ 1.
    #[must_use]
    pub fn is_sphere_traceable(self) -> bool {
        self.0.is_finite() && self.0 <= 1.0 + 1e-6
    }
}

impl Default for LipschitzBound {
    fn default() -> Self {
        Self::ONE
    }
}

/// One analytic SDF primitive. Each variant carries the parameters needed to
/// evaluate the distance + the Lipschitz-bound (always L ≤ 1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AnalyticSdfKind {
    /// Sphere : center + radius.
    Sphere { center: [f32; 3], radius: f32 },
    /// Axis-aligned box : center + half-extent per-axis.
    Box {
        center: [f32; 3],
        half_extents: [f32; 3],
    },
    /// PGA-derived infinite plane : `n · p + d = 0` ; `n` must be unit.
    Plane(Plane),
    /// Capsule : two endpoints + radius.
    Capsule {
        a: [f32; 3],
        b: [f32; 3],
        radius: f32,
    },
    /// Torus : center + major-radius (R) + minor-radius (r).
    Torus {
        center: [f32; 3],
        major: f32,
        minor: f32,
    },
    /// Vertical cylinder : center + radius + half-height.
    Cylinder {
        center: [f32; 3],
        radius: f32,
        half_height: f32,
    },
}

/// Analytic SDF primitive with its Lipschitz-bound.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AnalyticSdf {
    /// Underlying primitive shape.
    pub kind: AnalyticSdfKind,
    /// Lipschitz-bound. All primitives in this crate are L=1 by construction.
    pub lipschitz: LipschitzBound,
}

impl AnalyticSdf {
    /// New sphere primitive.
    #[must_use]
    pub fn sphere(cx: f32, cy: f32, cz: f32, radius: f32) -> Self {
        AnalyticSdf {
            kind: AnalyticSdfKind::Sphere {
                center: [cx, cy, cz],
                radius,
            },
            lipschitz: LipschitzBound::ONE,
        }
    }

    /// New box primitive.
    #[must_use]
    pub fn cube(cx: f32, cy: f32, cz: f32, hex: f32, hey: f32, hez: f32) -> Self {
        AnalyticSdf {
            kind: AnalyticSdfKind::Box {
                center: [cx, cy, cz],
                half_extents: [hex, hey, hez],
            },
            lipschitz: LipschitzBound::ONE,
        }
    }

    /// New plane primitive from a PGA Plane.
    #[must_use]
    pub fn plane(p: Plane) -> Self {
        AnalyticSdf {
            kind: AnalyticSdfKind::Plane(p),
            lipschitz: LipschitzBound::ONE,
        }
    }

    /// New capsule primitive.
    #[must_use]
    pub fn capsule(a: [f32; 3], b: [f32; 3], radius: f32) -> Self {
        AnalyticSdf {
            kind: AnalyticSdfKind::Capsule { a, b, radius },
            lipschitz: LipschitzBound::ONE,
        }
    }

    /// New torus primitive (major, minor radius).
    #[must_use]
    pub fn torus(cx: f32, cy: f32, cz: f32, major: f32, minor: f32) -> Self {
        AnalyticSdf {
            kind: AnalyticSdfKind::Torus {
                center: [cx, cy, cz],
                major,
                minor,
            },
            lipschitz: LipschitzBound::ONE,
        }
    }

    /// New vertical cylinder primitive.
    #[must_use]
    pub fn cylinder(cx: f32, cy: f32, cz: f32, radius: f32, half_height: f32) -> Self {
        AnalyticSdf {
            kind: AnalyticSdfKind::Cylinder {
                center: [cx, cy, cz],
                radius,
                half_height,
            },
            lipschitz: LipschitzBound::ONE,
        }
    }

    /// Evaluate the SDF at `p`. Returns the signed distance (negative = inside).
    #[must_use]
    pub fn evaluate(&self, p: [f32; 3]) -> f32 {
        match &self.kind {
            AnalyticSdfKind::Sphere { center, radius } => {
                let dx = p[0] - center[0];
                let dy = p[1] - center[1];
                let dz = p[2] - center[2];
                (dx * dx + dy * dy + dz * dz).sqrt() - radius
            }
            AnalyticSdfKind::Box {
                center,
                half_extents,
            } => {
                let qx = (p[0] - center[0]).abs() - half_extents[0];
                let qy = (p[1] - center[1]).abs() - half_extents[1];
                let qz = (p[2] - center[2]).abs() - half_extents[2];
                let q_outside =
                    ((qx.max(0.0)).powi(2) + (qy.max(0.0)).powi(2) + (qz.max(0.0)).powi(2)).sqrt();
                let q_inside = qx.max(qy).max(qz).min(0.0);
                q_outside + q_inside
            }
            AnalyticSdfKind::Plane(plane) => {
                // n · p + d ; PGA plane stores e1=a e2=b e3=c e0=d.
                plane.e1 * p[0] + plane.e2 * p[1] + plane.e3 * p[2] + plane.e0
            }
            AnalyticSdfKind::Capsule { a, b, radius } => capsule_sdf(p, *a, *b, *radius),
            AnalyticSdfKind::Torus {
                center,
                major,
                minor,
            } => {
                let dx = p[0] - center[0];
                let dy = p[1] - center[1];
                let dz = p[2] - center[2];
                let xy_len = (dx * dx + dz * dz).sqrt();
                let qx = xy_len - major;
                let qy = dy;
                (qx * qx + qy * qy).sqrt() - minor
            }
            AnalyticSdfKind::Cylinder {
                center,
                radius,
                half_height,
            } => {
                let dx = p[0] - center[0];
                let dy = p[1] - center[1];
                let dz = p[2] - center[2];
                let xy = (dx * dx + dz * dz).sqrt() - radius;
                let z_off = dy.abs() - half_height;
                let q_outside = ((xy.max(0.0)).powi(2) + (z_off.max(0.0)).powi(2)).sqrt();
                let q_inside = xy.max(z_off).min(0.0);
                q_outside + q_inside
            }
        }
    }

    /// Return the analytic Lipschitz-bound.
    #[must_use]
    pub fn lipschitz(&self) -> LipschitzBound {
        self.lipschitz
    }
}

/// Closest-point distance from `p` to segment `a..b`, minus `radius`.
///
/// Used for capsule SDF. Algebraically the segment-projection is
/// `t = clamp(((p-a) · (b-a)) / ‖b-a‖², 0, 1)`.
#[must_use]
fn capsule_sdf(p: [f32; 3], a: [f32; 3], b: [f32; 3], radius: f32) -> f32 {
    let pa = [p[0] - a[0], p[1] - a[1], p[2] - a[2]];
    let ba = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let ba_dot_ba = ba[0] * ba[0] + ba[1] * ba[1] + ba[2] * ba[2];
    let pa_dot_ba = pa[0] * ba[0] + pa[1] * ba[1] + pa[2] * ba[2];
    let t = if ba_dot_ba > 0.0 {
        (pa_dot_ba / ba_dot_ba).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let q = [pa[0] - ba[0] * t, pa[1] - ba[1] * t, pa[2] - ba[2] * t];
    (q[0] * q[0] + q[1] * q[1] + q[2] * q[2]).sqrt() - radius
}

/// Composition operator over two SDFs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CompositionOp {
    /// `min(a, b)` — sharp union.
    HardUnion,
    /// `-(1/k) ln(e^{-k a} + e^{-k b})` — smooth union with parameter `k > 0`.
    SmoothUnion { k: f32 },
    /// `max(a, b)` — sharp intersection.
    HardIntersection,
    /// `max(a, -b)` — subtraction (a − b).
    Subtraction,
    /// `max(min(a, b), -max(a, b))` — symmetric difference.
    Xor,
}

/// SDF composition tree node : either a leaf (analytic primitive) or an interior
/// node combining two children under a [`CompositionOp`].
#[derive(Debug, Clone)]
pub enum SdfComposition {
    /// Analytic-primitive leaf.
    Leaf(AnalyticSdf),
    /// Interior node : op + two children. Boxed to keep the type Sized.
    Node {
        op: CompositionOp,
        a: Box<SdfComposition>,
        b: Box<SdfComposition>,
    },
}

impl SdfComposition {
    /// Wrap a primitive as a leaf node.
    #[must_use]
    pub fn from_primitive(prim: AnalyticSdf) -> Self {
        SdfComposition::Leaf(prim)
    }

    /// Hard-union (min) of two compositions.
    #[must_use]
    pub fn hard_union(a: SdfComposition, b: SdfComposition) -> Self {
        SdfComposition::Node {
            op: CompositionOp::HardUnion,
            a: Box::new(a),
            b: Box::new(b),
        }
    }

    /// Smooth-union with smoothness `k` (typical 0.05 .. 0.5).
    #[must_use]
    pub fn smooth_union(a: SdfComposition, b: SdfComposition, k: f32) -> Self {
        SdfComposition::Node {
            op: CompositionOp::SmoothUnion { k },
            a: Box::new(a),
            b: Box::new(b),
        }
    }

    /// Hard-intersection (max) of two compositions.
    #[must_use]
    pub fn hard_intersection(a: SdfComposition, b: SdfComposition) -> Self {
        SdfComposition::Node {
            op: CompositionOp::HardIntersection,
            a: Box::new(a),
            b: Box::new(b),
        }
    }

    /// Subtraction : `a - b`.
    #[must_use]
    pub fn subtraction(a: SdfComposition, b: SdfComposition) -> Self {
        SdfComposition::Node {
            op: CompositionOp::Subtraction,
            a: Box::new(a),
            b: Box::new(b),
        }
    }

    /// Xor : symmetric difference.
    #[must_use]
    pub fn xor(a: SdfComposition, b: SdfComposition) -> Self {
        SdfComposition::Node {
            op: CompositionOp::Xor,
            a: Box::new(a),
            b: Box::new(b),
        }
    }

    /// Evaluate the SDF at `p`.
    #[must_use]
    pub fn evaluate(&self, p: [f32; 3]) -> f32 {
        match self {
            SdfComposition::Leaf(prim) => prim.evaluate(p),
            SdfComposition::Node { op, a, b } => {
                let da = a.evaluate(p);
                let db = b.evaluate(p);
                evaluate_op(*op, da, db)
            }
        }
    }

    /// Return the Lipschitz-bound of the composition. Combination is
    /// `combine_max`-conservative — bounds can only grow under composition
    /// without re-normalization (which is why the smooth-min `k` parameter is
    /// kept low ; large `k` is sharper but does not violate L=1 because the
    /// log-sum-exp is 1-Lipschitz in its inputs).
    #[must_use]
    pub fn lipschitz(&self) -> LipschitzBound {
        match self {
            SdfComposition::Leaf(prim) => prim.lipschitz(),
            SdfComposition::Node { a, b, .. } => {
                LipschitzBound::combine_max(a.lipschitz(), b.lipschitz())
            }
        }
    }

    /// Number of leaf primitives in the composition tree (helper for tests +
    /// budget projections).
    #[must_use]
    pub fn leaf_count(&self) -> usize {
        match self {
            SdfComposition::Leaf(_) => 1,
            SdfComposition::Node { a, b, .. } => a.leaf_count() + b.leaf_count(),
        }
    }

    /// Tree-depth (helper for budget projections).
    #[must_use]
    pub fn depth(&self) -> usize {
        match self {
            SdfComposition::Leaf(_) => 1,
            SdfComposition::Node { a, b, .. } => 1 + a.depth().max(b.depth()),
        }
    }
}

/// Evaluate a composition op given two child distances `da`, `db`.
#[must_use]
pub fn evaluate_op(op: CompositionOp, da: f32, db: f32) -> f32 {
    match op {
        CompositionOp::HardUnion => da.min(db),
        CompositionOp::SmoothUnion { k } => smooth_min(da, db, k),
        CompositionOp::HardIntersection => da.max(db),
        CompositionOp::Subtraction => da.max(-db),
        CompositionOp::Xor => da.min(db).max(-(da.max(db))),
    }
}

/// Polynomial smooth-min (Inigo Quilez form) — k > 0 controls smoothness.
///
/// Returns a value that approaches `min(a, b)` as k → 0 (sharp) and gives a
/// smooth blend as k grows large. Lipschitz-1 in its inputs.
///
/// Reference : <https://iquilezles.org/articles/smin/> (polynomial section).
#[must_use]
pub fn smooth_min(a: f32, b: f32, k: f32) -> f32 {
    if k <= 0.0 {
        return a.min(b);
    }
    let h = (0.5 + 0.5 * (b - a) / k).clamp(0.0, 1.0);
    // mix(b, a, h) - k*h*(1-h)
    b * (1.0 - h) + a * h - k * h * (1.0 - h)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cssl_pga::Plane;

    #[test]
    fn lipschitz_one_is_traceable() {
        assert!(LipschitzBound::ONE.is_sphere_traceable());
    }

    #[test]
    fn lipschitz_unknown_not_traceable() {
        assert!(!LipschitzBound::UNKNOWN.is_sphere_traceable());
    }

    #[test]
    fn sphere_at_origin_evaluates_zero_at_radius() {
        let s = AnalyticSdf::sphere(0.0, 0.0, 0.0, 1.0);
        assert!((s.evaluate([1.0, 0.0, 0.0])).abs() < 1e-6);
        assert!((s.evaluate([0.0, 1.0, 0.0])).abs() < 1e-6);
        assert!(s.evaluate([0.0, 0.0, 0.0]) < 0.0);
        assert!(s.evaluate([2.0, 0.0, 0.0]) > 0.0);
    }

    #[test]
    fn sphere_distance_is_radial() {
        let s = AnalyticSdf::sphere(1.0, 2.0, 3.0, 0.5);
        // Point at (1+0.5, 2, 3) is on surface.
        assert!((s.evaluate([1.5, 2.0, 3.0])).abs() < 1e-6);
        // Point at (4, 2, 3) is 3-0.5 = 2.5 outside.
        assert!((s.evaluate([4.0, 2.0, 3.0]) - 2.5).abs() < 1e-6);
    }

    #[test]
    fn box_at_origin_zero_at_face() {
        let b = AnalyticSdf::cube(0.0, 0.0, 0.0, 1.0, 1.0, 1.0);
        assert!((b.evaluate([1.0, 0.0, 0.0])).abs() < 1e-6);
        assert!(b.evaluate([0.0, 0.0, 0.0]) < 0.0);
        assert!(b.evaluate([2.0, 0.0, 0.0]) > 0.0);
    }

    #[test]
    fn box_corner_diagonal_distance() {
        // sqrt(3) - 1 outside the unit-cube corner along diagonal.
        let b = AnalyticSdf::cube(0.0, 0.0, 0.0, 1.0, 1.0, 1.0);
        let d = b.evaluate([2.0, 2.0, 2.0]);
        let expected = (1.0_f32.powi(2) * 3.0).sqrt(); // distance from corner
        assert!((d - expected).abs() < 1e-5);
    }

    #[test]
    fn plane_signed_distance() {
        let p = Plane::new(0.0, 1.0, 0.0, 0.0); // y = 0
        let s = AnalyticSdf::plane(p);
        assert!((s.evaluate([0.0, 0.0, 0.0])).abs() < 1e-6);
        assert!((s.evaluate([0.0, 5.0, 0.0]) - 5.0).abs() < 1e-6);
        assert!((s.evaluate([0.0, -3.0, 0.0]) + 3.0).abs() < 1e-6);
    }

    #[test]
    fn capsule_endpoint_zero() {
        let cap = AnalyticSdf::capsule([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], 0.5);
        // At endpoint a + radius along normal.
        assert!((cap.evaluate([0.0, 0.5, 0.0])).abs() < 1e-6);
        // Halfway between, +radius up.
        assert!((cap.evaluate([0.5, 0.5, 0.0])).abs() < 1e-6);
    }

    #[test]
    fn torus_surface_zero_in_plane() {
        let t = AnalyticSdf::torus(0.0, 0.0, 0.0, 2.0, 0.5);
        // Point on outer ring : (R+r, 0, 0).
        assert!((t.evaluate([2.5, 0.0, 0.0])).abs() < 1e-6);
        // Point on inner ring : (R-r, 0, 0).
        assert!((t.evaluate([1.5, 0.0, 0.0])).abs() < 1e-6);
    }

    #[test]
    fn cylinder_side_zero_at_radius() {
        let c = AnalyticSdf::cylinder(0.0, 0.0, 0.0, 1.0, 2.0);
        assert!((c.evaluate([1.0, 0.0, 0.0])).abs() < 1e-6);
    }

    #[test]
    fn smooth_min_approaches_hard_min_as_k_decreases() {
        // Smaller k = sharper, closer to true min(a, b).
        let a = 1.0;
        let b = 2.0;
        let s_sharp = smooth_min(a, b, 0.01);
        let s_blurry = smooth_min(a, b, 5.0);
        let m = a.min(b);
        // Smaller k is closer to true min.
        assert!((s_sharp - m).abs() < (s_blurry - m).abs());
    }

    #[test]
    fn smooth_min_with_zero_k_is_hard_min() {
        let s = smooth_min(1.0, 2.0, 0.0);
        assert!((s - 1.0).abs() < 1e-6);
    }

    #[test]
    fn composition_hard_union_is_min() {
        let a = SdfComposition::from_primitive(AnalyticSdf::sphere(0.0, 0.0, 0.0, 1.0));
        let b = SdfComposition::from_primitive(AnalyticSdf::sphere(2.0, 0.0, 0.0, 1.0));
        let u = SdfComposition::hard_union(a, b);
        // At (1, 0, 0) : on first sphere ; second sphere distance is 0.
        assert!((u.evaluate([1.0, 0.0, 0.0])).abs() < 1e-6);
        // Halfway between centers : closest is first or second sphere ; min wins.
        let d = u.evaluate([1.0, 0.0, 0.0]);
        assert!(d.abs() < 1e-6);
    }

    #[test]
    fn composition_subtraction_carves() {
        // (Big sphere) − (small sphere at center) leaves a shell.
        let big = SdfComposition::from_primitive(AnalyticSdf::sphere(0.0, 0.0, 0.0, 2.0));
        let small = SdfComposition::from_primitive(AnalyticSdf::sphere(0.0, 0.0, 0.0, 1.0));
        let shell = SdfComposition::subtraction(big, small);
        // Outside small : sign tracks distance to outer minus 0 (inside small carve).
        // At (1.5, 0, 0) : inside big (-0.5), outside small (+0.5). Subtraction = max(-0.5, -0.5) = -0.5.
        let d = shell.evaluate([1.5, 0.0, 0.0]);
        assert!(d < 0.0);
        // At (0.5, 0, 0) : inside big (-1.5), inside small (-0.5). Subtraction = max(-1.5, 0.5) = 0.5.
        let d = shell.evaluate([0.5, 0.0, 0.0]);
        assert!(d > 0.0);
    }

    #[test]
    fn composition_intersection() {
        let s1 = SdfComposition::from_primitive(AnalyticSdf::sphere(0.0, 0.0, 0.0, 2.0));
        let s2 = SdfComposition::from_primitive(AnalyticSdf::sphere(1.0, 0.0, 0.0, 2.0));
        let inter = SdfComposition::hard_intersection(s1, s2);
        // Inside both
        assert!(inter.evaluate([0.5, 0.0, 0.0]) < 0.0);
        // Outside both
        assert!(inter.evaluate([5.0, 0.0, 0.0]) > 0.0);
    }

    #[test]
    fn composition_xor() {
        let s1 = SdfComposition::from_primitive(AnalyticSdf::sphere(0.0, 0.0, 0.0, 1.0));
        let s2 = SdfComposition::from_primitive(AnalyticSdf::sphere(1.5, 0.0, 0.0, 1.0));
        let x = SdfComposition::xor(s1, s2);
        // Symmetric difference : in one but not both.
        assert!(x.evaluate([0.0, 0.0, 0.0]) < 0.0); // in s1 only
        assert!(x.evaluate([0.75, 0.0, 0.0]) > 0.0); // in both → excluded
    }

    #[test]
    fn composition_lipschitz_preserved() {
        let s1 = SdfComposition::from_primitive(AnalyticSdf::sphere(0.0, 0.0, 0.0, 1.0));
        let s2 = SdfComposition::from_primitive(AnalyticSdf::sphere(2.0, 0.0, 0.0, 1.0));
        let u = SdfComposition::smooth_union(s1, s2, 0.1);
        assert!(u.lipschitz().is_sphere_traceable());
    }

    #[test]
    fn composition_leaf_count_and_depth() {
        let s1 = SdfComposition::from_primitive(AnalyticSdf::sphere(0.0, 0.0, 0.0, 1.0));
        let s2 = SdfComposition::from_primitive(AnalyticSdf::sphere(2.0, 0.0, 0.0, 1.0));
        let s3 = SdfComposition::from_primitive(AnalyticSdf::sphere(4.0, 0.0, 0.0, 1.0));
        let u1 = SdfComposition::hard_union(s1, s2);
        let u2 = SdfComposition::hard_union(u1, s3);
        assert_eq!(u2.leaf_count(), 3);
        assert_eq!(u2.depth(), 3);
    }
}
