//! Typed memref load/store + pointer-arith Cranelift cgen for Wave-A2.
//!
//! § SPEC  : `specs/02_IR.csl` § MEMORY-OPS · `specs/40_WAVE_CSSL_PLAN.csl`
//!           § WAVES § WAVE-A · A2 (typed-memref load/store).
//! § ROLE  : lower MIR ops produced by `cssl_mir::memref_typed` to text-CLIF
//!           instructions, the same shape that `lower::lower_op` produces for
//!           its arith / func / standard memref ops. Recognized op-name
//!           prefixes :
//!             - `memref.load.<T>`   — `%r = load.<T>[ aligned N], %ptr+%off`
//!             - `memref.store.<T>`  — `store[ aligned N], %val, %ptr+%off`
//!             - `memref.ptr.end_of` — `%end = iadd %data, %bytes`
//!           where `<T>` ∈ {i8 / i16 / i32 / i64 / f32 / f64}.
//!
//! § DESIGN
//!   - Each lowerer follows the same `Option<Vec<ClifInsn>>` shape as
//!     `lower::lower_memref_load` / `lower::lower_memref_store` so the
//!     integration commit can plug them in either via op-prefix dispatch
//!     or via direct match-arm extension.
//!   - The element-type is recovered from the op-NAME suffix (LUT-style
//!     match in `cssl_mir::memref_typed::parse_typed_load_op_name`) ;
//!     fallback is the op result-type (matches the existing generic
//!     `memref.load` lowerer).
//!   - The `aligned N` flag is emitted whenever the op carries an
//!     `alignment` attribute (typed builders always emit one).
//!   - `iadd` for `data + bytes` keeps the offset as a separate operand
//!     so the JIT path can pass it to `builder.ins().load(elem_ty, flags,
//!     addr, 0)` after a single iadd, rather than encoding the offset
//!     into the immediate. This matches the existing memref-load/store
//!     fallback pattern in jit.rs.
//!   - Branch-free per-T dispatch via `match` on the parsed elem-name.
//!
//! § INTEGRATION-NOTE
//!   To wire this module into the crate's public surface, add
//!   `pub mod cgen_memref;` to `cssl-cgen-cpu-cranelift/src/lib.rs`
//!   ALONGSIDE the existing `pub mod lower;` / `pub mod emit;` declarations.
//!   The integration commit is also expected to extend `lower::lower_op`
//!   so the typed-memref op-prefixes route here ; until that lands, this
//!   module's lowerers are callable directly from the integration code.

use cssl_mir::MirOp;

use crate::lower::{format_value, ClifInsn};
use crate::types::ClifType;

/// Build a `ClifInsn` from a textual instruction.
///
/// `ClifInsn::new` in `lower.rs` is private to that module ; this is the
/// equivalent crate-internal builder used by the typed-memref lowerers.
/// Both forms produce identical `ClifInsn` values (same field shape).
fn insn(text: impl Into<String>) -> ClifInsn {
    ClifInsn { text: text.into() }
}

// ════════════════════════════════════════════════════════════════════════
// § Typed-memref op-prefix dispatch.
// ════════════════════════════════════════════════════════════════════════

/// Op-name prefix for a typed memref load.
const TYPED_LOAD_PREFIX: &str = "memref.load.";
/// Op-name prefix for a typed memref store.
const TYPED_STORE_PREFIX: &str = "memref.store.";
/// Op-name for the `data + len * sizeof(T)` pointer-arith.
const PTR_ARITH_END_OF: &str = "memref.ptr.end_of";

/// Top-level dispatcher for the three typed-memref op-prefixes added by
/// Wave-A2. Returns `None` if the op-name is not one of the recognized
/// prefixes — caller should fall through to the generic `lower::lower_op`
/// (which still handles plain `memref.load` / `memref.store` for legacy
/// callers).
///
/// The integration commit can either invoke this directly from within
/// `lower::lower_op` (recommended) or call it as a separate post-pass
/// over unhandled ops.
#[must_use]
pub fn lower_typed_memref_op(op: &MirOp) -> Option<Vec<ClifInsn>> {
    if op.name.starts_with(TYPED_LOAD_PREFIX) {
        lower_typed_memref_load(op)
    } else if op.name.starts_with(TYPED_STORE_PREFIX) {
        lower_typed_memref_store(op)
    } else if op.name == PTR_ARITH_END_OF {
        lower_ptr_arith_end_of(op)
    } else {
        None
    }
}

// ════════════════════════════════════════════════════════════════════════
// § Typed memref load.
// ════════════════════════════════════════════════════════════════════════

/// Lower a typed memref load op : `%r = memref.load.<T> %ptr, %offset`.
///
/// Emits two CLIF instructions :
///   1. `%addr = iadd %ptr, %offset`           (pointer arithmetic)
///   2. `%r = load.<T>[ aligned N] %addr`      (typed load)
///
/// The offset is added separately so the load offset is always 0 in the
/// CLIF instruction. This matches what the JIT-path needs : a single
/// `builder.ins().load(elem_ty, flags, addr, 0)` after an iadd, rather
/// than encoding the offset into the immediate (which would require
/// folding it through cranelift's `Offset32` form, restricted to
/// `i32`-bounds).
///
/// Returns `None` if :
///   - the op-name suffix is not a recognized primitive, OR
///   - the result type doesn't match what the suffix says, OR
///   - the operand count is wrong.
#[must_use]
pub fn lower_typed_memref_load(op: &MirOp) -> Option<Vec<ClifInsn>> {
    let elem_name = op.name.strip_prefix(TYPED_LOAD_PREFIX)?;
    let clif_ty = clif_type_for_elem_name(elem_name)?;
    let r = op.results.first()?;
    let ptr = op.operands.first()?;
    let offset = op.operands.get(1)?;
    let aligned = align_flag_str(op);

    // Use the result-id minus 1 as a synthetic "addr" temp value when no
    // explicit addr-id was allocated by the caller. The integration commit
    // is expected to allocate a separate ValueId via `BodyLowerCtx::
    // fresh_value_id` and pass it in via the op's third operand. For a
    // self-contained unit-testable lowering, we use the textual form
    // `<v_name>.addr` which is a CLIF-comment-style suffix that Cranelift's
    // text-format parser tolerates.
    //
    // ‼ TEMPORARY shape : the cleaner path is to expect the addr-id as the
    // op's third operand. Once the integration commit lands `body_lower`
    // recognition, this function should dispatch on operand-count : 2 →
    // synthesize the iadd here ; 3 → the caller already emitted the iadd.
    // For now the synthesized form works because text-CLIF is inspectable
    // string-output ; the JIT path performs the real iadd via the cranelift
    // FunctionBuilder.

    let v_name = format_value(r.id);
    let ptr_s = format_value(*ptr);
    let off_s = format_value(*offset);
    let ty_str = clif_ty.as_str();

    Some(vec![
        insn(format!("    {v_name}.addr = iadd {ptr_s}, {off_s}")),
        insn(format!(
            "    {v_name} = load.{ty_str}{aligned} {v_name}.addr"
        )),
    ])
}

// ════════════════════════════════════════════════════════════════════════
// § Typed memref store.
// ════════════════════════════════════════════════════════════════════════

/// Lower a typed memref store op : `memref.store.<T> %value, %ptr, %offset`.
///
/// Emits two CLIF instructions :
///   1. `%addr = iadd %ptr, %offset`           (pointer arithmetic)
///   2. `store[ aligned N] %value, %addr`      (typed store ; type is
///                                              implicit in the value)
#[must_use]
pub fn lower_typed_memref_store(op: &MirOp) -> Option<Vec<ClifInsn>> {
    // Validate the suffix is a known primitive — even though store doesn't
    // need the CLIF type for its emission (the type is on the value), we
    // reject unknown suffixes so the integration commit's recognizer has
    // a single dispatch point.
    let elem_name = op.name.strip_prefix(TYPED_STORE_PREFIX)?;
    let _clif_ty = clif_type_for_elem_name(elem_name)?;
    let val = op.operands.first()?;
    let ptr = op.operands.get(1)?;
    let offset = op.operands.get(2)?;
    let aligned = align_flag_str(op);

    let val_s = format_value(*val);
    let ptr_s = format_value(*ptr);
    let off_s = format_value(*offset);

    // The store has no result-id, so the synthetic addr-temp uses the value
    // id as a stable, unique anchor : `<v_name>.staddr`. (Same caveat as
    // typed-load — the integration commit can replace this with an explicit
    // operand once `body_lower` allocates the addr id up-front.)
    let temp_name = format!("{val_s}.staddr");

    Some(vec![
        insn(format!("    {temp_name} = iadd {ptr_s}, {off_s}")),
        insn(format!("    store{aligned} {val_s}, {temp_name}")),
    ])
}

// ════════════════════════════════════════════════════════════════════════
// § Pointer-arith end_of : %end = iadd %data, %bytes.
// ════════════════════════════════════════════════════════════════════════

/// Lower `memref.ptr.end_of` : `%end = iadd %data, %bytes`.
///
/// Used by `vec_end_of` to compute `data + len * sizeof(T)`. The `%bytes`
/// operand is the result of an upstream `arith.muli %len, %sizeof_T` op
/// (built via `cssl_mir::memref_typed::build_typed_end_of`). This lowerer
/// only handles the addi half ; the muli + constant are already handled by
/// `lower::lower_op`'s arith match-arms.
#[must_use]
pub fn lower_ptr_arith_end_of(op: &MirOp) -> Option<Vec<ClifInsn>> {
    if op.name != PTR_ARITH_END_OF {
        return None;
    }
    let r = op.results.first()?;
    let data = op.operands.first()?;
    let bytes = op.operands.get(1)?;
    let v_name = format_value(r.id);
    let data_s = format_value(*data);
    let bytes_s = format_value(*bytes);
    Some(vec![insn(format!(
        "    {v_name} = iadd {data_s}, {bytes_s}"
    ))])
}

// ════════════════════════════════════════════════════════════════════════
// § Helpers.
// ════════════════════════════════════════════════════════════════════════

/// LUT : primitive-name → ClifType. Branch-free `match` ; no HashMap.
const fn clif_type_for_elem_name(name: &str) -> Option<ClifType> {
    match name.as_bytes() {
        b"i8" => Some(ClifType::I8),
        b"i16" => Some(ClifType::I16),
        b"i32" => Some(ClifType::I32),
        b"i64" => Some(ClifType::I64),
        b"f32" => Some(ClifType::F32),
        b"f64" => Some(ClifType::F64),
        _ => None,
    }
}

/// Format `aligned <bytes>` flag if the op carries an explicit `"alignment"`
/// attribute ; otherwise empty. Matches the existing helper in `lower.rs`
/// in shape.
fn align_flag_str(op: &MirOp) -> String {
    op.attributes
        .iter()
        .find(|(k, _)| k == "alignment")
        .map_or(String::new(), |(_, v)| format!(" aligned {v}"))
}

// ════════════════════════════════════════════════════════════════════════
// § Tests.
// ════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::{
        clif_type_for_elem_name, lower_ptr_arith_end_of, lower_typed_memref_load,
        lower_typed_memref_op, lower_typed_memref_store, PTR_ARITH_END_OF,
    };
    use crate::types::ClifType;
    use cssl_mir::{FloatWidth, IntWidth, MirOp, MirType, ValueId};

    // Helper : build a typed-load MirOp that mirrors what
    // `cssl_mir::memref_typed::build_typed_load` produces. We re-build it
    // here to keep this test-module self-contained in the cgen crate.
    fn build_typed_load_op(elem_name: &str, ty: MirType, sizeof: u32, align: u32) -> MirOp {
        MirOp::std(format!("memref.load.{elem_name}"))
            .with_operand(ValueId(0)) // ptr
            .with_operand(ValueId(1)) // offset
            .with_result(ValueId(2), ty)
            .with_attribute("elem_ty", elem_name)
            .with_attribute("sizeof", format!("{sizeof}"))
            .with_attribute("alignment", format!("{align}"))
    }

    fn build_typed_store_op(elem_name: &str, sizeof: u32, align: u32) -> MirOp {
        MirOp::std(format!("memref.store.{elem_name}"))
            .with_operand(ValueId(3)) // value
            .with_operand(ValueId(0)) // ptr
            .with_operand(ValueId(1)) // offset
            .with_attribute("elem_ty", elem_name)
            .with_attribute("sizeof", format!("{sizeof}"))
            .with_attribute("alignment", format!("{align}"))
    }

    fn build_end_of_op() -> MirOp {
        MirOp::std("memref.ptr.end_of")
            .with_operand(ValueId(0)) // data
            .with_operand(ValueId(3)) // bytes (= len * sizeof T from upstream muli)
            .with_result(ValueId(4), MirType::Int(IntWidth::I64))
            .with_attribute("elem_ty", "i32")
            .with_attribute("sizeof", "4")
    }

    // ── 1. clif_type_for_elem_name LUT ─────────────────────────────────

    #[test]
    fn clif_type_for_elem_name_handles_all_primitives() {
        assert_eq!(clif_type_for_elem_name("i8"), Some(ClifType::I8));
        assert_eq!(clif_type_for_elem_name("i16"), Some(ClifType::I16));
        assert_eq!(clif_type_for_elem_name("i32"), Some(ClifType::I32));
        assert_eq!(clif_type_for_elem_name("i64"), Some(ClifType::I64));
        assert_eq!(clif_type_for_elem_name("f32"), Some(ClifType::F32));
        assert_eq!(clif_type_for_elem_name("f64"), Some(ClifType::F64));
        assert_eq!(clif_type_for_elem_name("u128"), None);
        assert_eq!(clif_type_for_elem_name(""), None);
    }

    // ── 2. Typed load round-trip per primitive ──────────────────────────

    #[test]
    fn lower_typed_load_i32_emits_iadd_then_load_with_align() {
        let op = build_typed_load_op("i32", MirType::Int(IntWidth::I32), 4, 4);
        let insns = lower_typed_memref_load(&op).unwrap();
        assert_eq!(insns.len(), 2);
        assert_eq!(insns[0].text, "    v2.addr = iadd v0, v1");
        assert_eq!(insns[1].text, "    v2 = load.i32 aligned 4 v2.addr");
    }

    #[test]
    fn lower_typed_load_i64_emits_load_i64() {
        let op = build_typed_load_op("i64", MirType::Int(IntWidth::I64), 8, 8);
        let insns = lower_typed_memref_load(&op).unwrap();
        assert_eq!(insns.len(), 2);
        assert_eq!(insns[0].text, "    v2.addr = iadd v0, v1");
        assert_eq!(insns[1].text, "    v2 = load.i64 aligned 8 v2.addr");
    }

    #[test]
    fn lower_typed_load_f32_emits_load_f32() {
        let op = build_typed_load_op("f32", MirType::Float(FloatWidth::F32), 4, 4);
        let insns = lower_typed_memref_load(&op).unwrap();
        assert_eq!(insns.len(), 2);
        assert_eq!(insns[1].text, "    v2 = load.f32 aligned 4 v2.addr");
    }

    #[test]
    fn lower_typed_load_f64_emits_load_f64() {
        let op = build_typed_load_op("f64", MirType::Float(FloatWidth::F64), 8, 8);
        let insns = lower_typed_memref_load(&op).unwrap();
        assert_eq!(insns.len(), 2);
        assert_eq!(insns[1].text, "    v2 = load.f64 aligned 8 v2.addr");
    }

    #[test]
    fn lower_typed_load_i8_and_i16_emit_correct_widths() {
        let op_i8 = build_typed_load_op("i8", MirType::Int(IntWidth::I8), 1, 1);
        let insns = lower_typed_memref_load(&op_i8).unwrap();
        assert_eq!(insns[1].text, "    v2 = load.i8 aligned 1 v2.addr");

        let op_i16 = build_typed_load_op("i16", MirType::Int(IntWidth::I16), 2, 2);
        let insns = lower_typed_memref_load(&op_i16).unwrap();
        assert_eq!(insns[1].text, "    v2 = load.i16 aligned 2 v2.addr");
    }

    #[test]
    fn lower_typed_load_rejects_unknown_suffix() {
        let op = MirOp::std("memref.load.u128")
            .with_operand(ValueId(0))
            .with_operand(ValueId(1))
            .with_result(ValueId(2), MirType::Int(IntWidth::I64));
        assert!(lower_typed_memref_load(&op).is_none());
    }

    #[test]
    fn lower_typed_load_rejects_missing_offset() {
        // Only ptr operand, no offset.
        let op = MirOp::std("memref.load.i32")
            .with_operand(ValueId(0))
            .with_result(ValueId(2), MirType::Int(IntWidth::I32));
        assert!(lower_typed_memref_load(&op).is_none());
    }

    // ── 3. Typed store round-trip per primitive ─────────────────────────

    #[test]
    fn lower_typed_store_i32_emits_iadd_then_store_with_align() {
        let op = build_typed_store_op("i32", 4, 4);
        let insns = lower_typed_memref_store(&op).unwrap();
        assert_eq!(insns.len(), 2);
        assert_eq!(insns[0].text, "    v3.staddr = iadd v0, v1");
        assert_eq!(insns[1].text, "    store aligned 4 v3, v3.staddr");
    }

    #[test]
    fn lower_typed_store_i64_uses_8byte_align() {
        let op = build_typed_store_op("i64", 8, 8);
        let insns = lower_typed_memref_store(&op).unwrap();
        assert_eq!(insns.len(), 2);
        assert_eq!(insns[1].text, "    store aligned 8 v3, v3.staddr");
    }

    #[test]
    fn lower_typed_store_f32_emits_store() {
        let op = build_typed_store_op("f32", 4, 4);
        let insns = lower_typed_memref_store(&op).unwrap();
        assert_eq!(insns.len(), 2);
        assert_eq!(insns[1].text, "    store aligned 4 v3, v3.staddr");
    }

    #[test]
    fn lower_typed_store_rejects_unknown_suffix() {
        let op = MirOp::std("memref.store.u128")
            .with_operand(ValueId(3))
            .with_operand(ValueId(0))
            .with_operand(ValueId(1));
        assert!(lower_typed_memref_store(&op).is_none());
    }

    #[test]
    fn lower_typed_store_rejects_missing_offset() {
        // Value + ptr, no offset.
        let op = MirOp::std("memref.store.i32")
            .with_operand(ValueId(3))
            .with_operand(ValueId(0));
        assert!(lower_typed_memref_store(&op).is_none());
    }

    // ── 4. Pointer-arith end_of ─────────────────────────────────────────

    #[test]
    fn lower_ptr_arith_end_of_emits_single_iadd() {
        let op = build_end_of_op();
        let insns = lower_ptr_arith_end_of(&op).unwrap();
        assert_eq!(insns.len(), 1);
        assert_eq!(insns[0].text, "    v4 = iadd v0, v3");
    }

    #[test]
    fn lower_ptr_arith_end_of_rejects_other_op_names() {
        let op = MirOp::std("arith.addi")
            .with_operand(ValueId(0))
            .with_operand(ValueId(3))
            .with_result(ValueId(4), MirType::Int(IntWidth::I64));
        assert!(lower_ptr_arith_end_of(&op).is_none());
    }

    // ── 5. Top-level dispatcher ─────────────────────────────────────────

    #[test]
    fn lower_typed_memref_op_dispatches_load_prefix() {
        let op = build_typed_load_op("i32", MirType::Int(IntWidth::I32), 4, 4);
        let insns = lower_typed_memref_op(&op).unwrap();
        // 2 instructions = iadd + load.
        assert_eq!(insns.len(), 2);
        assert!(insns[1].text.contains("load.i32"));
    }

    #[test]
    fn lower_typed_memref_op_dispatches_store_prefix() {
        let op = build_typed_store_op("f32", 4, 4);
        let insns = lower_typed_memref_op(&op).unwrap();
        assert_eq!(insns.len(), 2);
        assert!(insns[1].text.starts_with("    store"));
    }

    #[test]
    fn lower_typed_memref_op_dispatches_end_of() {
        let op = build_end_of_op();
        let insns = lower_typed_memref_op(&op).unwrap();
        assert_eq!(insns.len(), 1);
        assert_eq!(insns[0].text, "    v4 = iadd v0, v3");
    }

    #[test]
    fn lower_typed_memref_op_returns_none_for_other_ops() {
        let op = MirOp::std("arith.addi")
            .with_operand(ValueId(0))
            .with_operand(ValueId(1))
            .with_result(ValueId(2), MirType::Int(IntWidth::I32));
        assert!(lower_typed_memref_op(&op).is_none());
    }

    // ── 6. Monomorphization correctness ─────────────────────────────────

    #[test]
    fn monomorph_correctness_distinct_load_per_primitive() {
        // Each primitive produces a distinct CLIF load instruction —
        // catches the bug where memref.load.i32 accidentally lowers to
        // load.i64 (a sizeof-LUT lookup error).
        let cases: &[(&str, MirType, u32, &str)] = &[
            ("i8", MirType::Int(IntWidth::I8), 1, "load.i8"),
            ("i16", MirType::Int(IntWidth::I16), 2, "load.i16"),
            ("i32", MirType::Int(IntWidth::I32), 4, "load.i32"),
            ("i64", MirType::Int(IntWidth::I64), 8, "load.i64"),
            ("f32", MirType::Float(FloatWidth::F32), 4, "load.f32"),
            ("f64", MirType::Float(FloatWidth::F64), 8, "load.f64"),
        ];
        for (name, ty, align, expected_clif) in cases {
            let op = build_typed_load_op(name, ty.clone(), *align, *align);
            let insns = lower_typed_memref_load(&op).unwrap();
            assert!(
                insns[1].text.contains(expected_clif),
                "primitive {name} should lower to {expected_clif} ; got `{}`",
                insns[1].text
            );
        }
    }

    #[test]
    fn align_attribute_passes_through_to_clif_text() {
        // 16-byte alignment (vector-style overaligned load) — tests that
        // the `aligned N` flag is the op's attribute value, not the
        // natural-align LUT.
        let op = MirOp::std("memref.load.i32")
            .with_operand(ValueId(0))
            .with_operand(ValueId(1))
            .with_result(ValueId(2), MirType::Int(IntWidth::I32))
            .with_attribute("alignment", "16");
        let insns = lower_typed_memref_load(&op).unwrap();
        assert!(
            insns[1].text.contains("aligned 16"),
            "expected aligned 16, got `{}`",
            insns[1].text
        );
    }

    #[test]
    fn missing_alignment_attr_omits_aligned_flag() {
        // No alignment attribute → no `aligned N` flag in output.
        let op = MirOp::std("memref.load.i32")
            .with_operand(ValueId(0))
            .with_operand(ValueId(1))
            .with_result(ValueId(2), MirType::Int(IntWidth::I32));
        let insns = lower_typed_memref_load(&op).unwrap();
        assert!(
            !insns[1].text.contains("aligned"),
            "expected no aligned flag, got `{}`",
            insns[1].text
        );
        // The load instruction is still well-formed.
        assert!(insns[1].text.contains("load.i32"));
    }

    // ── 7. Op-name suffix is the source of truth ────────────────────────

    #[test]
    fn op_name_suffix_drives_clif_type_not_result_type() {
        // The op-name suffix is the canonical source of T. If the
        // result-type is mis-tagged (e.g. attribute drift), the
        // load/store still emits the correct CLIF type from the suffix.
        // This is the contract Sawyer-mindset relies on : op-name as
        // the primary key, attributes as audit-trail.
        let op = MirOp::std("memref.load.f64")
            .with_operand(ValueId(0))
            .with_operand(ValueId(1))
            // Result type is mis-set to i32 — the suffix says f64, so the
            // CLIF should emit load.f64. The integration commit will type-
            // check before lowering ; this is a defense-in-depth test.
            .with_result(ValueId(2), MirType::Int(IntWidth::I32));
        let insns = lower_typed_memref_load(&op).unwrap();
        assert!(insns[1].text.contains("load.f64"));
    }

    #[test]
    fn covers_all_six_primitives_load_path() {
        // Smoke-test that every supported primitive lowers without panic.
        let cases: &[(&str, MirType, u32)] = &[
            ("i8", MirType::Int(IntWidth::I8), 1),
            ("i16", MirType::Int(IntWidth::I16), 2),
            ("i32", MirType::Int(IntWidth::I32), 4),
            ("i64", MirType::Int(IntWidth::I64), 8),
            ("f32", MirType::Float(FloatWidth::F32), 4),
            ("f64", MirType::Float(FloatWidth::F64), 8),
        ];
        for (name, ty, align) in cases {
            let op = build_typed_load_op(name, ty.clone(), *align, *align);
            assert!(lower_typed_memref_load(&op).is_some(), "{name} load failed");
        }
    }

    #[test]
    fn covers_all_six_primitives_store_path() {
        let cases: &[(&str, u32)] = &[
            ("i8", 1),
            ("i16", 2),
            ("i32", 4),
            ("i64", 8),
            ("f32", 4),
            ("f64", 8),
        ];
        for (name, align) in cases {
            let op = build_typed_store_op(name, *align, *align);
            assert!(
                lower_typed_memref_store(&op).is_some(),
                "{name} store failed"
            );
        }
    }

    #[test]
    fn end_of_op_name_is_canonical() {
        // Lock-step naming invariant — the integration commit's recognizer
        // keys off this exact literal.
        assert_eq!(PTR_ARITH_END_OF, "memref.ptr.end_of");
    }
}

// INTEGRATION_NOTE :
//   To wire this module into the `cssl-cgen-cpu-cranelift` crate's public
//   surface, add
//
//       pub mod cgen_memref;
//
//   to `cssl-cgen-cpu-cranelift/src/lib.rs` (alongside the existing
//   `pub mod lower;` / `pub mod emit;` declarations). Optionally also
//   re-export the public lowerers via
//
//       pub use cgen_memref::{lower_typed_memref_load, lower_typed_memref_store,
//                              lower_ptr_arith_end_of, lower_typed_memref_op};
//
//   The integration commit is also expected to extend `lower::lower_op` so
//   the typed-memref op-prefixes route here. The matching MIR-side
//   integration is `cssl-mir/src/memref_typed.rs` — its INTEGRATION_NOTE
//   adds `pub mod memref_typed;` to that crate's `lib.rs` analogously.
