//! § T11-W19-G · csslc QUANTUM-CIRCUIT primitives — integration tests
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § APOCKY-DIRECTIVE (verbatim · T11-W19-G)
//!   "Add quantum-circuit primitives to csslc so .csl source can express
//!    directly · qbind / qsuperpose / qmeasure / qentangle · these compile
//!    to extern "C" calls into cssl_host_quantum_hdc::{bind, bundle,
//!    permute, coherence}"
//!
//! § COVERAGE
//!   For each q-prim (qbind / qsuperpose / qmeasure / qentangle) :
//!     1. csslc check passes (lex → parse → HIR → MIR pipeline accepts the
//!        callee + emits a `func.call` op with the mangled symbol name).
//!     2. The generated MIR contains exactly one `func.call` op whose
//!        `callee` attribute is the `cssl_quantum_<name>` mangled symbol —
//!        i.e. the q-prim recognizer fired and rewrote the target.
//!     3. The result-type for that op matches the expected ABI :
//!        - `qbind` / `qsuperpose` / `qentangle` → `i64` handle
//!        - `qmeasure` → `i32` basis-index
//!
//!   Plus one composite test using all 4 q-prims in a single fn body, to
//!   verify the recognizer fires per-call rather than per-fn.
//!
//! § PIPELINE PHASE EXERCISED
//!   - cssl-lex (tokens)
//!   - cssl-parse (CST)
//!   - cssl-hir lower_module (HIR)
//!   - cssl-mir lower_module_signatures + lower_fn_body (MIR)
//!
//!   The downstream cgen-cpu-cranelift JIT step is not executed here ;
//!   the FFI symbols are linked at runtime by `cssl-host-quantum-hdc`
//!   (which has its own 9 unit tests in `ffi::tests`). This integration
//!   layer validates the csslc-side rewrite + result-type assignment.
//!
//! § REFERENCE
//!   - `compiler-rs/crates/cssl-host-quantum-hdc/src/ffi.rs` — FFI shim
//!   - `compiler-rs/crates/cssl-mir/src/body_lower.rs` — q-prim recognizer
//!     in `lower_call` + result-type entries in `infer_intrinsic_result_type`
//!   - tests/fixtures/quantum_prims/q*.csl — per-prim and composite fixtures
//!
//! § ATTESTATION (PRIME_DIRECTIVE §11)
//!   t∞: ¬(hurt ∨ harm) .making-of-T11-W19-G @ (anyone ∨ anything ∨ anybody)

use std::path::PathBuf;
use std::process::ExitCode;

use cssl_ast::{SourceFile, SourceId, Surface};
use cssl_hir::lower_module;
use cssl_mir::{lower_fn_body, lower_module_signatures, IntWidth, LowerCtx, MirModule, MirType};
use csslc::commands::check;
use csslc::exit_code;

fn fixture_path(rel: &str) -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.join("tests").join("fixtures").join(rel)
}

fn assert_check_succeeds(rel: &str) {
    let path = fixture_path(rel);
    let source = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("fixture '{rel}' read-error: {e}"));
    let code = check::run_with_source(&path, &source);
    let ok: ExitCode = ExitCode::from(exit_code::SUCCESS);
    assert_eq!(
        format!("{code:?}"),
        format!("{ok:?}"),
        "fixture '{rel}' must pass check ; got non-success exit"
    );
}

/// § Lower a `.csl` fixture to a populated `MirModule` whose function
///   bodies are filled with lowered ops. Returns the module so each test
///   can introspect the emitted `func.call` ops.
fn lower_fixture_to_mir(rel: &str) -> MirModule {
    let path = fixture_path(rel);
    let source_text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("fixture '{rel}' read-error: {e}"));
    let source =
        SourceFile::new(SourceId::first(), path.to_string_lossy().as_ref(), &source_text, Surface::RustHybrid);
    let tokens = cssl_lex::lex(&source);
    let (cst, parse_diags) = cssl_parse::parse(&source, &tokens);
    assert_eq!(
        parse_diags.error_count(),
        0,
        "fixture '{rel}' parse errors : {parse_diags:?}"
    );
    let (hir, interner, hir_diags) = lower_module(&source, &cst);
    assert_eq!(
        hir_diags.error_count(),
        0,
        "fixture '{rel}' HIR-lower errors : {hir_diags:?}"
    );
    // Build MIR with signature-only lowering, then populate each fn body.
    let lower_ctx = LowerCtx::new(&interner);
    let mut mir = lower_module_signatures(&lower_ctx, &hir);
    for item in &hir.items {
        if let cssl_hir::HirItem::Fn(hir_fn) = item {
            let fn_name = interner.resolve(hir_fn.name);
            let mir_fn = mir
                .funcs
                .iter_mut()
                .find(|f| f.name == fn_name)
                .unwrap_or_else(|| panic!("MIR fn '{fn_name}' missing for fixture '{rel}'"));
            lower_fn_body(&interner, Some(&source), hir_fn, mir_fn);
        }
    }
    mir
}

/// § Find the unique `func.call` op in the named fn whose callee
///   attribute matches `expected_callee`. Panics otherwise.
fn assert_func_call_with_callee_and_result(
    mir: &MirModule,
    fn_name: &str,
    expected_callee: &str,
    expected_result: &MirType,
) {
    let f = mir
        .funcs
        .iter()
        .find(|f| f.name == fn_name)
        .unwrap_or_else(|| panic!("fn '{fn_name}' missing in MIR module"));
    let entry = f
        .body
        .entry()
        .unwrap_or_else(|| panic!("fn '{fn_name}' has no entry block"));
    let mut matches = entry.ops.iter().filter(|op| {
        op.name == "func.call"
            && op
                .attributes
                .iter()
                .any(|(k, v)| k == "callee" && v == expected_callee)
    });
    let op = matches
        .next()
        .unwrap_or_else(|| panic!("expected func.call @{expected_callee} in fn '{fn_name}'"));
    let extra = matches.count();
    assert_eq!(
        extra, 0,
        "expected exactly 1 func.call @{expected_callee} in fn '{fn_name}', got {}",
        extra + 1
    );
    let result_ty = op
        .results
        .first()
        .map(|v| v.ty.clone())
        .unwrap_or(MirType::None);
    assert_eq!(
        format!("{result_ty}"),
        format!("{expected_result}"),
        "func.call @{expected_callee} result-type mismatch in fn '{fn_name}'"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// § PER-PRIM TESTS · check pipeline + recognizer + result-type
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn t11_w19_qbind_check_passes() {
    assert_check_succeeds("quantum_prims/qbind.csl");
}

#[test]
fn t11_w19_qbind_lowers_to_extern_call() {
    let mir = lower_fixture_to_mir("quantum_prims/qbind.csl");
    assert_func_call_with_callee_and_result(
        &mir,
        "run_bind",
        "cssl_quantum_qbind",
        &MirType::Int(IntWidth::I64),
    );
}

#[test]
fn t11_w19_qsuperpose_check_passes() {
    assert_check_succeeds("quantum_prims/qsuperpose.csl");
}

#[test]
fn t11_w19_qsuperpose_lowers_to_extern_call() {
    let mir = lower_fixture_to_mir("quantum_prims/qsuperpose.csl");
    assert_func_call_with_callee_and_result(
        &mir,
        "run_superpose",
        "cssl_quantum_qsuperpose",
        &MirType::Int(IntWidth::I64),
    );
}

#[test]
fn t11_w19_qmeasure_check_passes() {
    assert_check_succeeds("quantum_prims/qmeasure.csl");
}

#[test]
fn t11_w19_qmeasure_lowers_to_extern_call() {
    let mir = lower_fixture_to_mir("quantum_prims/qmeasure.csl");
    assert_func_call_with_callee_and_result(
        &mir,
        "run_measure",
        "cssl_quantum_qmeasure",
        &MirType::Int(IntWidth::I32),
    );
}

#[test]
fn t11_w19_qentangle_check_passes() {
    assert_check_succeeds("quantum_prims/qentangle.csl");
}

#[test]
fn t11_w19_qentangle_lowers_to_extern_call() {
    let mir = lower_fixture_to_mir("quantum_prims/qentangle.csl");
    assert_func_call_with_callee_and_result(
        &mir,
        "run_entangle",
        "cssl_quantum_qentangle",
        &MirType::Int(IntWidth::I64),
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// § COMPOSITE TEST · all 4 q-prims composed in one fn body
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn t11_w19_qcircuit_check_passes() {
    assert_check_succeeds("quantum_prims/qcircuit.csl");
}

#[test]
fn t11_w19_qcircuit_emits_all_four_extern_calls() {
    let mir = lower_fixture_to_mir("quantum_prims/qcircuit.csl");
    let f = mir
        .funcs
        .iter()
        .find(|f| f.name == "run_circuit")
        .expect("run_circuit fn must be present");
    let entry = f
        .body
        .entry()
        .expect("run_circuit must have an entry block");
    let extern_calls: Vec<&str> = entry
        .ops
        .iter()
        .filter(|op| op.name == "func.call")
        .filter_map(|op| {
            op.attributes
                .iter()
                .find(|(k, _)| k == "callee")
                .map(|(_, v)| v.as_str())
        })
        .collect();
    assert!(
        extern_calls.contains(&"cssl_quantum_qbind"),
        "qcircuit must emit qbind ; emitted = {extern_calls:?}"
    );
    assert!(
        extern_calls.contains(&"cssl_quantum_qsuperpose"),
        "qcircuit must emit qsuperpose ; emitted = {extern_calls:?}"
    );
    assert!(
        extern_calls.contains(&"cssl_quantum_qentangle"),
        "qcircuit must emit qentangle ; emitted = {extern_calls:?}"
    );
    assert!(
        extern_calls.contains(&"cssl_quantum_qmeasure"),
        "qcircuit must emit qmeasure ; emitted = {extern_calls:?}"
    );
}
