//! § resolve_call_result — fix up opaque call-result types from sig-table
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § T11-LOA-PURE-CSSL (W-LOA-pure-cssl-engine)
//!
//! § PROBLEM
//!   The body-lowering pass mints `MirType::Opaque("!cssl.call_result.<name>")`
//!   for every `func.call` op whose callee isn't a math-intrinsic — see
//!   `body_lower::lower_call`. This is a placeholder ; downstream codegen
//!   (cranelift / native-x64) treats `MirType::Opaque(_)` as a non-scalar
//!   carrier-type which fails the stage-0 scalars-only gate.
//!
//!   For calls to functions DEFINED in the same module (regular `fn` or
//!   `extern fn` declarations), the actual return type IS available in the
//!   already-lowered `MirFunc` signature : `mir_module.find_func(name).results`.
//!   This pass walks every `func.call` op in the module + replaces the
//!   opaque result-type with the resolved scalar return type.
//!
//! § ALGORITHM
//!   1. Build a name → return-type[0] table from `module.funcs`.
//!   2. For each fn body, for each entry-block op : if op.name == "func.call"
//!      AND op.results[0].ty matches `Opaque(!cssl.call_result.X)` AND the
//!      table contains X with a single scalar return type, rewrite the
//!      result.ty to that scalar.
//!   3. Recursively descend into op.regions for nested ops (scf.if branches /
//!      scf.while / etc.).
//!
//! § STAGE-1 PATH
//!   When body-lowering is rewritten to thread the module's signature table
//!   through `BodyLowerCtx`, this pass becomes redundant. Stage-1 keeps it
//!   as a defense-in-depth check — discovering a stray opaque-call-result is
//!   actionable feedback that body-lowering missed an inference path.

use crate::block::{MirOp, MirRegion};
use crate::func::{MirFunc, MirModule};
use crate::value::{MirType, MirValue};

/// Walk `module` + replace opaque `func.call` result types with the actual
/// return types from the matching `MirFunc.results`. Returns the count of
/// rewrites performed (useful for diagnostic logging).
pub fn resolve_call_result_types(module: &mut MirModule) -> usize {
    // § Build the name → return-type[0] index. We only support single-result
    // fns at this stage ; multi-result tuple-returning fns get their own
    // pass when tuples land.
    let sig_table: std::collections::HashMap<String, MirType> = module
        .funcs
        .iter()
        .filter_map(|f| {
            if f.results.len() == 1 {
                Some((f.name.clone(), f.results[0].clone()))
            } else {
                None
            }
        })
        .collect();

    let mut rewrites = 0_usize;
    for func in &mut module.funcs {
        rewrites += rewrite_func(func, &sig_table);
    }
    rewrites
}

/// Walk one fn's entry block + nested regions, rewriting opaque call-results.
fn rewrite_func(
    func: &mut MirFunc,
    sig_table: &std::collections::HashMap<String, MirType>,
) -> usize {
    let mut rewrites = 0_usize;
    let body = &mut func.body;
    if let Some(entry) = body.blocks.first_mut() {
        for op in &mut entry.ops {
            rewrites += rewrite_op(op, sig_table);
        }
    }
    rewrites
}

/// Rewrite a single op + recurse into nested regions. Returns the count of
/// rewrites performed in this op + descendants.
fn rewrite_op(op: &mut MirOp, sig_table: &std::collections::HashMap<String, MirType>) -> usize {
    let mut rewrites = 0_usize;
    if op.name == "func.call" && op.results.len() == 1 {
        if let MirType::Opaque(opaque_name) = &op.results[0].ty {
            if let Some(stripped) = opaque_name.strip_prefix("!cssl.call_result.") {
                let lookup_name = stripped.to_string();
                if let Some(target_ty) = sig_table.get(&lookup_name) {
                    // Found the callee's signature ; rewrite the result type.
                    op.results[0] = MirValue {
                        id: op.results[0].id,
                        ty: target_ty.clone(),
                    };
                    rewrites += 1;
                }
            }
        }
    }
    // Recurse into nested regions (scf.if / scf.while / scf.for / scf.match
    // / closures-with-bodies).
    for region in &mut op.regions {
        rewrites += rewrite_region(region, sig_table);
    }
    rewrites
}

fn rewrite_region(
    region: &mut MirRegion,
    sig_table: &std::collections::HashMap<String, MirType>,
) -> usize {
    let mut rewrites = 0_usize;
    for block in &mut region.blocks {
        for op in &mut block.ops {
            rewrites += rewrite_op(op, sig_table);
        }
    }
    rewrites
}

// ═════════════════════════════════════════════════════════════════════════
// § TESTS
// ═════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::MirOp;
    use crate::func::MirFunc;
    use crate::value::{IntWidth, ValueId};

    fn i32_ty() -> MirType {
        MirType::Int(IntWidth::I32)
    }

    /// Build a synthetic module with one extern-fn signature + one main fn
    /// that calls it, with the main fn's `func.call` op carrying the
    /// opaque-result-type that body-lowering would mint.
    fn fixture() -> MirModule {
        let mut module = MirModule::new();

        // extern fn target() -> i32 (signature-only)
        let target = MirFunc::new("__cssl_engine_run", vec![], vec![i32_ty()]);
        module.push_func(target);

        // fn main() -> i32 { __cssl_engine_run() }   — pre-fixup result is opaque.
        let mut main_fn = MirFunc::new("main", vec![], vec![i32_ty()]);
        main_fn.push_op(
            MirOp::std("func.call")
                .with_attribute("callee", "__cssl_engine_run")
                .with_result(
                    ValueId(0),
                    MirType::Opaque("!cssl.call_result.__cssl_engine_run".to_string()),
                ),
        );
        main_fn.push_op(MirOp::std("func.return").with_operand(ValueId(0)));
        module.push_func(main_fn);

        module
    }

    #[test]
    fn rewrites_opaque_to_extern_return_type() {
        let mut module = fixture();
        let n = resolve_call_result_types(&mut module);
        assert_eq!(n, 1, "exactly one rewrite expected");
        // Verify the call op now carries i32 (not opaque).
        let main_fn = module
            .funcs
            .iter()
            .find(|f| f.name == "main")
            .expect("main fn must be present");
        let call_op = &main_fn.body.blocks[0].ops[0];
        assert_eq!(call_op.name, "func.call");
        assert_eq!(call_op.results[0].ty, i32_ty());
    }

    #[test]
    fn leaves_unknown_callees_unrewritten() {
        let mut module = MirModule::new();
        let mut f = MirFunc::new("main", vec![], vec![i32_ty()]);
        f.push_op(
            MirOp::std("func.call")
                .with_attribute("callee", "no_such_target")
                .with_result(
                    ValueId(0),
                    MirType::Opaque("!cssl.call_result.no_such_target".to_string()),
                ),
        );
        f.push_op(MirOp::std("func.return").with_operand(ValueId(0)));
        module.push_func(f);
        let n = resolve_call_result_types(&mut module);
        assert_eq!(n, 0, "no rewrites for unresolved callee");
        // The opaque type is preserved.
        let f = &module.funcs[0];
        let call_op = &f.body.blocks[0].ops[0];
        assert!(matches!(call_op.results[0].ty, MirType::Opaque(_)));
    }

    #[test]
    fn skips_non_call_ops() {
        let mut module = MirModule::new();
        let mut f = MirFunc::new("main", vec![], vec![i32_ty()]);
        // arith.constant with an opaque result-ty (synthetic edge-case) should
        // NOT be rewritten — the pass only touches func.call ops.
        f.push_op(
            MirOp::std("arith.constant")
                .with_attribute("value", "42")
                .with_result(
                    ValueId(0),
                    MirType::Opaque("!cssl.call_result.fake".to_string()),
                ),
        );
        f.push_op(MirOp::std("func.return").with_operand(ValueId(0)));
        // Pretend "fake" exists in the table — pass should still skip it
        // because the op-name is not `func.call`.
        let target = MirFunc::new("fake", vec![], vec![i32_ty()]);
        module.push_func(target);
        module.push_func(f);
        let n = resolve_call_result_types(&mut module);
        assert_eq!(n, 0, "non-call ops must not be rewritten");
    }

    #[test]
    fn multi_result_fns_are_skipped() {
        // A 2-result fn doesn't match the "single scalar result" criterion ;
        // calls to it stay opaque. (Stage-0 doesn't emit multi-result calls
        // anyway — tuple returns lower to struct-by-value with separate ops.)
        let mut module = MirModule::new();
        let multi = MirFunc::new("multi", vec![], vec![i32_ty(), i32_ty()]);
        module.push_func(multi);
        let mut main_fn = MirFunc::new("main", vec![], vec![i32_ty()]);
        main_fn.push_op(
            MirOp::std("func.call")
                .with_attribute("callee", "multi")
                .with_result(
                    ValueId(0),
                    MirType::Opaque("!cssl.call_result.multi".to_string()),
                ),
        );
        main_fn.push_op(MirOp::std("func.return").with_operand(ValueId(0)));
        module.push_func(main_fn);
        let n = resolve_call_result_types(&mut module);
        assert_eq!(n, 0);
    }
}
