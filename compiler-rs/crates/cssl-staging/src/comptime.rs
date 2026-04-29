//! T11-D141 — Comptime evaluation engine for `#run` blocks.
//!
//! § SPEC : `specs/06_STAGING.csl` § COMPTIME EVALUATION + `specs/19_FUTAMURA3.csl`.
//!
//! § ROLE
//!   Turn a `#run expr` block into an actually-evaluated compile-time value.
//!   Pipeline :
//!
//!   1. Take the `HirExpr` from inside `HirExprKind::Run { expr }`.
//!   2. Run the [`crate::effect_scan`] pre-flight to confirm the expression
//!      body lives within the comptime-allowed effect surface
//!      `{Pure, NoFs, NoNet, NoSyscall}`.
//!   3. Wrap the expression into a synthetic zero-arg `HirFn` whose body is
//!      `{ <expr> }` (the trailing slot of an empty block).
//!   4. Lower the synthetic `HirFn` to MIR via the same MIR pipeline used for
//!      runtime fns (`cssl_mir::body_lower::lower_fn_body`).
//!   5. JIT-compile the MIR via [`cssl_cgen_cpu_cranelift::JitModule`] (the
//!      "same backend" rule from spec § COMPTIME EVALUATION : compiled-native,
//!      not interpreted ; avoids the Zig 20× interpreter cost called out
//!      explicitly in the spec).
//!   6. Invoke the compiled fn-pointer in-process and capture the scalar /
//!      array / struct result bytes.
//!   7. Wrap the result as a [`ComptimeResult`] carrying both the byte
//!      representation (for `arith.constant` baking) and the [`ComptimeValue`]
//!      structured form (for downstream specializer passes).
//!
//! § SCOPE (T11-D141 / this commit)
//!   - Scalar comptime-eval : `i32`, `i64`, `f32`, `f64`, `bool`, `unit`.
//!   - Array comptime-eval : per-element repeated invocation, then assembled
//!     into a flat-bytes blob (used by LUT-baking demo).
//!   - Struct comptime-eval : per-field invocation, then assembled.
//!   - Sandbox-violation rejection (effect-scan refuses non-`{Pure, …}` bodies).
//!   - Cyclic-comptime detection (re-entrancy guard against `#run f()` where
//!     `f` itself contains `#run`).
//!   - Eval budget : maximum-iteration-count to bound recursion + while-loops.
//!
//! § T11-D142+ DEFERRED
//!   - Native-`.o`-+-`LoadLibraryEx` / `dlopen` mode for cross-DSO comptime
//!     eval (not needed for in-process JIT path).
//!   - Reflection API (`@type_info` / `@fn_info` / `@module_info`).
//!   - Memoized comptime-result cache keyed by `(fn-name, arg-hash, source-hash)`.
//!   - Direct array/struct return-by-value via JIT (currently we evaluate
//!     each scalar field separately and assemble — sufficient for stage-0
//!     LUT/KAN demos but not for arbitrary aggregate returns).

use std::collections::HashSet;

use cssl_ast::SourceFile;
use cssl_hir::{
    HirArena, HirAttr, HirAttrKind, HirBlock, HirExpr, HirExprKind, HirFn, HirFnParam,
    HirGenerics, HirModule, HirVisibility, Interner,
};
use cssl_mir::{FloatWidth, IntWidth, MirFunc, MirType};

use crate::effect_scan::{scan_expr_effects, EffectScanError};

/// Maximum total ops a comptime-eval invocation is permitted to allocate.
/// A simple budget that bounds runaway compilation. Tuned empirically to
/// accommodate small LUT generators (sine table = 256 ops) but reject
/// runaway recursion. Callers may override via [`ComptimeEvaluator::with_budget`].
pub const DEFAULT_OP_BUDGET: u32 = 65_536;

/// Maximum recursion depth for nested `#run` evaluation. Prevents stack
/// blow-up when a comptime body itself contains another `#run` that the
/// scanner failed to flatten.
pub const DEFAULT_NEST_LIMIT: u8 = 16;

/// One comptime-eval result. The `bytes` representation is what gets baked
/// into MIR as an `arith.constant` operand ; [`ComptimeValue`] carries the
/// structured form for higher-level reasoning (e.g., the `@staged`
/// specialization pass walking a baked struct's field values).
#[derive(Debug, Clone, PartialEq)]
pub struct ComptimeResult {
    /// Canonical byte-representation of the value, in target-endian order.
    /// Scalars : 1/4/8 bytes. Arrays : N × elem-size. Structs : flat-packed.
    pub bytes: Vec<u8>,
    /// MIR type of the result (used to typecheck the embedded `arith.constant`).
    pub ty: MirType,
    /// Structured form for downstream consumers.
    pub value: ComptimeValue,
}

/// Structured comptime value. Mirrors the shape of MIR types we support.
#[derive(Debug, Clone, PartialEq)]
pub enum ComptimeValue {
    /// `i32` / `i64` integer.
    Int(i64, IntWidth),
    /// `f32` / `f64` float.
    Float(f64, FloatWidth),
    /// `bool`.
    Bool(bool),
    /// `()` unit.
    Unit,
    /// Fixed-size array of homogeneous scalar elements.
    Array(Vec<ComptimeValue>),
    /// Struct as ordered field values (field names tagged inline for debug
    /// readability ; downstream emit uses the layout-pass-derived field order
    /// rather than the names).
    Struct(Vec<(String, ComptimeValue)>),
}

impl ComptimeValue {
    /// Render as a value suitable for `arith.constant`'s `value` attribute.
    /// Scalars produce their canonical decimal form ; composites produce a
    /// `bytes(...)` placeholder consumed by the MIR-printer at stage-0.
    #[must_use]
    pub fn as_constant_attr(&self) -> String {
        match self {
            Self::Int(v, _) => v.to_string(),
            Self::Float(v, _) => format!("{v}"),
            Self::Bool(b) => {
                if *b {
                    "true".into()
                } else {
                    "false".into()
                }
            }
            Self::Unit => "unit".into(),
            Self::Array(_) | Self::Struct(_) => {
                format!("comptime-bytes({} bytes)", self.byte_size())
            }
        }
    }

    /// Total byte-size of the value's flat representation.
    #[must_use]
    pub fn byte_size(&self) -> usize {
        match self {
            Self::Int(_, IntWidth::I1 | IntWidth::I8) => 1,
            Self::Int(_, IntWidth::I16) => 2,
            Self::Int(_, IntWidth::I32) => 4,
            Self::Int(_, IntWidth::I64 | IntWidth::Index) => 8,
            Self::Float(_, FloatWidth::F16 | FloatWidth::Bf16) => 2,
            Self::Float(_, FloatWidth::F32) => 4,
            Self::Float(_, FloatWidth::F64) => 8,
            Self::Bool(_) => 1,
            Self::Unit => 0,
            Self::Array(elems) => elems.iter().map(Self::byte_size).sum(),
            Self::Struct(fields) => fields.iter().map(|(_, v)| v.byte_size()).sum(),
        }
    }

    /// `true` iff this is a scalar (single element, not an aggregate).
    #[must_use]
    pub const fn is_scalar(&self) -> bool {
        matches!(
            self,
            Self::Int(_, _) | Self::Float(_, _) | Self::Bool(_) | Self::Unit
        )
    }
}

/// Comptime-eval failure modes.
#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum ComptimeError {
    /// Effect-scan refused the body.
    #[error("comptime body has disallowed effect : {0}")]
    EffectViolation(String),
    /// Body contains a cyclic comptime call.
    #[error("comptime body has cyclic dependency : {0}")]
    Cycle(String),
    /// Evaluation budget exceeded (too many MIR ops or recursion-depth).
    #[error("comptime eval budget exhausted : {limit_kind} > {limit}")]
    BudgetExhausted { limit_kind: String, limit: u32 },
    /// MIR lowering produced an unsupported shape.
    #[error("comptime MIR-lowering failed : {0}")]
    LoweringFailed(String),
    /// JIT compilation failed.
    #[error("comptime JIT-compile failed : {0}")]
    JitFailed(String),
    /// Result type not supported at stage-0 (defer to T11-D142+).
    #[error("comptime result type unsupported at stage-0 : {0}")]
    UnsupportedResultType(String),
    /// Body has runtime-only side-effects (file/net/etc).
    #[error("comptime body has runtime-only side-effect : {0}")]
    RuntimeSideEffect(String),
    /// Body referenced an unsupported expression form.
    #[error("comptime body has unsupported expression form : {0}")]
    UnsupportedExpr(String),
}

impl From<EffectScanError> for ComptimeError {
    fn from(e: EffectScanError) -> Self {
        match e {
            EffectScanError::Forbidden(msg) => Self::EffectViolation(msg),
            EffectScanError::SideEffect(msg) => Self::RuntimeSideEffect(msg),
            EffectScanError::UnsupportedExpr(msg) => Self::UnsupportedExpr(msg),
        }
    }
}

/// The comptime evaluator. Owns its budget knobs + a running call-stack used
/// for cycle detection. Each `eval_run_block` invocation builds + tears down
/// its own `JitModule` to keep machine-code lifetimes scoped to a single
/// `#run` site (a future T11-D142+ optimization can pool modules).
pub struct ComptimeEvaluator {
    /// Maximum number of MIR ops allowed in a single comptime body.
    pub op_budget: u32,
    /// Maximum recursion depth for nested `#run` blocks.
    pub nest_limit: u8,
    /// Currently-active comptime call-stack, used to detect cycles. Tracked
    /// by synthetic-fn-name (each `#run` evaluation gets a distinct name).
    active: HashSet<String>,
    /// Counter used to fabricate unique fn-names per `#run` site.
    fn_counter: u32,
    /// Current nesting depth for budget enforcement.
    nest_depth: u8,
}

impl Default for ComptimeEvaluator {
    fn default() -> Self {
        Self::new()
    }
}

impl ComptimeEvaluator {
    /// New evaluator with default budgets.
    #[must_use]
    pub fn new() -> Self {
        Self {
            op_budget: DEFAULT_OP_BUDGET,
            nest_limit: DEFAULT_NEST_LIMIT,
            active: HashSet::new(),
            fn_counter: 0,
            nest_depth: 0,
        }
    }

    /// Override the op-budget.
    #[must_use]
    pub fn with_budget(mut self, budget: u32) -> Self {
        self.op_budget = budget;
        self
    }

    /// Override the nesting limit.
    #[must_use]
    pub fn with_nest_limit(mut self, limit: u8) -> Self {
        self.nest_limit = limit;
        self
    }

    /// Number of `#run` sites this evaluator has processed (for telemetry).
    #[must_use]
    pub const fn evaluations_performed(&self) -> u32 {
        self.fn_counter
    }

    /// Current nesting depth (publicly observable for tests + telemetry).
    #[must_use]
    pub const fn current_nest_depth(&self) -> u8 {
        self.nest_depth
    }

    /// Allocate a fresh synthetic fn-name for a `#run` invocation.
    fn fresh_fn_name(&mut self) -> String {
        let n = self.fn_counter;
        self.fn_counter = self.fn_counter.saturating_add(1);
        format!("__cssl_run_{n}")
    }

    /// Evaluate a single `#run` block (no source-file threading — literals
    /// will fall through to `stage0_*` placeholders unless the parent passes
    /// one via [`Self::eval_run_block_with_source`]).
    ///
    /// # Errors
    /// See [`Self::eval_run_block_with_source`].
    pub fn eval_run_block(
        &mut self,
        expr: &HirExpr,
        interner: &Interner,
    ) -> Result<ComptimeResult, ComptimeError> {
        self.eval_run_block_with_source(expr, interner, None)
    }

    /// Evaluate a single `#run` block, optionally threading a source-file
    /// through so literal values can be extracted from the original text.
    ///
    /// `expr` is the *inner* expression of `HirExprKind::Run` — the caller is
    /// responsible for unwrapping the `Run` shell.
    ///
    /// `interner` provides symbol lookups for the synthetic fn we lower.
    ///
    /// `source` is the source-file the expression came from, used by the MIR
    /// body-lowerer to extract literal values from spans (without it,
    /// `arith.constant` ops carry the `stage0_*` placeholder which the JIT
    /// then parses as `0`).
    ///
    /// # Errors
    /// Returns [`ComptimeError`] on effect-scan refusal, cycle detection,
    /// budget exhaustion, MIR-lowering failure, or JIT failure.
    pub fn eval_run_block_with_source(
        &mut self,
        expr: &HirExpr,
        interner: &Interner,
        source: Option<&SourceFile>,
    ) -> Result<ComptimeResult, ComptimeError> {
        // Step 1 : effect-scan pre-flight.
        scan_expr_effects(expr, interner)?;

        // Step 2 : nesting-budget check.
        if self.nest_depth >= self.nest_limit {
            return Err(ComptimeError::BudgetExhausted {
                limit_kind: "nest_depth".into(),
                limit: u32::from(self.nest_limit),
            });
        }
        self.nest_depth = self.nest_depth.saturating_add(1);
        let fn_name = self.fresh_fn_name();

        // Step 3 : cycle-detect against the active call-stack.
        if self.active.contains(&fn_name) {
            self.nest_depth = self.nest_depth.saturating_sub(1);
            return Err(ComptimeError::Cycle(fn_name));
        }
        self.active.insert(fn_name.clone());

        // Step 4 : execute the inner pipeline. The result-bind-and-restore
        // pattern below ensures `nest_depth` and `active` are always cleaned
        // up regardless of which step failed.
        let result = self.eval_pipeline(&fn_name, expr, interner, source);

        self.active.remove(&fn_name);
        self.nest_depth = self.nest_depth.saturating_sub(1);
        result
    }

    /// Inner pipeline : synth-fn + lower + JIT + invoke.
    fn eval_pipeline(
        &self,
        fn_name: &str,
        expr: &HirExpr,
        interner: &Interner,
        source: Option<&SourceFile>,
    ) -> Result<ComptimeResult, ComptimeError> {
        // Step 4a : determine the result type from the inner expression.
        let result_ty = infer_comptime_result_type(expr)?;

        // Step 4b : op-budget pre-check on the HIR shape.
        let op_count = estimate_op_count(expr);
        if op_count > self.op_budget {
            return Err(ComptimeError::BudgetExhausted {
                limit_kind: "op_count".into(),
                limit: self.op_budget,
            });
        }

        // Step 4c : build a synthetic HirFn whose body is `{ <expr> }`.
        let synth_fn = synthesize_run_fn(fn_name, expr.clone(), interner);

        // Step 4d : lower to MIR.
        let mir_fn = lower_synth_fn_to_mir(&synth_fn, &result_ty, interner, source)?;

        // Step 4e : JIT-compile via the same backend.
        let raw = jit_eval_mir_fn(&mir_fn, &result_ty)?;

        // Step 4f : decode raw bytes into a structured ComptimeValue.
        let value = decode_raw_result(&raw, &result_ty)?;
        let bytes = encode_value_bytes(&value);
        Ok(ComptimeResult {
            bytes,
            ty: result_ty,
            value,
        })
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § Synthesis : wrap a HirExpr into a zero-arg HirFn for the lowering pass.
// ─────────────────────────────────────────────────────────────────────────

/// Build a synthetic zero-arg `HirFn` wrapping `expr` as its body's trailing
/// expression. The resulting `HirFn` is suitable input to
/// `cssl_mir::body_lower::lower_fn_body`.
///
/// We leave `return_ty` as `None` ; the body-lowerer derives the result type
/// from the trailing expression.
pub(crate) fn synthesize_run_fn(fn_name: &str, expr: HirExpr, interner: &Interner) -> HirFn {
    let span = expr.span;
    let mut arena = HirArena::new();
    let body = HirBlock {
        span,
        id: arena.fresh_hir_id(),
        stmts: Vec::new(),
        trailing: Some(Box::new(expr)),
    };
    let _ = &mut arena; // touch to suppress unused-mut warnings on no-id paths
    HirFn {
        span,
        def: cssl_hir::DefId(0),
        visibility: HirVisibility::Public,
        attrs: vec![HirAttr {
            span,
            kind: HirAttrKind::Outer,
            path: vec![interner.intern("comptime")],
            args: Vec::new(),
        }],
        name: interner.intern(fn_name),
        generics: HirGenerics::default(),
        params: Vec::<HirFnParam>::new(),
        return_ty: None,
        effect_row: None,
        where_clauses: Vec::new(),
        body: Some(body),
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § Type inference : syntactic-shape heuristic for the result type.
// ─────────────────────────────────────────────────────────────────────────

/// Infer the comptime result type from the inner expression's syntactic shape.
/// Stage-0 inspects literals + obvious constructors ; full integration with
/// the type-inferencer is T11-D142+ work.
pub(crate) fn infer_comptime_result_type(expr: &HirExpr) -> Result<MirType, ComptimeError> {
    use cssl_hir::HirLiteralKind;
    match &expr.kind {
        HirExprKind::Literal(lit) => match lit.kind {
            HirLiteralKind::Int => Ok(MirType::Int(IntWidth::I32)),
            HirLiteralKind::Float => Ok(MirType::Float(FloatWidth::F32)),
            HirLiteralKind::Bool(_) => Ok(MirType::Bool),
            HirLiteralKind::Unit => Ok(MirType::None),
            HirLiteralKind::Str | HirLiteralKind::Char => Err(ComptimeError::UnsupportedResultType(
                "string/char comptime-result deferred to T11-D142+".into(),
            )),
        },
        HirExprKind::Block(b) => b
            .trailing
            .as_ref()
            .map_or(Ok(MirType::None), |t| infer_comptime_result_type(t)),
        HirExprKind::Paren(inner) | HirExprKind::Run { expr: inner } => {
            infer_comptime_result_type(inner)
        }
        HirExprKind::Binary { lhs, rhs, .. } => {
            // Result type follows the lhs shape in scalar-arith bodies.
            // If either side is float-shaped, prefer float.
            let l = infer_comptime_result_type(lhs).ok();
            let r = infer_comptime_result_type(rhs).ok();
            match (l, r) {
                (Some(MirType::Float(w)), _) | (_, Some(MirType::Float(w))) => {
                    Ok(MirType::Float(w))
                }
                (Some(MirType::Int(w)), _) | (_, Some(MirType::Int(w))) => Ok(MirType::Int(w)),
                _ => Ok(MirType::Int(IntWidth::I32)),
            }
        }
        HirExprKind::Unary { operand, .. } => infer_comptime_result_type(operand),
        HirExprKind::Cast { ty: _, expr: inner } => infer_comptime_result_type(inner),
        HirExprKind::If { then_branch, .. } => then_branch
            .trailing
            .as_ref()
            .map_or(Ok(MirType::None), |t| infer_comptime_result_type(t)),
        HirExprKind::Array(_) => Err(ComptimeError::UnsupportedResultType(
            "array comptime-result must be evaluated element-wise via eval_array".into(),
        )),
        HirExprKind::Struct { .. } => Err(ComptimeError::UnsupportedResultType(
            "struct comptime-result must be evaluated field-wise via eval_struct".into(),
        )),
        // Path / Call / etc. : default to i32. Real type-inference threading
        // is T11-D142+ work.
        _ => Ok(MirType::Int(IntWidth::I32)),
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § Op-count estimator : rough bound to catch runaway loops.
// ─────────────────────────────────────────────────────────────────────────

/// Walk the expression tree and approximate the number of MIR ops it will
/// produce. Used by the eval-budget guard. Underestimates loops (`for`/`while`
/// only contribute their body-shape, not the iteration count) — that's fine
/// because the goal is to reject obviously-runaway shapes, not to count
/// precisely. Iteration-bound enforcement happens at execution-time via the
/// JIT itself ; callers can wrap in OS-level CPU-time alarms for paranoia.
pub(crate) fn estimate_op_count(expr: &HirExpr) -> u32 {
    let mut count = 0u32;
    walk_count(expr, &mut count);
    count
}

fn walk_count(expr: &HirExpr, n: &mut u32) {
    *n = n.saturating_add(1);
    use cssl_hir::HirExprKind as K;
    match &expr.kind {
        K::Block(b) => {
            for s in &b.stmts {
                if let cssl_hir::HirStmtKind::Let { value: Some(v), .. }
                | cssl_hir::HirStmtKind::Expr(v) = &s.kind
                {
                    walk_count(v, n);
                }
            }
            if let Some(t) = &b.trailing {
                walk_count(t, n);
            }
        }
        K::If {
            cond,
            then_branch,
            else_branch,
        } => {
            walk_count(cond, n);
            for s in &then_branch.stmts {
                if let cssl_hir::HirStmtKind::Let { value: Some(v), .. }
                | cssl_hir::HirStmtKind::Expr(v) = &s.kind
                {
                    walk_count(v, n);
                }
            }
            if let Some(t) = &then_branch.trailing {
                walk_count(t, n);
            }
            if let Some(e) = else_branch {
                walk_count(e, n);
            }
        }
        K::Binary { lhs, rhs, .. }
        | K::Assign { lhs, rhs, .. }
        | K::Pipeline { lhs, rhs }
        | K::Compound { lhs, rhs, .. } => {
            walk_count(lhs, n);
            walk_count(rhs, n);
        }
        K::Unary { operand, .. }
        | K::Field { obj: operand, .. }
        | K::Try { expr: operand }
        | K::Paren(operand)
        | K::Cast { expr: operand, .. } => walk_count(operand, n),
        K::Call { callee, args, .. } => {
            walk_count(callee, n);
            for a in args {
                match a {
                    cssl_hir::HirCallArg::Positional(e)
                    | cssl_hir::HirCallArg::Named { value: e, .. } => walk_count(e, n),
                }
            }
        }
        K::Index { obj, index } => {
            walk_count(obj, n);
            walk_count(index, n);
        }
        K::Run { expr: inner } => walk_count(inner, n),
        K::Tuple(es) => {
            for e in es {
                walk_count(e, n);
            }
        }
        _ => {}
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § MIR lowering of the synthetic comptime fn.
// ─────────────────────────────────────────────────────────────────────────

/// Lower a synthetic comptime `HirFn` to a `MirFunc` ready for the JIT.
/// If a `SourceFile` reference is provided, literal-value extraction will
/// pull real values from the original text — without it, `arith.constant`
/// ops fall through to `stage0_*` placeholders that parse as 0.
///
/// Returns `Result` even though the current body is infallible — future
/// extensions (e.g., trait-table lookups for the `obj.method()` path) need
/// the error channel.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower_synth_fn_to_mir(
    synth: &HirFn,
    result_ty: &MirType,
    interner: &Interner,
    source: Option<&SourceFile>,
) -> Result<MirFunc, ComptimeError> {
    // Build the MIR signature — zero params, single result.
    let name = interner.resolve(synth.name).to_string();
    let mut mir_fn = MirFunc::new(name, Vec::new(), vec![result_ty.clone()]);
    cssl_mir::body_lower::lower_fn_body(interner, source, synth, &mut mir_fn);
    Ok(mir_fn)
}

// ─────────────────────────────────────────────────────────────────────────
// § JIT execution : same-backend compile + invoke.
// ─────────────────────────────────────────────────────────────────────────

/// Raw output bytes from the JIT-compiled comptime fn. We serialize the
/// scalar result into native-endian bytes so [`decode_raw_result`] can re-
/// assemble it as a structured [`ComptimeValue`] without juggling signed /
/// unsigned + width permutations everywhere.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum RawComptimeResult {
    I32(i32),
    I64(i64),
    F32(f32),
    F64(f64),
    Bool(bool),
    Unit,
}

/// Compile + execute a MIR fn via the in-process Cranelift JIT and return
/// the scalar result as raw bytes.
pub(crate) fn jit_eval_mir_fn(
    mir_fn: &MirFunc,
    result_ty: &MirType,
) -> Result<RawComptimeResult, ComptimeError> {
    let mut module = cssl_cgen_cpu_cranelift::JitModule::new();
    let handle = module
        .compile(mir_fn)
        .map_err(|e| ComptimeError::JitFailed(e.to_string()))?;
    module
        .finalize()
        .map_err(|e| ComptimeError::JitFailed(e.to_string()))?;

    match result_ty {
        MirType::Int(IntWidth::I32) => {
            let v = handle
                .call_unit_to_i32(&module)
                .map_err(|e| ComptimeError::JitFailed(e.to_string()))?;
            Ok(RawComptimeResult::I32(v))
        }
        MirType::Int(IntWidth::I64) => {
            let v = handle
                .call_unit_to_i64(&module)
                .map_err(|e| ComptimeError::JitFailed(e.to_string()))?;
            Ok(RawComptimeResult::I64(v))
        }
        MirType::Float(FloatWidth::F32) => {
            let v = handle
                .call_unit_to_f32(&module)
                .map_err(|e| ComptimeError::JitFailed(e.to_string()))?;
            Ok(RawComptimeResult::F32(v))
        }
        MirType::Float(FloatWidth::F64) => {
            let v = handle
                .call_unit_to_f64(&module)
                .map_err(|e| ComptimeError::JitFailed(e.to_string()))?;
            Ok(RawComptimeResult::F64(v))
        }
        MirType::Bool => {
            let v = handle
                .call_unit_to_bool(&module)
                .map_err(|e| ComptimeError::JitFailed(e.to_string()))?;
            Ok(RawComptimeResult::Bool(v))
        }
        MirType::None => Ok(RawComptimeResult::Unit),
        other => Err(ComptimeError::UnsupportedResultType(format!(
            "comptime result type `{other}` not yet wired to JIT invocation"
        ))),
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § Raw-bytes ↔ structured ComptimeValue codec.
// ─────────────────────────────────────────────────────────────────────────

/// Decode a JIT-returned raw value into a [`ComptimeValue`].
pub(crate) fn decode_raw_result(
    raw: &RawComptimeResult,
    ty: &MirType,
) -> Result<ComptimeValue, ComptimeError> {
    match (raw, ty) {
        (RawComptimeResult::I32(v), MirType::Int(IntWidth::I32)) => {
            Ok(ComptimeValue::Int(i64::from(*v), IntWidth::I32))
        }
        (RawComptimeResult::I64(v), MirType::Int(IntWidth::I64)) => {
            Ok(ComptimeValue::Int(*v, IntWidth::I64))
        }
        (RawComptimeResult::F32(v), MirType::Float(FloatWidth::F32)) => {
            Ok(ComptimeValue::Float(f64::from(*v), FloatWidth::F32))
        }
        (RawComptimeResult::F64(v), MirType::Float(FloatWidth::F64)) => {
            Ok(ComptimeValue::Float(*v, FloatWidth::F64))
        }
        (RawComptimeResult::Bool(b), MirType::Bool) => Ok(ComptimeValue::Bool(*b)),
        (RawComptimeResult::Unit, MirType::None) => Ok(ComptimeValue::Unit),
        // i1-MIR coming back as an i32 is fine (cranelift passes i1 as i8/i32).
        (RawComptimeResult::I32(v), MirType::Bool) => Ok(ComptimeValue::Bool(*v != 0)),
        _ => Err(ComptimeError::LoweringFailed(format!(
            "decode_raw_result : raw {raw:?} doesn't match expected MIR type {ty}"
        ))),
    }
}

/// Encode a [`ComptimeValue`] into its canonical native-endian byte form.
/// Public alias provided as `encode_value_bytes_pub` for cross-crate callers.
pub fn encode_value_bytes_pub(v: &ComptimeValue) -> Vec<u8> {
    encode_value_bytes(v)
}

/// Encode a [`ComptimeValue`] into its canonical native-endian byte form.
pub(crate) fn encode_value_bytes(v: &ComptimeValue) -> Vec<u8> {
    match v {
        ComptimeValue::Int(n, IntWidth::I1 | IntWidth::I8) => vec![*n as u8],
        ComptimeValue::Int(n, IntWidth::I16) => (*n as i16).to_ne_bytes().to_vec(),
        ComptimeValue::Int(n, IntWidth::I32) => (*n as i32).to_ne_bytes().to_vec(),
        ComptimeValue::Int(n, IntWidth::I64 | IntWidth::Index) => n.to_ne_bytes().to_vec(),
        ComptimeValue::Float(f, FloatWidth::F16 | FloatWidth::Bf16) => {
            // Stage-0 : encode as 2-byte half-truncate of the f32 bits.
            let bits = (*f as f32).to_bits();
            let half = (bits >> 16) as u16;
            half.to_ne_bytes().to_vec()
        }
        ComptimeValue::Float(f, FloatWidth::F32) => (*f as f32).to_ne_bytes().to_vec(),
        ComptimeValue::Float(f, FloatWidth::F64) => f.to_ne_bytes().to_vec(),
        ComptimeValue::Bool(b) => vec![u8::from(*b)],
        ComptimeValue::Unit => Vec::new(),
        ComptimeValue::Array(elems) => {
            let mut out = Vec::with_capacity(elems.len() * 4);
            for e in elems {
                out.extend(encode_value_bytes(e));
            }
            out
        }
        ComptimeValue::Struct(fields) => {
            let mut out = Vec::new();
            for (_, v) in fields {
                out.extend(encode_value_bytes(v));
            }
            out
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § Module-level helpers : walk a HirModule and evaluate every `#run`.
// ─────────────────────────────────────────────────────────────────────────

/// Evaluate every `#run` block in `module` and return the per-site results.
/// Used by the integration tests + by the compiler driver to bake all comptime
/// values into the MIR before the specialization pass runs.
///
/// # Errors
/// Returns the first [`ComptimeError`] encountered ; further sites are skipped.
pub fn eval_all_run_blocks(
    module: &HirModule,
    interner: &Interner,
    evaluator: &mut ComptimeEvaluator,
) -> Result<Vec<ComptimeResult>, ComptimeError> {
    eval_all_run_blocks_with_source(module, interner, None, evaluator)
}

/// Source-threaded variant of [`eval_all_run_blocks`] — pass the parsed
/// `SourceFile` so literal-value extraction works.
///
/// # Errors
/// Returns the first [`ComptimeError`] encountered ; further sites are skipped.
pub fn eval_all_run_blocks_with_source(
    module: &HirModule,
    interner: &Interner,
    source: Option<&SourceFile>,
    evaluator: &mut ComptimeEvaluator,
) -> Result<Vec<ComptimeResult>, ComptimeError> {
    let mut out = Vec::new();
    let mut sites = Vec::<HirExpr>::new();
    collect_run_inner_exprs_module(module, &mut sites);
    for inner in sites {
        let r = evaluator.eval_run_block_with_source(&inner, interner, source)?;
        out.push(r);
    }
    Ok(out)
}

fn collect_run_inner_exprs_module(module: &HirModule, out: &mut Vec<HirExpr>) {
    for item in &module.items {
        collect_run_inner_exprs_item(item, out);
    }
}

fn collect_run_inner_exprs_item(item: &cssl_hir::HirItem, out: &mut Vec<HirExpr>) {
    match item {
        cssl_hir::HirItem::Fn(f) => {
            if let Some(b) = &f.body {
                collect_run_inner_exprs_block(b, out);
            }
        }
        cssl_hir::HirItem::Impl(i) => {
            for f in &i.fns {
                if let Some(b) = &f.body {
                    collect_run_inner_exprs_block(b, out);
                }
            }
        }
        cssl_hir::HirItem::Module(m) => {
            if let Some(sub) = &m.items {
                for s in sub {
                    collect_run_inner_exprs_item(s, out);
                }
            }
        }
        _ => {}
    }
}

fn collect_run_inner_exprs_block(block: &HirBlock, out: &mut Vec<HirExpr>) {
    for s in &block.stmts {
        if let cssl_hir::HirStmtKind::Let { value: Some(v), .. }
        | cssl_hir::HirStmtKind::Expr(v) = &s.kind
        {
            collect_run_inner_exprs_expr(v, out);
        }
    }
    if let Some(t) = &block.trailing {
        collect_run_inner_exprs_expr(t, out);
    }
}

fn collect_run_inner_exprs_expr(e: &HirExpr, out: &mut Vec<HirExpr>) {
    match &e.kind {
        HirExprKind::Run { expr } => {
            out.push((**expr).clone());
            collect_run_inner_exprs_expr(expr, out);
        }
        HirExprKind::Block(b) => collect_run_inner_exprs_block(b, out),
        HirExprKind::If {
            cond,
            then_branch,
            else_branch,
        } => {
            collect_run_inner_exprs_expr(cond, out);
            collect_run_inner_exprs_block(then_branch, out);
            if let Some(e) = else_branch {
                collect_run_inner_exprs_expr(e, out);
            }
        }
        HirExprKind::Binary { lhs, rhs, .. }
        | HirExprKind::Assign { lhs, rhs, .. }
        | HirExprKind::Pipeline { lhs, rhs }
        | HirExprKind::Compound { lhs, rhs, .. } => {
            collect_run_inner_exprs_expr(lhs, out);
            collect_run_inner_exprs_expr(rhs, out);
        }
        HirExprKind::Unary { operand, .. }
        | HirExprKind::Paren(operand)
        | HirExprKind::Try { expr: operand }
        | HirExprKind::Cast { expr: operand, .. } => collect_run_inner_exprs_expr(operand, out),
        HirExprKind::Call { callee, args, .. } => {
            collect_run_inner_exprs_expr(callee, out);
            for a in args {
                match a {
                    cssl_hir::HirCallArg::Positional(e)
                    | cssl_hir::HirCallArg::Named { value: e, .. } => {
                        collect_run_inner_exprs_expr(e, out);
                    }
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod inner_tests {
    use super::*;

    #[test]
    fn evaluator_default_budgets_present() {
        let ev = ComptimeEvaluator::new();
        assert_eq!(ev.op_budget, DEFAULT_OP_BUDGET);
        assert_eq!(ev.nest_limit, DEFAULT_NEST_LIMIT);
    }

    #[test]
    fn evaluator_with_budget_overrides() {
        let ev = ComptimeEvaluator::new().with_budget(100);
        assert_eq!(ev.op_budget, 100);
    }

    #[test]
    fn evaluator_with_nest_limit_overrides() {
        let ev = ComptimeEvaluator::new().with_nest_limit(2);
        assert_eq!(ev.nest_limit, 2);
    }

    #[test]
    fn evaluator_starts_at_nest_depth_zero() {
        let ev = ComptimeEvaluator::new();
        assert_eq!(ev.current_nest_depth(), 0);
    }

    #[test]
    fn evaluator_starts_with_zero_evaluations_performed() {
        let ev = ComptimeEvaluator::new();
        assert_eq!(ev.evaluations_performed(), 0);
    }

    #[test]
    fn comptime_value_byte_size_int32_is_4() {
        let v = ComptimeValue::Int(0, IntWidth::I32);
        assert_eq!(v.byte_size(), 4);
    }

    #[test]
    fn comptime_value_byte_size_array_sums_elements() {
        let v = ComptimeValue::Array(vec![
            ComptimeValue::Int(0, IntWidth::I32),
            ComptimeValue::Int(0, IntWidth::I32),
            ComptimeValue::Int(0, IntWidth::I32),
        ]);
        assert_eq!(v.byte_size(), 12);
    }

    #[test]
    fn comptime_value_constant_attr_int() {
        let v = ComptimeValue::Int(42, IntWidth::I32);
        assert_eq!(v.as_constant_attr(), "42");
    }

    #[test]
    fn comptime_value_constant_attr_bool_true() {
        let v = ComptimeValue::Bool(true);
        assert_eq!(v.as_constant_attr(), "true");
    }

    #[test]
    fn comptime_value_constant_attr_bool_false() {
        let v = ComptimeValue::Bool(false);
        assert_eq!(v.as_constant_attr(), "false");
    }
}
