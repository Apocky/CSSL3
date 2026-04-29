//! T11-D141 — KAN-weight comptime-bake mock demo.
//!
//! § ROLE
//!   Demonstrate that a KAN (Kolmogorov-Arnold Network) layer's weight matrix
//!   can be trained / pre-computed at compile time and baked into MIR as a
//!   single `cssl.struct.assemble` op carrying the weight tensor + activation
//!   coefficients.
//!
//!   This is a **mock** training fn — the real `train_kan_offline(..)` would
//!   run a full KAN training loop (LBFGS / Adam over a small dataset). At
//!   stage-0 we generate deterministic synthetic weights that satisfy the
//!   shape constraints + carry recognizable canonical values so downstream
//!   tests can assert on the bake.
//!
//! § PIPELINE
//!   1. Run [`mock_train_kan_layer`] — produces a [`MockKanLayer`] with
//!      `(in_dim, out_dim, knot_count)` plus the flat weight + bias buffers.
//!   2. Wrap as a [`crate::comptime::ComptimeResult`] (`ComptimeValue::Struct`).
//!   3. Bake via [`crate::bake::bake_result`] → [`crate::bake::BakedOps`].
//!
//!   The downstream T11-D142 specializer integrates the baked-in weights with
//!   the runtime KAN-eval kernel by replacing the `kan.layer.lookup` op with
//!   the assembled-struct value, eliminating the runtime weight-load.

use cssl_mir::{FloatWidth, IntWidth, MirFunc, MirModule, MirType};

use crate::bake::{bake_result, BakedOps};
use crate::comptime::{ComptimeResult, ComptimeValue};

/// One KAN layer's metadata + weight+bias buffer (flat, row-major).
#[derive(Debug, Clone, PartialEq)]
pub struct MockKanLayer {
    pub in_dim: u32,
    pub out_dim: u32,
    pub knot_count: u32,
    /// Flat weights buffer. Length = `in_dim × out_dim × knot_count`.
    pub weights: Vec<f32>,
    /// Per-output bias. Length = `out_dim`.
    pub biases: Vec<f32>,
}

impl MockKanLayer {
    /// Total weight count.
    #[must_use]
    pub fn weight_len(&self) -> usize {
        (self.in_dim as usize)
            .saturating_mul(self.out_dim as usize)
            .saturating_mul(self.knot_count as usize)
    }

    /// Total bytes when baked (i32 dims + f32 weight/bias arrays).
    #[must_use]
    pub fn baked_byte_size(&self) -> usize {
        const DIM_BYTES: usize = 4 * 3; // in_dim + out_dim + knot_count
        DIM_BYTES + self.weights.len() * 4 + self.biases.len() * 4
    }
}

/// Run the mock training fn. Returns a deterministic [`MockKanLayer`] with
/// canonical synthetic weights.
///
/// The "training" is :
///   - `weights[k]` = `0.5 + 0.01 × k` (deterministic ramp)
///   - `biases[j]`  = `0.1 × j`
///
/// Real training is out-of-scope for this slice — the goal is to prove the
/// bake-pipeline works end-to-end on a multi-field struct value.
#[must_use]
pub fn mock_train_kan_layer(in_dim: u32, out_dim: u32, knot_count: u32) -> MockKanLayer {
    let weight_len = (in_dim as usize)
        .saturating_mul(out_dim as usize)
        .saturating_mul(knot_count as usize);
    let mut weights = Vec::with_capacity(weight_len);
    for k in 0..weight_len {
        weights.push(0.01_f32.mul_add(k as f32, 0.5_f32));
    }
    let mut biases = Vec::with_capacity(out_dim as usize);
    for j in 0..out_dim {
        biases.push(0.1_f32 * (j as f32));
    }
    MockKanLayer {
        in_dim,
        out_dim,
        knot_count,
        weights,
        biases,
    }
}

/// Wrap a [`MockKanLayer`] as a [`ComptimeResult`] suitable for baking.
#[must_use]
pub fn kan_layer_as_comptime(layer: &MockKanLayer) -> ComptimeResult {
    let weight_field = ComptimeValue::Array(
        layer
            .weights
            .iter()
            .map(|w| ComptimeValue::Float(f64::from(*w), FloatWidth::F32))
            .collect(),
    );
    let bias_field = ComptimeValue::Array(
        layer
            .biases
            .iter()
            .map(|b| ComptimeValue::Float(f64::from(*b), FloatWidth::F32))
            .collect(),
    );
    let in_dim_field = ComptimeValue::Int(i64::from(layer.in_dim), IntWidth::I32);
    let out_dim_field = ComptimeValue::Int(i64::from(layer.out_dim), IntWidth::I32);
    let knot_field = ComptimeValue::Int(i64::from(layer.knot_count), IntWidth::I32);
    let value = ComptimeValue::Struct(vec![
        ("in_dim".to_string(), in_dim_field),
        ("out_dim".to_string(), out_dim_field),
        ("knot_count".to_string(), knot_field),
        ("weights".to_string(), weight_field),
        ("biases".to_string(), bias_field),
    ]);
    let bytes = crate::comptime::encode_value_bytes_pub(&value);
    ComptimeResult {
        bytes,
        ty: MirType::Opaque("!cssl.kan.layer".to_string()),
        value,
    }
}

/// Train + bake a KAN layer end-to-end. Convenience wrapper invoked by the
/// integration test.
#[must_use]
pub fn bake_kan_layer_mir(in_dim: u32, out_dim: u32, knot_count: u32) -> BakedOps {
    let layer = mock_train_kan_layer(in_dim, out_dim, knot_count);
    let result = kan_layer_as_comptime(&layer);
    let mut next_value_id: u32 = 0;
    bake_result(&result, &mut next_value_id)
}

/// Integrate a baked KAN layer into a MIR module as a `__kan_<name>_init` fn.
/// Returns the index of the inserted fn.
pub fn integrate_kan_layer_into_module(
    module: &mut MirModule,
    name: &str,
    in_dim: u32,
    out_dim: u32,
    knot_count: u32,
) -> usize {
    let baked = bake_kan_layer_mir(in_dim, out_dim, knot_count);
    let result_ty = baked.result_ty.clone();
    let fn_name = format!("__kan_{name}_init");
    let mut init_fn = MirFunc::new(fn_name, Vec::new(), vec![result_ty.clone()]);
    init_fn.next_value_id = baked.result_id.0.saturating_add(1);
    init_fn.attributes.push((
        "comptime_baked".to_string(),
        format!("kan_layer[{in_dim}x{out_dim}x{knot_count}]"),
    ));
    if let Some(entry) = init_fn.body.entry_mut() {
        for op in baked.ops {
            entry.push(op);
        }
        entry.push(
            cssl_mir::MirOp::std("func.return")
                .with_operand(baked.result_id)
                .with_attribute("source_loc", "<comptime-kan>"),
        );
    }
    let idx = module.funcs.len();
    module.push_func(init_fn);
    idx
}
