//! § Multivector — full 16-component element of G(3,0,1) (f32)
//!
//! § STORAGE
//!   16 floats in the canonical-blade order — see [`crate::basis`]. The
//!   `#[repr(C)]` layout means a `&[Multivector]` slice can be cast to a
//!   `&[f32]` of 16× length for SIMD or GPU upload paths.
//!
//! § PRODUCTS
//!   - Geometric product `a * b` — full 16×16 expansion. The single
//!     algebraically-fundamental product ; outer / inner / regressive are
//!     grade-projections of this product.
//!   - Outer (wedge) product `a ^ b` — meet of subspaces, grade increases.
//!   - Inner product `a | b` — contraction, grade decreases.
//!   - Regressive product `a & b` — dual of the wedge ; "join" of subspaces
//!     in the projective sense.
//!
//! § INVOLUTIONS
//!   - [`Multivector::reverse`] — reverse the order of basis vectors in each
//!     blade : `(e₁ e₂)~ = e₂ e₁ = -e₁ e₂`. Sign is `(-1)^(k(k-1)/2)` where
//!     `k` is the grade. Used in the sandwich product `M v M̃`.
//!   - [`Multivector::grade_involution`] — flip sign of odd-grade blades :
//!     used to distinguish even (rotors / motors) from odd (reflections)
//!     elements.
//!   - [`Multivector::clifford_conjugation`] — combination of the above ;
//!     used in norm computations.
//!
//! § DUAL
//!   [`Multivector::dual`] — multiplication by the pseudoscalar inverse.
//!   In G(3,0,1) the dual is degenerate (`e₀² = 0` makes `I⁻¹` not exist in
//!   the usual sense) ; we use the **Hodge-star convention** which swaps
//!   grade-k blades with grade-(4-k) blades. This is the convention used
//!   throughout Klein-style PGA where the dual converts plane↔point.

use core::ops::{Add, AddAssign, BitAnd, BitOr, BitXor, Mul, MulAssign, Neg, Sub, SubAssign};

use crate::basis::{Grade, BLADE_COUNT};

/// 16-component multivector in G(3,0,1), `f32` precision.
///
/// Components are stored in the canonical-blade order (see [`crate::basis`]).
/// `#[repr(C)]` so a `&[Multivector]` slice maps directly onto 16 × N floats
/// for SIMD or GPU upload.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[repr(C)]
pub struct Multivector {
    /// 16 component floats in canonical-blade order.
    coeffs: [f32; BLADE_COUNT],
}

impl Multivector {
    // ─── constructors ───────────────────────────────────────────────────────

    /// Construct from a raw `[f32; 16]` array in the canonical order.
    #[must_use]
    pub const fn from_array(coeffs: [f32; BLADE_COUNT]) -> Self {
        Self { coeffs }
    }

    /// Construct a multivector with only a scalar component.
    #[must_use]
    pub const fn from_scalar(s: f32) -> Self {
        let mut a = [0.0_f32; BLADE_COUNT];
        a[0] = s;
        Self::from_array(a)
    }

    /// Borrow the raw component array. Consumers that need a `&[f32]` slice
    /// can `as_array().as_slice()` or use `as_slice` directly.
    #[must_use]
    pub const fn as_array(&self) -> &[f32; BLADE_COUNT] {
        &self.coeffs
    }

    /// Borrow the raw component array as a slice.
    #[must_use]
    pub const fn as_slice(&self) -> &[f32] {
        &self.coeffs
    }

    /// Read the coefficient at canonical-blade index `i`.
    ///
    /// # Panics
    /// Panics if `i >= 16`.
    #[must_use]
    pub const fn coefficient(&self, i: usize) -> f32 {
        self.coeffs[i]
    }

    /// Write the coefficient at canonical-blade index `i`. Returns the
    /// modified multivector for fluent construction.
    ///
    /// # Panics
    /// Panics if `i >= 16`.
    #[must_use]
    pub const fn with_coefficient(mut self, i: usize, v: f32) -> Self {
        self.coeffs[i] = v;
        self
    }

    // ─── named-component accessors ──────────────────────────────────────────
    //
    // Each accessor names one of the 16 canonical blades. These let downstream
    // code read e.g. `mv.e23()` rather than `mv.coefficient(5)`. The blade
    // names match `crate::basis::BASIS_NAMES`.

    /// Scalar (grade-0) coefficient.
    #[must_use]
    pub const fn s(&self) -> f32 {
        self.coeffs[0]
    }
    /// `e₁` (grade-1 plane) coefficient.
    #[must_use]
    pub const fn e1(&self) -> f32 {
        self.coeffs[1]
    }
    /// `e₂` (grade-1 plane) coefficient.
    #[must_use]
    pub const fn e2(&self) -> f32 {
        self.coeffs[2]
    }
    /// `e₃` (grade-1 plane) coefficient.
    #[must_use]
    pub const fn e3(&self) -> f32 {
        self.coeffs[3]
    }
    /// `e₀` (grade-1 ideal-plane) coefficient.
    #[must_use]
    pub const fn e0(&self) -> f32 {
        self.coeffs[4]
    }
    /// `e₂₃` (grade-2 rotation generator about x) coefficient.
    #[must_use]
    pub const fn e23(&self) -> f32 {
        self.coeffs[5]
    }
    /// `e₃₁` (grade-2 rotation generator about y) coefficient.
    #[must_use]
    pub const fn e31(&self) -> f32 {
        self.coeffs[6]
    }
    /// `e₁₂` (grade-2 rotation generator about z) coefficient.
    #[must_use]
    pub const fn e12(&self) -> f32 {
        self.coeffs[7]
    }
    /// `e₀₁` (grade-2 translation generator along x) coefficient.
    #[must_use]
    pub const fn e01(&self) -> f32 {
        self.coeffs[8]
    }
    /// `e₀₂` (grade-2 translation generator along y) coefficient.
    #[must_use]
    pub const fn e02(&self) -> f32 {
        self.coeffs[9]
    }
    /// `e₀₃` (grade-2 translation generator along z) coefficient.
    #[must_use]
    pub const fn e03(&self) -> f32 {
        self.coeffs[10]
    }
    /// `e₀₃₂` (grade-3 point x-component) coefficient.
    #[must_use]
    pub const fn e032(&self) -> f32 {
        self.coeffs[11]
    }
    /// `e₀₁₃` (grade-3 point y-component) coefficient.
    #[must_use]
    pub const fn e013(&self) -> f32 {
        self.coeffs[12]
    }
    /// `e₀₂₁` (grade-3 point z-component) coefficient.
    #[must_use]
    pub const fn e021(&self) -> f32 {
        self.coeffs[13]
    }
    /// `e₁₂₃` (grade-3 point normalization-component) coefficient.
    #[must_use]
    pub const fn e123(&self) -> f32 {
        self.coeffs[14]
    }
    /// `e₀₁₂₃` (grade-4 pseudoscalar) coefficient.
    #[must_use]
    pub const fn e0123(&self) -> f32 {
        self.coeffs[15]
    }

    // ─── grade projection ────────────────────────────────────────────────────

    /// Grade-projection `⟨A⟩_k` — extract only the components of grade `g`,
    /// zero everything else.
    #[must_use]
    pub fn grade_project(&self, g: Grade) -> Self {
        let mut out = Self::default();
        let (lo, hi) = g.index_range();
        for i in lo..hi {
            out.coeffs[i] = self.coeffs[i];
        }
        out
    }

    /// Convenience : grade-0 scalar projection.
    #[must_use]
    pub fn grade0(&self) -> Self {
        self.grade_project(Grade::Scalar)
    }
    /// Convenience : grade-1 vector / plane projection.
    #[must_use]
    pub fn grade1(&self) -> Self {
        self.grade_project(Grade::Vector)
    }
    /// Convenience : grade-2 bivector / line projection.
    #[must_use]
    pub fn grade2(&self) -> Self {
        self.grade_project(Grade::Bivector)
    }
    /// Convenience : grade-3 trivector / point projection.
    #[must_use]
    pub fn grade3(&self) -> Self {
        self.grade_project(Grade::Trivector)
    }
    /// Convenience : grade-4 pseudoscalar projection.
    #[must_use]
    pub fn grade4(&self) -> Self {
        self.grade_project(Grade::Pseudoscalar)
    }

    /// True if `self` has zero coefficient on every blade.
    #[must_use]
    pub fn is_zero(&self) -> bool {
        self.coeffs.iter().all(|&v| v == 0.0)
    }

    // ─── involutions ────────────────────────────────────────────────────────

    /// Reverse `~A` : reverse the order of basis vectors in each blade.
    /// Sign is `(-1)^(k(k-1)/2)` where `k` is the blade grade.
    ///
    /// For grades 0..4 the signs are `+ + - - +` :
    ///   - grade-0 : `+1` (no swap)
    ///   - grade-1 : `+1` (no swap)
    ///   - grade-2 : `-1` (one swap : `e_i e_j → e_j e_i = -e_i e_j`)
    ///   - grade-3 : `-1` (three swaps)
    ///   - grade-4 : `+1` (six swaps)
    ///
    /// This is the involution used in the sandwich product `M v M̃`.
    #[must_use]
    pub fn reverse(&self) -> Self {
        let c = &self.coeffs;
        Self::from_array([
            c[0], // grade-0 : +
            c[1], // grade-1 : +
            c[2], c[3], c[4], -c[5], // grade-2 : −
            -c[6], -c[7], -c[8], -c[9], -c[10], -c[11], // grade-3 : −
            -c[12], -c[13], -c[14], c[15], // grade-4 : +
        ])
    }

    /// Grade involution `Â` : flip sign of odd-grade blades.
    /// Sign is `(-1)^k` for grade `k`.
    ///
    /// For grades 0..4 the signs are `+ - + - +`. Even-grade elements
    /// (motors, rotors) are fixed by this involution ; odd-grade elements
    /// (reflections, single planes) are negated.
    #[must_use]
    pub fn grade_involution(&self) -> Self {
        let c = &self.coeffs;
        Self::from_array([
            c[0],  // grade-0 : +
            -c[1], // grade-1 : −
            -c[2], -c[3], -c[4], c[5], // grade-2 : +
            c[6], c[7], c[8], c[9], c[10], -c[11], // grade-3 : −
            -c[12], -c[13], -c[14], c[15], // grade-4 : +
        ])
    }

    /// Clifford conjugation `Ā = ~Â` : composition of reverse + grade
    /// involution. Sign is `(-1)^(k(k+1)/2)` for grade `k`.
    ///
    /// For grades 0..4 the signs are `+ - - + +`.
    #[must_use]
    pub fn clifford_conjugation(&self) -> Self {
        let c = &self.coeffs;
        Self::from_array([
            c[0],  // grade-0 : +
            -c[1], // grade-1 : −
            -c[2], -c[3], -c[4], -c[5], // grade-2 : −
            -c[6], -c[7], -c[8], -c[9], -c[10], c[11], // grade-3 : +
            c[12], c[13], c[14], c[15], // grade-4 : +
        ])
    }

    /// Hodge-style dual : swaps grade-k ↔ grade-(4-k). The convention used
    /// here matches Klein-PGA : the dual of a plane (grade-1) is a point
    /// (grade-3), and vice versa.
    ///
    /// Specifically, for a multivector `A` with components `a_i`, the dual
    /// `A*` has components in the swap pairs :
    ///   - `(s) ↔ (e₀₁₂₃)` : grade-0 ↔ grade-4
    ///   - `(e₁) ↔ (e₀₃₂)`, `(e₂) ↔ (e₀₁₃)`, `(e₃) ↔ (e₀₂₁)`, `(e₀) ↔ (e₁₂₃)`
    ///   - `(e₂₃) ↔ (e₀₁)`, `(e₃₁) ↔ (e₀₂)`, `(e₁₂) ↔ (e₀₃)`
    ///
    /// The dual is involutive : `A.dual().dual() == A`.
    #[must_use]
    pub fn dual(&self) -> Self {
        let c = &self.coeffs;
        Self::from_array([
            c[15], // s ← e0123
            c[11], // e1 ← e032
            c[12], // e2 ← e013
            c[13], // e3 ← e021
            c[14], // e0 ← e123
            c[8],  // e23 ← e01
            c[9],  // e31 ← e02
            c[10], // e12 ← e03
            c[5],  // e01 ← e23
            c[6],  // e02 ← e31
            c[7],  // e03 ← e12
            c[1],  // e032 ← e1
            c[2],  // e013 ← e2
            c[3],  // e021 ← e3
            c[4],  // e123 ← e0
            c[0],  // e0123 ← s
        ])
    }

    // ─── norms ──────────────────────────────────────────────────────────────

    /// Squared norm `‖A‖² = ⟨A Ā⟩₀` — the scalar part of `A` times its
    /// Clifford conjugate.
    ///
    /// In the degenerate G(3,0,1) signature, the squared norm of a
    /// translator is **always 0** (the `e₀_*` components contribute
    /// nothing because `e₀² = 0`). For pure rotors the squared norm
    /// equals `1` modulo float drift. For motors it equals the rotor
    /// part's squared norm.
    #[must_use]
    pub fn norm_squared(&self) -> f32 {
        let c = &self.coeffs;
        // Direct expansion of (A Ā)₀ — contribution from each blade.
        // grade-0 :  + s²
        // grade-1 (e₁,e₂,e₃) : + e1² + e2² + e3²        (e₀ is null → 0)
        // grade-2 (e23,e31,e12) : + e23² + e31² + e12²  (e₀_* are null → 0)
        // grade-3 (e123) : + e123²                       (others null → 0)
        // grade-4 (e0123) :  pseudoscalar — contains e₀ → 0
        c[0] * c[0]
            + c[1] * c[1]
            + c[2] * c[2]
            + c[3] * c[3]
            + c[5] * c[5]
            + c[6] * c[6]
            + c[7] * c[7]
            + c[14] * c[14]
    }

    /// Euclidean norm `‖A‖`. See [`Self::norm_squared`] for the degenerate
    /// behavior of translation generators.
    #[must_use]
    pub fn norm(&self) -> f32 {
        self.norm_squared().sqrt()
    }

    // ─── geometric product ──────────────────────────────────────────────────

    /// Geometric product `a * b`.
    ///
    /// This is the algebraically-fundamental product of GA. The 16 × 16
    /// blade-multiplication table for G(3,0,1) is precomputed at compile
    /// time as [`BLADE_PRODUCT`] from the Cayley rules — see the table
    /// section below. The compiler folds away the zero-coefficient paths
    /// for sparse inputs (motors, rotors, points etc.) automatically.
    #[must_use]
    pub fn geometric(&self, b: &Self) -> Self {
        let a = &self.coeffs;
        let b = &b.coeffs;
        let mut r = [0.0_f32; BLADE_COUNT];
        for i in 0..BLADE_COUNT {
            if a[i] == 0.0 {
                continue;
            }
            for j in 0..BLADE_COUNT {
                if b[j] == 0.0 {
                    continue;
                }
                let (idx, sign) = BLADE_PRODUCT[i][j];
                if sign != 0 {
                    r[idx] += f32::from(sign) * a[i] * b[j];
                }
            }
        }
        Self::from_array(r)
    }

    /// Outer (wedge) product `a ^ b` — antisymmetric part of the geometric
    /// product, taking grade-(p+q) part for grade-p and grade-q inputs.
    ///
    /// Geometrically : `plane ^ plane = line` (the line where they intersect),
    /// `point ^ point = line` (the line through the two points). Increases
    /// grade.
    #[must_use]
    pub fn outer(&self, b: &Self) -> Self {
        let a = &self.coeffs;
        let b = &b.coeffs;
        let mut r = [0.0_f32; BLADE_COUNT];
        for i in 0..BLADE_COUNT {
            if a[i] == 0.0 {
                continue;
            }
            let gi = crate::basis::grade_of(i).rank();
            for j in 0..BLADE_COUNT {
                if b[j] == 0.0 {
                    continue;
                }
                let gj = crate::basis::grade_of(j).rank();
                let (idx, sign) = BLADE_PRODUCT[i][j];
                if sign != 0 {
                    let gk = crate::basis::grade_of(idx).rank();
                    if gk == gi + gj {
                        r[idx] += f32::from(sign) * a[i] * b[j];
                    }
                }
            }
        }
        Self::from_array(r)
    }

    /// Inner (left contraction) product `a | b` — grade-|p−q| part of the
    /// geometric product for grade-p and grade-q inputs.
    ///
    /// Geometrically : `plane | plane = scalar` (the cosine of the dihedral
    /// angle for unit planes), `line | line = scalar` (a measure of the
    /// shared subspace). Decreases grade.
    #[must_use]
    pub fn inner(&self, b: &Self) -> Self {
        let a = &self.coeffs;
        let b = &b.coeffs;
        let mut r = [0.0_f32; BLADE_COUNT];
        for i in 0..BLADE_COUNT {
            if a[i] == 0.0 {
                continue;
            }
            let gi = crate::basis::grade_of(i).rank();
            for j in 0..BLADE_COUNT {
                if b[j] == 0.0 {
                    continue;
                }
                let gj = crate::basis::grade_of(j).rank();
                let (idx, sign) = BLADE_PRODUCT[i][j];
                if sign != 0 {
                    let gk = crate::basis::grade_of(idx).rank();
                    let want = gi.abs_diff(gj);
                    if gk == want {
                        r[idx] += f32::from(sign) * a[i] * b[j];
                    }
                }
            }
        }
        Self::from_array(r)
    }

    /// Regressive product `a & b` — the dual of the wedge ; "join" of two
    /// subspaces. For PGA : `point & point = line through them`,
    /// `line & point = plane through both`, `plane & plane = scalar
    /// (incidence test if both unit)`.
    ///
    /// Implementation : `(a & b) = ((a*) ^ (b*))*` where `*` is the dual.
    #[must_use]
    pub fn regressive(&self, b: &Self) -> Self {
        self.dual().outer(&b.dual()).dual()
    }

    /// Sandwich product `M v M̃` — apply a motor (or any unit element) `M`
    /// to a multivector `v` by conjugation. This is the canonical way to
    /// transform geometric primitives (planes, points, lines) by rigid
    /// motions in PGA.
    ///
    /// For a unit rotor `R`, `R.sandwich(point) = rotated_point`. For a
    /// motor `M = T R`, `M.sandwich(point) = translated_rotated_point`.
    /// Sign-preserving (orientation-preserving) iff `M` is even-grade.
    #[must_use]
    pub fn sandwich(&self, v: &Self) -> Self {
        self.geometric(v).geometric(&self.reverse())
    }

    // ─── arithmetic on individual components ────────────────────────────────

    /// Component-wise scalar multiplication. Equivalent to `s * self` but
    /// reads more naturally for chained transforms.
    #[must_use]
    pub fn scale(&self, s: f32) -> Self {
        let mut a = self.coeffs;
        for v in &mut a {
            *v *= s;
        }
        Self::from_array(a)
    }
}

// ─── operator overloads ─────────────────────────────────────────────────────

impl Add for Multivector {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        let mut a = self.coeffs;
        for i in 0..BLADE_COUNT {
            a[i] += rhs.coeffs[i];
        }
        Self::from_array(a)
    }
}

impl AddAssign for Multivector {
    fn add_assign(&mut self, rhs: Self) {
        for i in 0..BLADE_COUNT {
            self.coeffs[i] += rhs.coeffs[i];
        }
    }
}

impl Sub for Multivector {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        let mut a = self.coeffs;
        for i in 0..BLADE_COUNT {
            a[i] -= rhs.coeffs[i];
        }
        Self::from_array(a)
    }
}

impl SubAssign for Multivector {
    fn sub_assign(&mut self, rhs: Self) {
        for i in 0..BLADE_COUNT {
            self.coeffs[i] -= rhs.coeffs[i];
        }
    }
}

impl Neg for Multivector {
    type Output = Self;
    fn neg(self) -> Self {
        let mut a = self.coeffs;
        for v in &mut a {
            *v = -*v;
        }
        Self::from_array(a)
    }
}

/// `Multivector * Multivector` is the geometric product.
impl Mul for Multivector {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self {
        self.geometric(&rhs)
    }
}

impl MulAssign for Multivector {
    fn mul_assign(&mut self, rhs: Self) {
        *self = self.geometric(&rhs);
    }
}

/// `Multivector * f32` is component-wise scalar multiplication.
impl Mul<f32> for Multivector {
    type Output = Self;
    fn mul(self, rhs: f32) -> Self {
        self.scale(rhs)
    }
}

/// `f32 * Multivector` (mirror of [`Mul<f32>`]).
impl Mul<Multivector> for f32 {
    type Output = Multivector;
    fn mul(self, rhs: Multivector) -> Multivector {
        rhs.scale(self)
    }
}

/// `^` is the outer (wedge) product. Operator-overload precedence in Rust
/// follows the bitwise-XOR rules — chain with parentheses for clarity.
impl BitXor for Multivector {
    type Output = Self;
    fn bitxor(self, rhs: Self) -> Self {
        self.outer(&rhs)
    }
}

/// `|` is the inner (left contraction) product. Same precedence note as
/// outer-product `^`.
impl BitOr for Multivector {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        self.inner(&rhs)
    }
}

/// `&` is the regressive product. Same precedence note as the others.
impl BitAnd for Multivector {
    type Output = Self;
    fn bitand(self, rhs: Self) -> Self {
        self.regressive(&rhs)
    }
}

// ════════════════════════════════════════════════════════════════════════════
// § BLADE-PRODUCT TABLE
// ════════════════════════════════════════════════════════════════════════════
//
// `BLADE_PRODUCT[i][j] = (result_index, sign)` for the geometric product of
// the unit blades at canonical index `i` and `j`. `sign` is `+1`, `−1`, or
// `0` (the latter for any product involving the null direction `e₀` an
// even number of times — `e₀² = 0` annihilates everything).
//
// This table is the source of truth for the blade-multiplication structure
// of G(3,0,1). It is generated by the `blade_product_table` const-fn below
// at compile time, computed from the Cayley-table rules :
//
//   1. Each blade is a sorted product of basis vectors `{e₀, e₁, e₂, e₃}`
//      represented as a 4-bit mask (bit 0 = e₀, bit 1 = e₁, bit 2 = e₂,
//      bit 3 = e₃).
//   2. The product of two blades `A * B` is :
//        a. The XOR of the masks (basis vectors that appear in exactly one
//           of A or B survive ; pairs cancel).
//        b. A sign correction for the swaps needed to bring the bases into
//           sorted order (each swap flips the sign).
//        c. A sign correction for each squared basis vector — `e₀² = 0` ;
//           `e₁² = e₂² = e₃² = +1`.
//   3. The result-mask is mapped back to the canonical-blade index.
//
// The mapping mask ↔ canonical index is non-trivial because the canonical
// order (specifically the negated trivectors) is not lex order. We use a
// two-step process : compute the (lex-mask, sign) of the product, then map
// to canonical index applying any orientation flip.

/// Mask of basis vectors in the canonical-order blade at index `i`.
/// Bit 0 = e₀, bit 1 = e₁, bit 2 = e₂, bit 3 = e₃.
const BLADE_MASK: [u8; BLADE_COUNT] = [
    0b0000, // 0  : 1
    0b0010, // 1  : e1
    0b0100, // 2  : e2
    0b1000, // 3  : e3
    0b0001, // 4  : e0
    0b1100, // 5  : e23
    0b1010, // 6  : e31    (lex e1 e3 — orientation flip in canonical, see SIGN below)
    0b0110, // 7  : e12
    0b0011, // 8  : e01
    0b0101, // 9  : e02
    0b1001, // 10 : e03
    0b1101, // 11 : e032   (lex e0 e2 e3 — flip)
    0b1011, // 12 : e013   (lex e0 e1 e3)
    0b0111, // 13 : e021   (lex e0 e1 e2 — flip)
    0b1110, // 14 : e123
    0b1111, // 15 : e0123
];

/// Sign of the canonical-order blade relative to its lex-sorted form.
/// e.g. `e_31 = -e_13` so `BLADE_SIGN[6] = -1`.
const BLADE_SIGN: [i8; BLADE_COUNT] = [
    1,  // 0  : 1
    1,  // 1  : e1
    1,  // 2  : e2
    1,  // 3  : e3
    1,  // 4  : e0
    1,  // 5  : e23  = e2 e3
    -1, // 6  : e31  = -e1 e3
    1,  // 7  : e12  = e1 e2
    1,  // 8  : e01  = e0 e1
    1,  // 9  : e02  = e0 e2
    1,  // 10 : e03  = e0 e3
    -1, // 11 : e032 = -e0 e2 e3
    1,  // 12 : e013 = e0 e1 e3
    -1, // 13 : e021 = -e0 e1 e2
    1,  // 14 : e123 = e1 e2 e3
    1,  // 15 : e0123
];

/// Look up canonical-order blade index from its lex-sorted mask.
/// `MASK_TO_INDEX[mask]` is the canonical index of the lex-blade with that mask.
const MASK_TO_INDEX: [usize; 16] = {
    let mut t = [0_usize; 16];
    let mut i = 0;
    while i < BLADE_COUNT {
        t[BLADE_MASK[i] as usize] = i;
        i += 1;
    }
    t
};

/// Build the 16×16 blade-product table at compile time.
const BLADE_PRODUCT: [[(usize, i8); BLADE_COUNT]; BLADE_COUNT] = {
    let mut table = [[(0_usize, 0_i8); BLADE_COUNT]; BLADE_COUNT];
    let mut i = 0;
    while i < BLADE_COUNT {
        let mut j = 0;
        while j < BLADE_COUNT {
            // Compute the product of unit-blades at canonical index i and j.
            // Step 1 : the lex-form sign for blade i (BLADE_SIGN[i]) and blade j (BLADE_SIGN[j]).
            let sign_lex_i = BLADE_SIGN[i];
            let sign_lex_j = BLADE_SIGN[j];

            // Step 2 : multiply the two lex-form blades.
            let (result_mask, sign_mul) = multiply_lex(BLADE_MASK[i], BLADE_MASK[j]);

            if sign_mul == 0 {
                table[i][j] = (0, 0);
            } else {
                let result_idx = MASK_TO_INDEX[result_mask as usize];
                let sign_result_lex = BLADE_SIGN[result_idx];
                // Sign flow : the user's input is in canonical form (sign = sign_lex_i),
                // so the lex-form coefficient is sign_lex_i × user_coefficient.
                // Same for j. The lex-multiplication produces sign_mul, so the lex
                // output is (sign_lex_i × sign_lex_j × sign_mul) × user_i × user_j.
                // To convert this lex output to canonical output we divide by
                // sign_result_lex (which equals multiplication since it's ±1).
                let sign_canonical = sign_lex_i * sign_lex_j * sign_mul * sign_result_lex;
                table[i][j] = (result_idx, sign_canonical);
            }

            j += 1;
        }
        i += 1;
    }
    table
};

/// Public accessor for the precomputed blade-product table — used by
/// the f64 [`crate::multivector_f64::Multivector`] variant to share the
/// Cayley-table without duplicating it.
///
/// Returns `(result_index, sign)` for the geometric product of the
/// canonical-order blades at indices `i` and `j`. `sign` is `+1`, `-1`,
/// or `0` (the latter whenever the product involves `e₀²`).
///
/// # Panics
/// Panics if `i >= 16` or `j >= 16`.
#[must_use]
pub const fn blade_product_entry(i: usize, j: usize) -> (usize, i8) {
    BLADE_PRODUCT[i][j]
}

/// Multiply two lex-sorted blades represented as masks. Returns
/// `(result_mask, sign)` where `sign ∈ {-1, 0, +1}`. `sign = 0` whenever
/// the result includes a doubled `e₀` (which squares to 0).
const fn multiply_lex(a: u8, b: u8) -> (u8, i8) {
    // Sign from sorting : count swaps needed to merge `a` (sorted) with
    // `b` (sorted). For each set bit `bj` in `b`, count the bits in `a`
    // with higher index than `bj` — those need to swap past `bj`.
    let mut sign: i8 = 1;
    let mut bj = 0;
    while bj < 4 {
        if (b >> bj) & 1 == 1 {
            // Count bits in `a` at positions > bj.
            let mut bi = bj + 1;
            while bi < 4 {
                if (a >> bi) & 1 == 1 {
                    sign = -sign;
                }
                bi += 1;
            }
        }
        bj += 1;
    }

    // Sign from squaring shared bases. Common = a & b.
    let common = a & b;
    // Bit 0 = e₀ : if shared, the result is 0.
    if (common & 1) == 1 {
        return (0, 0);
    }
    // Bits 1..=3 are e₁,e₂,e₃ — each squares to +1 (no sign change).

    let result_mask = a ^ b;
    (result_mask, sign)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::basis::{E0, E0123, E032, E1, E12, E123, E2, E23, E3, E31, IDENTITY, ZERO};

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-5
    }
    fn mv_approx(a: &Multivector, b: &Multivector) -> bool {
        for i in 0..BLADE_COUNT {
            if !approx(a.coefficient(i), b.coefficient(i)) {
                eprintln!(
                    "mv mismatch at index {} : a={} b={}",
                    i,
                    a.coefficient(i),
                    b.coefficient(i)
                );
                return false;
            }
        }
        true
    }

    // ── basic algebra ────────────────────────────────────────────────────────

    #[test]
    fn scalar_unit_squares_to_one() {
        // 1 * 1 = 1.
        let one = IDENTITY;
        assert!(mv_approx(&(one * one), &one));
    }

    #[test]
    fn spatial_basis_vectors_square_to_plus_one() {
        for &b in &[E1, E2, E3] {
            let sq = b * b;
            assert!(mv_approx(&sq, &IDENTITY), "blade^2 should be +1");
        }
    }

    #[test]
    fn null_basis_vector_squares_to_zero() {
        // e₀² = 0 by construction (degenerate signature).
        let sq = E0 * E0;
        assert!(mv_approx(&sq, &ZERO));
    }

    #[test]
    fn anticommutation_of_distinct_basis_vectors() {
        // e₁ e₂ = -e₂ e₁
        let lhs = E1 * E2;
        let rhs = -(E2 * E1);
        assert!(mv_approx(&lhs, &rhs));
    }

    #[test]
    fn bivector_e12_factors_to_e1_e2() {
        // e₁ e₂ should equal the unit blade e₁₂.
        let p = E1 * E2;
        assert!(mv_approx(&p, &E12));
    }

    #[test]
    fn bivector_e23_factors_to_e2_e3() {
        let p = E2 * E3;
        assert!(mv_approx(&p, &E23));
    }

    #[test]
    fn bivector_e31_factors_with_proper_sign() {
        // e₃ e₁ should equal e₃₁ (canonical).
        let p = E3 * E1;
        assert!(mv_approx(&p, &E31));
    }

    #[test]
    fn rotor_squared_norm_is_unit_for_unit_axis() {
        // A rotor R = cos(θ/2) - sin(θ/2) e₂₃ should have ‖R‖² = 1.
        let theta = 0.7_f32;
        let r = (IDENTITY * (theta / 2.0).cos()) - (E23 * (theta / 2.0).sin());
        assert!(approx(r.norm_squared(), 1.0));
    }

    // ── grade projection ─────────────────────────────────────────────────────

    #[test]
    fn grade_projection_isolates_components() {
        let mv = (IDENTITY * 2.0) + (E1 * 3.0) + (E12 * 5.0) + (E123 * 7.0) + (E0123 * 11.0);
        assert!(approx(mv.grade0().s(), 2.0));
        assert!(approx(mv.grade1().e1(), 3.0));
        assert!(approx(mv.grade2().e12(), 5.0));
        assert!(approx(mv.grade3().e123(), 7.0));
        assert!(approx(mv.grade4().e0123(), 11.0));
    }

    #[test]
    fn grade_projection_zeros_other_grades() {
        let mv = (IDENTITY * 2.0) + (E1 * 3.0) + (E12 * 5.0);
        let g0 = mv.grade0();
        assert!(approx(g0.e1(), 0.0));
        assert!(approx(g0.e12(), 0.0));
    }

    // ── involutions ──────────────────────────────────────────────────────────

    #[test]
    fn reverse_double_application_is_identity() {
        let mv = (IDENTITY * 1.5) + (E1 * 2.0) + (E12 * 3.0) + (E123 * 0.5) + (E0123 * 0.7);
        assert!(mv_approx(&mv.reverse().reverse(), &mv));
    }

    #[test]
    fn reverse_signs_match_grade_signature() {
        // grade signs : + + - - +
        let s_one = (IDENTITY).reverse();
        let v_one = (E1).reverse();
        let bv_one = (E12).reverse();
        let tv_one = (E123).reverse();
        let ps_one = (E0123).reverse();
        assert!(mv_approx(&s_one, &IDENTITY));
        assert!(mv_approx(&v_one, &E1));
        assert!(mv_approx(&bv_one, &(-E12)));
        assert!(mv_approx(&tv_one, &(-E123)));
        assert!(mv_approx(&ps_one, &E0123));
    }

    #[test]
    fn grade_involution_signs_match_parity() {
        // signs : + - + - +
        assert!(mv_approx(&IDENTITY.grade_involution(), &IDENTITY));
        assert!(mv_approx(&E1.grade_involution(), &(-E1)));
        assert!(mv_approx(&E12.grade_involution(), &E12));
        assert!(mv_approx(&E123.grade_involution(), &(-E123)));
        assert!(mv_approx(&E0123.grade_involution(), &E0123));
    }

    #[test]
    fn clifford_conjugation_signs_match_kk_plus_one_over_two() {
        // signs : + - - + +
        assert!(mv_approx(&IDENTITY.clifford_conjugation(), &IDENTITY));
        assert!(mv_approx(&E1.clifford_conjugation(), &(-E1)));
        assert!(mv_approx(&E12.clifford_conjugation(), &(-E12)));
        assert!(mv_approx(&E123.clifford_conjugation(), &E123));
        assert!(mv_approx(&E0123.clifford_conjugation(), &E0123));
    }

    // ── dual ─────────────────────────────────────────────────────────────────

    #[test]
    fn dual_is_involutive() {
        let mv = (IDENTITY * 1.5) + (E1 * 2.0) + (E12 * 3.0) + (E0 * 4.0) + (E0123 * 0.7);
        assert!(mv_approx(&mv.dual().dual(), &mv));
    }

    #[test]
    fn dual_swaps_grade_pairs() {
        // grade-0 ↔ grade-4 ; grade-1 ↔ grade-3 ; grade-2 ↔ grade-2.
        assert!(mv_approx(&IDENTITY.dual(), &E0123));
        assert!(mv_approx(&E0123.dual(), &IDENTITY));
        assert!(mv_approx(&E1.dual(), &E032));
        assert!(mv_approx(&E032.dual(), &E1));
    }

    // ── outer / inner / regressive ──────────────────────────────────────────

    #[test]
    fn outer_increases_grade() {
        // e₁ ^ e₂ should be grade-2 (= e₁₂).
        let p = E1 ^ E2;
        assert!(mv_approx(&p, &E12));
    }

    #[test]
    fn inner_decreases_grade_to_scalar_for_same_basis() {
        // e₁ | e₁ = +1 (grade-0).
        let p = E1 | E1;
        assert!(mv_approx(&p, &IDENTITY));
    }

    #[test]
    fn outer_of_collinear_bases_is_zero() {
        // e₁ ^ e₁ = 0 (Grassmann identity).
        let p = E1 ^ E1;
        assert!(mv_approx(&p, &ZERO));
    }

    #[test]
    fn regressive_dual_relation() {
        // For grade-1 elements e₁ and e₂, regressive = (e₁* ^ e₂*)*.
        let lhs = E1 & E2;
        let rhs = (E1.dual() ^ E2.dual()).dual();
        assert!(mv_approx(&lhs, &rhs));
    }

    // ── geometric product : associativity / distributivity ──────────────────

    #[test]
    fn geometric_product_is_associative() {
        let a = (IDENTITY * 0.5) + (E1 * 1.3) + (E23 * 0.7);
        let b = (IDENTITY * 1.1) + (E2 * 0.9) + (E12 * 0.4);
        let c = (IDENTITY * 0.3) + (E3 * 1.7);
        let lhs = (a * b) * c;
        let rhs = a * (b * c);
        assert!(mv_approx(&lhs, &rhs));
    }

    #[test]
    fn geometric_product_distributes_over_addition() {
        let a = (IDENTITY * 0.5) + (E1 * 1.3);
        let b = (IDENTITY * 1.1) + (E2 * 0.9);
        let c = E12 * 0.4;
        let lhs = a * (b + c);
        let rhs = (a * b) + (a * c);
        assert!(mv_approx(&lhs, &rhs));
    }

    // ── sandwich product ────────────────────────────────────────────────────

    #[test]
    fn sandwich_with_identity_is_identity() {
        let v = (IDENTITY * 2.0) + (E1 * 3.0) + (E12 * 5.0);
        let r = IDENTITY.sandwich(&v);
        assert!(mv_approx(&r, &v));
    }

    #[test]
    fn rotor_e12_sandwich_rotates_e1_to_e1_180() {
        // R = e₁₂ acts as a 180-degree rotation in the e₁-e₂ plane.
        // R v R̃ where R = e₁₂, R̃ = -e₁₂.
        // Expected : e₁ → -e₁ (180 deg flip).
        let r = E12;
        let rotated = r.sandwich(&E1);
        assert!(mv_approx(&rotated, &(-E1)));
    }

    // ── operator overloads ──────────────────────────────────────────────────

    #[test]
    fn operator_mul_is_geometric_product() {
        let a = E1 + E2;
        let b = E1 + E3;
        assert!(mv_approx(&(a * b), &a.geometric(&b)));
    }

    #[test]
    fn operator_xor_is_outer_product() {
        let a = E1 + E2;
        let b = E1 + E3;
        assert!(mv_approx(&(a ^ b), &a.outer(&b)));
    }

    #[test]
    fn operator_or_is_inner_product() {
        let a = E1 + E2;
        let b = E1 + E3;
        assert!(mv_approx(&(a | b), &a.inner(&b)));
    }

    #[test]
    fn scalar_multiplication_commutes() {
        let mv = (E1 * 2.0) + (E12 * 3.0);
        let lhs = mv * 5.0;
        let rhs = 5.0 * mv;
        assert!(mv_approx(&lhs, &rhs));
    }

    // ── repr-C layout sanity ────────────────────────────────────────────────

    #[test]
    fn repr_c_layout_size_is_64_bytes() {
        // 16 × f32 = 64 bytes.
        assert_eq!(core::mem::size_of::<Multivector>(), 64);
    }

    #[test]
    fn repr_c_layout_alignment_is_4() {
        assert_eq!(core::mem::align_of::<Multivector>(), 4);
    }
}
