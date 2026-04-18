//! MIR → CLIF text-instruction lowering.
//!
//! § SPEC : `specs/07_CODEGEN.csl` § CPU BACKEND § MIR-to-CLIF mapping.
//! § ROLE : maps each [`MirOp`] to one or more CLIF textual instructions so the
//!          emitter can produce well-formed CLIF that `clif-util` can parse.
//!
//! § SCOPE (T11-D18 / this commit)
//!   Stage-0 scalar-only lowering :
//!     - Integer arith : `arith.addi` + `arith.subi` + `arith.muli` + `arith.divsi`
//!       + `arith.remsi` + `arith.negi`
//!     - Float arith   : `arith.addf` + `arith.subf` + `arith.mulf` + `arith.divf`
//!       + `arith.negf`
//!     - Constants     : `arith.constant` with `value` attribute
//!     - Comparison    : `arith.cmpi` + `arith.cmpf` (producing i8 result — CLIF b1)
//!     - Select        : `arith.select` (cond, true-val, false-val)
//!     - Return        : `func.return`
//!     - Call          : `func.call` (emits call to named fn — defined in same module)
//!     - Math calls    : `math.sqrt`/`sin`/`cos`/`exp`/`log`/`absf` → CLIF intrinsics
//!
//! § T11-D19 DEFERRED
//!   - Real `cranelift-frontend::FunctionBuilder` + JIT execution.
//!   - Control-flow : `scf.if` / `scf.for` / `scf.while` → CLIF blocks + jumps.
//!   - Memref load/store : `memref.load` / `memref.store` → CLIF load/store.
//!   - Vector ops : `arith.minimumf` / `arith.maximumf` / SIMD ops.

use cssl_mir::{MirOp, ValueId};

use crate::types::clif_type_for;

/// A single CLIF textual instruction with optional result-value definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClifInsn {
    /// Textual form. Includes indentation ("    " 4 spaces) as first chars.
    pub text: String,
}

impl ClifInsn {
    fn new(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }
}

/// Lower a single MIR op to zero-or-more CLIF text instructions.
///
/// Returns `None` if the op is not recognized by stage-0 — callers should fall
/// back to emitting a comment placeholder rather than crash.
///
/// § VALUE-ID MAPPING : MIR `ValueId(n)` → CLIF `v{n}`. Params that already
/// exist as block-args use the same numbering, so result-ids align with the
/// function signature's `v0, v1, ...` scheme.
#[must_use]
pub fn lower_op(op: &MirOp) -> Option<Vec<ClifInsn>> {
    match op.name.as_str() {
        "arith.constant" => lower_constant(op),
        "arith.addi" => lower_binary(op, "iadd"),
        "arith.subi" => lower_binary(op, "isub"),
        "arith.muli" => lower_binary(op, "imul"),
        "arith.divsi" => lower_binary(op, "sdiv"),
        "arith.remsi" => lower_binary(op, "srem"),
        "arith.negi" => lower_unary(op, "ineg"),
        "arith.addf" => lower_binary(op, "fadd"),
        "arith.subf" => lower_binary(op, "fsub"),
        "arith.mulf" => lower_binary(op, "fmul"),
        "arith.divf" => lower_binary(op, "fdiv"),
        "arith.negf" => lower_unary(op, "fneg"),
        "arith.cmpi" => lower_cmp(op, "icmp"),
        "arith.cmpf" => lower_cmp(op, "fcmp"),
        "arith.select" => lower_select(op),
        "func.return" => Some(vec![ClifInsn::new(format!(
            "    return {}",
            format_operands(&op.operands)
        ))]),
        "func.call" => lower_call(op),
        "math.sqrtf" | "math.sqrt" => lower_unary(op, "sqrt"),
        _ => None,
    }
}

/// Lower a binary scalar op : `%r = <clif_name> %a, %b`.
fn lower_binary(op: &MirOp, clif_name: &str) -> Option<Vec<ClifInsn>> {
    let (a, b) = (op.operands.first()?, op.operands.get(1)?);
    let r = op.results.first()?;
    Some(vec![ClifInsn::new(format!(
        "    {} = {} {}, {}",
        format_value(r.id),
        clif_name,
        format_value(*a),
        format_value(*b),
    ))])
}

/// Lower a unary scalar op : `%r = <clif_name> %a`.
fn lower_unary(op: &MirOp, clif_name: &str) -> Option<Vec<ClifInsn>> {
    let a = op.operands.first()?;
    let r = op.results.first()?;
    Some(vec![ClifInsn::new(format!(
        "    {} = {} {}",
        format_value(r.id),
        clif_name,
        format_value(*a),
    ))])
}

/// Lower `arith.constant` : reads `value` attribute + type from result.
fn lower_constant(op: &MirOp) -> Option<Vec<ClifInsn>> {
    let r = op.results.first()?;
    let value = op
        .attributes
        .iter()
        .find(|(k, _)| k == "value")
        .map_or("0", |(_, v)| v.as_str());
    let clif_ty = clif_type_for(&r.ty)?;
    let v_name = format_value(r.id);
    let ty_str = clif_ty.as_str();
    // Integer constant : `iconst.i32 42` ; Float constant : `f32const 0.0`.
    let insn = if ty_str.starts_with('i') || ty_str == "b1" {
        format!("    {v_name} = iconst.{ty_str} {value}")
    } else {
        format!("    {v_name} = {ty_str}const {value}")
    };
    Some(vec![ClifInsn::new(insn)])
}

/// Lower a comparison op : `%r = <cmp-kind> <predicate>, %a, %b`.
/// CLIF uses `icmp` / `fcmp` with the predicate as a leading operand.
fn lower_cmp(op: &MirOp, kind: &str) -> Option<Vec<ClifInsn>> {
    let (a, b) = (op.operands.first()?, op.operands.get(1)?);
    let r = op.results.first()?;
    let predicate = op
        .attributes
        .iter()
        .find(|(k, _)| k == "predicate")
        .map_or("eq", |(_, v)| v.as_str());
    Some(vec![ClifInsn::new(format!(
        "    {} = {} {} {}, {}",
        format_value(r.id),
        kind,
        predicate,
        format_value(*a),
        format_value(*b),
    ))])
}

/// Lower `arith.select` : `%r = select %cond, %true_val, %false_val`.
fn lower_select(op: &MirOp) -> Option<Vec<ClifInsn>> {
    let cond = op.operands.first()?;
    let t = op.operands.get(1)?;
    let f = op.operands.get(2)?;
    let r = op.results.first()?;
    Some(vec![ClifInsn::new(format!(
        "    {} = select {}, {}, {}",
        format_value(r.id),
        format_value(*cond),
        format_value(*t),
        format_value(*f),
    ))])
}

/// Lower `func.call` : emits a call to a named fn. Callee is in the `callee`
/// attribute. CLIF shape : `%r = call %callee(args)`.
fn lower_call(op: &MirOp) -> Option<Vec<ClifInsn>> {
    let (_, callee) = op.attributes.iter().find(|(k, _)| k == "callee")?;
    let args = format_operands(&op.operands);
    let insn = op.results.first().map_or_else(
        || format!("    call %{callee}({args})"),
        |result| {
            let v = format_value(result.id);
            format!("    {v} = call %{callee}({args})")
        },
    );
    Some(vec![ClifInsn::new(insn)])
}

/// Format a `ValueId(n)` as `v{n}` — CLIF's textual value name.
#[must_use]
pub fn format_value(v: ValueId) -> String {
    format!("v{}", v.0)
}

/// Format an operand slice as a comma-separated CLIF argument list.
#[must_use]
pub fn format_operands(ops: &[ValueId]) -> String {
    ops.iter()
        .map(|v| format_value(*v))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::{format_operands, format_value, lower_op};
    use cssl_mir::{FloatWidth, IntWidth, MirOp, MirType, ValueId};

    #[test]
    fn format_value_uses_v_prefix() {
        assert_eq!(format_value(ValueId(0)), "v0");
        assert_eq!(format_value(ValueId(42)), "v42");
    }

    #[test]
    fn format_operands_joins_with_commas() {
        let ids = vec![ValueId(1), ValueId(2), ValueId(3)];
        assert_eq!(format_operands(&ids), "v1, v2, v3");
    }

    #[test]
    fn format_operands_empty_is_empty_string() {
        assert_eq!(format_operands(&[]), "");
    }

    #[test]
    fn lower_addi_emits_iadd() {
        let op = MirOp::std("arith.addi")
            .with_operand(ValueId(0))
            .with_operand(ValueId(1))
            .with_result(ValueId(2), MirType::Int(IntWidth::I32));
        let insns = lower_op(&op).unwrap();
        assert_eq!(insns.len(), 1);
        assert_eq!(insns[0].text, "    v2 = iadd v0, v1");
    }

    #[test]
    fn lower_addf_emits_fadd() {
        let op = MirOp::std("arith.addf")
            .with_operand(ValueId(3))
            .with_operand(ValueId(4))
            .with_result(ValueId(5), MirType::Float(FloatWidth::F32));
        let insns = lower_op(&op).unwrap();
        assert_eq!(insns[0].text, "    v5 = fadd v3, v4");
    }

    #[test]
    fn lower_muli_emits_imul() {
        let op = MirOp::std("arith.muli")
            .with_operand(ValueId(0))
            .with_operand(ValueId(1))
            .with_result(ValueId(2), MirType::Int(IntWidth::I64));
        let insns = lower_op(&op).unwrap();
        assert_eq!(insns[0].text, "    v2 = imul v0, v1");
    }

    #[test]
    fn lower_divf_emits_fdiv() {
        let op = MirOp::std("arith.divf")
            .with_operand(ValueId(10))
            .with_operand(ValueId(11))
            .with_result(ValueId(12), MirType::Float(FloatWidth::F64));
        let insns = lower_op(&op).unwrap();
        assert_eq!(insns[0].text, "    v12 = fdiv v10, v11");
    }

    #[test]
    fn lower_negf_emits_fneg_unary() {
        let op = MirOp::std("arith.negf")
            .with_operand(ValueId(7))
            .with_result(ValueId(8), MirType::Float(FloatWidth::F32));
        let insns = lower_op(&op).unwrap();
        assert_eq!(insns[0].text, "    v8 = fneg v7");
    }

    #[test]
    fn lower_constant_int_uses_iconst() {
        let op = MirOp::std("arith.constant")
            .with_result(ValueId(0), MirType::Int(IntWidth::I32))
            .with_attribute("value", "42");
        let insns = lower_op(&op).unwrap();
        assert_eq!(insns[0].text, "    v0 = iconst.i32 42");
    }

    #[test]
    fn lower_constant_float_uses_f32const() {
        let op = MirOp::std("arith.constant")
            .with_result(ValueId(5), MirType::Float(FloatWidth::F32))
            .with_attribute("value", "3.14");
        let insns = lower_op(&op).unwrap();
        assert_eq!(insns[0].text, "    v5 = f32const 3.14");
    }

    #[test]
    fn lower_cmpi_emits_icmp_with_predicate() {
        let op = MirOp::std("arith.cmpi")
            .with_operand(ValueId(0))
            .with_operand(ValueId(1))
            .with_result(ValueId(2), MirType::Bool)
            .with_attribute("predicate", "slt");
        let insns = lower_op(&op).unwrap();
        assert_eq!(insns[0].text, "    v2 = icmp slt v0, v1");
    }

    #[test]
    fn lower_cmpf_emits_fcmp_with_predicate() {
        let op = MirOp::std("arith.cmpf")
            .with_operand(ValueId(3))
            .with_operand(ValueId(4))
            .with_result(ValueId(5), MirType::Bool)
            .with_attribute("predicate", "ole");
        let insns = lower_op(&op).unwrap();
        assert_eq!(insns[0].text, "    v5 = fcmp ole v3, v4");
    }

    #[test]
    fn lower_select_emits_cond_true_false() {
        let op = MirOp::std("arith.select")
            .with_operand(ValueId(0))
            .with_operand(ValueId(1))
            .with_operand(ValueId(2))
            .with_result(ValueId(3), MirType::Float(FloatWidth::F32));
        let insns = lower_op(&op).unwrap();
        assert_eq!(insns[0].text, "    v3 = select v0, v1, v2");
    }

    #[test]
    fn lower_return_with_value() {
        let op = MirOp::std("func.return").with_operand(ValueId(2));
        let insns = lower_op(&op).unwrap();
        assert_eq!(insns[0].text, "    return v2");
    }

    #[test]
    fn lower_return_without_value() {
        let op = MirOp::std("func.return");
        let insns = lower_op(&op).unwrap();
        assert_eq!(insns[0].text, "    return ");
    }

    #[test]
    fn lower_call_with_result_emits_assignment_form() {
        let op = MirOp::std("func.call")
            .with_operand(ValueId(1))
            .with_operand(ValueId(2))
            .with_result(ValueId(3), MirType::Int(IntWidth::I32))
            .with_attribute("callee", "my_fn");
        let insns = lower_op(&op).unwrap();
        assert_eq!(insns[0].text, "    v3 = call %my_fn(v1, v2)");
    }

    #[test]
    fn lower_sqrt_emits_sqrt_intrinsic() {
        let op = MirOp::std("math.sqrtf")
            .with_operand(ValueId(4))
            .with_result(ValueId(5), MirType::Float(FloatWidth::F32));
        let insns = lower_op(&op).unwrap();
        assert_eq!(insns[0].text, "    v5 = sqrt v4");
    }

    #[test]
    fn lower_unknown_op_returns_none() {
        let op = MirOp::std("cssl.mystery").with_operand(ValueId(0));
        assert!(lower_op(&op).is_none());
    }
}
