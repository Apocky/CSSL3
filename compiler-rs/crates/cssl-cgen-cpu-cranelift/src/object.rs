//! § object — cranelift-object backend (T11-D54, S6-A3).
//!
//! Stage-0 minimum viable : translate a `MirModule` containing simple
//! scalar-only functions into a relocatable object file (.obj on Windows-MSVC,
//! .o on Linux/ELF, .o on macOS/Mach-O) suitable for linking with `cssl-rt`
//! (S6-A4).
//!
//! § STRATEGY
//!   - Build a `cranelift_object::ObjectModule` for the host target by default.
//!   - For each `MirFunc` in the module, declare an exported function with a
//!     cranelift signature derived from the MIR types, then define its body
//!     using a small per-op lowering table that supports the subset needed
//!     for `fn main() -> i32 { N }` and basic scalar arithmetic.
//!   - Call `module.finish()` to produce an `ObjectProduct`, then `.emit()`
//!     to materialize the object file's bytes.
//!
//! § SUBSET HANDLED  (S6-A3, expanded by S6-B+)
//!   - Const : `arith.constant` (i32 / i64 / f32 / f64).
//!   - Return : `func.return` with operand list.
//!   - Arith  : `arith.addi` / `subi` / `muli` / `addf` / `subf` / `mulf` / `divf`.
//!
//! § DEFERRED to later phases
//!   - Per-call FuncRef declarations (deferred — handled by S6-A3 follow-up
//!     once the JIT lowering helpers are extracted into a shared module).
//!   - Multi-block bodies + control-flow (S6-C1/C2).
//!   - GPU body emission for non-CPU `MirFunc`s (S6-D phases).
//!   - DWARF-5 / CodeView debug-info (deferred ; spec calls it out).
//!   - Cross-platform target-triple resolution beyond the host.
//!
//! § INVARIANTS
//!   - Returned bytes always have a valid first-byte magic for the chosen
//!     object format (`0x7F E L F` for ELF ; `0x4C 0x01` or `0x64 0x86`-style
//!     leading bytes for COFF ; `0xFE ED FA CE`-style for Mach-O).
//!   - Every `MirFunc` named in the input maps to a defined symbol in the
//!     output. Name mangling is identity (CSSLv3 names already use
//!     `[a-zA-Z0-9_]` after monomorphization).

use std::collections::HashMap;

use cranelift_codegen::ir::{types as cl_types, AbiParam, InstBuilder, Signature, UserFuncName};
use cranelift_codegen::settings::Configurable as _;
use cranelift_codegen::{settings, Context};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{Linkage, Module};
use cranelift_object::{ObjectBuilder, ObjectModule};
use cssl_mir::{FloatWidth, IntWidth, MirFunc, MirModule, MirOp, MirType, ValueId};
use thiserror::Error;

// ───────────────────────────────────────────────────────────────────────
// § ObjectFormat helpers (reuses `abi::ObjectFormat`)
// ───────────────────────────────────────────────────────────────────────

/// Default object-file format for the host platform.
#[must_use]
pub const fn host_default_format() -> crate::abi::ObjectFormat {
    if cfg!(target_os = "windows") {
        crate::abi::ObjectFormat::Coff
    } else if cfg!(target_os = "macos") {
        crate::abi::ObjectFormat::MachO
    } else {
        crate::abi::ObjectFormat::Elf
    }
}

/// Magic-byte signature the produced object file SHOULD start with for the
/// given object format. ELF uses `\x7FELF`, COFF uses the AMD64 machine
/// header `0x64 0x86`, Mach-O uses the 64-bit little-endian magic
/// `0xFE 0xED 0xFA 0xCF` (read in file order : `0xCF 0xFA 0xED 0xFE`).
#[must_use]
pub const fn magic_prefix(fmt: crate::abi::ObjectFormat) -> &'static [u8] {
    match fmt {
        crate::abi::ObjectFormat::Elf => b"\x7FELF",
        crate::abi::ObjectFormat::Coff => &[0x64, 0x86],
        crate::abi::ObjectFormat::MachO => &[0xCF, 0xFA, 0xED, 0xFE],
    }
}

// ───────────────────────────────────────────────────────────────────────
// § ObjectError — emission failure modes
// ───────────────────────────────────────────────────────────────────────

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ObjectError {
    /// Cranelift refused to build a host ISA (very unusual ; reproducer in
    /// tests would require a misconfigured toolchain).
    #[error("cranelift native ISA unavailable : {0}")]
    NoIsa(String),

    /// A `MirFunc` parameter or result has a non-scalar type that the
    /// stage-0 lowering doesn't handle.
    #[error(
        "fn `{fn_name}` param/result #{slot} has non-scalar MIR type `{ty}` ; stage-0 scalars-only"
    )]
    NonScalarType {
        fn_name: String,
        slot: usize,
        ty: String,
    },

    /// Cranelift reported a codegen / declaration error.
    #[error("fn `{fn_name}` cranelift error : {detail}")]
    LoweringFailed { fn_name: String, detail: String },

    /// A MIR op-name is not in the stage-0 object-emit subset.
    #[error("fn `{fn_name}` uses MIR op `{op_name}` ; not in stage-0 object-emit subset")]
    UnsupportedOp { fn_name: String, op_name: String },

    /// Multi-block bodies require structured-CFG support which lands later.
    #[error(
        "fn `{fn_name}` has multi-block body ; stage-0 object-emit supports only the entry block"
    )]
    MultiBlockBody { fn_name: String },

    /// A `MirFunc` referenced an unknown ValueId.
    #[error("fn `{fn_name}` references unknown ValueId({value_id})")]
    UnknownValueId { fn_name: String, value_id: u32 },
}

// ───────────────────────────────────────────────────────────────────────
// § public API
// ───────────────────────────────────────────────────────────────────────

/// Translate a `MirModule` to host-target object-file bytes.
///
/// # Errors
/// Returns [`ObjectError`] on cranelift / MIR / signature problems.
pub fn emit_object_module(module: &MirModule) -> Result<Vec<u8>, ObjectError> {
    emit_object_module_with_format(module, host_default_format())
}

/// Translate `MirModule` → object bytes, requesting the given format.
///
/// At stage-0 the format parameter is informational ; the produced bytes are
/// always for the host platform's native format (cranelift_native picks the
/// host ISA + format). Cross-compilation will be added when the target-triple
/// resolution lands.
///
/// # Errors
/// Returns [`ObjectError`] on cranelift / MIR / signature problems.
#[allow(clippy::too_many_lines)]
pub fn emit_object_module_with_format(
    module: &MirModule,
    _format: crate::abi::ObjectFormat,
) -> Result<Vec<u8>, ObjectError> {
    // § 1. Build host ISA via cranelift_native.
    let mut flag_builder = settings::builder();
    flag_builder
        .set("use_colocated_libcalls", "false")
        .map_err(|e| ObjectError::NoIsa(format!("flag set : {e}")))?;
    flag_builder
        .set("is_pic", "false")
        .map_err(|e| ObjectError::NoIsa(format!("flag set : {e}")))?;
    let isa_builder =
        cranelift_native::builder().map_err(|msg| ObjectError::NoIsa(msg.to_string()))?;
    let isa = isa_builder
        .finish(settings::Flags::new(flag_builder))
        .map_err(|e| ObjectError::NoIsa(format!("isa.finish : {e}")))?;

    // § 2. Build the ObjectModule.
    let obj_builder = ObjectBuilder::new(
        isa,
        b"cssl_object".to_vec(),
        cranelift_module::default_libcall_names(),
    )
    .map_err(|e| ObjectError::LoweringFailed {
        fn_name: "<module>".to_string(),
        detail: format!("ObjectBuilder::new : {e}"),
    })?;
    let mut obj_module = ObjectModule::new(obj_builder);

    // § 3. Declare + define each MirFunc.
    let mut builder_ctx = FunctionBuilderContext::new();
    let mut codegen_ctx = Context::new();

    for mir_fn in &module.funcs {
        if mir_fn.is_generic {
            continue; // skip unspecialized generic fns
        }
        compile_one_fn(&mut obj_module, &mut builder_ctx, &mut codegen_ctx, mir_fn)?;
    }

    // § 4. Finish + emit.
    let product = obj_module.finish();
    product.emit().map_err(|e| ObjectError::LoweringFailed {
        fn_name: "<module>".to_string(),
        detail: format!("ObjectProduct.emit : {e}"),
    })
}

// ───────────────────────────────────────────────────────────────────────
// § per-fn compilation
// ───────────────────────────────────────────────────────────────────────

fn compile_one_fn(
    obj_module: &mut ObjectModule,
    builder_ctx: &mut FunctionBuilderContext,
    codegen_ctx: &mut Context,
    mir_fn: &MirFunc,
) -> Result<(), ObjectError> {
    if mir_fn.body.blocks.len() > 1 {
        return Err(ObjectError::MultiBlockBody {
            fn_name: mir_fn.name.clone(),
        });
    }

    // § 1. Build cranelift signature.
    let call_conv = obj_module.isa().default_call_conv();
    let mut sig = Signature::new(call_conv);
    for (idx, p_ty) in mir_fn.params.iter().enumerate() {
        let cl_ty = mir_type_to_cl(p_ty).ok_or_else(|| ObjectError::NonScalarType {
            fn_name: mir_fn.name.clone(),
            slot: idx,
            ty: format!("{p_ty}"),
        })?;
        sig.params.push(AbiParam::new(cl_ty));
    }
    for (idx, r_ty) in mir_fn.results.iter().enumerate() {
        let cl_ty = mir_type_to_cl(r_ty).ok_or_else(|| ObjectError::NonScalarType {
            fn_name: mir_fn.name.clone(),
            slot: idx,
            ty: format!("{r_ty}"),
        })?;
        sig.returns.push(AbiParam::new(cl_ty));
    }

    let func_id = obj_module
        .declare_function(&mir_fn.name, Linkage::Export, &sig)
        .map_err(|e| ObjectError::LoweringFailed {
            fn_name: mir_fn.name.clone(),
            detail: format!("declare_function : {e}"),
        })?;

    codegen_ctx.clear();
    codegen_ctx.func.signature = sig.clone();
    codegen_ctx.func.name = UserFuncName::user(0, func_id.as_u32());

    // § 2. Build body.
    {
        let mut builder = FunctionBuilder::new(&mut codegen_ctx.func, builder_ctx);
        let entry = builder.create_block();
        builder.append_block_params_for_function_params(entry);
        builder.switch_to_block(entry);
        builder.seal_block(entry);

        let mut value_map: HashMap<ValueId, cranelift_codegen::ir::Value> = HashMap::new();
        let block_params: Vec<_> = builder.block_params(entry).to_vec();
        if let Some(entry_mir_block) = mir_fn.body.blocks.first() {
            for (arg_meta, &bp) in entry_mir_block.args.iter().zip(block_params.iter()) {
                value_map.insert(arg_meta.id, bp);
            }

            let mut returned = false;
            for op in &entry_mir_block.ops {
                if returned {
                    break;
                }
                returned = lower_one_op(op, &mut builder, &mut value_map, &mir_fn.name)?;
            }
            if !returned {
                // No explicit return ; emit an implicit return.
                if mir_fn.results.is_empty() {
                    builder.ins().return_(&[]);
                } else {
                    return Err(ObjectError::LoweringFailed {
                        fn_name: mir_fn.name.clone(),
                        detail: "fn body is missing a `func.return` terminator".to_string(),
                    });
                }
            }
        } else {
            // Empty body → return-void (only valid if results empty).
            if mir_fn.results.is_empty() {
                builder.ins().return_(&[]);
            } else {
                return Err(ObjectError::LoweringFailed {
                    fn_name: mir_fn.name.clone(),
                    detail: "empty body but non-empty results".to_string(),
                });
            }
        }

        builder.finalize();
    }

    // § 3. Define the function in the object module.
    obj_module
        .define_function(func_id, codegen_ctx)
        .map_err(|e| ObjectError::LoweringFailed {
            fn_name: mir_fn.name.clone(),
            detail: format!("define_function : {e}"),
        })?;

    Ok(())
}

// ───────────────────────────────────────────────────────────────────────
// § per-op lowering (subset)
// ───────────────────────────────────────────────────────────────────────

fn lower_one_op(
    op: &MirOp,
    builder: &mut FunctionBuilder<'_>,
    value_map: &mut HashMap<ValueId, cranelift_codegen::ir::Value>,
    fn_name: &str,
) -> Result<bool, ObjectError> {
    match op.name.as_str() {
        "arith.constant" => {
            let r = op
                .results
                .first()
                .ok_or_else(|| ObjectError::LoweringFailed {
                    fn_name: fn_name.to_string(),
                    detail: "arith.constant with no result".to_string(),
                })?;
            let value_str = op
                .attributes
                .iter()
                .find(|(k, _)| k == "value")
                .map_or("0", |(_, v)| v.as_str());
            let cl_ty = mir_type_to_cl(&r.ty).ok_or_else(|| ObjectError::NonScalarType {
                fn_name: fn_name.to_string(),
                slot: 0,
                ty: format!("{}", r.ty),
            })?;
            let v = if cl_ty == cl_types::F32 {
                builder
                    .ins()
                    .f32const(value_str.parse::<f32>().unwrap_or(0.0))
            } else if cl_ty == cl_types::F64 {
                builder
                    .ins()
                    .f64const(value_str.parse::<f64>().unwrap_or(0.0))
            } else {
                builder
                    .ins()
                    .iconst(cl_ty, value_str.parse::<i64>().unwrap_or(0))
            };
            value_map.insert(r.id, v);
            Ok(false)
        }
        "arith.addi" => binary_int(op, builder, value_map, fn_name, |b, a, c| {
            b.ins().iadd(a, c)
        }),
        "arith.subi" => binary_int(op, builder, value_map, fn_name, |b, a, c| {
            b.ins().isub(a, c)
        }),
        "arith.muli" => binary_int(op, builder, value_map, fn_name, |b, a, c| {
            b.ins().imul(a, c)
        }),
        "arith.addf" => binary_int(op, builder, value_map, fn_name, |b, a, c| {
            b.ins().fadd(a, c)
        }),
        "arith.subf" => binary_int(op, builder, value_map, fn_name, |b, a, c| {
            b.ins().fsub(a, c)
        }),
        "arith.mulf" => binary_int(op, builder, value_map, fn_name, |b, a, c| {
            b.ins().fmul(a, c)
        }),
        "arith.divf" => binary_int(op, builder, value_map, fn_name, |b, a, c| {
            b.ins().fdiv(a, c)
        }),
        // T11-D59 / S6-C3 : memref.load + memref.store. See
        // `specs/02_IR.csl § MEMORY-OPS` and the JIT-side mirror in `jit.rs`.
        "memref.load" => obj_lower_memref_load(op, builder, value_map, fn_name),
        "memref.store" => obj_lower_memref_store(op, builder, value_map, fn_name),
        "func.return" => {
            let mut args = Vec::with_capacity(op.operands.len());
            for vid in &op.operands {
                let v = *value_map
                    .get(vid)
                    .ok_or_else(|| ObjectError::UnknownValueId {
                        fn_name: fn_name.to_string(),
                        value_id: vid.0,
                    })?;
                args.push(v);
            }
            builder.ins().return_(&args);
            Ok(true)
        }
        other => Err(ObjectError::UnsupportedOp {
            fn_name: fn_name.to_string(),
            op_name: other.to_string(),
        }),
    }
}

// ───────────────────────────────────────────────────────────────────────
// § T11-D59 / S6-C3 : object-emit memref.load / memref.store helpers.
//
// Mirrors the JIT lowering in `jit.rs`. The two paths share the same
// alignment + ptr+offset derivation logic, but the JIT and Object backends
// each declare their own helper (no shared module yet — extracting them is
// the deferred follow-up that lets cmp / select / call also be one source
// of truth).
// ───────────────────────────────────────────────────────────────────────

fn obj_memref_alignment(op: &MirOp, elem_ty: &MirType) -> Option<u32> {
    let natural = elem_ty.natural_alignment()?;
    let parsed = op
        .attributes
        .iter()
        .find(|(k, _)| k == "alignment")
        .and_then(|(_, v)| v.parse::<u32>().ok());
    Some(parsed.map_or(natural, |a| a.max(natural)))
}

fn obj_memref_flags(_align: u32) -> cranelift_codegen::ir::MemFlags {
    let mut flags = cranelift_codegen::ir::MemFlags::new();
    flags.set_aligned();
    flags
}

fn obj_memref_effective_addr(
    builder: &mut FunctionBuilder<'_>,
    value_map: &HashMap<ValueId, cranelift_codegen::ir::Value>,
    ptr_id: ValueId,
    offset_id: Option<ValueId>,
    fn_name: &str,
) -> Result<cranelift_codegen::ir::Value, ObjectError> {
    let ptr = *value_map
        .get(&ptr_id)
        .ok_or_else(|| ObjectError::UnknownValueId {
            fn_name: fn_name.to_string(),
            value_id: ptr_id.0,
        })?;
    if let Some(off_id) = offset_id {
        let off = *value_map
            .get(&off_id)
            .ok_or_else(|| ObjectError::UnknownValueId {
                fn_name: fn_name.to_string(),
                value_id: off_id.0,
            })?;
        Ok(builder.ins().iadd(ptr, off))
    } else {
        Ok(ptr)
    }
}

fn obj_lower_memref_load(
    op: &MirOp,
    builder: &mut FunctionBuilder<'_>,
    value_map: &mut HashMap<ValueId, cranelift_codegen::ir::Value>,
    fn_name: &str,
) -> Result<bool, ObjectError> {
    let r = op
        .results
        .first()
        .ok_or_else(|| ObjectError::LoweringFailed {
            fn_name: fn_name.to_string(),
            detail: "memref.load with no result".to_string(),
        })?;
    let elem_ty = mir_type_to_cl(&r.ty).ok_or_else(|| ObjectError::NonScalarType {
        fn_name: fn_name.to_string(),
        slot: 0,
        ty: format!("{}", r.ty),
    })?;
    let &ptr_id = op
        .operands
        .first()
        .ok_or_else(|| ObjectError::LoweringFailed {
            fn_name: fn_name.to_string(),
            detail: "memref.load expected at least 1 operand (ptr)".to_string(),
        })?;
    let offset_id = op.operands.get(1).copied();
    if op.operands.len() > 2 {
        return Err(ObjectError::LoweringFailed {
            fn_name: fn_name.to_string(),
            detail: format!(
                "memref.load expected 1 or 2 operands ; got {}",
                op.operands.len()
            ),
        });
    }
    let align = obj_memref_alignment(op, &r.ty).ok_or_else(|| ObjectError::NonScalarType {
        fn_name: fn_name.to_string(),
        slot: 0,
        ty: format!("{}", r.ty),
    })?;
    let addr = obj_memref_effective_addr(builder, value_map, ptr_id, offset_id, fn_name)?;
    let flags = obj_memref_flags(align);
    let v = builder.ins().load(elem_ty, flags, addr, 0);
    value_map.insert(r.id, v);
    Ok(false)
}

fn obj_lower_memref_store(
    op: &MirOp,
    builder: &mut FunctionBuilder<'_>,
    value_map: &mut HashMap<ValueId, cranelift_codegen::ir::Value>,
    fn_name: &str,
) -> Result<bool, ObjectError> {
    if !op.results.is_empty() {
        return Err(ObjectError::LoweringFailed {
            fn_name: fn_name.to_string(),
            detail: format!(
                "memref.store must have 0 results ; got {}",
                op.results.len()
            ),
        });
    }
    let &val_id = op
        .operands
        .first()
        .ok_or_else(|| ObjectError::LoweringFailed {
            fn_name: fn_name.to_string(),
            detail: "memref.store expected operands (val, ptr [, offset])".to_string(),
        })?;
    let &ptr_id = op
        .operands
        .get(1)
        .ok_or_else(|| ObjectError::LoweringFailed {
            fn_name: fn_name.to_string(),
            detail: "memref.store expected at least 2 operands (val, ptr)".to_string(),
        })?;
    let offset_id = op.operands.get(2).copied();
    if op.operands.len() > 3 {
        return Err(ObjectError::LoweringFailed {
            fn_name: fn_name.to_string(),
            detail: format!(
                "memref.store expected 2 or 3 operands ; got {}",
                op.operands.len()
            ),
        });
    }
    let val = *value_map
        .get(&val_id)
        .ok_or_else(|| ObjectError::UnknownValueId {
            fn_name: fn_name.to_string(),
            value_id: val_id.0,
        })?;
    let val_ty = builder.func.dfg.value_type(val);
    let mir_elem = obj_cl_to_mir_for_align(val_ty);
    let align = mir_elem
        .as_ref()
        .and_then(|t| obj_memref_alignment(op, t))
        .ok_or_else(|| ObjectError::LoweringFailed {
            fn_name: fn_name.to_string(),
            detail: format!("memref.store value type `{val_ty}` has no natural alignment"),
        })?;
    let addr = obj_memref_effective_addr(builder, value_map, ptr_id, offset_id, fn_name)?;
    let flags = obj_memref_flags(align);
    builder.ins().store(flags, val, addr, 0);
    Ok(false)
}

fn obj_cl_to_mir_for_align(t: cranelift_codegen::ir::Type) -> Option<MirType> {
    if t == cl_types::I8 {
        Some(MirType::Int(IntWidth::I8))
    } else if t == cl_types::I16 {
        Some(MirType::Int(IntWidth::I16))
    } else if t == cl_types::I32 {
        Some(MirType::Int(IntWidth::I32))
    } else if t == cl_types::I64 {
        Some(MirType::Int(IntWidth::I64))
    } else if t == cl_types::F32 {
        Some(MirType::Float(FloatWidth::F32))
    } else if t == cl_types::F64 {
        Some(MirType::Float(FloatWidth::F64))
    } else {
        None
    }
}

fn binary_int<F>(
    op: &MirOp,
    builder: &mut FunctionBuilder<'_>,
    value_map: &mut HashMap<ValueId, cranelift_codegen::ir::Value>,
    fn_name: &str,
    emit: F,
) -> Result<bool, ObjectError>
where
    F: FnOnce(
        &mut FunctionBuilder<'_>,
        cranelift_codegen::ir::Value,
        cranelift_codegen::ir::Value,
    ) -> cranelift_codegen::ir::Value,
{
    let r = op
        .results
        .first()
        .ok_or_else(|| ObjectError::LoweringFailed {
            fn_name: fn_name.to_string(),
            detail: format!("{} with no result", op.name),
        })?;
    if op.operands.len() < 2 {
        return Err(ObjectError::LoweringFailed {
            fn_name: fn_name.to_string(),
            detail: format!("{} expected 2 operands, got {}", op.name, op.operands.len()),
        });
    }
    let a = *value_map
        .get(&op.operands[0])
        .ok_or_else(|| ObjectError::UnknownValueId {
            fn_name: fn_name.to_string(),
            value_id: op.operands[0].0,
        })?;
    let b = *value_map
        .get(&op.operands[1])
        .ok_or_else(|| ObjectError::UnknownValueId {
            fn_name: fn_name.to_string(),
            value_id: op.operands[1].0,
        })?;
    let v = emit(builder, a, b);
    value_map.insert(r.id, v);
    Ok(false)
}

// ───────────────────────────────────────────────────────────────────────
// § MirType → cranelift Type
// ───────────────────────────────────────────────────────────────────────

fn mir_type_to_cl(t: &MirType) -> Option<cranelift_codegen::ir::Type> {
    match t {
        MirType::Int(IntWidth::I32) => Some(cl_types::I32),
        MirType::Int(IntWidth::I64) => Some(cl_types::I64),
        MirType::Int(IntWidth::I16) => Some(cl_types::I16),
        MirType::Int(IntWidth::I8) => Some(cl_types::I8),
        MirType::Float(FloatWidth::F32) => Some(cl_types::F32),
        MirType::Float(FloatWidth::F64) => Some(cl_types::F64),
        MirType::Bool => Some(cl_types::I8),
        _ => None,
    }
}

// ───────────────────────────────────────────────────────────────────────
// § tests
// ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use cssl_mir::{MirFunc, MirModule, MirOp, ValueId};

    /// Construct a `fn <name>() -> <ret_ty> { value }` MirFunc using only the
    /// real cssl_mir builder API.
    fn build_const_return_fn(name: &str, value: i64, ret_ty: MirType) -> MirFunc {
        let mut f = MirFunc::new(name, vec![], vec![ret_ty.clone()]);
        let const_op = MirOp::std("arith.constant")
            .with_attribute("value", value.to_string())
            .with_result(ValueId(0), ret_ty);
        let return_op = MirOp::std("func.return").with_operand(ValueId(0));
        f.push_op(const_op);
        f.push_op(return_op);
        f
    }

    use crate::abi::ObjectFormat;

    #[test]
    fn host_default_format_is_platform_appropriate() {
        let fmt = host_default_format();
        if cfg!(target_os = "windows") {
            assert_eq!(fmt, ObjectFormat::Coff);
        } else if cfg!(target_os = "macos") {
            assert_eq!(fmt, ObjectFormat::MachO);
        } else {
            assert_eq!(fmt, ObjectFormat::Elf);
        }
    }

    #[test]
    fn abi_extensions_match_format() {
        assert_eq!(ObjectFormat::Coff.extension(), ".obj");
        assert_eq!(ObjectFormat::Elf.extension(), ".o");
        assert_eq!(ObjectFormat::MachO.extension(), ".o");
    }

    #[test]
    fn magic_prefixes_match_format() {
        assert_eq!(magic_prefix(ObjectFormat::Elf), b"\x7FELF");
        assert_eq!(magic_prefix(ObjectFormat::Coff), &[0x64_u8, 0x86]);
        assert_eq!(
            magic_prefix(ObjectFormat::MachO),
            &[0xCF_u8, 0xFA, 0xED, 0xFE]
        );
    }

    #[test]
    fn emit_minimal_main_returns_bytes() {
        let main = build_const_return_fn("main", 42, MirType::Int(IntWidth::I32));
        let mut module = MirModule::new();
        module.push_func(main);
        let bytes = emit_object_module(&module).expect("emit ok");
        assert!(!bytes.is_empty(), "produced object bytes");
    }

    #[test]
    fn emit_minimal_main_starts_with_host_magic() {
        let main = build_const_return_fn("main", 42, MirType::Int(IntWidth::I32));
        let mut module = MirModule::new();
        module.push_func(main);
        let bytes = emit_object_module(&module).expect("emit ok");
        let host_magic = magic_prefix(host_default_format());
        assert!(
            bytes.starts_with(host_magic),
            "expected magic {:02X?} ; first 8 bytes : {:02X?}",
            host_magic,
            &bytes[..bytes.len().min(8)],
        );
    }

    #[test]
    fn emit_main_with_i64_return_succeeds() {
        let main = build_const_return_fn("main_i64", 42, MirType::Int(IntWidth::I64));
        let mut module = MirModule::new();
        module.push_func(main);
        let bytes = emit_object_module(&module).expect("emit ok");
        assert!(!bytes.is_empty());
    }

    #[test]
    fn emit_main_with_f32_return_succeeds() {
        let mut f = MirFunc::new("main_f32", vec![], vec![MirType::Float(FloatWidth::F32)]);
        f.push_op(
            MirOp::std("arith.constant")
                .with_attribute("value", "2.5")
                .with_result(ValueId(0), MirType::Float(FloatWidth::F32)),
        );
        f.push_op(MirOp::std("func.return").with_operand(ValueId(0)));
        let mut module = MirModule::new();
        module.push_func(f);
        let bytes = emit_object_module(&module).expect("emit ok");
        assert!(!bytes.is_empty());
    }

    #[test]
    fn emit_skips_generic_fns() {
        let mut generic = build_const_return_fn("generic", 0, MirType::Int(IntWidth::I32));
        generic.is_generic = true;
        let main = build_const_return_fn("main", 7, MirType::Int(IntWidth::I32));
        let mut module = MirModule::new();
        module.push_func(generic);
        module.push_func(main);
        let bytes = emit_object_module(&module).expect("emit ok");
        // The bytes should still be a valid object file with `main` defined.
        let host_magic = magic_prefix(host_default_format());
        assert!(bytes.starts_with(host_magic));
    }

    #[test]
    fn emit_unsupported_op_returns_error() {
        let mut f = MirFunc::new("unsupported", vec![], vec![]);
        f.push_op(MirOp::std("frobnicate.foo"));
        f.push_op(MirOp::std("func.return"));
        let mut module = MirModule::new();
        module.push_func(f);
        let r = emit_object_module(&module);
        assert!(matches!(r, Err(ObjectError::UnsupportedOp { .. })));
    }

    #[test]
    fn emit_multi_block_body_returns_error() {
        use cssl_mir::MirBlock;
        let mut f = MirFunc::new("multi", vec![], vec![]);
        f.body.push(MirBlock::new("exit"));
        let mut module = MirModule::new();
        module.push_func(f);
        let r = emit_object_module(&module);
        assert!(matches!(r, Err(ObjectError::MultiBlockBody { .. })));
    }

    #[test]
    fn emit_module_with_zero_fns_is_empty_but_valid() {
        let module = MirModule::new();
        let bytes = emit_object_module(&module).expect("emit ok");
        // Empty modules still produce a valid object header.
        let host_magic = magic_prefix(host_default_format());
        assert!(bytes.starts_with(host_magic));
    }

    #[test]
    fn emit_addi_function_succeeds() {
        // fn add(a: i32, b: i32) -> i32 { a + b }
        // MirFunc::new wires entry-block args from the params list with
        // sequential ValueId(0..n).
        let mut f = MirFunc::new(
            "add",
            vec![MirType::Int(IntWidth::I32), MirType::Int(IntWidth::I32)],
            vec![MirType::Int(IntWidth::I32)],
        );
        f.push_op(
            MirOp::std("arith.addi")
                .with_operand(ValueId(0))
                .with_operand(ValueId(1))
                .with_result(ValueId(2), MirType::Int(IntWidth::I32)),
        );
        f.push_op(MirOp::std("func.return").with_operand(ValueId(2)));
        let mut module = MirModule::new();
        module.push_func(f);
        let bytes = emit_object_module(&module).expect("emit ok");
        assert!(!bytes.is_empty());
    }

    // ─────────────────────────────────────────────────────────────────────
    // § T11-D59 / S6-C3 : memref.load + memref.store object-emit tests.
    //
    // These tests confirm the object backend produces non-empty bytes with
    // the host-magic prefix when the input MIR contains memref ops. End-to-
    // end runtime verification of the produced object lives in the JIT
    // module (above) ; here we verify the codegen path doesn't reject the
    // ops or panic.
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn obj_emit_memref_load_i32_succeeds() {
        // fn load_i32(ptr: i64) -> i32 { memref.load ptr }
        let mut f = MirFunc::new(
            "load_i32",
            vec![MirType::Int(IntWidth::I64)],
            vec![MirType::Int(IntWidth::I32)],
        );
        f.push_op(
            MirOp::std("memref.load")
                .with_operand(ValueId(0))
                .with_result(ValueId(1), MirType::Int(IntWidth::I32)),
        );
        f.push_op(MirOp::std("func.return").with_operand(ValueId(1)));
        let mut module = MirModule::new();
        module.push_func(f);
        let bytes = emit_object_module(&module).expect("emit ok");
        assert!(bytes.starts_with(magic_prefix(host_default_format())));
    }

    #[test]
    fn obj_emit_memref_store_i32_succeeds() {
        // fn store_i32(val: i32, ptr: i64) { memref.store val, ptr }
        let mut f = MirFunc::new(
            "store_i32",
            vec![MirType::Int(IntWidth::I32), MirType::Int(IntWidth::I64)],
            vec![],
        );
        f.push_op(
            MirOp::std("memref.store")
                .with_operand(ValueId(0))
                .with_operand(ValueId(1)),
        );
        f.push_op(MirOp::std("func.return"));
        let mut module = MirModule::new();
        module.push_func(f);
        let bytes = emit_object_module(&module).expect("emit ok");
        assert!(bytes.starts_with(magic_prefix(host_default_format())));
    }

    #[test]
    fn obj_emit_memref_load_with_offset_succeeds() {
        // fn load_at(ptr: i64, off: i64) -> i32 { memref.load(ptr, off) }
        let mut f = MirFunc::new(
            "load_at",
            vec![MirType::Int(IntWidth::I64), MirType::Int(IntWidth::I64)],
            vec![MirType::Int(IntWidth::I32)],
        );
        f.push_op(
            MirOp::std("memref.load")
                .with_operand(ValueId(0))
                .with_operand(ValueId(1))
                .with_result(ValueId(2), MirType::Int(IntWidth::I32)),
        );
        f.push_op(MirOp::std("func.return").with_operand(ValueId(2)));
        let mut module = MirModule::new();
        module.push_func(f);
        let bytes = emit_object_module(&module).expect("emit ok");
        assert!(bytes.starts_with(magic_prefix(host_default_format())));
    }

    #[test]
    fn obj_emit_memref_load_with_alignment_attr_succeeds() {
        let mut f = MirFunc::new(
            "load_aligned",
            vec![MirType::Int(IntWidth::I64)],
            vec![MirType::Int(IntWidth::I32)],
        );
        f.push_op(
            MirOp::std("memref.load")
                .with_operand(ValueId(0))
                .with_result(ValueId(1), MirType::Int(IntWidth::I32))
                .with_attribute("alignment", "8"),
        );
        f.push_op(MirOp::std("func.return").with_operand(ValueId(1)));
        let mut module = MirModule::new();
        module.push_func(f);
        let bytes = emit_object_module(&module).expect("emit ok");
        assert!(bytes.starts_with(magic_prefix(host_default_format())));
    }

    #[test]
    fn obj_emit_memref_store_with_result_errors() {
        let mut f = MirFunc::new(
            "bad_store",
            vec![MirType::Int(IntWidth::I32), MirType::Int(IntWidth::I64)],
            vec![],
        );
        f.push_op(
            MirOp::std("memref.store")
                .with_operand(ValueId(0))
                .with_operand(ValueId(1))
                .with_result(ValueId(2), MirType::Int(IntWidth::I32)),
        );
        f.push_op(MirOp::std("func.return"));
        let mut module = MirModule::new();
        module.push_func(f);
        let r = emit_object_module(&module);
        assert!(matches!(r, Err(ObjectError::LoweringFailed { .. })));
    }
}
