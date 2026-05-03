//! § T11-W18-CSSLC-ADVANCE2 — csslc end-to-end test
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § COVERAGE
//!   Two independent gaps surfaced by the prior W18 csslc-scalar-arith wave :
//!     1. `arith.remf` (float remainder) needed a libm callout (cranelift has
//!        no `frem` instruction). Pre-A2 the cgen-cpu-cranelift dispatch table
//!        rejected with "MIR op `arith.remf` ; not in stage-0 object-emit
//!        subset". Now lowered via `fmodf` (f32) / `fmod` (f64) Linkage::Import
//!        declared by `declare_fmod_imports_for_fn`.
//!     2. `if/else if/else` cascade with bare-ident yields surfaced
//!        `MirType::Opaque(!cssl.unresolved.<name>)` because branch sub-contexts
//!        started with empty param_vars/local_vars maps. Now sub() clones the
//!        binding tables (mirroring local_cells) so bare-ident yields resolve
//!        against the enclosing fn's bindings.
//!
//! § GATE-DEFINITION
//!   `assert_object_build_succeeds` returns SUCCESS with `--emit=object` ⇔ all
//!   passes accept the source AND the cranelift backend produces an object
//!   file. We target `--emit=object` (not `--emit=exe`) so we don't need a
//!   `main` and a host linker — codegen-success is the load-bearing signal.
//!
//! § FIXTURES
//!   - tests/fixtures/csslc_advance2/{remf_f32,remf_f64,remf_chained,
//!     remf_in_branch}.csl
//!   - tests/fixtures/csslc_advance2/{cascade_bare_param,cascade_bare_local,
//!     cascade_mixed_yield}.csl
//!
//! § ATTESTATION (PRIME_DIRECTIVE §11)
//!   t∞: ¬(hurt ∨ harm) .making-of-T11-W18-A2 @ (anyone ∨ anything ∨ anybody)

use std::path::PathBuf;
use std::process::ExitCode;

use csslc::cli::{Backend, BuildArgs, EmitMode};
use csslc::commands::{build, check};
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

fn assert_object_build_succeeds(rel: &str) {
    let path = fixture_path(rel);
    let source = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("fixture '{rel}' read-error: {e}"));
    let mut out = path.clone();
    out.set_extension("obj");
    let args = BuildArgs {
        input: path.clone(),
        output: Some(out),
        target: None,
        emit: EmitMode::Object,
        opt_level: 0,
        backend: Backend::Cranelift,
        module_paths: Vec::new(),
    };
    let code = build::run_with_source(&path, &source, &args);
    let ok: ExitCode = ExitCode::from(exit_code::SUCCESS);
    assert_eq!(
        format!("{code:?}"),
        format!("{ok:?}"),
        "fixture '{rel}' must produce an object file ; got non-success exit"
    );
}

// ── arith.remf : check-only gates ─────────────────────────────────────

#[test]
fn t11_w18_a2_remf_f32_check_passes() {
    assert_check_succeeds("csslc_advance2/remf_f32.csl");
}

#[test]
fn t11_w18_a2_remf_f64_check_passes() {
    assert_check_succeeds("csslc_advance2/remf_f64.csl");
}

#[test]
fn t11_w18_a2_remf_chained_check_passes() {
    assert_check_succeeds("csslc_advance2/remf_chained.csl");
}

#[test]
fn t11_w18_a2_remf_in_branch_check_passes() {
    assert_check_succeeds("csslc_advance2/remf_in_branch.csl");
}

// ── arith.remf : object-emit gates (the actual A2 advancement) ────────

#[test]
fn t11_w18_a2_remf_f32_emits_object() {
    // Pre-A2 : rejected with "MIR op `arith.remf` ; not in stage-0 object-emit
    // subset". Post-A2 : routed through libm fmodf via FmodImports pre-scan.
    assert_object_build_succeeds("csslc_advance2/remf_f32.csl");
}

#[test]
fn t11_w18_a2_remf_f64_emits_object() {
    // Pre-A2 : same rejection at f64 width. Post-A2 : libm `fmod` symbol.
    assert_object_build_succeeds("csslc_advance2/remf_f64.csl");
}

#[test]
fn t11_w18_a2_remf_chained_emits_object() {
    // Multiple arith.remf in one fn — exercises the import-deduplication path
    // (one FuncRef per width, multiple call sites share it).
    assert_object_build_succeeds("csslc_advance2/remf_chained.csl");
}

#[test]
fn t11_w18_a2_remf_in_branch_emits_object() {
    // arith.remf nested in scf.if then-branch — tests the 1-level region
    // descent in declare_fmod_imports_for_fn's pre-scan walker.
    assert_object_build_succeeds("csslc_advance2/remf_in_branch.csl");
}

// ── if/else cascade : check-only gates ────────────────────────────────

#[test]
fn t11_w18_a2_cascade_bare_param_check_passes() {
    assert_check_succeeds("csslc_advance2/cascade_bare_param.csl");
}

#[test]
fn t11_w18_a2_cascade_bare_local_check_passes() {
    assert_check_succeeds("csslc_advance2/cascade_bare_local.csl");
}

#[test]
fn t11_w18_a2_cascade_mixed_yield_check_passes() {
    assert_check_succeeds("csslc_advance2/cascade_mixed_yield.csl");
}

// ── if/else cascade : object-emit gates ───────────────────────────────

#[test]
fn t11_w18_a2_cascade_bare_param_emits_object() {
    // Pre-A2 : sub_ctx had empty param_vars, so `x`/`y`/`z` yields routed
    // through lower_path's unresolved fallback and emitted cssl.path_ref ops
    // with MirType::Opaque(!cssl.unresolved.<name>) — codegen rejected.
    // Post-A2 : sub_ctx clones param_vars, bare yields resolve cleanly.
    assert_object_build_succeeds("csslc_advance2/cascade_bare_param.csl");
}

#[test]
fn t11_w18_a2_cascade_bare_local_emits_object() {
    // Same fix surface — local_vars now flows into sub_ctx alongside
    // param_vars (and the existing local_cells inheritance).
    assert_object_build_succeeds("csslc_advance2/cascade_bare_local.csl");
}

#[test]
fn t11_w18_a2_cascade_mixed_yield_emits_object() {
    // Mixed param + local + literal yields in one cascade — confirms the
    // fix doesn't regress the literal-yield path (which never needed
    // binding-table lookup).
    assert_object_build_succeeds("csslc_advance2/cascade_mixed_yield.csl");
}
