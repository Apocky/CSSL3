//! Const-propagation pass for `@staged` specialization.
//!
//! § OBJECTIVE
//!   When [`crate::SpecializationPass`] clones a callee + binds the comptime
//!   args, the clone's body has SSA-values that can now be folded :
//!     - parameter loads of bound comptime args            → constant
//!     - `arith.add` / `arith.sub` / `arith.mul` of consts → constant
//!     - `arith.cmp*` of consts                            → bool
//!     - `scf.if cond { ... }` with const cond             → branch fold
//!     - `arith.constant` chained through `arith.bitcast`  → propagate
//!
//!   The pass is iterative : we run the folding worklist until a fixed-point,
//!   then return the [`ConstPropReport`] for the caller (specializer) to use
//!   for downstream DCE bookkeeping.
//!
//! § INTERFACE
//!   - [`ConstEnv`]            — owns the value-id → Value binding map.
//!   - [`fold_arith`]          — fold a single arith.* op given const operands.
//!   - [`run_const_prop_pass`] — iterative top-level driver.
//!   - [`ConstPropReport`]     — aggregate counters per pass-run.
//!
//! § STAGE-0 SCOPE
//!   We do NOT touch ops outside the arith / scf / cssl.constant family at
//!   this pass — heap, IFC, telemetry, network ops are passed through. The
//!   [`crate::dce`] pass handles unreachable-block removal after const-prop
//!   simplifies branches.

use std::collections::HashMap;

use cssl_mir::{IntWidth, MirBlock, MirFunc, MirOp, MirRegion, MirType, ValueId};

use crate::value::{CompIntWidth, Value};

/// Live const-environment : maps SSA-value-id → known comptime-Value.
#[derive(Debug, Default, Clone)]
pub struct ConstEnv {
    bindings: HashMap<ValueId, Value>,
}

impl ConstEnv {
    /// Empty environment.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Bind a value-id to a comptime-value. Idempotent : rebinding to the
    /// same value is a no-op ; rebinding to a different value indicates a
    /// const-prop bug + should never happen for SSA-values.
    pub fn bind(&mut self, id: ValueId, v: Value) {
        self.bindings.insert(id, v);
    }

    /// Look up a binding ; returns `None` for runtime-only values.
    #[must_use]
    pub fn get(&self, id: ValueId) -> Option<&Value> {
        self.bindings.get(&id)
    }

    /// `true` iff this id has a known constant binding.
    #[must_use]
    pub fn is_const(&self, id: ValueId) -> bool {
        self.bindings.contains_key(&id)
    }

    /// Number of bindings.
    #[must_use]
    pub fn len(&self) -> usize {
        self.bindings.len()
    }

    /// `true` iff empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bindings.is_empty()
    }

    /// Remove a binding (used when const-prop strength-reduces an op : the
    /// previous binding becomes stale + must be retracted).
    pub fn unbind(&mut self, id: ValueId) {
        self.bindings.remove(&id);
    }
}

/// Aggregated result of a const-prop pass run.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ConstPropReport {
    /// Number of arith.* ops folded to a constant.
    pub arith_folds: u32,
    /// Number of scf.if / cssl.if-style branch-folds (cond → known) applied.
    pub branch_folds: u32,
    /// Number of `arith.cmp*` ops folded.
    pub cmp_folds: u32,
    /// Number of fixed-point iterations the worklist took.
    pub iterations: u32,
}

impl ConstPropReport {
    /// Total simplifications across all categories.
    #[must_use]
    pub fn total(self) -> u32 {
        self.arith_folds + self.branch_folds + self.cmp_folds
    }
}

/// Map a `MirType::Int(IntWidth)` to a [`CompIntWidth`] so folded ints carry
/// their original width.
#[must_use]
pub const fn int_width_from_mir(w: IntWidth) -> CompIntWidth {
    match w {
        IntWidth::I1 => CompIntWidth::I1,
        IntWidth::I8 => CompIntWidth::I8,
        IntWidth::I16 => CompIntWidth::I16,
        IntWidth::I32 | IntWidth::Index => CompIntWidth::I32,
        IntWidth::I64 => CompIntWidth::I64,
    }
}

/// Look up two operand values + treat both as constants. Returns `None` if
/// either is non-const.
fn binary_consts<'a>(env: &'a ConstEnv, operands: &[ValueId]) -> Option<(&'a Value, &'a Value)> {
    if operands.len() < 2 {
        return None;
    }
    let a = env.get(operands[0])?;
    let b = env.get(operands[1])?;
    Some((a, b))
}

/// Fold a single arith.* op given the const-env. Returns `Some(new_value)`
/// if the op simplifies to a constant ; `None` if it cannot be folded.
///
/// § HANDLES
///   - `arith.constant`               — read attribute, produce Value.
///   - `arith.addi / subi / muli`     — integer arithmetic.
///   - `arith.divi_s / remi_s`        — integer signed div/rem (skip /0).
///   - `arith.addf / subf / mulf / divf` — float arithmetic.
///   - `arith.andi / ori / xori`      — integer bit-ops.
///   - `arith.shli / shrsi`           — shift-left + signed-shift-right.
///   - `arith.cmpi {eq,ne,slt,sle,sgt,sge}` — integer comparison.
///   - `arith.cmpf {oeq,one,olt,ole,ogt,oge}` — float comparison.
///   - `arith.select`                 — ternary select on const cond.
#[must_use]
pub fn fold_arith(op: &MirOp, env: &ConstEnv) -> Option<Value> {
    match op.name.as_str() {
        // Constant : read the attribute payload + the result type.
        "arith.constant" => fold_arith_constant(op),
        // Integer arithmetic (signed semantics ; saturate to declared width on truncate).
        "arith.addi" => binary_consts(env, &op.operands)
            .and_then(|(a, b)| arith_int_op(a, b, |x, y| x.wrapping_add(y))),
        "arith.subi" => binary_consts(env, &op.operands)
            .and_then(|(a, b)| arith_int_op(a, b, |x, y| x.wrapping_sub(y))),
        "arith.muli" => binary_consts(env, &op.operands)
            .and_then(|(a, b)| arith_int_op(a, b, |x, y| x.wrapping_mul(y))),
        "arith.divi_s" => binary_consts(env, &op.operands).and_then(arith_int_div),
        "arith.remi_s" => binary_consts(env, &op.operands).and_then(arith_int_rem),
        "arith.andi" => {
            binary_consts(env, &op.operands).and_then(|(a, b)| arith_int_op(a, b, |x, y| x & y))
        }
        "arith.ori" => {
            binary_consts(env, &op.operands).and_then(|(a, b)| arith_int_op(a, b, |x, y| x | y))
        }
        "arith.xori" => {
            binary_consts(env, &op.operands).and_then(|(a, b)| arith_int_op(a, b, |x, y| x ^ y))
        }
        "arith.shli" => binary_consts(env, &op.operands).and_then(arith_int_shl),
        "arith.shrsi" => binary_consts(env, &op.operands).and_then(arith_int_shrs),
        // Float arithmetic.
        "arith.addf" => {
            binary_consts(env, &op.operands).and_then(|(a, b)| arith_float_op(a, b, |x, y| x + y))
        }
        "arith.subf" => {
            binary_consts(env, &op.operands).and_then(|(a, b)| arith_float_op(a, b, |x, y| x - y))
        }
        "arith.mulf" => {
            binary_consts(env, &op.operands).and_then(|(a, b)| arith_float_op(a, b, |x, y| x * y))
        }
        "arith.divf" => {
            binary_consts(env, &op.operands).and_then(|(a, b)| arith_float_op(a, b, |x, y| x / y))
        }
        // Integer compare.
        "arith.cmpi" => {
            binary_consts(env, &op.operands).and_then(|(a, b)| arith_int_cmp(a, b, &op.attributes))
        }
        // Float compare.
        "arith.cmpf" => binary_consts(env, &op.operands)
            .and_then(|(a, b)| arith_float_cmp(a, b, &op.attributes)),
        // Select : if cond is const, pick the corresponding branch (which
        // must itself be const for a full fold).
        "arith.select" => arith_select(op, env),
        _ => None,
    }
}

/// Fold an `arith.constant` op by reading the `value` + result-type
/// attributes. The op's result type comes from `op.results[0].ty`.
fn fold_arith_constant(op: &MirOp) -> Option<Value> {
    let ty = op.results.first().map(|r| &r.ty)?;
    // Look for `("value", "...")` ; arith.constant attaches the literal text.
    let val_str =
        op.attributes
            .iter()
            .find_map(|(k, v)| if k == "value" { Some(v.as_str()) } else { None })?;
    match ty {
        MirType::Int(w) => {
            let parsed: i64 = val_str.parse().ok()?;
            let cw = int_width_from_mir(*w);
            Some(Value::Int(cw.saturate(parsed), cw))
        }
        MirType::Float(_) => val_str.parse::<f64>().ok().map(Value::Float),
        MirType::Bool => match val_str {
            "true" | "1" => Some(Value::Bool(true)),
            "false" | "0" => Some(Value::Bool(false)),
            _ => None,
        },
        _ => None,
    }
}

fn arith_int_op<F: Fn(i64, i64) -> i64>(a: &Value, b: &Value, f: F) -> Option<Value> {
    if let (Value::Int(x, wa), Value::Int(y, wb)) = (a, b) {
        // Promote to the wider lane.
        let w = if (*wa as u8) >= (*wb as u8) { *wa } else { *wb };
        Some(Value::Int(w.saturate(f(*x, *y)), w))
    } else {
        None
    }
}

fn arith_int_div(pair: (&Value, &Value)) -> Option<Value> {
    let (a, b) = pair;
    if let (Value::Int(x, wa), Value::Int(y, wb)) = (a, b) {
        if *y == 0 {
            return None;
        }
        let w = if (*wa as u8) >= (*wb as u8) { *wa } else { *wb };
        Some(Value::Int(w.saturate(x.wrapping_div(*y)), w))
    } else {
        None
    }
}

fn arith_int_rem(pair: (&Value, &Value)) -> Option<Value> {
    let (a, b) = pair;
    if let (Value::Int(x, wa), Value::Int(y, wb)) = (a, b) {
        if *y == 0 {
            return None;
        }
        let w = if (*wa as u8) >= (*wb as u8) { *wa } else { *wb };
        Some(Value::Int(w.saturate(x.wrapping_rem(*y)), w))
    } else {
        None
    }
}

fn arith_int_shl(pair: (&Value, &Value)) -> Option<Value> {
    let (a, b) = pair;
    if let (Value::Int(x, w), Value::Int(s, _)) = (a, b) {
        // Bound shift to the declared width to avoid undefined-shift behavior.
        let max_shift = match w {
            CompIntWidth::I1 => 0,
            CompIntWidth::I8 => 7,
            CompIntWidth::I16 => 15,
            CompIntWidth::I32 => 31,
            CompIntWidth::I64 => 63,
        };
        if *s < 0 || *s > max_shift {
            return None;
        }
        Some(Value::Int(w.saturate(x.wrapping_shl(*s as u32)), *w))
    } else {
        None
    }
}

fn arith_int_shrs(pair: (&Value, &Value)) -> Option<Value> {
    let (a, b) = pair;
    if let (Value::Int(x, w), Value::Int(s, _)) = (a, b) {
        let max_shift = match w {
            CompIntWidth::I1 => 0,
            CompIntWidth::I8 => 7,
            CompIntWidth::I16 => 15,
            CompIntWidth::I32 => 31,
            CompIntWidth::I64 => 63,
        };
        if *s < 0 || *s > max_shift {
            return None;
        }
        Some(Value::Int(w.saturate(x.wrapping_shr(*s as u32)), *w))
    } else {
        None
    }
}

fn arith_float_op<F: Fn(f64, f64) -> f64>(a: &Value, b: &Value, f: F) -> Option<Value> {
    if let (Value::Float(x), Value::Float(y)) = (a, b) {
        Some(Value::Float(f(*x, *y)))
    } else {
        None
    }
}

fn arith_int_cmp(a: &Value, b: &Value, attrs: &[(String, String)]) -> Option<Value> {
    let pred = find_attr(attrs, "predicate")?;
    if let (Value::Int(x, _), Value::Int(y, _)) = (a, b) {
        let ord = x.cmp(y);
        let r = match pred {
            "eq" => *x == *y,
            "ne" => *x != *y,
            "slt" => ord.is_lt(),
            "sle" => ord.is_le(),
            "sgt" => ord.is_gt(),
            "sge" => ord.is_ge(),
            _ => return None,
        };
        Some(Value::Bool(r))
    } else {
        None
    }
}

fn arith_float_cmp(a: &Value, b: &Value, attrs: &[(String, String)]) -> Option<Value> {
    let pred = find_attr(attrs, "predicate")?;
    if let (Value::Float(x), Value::Float(y)) = (a, b) {
        let r = match pred {
            "oeq" => x == y,
            "one" => x != y,
            "olt" => x < y,
            "ole" => x <= y,
            "ogt" => x > y,
            "oge" => x >= y,
            _ => return None,
        };
        Some(Value::Bool(r))
    } else {
        None
    }
}

fn arith_select(op: &MirOp, env: &ConstEnv) -> Option<Value> {
    if op.operands.len() < 3 {
        return None;
    }
    let cond = env.get(op.operands[0])?;
    let then_v = env.get(op.operands[1])?;
    let else_v = env.get(op.operands[2])?;
    if let Value::Bool(b) = cond {
        if *b {
            Some(then_v.clone())
        } else {
            Some(else_v.clone())
        }
    } else {
        None
    }
}

fn find_attr<'a>(attrs: &'a [(String, String)], key: &str) -> Option<&'a str> {
    attrs
        .iter()
        .find_map(|(k, v)| if k == key { Some(v.as_str()) } else { None })
}

/// Run const-prop over a single block + populate the env with newly-folded
/// values. Returns `true` if any op was folded ; the caller iterates until
/// a fixed-point.
///
/// The fold rewrites `op` in-place :
///   - For arith.* ops that fold to a const : rewrite the op to
///     `arith.constant` carrying the resolved Value's textual form.
///   - For scf.if with const cond : the op is left alone (the DCE pass
///     handles branch-elimination after seeing the recorded const-cond).
fn fold_block_pass(block: &mut MirBlock, env: &mut ConstEnv, report: &mut ConstPropReport) -> bool {
    let mut changed = false;
    let mut i = 0;
    while i < block.ops.len() {
        // First : recurse into nested regions (scf.if / scf.for / cssl.region).
        let nested_changed = {
            let op = &mut block.ops[i];
            let mut any = false;
            for region in &mut op.regions {
                if fold_region_pass(region, env, report) {
                    any = true;
                }
            }
            any
        };
        if nested_changed {
            changed = true;
        }

        // Then : try to fold this op. We skip ops whose result-id is
        // already bound in the env — that means we already folded this op
        // on a previous iteration + further folding is a no-op (would
        // diverge if not skipped).
        let folded_value = {
            let op = &block.ops[i];
            let already_bound = op.results.iter().any(|r| env.is_const(r.id));
            if already_bound {
                None
            } else {
                fold_arith(op, env)
            }
        };
        if let Some(v) = folded_value {
            // Capture the bookkeeping state before mutating the slot.
            let op_name = block.ops[i].name.clone();
            let result_ids: Vec<_> = block.ops[i].results.iter().map(|r| r.id).collect();
            for id in result_ids {
                env.bind(id, v.clone());
            }
            // Rewrite the op as `arith.constant <v>` so downstream codegen
            // sees a clean shape. We preserve the result type ; we ditch
            // the operands.
            let folded = rewrite_to_constant(&block.ops[i], &v);
            block.ops[i] = folded;
            // Track stat.
            match op_name.as_str() {
                "arith.cmpi" | "arith.cmpf" => {
                    report.cmp_folds = report.cmp_folds.saturating_add(1);
                }
                _ => {
                    report.arith_folds = report.arith_folds.saturating_add(1);
                }
            }
            changed = true;
        }
        i += 1;
    }
    changed
}

fn fold_region_pass(
    region: &mut MirRegion,
    env: &mut ConstEnv,
    report: &mut ConstPropReport,
) -> bool {
    let mut changed = false;
    for block in &mut region.blocks {
        if fold_block_pass(block, env, report) {
            changed = true;
        }
    }
    changed
}

/// Rewrite an op to `arith.constant <v>` preserving the result-type slot.
fn rewrite_to_constant(op: &MirOp, v: &Value) -> MirOp {
    let result = op.results.first().cloned();
    let mut new_op = MirOp::std("arith.constant");
    if let Some(r) = result {
        new_op.results.push(r);
    }
    let value_str = match v {
        Value::Int(n, _) => n.to_string(),
        Value::Float(f) => format!("{f}"),
        Value::Bool(b) => if *b { "true" } else { "false" }.to_string(),
        Value::Str(s) => s.clone(),
        Value::Sym(s) => s.clone(),
        Value::Unit => "()".to_string(),
        Value::Tuple(_) => v.mangle_fragment(),
    };
    new_op.attributes.push(("value".into(), value_str));
    new_op
}

/// Iterate const-prop over the function body until a fixed-point is reached.
/// The caller pre-binds the comptime-arg values into `env`. Returns the
/// per-pass report so the specializer can roll up stats.
pub fn run_const_prop_pass(func: &mut MirFunc, env: &mut ConstEnv) -> ConstPropReport {
    let mut report = ConstPropReport::default();
    let max_iters = 32;
    while report.iterations < max_iters {
        report.iterations = report.iterations.saturating_add(1);
        let changed = fold_region_pass(&mut func.body, env, &mut report);
        if !changed {
            break;
        }
    }
    report
}

/// Detect `scf.if` ops whose condition operand is bound to a constant Bool,
/// + return a list of `(block_idx, op_idx, branch)` tuples so the DCE pass
/// can eliminate the unreachable region. `branch == true` ⇒ then-region wins ;
/// `false` ⇒ else-region wins. Walks only the top-level blocks/ops at this
/// depth — the DCE pass recurses on its own.
#[must_use]
pub fn collect_branch_folds(func: &MirFunc, env: &ConstEnv) -> Vec<BranchFold> {
    let mut out = Vec::new();
    for (b_idx, block) in func.body.blocks.iter().enumerate() {
        for (o_idx, op) in block.ops.iter().enumerate() {
            if op.name == "scf.if" || op.name == "cssl.if" {
                if let Some(first) = op.operands.first() {
                    if let Some(Value::Bool(b)) = env.get(*first) {
                        out.push(BranchFold {
                            block_idx: b_idx,
                            op_idx: o_idx,
                            taken_branch: *b,
                        });
                    }
                }
            }
        }
    }
    out
}

/// One eligible branch-fold site : block-idx + op-idx + which branch wins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BranchFold {
    /// Index into `func.body.blocks`.
    pub block_idx: usize,
    /// Index into the block's `ops`.
    pub op_idx: usize,
    /// `true` ⇒ then-branch wins ; `false` ⇒ else-branch wins.
    pub taken_branch: bool,
}

#[cfg(test)]
mod tests {
    use super::{
        collect_branch_folds, fold_arith, int_width_from_mir, run_const_prop_pass, ConstEnv,
        ConstPropReport,
    };
    use crate::value::{CompIntWidth, Value};
    use cssl_mir::{IntWidth, MirFunc, MirOp, MirType, MirValue, ValueId};

    fn mk_int_const(id: u32, val: i64, w: IntWidth) -> MirOp {
        let mut op = MirOp::std("arith.constant");
        op.results.push(MirValue::new(ValueId(id), MirType::Int(w)));
        op.attributes.push(("value".into(), val.to_string()));
        op
    }

    fn mk_bool_const(id: u32, val: bool) -> MirOp {
        let mut op = MirOp::std("arith.constant");
        op.results.push(MirValue::new(ValueId(id), MirType::Bool));
        op.attributes.push((
            "value".into(),
            if val { "true" } else { "false" }.to_string(),
        ));
        op
    }

    fn mk_arith(op_name: &str, lhs: u32, rhs: u32, result_id: u32, ty: MirType) -> MirOp {
        let mut op = MirOp::std(op_name);
        op.operands.push(ValueId(lhs));
        op.operands.push(ValueId(rhs));
        op.results.push(MirValue::new(ValueId(result_id), ty));
        op
    }

    #[test]
    fn const_env_default_empty() {
        let e = ConstEnv::new();
        assert!(e.is_empty());
        assert_eq!(e.len(), 0);
    }

    #[test]
    fn const_env_bind_and_lookup() {
        let mut e = ConstEnv::new();
        e.bind(ValueId(1), Value::Int(42, CompIntWidth::I32));
        assert!(e.is_const(ValueId(1)));
        assert!(!e.is_const(ValueId(2)));
        assert_eq!(e.len(), 1);
    }

    #[test]
    fn const_env_unbind_removes() {
        let mut e = ConstEnv::new();
        e.bind(ValueId(1), Value::Bool(true));
        assert!(e.is_const(ValueId(1)));
        e.unbind(ValueId(1));
        assert!(!e.is_const(ValueId(1)));
    }

    #[test]
    fn int_width_from_mir_index_maps_to_i32() {
        // index is host-pointer-sized but we treat it as i32 in stage-0 const-prop
        // for cross-host stability. Renaming requires an ABI-stability decision.
        assert_eq!(int_width_from_mir(IntWidth::Index), CompIntWidth::I32);
    }

    #[test]
    fn int_width_from_mir_canonical_mappings() {
        assert_eq!(int_width_from_mir(IntWidth::I1), CompIntWidth::I1);
        assert_eq!(int_width_from_mir(IntWidth::I8), CompIntWidth::I8);
        assert_eq!(int_width_from_mir(IntWidth::I16), CompIntWidth::I16);
        assert_eq!(int_width_from_mir(IntWidth::I32), CompIntWidth::I32);
        assert_eq!(int_width_from_mir(IntWidth::I64), CompIntWidth::I64);
    }

    #[test]
    fn fold_arith_constant_int() {
        let op = mk_int_const(0, 7, IntWidth::I32);
        let env = ConstEnv::new();
        let v = fold_arith(&op, &env).unwrap();
        assert_eq!(v, Value::Int(7, CompIntWidth::I32));
    }

    #[test]
    fn fold_arith_constant_bool_true_or_false() {
        let env = ConstEnv::new();
        let op_t = mk_bool_const(0, true);
        let op_f = mk_bool_const(0, false);
        assert_eq!(fold_arith(&op_t, &env).unwrap(), Value::Bool(true));
        assert_eq!(fold_arith(&op_f, &env).unwrap(), Value::Bool(false));
    }

    #[test]
    fn fold_arith_addi_with_known_operands() {
        let mut env = ConstEnv::new();
        env.bind(ValueId(0), Value::Int(3, CompIntWidth::I32));
        env.bind(ValueId(1), Value::Int(4, CompIntWidth::I32));
        let op = mk_arith("arith.addi", 0, 1, 2, MirType::Int(IntWidth::I32));
        let v = fold_arith(&op, &env).unwrap();
        assert_eq!(v, Value::Int(7, CompIntWidth::I32));
    }

    #[test]
    fn fold_arith_subi_negative_result_saturates_to_width() {
        let mut env = ConstEnv::new();
        env.bind(ValueId(0), Value::Int(0, CompIntWidth::I8));
        env.bind(ValueId(1), Value::Int(200, CompIntWidth::I8));
        let op = mk_arith("arith.subi", 0, 1, 2, MirType::Int(IntWidth::I8));
        // 0 - 200 = -200 ; saturates to i8::MIN (-128).
        let v = fold_arith(&op, &env).unwrap();
        assert_eq!(v, Value::Int(-128, CompIntWidth::I8));
    }

    #[test]
    fn fold_arith_muli() {
        let mut env = ConstEnv::new();
        env.bind(ValueId(0), Value::Int(6, CompIntWidth::I32));
        env.bind(ValueId(1), Value::Int(7, CompIntWidth::I32));
        let op = mk_arith("arith.muli", 0, 1, 2, MirType::Int(IntWidth::I32));
        assert_eq!(
            fold_arith(&op, &env).unwrap(),
            Value::Int(42, CompIntWidth::I32)
        );
    }

    #[test]
    fn fold_arith_divi_s_skips_division_by_zero() {
        let mut env = ConstEnv::new();
        env.bind(ValueId(0), Value::Int(6, CompIntWidth::I32));
        env.bind(ValueId(1), Value::Int(0, CompIntWidth::I32));
        let op = mk_arith("arith.divi_s", 0, 1, 2, MirType::Int(IntWidth::I32));
        // Division by zero must NOT fold ; const-prop returns None so the
        // op stays unchanged (the runtime ABI handles div-by-zero).
        assert!(fold_arith(&op, &env).is_none());
    }

    #[test]
    fn fold_arith_remi_skips_division_by_zero() {
        let mut env = ConstEnv::new();
        env.bind(ValueId(0), Value::Int(6, CompIntWidth::I32));
        env.bind(ValueId(1), Value::Int(0, CompIntWidth::I32));
        let op = mk_arith("arith.remi_s", 0, 1, 2, MirType::Int(IntWidth::I32));
        assert!(fold_arith(&op, &env).is_none());
    }

    #[test]
    fn fold_arith_divi_s_signed() {
        let mut env = ConstEnv::new();
        env.bind(ValueId(0), Value::Int(20, CompIntWidth::I32));
        env.bind(ValueId(1), Value::Int(3, CompIntWidth::I32));
        let op = mk_arith("arith.divi_s", 0, 1, 2, MirType::Int(IntWidth::I32));
        assert_eq!(
            fold_arith(&op, &env).unwrap(),
            Value::Int(6, CompIntWidth::I32)
        );
    }

    #[test]
    fn fold_arith_andi() {
        let mut env = ConstEnv::new();
        env.bind(ValueId(0), Value::Int(0xff, CompIntWidth::I32));
        env.bind(ValueId(1), Value::Int(0x0f, CompIntWidth::I32));
        let op = mk_arith("arith.andi", 0, 1, 2, MirType::Int(IntWidth::I32));
        assert_eq!(
            fold_arith(&op, &env).unwrap(),
            Value::Int(0x0f, CompIntWidth::I32)
        );
    }

    #[test]
    fn fold_arith_ori_xori() {
        let mut env = ConstEnv::new();
        env.bind(ValueId(0), Value::Int(0x0f, CompIntWidth::I32));
        env.bind(ValueId(1), Value::Int(0xf0, CompIntWidth::I32));
        let or_op = mk_arith("arith.ori", 0, 1, 2, MirType::Int(IntWidth::I32));
        let xor_op = mk_arith("arith.xori", 0, 1, 3, MirType::Int(IntWidth::I32));
        assert_eq!(
            fold_arith(&or_op, &env).unwrap(),
            Value::Int(0xff, CompIntWidth::I32)
        );
        assert_eq!(
            fold_arith(&xor_op, &env).unwrap(),
            Value::Int(0xff, CompIntWidth::I32)
        );
    }

    #[test]
    fn fold_arith_shli() {
        let mut env = ConstEnv::new();
        env.bind(ValueId(0), Value::Int(1, CompIntWidth::I32));
        env.bind(ValueId(1), Value::Int(4, CompIntWidth::I32));
        let op = mk_arith("arith.shli", 0, 1, 2, MirType::Int(IntWidth::I32));
        assert_eq!(
            fold_arith(&op, &env).unwrap(),
            Value::Int(16, CompIntWidth::I32)
        );
    }

    #[test]
    fn fold_arith_shli_out_of_range_skipped() {
        let mut env = ConstEnv::new();
        env.bind(ValueId(0), Value::Int(1, CompIntWidth::I32));
        env.bind(ValueId(1), Value::Int(64, CompIntWidth::I32));
        let op = mk_arith("arith.shli", 0, 1, 2, MirType::Int(IntWidth::I32));
        // Shift by 64 in i32 lane is undefined ; const-prop must skip.
        assert!(fold_arith(&op, &env).is_none());
    }

    #[test]
    fn fold_arith_shrsi() {
        let mut env = ConstEnv::new();
        env.bind(ValueId(0), Value::Int(-32, CompIntWidth::I32));
        env.bind(ValueId(1), Value::Int(2, CompIntWidth::I32));
        let op = mk_arith("arith.shrsi", 0, 1, 2, MirType::Int(IntWidth::I32));
        // arithmetic shift right by 2 of -32 = -8 (sign-extended).
        assert_eq!(
            fold_arith(&op, &env).unwrap(),
            Value::Int(-8, CompIntWidth::I32)
        );
    }

    #[test]
    fn fold_arith_addf() {
        let mut env = ConstEnv::new();
        env.bind(ValueId(0), Value::Float(1.5));
        env.bind(ValueId(1), Value::Float(2.25));
        let op = mk_arith(
            "arith.addf",
            0,
            1,
            2,
            MirType::Float(cssl_mir::FloatWidth::F32),
        );
        let v = fold_arith(&op, &env).unwrap();
        if let Value::Float(f) = v {
            assert!((f - 3.75).abs() < 1e-6);
        } else {
            panic!("expected Float result");
        }
    }

    #[test]
    fn fold_arith_mulf() {
        let mut env = ConstEnv::new();
        env.bind(ValueId(0), Value::Float(3.0));
        env.bind(ValueId(1), Value::Float(4.0));
        let op = mk_arith(
            "arith.mulf",
            0,
            1,
            2,
            MirType::Float(cssl_mir::FloatWidth::F32),
        );
        let v = fold_arith(&op, &env).unwrap();
        assert_eq!(v, Value::Float(12.0));
    }

    #[test]
    fn fold_arith_cmpi_eq() {
        let mut env = ConstEnv::new();
        env.bind(ValueId(0), Value::Int(5, CompIntWidth::I32));
        env.bind(ValueId(1), Value::Int(5, CompIntWidth::I32));
        let mut op = mk_arith("arith.cmpi", 0, 1, 2, MirType::Bool);
        op.attributes.push(("predicate".into(), "eq".into()));
        assert_eq!(fold_arith(&op, &env).unwrap(), Value::Bool(true));
    }

    #[test]
    fn fold_arith_cmpi_slt_sgt_ne() {
        let mut env = ConstEnv::new();
        env.bind(ValueId(0), Value::Int(3, CompIntWidth::I32));
        env.bind(ValueId(1), Value::Int(5, CompIntWidth::I32));
        let mut lt = mk_arith("arith.cmpi", 0, 1, 2, MirType::Bool);
        lt.attributes.push(("predicate".into(), "slt".into()));
        let mut gt = mk_arith("arith.cmpi", 0, 1, 3, MirType::Bool);
        gt.attributes.push(("predicate".into(), "sgt".into()));
        let mut ne = mk_arith("arith.cmpi", 0, 1, 4, MirType::Bool);
        ne.attributes.push(("predicate".into(), "ne".into()));
        assert_eq!(fold_arith(&lt, &env).unwrap(), Value::Bool(true));
        assert_eq!(fold_arith(&gt, &env).unwrap(), Value::Bool(false));
        assert_eq!(fold_arith(&ne, &env).unwrap(), Value::Bool(true));
    }

    #[test]
    fn fold_arith_cmpf_olt() {
        let mut env = ConstEnv::new();
        env.bind(ValueId(0), Value::Float(1.0));
        env.bind(ValueId(1), Value::Float(2.0));
        let mut op = mk_arith("arith.cmpf", 0, 1, 2, MirType::Bool);
        op.attributes.push(("predicate".into(), "olt".into()));
        assert_eq!(fold_arith(&op, &env).unwrap(), Value::Bool(true));
    }

    #[test]
    fn fold_arith_select_picks_then_branch_when_cond_true() {
        let mut env = ConstEnv::new();
        env.bind(ValueId(0), Value::Bool(true));
        env.bind(ValueId(1), Value::Int(10, CompIntWidth::I32));
        env.bind(ValueId(2), Value::Int(20, CompIntWidth::I32));
        let mut op = MirOp::std("arith.select");
        op.operands.push(ValueId(0));
        op.operands.push(ValueId(1));
        op.operands.push(ValueId(2));
        op.results
            .push(MirValue::new(ValueId(3), MirType::Int(IntWidth::I32)));
        assert_eq!(
            fold_arith(&op, &env).unwrap(),
            Value::Int(10, CompIntWidth::I32)
        );
    }

    #[test]
    fn fold_arith_select_picks_else_when_cond_false() {
        let mut env = ConstEnv::new();
        env.bind(ValueId(0), Value::Bool(false));
        env.bind(ValueId(1), Value::Int(10, CompIntWidth::I32));
        env.bind(ValueId(2), Value::Int(20, CompIntWidth::I32));
        let mut op = MirOp::std("arith.select");
        op.operands.push(ValueId(0));
        op.operands.push(ValueId(1));
        op.operands.push(ValueId(2));
        op.results
            .push(MirValue::new(ValueId(3), MirType::Int(IntWidth::I32)));
        assert_eq!(
            fold_arith(&op, &env).unwrap(),
            Value::Int(20, CompIntWidth::I32)
        );
    }

    #[test]
    fn fold_arith_unknown_op_returns_none() {
        let env = ConstEnv::new();
        let op = MirOp::std("cssl.gpu.barrier");
        assert!(fold_arith(&op, &env).is_none());
    }

    #[test]
    fn run_const_prop_pass_folds_chain() {
        // build  : %0 = const 3 ; %1 = const 4 ; %2 = addi %0, %1 ; %3 = muli %2, %0
        let mut f = MirFunc::new("foo", vec![], vec![MirType::Int(IntWidth::I32)]);
        f.next_value_id = 4;
        let entry = f.body.entry_mut().unwrap();
        entry.push(mk_int_const(0, 3, IntWidth::I32));
        entry.push(mk_int_const(1, 4, IntWidth::I32));
        entry.push(mk_arith("arith.addi", 0, 1, 2, MirType::Int(IntWidth::I32)));
        entry.push(mk_arith("arith.muli", 2, 0, 3, MirType::Int(IntWidth::I32)));

        let mut env = ConstEnv::new();
        let report = run_const_prop_pass(&mut f, &mut env);
        // 4 ops folded (2 constants + 1 add + 1 mul).
        assert_eq!(report.arith_folds, 4);
        // Final value : (3 + 4) * 3 = 21.
        assert_eq!(
            env.get(ValueId(3)),
            Some(&Value::Int(21, CompIntWidth::I32))
        );
    }

    #[test]
    fn run_const_prop_reaches_fixed_point() {
        // No ops to fold ⇒ exit immediately on iter 1.
        let mut f = MirFunc::new("noop", vec![], vec![]);
        let mut env = ConstEnv::new();
        let report = run_const_prop_pass(&mut f, &mut env);
        assert_eq!(report.arith_folds, 0);
        assert_eq!(report.iterations, 1);
    }

    #[test]
    fn report_total_sums_all_categories() {
        let r = ConstPropReport {
            arith_folds: 3,
            branch_folds: 2,
            cmp_folds: 1,
            iterations: 2,
        };
        assert_eq!(r.total(), 6);
    }

    #[test]
    fn collect_branch_folds_finds_known_cond() {
        let mut f = MirFunc::new("foo", vec![], vec![]);
        let entry = f.body.entry_mut().unwrap();
        let mut if_op = MirOp::std("scf.if");
        if_op.operands.push(ValueId(0));
        entry.push(if_op);

        let mut env = ConstEnv::new();
        env.bind(ValueId(0), Value::Bool(true));

        let folds = collect_branch_folds(&f, &env);
        assert_eq!(folds.len(), 1);
        assert!(folds[0].taken_branch);
    }

    #[test]
    fn collect_branch_folds_skips_runtime_cond() {
        let mut f = MirFunc::new("foo", vec![], vec![]);
        let entry = f.body.entry_mut().unwrap();
        let mut if_op = MirOp::std("scf.if");
        if_op.operands.push(ValueId(0));
        entry.push(if_op);

        let env = ConstEnv::new();
        let folds = collect_branch_folds(&f, &env);
        assert!(folds.is_empty());
    }

    #[test]
    fn collect_branch_folds_recognizes_cssl_if_too() {
        let mut f = MirFunc::new("foo", vec![], vec![]);
        let entry = f.body.entry_mut().unwrap();
        let mut if_op = MirOp::std("cssl.if");
        if_op.operands.push(ValueId(0));
        entry.push(if_op);

        let mut env = ConstEnv::new();
        env.bind(ValueId(0), Value::Bool(false));

        let folds = collect_branch_folds(&f, &env);
        assert_eq!(folds.len(), 1);
        assert!(!folds[0].taken_branch);
    }

    #[test]
    fn run_const_prop_promotes_widths() {
        // i8 + i32 ⇒ result is i32 (wider lane wins).
        let mut f = MirFunc::new("widen", vec![], vec![]);
        f.next_value_id = 3;
        let entry = f.body.entry_mut().unwrap();
        entry.push(mk_int_const(0, 5, IntWidth::I8));
        entry.push(mk_int_const(1, 10, IntWidth::I32));
        entry.push(mk_arith("arith.addi", 0, 1, 2, MirType::Int(IntWidth::I32)));
        let mut env = ConstEnv::new();
        run_const_prop_pass(&mut f, &mut env);
        assert_eq!(
            env.get(ValueId(2)),
            Some(&Value::Int(15, CompIntWidth::I32))
        );
    }
}
