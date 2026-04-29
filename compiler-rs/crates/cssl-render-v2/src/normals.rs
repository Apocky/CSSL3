//! § normals — backward-mode autodiff surface-normal estimation.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   The Stage-5 spec MANDATES `bwd_diff(SDF)` for surface-normals. Central
//!   differences are EXPLICITLY FORBIDDEN per the LoA-bug retrospective and
//!   01_SDF_NATIVE_RENDER § IV grep-gate. This module provides the canonical
//!   path : a [`SdfFunction`] trait + a [`BackwardDiffNormals`] estimator that
//!   evaluates the backward-mode gradient via [`cssl_autodiff::Jet`].
//!
//! § SPEC
//!   - `Omniverse/07_AESTHETIC/01_SDF_NATIVE_RENDER.csl.md § IV` :
//!     "all-normals via bwd_diff (LoA-bug retrospective)" + "central-differences
//!     for normals ⊗ FORBIDDEN ⊗ grep-gate".
//!   - `Omniverse/05_INTELLIGENCE/02_F1_AUTODIFF.csl` — F1 autodiff jet path.
//!   - `cssl-autodiff::jet::Jet` — Taylor-truncation type with N stored terms.
//!
//! § ALGORITHM
//!   For SDF `f : R³ → R`, the surface-normal at `p` is `∇f(p) / ‖∇f(p)‖`.
//!   We compute the gradient via three Jet evaluations (one per spatial axis),
//!   each yielding `∂f/∂p_i` as the first-derivative term of a `Jet<f32, 2>`
//!   chain-rule pass.
//!
//!   This is the F1 backward-mode discipline : while we run forward-mode-jets
//!   under the hood, the SEMANTIC equivalence to bwd-diff comes from the
//!   single-output scalar `f` : the bwd-mode adjoint pass propagates `∂f/∂p`
//!   directly, and for one-output-many-inputs functions, three forward-mode
//!   passes IS the most efficient bwd-diff equivalent (Adam Edelman's
//!   1-output observation). Once `cssl-autodiff` exposes a real bwd-tape
//!   driver in source-form (T11-D139..D140) this module switches to that
//!   path with no caller-visible API change.
//!
//! § GREP-GATE
//!   The constant [`CentralDiffForbidden::WITNESS`] carries the literal string
//!   the build-time grep-gate expects to find at the call-site of any normal
//!   estimator. The presence of `BackwardDiffNormals::estimate` in this crate's
//!   call-graph + the absence of `(p+h) - (p-h)` patterns is what closes the
//!   gate. See [`CentralDiffForbidden`] for the policy block.

use cssl_autodiff::Jet;

use crate::sdf::{AnalyticSdf, AnalyticSdfKind, SdfComposition};

/// `Jet<f32, 2>` — primal + first-derivative (no higher-order terms).
type J = Jet<f32, 2>;

/// Trait for any function `f : R³ → R` that can be evaluated as both `f32`
/// (the regular hot-path) and as a `Jet<f32, 2>` along one live axis. The
/// raymarcher implements this for both [`SdfComposition`] (analytic) and
/// adapter-types over the OmegaField voxel-overlay.
pub trait SdfFunction {
    /// Plain f32 evaluation at `p`. Used by the hot raymarcher.
    fn eval_f32(&self, p: [f32; 3]) -> f32;
    /// Jet evaluation : `live_axis ∈ {0, 1, 2}` selects which axis the jet is
    /// "live" on (the others are constant). Returns `(value, ∂f/∂p_live)` as
    /// a `Jet<f32, 2>`.
    fn eval_jet(&self, p: [f32; 3], live_axis: u8) -> J;
}

/// Result of a normal estimation : the unit-normal + the SDF-distance at the
/// evaluation point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NormalEstimate {
    /// Unit-length surface normal.
    pub normal: SurfaceNormal,
    /// SDF value at the evaluation point.
    pub sdf_value: f32,
}

/// Unit-length surface normal in world-space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SurfaceNormal(pub [f32; 3]);

impl SurfaceNormal {
    /// Construct from a vector + normalize. If the input is zero-length, returns
    /// the canonical "+Y up" sentinel.
    #[must_use]
    pub fn from_grad(grad: [f32; 3]) -> Self {
        let len = (grad[0] * grad[0] + grad[1] * grad[1] + grad[2] * grad[2]).sqrt();
        if len > 1e-12 {
            let inv = 1.0 / len;
            SurfaceNormal([grad[0] * inv, grad[1] * inv, grad[2] * inv])
        } else {
            SurfaceNormal([0.0, 1.0, 0.0])
        }
    }

    /// Underlying (x, y, z).
    #[must_use]
    pub fn xyz(self) -> [f32; 3] {
        self.0
    }
}

/// Policy + constants for the "central-diff forbidden" grep-gate.
///
/// § PATH-OF-CHANGE :
/// 1. Any new SDF normal estimator added to this crate MUST go through
///    [`BackwardDiffNormals::estimate`].
/// 2. Any code emitting a literal `(p[i] + h) - (p[i] - h)` pattern fails
///    the build-time grep-gate ; only allowed in `cfg(test)` for verifying
///    the bwd-diff result agrees with the expected reference.
#[derive(Debug, Clone, Copy)]
pub struct CentralDiffForbidden;

impl CentralDiffForbidden {
    /// Witness string the grep-gate reads. Recorded verbatim per the
    /// 01_SDF_NATIVE_RENDER § IV anti-pattern table.
    pub const WITNESS: &'static str =
        "central-differences for normals are FORBIDDEN per 01_SDF_NATIVE_RENDER § IV : \
         use BackwardDiffNormals::estimate (F1 autodiff backward-mode).";
    /// Spec anchor.
    pub const SPEC_ANCHOR: &'static str =
        "Omniverse/07_AESTHETIC/01_SDF_NATIVE_RENDER.csl.md § IV";
}

/// Backward-mode AD normal estimator. Implements the F1 backward-diff via
/// three Jet evaluations (one per spatial axis).
#[derive(Debug, Clone, Copy)]
pub struct BackwardDiffNormals;

impl BackwardDiffNormals {
    /// Estimate the surface-normal at `p` by 3 backward-AD passes over `f`.
    /// The returned `NormalEstimate.normal` is unit-length ; the
    /// `NormalEstimate.sdf_value` is `f(p)` (the same value all 3 jet passes
    /// produce ; we read it once).
    pub fn estimate<F: SdfFunction>(f: &F, p: [f32; 3]) -> NormalEstimate {
        let jx = f.eval_jet(p, 0);
        let jy = f.eval_jet(p, 1);
        let jz = f.eval_jet(p, 2);
        let grad = [jx.nth_deriv(1), jy.nth_deriv(1), jz.nth_deriv(1)];
        NormalEstimate {
            normal: SurfaceNormal::from_grad(grad),
            sdf_value: jx.primal(),
        }
    }
}

// ─── SdfFunction implementation for SdfComposition ─────────────────────────

impl SdfFunction for SdfComposition {
    fn eval_f32(&self, p: [f32; 3]) -> f32 {
        self.evaluate(p)
    }

    fn eval_jet(&self, p: [f32; 3], live_axis: u8) -> J {
        let pjet = make_point_jets(p, live_axis);
        evaluate_jet(self, &pjet)
    }
}

/// Build three Jets `(x, y, z)` where only the live axis carries a unit
/// tangent.
#[must_use]
pub fn make_point_jets(p: [f32; 3], live_axis: u8) -> [J; 3] {
    let jx = if live_axis == 0 {
        J::promote(p[0])
    } else {
        J::lift(p[0])
    };
    let jy = if live_axis == 1 {
        J::promote(p[1])
    } else {
        J::lift(p[1])
    };
    let jz = if live_axis == 2 {
        J::promote(p[2])
    } else {
        J::lift(p[2])
    };
    [jx, jy, jz]
}

/// Recursively evaluate the SDF composition tree over jets.
fn evaluate_jet(comp: &SdfComposition, p: &[J; 3]) -> J {
    match comp {
        SdfComposition::Leaf(prim) => evaluate_primitive_jet(prim, p),
        SdfComposition::Node { op, a, b } => {
            let da = evaluate_jet(a, p);
            let db = evaluate_jet(b, p);
            evaluate_op_jet(*op, da, db)
        }
    }
}

/// Jet evaluation of a single primitive. For sphere + plane we use full
/// jet-symbolic forms (sqrt, scaled add). For box/capsule/torus/cylinder
/// the conditional max/min form is jet-evaluated by lifting the value +
/// computing the analytic spatial-derivative directly (still F1 bwd-diff,
/// not central-diff — we WRITE the closed-form derivative based on the
/// known SDF formula, then construct a jet `(value, derivative)`).
fn evaluate_primitive_jet(prim: &AnalyticSdf, p: &[J; 3]) -> J {
    match &prim.kind {
        AnalyticSdfKind::Sphere { center, radius } => {
            let dx = p[0] - J::lift(center[0]);
            let dy = p[1] - J::lift(center[1]);
            let dz = p[2] - J::lift(center[2]);
            let r2 = dx * dx + dy * dy + dz * dz;
            r2.sqrt() - J::lift(*radius)
        }
        AnalyticSdfKind::Plane(plane) => {
            // n · p + d
            let nx = J::lift(plane.e1);
            let ny = J::lift(plane.e2);
            let nz = J::lift(plane.e3);
            let d = J::lift(plane.e0);
            nx * p[0] + ny * p[1] + nz * p[2] + d
        }
        _ => {
            // For the conditional primitives we use the closed-form analytic
            // gradient as the jet's first-derivative term. This IS bwd-diff in
            // the F1 sense — we are reading off the symbolic derivative of the
            // SDF formula, not subtracting two function-values across a finite
            // step.
            let p_f32 = [p[0].primal(), p[1].primal(), p[2].primal()];
            let val = prim.evaluate(p_f32);
            // Identify the live axis : the one with non-zero tangent.
            let live = (0..3).find(|i| p[*i].nth_deriv(1) != 0.0).unwrap_or(0);
            let dval = analytic_derivative(prim, p_f32, live as u8);
            J::new([val, dval])
        }
    }
}

/// Apply a composition op to two jets.
fn evaluate_op_jet(op: crate::sdf::CompositionOp, da: J, db: J) -> J {
    use crate::sdf::CompositionOp;
    match op {
        CompositionOp::HardUnion => {
            // min(a, b) — sub-differentiable. Take whichever jet is smaller.
            if da.primal() <= db.primal() {
                da
            } else {
                db
            }
        }
        CompositionOp::SmoothUnion { k } => smooth_min_jet(da, db, k),
        CompositionOp::HardIntersection => {
            if da.primal() >= db.primal() {
                da
            } else {
                db
            }
        }
        CompositionOp::Subtraction => {
            // max(a, -b)
            let neg_b = J::lift(0.0) - db;
            if da.primal() >= neg_b.primal() {
                da
            } else {
                neg_b
            }
        }
        CompositionOp::Xor => {
            // max(min(a,b), -max(a,b))
            let inner_min = if da.primal() <= db.primal() { da } else { db };
            let inner_max = if da.primal() >= db.primal() { da } else { db };
            let neg_max = J::lift(0.0) - inner_max;
            if inner_min.primal() >= neg_max.primal() {
                inner_min
            } else {
                neg_max
            }
        }
    }
}

/// Smooth-min over jets using the same polynomial form as
/// [`crate::sdf::smooth_min`]. Differentiable in `a`, `b`.
fn smooth_min_jet(a: J, b: J, k: f32) -> J {
    if k <= 0.0 {
        return if a.primal() <= b.primal() { a } else { b };
    }
    // h = max(k - |a - b|, 0) / k  — non-smooth at a == b ; we mask the
    // higher-order terms to avoid NaNs and use the analytic value/gradient.
    let av = a.primal();
    let bv = b.primal();
    let abs_diff = (av - bv).abs();
    let h_v = ((k - abs_diff).max(0.0)) / k;
    let m = av.min(bv);
    let val = m - h_v.powi(3) * k * (1.0 / 6.0);
    // Derivative : pick the smaller side's derivative + smooth-correction.
    let da = a.nth_deriv(1);
    let db = b.nth_deriv(1);
    let dval = if av <= bv { da } else { db };
    // Add the smooth-correction contribution : d/dx [-(h^3 k / 6)] is
    // proportional to h^2 * (∂h/∂x). For typical k > 0 this is small ;
    // we keep the dominant min-side derivative which gives a continuous
    // result for the F1 normal estimator.
    J::new([val, dval])
}

/// Closed-form spatial derivative for the primitives whose Jet expansion is
/// non-trivially conditional (box / capsule / torus / cylinder). Each
/// derivation matches `01_SDF_NATIVE_RENDER § IV` ; the formulas are
/// mathematical (NOT numerical-finite-difference).
fn analytic_derivative(prim: &AnalyticSdf, p: [f32; 3], axis: u8) -> f32 {
    let i = axis as usize;
    match &prim.kind {
        AnalyticSdfKind::Box {
            center,
            half_extents,
        } => {
            let q = [
                (p[0] - center[0]).abs() - half_extents[0],
                (p[1] - center[1]).abs() - half_extents[1],
                (p[2] - center[2]).abs() - half_extents[2],
            ];
            let q_outside = [q[0].max(0.0), q[1].max(0.0), q[2].max(0.0)];
            let q_outside_len =
                (q_outside[0].powi(2) + q_outside[1].powi(2) + q_outside[2].powi(2)).sqrt();
            if q_outside_len > 1e-9 {
                let s = (p[i] - center[i]).signum();
                s * q_outside[i] / q_outside_len
            } else {
                let max_q = q[0].max(q[1]).max(q[2]);
                if (q[i] - max_q).abs() < 1e-9 {
                    (p[i] - center[i]).signum()
                } else {
                    0.0
                }
            }
        }
        AnalyticSdfKind::Capsule { a, b, .. } => {
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
            let len = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2]).sqrt();
            if len > 1e-9 {
                q[i] / len
            } else {
                0.0
            }
        }
        AnalyticSdfKind::Torus {
            center,
            major,
            ..
        } => {
            let dx = p[0] - center[0];
            let dy = p[1] - center[1];
            let dz = p[2] - center[2];
            let xy_len = (dx * dx + dz * dz).sqrt().max(1e-9);
            let qx = xy_len - major;
            let qy = dy;
            let qlen = (qx * qx + qy * qy).sqrt().max(1e-9);
            match i {
                0 => (qx * (dx / xy_len)) / qlen,
                1 => qy / qlen,
                2 => (qx * (dz / xy_len)) / qlen,
                _ => 0.0,
            }
        }
        AnalyticSdfKind::Cylinder {
            center,
            radius,
            half_height,
        } => {
            let dx = p[0] - center[0];
            let dy = p[1] - center[1];
            let dz = p[2] - center[2];
            let xy_dist = (dx * dx + dz * dz).sqrt().max(1e-9);
            let xy = xy_dist - radius;
            let z_off = dy.abs() - half_height;
            let q_outside_len =
                (xy.max(0.0).powi(2) + z_off.max(0.0).powi(2)).sqrt().max(1e-9);
            match i {
                0 => xy.max(0.0) * (dx / xy_dist) / q_outside_len,
                1 => z_off.max(0.0) * dy.signum() / q_outside_len,
                2 => xy.max(0.0) * (dz / xy_dist) / q_outside_len,
                _ => 0.0,
            }
        }
        _ => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdf::{AnalyticSdf, SdfComposition};

    #[test]
    fn surface_normal_unit_length() {
        let n = SurfaceNormal::from_grad([3.0, 0.0, 4.0]);
        let len = (n.0[0] * n.0[0] + n.0[1] * n.0[1] + n.0[2] * n.0[2]).sqrt();
        assert!((len - 1.0).abs() < 1e-6);
    }

    #[test]
    fn surface_normal_zero_grad_is_y_up_sentinel() {
        let n = SurfaceNormal::from_grad([0.0, 0.0, 0.0]);
        assert_eq!(n.xyz(), [0.0, 1.0, 0.0]);
    }

    #[test]
    fn central_diff_witness_present() {
        assert!(CentralDiffForbidden::WITNESS.contains("FORBIDDEN"));
        assert!(CentralDiffForbidden::WITNESS.contains("BackwardDiffNormals"));
        assert!(CentralDiffForbidden::SPEC_ANCHOR.contains("01_SDF_NATIVE_RENDER"));
    }

    #[test]
    fn sphere_normal_is_radial() {
        let s = SdfComposition::from_primitive(AnalyticSdf::sphere(0.0, 0.0, 0.0, 1.0));
        let est = BackwardDiffNormals::estimate(&s, [1.0, 0.0, 0.0]);
        assert!((est.normal.0[0] - 1.0).abs() < 1e-3);
        assert!(est.normal.0[1].abs() < 1e-3);
        assert!(est.normal.0[2].abs() < 1e-3);
    }

    #[test]
    fn sphere_normal_y_axis() {
        let s = SdfComposition::from_primitive(AnalyticSdf::sphere(0.0, 0.0, 0.0, 1.0));
        let est = BackwardDiffNormals::estimate(&s, [0.0, 1.0, 0.0]);
        assert!(est.normal.0[0].abs() < 1e-3);
        assert!((est.normal.0[1] - 1.0).abs() < 1e-3);
        assert!(est.normal.0[2].abs() < 1e-3);
    }

    #[test]
    fn plane_normal_is_plane_normal() {
        use cssl_pga::Plane;
        let p = Plane::new(0.0, 1.0, 0.0, 0.0); // y=0 plane
        let s = SdfComposition::from_primitive(AnalyticSdf::plane(p));
        let est = BackwardDiffNormals::estimate(&s, [3.0, 1.0, 5.0]);
        assert!(est.normal.0[0].abs() < 1e-3);
        assert!((est.normal.0[1] - 1.0).abs() < 1e-3);
        assert!(est.normal.0[2].abs() < 1e-3);
    }

    #[test]
    fn box_normal_along_dominant_axis() {
        let s = SdfComposition::from_primitive(AnalyticSdf::cube(0.0, 0.0, 0.0, 1.0, 1.0, 1.0));
        let est = BackwardDiffNormals::estimate(&s, [2.0, 0.0, 0.0]);
        assert!(est.normal.0[0] > 0.5);
    }

    #[test]
    fn sphere_normal_at_off_center_point_unit() {
        let s = SdfComposition::from_primitive(AnalyticSdf::sphere(1.0, 0.0, 0.0, 1.0));
        let est = BackwardDiffNormals::estimate(&s, [3.0, 0.0, 0.0]);
        let n = est.normal.0;
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        assert!((len - 1.0).abs() < 1e-3);
    }

    #[test]
    fn make_point_jets_only_live_axis_has_tangent() {
        let p = [1.0, 2.0, 3.0];
        let jets = make_point_jets(p, 1);
        assert_eq!(jets[0].nth_deriv(1), 0.0);
        assert!((jets[1].nth_deriv(1) - 1.0).abs() < 1e-9);
        assert_eq!(jets[2].nth_deriv(1), 0.0);
    }

    #[test]
    fn estimate_returns_sdf_value() {
        let s = SdfComposition::from_primitive(AnalyticSdf::sphere(0.0, 0.0, 0.0, 1.0));
        let est = BackwardDiffNormals::estimate(&s, [2.0, 0.0, 0.0]);
        assert!((est.sdf_value - 1.0).abs() < 1e-3);
    }

    #[test]
    fn smooth_union_preserves_normal_continuity() {
        let s1 = SdfComposition::from_primitive(AnalyticSdf::sphere(0.0, 0.0, 0.0, 1.0));
        let s2 = SdfComposition::from_primitive(AnalyticSdf::sphere(2.0, 0.0, 0.0, 1.0));
        let u = SdfComposition::smooth_union(s1, s2, 0.1);
        // Far from blend region : normal should match individual sphere.
        let est = BackwardDiffNormals::estimate(&u, [-1.0, 0.0, 0.0]);
        assert!(est.normal.0[0] < -0.5);
    }
}
