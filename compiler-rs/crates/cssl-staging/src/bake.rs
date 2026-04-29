//! T11-D141 — Embed comptime-eval results into MIR as `arith.constant` ops.
//!
//! § ROLE
//!   After [`crate::comptime::ComptimeEvaluator::eval_run_block`] produces a
//!   [`crate::comptime::ComptimeResult`], we need to splice that value into
//!   the MIR module that follows. The naive shape is :
//!
//!   ```text
//!     %k = arith.constant <baked-bytes> : <result-ty>
//!   ```
//!
//!   For scalars the `value` attribute is the canonical decimal form. For
//!   composites (arrays / structs) we emit a sequence of scalar `arith.constant`
//!   ops feeding into a `cssl.array.assemble` or `cssl.struct.assemble` op
//!   carrying the ordered scalar operands. The `assemble` op is a stage-0
//!   placeholder — downstream lowering converts it to a memref.alloca + per-
//!   element memref.store sequence.
//!
//! § BAKE-SHAPES
//!   - Scalar : 1 op (`arith.constant`).
//!   - Array(N elems) : N+1 ops (N constants + 1 `cssl.array.assemble`).
//!   - Struct(M fields) : M+1 ops (M constants + 1 `cssl.struct.assemble`).
//!
//!   The `assemble` ops carry the ordered scalar values as operands and an
//!   `elem_count` / `field_count` attribute. They render in MLIR-text as
//!   `%v = cssl.array.assemble %a, %b, %c : ! cssl.bytes`.

use cssl_mir::{MirOp, MirType, MirValue, ValueId};

use crate::comptime::{ComptimeResult, ComptimeValue};

/// One MIR-op sequence that bakes a [`ComptimeResult`]. The caller installs
/// the ops into the parent block + uses [`Self::result_id`] as the value-id
/// representing the baked constant.
#[derive(Debug, Clone)]
pub struct BakedOps {
    /// Sequence of MIR ops in emission order.
    pub ops: Vec<MirOp>,
    /// The final SSA value-id holding the baked value (top-level result).
    pub result_id: ValueId,
    /// The MIR type of the baked value.
    pub result_ty: MirType,
}

impl BakedOps {
    /// Number of ops emitted.
    #[must_use]
    pub fn op_count(&self) -> usize {
        self.ops.len()
    }
}

/// Bake a [`ComptimeResult`] into a sequence of MIR ops.
///
/// `next_value_id` is the caller-managed monotonic ValueId source ; this fn
/// allocates from it and returns the final id via [`BakedOps::result_id`].
/// The caller is responsible for advancing its own counter past
/// [`BakedOps::result_id`].
pub fn bake_result(result: &ComptimeResult, next_value_id: &mut u32) -> BakedOps {
    bake_value(&result.value, &result.ty, next_value_id)
}

fn bake_value(value: &ComptimeValue, ty: &MirType, next: &mut u32) -> BakedOps {
    match value {
        ComptimeValue::Int(_, _)
        | ComptimeValue::Float(_, _)
        | ComptimeValue::Bool(_)
        | ComptimeValue::Unit => bake_scalar(value, ty, next),
        ComptimeValue::Array(elems) => bake_array(elems, ty, next),
        ComptimeValue::Struct(fields) => bake_struct(fields, ty, next),
    }
}

fn bake_scalar(value: &ComptimeValue, ty: &MirType, next: &mut u32) -> BakedOps {
    let id = ValueId(*next);
    *next = next.saturating_add(1);
    let attr = value.as_constant_attr();
    let op = MirOp::std("arith.constant")
        .with_result(id, ty.clone())
        .with_attribute("value", attr)
        .with_attribute("source_loc", "<comptime>");
    BakedOps {
        ops: vec![op],
        result_id: id,
        result_ty: ty.clone(),
    }
}

fn bake_array(elems: &[ComptimeValue], _container_ty: &MirType, next: &mut u32) -> BakedOps {
    let elem_ty = elems.first().map_or(MirType::None, infer_scalar_mir_type);
    let mut ops = Vec::with_capacity(elems.len() + 1);
    let mut elem_ids = Vec::with_capacity(elems.len());
    for e in elems {
        let mut nested = bake_value(e, &elem_ty, next);
        elem_ids.push(nested.result_id);
        ops.append(&mut nested.ops);
    }
    // Emit the assemble op carrying the elem-ids as operands.
    let assemble_id = ValueId(*next);
    *next = next.saturating_add(1);
    let mut assemble = MirOp::std("cssl.array.assemble")
        .with_result(
            assemble_id,
            MirType::Opaque(format!("!cssl.array<{elem_ty}>")),
        )
        .with_attribute("elem_count", elems.len().to_string())
        .with_attribute("elem_type", format!("{elem_ty}"))
        .with_attribute("source_loc", "<comptime>");
    for id in &elem_ids {
        assemble = assemble.with_operand(*id);
    }
    ops.push(assemble);
    let assemble_ty = MirType::Opaque(format!("!cssl.array<{elem_ty}>"));
    BakedOps {
        ops,
        result_id: assemble_id,
        result_ty: assemble_ty,
    }
}

fn bake_struct(
    fields: &[(String, ComptimeValue)],
    _container_ty: &MirType,
    next: &mut u32,
) -> BakedOps {
    let mut ops = Vec::with_capacity(fields.len() + 1);
    let mut field_ids = Vec::with_capacity(fields.len());
    for (_name, v) in fields {
        let field_ty = infer_scalar_mir_type(v);
        let mut nested = bake_value(v, &field_ty, next);
        field_ids.push((nested.result_id, field_ty.clone()));
        ops.append(&mut nested.ops);
    }
    let assemble_id = ValueId(*next);
    *next = next.saturating_add(1);
    let field_names: Vec<&str> = fields.iter().map(|(n, _)| n.as_str()).collect();
    let mut assemble = MirOp::std("cssl.struct.assemble")
        .with_result(
            assemble_id,
            MirType::Opaque("!cssl.struct.literal".to_string()),
        )
        .with_attribute("field_count", fields.len().to_string())
        .with_attribute("field_names", field_names.join(","))
        .with_attribute("source_loc", "<comptime>");
    for (id, _) in &field_ids {
        assemble = assemble.with_operand(*id);
    }
    ops.push(assemble);
    BakedOps {
        ops,
        result_id: assemble_id,
        result_ty: MirType::Opaque("!cssl.struct.literal".to_string()),
    }
}

fn infer_scalar_mir_type(v: &ComptimeValue) -> MirType {
    match v {
        ComptimeValue::Int(_, w) => MirType::Int(*w),
        ComptimeValue::Float(_, w) => MirType::Float(*w),
        ComptimeValue::Bool(_) => MirType::Bool,
        ComptimeValue::Unit => MirType::None,
        ComptimeValue::Array(_) => MirType::Opaque("!cssl.array".into()),
        ComptimeValue::Struct(_) => MirType::Opaque("!cssl.struct".into()),
    }
}

/// Convenience : bake a sequence of scalar comptime-results into a flat
/// `cssl.array.assemble` op. Used by the LUT-baking demo to bake all 256
/// sine-table entries in one shot.
pub fn bake_lut(values: &[ComptimeResult], next_value_id: &mut u32) -> BakedOps {
    let elem_ty = values.first().map_or(MirType::None, |r| r.ty.clone());
    let mut ops = Vec::with_capacity(values.len() + 1);
    let mut elem_ids = Vec::with_capacity(values.len());
    for v in values {
        let mut nested = bake_value(&v.value, &v.ty, next_value_id);
        elem_ids.push(nested.result_id);
        ops.append(&mut nested.ops);
    }
    let id = ValueId(*next_value_id);
    *next_value_id = next_value_id.saturating_add(1);
    let mut assemble = MirOp::std("cssl.array.assemble")
        .with_result(id, MirType::Opaque(format!("!cssl.lut<{elem_ty}>")))
        .with_attribute("elem_count", values.len().to_string())
        .with_attribute("elem_type", format!("{elem_ty}"))
        .with_attribute("kind", "lut")
        .with_attribute("source_loc", "<comptime-lut>");
    for eid in &elem_ids {
        assemble = assemble.with_operand(*eid);
    }
    ops.push(assemble);
    BakedOps {
        ops,
        result_id: id,
        result_ty: MirType::Opaque(format!("!cssl.lut<{elem_ty}>")),
    }
}

/// Build an `arith.constant` op directly from a [`ComptimeResult`] with no
/// intermediate scalar-decomposition. Returns `None` for composite values —
/// callers should use [`bake_result`] for those.
#[must_use]
pub fn bake_scalar_constant(result: &ComptimeResult, value_id: ValueId) -> Option<MirOp> {
    if !result.value.is_scalar() {
        return None;
    }
    let attr = result.value.as_constant_attr();
    Some(
        MirOp::std("arith.constant")
            .with_result(value_id, result.ty.clone())
            .with_attribute("value", attr)
            .with_attribute("source_loc", "<comptime-scalar>"),
    )
}

/// Inspect a baked op to verify it's a comptime-emitted constant. Used by
/// downstream passes that want to distinguish runtime-emitted constants from
/// comptime-baked ones (the `source_loc` attribute carries the marker
/// `<comptime>` / `<comptime-lut>` / `<comptime-scalar>`).
#[must_use]
pub fn is_comptime_baked(op: &MirOp) -> bool {
    op.attributes
        .iter()
        .find(|(k, _)| k == "source_loc")
        .is_some_and(|(_, v)| v.starts_with("<comptime"))
}

/// Public helper exposing the canonical scalar mir-type for a given comptime
/// value. Useful for tests + downstream specialization passes.
#[must_use]
pub fn scalar_mir_type(v: &ComptimeValue) -> MirType {
    infer_scalar_mir_type(v)
}

// Suppress unused-warning for MirValue in some build configs.
const _: fn() = || {
    let _: MirValue;
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::comptime::{ComptimeResult, ComptimeValue};
    use cssl_mir::{FloatWidth, IntWidth};

    #[test]
    fn bake_int_emits_one_arith_constant() {
        let r = ComptimeResult {
            bytes: 5_i32.to_ne_bytes().to_vec(),
            ty: MirType::Int(IntWidth::I32),
            value: ComptimeValue::Int(5, IntWidth::I32),
        };
        let mut next = 0u32;
        let baked = bake_result(&r, &mut next);
        assert_eq!(baked.op_count(), 1);
        assert_eq!(baked.ops[0].name, "arith.constant");
    }

    #[test]
    fn bake_float_emits_one_arith_constant() {
        let r = ComptimeResult {
            bytes: 1.5_f32.to_ne_bytes().to_vec(),
            ty: MirType::Float(FloatWidth::F32),
            value: ComptimeValue::Float(1.5, FloatWidth::F32),
        };
        let mut next = 0u32;
        let baked = bake_result(&r, &mut next);
        assert_eq!(baked.op_count(), 1);
        let val = baked.ops[0]
            .attributes
            .iter()
            .find(|(k, _)| k == "value")
            .map(|(_, v)| v.as_str());
        assert!(val.is_some());
    }

    #[test]
    fn bake_unit_emits_one_arith_constant() {
        let r = ComptimeResult {
            bytes: Vec::new(),
            ty: MirType::None,
            value: ComptimeValue::Unit,
        };
        let mut next = 0u32;
        let baked = bake_result(&r, &mut next);
        assert_eq!(baked.op_count(), 1);
    }

    #[test]
    fn bake_array_assemble_carries_elem_count_attr() {
        let r = ComptimeResult {
            bytes: Vec::new(),
            ty: MirType::Opaque("array".into()),
            value: ComptimeValue::Array(vec![
                ComptimeValue::Int(1, IntWidth::I32),
                ComptimeValue::Int(2, IntWidth::I32),
                ComptimeValue::Int(3, IntWidth::I32),
                ComptimeValue::Int(4, IntWidth::I32),
            ]),
        };
        let mut next = 0u32;
        let baked = bake_result(&r, &mut next);
        let assemble = baked.ops.last().unwrap();
        assert_eq!(assemble.name, "cssl.array.assemble");
        let count = assemble
            .attributes
            .iter()
            .find(|(k, _)| k == "elem_count")
            .map(|(_, v)| v.as_str());
        assert_eq!(count, Some("4"));
    }

    #[test]
    fn bake_struct_assemble_carries_field_names_attr() {
        let r = ComptimeResult {
            bytes: Vec::new(),
            ty: MirType::Opaque("struct".into()),
            value: ComptimeValue::Struct(vec![
                ("a".into(), ComptimeValue::Int(1, IntWidth::I32)),
                ("b".into(), ComptimeValue::Int(2, IntWidth::I32)),
            ]),
        };
        let mut next = 0u32;
        let baked = bake_result(&r, &mut next);
        let assemble = baked.ops.last().unwrap();
        let names = assemble
            .attributes
            .iter()
            .find(|(k, _)| k == "field_names")
            .map(|(_, v)| v.as_str());
        assert_eq!(names, Some("a,b"));
    }

    #[test]
    fn is_comptime_baked_recognizes_marker() {
        let r = ComptimeResult {
            bytes: Vec::new(),
            ty: MirType::Int(IntWidth::I32),
            value: ComptimeValue::Int(0, IntWidth::I32),
        };
        let mut next = 0u32;
        let baked = bake_result(&r, &mut next);
        assert!(is_comptime_baked(&baked.ops[0]));
    }

    #[test]
    fn is_comptime_baked_returns_false_for_unmarked() {
        let op = MirOp::std("arith.constant").with_attribute("source_loc", "src.csl:1:1");
        assert!(!is_comptime_baked(&op));
    }

    #[test]
    fn scalar_mir_type_handles_all_int_widths() {
        assert_eq!(
            scalar_mir_type(&ComptimeValue::Int(0, IntWidth::I32)),
            MirType::Int(IntWidth::I32)
        );
        assert_eq!(
            scalar_mir_type(&ComptimeValue::Int(0, IntWidth::I64)),
            MirType::Int(IntWidth::I64)
        );
    }

    #[test]
    fn scalar_mir_type_handles_unit() {
        assert_eq!(scalar_mir_type(&ComptimeValue::Unit), MirType::None);
    }

    #[test]
    fn scalar_mir_type_handles_bool() {
        assert_eq!(scalar_mir_type(&ComptimeValue::Bool(true)), MirType::Bool);
    }
}
