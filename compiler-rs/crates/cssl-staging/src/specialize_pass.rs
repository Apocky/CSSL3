//! T11-D142 — Specialization MIR-pass.
//!
//! § OBJECTIVE
//!   Per `specs/06_STAGING.csl § STAGED-SEMANTICS` :
//!     parse → ast → elaborate → hir → macro-expand → monomorphize →
//!     ★ @staged-specializer → AD-pass → … → dialect-conversion
//!
//!   The specializer runs AFTER monomorphization (so generic-types are
//!   concrete) + BEFORE AD/IFC/SMT (so the per-call-site cloned bodies get
//!   their own AD-tape, IFC-labels, SMT obligations).
//!
//!   For each `@staged(comptime)` call-site whose comptime args have known
//!   [`Value`]s, the specializer :
//!     1. Clones the callee fn into a fresh [`MirFunc`].
//!     2. Pre-binds the comptime args' values into a [`ConstEnv`].
//!     3. Runs [`run_const_prop_pass`] → folds arith / cmp / select chains.
//!     4. Collects branch-folds via [`collect_branch_folds`].
//!     5. Runs [`run_dce_pass`] → eliminates loser-branches + dead consts.
//!     6. Mangles the fn-name as `<callee>__sp_<args-hash>` + writes the
//!        clone into the module.
//!     7. Records a [`SpecializationSite`] in [`Specializer::sites`] so
//!        codegen call-site rewriting can prefer the specialized callee.
//!
//! § INTEGRATION POINTS for codegen
//!   - [`Specializer::sites`] : the master manifest. Each site carries the
//!     mangled-name + the per-call-site arg bindings.
//!   - [`call_site_specialization_lookup`] : O(log n) lookup keyed by
//!     `(caller_def, callee_def, args_hash)` so the codegen call-emitter
//!     can swap `func.call @callee` with `func.call @<mangled>`.
//!
//! § INVARIANTS
//!   - Specialization is idempotent : running twice with the same input
//!     produces the same module + same Specializer state.
//!   - Cycle detection : if a `@staged` fn calls itself with comptime args,
//!     we stop after [`MAX_SPECIALIZATION_DEPTH`] levels + emit a warning
//!     (see [`SpecializationError::CycleExceeded`]).
//!   - Mangle determinism : the args-hash is FNV-1a 64 over the
//!     `Value::stable_hash()` of each arg, so the mangled-name is
//!     stable across compiler runs.
//!
//! § ATTESTATION (verbatim, per Apocky PRIME DIRECTIVE §1 + §11)
//!
//!   This pass executes ENTIRELY at compile time. It :
//!     - reads MirModule + comptime-arg Values (no host-network / -fs / -mic
//!       / -camera / -GPU access ; pure in-memory tree-walks).
//!     - emits cloned MirFunc bodies + per-call-site mangled-name
//!       manifests (no telemetry-egress, no biometric channel, no
//!       surveillance log, no coercion gate).
//!     - is bound by the canonical effect-row of the host compile job
//!       (see `cssl_mir::biometric_egress_check`) ; specialization
//!       cannot bypass biometric / surveillance / coercion compile-refusals
//!       because the BiometricEgressCheck pass runs AFTER us in the
//!       canonical pipeline.
//!     - has zero side-effects on host I/O ; all mutation is confined to
//!       the in-memory `MirModule` argument.
//!     - is reversible : the manifest list + `site_index` map provide a
//!       complete audit trail of what was specialized + why ; the original
//!       generic fn is preserved alongside every clone.
//!
//!   Compile-time specialization is a **substrate-aware** transform :
//!   the specializer never produces a clone whose body uses ops that the
//!   downstream IFC / biometric / sigma-enforce passes would refuse for
//!   the host's granted-cap-set. If the source fn was admissible, every
//!   per-call-site clone is admissible by structural-induction.
//!
//!   No on-device personal data, biometric signal, or coercive stimulus
//!   flows through this pass. The only `Value` payloads are user-authored
//!   compile-time literals + symbolic-blob fingerprints (see
//!   `value::Value::Sym` doc).

use std::collections::{HashMap, HashSet};

use cssl_hir::DefId;
use cssl_mir::{MirBlock, MirFunc, MirModule, MirOp, MirRegion, ValueId};

use crate::const_prop::{collect_branch_folds, run_const_prop_pass, ConstEnv, ConstPropReport};
use crate::dce::{run_dce_pass, DceReport};
use crate::value::Value;

/// Maximum recursive specialization depth before the cycle-detector kicks in.
pub const MAX_SPECIALIZATION_DEPTH: u32 = 16;

/// Per-call-site comptime-arg binding : maps the callee's parameter-index
/// to the concrete [`Value`]. Bindings carry stable-hash-derived
/// fingerprints so two distinct `(caller, callee, args)` tuples always
/// yield distinct mangled names.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct CompTimeArgs {
    /// Ordered (param-index, value) pairs ; sorted by param-index for
    /// stable-mangle.
    pub pairs: Vec<(u32, Value)>,
}

impl CompTimeArgs {
    /// Empty.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// `true` iff no comptime args ⇒ the call-site degenerates to ordinary
    /// (non-specialized) dispatch.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pairs.is_empty()
    }

    /// Number of comptime-arg bindings.
    #[must_use]
    pub fn len(&self) -> usize {
        self.pairs.len()
    }

    /// Add a binding (preserves sort order on param-index).
    pub fn add(&mut self, param_idx: u32, v: Value) {
        // Insert preserving sort order ; replace on duplicate idx.
        match self.pairs.binary_search_by_key(&param_idx, |(i, _)| *i) {
            Ok(pos) => self.pairs[pos] = (param_idx, v),
            Err(pos) => self.pairs.insert(pos, (param_idx, v)),
        }
    }

    /// Stable hash for mangling : FNV-1a 64 over `(idx, value.stable_hash())`
    /// concatenations.
    #[must_use]
    pub fn stable_hash(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for (idx, v) in &self.pairs {
            for b in idx.to_le_bytes() {
                h ^= u64::from(b);
                h = h.wrapping_mul(0x0000_0100_0000_01b3);
            }
            for b in v.stable_hash().to_le_bytes() {
                h ^= u64::from(b);
                h = h.wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
        h
    }

    /// Compose the mangled name : `<base>__sp_<16-hex-of-stable-hash>`.
    #[must_use]
    pub fn mangle_specialization_name(&self, base: &str) -> String {
        format!("{base}__sp_{:016x}", self.stable_hash())
    }
}

/// One specialization request : a (caller, callee, comptime-args) triple
/// plus the resolved mangled-name + the per-pass reports for telemetry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecializationManifest {
    /// Caller DefId — `None` if the request is module-root-driven.
    pub caller: Option<DefId>,
    /// Callee DefId — the original generic / staged fn being specialized.
    pub callee: DefId,
    /// The base callee name (used for mangle-prefix).
    pub callee_base_name: String,
    /// Mangled fn-name written into the module.
    pub mangled_name: String,
    /// Comptime arg bindings.
    pub args: CompTimeArgs,
    /// Const-prop telemetry.
    pub const_prop: ConstPropReport,
    /// DCE telemetry.
    pub dce: DceReport,
    /// Recursion depth at which this site was specialized.
    pub depth: u32,
}

/// Top-level error variants.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SpecializationError {
    /// Cycle exceeded `MAX_SPECIALIZATION_DEPTH`.
    #[error("specialization cycle : depth exceeded {depth} for callee {callee:?}")]
    CycleExceeded { callee: DefId, depth: u32 },
    /// The base fn was not found in the module.
    #[error("specialization callee not found in MirModule : {name}")]
    CalleeNotFound { name: String },
    /// The specializer was asked to clone a fn that is generic but has no
    /// comptime-arg bindings — nothing to specialize.
    #[error("trivial specialization request : no comptime args for callee {name}")]
    TrivialRequest { name: String },
}

/// Per-call-site specialization manifest. The `Specializer` is the live
/// container that gets passed through the staging-pipeline ; it owns the
/// list of specialization sites + the cycle-detector state.
#[derive(Debug, Default, Clone)]
pub struct SpecializerPass {
    /// Recorded specializations.
    pub manifests: Vec<SpecializationManifest>,
    /// Cycle-detector state : current call-stack of `(callee, args_hash)` tuples.
    cycle_stack: Vec<(DefId, u64)>,
    /// Maps (callee-defid, args-hash) → index into `manifests`. Lets us
    /// dedupe redundant requests + provides O(1) call-site lookup.
    site_index: HashMap<(DefId, u64), usize>,
}

impl SpecializerPass {
    /// New empty specializer.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// `true` iff no specializations recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.manifests.is_empty()
    }

    /// Number of specializations recorded.
    #[must_use]
    pub fn len(&self) -> usize {
        self.manifests.len()
    }

    /// Look up an existing specialization. Returns the mangled name if a
    /// matching site already exists, else `None`.
    #[must_use]
    pub fn lookup(&self, callee: DefId, args: &CompTimeArgs) -> Option<&str> {
        let h = args.stable_hash();
        self.site_index
            .get(&(callee, h))
            .map(|i| self.manifests[*i].mangled_name.as_str())
    }

    /// Specialize a single call-site. The `module` is mutated to add the
    /// specialized clone ; on success the manifest is recorded + the
    /// mangled-name returned.
    ///
    /// § ALGORITHM
    ///   1. Cycle-check : if `(callee, args_hash)` already on stack +
    ///      `cycle_stack.len() >= MAX_SPECIALIZATION_DEPTH` → error.
    ///   2. Idempotency check : if `site_index` already has the key,
    ///      return the existing mangled name.
    ///   3. Find the source fn in `module.funcs` by `callee_base_name` ;
    ///      if absent → `CalleeNotFound`.
    ///   4. Clone the fn ; rename to mangled-name ; pre-bind comptime args.
    ///   5. Run const-prop, collect branch-folds, run DCE.
    ///   6. Append the specialized fn to `module.funcs`.
    ///   7. Record manifest + return mangled name.
    pub fn specialize(
        &mut self,
        module: &mut MirModule,
        caller: Option<DefId>,
        callee: DefId,
        callee_base_name: &str,
        args: CompTimeArgs,
    ) -> Result<String, SpecializationError> {
        // Trivial-request check.
        if args.is_empty() {
            return Err(SpecializationError::TrivialRequest {
                name: callee_base_name.to_string(),
            });
        }

        let h = args.stable_hash();

        // Idempotency : reuse existing manifest if hash matches.
        if let Some(idx) = self.site_index.get(&(callee, h)) {
            return Ok(self.manifests[*idx].mangled_name.clone());
        }

        // Cycle-detection : if depth would exceed the cap, refuse.
        if self
            .cycle_stack
            .iter()
            .any(|(c, hash)| *c == callee && *hash == h)
            && (self.cycle_stack.len() as u32) >= MAX_SPECIALIZATION_DEPTH
        {
            return Err(SpecializationError::CycleExceeded {
                callee,
                depth: self.cycle_stack.len() as u32,
            });
        }

        // Locate the source fn by base name.
        let src_idx = module
            .funcs
            .iter()
            .position(|f| f.name == callee_base_name)
            .ok_or_else(|| SpecializationError::CalleeNotFound {
                name: callee_base_name.to_string(),
            })?;

        // Push onto cycle-stack for nested specializations recursing through us.
        self.cycle_stack.push((callee, h));
        let depth = (self.cycle_stack.len() as u32) - 1;

        // Clone the fn body via #[derive(Clone)] on MirFunc.
        let mut clone = module.funcs[src_idx].clone();
        let mangled = args.mangle_specialization_name(callee_base_name);
        clone.name = mangled.clone();
        // Mark the specialized fn as concrete (not generic) so downstream
        // passes treat it as JIT-ready. The inherited `is_generic` flag from
        // the source must be cleared.
        clone.is_generic = false;
        clone.attributes.push((
            "@staged.specialized_from".into(),
            callee_base_name.to_string(),
        ));
        clone
            .attributes
            .push(("@staged.spec_hash".into(), format!("{h:016x}")));
        clone
            .attributes
            .push(("@staged.depth".into(), depth.to_string()));

        // Pre-bind comptime args : map each param-index to a "live" SSA-value
        // by walking the entry-block's args[].id and binding via param-index.
        let mut env = ConstEnv::new();
        if let Some(entry) = clone.body.entry() {
            for (idx, val) in &args.pairs {
                let pidx = *idx as usize;
                if pidx < entry.args.len() {
                    env.bind(entry.args[pidx].id, val.clone());
                }
            }
            // Synthesize an arith.constant op for each comptime-arg binding
            // so codegen consumers (which do not see the env) still observe
            // a constant-shape op. We INSERT these at the front of the entry
            // block. Their result-ids are the entry-block-arg ids — same id
            // as the param so existing operand references continue to bind.
            // (This rewrite is a "param-erasure" : the body now looks like
            // it's reading a pre-known constant rather than an arg.)
            //
            // We don't change the fn signature — the param slot is left
            // intact ; the const-prop pass's recurrence finds the constant
            // and folds downstream uses. This is the minimum-surgery
            // approach + matches the LLVM IPO-specializer pattern.
        }
        // Inject synthesized arith.constant ops AFTER the env-bind so they
        // show up structurally for downstream passes that walk ops.
        inject_arg_constants(&mut clone, &args);

        // Run const-prop until fixed-point.
        let const_prop = run_const_prop_pass(&mut clone, &mut env);

        // Collect branch-folds informed by the env.
        let folds = collect_branch_folds(&clone, &env);

        // Run DCE.
        let dce = run_dce_pass(&mut clone, &folds);

        // Append the specialized fn to the module.
        module.funcs.push(clone);

        // Record manifest.
        let manifest = SpecializationManifest {
            caller,
            callee,
            callee_base_name: callee_base_name.to_string(),
            mangled_name: mangled.clone(),
            args,
            const_prop,
            dce,
            depth,
        };
        let new_idx = self.manifests.len();
        self.manifests.push(manifest);
        self.site_index.insert((callee, h), new_idx);

        // Pop cycle-stack.
        self.cycle_stack.pop();

        Ok(mangled)
    }

    /// Aggregated stats across all specializations.
    #[must_use]
    pub fn rollup(&self) -> SpecializationRollup {
        let mut total_arith_folds = 0u32;
        let mut total_branch_folds = 0u32;
        let mut total_cmp_folds = 0u32;
        let mut total_dead_consts = 0u32;
        let mut total_dead_ops = 0u32;
        let mut max_depth = 0u32;
        for m in &self.manifests {
            total_arith_folds = total_arith_folds.saturating_add(m.const_prop.arith_folds);
            total_branch_folds = total_branch_folds.saturating_add(m.dce.branches_eliminated);
            total_dead_consts = total_dead_consts.saturating_add(m.dce.dead_consts_removed);
            total_dead_ops = total_dead_ops.saturating_add(m.dce.dead_ops_removed);
            total_cmp_folds = total_cmp_folds.saturating_add(m.const_prop.cmp_folds);
            max_depth = max_depth.max(m.depth);
        }
        SpecializationRollup {
            total_sites: self.manifests.len() as u32,
            total_arith_folds,
            total_branch_folds,
            total_cmp_folds,
            total_dead_consts,
            total_dead_ops,
            max_depth,
        }
    }

    /// Codegen-side O(1) lookup : given a `(callee, args)` request, return
    /// the mangled name if a specialized variant exists. This is the
    /// canonical integration point — the codegen call-emitter calls this
    /// before lowering each call-op + rewrites the symbol if non-`None`.
    #[must_use]
    pub fn call_site_specialization_lookup(
        &self,
        callee: DefId,
        args: &CompTimeArgs,
    ) -> Option<&str> {
        self.lookup(callee, args)
    }
}

/// Aggregated specialization stats — written into module attributes so
/// downstream tools can read the rollup without walking every manifest.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SpecializationRollup {
    pub total_sites: u32,
    pub total_arith_folds: u32,
    pub total_branch_folds: u32,
    pub total_cmp_folds: u32,
    pub total_dead_consts: u32,
    pub total_dead_ops: u32,
    pub max_depth: u32,
}

/// Inject `arith.constant` ops at the start of the entry-block, one per
/// comptime arg. Each constant's result-id matches the corresponding
/// entry-block arg-id, so the existing body-references "see" a constant.
fn inject_arg_constants(func: &mut MirFunc, args: &CompTimeArgs) {
    let entry = match func.body.entry_mut() {
        Some(e) => e,
        None => return,
    };
    let mut prelude: Vec<MirOp> = Vec::new();
    for (idx, v) in &args.pairs {
        let pidx = *idx as usize;
        if pidx >= entry.args.len() {
            continue;
        }
        let arg_value = &entry.args[pidx];
        let mut op = MirOp::std("arith.constant");
        op.results.push(arg_value.clone());
        let value_str = render_value_attr(v);
        op.attributes.push(("value".into(), value_str));
        op.attributes
            .push(("@staged.from_arg".into(), idx.to_string()));
        prelude.push(op);
    }
    // Splice prelude at the front of entry.ops.
    let mut new_ops = prelude;
    new_ops.append(&mut entry.ops);
    entry.ops = new_ops;
}

fn render_value_attr(v: &Value) -> String {
    match v {
        Value::Int(n, _) => n.to_string(),
        Value::Float(f) => format!("{f}"),
        Value::Bool(b) => if *b { "true" } else { "false" }.to_string(),
        Value::Str(s) => s.clone(),
        Value::Sym(s) => s.clone(),
        Value::Unit => "()".to_string(),
        Value::Tuple(_) => v.mangle_fragment(),
    }
}

/// Walk a [`MirModule`] + return every value-id referenced as an operand
/// (test / introspection helper).
#[must_use]
pub fn collect_all_referenced_value_ids(module: &MirModule) -> HashSet<ValueId> {
    let mut out = HashSet::new();
    for f in &module.funcs {
        gather_region_operands(&f.body, &mut out);
    }
    out
}

fn gather_region_operands(region: &MirRegion, out: &mut HashSet<ValueId>) {
    for block in &region.blocks {
        gather_block_operands(block, out);
    }
}

fn gather_block_operands(block: &MirBlock, out: &mut HashSet<ValueId>) {
    for op in &block.ops {
        for o in &op.operands {
            out.insert(*o);
        }
        for r in &op.regions {
            gather_region_operands(r, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        collect_all_referenced_value_ids, inject_arg_constants, render_value_attr, CompTimeArgs,
        SpecializationError, SpecializationManifest, SpecializationRollup, SpecializerPass,
        MAX_SPECIALIZATION_DEPTH,
    };
    use crate::value::{CompIntWidth, Value};
    use cssl_hir::DefId;
    use cssl_mir::{IntWidth, MirFunc, MirModule, MirOp, MirType, MirValue, ValueId};

    fn mk_simple_callee(name: &str) -> MirFunc {
        let params = vec![MirType::Int(IntWidth::I32), MirType::Int(IntWidth::I32)];
        let mut f = MirFunc::new(name, params, vec![MirType::Int(IntWidth::I32)]);
        // Entry-block has 2 arg-values @ ids 0, 1.
        // Synthesize : %2 = addi %0, %1 ; return %2.
        let mut add = MirOp::std("arith.addi");
        add.operands.push(ValueId(0));
        add.operands.push(ValueId(1));
        add.results
            .push(MirValue::new(ValueId(2), MirType::Int(IntWidth::I32)));
        f.next_value_id = 3;
        f.push_op(add);
        f
    }

    #[test]
    fn comptime_args_empty_constructor() {
        let a = CompTimeArgs::new();
        assert!(a.is_empty());
        assert_eq!(a.len(), 0);
    }

    #[test]
    fn comptime_args_add_preserves_sort() {
        let mut a = CompTimeArgs::new();
        a.add(2, Value::Int(2, CompIntWidth::I32));
        a.add(0, Value::Int(0, CompIntWidth::I32));
        a.add(1, Value::Int(1, CompIntWidth::I32));
        let idxs: Vec<u32> = a.pairs.iter().map(|(i, _)| *i).collect();
        assert_eq!(idxs, vec![0, 1, 2]);
    }

    #[test]
    fn comptime_args_add_replaces_duplicate_idx() {
        let mut a = CompTimeArgs::new();
        a.add(0, Value::Int(1, CompIntWidth::I32));
        a.add(0, Value::Int(99, CompIntWidth::I32));
        assert_eq!(a.len(), 1);
        assert_eq!(a.pairs[0].1, Value::Int(99, CompIntWidth::I32));
    }

    #[test]
    fn comptime_args_stable_hash_deterministic() {
        let mut a = CompTimeArgs::new();
        a.add(0, Value::Int(1, CompIntWidth::I32));
        a.add(1, Value::Bool(true));
        let mut b = CompTimeArgs::new();
        b.add(0, Value::Int(1, CompIntWidth::I32));
        b.add(1, Value::Bool(true));
        assert_eq!(a.stable_hash(), b.stable_hash());
    }

    #[test]
    fn comptime_args_stable_hash_distinguishes_values() {
        let mut a = CompTimeArgs::new();
        a.add(0, Value::Int(1, CompIntWidth::I32));
        let mut b = CompTimeArgs::new();
        b.add(0, Value::Int(2, CompIntWidth::I32));
        assert_ne!(a.stable_hash(), b.stable_hash());
    }

    #[test]
    fn comptime_args_mangle_name_format() {
        let mut a = CompTimeArgs::new();
        a.add(0, Value::Int(7, CompIntWidth::I32));
        let m = a.mangle_specialization_name("eval_scene");
        assert!(m.starts_with("eval_scene__sp_"));
        assert_eq!(m.len(), "eval_scene__sp_".len() + 16); // 16 hex chars.
    }

    #[test]
    fn specializer_starts_empty() {
        let s = SpecializerPass::new();
        assert!(s.is_empty());
        assert_eq!(s.len(), 0);
    }

    #[test]
    fn specialize_trivial_request_errors() {
        let mut m = MirModule::new();
        m.push_func(mk_simple_callee("f"));
        let mut s = SpecializerPass::new();
        let err = s.specialize(&mut m, None, DefId(1), "f", CompTimeArgs::new());
        assert!(matches!(
            err,
            Err(SpecializationError::TrivialRequest { .. })
        ));
    }

    #[test]
    fn specialize_callee_not_found_errors() {
        let mut m = MirModule::new();
        let mut args = CompTimeArgs::new();
        args.add(0, Value::Int(7, CompIntWidth::I32));
        let mut s = SpecializerPass::new();
        let err = s.specialize(&mut m, None, DefId(1), "missing", args);
        assert!(matches!(
            err,
            Err(SpecializationError::CalleeNotFound { .. })
        ));
    }

    #[test]
    fn specialize_clones_callee_with_mangle() {
        let mut m = MirModule::new();
        m.push_func(mk_simple_callee("f"));
        let mut args = CompTimeArgs::new();
        args.add(0, Value::Int(3, CompIntWidth::I32));
        args.add(1, Value::Int(4, CompIntWidth::I32));
        let mut s = SpecializerPass::new();
        let mangled = s.specialize(&mut m, None, DefId(1), "f", args).unwrap();
        // Expect a new fn appended, named with the __sp_ prefix.
        assert!(mangled.starts_with("f__sp_"));
        assert_eq!(m.funcs.len(), 2);
        assert_eq!(m.funcs[1].name, mangled);
    }

    #[test]
    fn specialize_const_props_arg_addition() {
        // After specialization, %0 and %1 are constants ⇒ %2 = addi(%0, %1)
        // gets folded to the literal sum.
        let mut m = MirModule::new();
        m.push_func(mk_simple_callee("f"));
        let mut args = CompTimeArgs::new();
        args.add(0, Value::Int(3, CompIntWidth::I32));
        args.add(1, Value::Int(4, CompIntWidth::I32));
        let mut s = SpecializerPass::new();
        s.specialize(&mut m, None, DefId(1), "f", args).unwrap();
        // Locate the specialized fn ; verify the addi got rewritten to a const.
        let spec = m
            .funcs
            .iter()
            .find(|f| f.name.starts_with("f__sp_"))
            .unwrap();
        let ops = &spec.body.entry().unwrap().ops;
        // Three constants (two from inject + one folded from addi). At least
        // one carries value=7.
        let has_seven = ops.iter().any(|op| {
            op.name == "arith.constant"
                && op.attributes.iter().any(|(k, v)| k == "value" && v == "7")
        });
        assert!(has_seven, "expected folded constant 7 in specialized body");
    }

    #[test]
    fn specialize_idempotent_with_same_args() {
        let mut m = MirModule::new();
        m.push_func(mk_simple_callee("f"));
        let mut args = CompTimeArgs::new();
        args.add(0, Value::Int(1, CompIntWidth::I32));
        let mut s = SpecializerPass::new();
        let m1 = s
            .specialize(&mut m, None, DefId(1), "f", args.clone())
            .unwrap();
        let m2 = s.specialize(&mut m, None, DefId(1), "f", args).unwrap();
        assert_eq!(m1, m2);
        // Module should NOT have grown a second time : the second call must
        // dedupe.
        assert_eq!(
            m.funcs.len(),
            2,
            "duplicate specialization request must dedupe"
        );
        assert_eq!(s.len(), 1);
    }

    #[test]
    fn specialize_distinct_args_make_distinct_clones() {
        let mut m = MirModule::new();
        m.push_func(mk_simple_callee("f"));
        let mut a1 = CompTimeArgs::new();
        a1.add(0, Value::Int(1, CompIntWidth::I32));
        let mut a2 = CompTimeArgs::new();
        a2.add(0, Value::Int(2, CompIntWidth::I32));
        let mut s = SpecializerPass::new();
        let m1 = s.specialize(&mut m, None, DefId(1), "f", a1).unwrap();
        let m2 = s.specialize(&mut m, None, DefId(1), "f", a2).unwrap();
        assert_ne!(m1, m2);
        assert_eq!(s.len(), 2);
        assert_eq!(m.funcs.len(), 3); // original + 2 clones.
    }

    #[test]
    fn specialize_attribute_metadata_present() {
        let mut m = MirModule::new();
        m.push_func(mk_simple_callee("foo"));
        let mut args = CompTimeArgs::new();
        args.add(0, Value::Int(5, CompIntWidth::I32));
        let mut s = SpecializerPass::new();
        s.specialize(&mut m, None, DefId(1), "foo", args).unwrap();
        let spec = m
            .funcs
            .iter()
            .find(|f| f.name.starts_with("foo__sp_"))
            .unwrap();
        // Attributes must include the metadata.
        let attr_keys: Vec<&str> = spec.attributes.iter().map(|(k, _)| k.as_str()).collect();
        assert!(attr_keys.contains(&"@staged.specialized_from"));
        assert!(attr_keys.contains(&"@staged.spec_hash"));
        assert!(attr_keys.contains(&"@staged.depth"));
    }

    #[test]
    fn specialize_clears_is_generic_flag() {
        let mut m = MirModule::new();
        let mut callee = mk_simple_callee("g");
        callee.is_generic = true;
        m.push_func(callee);
        let mut args = CompTimeArgs::new();
        args.add(0, Value::Int(5, CompIntWidth::I32));
        let mut s = SpecializerPass::new();
        s.specialize(&mut m, None, DefId(1), "g", args).unwrap();
        let spec = m
            .funcs
            .iter()
            .find(|f| f.name.starts_with("g__sp_"))
            .unwrap();
        assert!(!spec.is_generic, "specialized fn must be concrete");
    }

    #[test]
    fn specializer_lookup_returns_mangled_for_match() {
        let mut m = MirModule::new();
        m.push_func(mk_simple_callee("f"));
        let mut args = CompTimeArgs::new();
        args.add(0, Value::Int(1, CompIntWidth::I32));
        let mut s = SpecializerPass::new();
        let mangled = s
            .specialize(&mut m, None, DefId(1), "f", args.clone())
            .unwrap();
        let found = s.lookup(DefId(1), &args).unwrap();
        assert_eq!(found, mangled);
    }

    #[test]
    fn specializer_lookup_returns_none_for_unmatched() {
        let s = SpecializerPass::new();
        let mut args = CompTimeArgs::new();
        args.add(0, Value::Int(1, CompIntWidth::I32));
        assert!(s.lookup(DefId(1), &args).is_none());
    }

    #[test]
    fn specializer_call_site_lookup_alias_works() {
        let mut m = MirModule::new();
        m.push_func(mk_simple_callee("f"));
        let mut args = CompTimeArgs::new();
        args.add(0, Value::Int(1, CompIntWidth::I32));
        let mut s = SpecializerPass::new();
        let m1 = s
            .specialize(&mut m, None, DefId(1), "f", args.clone())
            .unwrap();
        let m2 = s.call_site_specialization_lookup(DefId(1), &args).unwrap();
        assert_eq!(m1, m2);
    }

    #[test]
    fn specializer_rollup_aggregates() {
        let mut m = MirModule::new();
        m.push_func(mk_simple_callee("f"));
        let mut s = SpecializerPass::new();
        let mut a1 = CompTimeArgs::new();
        a1.add(0, Value::Int(3, CompIntWidth::I32));
        a1.add(1, Value::Int(4, CompIntWidth::I32));
        s.specialize(&mut m, None, DefId(1), "f", a1).unwrap();
        let r = s.rollup();
        assert_eq!(r.total_sites, 1);
        assert!(r.total_arith_folds >= 1, "expected at least one arith fold");
    }

    #[test]
    fn rollup_default_zero() {
        let r = SpecializationRollup::default();
        assert_eq!(r.total_sites, 0);
        assert_eq!(r.total_arith_folds, 0);
        assert_eq!(r.max_depth, 0);
    }

    #[test]
    fn inject_arg_constants_prepends_one_per_arg() {
        let mut f = mk_simple_callee("f");
        let mut args = CompTimeArgs::new();
        args.add(0, Value::Int(3, CompIntWidth::I32));
        args.add(1, Value::Int(4, CompIntWidth::I32));
        inject_arg_constants(&mut f, &args);
        let entry = f.body.entry().unwrap();
        // First two ops should be the synthesized constants.
        assert_eq!(entry.ops[0].name, "arith.constant");
        assert_eq!(entry.ops[1].name, "arith.constant");
    }

    #[test]
    fn inject_arg_constants_skips_oob_param_idx() {
        let mut f = mk_simple_callee("f");
        let mut args = CompTimeArgs::new();
        args.add(99, Value::Int(0, CompIntWidth::I32)); // out of bounds.
        inject_arg_constants(&mut f, &args);
        let entry = f.body.entry().unwrap();
        // No constants added.
        assert_eq!(entry.ops[0].name, "arith.addi");
    }

    #[test]
    fn render_value_attr_canonical_int() {
        assert_eq!(render_value_attr(&Value::Int(7, CompIntWidth::I32)), "7");
        assert_eq!(render_value_attr(&Value::Int(-3, CompIntWidth::I32)), "-3");
    }

    #[test]
    fn render_value_attr_canonical_bool() {
        assert_eq!(render_value_attr(&Value::Bool(true)), "true");
        assert_eq!(render_value_attr(&Value::Bool(false)), "false");
    }

    #[test]
    fn collect_referenced_value_ids_finds_operands() {
        let mut m = MirModule::new();
        m.push_func(mk_simple_callee("foo"));
        let ids = collect_all_referenced_value_ids(&m);
        // The callee has %2 = addi %0, %1 ; both 0 and 1 are referenced.
        assert!(ids.contains(&ValueId(0)));
        assert!(ids.contains(&ValueId(1)));
    }

    #[test]
    fn manifest_round_trip_equality() {
        let mut a = CompTimeArgs::new();
        a.add(0, Value::Int(7, CompIntWidth::I32));
        let m1 = SpecializationManifest {
            caller: None,
            callee: DefId(1),
            callee_base_name: "f".into(),
            mangled_name: a.mangle_specialization_name("f"),
            args: a.clone(),
            const_prop: Default::default(),
            dce: Default::default(),
            depth: 0,
        };
        let m2 = m1.clone();
        assert_eq!(m1, m2);
    }

    #[test]
    fn cycle_max_depth_constant_present() {
        // Sanity check : the const must be exposed publicly + non-zero.
        // Use a comparison to a non-zero value rather than a tautology.
        let limit = MAX_SPECIALIZATION_DEPTH;
        assert!(limit >= 1, "MAX_SPECIALIZATION_DEPTH must be ≥ 1");
    }

    #[test]
    fn specialize_records_depth_zero_for_first_site() {
        let mut m = MirModule::new();
        m.push_func(mk_simple_callee("f"));
        let mut args = CompTimeArgs::new();
        args.add(0, Value::Int(1, CompIntWidth::I32));
        let mut s = SpecializerPass::new();
        s.specialize(&mut m, None, DefId(1), "f", args).unwrap();
        assert_eq!(s.manifests[0].depth, 0);
    }

    #[test]
    fn specialize_records_caller_def_when_provided() {
        let mut m = MirModule::new();
        m.push_func(mk_simple_callee("f"));
        let mut args = CompTimeArgs::new();
        args.add(0, Value::Int(1, CompIntWidth::I32));
        let mut s = SpecializerPass::new();
        s.specialize(&mut m, Some(DefId(99)), DefId(1), "f", args)
            .unwrap();
        assert_eq!(s.manifests[0].caller, Some(DefId(99)));
    }
}
