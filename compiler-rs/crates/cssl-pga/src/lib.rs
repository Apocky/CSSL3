//! § cssl-pga — Projective Geometric Algebra G(3,0,1)
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Foundation crate for **Axiom-10 § I** (OPUS Mathematical Primitives).
//!   Provides the canonical PGA primitives — multivectors, geometric/outer/
//!   inner/regressive products, motors (rigid motions), rotors (rotations),
//!   translators (translations), exp/log map (Lie-algebra ↔ Lie-group), and
//!   the sandwich-product `M v M̃` that transforms any geometric primitive
//!   under a rigid motion.
//!
//!   PGA replaces five separate machineries from the legacy `cssl-math`
//!   surface with one algebraically-closed system :
//!
//!   | legacy                  | PGA replacement                            |
//!   |-------------------------|--------------------------------------------|
//!   | `Quat` (rotations)      | grade-2 unit rotor (3 bivector components) |
//!   | dual-quaternion (rigid) | grade-2 motor (rotor ⊕ translator)         |
//!   | `Mat4` (rigid xform)    | motor + sandwich product                    |
//!   | Plücker line coords     | grade-2 line bivector                       |
//!   | homogeneous point coord | grade-3 trivector (Klein convention)        |
//!
//!   The legacy `cssl-math` surface is NOT removed — it is the lower-fidelity
//!   `f32`-only surface used by the renderer hot-paths and the GPU upload
//!   buffers. `cssl-pga` is the **algebraically-closed** surface used by
//!   physics solvers, manifold-aware integrators, body-omnoid bivector
//!   dynamics, and the FieldCell `multivec_dynamics_lo` field.
//!
//! § SPEC ANCHOR
//!   - `Omniverse/01_AXIOMS/10_OPUS_MATH.csl § I` — PGA primitive specification.
//!   - `Omniverse/01_AXIOMS/10_OPUS_MATH.csl § VII` — composition guarantee
//!     with KAN, MERA, RC, PH, and Operads.
//!   - `Omniverse/01_AXIOMS/10_OPUS_MATH.csl § IX` — anti-pattern table.
//!     "Quaternion + matrix mixing" is forbidden ; PGA is the canonical path.
//!
//! § SIGNATURE — G(3,0,1)
//!   Basis vectors satisfy :
//!     `e₁² = +1`, `e₂² = +1`, `e₃² = +1`, `e₀² = 0` (degenerate / null).
//!   The null `e₀` direction encodes the projective ideal — points at
//!   infinity, parallel-line meets, plane normals — without the catastrophic
//!   `0/0` divisions of the homogeneous-coordinate formulation.
//!
//! § GRADE STRUCTURE — 16 components total
//!     grade-0 : scalar           1 component   `s`
//!     grade-1 : vector           4 components  `e₁ e₂ e₃ e₀` (planes in Klein)
//!     grade-2 : bivector         6 components  `e₂₃ e₃₁ e₁₂ e₀₁ e₀₂ e₀₃`
//!                                              (3 spatial + 3 ideal — lines /
//!                                               rotors+translators)
//!     grade-3 : trivector        4 components  `e₀₃₂ e₀₁₃ e₀₂₁ e₁₂₃`
//!                                              (points in Klein-style PGA)
//!     grade-4 : pseudoscalar     1 component   `e₀₁₂₃` (volume / orientation)
//!     ─────────────────────────────────────────
//!                                16 components → power-of-2 ✓
//!
//! § KLEIN-STYLE CONVENTION
//!   This crate uses the **plane-based** PGA convention popularized by the
//!   Klein PGA Rust library and the Bivector.net pedagogical materials :
//!   grade-1 vectors represent **planes** (not points), grade-3 trivectors
//!   represent **points** (not planes). The duality is :
//!
//!   ```text
//!   point ⊻ point = line          (join — grade-3 ∨ grade-3 = grade-2)
//!   plane ∧ plane = line          (meet — grade-1 ∧ grade-1 = grade-2)
//!   plane ∧ point = scalar        (incidence — sign tells side)
//!   line ∨ point  = plane         (build plane through line and point)
//!   line ∧ plane  = point         (intersect line with plane)
//!   ```
//!
//!   This convention makes rigid-motion **bivectors** the natural generators
//!   of motion : rotation generators live in the spatial-bivector subspace
//!   (`e₂₃ e₃₁ e₁₂` ; equivalent to quaternion imaginaries), translation
//!   generators live in the ideal-bivector subspace (`e₀₁ e₀₂ e₀₃` ; equivalent
//!   to dual-quaternion translation epsilon-coefficients).
//!
//! § STORAGE LAYOUT
//!   Every PGA type is `#[repr(C)]`. The full [`Multivector`] is 16 floats
//!   in a fixed canonical order (see [`mv`]). Specialized lower-storage types
//!   for the common cases ([`Plane`], [`Line`], [`Point`], [`Rotor`],
//!   [`Translator`], [`Motor`]) are first-class — operations on them stay in
//!   the lower-storage representation rather than expanding to a full
//!   16-component multivector.
//!
//! § F32 + F64 PARALLEL SURFACES
//!   Every type has both `f32` and `f64` variants — `Multivector32`/
//!   `Multivector64`, `Motor32`/`Motor64`, etc. The `f32` variants alias to
//!   the unqualified type names ([`Multivector`] ≡ [`Multivector32`]) for
//!   ergonomic parity with `cssl-math`. The `f64` variants are used by
//!   physics solvers, manifold-aware integrators, and any path that
//!   accumulates many compositions — bivector dynamics integrating over
//!   simulation-frame timesteps drifts off-manifold without `f64` precision
//!   in the exp/log paths.
//!
//! § NUMERIC STABILITY DISCIPLINE
//!   All operations are TOTAL — never produce NaN or infinity for finite
//!   inputs. Degenerate cases (motor of zero, log of identity, exp of large
//!   bivector) return a sensible sentinel. The exp/log paths are written
//!   for round-trip closure : `bv.exp().log() ≈ bv` to within f32/f64 ULP
//!   precision for bivectors in the principal range. The sandwich product
//!   guarantees orientation preservation : reflection × reflection = rotation
//!   (no parity flip drift).
//!
//! § SIMD-AWARENESS
//!   The `Multivector` storage is laid out so the 16-component representation
//!   maps to four 128-bit SSE registers (or one 512-bit AVX-512 register).
//!   The geometric-product expansion is written in a form the compiler
//!   auto-vectorizes for the common subexpressions ; hand-tuned `simd::*`
//!   variants are deferred to a follow-up perf slice.
//!
//! § INTEGRATION POINTS
//!   - **FieldCell `multivec_dynamics_lo`** : the substrate `Ω`-field cell
//!     stores low-order multivector dynamics (typically grade-0 + grade-2
//!     + grade-4 = even subalgebra = motor space). Per-cell motors compose
//!     via [`Motor::compose`] under the operad-of-cell-merges (Axiom 6 § VI).
//!   - **Body-omnoid bivector dynamics** : rigid bodies have a state of
//!     `(pose : Motor, twist : Bivector)`. Time-integration is
//!     `pose.compose(twist.exp_motor(dt))` (Lie-group integration on SE(3)).
//!   - **Manifold-aware rotations** : interpolating between two rotors via
//!     `r0.log().slerp_bv(r1.log(), t).exp_rotor()` stays on the rotation
//!     manifold by construction — no slerp-near-collinear fallback needed.
//!   - **Plane-based reflection** : `Plane::reflect(p)` is the sandwich
//!     `π p π̃` ; composition-of-reflections produces rigid motions
//!     algebraically. The legacy `cssl-math::Plane::reflect_point` is the
//!     `f32`-only Vec3-based shortcut ; the PGA path is the canonical one.
//!
//! § PRIME-DIRECTIVE
//!   Pure compute. No I/O, no logging, no allocation (every type is a
//!   stack-resident `Copy`). Behavior is what it appears to be — total,
//!   deterministic, transparent. The sandwich product preserves orientation
//!   by construction ; the exp/log map is bit-exact round-trip on finite
//!   bivectors in the principal range.

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
// § Style allowances — same set as cssl-math (the foundation surface) so the
// lint posture reads consistently across foundation crates.
#![allow(clippy::similar_names)]
#![allow(clippy::many_single_char_names)]
#![allow(clippy::neg_multiply)]
#![allow(clippy::suboptimal_flops)]
#![allow(clippy::float_cmp)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::excessive_precision)]
#![allow(clippy::approx_constant)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::needless_range_loop)]
#![allow(clippy::redundant_closure_for_method_calls)]

pub mod basis;
pub mod multivector;
pub mod multivector_f64;
pub mod plane;
pub mod line;
pub mod point;
pub mod rotor;
pub mod translator;
pub mod motor;
pub mod expmap;
pub mod ops;

#[cfg(feature = "cssl-math-bridge")]
pub mod bridge;

// ── public re-exports — f32 surface ────────────────────────────────────────
pub use basis::{Grade, BASIS_NAMES};
pub use multivector::Multivector as Multivector32;
pub use multivector_f64::Multivector as Multivector64;
pub use plane::Plane;
pub use line::Line;
pub use point::Point;
pub use rotor::Rotor;
pub use translator::Translator;
pub use motor::Motor;

/// Convenience alias — the unqualified `Multivector` is the `f32` variant,
/// matching `cssl-math` ergonomic conventions. Use [`Multivector64`]
/// explicitly when the f64 path is required.
pub type Multivector = Multivector32;

/// Crate version exposed for scaffold verification — mirrors the same
/// constant pattern used elsewhere in the workspace.
pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod scaffold_tests {
    use super::STAGE0_SCAFFOLD;

    #[test]
    fn scaffold_version_present() {
        assert!(!STAGE0_SCAFFOLD.is_empty());
    }
}

/// Convenience module re-exporting the canonical short names for
/// constructing multivectors. `use cssl_pga::mv::*;` brings the basis-blade
/// constants and the constructors into scope.
pub mod mv {
    pub use crate::basis::{
        E0, E01, E0123, E013, E02, E021, E03, E032, E1, E12, E123, E2, E23, E3, E31, IDENTITY, ZERO,
    };
    pub use crate::multivector::Multivector;
}
