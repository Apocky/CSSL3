//! § basis — G(3,0,1) blade ordering + grade tables
//!
//! § CANONICAL ORDER
//!   The 16 basis blades of G(3,0,1) are stored in a fixed canonical order.
//!   Switching the order = MAJOR-VERSION bump.
//!
//!   ```text
//!   index | blade   | grade | name        | substrate role
//!   ──────┼─────────┼───────┼─────────────┼──────────────────────────────────
//!     0   |   1     |   0   | scalar      | scalar coefficient
//!     1   |   e₁    |   1   | e1          | x-plane (Klein)
//!     2   |   e₂    |   1   | e2          | y-plane (Klein)
//!     3   |   e₃    |   1   | e3          | z-plane (Klein)
//!     4   |   e₀    |   1   | e0          | ideal-plane / horizon (Klein)
//!     5   |   e₂₃   |   2   | e23         | rotation generator about x-axis
//!     6   |   e₃₁   |   2   | e31         | rotation generator about y-axis
//!     7   |   e₁₂   |   2   | e12         | rotation generator about z-axis
//!     8   |   e₀₁   |   2   | e01         | translation generator along x
//!     9   |   e₀₂   |   2   | e02         | translation generator along y
//!    10   |   e₀₃   |   2   | e03         | translation generator along z
//!    11   |   e₀₃₂  |   3   | e032        | x-component of point trivector
//!    12   |   e₀₁₃  |   3   | e013        | y-component of point trivector
//!    13   |   e₀₂₁  |   3   | e021        | z-component of point trivector
//!    14   |   e₁₂₃  |   3   | e123        | normalization-component of point
//!    15   |   e₀₁₂₃ |   4   | e0123       | pseudoscalar / oriented volume
//!   ```
//!
//!   The grade-3 (point) trivectors use the **negated cyclic permutations**
//!   `e₀₃₂ e₀₁₃ e₀₂₁ e₁₂₃` (rather than `e₀₂₃ e₀₃₁ e₀₁₂ e₁₂₃`). This is the
//!   Klein-PGA convention chosen so a point-trivector with components
//!   `(x, y, z, 1)` reads naturally — see [`Point`](crate::Point). The
//!   negated form arises from the index-ordering convention `e₀ ∧ e₃ ∧ e₂ =
//!   -e₀ ∧ e₂ ∧ e₃ = -e₀₂₃` ; applying this consistently keeps the grade-3
//!   blade-multiplication table sign-clean.
//!
//! § BLADE-INDEX HELPERS
//!   - [`Grade`] : the grade (0..=4) of a blade.
//!   - [`grade_of`] : array lookup → grade for index 0..16.
//!   - [`BASIS_NAMES`] : the 16 blade names for `Debug` / pretty-printing.
//!   - [`E0`] .. [`E0123`] : the 16 unit-blade [`Multivector`] constants.

use crate::multivector::Multivector;

/// Total number of basis blades in G(3,0,1) — `2^4 = 16`.
pub const BLADE_COUNT: usize = 16;

/// The five grades of G(3,0,1).
///
/// Grade-0 is the scalar component, grade-1 is the vector subspace
/// (planes in Klein-PGA), grade-2 is the bivector subspace
/// (lines / motion generators), grade-3 is the trivector subspace
/// (points in Klein-PGA), grade-4 is the pseudoscalar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Grade {
    /// Scalar — single component at index 0.
    Scalar,
    /// Vector — four components (e₁ e₂ e₃ e₀).
    Vector,
    /// Bivector — six components (e₂₃ e₃₁ e₁₂ e₀₁ e₀₂ e₀₃).
    Bivector,
    /// Trivector — four components (e₀₃₂ e₀₁₃ e₀₂₁ e₁₂₃).
    Trivector,
    /// Pseudoscalar — single component at index 15.
    Pseudoscalar,
}

impl Grade {
    /// Numeric grade (0..=4).
    #[must_use]
    pub const fn rank(self) -> usize {
        match self {
            Grade::Scalar => 0,
            Grade::Vector => 1,
            Grade::Bivector => 2,
            Grade::Trivector => 3,
            Grade::Pseudoscalar => 4,
        }
    }

    /// Half-open index range `[lo, hi)` of blades of this grade in the
    /// canonical 16-component layout.
    #[must_use]
    pub const fn index_range(self) -> (usize, usize) {
        match self {
            Grade::Scalar => (0, 1),
            Grade::Vector => (1, 5),
            Grade::Bivector => (5, 11),
            Grade::Trivector => (11, 15),
            Grade::Pseudoscalar => (15, 16),
        }
    }
}

/// Grade lookup for each of the 16 canonical blade indices.
const GRADE_TABLE: [Grade; BLADE_COUNT] = [
    Grade::Scalar,                // 0  : 1
    Grade::Vector,                // 1  : e1
    Grade::Vector,                // 2  : e2
    Grade::Vector,                // 3  : e3
    Grade::Vector,                // 4  : e0
    Grade::Bivector,              // 5  : e23
    Grade::Bivector,              // 6  : e31
    Grade::Bivector,              // 7  : e12
    Grade::Bivector,              // 8  : e01
    Grade::Bivector,              // 9  : e02
    Grade::Bivector,              // 10 : e03
    Grade::Trivector,             // 11 : e032
    Grade::Trivector,             // 12 : e013
    Grade::Trivector,             // 13 : e021
    Grade::Trivector,             // 14 : e123
    Grade::Pseudoscalar,          // 15 : e0123
];

/// Look up the grade of the blade at canonical index `i`.
///
/// # Panics
/// Panics if `i >= 16`.
#[must_use]
pub const fn grade_of(i: usize) -> Grade {
    GRADE_TABLE[i]
}

/// Names of the 16 canonical blades, in the order they appear in
/// [`Multivector`] storage. Used by `Debug` / pretty-printers.
pub const BASIS_NAMES: [&str; BLADE_COUNT] = [
    "1", "e1", "e2", "e3", "e0", "e23", "e31", "e12", "e01", "e02", "e03", "e032", "e013", "e021",
    "e123", "e0123",
];

// ── unit-blade Multivector constants ────────────────────────────────────────
//
// These const constructors give human-readable names to the canonical basis
// blades. They are the typical way to write a literal multivector — e.g.
// `2.0 * E1 + 3.0 * E2` — and underpin most tests.

/// The zero multivector — every component is 0.
pub const ZERO: Multivector = Multivector::from_array([0.0; BLADE_COUNT]);

/// The scalar identity multivector — component 0 is 1, all others 0.
pub const IDENTITY: Multivector = {
    let mut a = [0.0_f32; BLADE_COUNT];
    a[0] = 1.0;
    Multivector::from_array(a)
};

macro_rules! basis_const {
    ($name:ident, $idx:expr) => {
        #[doc = concat!("Unit basis blade `", stringify!($name), "` at canonical index ", stringify!($idx), ".")]
        pub const $name: Multivector = {
            let mut a = [0.0_f32; BLADE_COUNT];
            a[$idx] = 1.0;
            Multivector::from_array(a)
        };
    };
}

// grade-1 (planes in Klein-PGA)
basis_const!(E1, 1);
basis_const!(E2, 2);
basis_const!(E3, 3);
basis_const!(E0, 4);

// grade-2 (lines / motion generators)
basis_const!(E23, 5);
basis_const!(E31, 6);
basis_const!(E12, 7);
basis_const!(E01, 8);
basis_const!(E02, 9);
basis_const!(E03, 10);

// grade-3 (points in Klein-PGA)
basis_const!(E032, 11);
basis_const!(E013, 12);
basis_const!(E021, 13);
basis_const!(E123, 14);

// grade-4 (pseudoscalar)
basis_const!(E0123, 15);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grade_table_self_consistent() {
        // The `index_range` must enumerate exactly the indices of that grade.
        for &g in &[
            Grade::Scalar,
            Grade::Vector,
            Grade::Bivector,
            Grade::Trivector,
            Grade::Pseudoscalar,
        ] {
            let (lo, hi) = g.index_range();
            for i in lo..hi {
                assert_eq!(grade_of(i), g, "blade {i} grade mismatch");
            }
        }
    }

    #[test]
    fn grade_count_check() {
        // 1 + 4 + 6 + 4 + 1 = 16.
        assert_eq!(Grade::Scalar.index_range().1 - Grade::Scalar.index_range().0, 1);
        assert_eq!(Grade::Vector.index_range().1 - Grade::Vector.index_range().0, 4);
        assert_eq!(Grade::Bivector.index_range().1 - Grade::Bivector.index_range().0, 6);
        assert_eq!(Grade::Trivector.index_range().1 - Grade::Trivector.index_range().0, 4);
        assert_eq!(Grade::Pseudoscalar.index_range().1 - Grade::Pseudoscalar.index_range().0, 1);
    }

    #[test]
    fn basis_names_count_matches_blades() {
        assert_eq!(BASIS_NAMES.len(), BLADE_COUNT);
    }

    #[test]
    fn rank_matches_grade_enum_position() {
        assert_eq!(Grade::Scalar.rank(), 0);
        assert_eq!(Grade::Vector.rank(), 1);
        assert_eq!(Grade::Bivector.rank(), 2);
        assert_eq!(Grade::Trivector.rank(), 3);
        assert_eq!(Grade::Pseudoscalar.rank(), 4);
    }

    #[test]
    fn unit_blade_constants_have_correct_index_set() {
        // Each constant has exactly one nonzero coefficient, and that
        // coefficient is at the expected canonical index.
        let pairs: &[(Multivector, usize)] = &[
            (E1, 1),
            (E2, 2),
            (E3, 3),
            (E0, 4),
            (E23, 5),
            (E31, 6),
            (E12, 7),
            (E01, 8),
            (E02, 9),
            (E03, 10),
            (E032, 11),
            (E013, 12),
            (E021, 13),
            (E123, 14),
            (E0123, 15),
        ];
        for (mv, idx) in pairs {
            for i in 0..BLADE_COUNT {
                let want = if i == *idx { 1.0 } else { 0.0 };
                assert_eq!(
                    mv.coefficient(i),
                    want,
                    "blade name {} component {} expected {}",
                    BASIS_NAMES[*idx],
                    i,
                    want
                );
            }
        }
    }

    #[test]
    fn identity_blade_is_scalar_one() {
        assert_eq!(IDENTITY.coefficient(0), 1.0);
        for i in 1..BLADE_COUNT {
            assert_eq!(IDENTITY.coefficient(i), 0.0);
        }
    }

    #[test]
    fn zero_blade_is_all_zero() {
        for i in 0..BLADE_COUNT {
            assert_eq!(ZERO.coefficient(i), 0.0);
        }
    }
}
