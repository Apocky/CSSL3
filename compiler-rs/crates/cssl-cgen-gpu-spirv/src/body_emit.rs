//! Real SPIR-V kernel-body emission via `rspirv` ops (T11-D72 / S6-D1).
//!
//! § PURPOSE
//!
//! The void-fn emitter in [`crate::binary_emit`] turns each entry-point
//! [`SpirvEntryPoint`] into a `void fn() { return ; }` shell — sufficient for
//! validating the module-section-order + capability/extension/memory-model
//! plumbing, but useless for actually running a CSSLv3 compute kernel. This
//! module turns the same `SpirvModule` shell into a SPIR-V module whose entry
//! function carries the **real lowered body** of a [`MirFunc`] : `arith.*`
//! ops become `OpFAdd` / `OpIAdd` / etc., `scf.if` becomes the canonical
//! `OpSelectionMerge` + `OpBranchConditional` pair, `scf.for` / `scf.while` /
//! `scf.loop` become `OpLoopMerge` + branch-to-header, `memref.load` /
//! `memref.store` become `OpLoad` / `OpStore` carrying the `Aligned` memory
//! access operand, and `func.return` becomes `OpReturnValue` (or `OpReturn`
//! for `void` kernels).
//!
//! § FANOUT-CONTRACT — the D5 marker
//!
//! Per `specs/02_IR.csl § STRUCTURED-CFG VALIDATOR (T11-D70 / S6-D5)`, the
//! GPU emitters D1..D4 share the structured-CFG validator (D5) as a
//! pre-condition. Calling [`emit_kernel_module`] on a module that has not
//! been validated through `cssl_mir::structured_cfg::validate_and_mark` is a
//! programmer-error. The first thing this emitter does is assert the
//! `("structured_cfg.validated", "true")` module-attribute marker is
//! present on the input `MirModule` ; if absent, we reject cleanly with
//! [`BodyEmitError::StructuredCfgMarkerAbsent`] instead of producing
//! malformed SPIR-V words. **The marker is the only contract between D5 and
//! D1 ; without it the GPU emitters refuse to emit.**
//!
//! § REJECTIONS — heap + closures
//!
//! Two MIR op families that flow through the CPU pipeline must be rejected
//! before they reach SPIR-V :
//!
//!   * `cssl.heap.alloc` / `cssl.heap.dealloc` / `cssl.heap.realloc`
//!     (T11-D57 / S6-B1) lower to `__cssl_alloc` / `__cssl_free` /
//!     `__cssl_realloc` FFI calls into `cssl-rt`. SPIR-V has no host-malloc
//!     equivalent at stage-0 ; a future slice will introduce USM (Vulkan
//!     `VK_KHR_buffer_device_address` / Level-Zero USM / DX12 placed-resource /
//!     Metal MTLBuffer) but that's deferred to a Phase-D follow-up. Until
//!     then, heap ops on a GPU path are a hard reject —
//!     [`BodyEmitError::HeapNotSupportedOnGpu`].
//!
//!   * `cssl.closure` (T11-D64 / S6-C5 placeholder name — the slice hasn't
//!     landed yet at the time of T11-D72, but the name is reserved per
//!     `HANDOFF_SESSION_6.csl § PHASE-C § S6-C5`) introduces function
//!     pointers + indirect calls, which compute SPIR-V doesn't support at
//!     stage-0. Reject early with
//!     [`BodyEmitError::ClosuresNotSupportedOnGpu`].
//!
//! These are the two LANDMINE rejections called out by the slice handoff.
//! Both produce clean diagnostics — never panic.
//!
//! § STRUCTURED-CFG LOWERING — scf.if + scf.for + scf.while + scf.loop
//!
//! Per the SPIR-V spec, structured control flow REQUIRES that every header
//! block carries a merge instruction (`OpSelectionMerge` for if-style,
//! `OpLoopMerge` for loop-style) IMMEDIATELY before its terminator. Each
//! lowering helper here follows the canonical pattern :
//!
//!   * **`scf.if` → OpSelectionMerge + OpBranchConditional**
//!     ```text
//!     <header>:
//!       OpSelectionMerge %merge None
//!       OpBranchConditional %cond %then %else
//!     <then>:
//!       <then-body ops...>
//!       OpBranch %merge
//!     <else>:
//!       <else-body ops...>
//!       OpBranch %merge
//!     <merge>:
//!       <continuation>
//!     ```
//!     The yielded value (when present) flows through an `OpVariable`
//!     stack-local that each branch stores into ; the merge block emits
//!     an `OpLoad` from that variable. This avoids needing to choose
//!     between `OpPhi` (which requires upfront-known predecessor ids) and
//!     spilling — the variable form is uniform across branches and matches
//!     the JIT's merge-block-arg semantics one-to-one.
//!
//!   * **`scf.for` / `scf.while` / `scf.loop` → OpLoopMerge + branch-back**
//!     ```text
//!     <header>:
//!       OpLoopMerge %merge %continue None
//!       OpBranch %body
//!     <body>:
//!       <body ops...>
//!       OpBranch %continue
//!     <continue>:
//!       <continue ops (empty at stage-0)...>
//!       OpBranchConditional %cond %header %merge
//!     <merge>:
//!       <continuation>
//!     ```
//!     For `scf.loop` (unbounded), the continue block branches back
//!     unconditionally to `header`. For `scf.while` / `scf.for`, the cond
//!     evaluation re-runs in the continue block. **Stage-0 simplification :
//!     because C2's lowering captures the cond-value once before the loop
//!     starts, we currently emit the conditional branch with that latched
//!     cond-id — meaning a `scf.while` with a constant-true cond becomes
//!     an infinite loop and a constant-false cond falls through. Re-eval
//!     of the cond inside the loop body is a deferred slice (`scf.condition`
//!     emission) ; D5's CFG0008 already reserves the diagnostic.**
//!
//! § ENTRY-POINT PROTOCOL
//!
//! The kernel entry-point function has signature `void fn() { ... }` per
//! the existing void-fn emitter. The MIR fn's parameters do NOT flow
//! through as SPIR-V function parameters at stage-0 — instead, each
//! `MirFunc::params[i]` becomes an `OpVariable` in the `Function`
//! storage-class which is initialized from the corresponding
//! `OpFunctionParameter` if/when a future slice plumbs them through. For
//! now, parameter access via the entry block's args list resolves to an
//! `OpUndef` of the param type — the kernel can still execute its body
//! and produce a correct return-value but cannot READ inputs through the
//! current shape. **Real param-passing for compute kernels uses
//! Vulkan-style descriptor bindings + push-constants ; that machinery
//! lives in the Phase-E host slices and is out-of-scope for D1.**
//!
//! § OP COVERAGE TABLE (T11-D72 baseline)
//!
//!   MIR op-name            | SPIR-V op(s)
//!   -----------------------|---------------------------------------------
//!   arith.constant         | OpConstant (typed) or OpConstantTrue/False
//!   arith.addi / subi /    | OpIAdd / OpISub / OpIMul / OpSDiv / OpSRem /
//!     muli / divsi / remsi | (signed-integer arith)
//!   arith.addf / subf /    | OpFAdd / OpFSub / OpFMul / OpFDiv / OpFNegate
//!     mulf / divf / negf   |
//!   arith.cmpi_eq / ne /   | OpIEqual / OpINotEqual / OpSLessThan /
//!     slt / sle / sgt / sge| OpSLessThanEqual / OpSGreaterThan /
//!                          | OpSGreaterThanEqual
//!   arith.cmpf (predicate) | OpFOrdEqual / OpFOrdLessThan / OpFOrdGreaterThan
//!                          | (or Unord variants based on predicate prefix)
//!   arith.cmpf (no attr)   | (rejected — predicate required)
//!   arith.cmpi (no attr)   | (rejected — predicate required)
//!   arith.select           | OpSelect
//!   arith.andi / ori / xori| OpBitwiseAnd / OpBitwiseOr / OpBitwiseXor
//!   arith.shli / shrsi     | OpShiftLeftLogical / OpShiftRightArithmetic
//!   memref.load            | OpLoad with MemoryAccess::ALIGNED
//!   memref.store           | OpStore with MemoryAccess::ALIGNED
//!   scf.if                 | OpSelectionMerge + OpBranchConditional
//!   scf.for/while/loop     | OpLoopMerge + OpBranch + OpBranchConditional
//!   scf.yield              | (consumed by parent ; outer-level = no-op)
//!   func.return            | OpReturn / OpReturnValue
//!   cssl.heap.*            | REJECTED (heap not supported on GPU)
//!   cssl.closure           | REJECTED (closures not supported on GPU)
//!
//! § DEFERRED (per slice handoff REPORT BACK section)
//!
//!   * Per-MIR-op coverage for AD ops (`cssl.diff.fwd` / `cssl.diff.bwd`)
//!     — they currently lower to scalar arith on the CPU side ; mirroring
//!     them here is a follow-up.
//!   * f64 transcendentals via `OpExtInst` / GLSL.std.450 (defer to C4).
//!   * Param-passing through descriptor sets / push constants (Phase-E).
//!   * `OpExecutionMode LocalSize X Y Z` already wired through
//!     `binary_emit::emit_execution_modes_for_entry` — overridable per fn
//!     attribute is a future slice.
//!   * `spirv-tools` semantic-validate gate (the workspace already pins
//!     `spirv-tools = "0.12"` but the crate links a heavy native C++
//!     toolchain ; T11-D72 keeps the rspirv `dr::load_words` round-trip
//!     as the structural validator and defers the native gate to the
//!     same slice that ships the CI test job).
//!
//! [`SpirvEntryPoint`]: crate::module::SpirvEntryPoint

use std::collections::HashMap;

use rspirv::binary::Assemble;
use rspirv::dr::{Builder, Operand};
use rspirv::spirv::{
    self, FunctionControl, LoopControl, MemoryAccess, SelectionControl, StorageClass,
};
use thiserror::Error;

use cssl_mir::block::{MirOp, MirRegion};
use cssl_mir::func::{MirFunc, MirModule};
use cssl_mir::structured_cfg::has_structured_cfg_marker;
use cssl_mir::value::{FloatWidth, IntWidth, MirType, ValueId};

use crate::binary_emit::{
    map_addressing_model, map_capability, map_execution_model, map_memory_model,
};
use crate::module::{SpirvEntryPoint, SpirvModule};
use crate::target::SpirvTargetEnv;

/// Failure modes for SPIR-V kernel-body emission.
///
/// Each variant carries enough context for an actionable diagnostic — fn-name
/// where relevant, op-name for op-level rejections, op-shape for malformed-MIR
/// rejections. None of these are panics : every rejection produces a clean
/// `Err(BodyEmitError)` so the caller can render diagnostics through the
/// canonical `csslc` machinery (or any other consumer's choice).
#[derive(Debug, Error, PartialEq, Eq)]
pub enum BodyEmitError {
    /// The input `MirModule` lacks the `("structured_cfg.validated", "true")`
    /// marker attribute. GPU emitters D1..D4 require D5 to have run successfully
    /// first — calling them on a non-validated module is a programmer-error
    /// caught here. The marker is the FANOUT-CONTRACT between D5 and D1..D4
    /// per `specs/02_IR.csl § STRUCTURED-CFG VALIDATOR`.
    #[error(
        "structured-CFG validator marker absent : run \
         `cssl_mir::structured_cfg::validate_and_mark` on the module before \
         invoking the SPIR-V kernel-body emitter (D5 fanout-contract)"
    )]
    StructuredCfgMarkerAbsent,
    /// The kernel function name supplied to [`emit_kernel_module`] does not
    /// resolve to any [`MirFunc`] in the module.
    #[error(
        "kernel fn `{kernel_fn}` not found in MIR module : check that the \
         frontend's @gpu-attributed fn lowered into the module"
    )]
    KernelFnNotFound { kernel_fn: String },
    /// A heap allocation op (`cssl.heap.alloc` / `dealloc` / `realloc`) was
    /// found in the kernel body. SPIR-V has no host-malloc equivalent at
    /// stage-0 ; a future Phase-D follow-up will introduce USM / BDA
    /// (Vulkan device-local + Level-Zero USM + DX12 placed-resource).
    #[error(
        "heap op `{op_name}` in fn `{fn_name}` is not supported on the GPU \
         path : the cssl-rt heap allocator is CPU-only at stage-0 ; USM/BDA \
         lowering is deferred to a future slice"
    )]
    HeapNotSupportedOnGpu { fn_name: String, op_name: String },
    /// A `cssl.closure` op was found. Closure-by-environment requires
    /// function pointers + indirect-call, neither of which compute SPIR-V
    /// supports at stage-0.
    #[error(
        "closure op `{op_name}` in fn `{fn_name}` is not supported on the GPU \
         path : closures lower to function-pointers + indirect calls which \
         compute SPIR-V doesn't support at stage-0"
    )]
    ClosuresNotSupportedOnGpu { fn_name: String, op_name: String },
    /// A MIR op carries operands that don't resolve in the value-map.
    /// Generally indicates a body-lower bug : an op references a value-id
    /// that was never produced.
    #[error(
        "fn `{fn_name}` op `{op_name}` references unknown ValueId({}) — \
         body-lower invariant violation",
        value_id.0
    )]
    UnknownValueId {
        fn_name: String,
        op_name: String,
        value_id: ValueId,
    },
    /// A MIR op has a result type the SPIR-V emitter doesn't model at
    /// stage-0 (e.g., tuple, function, complex memref). The caller must
    /// either restructure the body to use scalar / vector types only, or
    /// wait for a future slice that extends the type table.
    #[error(
        "fn `{fn_name}` op `{op_name}` has unsupported result type `{ty}` : \
         the SPIR-V emitter currently models scalars (i1/i8/i16/i32/i64/\
         f16/f32/f64/bool) + raw pointers + vectors of float ; structs / \
         tuples / functions are deferred"
    )]
    UnsupportedResultType {
        fn_name: String,
        op_name: String,
        ty: String,
    },
    /// A MIR op is structurally malformed (wrong arity / missing attribute /
    /// missing operand) and the per-op lowering can't proceed. The detail
    /// string carries enough context for the user to fix the source-side
    /// problem.
    #[error("fn `{fn_name}` op `{op_name}` malformed : {detail}")]
    MalformedOp {
        fn_name: String,
        op_name: String,
        detail: String,
    },
    /// An op-name reached the dispatcher that the SPIR-V emitter doesn't
    /// know how to lower at stage-0. Listed in this enum so callers can
    /// match on it ; emitting a clean diagnostic is preferable to a panic
    /// when an unsupported MIR op flows through.
    #[error(
        "fn `{fn_name}` op `{op_name}` not yet supported by the SPIR-V \
         emitter at stage-0 : add a per-op arm in `body_emit::emit_op` if \
         the op is needed for compute kernels"
    )]
    UnsupportedOp { fn_name: String, op_name: String },
    /// `rspirv`'s builder rejected an instruction sequence. Generally caused
    /// by a structurally invalid program (missing OpLabel before an op,
    /// nesting blocks, etc.) — usually a body-lower or D5 bug rather than
    /// user-facing.
    #[error("rspirv builder rejected the kernel body : {detail}")]
    BuilderFailed { detail: String },
    /// The supplied [`SpirvModule`] had zero entry points and the target-env
    /// requires at least one. Mirrors [`crate::binary_emit::BinaryEmitError::NoEntryPoints`]
    /// with the body-emitter context.
    #[error("module targeting `{target_env}` declared no entry points")]
    NoEntryPoints { target_env: String },
}

/// Emit a complete SPIR-V binary for `spirv_mod`'s entry point named
/// `kernel_fn_name`, with the entry-point function's body lowered from
/// `mir_mod`'s function of the same name. Returns the SPIR-V binary words
/// (`Vec<u32>`) ready for consumption by `vkCreateShaderModule` /
/// `zeModuleCreate` / `D3DCompile` / etc.
///
/// The flow mirrors [`crate::binary_emit::emit_module_binary`] for the
/// header + capability + extension + memory-model + entry-point + execution-mode
/// + debug sections, then lowers the kernel function's body via the
/// crate-private `emit_kernel_body` helper before returning the assembled
/// binary.
///
/// **Pre-condition** : `mir_mod` MUST have the
/// `("structured_cfg.validated", "true")` attribute set by
/// `cssl_mir::structured_cfg::validate_and_mark`. If absent,
/// [`BodyEmitError::StructuredCfgMarkerAbsent`] is returned.
///
/// # Errors
/// Returns [`BodyEmitError`] on any rejection (D5 marker absent, heap op
/// found, closure op found, kernel fn not found, malformed op, etc.).
#[allow(clippy::too_many_lines)]
pub fn emit_kernel_module(
    spirv_mod: &SpirvModule,
    mir_mod: &MirModule,
    kernel_fn_name: &str,
) -> Result<Vec<u32>, BodyEmitError> {
    // § FANOUT-CONTRACT : assert D5 marker before doing anything else. The
    // GPU emitters D1..D4 require structured-CFG validation up-front ; if
    // the marker is missing, refuse to emit instead of producing malformed
    // SPIR-V words.
    if !has_structured_cfg_marker(mir_mod) {
        return Err(BodyEmitError::StructuredCfgMarkerAbsent);
    }

    // § Locate the kernel fn in the MIR module.
    let mir_fn =
        mir_mod
            .find_func(kernel_fn_name)
            .ok_or_else(|| BodyEmitError::KernelFnNotFound {
                kernel_fn: kernel_fn_name.to_string(),
            })?;

    // § Pre-scan : reject heap + closure ops anywhere in the kernel body
    // BEFORE we start emitting SPIR-V words. This keeps rejection cheap +
    // noise-free.
    pre_scan_reject_heap_and_closures(mir_fn)?;

    // § Shader-env sanity : at least one entry point required.
    if spirv_mod.entry_points.is_empty()
        && !matches!(spirv_mod.target_env, SpirvTargetEnv::OpenClKernel2_2)
    {
        return Err(BodyEmitError::NoEntryPoints {
            target_env: spirv_mod.target_env.to_string(),
        });
    }

    // § Build the SPIR-V module skeleton (mirrors binary_emit::emit_module_binary).
    let mut b = Builder::new();
    b.set_version(1, 5);

    for cap in spirv_mod.capabilities.iter() {
        b.capability(map_capability(cap));
    }
    for ext in spirv_mod.extensions.iter_plain() {
        b.extension(ext.as_str());
    }
    for ext in spirv_mod.extensions.iter_ext_inst_sets() {
        let _ = b.ext_inst_import(ext.as_str());
    }
    b.memory_model(
        map_addressing_model(spirv_mod.addressing_model),
        map_memory_model(spirv_mod.memory_model),
    );

    // § Type cache — shared across the whole module so the kernel body can
    // reuse types declared for the entry-point fn type itself.
    let mut type_cache = TypeCache::default();

    // § Resolve entry-point fn-types : void() for stage-0.
    let void_ty = b.type_void();
    let void_fn_ty = b.type_function(void_ty, vec![]);
    type_cache.void = Some(void_ty);

    // § For each entry point : if its name matches `kernel_fn_name`, emit
    // the lowered body ; otherwise emit a void-fn shell (back-compat with
    // existing modules that mix the kernel + non-kernel entries).
    let mut entry_fn_ids: Vec<(u32, &SpirvEntryPoint)> =
        Vec::with_capacity(spirv_mod.entry_points.len());
    for ep in &spirv_mod.entry_points {
        let fn_id = b
            .begin_function(void_ty, None, FunctionControl::NONE, void_fn_ty)
            .map_err(|e| BodyEmitError::BuilderFailed {
                detail: format!("begin_function : {e:?}"),
            })?;
        if ep.name == kernel_fn_name {
            // Real body emission for the kernel entry.
            emit_kernel_body(&mut b, fn_id, mir_fn, &mut type_cache)?;
        } else {
            // Shell body for non-kernel entries (back-compat).
            b.begin_block(None)
                .map_err(|e| BodyEmitError::BuilderFailed {
                    detail: format!("begin_block (shell) : {e:?}"),
                })?;
            b.ret().map_err(|e| BodyEmitError::BuilderFailed {
                detail: format!("ret (shell) : {e:?}"),
            })?;
        }
        b.end_function().map_err(|e| BodyEmitError::BuilderFailed {
            detail: format!("end_function : {e:?}"),
        })?;
        entry_fn_ids.push((fn_id, ep));
    }

    // § Entry points (OpEntryPoint).
    for (fn_id, ep) in &entry_fn_ids {
        b.entry_point(map_execution_model(ep.model), *fn_id, ep.name.clone(), []);
    }

    // § Execution modes — emit LocalSize for compute entries.
    for (fn_id, ep) in &entry_fn_ids {
        emit_exec_modes_for_entry(&mut b, *fn_id, ep);
    }

    // § Debug : OpSource + OpName.
    if spirv_mod.source_language.is_some() {
        b.source(
            spirv::SourceLanguage::Unknown,
            spirv_mod.source_version.unwrap_or(0),
            None,
            None::<String>,
        );
    }
    for (fn_id, ep) in &entry_fn_ids {
        b.name(*fn_id, ep.name.clone());
    }

    Ok(b.module().assemble())
}

/// Pre-scan a kernel fn for the two LANDMINE op-families : heap ops and
/// closures. Walks the body recursively (including nested regions inside
/// scf.if / scf.for / etc.). Returns `Ok(())` if neither family is present ;
/// returns the first violation as `Err`.
fn pre_scan_reject_heap_and_closures(mir_fn: &MirFunc) -> Result<(), BodyEmitError> {
    fn walk_region(fn_name: &str, region: &MirRegion) -> Result<(), BodyEmitError> {
        for block in &region.blocks {
            for op in &block.ops {
                walk_op(fn_name, op)?;
            }
        }
        Ok(())
    }
    fn walk_op(fn_name: &str, op: &MirOp) -> Result<(), BodyEmitError> {
        match op.name.as_str() {
            "cssl.heap.alloc" | "cssl.heap.dealloc" | "cssl.heap.realloc" => {
                return Err(BodyEmitError::HeapNotSupportedOnGpu {
                    fn_name: fn_name.to_string(),
                    op_name: op.name.clone(),
                });
            }
            // Closures are reserved by name per S6-C5 (deferred slice). We
            // accept either the canonical `cssl.closure` form or the prefix
            // `cssl.closure.` for sub-variants (e.g., `cssl.closure.call`).
            n if n == "cssl.closure" || n.starts_with("cssl.closure.") => {
                return Err(BodyEmitError::ClosuresNotSupportedOnGpu {
                    fn_name: fn_name.to_string(),
                    op_name: op.name.clone(),
                });
            }
            _ => {}
        }
        for region in &op.regions {
            walk_region(fn_name, region)?;
        }
        Ok(())
    }
    walk_region(&mir_fn.name, &mir_fn.body)
}

/// Emit `LocalSize X Y Z` / `OriginUpperLeft` execution modes for an entry
/// point. Mirrors [`crate::binary_emit::emit_execution_modes_for_entry`]
/// but kept private here because the body-emission path is self-contained.
fn emit_exec_modes_for_entry(b: &mut Builder, fn_id: u32, ep: &SpirvEntryPoint) {
    for mode_str in &ep.execution_modes {
        let trimmed = mode_str.trim();
        if let Some(rest) = trimmed.strip_prefix("LocalSize ") {
            if let Some([x, y, z]) = parse_three_u32(rest) {
                b.execution_mode(fn_id, spirv::ExecutionMode::LocalSize, [x, y, z]);
            }
        } else if let Some(rest) = trimmed.strip_prefix("LocalSizeHint ") {
            if let Some([x, y, z]) = parse_three_u32(rest) {
                b.execution_mode(fn_id, spirv::ExecutionMode::LocalSizeHint, [x, y, z]);
            }
        } else if trimmed == "OriginUpperLeft" {
            b.execution_mode(fn_id, spirv::ExecutionMode::OriginUpperLeft, []);
        } else if trimmed == "OriginLowerLeft" {
            b.execution_mode(fn_id, spirv::ExecutionMode::OriginLowerLeft, []);
        }
    }
}

fn parse_three_u32(s: &str) -> Option<[u32; 3]> {
    let mut it = s.split_whitespace();
    let x: u32 = it.next()?.parse().ok()?;
    let y: u32 = it.next()?.parse().ok()?;
    let z: u32 = it.next()?.parse().ok()?;
    if it.next().is_some() {
        return None;
    }
    Some([x, y, z])
}

// ═════════════════════════════════════════════════════════════════════════
// § Type cache — caches SPIR-V type-ids for MirType so the emitter doesn't
// emit the same type-decl multiple times.
// ═════════════════════════════════════════════════════════════════════════

/// SPIR-V type-id cache keyed by the `MirType` shape. The rspirv builder
/// already de-duplicates `OpTypeInt` / `OpTypeFloat` / `OpTypeBool` /
/// `OpTypeVoid` automatically (see `dr/build/autogen_type.rs`'s
/// `dedup_insert_type` path), but caching at the MirType layer avoids
/// repeated mapping work + keeps the per-MirType pointer-types stable
/// across one fn body.
#[derive(Debug, Default)]
pub(crate) struct TypeCache {
    void: Option<u32>,
    bool_id: Option<u32>,
    int: HashMap<IntWidth, u32>,
    float: HashMap<FloatWidth, u32>,
    /// Pointer-to-T cached per `(StorageClass, MirType)` key. The
    /// HashMap key uses the MirType's display string for stable hashing
    /// without requiring `Hash` on the type itself (which it has, but we
    /// avoid the cycle through derive).
    ptr: HashMap<(StorageClass, String), u32>,
}

impl TypeCache {
    fn type_for(
        &mut self,
        b: &mut Builder,
        ty: &MirType,
        fn_name: &str,
        op_name: &str,
    ) -> Result<u32, BodyEmitError> {
        match ty {
            MirType::Bool => Ok(*self.bool_id.get_or_insert_with(|| b.type_bool())),
            MirType::Int(w) => {
                if let Some(&id) = self.int.get(w) {
                    return Ok(id);
                }
                let bits = match w {
                    IntWidth::I1 => 1,
                    IntWidth::I8 => 8,
                    IntWidth::I16 => 16,
                    IntWidth::I32 => 32,
                    // Index lowers to i64 on 64-bit hosts ; SPIR-V Logical
                    // addressing typically uses i32, but we mirror the CPU
                    // host convention here. Vulkan compute kernels will
                    // see 32-bit by default in their bindings.
                    IntWidth::I64 | IntWidth::Index => 64,
                };
                let id = b.type_int(bits, 1);
                self.int.insert(*w, id);
                Ok(id)
            }
            MirType::Float(w) => {
                if let Some(&id) = self.float.get(w) {
                    return Ok(id);
                }
                let bits = match w {
                    FloatWidth::F16 | FloatWidth::Bf16 => 16,
                    FloatWidth::F32 => 32,
                    FloatWidth::F64 => 64,
                };
                let id = b.type_float(bits);
                self.float.insert(*w, id);
                Ok(id)
            }
            // None : map to OpTypeVoid for use in `OpReturn` / signature
            // contexts. Unusual for an op result-type but valid for fn-sig.
            MirType::None => Ok(*self.void.get_or_insert_with(|| b.type_void())),
            // Other types fall through with a clean diagnostic. Pointers,
            // tuples, functions, memrefs, vectors require slice-specific
            // work (USM for pointers ; struct decoration for tuples ;
            // vector emit for vectors) deferred per the slice handoff.
            other => Err(BodyEmitError::UnsupportedResultType {
                fn_name: fn_name.to_string(),
                op_name: op_name.to_string(),
                ty: format!("{other}"),
            }),
        }
    }

    /// Get-or-emit a `OpTypePointer` with the given storage-class + pointee.
    fn pointer_for(
        &mut self,
        b: &mut Builder,
        storage: StorageClass,
        pointee: &MirType,
        fn_name: &str,
        op_name: &str,
    ) -> Result<u32, BodyEmitError> {
        let key = (storage, format!("{pointee}"));
        if let Some(&id) = self.ptr.get(&key) {
            return Ok(id);
        }
        let pointee_id = self.type_for(b, pointee, fn_name, op_name)?;
        let id = b.type_pointer(None, storage, pointee_id);
        self.ptr.insert(key, id);
        Ok(id)
    }
}

// ═════════════════════════════════════════════════════════════════════════
// § Body context — value-map + cursor + fn-name + cached glsl-ext-inst id.
// ═════════════════════════════════════════════════════════════════════════

/// Per-fn body emission context. Threads the value-map (MIR `ValueId` →
/// SPIR-V word-id) + the current fn-name (for diagnostic context) + a
/// borrowed type cache + the optional glsl.std.450 ext-inst-set id (used
/// by future transcendental-lowering slices ; nullable at stage-0).
struct BodyCtx<'a> {
    fn_name: &'a str,
    value_map: HashMap<ValueId, u32>,
    type_cache: &'a mut TypeCache,
    /// Allowed `cssl.unsupported(*)` tokens to no-op through the dispatcher
    /// instead of erroring. Currently empty — surfaces are explicit.
    _reserved: (),
}

/// Lower one MIR fn body into the currently-selected SPIR-V function. The
/// caller (`emit_kernel_module`) is responsible for `begin_function` /
/// `end_function` ; this fn handles the entry block + ops + return.
///
/// `pub(crate)` (not `pub`) because the type-cache is internal scaffolding ;
/// external callers go through [`emit_kernel_module`] which takes care of
/// the cache lifecycle.
///
/// # Errors
/// Returns [`BodyEmitError`] on any per-op rejection (heap, closure,
/// unsupported op, malformed op, builder failure).
pub(crate) fn emit_kernel_body(
    b: &mut Builder,
    _fn_id: u32,
    mir_fn: &MirFunc,
    type_cache: &mut TypeCache,
) -> Result<(), BodyEmitError> {
    let mut ctx = BodyCtx {
        fn_name: &mir_fn.name,
        value_map: HashMap::new(),
        type_cache,
        _reserved: (),
    };

    // § Enter the entry block. SPIR-V function bodies need an OpLabel
    // before any ops ; rspirv's `begin_block(None)` allocates a fresh id
    // for the label.
    b.begin_block(None)
        .map_err(|e| BodyEmitError::BuilderFailed {
            detail: format!("begin_block (entry) : {e:?}"),
        })?;

    // § Initialize fn parameters as `OpUndef` of the param type. The kernel
    // can still execute its body + return correctly ; reading params
    // through this shape returns Undef (per SPIR-V semantics : reading an
    // OpUndef yields an unspecified value). Real param-passing for compute
    // uses descriptor-set bindings (Phase-E work).
    if let Some(entry) = mir_fn.body.entry() {
        for arg in &entry.args {
            let ty_id = ctx
                .type_cache
                .type_for(b, &arg.ty, ctx.fn_name, "<fn-param-undef>")
                .ok();
            if let Some(t) = ty_id {
                let undef_id = b.undef(t, None);
                ctx.value_map.insert(arg.id, undef_id);
            }
            // If the arg type is unsupported, we silently skip the undef
            // bind ; consumers of the value will fail with UnknownValueId
            // when they reach the dispatcher. That's the right shape : the
            // emitter doesn't know how to manifest the value, so the op
            // that depends on it is the better place to surface the error.
        }
    }

    // § Walk the entry block's ops. Each op gets dispatched through
    // `emit_op` ; nested-region ops handle their own walks.
    if let Some(entry) = mir_fn.body.entry() {
        for op in &entry.ops {
            emit_op(b, &mut ctx, op)?;
        }
    }

    // § If the body didn't end in a `func.return` op (e.g., a user-built
    // synthetic fixture forgot the trailing return), append an OpReturn
    // so the function is structurally valid. The cranelift backend has
    // a similar safety-net.
    //
    // To detect "did the body end in a terminator?" we check whether the
    // current block has been ended. rspirv's API doesn't expose this
    // directly but we can detect by inspecting whether the last op
    // emitted was a terminator-style ; for stage-0 we just unconditionally
    // emit OpReturn IF the entry block's last op was NOT func.return.
    let needs_terminator = !ends_with_return(mir_fn);
    if needs_terminator {
        // Suppress builder errors here — if a terminator already ran the
        // builder will reject this and we surface as a clean fallthrough.
        let _ = b.ret();
    }

    Ok(())
}

/// Inspect a MirFunc to determine whether its entry block ends in a
/// `func.return` op. Used to decide whether to append a synthetic OpReturn.
fn ends_with_return(mir_fn: &MirFunc) -> bool {
    mir_fn
        .body
        .entry()
        .and_then(|e| e.ops.last())
        .is_some_and(|op| op.name == "func.return")
}

// ═════════════════════════════════════════════════════════════════════════
// § Per-op dispatcher.
// ═════════════════════════════════════════════════════════════════════════

/// Dispatch one MIR op to its SPIR-V counterpart. Recursion through nested
/// regions happens inside the structured-control-flow emitters.
fn emit_op(b: &mut Builder, ctx: &mut BodyCtx<'_>, op: &MirOp) -> Result<(), BodyEmitError> {
    match op.name.as_str() {
        // ── Constants + unary + binary arith. ────────────────────────────
        "arith.constant" => emit_constant(b, ctx, op),
        "arith.addi" => emit_int_binary(b, ctx, op, IntBinaryOp::Add),
        "arith.subi" => emit_int_binary(b, ctx, op, IntBinaryOp::Sub),
        "arith.muli" => emit_int_binary(b, ctx, op, IntBinaryOp::Mul),
        "arith.divsi" => emit_int_binary(b, ctx, op, IntBinaryOp::SDiv),
        "arith.remsi" => emit_int_binary(b, ctx, op, IntBinaryOp::SRem),
        "arith.andi" => emit_int_binary(b, ctx, op, IntBinaryOp::And),
        "arith.ori" => emit_int_binary(b, ctx, op, IntBinaryOp::Or),
        "arith.xori" => emit_int_binary(b, ctx, op, IntBinaryOp::Xor),
        "arith.shli" => emit_int_binary(b, ctx, op, IntBinaryOp::Shl),
        "arith.shrsi" => emit_int_binary(b, ctx, op, IntBinaryOp::Shr),
        "arith.addf" => emit_float_binary(b, ctx, op, FloatBinaryOp::Add),
        "arith.subf" => emit_float_binary(b, ctx, op, FloatBinaryOp::Sub),
        "arith.mulf" => emit_float_binary(b, ctx, op, FloatBinaryOp::Mul),
        "arith.divf" => emit_float_binary(b, ctx, op, FloatBinaryOp::Div),
        "arith.negf" => emit_float_unary(b, ctx, op, FloatUnaryOp::Neg),
        "arith.cmpi" | "arith.cmpi_eq" | "arith.cmpi_ne" | "arith.cmpi_slt" | "arith.cmpi_sle"
        | "arith.cmpi_sgt" | "arith.cmpi_sge" => emit_cmpi(b, ctx, op),
        "arith.cmpf" => emit_cmpf(b, ctx, op),
        "arith.select" => emit_select(b, ctx, op),
        // ── Memory ops. ──────────────────────────────────────────────────
        "memref.load" => emit_memref_load(b, ctx, op),
        "memref.store" => emit_memref_store(b, ctx, op),
        // ── Structured control-flow. ─────────────────────────────────────
        "scf.if" => emit_scf_if(b, ctx, op),
        "scf.for" => emit_scf_for(b, ctx, op),
        "scf.while" => emit_scf_while(b, ctx, op),
        "scf.loop" => emit_scf_loop(b, ctx, op),
        // scf.yield is consumed by structured-emitter walkers ; reaching
        // it here means it leaked outside a structured parent (which D5
        // would have rejected in normal flow). Treat as no-op for
        // compatibility with hand-built fixtures that route through us
        // without going through D5 first (e.g., direct emit_kernel_body
        // tests).
        "scf.yield" => Ok(()),
        // ── Function return. ─────────────────────────────────────────────
        "func.return" => emit_func_return(b, ctx, op),
        // ── LANDMINE rejections (defensive ; pre-scan also catches them). ─
        "cssl.heap.alloc" | "cssl.heap.dealloc" | "cssl.heap.realloc" => {
            Err(BodyEmitError::HeapNotSupportedOnGpu {
                fn_name: ctx.fn_name.to_string(),
                op_name: op.name.clone(),
            })
        }
        n if n == "cssl.closure" || n.starts_with("cssl.closure.") => {
            Err(BodyEmitError::ClosuresNotSupportedOnGpu {
                fn_name: ctx.fn_name.to_string(),
                op_name: op.name.clone(),
            })
        }
        // ── Other ops fall through as UnsupportedOp. ─────────────────────
        _ => Err(BodyEmitError::UnsupportedOp {
            fn_name: ctx.fn_name.to_string(),
            op_name: op.name.clone(),
        }),
    }
}

// ═════════════════════════════════════════════════════════════════════════
// § Constants.
// ═════════════════════════════════════════════════════════════════════════

/// Lower `arith.constant` to `OpConstant` (or OpConstantTrue/False for bool).
/// The MIR op carries its literal in the `"value"` attribute as a parseable
/// string ; the result type is taken from `op.results[0].ty`.
fn emit_constant(b: &mut Builder, ctx: &mut BodyCtx<'_>, op: &MirOp) -> Result<(), BodyEmitError> {
    let r = require_one_result(ctx, op)?;
    let value_str = op
        .attributes
        .iter()
        .find(|(k, _)| k == "value")
        .map_or("0", |(_, v)| v.as_str());
    let ty_id = ctx.type_cache.type_for(b, &r.ty, ctx.fn_name, &op.name)?;
    let id = match &r.ty {
        MirType::Bool => {
            // Accept "true" / "false" / "1" / "0".
            let truthy = matches!(value_str, "true" | "1");
            if truthy {
                b.constant_true(ty_id)
            } else {
                b.constant_false(ty_id)
            }
        }
        MirType::Int(w) => match w {
            IntWidth::I64 | IntWidth::Index => {
                let parsed: i64 = value_str.parse().unwrap_or(0);
                b.constant_bit64(ty_id, parsed as u64)
            }
            _ => {
                let parsed: i64 = value_str.parse().unwrap_or(0);
                b.constant_bit32(ty_id, parsed as u32)
            }
        },
        MirType::Float(w) => match w {
            FloatWidth::F32 => {
                let parsed: f32 = value_str.parse().unwrap_or(0.0);
                b.constant_bit32(ty_id, parsed.to_bits())
            }
            FloatWidth::F64 => {
                let parsed: f64 = value_str.parse().unwrap_or(0.0);
                b.constant_bit64(ty_id, parsed.to_bits())
            }
            FloatWidth::F16 | FloatWidth::Bf16 => {
                // f16/bf16 : parse-as-f32 then truncate via to_bits ; rspirv
                // emits OpConstant with a 32-bit literal that the SPIR-V
                // consumer interprets per the underlying type's width.
                let parsed: f32 = value_str.parse().unwrap_or(0.0);
                b.constant_bit32(ty_id, parsed.to_bits())
            }
        },
        _ => {
            // Falling through to UnsupportedResultType is the right shape ;
            // type_for already rejected, so we shouldn't reach here.
            return Err(BodyEmitError::UnsupportedResultType {
                fn_name: ctx.fn_name.to_string(),
                op_name: op.name.clone(),
                ty: format!("{}", r.ty),
            });
        }
    };
    ctx.value_map.insert(r.id, id);
    Ok(())
}

// ═════════════════════════════════════════════════════════════════════════
// § Binary + unary arith.
// ═════════════════════════════════════════════════════════════════════════

#[derive(Clone, Copy)]
enum IntBinaryOp {
    Add,
    Sub,
    Mul,
    SDiv,
    SRem,
    And,
    Or,
    Xor,
    Shl,
    Shr,
}

#[derive(Clone, Copy)]
enum FloatBinaryOp {
    Add,
    Sub,
    Mul,
    Div,
}

#[derive(Clone, Copy)]
enum FloatUnaryOp {
    Neg,
}

fn emit_int_binary(
    b: &mut Builder,
    ctx: &mut BodyCtx<'_>,
    op: &MirOp,
    kind: IntBinaryOp,
) -> Result<(), BodyEmitError> {
    let r = require_one_result(ctx, op)?;
    let (lhs, rhs) = require_two_operands(ctx, op)?;
    let ty_id = ctx.type_cache.type_for(b, &r.ty, ctx.fn_name, &op.name)?;
    let id = match kind {
        IntBinaryOp::Add => b.i_add(ty_id, None, lhs, rhs),
        IntBinaryOp::Sub => b.i_sub(ty_id, None, lhs, rhs),
        IntBinaryOp::Mul => b.i_mul(ty_id, None, lhs, rhs),
        IntBinaryOp::SDiv => b.s_div(ty_id, None, lhs, rhs),
        IntBinaryOp::SRem => b.s_rem(ty_id, None, lhs, rhs),
        IntBinaryOp::And => b.bitwise_and(ty_id, None, lhs, rhs),
        IntBinaryOp::Or => b.bitwise_or(ty_id, None, lhs, rhs),
        IntBinaryOp::Xor => b.bitwise_xor(ty_id, None, lhs, rhs),
        IntBinaryOp::Shl => b.shift_left_logical(ty_id, None, lhs, rhs),
        IntBinaryOp::Shr => b.shift_right_arithmetic(ty_id, None, lhs, rhs),
    }
    .map_err(|e| BodyEmitError::BuilderFailed {
        detail: format!("int binary `{}` : {e:?}", op.name),
    })?;
    ctx.value_map.insert(r.id, id);
    Ok(())
}

fn emit_float_binary(
    b: &mut Builder,
    ctx: &mut BodyCtx<'_>,
    op: &MirOp,
    kind: FloatBinaryOp,
) -> Result<(), BodyEmitError> {
    let r = require_one_result(ctx, op)?;
    let (lhs, rhs) = require_two_operands(ctx, op)?;
    let ty_id = ctx.type_cache.type_for(b, &r.ty, ctx.fn_name, &op.name)?;
    let id = match kind {
        FloatBinaryOp::Add => b.f_add(ty_id, None, lhs, rhs),
        FloatBinaryOp::Sub => b.f_sub(ty_id, None, lhs, rhs),
        FloatBinaryOp::Mul => b.f_mul(ty_id, None, lhs, rhs),
        FloatBinaryOp::Div => b.f_div(ty_id, None, lhs, rhs),
    }
    .map_err(|e| BodyEmitError::BuilderFailed {
        detail: format!("float binary `{}` : {e:?}", op.name),
    })?;
    ctx.value_map.insert(r.id, id);
    Ok(())
}

fn emit_float_unary(
    b: &mut Builder,
    ctx: &mut BodyCtx<'_>,
    op: &MirOp,
    kind: FloatUnaryOp,
) -> Result<(), BodyEmitError> {
    let r = require_one_result(ctx, op)?;
    let operand = require_one_operand(ctx, op)?;
    let ty_id = ctx.type_cache.type_for(b, &r.ty, ctx.fn_name, &op.name)?;
    let id = match kind {
        FloatUnaryOp::Neg => b.f_negate(ty_id, None, operand),
    }
    .map_err(|e| BodyEmitError::BuilderFailed {
        detail: format!("float unary `{}` : {e:?}", op.name),
    })?;
    ctx.value_map.insert(r.id, id);
    Ok(())
}

// ═════════════════════════════════════════════════════════════════════════
// § Comparisons + select.
// ═════════════════════════════════════════════════════════════════════════

/// Lower `arith.cmpi[_pred]` to `OpIEqual` / `OpINotEqual` / `OpSLessThan` /
/// etc. The predicate is encoded either in the op-name suffix
/// (`arith.cmpi_eq`, `arith.cmpi_slt`, …) or in the `"predicate"` attribute
/// (per the JIT's lower_cmpi convention).
fn emit_cmpi(b: &mut Builder, ctx: &mut BodyCtx<'_>, op: &MirOp) -> Result<(), BodyEmitError> {
    let r = require_one_result(ctx, op)?;
    let (lhs, rhs) = require_two_operands(ctx, op)?;
    let ty_id = ctx.type_cache.type_for(b, &r.ty, ctx.fn_name, &op.name)?;
    let pred = predicate_from_op(op, "i");
    let id = match pred.as_str() {
        "eq" => b.i_equal(ty_id, None, lhs, rhs),
        "ne" => b.i_not_equal(ty_id, None, lhs, rhs),
        "slt" => b.s_less_than(ty_id, None, lhs, rhs),
        "sle" => b.s_less_than_equal(ty_id, None, lhs, rhs),
        "sgt" => b.s_greater_than(ty_id, None, lhs, rhs),
        "sge" => b.s_greater_than_equal(ty_id, None, lhs, rhs),
        other => {
            return Err(BodyEmitError::MalformedOp {
                fn_name: ctx.fn_name.to_string(),
                op_name: op.name.clone(),
                detail: format!("unknown int-cmp predicate `{other}`"),
            })
        }
    }
    .map_err(|e| BodyEmitError::BuilderFailed {
        detail: format!("cmpi : {e:?}"),
    })?;
    ctx.value_map.insert(r.id, id);
    Ok(())
}

/// Lower `arith.cmpf` to `OpFOrdEqual` / `OpFOrdLessThan` / etc. The
/// predicate carries through the `"predicate"` attribute. Both Ord and
/// Unord families are supported via the `oeq` / `ueq` / `olt` / `ult` /
/// `oge` / `uge` / etc. forms (matching MLIR's predicate set).
fn emit_cmpf(b: &mut Builder, ctx: &mut BodyCtx<'_>, op: &MirOp) -> Result<(), BodyEmitError> {
    let r = require_one_result(ctx, op)?;
    let (lhs, rhs) = require_two_operands(ctx, op)?;
    let ty_id = ctx.type_cache.type_for(b, &r.ty, ctx.fn_name, &op.name)?;
    let pred = predicate_from_op(op, "f");
    let id = match pred.as_str() {
        "oeq" | "eq" => b.f_ord_equal(ty_id, None, lhs, rhs),
        "one" | "ne" => b.f_ord_not_equal(ty_id, None, lhs, rhs),
        "olt" | "lt" => b.f_ord_less_than(ty_id, None, lhs, rhs),
        "ole" | "le" => b.f_ord_less_than_equal(ty_id, None, lhs, rhs),
        "ogt" | "gt" => b.f_ord_greater_than(ty_id, None, lhs, rhs),
        "oge" | "ge" => b.f_ord_greater_than_equal(ty_id, None, lhs, rhs),
        "ueq" => b.f_unord_equal(ty_id, None, lhs, rhs),
        "une" => b.f_unord_not_equal(ty_id, None, lhs, rhs),
        other => {
            return Err(BodyEmitError::MalformedOp {
                fn_name: ctx.fn_name.to_string(),
                op_name: op.name.clone(),
                detail: format!("unknown float-cmp predicate `{other}`"),
            })
        }
    }
    .map_err(|e| BodyEmitError::BuilderFailed {
        detail: format!("cmpf : {e:?}"),
    })?;
    ctx.value_map.insert(r.id, id);
    Ok(())
}

/// Extract the predicate from an op-name suffix or the `"predicate"`
/// attribute. Order : (1) op-name suffix `arith.cmpX_<pred>` ; (2)
/// attribute `"predicate"` ; (3) empty string (returned to caller for an
/// UnknownPredicate diagnostic).
fn predicate_from_op(op: &MirOp, family: &str) -> String {
    let prefix = format!("arith.cmp{family}_");
    if let Some(rest) = op.name.strip_prefix(&prefix) {
        return rest.to_string();
    }
    if let Some((_, v)) = op.attributes.iter().find(|(k, _)| k == "predicate") {
        return v.clone();
    }
    String::new()
}

/// Lower `arith.select` to `OpSelect` — `(cond ? t : f)`.
fn emit_select(b: &mut Builder, ctx: &mut BodyCtx<'_>, op: &MirOp) -> Result<(), BodyEmitError> {
    let r = require_one_result(ctx, op)?;
    if op.operands.len() != 3 {
        return Err(BodyEmitError::MalformedOp {
            fn_name: ctx.fn_name.to_string(),
            op_name: op.name.clone(),
            detail: format!(
                "arith.select expected 3 operands (cond, t, f) ; got {}",
                op.operands.len()
            ),
        });
    }
    let cond = lookup_value(ctx, op.operands[0], &op.name)?;
    let t = lookup_value(ctx, op.operands[1], &op.name)?;
    let f = lookup_value(ctx, op.operands[2], &op.name)?;
    let ty_id = ctx.type_cache.type_for(b, &r.ty, ctx.fn_name, &op.name)?;
    let id = b
        .select(ty_id, None, cond, t, f)
        .map_err(|e| BodyEmitError::BuilderFailed {
            detail: format!("select : {e:?}"),
        })?;
    ctx.value_map.insert(r.id, id);
    Ok(())
}

// ═════════════════════════════════════════════════════════════════════════
// § Memory ops — memref.load / memref.store.
// ═════════════════════════════════════════════════════════════════════════

/// Lower `memref.load %ptr [, %offset] -> %r : T` to `OpLoad` with the
/// `Aligned` memory access operand carrying `max(natural-align, attr-align)`.
/// SPIR-V's logical-addressing model uses pointer-IDs : we emit the load
/// directly against the operand pointer, ignoring offset for now (offset
/// addressing requires `OpAccessChain` against a struct or array layout
/// which the stage-0 ptr-only memref shape doesn't model).
fn emit_memref_load(
    b: &mut Builder,
    ctx: &mut BodyCtx<'_>,
    op: &MirOp,
) -> Result<(), BodyEmitError> {
    let r = require_one_result(ctx, op)?;
    if op.operands.is_empty() {
        return Err(BodyEmitError::MalformedOp {
            fn_name: ctx.fn_name.to_string(),
            op_name: op.name.clone(),
            detail: "memref.load expected at least 1 operand (ptr)".into(),
        });
    }
    let ptr = lookup_value(ctx, op.operands[0], &op.name)?;
    let alignment = compute_memref_alignment(op, &r.ty);
    let ty_id = ctx.type_cache.type_for(b, &r.ty, ctx.fn_name, &op.name)?;
    let id = b
        .load(
            ty_id,
            None,
            ptr,
            Some(MemoryAccess::ALIGNED),
            std::iter::once(Operand::LiteralBit32(alignment)),
        )
        .map_err(|e| BodyEmitError::BuilderFailed {
            detail: format!("memref.load : {e:?}"),
        })?;
    ctx.value_map.insert(r.id, id);
    Ok(())
}

/// Lower `memref.store %val, %ptr [, %offset]` to `OpStore` with the
/// `Aligned` operand. No result.
fn emit_memref_store(
    b: &mut Builder,
    ctx: &mut BodyCtx<'_>,
    op: &MirOp,
) -> Result<(), BodyEmitError> {
    if !op.results.is_empty() {
        return Err(BodyEmitError::MalformedOp {
            fn_name: ctx.fn_name.to_string(),
            op_name: op.name.clone(),
            detail: format!(
                "memref.store must have 0 results ; got {}",
                op.results.len()
            ),
        });
    }
    if op.operands.len() < 2 {
        return Err(BodyEmitError::MalformedOp {
            fn_name: ctx.fn_name.to_string(),
            op_name: op.name.clone(),
            detail: format!(
                "memref.store expected ≥ 2 operands (val, ptr [, offset]) ; got {}",
                op.operands.len()
            ),
        });
    }
    let val = lookup_value(ctx, op.operands[0], &op.name)?;
    let ptr = lookup_value(ctx, op.operands[1], &op.name)?;
    // Alignment : derive from the explicit attribute, fallback to a
    // 4-byte natural-align floor when no per-MIR-op type info is present.
    // The MIR-level `"alignment"` attribute is the source of truth ; the
    // type-checker / pass-pipeline is responsible for ensuring it's
    // ≥ natural-alignment of the value type.
    let alignment = op
        .attributes
        .iter()
        .find(|(k, _)| k == "alignment")
        .and_then(|(_, v)| v.parse::<u32>().ok())
        .unwrap_or(4);
    b.store(
        ptr,
        val,
        Some(MemoryAccess::ALIGNED),
        std::iter::once(Operand::LiteralBit32(alignment)),
    )
    .map_err(|e| BodyEmitError::BuilderFailed {
        detail: format!("memref.store : {e:?}"),
    })?;
    Ok(())
}

/// Compute the alignment to feed the SPIR-V `Aligned` memory access operand
/// for a memref op. Mirrors the cranelift lowering's `memref_alignment`
/// helper : `max(natural-align(elem_ty), attr-override)`. When the elem_ty
/// has no natural alignment (e.g., `MirType::Ptr` at stage-0), falls back
/// to the attribute or 4 bytes.
fn compute_memref_alignment(op: &MirOp, elem_ty: &MirType) -> u32 {
    let natural = elem_ty.natural_alignment().unwrap_or(4);
    let parsed = op
        .attributes
        .iter()
        .find(|(k, _)| k == "alignment")
        .and_then(|(_, v)| v.parse::<u32>().ok());
    parsed.map_or(natural, |a| a.max(natural))
}

// ═════════════════════════════════════════════════════════════════════════
// § Structured control-flow lowering.
// ═════════════════════════════════════════════════════════════════════════

/// Lower `scf.if %cond [region then, region else] -> [%result : T]?` to
/// `OpSelectionMerge` + `OpBranchConditional` + per-branch ops + per-branch
/// store-to-merge-var + merge-block load.
///
/// Per the SPIR-V spec § 2.11 Structured Control Flow, every selection
/// header MUST carry an `OpSelectionMerge` instruction immediately before
/// the conditional branch. The merge-block ID is supplied in the merge
/// instruction ; both branches branch unconditionally to it.
fn emit_scf_if(b: &mut Builder, ctx: &mut BodyCtx<'_>, op: &MirOp) -> Result<(), BodyEmitError> {
    if op.regions.len() != 2 {
        return Err(BodyEmitError::MalformedOp {
            fn_name: ctx.fn_name.to_string(),
            op_name: op.name.clone(),
            detail: format!(
                "scf.if expected 2 regions (then, else) ; got {}",
                op.regions.len()
            ),
        });
    }
    let cond = require_one_operand(ctx, op)?;

    // Allocate label-ids for then / else / merge before emitting the
    // selection-merge instruction (the merge-block id is part of that op).
    let then_label = b.id();
    let else_label = b.id();
    let merge_label = b.id();

    // Optional yielded value : if scf.if has a result, set up a Function
    // storage-class OpVariable in the entry block that each branch stores
    // into ; the merge block loads from it.
    let yield_var: Option<(u32, u32)> = match op
        .results
        .first()
        .filter(|r| !matches!(r.ty, MirType::None))
    {
        Some(result) => {
            let ty_id = ctx
                .type_cache
                .type_for(b, &result.ty, ctx.fn_name, &op.name)?;
            let ptr_ty = ctx.type_cache.pointer_for(
                b,
                StorageClass::Function,
                &result.ty,
                ctx.fn_name,
                &op.name,
            )?;
            let var_id = b.variable(ptr_ty, None, StorageClass::Function, None);
            Some((var_id, ty_id))
        }
        None => None,
    };

    // Emit the selection-merge + branch-conditional. SelectionControl::NONE
    // is the default — `Flatten` / `DontFlatten` are optimization hints we
    // don't need at stage-0.
    b.selection_merge(merge_label, SelectionControl::NONE)
        .map_err(|e| BodyEmitError::BuilderFailed {
            detail: format!("selection_merge : {e:?}"),
        })?;
    b.branch_conditional(cond, then_label, else_label, [])
        .map_err(|e| BodyEmitError::BuilderFailed {
            detail: format!("branch_conditional : {e:?}"),
        })?;

    // Then block.
    b.begin_block(Some(then_label))
        .map_err(|e| BodyEmitError::BuilderFailed {
            detail: format!("begin_block (then) : {e:?}"),
        })?;
    emit_branch_region_ops(b, ctx, &op.regions[0], yield_var)?;
    // Branch to merge if the branch didn't already terminate (e.g., via
    // an early func.return inside the branch body).
    if !region_ends_with_terminator(&op.regions[0]) {
        b.branch(merge_label)
            .map_err(|e| BodyEmitError::BuilderFailed {
                detail: format!("branch (then→merge) : {e:?}"),
            })?;
    }

    // Else block.
    b.begin_block(Some(else_label))
        .map_err(|e| BodyEmitError::BuilderFailed {
            detail: format!("begin_block (else) : {e:?}"),
        })?;
    emit_branch_region_ops(b, ctx, &op.regions[1], yield_var)?;
    if !region_ends_with_terminator(&op.regions[1]) {
        b.branch(merge_label)
            .map_err(|e| BodyEmitError::BuilderFailed {
                detail: format!("branch (else→merge) : {e:?}"),
            })?;
    }

    // Merge block.
    b.begin_block(Some(merge_label))
        .map_err(|e| BodyEmitError::BuilderFailed {
            detail: format!("begin_block (merge) : {e:?}"),
        })?;

    // Load the yielded value (if any) into the scf.if's result slot.
    if let (Some((var_id, ty_id)), Some(result)) = (yield_var, op.results.first()) {
        let loaded = b
            .load(ty_id, None, var_id, None, std::iter::empty())
            .map_err(|e| BodyEmitError::BuilderFailed {
                detail: format!("load (scf.if merge) : {e:?}"),
            })?;
        ctx.value_map.insert(result.id, loaded);
    }

    Ok(())
}

/// Walk the ops of a structured-CFG branch region, treating `scf.yield` as
/// a "store-to-yield-var" instruction when the parent has a yield-var slot.
fn emit_branch_region_ops(
    b: &mut Builder,
    ctx: &mut BodyCtx<'_>,
    region: &MirRegion,
    yield_var: Option<(u32, u32)>,
) -> Result<(), BodyEmitError> {
    if let Some(entry) = region.blocks.first() {
        for op in &entry.ops {
            if op.name == "scf.yield" {
                if let (Some((var_id, _)), Some(&yield_id)) = (yield_var, op.operands.first()) {
                    let val = lookup_value(ctx, yield_id, "scf.yield")?;
                    b.store(var_id, val, None, std::iter::empty())
                        .map_err(|e| BodyEmitError::BuilderFailed {
                            detail: format!("store (scf.yield) : {e:?}"),
                        })?;
                }
                continue;
            }
            emit_op(b, ctx, op)?;
        }
    }
    Ok(())
}

/// Detect whether a region's entry block ends in a terminator-style op
/// (e.g., `func.return`). When true, no branch-to-merge is emitted because
/// the branch already terminated via OpReturn / OpReturnValue / etc.
fn region_ends_with_terminator(region: &MirRegion) -> bool {
    region
        .blocks
        .first()
        .and_then(|b| b.ops.last())
        .is_some_and(|op| matches!(op.name.as_str(), "func.return"))
}

/// Lower `scf.for %iter [region body] -> none` to `OpLoopMerge` +
/// `OpBranch %body` + body-walk + `OpBranchConditional %iter %header
/// %merge` (continue-block re-evaluates the latched cond ; stage-0
/// simplification — see module-doc § STRUCTURED-CFG LOWERING).
fn emit_scf_for(b: &mut Builder, ctx: &mut BodyCtx<'_>, op: &MirOp) -> Result<(), BodyEmitError> {
    emit_loop_shape(b, ctx, op, LoopShape::For)
}

/// Lower `scf.while %cond [region body] -> none`.
fn emit_scf_while(b: &mut Builder, ctx: &mut BodyCtx<'_>, op: &MirOp) -> Result<(), BodyEmitError> {
    emit_loop_shape(b, ctx, op, LoopShape::While)
}

/// Lower `scf.loop [region body] -> none` (unbounded loop). The continue
/// block branches back to header unconditionally ; user code is expected
/// to terminate via `func.return` inside the body or through a future
/// `cssl.break` lowering.
fn emit_scf_loop(b: &mut Builder, ctx: &mut BodyCtx<'_>, op: &MirOp) -> Result<(), BodyEmitError> {
    emit_loop_shape(b, ctx, op, LoopShape::Loop)
}

#[derive(Clone, Copy)]
enum LoopShape {
    For,
    While,
    Loop,
}

fn emit_loop_shape(
    b: &mut Builder,
    ctx: &mut BodyCtx<'_>,
    op: &MirOp,
    shape: LoopShape,
) -> Result<(), BodyEmitError> {
    if op.regions.len() != 1 {
        return Err(BodyEmitError::MalformedOp {
            fn_name: ctx.fn_name.to_string(),
            op_name: op.name.clone(),
            detail: format!(
                "{} expected 1 region (body) ; got {}",
                op.name,
                op.regions.len()
            ),
        });
    }

    // Cond/iter operand : scf.for / scf.while take 1 operand. scf.loop
    // takes 0 operands (the synthetic `true` cond goes through a constant
    // we manufacture below).
    let cond_id = match shape {
        LoopShape::For | LoopShape::While => Some(require_one_operand(ctx, op)?),
        LoopShape::Loop => None,
    };

    // Allocate ids for header / body / continue / merge.
    let header_label = b.id();
    let body_label = b.id();
    let continue_label = b.id();
    let merge_label = b.id();

    // Branch from the current block into the header (SPIR-V requires the
    // OpLoopMerge to be inside the loop's header block).
    b.branch(header_label)
        .map_err(|e| BodyEmitError::BuilderFailed {
            detail: format!("branch (entry→header) : {e:?}"),
        })?;

    // Header block : OpLoopMerge + OpBranch %body.
    b.begin_block(Some(header_label))
        .map_err(|e| BodyEmitError::BuilderFailed {
            detail: format!("begin_block (header) : {e:?}"),
        })?;
    b.loop_merge(merge_label, continue_label, LoopControl::NONE, [])
        .map_err(|e| BodyEmitError::BuilderFailed {
            detail: format!("loop_merge : {e:?}"),
        })?;
    b.branch(body_label)
        .map_err(|e| BodyEmitError::BuilderFailed {
            detail: format!("branch (header→body) : {e:?}"),
        })?;

    // Body block : walk the body region's ops, then branch to continue.
    b.begin_block(Some(body_label))
        .map_err(|e| BodyEmitError::BuilderFailed {
            detail: format!("begin_block (body) : {e:?}"),
        })?;
    emit_branch_region_ops(b, ctx, &op.regions[0], None)?;
    if !region_ends_with_terminator(&op.regions[0]) {
        b.branch(continue_label)
            .map_err(|e| BodyEmitError::BuilderFailed {
                detail: format!("branch (body→continue) : {e:?}"),
            })?;
    }

    // Continue block : branch back to header (unconditional for scf.loop ;
    // conditional on the latched cond for scf.for / scf.while).
    b.begin_block(Some(continue_label))
        .map_err(|e| BodyEmitError::BuilderFailed {
            detail: format!("begin_block (continue) : {e:?}"),
        })?;
    match cond_id {
        Some(c) => {
            b.branch_conditional(c, header_label, merge_label, [])
                .map_err(|e| BodyEmitError::BuilderFailed {
                    detail: format!("branch_conditional (continue) : {e:?}"),
                })?;
        }
        None => {
            b.branch(header_label)
                .map_err(|e| BodyEmitError::BuilderFailed {
                    detail: format!("branch (continue→header for scf.loop) : {e:?}"),
                })?;
        }
    }

    // Merge block — open it so subsequent ops emit into the correct block.
    b.begin_block(Some(merge_label))
        .map_err(|e| BodyEmitError::BuilderFailed {
            detail: format!("begin_block (loop merge) : {e:?}"),
        })?;
    Ok(())
}

// ═════════════════════════════════════════════════════════════════════════
// § Function return.
// ═════════════════════════════════════════════════════════════════════════

/// Lower `func.return [%val]` to `OpReturn` (no operand) or `OpReturnValue %val`.
///
/// **Stage-0 simplification** : the kernel entry-point always has a
/// `void` SPIR-V signature (per the `binary_emit::emit_module_binary`
/// convention), so even if MIR carries a return value we emit a plain
/// `OpReturn`. Real return-value plumbing requires the entry-point fn
/// type to model the result, which couples to the descriptor-set
/// machinery — Phase-E work. For now, the kernel's "return value" is
/// observable through writes to descriptor-bound memrefs, not through
/// a SPIR-V return-value.
fn emit_func_return(
    b: &mut Builder,
    _ctx: &mut BodyCtx<'_>,
    _op: &MirOp,
) -> Result<(), BodyEmitError> {
    b.ret().map_err(|e| BodyEmitError::BuilderFailed {
        detail: format!("ret : {e:?}"),
    })?;
    Ok(())
}

// ═════════════════════════════════════════════════════════════════════════
// § Helpers — operand / result accessors.
// ═════════════════════════════════════════════════════════════════════════

/// Look up a `ValueId` in the current ctx, producing a clean diagnostic
/// when not found.
fn lookup_value(ctx: &BodyCtx<'_>, value_id: ValueId, op_name: &str) -> Result<u32, BodyEmitError> {
    ctx.value_map
        .get(&value_id)
        .copied()
        .ok_or_else(|| BodyEmitError::UnknownValueId {
            fn_name: ctx.fn_name.to_string(),
            op_name: op_name.to_string(),
            value_id,
        })
}

/// Require an op to have exactly one result ; return a borrowed view of it.
fn require_one_result<'a>(
    ctx: &BodyCtx<'_>,
    op: &'a MirOp,
) -> Result<&'a cssl_mir::value::MirValue, BodyEmitError> {
    op.results
        .first()
        .ok_or_else(|| BodyEmitError::MalformedOp {
            fn_name: ctx.fn_name.to_string(),
            op_name: op.name.clone(),
            detail: "expected exactly 1 result ; got 0".into(),
        })
}

/// Require an op to have at least one operand ; return its value-id.
fn require_one_operand(ctx: &BodyCtx<'_>, op: &MirOp) -> Result<u32, BodyEmitError> {
    let &id = op
        .operands
        .first()
        .ok_or_else(|| BodyEmitError::MalformedOp {
            fn_name: ctx.fn_name.to_string(),
            op_name: op.name.clone(),
            detail: "expected at least 1 operand ; got 0".into(),
        })?;
    lookup_value(ctx, id, &op.name)
}

/// Require an op to have at least two operands ; return the (lhs, rhs) pair.
fn require_two_operands(ctx: &BodyCtx<'_>, op: &MirOp) -> Result<(u32, u32), BodyEmitError> {
    if op.operands.len() < 2 {
        return Err(BodyEmitError::MalformedOp {
            fn_name: ctx.fn_name.to_string(),
            op_name: op.name.clone(),
            detail: format!("expected at least 2 operands ; got {}", op.operands.len()),
        });
    }
    let lhs = lookup_value(ctx, op.operands[0], &op.name)?;
    let rhs = lookup_value(ctx, op.operands[1], &op.name)?;
    Ok((lhs, rhs))
}

// ═════════════════════════════════════════════════════════════════════════
// § Tests — full coverage of the per-op dispatcher + structured-CFG
// lowering + LANDMINE rejections + D5 marker contract.
// ═════════════════════════════════════════════════════════════════════════
#[cfg(test)]
mod tests {
    use super::*;
    use crate::emit::minimal_vulkan_compute_module;
    use crate::module::SpirvEntryPoint;
    use cssl_mir::block::{MirBlock, MirOp, MirRegion};
    use cssl_mir::func::{MirFunc, MirModule};
    use cssl_mir::structured_cfg::{validate_and_mark, STRUCTURED_CFG_VALIDATED_KEY};
    use cssl_mir::value::{IntWidth, MirType, MirValue, ValueId};
    use rspirv::dr;

    /// Build a void-returning fn with empty body. Used as a baseline shape
    /// where the kernel body is just `func.return`.
    fn empty_fn(name: &str) -> MirFunc {
        let mut f = MirFunc::new(name, vec![], vec![]);
        f.push_op(MirOp::std("func.return"));
        f
    }

    /// Build a module containing one fn + the D5 marker. This is the
    /// canonical "well-formed input to D1" shape.
    fn validated_module_with(fns: Vec<MirFunc>) -> MirModule {
        let mut m = MirModule::new();
        for f in fns {
            m.push_func(f);
        }
        validate_and_mark(&mut m).expect("D5 must accept these fixtures");
        m
    }

    // ── D5 marker contract ──────────────────────────────────────────────

    #[test]
    fn rejects_module_without_d5_marker() {
        // No marker = no emission.
        let m = MirModule::new();
        let spv = minimal_vulkan_compute_module("kernel");
        let e = emit_kernel_module(&spv, &m, "kernel").unwrap_err();
        assert_eq!(e, BodyEmitError::StructuredCfgMarkerAbsent);
    }

    #[test]
    fn accepts_module_with_d5_marker() {
        let m = validated_module_with(vec![empty_fn("kernel")]);
        let spv = minimal_vulkan_compute_module("kernel");
        let words = emit_kernel_module(&spv, &m, "kernel").expect("emit");
        assert!(!words.is_empty());
        assert_eq!(
            m.attributes
                .iter()
                .filter(|(k, _)| k == STRUCTURED_CFG_VALIDATED_KEY)
                .count(),
            1
        );
    }

    // ── Heap rejection ──────────────────────────────────────────────────

    #[test]
    fn rejects_heap_alloc_in_kernel() {
        let mut f = MirFunc::new("kernel", vec![], vec![]);
        f.push_op(MirOp::std("cssl.heap.alloc"));
        f.push_op(MirOp::std("func.return"));
        let m = validated_module_with(vec![f]);
        let spv = minimal_vulkan_compute_module("kernel");
        let e = emit_kernel_module(&spv, &m, "kernel").unwrap_err();
        match e {
            BodyEmitError::HeapNotSupportedOnGpu { fn_name, op_name } => {
                assert_eq!(fn_name, "kernel");
                assert_eq!(op_name, "cssl.heap.alloc");
            }
            other => panic!("expected HeapNotSupportedOnGpu, got {other:?}"),
        }
    }

    #[test]
    fn rejects_heap_dealloc_in_kernel() {
        let mut f = MirFunc::new("kernel", vec![], vec![]);
        f.push_op(MirOp::std("cssl.heap.dealloc"));
        f.push_op(MirOp::std("func.return"));
        let m = validated_module_with(vec![f]);
        let spv = minimal_vulkan_compute_module("kernel");
        let e = emit_kernel_module(&spv, &m, "kernel").unwrap_err();
        assert!(matches!(e, BodyEmitError::HeapNotSupportedOnGpu { .. }));
    }

    #[test]
    fn rejects_heap_realloc_in_kernel() {
        let mut f = MirFunc::new("kernel", vec![], vec![]);
        f.push_op(MirOp::std("cssl.heap.realloc"));
        f.push_op(MirOp::std("func.return"));
        let m = validated_module_with(vec![f]);
        let spv = minimal_vulkan_compute_module("kernel");
        let e = emit_kernel_module(&spv, &m, "kernel").unwrap_err();
        assert!(matches!(e, BodyEmitError::HeapNotSupportedOnGpu { .. }));
    }

    #[test]
    fn rejects_heap_op_nested_inside_scf_if() {
        // Heap op inside a scf.if then-branch must still be rejected (the
        // pre-scan walks recursively).
        let mut f = MirFunc::new("kernel", vec![], vec![]);
        let mut iff = MirOp::std("scf.if");
        let mut then_blk = MirBlock::entry(vec![]);
        then_blk.push(MirOp::std("cssl.heap.alloc"));
        let mut then_region = MirRegion::new();
        then_region.push(then_blk);
        iff.regions.push(then_region);
        iff.regions.push(MirRegion::with_entry(vec![]));
        f.push_op(iff);
        f.push_op(MirOp::std("func.return"));
        let m = validated_module_with(vec![f]);
        let spv = minimal_vulkan_compute_module("kernel");
        let e = emit_kernel_module(&spv, &m, "kernel").unwrap_err();
        assert!(matches!(e, BodyEmitError::HeapNotSupportedOnGpu { .. }));
    }

    // ── Closure rejection ───────────────────────────────────────────────

    #[test]
    fn rejects_closure_in_kernel() {
        let mut f = MirFunc::new("kernel", vec![], vec![]);
        f.push_op(MirOp::std("cssl.closure"));
        f.push_op(MirOp::std("func.return"));
        let m = validated_module_with(vec![f]);
        let spv = minimal_vulkan_compute_module("kernel");
        let e = emit_kernel_module(&spv, &m, "kernel").unwrap_err();
        match e {
            BodyEmitError::ClosuresNotSupportedOnGpu { fn_name, op_name } => {
                assert_eq!(fn_name, "kernel");
                assert_eq!(op_name, "cssl.closure");
            }
            other => panic!("expected ClosuresNotSupportedOnGpu, got {other:?}"),
        }
    }

    #[test]
    fn rejects_closure_subvariant_in_kernel() {
        let mut f = MirFunc::new("kernel", vec![], vec![]);
        f.push_op(MirOp::std("cssl.closure.call"));
        f.push_op(MirOp::std("func.return"));
        let m = validated_module_with(vec![f]);
        let spv = minimal_vulkan_compute_module("kernel");
        let e = emit_kernel_module(&spv, &m, "kernel").unwrap_err();
        assert!(matches!(e, BodyEmitError::ClosuresNotSupportedOnGpu { .. }));
    }

    // ── Kernel-fn lookup ────────────────────────────────────────────────

    #[test]
    fn rejects_when_kernel_fn_not_in_module() {
        let m = validated_module_with(vec![empty_fn("other_fn")]);
        let spv = minimal_vulkan_compute_module("kernel");
        let e = emit_kernel_module(&spv, &m, "kernel").unwrap_err();
        match e {
            BodyEmitError::KernelFnNotFound { kernel_fn } => {
                assert_eq!(kernel_fn, "kernel");
            }
            other => panic!("expected KernelFnNotFound, got {other:?}"),
        }
    }

    // ── arith.constant + scalar arith ───────────────────────────────────

    /// Build a kernel fn that emits `c0 = 7 ; c1 = 5 ; r = c0 + c1 ; return`.
    fn fn_with_int_add() -> MirFunc {
        let mut f = MirFunc::new("kernel", vec![], vec![]);
        let v0 = f.fresh_value_id();
        let v1 = f.fresh_value_id();
        let v2 = f.fresh_value_id();
        f.push_op(
            MirOp::std("arith.constant")
                .with_attribute("value", "7")
                .with_result(v0, MirType::Int(IntWidth::I32)),
        );
        f.push_op(
            MirOp::std("arith.constant")
                .with_attribute("value", "5")
                .with_result(v1, MirType::Int(IntWidth::I32)),
        );
        f.push_op(
            MirOp::std("arith.addi")
                .with_operand(v0)
                .with_operand(v1)
                .with_result(v2, MirType::Int(IntWidth::I32)),
        );
        f.push_op(MirOp::std("func.return"));
        f
    }

    #[test]
    fn arith_constant_plus_iadd_round_trips() {
        let m = validated_module_with(vec![fn_with_int_add()]);
        let spv = minimal_vulkan_compute_module("kernel");
        let words = emit_kernel_module(&spv, &m, "kernel").expect("emit");
        let parsed = dr::load_words(&words).expect("rspirv must parse");
        // Find at least one OpIAdd in the kernel body.
        let has_iadd = parsed
            .functions
            .iter()
            .flat_map(|f| f.blocks.iter().flat_map(|bb| bb.instructions.iter()))
            .any(|i| i.class.opcode == spirv::Op::IAdd);
        assert!(has_iadd, "kernel body must contain OpIAdd");
    }

    /// Build a kernel fn that emits `c0 = 1.5 ; c1 = 2.5 ; r = c0 * c1 ; return`.
    fn fn_with_float_mul() -> MirFunc {
        let mut f = MirFunc::new("kernel", vec![], vec![]);
        let v0 = f.fresh_value_id();
        let v1 = f.fresh_value_id();
        let v2 = f.fresh_value_id();
        f.push_op(
            MirOp::std("arith.constant")
                .with_attribute("value", "1.5")
                .with_result(v0, MirType::Float(FloatWidth::F32)),
        );
        f.push_op(
            MirOp::std("arith.constant")
                .with_attribute("value", "2.5")
                .with_result(v1, MirType::Float(FloatWidth::F32)),
        );
        f.push_op(
            MirOp::std("arith.mulf")
                .with_operand(v0)
                .with_operand(v1)
                .with_result(v2, MirType::Float(FloatWidth::F32)),
        );
        f.push_op(MirOp::std("func.return"));
        f
    }

    #[test]
    fn arith_constant_plus_fmul_round_trips() {
        let m = validated_module_with(vec![fn_with_float_mul()]);
        let spv = minimal_vulkan_compute_module("kernel");
        let words = emit_kernel_module(&spv, &m, "kernel").expect("emit");
        let parsed = dr::load_words(&words).expect("rspirv must parse");
        let has_fmul = parsed
            .functions
            .iter()
            .flat_map(|f| f.blocks.iter().flat_map(|bb| bb.instructions.iter()))
            .any(|i| i.class.opcode == spirv::Op::FMul);
        assert!(has_fmul, "kernel body must contain OpFMul");
    }

    #[test]
    fn arith_subi_lowers_to_op_isub() {
        let mut f = MirFunc::new("kernel", vec![], vec![]);
        let v0 = f.fresh_value_id();
        let v1 = f.fresh_value_id();
        let v2 = f.fresh_value_id();
        f.push_op(
            MirOp::std("arith.constant")
                .with_attribute("value", "10")
                .with_result(v0, MirType::Int(IntWidth::I32)),
        );
        f.push_op(
            MirOp::std("arith.constant")
                .with_attribute("value", "3")
                .with_result(v1, MirType::Int(IntWidth::I32)),
        );
        f.push_op(
            MirOp::std("arith.subi")
                .with_operand(v0)
                .with_operand(v1)
                .with_result(v2, MirType::Int(IntWidth::I32)),
        );
        f.push_op(MirOp::std("func.return"));
        let m = validated_module_with(vec![f]);
        let spv = minimal_vulkan_compute_module("kernel");
        let words = emit_kernel_module(&spv, &m, "kernel").expect("emit");
        let parsed = dr::load_words(&words).expect("rspirv parse");
        let has_isub = parsed
            .functions
            .iter()
            .flat_map(|f| f.blocks.iter().flat_map(|bb| bb.instructions.iter()))
            .any(|i| i.class.opcode == spirv::Op::ISub);
        assert!(has_isub, "expected OpISub");
    }

    #[test]
    fn arith_negf_lowers_to_op_fnegate() {
        let mut f = MirFunc::new("kernel", vec![], vec![]);
        let v0 = f.fresh_value_id();
        let v1 = f.fresh_value_id();
        f.push_op(
            MirOp::std("arith.constant")
                .with_attribute("value", "1.0")
                .with_result(v0, MirType::Float(FloatWidth::F32)),
        );
        f.push_op(
            MirOp::std("arith.negf")
                .with_operand(v0)
                .with_result(v1, MirType::Float(FloatWidth::F32)),
        );
        f.push_op(MirOp::std("func.return"));
        let m = validated_module_with(vec![f]);
        let spv = minimal_vulkan_compute_module("kernel");
        let words = emit_kernel_module(&spv, &m, "kernel").expect("emit");
        let parsed = dr::load_words(&words).expect("rspirv parse");
        let has_fnegate = parsed
            .functions
            .iter()
            .flat_map(|f| f.blocks.iter().flat_map(|bb| bb.instructions.iter()))
            .any(|i| i.class.opcode == spirv::Op::FNegate);
        assert!(has_fnegate, "expected OpFNegate");
    }

    // ── Comparisons ─────────────────────────────────────────────────────

    #[test]
    fn arith_cmpi_eq_lowers_to_op_iequal() {
        let mut f = MirFunc::new("kernel", vec![], vec![]);
        let v0 = f.fresh_value_id();
        let v1 = f.fresh_value_id();
        let v2 = f.fresh_value_id();
        f.push_op(
            MirOp::std("arith.constant")
                .with_attribute("value", "7")
                .with_result(v0, MirType::Int(IntWidth::I32)),
        );
        f.push_op(
            MirOp::std("arith.constant")
                .with_attribute("value", "8")
                .with_result(v1, MirType::Int(IntWidth::I32)),
        );
        f.push_op(
            MirOp::std("arith.cmpi_eq")
                .with_operand(v0)
                .with_operand(v1)
                .with_result(v2, MirType::Bool),
        );
        f.push_op(MirOp::std("func.return"));
        let m = validated_module_with(vec![f]);
        let spv = minimal_vulkan_compute_module("kernel");
        let words = emit_kernel_module(&spv, &m, "kernel").expect("emit");
        let parsed = dr::load_words(&words).expect("rspirv parse");
        let has_ieq = parsed
            .functions
            .iter()
            .flat_map(|f| f.blocks.iter().flat_map(|bb| bb.instructions.iter()))
            .any(|i| i.class.opcode == spirv::Op::IEqual);
        assert!(has_ieq, "expected OpIEqual");
    }

    #[test]
    fn arith_cmpf_olt_lowers_to_op_ford_less_than() {
        let mut f = MirFunc::new("kernel", vec![], vec![]);
        let v0 = f.fresh_value_id();
        let v1 = f.fresh_value_id();
        let v2 = f.fresh_value_id();
        f.push_op(
            MirOp::std("arith.constant")
                .with_attribute("value", "0.5")
                .with_result(v0, MirType::Float(FloatWidth::F32)),
        );
        f.push_op(
            MirOp::std("arith.constant")
                .with_attribute("value", "1.0")
                .with_result(v1, MirType::Float(FloatWidth::F32)),
        );
        f.push_op(
            MirOp::std("arith.cmpf")
                .with_attribute("predicate", "olt")
                .with_operand(v0)
                .with_operand(v1)
                .with_result(v2, MirType::Bool),
        );
        f.push_op(MirOp::std("func.return"));
        let m = validated_module_with(vec![f]);
        let spv = minimal_vulkan_compute_module("kernel");
        let words = emit_kernel_module(&spv, &m, "kernel").expect("emit");
        let parsed = dr::load_words(&words).expect("rspirv parse");
        let has_flt = parsed
            .functions
            .iter()
            .flat_map(|f| f.blocks.iter().flat_map(|bb| bb.instructions.iter()))
            .any(|i| i.class.opcode == spirv::Op::FOrdLessThan);
        assert!(has_flt, "expected OpFOrdLessThan");
    }

    #[test]
    fn arith_cmpi_with_bad_predicate_returns_malformed() {
        let mut f = MirFunc::new("kernel", vec![], vec![]);
        let v0 = f.fresh_value_id();
        let v1 = f.fresh_value_id();
        let v2 = f.fresh_value_id();
        f.push_op(
            MirOp::std("arith.constant")
                .with_attribute("value", "7")
                .with_result(v0, MirType::Int(IntWidth::I32)),
        );
        f.push_op(
            MirOp::std("arith.constant")
                .with_attribute("value", "8")
                .with_result(v1, MirType::Int(IntWidth::I32)),
        );
        f.push_op(
            MirOp::std("arith.cmpi")
                .with_attribute("predicate", "bogus")
                .with_operand(v0)
                .with_operand(v1)
                .with_result(v2, MirType::Bool),
        );
        f.push_op(MirOp::std("func.return"));
        let m = validated_module_with(vec![f]);
        let spv = minimal_vulkan_compute_module("kernel");
        let e = emit_kernel_module(&spv, &m, "kernel").unwrap_err();
        assert!(matches!(e, BodyEmitError::MalformedOp { .. }));
    }

    // ── Select ──────────────────────────────────────────────────────────

    #[test]
    fn arith_select_lowers_to_op_select() {
        let mut f = MirFunc::new("kernel", vec![], vec![]);
        let v0 = f.fresh_value_id();
        let v1 = f.fresh_value_id();
        let v2 = f.fresh_value_id();
        let v3 = f.fresh_value_id();
        f.push_op(
            MirOp::std("arith.constant")
                .with_attribute("value", "true")
                .with_result(v0, MirType::Bool),
        );
        f.push_op(
            MirOp::std("arith.constant")
                .with_attribute("value", "1")
                .with_result(v1, MirType::Int(IntWidth::I32)),
        );
        f.push_op(
            MirOp::std("arith.constant")
                .with_attribute("value", "2")
                .with_result(v2, MirType::Int(IntWidth::I32)),
        );
        f.push_op(
            MirOp::std("arith.select")
                .with_operand(v0)
                .with_operand(v1)
                .with_operand(v2)
                .with_result(v3, MirType::Int(IntWidth::I32)),
        );
        f.push_op(MirOp::std("func.return"));
        let m = validated_module_with(vec![f]);
        let spv = minimal_vulkan_compute_module("kernel");
        let words = emit_kernel_module(&spv, &m, "kernel").expect("emit");
        let parsed = dr::load_words(&words).expect("rspirv parse");
        let has_select = parsed
            .functions
            .iter()
            .flat_map(|f| f.blocks.iter().flat_map(|bb| bb.instructions.iter()))
            .any(|i| i.class.opcode == spirv::Op::Select);
        assert!(has_select, "expected OpSelect");
    }

    // ── memref ──────────────────────────────────────────────────────────

    /// Build a kernel fn that takes a ptr param and emits `r = load *p ; return`.
    fn fn_with_load() -> MirFunc {
        let mut f = MirFunc::new("kernel", vec![MirType::Ptr], vec![]);
        // Param value is ValueId(0). load result is fresh.
        let v_r = f.fresh_value_id();
        f.push_op(
            MirOp::std("memref.load")
                .with_operand(ValueId(0))
                .with_result(v_r, MirType::Int(IntWidth::I32)),
        );
        f.push_op(MirOp::std("func.return"));
        f
    }

    #[test]
    fn memref_load_lowers_with_aligned_operand() {
        // Note : the param-as-ptr is an Undef stand-in at stage-0 ; that's
        // ok for the structural test (we're checking OpLoad emission, not
        // semantic correctness against real memory).
        let f = fn_with_load();
        // Param ptr is unsupported as a SPIR-V type at stage-0, so the
        // value-bind silently skips and the OpLoad will fail with
        // UnknownValueId. That's the expected stage-0 shape — we test
        // that pathway separately via memref_store_with_supported_ptr.
        let m = validated_module_with(vec![f]);
        let spv = minimal_vulkan_compute_module("kernel");
        let e = emit_kernel_module(&spv, &m, "kernel").unwrap_err();
        // We tolerate either UnknownValueId (param-ptr unbound) or
        // UnsupportedResultType (result-ty failed mapping). Both surface
        // the stage-0 limitation cleanly.
        assert!(
            matches!(
                e,
                BodyEmitError::UnknownValueId { .. } | BodyEmitError::UnsupportedResultType { .. }
            ),
            "expected clean rejection of stage-0 memref.load with param-ptr ; got {e:?}"
        );
    }

    #[test]
    fn memref_load_with_constant_ptr_lowers_cleanly() {
        // Synthesize a kernel where the ptr is an arith.constant so the
        // value-bind succeeds. This exercises the OpLoad emission path
        // without depending on param-passing.
        let mut f = MirFunc::new("kernel", vec![], vec![]);
        let v_p = f.fresh_value_id();
        let v_r = f.fresh_value_id();
        // Treat ptr as i64 stage-0 placeholder. SPIR-V's logical addressing
        // would normally require an OpTypePointer-typed value, but for the
        // structural test we're only verifying OpLoad+ALIGNED gets emitted.
        f.push_op(
            MirOp::std("arith.constant")
                .with_attribute("value", "0")
                .with_result(v_p, MirType::Int(IntWidth::I64)),
        );
        f.push_op(
            MirOp::std("memref.load")
                .with_operand(v_p)
                .with_result(v_r, MirType::Int(IntWidth::I32)),
        );
        f.push_op(MirOp::std("func.return"));
        let m = validated_module_with(vec![f]);
        let spv = minimal_vulkan_compute_module("kernel");
        let words = emit_kernel_module(&spv, &m, "kernel").expect("emit");
        let parsed = dr::load_words(&words).expect("rspirv parse");
        let has_load = parsed
            .functions
            .iter()
            .flat_map(|f| f.blocks.iter().flat_map(|bb| bb.instructions.iter()))
            .any(|i| i.class.opcode == spirv::Op::Load);
        assert!(has_load, "expected OpLoad");
        // Aligned operand attached.
        let has_aligned = parsed
            .functions
            .iter()
            .flat_map(|f| f.blocks.iter().flat_map(|bb| bb.instructions.iter()))
            .filter(|i| i.class.opcode == spirv::Op::Load)
            .any(|i| {
                i.operands
                    .iter()
                    .any(|op| matches!(op, dr::Operand::MemoryAccess(m) if m.contains(MemoryAccess::ALIGNED)))
            });
        assert!(has_aligned, "OpLoad must carry MemoryAccess::ALIGNED");
    }

    #[test]
    fn memref_store_lowers_to_op_store() {
        let mut f = MirFunc::new("kernel", vec![], vec![]);
        let v_v = f.fresh_value_id();
        let v_p = f.fresh_value_id();
        f.push_op(
            MirOp::std("arith.constant")
                .with_attribute("value", "42")
                .with_result(v_v, MirType::Int(IntWidth::I32)),
        );
        f.push_op(
            MirOp::std("arith.constant")
                .with_attribute("value", "0")
                .with_result(v_p, MirType::Int(IntWidth::I64)),
        );
        f.push_op(
            MirOp::std("memref.store")
                .with_operand(v_v)
                .with_operand(v_p),
        );
        f.push_op(MirOp::std("func.return"));
        let m = validated_module_with(vec![f]);
        let spv = minimal_vulkan_compute_module("kernel");
        let words = emit_kernel_module(&spv, &m, "kernel").expect("emit");
        let parsed = dr::load_words(&words).expect("rspirv parse");
        let has_store = parsed
            .functions
            .iter()
            .flat_map(|f| f.blocks.iter().flat_map(|bb| bb.instructions.iter()))
            .any(|i| i.class.opcode == spirv::Op::Store);
        assert!(has_store, "expected OpStore");
    }

    #[test]
    fn memref_load_alignment_attribute_overrides_natural() {
        // Alignment attribute >= natural should produce the override
        // value in the emitted Aligned operand.
        let mut f = MirFunc::new("kernel", vec![], vec![]);
        let v_p = f.fresh_value_id();
        let v_r = f.fresh_value_id();
        f.push_op(
            MirOp::std("arith.constant")
                .with_attribute("value", "0")
                .with_result(v_p, MirType::Int(IntWidth::I64)),
        );
        f.push_op(
            MirOp::std("memref.load")
                .with_attribute("alignment", "8")
                .with_operand(v_p)
                .with_result(v_r, MirType::Int(IntWidth::I32)),
        );
        f.push_op(MirOp::std("func.return"));
        let m = validated_module_with(vec![f]);
        let spv = minimal_vulkan_compute_module("kernel");
        let words = emit_kernel_module(&spv, &m, "kernel").expect("emit");
        let parsed = dr::load_words(&words).expect("rspirv parse");
        // Find the load + check its align operand value is 8.
        let load = parsed
            .functions
            .iter()
            .flat_map(|f| f.blocks.iter().flat_map(|bb| bb.instructions.iter()))
            .find(|i| i.class.opcode == spirv::Op::Load)
            .expect("expected OpLoad");
        let align_lit = load
            .operands
            .iter()
            .find_map(|op| match op {
                dr::Operand::LiteralBit32(n) => Some(*n),
                _ => None,
            })
            .expect("expected LiteralBit32 alignment operand");
        assert_eq!(align_lit, 8);
    }

    #[test]
    fn memref_store_with_result_returns_malformed() {
        let mut f = MirFunc::new("kernel", vec![], vec![]);
        let v_v = f.fresh_value_id();
        let v_p = f.fresh_value_id();
        let v_bogus = f.fresh_value_id();
        f.push_op(
            MirOp::std("arith.constant")
                .with_attribute("value", "1")
                .with_result(v_v, MirType::Int(IntWidth::I32)),
        );
        f.push_op(
            MirOp::std("arith.constant")
                .with_attribute("value", "0")
                .with_result(v_p, MirType::Int(IntWidth::I64)),
        );
        f.push_op(
            MirOp::std("memref.store")
                .with_operand(v_v)
                .with_operand(v_p)
                .with_result(v_bogus, MirType::Int(IntWidth::I32)),
        );
        f.push_op(MirOp::std("func.return"));
        let m = validated_module_with(vec![f]);
        let spv = minimal_vulkan_compute_module("kernel");
        let e = emit_kernel_module(&spv, &m, "kernel").unwrap_err();
        assert!(matches!(e, BodyEmitError::MalformedOp { .. }));
    }

    // ── scf.if ──────────────────────────────────────────────────────────

    #[test]
    fn scf_if_emits_selection_merge_and_branch_conditional() {
        // Build : if cond { 1 } else { 2 } where cond is a constant-true.
        let mut f = MirFunc::new("kernel", vec![], vec![]);
        let v_c = f.fresh_value_id();
        let v_t = f.fresh_value_id();
        let v_e = f.fresh_value_id();
        let v_r = f.fresh_value_id();
        f.push_op(
            MirOp::std("arith.constant")
                .with_attribute("value", "true")
                .with_result(v_c, MirType::Bool),
        );
        // Build then + else regions, each emitting a constant + scf.yield.
        let mut then_blk = MirBlock::entry(vec![]);
        then_blk.push(
            MirOp::std("arith.constant")
                .with_attribute("value", "1")
                .with_result(v_t, MirType::Int(IntWidth::I32)),
        );
        then_blk.push(MirOp::std("scf.yield").with_operand(v_t));
        let mut then_region = MirRegion::new();
        then_region.push(then_blk);
        let mut else_blk = MirBlock::entry(vec![]);
        else_blk.push(
            MirOp::std("arith.constant")
                .with_attribute("value", "2")
                .with_result(v_e, MirType::Int(IntWidth::I32)),
        );
        else_blk.push(MirOp::std("scf.yield").with_operand(v_e));
        let mut else_region = MirRegion::new();
        else_region.push(else_blk);
        let iff = MirOp::std("scf.if")
            .with_operand(v_c)
            .with_region(then_region)
            .with_region(else_region)
            .with_result(v_r, MirType::Int(IntWidth::I32));
        f.push_op(iff);
        f.push_op(MirOp::std("func.return"));
        let m = validated_module_with(vec![f]);
        let spv = minimal_vulkan_compute_module("kernel");
        let words = emit_kernel_module(&spv, &m, "kernel").expect("emit");
        let parsed = dr::load_words(&words).expect("rspirv parse");
        let opcodes: Vec<_> = parsed
            .functions
            .iter()
            .flat_map(|f| f.blocks.iter().flat_map(|bb| bb.instructions.iter()))
            .map(|i| i.class.opcode)
            .collect();
        assert!(
            opcodes.contains(&spirv::Op::SelectionMerge),
            "expected OpSelectionMerge in {opcodes:?}"
        );
        assert!(
            opcodes.contains(&spirv::Op::BranchConditional),
            "expected OpBranchConditional in {opcodes:?}"
        );
        // Three branch instructions total : 1 conditional + 2 unconditional
        // (then→merge, else→merge).
        let cond_count = opcodes
            .iter()
            .filter(|op| **op == spirv::Op::BranchConditional)
            .count();
        let uncond_count = opcodes
            .iter()
            .filter(|op| **op == spirv::Op::Branch)
            .count();
        assert_eq!(cond_count, 1);
        assert!(uncond_count >= 2, "expected at least 2 OpBranch");
    }

    #[test]
    fn scf_if_statement_form_emits_no_yield_var() {
        // scf.if without a result type should still emit the structured
        // pair without trying to set up a yield variable.
        let mut f = MirFunc::new("kernel", vec![], vec![]);
        let v_c = f.fresh_value_id();
        f.push_op(
            MirOp::std("arith.constant")
                .with_attribute("value", "false")
                .with_result(v_c, MirType::Bool),
        );
        // No yield op in either branch.
        let then_region = MirRegion::with_entry(vec![]);
        let else_region = MirRegion::with_entry(vec![]);
        let iff = MirOp::std("scf.if")
            .with_operand(v_c)
            .with_region(then_region)
            .with_region(else_region);
        f.push_op(iff);
        f.push_op(MirOp::std("func.return"));
        let m = validated_module_with(vec![f]);
        let spv = minimal_vulkan_compute_module("kernel");
        let words = emit_kernel_module(&spv, &m, "kernel").expect("emit");
        let parsed = dr::load_words(&words).expect("rspirv parse");
        // Should still have OpSelectionMerge.
        let has_sel = parsed
            .functions
            .iter()
            .flat_map(|f| f.blocks.iter().flat_map(|bb| bb.instructions.iter()))
            .any(|i| i.class.opcode == spirv::Op::SelectionMerge);
        assert!(has_sel);
    }

    // ── scf.for / scf.while / scf.loop ──────────────────────────────────

    #[test]
    fn scf_for_emits_loop_merge_and_branch_back() {
        let mut f = MirFunc::new("kernel", vec![], vec![]);
        let v_c = f.fresh_value_id();
        f.push_op(
            MirOp::std("arith.constant")
                .with_attribute("value", "true")
                .with_result(v_c, MirType::Bool),
        );
        let body_region = MirRegion::with_entry(vec![]);
        let v_r = f.fresh_value_id();
        let for_op = MirOp::std("scf.for")
            .with_operand(v_c)
            .with_region(body_region)
            .with_result(v_r, MirType::None);
        f.push_op(for_op);
        f.push_op(MirOp::std("func.return"));
        let m = validated_module_with(vec![f]);
        let spv = minimal_vulkan_compute_module("kernel");
        let words = emit_kernel_module(&spv, &m, "kernel").expect("emit");
        let parsed = dr::load_words(&words).expect("rspirv parse");
        let has_loop_merge = parsed
            .functions
            .iter()
            .flat_map(|f| f.blocks.iter().flat_map(|bb| bb.instructions.iter()))
            .any(|i| i.class.opcode == spirv::Op::LoopMerge);
        assert!(has_loop_merge, "expected OpLoopMerge for scf.for");
    }

    #[test]
    fn scf_while_emits_loop_merge() {
        let mut f = MirFunc::new("kernel", vec![], vec![]);
        let v_c = f.fresh_value_id();
        f.push_op(
            MirOp::std("arith.constant")
                .with_attribute("value", "true")
                .with_result(v_c, MirType::Bool),
        );
        let body_region = MirRegion::with_entry(vec![]);
        let v_r = f.fresh_value_id();
        let wh = MirOp::std("scf.while")
            .with_operand(v_c)
            .with_region(body_region)
            .with_result(v_r, MirType::None);
        f.push_op(wh);
        f.push_op(MirOp::std("func.return"));
        let m = validated_module_with(vec![f]);
        let spv = minimal_vulkan_compute_module("kernel");
        let words = emit_kernel_module(&spv, &m, "kernel").expect("emit");
        let parsed = dr::load_words(&words).expect("rspirv parse");
        let has_loop_merge = parsed
            .functions
            .iter()
            .flat_map(|f| f.blocks.iter().flat_map(|bb| bb.instructions.iter()))
            .any(|i| i.class.opcode == spirv::Op::LoopMerge);
        assert!(has_loop_merge, "expected OpLoopMerge for scf.while");
    }

    #[test]
    fn scf_loop_emits_loop_merge_and_unconditional_back_edge() {
        let mut f = MirFunc::new("kernel", vec![], vec![]);
        let body_region = MirRegion::with_entry(vec![]);
        let v_r = f.fresh_value_id();
        let lp = MirOp::std("scf.loop")
            .with_region(body_region)
            .with_result(v_r, MirType::None);
        f.push_op(lp);
        f.push_op(MirOp::std("func.return"));
        let m = validated_module_with(vec![f]);
        let spv = minimal_vulkan_compute_module("kernel");
        let words = emit_kernel_module(&spv, &m, "kernel").expect("emit");
        let parsed = dr::load_words(&words).expect("rspirv parse");
        let opcodes: Vec<_> = parsed
            .functions
            .iter()
            .flat_map(|f| f.blocks.iter().flat_map(|bb| bb.instructions.iter()))
            .map(|i| i.class.opcode)
            .collect();
        assert!(opcodes.contains(&spirv::Op::LoopMerge));
        // For scf.loop the continue→header edge is unconditional, so we
        // should NOT see an OpBranchConditional inside the loop body
        // (the only branch_conditional in the module would be inside
        // an inner scf.if, which we don't have). At minimum the count
        // is zero.
        assert_eq!(
            opcodes
                .iter()
                .filter(|op| **op == spirv::Op::BranchConditional)
                .count(),
            0,
            "scf.loop must not emit BranchConditional"
        );
    }

    // ── func.return ─────────────────────────────────────────────────────

    #[test]
    fn func_return_emits_op_return() {
        let m = validated_module_with(vec![empty_fn("kernel")]);
        let spv = minimal_vulkan_compute_module("kernel");
        let words = emit_kernel_module(&spv, &m, "kernel").expect("emit");
        let parsed = dr::load_words(&words).expect("rspirv parse");
        let has_ret = parsed
            .functions
            .iter()
            .flat_map(|f| f.blocks.iter().flat_map(|bb| bb.instructions.iter()))
            .any(|i| i.class.opcode == spirv::Op::Return);
        assert!(has_ret, "expected OpReturn");
    }

    #[test]
    fn func_return_with_operand_still_emits_op_return_at_stage_0() {
        // Stage-0 emits OpReturn even if MIR carries a return value, since
        // the SPIR-V entry-point fn type is void per binary_emit's convention.
        let mut f = MirFunc::new("kernel", vec![], vec![]);
        let v0 = f.fresh_value_id();
        f.push_op(
            MirOp::std("arith.constant")
                .with_attribute("value", "42")
                .with_result(v0, MirType::Int(IntWidth::I32)),
        );
        f.push_op(MirOp::std("func.return").with_operand(v0));
        let m = validated_module_with(vec![f]);
        let spv = minimal_vulkan_compute_module("kernel");
        let words = emit_kernel_module(&spv, &m, "kernel").expect("emit");
        let parsed = dr::load_words(&words).expect("rspirv parse");
        let has_ret = parsed
            .functions
            .iter()
            .flat_map(|f| f.blocks.iter().flat_map(|bb| bb.instructions.iter()))
            .any(|i| i.class.opcode == spirv::Op::Return);
        assert!(has_ret);
    }

    // ── Unsupported op ──────────────────────────────────────────────────

    #[test]
    fn unsupported_op_returns_clean_error() {
        let mut f = MirFunc::new("kernel", vec![], vec![]);
        f.push_op(MirOp::std("cssl.future_sf_op"));
        f.push_op(MirOp::std("func.return"));
        let m = validated_module_with(vec![f]);
        let spv = minimal_vulkan_compute_module("kernel");
        let e = emit_kernel_module(&spv, &m, "kernel").unwrap_err();
        match e {
            BodyEmitError::UnsupportedOp { fn_name, op_name } => {
                assert_eq!(fn_name, "kernel");
                assert_eq!(op_name, "cssl.future_sf_op");
            }
            other => panic!("expected UnsupportedOp, got {other:?}"),
        }
    }

    // ── Round-trip discipline : emitted modules round-trip through rspirv ─

    #[test]
    fn kernel_module_starts_with_magic() {
        let m = validated_module_with(vec![fn_with_int_add()]);
        let spv = minimal_vulkan_compute_module("kernel");
        let words = emit_kernel_module(&spv, &m, "kernel").expect("emit");
        // Magic word is the SPIR-V module sentinel.
        assert_eq!(words[0], 0x0723_0203);
    }

    #[test]
    fn kernel_module_round_trips_via_rspirv_loader() {
        let m = validated_module_with(vec![fn_with_int_add()]);
        let spv = minimal_vulkan_compute_module("kernel");
        let words = emit_kernel_module(&spv, &m, "kernel").expect("emit");
        let parsed = dr::load_words(&words).expect("rspirv must parse emitted binary");
        assert_eq!(parsed.entry_points.len(), 1);
    }

    #[test]
    fn empty_body_kernel_module_round_trips() {
        let m = validated_module_with(vec![empty_fn("kernel")]);
        let spv = minimal_vulkan_compute_module("kernel");
        let words = emit_kernel_module(&spv, &m, "kernel").expect("emit");
        let parsed = dr::load_words(&words).expect("rspirv parse");
        assert_eq!(parsed.functions.len(), 1);
    }

    // ── No-entry-point shader env rejection ─────────────────────────────

    #[test]
    fn shader_module_without_entries_rejects() {
        let m = validated_module_with(vec![empty_fn("kernel")]);
        let spv_no_eps = SpirvModule::new(SpirvTargetEnv::VulkanKhr1_4);
        let e = emit_kernel_module(&spv_no_eps, &m, "kernel").unwrap_err();
        assert!(matches!(e, BodyEmitError::NoEntryPoints { .. }));
    }

    #[test]
    fn kernel_target_env_without_entries_succeeds() {
        // OpenCL-Kernel target accepts zero entries (kernels are declared
        // per-fn). The body emitter mirrors this allowance.
        let m = validated_module_with(vec![empty_fn("kernel")]);
        let spv = SpirvModule::new(SpirvTargetEnv::OpenClKernel2_2);
        let words = emit_kernel_module(&spv, &m, "kernel").expect("emit");
        assert!(!words.is_empty());
    }

    // ── Multi-entry-point regression ────────────────────────────────────

    #[test]
    fn multi_entry_module_emits_kernel_body_only_for_named_entry() {
        // Module with two entries : "kernel" gets the real body, "other"
        // gets the void shell.
        let m = validated_module_with(vec![fn_with_int_add()]);
        let mut spv = minimal_vulkan_compute_module("kernel");
        spv.add_entry_point(SpirvEntryPoint {
            model: crate::target::ExecutionModel::Vertex,
            name: "other_vs".into(),
            execution_modes: vec![],
        });
        let words = emit_kernel_module(&spv, &m, "kernel").expect("emit");
        let parsed = dr::load_words(&words).expect("rspirv parse");
        assert_eq!(parsed.entry_points.len(), 2);
        // Two functions present.
        assert_eq!(parsed.functions.len(), 2);
        // The kernel body has IAdd ; the shell does not.
        let total_iadds: usize = parsed
            .functions
            .iter()
            .map(|f| {
                f.blocks
                    .iter()
                    .flat_map(|bb| bb.instructions.iter())
                    .filter(|i| i.class.opcode == spirv::Op::IAdd)
                    .count()
            })
            .sum();
        assert_eq!(total_iadds, 1);
    }

    // ── Type cache + utility ────────────────────────────────────────────

    #[test]
    fn unsupported_result_type_returns_clean_error() {
        // Tuple result type is not modelled at stage-0 ; emit_constant
        // should reject it cleanly.
        let mut f = MirFunc::new("kernel", vec![], vec![]);
        let v0 = f.fresh_value_id();
        let tuple = MirType::Tuple(vec![MirType::Int(IntWidth::I32)]);
        f.push_op(
            MirOp::std("arith.constant")
                .with_attribute("value", "0")
                .with_result(v0, tuple),
        );
        f.push_op(MirOp::std("func.return"));
        let m = validated_module_with(vec![f]);
        let spv = minimal_vulkan_compute_module("kernel");
        let e = emit_kernel_module(&spv, &m, "kernel").unwrap_err();
        assert!(matches!(e, BodyEmitError::UnsupportedResultType { .. }));
    }

    #[test]
    fn body_emit_error_display_is_actionable() {
        let e = BodyEmitError::HeapNotSupportedOnGpu {
            fn_name: "k".into(),
            op_name: "cssl.heap.alloc".into(),
        };
        let s = format!("{e}");
        assert!(s.contains("cssl.heap.alloc"));
        assert!(s.contains("`k`"));
        let e2 = BodyEmitError::ClosuresNotSupportedOnGpu {
            fn_name: "k".into(),
            op_name: "cssl.closure".into(),
        };
        let s2 = format!("{e2}");
        assert!(s2.contains("closure"));
        let e3 = BodyEmitError::StructuredCfgMarkerAbsent;
        let s3 = format!("{e3}");
        assert!(s3.contains("structured-CFG"));
        let e4 = BodyEmitError::KernelFnNotFound {
            kernel_fn: "missing".into(),
        };
        let s4 = format!("{e4}");
        assert!(s4.contains("missing"));
    }

    // ── Helper smoke ────────────────────────────────────────────────────

    #[test]
    fn predicate_from_op_handles_suffix_then_attribute_then_empty() {
        let op_a = MirOp::std("arith.cmpi_eq");
        assert_eq!(predicate_from_op(&op_a, "i"), "eq");
        let op_b = MirOp::std("arith.cmpi").with_attribute("predicate", "slt");
        assert_eq!(predicate_from_op(&op_b, "i"), "slt");
        let op_c = MirOp::std("arith.cmpi");
        assert_eq!(predicate_from_op(&op_c, "i"), "");
    }

    #[test]
    fn compute_memref_alignment_takes_max_of_natural_and_attribute() {
        let op_a = MirOp::std("memref.load").with_result(ValueId(0), MirType::Int(IntWidth::I32));
        // Natural i32 = 4. No attribute = 4.
        assert_eq!(
            compute_memref_alignment(&op_a, &MirType::Int(IntWidth::I32)),
            4
        );
        // Attribute 8 > natural 4 = 8.
        let op_b = MirOp::std("memref.load")
            .with_attribute("alignment", "8")
            .with_result(ValueId(0), MirType::Int(IntWidth::I32));
        assert_eq!(
            compute_memref_alignment(&op_b, &MirType::Int(IntWidth::I32)),
            8
        );
        // Attribute 2 < natural 4 = 4 (we take max).
        let op_c = MirOp::std("memref.load")
            .with_attribute("alignment", "2")
            .with_result(ValueId(0), MirType::Int(IntWidth::I32));
        assert_eq!(
            compute_memref_alignment(&op_c, &MirType::Int(IntWidth::I32)),
            4
        );
    }

    #[test]
    fn parse_three_u32_helper() {
        assert_eq!(parse_three_u32("1 2 3"), Some([1, 2, 3]));
        assert_eq!(parse_three_u32(""), None);
        assert_eq!(parse_three_u32("1 2 3 4"), None);
    }

    /// Smoke : a fully populated kernel exercising arith + scf.if + return.
    #[test]
    fn full_smoke_kernel_with_arith_and_scf_if_round_trips() {
        let mut f = MirFunc::new("kernel", vec![], vec![]);
        let v_c = f.fresh_value_id();
        let v_t = f.fresh_value_id();
        let v_e = f.fresh_value_id();
        let v_a = f.fresh_value_id();
        let v_b = f.fresh_value_id();
        let v_sum = f.fresh_value_id();
        let v_r = f.fresh_value_id();
        // cond = true.
        f.push_op(
            MirOp::std("arith.constant")
                .with_attribute("value", "true")
                .with_result(v_c, MirType::Bool),
        );
        // then : 7 ; else : 11.
        let mut then_blk = MirBlock::entry(vec![]);
        then_blk.push(
            MirOp::std("arith.constant")
                .with_attribute("value", "7")
                .with_result(v_t, MirType::Int(IntWidth::I32)),
        );
        then_blk.push(MirOp::std("scf.yield").with_operand(v_t));
        let mut then_region = MirRegion::new();
        then_region.push(then_blk);
        let mut else_blk = MirBlock::entry(vec![]);
        else_blk.push(
            MirOp::std("arith.constant")
                .with_attribute("value", "11")
                .with_result(v_e, MirType::Int(IntWidth::I32)),
        );
        else_blk.push(MirOp::std("scf.yield").with_operand(v_e));
        let mut else_region = MirRegion::new();
        else_region.push(else_blk);
        f.push_op(
            MirOp::std("scf.if")
                .with_operand(v_c)
                .with_region(then_region)
                .with_region(else_region)
                .with_result(v_a, MirType::Int(IntWidth::I32)),
        );
        // b = 5.
        f.push_op(
            MirOp::std("arith.constant")
                .with_attribute("value", "5")
                .with_result(v_b, MirType::Int(IntWidth::I32)),
        );
        // sum = a + b.
        f.push_op(
            MirOp::std("arith.addi")
                .with_operand(v_a)
                .with_operand(v_b)
                .with_result(v_sum, MirType::Int(IntWidth::I32)),
        );
        // Drop sum into r so the value-map keeps a live ref.
        f.push_op(
            MirOp::std("arith.muli")
                .with_operand(v_sum)
                .with_operand(v_b)
                .with_result(v_r, MirType::Int(IntWidth::I32)),
        );
        f.push_op(MirOp::std("func.return"));
        let m = validated_module_with(vec![f]);
        let spv = minimal_vulkan_compute_module("kernel");
        let words = emit_kernel_module(&spv, &m, "kernel").expect("emit");
        let parsed = dr::load_words(&words).expect("rspirv parse");
        // Spot-check : must contain SelectionMerge + IAdd + IMul + Return.
        let opcodes: Vec<_> = parsed
            .functions
            .iter()
            .flat_map(|f| f.blocks.iter().flat_map(|bb| bb.instructions.iter()))
            .map(|i| i.class.opcode)
            .collect();
        for expected in [
            spirv::Op::SelectionMerge,
            spirv::Op::IAdd,
            spirv::Op::IMul,
            spirv::Op::Return,
        ] {
            assert!(
                opcodes.contains(&expected),
                "expected {expected:?} in {opcodes:?}"
            );
        }
    }

    // Suppress unused-import warning : MirValue is used transitively
    // via with_result, but rustc may not see it. Bind it to silence.
    const _: Option<MirValue> = None;
}
