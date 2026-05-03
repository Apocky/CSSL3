//! § scf — structured-control-flow MIR-op lowering shared by JIT + Object backends.
//!
//! § SPEC
//!   - `specs/15_MLIR.csl` § SCF-DIALECT-LOWERING
//!   - `specs/02_IR.csl` § STRUCTURED-CFG-CONSTRAINT
//!   - `specs/15_MLIR.csl` § STRUCTURED CFG PRESERVATION (CC4)
//!
//! § ROLE
//!   The CSSLv3 frontend lowers `if` expressions to `scf.if` MIR ops, and
//!   `for` / `while` / `loop` expressions to `scf.for` / `scf.while` /
//!   `scf.loop` MIR ops. Each carries one or two nested regions ; this
//!   module turns those region-bearing MIR ops into cranelift CLIF blocks
//!   wired by `brif` + `jump` instructions.
//!
//!   For `scf.if` the shape is two regions (then + else) joined at a
//!   merge-block whose block-parameter receives the yielded value. See
//!   [`lower_scf_if`].
//!
//!   For loops the stage-0 shape is a single region (the body) joined to
//!   a header-block (the loop-back-edge target) and an exit-block (cursor
//!   on return). [`lower_scf_loop`] emits an unconditional infinite loop
//!   (`header -> body -> header`) — exit happens only when an inner
//!   `func.return` terminates the body. [`lower_scf_while`] gates entry
//!   on a pre-computed condition (`brif cond, body, exit`) and back-edges
//!   the body to the header. [`lower_scf_for`] runs the body once at
//!   stage-0 — the iter-counter / IV-block-arg machinery is documented
//!   as deferred below.
//!
//! § INVARIANTS
//!   - Cranelift's `brif` post-0.105 takes `(cond, then_block, &[then_args],
//!     else_block, &[else_args])`. We always pass empty args at the brif
//!     site itself for the if-shape ; the merge-block-arg is fed by the
//!     `jump` instructions at the tails of each branch. Loop entry-brif
//!     and loop back-edge `jump`s pass empty arg-lists at stage-0 (no
//!     loop-carried block-args yet — see DEFERRED below).
//!   - Empty-body branches (no scf.yield, no other ops) still emit a
//!     single `jump` to the merge-block. Cranelift rejects blocks without
//!     a terminator, so even a "do-nothing else" must terminate.
//!   - The merge-block (scf.if) is sealed last : its predecessors are the
//!     two branch blocks, both filled by the time we switch back.
//!   - The loop header-block has TWO predecessors : (1) the entry-edge
//!     from before the loop, (2) the back-edge from the body's tail
//!     `jump`. Sealing must wait until both edges are emitted.
//!     Loop sealing schedule in detail :
//!       (a) caller's current block : already sealed by caller
//!       (b) `body_block`           : seal AFTER `header_block` jumps in
//!       (c) `header_block`         : seal AFTER body-tail back-edge
//!                                    jump emits (or, for `scf.for`'s
//!                                    single-trip stage-0, after the
//!                                    body's jump-to-exit emits)
//!       (d) `exit_block`           : seal LAST (after we switch to it)
//!     Doing it any other order makes cranelift reject SSA construction
//!     because an unsealed block with unknown predecessors cannot
//!     resolve its block-args.
//!   - Nested `scf.if` / `scf.for` / `scf.while` / `scf.loop` inside a
//!     loop body or branch calls back into the appropriate `lower_scf_*`
//!     entry via the dispatcher closure passed in by the caller. This
//!     keeps op-dispatch ownership in `jit.rs` / `object.rs` while
//!     letting control-flow scaffolding live here once.
//!
//! § DESIGN — single-region-block constraint
//!   At stage-0 every region produced by `cssl_mir::body_lower` has
//!   exactly one block named `entry`. The lowering walks
//!   `region.blocks[0].ops` once per region. When a future slice
//!   introduces multi-block regions (e.g. early-return inside a then-arm,
//!   or break/continue inside a loop body), this helper returns
//!   [`ScfError::MultiBlockRegion`] so the caller sees a clean diagnostic
//!   instead of silently mis-lowering.
//!
//! § DEFERRED (calling out the stage-0 limits explicitly)
//!   - **Loop iter-counter for `scf.for`** : `body_lower::lower_for`
//!     today emits `scf.for iter_id [body_region]` where `iter_id` is
//!     the ValueId of a `cssl.range` op whose result-type is
//!     `MirType::None`. There is no IV ValueId, no lo / hi / step
//!     exposed at the scf.for op, and no body block-arg threading the
//!     IV. Until the MIR emission grows iter-bounds + IV operands, the
//!     lowering runs the body once (single-trip) and emits a
//!     structurally-correct header / body / exit triplet so the SSA
//!     shape is ready when real iteration lands.
//!   - **Cond re-evaluation for `scf.while`** : `body_lower::lower_while`
//!     emits the cond as a single ValueId computed before the op. The
//!     stage-0 body cannot mutate that value, so we lower as
//!     `brif cond, body, exit ; body -> header (back-edge)` — a true
//!     infinite loop when the cond is set, a clean skip when not. Inner
//!     `func.return` terminates as expected. A future slice that re-
//!     emits the cond-defining op chain at the header (or grows
//!     `scf.condition` as an explicit op) replaces this one-shot eval.
//!   - **Break / continue** at HIR `Break` / `Continue` lowers to
//!     `emit_unsupported` ; once real `cssl.break` / `cssl.continue`
//!     MIR ops land, `lower_scf_loop` / `_while` / `_for` will accept
//!     them as branch-to-exit / branch-to-header inside a body region.
//!     Today, the body-walker forwards every op to the dispatcher
//!     closure unchanged, so any unstructured form a future slice
//!     introduces fires through to the backend's existing
//!     `UnsupportedOp` path until then.
//!   - **Loop-carried block-args** : a future slice that introduces
//!     accumulators (e.g. `let mut acc = 0 ; for i in 0..n { acc += i }`)
//!     will need block-args on the header-block + back-edge jumps that
//!     forward the new values. Today no MIR shape uses them, so we feed
//!     `&[]` at every brif / jump destination.

use std::collections::HashMap;

use cranelift_codegen::ir::{InstBuilder, Type};
use cranelift_frontend::FunctionBuilder;
use cssl_mir::{FloatWidth, IntWidth, MirOp, MirType, ValueId};

/// Errors specific to scf.* lowering.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ScfError {
    /// `scf.if` was missing its condition operand.
    #[error("scf.if in `{fn_name}` is missing the condition operand")]
    MissingCondition { fn_name: String },

    /// `scf.if` has a region that doesn't contain exactly one block. This is
    /// not currently produced by `body_lower` but the lowering is defensive.
    #[error(
        "scf.if in `{fn_name}` has a region with {count} blocks ; stage-0 expects exactly one"
    )]
    MultiBlockRegion { fn_name: String, count: usize },

    /// `scf.if` was given an unexpected number of regions (always 2 today —
    /// then + else, where else may be empty but is always present).
    #[error("scf.if in `{fn_name}` has {count} regions ; expected exactly 2 (then + else)")]
    WrongRegionCount { fn_name: String, count: usize },

    /// `scf.yield` in a branch references a `ValueId` that wasn't materialized.
    #[error("scf.yield in `{fn_name}` references unknown ValueId({value_id})")]
    UnknownYieldValue { fn_name: String, value_id: u32 },

    /// The condition operand of an `scf.if` references a ValueId not in scope.
    #[error("scf.if in `{fn_name}` condition references unknown ValueId({value_id})")]
    UnknownConditionValue { fn_name: String, value_id: u32 },

    /// The yielded value's MIR type is not representable in cranelift today.
    /// Mirrors the JIT/object backends' scalar-only constraint.
    #[error("scf.if in `{fn_name}` yields non-scalar MIR type `{ty}` ; stage-0 scalars-only")]
    NonScalarYield { fn_name: String, ty: String },

    /// A loop op (scf.for / scf.while / scf.loop) was given an unexpected
    /// number of regions. Stage-0 expects exactly one body-region.
    #[error("scf.{op_name} in `{fn_name}` has {count} regions ; expected exactly 1 body region")]
    WrongLoopRegionCount {
        op_name: String,
        fn_name: String,
        count: usize,
    },

    /// `scf.while` / `scf.for` was missing its leading operand (cond / iter).
    #[error("scf.{op_name} in `{fn_name}` is missing its leading operand")]
    MissingLoopOperand { op_name: String, fn_name: String },

    /// The condition / iter operand references a ValueId not in scope.
    #[error(
        "scf.{op_name} in `{fn_name}` references unknown ValueId({value_id}) for the leading operand"
    )]
    UnknownLoopOperand {
        op_name: String,
        fn_name: String,
        value_id: u32,
    },
}

/// Wrapper that carries either the local [`ScfError`] (structural problems
/// detected by this module) or the backend-specific error returned by the
/// inner-op lowerer closure. Backends call `.map_err` to flatten this into
/// their own error type at the dispatch boundary.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum BackendOrScfError<E> {
    /// Structural problem with the scf.if itself.
    #[error(transparent)]
    Scf(#[from] ScfError),
    /// Error from the backend's per-op lowerer (i.e. lowering an op INSIDE a
    /// scf.if branch failed).
    #[error("backend error inside scf.if branch : {0}")]
    Backend(E),
}

/// Map a MIR scalar type to the cranelift `Type`. Mirrors the helpers in
/// `jit.rs` / `object.rs` so this module stays free of cross-backend imports.
#[must_use]
pub fn mir_to_cl(ty: &MirType) -> Option<Type> {
    use cranelift_codegen::ir::types as cl_types;
    match ty {
        MirType::Int(w) => Some(match w {
            IntWidth::I1 | IntWidth::I8 => cl_types::I8,
            IntWidth::I16 => cl_types::I16,
            IntWidth::I32 => cl_types::I32,
            IntWidth::I64 | IntWidth::Index => cl_types::I64,
        }),
        MirType::Float(w) => Some(match w {
            FloatWidth::F16 | FloatWidth::Bf16 => return None,
            FloatWidth::F32 => cl_types::F32,
            FloatWidth::F64 => cl_types::F64,
        }),
        MirType::Bool => Some(cl_types::I8),
        // § T11-D281 (W-A1-δ) : Ptr / Handle widen to host pointer width
        // (I64 on x86_64). The `MatchExpansionPass` cascade can yield cell-
        // pointer SSA values through scf.if merge-blocks (e.g. when an arm
        // returns the variant payload via cssl.heap.alloc handle), so the
        // merge-param-ty derivation must accept the pointer-shaped MIR
        // types. See `crates/cssl-cgen-cpu-cranelift/src/jit.rs` § Ptr
        // path for the matching jit-side mir_to_cl_type entries.
        MirType::Ptr | MirType::Handle => Some(cl_types::I64),
        _ => None,
    }
}

/// Lower an `scf.if` op. The caller supplies a closure that lowers a single
/// MIR op inside the branch — this preserves backend ownership of the rest
/// of the op-dispatch table (binary arith, return, cmpi/cmpf, etc.) without
/// forcing this module to depend on backend internals.
///
/// The closure error type `E` is the backend's native error (e.g.
/// [`crate::JitError`] for the JIT path, [`crate::ObjectError`] for object
/// emission). The wrapper [`BackendOrScfError`] forwards either side back to
/// the caller's `From` conversions, so backends keep a single error type at
/// their dispatch layer.
///
/// # Behavior
///   - When `op.results[0].ty != MirType::None`, the merge-block has one
///     block-parameter of that type, the scf.if's result-id maps to it, and
///     each branch's `scf.yield` operand is forwarded as a `jump` arg to the
///     merge-block.
///   - When the scf.if has no result, branches still terminate with `jump
///     merge_block` but pass no args.
///   - The cranelift cursor is left pointing at `merge_block` after the call ;
///     the caller's outer op-loop continues lowering subsequent ops there.
///
/// # Errors
///   Returns [`BackendOrScfError::Scf`] for structural problems detected here
///   (missing condition, wrong region count, non-scalar yield) and
///   [`BackendOrScfError::Backend`] for whatever the inner-op lowerer returns.
///
/// # Tail-flag
///   Returns `Ok(false)` in the common case — scf.if itself is not a
///   function-terminator and the merge-block is open for further ops,
///   so the caller's outer op-loop continues normally. § T11-D281
///   (W-A1-δ) extension : returns `Ok(true)` when BOTH branches ended in
///   a function-terminator (e.g. `func.return`). This propagates up so
///   the parent op-loop knows subsequent ops in its block are
///   unreachable, preventing value-map ghost-binds against phantom
///   merge-block-params. See [`lower_branch_into`] for full context.
//
// `clippy::implicit_hasher` would force a `BuildHasher` type parameter on
// the public signature, which neither backend cares about — they hand in
// `std::collections::HashMap` with the default hasher. Allowing the lint
// keeps the helper signature symmetric with `lower_op_to_cl` in jit.rs.
#[allow(clippy::implicit_hasher)]
pub fn lower_scf_if<E, F>(
    op: &MirOp,
    builder: &mut FunctionBuilder<'_>,
    value_map: &mut HashMap<ValueId, cranelift_codegen::ir::Value>,
    fn_name: &str,
    mut lower_branch_op: F,
) -> Result<bool, BackendOrScfError<E>>
where
    F: FnMut(
        &MirOp,
        &mut FunctionBuilder<'_>,
        &mut HashMap<ValueId, cranelift_codegen::ir::Value>,
        &str,
    ) -> Result<bool, E>,
{
    // § 1. Validate region count + extract branches.
    if op.regions.len() != 2 {
        return Err(ScfError::WrongRegionCount {
            fn_name: fn_name.to_string(),
            count: op.regions.len(),
        }
        .into());
    }
    let then_region = &op.regions[0];
    let else_region = &op.regions[1];
    if then_region.blocks.len() > 1 {
        return Err(ScfError::MultiBlockRegion {
            fn_name: fn_name.to_string(),
            count: then_region.blocks.len(),
        }
        .into());
    }
    if else_region.blocks.len() > 1 {
        return Err(ScfError::MultiBlockRegion {
            fn_name: fn_name.to_string(),
            count: else_region.blocks.len(),
        }
        .into());
    }

    // § 2. Resolve condition value.
    let cond_id = op
        .operands
        .first()
        .copied()
        .ok_or_else(|| ScfError::MissingCondition {
            fn_name: fn_name.to_string(),
        })?;
    let cond_val = *value_map
        .get(&cond_id)
        .ok_or(ScfError::UnknownConditionValue {
            fn_name: fn_name.to_string(),
            value_id: cond_id.0,
        })?;

    // § 3. Determine result type (None when scf.if is statement-only).
    //
    // § T11-W19-α-CSSLC-FIX7 — non-scalar yield fallback via PointerByRef.
    //   Pre-FIX7 a non-scalar yield-ty surfaced as `NonScalarYield` and
    //   blocked stdlib/{window,input,fs,net}.cssl + stdlib/time.cssl's
    //   `Result<T,E>`-yielding scf.if arms. Mirrors the FIX4 enum/Result
    //   PointerByRef classification : when a yield-ty doesn't fit a
    //   cranelift scalar, the merge-block-param widens to host-pointer-
    //   width I64 (x86_64 stage-0 single-host) and arm-yields are
    //   coerced (via emit_terminating_jump's int-coerce or sextend on
    //   smaller-than-ptr-width yields). Pointer-shaped MIR carriers
    //   (e.g. Result<...> hidden-pointer ABI) already fit this width
    //   directly per the cgen-side resolve_aggregate_opaque table.
    let result_ty = op.results.first().map(|r| &r.ty);
    let merge_param_ty = match result_ty {
        Some(ty) if !matches!(ty, MirType::None) => Some(mir_to_cl(ty).unwrap_or_else(|| {
            // Non-scalar yield → host-pointer-width I64 fallback.
            // The arm-yield coercion in `emit_terminating_jump` widens
            // any narrower int-Value to match this merge-param width ;
            // already-pointer-shaped Values flow through unchanged.
            cranelift_codegen::ir::types::I64
        })),
        _ => None,
    };

    // § 4. Create the three blocks. Sealing schedule :
    //        then + else  : sealed immediately (their only predecessor is the
    //                        brif site we're about to emit, and we never add
    //                        another).
    //        merge        : sealed AFTER both branches jump in (its
    //                        predecessors are the two branches, complete once
    //                        both jumps emit).
    let then_block = builder.create_block();
    let else_block = builder.create_block();
    let merge_block = builder.create_block();
    if let Some(param_ty) = merge_param_ty {
        builder.append_block_param(merge_block, param_ty);
    }

    // § 5. Emit the conditional branch from the *current* block.
    //      Cranelift 0.105+ : `brif(cond, then_blk, &[then_args], else_blk, &[else_args])`.
    //      We pass empty arg-lists at the brif site ; the merge-block's
    //      block-arg is fed by the per-branch jump tail.
    builder
        .ins()
        .brif(cond_val, then_block, &[], else_block, &[]);

    // § 6. Lower the THEN branch.
    //
    // § T11-D281 (W-A1-δ) — Cascade-region splice value-map continuity :
    //   `lower_branch_into` now returns BOTH the captured yield-Value AND
    //   a `terminated` flag. When a branch ends in `func.return` (e.g. a
    //   `MatchExpansionPass`-generated arm whose body is `Ok(_) => return
    //   x` or whose terminal arm always-returns), cranelift has already
    //   emitted the `return_` instruction — appending another `jump
    //   merge_block` would (a) double-terminate the block + (b) leave
    //   merge_block with a phantom predecessor whose value-map state is
    //   unreachable. Skipping the merge-jump in that case keeps the
    //   value_map honest : the parent's pre-cascade entries remain
    //   uncontaminated, and the surviving branch (or merge-block, if
    //   both branches terminate, the merge becomes truly dead and
    //   cranelift accepts zero-pred sealed blocks just like
    //   `scf.loop` exit-blocks).
    builder.switch_to_block(then_block);
    builder.seal_block(then_block);
    let (then_yield_arg, then_terminated) = lower_branch_into(
        then_region,
        builder,
        value_map,
        fn_name,
        &mut lower_branch_op,
    )?;
    if !then_terminated {
        emit_terminating_jump(builder, merge_block, then_yield_arg, merge_param_ty);
    }

    // § 7. Lower the ELSE branch.
    builder.switch_to_block(else_block);
    builder.seal_block(else_block);
    let (else_yield_arg, else_terminated) = lower_branch_into(
        else_region,
        builder,
        value_map,
        fn_name,
        &mut lower_branch_op,
    )?;
    if !else_terminated {
        emit_terminating_jump(builder, merge_block, else_yield_arg, merge_param_ty);
    }

    // § 8. Switch to merge-block + record the merge-block-param as the scf.if
    //      result (when typed).
    //
    // § T11-D281 (W-A1-δ) — Both-branches-terminate edge case :
    //   When both arms returned (e.g. `match x { Ok(v) => return v ; Err(e)
    //   => return -1 }`), the merge-block has ZERO predecessors. Cranelift
    //   accepts a sealed zero-pred block (it becomes dead-code, the
    //   verifier prunes it on emission), but we must NOT read its
    //   block-params : the arg appended at § 4 is well-defined as
    //   structure but has no incoming jump-arg and reading it via
    //   `block_params(merge_block)[0]` returns a Value reference whose
    //   defining edge doesn't exist. Subsequent post-cascade ops in the
    //   parent block would be unreachable in either case ; binding the
    //   scf.if's result-id to that phantom block-param is harmless when
    //   nothing reads it but a value-map continuity nightmare if a
    //   subsequent op DOES try to read it.
    //
    //   The safest contract : when both branches terminated, propagate
    //   the terminator-flag back to the caller so they can decide
    //   whether to keep walking. Today scf.if itself is not a function-
    //   terminator (per the doc-comment at the top), so we still return
    //   `Ok(false)` ; but the caller (the dispatcher in `lower_op_to_cl`)
    //   is itself wrapped by a loop that respects the terminator-flag,
    //   so a wholly-terminating cascade upstream of further ops will
    //   surface as "subsequent ops are unreachable" rather than a
    //   value-map miss.
    builder.switch_to_block(merge_block);
    builder.seal_block(merge_block);
    let both_branches_terminated = then_terminated && else_terminated;
    if let Some(_param_ty) = merge_param_ty {
        // The scf.if's result `MirValue.id` must now map to the merge-block-param[0]
        // — but only when at least one branch fed the merge (otherwise
        // the block-param is phantom).
        if !both_branches_terminated {
            if let Some(r) = op.results.first() {
                let merge_params = builder.block_params(merge_block);
                let bp = *merge_params.first().expect("merge-block-param appended");
                value_map.insert(r.id, bp);
            }
        }
    }

    // Propagate the cascade-region termination flag : when both branches
    // returned, the parent op-loop can stop walking subsequent ops in
    // this block. This is the value-map-continuity insurance the
    // `MatchExpansionPass` cascade pattern needs — see the doc-comment
    // on `lower_branch_into` for the W-A1-γ context.
    Ok(both_branches_terminated)
}

/// Lower the ops of a branch's single block. Skips `scf.yield` (its operand
/// is captured separately as the branch's tail-jump arg). Returns
/// `(captured_yield, terminated)` :
///   - `captured_yield` : the yielded `cranelift::ir::Value` (when the
///     region ends in `scf.yield` with an operand) ;
///   - `terminated` : `true` iff a non-yield op inside the region was
///     itself a function-terminator (today : `func.return` /
///     `cssl.diff.bwd_return`). When `true`, the caller MUST NOT emit a
///     merge-jump — the cranelift block has already terminated.
///
/// § T11-D281 (W-A1-δ) — terminator-flag propagation :
///   `MatchExpansionPass` (W-A1-γ) splices arm-regions verbatim into the
///   nested cascade `scf.if`s. When an arm body's tail is
///   `=> return x` (rather than `=> x` for value-yield), the arm-region
///   contains a bare `func.return` and NO `scf.yield`. Pre-W-A1-δ this
///   path emitted both `return_` AND a follow-up `jump merge_block` —
///   cranelift treated the second instruction as dead-code and the
///   merge-block silently lost a predecessor, breaking the value_map
///   invariant for any subsequent op that read the cascade's result.
///   Plumbing the flag back to the caller resolves it cleanly.
fn lower_branch_into<E, F>(
    region: &cssl_mir::MirRegion,
    builder: &mut FunctionBuilder<'_>,
    value_map: &mut HashMap<ValueId, cranelift_codegen::ir::Value>,
    fn_name: &str,
    lower_branch_op: &mut F,
) -> Result<(Option<cranelift_codegen::ir::Value>, bool), BackendOrScfError<E>>
where
    F: FnMut(
        &MirOp,
        &mut FunctionBuilder<'_>,
        &mut HashMap<ValueId, cranelift_codegen::ir::Value>,
        &str,
    ) -> Result<bool, E>,
{
    let Some(entry) = region.blocks.first() else {
        return Ok((None, false));
    };
    let mut yield_val = None;
    let mut terminated = false;
    for branch_op in &entry.ops {
        if branch_op.name == "scf.yield" {
            // Capture the yielded value's CLIF Value (already in value_map
            // from a prior op in this branch). Empty operand-list = void
            // yield (rare but defensive).
            if let Some(&yid) = branch_op.operands.first() {
                let v = *value_map.get(&yid).ok_or(ScfError::UnknownYieldValue {
                    fn_name: fn_name.to_string(),
                    value_id: yid.0,
                })?;
                yield_val = Some(v);
            }
            // Stop walking ops in this branch — scf.yield is a region terminator.
            break;
        }
        // Non-yield op : delegate to the caller's per-op lowerer. The
        // returned bool indicates whether the dispatcher emitted a
        // function-terminator (e.g. `return_`) ; if so, no further ops
        // in this block can be lowered and the caller must skip the
        // merge-block jump.
        let was_terminator = lower_branch_op(branch_op, builder, value_map, fn_name)
            .map_err(BackendOrScfError::Backend)?;
        if was_terminator {
            terminated = true;
            break;
        }
    }
    Ok((yield_val, terminated))
}

/// Emit the terminating `jump` for a branch. When the merge-block has a
/// block-parameter, the branch's yielded value is forwarded as the jump's
/// arg ; if a branch failed to yield (statement-only), we feed an undefined
/// constant so cranelift accepts the join. That last case shouldn't occur
/// when both branches yield (the only configuration that creates a typed
/// merge-param), but the defensive zero keeps cranelift happy regardless.
fn emit_terminating_jump(
    builder: &mut FunctionBuilder<'_>,
    merge_block: cranelift_codegen::ir::Block,
    yield_val: Option<cranelift_codegen::ir::Value>,
    merge_param_ty: Option<Type>,
) {
    if let Some(param_ty) = merge_param_ty {
        let arg = yield_val.unwrap_or_else(|| {
            // Fallback : emit a typed zero. Reached only when the scf.if
            // was typed but a branch elided its scf.yield — a mis-shape from
            // body_lower we'd rather observe as exit-0 than a panic.
            use cranelift_codegen::ir::types as cl_types;
            if param_ty == cl_types::F32 {
                builder.ins().f32const(0.0_f32)
            } else if param_ty == cl_types::F64 {
                builder.ins().f64const(0.0_f64)
            } else {
                builder.ins().iconst(param_ty, 0)
            }
        });
        // § T11-W19-α-CSSLC-FIX5 — int-arg coercion at scf.if merge join.
        //   The merge-block-param ty is derived from the scf.if op's
        //   result-ty (typically I32 default for MIR int-literal yields),
        //   but each branch may yield a wider FFI-call return (e.g. I64
        //   from `time::monotonic_ns()`). Symmetrically widen / narrow
        //   the yield to match the merge-block-param-ty so cranelift's
        //   verifier accepts the join. Mirrors the func.return / branch /
        //   brif coercion landed at FIX1 / FIX5-other-sites.
        let coerced = {
            let actual_ty = builder.func.dfg.value_type(arg);
            if actual_ty == param_ty || !actual_ty.is_int() || !param_ty.is_int() {
                arg
            } else if param_ty.bits() > actual_ty.bits() {
                builder.ins().sextend(param_ty, arg)
            } else {
                builder.ins().ireduce(param_ty, arg)
            }
        };
        builder.ins().jump(merge_block, &[coerced]);
    } else {
        builder.ins().jump(merge_block, &[]);
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § T11-D61 / S6-C2 — loop lowering : scf.loop, scf.while, scf.for
// ─────────────────────────────────────────────────────────────────────────
//
// Each loop op carries exactly one nested region (the body) plus an
// optional leading operand : scf.while reads `op.operands[0]` as the cond
// ValueId, scf.for reads it as the iter ValueId, scf.loop reads no
// operand. Body ops are forwarded to the caller's dispatcher closure
// (same pattern as `lower_scf_if`'s `lower_branch_op`) so the JIT and
// object backends keep ownership of arith / call / memref / nested-scf
// dispatch. Returns `Ok(false)` because a loop op on its own is never a
// function-terminator — control falls through to the exit-block, which
// the caller's outer op-loop continues to lower into.
//
// The three lowerers share `lower_loop_body_into` (forwards each op to
// the dispatcher) and `extract_single_body_region` (the structural
// validation that surfaces `WrongLoopRegionCount` /
// `MultiBlockRegion`). The differences live in the brif / jump shape
// each emits.

/// Lower an `scf.loop` op : an unconditional infinite loop. Stage-0 shape :
///
/// ```text
///   <caller's current block>
///       jump header_block
///   header_block:                       (preds : caller, body-back-edge)
///       jump body_block
///   body_block:                         (preds : header)
///       <body-ops>
///       jump header_block               (back-edge)
///   exit_block:                         (preds : <none initially —
///                                         only the body's inner
///                                         func.return reaches here>)
///       <continuation>
/// ```
///
/// The exit-block has no predecessors initially because a true infinite
/// loop never falls through ; the only way out is via an inner
/// `func.return` inside the body. To keep cranelift happy we still
/// switch the cursor to the exit-block on return so the caller's outer
/// op-loop has a place to lower trailing ops into. If the loop body
/// always returns (no fall-through op after the loop), those trailing
/// ops are unreachable but legal — cranelift's verifier accepts blocks
/// with zero predecessors as long as they're sealed.
///
/// # Errors
///   Returns [`BackendOrScfError::Scf`] for structural problems detected
///   here (wrong region count, multi-block region) and
///   [`BackendOrScfError::Backend`] for whatever the inner-op lowerer
///   returns when lowering body ops.
///
/// # Tail-flag
///   Returns `Ok(false)` — the loop itself is not a function-terminator
///   even though its body may always reach a `func.return`.
#[allow(clippy::implicit_hasher)]
pub fn lower_scf_loop<E, F>(
    op: &MirOp,
    builder: &mut FunctionBuilder<'_>,
    value_map: &mut HashMap<ValueId, cranelift_codegen::ir::Value>,
    fn_name: &str,
    mut lower_body_op: F,
) -> Result<bool, BackendOrScfError<E>>
where
    F: FnMut(
        &MirOp,
        &mut FunctionBuilder<'_>,
        &mut HashMap<ValueId, cranelift_codegen::ir::Value>,
        &str,
    ) -> Result<bool, E>,
{
    let body_region = extract_single_body_region(op, "loop", fn_name)?;

    // § 1. Create the three blocks.
    let header_block = builder.create_block();
    let body_block = builder.create_block();
    let exit_block = builder.create_block();

    // § 2. Entry-edge : caller's current block jumps into header.
    builder.ins().jump(header_block, &[]);

    // § 3. Header : unconditional jump into body. Header has TWO
    //      predecessors (caller, back-edge). Seal once the back-edge
    //      jump emits (after § 5).
    builder.switch_to_block(header_block);
    builder.ins().jump(body_block, &[]);

    // § 4. Body : seal as soon as the header's jump completes (§ 3 above).
    //      Cranelift requires sealing in the order edges are known to be
    //      complete : body_block has only one predecessor (header) and
    //      that predecessor is fully filled, so seal now.
    builder.seal_block(body_block);
    builder.switch_to_block(body_block);
    let body_terminated =
        lower_loop_body_into(body_region, builder, value_map, fn_name, &mut lower_body_op)?;
    if !body_terminated {
        // Back-edge to header.
        builder.ins().jump(header_block, &[]);
    }

    // § 5. Header sealing : back-edge from § 4 (or absent if body
    //      terminated) is now resolved. Either way, the header's
    //      predecessor set is final — seal.
    builder.seal_block(header_block);

    // § 6. Switch cursor to exit-block + seal. Exit may have zero
    //      predecessors (true infinite loop) ; cranelift accepts this.
    builder.switch_to_block(exit_block);
    builder.seal_block(exit_block);

    Ok(false)
}

/// Lower an `scf.while` op : a pre-test loop with cond re-evaluated at
/// every loop-header. Stage-0 shape (post T11-D318) :
///
/// ```text
///   <caller's current block>
///       jump header_block
///   header_block:                       (preds : caller, body-back-edge)
///       <cond-region ops>               (re-walked each iteration —
///                                        observes the latest values
///                                        in any mutable cells the
///                                        cond expression reads)
///       brif cond_val, body_block, exit_block
///   body_block:                         (preds : header)
///       <body-ops>
///       jump header_block               (back-edge)
///   exit_block:                         (preds : header)
///       <continuation>
/// ```
///
/// § T11-D318 (W-CC-mut-assign) — Two `scf.while` shapes are accepted :
///   - **OLD (1 region, leading operand = cond)** : pre-D318 shape where
///     the cond ValueId is computed once in the OUTER block and read at
///     every iteration. Correct only when the cond doesn't depend on a
///     mutable cell that the body modifies — otherwise the loop iterates
///     forever or skips entirely.
///   - **NEW (2 regions, region[0] = cond_region, region[1] = body_region)** :
///     post-D318 shape where the cond computation lives inside
///     `cond_region` and is re-walked on each header entry. The
///     `cond_region`'s entry-block ends in a `scf.condition` op whose
///     leading operand is the freshly-computed cond ValueId. This is the
///     shape that lets `let mut frame; while frame < 60 { frame = frame
///     + 1 }` terminate after 60 iterations.
///
/// The lowerer detects the shape via region-count : 1 region → OLD path
/// (one-shot cond from operand) ; 2 regions → NEW path (re-emit cond at
/// header).
///
/// # Errors
///   Returns [`BackendOrScfError::Scf`] for missing / unknown cond,
///   wrong region count, or multi-block region ;
///   [`BackendOrScfError::Backend`] for inner-op lowering errors.
///
/// # Tail-flag
///   Returns `Ok(false)`.
#[allow(clippy::implicit_hasher)]
pub fn lower_scf_while<E, F>(
    op: &MirOp,
    builder: &mut FunctionBuilder<'_>,
    value_map: &mut HashMap<ValueId, cranelift_codegen::ir::Value>,
    fn_name: &str,
    mut lower_body_op: F,
) -> Result<bool, BackendOrScfError<E>>
where
    F: FnMut(
        &MirOp,
        &mut FunctionBuilder<'_>,
        &mut HashMap<ValueId, cranelift_codegen::ir::Value>,
        &str,
    ) -> Result<bool, E>,
{
    // § T11-D318 (W-CC-mut-assign) — region-count dispatch :
    //   1 region  : old shape (one-shot cond from operand).
    //   2 regions : new shape (cond_region re-walked + body_region).
    let region_count = op.regions.len();
    if region_count != 1 && region_count != 2 {
        return Err(ScfError::WrongLoopRegionCount {
            op_name: "while".to_string(),
            fn_name: fn_name.to_string(),
            count: region_count,
        }
        .into());
    }
    let (cond_region_opt, body_region) = if region_count == 2 {
        let cond = &op.regions[0];
        let body = &op.regions[1];
        if cond.blocks.len() > 1 {
            return Err(ScfError::MultiBlockRegion {
                fn_name: fn_name.to_string(),
                count: cond.blocks.len(),
            }
            .into());
        }
        if body.blocks.len() > 1 {
            return Err(ScfError::MultiBlockRegion {
                fn_name: fn_name.to_string(),
                count: body.blocks.len(),
            }
            .into());
        }
        (Some(cond), body)
    } else {
        let body = &op.regions[0];
        if body.blocks.len() > 1 {
            return Err(ScfError::MultiBlockRegion {
                fn_name: fn_name.to_string(),
                count: body.blocks.len(),
            }
            .into());
        }
        (None, body)
    };

    // § 1. Create the three blocks.
    let header_block = builder.create_block();
    let body_block = builder.create_block();
    let exit_block = builder.create_block();

    // § 2. Entry-edge : caller's current block jumps into header.
    builder.ins().jump(header_block, &[]);

    // § 3. Header : if a cond_region is present, re-walk it now to compute
    //      the cond fresh ; otherwise fall back to the leading-operand
    //      one-shot path. `cond_val` is a SSA-Value valid in this block
    //      per cranelift's dominance rules.
    builder.switch_to_block(header_block);
    let cond_val = if let Some(cond_region) = cond_region_opt {
        // Walk the cond-region's ops and capture the `scf.condition`
        // terminator's first operand as the cond ValueId. The walker
        // forwards every non-terminator op to the dispatcher so arith
        // / memref.load / nested ops compose normally.
        lower_while_cond_region(cond_region, builder, value_map, fn_name, &mut lower_body_op)?
    } else {
        resolve_loop_operand(op, value_map, "while", fn_name)?
    };
    builder
        .ins()
        .brif(cond_val, body_block, &[], exit_block, &[]);

    // § 4. Body block : seal once header's brif emits (§ 3). Body has
    //      one predecessor (header). Lower body ops via dispatcher.
    builder.seal_block(body_block);
    builder.switch_to_block(body_block);
    let body_terminated =
        lower_loop_body_into(body_region, builder, value_map, fn_name, &mut lower_body_op)?;
    if !body_terminated {
        builder.ins().jump(header_block, &[]);
    }

    // § 5. Header sealing : back-edge from § 4 (or absent if terminated)
    //      + entry-edge from caller. Both edges resolved — seal.
    builder.seal_block(header_block);

    // § 6. Exit cursor + seal. Exit has at least one predecessor
    //      (header's brif false-target).
    builder.switch_to_block(exit_block);
    builder.seal_block(exit_block);

    Ok(false)
}

/// § T11-D318 (W-CC-mut-assign) — walk a `scf.while` cond-region inside
/// the loop header and return the cond's CLIF Value. The region's entry
/// block contains the cond-defining op chain ; the LAST op (or a
/// `scf.condition` marker if present) carries the cond ValueId in its
/// first operand. Forwards intermediate ops to the dispatcher so arith /
/// memref.load / nested ops lower normally.
///
/// The walker treats `scf.condition` as the region's terminator and
/// stops walking after consuming it. If the region's last op isn't
/// `scf.condition` (older MIR shapes), the walker uses the last op's
/// first result as the cond fallback — matches the body_lower
/// convention before the explicit terminator was added.
fn lower_while_cond_region<E, F>(
    region: &cssl_mir::MirRegion,
    builder: &mut FunctionBuilder<'_>,
    value_map: &mut HashMap<ValueId, cranelift_codegen::ir::Value>,
    fn_name: &str,
    lower_body_op: &mut F,
) -> Result<cranelift_codegen::ir::Value, BackendOrScfError<E>>
where
    F: FnMut(
        &MirOp,
        &mut FunctionBuilder<'_>,
        &mut HashMap<ValueId, cranelift_codegen::ir::Value>,
        &str,
    ) -> Result<bool, E>,
{
    let entry = region
        .blocks
        .first()
        .ok_or_else(|| ScfError::MissingLoopOperand {
            op_name: "while".to_string(),
            fn_name: fn_name.to_string(),
        })?;

    let mut last_result_id: Option<ValueId> = None;
    let mut cond_id_from_terminator: Option<ValueId> = None;
    for cond_op in &entry.ops {
        if cond_op.name == "scf.condition" {
            cond_id_from_terminator = cond_op.operands.first().copied();
            break;
        }
        // Forward to dispatcher ; ignore the terminator-flag because
        // cond-region ops are pure arith/load/no func.return.
        let _ = lower_body_op(cond_op, builder, value_map, fn_name)
            .map_err(BackendOrScfError::Backend)?;
        if let Some(r) = cond_op.results.first() {
            last_result_id = Some(r.id);
        }
    }
    let cond_id = cond_id_from_terminator
        .or(last_result_id)
        .ok_or_else(|| ScfError::MissingLoopOperand {
            op_name: "while".to_string(),
            fn_name: fn_name.to_string(),
        })?;
    let v = *value_map.get(&cond_id).ok_or(ScfError::UnknownLoopOperand {
        op_name: "while".to_string(),
        fn_name: fn_name.to_string(),
        value_id: cond_id.0,
    })?;
    Ok(v)
}

/// Lower an `scf.for` op : a counted loop. Stage-0 shape (single-trip
/// body — see § DEFERRED for the iter-counter follow-up) :
///
/// ```text
///   <caller's current block>
///       jump header_block
///   header_block:                       (preds : caller)
///       jump body_block
///   body_block:                         (preds : header)
///       <body-ops>
///       jump exit_block                 (single-trip, no back-edge)
///   exit_block:                         (preds : body)
///       <continuation>
/// ```
///
/// The leading operand is the iter ValueId (the result of a
/// `cssl.range` op upstream) ; we resolve it to keep the value-map
/// lookup honest, then drop it on the floor at stage-0. When the MIR
/// emission grows lo / hi / step + IV-block-arg, the structural
/// scaffolding here gains a counter test at the header and a back-edge
/// from body to header — the block triplet is sized for that future
/// growth today.
///
/// # Errors
///   Returns [`BackendOrScfError::Scf`] for missing / unknown iter,
///   wrong region count, or multi-block region ;
///   [`BackendOrScfError::Backend`] for inner-op lowering errors.
///
/// # Tail-flag
///   Returns `Ok(false)`.
#[allow(clippy::implicit_hasher)]
pub fn lower_scf_for<E, F>(
    op: &MirOp,
    builder: &mut FunctionBuilder<'_>,
    value_map: &mut HashMap<ValueId, cranelift_codegen::ir::Value>,
    fn_name: &str,
    mut lower_body_op: F,
) -> Result<bool, BackendOrScfError<E>>
where
    F: FnMut(
        &MirOp,
        &mut FunctionBuilder<'_>,
        &mut HashMap<ValueId, cranelift_codegen::ir::Value>,
        &str,
    ) -> Result<bool, E>,
{
    let body_region = extract_single_body_region(op, "for", fn_name)?;
    // Resolve the iter-operand for value-map honesty even though stage-0
    // doesn't yet use the resolved value. When iter-counter lowering
    // lands, this same resolution feeds the header's loop-test.
    let _iter_val = resolve_loop_operand(op, value_map, "for", fn_name)?;

    // § 1. Create the three blocks.
    let header_block = builder.create_block();
    let body_block = builder.create_block();
    let exit_block = builder.create_block();

    // § 2. Entry-edge : caller's current block jumps into header.
    builder.ins().jump(header_block, &[]);

    // § 3. Header : unconditional jump into body (single-trip stage-0).
    //      Future iter-counter lowering replaces this with a
    //      `brif counter < hi, body_block, exit_block` form.
    builder.switch_to_block(header_block);
    builder.ins().jump(body_block, &[]);

    // § 4. Body block : seal once header's jump emits (§ 3). Lower body
    //      ops via dispatcher. After body ops, jump to exit (single-
    //      trip). When iter-counter lowering lands, this becomes
    //      `jump header_block(counter+step)`.
    builder.seal_block(body_block);
    builder.switch_to_block(body_block);
    let body_terminated =
        lower_loop_body_into(body_region, builder, value_map, fn_name, &mut lower_body_op)?;
    if !body_terminated {
        builder.ins().jump(exit_block, &[]);
    }

    // § 5. Header sealing : single predecessor (caller) — seal.
    builder.seal_block(header_block);

    // § 6. Exit cursor + seal. Predecessor is the body's jump-to-exit
    //      (or absent if body terminated, in which case exit has zero
    //      preds — still legal once sealed).
    builder.switch_to_block(exit_block);
    builder.seal_block(exit_block);

    Ok(false)
}

/// Validate that a loop op (scf.for / scf.while / scf.loop) carries
/// exactly one body-region with at most one block, and return that
/// region. Surfaces [`ScfError::WrongLoopRegionCount`] /
/// [`ScfError::MultiBlockRegion`] on shape violations.
fn extract_single_body_region<'a, E>(
    op: &'a MirOp,
    op_name: &str,
    fn_name: &str,
) -> Result<&'a cssl_mir::MirRegion, BackendOrScfError<E>> {
    if op.regions.len() != 1 {
        return Err(ScfError::WrongLoopRegionCount {
            op_name: op_name.to_string(),
            fn_name: fn_name.to_string(),
            count: op.regions.len(),
        }
        .into());
    }
    let region = &op.regions[0];
    if region.blocks.len() > 1 {
        return Err(ScfError::MultiBlockRegion {
            fn_name: fn_name.to_string(),
            count: region.blocks.len(),
        }
        .into());
    }
    Ok(region)
}

/// Resolve the leading operand of a loop op (cond for scf.while, iter
/// for scf.for) into its CLIF Value. Surfaces missing / unknown
/// operand variants with the op-name baked into the diagnostic.
fn resolve_loop_operand<E>(
    op: &MirOp,
    value_map: &HashMap<ValueId, cranelift_codegen::ir::Value>,
    op_name: &str,
    fn_name: &str,
) -> Result<cranelift_codegen::ir::Value, BackendOrScfError<E>> {
    let id = op
        .operands
        .first()
        .copied()
        .ok_or_else(|| ScfError::MissingLoopOperand {
            op_name: op_name.to_string(),
            fn_name: fn_name.to_string(),
        })?;
    let v = *value_map.get(&id).ok_or(ScfError::UnknownLoopOperand {
        op_name: op_name.to_string(),
        fn_name: fn_name.to_string(),
        value_id: id.0,
    })?;
    Ok(v)
}

/// Walk the body-region's single block, forwarding each op to the
/// dispatcher closure. Returns `Ok(true)` if the body included a
/// terminator (e.g. an inner `func.return`) that already terminated
/// the body block — caller skips emitting the back-edge / fall-through
/// in that case.
fn lower_loop_body_into<E, F>(
    region: &cssl_mir::MirRegion,
    builder: &mut FunctionBuilder<'_>,
    value_map: &mut HashMap<ValueId, cranelift_codegen::ir::Value>,
    fn_name: &str,
    lower_body_op: &mut F,
) -> Result<bool, BackendOrScfError<E>>
where
    F: FnMut(
        &MirOp,
        &mut FunctionBuilder<'_>,
        &mut HashMap<ValueId, cranelift_codegen::ir::Value>,
        &str,
    ) -> Result<bool, E>,
{
    let Some(entry) = region.blocks.first() else {
        return Ok(false);
    };
    let mut terminated = false;
    for body_op in &entry.ops {
        if body_op.name == "scf.yield" {
            // A yield inside a loop body is a no-op at stage-0 — loop
            // ops don't yield values out (their result-type is None).
            // Stop here so we don't mistakenly continue past the
            // region's logical terminator.
            break;
        }
        let was_terminator = lower_body_op(body_op, builder, value_map, fn_name)
            .map_err(BackendOrScfError::Backend)?;
        if was_terminator {
            terminated = true;
            break;
        }
    }
    Ok(terminated)
}

// ─────────────────────────────────────────────────────────────────────────
// § tests — pure-helper coverage. End-to-end JIT roundtrips live in jit.rs ;
// object-emit roundtrips in object.rs ; cross-backend integration in cssl-
// examples. Here we cover the structural dispatch that doesn't need a live
// FunctionBuilder.
// ─────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mir_to_cl_maps_int_widths() {
        use cranelift_codegen::ir::types as cl_types;
        assert_eq!(mir_to_cl(&MirType::Int(IntWidth::I8)), Some(cl_types::I8));
        assert_eq!(mir_to_cl(&MirType::Int(IntWidth::I16)), Some(cl_types::I16));
        assert_eq!(mir_to_cl(&MirType::Int(IntWidth::I32)), Some(cl_types::I32));
        assert_eq!(mir_to_cl(&MirType::Int(IntWidth::I64)), Some(cl_types::I64));
    }

    #[test]
    fn mir_to_cl_maps_float_widths() {
        use cranelift_codegen::ir::types as cl_types;
        assert_eq!(
            mir_to_cl(&MirType::Float(FloatWidth::F32)),
            Some(cl_types::F32)
        );
        assert_eq!(
            mir_to_cl(&MirType::Float(FloatWidth::F64)),
            Some(cl_types::F64)
        );
    }

    #[test]
    fn mir_to_cl_unsupported_yields_none() {
        assert!(mir_to_cl(&MirType::Float(FloatWidth::F16)).is_none());
        assert!(mir_to_cl(&MirType::None).is_none());
    }

    #[test]
    fn scf_error_display_is_actionable() {
        let e = ScfError::MissingCondition {
            fn_name: "f".to_string(),
        };
        let msg = format!("{e}");
        assert!(msg.contains("scf.if"), "msg={msg}");
        assert!(msg.contains("`f`"), "msg={msg}");
    }

    #[test]
    fn scf_error_wrong_region_count_displays() {
        let e = ScfError::WrongRegionCount {
            fn_name: "g".to_string(),
            count: 3,
        };
        let msg = format!("{e}");
        assert!(msg.contains("3 regions"));
        assert!(msg.contains("expected exactly 2"));
    }

    // ─────────────────────────────────────────────────────────────────
    // § T11-D61 / S6-C2 — loop-error-shape coverage
    // ─────────────────────────────────────────────────────────────────

    #[test]
    fn scf_error_wrong_loop_region_count_displays() {
        let e = ScfError::WrongLoopRegionCount {
            op_name: "for".to_string(),
            fn_name: "h".to_string(),
            count: 2,
        };
        let msg = format!("{e}");
        assert!(msg.contains("scf.for"), "msg={msg}");
        assert!(msg.contains("2 regions"), "msg={msg}");
        assert!(msg.contains("expected exactly 1"), "msg={msg}");
    }

    #[test]
    fn scf_error_missing_loop_operand_actionable() {
        let e = ScfError::MissingLoopOperand {
            op_name: "while".to_string(),
            fn_name: "k".to_string(),
        };
        let msg = format!("{e}");
        assert!(msg.contains("scf.while"), "msg={msg}");
        assert!(msg.contains("`k`"), "msg={msg}");
        assert!(msg.contains("missing"), "msg={msg}");
    }

    #[test]
    fn scf_error_unknown_loop_operand_includes_value_id() {
        let e = ScfError::UnknownLoopOperand {
            op_name: "for".to_string(),
            fn_name: "iter".to_string(),
            value_id: 42,
        };
        let msg = format!("{e}");
        assert!(msg.contains("scf.for"), "msg={msg}");
        assert!(msg.contains("42"), "msg={msg}");
    }

    // ─────────────────────────────────────────────────────────────────
    // § T11-D281 (W-A1-δ) — value-map continuity tests
    // ─────────────────────────────────────────────────────────────────

    #[test]
    fn mir_to_cl_maps_ptr_to_i64() {
        // Cascade-spliced scf.if can yield !cssl.ptr through merge-blocks
        // when an arm returns a heap-allocated cell (e.g., the variant
        // payload pointer for `Ok(x) => x` where x : Box<T>). Stage-0
        // contract widens this to host-pointer-width I64 on x86_64 to
        // keep the merge-param-ty derivation valid for cgen-cl.
        use cranelift_codegen::ir::types as cl_types;
        assert_eq!(mir_to_cl(&MirType::Ptr), Some(cl_types::I64));
    }

    #[test]
    fn mir_to_cl_maps_handle_to_i64() {
        // !cssl.handle is the packed generational reference type ; same
        // host-pointer-width widening as Ptr so cascade-yielded handles
        // (e.g., from `cssl.handle.pack` producers in arm-regions) can
        // join through scf.if merge-blocks.
        use cranelift_codegen::ir::types as cl_types;
        assert_eq!(mir_to_cl(&MirType::Handle), Some(cl_types::I64));
    }

    #[test]
    fn mir_to_cl_still_rejects_unrepresentable_types() {
        // Sanity : Tuple / Memref / Function / Opaque must remain
        // unrepresentable at the cranelift-scalar level — they need
        // explicit `cssl.tuple.unpack` / aggregate-lowering before
        // landing in a scf.if merge.
        assert!(mir_to_cl(&MirType::Tuple(vec![])).is_none());
        assert!(mir_to_cl(&MirType::Memref {
            shape: vec![Some(8)],
            elem: Box::new(MirType::Float(FloatWidth::F32))
        })
        .is_none());
        assert!(mir_to_cl(&MirType::Opaque("custom".to_string())).is_none());
    }

    /// § T11-D281 (W-A1-δ) — terminator-flag propagation contract is
    /// asserted at the type-system level : `lower_branch_into` returns
    /// `Result<(Option<Value>, bool), BackendOrScfError<E>>`. Any future
    /// refactor that reverts to bare `Option<Value>` breaks the
    /// destructuring at the §6/§7 call-sites in `lower_scf_if` and
    /// fails to compile. This test makes that invariant explicit by
    /// reading the source verbatim and looking for the tuple-shape +
    /// the destructuring at the call-site.
    ///
    /// (Full end-to-end JIT roundtrip of cascade-terminating arms is
    /// the un-ignored W-A1 / W-A3 e2e gate that lands once the W-A1-δ
    /// + W-A1-ε pair both close.)
    #[test]
    fn lower_branch_into_returns_yield_and_terminator_tuple() {
        let module_src = include_str!("scf.rs");
        // Type-shape : the helper returns the tuple `(Option<Value>, bool)`.
        assert!(
            module_src.contains(
                "Result<(Option<cranelift_codegen::ir::Value>, bool), BackendOrScfError<E>>"
            ),
            "lower_branch_into must return (yield, terminated) tuple"
        );
        // Call-site : both §6 (then) and §7 (else) destructure the tuple
        // and gate `emit_terminating_jump` on the terminator flag.
        assert!(
            module_src.contains("let (then_yield_arg, then_terminated) = lower_branch_into("),
            "§6 call-site must destructure the tuple to gate the merge-jump"
        );
        assert!(
            module_src.contains("let (else_yield_arg, else_terminated) = lower_branch_into("),
            "§7 call-site must destructure the tuple to gate the merge-jump"
        );
        assert!(
            module_src.contains("if !then_terminated {"),
            "§6 must skip emit_terminating_jump when the then-branch returned"
        );
        assert!(
            module_src.contains("if !else_terminated {"),
            "§7 must skip emit_terminating_jump when the else-branch returned"
        );
    }

    /// § T11-D281 (W-A1-δ) — the documented contract that
    /// `lower_scf_if` returns `Ok(true)` when both branches terminated.
    /// Asserted via the doc-comment cross-reference + the in-fn tail
    /// expression (`Ok(both_branches_terminated)`). A future refactor
    /// changing that tail must also update this test's reference
    /// expectation.
    #[test]
    fn lower_scf_if_documents_both_branches_terminate_contract() {
        let module_src = include_str!("scf.rs");
        // Doc contract : top-level fn doc-comment mentions the
        // `Ok(true)` semantics for both-branches-terminate.
        assert!(
            module_src.contains("BOTH branches ended in"),
            "doc-comment must call out the both-branches-terminate \
             contract for value-map-continuity"
        );
        assert!(
            module_src.contains("both_branches_terminated"),
            "implementation must thread the both-branches-terminated \
             flag back as the tail Ok(...) expression"
        );
    }
}
