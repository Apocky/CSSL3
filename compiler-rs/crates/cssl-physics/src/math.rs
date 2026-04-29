//! Math primitives — Vec3 / Mat3 / Quat
//!
//! § THESIS
//!   `cssl-physics` consumes M1 (`cssl-math`) when it merges to parallel-fanout.
//!   Until then we ship a SELF-CONTAINED minimal math layer here. Surface is
//!   intentionally narrow : just what the physics solver needs. When M1 lands,
//!   this module becomes a thin re-export shim ; the public types `Vec3`,
//!   `Mat3`, `Quat` keep their names and field layouts so downstream code
//!   doesn't break.
//!
//! § DETERMINISM
//!   - All ops are explicit `f64` — no f32 promotion paths.
//!   - No FMA : `(a*b)+c` is written as `a.mul(b).add(c)` style or as two
//!     statements. The compiler is NOT permitted to fuse via `-ffast-math` ;
//!     this is enforced at the workspace level by `RUSTFLAGS=-Cno-prefer-dynamic
//!     -Ctarget-feature=-fma` for physics builds (probed at `flush_denormals_to_zero`).
//!   - No transcendentals in the hot path — only `sqrt`, `abs`, `min`, `max`,
//!     `signum`. (`Quat::from_axis_angle` uses `sin`/`cos` but that's call-time,
//!     not per-step.)
//!
//! § COORDINATE SYSTEM
//!   Right-handed, Y-up by convention. Matches the projections-crate convention
//!   (S8-H3) for consistency. Direction of rotation : right-hand rule (positive
//!   angle about +axis rotates +X→+Z for axis=+Y, etc.).

use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};

// ────────────────────────────────────────────────────────────────────────
// § Vec3 — three-component f64 vector
// ────────────────────────────────────────────────────────────────────────

/// A three-component f64 vector.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Vec3 {
    pub const ZERO: Vec3 = Vec3 {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };
    pub const X: Vec3 = Vec3 {
        x: 1.0,
        y: 0.0,
        z: 0.0,
    };
    pub const Y: Vec3 = Vec3 {
        x: 0.0,
        y: 1.0,
        z: 0.0,
    };
    pub const Z: Vec3 = Vec3 {
        x: 0.0,
        y: 0.0,
        z: 1.0,
    };

    #[must_use]
    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }

    #[must_use]
    pub const fn splat(v: f64) -> Self {
        Self { x: v, y: v, z: v }
    }

    #[must_use]
    pub fn dot(self, other: Self) -> f64 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    #[must_use]
    pub fn cross(self, other: Self) -> Self {
        Self {
            x: self.y * other.z - self.z * other.y,
            y: self.z * other.x - self.x * other.z,
            z: self.x * other.y - self.y * other.x,
        }
    }

    #[must_use]
    pub fn length_sq(self) -> f64 {
        self.dot(self)
    }

    #[must_use]
    pub fn length(self) -> f64 {
        self.length_sq().sqrt()
    }

    /// Normalize. Returns `Vec3::ZERO` if length is below `1e-12` to avoid
    /// NaN propagation through the solver.
    #[must_use]
    pub fn normalize_or_zero(self) -> Self {
        let l = self.length();
        if l < 1e-12 {
            Self::ZERO
        } else {
            self / l
        }
    }

    /// Returns the elementwise minimum.
    #[must_use]
    pub fn min(self, other: Self) -> Self {
        Self {
            x: self.x.min(other.x),
            y: self.y.min(other.y),
            z: self.z.min(other.z),
        }
    }

    /// Returns the elementwise maximum.
    #[must_use]
    pub fn max(self, other: Self) -> Self {
        Self {
            x: self.x.max(other.x),
            y: self.y.max(other.y),
            z: self.z.max(other.z),
        }
    }

    /// Returns the elementwise absolute value.
    #[must_use]
    pub fn abs(self) -> Self {
        Self {
            x: self.x.abs(),
            y: self.y.abs(),
            z: self.z.abs(),
        }
    }

    /// Distance squared between two points.
    #[must_use]
    pub fn distance_sq(self, other: Self) -> f64 {
        (other - self).length_sq()
    }

    /// Distance between two points.
    #[must_use]
    pub fn distance(self, other: Self) -> f64 {
        self.distance_sq(other).sqrt()
    }

    /// Component-wise multiply.
    #[must_use]
    pub fn component_mul(self, other: Self) -> Self {
        Self {
            x: self.x * other.x,
            y: self.y * other.y,
            z: self.z * other.z,
        }
    }
}

impl Add for Vec3 {
    type Output = Vec3;
    fn add(self, rhs: Self) -> Self {
        Self {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
            z: self.z + rhs.z,
        }
    }
}

impl Sub for Vec3 {
    type Output = Vec3;
    fn sub(self, rhs: Self) -> Self {
        Self {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
            z: self.z - rhs.z,
        }
    }
}

impl Neg for Vec3 {
    type Output = Vec3;
    fn neg(self) -> Self {
        Self {
            x: -self.x,
            y: -self.y,
            z: -self.z,
        }
    }
}

impl Mul<f64> for Vec3 {
    type Output = Vec3;
    fn mul(self, rhs: f64) -> Self {
        Self {
            x: self.x * rhs,
            y: self.y * rhs,
            z: self.z * rhs,
        }
    }
}

impl Div<f64> for Vec3 {
    type Output = Vec3;
    fn div(self, rhs: f64) -> Self {
        Self {
            x: self.x / rhs,
            y: self.y / rhs,
            z: self.z / rhs,
        }
    }
}

impl AddAssign for Vec3 {
    fn add_assign(&mut self, rhs: Self) {
        self.x += rhs.x;
        self.y += rhs.y;
        self.z += rhs.z;
    }
}

impl SubAssign for Vec3 {
    fn sub_assign(&mut self, rhs: Self) {
        self.x -= rhs.x;
        self.y -= rhs.y;
        self.z -= rhs.z;
    }
}

// ────────────────────────────────────────────────────────────────────────
// § Mat3 — 3×3 f64 matrix (column-major, right-handed)
// ────────────────────────────────────────────────────────────────────────

/// A 3×3 row-major f64 matrix. Used for inertia tensors + rotation matrices.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mat3 {
    /// Row 0
    pub r0: Vec3,
    /// Row 1
    pub r1: Vec3,
    /// Row 2
    pub r2: Vec3,
}

impl Mat3 {
    pub const IDENTITY: Mat3 = Mat3 {
        r0: Vec3::X,
        r1: Vec3::Y,
        r2: Vec3::Z,
    };

    pub const ZERO: Mat3 = Mat3 {
        r0: Vec3::ZERO,
        r1: Vec3::ZERO,
        r2: Vec3::ZERO,
    };

    #[must_use]
    pub const fn from_rows(r0: Vec3, r1: Vec3, r2: Vec3) -> Self {
        Self { r0, r1, r2 }
    }

    /// Diagonal matrix with the given diagonal vector.
    #[must_use]
    pub fn diagonal(d: Vec3) -> Self {
        Self {
            r0: Vec3::new(d.x, 0.0, 0.0),
            r1: Vec3::new(0.0, d.y, 0.0),
            r2: Vec3::new(0.0, 0.0, d.z),
        }
    }

    /// Transpose.
    #[must_use]
    pub fn transpose(self) -> Self {
        Self {
            r0: Vec3::new(self.r0.x, self.r1.x, self.r2.x),
            r1: Vec3::new(self.r0.y, self.r1.y, self.r2.y),
            r2: Vec3::new(self.r0.z, self.r1.z, self.r2.z),
        }
    }

    /// Determinant.
    #[must_use]
    pub fn determinant(self) -> f64 {
        self.r0.x * (self.r1.y * self.r2.z - self.r1.z * self.r2.y)
            - self.r0.y * (self.r1.x * self.r2.z - self.r1.z * self.r2.x)
            + self.r0.z * (self.r1.x * self.r2.y - self.r1.y * self.r2.x)
    }

    /// Inverse. Returns `None` if singular (det near zero).
    #[must_use]
    pub fn try_inverse(self) -> Option<Self> {
        let det = self.determinant();
        if det.abs() < 1e-12 {
            return None;
        }
        let inv_det = 1.0 / det;
        // Cofactor expansion.
        let c00 = self.r1.y * self.r2.z - self.r1.z * self.r2.y;
        let c01 = self.r1.z * self.r2.x - self.r1.x * self.r2.z;
        let c02 = self.r1.x * self.r2.y - self.r1.y * self.r2.x;
        let c10 = self.r0.z * self.r2.y - self.r0.y * self.r2.z;
        let c11 = self.r0.x * self.r2.z - self.r0.z * self.r2.x;
        let c12 = self.r0.y * self.r2.x - self.r0.x * self.r2.y;
        let c20 = self.r0.y * self.r1.z - self.r0.z * self.r1.y;
        let c21 = self.r0.z * self.r1.x - self.r0.x * self.r1.z;
        let c22 = self.r0.x * self.r1.y - self.r0.y * self.r1.x;
        // Adjugate is transpose-of-cofactor ; multiply by inv_det.
        Some(Self {
            r0: Vec3::new(c00 * inv_det, c10 * inv_det, c20 * inv_det),
            r1: Vec3::new(c01 * inv_det, c11 * inv_det, c21 * inv_det),
            r2: Vec3::new(c02 * inv_det, c12 * inv_det, c22 * inv_det),
        })
    }

    /// Multiply matrix * vector.
    #[must_use]
    pub fn mul_vec3(self, v: Vec3) -> Vec3 {
        Vec3::new(self.r0.dot(v), self.r1.dot(v), self.r2.dot(v))
    }

    /// Multiply matrix * matrix.
    #[must_use]
    pub fn mul_mat3(self, other: Self) -> Self {
        let other_t = other.transpose();
        Self {
            r0: Vec3::new(
                self.r0.dot(other_t.r0),
                self.r0.dot(other_t.r1),
                self.r0.dot(other_t.r2),
            ),
            r1: Vec3::new(
                self.r1.dot(other_t.r0),
                self.r1.dot(other_t.r1),
                self.r1.dot(other_t.r2),
            ),
            r2: Vec3::new(
                self.r2.dot(other_t.r0),
                self.r2.dot(other_t.r1),
                self.r2.dot(other_t.r2),
            ),
        }
    }

    /// Skew-symmetric matrix : `[v]_×` such that `[v]_× · w = v × w`.
    #[must_use]
    pub fn skew(v: Vec3) -> Self {
        Self {
            r0: Vec3::new(0.0, -v.z, v.y),
            r1: Vec3::new(v.z, 0.0, -v.x),
            r2: Vec3::new(-v.y, v.x, 0.0),
        }
    }
}

impl Add for Mat3 {
    type Output = Mat3;
    fn add(self, rhs: Self) -> Self {
        Self {
            r0: self.r0 + rhs.r0,
            r1: self.r1 + rhs.r1,
            r2: self.r2 + rhs.r2,
        }
    }
}

impl Sub for Mat3 {
    type Output = Mat3;
    fn sub(self, rhs: Self) -> Self {
        Self {
            r0: self.r0 - rhs.r0,
            r1: self.r1 - rhs.r1,
            r2: self.r2 - rhs.r2,
        }
    }
}

// ────────────────────────────────────────────────────────────────────────
// § Quat — quaternion (w, x, y, z) f64
// ────────────────────────────────────────────────────────────────────────

/// A unit quaternion `(w, x, y, z)` representing 3D rotation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quat {
    pub w: f64,
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Quat {
    pub const IDENTITY: Quat = Quat {
        w: 1.0,
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };

    #[must_use]
    pub const fn new(w: f64, x: f64, y: f64, z: f64) -> Self {
        Self { w, x, y, z }
    }

    /// Construct from axis-angle. `axis` need not be normalized — we
    /// normalize internally. `angle_rad` is in radians.
    #[must_use]
    pub fn from_axis_angle(axis: Vec3, angle_rad: f64) -> Self {
        let n = axis.normalize_or_zero();
        let half = angle_rad * 0.5;
        let s = half.sin();
        Self {
            w: half.cos(),
            x: n.x * s,
            y: n.y * s,
            z: n.z * s,
        }
    }

    /// Compose two rotations : `self * other` means "rotate by other, then by self".
    #[must_use]
    pub fn mul_quat(self, rhs: Self) -> Self {
        Self {
            w: self.w * rhs.w - self.x * rhs.x - self.y * rhs.y - self.z * rhs.z,
            x: self.w * rhs.x + self.x * rhs.w + self.y * rhs.z - self.z * rhs.y,
            y: self.w * rhs.y - self.x * rhs.z + self.y * rhs.w + self.z * rhs.x,
            z: self.w * rhs.z + self.x * rhs.y - self.y * rhs.x + self.z * rhs.w,
        }
    }

    /// Rotate a vector by this quaternion. Uses the formula
    /// `v' = v + 2 q.xyz × (q.xyz × v + q.w v)`.
    #[must_use]
    pub fn rotate_vec3(self, v: Vec3) -> Vec3 {
        let q_vec = Vec3::new(self.x, self.y, self.z);
        let t = q_vec.cross(v) * 2.0;
        v + t * self.w + q_vec.cross(t)
    }

    /// Conjugate (inverse for a unit quaternion).
    #[must_use]
    pub fn conjugate(self) -> Self {
        Self {
            w: self.w,
            x: -self.x,
            y: -self.y,
            z: -self.z,
        }
    }

    /// Squared magnitude.
    #[must_use]
    pub fn length_sq(self) -> f64 {
        self.w * self.w + self.x * self.x + self.y * self.y + self.z * self.z
    }

    /// Magnitude.
    #[must_use]
    pub fn length(self) -> f64 {
        self.length_sq().sqrt()
    }

    /// Normalize. Returns identity if degenerate.
    #[must_use]
    pub fn normalize(self) -> Self {
        let l = self.length();
        if l < 1e-12 {
            Self::IDENTITY
        } else {
            Self {
                w: self.w / l,
                x: self.x / l,
                y: self.y / l,
                z: self.z / l,
            }
        }
    }

    /// Convert to a 3×3 rotation matrix.
    #[must_use]
    pub fn to_mat3(self) -> Mat3 {
        let q = self;
        let xx = q.x * q.x;
        let yy = q.y * q.y;
        let zz = q.z * q.z;
        let xy = q.x * q.y;
        let xz = q.x * q.z;
        let yz = q.y * q.z;
        let wx = q.w * q.x;
        let wy = q.w * q.y;
        let wz = q.w * q.z;
        Mat3 {
            r0: Vec3::new(1.0 - 2.0 * (yy + zz), 2.0 * (xy - wz), 2.0 * (xz + wy)),
            r1: Vec3::new(2.0 * (xy + wz), 1.0 - 2.0 * (xx + zz), 2.0 * (yz - wx)),
            r2: Vec3::new(2.0 * (xz - wy), 2.0 * (yz + wx), 1.0 - 2.0 * (xx + yy)),
        }
    }

    /// Apply an angular-velocity step : `q' = normalize(q + 0.5 * (omega ⊗ q) * dt)`
    /// where `omega ⊗ q` is the quaternion product `[0, omega] * q`. Used by
    /// the symplectic integrator to advance orientation.
    #[must_use]
    pub fn integrate(self, omega: Vec3, dt: f64) -> Self {
        // omega-as-quaternion = (0, omega.x, omega.y, omega.z)
        // dq/dt = 0.5 * omega_q * q
        let dq = Self {
            w: -0.5 * dt * (omega.x * self.x + omega.y * self.y + omega.z * self.z),
            x: 0.5 * dt * (omega.x * self.w + omega.y * self.z - omega.z * self.y),
            y: 0.5 * dt * (-omega.x * self.z + omega.y * self.w + omega.z * self.x),
            z: 0.5 * dt * (omega.x * self.y - omega.y * self.x + omega.z * self.w),
        };
        Self {
            w: self.w + dq.w,
            x: self.x + dq.x,
            y: self.y + dq.y,
            z: self.z + dq.z,
        }
        .normalize()
    }
}

// ════════════════════════════════════════════════════════════════════════
// § Tests
// ════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f64 = 1e-9;

    fn approx_eq(a: f64, b: f64) -> bool {
        (a - b).abs() < EPS
    }

    fn vec3_approx_eq(a: Vec3, b: Vec3) -> bool {
        approx_eq(a.x, b.x) && approx_eq(a.y, b.y) && approx_eq(a.z, b.z)
    }

    // ─── Vec3 ───

    #[test]
    fn vec3_zero_is_zero() {
        assert_eq!(Vec3::ZERO, Vec3::new(0.0, 0.0, 0.0));
    }

    #[test]
    fn vec3_basis_vectors() {
        assert_eq!(Vec3::X, Vec3::new(1.0, 0.0, 0.0));
        assert_eq!(Vec3::Y, Vec3::new(0.0, 1.0, 0.0));
        assert_eq!(Vec3::Z, Vec3::new(0.0, 0.0, 1.0));
    }

    #[test]
    fn vec3_dot_orthogonal_is_zero() {
        assert_eq!(Vec3::X.dot(Vec3::Y), 0.0);
        assert_eq!(Vec3::Y.dot(Vec3::Z), 0.0);
    }

    #[test]
    fn vec3_dot_self_is_length_sq() {
        let v = Vec3::new(3.0, 4.0, 0.0);
        assert_eq!(v.dot(v), 25.0);
        assert_eq!(v.length_sq(), 25.0);
        assert_eq!(v.length(), 5.0);
    }

    #[test]
    fn vec3_cross_anticommutative() {
        let a = Vec3::new(1.0, 2.0, 3.0);
        let b = Vec3::new(4.0, 5.0, 6.0);
        let ab = a.cross(b);
        let ba = b.cross(a);
        assert!(vec3_approx_eq(ab, -ba));
    }

    #[test]
    fn vec3_cross_basis_x_y_z() {
        assert_eq!(Vec3::X.cross(Vec3::Y), Vec3::Z);
        assert_eq!(Vec3::Y.cross(Vec3::Z), Vec3::X);
        assert_eq!(Vec3::Z.cross(Vec3::X), Vec3::Y);
    }

    #[test]
    fn vec3_normalize_or_zero_unit_length() {
        let v = Vec3::new(3.0, 4.0, 0.0).normalize_or_zero();
        assert!(approx_eq(v.length(), 1.0));
    }

    #[test]
    fn vec3_normalize_or_zero_zero() {
        let v = Vec3::ZERO.normalize_or_zero();
        assert_eq!(v, Vec3::ZERO);
    }

    #[test]
    fn vec3_arithmetic_ops() {
        let a = Vec3::new(1.0, 2.0, 3.0);
        let b = Vec3::new(4.0, 5.0, 6.0);
        assert_eq!(a + b, Vec3::new(5.0, 7.0, 9.0));
        assert_eq!(a - b, Vec3::new(-3.0, -3.0, -3.0));
        assert_eq!(a * 2.0, Vec3::new(2.0, 4.0, 6.0));
        assert_eq!(b / 2.0, Vec3::new(2.0, 2.5, 3.0));
        assert_eq!(-a, Vec3::new(-1.0, -2.0, -3.0));
    }

    #[test]
    fn vec3_min_max_abs() {
        let a = Vec3::new(-1.0, 2.0, -3.0);
        let b = Vec3::new(2.0, -1.0, 3.0);
        assert_eq!(a.min(b), Vec3::new(-1.0, -1.0, -3.0));
        assert_eq!(a.max(b), Vec3::new(2.0, 2.0, 3.0));
        assert_eq!(a.abs(), Vec3::new(1.0, 2.0, 3.0));
    }

    #[test]
    fn vec3_distance() {
        let a = Vec3::new(0.0, 0.0, 0.0);
        let b = Vec3::new(3.0, 4.0, 0.0);
        assert_eq!(a.distance_sq(b), 25.0);
        assert_eq!(a.distance(b), 5.0);
    }

    #[test]
    fn vec3_component_mul() {
        let a = Vec3::new(2.0, 3.0, 4.0);
        let b = Vec3::new(5.0, 6.0, 7.0);
        assert_eq!(a.component_mul(b), Vec3::new(10.0, 18.0, 28.0));
    }

    #[test]
    fn vec3_assign_ops() {
        let mut a = Vec3::new(1.0, 2.0, 3.0);
        a += Vec3::new(1.0, 1.0, 1.0);
        assert_eq!(a, Vec3::new(2.0, 3.0, 4.0));
        a -= Vec3::new(2.0, 3.0, 4.0);
        assert_eq!(a, Vec3::ZERO);
    }

    // ─── Mat3 ───

    #[test]
    fn mat3_identity_mul_vec_returns_vec() {
        let v = Vec3::new(1.0, 2.0, 3.0);
        assert_eq!(Mat3::IDENTITY.mul_vec3(v), v);
    }

    #[test]
    fn mat3_diagonal_scales() {
        let m = Mat3::diagonal(Vec3::new(2.0, 3.0, 4.0));
        assert_eq!(
            m.mul_vec3(Vec3::new(1.0, 1.0, 1.0)),
            Vec3::new(2.0, 3.0, 4.0)
        );
    }

    #[test]
    fn mat3_transpose_self_inverse() {
        let m = Mat3::from_rows(
            Vec3::new(1.0, 2.0, 3.0),
            Vec3::new(4.0, 5.0, 6.0),
            Vec3::new(7.0, 8.0, 9.0),
        );
        assert_eq!(m.transpose().transpose(), m);
    }

    #[test]
    fn mat3_determinant_identity_is_one() {
        assert_eq!(Mat3::IDENTITY.determinant(), 1.0);
    }

    #[test]
    fn mat3_determinant_diagonal() {
        let m = Mat3::diagonal(Vec3::new(2.0, 3.0, 4.0));
        assert_eq!(m.determinant(), 24.0);
    }

    #[test]
    fn mat3_inverse_identity() {
        let inv = Mat3::IDENTITY.try_inverse().expect("identity invertible");
        assert_eq!(inv, Mat3::IDENTITY);
    }

    #[test]
    fn mat3_inverse_diagonal() {
        let m = Mat3::diagonal(Vec3::new(2.0, 4.0, 8.0));
        let inv = m.try_inverse().expect("diag invertible");
        // Inverse of diag(2,4,8) is diag(0.5, 0.25, 0.125) — check via mul_vec3.
        let v = Vec3::new(1.0, 1.0, 1.0);
        assert!(vec3_approx_eq(inv.mul_vec3(v), Vec3::new(0.5, 0.25, 0.125)));
    }

    #[test]
    fn mat3_inverse_singular_returns_none() {
        let m = Mat3::ZERO;
        assert!(m.try_inverse().is_none());
    }

    #[test]
    fn mat3_skew_cross_product() {
        let v = Vec3::new(1.0, 2.0, 3.0);
        let w = Vec3::new(4.0, 5.0, 6.0);
        // [v]_x · w == v × w
        assert!(vec3_approx_eq(Mat3::skew(v).mul_vec3(w), v.cross(w)));
    }

    #[test]
    fn mat3_mul_mat3_identity() {
        let m = Mat3::from_rows(
            Vec3::new(1.0, 2.0, 3.0),
            Vec3::new(4.0, 5.0, 6.0),
            Vec3::new(7.0, 8.0, 9.0),
        );
        assert_eq!(m.mul_mat3(Mat3::IDENTITY), m);
        assert_eq!(Mat3::IDENTITY.mul_mat3(m), m);
    }

    #[test]
    fn mat3_arithmetic_ops() {
        let a = Mat3::IDENTITY;
        let b = Mat3::diagonal(Vec3::splat(2.0));
        let sum = a + b;
        assert_eq!(sum.mul_vec3(Vec3::new(1.0, 1.0, 1.0)), Vec3::splat(3.0));
        let diff = b - a;
        assert_eq!(diff.mul_vec3(Vec3::new(1.0, 1.0, 1.0)), Vec3::splat(1.0));
    }

    // ─── Quat ───

    #[test]
    fn quat_identity_is_no_op_rotation() {
        let v = Vec3::new(1.0, 2.0, 3.0);
        assert_eq!(Quat::IDENTITY.rotate_vec3(v), v);
    }

    #[test]
    fn quat_from_axis_angle_y_pi_negates_x_z() {
        let q = Quat::from_axis_angle(Vec3::Y, std::f64::consts::PI);
        let rotated = q.rotate_vec3(Vec3::X);
        assert!(vec3_approx_eq(rotated, Vec3::new(-1.0, 0.0, 0.0)));
    }

    #[test]
    fn quat_from_axis_angle_x_half_pi_rotates_y_to_z() {
        let q = Quat::from_axis_angle(Vec3::X, std::f64::consts::FRAC_PI_2);
        let rotated = q.rotate_vec3(Vec3::Y);
        assert!(vec3_approx_eq(rotated, Vec3::Z));
    }

    #[test]
    fn quat_compose_two_rotations() {
        let qa = Quat::from_axis_angle(Vec3::X, std::f64::consts::FRAC_PI_2);
        let qb = Quat::from_axis_angle(Vec3::X, std::f64::consts::FRAC_PI_2);
        let q = qa.mul_quat(qb);
        // Two 90-deg rotations = 180 deg ; Y → -Y
        let rotated = q.rotate_vec3(Vec3::Y);
        assert!(vec3_approx_eq(rotated, -Vec3::Y));
    }

    #[test]
    fn quat_conjugate_inverts() {
        let q = Quat::from_axis_angle(Vec3::Y, 0.7);
        let qc = q.conjugate();
        let r = q.mul_quat(qc);
        assert!(approx_eq(r.w, 1.0));
        assert!(approx_eq(r.x, 0.0));
        assert!(approx_eq(r.y, 0.0));
        assert!(approx_eq(r.z, 0.0));
    }

    #[test]
    fn quat_normalize_unit_length() {
        let q = Quat::new(2.0, 0.0, 0.0, 0.0).normalize();
        assert!(approx_eq(q.length(), 1.0));
    }

    #[test]
    fn quat_normalize_zero_returns_identity() {
        let q = Quat::new(0.0, 0.0, 0.0, 0.0).normalize();
        assert_eq!(q, Quat::IDENTITY);
    }

    #[test]
    fn quat_to_mat3_identity() {
        assert_eq!(Quat::IDENTITY.to_mat3(), Mat3::IDENTITY);
    }

    #[test]
    fn quat_to_mat3_rotates_consistently() {
        let q = Quat::from_axis_angle(Vec3::Z, std::f64::consts::FRAC_PI_2);
        let m = q.to_mat3();
        // X → Y
        assert!(vec3_approx_eq(m.mul_vec3(Vec3::X), Vec3::Y));
        // Match quat-direct-rotation
        assert!(vec3_approx_eq(m.mul_vec3(Vec3::X), q.rotate_vec3(Vec3::X)));
    }

    #[test]
    fn quat_integrate_zero_omega_no_change() {
        let q = Quat::from_axis_angle(Vec3::Y, 0.5);
        let q2 = q.integrate(Vec3::ZERO, 0.016);
        assert!(approx_eq(q.w, q2.w));
        assert!(approx_eq(q.x, q2.x));
        assert!(approx_eq(q.y, q2.y));
        assert!(approx_eq(q.z, q2.z));
    }

    #[test]
    fn quat_integrate_y_axis_advances_y_rotation() {
        // Start with identity ; integrate Y-axis omega for one tick → small rotation.
        let q0 = Quat::IDENTITY;
        let omega = Vec3::new(0.0, 1.0, 0.0); // 1 rad/s about Y
        let q1 = q0.integrate(omega, 0.1);
        // After 0.1s at 1 rad/s, expect ~0.1 rad rotation about Y. Compare against
        // the direct construction.
        let expected = Quat::from_axis_angle(Vec3::Y, 0.1);
        // Symplectic integration is first-order — accept tolerance ~1e-3.
        assert!((q1.w - expected.w).abs() < 1e-3);
        assert!((q1.y - expected.y).abs() < 1e-3);
    }

    #[test]
    fn quat_integrate_preserves_unit_length() {
        let mut q = Quat::IDENTITY;
        let omega = Vec3::new(1.0, 2.0, 0.5);
        for _ in 0..100 {
            q = q.integrate(omega, 0.01);
        }
        assert!((q.length() - 1.0).abs() < 1e-9);
    }
}
