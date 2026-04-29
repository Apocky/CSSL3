//! Block-level dead-code elimination over a specialized [`MirFunc`].
//!
//! § OBJECTIVE
//!   After [`crate::const_prop::run_const_prop_pass`] folds an op-chain, two
//!   sources of dead code remain :
//!   - **branch-folded ops** : `scf.if` / `cssl.if` whose condition resolved
//!     to a known-const Bool — the loser branch is unreachable.
//!   - **dead arith.constant** ops whose results are not consumed by any
//!     downstream op (introduced by const-prop's rewrite-to-constant when the
//!     successor was also folded).
//!
//!   This pass runs `eliminate_branches` first (using a list of [`BranchFold`]
//!   produced by `collect_branch_folds`), then `eliminate_dead_arith_consts`
//!   in a worklist loop.
//!
//! § DESIGN — simplified LLVM-style
//!   We do NOT do full liveness over a CFG ; structured-CFG is the invariant
//!   maintained upstream by D5 (`structured_cfg`). Instead we do :
//!   - **Branch fold** : replace `scf.if` op with the surviving region inlined
//!     directly into the parent block (the surviving region's ops are spliced).
//!   - **Dead arith.const removal** : walk all ops + collect operand-value-ids ;
//!     any `arith.constant` whose result is in nobody's operand-list is removed.
//!     Iterates until a fixed-point.
//!
//! § REPORT
//!   [`DceReport`] tracks per-category counters so the specializer can write
//!   them into the manifest.

use std::collections::HashSet;

use cssl_mir::{MirBlock, MirFunc, MirOp, MirRegion, ValueId};

use crate::const_prop::BranchFold;

/// Aggregated DCE statistics.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DceReport {
    /// Number of `scf.if` / `cssl.if` ops collapsed.
    pub branches_eliminated: u32,
    /// Number of dead `arith.constant` ops removed.
    pub dead_consts_removed: u32,
    /// Number of dead non-constant ops removed (e.g., orphan arith.* with no
    /// observed-side-effect consumer).
    pub dead_ops_removed: u32,
    /// Number of fixed-point iterations the worklist took for dead-op
    /// removal.
    pub iterations: u32,
}

impl DceReport {
    /// Total simplifications.
    #[must_use]
    pub fn total(self) -> u32 {
        self.branches_eliminated + self.dead_consts_removed + self.dead_ops_removed
    }
}

/// Eliminate branch-folded scf.if / cssl.if ops by inlining the surviving
/// region. Mutates `func` in-place.
///
/// § IMPLEMENTATION
///   For each [`BranchFold`] (sorted by op-idx descending so removals don't
///   shift later indices) :
///   1. Locate the op in `func.body.blocks[block_idx].ops[op_idx]`.
///   2. Pick the surviving region : `regions[0]` ⇒ then-branch ;
///      `regions[1]` ⇒ else-branch (if present).
///   3. Splice the surviving region's entry-block ops into the parent block
///      at `op_idx`, replacing the if-op.
///   4. If no surviving region exists for the chosen branch, just remove
///      the op (treat as no-op fold).
pub fn eliminate_branches(func: &mut MirFunc, folds: &[BranchFold]) -> u32 {
    // Group by block so multi-fold sites can sort within each block.
    let mut by_block: std::collections::HashMap<usize, Vec<BranchFold>> =
        std::collections::HashMap::new();
    for f in folds {
        by_block.entry(f.block_idx).or_default().push(*f);
    }

    let mut count = 0u32;
    for (b_idx, mut group) in by_block {
        // Process descending so earlier removes don't shift later indices.
        group.sort_by(|a, b| b.op_idx.cmp(&a.op_idx));
        let block = match func.body.blocks.get_mut(b_idx) {
            Some(b) => b,
            None => continue,
        };
        for fold in group {
            if fold.op_idx >= block.ops.len() {
                continue;
            }
            let op = block.ops.remove(fold.op_idx);
            let inlined_ops = pick_branch_ops(&op, fold.taken_branch);
            for (i, inl) in inlined_ops.into_iter().enumerate() {
                block.ops.insert(fold.op_idx + i, inl);
            }
            count = count.saturating_add(1);
        }
    }
    count
}

/// Extract the surviving region's entry-block ops.
fn pick_branch_ops(op: &MirOp, taken: bool) -> Vec<MirOp> {
    let region_idx = if taken { 0 } else { 1 };
    op.regions
        .get(region_idx)
        .and_then(|r| r.blocks.first())
        .map(|b| b.ops.clone())
        .unwrap_or_default()
}

/// Eliminate `arith.constant` ops whose result-id is never used. Iterative
/// worklist : after one pass removes some consts, others may now be dead.
///
/// § INVARIANT — uses the entry-block param ids as live (so const-prop
/// bindings on params don't get stripped). Recurses into nested regions.
///
/// § RETURN-VALUE PRESERVATION — the LAST op in the entry block is
/// considered "the result" of the fn ; its results are preserved as live
/// regardless of operand-references. Without this, a fully-const-folded
/// fn whose body becomes `arith.constant <answer>` would have the answer
/// stripped (no operand-references → "dead").
pub fn eliminate_dead_arith_consts(func: &mut MirFunc) -> (u32, u32) {
    let mut count = 0u32;
    let mut iters = 0u32;
    let max_iters = 16;
    loop {
        if iters >= max_iters {
            break;
        }
        iters = iters.saturating_add(1);
        // Compute the set of "live" value-ids : any ValueId referenced as an
        // operand by any op anywhere in the body.
        let mut live: HashSet<ValueId> = HashSet::new();
        gather_live_operands(&func.body, &mut live);

        // Also keep entry-args + the fn's "return-value" implicitly live.
        if let Some(entry) = func.body.entry() {
            for arg in &entry.args {
                live.insert(arg.id);
            }
            // Treat the trailing op's results as live (return-value).
            if let Some(last_op) = entry.ops.last() {
                for r in &last_op.results {
                    live.insert(r.id);
                }
            }
        }

        let removed = remove_dead_consts_from_region(&mut func.body, &live);
        if removed == 0 {
            break;
        }
        count = count.saturating_add(removed);
    }
    (count, iters)
}

fn gather_live_operands(region: &MirRegion, live: &mut HashSet<ValueId>) {
    for block in &region.blocks {
        for op in &block.ops {
            for operand in &op.operands {
                live.insert(*operand);
            }
            for nested in &op.regions {
                gather_live_operands(nested, live);
            }
        }
    }
}

fn remove_dead_consts_from_region(region: &mut MirRegion, live: &HashSet<ValueId>) -> u32 {
    let mut removed = 0u32;
    for block in &mut region.blocks {
        removed = removed.saturating_add(remove_dead_consts_from_block(block, live));
    }
    removed
}

fn remove_dead_consts_from_block(block: &mut MirBlock, live: &HashSet<ValueId>) -> u32 {
    let mut removed = 0u32;
    let mut idx = 0;
    while idx < block.ops.len() {
        // Recurse first into nested regions.
        for region in &mut block.ops[idx].regions {
            removed = removed.saturating_add(remove_dead_consts_from_region(region, live));
        }
        // Now check the op itself.
        let kill = is_dead_arith_constant(&block.ops[idx], live);
        if kill {
            block.ops.remove(idx);
            removed = removed.saturating_add(1);
        } else {
            idx += 1;
        }
    }
    removed
}

fn is_dead_arith_constant(op: &MirOp, live: &HashSet<ValueId>) -> bool {
    if op.name != "arith.constant" {
        return false;
    }
    op.results.iter().all(|r| !live.contains(&r.id))
}

/// Eliminate dead non-constant ops : ops whose results are unused AND whose
/// op-name is in the safe-to-DCE list (pure arithmetic). Side-effect ops
/// (`cssl.heap.*`, `cssl.fs.*`, `cssl.net.*`, `cssl.telemetry.*`,
/// `cssl.verify.*`, `cssl.gpu.*`, etc.) are NEVER removed.
pub fn eliminate_dead_ops(func: &mut MirFunc) -> u32 {
    let mut count = 0u32;
    let mut iters = 0u32;
    let max_iters = 16;
    loop {
        if iters >= max_iters {
            break;
        }
        iters = iters.saturating_add(1);
        let mut live: HashSet<ValueId> = HashSet::new();
        gather_live_operands(&func.body, &mut live);
        if let Some(entry) = func.body.entry() {
            for arg in &entry.args {
                live.insert(arg.id);
            }
            // Preserve the trailing op's results as live (return-value).
            if let Some(last_op) = entry.ops.last() {
                for r in &last_op.results {
                    live.insert(r.id);
                }
            }
        }
        let removed = remove_dead_pure_ops_from_region(&mut func.body, &live);
        if removed == 0 {
            break;
        }
        count = count.saturating_add(removed);
    }
    count
}

fn remove_dead_pure_ops_from_region(region: &mut MirRegion, live: &HashSet<ValueId>) -> u32 {
    let mut removed = 0u32;
    for block in &mut region.blocks {
        removed = removed.saturating_add(remove_dead_pure_ops_from_block(block, live));
    }
    removed
}

fn remove_dead_pure_ops_from_block(block: &mut MirBlock, live: &HashSet<ValueId>) -> u32 {
    let mut removed = 0u32;
    let mut idx = 0;
    while idx < block.ops.len() {
        // Recurse first.
        for region in &mut block.ops[idx].regions {
            removed = removed.saturating_add(remove_dead_pure_ops_from_region(region, live));
        }
        if is_dead_pure_op(&block.ops[idx], live) {
            block.ops.remove(idx);
            removed = removed.saturating_add(1);
        } else {
            idx += 1;
        }
    }
    removed
}

fn is_dead_pure_op(op: &MirOp, live: &HashSet<ValueId>) -> bool {
    if !is_pure_dce_safe(&op.name) {
        return false;
    }
    if op.results.is_empty() {
        // No results ⇒ purely side-effecting (or no-op). Don't remove.
        return false;
    }
    op.results.iter().all(|r| !live.contains(&r.id))
}

/// Whitelist of op-names that are safe to DCE if their results are unused.
/// Side-effecting ops + ops touching the host environment are excluded.
fn is_pure_dce_safe(name: &str) -> bool {
    name.starts_with("arith.")
        || name == "cssl.diff.primal"
        || name == "cssl.diff.fwd"
        || name == "cssl.diff.bwd"
        || name == "cssl.jet.construct"
        || name == "cssl.jet.project"
        || name == "cssl.option.some"
        || name == "cssl.option.none"
        || name == "cssl.result.ok"
        || name == "cssl.result.err"
}

/// Run the full DCE pipeline : eliminate folded branches, then dead consts,
/// then dead pure ops. Returns the aggregated [`DceReport`].
pub fn run_dce_pass(func: &mut MirFunc, branch_folds: &[BranchFold]) -> DceReport {
    let branches_eliminated = eliminate_branches(func, branch_folds);
    let (dead_consts_removed, iterations) = eliminate_dead_arith_consts(func);
    let dead_ops_removed = eliminate_dead_ops(func);
    DceReport {
        branches_eliminated,
        dead_consts_removed,
        dead_ops_removed,
        iterations,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        eliminate_branches, eliminate_dead_arith_consts, eliminate_dead_ops, is_pure_dce_safe,
        run_dce_pass, DceReport,
    };
    use crate::const_prop::BranchFold;
    use cssl_mir::{IntWidth, MirFunc, MirOp, MirRegion, MirType, MirValue, ValueId};

    fn mk_const(id: u32) -> MirOp {
        let mut op = MirOp::std("arith.constant");
        op.results
            .push(MirValue::new(ValueId(id), MirType::Int(IntWidth::I32)));
        op.attributes.push(("value".into(), "1".into()));
        op
    }

    fn mk_user(operand: u32, result_id: u32) -> MirOp {
        let mut op = MirOp::std("arith.addi");
        op.operands.push(ValueId(operand));
        op.operands.push(ValueId(operand));
        op.results.push(MirValue::new(
            ValueId(result_id),
            MirType::Int(IntWidth::I32),
        ));
        op
    }

    fn mk_if_with_branches(cond_id: u32) -> MirOp {
        let mut if_op = MirOp::std("scf.if");
        if_op.operands.push(ValueId(cond_id));
        let mut then_region = MirRegion::with_entry(vec![]);
        if let Some(b) = then_region.entry_mut() {
            let mut yld = MirOp::std("scf.yield");
            yld.results
                .push(MirValue::new(ValueId(100), MirType::Int(IntWidth::I32)));
            b.push(yld);
        }
        let mut else_region = MirRegion::with_entry(vec![]);
        if let Some(b) = else_region.entry_mut() {
            let mut yld = MirOp::std("scf.yield");
            yld.results
                .push(MirValue::new(ValueId(200), MirType::Int(IntWidth::I32)));
            b.push(yld);
        }
        if_op.regions.push(then_region);
        if_op.regions.push(else_region);
        if_op
    }

    #[test]
    fn dce_report_total_sums_categories() {
        let r = DceReport {
            branches_eliminated: 2,
            dead_consts_removed: 3,
            dead_ops_removed: 1,
            iterations: 2,
        };
        assert_eq!(r.total(), 6);
    }

    #[test]
    fn eliminate_branches_takes_then_branch_when_true() {
        let mut f = MirFunc::new("test", vec![], vec![]);
        let entry = f.body.entry_mut().unwrap();
        entry.push(mk_if_with_branches(0));
        let folds = [BranchFold {
            block_idx: 0,
            op_idx: 0,
            taken_branch: true,
        }];
        let count = eliminate_branches(&mut f, &folds);
        assert_eq!(count, 1);
        // After elimination : the if-op gone ; replaced by then-region's yield.
        let entry = f.body.entry().unwrap();
        assert_eq!(entry.ops.len(), 1);
        assert_eq!(entry.ops[0].name, "scf.yield");
        assert_eq!(entry.ops[0].results[0].id, ValueId(100));
    }

    #[test]
    fn eliminate_branches_takes_else_branch_when_false() {
        let mut f = MirFunc::new("test", vec![], vec![]);
        let entry = f.body.entry_mut().unwrap();
        entry.push(mk_if_with_branches(0));
        let folds = [BranchFold {
            block_idx: 0,
            op_idx: 0,
            taken_branch: false,
        }];
        eliminate_branches(&mut f, &folds);
        let entry = f.body.entry().unwrap();
        assert_eq!(entry.ops[0].results[0].id, ValueId(200));
    }

    #[test]
    fn eliminate_branches_handles_missing_else_region() {
        let mut f = MirFunc::new("test", vec![], vec![]);
        let entry = f.body.entry_mut().unwrap();
        let mut if_op = MirOp::std("scf.if");
        if_op.operands.push(ValueId(0));
        // Only then-region.
        let mut then_region = MirRegion::with_entry(vec![]);
        if let Some(b) = then_region.entry_mut() {
            b.push(mk_const(50));
        }
        if_op.regions.push(then_region);
        entry.push(if_op);
        let folds = [BranchFold {
            block_idx: 0,
            op_idx: 0,
            taken_branch: false, // else picked but no else-region exists.
        }];
        let count = eliminate_branches(&mut f, &folds);
        assert_eq!(count, 1);
        // No-op : the if-op is removed but no replacement ops inlined.
        let entry = f.body.entry().unwrap();
        assert!(entry.ops.is_empty());
    }

    #[test]
    fn eliminate_branches_handles_multiple_in_same_block() {
        // Place two if-ops back-to-back ; verify both are processed without
        // index shift causing breakage.
        let mut f = MirFunc::new("test", vec![], vec![]);
        let entry = f.body.entry_mut().unwrap();
        entry.push(mk_if_with_branches(0));
        entry.push(mk_if_with_branches(0));
        let folds = [
            BranchFold {
                block_idx: 0,
                op_idx: 0,
                taken_branch: true,
            },
            BranchFold {
                block_idx: 0,
                op_idx: 1,
                taken_branch: false,
            },
        ];
        let count = eliminate_branches(&mut f, &folds);
        assert_eq!(count, 2);
        let entry = f.body.entry().unwrap();
        assert_eq!(entry.ops.len(), 2);
        assert_eq!(entry.ops[0].results[0].id, ValueId(100)); // then-branch
        assert_eq!(entry.ops[1].results[0].id, ValueId(200)); // else-branch
    }

    #[test]
    fn eliminate_dead_arith_consts_removes_unused() {
        // %0 = const ; %1 = const ; %2 = use(%0, %0)  →  %1 dead.
        let mut f = MirFunc::new("test", vec![], vec![]);
        let entry = f.body.entry_mut().unwrap();
        entry.push(mk_const(0));
        entry.push(mk_const(1)); // dead : not used by anybody.
        entry.push(mk_user(0, 2));
        let (removed, _) = eliminate_dead_arith_consts(&mut f);
        assert_eq!(removed, 1);
        let entry = f.body.entry().unwrap();
        assert_eq!(entry.ops.len(), 2);
        assert_eq!(entry.ops[0].name, "arith.constant");
        assert_eq!(entry.ops[1].name, "arith.addi");
    }

    #[test]
    fn eliminate_dead_arith_consts_iterates_to_fixed_point() {
        // Three independent consts ; the LAST one is treated as the
        // return-anchor + preserved. The other two are dead + removed.
        let mut f = MirFunc::new("test", vec![], vec![]);
        let entry = f.body.entry_mut().unwrap();
        entry.push(mk_const(0));
        entry.push(mk_const(1)); // dead
        entry.push(mk_const(2)); // anchor : last op
        let (removed, _) = eliminate_dead_arith_consts(&mut f);
        assert_eq!(removed, 2);
        let entry = f.body.entry().unwrap();
        assert_eq!(entry.ops.len(), 1);
        assert_eq!(entry.ops[0].results[0].id, ValueId(2));
    }

    #[test]
    fn eliminate_dead_arith_consts_keeps_used() {
        let mut f = MirFunc::new("test", vec![], vec![]);
        let entry = f.body.entry_mut().unwrap();
        entry.push(mk_const(0));
        entry.push(mk_user(0, 1));
        let (removed, _) = eliminate_dead_arith_consts(&mut f);
        assert_eq!(removed, 0);
        let entry = f.body.entry().unwrap();
        assert_eq!(entry.ops.len(), 2);
    }

    #[test]
    fn eliminate_dead_arith_consts_recurses_into_nested_region() {
        let mut f = MirFunc::new("test", vec![], vec![]);
        let entry = f.body.entry_mut().unwrap();
        let mut if_op = MirOp::std("scf.if");
        if_op.operands.push(ValueId(0));
        let mut then_region = MirRegion::with_entry(vec![]);
        if let Some(b) = then_region.entry_mut() {
            b.push(mk_const(50)); // dead : unused.
        }
        if_op.regions.push(then_region);
        entry.push(if_op);
        let (removed, _) = eliminate_dead_arith_consts(&mut f);
        assert_eq!(removed, 1);
    }

    #[test]
    fn eliminate_dead_ops_removes_dead_pure_arith() {
        // %0 = const ; %1 = const ; %2 = addi(%0, %1) ; trailing-yield(%2)
        //
        // Without a "return-anchor" the trailing addi would itself be dead
        // (its result %2 has no consumer). The DCE pass preserves the
        // trailing op's results as a return-anchor, so the addi survives ;
        // the consts are still operand-used by addi, so they survive too.
        // Result : zero removals.
        let mut f = MirFunc::new("test", vec![], vec![]);
        let entry = f.body.entry_mut().unwrap();
        entry.push(mk_const(0));
        entry.push(mk_const(1));
        let mut user = MirOp::std("arith.addi");
        user.operands.push(ValueId(0));
        user.operands.push(ValueId(1));
        user.results
            .push(MirValue::new(ValueId(2), MirType::Int(IntWidth::I32)));
        entry.push(user);

        let removed = eliminate_dead_ops(&mut f);
        assert_eq!(removed, 0);
        let entry = f.body.entry().unwrap();
        assert_eq!(entry.ops.len(), 3);
    }

    #[test]
    fn eliminate_dead_ops_removes_dead_pure_arith_when_followed_by_anchor() {
        // %0 = const ; %1 = const ; %2 = addi(%0, %1) ; %3 = const (return-anchor)
        // The trailing %3 const is the return-anchor, so it stays live.
        // First iter : the addi gets removed (result %2 has no consumer
        // once anchor is %3). Then the consts %0, %1 lose their consumer
        // and get removed too on subsequent iters. The anchor %3 stays.
        // Total removals : 3 (addi + 2 consts).
        let mut f = MirFunc::new("test", vec![], vec![]);
        let entry = f.body.entry_mut().unwrap();
        entry.push(mk_const(0));
        entry.push(mk_const(1));
        let mut user = MirOp::std("arith.addi");
        user.operands.push(ValueId(0));
        user.operands.push(ValueId(1));
        user.results
            .push(MirValue::new(ValueId(2), MirType::Int(IntWidth::I32)));
        entry.push(user);
        let mut anchor = MirOp::std("arith.constant");
        anchor
            .results
            .push(MirValue::new(ValueId(3), MirType::Int(IntWidth::I32)));
        anchor.attributes.push(("value".into(), "99".into()));
        entry.push(anchor);

        let removed = eliminate_dead_ops(&mut f);
        assert_eq!(removed, 3);
        let entry = f.body.entry().unwrap();
        // Only the anchor remains.
        assert_eq!(entry.ops.len(), 1);
        assert_eq!(entry.ops[0].results[0].id, ValueId(3));
    }

    #[test]
    fn eliminate_dead_ops_preserves_side_effects() {
        let mut f = MirFunc::new("test", vec![], vec![]);
        let entry = f.body.entry_mut().unwrap();
        let mut alloc = MirOp::std("cssl.heap.alloc");
        alloc.operands.push(ValueId(0));
        alloc.operands.push(ValueId(1));
        alloc.results.push(MirValue::new(ValueId(2), MirType::Ptr));
        entry.push(alloc);
        let removed = eliminate_dead_ops(&mut f);
        assert_eq!(removed, 0, "side-effecting op must not be DCE'd");
    }

    #[test]
    fn eliminate_dead_ops_preserves_telemetry_record() {
        let mut f = MirFunc::new("test", vec![], vec![]);
        let entry = f.body.entry_mut().unwrap();
        let mut tel = MirOp::std("cssl.telemetry.record");
        tel.operands.push(ValueId(0));
        entry.push(tel);
        let removed = eliminate_dead_ops(&mut f);
        assert_eq!(removed, 0);
    }

    #[test]
    fn run_dce_pass_chain() {
        // Build : %0 = const true ; if %0 { yield 100 } else { yield 200 }
        let mut f = MirFunc::new("test", vec![], vec![]);
        let entry = f.body.entry_mut().unwrap();
        let mut cond = MirOp::std("arith.constant");
        cond.results.push(MirValue::new(ValueId(0), MirType::Bool));
        cond.attributes.push(("value".into(), "true".into()));
        entry.push(cond);
        entry.push(mk_if_with_branches(0));

        // A fold-list says : block-0 op-1 takes then-branch.
        let folds = [BranchFold {
            block_idx: 0,
            op_idx: 1,
            taken_branch: true,
        }];
        let report = run_dce_pass(&mut f, &folds);
        assert_eq!(report.branches_eliminated, 1);
        // After branch-fold : %0 const + the inlined yield. The const is
        // now dead because nothing references %0 after the if-op vanished
        // (the yield doesn't operand-use %0).
        assert_eq!(report.dead_consts_removed, 1);
        let entry = f.body.entry().unwrap();
        // Only the inlined yield remains.
        assert_eq!(entry.ops.len(), 1);
        assert_eq!(entry.ops[0].name, "scf.yield");
    }

    #[test]
    fn is_pure_dce_safe_classifies_arith_pure() {
        assert!(is_pure_dce_safe("arith.addi"));
        assert!(is_pure_dce_safe("arith.muli"));
        assert!(is_pure_dce_safe("arith.constant"));
    }

    #[test]
    fn is_pure_dce_safe_excludes_side_effects() {
        assert!(!is_pure_dce_safe("cssl.heap.alloc"));
        assert!(!is_pure_dce_safe("cssl.heap.dealloc"));
        assert!(!is_pure_dce_safe("cssl.fs.open"));
        assert!(!is_pure_dce_safe("cssl.net.connect"));
        assert!(!is_pure_dce_safe("cssl.telemetry.probe"));
        assert!(!is_pure_dce_safe("cssl.gpu.barrier"));
    }

    #[test]
    fn is_pure_dce_safe_includes_diff_ops() {
        assert!(is_pure_dce_safe("cssl.diff.primal"));
        assert!(is_pure_dce_safe("cssl.diff.fwd"));
        assert!(is_pure_dce_safe("cssl.diff.bwd"));
    }

    #[test]
    fn eliminate_branches_with_empty_fold_list_is_noop() {
        let mut f = MirFunc::new("test", vec![], vec![]);
        let entry = f.body.entry_mut().unwrap();
        entry.push(mk_if_with_branches(0));
        let count = eliminate_branches(&mut f, &[]);
        assert_eq!(count, 0);
        let entry = f.body.entry().unwrap();
        assert_eq!(entry.ops.len(), 1);
    }

    #[test]
    fn eliminate_branches_skips_out_of_bounds_indices() {
        let mut f = MirFunc::new("test", vec![], vec![]);
        let folds = [BranchFold {
            block_idx: 99,
            op_idx: 99,
            taken_branch: true,
        }];
        let count = eliminate_branches(&mut f, &folds);
        assert_eq!(count, 0);
    }
}
