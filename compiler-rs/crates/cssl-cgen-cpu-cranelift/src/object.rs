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

use std::collections::{BTreeMap, HashMap};

use cranelift_codegen::ir::{
    types as cl_types, AbiParam, Block as ClBlock, InstBuilder, Signature, UserFuncName,
};
use cranelift_codegen::settings::Configurable as _;
use cranelift_codegen::{settings, Context};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{FuncId, Linkage, Module};
use cranelift_object::{ObjectBuilder, ObjectModule};
use cssl_mir::{
    FloatWidth, IntWidth, MirFunc, MirModule, MirOp, MirStructLayout, MirType, StructAbiClass,
    ValueId,
};
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

    /// Multi-block bodies are now SUPPORTED at stage-0 (T11-CC-1, W-CC-multiblock).
    /// Variant retained for diagnostic compatibility — it can fire when a
    /// MIR-block carries an unrecognized terminator-shape (operand counts that
    /// don't match `cssl.branch` / `cssl.brif` attribute counts, etc.).
    #[error(
        "fn `{fn_name}` multi-block body lowering failed : {detail}"
    )]
    MultiBlockBody { fn_name: String, detail: String },

    /// A `MirFunc` referenced an unknown ValueId.
    #[error("fn `{fn_name}` references unknown ValueId({value_id})")]
    UnknownValueId { fn_name: String, value_id: u32 },

    /// A `cssl.branch` / `cssl.brif` referenced a block index that doesn't
    /// exist in the parent fn's block-list.
    #[error(
        "fn `{fn_name}` branch-target block#{target_idx} out of range ({block_count} blocks)"
    )]
    BlockTargetOutOfRange {
        fn_name: String,
        target_idx: usize,
        block_count: usize,
    },

    /// A non-entry block contained a non-terminator at the LAST position.
    /// The MIR contract for multi-block bodies requires every block to end with
    /// `func.return` / `cssl.branch` / `cssl.brif`.
    #[error("fn `{fn_name}` block#{block_idx} (`{label}`) is missing a terminator")]
    BlockMissingTerminator {
        fn_name: String,
        block_idx: usize,
        label: String,
    },
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

    // § T11-CC-2 (W-CC-funccall) — pre-declare every non-generic, defined fn
    //   in the module so per-fn body lowering can resolve `func.call` callees
    //   that target a sibling fn declared LATER in source order. This mirrors
    //   `jit::JitModule.fn_table` : signature-only / extern fns stay un-pre-
    //   declared here ; they get a `Linkage::Import` declaration on first use
    //   inside `declare_callee_imports_for_fn`. The `pre_decl` map carries
    //   the cranelift `FuncId` keyed by source-form fn name for both pass-2
    //   reuse (when defining the body of fn `X`) AND for callsite resolution
    //   (when fn `Y` issues a `func.call @X`).
    let ptr_ty_for_decl = obj_module.isa().pointer_type();
    let mut fn_table: HashMap<String, FuncId> = HashMap::new();
    // T11-W17-A · stage-0 struct-FFI codegen — the layout-table threads
    // through every signature-emitting helper. `None` would degrade back
    // to the legacy scalar-only path ; `Some(&module.struct_layouts)`
    // unlocks newtype/POD struct FFI signatures.
    let struct_layouts: Option<&BTreeMap<String, MirStructLayout>> =
        Some(&module.struct_layouts);
    for mir_fn in &module.funcs {
        if mir_fn.is_generic {
            continue;
        }
        // § Extern declaration : a single empty entry block + no other blocks
        // is the canonical "extern fn" shape (interface-method / FFI decl).
        // Multi-block all-empty bodies are MALFORMED MIR — fall through so
        // pass-2's `compile_one_fn` can surface `MultiBlockBody`.
        if mir_fn.body.blocks.len() <= 1 && mir_fn.is_signature_only() {
            continue;
        }
        let func_id =
            declare_fn_signature(&mut obj_module, mir_fn, ptr_ty_for_decl, struct_layouts)?;
        fn_table.insert(mir_fn.name.clone(), func_id);
    }

    for mir_fn in &module.funcs {
        if mir_fn.is_generic {
            continue; // skip unspecialized generic fns
        }
        if mir_fn.body.blocks.len() <= 1 && mir_fn.is_signature_only() {
            continue; // extern decl ; no body to define ; resolved as import on use.
        }
        let func_id = *fn_table
            .get(&mir_fn.name)
            .ok_or_else(|| ObjectError::LoweringFailed {
                fn_name: mir_fn.name.clone(),
                detail: "pre-declared FuncId missing (pass-1 bug)".to_string(),
            })?;
        compile_one_fn(
            &mut obj_module,
            &mut builder_ctx,
            &mut codegen_ctx,
            mir_fn,
            func_id,
            &fn_table,
            struct_layouts,
        )?;
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

/// Build a cranelift `Signature` from the MIR fn's params + results.
///
/// Shared by pass-1 (pre-declare every non-generic, non-signature-only fn for
/// callsite-resolution) + pass-2 (emit body) + the callee-import pre-scan
/// (when an extern callee MUST be declared as `Linkage::Import`). Stage-0
/// scalar-only ; non-scalar slot types surface as [`ObjectError::NonScalarType`].
fn build_clif_signature(
    isa_call_conv: cranelift_codegen::isa::CallConv,
    fn_name: &str,
    params: &[MirType],
    results: &[MirType],
    ptr_ty: cranelift_codegen::ir::Type,
    struct_layouts: Option<&BTreeMap<String, MirStructLayout>>,
) -> Result<Signature, ObjectError> {
    let mut sig = Signature::new(isa_call_conv);
    for (idx, p_ty) in params.iter().enumerate() {
        let cl_ty = mir_type_to_cl_with_layouts(p_ty, ptr_ty, struct_layouts).ok_or_else(
            || ObjectError::NonScalarType {
                fn_name: fn_name.to_string(),
                slot: idx,
                ty: format!("{p_ty}"),
            },
        )?;
        sig.params.push(AbiParam::new(cl_ty));
    }
    for (idx, r_ty) in results.iter().enumerate() {
        let cl_ty = mir_type_to_cl_with_layouts(r_ty, ptr_ty, struct_layouts).ok_or_else(
            || ObjectError::NonScalarType {
                fn_name: fn_name.to_string(),
                slot: idx,
                ty: format!("{r_ty}"),
            },
        )?;
        sig.returns.push(AbiParam::new(cl_ty));
    }
    Ok(sig)
}

/// § T11-CC-2 (W-CC-funccall) pass-1 helper — declare a `MirFunc`'s symbol
/// against the object module without emitting a body. The returned `FuncId`
/// is stored in the per-module `fn_table` for both pass-2 body emission +
/// for sibling-fn `func.call` resolution. Linkage = `Export` so cross-TU
/// linking still works (matches the previous single-pass behavior).
fn declare_fn_signature(
    obj_module: &mut ObjectModule,
    mir_fn: &MirFunc,
    ptr_ty: cranelift_codegen::ir::Type,
    struct_layouts: Option<&BTreeMap<String, MirStructLayout>>,
) -> Result<FuncId, ObjectError> {
    let call_conv = obj_module.isa().default_call_conv();
    let sig = build_clif_signature(
        call_conv,
        &mir_fn.name,
        &mir_fn.params,
        &mir_fn.results,
        ptr_ty,
        struct_layouts,
    )?;
    obj_module
        .declare_function(&mir_fn.name, Linkage::Export, &sig)
        .map_err(|e| ObjectError::LoweringFailed {
            fn_name: mir_fn.name.clone(),
            detail: format!("declare_function : {e}"),
        })
}

fn compile_one_fn(
    obj_module: &mut ObjectModule,
    builder_ctx: &mut FunctionBuilderContext,
    codegen_ctx: &mut Context,
    mir_fn: &MirFunc,
    func_id: FuncId,
    fn_table: &HashMap<String, FuncId>,
    struct_layouts: Option<&BTreeMap<String, MirStructLayout>>,
) -> Result<(), ObjectError> {
    // § T11-CC-1 (W-CC-multiblock) — multi-block bodies are now supported.
    //   Each MIR-block in `mir_fn.body.blocks` maps 1:1 to a cranelift
    //   `Block`. The entry block (idx 0) is created via `create_block` +
    //   `append_block_params_for_function_params` (params come from the
    //   fn signature). Non-entry blocks are created via `create_block` +
    //   `append_block_params_for_block_signature` from each MIR block's
    //   `args` (the block-param SSA values).
    //
    //   Terminators recognized at the multi-block level :
    //     `func.return ARGS...`
    //     `cssl.branch  TARGET_BLK [ARG...]`            attr `target=N`
    //     `cssl.brif    COND, THEN_BLK, ELSE_BLK [...]` attrs `then_target=`,
    //         `else_target=`, `then_arg_count=`, `else_arg_count=` ;
    //         operands `[cond, then_args..., else_args...]`.
    //
    //   See `lower_one_op` for the per-op dispatch — the new arms `cssl.branch`
    //   / `cssl.brif` resolve their targets against the `block_map` slice
    //   indexed by MIR block-index.

    // § 1. Build cranelift signature.
    let call_conv = obj_module.isa().default_call_conv();
    // Stage-0 single-host : the active ISA's pointer type is what `__cssl_alloc`
    // and friends operate on. Cache once for both signature emission and the
    // per-op lowering loop below.
    let ptr_ty = obj_module.isa().pointer_type();
    let sig = build_clif_signature(
        call_conv,
        &mir_fn.name,
        &mir_fn.params,
        &mir_fn.results,
        ptr_ty,
        struct_layouts,
    )?;

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

    // § T11-CC-2 (W-CC-funccall) — pre-scan body ops for `func.call` references.
    //   For each unique callee referenced in this fn's body :
    //     - If `fn_table` has the callee → resolve it to the pre-declared
    //       `FuncId` (intra-module, sibling fn defined locally).
    //     - Otherwise → declare it as `Linkage::Import` using a signature
    //       derived from the callsite operand types (looked up via the per-
    //       fn value-type map) + result types (carried directly on the op).
    //   Each callee declared once per fn-body ; multiple `func.call` ops
    //   targeting the same callee share a single `FuncRef`. Mirrors the
    //   JIT-side pre-scan at `jit.rs` (~line 715) but uses an op-name-keyed
    //   import map similar to `HeapImports` — no `#width` keying needed
    //   because user-defined callees are width-monomorphic at this point.
    //   The walk descends 1 level into nested-region ops (scf.if/loop/while/for)
    //   so func.call ops inside structured-CFG regions still get their callees
    //   declared up front.
    let callee_refs = declare_callee_imports_for_fn(
        obj_module,
        codegen_ctx,
        mir_fn,
        fn_table,
        ptr_ty,
        struct_layouts,
    )?;

    // § T11-W18-CSSLC-ADVANCE2 — pre-scan body for `arith.remf` ; declare
    //   libm `fmodf` / `fmod` once per fn for the widths actually present.
    //   See [`declare_fmod_imports_for_fn`] for rationale (cranelift has no
    //   `frem` instruction so float-remainder MUST go through libm).
    let fmod_refs = declare_fmod_imports_for_fn(obj_module, codegen_ctx, mir_fn)?;

    // § 2. Build body — multi-block aware (§ T11-CC-1).
    {
        let mut builder = FunctionBuilder::new(&mut codegen_ctx.func, builder_ctx);

        let mut value_map: HashMap<ValueId, cranelift_codegen::ir::Value> = HashMap::new();

        let mir_blocks = &mir_fn.body.blocks;
        if mir_blocks.is_empty() {
            // Empty body → return-void (only valid if results empty).
            let entry = builder.create_block();
            builder.append_block_params_for_function_params(entry);
            builder.switch_to_block(entry);
            builder.seal_block(entry);
            if mir_fn.results.is_empty() {
                builder.ins().return_(&[]);
            } else {
                return Err(ObjectError::LoweringFailed {
                    fn_name: mir_fn.name.clone(),
                    detail: "empty body but non-empty results".to_string(),
                });
            }
            builder.finalize();
        } else {
            // § 2a. PRE-DECLARE one cranelift Block per MIR block.
            //   Entry block carries the fn-signature params ; non-entry blocks
            //   carry block-args derived from `MirBlock.args`.
            let mut block_map: Vec<ClBlock> = Vec::with_capacity(mir_blocks.len());
            for (idx, mir_block) in mir_blocks.iter().enumerate() {
                let cl_blk = builder.create_block();
                if idx == 0 {
                    builder.append_block_params_for_function_params(cl_blk);
                } else {
                    for arg_meta in &mir_block.args {
                        let cl_ty = mir_type_to_cl(&arg_meta.ty, ptr_ty).ok_or_else(|| {
                            ObjectError::NonScalarType {
                                fn_name: mir_fn.name.clone(),
                                slot: 0,
                                ty: format!("{}", arg_meta.ty),
                            }
                        })?;
                        builder.append_block_param(cl_blk, cl_ty);
                    }
                }
                block_map.push(cl_blk);
            }

            // § 2b. Walk each MIR block in order. Switch into its cranelift
            //   block, bind the block's args to its cranelift block-params,
            //   then lower ops. The LAST op should be the terminator —
            //   `func.return` / `cssl.branch` / `cssl.brif`. If none present,
            //   we error out for non-entry blocks ; the entry block keeps the
            //   single-block fallback (implicit-return for void fns) for
            //   backwards compat with the existing test suite.
            for (idx, mir_block) in mir_blocks.iter().enumerate() {
                let cl_blk = block_map[idx];
                builder.switch_to_block(cl_blk);

                // Bind block-params to ValueIds. For the entry block the
                // params come from the fn signature ; for non-entry blocks
                // they come from `MirBlock.args`. Either way the `MirBlock`
                // carries the canonical receiver-id list, so the binding
                // is the same.
                let block_params: Vec<_> = builder.block_params(cl_blk).to_vec();
                for (arg_meta, &bp) in mir_block.args.iter().zip(block_params.iter()) {
                    value_map.insert(arg_meta.id, bp);
                }

                let mut terminated = false;
                for op in &mir_block.ops {
                    if terminated {
                        break;
                    }
                    terminated = lower_one_op(
                        op,
                        &mut builder,
                        &mut value_map,
                        &mir_fn.name,
                        &heap_refs,
                        &callee_refs,
                        &fmod_refs,
                        ptr_ty,
                        &block_map,
                    )?;
                }

                if !terminated {
                    if mir_blocks.len() == 1 {
                        // Single-block fn : preserve existing semantics —
                        // implicit-return for void fns.
                        if mir_fn.results.is_empty() {
                            builder.ins().return_(&[]);
                        } else {
                            return Err(ObjectError::LoweringFailed {
                                fn_name: mir_fn.name.clone(),
                                detail: "fn body is missing a `func.return` terminator"
                                    .to_string(),
                            });
                        }
                    } else {
                        return Err(ObjectError::BlockMissingTerminator {
                            fn_name: mir_fn.name.clone(),
                            block_idx: idx,
                            label: mir_block.label.clone(),
                        });
                    }
                }
            }

            // § 2c. Seal all blocks. Cranelift's `seal_all_blocks` walks the
            //   block-list once and seals each ; back-edges (loop body →
            //   loop header) are handled because the back-edge is already
            //   emitted by the time we call this. Doing it after the walk
            //   keeps loop SSA construction sound.
            builder.seal_all_blocks();
            builder.finalize();
        }
    }

    // § 3. Define the function in the object module.
    obj_module
        .define_function(func_id, codegen_ctx)
        .map_err(|e| ObjectError::LoweringFailed {
            fn_name: mir_fn.name.clone(),
            detail: format!("define_function : {e:?}"),
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
// § T11-W18-CSSLC-ADVANCE2 — `arith.remf` libm import (fmodf / fmod).
//
//   Cranelift has no `frem` instruction ; the only IEEE-754-compliant float
//   remainder available on host CPUs is the libm `fmodf(f32, f32) -> f32`
//   and `fmod(f64, f64) -> f64`. We pre-scan each fn body for `arith.remf`
//   ops, and for each width seen in result-types we declare the matching
//   libm symbol as `Linkage::Import`, then bind a per-fn `FuncRef`.
//   Body lowering routes `arith.remf` through [`emit_fmod_call`] which
//   resolves the right `FuncRef` based on the op's result-width.
//
//   Symbol contract :
//     - `fmodf`  : standard libm symbol on glibc / musl / Apple libSystem /
//                  Microsoft UCRT (linked by S6-A4 linker for MSVC builds).
//     - `fmod`   : same, double-precision.
//   Both are `extern "C"` and ABI-passed in xmm-registers on x86_64 SSE2 ;
//   no shim layer is required.
//
//   § DEFERRED
//     IEEE-754-2008's `remainder` (rem-near, vs `fmod`'s rem-trunc) is NOT
//     wired ; CSSLv3 surface `%` on float follows C / Rust semantics which
//     is `fmod` (rem-trunc, sign-of-LHS). If a future `cssl.math.remainder`
//     op surfaces the strict-IEEE form it can declare `remainderf`/`remainder`
//     via the same shape.
// ───────────────────────────────────────────────────────────────────────

const FMOD_F32_SYMBOL: &str = "fmodf";
const FMOD_F64_SYMBOL: &str = "fmod";

/// Per-fn map of `(width-tag → cranelift FuncRef)` for the libm `fmod*`
/// import declared on demand. Mirrors [`HeapImports`].
#[derive(Default)]
struct FmodImports {
    /// f32 entry — declared when the body contains an `arith.remf` whose
    /// result is `MirType::Float(F32)`.
    f32_ref: Option<cranelift_codegen::ir::FuncRef>,
    /// f64 entry — same but F64.
    f64_ref: Option<cranelift_codegen::ir::FuncRef>,
}

impl FmodImports {
    fn for_width(&self, w: FloatWidth) -> Option<cranelift_codegen::ir::FuncRef> {
        match w {
            FloatWidth::F32 | FloatWidth::F16 | FloatWidth::Bf16 => self.f32_ref,
            FloatWidth::F64 => self.f64_ref,
        }
    }
}

/// Walk this fn's body (entry block + 1-level nested regions, mirroring the
/// callee-import pre-scan) ; for each `arith.remf` op, record which libm
/// width-symbol is needed, then declare each on the cranelift module.
fn declare_fmod_imports_for_fn(
    obj_module: &mut ObjectModule,
    codegen_ctx: &mut Context,
    mir_fn: &MirFunc,
) -> Result<FmodImports, ObjectError> {
    let mut imports = FmodImports::default();
    let Some(entry_block) = mir_fn.body.blocks.first() else {
        return Ok(imports);
    };
    let mut needs_f32 = false;
    let mut needs_f64 = false;
    fn walk(ops: &[MirOp], needs_f32: &mut bool, needs_f64: &mut bool) {
        for op in ops {
            if op.name == "arith.remf" {
                if let Some(r) = op.results.first() {
                    if let MirType::Float(w) = r.ty {
                        match w {
                            FloatWidth::F64 => *needs_f64 = true,
                            FloatWidth::F32 | FloatWidth::F16 | FloatWidth::Bf16 => *needs_f32 = true,
                        }
                    }
                }
            }
            for region in &op.regions {
                for blk in &region.blocks {
                    walk(&blk.ops, needs_f32, needs_f64);
                }
            }
        }
    }
    walk(&entry_block.ops, &mut needs_f32, &mut needs_f64);

    let call_conv = obj_module.isa().default_call_conv();

    if needs_f32 {
        let mut sig = Signature::new(call_conv);
        sig.params.push(AbiParam::new(cl_types::F32));
        sig.params.push(AbiParam::new(cl_types::F32));
        sig.returns.push(AbiParam::new(cl_types::F32));
        let id = obj_module
            .declare_function(FMOD_F32_SYMBOL, Linkage::Import, &sig)
            .map_err(|e| ObjectError::LoweringFailed {
                fn_name: mir_fn.name.clone(),
                detail: format!("declare {FMOD_F32_SYMBOL} : {e}"),
            })?;
        let fref = obj_module.declare_func_in_func(id, &mut codegen_ctx.func);
        imports.f32_ref = Some(fref);
    }
    if needs_f64 {
        let mut sig = Signature::new(call_conv);
        sig.params.push(AbiParam::new(cl_types::F64));
        sig.params.push(AbiParam::new(cl_types::F64));
        sig.returns.push(AbiParam::new(cl_types::F64));
        let id = obj_module
            .declare_function(FMOD_F64_SYMBOL, Linkage::Import, &sig)
            .map_err(|e| ObjectError::LoweringFailed {
                fn_name: mir_fn.name.clone(),
                detail: format!("declare {FMOD_F64_SYMBOL} : {e}"),
            })?;
        let fref = obj_module.declare_func_in_func(id, &mut codegen_ctx.func);
        imports.f64_ref = Some(fref);
    }
    Ok(imports)
}

/// Object-side `arith.remf` lowering — issue a `call` against the pre-
/// declared libm `fmodf` / `fmod` import, picking the symbol by the op's
/// result-type width.
fn emit_fmod_call(
    op: &MirOp,
    builder: &mut FunctionBuilder<'_>,
    value_map: &mut HashMap<ValueId, cranelift_codegen::ir::Value>,
    fn_name: &str,
    fmod_refs: &FmodImports,
) -> Result<bool, ObjectError> {
    let r = op.results.first().ok_or_else(|| ObjectError::LoweringFailed {
        fn_name: fn_name.to_string(),
        detail: "arith.remf with no result".to_string(),
    })?;
    let MirType::Float(w) = r.ty else {
        return Err(ObjectError::LoweringFailed {
            fn_name: fn_name.to_string(),
            detail: format!(
                "arith.remf result must be float ; got `{}`",
                r.ty
            ),
        });
    };
    let fref = fmod_refs.for_width(w).ok_or_else(|| ObjectError::LoweringFailed {
        fn_name: fn_name.to_string(),
        detail: format!(
            "arith.remf {w:?} : libm fmod import not declared (pre-scan bug)"
        ),
    })?;
    let (a_id, b_id) = (
        op.operands.first().ok_or_else(|| ObjectError::LoweringFailed {
            fn_name: fn_name.to_string(),
            detail: "arith.remf missing LHS operand".to_string(),
        })?,
        op.operands.get(1).ok_or_else(|| ObjectError::LoweringFailed {
            fn_name: fn_name.to_string(),
            detail: "arith.remf missing RHS operand".to_string(),
        })?,
    );
    let a = *value_map.get(a_id).ok_or_else(|| ObjectError::UnknownValueId {
        fn_name: fn_name.to_string(),
        value_id: a_id.0,
    })?;
    let b = *value_map.get(b_id).ok_or_else(|| ObjectError::UnknownValueId {
        fn_name: fn_name.to_string(),
        value_id: b_id.0,
    })?;
    let call = builder.ins().call(fref, &[a, b]);
    let cl_value = *builder
        .inst_results(call)
        .first()
        .ok_or_else(|| ObjectError::LoweringFailed {
            fn_name: fn_name.to_string(),
            detail: "fmod call produced no result".to_string(),
        })?;
    value_map.insert(r.id, cl_value);
    Ok(false)
}

// ───────────────────────────────────────────────────────────────────────
// § T11-CC-2 (W-CC-funccall) — `func.call` callee imports.
//
//   `CalleeImports` is the object-side mirror of [`HeapImports`] : a per-fn
//   map from source-form callee-name → cranelift `FuncRef`. Populated by the
//   pre-scan helper [`declare_callee_imports_for_fn`] and consumed at
//   `func.call` lowering time by [`obj_lower_func_call`].
//
//   Two callee classes coexist :
//     1. Intra-module : the callee is a `MirFunc` defined elsewhere in the
//        same `MirModule`. Pass-1 of `emit_object_module_with_format`
//        already declared it as `Linkage::Export` and stored its `FuncId`
//        in the per-module `fn_table`. Here we just re-bind it to the
//        current fn via `declare_func_in_func`.
//     2. Extern : the callee is NOT in the module (e.g., a host symbol like
//        `__cssl_loa_test_call_host_get_42` or a sibling stage-0 stub). Here
//        we declare it as `Linkage::Import` against the cranelift module
//        with a signature derived from the callsite : operand types come
//        from a value-type map built by walking the entry block's
//        block-args + each prior op's results ; result types come straight
//        off the `func.call` op's `results` field.
//
//   Stage-0 contract : single-result callees only (any result-count > 1
//   surfaces via the existing `LoweringFailed` path on first lowered op
//   that consumes the missing values). Predicate-suffix dispatch (jit's
//   `transcendental_callee_key` "name#width" form) is NOT used — user-
//   defined callees are width-monomorphic at object-emit time (the
//   `auto_monomorph` MIR pass has already rewritten any generic call-sites
//   to mangled-name concrete callsites).
// ───────────────────────────────────────────────────────────────────────

/// Per-fn map of source-form callee-name → cranelift `FuncRef` for that
/// callee within the current fn body. Mirrors [`HeapImports`].
#[derive(Default)]
struct CalleeImports {
    refs: HashMap<String, cranelift_codegen::ir::FuncRef>,
}

impl CalleeImports {
    fn get(&self, callee: &str) -> Option<cranelift_codegen::ir::FuncRef> {
        self.refs.get(callee).copied()
    }
}

/// Build a `ValueId → MirType` map by walking the entry block once.
/// Block-args + each op's results contribute their types. Used by the
/// callee-import pre-scan to derive an extern-callee's signature from
/// callsite operand types when no other signature source is available.
fn build_value_type_map(mir_fn: &MirFunc) -> HashMap<ValueId, MirType> {
    let mut tys: HashMap<ValueId, MirType> = HashMap::new();
    if let Some(entry) = mir_fn.body.blocks.first() {
        for arg in &entry.args {
            tys.insert(arg.id, arg.ty.clone());
        }
        for op in &entry.ops {
            for r in &op.results {
                tys.insert(r.id, r.ty.clone());
            }
            // Walk inner regions one level deep so structured-CFG ops
            // contribute their nested results to the type-map. Stage-0 only
            // needs entry-block-level visibility for func.call sigs (the
            // callee names referenced there) — but a callsite inside a then-
            // branch IS reachable from the dispatcher, so include it.
            for region in &op.regions {
                for inner_block in &region.blocks {
                    for arg in &inner_block.args {
                        tys.insert(arg.id, arg.ty.clone());
                    }
                    for inner_op in &inner_block.ops {
                        for r in &inner_op.results {
                            tys.insert(r.id, r.ty.clone());
                        }
                    }
                }
            }
        }
    }
    tys
}

/// Pre-scan the fn body for unique `func.call` callee names ; for each :
///   - resolve to the pre-declared `FuncId` from `fn_table` if present
///     (intra-module, sibling fn defined locally) ; bind via
///     `declare_func_in_func` so the body-lowerer can issue a `call`.
///   - else declare it as `Linkage::Import` with a signature derived from
///     the callsite operand types (via `value_types` lookup) + result
///     types (carried on the op).
///
/// Walks both the entry-block ops AND the immediate inner regions of any
/// structured-CFG ops in the entry block — `func.call` sites inside an
/// `scf.if` then-branch share the per-fn import surface with the parent.
fn declare_callee_imports_for_fn(
    obj_module: &mut ObjectModule,
    codegen_ctx: &mut Context,
    mir_fn: &MirFunc,
    fn_table: &HashMap<String, FuncId>,
    ptr_ty: cranelift_codegen::ir::Type,
    struct_layouts: Option<&BTreeMap<String, MirStructLayout>>,
) -> Result<CalleeImports, ObjectError> {
    let mut imports = CalleeImports::default();
    let Some(entry_block) = mir_fn.body.blocks.first() else {
        return Ok(imports);
    };

    let value_types = build_value_type_map(mir_fn);
    let call_conv = obj_module.isa().default_call_conv();

    // Recursive walker : visit the op's immediate body + any nested-region
    // ops one level deep. Stage-0 doesn't recurse arbitrarily — D5 will
    // tighten the structural surface — but we DO want `func.call` inside
    // `scf.if` / `scf.loop` to participate in the import surface.
    fn collect_callees<'a>(
        ops: &'a [MirOp],
        out: &mut Vec<&'a MirOp>,
    ) {
        for op in ops {
            if op.name == "func.call" {
                out.push(op);
            }
            for region in &op.regions {
                for block in &region.blocks {
                    collect_callees(&block.ops, out);
                }
            }
        }
    }
    let mut call_ops: Vec<&MirOp> = Vec::new();
    collect_callees(&entry_block.ops, &mut call_ops);

    for op in call_ops {
        let Some((_, callee)) = op.attributes.iter().find(|(k, _)| k == "callee") else {
            // Malformed `func.call` lacking a callee attribute — defer the
            // diagnostic to body lowering where we already produce a
            // descriptive `LoweringFailed`.
            continue;
        };
        if imports.refs.contains_key(callee) {
            continue;
        }

        // Path-1 : intra-module sibling fn already declared by pass-1.
        if let Some(&callee_id) = fn_table.get(callee) {
            let fref = obj_module.declare_func_in_func(callee_id, &mut codegen_ctx.func);
            imports.refs.insert(callee.clone(), fref);
            continue;
        }

        // Path-2 : extern callee — derive signature from the callsite.
        // Operand types : look up each operand-id in `value_types`. Missing
        // entries fall back to the host pointer type (matches the FFI
        // convention used for `__cssl_*` symbols).
        let mut param_tys: Vec<MirType> = Vec::with_capacity(op.operands.len());
        for vid in &op.operands {
            let mt = value_types
                .get(vid)
                .cloned()
                .unwrap_or(MirType::Ptr);
            param_tys.push(mt);
        }
        // Result types come straight from the op's results — stage-0 ≤ 1.
        let result_tys: Vec<MirType> =
            op.results.iter().map(|r| r.ty.clone()).collect();

        let sig = build_clif_signature(
            call_conv,
            callee,
            &param_tys,
            &result_tys,
            ptr_ty,
            struct_layouts,
        )?;
        let extern_id = obj_module
            .declare_function(callee, Linkage::Import, &sig)
            .map_err(|e| ObjectError::LoweringFailed {
                fn_name: mir_fn.name.clone(),
                detail: format!("declare extern callee `{callee}` : {e}"),
            })?;
        let fref = obj_module.declare_func_in_func(extern_id, &mut codegen_ctx.func);
        imports.refs.insert(callee.clone(), fref);
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
    callee_refs: &CalleeImports,
    fmod_refs: &FmodImports,
    ptr_ty: cranelift_codegen::ir::Type,
    block_map: &[ClBlock],
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
        // § T11-W18-CSSLC-ADVANCE2 — float remainder via libm callout.
        //   Cranelift has no `frem` ; we route through `fmodf` (f32) /
        //   `fmod` (f64) declared up-front by `declare_fmod_imports_for_fn`.
        //   Result-width determines which symbol is used. See `emit_fmod_call`
        //   for the resolution + call-emission. No verifier mismatch is
        //   possible since both libm signatures match the cranelift xmm-register
        //   ABI exactly on x86_64 SSE2 (and likewise on aarch64 NEON / RISC-V).
        "arith.remf" => emit_fmod_call(op, builder, value_map, fn_name, fmod_refs),
        // § T11-W18-CSSLC-SCALAR-ARITH-COMPLETION — unary negation +
        // bitwise + shift dispatch. body_lower emits these MIR ops for the
        // CSSL surface ops `-x` (int + float), `~x`, `x & y`, `x | y`,
        // `x ^ y`, `x << y`, `x >> y`. Prior to this slice every such body
        // was rejected with "not in stage-0 object-emit subset" — closing
        // the gap unlocks scalar arith for substrate-intelligence (KAN
        // bias-update sign-flips, hash-mix XOR, byte-shift packers).
        //
        // § Mapping
        //   arith.negi      → `b.ins().ineg(a)`         (stage-0 alias)
        //   arith.negf      → `b.ins().fneg(a)`
        //   arith.subi_neg  → `b.ins().ineg(a)`         (HIR-emit name for `-x` on int)
        //   arith.xori_not  → `b.ins().bnot(a)`         (HIR-emit name for `~x`)
        //   arith.andi      → `b.ins().band(a, b)`
        //   arith.ori       → `b.ins().bor(a, b)`
        //   arith.xori      → `b.ins().bxor(a, b)`
        //   arith.shli      → `b.ins().ishl(a, b)`
        //   arith.shrsi     → `b.ins().sshr(a, b)`
        //   arith.shrui     → `b.ins().ushr(a, b)`
        "arith.negi" | "arith.subi_neg" => unary_int(op, builder, value_map, fn_name, |b, a| {
            b.ins().ineg(a)
        }),
        "arith.negf" => unary_int(op, builder, value_map, fn_name, |b, a| b.ins().fneg(a)),
        "arith.xori_not" => unary_int(op, builder, value_map, fn_name, |b, a| b.ins().bnot(a)),
        "arith.andi" => binary_int(op, builder, value_map, fn_name, |b, a, c| {
            b.ins().band(a, c)
        }),
        "arith.ori" => binary_int(op, builder, value_map, fn_name, |b, a, c| b.ins().bor(a, c)),
        "arith.xori" => binary_int(op, builder, value_map, fn_name, |b, a, c| {
            b.ins().bxor(a, c)
        }),
        "arith.shli" => binary_int(op, builder, value_map, fn_name, |b, a, c| {
            b.ins().ishl(a, c)
        }),
        "arith.shrsi" => binary_int(op, builder, value_map, fn_name, |b, a, c| {
            b.ins().sshr(a, c)
        }),
        "arith.shrui" => binary_int(op, builder, value_map, fn_name, |b, a, c| {
            b.ins().ushr(a, c)
        }),
        // § T11-D316 (W-A2-δ stage-0-emit-expand) — signed integer divide /
        // remainder. Symmetric with the existing add/sub/mul triple ; needed
        // for `let q = x / y` style straight-line code that body_lower emits
        // as `arith.divi`.
        "arith.divi" | "arith.divsi" => binary_int(op, builder, value_map, fn_name, |b, a, c| {
            b.ins().sdiv(a, c)
        }),
        "arith.divui" => binary_int(op, builder, value_map, fn_name, |b, a, c| {
            b.ins().udiv(a, c)
        }),
        "arith.remi" | "arith.remsi" => binary_int(op, builder, value_map, fn_name, |b, a, c| {
            b.ins().srem(a, c)
        }),
        "arith.remui" => binary_int(op, builder, value_map, fn_name, |b, a, c| {
            b.ins().urem(a, c)
        }),
        // § T11-D316 (W-A2-δ) — integer + float comparisons. body_lower emits
        // the predicate as a name-suffix (`arith.cmpi_sgt`, `arith.cmpf_olt`,
        // …) rather than an attribute, so the dispatch arms list every variant
        // and the helpers `obj_lower_cmpi` / `obj_lower_cmpf` extract the
        // predicate via the shared `obj_predicate_from_op` extractor (mirrors
        // the SPIR-V emitter's pattern in `body_emit::predicate_from_op`).
        "arith.cmpi"
        | "arith.cmpi_eq"
        | "arith.cmpi_ne"
        | "arith.cmpi_slt"
        | "arith.cmpi_sle"
        | "arith.cmpi_sgt"
        | "arith.cmpi_sge"
        | "arith.cmpi_ult"
        | "arith.cmpi_ule"
        | "arith.cmpi_ugt"
        | "arith.cmpi_uge" => obj_lower_cmpi(op, builder, value_map, fn_name),
        "arith.cmpf"
        | "arith.cmpf_eq"
        | "arith.cmpf_oeq"
        | "arith.cmpf_ne"
        | "arith.cmpf_one"
        | "arith.cmpf_olt"
        | "arith.cmpf_lt"
        | "arith.cmpf_ole"
        | "arith.cmpf_le"
        | "arith.cmpf_ogt"
        | "arith.cmpf_gt"
        | "arith.cmpf_oge"
        | "arith.cmpf_ge"
        | "arith.cmpf_ult"
        | "arith.cmpf_ule"
        | "arith.cmpf_ugt"
        | "arith.cmpf_uge"
        | "arith.cmpf_ord"
        | "arith.cmpf_uno" => obj_lower_cmpf(op, builder, value_map, fn_name),
        // § T11-D316 (W-A2-δ) — `arith.select` ternary. (cond, t, f) → t if
        // cond else f. Cranelift's `select` natively expresses this so no
        // extra block-splitting is needed.
        "arith.select" => obj_lower_select(op, builder, value_map, fn_name),
        // § T11-D318 (W-CC-mut-assign) — `cssl.assign` is a marker op that
        //   body_lower emits AFTER `emit_local_store` to record the
        //   assignment target for diagnostics. The actual store is already
        //   in the op stream as `memref.store`. Treat as a no-op : bind
        //   the (unit) result and continue. The marker carries
        //   target=local_cell when cell-store path was taken.
        "cssl.assign" => Ok(false),
        // § T11-D59 (S6-C3) — memref.load + memref.store. See
        // `specs/02_IR.csl § MEMORY-OPS` and the JIT-side mirror in `jit.rs`.
        "memref.load" => obj_lower_memref_load(op, builder, value_map, fn_name, ptr_ty),
        "memref.store" => obj_lower_memref_store(op, builder, value_map, fn_name),
        "func.return" => {
            // § T11-W19 · int-literal-coercion at func.return
            //   MIR int-literals default to I32 ; fn-signatures may declare
            //   I64/I16/I8 returns. The verifier rejects the type mismatch
            //   ("result N has type iX, must match function signature of iY").
            //   Insert sextend/ireduce/etc. to bridge the gap. Float + scalar
            //   mismatches surface unchanged so the verifier still catches
            //   semantic bugs.
            let sig_returns: Vec<cranelift_codegen::ir::Type> = builder
                .func
                .signature
                .returns
                .iter()
                .map(|p| p.value_type)
                .collect();
            let mut args = Vec::with_capacity(op.operands.len());
            for (idx, vid) in op.operands.iter().enumerate() {
                let mut v = *value_map
                    .get(vid)
                    .ok_or_else(|| ObjectError::UnknownValueId {
                        fn_name: fn_name.to_string(),
                        value_id: vid.0,
                    })?;
                if let Some(&expected_ty) = sig_returns.get(idx) {
                    let actual_ty = builder.func.dfg.value_type(v);
                    if actual_ty != expected_ty
                        && expected_ty.is_int()
                        && actual_ty.is_int()
                    {
                        let exp_bits = expected_ty.bits();
                        let act_bits = actual_ty.bits();
                        if exp_bits > act_bits {
                            // Widen via sign-extend (cranelift IR is
                            // signedness-erased ; sextend works for both
                            // signed-positive and signed-negative literals
                            // that fit in the source width).
                            v = builder.ins().sextend(expected_ty, v);
                        } else if exp_bits < act_bits {
                            v = builder.ins().ireduce(expected_ty, v);
                        }
                    }
                }
                args.push(v);
            }
            builder.ins().return_(&args);
            Ok(true)
        }
        // § T11-CC-1 (W-CC-multiblock) — unconditional jump to another MIR
        //   block. Carries `target=N` attribute (block-index in the parent
        //   fn's `body.blocks`). Operands are the SSA values to forward as
        //   block-args of the destination.
        "cssl.branch" => obj_lower_cssl_branch(op, builder, value_map, fn_name, block_map),
        // § T11-CC-1 (W-CC-multiblock) — conditional branch. Carries
        //   `then_target=N` / `else_target=M` block-index attributes plus
        //   `then_arg_count=K` / `else_arg_count=L` so we know how to slice
        //   the operand list. Operand layout :
        //     `[cond, then_arg_0, …, then_arg_{K-1}, else_arg_0, …, else_arg_{L-1}]`.
        //   Cranelift's `brif(cond, then_blk, &then_args, else_blk, &else_args)`.
        "cssl.brif" => obj_lower_cssl_brif(op, builder, value_map, fn_name, block_map),
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
        // § T11-CC-2 (W-CC-funccall) — `func.call` lowering. Resolves the
        //   pre-declared `FuncRef` from `callee_refs` (built up-front by
        //   `declare_callee_imports_for_fn`) and emits a cranelift `call`.
        //   Stage-0 single-result : the first cranelift result-value is bound
        //   into `value_map` under the op's first result-id. Void callees
        //   produce no result and are valid (callsite carries no `.results`).
        "func.call" => obj_lower_func_call(op, builder, value_map, fn_name, callee_refs),
        // § T11-D58 (S6-C1) — structured-control-flow lowering. `scf.if`
        //   delegates to the shared `crate::scf::lower_scf_if` helper which
        //   creates the then/else/merge blocks + emits `brif`. `scf.yield`
        //   is consumed by that helper directly ; encountering it at the
        //   outer dispatch level means the parent region terminator leaked,
        //   which we treat as a no-op here. D5 (StructuredCfgValidator) will
        //   reject bare top-level scf.yield in a future slice.
        "scf.if" => lower_scf_if_in_object(
            op,
            builder,
            value_map,
            fn_name,
            heap_refs,
            callee_refs,
            fmod_refs,
            ptr_ty,
            block_map,
        ),
        // § T11-D61 (S6-C2) — structured loops. Each delegates to the
        //   matching `crate::scf::lower_scf_*` helper ; the body-walker
        //   dispatcher closure re-enters `lower_one_op` so nested ops
        //   (arith / heap / nested scf.*) flow through the same dispatch.
        "scf.loop" => lower_scf_loop_in_object(
            op,
            builder,
            value_map,
            fn_name,
            heap_refs,
            callee_refs,
            fmod_refs,
            ptr_ty,
            block_map,
        ),
        "scf.while" => lower_scf_while_in_object(
            op,
            builder,
            value_map,
            fn_name,
            heap_refs,
            callee_refs,
            fmod_refs,
            ptr_ty,
            block_map,
        ),
        "scf.for" => lower_scf_for_in_object(
            op,
            builder,
            value_map,
            fn_name,
            heap_refs,
            callee_refs,
            fmod_refs,
            ptr_ty,
            block_map,
        ),
        "scf.yield" => Ok(false),
        // § T11-D318 (W-CC-mut-assign) — `scf.condition` is the cond-region
        //   terminator inside the new `scf.while` shape. The cond-region
        //   walker (`scf::lower_while_cond_region`) consumes this op
        //   directly via its first operand ; if it leaks into the
        //   top-level dispatcher, treat as a no-op. Top-level scf.condition
        //   means the parent isn't an scf.while body — we don't reject it
        //   to keep the dispatcher robust against future MIR shapes.
        "scf.condition" => Ok(false),
        // § T11-D318 — `cssl.local.alloca` declares a stack-cell of the
        //   element type recorded in the `slot_ty` attribute. Returns a
        //   pointer-typed cranelift Value ; subsequent `memref.load` /
        //   `memref.store` against that Value reads/writes the cell. The
        //   slot is allocated via cranelift's `StackSlotData::new` with
        //   `ExplicitSlot` kind so its address is taken via `stack_addr`.
        //   Each `let mut x` declaration emits one of these at fn-prologue
        //   time (relative to the source position) ; the runtime cost is
        //   a single host-pointer-width register-load (the stack-addr).
        "cssl.local.alloca" => obj_lower_local_alloca(op, builder, value_map, fn_name, ptr_ty),
        // § T11-D77 (S6-C5 redo) — `cssl.closure` materializes the closure VALUE
        //   (the `(fn-ptr, env-ptr)` fat-pair). At stage-0 the body_lower has
        //   already emitted the env-pack sequence (arith.constant + arith.constant
        //   + cssl.heap.alloc + per-capture {arith.constant + memref.store}), so
        //   here we just bind the result-id to the env-ptr operand for closures
        //   with ≥1 capture, or to a typed-zero pointer sentinel for closures
        //   with no captures. Inner body region is intentionally not walked —
        //   indirect-call lowering through the closure is deferred per spec.
        "cssl.closure" => obj_lower_closure(op, builder, value_map, fn_name, ptr_ty),
        // § T11-D100 (J2 — closures callable) — `cssl.closure.call` marker.
        //   The body has been inlined upstream (in
        //   `body_lower::lower_closure_call`) — captures reloaded via
        //   memref.load, lambda params bound to call-site args, body ops
        //   emitted into the same block. This op is a pure value-map binder :
        //   look up the `yield_value_id` attribute (the body's trailing SSA-id)
        //   and re-bind the marker's result-id to it. See spec § CLOSURE-ENV
        //   "invocation (T11-D100 / J2 …)".
        "cssl.closure.call" => obj_lower_closure_call(op, value_map, fn_name),
        // § T11-D100 (J2) — call-site arity-mismatch / arg-lowering-failure
        //   marker. Bind the result-id to a typed-zero ptr sentinel ; error
        //   detail rides as an attribute for future diagnostic surfacing.
        //   Stage-0 doesn't trap at runtime — the error is structural at
        //   lowering time.
        "cssl.closure.call.error" => {
            Ok(obj_lower_closure_call_error(op, builder, value_map, ptr_ty))
        }
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

/// § T11-D100 (J2 — closures callable) — Object-side `cssl.closure.call` marker.
///
/// Mirrors [`crate::jit::jit_lower_closure_call`] : the body has already been
/// inlined upstream by `body_lower::lower_closure_call` (capture-reload memref
/// loads + lambda-param→arg bindings + body ops). This op binds its result-id
/// to the body's trailing SSA-value (recorded as the `yield_value_id` attr).
/// No cranelift instructions are emitted — pure value-map plumbing.
fn obj_lower_closure_call(
    op: &MirOp,
    value_map: &mut HashMap<ValueId, cranelift_codegen::ir::Value>,
    fn_name: &str,
) -> Result<bool, ObjectError> {
    let Some(r) = op.results.first() else {
        return Ok(false);
    };
    let yield_str = op
        .attributes
        .iter()
        .find(|(k, _)| k == "yield_value_id")
        .map(|(_, v)| v.as_str());
    let Some(yield_str) = yield_str else {
        return Ok(false);
    };
    let yield_raw: u32 = yield_str.parse().map_err(|e| ObjectError::LoweringFailed {
        fn_name: fn_name.to_string(),
        detail: format!("cssl.closure.call : malformed yield_value_id `{yield_str}` ({e})"),
    })?;
    let yield_target = ValueId(yield_raw);
    let v = *value_map
        .get(&yield_target)
        .ok_or_else(|| ObjectError::UnknownValueId {
            fn_name: fn_name.to_string(),
            value_id: yield_raw,
        })?;
    value_map.insert(r.id, v);
    Ok(false)
}

/// § T11-D100 (J2 — closures callable) — Object-side
/// `cssl.closure.call.error` arity-mismatch marker. Binds the result-id to a
/// typed-zero ptr sentinel ; error detail rides on attributes. Returns `false`
/// always — the op is not a terminator and the error doesn't trap at runtime
/// at stage-0 (the surface is structural, surfaced at MIR-build time).
fn obj_lower_closure_call_error(
    op: &MirOp,
    builder: &mut FunctionBuilder<'_>,
    value_map: &mut HashMap<ValueId, cranelift_codegen::ir::Value>,
    ptr_ty: cranelift_codegen::ir::Type,
) -> bool {
    let Some(r) = op.results.first() else {
        return false;
    };
    let v = builder.ins().iconst(ptr_ty, 0);
    value_map.insert(r.id, v);
    false
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

/// § T11-CC-2 (W-CC-funccall) — Object-side `func.call` lowering.
///
/// Reads the `callee` attribute, looks up the pre-declared cranelift
/// `FuncRef` in `callee_refs` (populated by
/// [`declare_callee_imports_for_fn`]), gathers operand cranelift `Value`s
/// from `value_map`, and emits a cranelift `call`. Single-result calls bind
/// the first cranelift result-value into the value-map under the op's first
/// result-id. Void calls produce no result.
fn obj_lower_func_call(
    op: &MirOp,
    builder: &mut FunctionBuilder<'_>,
    value_map: &mut HashMap<ValueId, cranelift_codegen::ir::Value>,
    fn_name: &str,
    callee_refs: &CalleeImports,
) -> Result<bool, ObjectError> {
    let (_, callee) = op
        .attributes
        .iter()
        .find(|(k, _)| k == "callee")
        .ok_or_else(|| ObjectError::LoweringFailed {
            fn_name: fn_name.to_string(),
            detail: "func.call missing `callee` attribute".to_string(),
        })?;
    let func_ref =
        callee_refs
            .get(callee.as_str())
            .ok_or_else(|| ObjectError::LoweringFailed {
                fn_name: fn_name.to_string(),
                detail: format!(
                    "func.call to `{callee}` : FuncRef not declared (pre-scan bug)"
                ),
            })?;

    let mut args: Vec<cranelift_codegen::ir::Value> = Vec::with_capacity(op.operands.len());
    for vid in &op.operands {
        let v = *value_map
            .get(vid)
            .ok_or_else(|| ObjectError::UnknownValueId {
                fn_name: fn_name.to_string(),
                value_id: vid.0,
            })?;
        args.push(v);
    }
    let inst = builder.ins().call(func_ref, &args);

    if let Some(r) = op.results.first() {
        let results = builder.inst_results(inst).to_vec();
        let cl_value = *results.first().ok_or_else(|| ObjectError::LoweringFailed {
            fn_name: fn_name.to_string(),
            detail: format!(
                "func.call to `{callee}` produced no cranelift result but op has {} result(s)",
                op.results.len()
            ),
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
    callee_refs: &CalleeImports,
    fmod_refs: &FmodImports,
    ptr_ty: cranelift_codegen::ir::Type,
    block_map: &[ClBlock],
) -> Result<bool, ObjectError> {
    crate::scf::lower_scf_if(
        op,
        builder,
        value_map,
        fn_name,
        |branch_op, b, vm, name| -> Result<bool, ObjectError> {
            lower_one_op(
                branch_op,
                b,
                vm,
                name,
                heap_refs,
                callee_refs,
                fmod_refs,
                ptr_ty,
                block_map,
            )
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
    callee_refs: &CalleeImports,
    fmod_refs: &FmodImports,
    ptr_ty: cranelift_codegen::ir::Type,
    block_map: &[ClBlock],
) -> Result<bool, ObjectError> {
    crate::scf::lower_scf_loop(
        op,
        builder,
        value_map,
        fn_name,
        |body_op, b, vm, name| -> Result<bool, ObjectError> {
            lower_one_op(
                body_op,
                b,
                vm,
                name,
                heap_refs,
                callee_refs,
                fmod_refs,
                ptr_ty,
                block_map,
            )
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
    callee_refs: &CalleeImports,
    fmod_refs: &FmodImports,
    ptr_ty: cranelift_codegen::ir::Type,
    block_map: &[ClBlock],
) -> Result<bool, ObjectError> {
    crate::scf::lower_scf_while(
        op,
        builder,
        value_map,
        fn_name,
        |body_op, b, vm, name| -> Result<bool, ObjectError> {
            lower_one_op(
                body_op,
                b,
                vm,
                name,
                heap_refs,
                callee_refs,
                fmod_refs,
                ptr_ty,
                block_map,
            )
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
    callee_refs: &CalleeImports,
    fmod_refs: &FmodImports,
    ptr_ty: cranelift_codegen::ir::Type,
    block_map: &[ClBlock],
) -> Result<bool, ObjectError> {
    crate::scf::lower_scf_for(
        op,
        builder,
        value_map,
        fn_name,
        |body_op, b, vm, name| -> Result<bool, ObjectError> {
            lower_one_op(
                body_op,
                b,
                vm,
                name,
                heap_refs,
                callee_refs,
                fmod_refs,
                ptr_ty,
                block_map,
            )
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
// § T11-CC-1 (W-CC-multiblock) — multi-block terminator helpers.
// ───────────────────────────────────────────────────────────────────────

/// Resolve a `target=N` style attribute to a cranelift Block from the
/// fn-scoped `block_map`. Returns `BlockTargetOutOfRange` when the
/// requested index is past the end of the MIR-block list.
fn resolve_block_target(
    op: &MirOp,
    attr_key: &str,
    fn_name: &str,
    block_map: &[ClBlock],
) -> Result<ClBlock, ObjectError> {
    let target_str = op
        .attributes
        .iter()
        .find(|(k, _)| k == attr_key)
        .map(|(_, v)| v.as_str())
        .ok_or_else(|| ObjectError::LoweringFailed {
            fn_name: fn_name.to_string(),
            detail: format!("{} : missing `{attr_key}` attribute", op.name),
        })?;
    let target_idx: usize =
        target_str
            .parse()
            .map_err(|e: std::num::ParseIntError| ObjectError::LoweringFailed {
                fn_name: fn_name.to_string(),
                detail: format!(
                    "{} : malformed `{attr_key}` attribute `{target_str}` ({e})",
                    op.name
                ),
            })?;
    block_map
        .get(target_idx)
        .copied()
        .ok_or_else(|| ObjectError::BlockTargetOutOfRange {
            fn_name: fn_name.to_string(),
            target_idx,
            block_count: block_map.len(),
        })
}

/// Lower `cssl.branch` : unconditional `jump` to the target MIR-block,
/// forwarding all operands as block-args.
fn obj_lower_cssl_branch(
    op: &MirOp,
    builder: &mut FunctionBuilder<'_>,
    value_map: &mut HashMap<ValueId, cranelift_codegen::ir::Value>,
    fn_name: &str,
    block_map: &[ClBlock],
) -> Result<bool, ObjectError> {
    let target_blk = resolve_block_target(op, "target", fn_name, block_map)?;
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
    builder.ins().jump(target_blk, &args);
    Ok(true)
}

/// Lower `cssl.brif` : conditional branch via cranelift's `brif`.
///   Operand layout :
///     `[cond, then_arg_0, …, then_arg_{K-1}, else_arg_0, …, else_arg_{L-1}]`
///   where K = `then_arg_count` attribute (default 0) and L =
///   `else_arg_count` attribute (default 0). The total operand count must
///   equal `1 + K + L` ; mismatch errors out cleanly.
fn obj_lower_cssl_brif(
    op: &MirOp,
    builder: &mut FunctionBuilder<'_>,
    value_map: &mut HashMap<ValueId, cranelift_codegen::ir::Value>,
    fn_name: &str,
    block_map: &[ClBlock],
) -> Result<bool, ObjectError> {
    let then_blk = resolve_block_target(op, "then_target", fn_name, block_map)?;
    let else_blk = resolve_block_target(op, "else_target", fn_name, block_map)?;
    let then_arg_count: usize = op
        .attributes
        .iter()
        .find(|(k, _)| k == "then_arg_count")
        .and_then(|(_, v)| v.parse().ok())
        .unwrap_or(0);
    let else_arg_count: usize = op
        .attributes
        .iter()
        .find(|(k, _)| k == "else_arg_count")
        .and_then(|(_, v)| v.parse().ok())
        .unwrap_or(0);
    let expected = 1 + then_arg_count + else_arg_count;
    if op.operands.len() != expected {
        return Err(ObjectError::LoweringFailed {
            fn_name: fn_name.to_string(),
            detail: format!(
                "cssl.brif : operand-count mismatch ; expected 1+{then_arg_count}+{else_arg_count}={expected} got {}",
                op.operands.len()
            ),
        });
    }
    let cond = *value_map
        .get(&op.operands[0])
        .ok_or_else(|| ObjectError::UnknownValueId {
            fn_name: fn_name.to_string(),
            value_id: op.operands[0].0,
        })?;
    let mut then_args = Vec::with_capacity(then_arg_count);
    for vid in &op.operands[1..1 + then_arg_count] {
        let v = *value_map
            .get(vid)
            .ok_or_else(|| ObjectError::UnknownValueId {
                fn_name: fn_name.to_string(),
                value_id: vid.0,
            })?;
        then_args.push(v);
    }
    let mut else_args = Vec::with_capacity(else_arg_count);
    for vid in &op.operands[1 + then_arg_count..] {
        let v = *value_map
            .get(vid)
            .ok_or_else(|| ObjectError::UnknownValueId {
                fn_name: fn_name.to_string(),
                value_id: vid.0,
            })?;
        else_args.push(v);
    }
    builder
        .ins()
        .brif(cond, then_blk, &then_args, else_blk, &else_args);
    Ok(true)
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

/// § T11-W18-CSSLC-SCALAR-ARITH-COMPLETION — single-operand op-emit
/// helper, symmetric to [`binary_int`]. Used for `arith.{negi,negf,
/// subi_neg,xori_not}` so the dispatch arms read uniformly. The emit
/// closure receives the resolved Cranelift `Value` for the single
/// operand and returns the produced `Value`. Errors mirror
/// [`binary_int`] : missing result / wrong-arity / unknown-value-id all
/// surface as `ObjectError::LoweringFailed` or `UnknownValueId`.
fn unary_int<F>(
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
    ) -> cranelift_codegen::ir::Value,
{
    let r = op
        .results
        .first()
        .ok_or_else(|| ObjectError::LoweringFailed {
            fn_name: fn_name.to_string(),
            detail: format!("{} with no result", op.name),
        })?;
    if op.operands.is_empty() {
        return Err(ObjectError::LoweringFailed {
            fn_name: fn_name.to_string(),
            detail: format!("{} expected 1 operand, got 0", op.name),
        });
    }
    let a = *value_map
        .get(&op.operands[0])
        .ok_or_else(|| ObjectError::UnknownValueId {
            fn_name: fn_name.to_string(),
            value_id: op.operands[0].0,
        })?;
    let v = emit(builder, a);
    value_map.insert(r.id, v);
    Ok(false)
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

// § T11-D316 (W-A2-δ) — comparison + select helpers. Predicate is recovered
// from either the op-name suffix (`arith.cmpi_sgt` → "sgt") or the legacy
// `predicate` attribute (JIT convention). Mirrors
// `body_emit::predicate_from_op` in cssl-cgen-gpu-spirv.

fn obj_predicate_from_op<'a>(op: &'a MirOp, family: char) -> &'a str {
    let prefix = if family == 'i' {
        "arith.cmpi_"
    } else {
        "arith.cmpf_"
    };
    if let Some(rest) = op.name.strip_prefix(prefix) {
        return rest;
    }
    op.attributes
        .iter()
        .find(|(k, _)| k == "predicate")
        .map_or("", |(_, v)| v.as_str())
}

fn parse_int_cc(s: &str) -> Option<cranelift_codegen::ir::condcodes::IntCC> {
    use cranelift_codegen::ir::condcodes::IntCC as I;
    Some(match s {
        "eq" => I::Equal,
        "ne" => I::NotEqual,
        "slt" => I::SignedLessThan,
        "sle" => I::SignedLessThanOrEqual,
        "sgt" => I::SignedGreaterThan,
        "sge" => I::SignedGreaterThanOrEqual,
        "ult" => I::UnsignedLessThan,
        "ule" => I::UnsignedLessThanOrEqual,
        "ugt" => I::UnsignedGreaterThan,
        "uge" => I::UnsignedGreaterThanOrEqual,
        _ => return None,
    })
}

fn parse_float_cc(s: &str) -> Option<cranelift_codegen::ir::condcodes::FloatCC> {
    use cranelift_codegen::ir::condcodes::FloatCC as F;
    Some(match s {
        "eq" | "oeq" => F::Equal,
        "ne" | "one" => F::NotEqual,
        "olt" | "lt" => F::LessThan,
        "ole" | "le" => F::LessThanOrEqual,
        "ogt" | "gt" => F::GreaterThan,
        "oge" | "ge" => F::GreaterThanOrEqual,
        "ult" => F::UnorderedOrLessThan,
        "ule" => F::UnorderedOrLessThanOrEqual,
        "ugt" => F::UnorderedOrGreaterThan,
        "uge" => F::UnorderedOrGreaterThanOrEqual,
        "ord" => F::Ordered,
        "uno" => F::Unordered,
        _ => return None,
    })
}

fn obj_lower_cmpi(
    op: &MirOp,
    builder: &mut FunctionBuilder<'_>,
    value_map: &mut HashMap<ValueId, cranelift_codegen::ir::Value>,
    fn_name: &str,
) -> Result<bool, ObjectError> {
    let pred_str = obj_predicate_from_op(op, 'i');
    let cc = parse_int_cc(pred_str).ok_or_else(|| ObjectError::LoweringFailed {
        fn_name: fn_name.to_string(),
        detail: format!("unknown {} predicate `{pred_str}`", op.name),
    })?;
    binary_int(op, builder, value_map, fn_name, |b, a, c| {
        b.ins().icmp(cc, a, c)
    })
}

fn obj_lower_cmpf(
    op: &MirOp,
    builder: &mut FunctionBuilder<'_>,
    value_map: &mut HashMap<ValueId, cranelift_codegen::ir::Value>,
    fn_name: &str,
) -> Result<bool, ObjectError> {
    let pred_str = obj_predicate_from_op(op, 'f');
    let cc = parse_float_cc(pred_str).ok_or_else(|| ObjectError::LoweringFailed {
        fn_name: fn_name.to_string(),
        detail: format!("unknown {} predicate `{pred_str}`", op.name),
    })?;
    binary_int(op, builder, value_map, fn_name, |b, a, c| {
        b.ins().fcmp(cc, a, c)
    })
}

fn obj_lower_select(
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
            detail: "arith.select with no result".to_string(),
        })?;
    if op.operands.len() != 3 {
        return Err(ObjectError::LoweringFailed {
            fn_name: fn_name.to_string(),
            detail: format!(
                "arith.select expected 3 operands (cond, t, f), got {}",
                op.operands.len()
            ),
        });
    }
    let cond = *value_map
        .get(&op.operands[0])
        .ok_or_else(|| ObjectError::UnknownValueId {
            fn_name: fn_name.to_string(),
            value_id: op.operands[0].0,
        })?;
    let t = *value_map
        .get(&op.operands[1])
        .ok_or_else(|| ObjectError::UnknownValueId {
            fn_name: fn_name.to_string(),
            value_id: op.operands[1].0,
        })?;
    let f = *value_map
        .get(&op.operands[2])
        .ok_or_else(|| ObjectError::UnknownValueId {
            fn_name: fn_name.to_string(),
            value_id: op.operands[2].0,
        })?;
    let v = builder.ins().select(cond, t, f);
    value_map.insert(r.id, v);
    Ok(false)
}

/// § T11-D318 (W-CC-mut-assign) — lower `cssl.local.alloca` to a
/// cranelift stack-slot + `stack_addr` instruction. The slot's size and
/// alignment are derived from the `slot_ty` attribute (the element type
/// of the cell). The result is a pointer-typed Value bound to the op's
/// result-id ; downstream `memref.load` / `memref.store` ops use that
/// Value as the address operand.
///
/// Stage-0 limits :
///   - Element types must be scalar (Int / Float / Bool / Ptr / Handle).
///     Tuple / struct / memref cell-types remain a future slice ; the
///     `slot_ty` parser falls back to a host-pointer-width slot for
///     anything it doesn't recognize so the slot is at least
///     well-aligned for the host ABI.
///   - The slot is allocated lazily on first reference per fn ; ordering
///     of `cssl.local.alloca` ops is preserved (each emits a new slot).
fn obj_lower_local_alloca(
    op: &MirOp,
    builder: &mut FunctionBuilder<'_>,
    value_map: &mut HashMap<ValueId, cranelift_codegen::ir::Value>,
    fn_name: &str,
    ptr_ty: cranelift_codegen::ir::Type,
) -> Result<bool, ObjectError> {
    use cranelift_codegen::ir::{StackSlotData, StackSlotKind};
    let r = op
        .results
        .first()
        .ok_or_else(|| ObjectError::LoweringFailed {
            fn_name: fn_name.to_string(),
            detail: "cssl.local.alloca with no result".to_string(),
        })?;
    let slot_ty_str = op
        .attributes
        .iter()
        .find(|(k, _)| k == "slot_ty")
        .map_or("", |(_, v)| v.as_str());
    let (slot_size, slot_align_log2) = parse_slot_ty_size_align(slot_ty_str, ptr_ty);
    let slot = builder.create_sized_stack_slot(StackSlotData::new(
        StackSlotKind::ExplicitSlot,
        slot_size,
        slot_align_log2,
    ));
    let addr = builder.ins().stack_addr(ptr_ty, slot, 0);
    value_map.insert(r.id, addr);
    Ok(false)
}

/// Parse a `slot_ty` attribute value (the MirType's Display form) into
/// `(byte_size, align_log2)` for cranelift `StackSlotData::new`. Returns
/// host-pointer-width defaults for any type we don't recognize so the
/// slot is at least well-aligned. The recognized scalar types match
/// `mir_type_to_cl`'s coverage : i8 / i16 / i32 / i64 / f32 / f64 / bool
/// / ptr / handle.
fn parse_slot_ty_size_align(
    slot_ty_str: &str,
    ptr_ty: cranelift_codegen::ir::Type,
) -> (u32, u8) {
    // MirType's Display form for scalars : "i32", "i64", "f32", "f64",
    // "bool", "ptr", "handle". Anything else falls back to host pointer
    // size / alignment.
    let s = slot_ty_str.trim();
    let host_ptr_bytes = u32::from(ptr_ty.bytes());
    let host_ptr_log2 = match host_ptr_bytes {
        8 => 3,
        4 => 2,
        2 => 1,
        _ => 3,
    };
    match s {
        "i1" | "i8" | "u8" | "bool" => (1, 0),
        "i16" | "u16" => (2, 1),
        "i32" | "u32" | "f32" => (4, 2),
        "i64" | "u64" | "f64" | "index" => (8, 3),
        "ptr" | "handle" | "!cssl.ptr" | "!cssl.handle" => (host_ptr_bytes, host_ptr_log2),
        _ => (host_ptr_bytes, host_ptr_log2),
    }
}

// ───────────────────────────────────────────────────────────────────────
// § MirType → cranelift Type
// ───────────────────────────────────────────────────────────────────────

fn mir_type_to_cl(
    t: &MirType,
    ptr_ty: cranelift_codegen::ir::Type,
) -> Option<cranelift_codegen::ir::Type> {
    // Internal-body call sites still use scalar-only lowering ; struct-FFI
    // codepath always goes through `mir_type_to_cl_with_layouts`.
    mir_type_to_cl_with_layouts(t, ptr_ty, None)
}

/// § T11-W17-A · stage-0 struct-FFI codegen
///
/// Map a MIR type to a cranelift `Type` for ABI-boundary lowering. Identical
/// to `mir_type_to_cl` for scalar types ; `MirType::Opaque("!cssl.struct.X")`
/// is resolved against the supplied layout-table :
///   - 1B  → I8     (newtype-byte struct)
///   - 2B  → I16    (newtype-u16 struct)
///   - ≤4B → I32    (small POD struct)
///   - ≤8B → I64    (newtype-u64 RunHandle case · LoA-systems primary)
///   - >8B → ptr_ty (Win-x64 / SysV >2-word ABI normal-rule)
///
/// Returns `None` if :
///   - layouts is `None` (caller declined struct resolution)
///   - the named struct is missing from the table
///   - the layout has zero bytes (empty struct)
fn mir_type_to_cl_with_layouts(
    t: &MirType,
    ptr_ty: cranelift_codegen::ir::Type,
    layouts: Option<&BTreeMap<String, MirStructLayout>>,
) -> Option<cranelift_codegen::ir::Type> {
    match t {
        MirType::Int(IntWidth::I32) => Some(cl_types::I32),
        MirType::Int(IntWidth::I64) => Some(cl_types::I64),
        MirType::Int(IntWidth::I16) => Some(cl_types::I16),
        MirType::Int(IntWidth::I8) => Some(cl_types::I8),
        MirType::Int(IntWidth::I1) => Some(cl_types::I8), // align with Bool below
        MirType::Int(IntWidth::Index) => Some(cl_types::I64),
        MirType::Float(FloatWidth::F32) => Some(cl_types::F32),
        MirType::Float(FloatWidth::F64) => Some(cl_types::F64),
        MirType::Bool => Some(cl_types::I8),
        // T11-D57 (S6-B1) — `!cssl.ptr` lowers to the active ISA's host
        //   pointer type. Tied to S6-A3's "ISA = host" assumption ;
        //   cross-compilation will revisit when target-triple resolution lands.
        MirType::Ptr | MirType::Handle => Some(ptr_ty),
        // T11-W17-A — struct-FFI : resolve `!cssl.struct.<name>` opaque to
        // the appropriate ABI scalar / pointer width.
        MirType::Opaque(s) => resolve_struct_opaque(s, ptr_ty, layouts),
        _ => None,
    }
}

/// Stage-0 ABI-class → cranelift `Type` mapping for a struct.
///
/// Helper for `mir_type_to_cl_with_layouts`. Stripped down from the full
/// abi-classification matrix because stage-0 only emits one register-sized
/// scalar OR a single pointer per slot.
///
/// § OPAQUE-NAME FORMS RECOGNIZED
///   The MIR opaque-tag for a struct can land in two forms depending on
///   which lowering path produced it :
///     - `"!cssl.struct.<name>"`   — explicit struct-construction op tag
///       (`body_lower::lower_struct` produces this for inline struct exprs)
///     - `"<name>"`                 — bare path-resolved type
///       (`lower::LowerCtx::lower_type` produces this for fn-signature
///       slots that name a struct via its identifier)
///   Both must resolve correctly so fn signatures + body construction
///   converge on the same ABI class.
#[inline]
fn resolve_struct_opaque(
    opaque_name: &str,
    ptr_ty: cranelift_codegen::ir::Type,
    layouts: Option<&BTreeMap<String, MirStructLayout>>,
) -> Option<cranelift_codegen::ir::Type> {
    let table = layouts?;
    // Try both forms : explicit "!cssl.struct.<name>" tag + bare "<name>"
    // path-resolved opaque. Whichever one matches the layout-table wins.
    let candidate = opaque_name
        .strip_prefix("!cssl.struct.")
        .unwrap_or(opaque_name);
    let layout = table.get(candidate)?;
    Some(match layout.abi_class()? {
        StructAbiClass::ScalarI8 => cl_types::I8,
        StructAbiClass::ScalarI16 => cl_types::I16,
        StructAbiClass::ScalarI32 => cl_types::I32,
        StructAbiClass::ScalarI64 => cl_types::I64,
        StructAbiClass::PointerByRef => ptr_ty,
    })
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

    // ─────────────────────────────────────────────────────────────────────
    // § T11-CC-1 (W-CC-multiblock) — multi-block body lowering tests.
    //
    // The single-block regression coverage lives in
    // `emit_minimal_main_returns_bytes` / `emit_addi_function_succeeds` /
    // friends ; the new tests exercise the multi-block branching path.
    // Each builds a synthetic MirFunc with N>1 blocks wired by `cssl.branch`
    // / `cssl.brif` terminators, asserts the produced object bytes carry
    // the host-platform magic, and where useful asserts a meaningful error
    // for malformed shapes.
    // ─────────────────────────────────────────────────────────────────────

    /// REGRESSION : the single-block path must still object-emit cleanly
    /// after the multi-block-aware refactor.
    #[test]
    fn single_block_still_compiles() {
        let main = build_const_return_fn("main", 42, MirType::Int(IntWidth::I32));
        let mut module = MirModule::new();
        module.push_func(main);
        let bytes = emit_object_module(&module).expect("single-block emit ok");
        let host_magic = magic_prefix(host_default_format());
        assert!(bytes.starts_with(host_magic));
    }

    /// `fn jump_then_return() -> i32 {
    ///   block 0 (entry) : jump block 1
    ///   block 1         : return 7
    /// }`
    #[test]
    fn two_block_jump_compiles() {
        use cssl_mir::MirBlock;
        let mut f = MirFunc::new("jump_then_return", vec![], vec![MirType::Int(IntWidth::I32)]);
        f.next_value_id = 1;
        // Entry : `cssl.branch target=1`
        f.push_op(MirOp::std("cssl.branch").with_attribute("target", "1"));
        // Block 1 : `%0 = arith.constant 7 ; func.return %0`
        let mut blk1 = MirBlock::new("ret");
        blk1.ops.push(
            MirOp::std("arith.constant")
                .with_attribute("value", "7")
                .with_result(ValueId(0), MirType::Int(IntWidth::I32)),
        );
        blk1.ops
            .push(MirOp::std("func.return").with_operand(ValueId(0)));
        f.body.push(blk1);
        let mut module = MirModule::new();
        module.push_func(f);
        let bytes = emit_object_module(&module).expect("two-block jump emit ok");
        assert!(bytes.starts_with(magic_prefix(host_default_format())));
    }

    /// `fn pick_branch(cond : bool) -> i32 {
    ///   block 0 (entry) : brif cond -> block 1 / block 2
    ///   block 1         : return 42
    ///   block 2         : return 0
    /// }`
    #[test]
    fn if_else_two_branches_compile() {
        use cssl_mir::MirBlock;
        let mut f = MirFunc::new(
            "pick_branch",
            vec![MirType::Bool],
            vec![MirType::Int(IntWidth::I32)],
        );
        f.next_value_id = 3;
        // Entry op : `cssl.brif (%0=cond) then=1 else=2`
        f.push_op(
            MirOp::std("cssl.brif")
                .with_operand(ValueId(0))
                .with_attribute("then_target", "1")
                .with_attribute("else_target", "2")
                .with_attribute("then_arg_count", "0")
                .with_attribute("else_arg_count", "0"),
        );
        // Then-block : `%1 = arith.constant 42 ; return %1`
        let mut then_blk = MirBlock::new("then");
        then_blk.ops.push(
            MirOp::std("arith.constant")
                .with_attribute("value", "42")
                .with_result(ValueId(1), MirType::Int(IntWidth::I32)),
        );
        then_blk
            .ops
            .push(MirOp::std("func.return").with_operand(ValueId(1)));
        f.body.push(then_blk);
        // Else-block : `%2 = arith.constant 0 ; return %2`
        let mut else_blk = MirBlock::new("else");
        else_blk.ops.push(
            MirOp::std("arith.constant")
                .with_attribute("value", "0")
                .with_result(ValueId(2), MirType::Int(IntWidth::I32)),
        );
        else_blk
            .ops
            .push(MirOp::std("func.return").with_operand(ValueId(2)));
        f.body.push(else_blk);
        let mut module = MirModule::new();
        module.push_func(f);
        let bytes = emit_object_module(&module).expect("if-else two-branch emit ok");
        assert!(bytes.starts_with(magic_prefix(host_default_format())));
    }

    /// `fn while_body(cond : bool, x : i32) -> i32 {
    ///   block 0 (entry)  : jump header (block 1)
    ///   block 1 (header) : brif cond -> body (block 2) / exit (block 3)
    ///   block 2 (body)   : jump header (block 1)        ; back-edge
    ///   block 3 (exit)   : return x
    /// }`
    /// Tests classic while-loop SSA shape with a back-edge.
    #[test]
    fn while_loop_compiles() {
        use cssl_mir::MirBlock;
        let mut f = MirFunc::new(
            "while_body",
            vec![MirType::Bool, MirType::Int(IntWidth::I32)],
            vec![MirType::Int(IntWidth::I32)],
        );
        f.next_value_id = 2;
        // Entry op : `cssl.branch target=1` (jump to header).
        f.push_op(MirOp::std("cssl.branch").with_attribute("target", "1"));
        // Block 1 (header) : `cssl.brif (%0=cond) then=2 else=3`.
        let mut header = MirBlock::new("header");
        header.ops.push(
            MirOp::std("cssl.brif")
                .with_operand(ValueId(0))
                .with_attribute("then_target", "2")
                .with_attribute("else_target", "3")
                .with_attribute("then_arg_count", "0")
                .with_attribute("else_arg_count", "0"),
        );
        f.body.push(header);
        // Block 2 (body) : back-edge to header.
        let mut body = MirBlock::new("body");
        body.ops
            .push(MirOp::std("cssl.branch").with_attribute("target", "1"));
        f.body.push(body);
        // Block 3 (exit) : `func.return %1=x`.
        let mut exit = MirBlock::new("exit");
        exit.ops
            .push(MirOp::std("func.return").with_operand(ValueId(1)));
        f.body.push(exit);
        let mut module = MirModule::new();
        module.push_func(f);
        let bytes = emit_object_module(&module).expect("while-loop emit ok");
        assert!(bytes.starts_with(magic_prefix(host_default_format())));
    }

    /// `fn passthrough(x : i32) -> i32 {
    ///   block 0 (entry) : jump block 1 forwarding %x
    ///   block 1 (tail)  : block-arg %1 : i32 ; return %1
    /// }`
    /// Verifies that block-param SSA values plumb across cssl.branch edges.
    #[test]
    fn block_args_pass_through() {
        use cssl_mir::{MirBlock, MirValue};
        let mut f = MirFunc::new(
            "passthrough",
            vec![MirType::Int(IntWidth::I32)],
            vec![MirType::Int(IntWidth::I32)],
        );
        f.next_value_id = 2;
        // Entry op : `cssl.branch target=1 forward=%0`. The destination
        // block's block-arg (%1) is the receiving SSA value.
        f.push_op(
            MirOp::std("cssl.branch")
                .with_operand(ValueId(0))
                .with_attribute("target", "1"),
        );
        // Block 1 (tail) : args=[%1 : i32], ops = `func.return %1`.
        let mut tail = MirBlock::new("tail");
        tail.args
            .push(MirValue::new(ValueId(1), MirType::Int(IntWidth::I32)));
        tail.ops
            .push(MirOp::std("func.return").with_operand(ValueId(1)));
        f.body.push(tail);
        let mut module = MirModule::new();
        module.push_func(f);
        let bytes = emit_object_module(&module).expect("block-args pass-through emit ok");
        assert!(bytes.starts_with(magic_prefix(host_default_format())));
    }

    /// MALFORMED : a non-entry block that ends without a terminator should
    /// produce `BlockMissingTerminator`. The single-block fallback still
    /// implicit-returns for the entry block, but for N>1 blocks every
    /// block must be explicitly terminated.
    #[test]
    fn multi_block_without_terminator_errors() {
        use cssl_mir::MirBlock;
        let mut f = MirFunc::new("bad_term", vec![], vec![]);
        f.push_op(MirOp::std("cssl.branch").with_attribute("target", "1"));
        // Block 1 : empty (no terminator). Multi-block-strict :
        // BlockMissingTerminator.
        f.body.push(MirBlock::new("dangler"));
        let mut module = MirModule::new();
        module.push_func(f);
        let r = emit_object_module(&module);
        assert!(
            matches!(r, Err(ObjectError::BlockMissingTerminator { .. })),
            "expected BlockMissingTerminator ; got {r:?}"
        );
    }

    /// MALFORMED : a `cssl.branch` whose `target=N` references a
    /// nonexistent block should produce `BlockTargetOutOfRange`.
    #[test]
    fn cssl_branch_with_invalid_target_errors() {
        let mut f = MirFunc::new("bad_target", vec![], vec![]);
        f.push_op(MirOp::std("cssl.branch").with_attribute("target", "99"));
        let mut module = MirModule::new();
        module.push_func(f);
        let r = emit_object_module(&module);
        assert!(
            matches!(r, Err(ObjectError::BlockTargetOutOfRange { target_idx: 99, .. })),
            "expected BlockTargetOutOfRange{{target_idx:99}} ; got {r:?}"
        );
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

    // ─────────────────────────────────────────────────────────────────────
    // § T11-D100 (J2 — closures callable) — Object-emit `cssl.closure.call`
    // marker + `cssl.closure.call.error` dispatch tests.
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn obj_emit_closure_call_marker_succeeds() {
        // The marker op is emitted upstream by body_lower's
        // lower_closure_call ; backend treats it as a no-op binder. Use a
        // canonical hand-built layout :
        //   %0 = arith.constant 14 : i32   # the inlined body's yield
        //   %1 = cssl.closure.call (%0) yield_value_id=0  -> i32
        //   func.return %1
        let mut f = MirFunc::new("call_marker", vec![], vec![MirType::Int(IntWidth::I32)]);
        f.push_op(
            MirOp::std("arith.constant")
                .with_result(ValueId(0), MirType::Int(IntWidth::I32))
                .with_attribute("value", "14"),
        );
        f.push_op(
            MirOp::std("cssl.closure.call")
                .with_operand(ValueId(0))
                .with_result(ValueId(1), MirType::Int(IntWidth::I32))
                .with_attribute("param_count", "0")
                .with_attribute("capture_count", "0")
                .with_attribute("env_size", "0")
                .with_attribute("env_align", "8")
                .with_attribute("yield_value_id", "0"),
        );
        f.push_op(MirOp::std("func.return").with_operand(ValueId(1)));
        let mut module = MirModule::new();
        module.push_func(f);
        let bytes = emit_object_module(&module).expect("emit ok");
        assert!(bytes.starts_with(magic_prefix(host_default_format())));
    }

    #[test]
    fn obj_emit_closure_call_marker_no_yield_id_is_no_op() {
        // The marker without a `yield_value_id` attribute is a pure no-op ;
        // the function returns void cleanly.
        let mut f = MirFunc::new("call_marker_void", vec![], vec![]);
        f.push_op(
            MirOp::std("cssl.closure.call")
                .with_attribute("param_count", "0")
                .with_attribute("capture_count", "0")
                .with_attribute("env_size", "0")
                .with_attribute("env_align", "8"),
        );
        f.push_op(MirOp::std("func.return"));
        let mut module = MirModule::new();
        module.push_func(f);
        let bytes = emit_object_module(&module).expect("emit ok");
        assert!(bytes.starts_with(magic_prefix(host_default_format())));
    }

    #[test]
    fn obj_emit_closure_call_with_unknown_yield_value_id_errors() {
        // The marker references a yield_value_id that wasn't bound by any
        // upstream op ⇒ UnknownValueId at the binder step.
        let mut f = MirFunc::new("bad_marker", vec![], vec![MirType::Int(IntWidth::I32)]);
        f.push_op(
            MirOp::std("cssl.closure.call")
                .with_result(ValueId(0), MirType::Int(IntWidth::I32))
                .with_attribute("yield_value_id", "999"),
        );
        f.push_op(MirOp::std("func.return").with_operand(ValueId(0)));
        let mut module = MirModule::new();
        module.push_func(f);
        let r = emit_object_module(&module);
        assert!(matches!(r, Err(ObjectError::UnknownValueId { .. })));
    }

    #[test]
    fn obj_emit_closure_call_error_op_lowers_to_zero_sentinel() {
        // The error op binds its result-id to a typed-zero ptr sentinel ;
        // surrounding ops execute cleanly. A function that consists purely
        // of the error op + return should object-emit without complaint.
        let mut f = MirFunc::new("call_err", vec![], vec![MirType::Ptr]);
        f.push_op(
            MirOp::std("cssl.closure.call.error")
                .with_result(ValueId(0), MirType::Ptr)
                .with_attribute("detail", "arity mismatch (test)")
                .with_attribute("expected_arity", "1")
                .with_attribute("actual_arity", "0"),
        );
        f.push_op(MirOp::std("func.return").with_operand(ValueId(0)));
        let mut module = MirModule::new();
        module.push_func(f);
        let bytes = emit_object_module(&module).expect("error op should object-emit");
        assert!(bytes.starts_with(magic_prefix(host_default_format())));
    }

    #[test]
    fn obj_emit_closure_call_with_capture_reload_chain_succeeds() {
        // Full end-to-end MIR shape that body_lower produces for a single-
        // capture closure call : env-pack at construct-site + capture-reload
        // memref.load at the call site + the marker. Hand-built to verify
        // the Object backend lowers the full chain cleanly.
        //
        // fn caller(y : i64) -> i64 {
        //   ; § construct site
        //   %sz   = arith.constant 8 : i64
        //   %al   = arith.constant 8 : i64
        //   %env  = cssl.heap.alloc(%sz, %al) -> !cssl.ptr
        //   %off  = arith.constant 0 : i64
        //   memref.store y, %env, %off
        //   %clos = cssl.closure(%y, %env)         ; capture_count=1
        //   ; § call site (inlined)
        //   %ro   = arith.constant 0 : i64
        //   %cap  = memref.load %env, %ro          ; origin=closure_capture_reload
        //   %arg  = arith.constant 5 : i64
        //   %sum  = arith.addi %arg, %cap          ; the inlined body : x + y
        //   %res  = cssl.closure.call(%clos, %arg) yield_value_id=%sum -> i64
        //   func.return %res
        // }
        use cssl_mir::{MirBlock, MirRegion};
        let i64_ty = MirType::Int(IntWidth::I64);
        let mut f = MirFunc::new("caller", vec![i64_ty.clone()], vec![i64_ty.clone()]);
        f.push_op(
            MirOp::std("arith.constant")
                .with_result(ValueId(1), i64_ty.clone())
                .with_attribute("value", "8"),
        );
        f.push_op(
            MirOp::std("arith.constant")
                .with_result(ValueId(2), i64_ty.clone())
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
                .with_result(ValueId(4), i64_ty.clone())
                .with_attribute("value", "0"),
        );
        f.push_op(
            MirOp::std("memref.store")
                .with_operand(ValueId(0))
                .with_operand(ValueId(3))
                .with_operand(ValueId(4))
                .with_attribute("alignment", "8"),
        );
        // cssl.closure construct.
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
        // Call-site capture-reload.
        f.push_op(
            MirOp::std("arith.constant")
                .with_result(ValueId(6), i64_ty.clone())
                .with_attribute("value", "0"),
        );
        f.push_op(
            MirOp::std("memref.load")
                .with_operand(ValueId(3))
                .with_operand(ValueId(6))
                .with_result(ValueId(7), i64_ty.clone())
                .with_attribute("alignment", "8")
                .with_attribute("origin", "closure_capture_reload"),
        );
        f.push_op(
            MirOp::std("arith.constant")
                .with_result(ValueId(8), i64_ty.clone())
                .with_attribute("value", "5"),
        );
        f.push_op(
            MirOp::std("arith.addi")
                .with_operand(ValueId(8))
                .with_operand(ValueId(7))
                .with_result(ValueId(9), i64_ty.clone()),
        );
        f.push_op(
            MirOp::std("cssl.closure.call")
                .with_operand(ValueId(5))
                .with_operand(ValueId(8))
                .with_result(ValueId(10), i64_ty)
                .with_attribute("param_count", "1")
                .with_attribute("capture_count", "1")
                .with_attribute("env_size", "8")
                .with_attribute("env_align", "8")
                .with_attribute("yield_value_id", "9"),
        );
        f.push_op(MirOp::std("func.return").with_operand(ValueId(10)));
        let mut module = MirModule::new();
        module.push_func(f);
        let bytes = emit_object_module(&module).expect("full closure-call chain must object-emit");
        assert!(bytes.starts_with(magic_prefix(host_default_format())));
    }

    // ─────────────────────────────────────────────────────────────────────
    // § T11-CC-2 (W-CC-funccall) — `func.call` lowering tests.
    //
    // Each test exercises a distinct facet of the CalleeImports + body-
    // lowering surface :
    //   1. extern_call_one_arg_one_result   — Linkage::Import path + 1-arg
    //                                         + i32-result binding.
    //   2. extern_call_no_args              — empty operand list, i32 result.
    //   3. extern_call_no_result            — void callsite (no result-bind).
    //   4. multi_call_same_callee_one_funcref
    //                                       — repeated callsites share one
    //                                         FuncRef declaration.
    //   5. intra_module_call                — sibling fn defined locally
    //                                         resolves through fn_table
    //                                         (no Linkage::Import).
    // Functional verification (link + run) lives in cssl-examples ; here we
    // assert the emit-pipeline accepts the MIR + returns valid object bytes
    // with the host magic prefix.
    // ─────────────────────────────────────────────────────────────────────

    /// Helper : build a `main` fn that calls `<callee>(<arg_const>)` with one
    /// i32 arg + binds the i32 result + returns it. Used by the extern_call_*
    /// fixtures so each test focuses on its own structural facet.
    fn build_caller_one_i32_arg(callee: &str, arg_value: i64) -> MirFunc {
        let i32_ty = MirType::Int(IntWidth::I32);
        let mut f = MirFunc::new("main", vec![], vec![i32_ty.clone()]);
        f.push_op(
            MirOp::std("arith.constant")
                .with_attribute("value", arg_value.to_string())
                .with_result(ValueId(0), i32_ty.clone()),
        );
        f.push_op(
            MirOp::std("func.call")
                .with_attribute("callee", callee)
                .with_operand(ValueId(0))
                .with_result(ValueId(1), i32_ty),
        );
        f.push_op(MirOp::std("func.return").with_operand(ValueId(1)));
        f
    }

    #[test]
    fn extern_call_one_arg_one_result() {
        // fn main() -> i32 { add42(13) }   where add42 : (i32) -> i32 is extern.
        // The callee is NOT defined in the module ; pre-scan must declare it
        // as Linkage::Import using the callsite-derived signature.
        let main_fn = build_caller_one_i32_arg("add42", 13);
        let mut module = MirModule::new();
        module.push_func(main_fn);
        let bytes = emit_object_module(&module).expect("emit extern call ok");
        assert!(
            bytes.starts_with(magic_prefix(host_default_format())),
            "extern_call output must carry the host object magic"
        );
        // Sanity : object body is non-trivial — a `call` instruction carries
        // a relocation entry which guarantees byte-volume above the empty
        // module floor (validated against control runs).
        assert!(bytes.len() > 100, "extern_call object too small : {}", bytes.len());
    }

    #[test]
    fn extern_call_no_args() {
        // fn main() -> i32 { host_get_42() }   host_get_42 : () -> i32 extern.
        // Validates the zero-operand path through CalleeImports + the call
        // instruction emission with an empty arg-list.
        let i32_ty = MirType::Int(IntWidth::I32);
        let mut main_fn = MirFunc::new("main", vec![], vec![i32_ty.clone()]);
        main_fn.push_op(
            MirOp::std("func.call")
                .with_attribute("callee", "host_get_42")
                .with_result(ValueId(0), i32_ty),
        );
        main_fn.push_op(MirOp::std("func.return").with_operand(ValueId(0)));
        let mut module = MirModule::new();
        module.push_func(main_fn);
        let bytes = emit_object_module(&module).expect("emit no-arg extern call ok");
        assert!(bytes.starts_with(magic_prefix(host_default_format())));
    }

    #[test]
    fn extern_call_no_result() {
        // fn main() { host_emit_event(7) }   host_emit_event : (i32) -> ()
        // Pure side-effect call : no result on the func.call op ; main itself
        // returns void. Validates the result-binding skip path in
        // `obj_lower_func_call`.
        let i32_ty = MirType::Int(IntWidth::I32);
        let mut main_fn = MirFunc::new("main", vec![], vec![]);
        main_fn.push_op(
            MirOp::std("arith.constant")
                .with_attribute("value", "7")
                .with_result(ValueId(0), i32_ty),
        );
        main_fn.push_op(
            MirOp::std("func.call")
                .with_attribute("callee", "host_emit_event")
                .with_operand(ValueId(0)),
        );
        main_fn.push_op(MirOp::std("func.return"));
        let mut module = MirModule::new();
        module.push_func(main_fn);
        let bytes = emit_object_module(&module).expect("emit void extern call ok");
        assert!(bytes.starts_with(magic_prefix(host_default_format())));
    }

    #[test]
    fn multi_call_same_callee_one_funcref() {
        // fn main() -> i32 { let a = side_effect_inc(0); let b = side_effect_inc(a); b }
        // Two `func.call`s naming the same extern callee. `declare_callee_imports_for_fn`
        // pre-scans uniquely so `side_effect_inc` is declared exactly once at
        // the module level + its `FuncRef` is shared by both call-sites. We
        // verify by running the emit pipeline (it would error
        // `IncompatibleSignature` or similar if double-declared).
        let i32_ty = MirType::Int(IntWidth::I32);
        let mut main_fn = MirFunc::new("main", vec![], vec![i32_ty.clone()]);
        main_fn.push_op(
            MirOp::std("arith.constant")
                .with_attribute("value", "0")
                .with_result(ValueId(0), i32_ty.clone()),
        );
        main_fn.push_op(
            MirOp::std("func.call")
                .with_attribute("callee", "side_effect_inc")
                .with_operand(ValueId(0))
                .with_result(ValueId(1), i32_ty.clone()),
        );
        main_fn.push_op(
            MirOp::std("func.call")
                .with_attribute("callee", "side_effect_inc")
                .with_operand(ValueId(1))
                .with_result(ValueId(2), i32_ty),
        );
        main_fn.push_op(MirOp::std("func.return").with_operand(ValueId(2)));
        let mut module = MirModule::new();
        module.push_func(main_fn);
        let bytes =
            emit_object_module(&module).expect("emit multi-call sharing one FuncRef ok");
        assert!(bytes.starts_with(magic_prefix(host_default_format())));
    }

    #[test]
    fn intra_module_call() {
        // fn helper(x : i32) -> i32 { x }   ; pass-through, body : return %0.
        // fn main() -> i32 { helper(42) }
        // Resolves `helper` through the fn_table populated by pass-1 of
        // `emit_object_module_with_format` ; NOT declared as Linkage::Import.
        // Validates the intra-module path inside `declare_callee_imports_for_fn`.
        let i32_ty = MirType::Int(IntWidth::I32);
        let mut helper = MirFunc::new("helper", vec![i32_ty.clone()], vec![i32_ty.clone()]);
        // helper's entry block has arg ValueId(0) wired by MirFunc::new.
        helper.push_op(MirOp::std("func.return").with_operand(ValueId(0)));

        let mut main_fn = MirFunc::new("main", vec![], vec![i32_ty.clone()]);
        main_fn.push_op(
            MirOp::std("arith.constant")
                .with_attribute("value", "42")
                .with_result(ValueId(0), i32_ty.clone()),
        );
        main_fn.push_op(
            MirOp::std("func.call")
                .with_attribute("callee", "helper")
                .with_operand(ValueId(0))
                .with_result(ValueId(1), i32_ty),
        );
        main_fn.push_op(MirOp::std("func.return").with_operand(ValueId(1)));

        let mut module = MirModule::new();
        // Push helper FIRST so pass-1 can declare it before main's body
        // pre-scan walks. (Pass-1 declares all fns regardless of order so
        // the reverse case also works ; this ordering matches what
        // body_lower emits in source order.)
        module.push_func(helper);
        module.push_func(main_fn);
        let bytes = emit_object_module(&module).expect("emit intra-module call ok");
        assert!(bytes.starts_with(magic_prefix(host_default_format())));
    }

    #[test]
    fn intra_module_call_reverse_decl_order() {
        // Stronger guarantee : `main` is pushed BEFORE `helper`, but the
        // pass-1 declare-all sweep means main's pre-scan still finds the
        // helper FuncId in fn_table. Source-order independence matters
        // because body_lower may not always emit callees first (mutual
        // recursion would break a single-pass design — this test pins the
        // 2-pass shape down).
        let i32_ty = MirType::Int(IntWidth::I32);
        let mut helper = MirFunc::new("helper", vec![i32_ty.clone()], vec![i32_ty.clone()]);
        helper.push_op(MirOp::std("func.return").with_operand(ValueId(0)));

        let mut main_fn = MirFunc::new("main", vec![], vec![i32_ty.clone()]);
        main_fn.push_op(
            MirOp::std("arith.constant")
                .with_attribute("value", "9")
                .with_result(ValueId(0), i32_ty.clone()),
        );
        main_fn.push_op(
            MirOp::std("func.call")
                .with_attribute("callee", "helper")
                .with_operand(ValueId(0))
                .with_result(ValueId(1), i32_ty),
        );
        main_fn.push_op(MirOp::std("func.return").with_operand(ValueId(1)));

        let mut module = MirModule::new();
        module.push_func(main_fn); // FORWARD-decl order : caller before callee.
        module.push_func(helper);
        let bytes = emit_object_module(&module).expect("emit reverse-order intra call ok");
        assert!(bytes.starts_with(magic_prefix(host_default_format())));
    }

    #[test]
    fn func_call_missing_callee_attribute_errors() {
        // Defensive : a `func.call` op without a `callee` attribute is
        // malformed MIR. The pre-scan tolerates it (skips the op for
        // import-declaration), but `obj_lower_func_call` MUST surface a
        // descriptive error rather than panic-or-skip.
        let i32_ty = MirType::Int(IntWidth::I32);
        let mut f = MirFunc::new("malformed", vec![], vec![i32_ty.clone()]);
        f.push_op(
            MirOp::std("func.call")
                .with_result(ValueId(0), i32_ty),
        );
        f.push_op(MirOp::std("func.return").with_operand(ValueId(0)));
        let mut module = MirModule::new();
        module.push_func(f);
        let r = emit_object_module(&module);
        assert!(
            matches!(
                r,
                Err(ObjectError::LoweringFailed { ref detail, .. })
                    if detail.contains("missing `callee`")
            ),
            "expected `missing callee` LoweringFailed ; got {r:?}"
        );
    }

    // ─────────────────────────────────────────────────────────────────────
    // § T11-D318 (W-CC-mut-assign) — object-emit support for the new
    //   `cssl.local.alloca` op + the two-region `scf.while` shape with a
    //   re-walked cond_region. The MIR fixture mirrors what `body_lower`
    //   emits for `let mut frame ; while frame < 60 { frame = frame + 1
    //   } ; frame`. Object-emit cannot exercise the runtime semantics
    //   directly (no JIT linker in this crate) — the JIT-side
    //   `jit_let_mut_while_loop_returns_60` test covers runtime
    //   correctness. Here we verify the byte-shape : the produced
    //   object file has the host magic prefix and no UnsupportedOp /
    //   panic from the new op-handlers.
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn emit_let_mut_frame_counter_object_shape_succeeds() {
        use cssl_mir::{MirBlock, MirRegion};
        let i32t = || MirType::Int(IntWidth::I32);
        let bool_t = || MirType::Bool;
        // Same MIR shape as `hand_built_let_mut_frame_counter` in jit.rs.
        let mut f = MirFunc::new("frame_counter_obj", vec![], vec![i32t()]);
        f.next_value_id = 13;
        {
            let entry = f.body.entry_mut().unwrap();
            entry.args = vec![];
            // alloca + init store.
            entry.ops.push(
                MirOp::std("cssl.local.alloca")
                    .with_result(ValueId(0), MirType::Ptr)
                    .with_attribute("slot_ty", "i32"),
            );
            entry.ops.push(
                MirOp::std("arith.constant")
                    .with_result(ValueId(1), i32t())
                    .with_attribute("value", "0"),
            );
            entry.ops.push(
                MirOp::std("memref.store")
                    .with_operand(ValueId(1))
                    .with_operand(ValueId(0)),
            );
            // Backward-compat outer-block cond computation (legacy operand).
            entry.ops.push(
                MirOp::std("memref.load")
                    .with_operand(ValueId(0))
                    .with_result(ValueId(3), i32t()),
            );
            entry.ops.push(
                MirOp::std("arith.constant")
                    .with_result(ValueId(2), i32t())
                    .with_attribute("value", "60"),
            );
            entry.ops.push(
                MirOp::std("arith.cmpi_slt")
                    .with_operand(ValueId(3))
                    .with_operand(ValueId(2))
                    .with_result(ValueId(4), bool_t()),
            );
            // Cond region (re-walked at each header).
            let mut cond_blk = MirBlock::new("entry");
            cond_blk.ops.push(
                MirOp::std("memref.load")
                    .with_operand(ValueId(0))
                    .with_result(ValueId(5), i32t()),
            );
            cond_blk.ops.push(
                MirOp::std("arith.constant")
                    .with_result(ValueId(6), i32t())
                    .with_attribute("value", "60"),
            );
            cond_blk.ops.push(
                MirOp::std("arith.cmpi_slt")
                    .with_operand(ValueId(5))
                    .with_operand(ValueId(6))
                    .with_result(ValueId(7), bool_t()),
            );
            cond_blk
                .ops
                .push(MirOp::std("scf.condition").with_operand(ValueId(7)));
            let mut cond_region = MirRegion::new();
            cond_region.push(cond_blk);
            // Body region (mutates the cell).
            let mut body_blk = MirBlock::new("entry");
            body_blk.ops.push(
                MirOp::std("memref.load")
                    .with_operand(ValueId(0))
                    .with_result(ValueId(8), i32t()),
            );
            body_blk.ops.push(
                MirOp::std("arith.constant")
                    .with_result(ValueId(9), i32t())
                    .with_attribute("value", "1"),
            );
            body_blk.ops.push(
                MirOp::std("arith.addi")
                    .with_operand(ValueId(8))
                    .with_operand(ValueId(9))
                    .with_result(ValueId(10), i32t()),
            );
            body_blk.ops.push(
                MirOp::std("memref.store")
                    .with_operand(ValueId(10))
                    .with_operand(ValueId(0)),
            );
            let mut body_region = MirRegion::new();
            body_region.push(body_blk);
            // scf.while with two regions.
            entry.ops.push(
                MirOp::std("scf.while")
                    .with_operand(ValueId(4))
                    .with_region(cond_region)
                    .with_region(body_region)
                    .with_result(ValueId(11), MirType::None),
            );
            // Trailing read + return.
            entry.ops.push(
                MirOp::std("memref.load")
                    .with_operand(ValueId(0))
                    .with_result(ValueId(12), i32t()),
            );
            entry
                .ops
                .push(MirOp::std("func.return").with_operand(ValueId(12)));
        }
        let mut module = MirModule::new();
        module.push_func(f);
        let bytes = emit_object_module(&module)
            .expect("frame_counter_obj must object-emit via cssl.local.alloca + scf.while two-region");
        assert!(
            bytes.starts_with(magic_prefix(host_default_format())),
            "object header magic should match host platform"
        );
        assert!(!bytes.is_empty(), "produced bytes should be non-empty");
    }
}
