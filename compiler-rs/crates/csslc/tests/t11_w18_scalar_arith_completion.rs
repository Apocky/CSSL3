//! § T11-W18-CSSLC-SCALAR-ARITH-COMPLETION — csslc end-to-end test
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § COVERAGE
//!   Pre-W18 csslc could parse + type-check float-comparison / integer-bitwise
//!   / integer-shift / float-negation source, but the cgen-cpu-cranelift body-
//!   emit subset rejected the resulting MIR with one of :
//!       - "MIR op `arith.negf` ; not in stage-0 object-emit subset"
//!       - "MIR op `arith.subi_neg` ; not in stage-0 object-emit subset"
//!       - "MIR op `arith.andi` ; not in stage-0 object-emit subset"
//!       - "MIR op `arith.shli` ; not in stage-0 object-emit subset"
//!       - "MIR op `arith.xori_not` ; not in stage-0 object-emit subset"
//!       - "fn `cmp_lt` cranelift error : Verifier errors"   ← cmpi on f32
//!
//!   W18 closes the gap : adds dispatch arms for every scalar-arith op
//!   body_lower can emit, fixes float-cmp tag selection, and honors the
//!   float-literal type-suffix at lowering time.
//!
//! § GATE-DEFINITION
//!   `build_run` returns SUCCESS with `--emit=object` ⇔ all passes accept
//!   the source AND the cranelift backend produces an object file. We
//!   target `--emit=object` (not `--emit=exe`) so we don't need a `main`
//!   and a host linker — the codegen-success-or-failure is the load-bearing
//!   signal for this slice.
//!
//! § DEFERRED to W19+
//!   - `arith.remf` (float remainder) — cranelift has no `frem` instr ;
//!     proper lowering needs a libm callout. Not yet wired.
//!   - SIMD lane-wise scalar-arith on the `vec3<f32>` family — covered
//!     under the existing simd_abi pipeline.
//!
//! § REFERENCE
//!   - tests/fixtures/scalar_arith/{float_cmp,float_neg,bitwise,
//!     float_lit_suffix,mixed}.csl
//!
//! § ATTESTATION (PRIME_DIRECTIVE §11)
//!   t∞: ¬(hurt ∨ harm) .making-of-T11-W18 @ (anyone ∨ anything ∨ anybody)

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
    // Place the object next to the fixture so multiple parallel test
    // invocations don't collide on a single shared output path.
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

// ── check-only gates ──────────────────────────────────────────────────

#[test]
fn t11_w18_float_cmp_check_passes() {
    assert_check_succeeds("scalar_arith/float_cmp.csl");
}

#[test]
fn t11_w18_float_neg_check_passes() {
    assert_check_succeeds("scalar_arith/float_neg.csl");
}

#[test]
fn t11_w18_bitwise_check_passes() {
    assert_check_succeeds("scalar_arith/bitwise.csl");
}

#[test]
fn t11_w18_float_lit_suffix_check_passes() {
    assert_check_succeeds("scalar_arith/float_lit_suffix.csl");
}

#[test]
fn t11_w18_mixed_check_passes() {
    assert_check_succeeds("scalar_arith/mixed.csl");
}

// ── object-emit gates (the actual W18 advancement) ────────────────────

#[test]
fn t11_w18_float_cmp_emits_object() {
    // Pre-W18 : panicked with cranelift Verifier errors because cmp on
    // f32 went through `arith.cmpi_*` (integer-cmp tag). Now routes
    // through `arith.cmpf_o*` and the body-emit `b.ins().fcmp(...)` arm.
    assert_object_build_succeeds("scalar_arith/float_cmp.csl");
}

#[test]
fn t11_w18_float_neg_emits_object() {
    // Pre-W18 : "MIR op `arith.negf` ; not in stage-0 object-emit subset".
    assert_object_build_succeeds("scalar_arith/float_neg.csl");
}

#[test]
fn t11_w18_bitwise_emits_object() {
    // Pre-W18 : "MIR op `arith.andi` ; not in stage-0 object-emit subset"
    // (and the same for ori/xori/shli/shrsi/xori_not).
    assert_object_build_succeeds("scalar_arith/bitwise.csl");
}

#[test]
fn t11_w18_float_lit_suffix_emits_object() {
    // Pre-W18 : `3.14f64` lowered as F32 ; if the use-site needed F64
    // there was an implicit-coercion gap that surfaced as a type-mismatch
    // at MIR-construction. Now the literal carries its declared width.
    assert_object_build_succeeds("scalar_arith/float_lit_suffix.csl");
}

#[test]
fn t11_w18_mixed_emits_object() {
    // Composite : exercises every newly-wired op in one fixture so any
    // single-arm regression surfaces here. Real LoA-substrate hot paths
    // (KAN sign-flip, morton-pack, FNV-1a hash) all collapse to this set.
    assert_object_build_succeeds("scalar_arith/mixed.csl");
}
