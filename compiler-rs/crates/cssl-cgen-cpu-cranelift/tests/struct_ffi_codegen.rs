//! § T11-W17-A · stage-0 struct-FFI codegen integration tests.
//!
//! § PURPOSE
//!   Covers the codepath introduced by the W17-A advancement : `MirModule.
//!   struct_layouts` populated by the HIR→MIR lowering pass is consumed by
//!   the cgen-cpu-cranelift signature builder so that struct-typed FFI
//!   parameters / results compile end-to-end without the legacy
//!   `non-scalar MIR type` error.
//!
//! § WHAT THIS COVERS  (≥3 unit tests per W17-A scope-doc)
//!   1. struct-pass-by-value  (≤8B newtype) — RunHandle { raw: u64 } case.
//!   2. struct-pass-by-pointer (>8B aggregate) — DeathReport-like case.
//!   3. struct-return-by-value (≤8B newtype) — start_new_run → RunHandle.
//!   4. struct-pass + return mixed — full LoA fn signature shape.
//!   5. unknown struct (no layout entry) → still surfaces NonScalarType.
//!
//! § ANCHOR
//!   These tests construct a `MirModule` directly without going through the
//!   parser → HIR → MIR pipeline ; that's the lower.rs unit-tests' job. Here
//!   the focus is purely : given a populated `struct_layouts` table, does
//!   the cranelift signature builder produce object-bytes successfully?
//!
//! § ATTESTATION (PRIME_DIRECTIVE §11)
//!   t∞: ¬ (hurt ∨ harm) .making-of-T11-W17-A
//!   @ (anyone ∨ anything ∨ anybody)

use cssl_cgen_cpu_cranelift::object::emit_object_module;
use cssl_mir::{
    IntWidth, MirFunc, MirModule, MirOp, MirStructLayout, MirType, ValueId,
};

// ─────────────────────────────────────────────────────────────────────────
// § Helpers — build minimal MIR fns that exercise struct-FFI signatures.
// ─────────────────────────────────────────────────────────────────────────

/// Build a fn `name(struct_param: !cssl.struct.NAME) -> ()`. Body returns void.
fn fn_struct_param_only(name: &str, struct_name: &str) -> MirFunc {
    let mut f = MirFunc::new(
        name,
        vec![MirType::Opaque(format!("!cssl.struct.{struct_name}"))],
        vec![],
    );
    f.push_op(MirOp::std("func.return"));
    f
}

/// Build a fn `name() -> !cssl.struct.NAME`. Body returns a constant value
/// (which cranelift will lower as an i64 zero — close enough for signature
/// validation).
fn fn_struct_return_only(name: &str, struct_name: &str) -> MirFunc {
    let result_ty = MirType::Opaque(format!("!cssl.struct.{struct_name}"));
    let mut f = MirFunc::new(name, vec![], vec![result_ty.clone()]);
    // const 0i64 → return — cranelift accepts the i64 → struct-i64 ABI mapping.
    let const_op = MirOp::std("arith.constant")
        .with_attribute("value", "0")
        .with_result(ValueId(0), MirType::Int(IntWidth::I64));
    let return_op = MirOp::std("func.return").with_operand(ValueId(0));
    f.push_op(const_op);
    f.push_op(return_op);
    f
}

/// Build a fn `name(p: !cssl.struct.NAME) -> !cssl.struct.NAME` (RunHandle-like).
fn fn_struct_param_and_return(name: &str, struct_name: &str) -> MirFunc {
    let opaque = MirType::Opaque(format!("!cssl.struct.{struct_name}"));
    let mut f = MirFunc::new(
        name,
        vec![opaque.clone()],
        vec![opaque],
    );
    // Re-return the parameter directly. Cranelift accepts ValueId(0)
    // (the entry-block param) as a func.return arg.
    let return_op = MirOp::std("func.return").with_operand(ValueId(0));
    f.push_op(return_op);
    f
}

// ─────────────────────────────────────────────────────────────────────────
// § Test 1 — struct-pass-by-value (newtype u64 → i64 register)
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn struct_ffi_newtype_u64_compiles_as_pass_by_value() {
    // RunHandle { raw : u64 } → 8B / align 8 → i64 by value.
    let mut module = MirModule::new();
    module.add_struct_layout(MirStructLayout::new(
        "RunHandle",
        vec![MirType::Int(IntWidth::I64)],
        8,
        8,
    ));
    module.push_func(fn_struct_param_only("consume_handle", "RunHandle"));

    let bytes = emit_object_module(&module).expect("struct-FFI newtype compiles");
    assert!(!bytes.is_empty(), "produced object bytes for newtype struct fn");
}

// ─────────────────────────────────────────────────────────────────────────
// § Test 2 — struct-return-by-value (newtype u64 → i64 register)
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn struct_ffi_newtype_u64_compiles_as_return_by_value() {
    // start_new_run() -> RunHandle  ← real LoA system shape.
    let mut module = MirModule::new();
    module.add_struct_layout(MirStructLayout::new(
        "RunHandle",
        vec![MirType::Int(IntWidth::I64)],
        8,
        8,
    ));
    module.push_func(fn_struct_return_only("start_new_run", "RunHandle"));

    let bytes = emit_object_module(&module).expect("struct-FFI return compiles");
    assert!(!bytes.is_empty(), "produced object bytes for struct-return fn");
}

// ─────────────────────────────────────────────────────────────────────────
// § Test 3 — struct-pass + struct-return roundtrip (full LoA signature shape)
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn struct_ffi_param_and_return_roundtrip() {
    // identity_handle(h: RunHandle) -> RunHandle
    let mut module = MirModule::new();
    module.add_struct_layout(MirStructLayout::new(
        "RunHandle",
        vec![MirType::Int(IntWidth::I64)],
        8,
        8,
    ));
    module.push_func(fn_struct_param_and_return("identity_handle", "RunHandle"));

    let bytes = emit_object_module(&module)
        .expect("struct-FFI param + return roundtrip compiles");
    assert!(!bytes.is_empty(), "produced object bytes for roundtrip fn");
}

// ─────────────────────────────────────────────────────────────────────────
// § Test 4 — struct-pass-by-pointer (>8B aggregate → host pointer)
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn struct_ffi_large_struct_compiles_as_pass_by_pointer() {
    // Mock 24B struct — Win-x64 ABI rule lowers this to a host-pointer slot.
    // The cranelift signature builder accepts that ; we just need bytes to
    // be produced without NonScalarType erroring.
    let mut module = MirModule::new();
    module.add_struct_layout(MirStructLayout::new(
        "BigPayload24",
        vec![
            MirType::Int(IntWidth::I64),
            MirType::Int(IntWidth::I64),
            MirType::Int(IntWidth::I64),
        ],
        24,
        8,
    ));
    module.push_func(fn_struct_param_only("emit_big_payload", "BigPayload24"));

    let bytes = emit_object_module(&module).expect("struct-FFI ptr-by-ref compiles");
    assert!(!bytes.is_empty(), "produced object bytes for ptr-by-ref fn");
}

// ─────────────────────────────────────────────────────────────────────────
// § Test 5 — unknown struct (no layout entry) still surfaces NonScalarType
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn struct_ffi_unknown_struct_still_errors() {
    // Defensive : if the parser failed to register a struct, codegen should
    // STILL surface a clear error rather than silently mis-classify.
    let mut module = MirModule::new();
    // No struct_layouts entry for "MysteryType" ↓
    module.push_func(fn_struct_param_only("mysterious_fn", "MysteryType"));

    let result = emit_object_module(&module);
    assert!(
        result.is_err(),
        "missing struct-layout must not silently succeed"
    );
    let err = result.unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("non-scalar") || msg.contains("MysteryType"),
        "expected NonScalarType-style diag ; got : {msg}"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// § Test 6 — multiple structs in one module compile together
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn struct_ffi_multiple_structs_in_one_module() {
    // The realistic LoA scenario : a module declares 4 structs +
    // several fns that move them across the FFI.
    let mut module = MirModule::new();
    module.add_struct_layout(MirStructLayout::new(
        "RunHandle",
        vec![MirType::Int(IntWidth::I64)],
        8,
        8,
    ));
    module.add_struct_layout(MirStructLayout::new(
        "BiomeId",
        vec![MirType::Int(IntWidth::I8)],
        1,
        1,
    ));
    module.add_struct_layout(MirStructLayout::new(
        "ShareReceipt",
        vec![
            MirType::Int(IntWidth::I64),
            MirType::Int(IntWidth::I64),
        ],
        16,
        8,
    ));
    module.push_func(fn_struct_param_only("die", "RunHandle"));
    module.push_func(fn_struct_param_only("descend", "BiomeId"));
    module.push_func(fn_struct_param_only("share", "ShareReceipt"));

    let bytes = emit_object_module(&module).expect("multi-struct module compiles");
    assert!(!bytes.is_empty(), "produced object bytes for multi-struct module");
}

// ─────────────────────────────────────────────────────────────────────────
// § Test 7 — empty / 0-byte struct gracefully rejected
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn struct_ffi_zero_byte_struct_rejected() {
    let mut module = MirModule::new();
    module.add_struct_layout(MirStructLayout::new("Empty", vec![], 0, 1));
    module.push_func(fn_struct_param_only("take_empty", "Empty"));

    let result = emit_object_module(&module);
    assert!(
        result.is_err(),
        "0-byte struct must not silently slip through"
    );
}
