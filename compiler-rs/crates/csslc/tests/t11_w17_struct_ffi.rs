//! § T11-W17-A · stage-0 struct-FFI codegen — csslc end-to-end test
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § COVERAGE
//!   Validates that real `.csl` fixtures with struct-typed FFI signatures
//!   pass the full csslc-check pipeline (lex → parse → HIR-lower → MIR-lower
//!   → codegen-signature-emit) without surfacing the legacy "non-scalar MIR
//!   type" error. The codegen step itself is exercised by the
//!   `cssl-cgen-cpu-cranelift` integration tests at
//!   `compiler-rs/crates/cssl-cgen-cpu-cranelift/tests/struct_ffi_codegen.rs`.
//!
//! § GATE-DEFINITION
//!   `check_run` returns SUCCESS ⇔ all pre-codegen passes accept the source.
//!   For struct-FFI signature validation the build-pipeline integration is
//!   the strongest signal that the W17-A advancement landed end-to-end.
//!
//! § DEFERRED to W17-B+
//!   - struct-field load/store body ops (currently `cssl.struct` op falls
//!     outside the cgen-cpu-cranelift body subset)
//!   - inline struct-construction in fn bodies (lower error)
//!
//! § REFERENCE
//!   - tests/fixtures/struct_ffi/runhandle_signature_only.csl   ← FFI signature only
//!
//! § ATTESTATION (PRIME_DIRECTIVE §11)
//!   t∞: ¬(hurt ∨ harm) .making-of-T11-W17-A @ (anyone ∨ anything ∨ anybody)

use std::path::PathBuf;
use std::process::ExitCode;

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

#[test]
fn t11_w17_runhandle_signature_only_passes_check() {
    // § The W17-A signature-side advancement : a fn signature
    // `fn end_run(handle: RunHandle) -> i32` referencing a newtype-u64
    // struct compiles end-to-end without `non-scalar MIR type` errors.
    assert_check_succeeds("struct_ffi/runhandle_signature_only.csl");
}
