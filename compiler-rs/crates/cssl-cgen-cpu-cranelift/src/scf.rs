//! § scf — structured-control-flow MIR-op lowering shared by JIT + Object backends.
//!
//! § SPEC
//!   - `specs/15_MLIR.csl` § SCF-DIALECT-LOWERING
//!   - `specs/02_IR.csl` § STRUCTURED-CFG-CONSTRAINT
//!   - `specs/15_MLIR.csl` § STRUCTURED CFG PRESERVATION (CC4)
//!
//! § ROLE
//!   The CSSLv3 frontend lowers `if` expressions to `scf.if` MIR ops with two
//!   nested regions (then + else). Each region's entry block holds the lowered
//!   ops plus an optional terminating `scf.yield <value-id>` op when the
//!   branch produces a value (i.e. the `if` is used as an expression).
//!
//!   This module turns one `scf.if` MIR op into the cranelift CLIF
//!   equivalent : a `brif` conditional branch into a then-block and an
//!   else-block, both jumping to a shared merge-block. When the scf.if has a
//!   non-`MirType::None` result the merge-block carries one block-parameter
//!   that receives the yielded value — that's the SSA-clean equivalent of
//!   the MLIR scf.if-as-expression contract.
//!
//! § INVARIANTS
//!   - Cranelift's `brif` post-0.105 takes `(cond, then_block, &[then_args],
//!     else_block, &[else_args])`. We always pass empty args at the brif site
//!     itself ; the merge-block-arg is fed by the `jump` instructions at the
//!     tails of each branch.
//!   - Empty-body branches (no scf.yield, no other ops) still emit a single
//!     `jump` to the merge-block. Cranelift rejects blocks without a
//!     terminator, so even a "do-nothing else" must terminate.
//!   - The merge-block is sealed last : its predecessors are the two branch
//!     blocks, both filled by the time we switch back.
//!   - Nested scf.if inside a region calls back into [`lower_scf_if`]
//!     recursively via the dispatcher closure passed in by the caller. This
//!     keeps op-dispatch ownership in `jit.rs` / `object.rs` while letting
//!     control-flow scaffolding live here once.
//!
//! § DESIGN — single-region-block constraint
//!   At stage-0 every region produced by `cssl_mir::body_lower` has exactly
//!   one block named `entry`. The lowering walks `region.blocks[0].ops` once
//!   per region. When a future slice introduces multi-block regions for a
//!   single scf.if branch (e.g. early-return inside a then-arm), this
//!   helper returns [`ScfError::MultiBlockRegion`] so the caller sees a clean
//!   diagnostic instead of silently mis-lowering.

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
///   Returns `Ok(false)` because scf.if itself is not a function-terminator.
///   Cranelift's `brif` is a block-terminator, but the merge-block is open
///   for further ops so the caller's "is this op a terminator" check stays
///   false.
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
    let result_ty = op.results.first().map(|r| &r.ty);
    let merge_param_ty = match result_ty {
        Some(ty) if !matches!(ty, MirType::None) => {
            Some(mir_to_cl(ty).ok_or_else(|| ScfError::NonScalarYield {
                fn_name: fn_name.to_string(),
                ty: format!("{ty}"),
            })?)
        }
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
    builder.switch_to_block(then_block);
    builder.seal_block(then_block);
    let then_yield_arg = lower_branch_into(
        then_region,
        builder,
        value_map,
        fn_name,
        &mut lower_branch_op,
    )?;
    emit_terminating_jump(builder, merge_block, then_yield_arg, merge_param_ty);

    // § 7. Lower the ELSE branch.
    builder.switch_to_block(else_block);
    builder.seal_block(else_block);
    let else_yield_arg = lower_branch_into(
        else_region,
        builder,
        value_map,
        fn_name,
        &mut lower_branch_op,
    )?;
    emit_terminating_jump(builder, merge_block, else_yield_arg, merge_param_ty);

    // § 8. Switch to merge-block + record the merge-block-param as the scf.if
    //      result (when typed).
    builder.switch_to_block(merge_block);
    builder.seal_block(merge_block);
    if let Some(_param_ty) = merge_param_ty {
        // The scf.if's result `MirValue.id` must now map to the merge-block-param[0].
        if let Some(r) = op.results.first() {
            let merge_params = builder.block_params(merge_block);
            let bp = *merge_params.first().expect("merge-block-param appended");
            value_map.insert(r.id, bp);
        }
    }

    Ok(false)
}

/// Lower the ops of a branch's single block. Skips `scf.yield` (its operand
/// is captured separately as the branch's tail-jump arg). Returns the
/// captured yield value (when present + scalar-typed).
fn lower_branch_into<E, F>(
    region: &cssl_mir::MirRegion,
    builder: &mut FunctionBuilder<'_>,
    value_map: &mut HashMap<ValueId, cranelift_codegen::ir::Value>,
    fn_name: &str,
    lower_branch_op: &mut F,
) -> Result<Option<cranelift_codegen::ir::Value>, BackendOrScfError<E>>
where
    F: FnMut(
        &MirOp,
        &mut FunctionBuilder<'_>,
        &mut HashMap<ValueId, cranelift_codegen::ir::Value>,
        &str,
    ) -> Result<bool, E>,
{
    let Some(entry) = region.blocks.first() else {
        return Ok(None);
    };
    let mut yield_val = None;
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
        // Non-yield op : delegate to the caller's per-op lowerer. We ignore
        // the "is-terminator" flag because scf.if branches must reach their
        // tail jump ; an inner func.return inside a branch would be a
        // structured-CFG violation that D5 (StructuredCfgValidator) will
        // catch in a later slice.
        let _ = lower_branch_op(branch_op, builder, value_map, fn_name)
            .map_err(BackendOrScfError::Backend)?;
    }
    Ok(yield_val)
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
        builder.ins().jump(merge_block, &[arg]);
    } else {
        builder.ins().jump(merge_block, &[]);
    }
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
}
