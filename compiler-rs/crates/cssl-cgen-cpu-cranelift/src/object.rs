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
//!   - Heap (S6-B1, T11-D57) : `cssl.heap.alloc` / `cssl.heap.dealloc` /
//!     `cssl.heap.realloc` lowered to `__cssl_alloc` / `__cssl_free` /
//!     `__cssl_realloc` import calls into `cssl-rt` (T11-D52, S6-A1).
//!     Result-bind discipline mirrors the MIR signature : `alloc` and
//!     `realloc` produce a single pointer-typed result ; `dealloc`
//!     produces nothing.
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
    // T11-D58 / S6-C1 : the outer fn body still has exactly one entry block
    // ; structured-CFG ops (scf.if, future scf.for/while) carry their own
    // nested regions inside that block. Multi-entry-block bodies remain
    // disallowed at stage-0 — those would imply unstructured CFG which
    // structured-CFG D5 will reject anyway.
    if mir_fn.body.blocks.len() > 1 {
        return Err(ObjectError::MultiBlockBody {
            fn_name: mir_fn.name.clone(),
        });
    }

    // § 1. Build cranelift signature.
    let call_conv = obj_module.isa().default_call_conv();
    // Stage-0 single-host : the active ISA's pointer type is what `__cssl_alloc`
    // and friends operate on. Cache once for both signature emission and the
    // per-op lowering loop below.
    let ptr_ty = obj_module.isa().pointer_type();
    let mut sig = Signature::new(call_conv);
    for (idx, p_ty) in mir_fn.params.iter().enumerate() {
        let cl_ty = mir_type_to_cl(p_ty, ptr_ty).ok_or_else(|| ObjectError::NonScalarType {
            fn_name: mir_fn.name.clone(),
            slot: idx,
            ty: format!("{p_ty}"),
        })?;
        sig.params.push(AbiParam::new(cl_ty));
    }
    for (idx, r_ty) in mir_fn.results.iter().enumerate() {
        let cl_ty = mir_type_to_cl(r_ty, ptr_ty).ok_or_else(|| ObjectError::NonScalarType {
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

    // § T11-D57 (S6-B1) — pre-scan body ops for `cssl.heap.*` references.
    //   For each unique heap op present in this fn's body, declare the
    //   corresponding `__cssl_*` symbol from `cssl-rt` as `Linkage::Import`,
    //   then bind a per-fn `FuncRef` so the body-lowering loop can emit
    //   a real `call` instruction. Mirrors the JIT pattern at `jit.rs`.
    //   Pattern is identical to the libm transcendental wiring landed in
    //   T11-D29 — duplicated here rather than refactored because cssl-mir
    //   cannot dev-dep cssl-cgen-cpu-cranelift (cycle landmine, see HANDOFF).
    let heap_refs = declare_heap_imports_for_fn(obj_module, codegen_ctx, mir_fn, ptr_ty)?;

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
                returned = lower_one_op(
                    op,
                    &mut builder,
                    &mut value_map,
                    &mir_fn.name,
                    &heap_refs,
                    ptr_ty,
                )?;
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
// § cssl-rt heap-FFI imports — declare-once-per-fn pre-scan.
//
//   Each entry in [`HeapImports`] maps the source-form MIR op-name to the
//   per-fn `FuncRef` that body lowering can issue a `call` against. Sigs
//   match the FFI surface in `cssl-rt::ffi`. The signatures are all
//   `usize`-shaped on the host pointer-type ; we use the active ISA's
//   pointer width for both `*mut u8` and `usize` so this matches the
//   Rust ABI on x86_64 (8 bytes) without target-specific branches.
// ───────────────────────────────────────────────────────────────────────

/// FFI symbol name on the cssl-rt side. Renaming either side requires
/// updating both — see HANDOFF_SESSION_6 § LANDMINES + cssl-rt/src/ffi.rs.
const HEAP_ALLOC_SYMBOL: &str = "__cssl_alloc";
const HEAP_FREE_SYMBOL: &str = "__cssl_free";
const HEAP_REALLOC_SYMBOL: &str = "__cssl_realloc";

/// Per-fn map of MIR heap-op name → cranelift `FuncRef` for the imported
/// `cssl-rt` symbol. An entry is only present when the fn body actually
/// references the corresponding op — keeps the import surface lean.
#[derive(Default)]
struct HeapImports {
    refs: HashMap<&'static str, cranelift_codegen::ir::FuncRef>,
}

impl HeapImports {
    fn get(&self, op_name: &str) -> Option<cranelift_codegen::ir::FuncRef> {
        self.refs.get(op_name).copied()
    }
}

fn declare_heap_imports_for_fn(
    obj_module: &mut ObjectModule,
    codegen_ctx: &mut Context,
    mir_fn: &MirFunc,
    ptr_ty: cranelift_codegen::ir::Type,
) -> Result<HeapImports, ObjectError> {
    let mut imports = HeapImports::default();
    let Some(entry_block) = mir_fn.body.blocks.first() else {
        return Ok(imports);
    };
    // Walk this fn's ops once and remember which heap ops are referenced.
    let mut needs_alloc = false;
    let mut needs_free = false;
    let mut needs_realloc = false;
    for op in &entry_block.ops {
        match op.name.as_str() {
            "cssl.heap.alloc" => needs_alloc = true,
            "cssl.heap.dealloc" => needs_free = true,
            "cssl.heap.realloc" => needs_realloc = true,
            _ => {}
        }
    }
    let call_conv = obj_module.isa().default_call_conv();
    let abi_ptr = AbiParam::new(ptr_ty);

    if needs_alloc {
        // (size : usize, align : usize) -> *mut u8
        let mut sig = Signature::new(call_conv);
        sig.params.push(abi_ptr);
        sig.params.push(abi_ptr);
        sig.returns.push(abi_ptr);
        let id = obj_module
            .declare_function(HEAP_ALLOC_SYMBOL, Linkage::Import, &sig)
            .map_err(|e| ObjectError::LoweringFailed {
                fn_name: mir_fn.name.clone(),
                detail: format!("declare {HEAP_ALLOC_SYMBOL} : {e}"),
            })?;
        let fref = obj_module.declare_func_in_func(id, &mut codegen_ctx.func);
        imports.refs.insert("cssl.heap.alloc", fref);
    }
    if needs_free {
        // (ptr : *mut u8, size : usize, align : usize) -> ()
        let mut sig = Signature::new(call_conv);
        sig.params.push(abi_ptr);
        sig.params.push(abi_ptr);
        sig.params.push(abi_ptr);
        let id = obj_module
            .declare_function(HEAP_FREE_SYMBOL, Linkage::Import, &sig)
            .map_err(|e| ObjectError::LoweringFailed {
                fn_name: mir_fn.name.clone(),
                detail: format!("declare {HEAP_FREE_SYMBOL} : {e}"),
            })?;
        let fref = obj_module.declare_func_in_func(id, &mut codegen_ctx.func);
        imports.refs.insert("cssl.heap.dealloc", fref);
    }
    if needs_realloc {
        // (ptr, old_size, new_size, align) -> *mut u8
        let mut sig = Signature::new(call_conv);
        sig.params.push(abi_ptr);
        sig.params.push(abi_ptr);
        sig.params.push(abi_ptr);
        sig.params.push(abi_ptr);
        sig.returns.push(abi_ptr);
        let id = obj_module
            .declare_function(HEAP_REALLOC_SYMBOL, Linkage::Import, &sig)
            .map_err(|e| ObjectError::LoweringFailed {
                fn_name: mir_fn.name.clone(),
                detail: format!("declare {HEAP_REALLOC_SYMBOL} : {e}"),
            })?;
        let fref = obj_module.declare_func_in_func(id, &mut codegen_ctx.func);
        imports.refs.insert("cssl.heap.realloc", fref);
    }
    Ok(imports)
}

// ───────────────────────────────────────────────────────────────────────
// § per-op lowering (subset)
// ───────────────────────────────────────────────────────────────────────

fn lower_one_op(
    op: &MirOp,
    builder: &mut FunctionBuilder<'_>,
    value_map: &mut HashMap<ValueId, cranelift_codegen::ir::Value>,
    fn_name: &str,
    heap_refs: &HeapImports,
    ptr_ty: cranelift_codegen::ir::Type,
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
            let cl_ty =
                mir_type_to_cl(&r.ty, ptr_ty).ok_or_else(|| ObjectError::NonScalarType {
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
        // § T11-D59 (S6-C3) — memref.load + memref.store. See
        // `specs/02_IR.csl § MEMORY-OPS` and the JIT-side mirror in `jit.rs`.
        "memref.load" => obj_lower_memref_load(op, builder, value_map, fn_name, ptr_ty),
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
        // § T11-D57 (S6-B1) — heap-FFI lowering. Each op resolves its
        //   import via the per-fn `HeapImports` map (declared up front in
        //   `declare_heap_imports_for_fn`), then issues a `call` carrying
        //   the operands in MIR-source order. `alloc` and `realloc` bind
        //   one ptr-typed result ; `dealloc` produces no result.
        "cssl.heap.alloc" => emit_heap_call(
            op, builder, value_map, fn_name, heap_refs, ptr_ty, /* expects_result = */ true,
        ),
        "cssl.heap.dealloc" => emit_heap_call(
            op, builder, value_map, fn_name, heap_refs, ptr_ty, /* expects_result = */ false,
        ),
        "cssl.heap.realloc" => emit_heap_call(
            op, builder, value_map, fn_name, heap_refs, ptr_ty, /* expects_result = */ true,
        ),
        // § T11-D58 (S6-C1) — structured-control-flow lowering. `scf.if`
        //   delegates to the shared `crate::scf::lower_scf_if` helper which
        //   creates the then/else/merge blocks + emits `brif`. `scf.yield`
        //   is consumed by that helper directly ; encountering it at the
        //   outer dispatch level means the parent region terminator leaked,
        //   which we treat as a no-op here. D5 (StructuredCfgValidator) will
        //   reject bare top-level scf.yield in a future slice.
        "scf.if" => lower_scf_if_in_object(op, builder, value_map, fn_name, heap_refs, ptr_ty),
        // § T11-D61 (S6-C2) — structured loops. Each delegates to the
        //   matching `crate::scf::lower_scf_*` helper ; the body-walker
        //   dispatcher closure re-enters `lower_one_op` so nested ops
        //   (arith / heap / nested scf.*) flow through the same dispatch.
        "scf.loop" => lower_scf_loop_in_object(op, builder, value_map, fn_name, heap_refs, ptr_ty),
        "scf.while" => {
            lower_scf_while_in_object(op, builder, value_map, fn_name, heap_refs, ptr_ty)
        }
        "scf.for" => lower_scf_for_in_object(op, builder, value_map, fn_name, heap_refs, ptr_ty),
        "scf.yield" => Ok(false),
        // § T11-D77 (S6-C5 redo) — `cssl.closure` materializes the closure VALUE
        //   (the `(fn-ptr, env-ptr)` fat-pair). At stage-0 the body_lower has
        //   already emitted the env-pack sequence (arith.constant + arith.constant
        //   + cssl.heap.alloc + per-capture {arith.constant + memref.store}), so
        //   here we just bind the result-id to the env-ptr operand for closures
        //   with ≥1 capture, or to a typed-zero pointer sentinel for closures
        //   with no captures. Inner body region is intentionally not walked —
        //   indirect-call lowering through the closure is deferred per spec.
        "cssl.closure" => obj_lower_closure(op, builder, value_map, fn_name, ptr_ty),
        other => Err(ObjectError::UnsupportedOp {
            fn_name: fn_name.to_string(),
            op_name: other.to_string(),
        }),
    }
}

/// § T11-D77 (S6-C5 redo) — `cssl.closure` lowering for the object backend.
///
/// Stage-0 contract :
///   - Reads `capture_count` from op attributes. The trailing operand is the
///     env-ptr when `capture_count ≥ 1` (operand-index = capture_count after
///     the K capture-source operands). When `capture_count = 0`, no env-ptr
///     operand exists ; bind the result to a typed-zero ptr sentinel.
///   - Binds the closure result-id (when present) so subsequent ops can
///     reference the closure value (e.g., a debug print, a return).
///   - Emits NO cranelift instructions of its own (the env-pack already
///     emitted its alloc + stores ; the closure value is the env-ptr at
///     stage-0).
///
/// The inner body region is intentionally NOT walked — that's the
/// indirect-call-site lowerer's job once a CSSLv3 source-call-site against
/// a closure-typed value parses + lowers. Until then the body region rides
/// along the MIR for diagnostic + future-pass consumption.
fn obj_lower_closure(
    op: &MirOp,
    builder: &mut FunctionBuilder<'_>,
    value_map: &mut HashMap<ValueId, cranelift_codegen::ir::Value>,
    fn_name: &str,
    ptr_ty: cranelift_codegen::ir::Type,
) -> Result<bool, ObjectError> {
    let capture_count: usize = op
        .attributes
        .iter()
        .find(|(k, _)| k == "capture_count")
        .and_then(|(_, v)| v.parse().ok())
        .unwrap_or(0);

    // Locate the closure result-id (when typed). Stage-0 always emits one
    // result of type `!cssl.closure`, but we tolerate its absence rather
    // than panic — a future MIR-pass might strip the result for
    // diagnostic-only uses.
    let Some(r) = op.results.first() else {
        return Ok(false);
    };

    // The env-ptr operand (when present) is at index = capture_count.
    let env_value =
        if capture_count > 0 {
            let env_op_idx = capture_count;
            let env_vid = op.operands.get(env_op_idx).copied().ok_or_else(|| {
                ObjectError::LoweringFailed {
                    fn_name: fn_name.to_string(),
                    detail: format!(
                    "cssl.closure : capture_count={capture_count} but operand[{env_op_idx}] missing"
                ),
                }
            })?;
            *value_map
                .get(&env_vid)
                .ok_or_else(|| ObjectError::UnknownValueId {
                    fn_name: fn_name.to_string(),
                    value_id: env_vid.0,
                })?
        } else {
            // No captures ⇒ env-ptr is null (typed zero). Stage-0 sentinel that
            // preserves the value-map invariant ("every result-id has a value")
            // without a real allocation.
            builder.ins().iconst(ptr_ty, 0)
        };

    value_map.insert(r.id, env_value);
    Ok(false)
}

/// Shared helper for the three `cssl.heap.*` ops. Resolves the import,
/// gathers operands (with type-coercion to the host pointer-type so a
/// `i64`-typed `arith.constant` for size/align matches the FFI signature),
/// and binds the call's result if any.
///
/// § COERCION
///   Operands flow through MIR as scalar integers (`i64`) for size/align
///   and `!cssl.ptr` for pointers. Cranelift wants every operand to match
///   the imported function's `AbiParam` type (host-ptr-width). We coerce
///   non-matching integer operands via `uextend` / `ireduce` as needed —
///   this is correct for `usize`-shaped sizes on 64-bit hosts (no-op when
///   already `i64`) and would also work for 32-bit hosts (would emit a
///   single `ireduce`). For pointer operands we rely on `MirType::Ptr` →
///   `ptr_ty` already matching, so no coercion is needed.
fn emit_heap_call(
    op: &MirOp,
    builder: &mut FunctionBuilder<'_>,
    value_map: &mut HashMap<ValueId, cranelift_codegen::ir::Value>,
    fn_name: &str,
    heap_refs: &HeapImports,
    ptr_ty: cranelift_codegen::ir::Type,
    expects_result: bool,
) -> Result<bool, ObjectError> {
    let fref = heap_refs
        .get(op.name.as_str())
        .ok_or_else(|| ObjectError::LoweringFailed {
            fn_name: fn_name.to_string(),
            detail: format!("`{}` import not declared (pre-scan bug)", op.name),
        })?;
    let mut args = Vec::with_capacity(op.operands.len());
    for vid in &op.operands {
        let raw = *value_map
            .get(vid)
            .ok_or_else(|| ObjectError::UnknownValueId {
                fn_name: fn_name.to_string(),
                value_id: vid.0,
            })?;
        let raw_ty = builder.func.dfg.value_type(raw);
        let coerced = if raw_ty == ptr_ty {
            raw
        } else if raw_ty.bits() < ptr_ty.bits() {
            builder.ins().uextend(ptr_ty, raw)
        } else {
            // raw_ty.bits() > ptr_ty.bits() : narrow.
            builder.ins().ireduce(ptr_ty, raw)
        };
        args.push(coerced);
    }
    let call = builder.ins().call(fref, &args);
    if expects_result {
        let r = op
            .results
            .first()
            .ok_or_else(|| ObjectError::LoweringFailed {
                fn_name: fn_name.to_string(),
                detail: format!("{} expects a result but op carries none", op.name),
            })?;
        let results = builder.inst_results(call).to_vec();
        let cl_value = *results.first().ok_or_else(|| ObjectError::LoweringFailed {
            fn_name: fn_name.to_string(),
            detail: format!("{} produced no cranelift result value", op.name),
        })?;
        value_map.insert(r.id, cl_value);
    }
    Ok(false)
}

/// Adapter : translate the shared scf-helper's [`crate::scf::BackendOrScfError`]
/// into [`ObjectError`] so the outer object-emit dispatch keeps one error
/// type. Mirrors `lower_scf_if_in_jit` in `jit.rs`.
fn lower_scf_if_in_object(
    op: &MirOp,
    builder: &mut FunctionBuilder<'_>,
    value_map: &mut HashMap<ValueId, cranelift_codegen::ir::Value>,
    fn_name: &str,
    heap_refs: &HeapImports,
    ptr_ty: cranelift_codegen::ir::Type,
) -> Result<bool, ObjectError> {
    crate::scf::lower_scf_if(
        op,
        builder,
        value_map,
        fn_name,
        |branch_op, b, vm, name| -> Result<bool, ObjectError> {
            lower_one_op(branch_op, b, vm, name, heap_refs, ptr_ty)
        },
    )
    .map_err(|e| match e {
        crate::scf::BackendOrScfError::Scf(scf_err) => ObjectError::LoweringFailed {
            fn_name: fn_name.to_string(),
            detail: format!("scf.if : {scf_err}"),
        },
        crate::scf::BackendOrScfError::Backend(obj_err) => obj_err,
    })
}

/// Adapter : delegate `scf.loop` lowering to [`crate::scf::lower_scf_loop`].
fn lower_scf_loop_in_object(
    op: &MirOp,
    builder: &mut FunctionBuilder<'_>,
    value_map: &mut HashMap<ValueId, cranelift_codegen::ir::Value>,
    fn_name: &str,
    heap_refs: &HeapImports,
    ptr_ty: cranelift_codegen::ir::Type,
) -> Result<bool, ObjectError> {
    crate::scf::lower_scf_loop(
        op,
        builder,
        value_map,
        fn_name,
        |body_op, b, vm, name| -> Result<bool, ObjectError> {
            lower_one_op(body_op, b, vm, name, heap_refs, ptr_ty)
        },
    )
    .map_err(|e| match e {
        crate::scf::BackendOrScfError::Scf(scf_err) => ObjectError::LoweringFailed {
            fn_name: fn_name.to_string(),
            detail: format!("scf.loop : {scf_err}"),
        },
        crate::scf::BackendOrScfError::Backend(obj_err) => obj_err,
    })
}

/// Adapter : delegate `scf.while` lowering to [`crate::scf::lower_scf_while`].
fn lower_scf_while_in_object(
    op: &MirOp,
    builder: &mut FunctionBuilder<'_>,
    value_map: &mut HashMap<ValueId, cranelift_codegen::ir::Value>,
    fn_name: &str,
    heap_refs: &HeapImports,
    ptr_ty: cranelift_codegen::ir::Type,
) -> Result<bool, ObjectError> {
    crate::scf::lower_scf_while(
        op,
        builder,
        value_map,
        fn_name,
        |body_op, b, vm, name| -> Result<bool, ObjectError> {
            lower_one_op(body_op, b, vm, name, heap_refs, ptr_ty)
        },
    )
    .map_err(|e| match e {
        crate::scf::BackendOrScfError::Scf(scf_err) => ObjectError::LoweringFailed {
            fn_name: fn_name.to_string(),
            detail: format!("scf.while : {scf_err}"),
        },
        crate::scf::BackendOrScfError::Backend(obj_err) => obj_err,
    })
}

/// Adapter : delegate `scf.for` lowering to [`crate::scf::lower_scf_for`].
fn lower_scf_for_in_object(
    op: &MirOp,
    builder: &mut FunctionBuilder<'_>,
    value_map: &mut HashMap<ValueId, cranelift_codegen::ir::Value>,
    fn_name: &str,
    heap_refs: &HeapImports,
    ptr_ty: cranelift_codegen::ir::Type,
) -> Result<bool, ObjectError> {
    crate::scf::lower_scf_for(
        op,
        builder,
        value_map,
        fn_name,
        |body_op, b, vm, name| -> Result<bool, ObjectError> {
            lower_one_op(body_op, b, vm, name, heap_refs, ptr_ty)
        },
    )
    .map_err(|e| match e {
        crate::scf::BackendOrScfError::Scf(scf_err) => ObjectError::LoweringFailed {
            fn_name: fn_name.to_string(),
            detail: format!("scf.for : {scf_err}"),
        },
        crate::scf::BackendOrScfError::Backend(obj_err) => obj_err,
    })
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
    ptr_ty: cranelift_codegen::ir::Type,
) -> Result<bool, ObjectError> {
    let r = op
        .results
        .first()
        .ok_or_else(|| ObjectError::LoweringFailed {
            fn_name: fn_name.to_string(),
            detail: "memref.load with no result".to_string(),
        })?;
    let elem_ty = mir_type_to_cl(&r.ty, ptr_ty).ok_or_else(|| ObjectError::NonScalarType {
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

fn mir_type_to_cl(
    t: &MirType,
    ptr_ty: cranelift_codegen::ir::Type,
) -> Option<cranelift_codegen::ir::Type> {
    match t {
        MirType::Int(IntWidth::I32) => Some(cl_types::I32),
        MirType::Int(IntWidth::I64) => Some(cl_types::I64),
        MirType::Int(IntWidth::I16) => Some(cl_types::I16),
        MirType::Int(IntWidth::I8) => Some(cl_types::I8),
        MirType::Float(FloatWidth::F32) => Some(cl_types::F32),
        MirType::Float(FloatWidth::F64) => Some(cl_types::F64),
        MirType::Bool => Some(cl_types::I8),
        // T11-D57 (S6-B1) — `!cssl.ptr` lowers to the active ISA's host
        //   pointer type. Tied to S6-A3's "ISA = host" assumption ;
        //   cross-compilation will revisit when target-triple resolution lands.
        MirType::Ptr | MirType::Handle => Some(ptr_ty),
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
    // § T11-D57 (S6-B1) — heap-FFI lowering.
    //   Builds a synthetic MirFunc that exercises each of the three
    //   `cssl.heap.*` ops in turn and asserts the produced object bytes
    //   start with the host magic. Functional verification (run the .o,
    //   confirm it allocs/frees) lives in the cssl-examples integration
    //   gate where it can link against cssl-rt.
    // ─────────────────────────────────────────────────────────────────────

    /// Build a MirFunc with two i64 entry-args (size, align) → !cssl.ptr,
    /// emitting a single `cssl.heap.alloc(size, align) -> ptr` then
    /// returning the pointer.
    fn build_alloc_passthrough() -> MirFunc {
        let mut f = MirFunc::new(
            "alloc_passthrough",
            vec![MirType::Int(IntWidth::I64), MirType::Int(IntWidth::I64)],
            vec![MirType::Ptr],
        );
        f.push_op(
            MirOp::std("cssl.heap.alloc")
                .with_operand(ValueId(0))
                .with_operand(ValueId(1))
                .with_result(ValueId(2), MirType::Ptr),
        );
        f.push_op(MirOp::std("func.return").with_operand(ValueId(2)));
        f
    }

    #[test]
    fn emit_heap_alloc_imports_cssl_alloc_symbol() {
        // The produced object must declare the imported `__cssl_alloc`
        // symbol — verified by compiling without panic.
        let mut module = MirModule::new();
        module.push_func(build_alloc_passthrough());
        let bytes = emit_object_module(&module).expect("emit heap.alloc ok");
        let host_magic = magic_prefix(host_default_format());
        assert!(
            bytes.starts_with(host_magic),
            "heap.alloc-using fn produced invalid object header"
        );
    }

    // ─────────────────────────────────────────────────────────────────────
    // § T11-D58 (S6-C1) — scf.if lowering through the object backend
    // ─────────────────────────────────────────────────────────────────────
    //
    // The shared `crate::scf` helper means object.rs and jit.rs use the same
    // brif/blocks shape. These tests assert that the object backend accepts
    // scf.if MIR + produces a valid object file with the host-platform magic.
    // Roundtrip-runtime tests live in jit.rs (which can actually execute) ;
    // here we verify byte-shape invariants.

    fn build_pick_i32_module() -> MirModule {
        use cssl_mir::{MirBlock, MirRegion};
        let mut f = MirFunc::new(
            "pick",
            vec![
                MirType::Bool,
                MirType::Int(IntWidth::I32),
                MirType::Int(IntWidth::I32),
            ],
            vec![MirType::Int(IntWidth::I32)],
        );
        f.next_value_id = 4;
        {
            let entry = f.body.entry_mut().unwrap();
            // MirFunc::new wires args ; we leave them.
            let mut then_blk = MirBlock::new("entry");
            then_blk
                .ops
                .push(MirOp::std("scf.yield").with_operand(ValueId(1)));
            let mut then_region = MirRegion::new();
            then_region.push(then_blk);
            let mut else_blk = MirBlock::new("entry");
            else_blk
                .ops
                .push(MirOp::std("scf.yield").with_operand(ValueId(2)));
            let mut else_region = MirRegion::new();
            else_region.push(else_blk);
            entry.ops.push(
                MirOp::std("scf.if")
                    .with_operand(ValueId(0))
                    .with_region(then_region)
                    .with_region(else_region)
                    .with_result(ValueId(3), MirType::Int(IntWidth::I32)),
            );
            entry
                .ops
                .push(MirOp::std("func.return").with_operand(ValueId(3)));
        }
        let mut m = MirModule::new();
        m.push_func(f);
        m
    }

    #[test]
    fn emit_scf_if_pick_succeeds() {
        let module = build_pick_i32_module();
        let bytes = emit_object_module(&module).expect("emit ok");
        assert!(!bytes.is_empty());
    }

    #[test]
    fn emit_scf_if_pick_starts_with_host_magic() {
        let module = build_pick_i32_module();
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
    fn emit_heap_dealloc_imports_cssl_free_symbol() {
        // fn dealloc_call(ptr : !cssl.ptr, size : i64, align : i64) -> ()
        // emits `cssl.heap.dealloc` then a void return.
        let mut f = MirFunc::new(
            "dealloc_call",
            vec![
                MirType::Ptr,
                MirType::Int(IntWidth::I64),
                MirType::Int(IntWidth::I64),
            ],
            vec![],
        );
        f.push_op(
            MirOp::std("cssl.heap.dealloc")
                .with_operand(ValueId(0))
                .with_operand(ValueId(1))
                .with_operand(ValueId(2)),
        );
        f.push_op(MirOp::std("func.return"));
        let mut module = MirModule::new();
        module.push_func(f);
        let bytes = emit_object_module(&module).expect("emit heap.dealloc ok");
        let host_magic = magic_prefix(host_default_format());
        assert!(bytes.starts_with(host_magic));
    }

    #[test]
    fn emit_heap_realloc_imports_cssl_realloc_symbol() {
        // fn realloc_call(ptr, old, new, align) -> !cssl.ptr — exercises
        // the 4-operand → 1-result shape unique to realloc.
        let mut f = MirFunc::new(
            "realloc_call",
            vec![
                MirType::Ptr,
                MirType::Int(IntWidth::I64),
                MirType::Int(IntWidth::I64),
                MirType::Int(IntWidth::I64),
            ],
            vec![MirType::Ptr],
        );
        f.push_op(
            MirOp::std("cssl.heap.realloc")
                .with_operand(ValueId(0))
                .with_operand(ValueId(1))
                .with_operand(ValueId(2))
                .with_operand(ValueId(3))
                .with_result(ValueId(4), MirType::Ptr),
        );
        f.push_op(MirOp::std("func.return").with_operand(ValueId(4)));
        let mut module = MirModule::new();
        module.push_func(f);
        let bytes = emit_object_module(&module).expect("emit heap.realloc ok");
        let host_magic = magic_prefix(host_default_format());
        assert!(bytes.starts_with(host_magic));
    }

    #[test]
    fn emit_heap_alloc_with_constant_operands_succeeds() {
        // fn alloc_blob() -> !cssl.ptr {
        //   let sz = arith.constant 64 : i64
        //   let al = arith.constant  8 : i64
        //   cssl.heap.alloc(sz, al) -> ptr
        //   return ptr
        // }
        // Mirrors the body_lower Box::new() shape end-to-end.
        let mut f = MirFunc::new("alloc_blob", vec![], vec![MirType::Ptr]);
        f.push_op(
            MirOp::std("arith.constant")
                .with_attribute("value", "64")
                .with_result(ValueId(0), MirType::Int(IntWidth::I64)),
        );
        f.push_op(
            MirOp::std("arith.constant")
                .with_attribute("value", "8")
                .with_result(ValueId(1), MirType::Int(IntWidth::I64)),
        );
        f.push_op(
            MirOp::std("cssl.heap.alloc")
                .with_operand(ValueId(0))
                .with_operand(ValueId(1))
                .with_result(ValueId(2), MirType::Ptr),
        );
        f.push_op(MirOp::std("func.return").with_operand(ValueId(2)));
        let mut module = MirModule::new();
        module.push_func(f);
        let bytes = emit_object_module(&module).expect("emit alloc_blob ok");
        let host_magic = magic_prefix(host_default_format());
        assert!(bytes.starts_with(host_magic));
        assert!(!bytes.is_empty());
    }

    #[test]
    fn emit_scf_if_with_branch_arith_succeeds() {
        // fn body_with_arith(cond: bool, a: i32) -> i32 {
        //   let one = 1; if cond { a + one } else { a - one }
        // }
        use cssl_mir::{MirBlock, MirRegion};
        let mut f = MirFunc::new(
            "branch_arith",
            vec![MirType::Bool, MirType::Int(IntWidth::I32)],
            vec![MirType::Int(IntWidth::I32)],
        );
        f.next_value_id = 6;
        {
            let entry = f.body.entry_mut().unwrap();
            entry.ops.push(
                MirOp::std("arith.constant")
                    .with_result(ValueId(2), MirType::Int(IntWidth::I32))
                    .with_attribute("value", "1"),
            );
            let mut then_blk = MirBlock::new("entry");
            then_blk.ops.push(
                MirOp::std("arith.addi")
                    .with_operand(ValueId(1))
                    .with_operand(ValueId(2))
                    .with_result(ValueId(3), MirType::Int(IntWidth::I32)),
            );
            then_blk
                .ops
                .push(MirOp::std("scf.yield").with_operand(ValueId(3)));
            let mut then_region = MirRegion::new();
            then_region.push(then_blk);
            let mut else_blk = MirBlock::new("entry");
            else_blk.ops.push(
                MirOp::std("arith.subi")
                    .with_operand(ValueId(1))
                    .with_operand(ValueId(2))
                    .with_result(ValueId(4), MirType::Int(IntWidth::I32)),
            );
            else_blk
                .ops
                .push(MirOp::std("scf.yield").with_operand(ValueId(4)));
            let mut else_region = MirRegion::new();
            else_region.push(else_blk);
            entry.ops.push(
                MirOp::std("scf.if")
                    .with_operand(ValueId(0))
                    .with_region(then_region)
                    .with_region(else_region)
                    .with_result(ValueId(5), MirType::Int(IntWidth::I32)),
            );
            entry
                .ops
                .push(MirOp::std("func.return").with_operand(ValueId(5)));
        }
        let mut module = MirModule::new();
        module.push_func(f);
        let bytes = emit_object_module(&module).expect("emit ok");
        let host_magic = magic_prefix(host_default_format());
        assert!(bytes.starts_with(host_magic));
    }

    #[test]
    fn emit_scf_if_statement_form_succeeds() {
        // fn stmt_if(cond: bool, a: i32) -> i32 { if cond { 0 } a }
        use cssl_mir::{MirBlock, MirRegion};
        let mut f = MirFunc::new(
            "stmt_if",
            vec![MirType::Bool, MirType::Int(IntWidth::I32)],
            vec![MirType::Int(IntWidth::I32)],
        );
        f.next_value_id = 4;
        {
            let entry = f.body.entry_mut().unwrap();
            let mut then_blk = MirBlock::new("entry");
            then_blk.ops.push(
                MirOp::std("arith.constant")
                    .with_result(ValueId(2), MirType::Int(IntWidth::I32))
                    .with_attribute("value", "0"),
            );
            let mut then_region = MirRegion::new();
            then_region.push(then_blk);
            let else_region = MirRegion::new();
            entry.ops.push(
                MirOp::std("scf.if")
                    .with_operand(ValueId(0))
                    .with_region(then_region)
                    .with_region(else_region)
                    .with_result(ValueId(3), MirType::None),
            );
            entry
                .ops
                .push(MirOp::std("func.return").with_operand(ValueId(1)));
        }
        let mut module = MirModule::new();
        module.push_func(f);
        let bytes = emit_object_module(&module).expect("emit ok");
        let host_magic = magic_prefix(host_default_format());
        assert!(bytes.starts_with(host_magic));
    }

    #[test]
    fn emit_scf_if_with_one_region_returns_error() {
        use cssl_mir::{MirBlock, MirRegion};
        let mut f = MirFunc::new(
            "bad",
            vec![MirType::Bool, MirType::Int(IntWidth::I32)],
            vec![MirType::Int(IntWidth::I32)],
        );
        f.next_value_id = 3;
        {
            let entry = f.body.entry_mut().unwrap();
            let mut then_blk = MirBlock::new("entry");
            then_blk
                .ops
                .push(MirOp::std("scf.yield").with_operand(ValueId(1)));
            let mut then_region = MirRegion::new();
            then_region.push(then_blk);
            entry.ops.push(
                MirOp::std("scf.if")
                    .with_operand(ValueId(0))
                    .with_region(then_region)
                    .with_result(ValueId(2), MirType::Int(IntWidth::I32)),
            );
            entry
                .ops
                .push(MirOp::std("func.return").with_operand(ValueId(1)));
        }
        let mut module = MirModule::new();
        module.push_func(f);
        let r = emit_object_module(&module);
        assert!(
            matches!(r, Err(ObjectError::LoweringFailed { ref detail, .. }) if detail.contains("scf.if")),
            "unexpected result : {r:?}"
        );
    }

    // ─────────────────────────────────────────────────────────────────────
    // § T11-D59 (S6-C3) : memref.load + memref.store object-emit tests.
    //
    // These tests confirm the object backend produces non-empty bytes with
    // the host-magic prefix when the input MIR contains memref ops. End-to-
    // end runtime verification of the produced object lives in the JIT
    // module ; here we verify the codegen path doesn't reject the ops or
    // panic.
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

    // ─────────────────────────────────────────────────────────────────
    // § T11-D61 (S6-C2) — scf.loop / scf.while / scf.for object-emit
    // ─────────────────────────────────────────────────────────────────
    //
    // These tests verify the object backend accepts loop ops + produces
    // a valid object file with the host-platform magic. Roundtrip-
    // runtime tests live in jit.rs (which can actually execute) ; here
    // we verify byte-shape invariants + structural-error propagation.

    /// `fn loop_ret(x : i32) -> i32 { loop { return x } }`
    #[test]
    fn obj_emit_scf_loop_with_inner_return_succeeds() {
        use cssl_mir::{MirBlock, MirRegion};
        let mut f = MirFunc::new(
            "loop_ret",
            vec![MirType::Int(IntWidth::I32)],
            vec![MirType::Int(IntWidth::I32)],
        );
        f.next_value_id = 1;
        // Body : `func.return v0`
        let mut body_blk = MirBlock::new("entry");
        body_blk
            .ops
            .push(MirOp::std("func.return").with_operand(ValueId(0)));
        let mut body_region = MirRegion::new();
        body_region.push(body_blk);
        f.push_op(
            MirOp::std("scf.loop")
                .with_region(body_region)
                .with_result(ValueId(0), MirType::None),
        );
        f.push_op(MirOp::std("func.return").with_operand(ValueId(0)));
        let mut module = MirModule::new();
        module.push_func(f);
        let bytes = emit_object_module(&module).expect("emit ok");
        assert!(bytes.starts_with(magic_prefix(host_default_format())));
    }

    /// `fn while_skip(c : bool, x : i32) -> i32 { while c { return 99 } x }`
    #[test]
    fn obj_emit_scf_while_with_branching_succeeds() {
        use cssl_mir::{MirBlock, MirRegion};
        let mut f = MirFunc::new(
            "while_skip",
            vec![MirType::Bool, MirType::Int(IntWidth::I32)],
            vec![MirType::Int(IntWidth::I32)],
        );
        f.next_value_id = 3;
        let mut body_blk = MirBlock::new("entry");
        body_blk.ops.push(
            MirOp::std("arith.constant")
                .with_result(ValueId(2), MirType::Int(IntWidth::I32))
                .with_attribute("value", "99"),
        );
        body_blk
            .ops
            .push(MirOp::std("func.return").with_operand(ValueId(2)));
        let mut body_region = MirRegion::new();
        body_region.push(body_blk);
        f.push_op(
            MirOp::std("scf.while")
                .with_operand(ValueId(0))
                .with_region(body_region)
                .with_result(ValueId(0), MirType::None),
        );
        f.push_op(MirOp::std("func.return").with_operand(ValueId(1)));
        let mut module = MirModule::new();
        module.push_func(f);
        let bytes = emit_object_module(&module).expect("emit ok");
        assert!(bytes.starts_with(magic_prefix(host_default_format())));
    }

    /// `fn for_passthrough(iter : i64, x : i32) -> i32 { for _ in iter { } x }`
    #[test]
    fn obj_emit_scf_for_passthrough_succeeds() {
        use cssl_mir::{MirBlock, MirRegion};
        let mut f = MirFunc::new(
            "for_passthrough",
            vec![MirType::Int(IntWidth::I64), MirType::Int(IntWidth::I32)],
            vec![MirType::Int(IntWidth::I32)],
        );
        f.next_value_id = 2;
        let body_blk = MirBlock::new("entry");
        let mut body_region = MirRegion::new();
        body_region.push(body_blk);
        f.push_op(
            MirOp::std("scf.for")
                .with_operand(ValueId(0))
                .with_region(body_region)
                .with_result(ValueId(0), MirType::None),
        );
        f.push_op(MirOp::std("func.return").with_operand(ValueId(1)));
        let mut module = MirModule::new();
        module.push_func(f);
        let bytes = emit_object_module(&module).expect("emit ok");
        assert!(bytes.starts_with(magic_prefix(host_default_format())));
    }

    /// scf.loop with TWO regions → WrongLoopRegionCount → LoweringFailed.
    #[test]
    fn obj_emit_scf_loop_with_two_regions_errors() {
        use cssl_mir::{MirBlock, MirRegion};
        let mut f = MirFunc::new(
            "bad_loop",
            vec![MirType::Int(IntWidth::I32)],
            vec![MirType::Int(IntWidth::I32)],
        );
        f.next_value_id = 1;
        let mut r1_blk = MirBlock::new("entry");
        r1_blk
            .ops
            .push(MirOp::std("func.return").with_operand(ValueId(0)));
        let mut r1 = MirRegion::new();
        r1.push(r1_blk);
        let r2 = MirRegion::new();
        f.push_op(
            MirOp::std("scf.loop")
                .with_region(r1)
                .with_region(r2)
                .with_result(ValueId(0), MirType::None),
        );
        f.push_op(MirOp::std("func.return").with_operand(ValueId(0)));
        let mut module = MirModule::new();
        module.push_func(f);
        let r = emit_object_module(&module);
        assert!(
            matches!(r, Err(ObjectError::LoweringFailed { ref detail, .. }) if detail.contains("scf.loop")),
            "unexpected result : {r:?}"
        );
    }

    // ─────────────────────────────────────────────────────────────────────
    // § T11-D77 (S6-C5 redo) — Object-emit closure dispatch.
    //
    // Stage-0 contract : a fn whose body contains a `cssl.closure` op with
    // capture_count=0 (no env-pack) must object-emit cleanly. A fn that
    // ALSO contains the env-pack sequence (alloc + memref.store + closure)
    // must also emit cleanly because the heap-alloc machinery is already
    // wired up via S6-B1 (T11-D57).
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn obj_emit_zero_capture_closure_succeeds() {
        // Build `fn make_clos() { cssl.closure() ; func.return }`. The
        // closure op binds its result to a typed-zero ptr sentinel ; no
        // env-pack ops are emitted (capture_count=0).
        use cssl_mir::{MirBlock, MirRegion};
        let mut f = MirFunc::new("make_clos", vec![], vec![]);
        let mut body_region = MirRegion::new();
        body_region.push(MirBlock::new("entry"));
        f.push_op(
            MirOp::std("cssl.closure")
                .with_result(ValueId(0), MirType::Opaque("!cssl.closure".into()))
                .with_region(body_region)
                .with_attribute("param_count", "0")
                .with_attribute("capture_count", "0")
                .with_attribute("env_size", "0")
                .with_attribute("env_align", "8")
                .with_attribute("cap_value", "val"),
        );
        f.push_op(MirOp::std("func.return"));
        let mut module = MirModule::new();
        module.push_func(f);
        let bytes = emit_object_module(&module).expect("emit ok");
        assert!(bytes.starts_with(magic_prefix(host_default_format())));
    }

    #[test]
    fn obj_emit_closure_with_one_capture_pulls_in_heap_alloc() {
        // Build the full env-pack sequence body_lower emits when there's one
        // capture : arith.constant + arith.constant + cssl.heap.alloc +
        // arith.constant (offset) + memref.store + cssl.closure.
        //
        // fn capture_one(y : i64) -> () {
        //   %sz = arith.constant 8 : i64
        //   %al = arith.constant 8 : i64
        //   %env = cssl.heap.alloc(%sz, %al) -> !cssl.ptr
        //   %off = arith.constant 0 : i64
        //   memref.store y, %env, %off
        //   %clos = cssl.closure(%y, %env) -> !cssl.closure
        //         { capture_count=1, env_size=8, env_align=8, cap_value="val" }
        //   func.return
        // }
        use cssl_mir::{MirBlock, MirRegion};
        let mut f = MirFunc::new("capture_one", vec![MirType::Int(IntWidth::I64)], vec![]);
        // entry-arg : y at ValueId(0).
        f.push_op(
            MirOp::std("arith.constant")
                .with_result(ValueId(1), MirType::Int(IntWidth::I64))
                .with_attribute("value", "8"),
        );
        f.push_op(
            MirOp::std("arith.constant")
                .with_result(ValueId(2), MirType::Int(IntWidth::I64))
                .with_attribute("value", "8"),
        );
        f.push_op(
            MirOp::std("cssl.heap.alloc")
                .with_operand(ValueId(1))
                .with_operand(ValueId(2))
                .with_result(ValueId(3), MirType::Ptr)
                .with_attribute("cap", "iso")
                .with_attribute("origin", "closure_env"),
        );
        f.push_op(
            MirOp::std("arith.constant")
                .with_result(ValueId(4), MirType::Int(IntWidth::I64))
                .with_attribute("value", "0"),
        );
        f.push_op(
            MirOp::std("memref.store")
                .with_operand(ValueId(0))
                .with_operand(ValueId(3))
                .with_operand(ValueId(4))
                .with_attribute("alignment", "8"),
        );
        // cssl.closure with operands : [capture_0=v0(y), env_ptr=v3].
        let mut body_region = MirRegion::new();
        body_region.push(MirBlock::new("entry"));
        f.push_op(
            MirOp::std("cssl.closure")
                .with_operand(ValueId(0))
                .with_operand(ValueId(3))
                .with_result(ValueId(5), MirType::Opaque("!cssl.closure".into()))
                .with_region(body_region)
                .with_attribute("param_count", "1")
                .with_attribute("capture_count", "1")
                .with_attribute("env_size", "8")
                .with_attribute("env_align", "8")
                .with_attribute("cap_value", "val")
                .with_attribute("capture_names", "y"),
        );
        f.push_op(MirOp::std("func.return"));
        let mut module = MirModule::new();
        module.push_func(f);
        let bytes = emit_object_module(&module).expect("emit ok with capture");
        assert!(bytes.starts_with(magic_prefix(host_default_format())));
    }

    #[test]
    fn obj_closure_capture_mismatch_errors_cleanly() {
        // capture_count=2 in attributes but only 1 operand provided ⇒ env-ptr
        // operand index = 2 but operands.len() = 1 ⇒ LoweringFailed with an
        // actionable detail message.
        use cssl_mir::{MirBlock, MirRegion};
        let mut f = MirFunc::new("bad_closure", vec![MirType::Int(IntWidth::I64)], vec![]);
        let mut body_region = MirRegion::new();
        body_region.push(MirBlock::new("entry"));
        f.push_op(
            MirOp::std("cssl.closure")
                .with_operand(ValueId(0))
                .with_result(ValueId(1), MirType::Opaque("!cssl.closure".into()))
                .with_region(body_region)
                .with_attribute("param_count", "0")
                .with_attribute("capture_count", "2"),
        );
        f.push_op(MirOp::std("func.return"));
        let mut module = MirModule::new();
        module.push_func(f);
        let r = emit_object_module(&module);
        assert!(matches!(r, Err(ObjectError::LoweringFailed { .. })));
    }
}
