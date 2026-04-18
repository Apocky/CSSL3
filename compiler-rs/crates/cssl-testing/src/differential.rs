//! Differential-backend oracle (`@differential`) — R18 central-test.
//!
//! § SPEC   : `specs/23_TESTING.csl` § oracle-modes • differential.
//! § GATE   : T28 (OG8) ship-gate — Vulkan × Level-Zero bit-exact on same SPIR-V.
//! § ROLE   : runs same compiled kernel on multiple backends with seeded inputs;
//!            compares outputs bit-exact for `{PureDet}`-tagged fns, with ULP-tolerance
//!            for non-PureDet.
//! § STATUS : T11-phase-2b live (abstract two-impl comparator + ULP-helper) ;
//!            real Vulkan × Level-Zero dispatch deferred to T10-phase-2 FFI.

/// Backend registered for differential comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Backend {
    /// Vulkan 1.4.333 via `cssl-host-vulkan`.
    Vulkan,
    /// Intel Level-Zero compute via `cssl-host-level-zero`.
    LevelZero,
    /// D3D12 via `cssl-host-d3d12` (Windows).
    D3d12,
    /// Metal via `cssl-host-metal` (Apple).
    Metal,
    /// WebGPU via `cssl-host-webgpu` (browser + native).
    WebGpu,
    /// CPU reference implementation (used as the oracle for correctness).
    CpuRef,
}

/// Config for the `@differential` oracle.
#[derive(Debug, Clone)]
pub struct Config {
    /// Backends to compare (minimum length 2 at T10; at T1 this list is empty stub).
    pub backends: Vec<Backend>,
    /// If `true`, require byte-for-byte equality (used with `{PureDet}` tag).
    /// If `false`, `ulp_tolerance` bounds allowed float divergence.
    pub pure_det: bool,
    /// ULPs of allowed float divergence when `pure_det == false`.
    pub ulp_tolerance: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            backends: Vec::new(),
            pure_det: true,
            ulp_tolerance: 0,
        }
    }
}

/// Outcome of running the `@differential` oracle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Stage0 stub — body populated at T10.
    Stage0Unimplemented,
    /// All backends produced matching output within tolerance.
    Ok,
    /// Backend `Backend` diverged at the described input.
    Divergence {
        backend: Backend,
        delta: String,
        message: String,
    },
}

/// Dispatcher trait for `@differential` oracle.
pub trait Dispatcher {
    /// Execute the oracle for the configured backend matrix.
    fn run(&self, config: &Config) -> Outcome;
}

/// Stage0 stub dispatcher — always returns `Stage0Unimplemented`.
#[derive(Debug, Default, Clone, Copy)]
pub struct Stage0Stub;

impl Dispatcher for Stage0Stub {
    fn run(&self, _config: &Config) -> Outcome {
        Outcome::Stage0Unimplemented
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § Live two-impl comparator : abstract over backend-identity, useful for
//   validating CPU-ref vs Cranelift-JIT, or Vulkan vs Level-Zero once FFI lands.
// ─────────────────────────────────────────────────────────────────────────

/// Compare two implementations `a` + `b` on a shared input-sequence. Returns
/// `Ok` if every input produces equal output under `eq`, `Divergence`
/// (tagged with `backend_b`) at the first mismatch.
///
/// `backend_a` + `backend_b` are labels — used only in the `Divergence`
/// report. Typical usage : `backend_a = CpuRef`, `backend_b = <target>`.
///
/// `eq` : for `{PureDet}`-tagged fns, use exact `==` ; for float outputs,
/// use an ULP-based predicate like `|x, y| ulp_diff_f32(x, y) <= tolerance`.
pub fn check_two_impls<T, U, A, B, Eq>(
    inputs: &[T],
    backend_a: Backend,
    mut a: A,
    backend_b: Backend,
    mut b: B,
    eq: Eq,
) -> Outcome
where
    T: core::fmt::Debug,
    U: core::fmt::Debug,
    A: FnMut(&T) -> U,
    B: FnMut(&T) -> U,
    Eq: Fn(&U, &U) -> bool,
{
    for (i, inp) in inputs.iter().enumerate() {
        let out_a = a(inp);
        let out_b = b(inp);
        if !eq(&out_a, &out_b) {
            return Outcome::Divergence {
                backend: backend_b,
                delta: format!(
                    "input[{i}]={inp:?} • {backend_a:?}={out_a:?} • {backend_b:?}={out_b:?}"
                ),
                message: format!("differential backend-mismatch at input[{i}]"),
            };
        }
    }
    Outcome::Ok
}

/// ULP-distance between two `f32` values. Equivalent to `|bits(a) - bits(b)|`
/// on the total-ordered IEEE-754 space. Returns `u32::MAX` for NaN inputs.
/// `ulp_diff_f32(+0.0, -0.0) == 1` (they're adjacent in the total ordering).
#[must_use]
pub fn ulp_diff_f32(a: f32, b: f32) -> u32 {
    if a.is_nan() || b.is_nan() {
        return u32::MAX;
    }
    sortable_u32(a).abs_diff(sortable_u32(b))
}

/// Total-ordering bit-reinterpretation : maps `f32` to `u32` such that the
/// `u32` ordering matches the numeric ordering on non-NaN floats. Positive
/// floats get the sign-bit toggled (moving them into the upper half of
/// `u32`-space) ; negative floats are fully bit-inverted (so more-negative
/// maps to smaller values). Standard trick for ULP-distance.
fn sortable_u32(x: f32) -> u32 {
    let bits = x.to_bits();
    if bits & 0x8000_0000 != 0 {
        !bits
    } else {
        bits ^ 0x8000_0000
    }
}

/// Predicate helper : `|a, b| ulp_diff_f32(*a, *b) <= tolerance`.
/// Use as the `eq` argument of `check_two_impls` when comparing float outputs.
pub fn ulp_tolerant_eq_f32(tolerance: u32) -> impl Fn(&f32, &f32) -> bool {
    move |a, b| ulp_diff_f32(*a, *b) <= tolerance
}

#[cfg(test)]
mod tests {
    use super::{
        check_two_impls, ulp_diff_f32, ulp_tolerant_eq_f32, Backend, Config, Dispatcher, Outcome,
        Stage0Stub,
    };

    #[test]
    fn stub_returns_unimplemented() {
        assert_eq!(
            Stage0Stub.run(&Config::default()),
            Outcome::Stage0Unimplemented
        );
    }

    #[test]
    fn two_matching_impls_are_ok() {
        let inputs = [1i64, 2, 3, 10, 100];
        let outcome = check_two_impls(
            &inputs,
            Backend::CpuRef,
            |x: &i64| x * 2,
            Backend::Vulkan,
            |x: &i64| x + x,
            |a, b| a == b,
        );
        assert_eq!(outcome, Outcome::Ok);
    }

    #[test]
    fn divergence_pinpoints_failing_backend() {
        let inputs = [1i64, 2, 3];
        let outcome = check_two_impls(
            &inputs,
            Backend::CpuRef,
            |x: &i64| x + 1,
            Backend::LevelZero,
            |x: &i64| x + 2, // bug : off-by-one
            |a, b| a == b,
        );
        match outcome {
            Outcome::Divergence {
                backend,
                delta,
                message,
            } => {
                assert_eq!(backend, Backend::LevelZero);
                assert!(delta.contains("input[0]=1"));
                assert!(delta.contains("CpuRef=2"));
                assert!(delta.contains("LevelZero=3"));
                assert!(message.contains("input[0]"));
            }
            other => panic!("expected Divergence, got {other:?}"),
        }
    }

    #[test]
    fn ulp_diff_zero_for_identical_floats() {
        assert_eq!(ulp_diff_f32(1.0, 1.0), 0);
        assert_eq!(ulp_diff_f32(-1.0, -1.0), 0);
        assert_eq!(ulp_diff_f32(0.0, -0.0), 1); // +0 and -0 are adjacent in total-order
    }

    #[test]
    fn ulp_diff_one_for_adjacent_floats() {
        let a = 1.0f32;
        let b = f32::from_bits(a.to_bits() + 1);
        assert_eq!(ulp_diff_f32(a, b), 1);
    }

    #[test]
    fn ulp_diff_nan_is_max() {
        assert_eq!(ulp_diff_f32(f32::NAN, 1.0), u32::MAX);
        assert_eq!(ulp_diff_f32(1.0, f32::NAN), u32::MAX);
    }

    #[test]
    fn ulp_tolerant_eq_accepts_close_floats() {
        let eq = ulp_tolerant_eq_f32(4);
        let a = 1.0f32;
        let b = f32::from_bits(a.to_bits() + 3);
        assert!(eq(&a, &b));
        let c = f32::from_bits(a.to_bits() + 5);
        assert!(!eq(&a, &c));
    }

    #[test]
    fn check_two_impls_with_ulp_tolerance() {
        let inputs = [0.1f32, 0.2, 0.3, 1.0, 10.0];
        // Both impls compute `x * 2` but one uses `x + x` instead (equivalent modulo float-order).
        let outcome = check_two_impls(
            &inputs,
            Backend::CpuRef,
            |x: &f32| x * 2.0,
            Backend::Vulkan,
            |x: &f32| x + x,
            ulp_tolerant_eq_f32(1),
        );
        assert_eq!(outcome, Outcome::Ok);
    }

    #[test]
    fn empty_inputs_is_ok() {
        let inputs: [i64; 0] = [];
        let outcome = check_two_impls(
            &inputs,
            Backend::CpuRef,
            |x: &i64| *x,
            Backend::Vulkan,
            |x: &i64| *x,
            |a, b| a == b,
        );
        assert_eq!(outcome, Outcome::Ok);
    }
}
