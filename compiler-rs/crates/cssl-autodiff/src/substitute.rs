//! Real dual-substitution : expand recognized primitive-ops into tangent-carrying
//! MIR ops (forward mode) + adjoint-accumulation MIR ops (reverse mode).
//!
//! § SPEC : `specs/05_AUTODIFF.csl` § IMPLEMENTATION § source-to-source on MIR +
//!          §§ per-op rules-table.
//!
//! § SCOPE (T7-phase-2b / this commit)
//!   Phase-2a emitted `diff_recipe_{fwd,bwd}` attributes on cloned primal ops.
//!   Phase-2b replaces those attributes with **actual** MLIR `arith.*` + `func.call`
//!   ops that propagate tangent / adjoint values through the body. The textual
//!   recipe is still attached for debugging + visibility, but the real work is
//!   done by the emitted ops.
//!
//!   § FORWARD (JVP) : fn f(x) = y → fn f_fwd(x, d_x) = (y, d_y)
//!     For each recognized primitive op `%y = primitive(%a, %b, ...)`, emit one
//!     or more tangent ops that compute `%d_y = tangent_expr(%a, %b, %d_a, %d_b, ...)`
//!     using the chain-rule per `rules::DiffRuleTable` (FAdd / FSub / FMul / FDiv /
//!     FNeg + Sqrt / Sin / Cos / Exp / Log).
//!
//!   § REVERSE (VJP) : fn f(x) = y → fn f_bwd(x, d_y) = d_x
//!     For each recognized primitive op **in reverse iteration order**, emit
//!     adjoint-accumulation ops that update `%d_a += contribution_a(%d_y, %a, %b, ...)`
//!     etc. At the end the seeded adjoint-map holds `d_x` for each primal param.
//!
//!   Both variants append tangent / adjoint params after primal params and
//!   tangent / adjoint results after primal results (structured as additional
//!   entry-block-args + appended `func.return` operands). No tape / control-flow-
//!   record / GPU-location resolution at this commit — those are phase-2c work.
//!
//! § T7-phase-2c DEFERRED
//!   - Tape-buffer allocation for control-flow (scf.if / scf.for / scf.while).
//!   - `@checkpoint` selective recomputation (trade memory for FLOPs).
//!   - GPU-AD tape-location resolution (device / shared / unified memory).
//!   - Multi-return tangent-tuple emission when primal has multi-result.
//!   - Killer-app gate : `bwd_diff(sphere_sdf)(p).d_p` bit-exact vs analytic
//!     (composes with T9-phase-2 SMT unsat-verdict).

use std::collections::HashMap;

use cssl_mir::{FloatWidth, MirFunc, MirOp, MirRegion, MirType, MirValue, ValueId};

use crate::rules::{DiffMode, DiffRuleTable, Primitive};
use crate::walker::{op_to_primitive, specialize_transcendental};

/// Mapping from primal SSA value → tangent (fwd) or adjoint (bwd) SSA value.
///
/// Used by both forward- and reverse-mode substitution — the semantics of the
/// mapped value differ (tangent vs adjoint) but the data-structure is the same.
#[derive(Debug, Clone, Default)]
pub struct TangentMap {
    inner: HashMap<ValueId, ValueId>,
}

impl TangentMap {
    /// Empty map.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a primal → derivative mapping.
    pub fn insert(&mut self, primal: ValueId, derivative: ValueId) {
        self.inner.insert(primal, derivative);
    }

    /// Look up the derivative of a primal value.
    #[must_use]
    pub fn get(&self, primal: ValueId) -> Option<ValueId> {
        self.inner.get(&primal).copied()
    }

    /// Number of entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// `true` iff no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

/// Per-substitution telemetry.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SubstitutionReport {
    /// Number of primitive-ops that had their rule applied.
    pub primitives_substituted: u32,
    /// Number of tangent / adjoint ops emitted (including intermediate ops).
    pub tangent_ops_emitted: u32,
    /// Number of ops that looked like primitives but had no rule — annotated-only.
    pub unsupported_primitives: u32,
    /// Number of tangent / adjoint params synthesized on the variant signature.
    pub tangent_params_added: u32,
    /// Number of tangent / adjoint results added to the variant signature.
    pub tangent_results_added: u32,
}

impl SubstitutionReport {
    /// Short diagnostic-summary line.
    #[must_use]
    pub fn summary(&self) -> String {
        format!(
            "AD-substitute : {} primitives substituted / {} tangent-ops emitted \
             / {} unsupported / {} tangent-params / {} tangent-results",
            self.primitives_substituted,
            self.tangent_ops_emitted,
            self.unsupported_primitives,
            self.tangent_params_added,
            self.tangent_results_added,
        )
    }
}

/// Build the forward-mode variant of `primal` : returns `(variant, TangentMap, report)`.
///
/// The returned [`MirFunc`] :
///   * is named `<primal.name>_fwd`,
///   * has `primal.params ++ [tangent-params]` as its parameter list (one tangent
///     param per float primal param, skipping non-float params),
///   * carries `primal.results ++ [tangent-result]` in its result list when the
///     primal returns a value, and
///   * has its body populated with a clone of the primal ops interleaved with
///     the emitted tangent ops per the [`DiffRuleTable`].
///
/// The returned [`TangentMap`] reflects the final state after the walk — useful
/// for downstream inspection + tests.
#[must_use]
pub fn apply_fwd(
    primal: &MirFunc,
    rules: &DiffRuleTable,
) -> (MirFunc, TangentMap, SubstitutionReport) {
    apply_mode(primal, rules, DiffMode::Fwd)
}

/// Build the reverse-mode variant of `primal`.
///
/// The returned [`MirFunc`] :
///   * is named `<primal.name>_bwd`,
///   * has `primal.params ++ [adjoint-in-param]` as its parameter list (one
///     adjoint per primal float-result — stage-0 assumes 1 result),
///   * carries `[adjoint-out-results]` in its result list (one per primal float
///     param) appended after the primal results,
///   * has its body populated with a clone of the primal ops **followed by**
///     adjoint-accumulation ops emitted in reverse-iteration order.
#[must_use]
pub fn apply_bwd(
    primal: &MirFunc,
    rules: &DiffRuleTable,
) -> (MirFunc, TangentMap, SubstitutionReport) {
    apply_mode(primal, rules, DiffMode::Bwd)
}

// ─────────────────────────────────────────────────────────────────────────
// § Internal driver — shared fwd + bwd scaffolding, mode-dispatched body.
// ─────────────────────────────────────────────────────────────────────────

fn apply_mode(
    primal: &MirFunc,
    rules: &DiffRuleTable,
    mode: DiffMode,
) -> (MirFunc, TangentMap, SubstitutionReport) {
    let mut variant = primal.clone();
    variant.name = format!("{}{}", primal.name, mode.suffix());
    variant
        .attributes
        .push(("diff_variant".to_string(), mode_str(mode).to_string()));
    variant
        .attributes
        .push(("diff_primal_name".to_string(), primal.name.clone()));

    // Ensure `next_value_id` is strictly greater than every SSA-id already used
    // in the body (operands + results). Callers that build ops manually may not
    // advance `next_value_id`, so this scan keeps fresh-id allocation safe.
    reconcile_next_value_id(&mut variant);

    let original_param_count = primal.params.len();
    let mut tangent_map = TangentMap::new();
    let mut report = SubstitutionReport::default();

    // Synthesize tangent / adjoint params on the variant signature.
    synthesize_tangent_params(&mut variant, mode, &mut tangent_map, &mut report);

    // Apply per-mode substitution to the entry-block ops.
    if mode == DiffMode::Fwd {
        substitute_fwd(&mut variant, rules, &mut tangent_map, &mut report);
    } else {
        substitute_bwd(
            &mut variant,
            rules,
            original_param_count,
            &mut tangent_map,
            &mut report,
        );
    }

    // Synthesize tangent / adjoint result types on the variant signature.
    synthesize_tangent_results(&mut variant, mode, original_param_count, &mut report);

    (variant, tangent_map, report)
}

/// Scan every op-operand + op-result in every block / nested region and set
/// `variant.next_value_id` to `max(found) + 1`. This guards against callers that
/// hand-build `MirFunc` bodies without advancing the counter (as the test
/// helpers do).
fn reconcile_next_value_id(variant: &mut MirFunc) {
    let mut max_id: u32 = 0;
    for arg in variant.body.entry().map_or(&[][..], |e| e.args.as_slice()) {
        max_id = max_id.max(arg.id.0);
    }
    scan_region(&variant.body, &mut max_id);
    let desired = max_id.saturating_add(1);
    if desired > variant.next_value_id {
        variant.next_value_id = desired;
    }
}

fn scan_region(region: &MirRegion, max_id: &mut u32) {
    for block in &region.blocks {
        for arg in &block.args {
            *max_id = (*max_id).max(arg.id.0);
        }
        for op in &block.ops {
            for operand in &op.operands {
                *max_id = (*max_id).max(operand.0);
            }
            for result in &op.results {
                *max_id = (*max_id).max(result.id.0);
            }
            for nested in &op.regions {
                scan_region(nested, max_id);
            }
        }
    }
}

/// Synthesize tangent / adjoint params on the variant signature.
///
/// § FWD : interleave `[a, d_a, b, d_b, ...]` — every float primal param gets a
/// paired tangent param immediately after it. Non-float params are unchanged.
///
/// § BWD : keep primal params unchanged, then append one adjoint-in param per
/// primal **result** (the seeded d_y). Primal params carry no tangent-in the
/// reverse-mode signature — their adjoint-out values are returned as results
/// (see [`synthesize_tangent_results`]).
fn synthesize_tangent_params(
    variant: &mut MirFunc,
    mode: DiffMode,
    tangent_map: &mut TangentMap,
    report: &mut SubstitutionReport,
) {
    let Some(entry) = variant.body.entry_mut() else {
        return;
    };
    let original_args = core::mem::take(&mut entry.args);
    let original_params = core::mem::take(&mut variant.params);
    let mut new_args: Vec<MirValue> = Vec::with_capacity(original_args.len() * 2);
    let mut new_params: Vec<MirType> = Vec::with_capacity(original_params.len() * 2);
    match mode {
        DiffMode::Fwd => {
            for (i, arg) in original_args.iter().enumerate() {
                let primal_ty = original_params
                    .get(i)
                    .cloned()
                    .unwrap_or_else(|| arg.ty.clone());
                new_args.push(arg.clone());
                new_params.push(primal_ty.clone());
                if is_float(&primal_ty) || is_float(&arg.ty) {
                    let tangent_id = ValueId(variant.next_value_id);
                    variant.next_value_id = variant.next_value_id.saturating_add(1);
                    let tangent_ty = tangent_type_of(&arg.ty);
                    new_args.push(MirValue::new(tangent_id, tangent_ty.clone()));
                    new_params.push(tangent_ty);
                    tangent_map.insert(arg.id, tangent_id);
                    report.tangent_params_added = report.tangent_params_added.saturating_add(1);
                }
            }
        }
        DiffMode::Bwd => {
            // Primal params retained as-is (primal values reach adjoint-ops
            // verbatim).
            for (i, arg) in original_args.iter().enumerate() {
                let primal_ty = original_params
                    .get(i)
                    .cloned()
                    .unwrap_or_else(|| arg.ty.clone());
                new_args.push(arg.clone());
                new_params.push(primal_ty);
            }
            // Append one adjoint-in (d_y) per primal float-result.
            let result_types = variant.results.clone();
            for r in &result_types {
                if is_float(r) {
                    let adjoint_in_id = ValueId(variant.next_value_id);
                    variant.next_value_id = variant.next_value_id.saturating_add(1);
                    new_args.push(MirValue::new(adjoint_in_id, r.clone()));
                    new_params.push(r.clone());
                    // Seed : sentinel key ValueId(u32::MAX) carries the seeded d_y.
                    // The substitute_bwd driver connects this to the primal
                    // result when it locates `func.return` in the body.
                    tangent_map.insert(ValueId(u32::MAX), adjoint_in_id);
                    report.tangent_params_added = report.tangent_params_added.saturating_add(1);
                }
            }
        }
        DiffMode::Primal => {
            new_args = original_args;
            new_params = original_params;
        }
    }
    entry.args = new_args;
    variant.params = new_params;
}

/// Append tangent / adjoint result types to the variant signature.
///
/// § FWD : `[y, d_y]` interleaved for each primal result.
///
/// § BWD : drop primal results, return one adjoint-out per **original** primal
/// float-param. `original_param_count` tells us where the primal params end —
/// the adjoint-in params appended by [`synthesize_tangent_params`] are skipped.
fn synthesize_tangent_results(
    variant: &mut MirFunc,
    mode: DiffMode,
    original_param_count: usize,
    report: &mut SubstitutionReport,
) {
    let original_results = core::mem::take(&mut variant.results);
    let mut new_results: Vec<MirType> = Vec::with_capacity(original_results.len() * 2);
    match mode {
        DiffMode::Fwd => {
            for r in &original_results {
                new_results.push(r.clone());
                if is_float(r) {
                    new_results.push(tangent_type_of(r));
                    report.tangent_results_added = report.tangent_results_added.saturating_add(1);
                }
            }
        }
        DiffMode::Bwd => {
            for p in variant.params.iter().take(original_param_count) {
                if is_float(p) {
                    new_results.push(p.clone());
                    report.tangent_results_added = report.tangent_results_added.saturating_add(1);
                }
            }
        }
        DiffMode::Primal => {
            new_results = original_results;
        }
    }
    variant.results = new_results;
}

// ─────────────────────────────────────────────────────────────────────────
// § Forward-mode substitution : walk ops in-order, emit tangent-ops inline.
// ─────────────────────────────────────────────────────────────────────────

fn substitute_fwd(
    variant: &mut MirFunc,
    rules: &DiffRuleTable,
    tangent_map: &mut TangentMap,
    report: &mut SubstitutionReport,
) {
    let Some(entry) = variant.body.entry_mut() else {
        return;
    };
    let original_ops = core::mem::take(&mut entry.ops);
    let mut next_id = variant.next_value_id;
    let mut new_ops: Vec<MirOp> = Vec::with_capacity(original_ops.len() * 3);
    for op in original_ops {
        let primitive = recognize_primitive(&op);
        // T11-D23 : at `func.return %v`, append the tangent of `%v` as an
        // additional return-operand so the fn body returns (primal, tangent)
        // matching the signature synthesized by `synthesize_tangent_results`.
        // Without this append the fn signature says "2 results" but the body
        // only returns 1 — making the variant not directly executable.
        if op.name == "func.return" {
            let mut ret_op = op.clone();
            let primal_ret_ids: Vec<ValueId> = op.operands.clone();
            for primal_ret in &primal_ret_ids {
                if let Some(tangent_id) = tangent_map.get(*primal_ret) {
                    ret_op = ret_op.with_operand(tangent_id);
                }
            }
            new_ops.push(ret_op);
            continue;
        }
        new_ops.push(op.clone());
        if let Some(prim) = primitive {
            if let Some(rule) = rules.lookup(prim, DiffMode::Fwd) {
                let emitted =
                    emit_fwd_tangent_ops(&op, prim, rule.recipe, tangent_map, &mut next_id);
                report.tangent_ops_emitted = report
                    .tangent_ops_emitted
                    .saturating_add(emitted.len() as u32);
                report.primitives_substituted = report.primitives_substituted.saturating_add(1);
                new_ops.extend(emitted);
            } else {
                report.unsupported_primitives = report.unsupported_primitives.saturating_add(1);
            }
        }
        // Recurse into nested regions (scf.if / scf.for bodies).
        if let Some(last) = new_ops.last_mut() {
            for nested in &mut last.regions {
                substitute_fwd_region(nested, rules, tangent_map, report, &mut next_id);
            }
        }
    }
    entry.ops = new_ops;
    variant.next_value_id = next_id;
}

/// Recurse fwd-substitution into a nested region.
fn substitute_fwd_region(
    region: &mut MirRegion,
    rules: &DiffRuleTable,
    tangent_map: &mut TangentMap,
    report: &mut SubstitutionReport,
    next_id: &mut u32,
) {
    for block in &mut region.blocks {
        let ops = core::mem::take(&mut block.ops);
        let mut new_ops: Vec<MirOp> = Vec::with_capacity(ops.len() * 3);
        for op in ops {
            let primitive = recognize_primitive(&op);
            new_ops.push(op.clone());
            if let Some(prim) = primitive {
                if let Some(rule) = rules.lookup(prim, DiffMode::Fwd) {
                    let emitted =
                        emit_fwd_tangent_ops(&op, prim, rule.recipe, tangent_map, next_id);
                    report.tangent_ops_emitted = report
                        .tangent_ops_emitted
                        .saturating_add(emitted.len() as u32);
                    report.primitives_substituted = report.primitives_substituted.saturating_add(1);
                    new_ops.extend(emitted);
                } else {
                    report.unsupported_primitives = report.unsupported_primitives.saturating_add(1);
                }
            }
            if let Some(last) = new_ops.last_mut() {
                for nested in &mut last.regions {
                    substitute_fwd_region(nested, rules, tangent_map, report, next_id);
                }
            }
        }
        block.ops = new_ops;
    }
}

/// Emit the sequence of MIR ops that compute the tangent of `op` under fwd-mode.
///
/// Returns the emitted ops (ready to append). On the way, it updates
/// `tangent_map` so that `%d_y` is recorded for the primal result, and bumps
/// `next_id` for every fresh SSA value allocated.
fn emit_fwd_tangent_ops(
    op: &MirOp,
    prim: Primitive,
    recipe: &str,
    tangent_map: &mut TangentMap,
    next_id: &mut u32,
) -> Vec<MirOp> {
    let primal_result_id = match op.results.first() {
        Some(r) => r.id,
        None => return Vec::new(),
    };
    let result_ty = op
        .results
        .first()
        .map_or_else(default_tangent_ty, |r| r.ty.clone());
    match prim {
        Primitive::FAdd => emit_fadd_fwd(op, primal_result_id, &result_ty, tangent_map, next_id),
        Primitive::FSub => emit_fsub_fwd(op, primal_result_id, &result_ty, tangent_map, next_id),
        Primitive::FMul => emit_fmul_fwd(op, primal_result_id, &result_ty, tangent_map, next_id),
        Primitive::FDiv => emit_fdiv_fwd(op, primal_result_id, &result_ty, tangent_map, next_id),
        Primitive::FNeg => emit_fneg_fwd(op, primal_result_id, &result_ty, tangent_map, next_id),
        Primitive::Sqrt => emit_sqrt_fwd(op, primal_result_id, &result_ty, tangent_map, next_id),
        Primitive::Sin => emit_sin_fwd(op, primal_result_id, &result_ty, tangent_map, next_id),
        Primitive::Cos => emit_cos_fwd(op, primal_result_id, &result_ty, tangent_map, next_id),
        Primitive::Exp => emit_exp_fwd(op, primal_result_id, &result_ty, tangent_map, next_id),
        Primitive::Log => emit_log_fwd(op, primal_result_id, &result_ty, tangent_map, next_id),
        // T11-D15 : real branchful Fwd emission for piecewise-linear primitives.
        Primitive::Min => emit_min_fwd(op, primal_result_id, &result_ty, tangent_map, next_id),
        Primitive::Max => emit_max_fwd(op, primal_result_id, &result_ty, tangent_map, next_id),
        Primitive::Abs => emit_abs_fwd(op, primal_result_id, &result_ty, tangent_map, next_id),
        Primitive::Sign => emit_sign_fwd(op, primal_result_id, &result_ty, tangent_map, next_id),
        // Call / Load / Store / If / Loop — stage-0 emits a structural placeholder
        // that carries the recipe attribute. Full expansion is phase-2c (requires
        // tape / callee-variant resolution / region traversal).
        Primitive::Call | Primitive::Load | Primitive::Store | Primitive::If | Primitive::Loop => {
            let d_y = fresh_id(next_id);
            tangent_map.insert(primal_result_id, d_y);
            vec![MirOp::std("cssl.diff.fwd_placeholder")
                .with_result(d_y, result_ty.clone())
                .with_attribute("primitive", prim.name())
                .with_attribute("recipe", recipe)]
        }
    }
}

// ─── FAdd : y = a + b  ⇒  d_y = d_a + d_b ────────────────────────────────
fn emit_fadd_fwd(
    op: &MirOp,
    primal_result: ValueId,
    result_ty: &MirType,
    tangent_map: &mut TangentMap,
    next_id: &mut u32,
) -> Vec<MirOp> {
    let (Some(&a), Some(&b)) = (op.operands.first(), op.operands.get(1)) else {
        return Vec::new();
    };
    let d_a = tangent_or_zero(tangent_map, a);
    let d_b = tangent_or_zero(tangent_map, b);
    let d_y = fresh_id(next_id);
    tangent_map.insert(primal_result, d_y);
    vec![MirOp::std("arith.addf")
        .with_operand(d_a)
        .with_operand(d_b)
        .with_result(d_y, result_ty.clone())
        .with_attribute("diff_role", "tangent")
        .with_attribute("diff_primitive", "fadd")]
}

// ─── FSub : y = a - b  ⇒  d_y = d_a - d_b ────────────────────────────────
fn emit_fsub_fwd(
    op: &MirOp,
    primal_result: ValueId,
    result_ty: &MirType,
    tangent_map: &mut TangentMap,
    next_id: &mut u32,
) -> Vec<MirOp> {
    let (Some(&a), Some(&b)) = (op.operands.first(), op.operands.get(1)) else {
        return Vec::new();
    };
    let d_a = tangent_or_zero(tangent_map, a);
    let d_b = tangent_or_zero(tangent_map, b);
    let d_y = fresh_id(next_id);
    tangent_map.insert(primal_result, d_y);
    vec![MirOp::std("arith.subf")
        .with_operand(d_a)
        .with_operand(d_b)
        .with_result(d_y, result_ty.clone())
        .with_attribute("diff_role", "tangent")
        .with_attribute("diff_primitive", "fsub")]
}

// ─── FMul : y = a * b  ⇒  d_y = d_a*b + a*d_b  (2 muls + 1 add) ──────────
fn emit_fmul_fwd(
    op: &MirOp,
    primal_result: ValueId,
    result_ty: &MirType,
    tangent_map: &mut TangentMap,
    next_id: &mut u32,
) -> Vec<MirOp> {
    let (Some(&a), Some(&b)) = (op.operands.first(), op.operands.get(1)) else {
        return Vec::new();
    };
    let d_a = tangent_or_zero(tangent_map, a);
    let d_b = tangent_or_zero(tangent_map, b);
    let t0 = fresh_id(next_id); // d_a * b
    let t1 = fresh_id(next_id); // a * d_b
    let d_y = fresh_id(next_id);
    tangent_map.insert(primal_result, d_y);
    vec![
        MirOp::std("arith.mulf")
            .with_operand(d_a)
            .with_operand(b)
            .with_result(t0, result_ty.clone())
            .with_attribute("diff_role", "tangent")
            .with_attribute("diff_primitive", "fmul"),
        MirOp::std("arith.mulf")
            .with_operand(a)
            .with_operand(d_b)
            .with_result(t1, result_ty.clone())
            .with_attribute("diff_role", "tangent")
            .with_attribute("diff_primitive", "fmul"),
        MirOp::std("arith.addf")
            .with_operand(t0)
            .with_operand(t1)
            .with_result(d_y, result_ty.clone())
            .with_attribute("diff_role", "tangent")
            .with_attribute("diff_primitive", "fmul"),
    ]
}

// ─── FDiv : y = a / b  ⇒  d_y = (d_a*b - a*d_b) / (b*b)  (4 muls + 1 sub + 1 div) ─
fn emit_fdiv_fwd(
    op: &MirOp,
    primal_result: ValueId,
    result_ty: &MirType,
    tangent_map: &mut TangentMap,
    next_id: &mut u32,
) -> Vec<MirOp> {
    let (Some(&a), Some(&b)) = (op.operands.first(), op.operands.get(1)) else {
        return Vec::new();
    };
    let d_a = tangent_or_zero(tangent_map, a);
    let d_b = tangent_or_zero(tangent_map, b);
    let t0 = fresh_id(next_id); // d_a * b
    let t1 = fresh_id(next_id); // a * d_b
    let t2 = fresh_id(next_id); // t0 - t1
    let t3 = fresh_id(next_id); // b * b
    let d_y = fresh_id(next_id);
    tangent_map.insert(primal_result, d_y);
    vec![
        MirOp::std("arith.mulf")
            .with_operand(d_a)
            .with_operand(b)
            .with_result(t0, result_ty.clone())
            .with_attribute("diff_role", "tangent")
            .with_attribute("diff_primitive", "fdiv"),
        MirOp::std("arith.mulf")
            .with_operand(a)
            .with_operand(d_b)
            .with_result(t1, result_ty.clone())
            .with_attribute("diff_role", "tangent")
            .with_attribute("diff_primitive", "fdiv"),
        MirOp::std("arith.subf")
            .with_operand(t0)
            .with_operand(t1)
            .with_result(t2, result_ty.clone())
            .with_attribute("diff_role", "tangent")
            .with_attribute("diff_primitive", "fdiv"),
        MirOp::std("arith.mulf")
            .with_operand(b)
            .with_operand(b)
            .with_result(t3, result_ty.clone())
            .with_attribute("diff_role", "tangent")
            .with_attribute("diff_primitive", "fdiv"),
        MirOp::std("arith.divf")
            .with_operand(t2)
            .with_operand(t3)
            .with_result(d_y, result_ty.clone())
            .with_attribute("diff_role", "tangent")
            .with_attribute("diff_primitive", "fdiv"),
    ]
}

// ─── FNeg : y = -a  ⇒  d_y = -d_a ────────────────────────────────────────
fn emit_fneg_fwd(
    op: &MirOp,
    primal_result: ValueId,
    result_ty: &MirType,
    tangent_map: &mut TangentMap,
    next_id: &mut u32,
) -> Vec<MirOp> {
    let Some(&a) = op.operands.first() else {
        return Vec::new();
    };
    let d_a = tangent_or_zero(tangent_map, a);
    let d_y = fresh_id(next_id);
    tangent_map.insert(primal_result, d_y);
    vec![MirOp::std("arith.negf")
        .with_operand(d_a)
        .with_result(d_y, result_ty.clone())
        .with_attribute("diff_role", "tangent")
        .with_attribute("diff_primitive", "fneg")]
}

// ─── Sqrt : y = √a  ⇒  d_y = d_a / (2 * y)  (using the primal result y) ──
fn emit_sqrt_fwd(
    op: &MirOp,
    primal_result: ValueId,
    result_ty: &MirType,
    tangent_map: &mut TangentMap,
    next_id: &mut u32,
) -> Vec<MirOp> {
    let Some(&a) = op.operands.first() else {
        return Vec::new();
    };
    let d_a = tangent_or_zero(tangent_map, a);
    let two = fresh_id(next_id); // constant 2.0
    let t0 = fresh_id(next_id); // 2 * y
    let d_y = fresh_id(next_id);
    tangent_map.insert(primal_result, d_y);
    vec![
        MirOp::std("arith.constant")
            .with_result(two, result_ty.clone())
            .with_attribute("value", "2.0")
            .with_attribute("diff_role", "tangent")
            .with_attribute("diff_primitive", "sqrt"),
        MirOp::std("arith.mulf")
            .with_operand(two)
            .with_operand(primal_result)
            .with_result(t0, result_ty.clone())
            .with_attribute("diff_role", "tangent")
            .with_attribute("diff_primitive", "sqrt"),
        MirOp::std("arith.divf")
            .with_operand(d_a)
            .with_operand(t0)
            .with_result(d_y, result_ty.clone())
            .with_attribute("diff_role", "tangent")
            .with_attribute("diff_primitive", "sqrt"),
    ]
}

// ─── Sin : y = sin(a)  ⇒  d_y = d_a * cos(a) ─────────────────────────────
fn emit_sin_fwd(
    op: &MirOp,
    primal_result: ValueId,
    result_ty: &MirType,
    tangent_map: &mut TangentMap,
    next_id: &mut u32,
) -> Vec<MirOp> {
    let Some(&a) = op.operands.first() else {
        return Vec::new();
    };
    let d_a = tangent_or_zero(tangent_map, a);
    let cos_a = fresh_id(next_id);
    let d_y = fresh_id(next_id);
    tangent_map.insert(primal_result, d_y);
    vec![
        MirOp::std("func.call")
            .with_operand(a)
            .with_result(cos_a, result_ty.clone())
            .with_attribute("callee", "cos")
            .with_attribute("diff_role", "tangent")
            .with_attribute("diff_primitive", "sin"),
        MirOp::std("arith.mulf")
            .with_operand(d_a)
            .with_operand(cos_a)
            .with_result(d_y, result_ty.clone())
            .with_attribute("diff_role", "tangent")
            .with_attribute("diff_primitive", "sin"),
    ]
}

// ─── Cos : y = cos(a)  ⇒  d_y = -d_a * sin(a) ────────────────────────────
fn emit_cos_fwd(
    op: &MirOp,
    primal_result: ValueId,
    result_ty: &MirType,
    tangent_map: &mut TangentMap,
    next_id: &mut u32,
) -> Vec<MirOp> {
    let Some(&a) = op.operands.first() else {
        return Vec::new();
    };
    let d_a = tangent_or_zero(tangent_map, a);
    let sin_a = fresh_id(next_id);
    let neg_d_a = fresh_id(next_id);
    let d_y = fresh_id(next_id);
    tangent_map.insert(primal_result, d_y);
    vec![
        MirOp::std("func.call")
            .with_operand(a)
            .with_result(sin_a, result_ty.clone())
            .with_attribute("callee", "sin")
            .with_attribute("diff_role", "tangent")
            .with_attribute("diff_primitive", "cos"),
        MirOp::std("arith.negf")
            .with_operand(d_a)
            .with_result(neg_d_a, result_ty.clone())
            .with_attribute("diff_role", "tangent")
            .with_attribute("diff_primitive", "cos"),
        MirOp::std("arith.mulf")
            .with_operand(neg_d_a)
            .with_operand(sin_a)
            .with_result(d_y, result_ty.clone())
            .with_attribute("diff_role", "tangent")
            .with_attribute("diff_primitive", "cos"),
    ]
}

// ─── Exp : y = exp(a)  ⇒  d_y = d_a * y  (reuses primal result) ──────────
fn emit_exp_fwd(
    op: &MirOp,
    primal_result: ValueId,
    result_ty: &MirType,
    tangent_map: &mut TangentMap,
    next_id: &mut u32,
) -> Vec<MirOp> {
    let Some(&a) = op.operands.first() else {
        return Vec::new();
    };
    let d_a = tangent_or_zero(tangent_map, a);
    let d_y = fresh_id(next_id);
    tangent_map.insert(primal_result, d_y);
    vec![MirOp::std("arith.mulf")
        .with_operand(d_a)
        .with_operand(primal_result)
        .with_result(d_y, result_ty.clone())
        .with_attribute("diff_role", "tangent")
        .with_attribute("diff_primitive", "exp")]
}

// ─── Log : y = log(a)  ⇒  d_y = d_a / a ──────────────────────────────────
fn emit_log_fwd(
    op: &MirOp,
    primal_result: ValueId,
    result_ty: &MirType,
    tangent_map: &mut TangentMap,
    next_id: &mut u32,
) -> Vec<MirOp> {
    let Some(&a) = op.operands.first() else {
        return Vec::new();
    };
    let d_a = tangent_or_zero(tangent_map, a);
    let d_y = fresh_id(next_id);
    tangent_map.insert(primal_result, d_y);
    vec![MirOp::std("arith.divf")
        .with_operand(d_a)
        .with_operand(a)
        .with_result(d_y, result_ty.clone())
        .with_attribute("diff_role", "tangent")
        .with_attribute("diff_primitive", "log")]
}

// ─── Min : y = min(a, b)  ⇒  d_y = select(a ≤ b, d_a, d_b) ───────────────
fn emit_min_fwd(
    op: &MirOp,
    primal_result: ValueId,
    result_ty: &MirType,
    tangent_map: &mut TangentMap,
    next_id: &mut u32,
) -> Vec<MirOp> {
    emit_piecewise_binary_fwd(
        op,
        primal_result,
        result_ty,
        tangent_map,
        next_id,
        "ole", // ordered-less-equal : a ≤ b picks a's tangent
        "min",
    )
}

// ─── Max : y = max(a, b)  ⇒  d_y = select(a ≥ b, d_a, d_b) ───────────────
fn emit_max_fwd(
    op: &MirOp,
    primal_result: ValueId,
    result_ty: &MirType,
    tangent_map: &mut TangentMap,
    next_id: &mut u32,
) -> Vec<MirOp> {
    emit_piecewise_binary_fwd(
        op,
        primal_result,
        result_ty,
        tangent_map,
        next_id,
        "oge", // ordered-greater-equal : a ≥ b picks a's tangent
        "max",
    )
}

/// Shared `min`/`max` Fwd emitter : `d_y = select(cmp(a, b), d_a, d_b)`.
/// `predicate` selects between `"ole"` (min) and `"oge"` (max).
fn emit_piecewise_binary_fwd(
    op: &MirOp,
    primal_result: ValueId,
    result_ty: &MirType,
    tangent_map: &mut TangentMap,
    next_id: &mut u32,
    predicate: &'static str,
    prim_name: &'static str,
) -> Vec<MirOp> {
    let (Some(&a), Some(&b)) = (op.operands.first(), op.operands.get(1)) else {
        return Vec::new();
    };
    let d_a = tangent_or_zero(tangent_map, a);
    let d_b = tangent_or_zero(tangent_map, b);
    let cmp_id = fresh_id(next_id);
    let d_y = fresh_id(next_id);
    tangent_map.insert(primal_result, d_y);
    vec![
        MirOp::std("arith.cmpf")
            .with_operand(a)
            .with_operand(b)
            .with_result(cmp_id, MirType::Bool)
            .with_attribute("predicate", predicate)
            .with_attribute("diff_role", "tangent")
            .with_attribute("diff_primitive", prim_name),
        MirOp::std("arith.select")
            .with_operand(cmp_id)
            .with_operand(d_a)
            .with_operand(d_b)
            .with_result(d_y, result_ty.clone())
            .with_attribute("diff_role", "tangent")
            .with_attribute("diff_primitive", prim_name),
    ]
}

// ─── Abs : y = |x|  ⇒  d_y = select(x ≥ 0, d_x, -d_x) ────────────────────
fn emit_abs_fwd(
    op: &MirOp,
    primal_result: ValueId,
    result_ty: &MirType,
    tangent_map: &mut TangentMap,
    next_id: &mut u32,
) -> Vec<MirOp> {
    let Some(&x) = op.operands.first() else {
        return Vec::new();
    };
    let d_x = tangent_or_zero(tangent_map, x);
    let zero_id = fresh_id(next_id);
    let cmp_id = fresh_id(next_id);
    let neg_d_x = fresh_id(next_id);
    let d_y = fresh_id(next_id);
    tangent_map.insert(primal_result, d_y);
    vec![
        MirOp::std("arith.constant")
            .with_result(zero_id, result_ty.clone())
            .with_attribute("value", "0.0")
            .with_attribute("diff_role", "tangent")
            .with_attribute("diff_primitive", "abs"),
        MirOp::std("arith.cmpf")
            .with_operand(x)
            .with_operand(zero_id)
            .with_result(cmp_id, MirType::Bool)
            .with_attribute("predicate", "oge")
            .with_attribute("diff_role", "tangent")
            .with_attribute("diff_primitive", "abs"),
        MirOp::std("arith.negf")
            .with_operand(d_x)
            .with_result(neg_d_x, result_ty.clone())
            .with_attribute("diff_role", "tangent")
            .with_attribute("diff_primitive", "abs"),
        MirOp::std("arith.select")
            .with_operand(cmp_id)
            .with_operand(d_x)
            .with_operand(neg_d_x)
            .with_result(d_y, result_ty.clone())
            .with_attribute("diff_role", "tangent")
            .with_attribute("diff_primitive", "abs"),
    ]
}

// ─── Sign : y = sign(x)  ⇒  d_y = 0 ──────────────────────────────────────
fn emit_sign_fwd(
    _op: &MirOp,
    primal_result: ValueId,
    result_ty: &MirType,
    tangent_map: &mut TangentMap,
    next_id: &mut u32,
) -> Vec<MirOp> {
    let d_y = fresh_id(next_id);
    tangent_map.insert(primal_result, d_y);
    vec![MirOp::std("arith.constant")
        .with_result(d_y, result_ty.clone())
        .with_attribute("value", "0.0")
        .with_attribute("diff_role", "tangent")
        .with_attribute("diff_primitive", "sign")]
}

// ─────────────────────────────────────────────────────────────────────────
// § Reverse-mode substitution : walk ops in REVERSE, emit adjoint-ops.
// ─────────────────────────────────────────────────────────────────────────

fn substitute_bwd(
    variant: &mut MirFunc,
    rules: &DiffRuleTable,
    original_param_count: usize,
    tangent_map: &mut TangentMap,
    report: &mut SubstitutionReport,
) {
    let Some(entry) = variant.body.entry_mut() else {
        return;
    };
    let original_ops = core::mem::take(&mut entry.ops);
    let mut next_id = variant.next_value_id;
    let mut bwd_ops: Vec<MirOp> = Vec::with_capacity(original_ops.len() * 3);

    // Zero-init the adjoint of every primal float-param. This disambiguates
    // "primal-value used in adjoint op" from "initial adjoint of primal-param"
    // for downstream interpreters (e.g., `cssl_examples::ad_gate`). Without
    // this step, `tangent_or_zero` would alias the primal param's own ValueId
    // as the initial adjoint, forcing consumers to special-case the overlap.
    let zero_init_ty = entry
        .args
        .first()
        .map_or_else(default_tangent_ty, |a| tangent_type_of(&a.ty));
    for arg in entry.args.iter().take(original_param_count) {
        if is_float(&arg.ty) {
            let zero_id = ValueId(next_id);
            next_id = next_id.saturating_add(1);
            bwd_ops.push(
                MirOp::std("arith.constant")
                    .with_result(zero_id, zero_init_ty.clone())
                    .with_attribute("value", "0.0")
                    .with_attribute("diff_role", "adjoint")
                    .with_attribute("diff_primitive", "adjoint_init"),
            );
            tangent_map.insert(arg.id, zero_id);
            report.tangent_ops_emitted = report.tangent_ops_emitted.saturating_add(1);
        }
    }

    // Seed : locate `func.return` op + connect adjoint-in to its primal-result operand.
    let seed_adjoint = tangent_map.get(ValueId(u32::MAX));
    let mut primal_ops = original_ops.clone();
    if let Some(ret_op) = primal_ops.iter().rev().find(|o| o.name == "func.return") {
        if let Some(&ret_val) = ret_op.operands.first() {
            if let Some(d_y) = seed_adjoint {
                tangent_map.insert(ret_val, d_y);
            }
        }
    }

    // Walk primal ops in REVERSE order. For each recognized primitive, emit
    // adjoint-accumulation ops using the current adjoint-of-result.
    for op in primal_ops.iter_mut().rev() {
        if let Some(prim) = recognize_primitive(op) {
            if rules.lookup(prim, DiffMode::Bwd).is_some() {
                let emitted = emit_bwd_adjoint_ops(op, prim, tangent_map, &mut next_id);
                report.tangent_ops_emitted = report
                    .tangent_ops_emitted
                    .saturating_add(emitted.len() as u32);
                report.primitives_substituted = report.primitives_substituted.saturating_add(1);
                bwd_ops.extend(emitted);
            } else {
                report.unsupported_primitives = report.unsupported_primitives.saturating_add(1);
            }
        }
    }

    // Append a cssl.diff.bwd_return marker summarizing adjoint-outs for the
    // ORIGINAL primal float params (not the adjoint-in params we appended).
    let mut return_op =
        MirOp::std("cssl.diff.bwd_return").with_attribute("diff_role", "adjoint_return");
    for arg in entry.args.iter().take(original_param_count) {
        if is_float(&arg.ty) {
            if let Some(d_x) = tangent_map.get(arg.id) {
                return_op = return_op.with_operand(d_x);
            }
        }
    }
    bwd_ops.push(return_op);

    // Install : primal ops preserved (for recomputation) + bwd adjoint ops appended.
    let mut combined = original_ops;
    combined.extend(bwd_ops);
    entry.ops = combined;
    variant.next_value_id = next_id;
}

/// Emit the adjoint-accumulation ops for one primal op under reverse-mode.
///
/// Prepends inline `arith.constant 0.0 → %zero_adjoint` ops for any operand
/// whose adjoint hasn't been initialized yet — this covers intermediate MIR
/// values (like `%2 = x - r` in a chain-rule exercise). Primal params are
/// already zero-initialized at bwd-start by [`substitute_bwd`].
fn emit_bwd_adjoint_ops(
    op: &MirOp,
    prim: Primitive,
    tangent_map: &mut TangentMap,
    next_id: &mut u32,
) -> Vec<MirOp> {
    let primal_result_id = match op.results.first() {
        Some(r) => r.id,
        None => return Vec::new(),
    };
    let d_y = tangent_or_zero(tangent_map, primal_result_id);
    let result_ty = op
        .results
        .first()
        .map_or_else(default_tangent_ty, |r| r.ty.clone());

    // Zero-init any operand that hasn't received an adjoint yet. Handles the
    // "intermediate value whose adjoint starts at 0" case (distinct from the
    // bwd-start param zero-init performed in [`substitute_bwd`]).
    let mut init_ops: Vec<MirOp> = Vec::new();
    for &operand in &op.operands {
        if tangent_map.get(operand).is_none() {
            let zero_id = fresh_id(next_id);
            init_ops.push(
                MirOp::std("arith.constant")
                    .with_result(zero_id, result_ty.clone())
                    .with_attribute("value", "0.0")
                    .with_attribute("diff_role", "adjoint")
                    .with_attribute("diff_primitive", "adjoint_init"),
            );
            tangent_map.insert(operand, zero_id);
        }
    }

    let body_ops = match prim {
        // y = a + b  ⇒  d_a += d_y ; d_b += d_y
        Primitive::FAdd => emit_bwd_additive(op, &result_ty, d_y, tangent_map, next_id, false),
        // y = a - b  ⇒  d_a += d_y ; d_b -= d_y
        Primitive::FSub => emit_bwd_additive(op, &result_ty, d_y, tangent_map, next_id, true),
        // y = a * b  ⇒  d_a += d_y * b ; d_b += d_y * a
        Primitive::FMul => emit_bwd_multiplicative(op, &result_ty, d_y, tangent_map, next_id),
        // y = a / b  ⇒  d_a += d_y / b ; d_b -= d_y * a / (b*b)
        Primitive::FDiv => emit_bwd_div(op, &result_ty, d_y, tangent_map, next_id),
        // y = -a  ⇒  d_a -= d_y   (equivalently : d_a += -d_y)
        Primitive::FNeg => emit_bwd_neg(op, &result_ty, d_y, tangent_map, next_id),
        // y = √a  ⇒  d_a += d_y / (2 * y)
        Primitive::Sqrt => {
            emit_bwd_sqrt(op, primal_result_id, &result_ty, d_y, tangent_map, next_id)
        }
        // y = sin(a)  ⇒  d_a += d_y * cos(a)
        Primitive::Sin => emit_bwd_sin(op, &result_ty, d_y, tangent_map, next_id),
        // y = cos(a)  ⇒  d_a -= d_y * sin(a)
        Primitive::Cos => emit_bwd_cos(op, &result_ty, d_y, tangent_map, next_id),
        // y = exp(a)  ⇒  d_a += d_y * y
        Primitive::Exp => emit_bwd_exp(op, primal_result_id, &result_ty, d_y, tangent_map, next_id),
        // y = log(a)  ⇒  d_a += d_y / a
        Primitive::Log => emit_bwd_log(op, &result_ty, d_y, tangent_map, next_id),
        // T11-D15 : real branchful Bwd emission for piecewise-linear primitives.
        Primitive::Min => emit_bwd_min(op, &result_ty, d_y, tangent_map, next_id),
        Primitive::Max => emit_bwd_max(op, &result_ty, d_y, tangent_map, next_id),
        Primitive::Abs => emit_bwd_abs(op, &result_ty, d_y, tangent_map, next_id),
        Primitive::Sign => emit_bwd_sign(op, tangent_map, next_id),
        // Control / call / memory : stage-0 placeholder + recipe.
        Primitive::Call | Primitive::Load | Primitive::Store | Primitive::If | Primitive::Loop => {
            vec![MirOp::std("cssl.diff.bwd_placeholder")
                .with_operand(d_y)
                .with_attribute("primitive", prim.name())
                .with_attribute("diff_role", "adjoint")]
        }
    };

    // Prepend the init-ops so the adjoint-body ops see fresh zero-adjoints.
    let mut combined = init_ops;
    combined.extend(body_ops);
    combined
}

// ─── FAdd / FSub bwd ──────────────────────────────────────────────────────
// y = a ± b  ⇒  d_a += d_y ; d_b (±=) d_y
// `sub = true`  ⇒ d_b -= d_y    `sub = false` ⇒ d_b += d_y
fn emit_bwd_additive(
    op: &MirOp,
    result_ty: &MirType,
    d_y: ValueId,
    tangent_map: &mut TangentMap,
    next_id: &mut u32,
    sub: bool,
) -> Vec<MirOp> {
    let (Some(&a), Some(&b)) = (op.operands.first(), op.operands.get(1)) else {
        return Vec::new();
    };
    // § Self-reference safety : read `prev_d_b` AFTER the a-update has been
    // recorded in `tangent_map`. When `a == b` (e.g., `x + x`), the b-step
    // correctly accumulates on top of the just-updated a-adjoint.
    let prev_d_a = tangent_or_zero(tangent_map, a);
    let new_d_a = fresh_id(next_id);
    tangent_map.insert(a, new_d_a);
    let prev_d_b = tangent_or_zero(tangent_map, b);
    let new_d_b = fresh_id(next_id);
    tangent_map.insert(b, new_d_b);
    let b_op_name = if sub { "arith.subf" } else { "arith.addf" };
    vec![
        MirOp::std("arith.addf")
            .with_operand(prev_d_a)
            .with_operand(d_y)
            .with_result(new_d_a, result_ty.clone())
            .with_attribute("diff_role", "adjoint")
            .with_attribute("diff_primitive", if sub { "fsub" } else { "fadd" }),
        MirOp::std(b_op_name)
            .with_operand(prev_d_b)
            .with_operand(d_y)
            .with_result(new_d_b, result_ty.clone())
            .with_attribute("diff_role", "adjoint")
            .with_attribute("diff_primitive", if sub { "fsub" } else { "fadd" }),
    ]
}

// ─── FMul bwd ─────────────────────────────────────────────────────────────
fn emit_bwd_multiplicative(
    op: &MirOp,
    result_ty: &MirType,
    d_y: ValueId,
    tangent_map: &mut TangentMap,
    next_id: &mut u32,
) -> Vec<MirOp> {
    let (Some(&a), Some(&b)) = (op.operands.first(), op.operands.get(1)) else {
        return Vec::new();
    };
    // § Self-reference safety : do a's update before reading b's current
    // adjoint. For `a*a` this gives `d_a += d_y*a` then `d_a += d_y*a` again
    // (correct 2·d_y·a accumulation).
    let prev_d_a = tangent_or_zero(tangent_map, a);
    let contrib_a = fresh_id(next_id);
    let new_d_a = fresh_id(next_id);
    tangent_map.insert(a, new_d_a);
    let prev_d_b = tangent_or_zero(tangent_map, b);
    let contrib_b = fresh_id(next_id);
    let new_d_b = fresh_id(next_id);
    tangent_map.insert(b, new_d_b);
    vec![
        MirOp::std("arith.mulf")
            .with_operand(d_y)
            .with_operand(b)
            .with_result(contrib_a, result_ty.clone())
            .with_attribute("diff_role", "adjoint")
            .with_attribute("diff_primitive", "fmul"),
        MirOp::std("arith.addf")
            .with_operand(prev_d_a)
            .with_operand(contrib_a)
            .with_result(new_d_a, result_ty.clone())
            .with_attribute("diff_role", "adjoint")
            .with_attribute("diff_primitive", "fmul"),
        MirOp::std("arith.mulf")
            .with_operand(d_y)
            .with_operand(a)
            .with_result(contrib_b, result_ty.clone())
            .with_attribute("diff_role", "adjoint")
            .with_attribute("diff_primitive", "fmul"),
        MirOp::std("arith.addf")
            .with_operand(prev_d_b)
            .with_operand(contrib_b)
            .with_result(new_d_b, result_ty.clone())
            .with_attribute("diff_role", "adjoint")
            .with_attribute("diff_primitive", "fmul"),
    ]
}

// ─── FDiv bwd ─────────────────────────────────────────────────────────────
// y = a / b  ⇒  d_a += d_y / b ; d_b -= d_y * a / (b*b)
fn emit_bwd_div(
    op: &MirOp,
    result_ty: &MirType,
    d_y: ValueId,
    tangent_map: &mut TangentMap,
    next_id: &mut u32,
) -> Vec<MirOp> {
    let (Some(&a), Some(&b)) = (op.operands.first(), op.operands.get(1)) else {
        return Vec::new();
    };
    // § Self-reference safety : serialize the a-update before reading b's.
    let prev_d_a = tangent_or_zero(tangent_map, a);
    let ca = fresh_id(next_id); // d_y / b
    let new_d_a = fresh_id(next_id);
    tangent_map.insert(a, new_d_a);
    let prev_d_b = tangent_or_zero(tangent_map, b);
    let cb_num = fresh_id(next_id); // d_y * a
    let cb_den = fresh_id(next_id); // b * b
    let cb = fresh_id(next_id); // cb_num / cb_den
    let new_d_b = fresh_id(next_id);
    tangent_map.insert(b, new_d_b);
    vec![
        MirOp::std("arith.divf")
            .with_operand(d_y)
            .with_operand(b)
            .with_result(ca, result_ty.clone())
            .with_attribute("diff_role", "adjoint")
            .with_attribute("diff_primitive", "fdiv"),
        MirOp::std("arith.addf")
            .with_operand(prev_d_a)
            .with_operand(ca)
            .with_result(new_d_a, result_ty.clone())
            .with_attribute("diff_role", "adjoint")
            .with_attribute("diff_primitive", "fdiv"),
        MirOp::std("arith.mulf")
            .with_operand(d_y)
            .with_operand(a)
            .with_result(cb_num, result_ty.clone())
            .with_attribute("diff_role", "adjoint")
            .with_attribute("diff_primitive", "fdiv"),
        MirOp::std("arith.mulf")
            .with_operand(b)
            .with_operand(b)
            .with_result(cb_den, result_ty.clone())
            .with_attribute("diff_role", "adjoint")
            .with_attribute("diff_primitive", "fdiv"),
        MirOp::std("arith.divf")
            .with_operand(cb_num)
            .with_operand(cb_den)
            .with_result(cb, result_ty.clone())
            .with_attribute("diff_role", "adjoint")
            .with_attribute("diff_primitive", "fdiv"),
        MirOp::std("arith.subf")
            .with_operand(prev_d_b)
            .with_operand(cb)
            .with_result(new_d_b, result_ty.clone())
            .with_attribute("diff_role", "adjoint")
            .with_attribute("diff_primitive", "fdiv"),
    ]
}

// ─── FNeg bwd ─────────────────────────────────────────────────────────────
fn emit_bwd_neg(
    op: &MirOp,
    result_ty: &MirType,
    d_y: ValueId,
    tangent_map: &mut TangentMap,
    next_id: &mut u32,
) -> Vec<MirOp> {
    let Some(&a) = op.operands.first() else {
        return Vec::new();
    };
    let prev_d_a = tangent_or_zero(tangent_map, a);
    let new_d_a = fresh_id(next_id);
    tangent_map.insert(a, new_d_a);
    vec![MirOp::std("arith.subf")
        .with_operand(prev_d_a)
        .with_operand(d_y)
        .with_result(new_d_a, result_ty.clone())
        .with_attribute("diff_role", "adjoint")
        .with_attribute("diff_primitive", "fneg")]
}

// ─── Sqrt bwd :  d_a += d_y / (2 * y) ─────────────────────────────────────
fn emit_bwd_sqrt(
    op: &MirOp,
    primal_result: ValueId,
    result_ty: &MirType,
    d_y: ValueId,
    tangent_map: &mut TangentMap,
    next_id: &mut u32,
) -> Vec<MirOp> {
    let Some(&a) = op.operands.first() else {
        return Vec::new();
    };
    let prev_d_a = tangent_or_zero(tangent_map, a);
    let two = fresh_id(next_id);
    let two_y = fresh_id(next_id);
    let contrib = fresh_id(next_id);
    let new_d_a = fresh_id(next_id);
    tangent_map.insert(a, new_d_a);
    vec![
        MirOp::std("arith.constant")
            .with_result(two, result_ty.clone())
            .with_attribute("value", "2.0")
            .with_attribute("diff_role", "adjoint")
            .with_attribute("diff_primitive", "sqrt"),
        MirOp::std("arith.mulf")
            .with_operand(two)
            .with_operand(primal_result)
            .with_result(two_y, result_ty.clone())
            .with_attribute("diff_role", "adjoint")
            .with_attribute("diff_primitive", "sqrt"),
        MirOp::std("arith.divf")
            .with_operand(d_y)
            .with_operand(two_y)
            .with_result(contrib, result_ty.clone())
            .with_attribute("diff_role", "adjoint")
            .with_attribute("diff_primitive", "sqrt"),
        MirOp::std("arith.addf")
            .with_operand(prev_d_a)
            .with_operand(contrib)
            .with_result(new_d_a, result_ty.clone())
            .with_attribute("diff_role", "adjoint")
            .with_attribute("diff_primitive", "sqrt"),
    ]
}

// ─── Sin bwd :  d_a += d_y * cos(a) ───────────────────────────────────────
fn emit_bwd_sin(
    op: &MirOp,
    result_ty: &MirType,
    d_y: ValueId,
    tangent_map: &mut TangentMap,
    next_id: &mut u32,
) -> Vec<MirOp> {
    let Some(&a) = op.operands.first() else {
        return Vec::new();
    };
    let prev_d_a = tangent_or_zero(tangent_map, a);
    let cos_a = fresh_id(next_id);
    let contrib = fresh_id(next_id);
    let new_d_a = fresh_id(next_id);
    tangent_map.insert(a, new_d_a);
    vec![
        MirOp::std("func.call")
            .with_operand(a)
            .with_result(cos_a, result_ty.clone())
            .with_attribute("callee", "cos")
            .with_attribute("diff_role", "adjoint")
            .with_attribute("diff_primitive", "sin"),
        MirOp::std("arith.mulf")
            .with_operand(d_y)
            .with_operand(cos_a)
            .with_result(contrib, result_ty.clone())
            .with_attribute("diff_role", "adjoint")
            .with_attribute("diff_primitive", "sin"),
        MirOp::std("arith.addf")
            .with_operand(prev_d_a)
            .with_operand(contrib)
            .with_result(new_d_a, result_ty.clone())
            .with_attribute("diff_role", "adjoint")
            .with_attribute("diff_primitive", "sin"),
    ]
}

// ─── Cos bwd :  d_a -= d_y * sin(a) ───────────────────────────────────────
fn emit_bwd_cos(
    op: &MirOp,
    result_ty: &MirType,
    d_y: ValueId,
    tangent_map: &mut TangentMap,
    next_id: &mut u32,
) -> Vec<MirOp> {
    let Some(&a) = op.operands.first() else {
        return Vec::new();
    };
    let prev_d_a = tangent_or_zero(tangent_map, a);
    let sin_a = fresh_id(next_id);
    let contrib = fresh_id(next_id);
    let new_d_a = fresh_id(next_id);
    tangent_map.insert(a, new_d_a);
    vec![
        MirOp::std("func.call")
            .with_operand(a)
            .with_result(sin_a, result_ty.clone())
            .with_attribute("callee", "sin")
            .with_attribute("diff_role", "adjoint")
            .with_attribute("diff_primitive", "cos"),
        MirOp::std("arith.mulf")
            .with_operand(d_y)
            .with_operand(sin_a)
            .with_result(contrib, result_ty.clone())
            .with_attribute("diff_role", "adjoint")
            .with_attribute("diff_primitive", "cos"),
        MirOp::std("arith.subf")
            .with_operand(prev_d_a)
            .with_operand(contrib)
            .with_result(new_d_a, result_ty.clone())
            .with_attribute("diff_role", "adjoint")
            .with_attribute("diff_primitive", "cos"),
    ]
}

// ─── Exp bwd :  d_a += d_y * y ────────────────────────────────────────────
fn emit_bwd_exp(
    op: &MirOp,
    primal_result: ValueId,
    result_ty: &MirType,
    d_y: ValueId,
    tangent_map: &mut TangentMap,
    next_id: &mut u32,
) -> Vec<MirOp> {
    let Some(&a) = op.operands.first() else {
        return Vec::new();
    };
    let prev_d_a = tangent_or_zero(tangent_map, a);
    let contrib = fresh_id(next_id);
    let new_d_a = fresh_id(next_id);
    tangent_map.insert(a, new_d_a);
    vec![
        MirOp::std("arith.mulf")
            .with_operand(d_y)
            .with_operand(primal_result)
            .with_result(contrib, result_ty.clone())
            .with_attribute("diff_role", "adjoint")
            .with_attribute("diff_primitive", "exp"),
        MirOp::std("arith.addf")
            .with_operand(prev_d_a)
            .with_operand(contrib)
            .with_result(new_d_a, result_ty.clone())
            .with_attribute("diff_role", "adjoint")
            .with_attribute("diff_primitive", "exp"),
    ]
}

// ─── Min bwd :  d_a += select(a ≤ b, d_y, 0) ; d_b += select(a ≤ b, 0, d_y) ─
fn emit_bwd_min(
    op: &MirOp,
    result_ty: &MirType,
    d_y: ValueId,
    tangent_map: &mut TangentMap,
    next_id: &mut u32,
) -> Vec<MirOp> {
    emit_bwd_piecewise_binary(op, result_ty, d_y, tangent_map, next_id, "ole", "min")
}

// ─── Max bwd :  d_a += select(a ≥ b, d_y, 0) ; d_b += select(a ≥ b, 0, d_y) ─
fn emit_bwd_max(
    op: &MirOp,
    result_ty: &MirType,
    d_y: ValueId,
    tangent_map: &mut TangentMap,
    next_id: &mut u32,
) -> Vec<MirOp> {
    emit_bwd_piecewise_binary(op, result_ty, d_y, tangent_map, next_id, "oge", "max")
}

/// Shared `min`/`max` Bwd emitter : routes the incoming adjoint `d_y` to
/// whichever branch wins under the comparison `predicate`.
fn emit_bwd_piecewise_binary(
    op: &MirOp,
    result_ty: &MirType,
    d_y: ValueId,
    tangent_map: &mut TangentMap,
    next_id: &mut u32,
    predicate: &'static str,
    prim_name: &'static str,
) -> Vec<MirOp> {
    let (Some(&a), Some(&b)) = (op.operands.first(), op.operands.get(1)) else {
        return Vec::new();
    };
    let prev_d_a = tangent_or_zero(tangent_map, a);
    let new_d_a = fresh_id(next_id);
    tangent_map.insert(a, new_d_a);
    let prev_d_b = tangent_or_zero(tangent_map, b);
    let new_d_b = fresh_id(next_id);
    tangent_map.insert(b, new_d_b);

    let zero_id = fresh_id(next_id);
    let cmp_id = fresh_id(next_id);
    let contrib_a = fresh_id(next_id);
    let contrib_b = fresh_id(next_id);
    vec![
        MirOp::std("arith.constant")
            .with_result(zero_id, result_ty.clone())
            .with_attribute("value", "0.0")
            .with_attribute("diff_role", "adjoint")
            .with_attribute("diff_primitive", prim_name),
        MirOp::std("arith.cmpf")
            .with_operand(a)
            .with_operand(b)
            .with_result(cmp_id, MirType::Bool)
            .with_attribute("predicate", predicate)
            .with_attribute("diff_role", "adjoint")
            .with_attribute("diff_primitive", prim_name),
        MirOp::std("arith.select")
            .with_operand(cmp_id)
            .with_operand(d_y)
            .with_operand(zero_id)
            .with_result(contrib_a, result_ty.clone())
            .with_attribute("diff_role", "adjoint")
            .with_attribute("diff_primitive", prim_name),
        MirOp::std("arith.addf")
            .with_operand(prev_d_a)
            .with_operand(contrib_a)
            .with_result(new_d_a, result_ty.clone())
            .with_attribute("diff_role", "adjoint")
            .with_attribute("diff_primitive", prim_name),
        MirOp::std("arith.select")
            .with_operand(cmp_id)
            .with_operand(zero_id)
            .with_operand(d_y)
            .with_result(contrib_b, result_ty.clone())
            .with_attribute("diff_role", "adjoint")
            .with_attribute("diff_primitive", prim_name),
        MirOp::std("arith.addf")
            .with_operand(prev_d_b)
            .with_operand(contrib_b)
            .with_result(new_d_b, result_ty.clone())
            .with_attribute("diff_role", "adjoint")
            .with_attribute("diff_primitive", prim_name),
    ]
}

// ─── Abs bwd :  d_x += select(x ≥ 0, d_y, -d_y) ──────────────────────────
fn emit_bwd_abs(
    op: &MirOp,
    result_ty: &MirType,
    d_y: ValueId,
    tangent_map: &mut TangentMap,
    next_id: &mut u32,
) -> Vec<MirOp> {
    let Some(&x) = op.operands.first() else {
        return Vec::new();
    };
    let prev_d_x = tangent_or_zero(tangent_map, x);
    let new_d_x = fresh_id(next_id);
    tangent_map.insert(x, new_d_x);

    let zero_id = fresh_id(next_id);
    let cmp_id = fresh_id(next_id);
    let neg_d_y = fresh_id(next_id);
    let contrib = fresh_id(next_id);
    vec![
        MirOp::std("arith.constant")
            .with_result(zero_id, result_ty.clone())
            .with_attribute("value", "0.0")
            .with_attribute("diff_role", "adjoint")
            .with_attribute("diff_primitive", "abs"),
        MirOp::std("arith.cmpf")
            .with_operand(x)
            .with_operand(zero_id)
            .with_result(cmp_id, MirType::Bool)
            .with_attribute("predicate", "oge")
            .with_attribute("diff_role", "adjoint")
            .with_attribute("diff_primitive", "abs"),
        MirOp::std("arith.negf")
            .with_operand(d_y)
            .with_result(neg_d_y, result_ty.clone())
            .with_attribute("diff_role", "adjoint")
            .with_attribute("diff_primitive", "abs"),
        MirOp::std("arith.select")
            .with_operand(cmp_id)
            .with_operand(d_y)
            .with_operand(neg_d_y)
            .with_result(contrib, result_ty.clone())
            .with_attribute("diff_role", "adjoint")
            .with_attribute("diff_primitive", "abs"),
        MirOp::std("arith.addf")
            .with_operand(prev_d_x)
            .with_operand(contrib)
            .with_result(new_d_x, result_ty.clone())
            .with_attribute("diff_role", "adjoint")
            .with_attribute("diff_primitive", "abs"),
    ]
}

// ─── Sign bwd : d_x += 0 (no-op — sign has zero gradient a.e.) ────────────
fn emit_bwd_sign(op: &MirOp, tangent_map: &TangentMap, _next_id: &mut u32) -> Vec<MirOp> {
    let Some(&x) = op.operands.first() else {
        return Vec::new();
    };
    // No-op : sign(x) derivative is 0 a.e. ; we preserve the existing adjoint
    // without emitting a zero-contrib chain to keep the body compact.
    let _ = tangent_map.get(x);
    Vec::new()
}

// ─── Log bwd :  d_a += d_y / a ────────────────────────────────────────────
fn emit_bwd_log(
    op: &MirOp,
    result_ty: &MirType,
    d_y: ValueId,
    tangent_map: &mut TangentMap,
    next_id: &mut u32,
) -> Vec<MirOp> {
    let Some(&a) = op.operands.first() else {
        return Vec::new();
    };
    let prev_d_a = tangent_or_zero(tangent_map, a);
    let contrib = fresh_id(next_id);
    let new_d_a = fresh_id(next_id);
    tangent_map.insert(a, new_d_a);
    vec![
        MirOp::std("arith.divf")
            .with_operand(d_y)
            .with_operand(a)
            .with_result(contrib, result_ty.clone())
            .with_attribute("diff_role", "adjoint")
            .with_attribute("diff_primitive", "log"),
        MirOp::std("arith.addf")
            .with_operand(prev_d_a)
            .with_operand(contrib)
            .with_result(new_d_a, result_ty.clone())
            .with_attribute("diff_role", "adjoint")
            .with_attribute("diff_primitive", "log"),
    ]
}

// ─────────────────────────────────────────────────────────────────────────
// § Helpers
// ─────────────────────────────────────────────────────────────────────────

/// Allocate a fresh SSA value-id + advance `next_id`.
fn fresh_id(next_id: &mut u32) -> ValueId {
    let id = ValueId(*next_id);
    *next_id = next_id.saturating_add(1);
    id
}

/// Classify a MIR op as a recognized AD primitive (if any).
fn recognize_primitive(op: &MirOp) -> Option<Primitive> {
    let prim = op_to_primitive(&op.name)?;
    if prim == Primitive::Call {
        let callee = op
            .attributes
            .iter()
            .find(|(k, _)| k == "callee")
            .map(|(_, v)| v.as_str());
        Some(specialize_transcendental(prim, callee))
    } else {
        Some(prim)
    }
}

/// Look up the tangent / adjoint for `v`, or return `v` itself as a zero-ish
/// fallback when no entry exists. The stage-0 interpretation : an unknown
/// primal value has an implicit zero tangent ; since we don't emit `arith.constant 0`
/// for every gap, we reuse the primal value-id and rely on the attribute-tagged
/// `diff_role` classification to disambiguate in downstream consumers.
fn tangent_or_zero(map: &TangentMap, v: ValueId) -> ValueId {
    map.get(v).unwrap_or(v)
}

/// `true` iff the type is a float type (directly or via refinement / reference).
fn is_float(t: &MirType) -> bool {
    matches!(t, MirType::Float(_))
}

/// Get the tangent-type corresponding to a primal type. At stage-0 we use the
/// same type as the primal (scalar only ; jets come later in phase-2c).
fn tangent_type_of(t: &MirType) -> MirType {
    match t {
        MirType::Float(w) => MirType::Float(*w),
        MirType::Int(_) | MirType::Bool => MirType::Float(FloatWidth::F32),
        other => other.clone(),
    }
}

/// Default tangent type when the primal op has no result type annotation.
fn default_tangent_ty() -> MirType {
    MirType::Float(FloatWidth::F32)
}

/// Short name for a [`DiffMode`], used in variant attributes.
const fn mode_str(mode: DiffMode) -> &'static str {
    match mode {
        DiffMode::Primal => "primal",
        DiffMode::Fwd => "fwd",
        DiffMode::Bwd => "bwd",
    }
}

#[cfg(test)]
mod tests {
    use super::{apply_bwd, apply_fwd, SubstitutionReport, TangentMap};
    use crate::rules::DiffRuleTable;
    use cssl_mir::{
        FloatWidth, MirBlock, MirFunc, MirModule, MirOp, MirRegion, MirType, MirValue, ValueId,
    };

    /// Build a primal fn with the given body-ops + signature for testing.
    fn mk_primal(
        name: &str,
        param_types: Vec<MirType>,
        result_types: Vec<MirType>,
        ops: Vec<MirOp>,
    ) -> MirFunc {
        let mut f = MirFunc::new(name, param_types, result_types);
        for op in ops {
            f.push_op(op);
        }
        f
    }

    fn f32_ty() -> MirType {
        MirType::Float(FloatWidth::F32)
    }

    #[test]
    fn tangent_map_insert_and_get() {
        let mut m = TangentMap::new();
        assert!(m.is_empty());
        m.insert(ValueId(0), ValueId(100));
        assert_eq!(m.get(ValueId(0)), Some(ValueId(100)));
        assert_eq!(m.get(ValueId(1)), None);
        assert_eq!(m.len(), 1);
        assert!(!m.is_empty());
    }

    #[test]
    fn report_summary_mentions_counts() {
        let r = SubstitutionReport {
            primitives_substituted: 3,
            tangent_ops_emitted: 7,
            unsupported_primitives: 0,
            tangent_params_added: 2,
            tangent_results_added: 1,
        };
        let s = r.summary();
        assert!(s.contains("3 primitives"));
        assert!(s.contains("7 tangent-ops"));
        assert!(s.contains("2 tangent-params"));
        assert!(s.contains("1 tangent-results"));
    }

    // ─────────────────────────────────────────────────────────────────────
    // § FWD-mode per-primitive tests
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn fwd_fadd_emits_tangent_addf() {
        // y = a + b, where a=%0, b=%1, y=%2 + func.return %2
        let ops = vec![
            MirOp::std("arith.addf")
                .with_operand(ValueId(0))
                .with_operand(ValueId(1))
                .with_result(ValueId(2), f32_ty()),
            MirOp::std("func.return").with_operand(ValueId(2)),
        ];
        let primal = mk_primal("add", vec![f32_ty(), f32_ty()], vec![f32_ty()], ops);
        let (variant, tmap, report) = apply_fwd(&primal, &DiffRuleTable::canonical());
        assert_eq!(variant.name, "add_fwd");
        assert_eq!(report.primitives_substituted, 1);
        assert!(report.tangent_ops_emitted >= 1);
        // Params extended : [a, d_a, b, d_b]
        assert_eq!(variant.params.len(), 4);
        // Results extended : [y, d_y]
        assert_eq!(variant.results.len(), 2);
        // Find the tangent addf : it should carry the diff_role=tangent attr.
        let entry = variant.body.entry().unwrap();
        let tangent_addf = entry
            .ops
            .iter()
            .find(|o| {
                o.name == "arith.addf"
                    && o.attributes
                        .iter()
                        .any(|(k, v)| k == "diff_role" && v == "tangent")
            })
            .expect("expected tangent arith.addf");
        // Tangent addf operands should be d_a + d_b (from TangentMap).
        let d_a = tmap.get(ValueId(0)).unwrap();
        let d_b = tmap.get(ValueId(1)).unwrap();
        assert_eq!(tangent_addf.operands, vec![d_a, d_b]);
    }

    #[test]
    fn fwd_fsub_emits_tangent_subf() {
        let ops = vec![
            MirOp::std("arith.subf")
                .with_operand(ValueId(0))
                .with_operand(ValueId(1))
                .with_result(ValueId(2), f32_ty()),
            MirOp::std("func.return").with_operand(ValueId(2)),
        ];
        let primal = mk_primal("sub", vec![f32_ty(), f32_ty()], vec![f32_ty()], ops);
        let (variant, _, report) = apply_fwd(&primal, &DiffRuleTable::canonical());
        assert_eq!(report.primitives_substituted, 1);
        let entry = variant.body.entry().unwrap();
        let tangent_subf = entry.ops.iter().find(|o| {
            o.name == "arith.subf"
                && o.attributes
                    .iter()
                    .any(|(k, v)| k == "diff_role" && v == "tangent")
        });
        assert!(tangent_subf.is_some(), "expected tangent arith.subf");
    }

    #[test]
    fn fwd_fmul_emits_two_muls_plus_add() {
        let ops = vec![
            MirOp::std("arith.mulf")
                .with_operand(ValueId(0))
                .with_operand(ValueId(1))
                .with_result(ValueId(2), f32_ty()),
            MirOp::std("func.return").with_operand(ValueId(2)),
        ];
        let primal = mk_primal("mul", vec![f32_ty(), f32_ty()], vec![f32_ty()], ops);
        let (variant, _, report) = apply_fwd(&primal, &DiffRuleTable::canonical());
        assert_eq!(report.primitives_substituted, 1);
        let entry = variant.body.entry().unwrap();
        let tangent_muls = entry
            .ops
            .iter()
            .filter(|o| {
                o.name == "arith.mulf"
                    && o.attributes
                        .iter()
                        .any(|(k, v)| k == "diff_role" && v == "tangent")
            })
            .count();
        assert_eq!(tangent_muls, 2, "expected 2 tangent mulfs");
        let tangent_adds = entry
            .ops
            .iter()
            .filter(|o| {
                o.name == "arith.addf"
                    && o.attributes
                        .iter()
                        .any(|(k, v)| k == "diff_role" && v == "tangent")
            })
            .count();
        assert_eq!(tangent_adds, 1, "expected 1 tangent addf");
    }

    #[test]
    fn fwd_fdiv_emits_full_chain() {
        let ops = vec![
            MirOp::std("arith.divf")
                .with_operand(ValueId(0))
                .with_operand(ValueId(1))
                .with_result(ValueId(2), f32_ty()),
            MirOp::std("func.return").with_operand(ValueId(2)),
        ];
        let primal = mk_primal("div", vec![f32_ty(), f32_ty()], vec![f32_ty()], ops);
        let (variant, _, _) = apply_fwd(&primal, &DiffRuleTable::canonical());
        let entry = variant.body.entry().unwrap();
        // FDiv fwd emits : 2 mulfs + 1 subf + 1 mulf + 1 divf = 5 tangent ops.
        let tangent_ops = entry
            .ops
            .iter()
            .filter(|o| {
                o.attributes
                    .iter()
                    .any(|(k, v)| k == "diff_role" && v == "tangent")
            })
            .count();
        assert!(
            tangent_ops >= 5,
            "expected 5+ tangent ops for fdiv, got {tangent_ops}"
        );
    }

    #[test]
    fn fwd_fneg_emits_tangent_negf() {
        let ops = vec![
            MirOp::std("arith.negf")
                .with_operand(ValueId(0))
                .with_result(ValueId(1), f32_ty()),
            MirOp::std("func.return").with_operand(ValueId(1)),
        ];
        let primal = mk_primal("neg", vec![f32_ty()], vec![f32_ty()], ops);
        let (variant, _, report) = apply_fwd(&primal, &DiffRuleTable::canonical());
        assert_eq!(report.primitives_substituted, 1);
        let entry = variant.body.entry().unwrap();
        let tangent_neg = entry.ops.iter().find(|o| {
            o.name == "arith.negf"
                && o.attributes
                    .iter()
                    .any(|(k, v)| k == "diff_role" && v == "tangent")
        });
        assert!(tangent_neg.is_some(), "expected tangent arith.negf");
    }

    #[test]
    fn fwd_sqrt_emits_constant_mul_div_chain() {
        // Build a func.call with callee=sqrt.
        let ops = vec![
            MirOp::std("func.call")
                .with_operand(ValueId(0))
                .with_result(ValueId(1), f32_ty())
                .with_attribute("callee", "sqrt"),
            MirOp::std("func.return").with_operand(ValueId(1)),
        ];
        let primal = mk_primal("s", vec![f32_ty()], vec![f32_ty()], ops);
        let (variant, _, report) = apply_fwd(&primal, &DiffRuleTable::canonical());
        assert_eq!(report.primitives_substituted, 1);
        let entry = variant.body.entry().unwrap();
        let tangent_const = entry.ops.iter().any(|o| {
            o.name == "arith.constant"
                && o.attributes
                    .iter()
                    .any(|(k, v)| k == "diff_primitive" && v == "sqrt")
        });
        assert!(
            tangent_const,
            "expected tangent arith.constant 2.0 for sqrt"
        );
        let tangent_mul = entry.ops.iter().any(|o| {
            o.name == "arith.mulf"
                && o.attributes
                    .iter()
                    .any(|(k, v)| k == "diff_primitive" && v == "sqrt")
        });
        let tangent_div = entry.ops.iter().any(|o| {
            o.name == "arith.divf"
                && o.attributes
                    .iter()
                    .any(|(k, v)| k == "diff_primitive" && v == "sqrt")
        });
        assert!(tangent_mul && tangent_div);
    }

    #[test]
    fn fwd_sin_emits_cos_call_and_mul() {
        let ops = vec![
            MirOp::std("func.call")
                .with_operand(ValueId(0))
                .with_result(ValueId(1), f32_ty())
                .with_attribute("callee", "sin"),
            MirOp::std("func.return").with_operand(ValueId(1)),
        ];
        let primal = mk_primal("s", vec![f32_ty()], vec![f32_ty()], ops);
        let (variant, _, report) = apply_fwd(&primal, &DiffRuleTable::canonical());
        assert_eq!(report.primitives_substituted, 1);
        let entry = variant.body.entry().unwrap();
        let tangent_cos = entry.ops.iter().find(|o| {
            o.name == "func.call"
                && o.attributes
                    .iter()
                    .any(|(k, v)| k == "callee" && v == "cos")
                && o.attributes
                    .iter()
                    .any(|(k, v)| k == "diff_role" && v == "tangent")
        });
        assert!(tangent_cos.is_some(), "expected cos() call for sin-tangent");
    }

    #[test]
    fn fwd_exp_reuses_primal_result() {
        let ops = vec![
            MirOp::std("func.call")
                .with_operand(ValueId(0))
                .with_result(ValueId(1), f32_ty())
                .with_attribute("callee", "exp"),
            MirOp::std("func.return").with_operand(ValueId(1)),
        ];
        let primal = mk_primal("e", vec![f32_ty()], vec![f32_ty()], ops);
        let (variant, _, _) = apply_fwd(&primal, &DiffRuleTable::canonical());
        let entry = variant.body.entry().unwrap();
        let tangent_mul = entry.ops.iter().find(|o| {
            o.name == "arith.mulf"
                && o.attributes
                    .iter()
                    .any(|(k, v)| k == "diff_primitive" && v == "exp")
        });
        assert!(tangent_mul.is_some(), "expected tangent mulf for exp");
        // The second operand of the tangent-mul is the primal-result (y itself).
        let t = tangent_mul.unwrap();
        assert_eq!(t.operands[1], ValueId(1), "exp tangent reuses primal y=%1");
    }

    // ─────────────────────────────────────────────────────────────────────
    // § BWD-mode tests
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn bwd_fadd_emits_adjoint_accumulation() {
        let ops = vec![
            MirOp::std("arith.addf")
                .with_operand(ValueId(0))
                .with_operand(ValueId(1))
                .with_result(ValueId(2), f32_ty()),
            MirOp::std("func.return").with_operand(ValueId(2)),
        ];
        let primal = mk_primal("add", vec![f32_ty(), f32_ty()], vec![f32_ty()], ops);
        let (variant, _, report) = apply_bwd(&primal, &DiffRuleTable::canonical());
        assert_eq!(variant.name, "add_bwd");
        assert_eq!(report.primitives_substituted, 1);
        let entry = variant.body.entry().unwrap();
        let adjoint_ops = entry
            .ops
            .iter()
            .filter(|o| {
                o.attributes
                    .iter()
                    .any(|(k, v)| k == "diff_role" && v == "adjoint")
            })
            .count();
        assert!(adjoint_ops >= 2, "expected 2+ adjoint ops for FAdd bwd");
        // bwd signature : adjoint-out per primal param.
        assert_eq!(variant.results.len(), 2); // d_a + d_b
    }

    #[test]
    fn bwd_fmul_emits_contribution_and_accumulate() {
        let ops = vec![
            MirOp::std("arith.mulf")
                .with_operand(ValueId(0))
                .with_operand(ValueId(1))
                .with_result(ValueId(2), f32_ty()),
            MirOp::std("func.return").with_operand(ValueId(2)),
        ];
        let primal = mk_primal("mul", vec![f32_ty(), f32_ty()], vec![f32_ty()], ops);
        let (variant, _, _) = apply_bwd(&primal, &DiffRuleTable::canonical());
        let entry = variant.body.entry().unwrap();
        // FMul bwd emits 2 contrib muls + 2 accumulate adds = 4 adjoint ops.
        let adjoint_muls = entry
            .ops
            .iter()
            .filter(|o| {
                o.name == "arith.mulf"
                    && o.attributes
                        .iter()
                        .any(|(k, v)| k == "diff_role" && v == "adjoint")
            })
            .count();
        assert_eq!(adjoint_muls, 2);
    }

    #[test]
    fn bwd_ends_with_bwd_return() {
        let ops = vec![
            MirOp::std("arith.addf")
                .with_operand(ValueId(0))
                .with_operand(ValueId(1))
                .with_result(ValueId(2), f32_ty()),
            MirOp::std("func.return").with_operand(ValueId(2)),
        ];
        let primal = mk_primal("add", vec![f32_ty(), f32_ty()], vec![f32_ty()], ops);
        let (variant, _, _) = apply_bwd(&primal, &DiffRuleTable::canonical());
        let entry = variant.body.entry().unwrap();
        let last = entry.ops.last().unwrap();
        assert_eq!(last.name, "cssl.diff.bwd_return");
        assert_eq!(last.operands.len(), 2, "should return d_a + d_b");
    }

    // ─────────────────────────────────────────────────────────────────────
    // § Structural tests
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn fwd_preserves_primal_ops() {
        let ops = vec![
            MirOp::std("arith.addf")
                .with_operand(ValueId(0))
                .with_operand(ValueId(1))
                .with_result(ValueId(2), f32_ty()),
            MirOp::std("func.return").with_operand(ValueId(2)),
        ];
        let primal = mk_primal("add", vec![f32_ty(), f32_ty()], vec![f32_ty()], ops);
        let (variant, _, _) = apply_fwd(&primal, &DiffRuleTable::canonical());
        let entry = variant.body.entry().unwrap();
        // Primal addf should still be present (unchanged role).
        let primal_addf = entry.ops.iter().find(|o| {
            o.name == "arith.addf"
                && !o
                    .attributes
                    .iter()
                    .any(|(k, v)| k == "diff_role" && v == "tangent")
        });
        assert!(primal_addf.is_some());
    }

    #[test]
    fn fwd_on_non_primitive_ops_is_identity() {
        // func.return only — no primitives to differentiate.
        let ops = vec![MirOp::std("func.return").with_operand(ValueId(0))];
        let primal = mk_primal("id", vec![f32_ty()], vec![f32_ty()], ops);
        let (variant, _, report) = apply_fwd(&primal, &DiffRuleTable::canonical());
        assert_eq!(report.primitives_substituted, 0);
        assert_eq!(variant.name, "id_fwd");
    }

    #[test]
    fn sphere_sdf_shape_fwd_and_bwd() {
        // Mini-surrogate of sphere_sdf(p, r) = p - r
        let ops = vec![
            MirOp::std("arith.subf")
                .with_operand(ValueId(0))
                .with_operand(ValueId(1))
                .with_result(ValueId(2), f32_ty()),
            MirOp::std("func.return").with_operand(ValueId(2)),
        ];
        let primal = mk_primal("sphere_sdf", vec![f32_ty(), f32_ty()], vec![f32_ty()], ops);
        let (fwd, _, fwd_r) = apply_fwd(&primal, &DiffRuleTable::canonical());
        let (bwd, _, bwd_r) = apply_bwd(&primal, &DiffRuleTable::canonical());
        assert_eq!(fwd.name, "sphere_sdf_fwd");
        assert_eq!(bwd.name, "sphere_sdf_bwd");
        assert_eq!(fwd_r.primitives_substituted, 1);
        assert_eq!(bwd_r.primitives_substituted, 1);
        // Fwd has tangent arith.subf.
        let fwd_entry = fwd.body.entry().unwrap();
        let fwd_tangent = fwd_entry.ops.iter().any(|o| {
            o.name == "arith.subf"
                && o.attributes
                    .iter()
                    .any(|(k, v)| k == "diff_role" && v == "tangent")
        });
        assert!(fwd_tangent);
        // Bwd ends with bwd_return + has adjoint ops.
        let bwd_entry = bwd.body.entry().unwrap();
        assert_eq!(bwd_entry.ops.last().unwrap().name, "cssl.diff.bwd_return");
    }

    #[test]
    fn tangent_params_appear_in_signature() {
        let ops = vec![
            MirOp::std("arith.addf")
                .with_operand(ValueId(0))
                .with_operand(ValueId(1))
                .with_result(ValueId(2), f32_ty()),
            MirOp::std("func.return").with_operand(ValueId(2)),
        ];
        let primal = mk_primal("add", vec![f32_ty(), f32_ty()], vec![f32_ty()], ops);
        let (variant, _, report) = apply_fwd(&primal, &DiffRuleTable::canonical());
        assert_eq!(report.tangent_params_added, 2);
        assert_eq!(report.tangent_results_added, 1);
        // Variant entry args: [a, d_a, b, d_b]
        let entry = variant.body.entry().unwrap();
        assert_eq!(entry.args.len(), 4);
    }

    #[test]
    fn apply_fwd_on_empty_body_does_not_crash() {
        let primal = mk_primal("empty", vec![], vec![], vec![]);
        let (variant, _, report) = apply_fwd(&primal, &DiffRuleTable::canonical());
        assert_eq!(variant.name, "empty_fwd");
        assert_eq!(report.primitives_substituted, 0);
    }

    #[test]
    fn apply_bwd_on_empty_body_does_not_crash() {
        let primal = mk_primal("empty", vec![], vec![], vec![]);
        let (variant, _, report) = apply_bwd(&primal, &DiffRuleTable::canonical());
        assert_eq!(variant.name, "empty_bwd");
        assert_eq!(report.primitives_substituted, 0);
    }

    /// Silence the unused-import warning surfaced when the scaffolding-only
    /// helpers are used only in tests.
    #[test]
    fn types_roundtrip() {
        let _ = MirBlock::new("b");
        let _ = MirRegion::new();
        let _ = MirModule::new();
        let _ = MirValue::new(ValueId(0), f32_ty());
    }

    // ─────────────────────────────────────────────────────────────────────
    // § T11-D15 : branchful Min/Max/Abs/Sign Fwd + Bwd emission.
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn fwd_min_emits_cmpf_ole_plus_select() {
        let ops = vec![
            MirOp::std("arith.minimumf")
                .with_operand(ValueId(0))
                .with_operand(ValueId(1))
                .with_result(ValueId(2), f32_ty()),
            MirOp::std("func.return").with_operand(ValueId(2)),
        ];
        let primal = mk_primal("min_fn", vec![f32_ty(), f32_ty()], vec![f32_ty()], ops);
        let (variant, _, report) = apply_fwd(&primal, &DiffRuleTable::canonical());
        assert_eq!(report.primitives_substituted, 1);
        let entry = variant.body.entry().unwrap();
        // Must contain arith.cmpf with predicate="ole" + arith.select, both tangent-role.
        let has_cmpf_ole = entry.ops.iter().any(|o| {
            o.name == "arith.cmpf"
                && o.attributes
                    .iter()
                    .any(|(k, v)| k == "predicate" && v == "ole")
                && o.attributes
                    .iter()
                    .any(|(k, v)| k == "diff_role" && v == "tangent")
        });
        let has_select = entry.ops.iter().any(|o| {
            o.name == "arith.select"
                && o.attributes
                    .iter()
                    .any(|(k, v)| k == "diff_role" && v == "tangent")
                && o.attributes
                    .iter()
                    .any(|(k, v)| k == "diff_primitive" && v == "min")
        });
        assert!(has_cmpf_ole, "expected tangent arith.cmpf predicate=ole");
        assert!(
            has_select,
            "expected tangent arith.select diff_primitive=min"
        );
    }

    #[test]
    fn fwd_max_emits_cmpf_oge_plus_select() {
        let ops = vec![
            MirOp::std("arith.maximumf")
                .with_operand(ValueId(0))
                .with_operand(ValueId(1))
                .with_result(ValueId(2), f32_ty()),
            MirOp::std("func.return").with_operand(ValueId(2)),
        ];
        let primal = mk_primal("max_fn", vec![f32_ty(), f32_ty()], vec![f32_ty()], ops);
        let (variant, _, _) = apply_fwd(&primal, &DiffRuleTable::canonical());
        let entry = variant.body.entry().unwrap();
        let has_cmpf_oge = entry.ops.iter().any(|o| {
            o.name == "arith.cmpf"
                && o.attributes
                    .iter()
                    .any(|(k, v)| k == "predicate" && v == "oge")
        });
        assert!(has_cmpf_oge, "expected tangent arith.cmpf predicate=oge");
    }

    #[test]
    fn fwd_abs_emits_constant_cmpf_negf_select() {
        let ops = vec![
            MirOp::std("math.absf")
                .with_operand(ValueId(0))
                .with_result(ValueId(1), f32_ty()),
            MirOp::std("func.return").with_operand(ValueId(1)),
        ];
        let primal = mk_primal("abs_fn", vec![f32_ty()], vec![f32_ty()], ops);
        let (variant, _, report) = apply_fwd(&primal, &DiffRuleTable::canonical());
        assert_eq!(report.primitives_substituted, 1);
        let entry = variant.body.entry().unwrap();
        // Fwd body must emit : const 0 + cmpf oge + negf d_x + select.
        let has_const = entry.ops.iter().any(|o| {
            o.name == "arith.constant"
                && o.attributes
                    .iter()
                    .any(|(k, v)| k == "diff_primitive" && v == "abs")
        });
        let has_cmpf = entry.ops.iter().any(|o| {
            o.name == "arith.cmpf"
                && o.attributes
                    .iter()
                    .any(|(k, v)| k == "diff_primitive" && v == "abs")
        });
        let has_negf = entry.ops.iter().any(|o| {
            o.name == "arith.negf"
                && o.attributes
                    .iter()
                    .any(|(k, v)| k == "diff_primitive" && v == "abs")
        });
        let has_select = entry.ops.iter().any(|o| {
            o.name == "arith.select"
                && o.attributes
                    .iter()
                    .any(|(k, v)| k == "diff_primitive" && v == "abs")
        });
        assert!(has_const, "expected const 0.0 for abs fwd");
        assert!(has_cmpf, "expected cmpf for abs fwd");
        assert!(has_negf, "expected negf for abs fwd");
        assert!(has_select, "expected select for abs fwd");
    }

    #[test]
    fn fwd_sign_emits_constant_zero() {
        let ops = vec![
            MirOp::std("math.copysign")
                .with_operand(ValueId(0))
                .with_result(ValueId(1), f32_ty()),
            MirOp::std("func.return").with_operand(ValueId(1)),
        ];
        let primal = mk_primal("sign_fn", vec![f32_ty()], vec![f32_ty()], ops);
        let (variant, _, report) = apply_fwd(&primal, &DiffRuleTable::canonical());
        assert_eq!(report.primitives_substituted, 1);
        let entry = variant.body.entry().unwrap();
        let has_zero_const = entry.ops.iter().any(|o| {
            o.name == "arith.constant"
                && o.attributes.iter().any(|(k, v)| k == "value" && v == "0.0")
                && o.attributes
                    .iter()
                    .any(|(k, v)| k == "diff_primitive" && v == "sign")
        });
        assert!(has_zero_const, "expected zero-tangent for sign");
    }

    #[test]
    fn bwd_min_emits_select_plus_accumulate() {
        let ops = vec![
            MirOp::std("arith.minimumf")
                .with_operand(ValueId(0))
                .with_operand(ValueId(1))
                .with_result(ValueId(2), f32_ty()),
            MirOp::std("func.return").with_operand(ValueId(2)),
        ];
        let primal = mk_primal("min_fn", vec![f32_ty(), f32_ty()], vec![f32_ty()], ops);
        let (variant, _, report) = apply_bwd(&primal, &DiffRuleTable::canonical());
        assert_eq!(report.primitives_substituted, 1);
        let entry = variant.body.entry().unwrap();
        // Bwd should have at least : 1 cmpf + 2 selects + 2 addf (accumulation), all adjoint-role + diff_primitive=min.
        let cmpf_count = entry
            .ops
            .iter()
            .filter(|o| {
                o.name == "arith.cmpf"
                    && o.attributes
                        .iter()
                        .any(|(k, v)| k == "diff_role" && v == "adjoint")
                    && o.attributes
                        .iter()
                        .any(|(k, v)| k == "diff_primitive" && v == "min")
            })
            .count();
        let select_count = entry
            .ops
            .iter()
            .filter(|o| {
                o.name == "arith.select"
                    && o.attributes
                        .iter()
                        .any(|(k, v)| k == "diff_role" && v == "adjoint")
                    && o.attributes
                        .iter()
                        .any(|(k, v)| k == "diff_primitive" && v == "min")
            })
            .count();
        assert!(cmpf_count >= 1, "expected ≥ 1 adjoint cmpf for min bwd");
        assert!(
            select_count >= 2,
            "expected ≥ 2 adjoint selects for min bwd (one per branch)"
        );
    }

    #[test]
    fn bwd_abs_emits_select_plus_accumulate() {
        let ops = vec![
            MirOp::std("math.absf")
                .with_operand(ValueId(0))
                .with_result(ValueId(1), f32_ty()),
            MirOp::std("func.return").with_operand(ValueId(1)),
        ];
        let primal = mk_primal("abs_fn", vec![f32_ty()], vec![f32_ty()], ops);
        let (variant, _, _) = apply_bwd(&primal, &DiffRuleTable::canonical());
        let entry = variant.body.entry().unwrap();
        let select_count = entry
            .ops
            .iter()
            .filter(|o| {
                o.name == "arith.select"
                    && o.attributes
                        .iter()
                        .any(|(k, v)| k == "diff_primitive" && v == "abs")
            })
            .count();
        assert!(select_count >= 1, "expected ≥ 1 adjoint select for abs bwd");
    }

    #[test]
    fn bwd_sign_is_noop() {
        let ops = vec![
            MirOp::std("math.copysign")
                .with_operand(ValueId(0))
                .with_result(ValueId(1), f32_ty()),
            MirOp::std("func.return").with_operand(ValueId(1)),
        ];
        let primal = mk_primal("sign_fn", vec![f32_ty()], vec![f32_ty()], ops);
        let (variant, _, _) = apply_bwd(&primal, &DiffRuleTable::canonical());
        let entry = variant.body.entry().unwrap();
        // Sign bwd emits no ops : count only the bwd_return terminator + any preexisting primal.
        let sign_primitive_ops = entry
            .ops
            .iter()
            .filter(|o| {
                o.attributes
                    .iter()
                    .any(|(k, v)| k == "diff_primitive" && v == "sign")
            })
            .count();
        assert_eq!(sign_primitive_ops, 0, "sign bwd should emit zero body-ops");
    }

    #[test]
    fn min_and_max_no_longer_emit_fwd_placeholder() {
        let ops = vec![
            MirOp::std("arith.minimumf")
                .with_operand(ValueId(0))
                .with_operand(ValueId(1))
                .with_result(ValueId(2), f32_ty()),
            MirOp::std("func.return").with_operand(ValueId(2)),
        ];
        let primal = mk_primal("min_fn", vec![f32_ty(), f32_ty()], vec![f32_ty()], ops);
        let (variant, _, _) = apply_fwd(&primal, &DiffRuleTable::canonical());
        let entry = variant.body.entry().unwrap();
        let has_placeholder = entry
            .ops
            .iter()
            .any(|o| o.name == "cssl.diff.fwd_placeholder");
        assert!(
            !has_placeholder,
            "fwd_placeholder should no longer appear for min after T11-D15"
        );
    }
}
