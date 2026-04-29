//! T11-D141 — LUT-baking demo : build a 256-entry sine table at compile-time.
//!
//! § ROLE
//!   Demonstrate the end-to-end `#run` comptime-eval pipeline by generating
//!   a 256-element sine LUT at compile time and baking it into MIR as a
//!   `cssl.array.assemble` op. The downstream specializer (T11-D142) consumes
//!   the baked LUT and substitutes it for runtime sine calls in a hot fragment
//!   shader (per `specs/06_STAGING § COMPTIME EVALUATION`'s canonical example).
//!
//! § PIPELINE
//!   1. Build a Rust-side analytic value `sin(2π * i / 256)` for `i ∈ [0, 256)`.
//!   2. Wrap each as a [`crate::comptime::ComptimeResult`].
//!   3. Bake all 256 results via [`crate::bake::bake_lut`].
//!
//!   The MIR-level invocation path (HIR → synthetic-fn → JIT) for each
//!   element is a matching exercise — the LUT-baking demo focuses on the
//!   *embedding* shape ; the per-element JIT path is exercised by the
//!   scalar-comptime-eval tests in `comptime::tests`. This separation keeps
//!   the demo deterministic + fast (256 JIT roundtrips would dominate the
//!   test-suite runtime).
//!
//! § OUTPUT
//!   - [`build_sine_lut`] returns a `Vec<ComptimeResult>` of length 256.
//!   - [`bake_sine_lut_mir`] returns the [`crate::bake::BakedOps`] sequence.
//!   - [`integrate_sine_lut_into_module`] inserts the baked op-sequence as a
//!     synthetic `__sine_lut_init` fn in a `MirModule`. This shape is what the
//!     T11-D142 specialization pass picks up.

use cssl_mir::{FloatWidth, MirFunc, MirModule, MirType};

use crate::bake::{bake_lut, BakedOps};
use crate::comptime::{ComptimeResult, ComptimeValue};

/// Number of entries in the canonical sine LUT.
pub const SINE_LUT_SIZE: usize = 256;

/// Build the LUT of `Sin(2π * i / N)` for `i ∈ [0, N)` as `f32`. Returns one
/// [`ComptimeResult`] per entry. The underlying value is computed in Rust
/// (the comptime-eval pipeline's per-element JIT round-trip is exercised by
/// the scalar-comptime-eval tests) ; the demo asserts on the *embedding shape*,
/// not the per-element JIT round-trip.
#[must_use]
pub fn build_sine_lut() -> Vec<ComptimeResult> {
    let mut out = Vec::with_capacity(SINE_LUT_SIZE);
    let n = SINE_LUT_SIZE as f32;
    for i in 0..SINE_LUT_SIZE {
        let theta = (i as f32) / n * std::f32::consts::TAU;
        let v = theta.sin();
        let value = ComptimeValue::Float(f64::from(v), FloatWidth::F32);
        let bytes = v.to_ne_bytes().to_vec();
        out.push(ComptimeResult {
            bytes,
            ty: MirType::Float(FloatWidth::F32),
            value,
        });
    }
    out
}

/// Bake the sine LUT into a sequence of MIR ops. Returns the op-sequence + the
/// final ValueId pointing at the assembled LUT array.
#[must_use]
pub fn bake_sine_lut_mir() -> BakedOps {
    let lut = build_sine_lut();
    let mut next_value_id: u32 = 0;
    bake_lut(&lut, &mut next_value_id)
}

/// Integrate the baked sine LUT into a synthetic init fn within `module`.
/// The fn is named `__sine_lut_init` and has zero params + a single result of
/// type `!cssl.lut<f32>` carrying the assembled LUT array.
///
/// Returns the index of the inserted fn within `module.funcs`.
pub fn integrate_sine_lut_into_module(module: &mut MirModule) -> usize {
    let baked = bake_sine_lut_mir();
    let result_ty = baked.result_ty.clone();
    let mut init_fn = MirFunc::new(
        "__sine_lut_init".to_string(),
        Vec::new(),
        vec![result_ty.clone()],
    );
    init_fn.next_value_id = baked.result_id.0.saturating_add(1);
    init_fn.attributes.push((
        "comptime_baked".to_string(),
        format!("sine_lut[{SINE_LUT_SIZE}]"),
    ));
    if let Some(entry) = init_fn.body.entry_mut() {
        for op in baked.ops {
            entry.push(op);
        }
        // Emit `func.return` of the assembled LUT.
        entry.push(
            cssl_mir::MirOp::std("func.return")
                .with_operand(baked.result_id)
                .with_attribute("source_loc", "<comptime-lut>"),
        );
    }
    let idx = module.funcs.len();
    module.push_func(init_fn);
    idx
}

/// Convenience : sample the analytical sine LUT at index `i ∈ [0, N)` and
/// return the f32 value. Used by tests to assert that `build_sine_lut()` matches
/// the canonical analytical form exactly.
#[must_use]
pub fn analytical_sine_at(i: usize) -> f32 {
    let n = SINE_LUT_SIZE as f32;
    let theta = (i as f32) / n * std::f32::consts::TAU;
    theta.sin()
}
