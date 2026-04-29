//! T11-D141 — Integration tests for `#run` comptime evaluation.
//!
//! § COVERAGE
//!   - Scalar comptime-eval : i32 / i64 / f32 / f64 / bool / unit literals.
//!   - Constant-fold + arithmetic in `#run` body.
//!   - Sandbox-violation rejection : forbidden effect-tokens, forbidden fn-names.
//!   - Cycle detection over re-entrant evaluator state.
//!   - Op-budget exhaustion.
//!   - Nesting-limit enforcement.
//!   - Effect-row sandbox decisions.
//!   - LUT-bake demo end-to-end.
//!   - KAN-weight-bake mock demo end-to-end.
//!   - Bake-shape correctness (op-counts, attribute keys, source_loc markers).
//!   - encode/decode round-trips for ComptimeValue ↔ bytes.

#![allow(clippy::semicolon_if_nothing_returned)]
#![allow(clippy::cast_precision_loss)]

use cssl_ast::{SourceFile, SourceId, Surface};
use cssl_hir::{HirExpr, HirExprKind, Interner};
use cssl_mir::{FloatWidth, IntWidth, MirType};
use cssl_staging::{
    bake_lut, bake_result, bake_scalar_constant, bake_sine_lut_mir, build_sine_lut,
    check_sandbox_policy, eval_all_run_blocks_with_source, first_disallowed_effect,
    integrate_kan_layer_into_module, integrate_sine_lut_into_module, is_allowed_effect_token,
    is_comptime_baked, is_comptime_eligible_result_type, is_comptime_forbidden_effect,
    is_comptime_forbidden_fn, is_comptime_pure_fn, mock_train_kan_layer, scalar_mir_type,
    scan_expr_effects, BakedOps, ComptimeError, ComptimeEvaluator, ComptimeResult, ComptimeValue,
    EffectScanError, SandboxDecision, ALLOWED_PURE_FN_NAMES, FORBIDDEN_EFFECT_TOKENS,
    FORBIDDEN_FN_NAMES, SINE_LUT_SIZE,
};

// ─────────────────────────────────────────────────────────────────────────
// § Helpers : parse a CSSL source string + extract the inner #run expression.
// ─────────────────────────────────────────────────────────────────────────

fn parse(src: &str) -> (cssl_hir::HirModule, Interner, SourceFile) {
    let f = SourceFile::new(SourceId::first(), "<t>", src, Surface::RustHybrid);
    let toks = cssl_lex::lex(&f);
    let (cst, _bag) = cssl_parse::parse(&f, &toks);
    let (hir, interner, _lower_bag) = cssl_hir::lower_module(&f, &cst);
    (hir, interner, f)
}

/// Walk the HIR module to find the first `#run` expression and return its
/// inner expression (the operand of `HirExprKind::Run`).
fn find_first_run_inner(module: &cssl_hir::HirModule) -> Option<HirExpr> {
    let mut found = None;
    for item in &module.items {
        if let cssl_hir::HirItem::Fn(f) = item {
            if let Some(b) = &f.body {
                find_run_in_block(b, &mut found);
                if found.is_some() {
                    break;
                }
            }
        }
    }
    found
}

fn find_run_in_block(block: &cssl_hir::HirBlock, out: &mut Option<HirExpr>) {
    if out.is_some() {
        return;
    }
    for s in &block.stmts {
        if let cssl_hir::HirStmtKind::Let { value: Some(v), .. } | cssl_hir::HirStmtKind::Expr(v) =
            &s.kind
        {
            find_run_in_expr(v, out);
            if out.is_some() {
                return;
            }
        }
    }
    if let Some(t) = &block.trailing {
        find_run_in_expr(t, out);
    }
}

fn find_run_in_expr(e: &HirExpr, out: &mut Option<HirExpr>) {
    if out.is_some() {
        return;
    }
    if let HirExprKind::Run { expr } = &e.kind {
        *out = Some((**expr).clone());
        return;
    }
    match &e.kind {
        HirExprKind::Block(b) => find_run_in_block(b, out),
        HirExprKind::If {
            cond,
            then_branch,
            else_branch,
        } => {
            find_run_in_expr(cond, out);
            find_run_in_block(then_branch, out);
            if let Some(e) = else_branch {
                find_run_in_expr(e, out);
            }
        }
        HirExprKind::Binary { lhs, rhs, .. }
        | HirExprKind::Pipeline { lhs, rhs }
        | HirExprKind::Compound { lhs, rhs, .. } => {
            find_run_in_expr(lhs, out);
            find_run_in_expr(rhs, out);
        }
        HirExprKind::Paren(inner) | HirExprKind::Unary { operand: inner, .. } => {
            find_run_in_expr(inner, out)
        }
        HirExprKind::Call { callee, args, .. } => {
            find_run_in_expr(callee, out);
            for a in args {
                match a {
                    cssl_hir::HirCallArg::Positional(e)
                    | cssl_hir::HirCallArg::Named { value: e, .. } => find_run_in_expr(e, out),
                }
            }
        }
        _ => {}
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § Scalar comptime-eval tests.
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn comptime_int_literal_evaluates_to_constant() {
    let (hir, interner, src) = parse(r"fn f() -> i32 { #run 42 }");
    let inner = find_first_run_inner(&hir).expect("expected #run inner expr");
    let mut ev = ComptimeEvaluator::new();
    let r = ev
        .eval_run_block_with_source(&inner, &interner, Some(&src))
        .expect("eval ok");
    assert_eq!(r.ty, MirType::Int(IntWidth::I32));
    assert!(matches!(r.value, ComptimeValue::Int(42, IntWidth::I32)));
}

#[test]
fn comptime_arithmetic_int_evaluates_full_pipeline() {
    let (hir, interner, src) = parse(r"fn f() -> i32 { #run (3 + 4) }");
    let inner = find_first_run_inner(&hir).expect("expected #run inner");
    let mut ev = ComptimeEvaluator::new();
    let r = ev
        .eval_run_block_with_source(&inner, &interner, Some(&src))
        .expect("eval ok");
    if let ComptimeValue::Int(n, w) = r.value {
        assert_eq!(n, 7);
        assert_eq!(w, IntWidth::I32);
    } else {
        panic!("expected Int(7), got {:?}", r.value);
    }
}

#[test]
fn comptime_bool_true_evaluates() {
    let (hir, interner, src) = parse(r"fn f() -> bool { #run true }");
    let inner = find_first_run_inner(&hir).expect("expected #run inner");
    let mut ev = ComptimeEvaluator::new();
    let r = ev
        .eval_run_block_with_source(&inner, &interner, Some(&src))
        .expect("eval ok");
    assert_eq!(r.value, ComptimeValue::Bool(true));
}

#[test]
fn comptime_bool_false_evaluates() {
    let (hir, interner, src) = parse(r"fn f() -> bool { #run false }");
    let inner = find_first_run_inner(&hir).expect("expected #run inner");
    let mut ev = ComptimeEvaluator::new();
    let r = ev
        .eval_run_block_with_source(&inner, &interner, Some(&src))
        .expect("eval ok");
    assert_eq!(r.value, ComptimeValue::Bool(false));
}

#[test]
fn comptime_int_negation_evaluates() {
    let (hir, interner, src) = parse(r"fn f() -> i32 { #run (10 - 3) }");
    let inner = find_first_run_inner(&hir).expect("expected #run inner");
    let mut ev = ComptimeEvaluator::new();
    let r = ev
        .eval_run_block_with_source(&inner, &interner, Some(&src))
        .expect("eval ok");
    if let ComptimeValue::Int(n, _) = r.value {
        assert_eq!(n, 7);
    } else {
        panic!("expected Int(7), got {:?}", r.value);
    }
}

#[test]
fn comptime_int_multiplication_evaluates() {
    let (hir, interner, src) = parse(r"fn f() -> i32 { #run (6 * 7) }");
    let inner = find_first_run_inner(&hir).expect("expected #run inner");
    let mut ev = ComptimeEvaluator::new();
    let r = ev
        .eval_run_block_with_source(&inner, &interner, Some(&src))
        .expect("eval ok");
    if let ComptimeValue::Int(n, _) = r.value {
        assert_eq!(n, 42);
    } else {
        panic!("expected Int(42)");
    }
}

#[test]
fn comptime_eval_resets_nest_depth_after_completion() {
    let (hir, interner, src) = parse(r"fn f() -> i32 { #run 1 }");
    let inner = find_first_run_inner(&hir).expect("expected #run inner");
    let mut ev = ComptimeEvaluator::new();
    let _ = ev
        .eval_run_block_with_source(&inner, &interner, Some(&src))
        .expect("eval ok");
    assert_eq!(ev.current_nest_depth(), 0);
}

#[test]
fn comptime_eval_increments_evaluations_performed_counter() {
    let (hir, interner, src) = parse(r"fn f() -> i32 { #run 1 }");
    let inner = find_first_run_inner(&hir).expect("expected #run inner");
    let mut ev = ComptimeEvaluator::new();
    let before = ev.evaluations_performed();
    let _ = ev
        .eval_run_block_with_source(&inner, &interner, Some(&src))
        .expect("eval ok");
    assert!(ev.evaluations_performed() > before);
}

// ─────────────────────────────────────────────────────────────────────────
// § Sandbox + effect-scan tests.
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn effect_scan_rejects_perform_io() {
    // We can't easily construct a `perform IO::println` source — instead
    // exercise the scanner via a programmatic HirExpr.
    let interner = Interner::new();
    let span = cssl_ast::Span::new(SourceId::first(), 0, 0);
    let perform = HirExpr {
        span,
        id: cssl_hir::HirId(0),
        attrs: Vec::new(),
        kind: HirExprKind::Perform {
            path: vec![interner.intern("IO"), interner.intern("println")],
            def: None,
            args: Vec::new(),
        },
    };
    let err = scan_expr_effects(&perform, &interner).unwrap_err();
    assert!(matches!(err, EffectScanError::Forbidden(_)));
}

#[test]
fn effect_scan_rejects_call_to_forbidden_fn_println() {
    let interner = Interner::new();
    let span = cssl_ast::Span::new(SourceId::first(), 0, 0);
    let callee = HirExpr {
        span,
        id: cssl_hir::HirId(0),
        attrs: Vec::new(),
        kind: HirExprKind::Path {
            segments: vec![interner.intern("println")],
            def: None,
        },
    };
    let call = HirExpr {
        span,
        id: cssl_hir::HirId(1),
        attrs: Vec::new(),
        kind: HirExprKind::Call {
            callee: Box::new(callee),
            args: Vec::new(),
            type_args: Vec::new(),
        },
    };
    let err = scan_expr_effects(&call, &interner).unwrap_err();
    assert!(matches!(err, EffectScanError::Forbidden(_)));
}

#[test]
fn effect_scan_accepts_call_to_pure_intrinsic_sin() {
    let interner = Interner::new();
    let span = cssl_ast::Span::new(SourceId::first(), 0, 0);
    let callee = HirExpr {
        span,
        id: cssl_hir::HirId(0),
        attrs: Vec::new(),
        kind: HirExprKind::Path {
            segments: vec![interner.intern("sin")],
            def: None,
        },
    };
    let call = HirExpr {
        span,
        id: cssl_hir::HirId(1),
        attrs: Vec::new(),
        kind: HirExprKind::Call {
            callee: Box::new(callee),
            args: Vec::new(),
            type_args: Vec::new(),
        },
    };
    scan_expr_effects(&call, &interner).expect("sin is on the pure-allowlist");
}

#[test]
fn effect_scan_rejects_assignment_to_multi_segment_path() {
    let interner = Interner::new();
    let span = cssl_ast::Span::new(SourceId::first(), 0, 0);
    let lhs = HirExpr {
        span,
        id: cssl_hir::HirId(0),
        attrs: Vec::new(),
        kind: HirExprKind::Path {
            segments: vec![interner.intern("globals"), interner.intern("x")],
            def: None,
        },
    };
    let rhs = HirExpr {
        span,
        id: cssl_hir::HirId(1),
        attrs: Vec::new(),
        kind: HirExprKind::Literal(cssl_hir::HirLiteral {
            span,
            kind: cssl_hir::HirLiteralKind::Int,
        }),
    };
    let assign = HirExpr {
        span,
        id: cssl_hir::HirId(2),
        attrs: Vec::new(),
        kind: HirExprKind::Assign {
            op: None,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        },
    };
    let err = scan_expr_effects(&assign, &interner).unwrap_err();
    assert!(matches!(err, EffectScanError::SideEffect(_)));
}

#[test]
fn forbidden_effect_token_lookup_works() {
    assert!(is_comptime_forbidden_effect("IO"));
    assert!(is_comptime_forbidden_effect("Net"));
    assert!(is_comptime_forbidden_effect("Telemetry"));
    assert!(!is_comptime_forbidden_effect("Pure"));
    assert!(!is_comptime_forbidden_effect("Comptime"));
}

#[test]
fn forbidden_fn_name_lookup_works() {
    assert!(is_comptime_forbidden_fn("println"));
    assert!(is_comptime_forbidden_fn("read_file"));
    assert!(is_comptime_forbidden_fn("system"));
    assert!(!is_comptime_forbidden_fn("sin"));
    assert!(!is_comptime_forbidden_fn("max"));
}

#[test]
fn pure_fn_name_lookup_works() {
    assert!(is_comptime_pure_fn("sin"));
    assert!(is_comptime_pure_fn("cos"));
    assert!(is_comptime_pure_fn("min"));
    assert!(is_comptime_pure_fn("max"));
    assert!(!is_comptime_pure_fn("println"));
    assert!(!is_comptime_pure_fn("read_file"));
}

#[test]
fn forbidden_lists_are_non_empty() {
    assert!(!FORBIDDEN_EFFECT_TOKENS.is_empty());
    assert!(!FORBIDDEN_FN_NAMES.is_empty());
    assert!(!ALLOWED_PURE_FN_NAMES.is_empty());
}

#[test]
fn allowed_effect_token_lookup_works() {
    assert!(is_allowed_effect_token("Pure"));
    assert!(is_allowed_effect_token("NoFs"));
    assert!(is_allowed_effect_token("NoNet"));
    assert!(!is_allowed_effect_token("IO"));
    assert!(!is_allowed_effect_token("Net"));
}

#[test]
fn first_disallowed_effect_returns_offending_token() {
    let row = "{Pure, IO, NoNet}";
    let bad = first_disallowed_effect(row).expect("expected one disallowed");
    assert_eq!(bad, "IO");
}

#[test]
fn first_disallowed_effect_returns_none_for_allowed_row() {
    let row = "{Pure, NoFs, NoNet}";
    assert!(first_disallowed_effect(row).is_none());
}

#[test]
fn check_sandbox_policy_accepts_pure_i32() {
    let d = check_sandbox_policy(Some("{Pure}"), &MirType::Int(IntWidth::I32));
    assert!(d.is_allow());
}

#[test]
fn check_sandbox_policy_rejects_io_effect() {
    let d = check_sandbox_policy(Some("{IO}"), &MirType::Int(IntWidth::I32));
    assert!(!d.is_allow());
    assert!(d.reason().unwrap().contains("IO"));
}

#[test]
fn check_sandbox_policy_rejects_unsupported_result_type() {
    let d = check_sandbox_policy(Some("{Pure}"), &MirType::Handle);
    assert!(!d.is_allow());
}

#[test]
fn is_comptime_eligible_result_type_basics() {
    assert!(is_comptime_eligible_result_type(&MirType::Int(
        IntWidth::I32
    )));
    assert!(is_comptime_eligible_result_type(&MirType::Float(
        FloatWidth::F32
    )));
    assert!(is_comptime_eligible_result_type(&MirType::Bool));
    assert!(!is_comptime_eligible_result_type(&MirType::Handle));
}

// ─────────────────────────────────────────────────────────────────────────
// § Cycle + budget tests.
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn nest_limit_zero_rejects_any_eval() {
    let (hir, interner, src) = parse(r"fn f() -> i32 { #run 1 }");
    let inner = find_first_run_inner(&hir).expect("expected #run inner");
    let mut ev = ComptimeEvaluator::new().with_nest_limit(0);
    let err = ev
        .eval_run_block_with_source(&inner, &interner, Some(&src))
        .unwrap_err();
    assert!(matches!(
        err,
        ComptimeError::BudgetExhausted { ref limit_kind, .. } if limit_kind == "nest_depth"
    ));
}

#[test]
fn op_budget_rejects_oversize_body() {
    let (hir, interner, src) = parse(r"fn f() -> i32 { #run (1 + 2) }");
    let inner = find_first_run_inner(&hir).expect("expected #run inner");
    let mut ev = ComptimeEvaluator::new().with_budget(1); // way too small
    let err = ev
        .eval_run_block_with_source(&inner, &interner, Some(&src))
        .unwrap_err();
    assert!(matches!(
        err,
        ComptimeError::BudgetExhausted { ref limit_kind, .. } if limit_kind == "op_count"
    ));
}

// ─────────────────────────────────────────────────────────────────────────
// § Encode / decode round-trip tests.
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn encode_int32_roundtrip() {
    let v = ComptimeValue::Int(42, IntWidth::I32);
    let bytes = cssl_staging::comptime::encode_value_bytes_pub(&v);
    assert_eq!(bytes.len(), 4);
}

#[test]
fn encode_int64_roundtrip() {
    let v = ComptimeValue::Int(0x1234_5678_9abc_def0, IntWidth::I64);
    let bytes = cssl_staging::comptime::encode_value_bytes_pub(&v);
    assert_eq!(bytes.len(), 8);
}

#[test]
fn encode_f32_roundtrip() {
    let v = ComptimeValue::Float(1.5, FloatWidth::F32);
    let bytes = cssl_staging::comptime::encode_value_bytes_pub(&v);
    assert_eq!(bytes.len(), 4);
}

#[test]
fn encode_f64_roundtrip() {
    let v = ComptimeValue::Float(std::f64::consts::PI, FloatWidth::F64);
    let bytes = cssl_staging::comptime::encode_value_bytes_pub(&v);
    assert_eq!(bytes.len(), 8);
}

#[test]
fn encode_bool_roundtrip() {
    let bytes_t = cssl_staging::comptime::encode_value_bytes_pub(&ComptimeValue::Bool(true));
    let bytes_f = cssl_staging::comptime::encode_value_bytes_pub(&ComptimeValue::Bool(false));
    assert_eq!(bytes_t, vec![1u8]);
    assert_eq!(bytes_f, vec![0u8]);
}

#[test]
fn encode_unit_is_empty() {
    let bytes = cssl_staging::comptime::encode_value_bytes_pub(&ComptimeValue::Unit);
    assert!(bytes.is_empty());
}

#[test]
fn encode_array_of_f32_packs_elements() {
    let arr = ComptimeValue::Array(vec![
        ComptimeValue::Float(1.0, FloatWidth::F32),
        ComptimeValue::Float(2.0, FloatWidth::F32),
        ComptimeValue::Float(3.0, FloatWidth::F32),
    ]);
    let bytes = cssl_staging::comptime::encode_value_bytes_pub(&arr);
    assert_eq!(bytes.len(), 12);
}

#[test]
fn comptime_value_byte_size_scalars() {
    assert_eq!(ComptimeValue::Int(0, IntWidth::I32).byte_size(), 4);
    assert_eq!(ComptimeValue::Int(0, IntWidth::I64).byte_size(), 8);
    assert_eq!(ComptimeValue::Float(0.0, FloatWidth::F32).byte_size(), 4);
    assert_eq!(ComptimeValue::Bool(true).byte_size(), 1);
    assert_eq!(ComptimeValue::Unit.byte_size(), 0);
}

#[test]
fn comptime_value_is_scalar_predicate() {
    assert!(ComptimeValue::Int(0, IntWidth::I32).is_scalar());
    assert!(ComptimeValue::Bool(false).is_scalar());
    assert!(!ComptimeValue::Array(vec![]).is_scalar());
    assert!(!ComptimeValue::Struct(vec![]).is_scalar());
}

// ─────────────────────────────────────────────────────────────────────────
// § Bake-shape tests.
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn bake_scalar_emits_single_arith_constant() {
    let result = ComptimeResult {
        bytes: 42_i32.to_ne_bytes().to_vec(),
        ty: MirType::Int(IntWidth::I32),
        value: ComptimeValue::Int(42, IntWidth::I32),
    };
    let mut next: u32 = 0;
    let baked = bake_result(&result, &mut next);
    assert_eq!(baked.op_count(), 1);
    assert_eq!(baked.ops[0].name, "arith.constant");
    assert!(is_comptime_baked(&baked.ops[0]));
}

#[test]
fn bake_array_emits_n_plus_one_ops() {
    let result = ComptimeResult {
        bytes: Vec::new(),
        ty: MirType::Opaque("array<i32>".into()),
        value: ComptimeValue::Array(vec![
            ComptimeValue::Int(1, IntWidth::I32),
            ComptimeValue::Int(2, IntWidth::I32),
            ComptimeValue::Int(3, IntWidth::I32),
        ]),
    };
    let mut next: u32 = 0;
    let baked = bake_result(&result, &mut next);
    assert_eq!(baked.op_count(), 4); // 3 constants + 1 assemble
    assert_eq!(baked.ops.last().unwrap().name, "cssl.array.assemble");
}

#[test]
fn bake_struct_emits_m_plus_one_ops() {
    let result = ComptimeResult {
        bytes: Vec::new(),
        ty: MirType::Opaque("struct".into()),
        value: ComptimeValue::Struct(vec![
            ("x".into(), ComptimeValue::Int(1, IntWidth::I32)),
            ("y".into(), ComptimeValue::Int(2, IntWidth::I32)),
        ]),
    };
    let mut next: u32 = 0;
    let baked = bake_result(&result, &mut next);
    assert_eq!(baked.op_count(), 3);
    assert_eq!(baked.ops.last().unwrap().name, "cssl.struct.assemble");
}

#[test]
fn bake_scalar_constant_helper_returns_op_for_scalars() {
    let result = ComptimeResult {
        bytes: Vec::new(),
        ty: MirType::Int(IntWidth::I32),
        value: ComptimeValue::Int(7, IntWidth::I32),
    };
    let op = bake_scalar_constant(&result, cssl_mir::ValueId(0));
    assert!(op.is_some());
    assert_eq!(op.unwrap().name, "arith.constant");
}

#[test]
fn bake_scalar_constant_helper_returns_none_for_array() {
    let result = ComptimeResult {
        bytes: Vec::new(),
        ty: MirType::Opaque("array".into()),
        value: ComptimeValue::Array(vec![ComptimeValue::Int(0, IntWidth::I32)]),
    };
    let op = bake_scalar_constant(&result, cssl_mir::ValueId(0));
    assert!(op.is_none());
}

#[test]
fn scalar_mir_type_for_int32() {
    let ty = scalar_mir_type(&ComptimeValue::Int(0, IntWidth::I32));
    assert_eq!(ty, MirType::Int(IntWidth::I32));
}

#[test]
fn scalar_mir_type_for_f32() {
    let ty = scalar_mir_type(&ComptimeValue::Float(0.0, FloatWidth::F32));
    assert_eq!(ty, MirType::Float(FloatWidth::F32));
}

// ─────────────────────────────────────────────────────────────────────────
// § LUT-bake demo end-to-end.
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn build_sine_lut_returns_256_entries() {
    let lut = build_sine_lut();
    assert_eq!(lut.len(), SINE_LUT_SIZE);
}

#[test]
fn build_sine_lut_first_entry_is_zero() {
    let lut = build_sine_lut();
    if let ComptimeValue::Float(v, _) = &lut[0].value {
        assert!(v.abs() < 1e-6, "sin(0) should be 0, got {v}");
    } else {
        panic!("expected Float");
    }
}

#[test]
fn build_sine_lut_quarter_entry_is_one() {
    let lut = build_sine_lut();
    let q = SINE_LUT_SIZE / 4;
    if let ComptimeValue::Float(v, _) = &lut[q].value {
        assert!((v - 1.0).abs() < 1e-3, "sin(π/2) should be 1, got {v}");
    } else {
        panic!("expected Float at quarter index");
    }
}

#[test]
fn bake_sine_lut_mir_emits_257_ops() {
    let baked: BakedOps = bake_sine_lut_mir();
    // 256 arith.constant + 1 cssl.array.assemble.
    assert_eq!(baked.op_count(), SINE_LUT_SIZE + 1);
}

#[test]
fn bake_sine_lut_terminates_with_array_assemble() {
    let baked = bake_sine_lut_mir();
    assert_eq!(baked.ops.last().unwrap().name, "cssl.array.assemble");
}

#[test]
fn integrate_sine_lut_into_module_appends_init_fn() {
    let mut module = cssl_mir::MirModule::new();
    let idx = integrate_sine_lut_into_module(&mut module);
    assert_eq!(idx, 0);
    assert_eq!(module.funcs[0].name, "__sine_lut_init");
    let entry = module.funcs[0].body.entry().unwrap();
    let last_op = entry.ops.last().unwrap();
    assert_eq!(last_op.name, "func.return");
}

#[test]
fn integrate_sine_lut_into_module_carries_comptime_baked_attribute() {
    let mut module = cssl_mir::MirModule::new();
    integrate_sine_lut_into_module(&mut module);
    let attr = module.funcs[0]
        .attributes
        .iter()
        .find(|(k, _)| k == "comptime_baked");
    assert!(attr.is_some());
    let v = &attr.unwrap().1;
    assert!(v.contains("256"));
}

// ─────────────────────────────────────────────────────────────────────────
// § KAN-weight bake demo end-to-end.
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn mock_train_kan_layer_produces_correct_shape() {
    let layer = mock_train_kan_layer(2, 3, 4);
    assert_eq!(layer.in_dim, 2);
    assert_eq!(layer.out_dim, 3);
    assert_eq!(layer.knot_count, 4);
    assert_eq!(layer.weights.len(), 24);
    assert_eq!(layer.biases.len(), 3);
}

#[test]
fn mock_train_kan_layer_baked_byte_size() {
    let layer = mock_train_kan_layer(1, 1, 1);
    // 12 bytes (3 i32 dims) + 4 bytes (1 weight) + 4 bytes (1 bias) = 20
    assert_eq!(layer.baked_byte_size(), 20);
}

#[test]
fn integrate_kan_layer_into_module_appends_init_fn() {
    let mut module = cssl_mir::MirModule::new();
    let idx = integrate_kan_layer_into_module(&mut module, "L1", 2, 2, 2);
    assert_eq!(idx, 0);
    assert_eq!(module.funcs[0].name, "__kan_L1_init");
}

#[test]
fn integrate_kan_layer_carries_comptime_baked_attribute() {
    let mut module = cssl_mir::MirModule::new();
    integrate_kan_layer_into_module(&mut module, "test", 4, 5, 6);
    let attr = module.funcs[0]
        .attributes
        .iter()
        .find(|(k, _)| k == "comptime_baked");
    assert!(attr.is_some());
    let v = &attr.unwrap().1;
    assert!(v.contains("4x5x6"));
}

#[test]
fn kan_layer_baked_struct_has_5_fields() {
    let layer = mock_train_kan_layer(2, 2, 2);
    let result = cssl_staging::kan_layer_as_comptime(&layer);
    if let ComptimeValue::Struct(fields) = &result.value {
        assert_eq!(fields.len(), 5);
        let names: Vec<&str> = fields.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains(&"in_dim"));
        assert!(names.contains(&"out_dim"));
        assert!(names.contains(&"knot_count"));
        assert!(names.contains(&"weights"));
        assert!(names.contains(&"biases"));
    } else {
        panic!("expected Struct");
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § Module-walk integration tests.
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn eval_all_run_blocks_handles_zero_run_sites() {
    let (hir, interner, src) = parse(r"fn f() -> i32 { 42 }");
    let mut ev = ComptimeEvaluator::new();
    let results =
        eval_all_run_blocks_with_source(&hir, &interner, Some(&src), &mut ev).expect("ok");
    assert!(results.is_empty());
}

#[test]
fn eval_all_run_blocks_evaluates_single_site() {
    let (hir, interner, src) = parse(r"fn f() -> i32 { #run (10 + 5) }");
    let mut ev = ComptimeEvaluator::new();
    let results =
        eval_all_run_blocks_with_source(&hir, &interner, Some(&src), &mut ev).expect("ok");
    assert_eq!(results.len(), 1);
    if let ComptimeValue::Int(n, _) = results[0].value {
        assert_eq!(n, 15);
    } else {
        panic!("expected Int(15)");
    }
}

#[test]
fn eval_all_run_blocks_evaluates_multiple_sites() {
    let (hir, interner, src) = parse(
        r"fn a() -> i32 { #run 1 }
          fn b() -> i32 { #run 2 }",
    );
    let mut ev = ComptimeEvaluator::new();
    let results =
        eval_all_run_blocks_with_source(&hir, &interner, Some(&src), &mut ev).expect("ok");
    assert_eq!(results.len(), 2);
}

// ─────────────────────────────────────────────────────────────────────────
// § Bake-LUT helper test.
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn bake_lut_emits_correct_op_count_for_4_entries() {
    let entries: Vec<ComptimeResult> = (0..4)
        .map(|i| ComptimeResult {
            bytes: (i as f32).to_ne_bytes().to_vec(),
            ty: MirType::Float(FloatWidth::F32),
            value: ComptimeValue::Float(f64::from(i as f32), FloatWidth::F32),
        })
        .collect();
    let mut next: u32 = 0;
    let baked = bake_lut(&entries, &mut next);
    // 4 arith.constant + 1 cssl.array.assemble.
    assert_eq!(baked.op_count(), 5);
    assert_eq!(baked.ops.last().unwrap().name, "cssl.array.assemble");
    let kind = baked
        .ops
        .last()
        .unwrap()
        .attributes
        .iter()
        .find(|(k, _)| k == "kind")
        .map(|(_, v)| v.as_str());
    assert_eq!(kind, Some("lut"));
}

// ─────────────────────────────────────────────────────────────────────────
// § Sandbox-decision tests.
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn sandbox_decision_allow_predicate() {
    let d = SandboxDecision::Allow;
    assert!(d.is_allow());
    assert!(d.reason().is_none());
}

#[test]
fn sandbox_decision_reject_predicate() {
    let d = SandboxDecision::Reject("bad effect".into());
    assert!(!d.is_allow());
    assert_eq!(d.reason(), Some("bad effect"));
}
