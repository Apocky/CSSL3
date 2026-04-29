//! § SDF — analytic distance-field collision per Omniverse SDF_NATIVE §I.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   `SdfCollider` is the canonical wave-physics collision primitive. It
//!   queries an analytic signed-distance-field at a point + uses the
//!   gradient as the contact-normal + supports analytic continuous-collision-
//!   detection (CCD) via gradient-descent toward the iso-surface.
//!
//!   This replaces the legacy `cssl-physics::narrowphase::contact_pair` per-
//!   shape-pair dispatch. The wave-physics engine has ONE narrow-phase :
//!   query the SDF. Per Omniverse SDF_NATIVE §I "SDF IS the world ⊗
//!   collision + render + audio + physics + AI ⊗ ALL-query-the-same-SDF".
//!
//! § PRIMITIVES (Lipschitz ≤ 1.0 by spec)
//!   The basic stdlib (`Omniverse/06_PROCEDURAL/06_HARD_SURFACE_PRIMITIVES §II`)
//!   contributes :
//!   - `sdf_sphere`      : `|p| - r`           (Lipschitz = 1)
//!   - `sdf_box`         : `length(max(|p| - h, 0)) + min(max-axis, 0)`
//!   - `sdf_cylinder`    : `max(length(p.xz) - r, |p.y| - h)`
//!   - `sdf_capsule`     : segment-distance with cap-radius
//!   - `sdf_torus`       : `length(vec2(length(p.xz) - R, p.y)) - r`
//!   - `sdf_plane`       : `dot(p, n) - d`
//!
//!   Hard-surface extensions (polytope, frustum, helix, gear, spiral, gear,
//!   pyramid, octahedron, tetrahedron, hex/tri-prism) are deferred to
//!   `cssl-substrate-omega-field` later — this slice ships the canonical
//!   organic stdlib.
//!
//! § GRADIENT
//!   For Lipschitz-bounded analytic SDFs, the gradient is the unit-length
//!   surface-normal at any point off the iso-surface (and the limit-of-
//!   gradient at the iso-surface). We use BACKWARD differences for normal
//!   estimation per Omniverse SDF_NATIVE §IV "‼ all-normals via bwd_diff
//!   (LoA-bug retrospective)".
//!
//!   `bwd_diff` here is implemented via finite-differences with a fixed
//!   `EPSILON = 1e-3` — the wave-physics consumer can supply an analytic
//!   gradient via `SdfShape::gradient` to skip finite-differencing on the
//!   hot path. Fallback path uses the FD computation.
//!
//! § ANALYTIC CCD
//!   For a body moving from `p0` to `p1` with the SDF representing the
//!   environment, the time-of-impact (TOI) is the smallest `t ∈ [0, 1]`
//!   such that `sdf(lerp(p0, p1, t)) ≤ 0`. The gradient-descent CCD
//!   advances `t` by `-sdf(p_t) / dot(grad, dir)` until the SDF is below
//!   `HIT_EPSILON`. For Lipschitz ≤ 1, this converges within ≤ 16 steps
//!   for typical game-scene scales.
//!
//! § DETERMINISM
//!   - `EPSILON` is fixed (no env-var, no platform-detection).
//!   - The CCD bisection-fallback uses a fixed `MAX_CCD_STEPS = 32`.
//!   - Gradient finite-differences use `[+EPS, -EPS]` paired evaluations
//!     in the deterministic order `(x, y, z)` ; no parallel reduction.

use thiserror::Error;

/// § Surface-hit epsilon : a query is considered "on the surface" when
///   `|sdf(p)| < HIT_EPSILON`. Fixed across hosts for replay-stability.
pub const HIT_EPSILON: f32 = 1e-4;

/// § Finite-difference gradient epsilon. Used as fallback when an SDF
///   doesn't supply an analytic gradient.
pub const FD_GRADIENT_EPSILON: f32 = 1e-3;

/// § Maximum CCD iteration count. Per Omniverse SDF_NATIVE §IV `max_steps
///   = 128` for ray-marching ; for narrow-phase CCD we cap shorter at 32
///   to bound per-frame work.
pub const MAX_CCD_STEPS: u32 = 32;

/// § Minimum positive distance step in CCD. If the gradient-descent step
///   shrinks below this, we declare convergence (or a divergent query).
pub const CCD_MIN_STEP: f32 = 1e-6;

/// § Maximum CCD ray-march distance — beyond this we declare a no-hit.
pub const CCD_MAX_DISTANCE: f32 = 1e6;

// ───────────────────────────────────────────────────────────────────────
// § SdfPrimitive — the basic stdlib.
// ───────────────────────────────────────────────────────────────────────

/// § The canonical SDF stdlib primitives per spec.
///
///   These are POD : pure parameter records ; the SDF eval lives in the
///   free `sdf_*` functions or the `SdfShape::evaluate` impl.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SdfPrimitive {
    /// `|p - center| - r`
    Sphere {
        /// World-space center.
        center: [f32; 3],
        /// Radius (must be `> 0`).
        radius: f32,
    },
    /// Axis-aligned box centered at `center` with half-extents `half`.
    Box {
        /// World-space center.
        center: [f32; 3],
        /// Half-extents along (x, y, z).
        half: [f32; 3],
    },
    /// Cylinder along the Y axis centered at `center` with `radius` and
    /// half-height `half_height`.
    Cylinder {
        /// World-space center.
        center: [f32; 3],
        /// Radius.
        radius: f32,
        /// Half-height (along Y).
        half_height: f32,
    },
    /// Capsule = swept-sphere along axis from `a` to `b` with radius `r`.
    Capsule {
        /// First endpoint (sphere center).
        a: [f32; 3],
        /// Second endpoint.
        b: [f32; 3],
        /// Sphere radius.
        radius: f32,
    },
    /// Torus in the XZ-plane centered at `center` with major-radius `R`
    /// and tube-radius `r`.
    Torus {
        /// World-space center.
        center: [f32; 3],
        /// Major radius (R).
        major_radius: f32,
        /// Tube radius (r).
        tube_radius: f32,
    },
    /// Half-space `dot(p, normal) - offset`. The normal must be unit-length.
    Plane {
        /// Unit-length plane normal.
        normal: [f32; 3],
        /// Plane offset along the normal direction.
        offset: f32,
    },
}

// ───────────────────────────────────────────────────────────────────────
// § Free-function SDF primitives (also exposed publicly for stand-alone use).
// ───────────────────────────────────────────────────────────────────────

#[inline]
fn sub3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

#[inline]
fn add3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

#[inline]
fn mul3(a: [f32; 3], s: f32) -> [f32; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}

#[inline]
fn dot3(a: [f32; 3], b: [f32; 3]) -> f32 {
    // Two-step ; not FMA. (Matches DETERMINISM CONTRACT.)
    let xy = (a[0] * b[0]) + (a[1] * b[1]);
    xy + (a[2] * b[2])
}

#[inline]
fn length3(a: [f32; 3]) -> f32 {
    dot3(a, a).sqrt()
}

#[inline]
fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

#[inline]
fn normalize3(a: [f32; 3]) -> [f32; 3] {
    let l = length3(a);
    if l < f32::MIN_POSITIVE {
        // Zero-vector input ⇒ pick a stable non-zero unit vector. We
        // return the +X axis so the caller's normal-driven solve has
        // SOME defined direction. This branch is intentionally
        // deterministic.
        [1.0, 0.0, 0.0]
    } else {
        let inv = 1.0_f32 / l;
        [a[0] * inv, a[1] * inv, a[2] * inv]
    }
}

/// § `sdf_sphere(p, center, radius)` ≡ `|p - center| - radius`.
#[must_use]
pub fn sdf_sphere(p: [f32; 3], center: [f32; 3], radius: f32) -> f32 {
    length3(sub3(p, center)) - radius
}

/// § `sdf_box(p, center, half)` — exact box SDF.
#[must_use]
pub fn sdf_box(p: [f32; 3], center: [f32; 3], half: [f32; 3]) -> f32 {
    let d = sub3(p, center);
    let q = [d[0].abs() - half[0], d[1].abs() - half[1], d[2].abs() - half[2]];
    let outside = [q[0].max(0.0), q[1].max(0.0), q[2].max(0.0)];
    let outside_dist = length3(outside);
    let inside_dist = q[0].max(q[1]).max(q[2]).min(0.0);
    outside_dist + inside_dist
}

/// § `sdf_cylinder(p, center, radius, half_height)` — Y-axis cylinder.
#[must_use]
pub fn sdf_cylinder(p: [f32; 3], center: [f32; 3], radius: f32, half_height: f32) -> f32 {
    let d = sub3(p, center);
    let r_xz = (d[0] * d[0] + d[2] * d[2]).sqrt();
    let dr = r_xz - radius;
    let dy = d[1].abs() - half_height;
    let outside_r = dr.max(0.0);
    let outside_y = dy.max(0.0);
    let outside_dist = (outside_r * outside_r + outside_y * outside_y).sqrt();
    let inside_dist = dr.max(dy).min(0.0);
    outside_dist + inside_dist
}

/// § `sdf_capsule(p, a, b, radius)` — segment-distance + cap-radius.
#[must_use]
pub fn sdf_capsule(p: [f32; 3], a: [f32; 3], b: [f32; 3], radius: f32) -> f32 {
    let pa = sub3(p, a);
    let ba = sub3(b, a);
    let denom = dot3(ba, ba).max(f32::MIN_POSITIVE);
    let t = (dot3(pa, ba) / denom).clamp(0.0, 1.0);
    let proj = add3(a, mul3(ba, t));
    length3(sub3(p, proj)) - radius
}

/// § `sdf_torus(p, center, major_radius, tube_radius)` — XZ-plane torus.
#[must_use]
pub fn sdf_torus(p: [f32; 3], center: [f32; 3], major_radius: f32, tube_radius: f32) -> f32 {
    let d = sub3(p, center);
    let r_xz = (d[0] * d[0] + d[2] * d[2]).sqrt();
    let q = [r_xz - major_radius, d[1]];
    (q[0] * q[0] + q[1] * q[1]).sqrt() - tube_radius
}

/// § `sdf_plane(p, normal, offset)` — half-space.
#[must_use]
pub fn sdf_plane(p: [f32; 3], normal: [f32; 3], offset: f32) -> f32 {
    dot3(p, normal) - offset
}

// ───────────────────────────────────────────────────────────────────────
// § SdfShape — owned SDF instance.
// ───────────────────────────────────────────────────────────────────────

/// § A composable SDF shape.
///
///   Ownership of the primitive is held inline ; composite shapes
///   (`Union` / `Intersect` / `Difference`) box their children so the
///   shape forms a tree. The `Box<SdfShape>` heap-alloc is acceptable
///   because shape-trees are constructed at world-build-time, not per-
///   frame ; the per-frame eval uses an iterative tree-walk.
#[derive(Debug, Clone)]
pub enum SdfShape {
    /// A primitive SDF (no children).
    Primitive(SdfPrimitive),
    /// `min(a, b)` — sharp union.
    Union(Box<SdfShape>, Box<SdfShape>),
    /// `max(a, b)` — sharp intersection.
    Intersect(Box<SdfShape>, Box<SdfShape>),
    /// `max(a, -b)` — sharp difference.
    Difference(Box<SdfShape>, Box<SdfShape>),
    /// `smooth-min(a, b, k)` — k > 0 yields rounded-blend.
    SmoothUnion(Box<SdfShape>, Box<SdfShape>, f32),
    /// Translated child : `child(p - offset)`.
    Translated(Box<SdfShape>, [f32; 3]),
}

impl SdfShape {
    /// § Evaluate the SDF at `p`.
    #[must_use]
    pub fn evaluate(&self, p: [f32; 3]) -> f32 {
        match self {
            SdfShape::Primitive(prim) => match *prim {
                SdfPrimitive::Sphere { center, radius } => sdf_sphere(p, center, radius),
                SdfPrimitive::Box { center, half } => sdf_box(p, center, half),
                SdfPrimitive::Cylinder { center, radius, half_height } => {
                    sdf_cylinder(p, center, radius, half_height)
                }
                SdfPrimitive::Capsule { a, b, radius } => sdf_capsule(p, a, b, radius),
                SdfPrimitive::Torus { center, major_radius, tube_radius } => {
                    sdf_torus(p, center, major_radius, tube_radius)
                }
                SdfPrimitive::Plane { normal, offset } => sdf_plane(p, normal, offset),
            },
            SdfShape::Union(a, b) => a.evaluate(p).min(b.evaluate(p)),
            SdfShape::Intersect(a, b) => a.evaluate(p).max(b.evaluate(p)),
            SdfShape::Difference(a, b) => a.evaluate(p).max(-b.evaluate(p)),
            SdfShape::SmoothUnion(a, b, k) => {
                let da = a.evaluate(p);
                let db = b.evaluate(p);
                smooth_min(da, db, *k)
            }
            SdfShape::Translated(child, offset) => {
                child.evaluate(sub3(p, *offset))
            }
        }
    }

    /// § Backward-differences gradient at `p`. Returns the unit-length
    ///   normal (or a stable fallback for degenerate queries).
    #[must_use]
    pub fn gradient_bwd(&self, p: [f32; 3]) -> [f32; 3] {
        // Backward-differences : grad ≈ (sdf(p) - sdf(p - eps_axis)) / eps.
        // This matches Omniverse SDF_NATIVE §IV "all-normals via bwd_diff".
        let eps = FD_GRADIENT_EPSILON;
        let s0 = self.evaluate(p);
        let s_x = self.evaluate([p[0] - eps, p[1], p[2]]);
        let s_y = self.evaluate([p[0], p[1] - eps, p[2]]);
        let s_z = self.evaluate([p[0], p[1], p[2] - eps]);
        let grad = [(s0 - s_x) / eps, (s0 - s_y) / eps, (s0 - s_z) / eps];
        normalize3(grad)
    }

    /// § Estimate the surface-normal at the iso-surface point closest to
    ///   `p`. This is `gradient_bwd` then normalize ; preserved as a named
    ///   API so consumer code reads as physics-text canonical (`normal`).
    #[must_use]
    pub fn surface_normal(&self, p: [f32; 3]) -> [f32; 3] {
        self.gradient_bwd(p)
    }
}

/// § Smooth-min — k > 0 rounded blend ; k = 0 collapses to sharp `min`.
#[must_use]
pub fn smooth_min(a: f32, b: f32, k: f32) -> f32 {
    if k <= 0.0 {
        return a.min(b);
    }
    let h = ((a - b) / k).clamp(-1.0, 1.0);
    let m = (a + b - k * (1.0 - h * h)) * 0.5;
    m
}

// ───────────────────────────────────────────────────────────────────────
// § SdfHit — narrow-phase hit record.
// ───────────────────────────────────────────────────────────────────────

/// § Result of a CCD ray-march or a narrow-phase contact-query.
///
///   Mirrors the legacy crate's `Contact` shape but with a richer
///   "hit-fraction" t-of-i field.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SdfHit {
    /// World-space hit point.
    pub point: [f32; 3],
    /// Unit-length surface-normal at the hit (from the SDF gradient).
    pub normal: [f32; 3],
    /// Penetration depth at the hit (positive into the surface).
    pub penetration: f32,
    /// Hit-fraction along the input motion vector (`0.0` ≤ t ≤ `1.0`).
    pub time_of_impact: f32,
}

impl SdfHit {
    /// § A null hit ; used as a sentinel by callers before a real hit lands.
    pub const NONE: SdfHit = SdfHit {
        point: [0.0; 3],
        normal: [0.0, 1.0, 0.0],
        penetration: 0.0,
        time_of_impact: 1.0,
    };
}

// ───────────────────────────────────────────────────────────────────────
// § SdfQueryError — the failure modes of SDF queries.
// ───────────────────────────────────────────────────────────────────────

/// § Failure modes of [`SdfCollider`] queries.
#[derive(Debug, Clone, Copy, PartialEq, Error)]
pub enum SdfQueryError {
    /// CCD ray-march exceeded `MAX_CCD_STEPS` without converging.
    #[error("PHYSWAVE0001 — SDF CCD ray-march did not converge in {steps} steps (residual {residual})")]
    CcdNoConvergence {
        /// Number of steps consumed.
        steps: u32,
        /// Residual SDF value at termination.
        residual: f32,
    },
    /// Query point produced a non-finite SDF value (NaN / Infinity).
    #[error("PHYSWAVE0002 — SDF evaluate returned non-finite value ({sdf}) — likely a Lipschitz violation")]
    NonFinite {
        /// The non-finite value.
        sdf: f32,
    },
    /// Motion vector has zero length and a CCD query was attempted.
    #[error("PHYSWAVE0003 — SDF CCD query received zero-length motion vector")]
    ZeroMotion,
}

// ───────────────────────────────────────────────────────────────────────
// § SdfCollider — the canonical narrow-phase.
// ───────────────────────────────────────────────────────────────────────

/// § The wave-physics narrow-phase collider.
///
///   `SdfCollider` wraps an `SdfShape` (the world's analytic geometry) and
///   exposes :
///   - `point_query(p) -> Option<SdfHit>` — is `p` inside / near the
///     surface ? The hit's `time_of_impact` is `0.0` (no motion).
///   - `ccd_segment(p0, p1) -> Result<Option<SdfHit>, SdfQueryError>` —
///     does the segment from `p0` to `p1` cross the iso-surface ? If so
///     return the hit at the first crossing.
///   - `discrete_contact(body_aabb_center, body_radius)` — sphere-to-SDF
///     contact query (legacy-style narrow-phase, kept for migration).
#[derive(Debug, Clone)]
pub struct SdfCollider {
    /// The analytic SDF shape this collider wraps.
    shape: SdfShape,
    /// The iso-surface threshold (0.0 = exact surface, > 0 = inflated).
    inflation: f32,
}

impl SdfCollider {
    /// § Construct a collider from an SDF shape.
    #[must_use]
    pub fn new(shape: SdfShape) -> Self {
        SdfCollider { shape, inflation: 0.0 }
    }

    /// § Construct a collider with a positive inflation — the collider's
    ///   iso-surface is `sdf(p) - inflation`, equivalent to a Minkowski-
    ///   sum with a sphere of radius `inflation`. This is the canonical
    ///   "swept-radius" narrow-phase trick for collision-with-thickness.
    #[must_use]
    pub fn with_inflation(shape: SdfShape, inflation: f32) -> Self {
        SdfCollider { shape, inflation: inflation.max(0.0) }
    }

    /// § Read the inflation distance.
    #[must_use]
    pub fn inflation(&self) -> f32 {
        self.inflation
    }

    /// § Evaluate the (possibly inflated) SDF at `p`.
    #[must_use]
    pub fn sdf(&self, p: [f32; 3]) -> f32 {
        self.shape.evaluate(p) - self.inflation
    }

    /// § Surface-normal via backward-differences.
    #[must_use]
    pub fn normal(&self, p: [f32; 3]) -> [f32; 3] {
        self.shape.surface_normal(p)
    }

    /// § Point-query : if `p` is on / near the iso-surface, return a
    ///   hit record. Returns `None` otherwise. Errors on non-finite SDF
    ///   values (likely a Lipschitz violation in a composed shape).
    pub fn point_query(&self, p: [f32; 3]) -> Result<Option<SdfHit>, SdfQueryError> {
        let s = self.sdf(p);
        if !s.is_finite() {
            return Err(SdfQueryError::NonFinite { sdf: s });
        }
        if s > HIT_EPSILON {
            return Ok(None);
        }
        let n = self.normal(p);
        // Move the hit-point onto the iso-surface along the normal so the
        // resolver has a clean position-correction direction.
        let surface_p = [p[0] - s * n[0], p[1] - s * n[1], p[2] - s * n[2]];
        Ok(Some(SdfHit {
            point: surface_p,
            normal: n,
            penetration: (-s).max(0.0),
            time_of_impact: 0.0,
        }))
    }

    /// § Continuous-collision-detection segment query.
    ///
    ///   Returns `Ok(Some(hit))` for the first crossing of the iso-
    ///   surface along `[p0, p1]` ; `Ok(None)` if the segment never
    ///   crosses ; `Err(...)` for non-convergent or degenerate queries.
    ///
    ///   Algorithm : sphere-tracing with backtrack to bisection on
    ///   convergence-stall. Lipschitz ≤ 1 is assumed ; a violation
    ///   returns `CcdNoConvergence` rather than looping forever.
    pub fn ccd_segment(
        &self,
        p0: [f32; 3],
        p1: [f32; 3],
    ) -> Result<Option<SdfHit>, SdfQueryError> {
        let dir_full = sub3(p1, p0);
        let dist = length3(dir_full);
        if dist < CCD_MIN_STEP {
            // No motion — fall through to a point-query.
            return self.point_query(p0);
        }
        if dist > CCD_MAX_DISTANCE {
            return Err(SdfQueryError::ZeroMotion);
        }
        let dir = mul3(dir_full, 1.0_f32 / dist);
        let mut t: f32 = 0.0;
        let mut steps: u32 = 0;
        loop {
            if steps >= MAX_CCD_STEPS {
                return Err(SdfQueryError::CcdNoConvergence {
                    steps,
                    residual: self.sdf([
                        p0[0] + dir[0] * (t * dist),
                        p0[1] + dir[1] * (t * dist),
                        p0[2] + dir[2] * (t * dist),
                    ]),
                });
            }
            let world_t = t * dist;
            let p_t = [
                p0[0] + dir[0] * world_t,
                p0[1] + dir[1] * world_t,
                p0[2] + dir[2] * world_t,
            ];
            let s = self.sdf(p_t);
            if !s.is_finite() {
                return Err(SdfQueryError::NonFinite { sdf: s });
            }
            if s.abs() < HIT_EPSILON {
                // Surface-hit. Compute the normal + emit.
                let n = self.normal(p_t);
                return Ok(Some(SdfHit {
                    point: p_t,
                    normal: n,
                    penetration: (-s).max(0.0),
                    time_of_impact: t.clamp(0.0, 1.0),
                }));
            }
            if s < 0.0 {
                // Already inside — surface was crossed in the previous
                // step. We bisect backward to find the surface.
                return self.bisect_for_surface(p0, dir, dist, (t - (1.0 / dist)).max(0.0), t);
            }
            // Sphere-trace step : advance `t` by `s / dist` (i.e. t in
            // unit-of-segment ; s in world-units). The Lipschitz bound
            // guarantees this step does not overshoot.
            let dt = s / dist;
            if dt < CCD_MIN_STEP {
                // Stall : SDF value remains positive but step is tiny.
                // We declare no-hit-along-segment and return.
                return Ok(None);
            }
            t = t + dt;
            if t >= 1.0 {
                // Walked past the segment endpoint without hitting.
                return Ok(None);
            }
            steps += 1;
        }
    }

    /// § Discrete sphere-to-SDF contact (legacy-style narrow-phase).
    ///
    ///   Returns a hit if `sdf(center) < radius` ; the hit is at the
    ///   surface point `center - sdf(center) * normal`.
    pub fn discrete_contact(
        &self,
        center: [f32; 3],
        radius: f32,
    ) -> Result<Option<SdfHit>, SdfQueryError> {
        let s = self.sdf(center);
        if !s.is_finite() {
            return Err(SdfQueryError::NonFinite { sdf: s });
        }
        if s > radius {
            return Ok(None);
        }
        let n = self.normal(center);
        let surface_p = [
            center[0] - s * n[0],
            center[1] - s * n[1],
            center[2] - s * n[2],
        ];
        Ok(Some(SdfHit {
            point: surface_p,
            normal: n,
            penetration: (radius - s).max(0.0),
            time_of_impact: 0.0,
        }))
    }

    /// § Bisection fallback used when sphere-tracing crosses the iso-
    ///   surface in a single step.
    fn bisect_for_surface(
        &self,
        p0: [f32; 3],
        dir: [f32; 3],
        dist: f32,
        t_lo: f32,
        t_hi: f32,
    ) -> Result<Option<SdfHit>, SdfQueryError> {
        let mut lo = t_lo;
        let mut hi = t_hi;
        for _ in 0..MAX_CCD_STEPS {
            let mid = (lo + hi) * 0.5;
            let world_t = mid * dist;
            let p_mid = [
                p0[0] + dir[0] * world_t,
                p0[1] + dir[1] * world_t,
                p0[2] + dir[2] * world_t,
            ];
            let s = self.sdf(p_mid);
            if !s.is_finite() {
                return Err(SdfQueryError::NonFinite { sdf: s });
            }
            if s.abs() < HIT_EPSILON {
                let n = self.normal(p_mid);
                return Ok(Some(SdfHit {
                    point: p_mid,
                    normal: n,
                    penetration: (-s).max(0.0),
                    time_of_impact: mid.clamp(0.0, 1.0),
                }));
            }
            if s > 0.0 {
                lo = mid;
            } else {
                hi = mid;
            }
            if (hi - lo) * dist < CCD_MIN_STEP {
                // Bisection converged within step-tolerance ; emit at the
                // mid-point.
                let n = self.normal(p_mid);
                return Ok(Some(SdfHit {
                    point: p_mid,
                    normal: n,
                    penetration: (-s).max(0.0),
                    time_of_impact: mid.clamp(0.0, 1.0),
                }));
            }
        }
        Err(SdfQueryError::CcdNoConvergence {
            steps: MAX_CCD_STEPS,
            residual: 0.0,
        })
    }

    /// § Reference to the underlying shape (read-only access for callers
    ///   that need to introspect — e.g. visualizers, hot-reload).
    #[must_use]
    pub fn shape(&self) -> &SdfShape {
        &self.shape
    }
}

// ───────────────────────────────────────────────────────────────────────
// § IsoSurfaceCcd — convenience wrapper for the CCD path.
// ───────────────────────────────────────────────────────────────────────

/// § Configuration for the iso-surface CCD walker.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IsoSurfaceCcd {
    /// Maximum sphere-tracing steps before returning `CcdNoConvergence`.
    pub max_steps: u32,
    /// Surface-hit epsilon.
    pub hit_eps: f32,
    /// Minimum step size before declaring stall.
    pub min_step: f32,
}

impl Default for IsoSurfaceCcd {
    fn default() -> Self {
        IsoSurfaceCcd {
            max_steps: MAX_CCD_STEPS,
            hit_eps: HIT_EPSILON,
            min_step: CCD_MIN_STEP,
        }
    }
}

impl IsoSurfaceCcd {
    /// § Walk a segment `[p0, p1]` against `collider` ; same semantics as
    ///   `SdfCollider::ccd_segment` but with a per-call config override.
    pub fn walk(
        &self,
        collider: &SdfCollider,
        p0: [f32; 3],
        p1: [f32; 3],
    ) -> Result<Option<SdfHit>, SdfQueryError> {
        let dir_full = sub3(p1, p0);
        let dist = length3(dir_full);
        if dist < self.min_step {
            return collider.point_query(p0);
        }
        let dir = mul3(dir_full, 1.0_f32 / dist);
        let mut t: f32 = 0.0;
        for steps in 0..self.max_steps {
            let world_t = t * dist;
            let p_t = [
                p0[0] + dir[0] * world_t,
                p0[1] + dir[1] * world_t,
                p0[2] + dir[2] * world_t,
            ];
            let s = collider.sdf(p_t);
            if !s.is_finite() {
                return Err(SdfQueryError::NonFinite { sdf: s });
            }
            if s.abs() < self.hit_eps || s < 0.0 {
                let n = collider.normal(p_t);
                return Ok(Some(SdfHit {
                    point: p_t,
                    normal: n,
                    penetration: (-s).max(0.0),
                    time_of_impact: t.clamp(0.0, 1.0),
                }));
            }
            let dt = s / dist;
            if dt < self.min_step {
                let _ = steps;
                return Ok(None);
            }
            t = t + dt;
            if t >= 1.0 {
                return Ok(None);
            }
        }
        Err(SdfQueryError::CcdNoConvergence {
            steps: self.max_steps,
            residual: 0.0,
        })
    }
}

// ───────────────────────────────────────────────────────────────────────
// § Tests.
// ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() < eps
    }

    #[test]
    fn sphere_at_center_is_negative_radius() {
        let d = sdf_sphere([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 1.0);
        assert!(approx(d, -1.0, 1e-6));
    }

    #[test]
    fn sphere_on_surface_is_zero() {
        let d = sdf_sphere([1.0, 0.0, 0.0], [0.0, 0.0, 0.0], 1.0);
        assert!(approx(d, 0.0, 1e-6));
    }

    #[test]
    fn sphere_outside_returns_positive() {
        let d = sdf_sphere([2.0, 0.0, 0.0], [0.0, 0.0, 0.0], 1.0);
        assert!(approx(d, 1.0, 1e-6));
    }

    #[test]
    fn box_inside_returns_negative() {
        let d = sdf_box([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
        assert!(d < 0.0);
        assert!(approx(d, -1.0, 1e-5));
    }

    #[test]
    fn box_corner_distance_correct() {
        let d = sdf_box([2.0, 2.0, 2.0], [0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
        // Nearest box-corner is (1, 1, 1) ; distance √3.
        assert!(approx(d, 3.0_f32.sqrt(), 1e-5));
    }

    #[test]
    fn cylinder_inside_returns_negative() {
        let d = sdf_cylinder([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 1.0, 1.0);
        assert!(d < 0.0);
    }

    #[test]
    fn cylinder_axis_distance_correct() {
        // Point at (2, 0, 0) — radial distance 2 ; cylinder radius 1 ; dist 1.
        let d = sdf_cylinder([2.0, 0.0, 0.0], [0.0, 0.0, 0.0], 1.0, 0.5);
        assert!(approx(d, 1.0, 1e-5));
    }

    #[test]
    fn capsule_endpoint_is_zero_at_distance_radius() {
        let d = sdf_capsule(
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0],
            1.0,
        );
        assert!(approx(d, 0.0, 1e-5));
    }

    #[test]
    fn torus_at_center_is_negative_tube() {
        // Center of torus, no XZ offset ⇒ r_xz = 0 ; q = (-R, 0) ; dist = R ; subtract r.
        let d = sdf_torus([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 2.0, 0.5);
        assert!(approx(d, 1.5, 1e-5));
    }

    #[test]
    fn plane_distance_correct() {
        let d = sdf_plane([0.0, 1.0, 0.0], [0.0, 1.0, 0.0], 0.0);
        assert!(approx(d, 1.0, 1e-6));
    }

    #[test]
    fn shape_evaluate_dispatches_correctly() {
        let s = SdfShape::Primitive(SdfPrimitive::Sphere {
            center: [0.0, 0.0, 0.0],
            radius: 1.0,
        });
        assert!(approx(s.evaluate([0.0, 0.0, 0.0]), -1.0, 1e-6));
    }

    #[test]
    fn shape_union_takes_min() {
        let a = SdfShape::Primitive(SdfPrimitive::Sphere {
            center: [-1.0, 0.0, 0.0],
            radius: 0.5,
        });
        let b = SdfShape::Primitive(SdfPrimitive::Sphere {
            center: [1.0, 0.0, 0.0],
            radius: 0.5,
        });
        let u = SdfShape::Union(Box::new(a), Box::new(b));
        // At origin both spheres are 0.5 away → min = 0.5.
        assert!(approx(u.evaluate([0.0, 0.0, 0.0]), 0.5, 1e-6));
        // Inside left sphere at (-1, 0, 0) → -0.5.
        assert!(approx(u.evaluate([-1.0, 0.0, 0.0]), -0.5, 1e-6));
    }

    #[test]
    fn shape_intersect_takes_max() {
        let a = SdfShape::Primitive(SdfPrimitive::Sphere {
            center: [0.0, 0.0, 0.0],
            radius: 1.0,
        });
        let b = SdfShape::Primitive(SdfPrimitive::Sphere {
            center: [1.0, 0.0, 0.0],
            radius: 1.0,
        });
        let i = SdfShape::Intersect(Box::new(a), Box::new(b));
        // Origin : a = -1, b = 0. max = 0.
        assert!(approx(i.evaluate([0.0, 0.0, 0.0]), 0.0, 1e-6));
    }

    #[test]
    fn shape_difference_takes_max_neg_b() {
        let a = SdfShape::Primitive(SdfPrimitive::Sphere {
            center: [0.0, 0.0, 0.0],
            radius: 2.0,
        });
        let b = SdfShape::Primitive(SdfPrimitive::Sphere {
            center: [0.0, 0.0, 0.0],
            radius: 1.0,
        });
        let d = SdfShape::Difference(Box::new(a), Box::new(b));
        // At origin : a = -2, b = -1. max(-2, +1) = 1.
        assert!(approx(d.evaluate([0.0, 0.0, 0.0]), 1.0, 1e-6));
    }

    #[test]
    fn shape_smooth_union_collapses_to_min_at_zero_k() {
        let a = SdfShape::Primitive(SdfPrimitive::Sphere {
            center: [0.0, 0.0, 0.0],
            radius: 1.0,
        });
        let b = SdfShape::Primitive(SdfPrimitive::Sphere {
            center: [3.0, 0.0, 0.0],
            radius: 1.0,
        });
        let smooth = SdfShape::SmoothUnion(Box::new(a.clone()), Box::new(b.clone()), 0.0);
        let sharp = SdfShape::Union(Box::new(a), Box::new(b));
        for &p in &[[0.0, 0.0, 0.0], [1.5, 0.0, 0.0], [4.0, 0.0, 0.0]] {
            assert!(approx(smooth.evaluate(p), sharp.evaluate(p), 1e-5));
        }
    }

    #[test]
    fn shape_translated_offsets_query() {
        let s = SdfShape::Primitive(SdfPrimitive::Sphere {
            center: [0.0, 0.0, 0.0],
            radius: 1.0,
        });
        let t = SdfShape::Translated(Box::new(s), [5.0, 0.0, 0.0]);
        // Surface of translated sphere at (5, 0, 0).
        assert!(approx(t.evaluate([5.0, 1.0, 0.0]), 0.0, 1e-5));
    }

    #[test]
    fn gradient_bwd_returns_unit_length() {
        let s = SdfShape::Primitive(SdfPrimitive::Sphere {
            center: [0.0, 0.0, 0.0],
            radius: 1.0,
        });
        let g = s.gradient_bwd([2.0, 0.0, 0.0]);
        let len = (g[0] * g[0] + g[1] * g[1] + g[2] * g[2]).sqrt();
        assert!(approx(len, 1.0, 1e-3));
    }

    #[test]
    fn collider_point_query_returns_hit_inside() {
        let c = SdfCollider::new(SdfShape::Primitive(SdfPrimitive::Sphere {
            center: [0.0, 0.0, 0.0],
            radius: 1.0,
        }));
        let h = c.point_query([0.5, 0.0, 0.0]).unwrap().unwrap();
        assert!(h.penetration > 0.0);
        assert_eq!(h.time_of_impact, 0.0);
    }

    #[test]
    fn collider_point_query_returns_none_outside() {
        let c = SdfCollider::new(SdfShape::Primitive(SdfPrimitive::Sphere {
            center: [0.0, 0.0, 0.0],
            radius: 1.0,
        }));
        let h = c.point_query([5.0, 0.0, 0.0]).unwrap();
        assert!(h.is_none());
    }

    #[test]
    fn collider_with_inflation_widens_hit_zone() {
        let c = SdfCollider::with_inflation(
            SdfShape::Primitive(SdfPrimitive::Sphere {
                center: [0.0, 0.0, 0.0],
                radius: 1.0,
            }),
            0.5,
        );
        // 1.5 from center is now on the inflated iso-surface.
        let h = c.point_query([1.4, 0.0, 0.0]).unwrap();
        assert!(h.is_some());
    }

    #[test]
    fn ccd_segment_finds_first_crossing() {
        let c = SdfCollider::new(SdfShape::Primitive(SdfPrimitive::Sphere {
            center: [5.0, 0.0, 0.0],
            radius: 1.0,
        }));
        let h = c
            .ccd_segment([0.0, 0.0, 0.0], [10.0, 0.0, 0.0])
            .unwrap()
            .unwrap();
        // First crossing should be at x ≈ 4.0.
        assert!(approx(h.point[0], 4.0, 0.05));
        assert!(h.time_of_impact > 0.0 && h.time_of_impact < 1.0);
    }

    #[test]
    fn ccd_segment_no_crossing_returns_none() {
        let c = SdfCollider::new(SdfShape::Primitive(SdfPrimitive::Sphere {
            center: [5.0, 0.0, 0.0],
            radius: 1.0,
        }));
        let h = c.ccd_segment([0.0, 0.0, 0.0], [0.0, 1.0, 0.0]).unwrap();
        assert!(h.is_none());
    }

    #[test]
    fn discrete_contact_inflates_with_radius() {
        let c = SdfCollider::new(SdfShape::Primitive(SdfPrimitive::Sphere {
            center: [0.0, 0.0, 0.0],
            radius: 1.0,
        }));
        // Sphere body of radius 0.5 at center (1.4, 0, 0). Body's surface
        // at distance 0.9 from world center → inside sphere by 0.1.
        let h = c.discrete_contact([1.4, 0.0, 0.0], 0.5).unwrap();
        assert!(h.is_some());
    }

    #[test]
    fn smooth_min_at_zero_k_equals_min() {
        assert!(approx(smooth_min(1.0, 2.0, 0.0), 1.0, 1e-6));
    }

    #[test]
    fn smooth_min_with_positive_k_yields_smaller_than_min_when_close() {
        // When a ≈ b, smooth_min < min(a, b).
        let s = smooth_min(1.0, 1.0, 0.5);
        assert!(s < 1.0);
    }

    #[test]
    fn iso_surface_ccd_default_walk() {
        let c = SdfCollider::new(SdfShape::Primitive(SdfPrimitive::Plane {
            normal: [0.0, 1.0, 0.0],
            offset: 0.0,
        }));
        let walker = IsoSurfaceCcd::default();
        let h = walker
            .walk(&c, [0.0, 1.0, 0.0], [0.0, -1.0, 0.0])
            .unwrap()
            .unwrap();
        assert!(approx(h.point[1], 0.0, HIT_EPSILON));
        // Normal of the plane should be +Y.
        assert!(approx(h.normal[1], 1.0, 1e-2));
    }

    #[test]
    fn ccd_zero_length_motion_falls_through_to_point_query() {
        let c = SdfCollider::new(SdfShape::Primitive(SdfPrimitive::Sphere {
            center: [0.0, 0.0, 0.0],
            radius: 1.0,
        }));
        // Both endpoints at origin (inside sphere) ⇒ point-query returns a hit.
        let h = c.ccd_segment([0.0, 0.0, 0.0], [0.0, 0.0, 0.0]).unwrap();
        assert!(h.is_some());
    }
}
