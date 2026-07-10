//! § T11-W20-RENDERER-TRANSPORT — W-1 renderer transport integration gates
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § COVERAGE
//!   Product-path validation for the renderer refactor foundation:
//!     1. New renderer CSSL modules parse/check cleanly.
//!     2. `gpu_transport_abi_exhaustive.cssl` lowers every W-1 GPU transport
//!        symbol into a `cssl.gpu.*` MIR op.
//!     3. Exhaustive/direct/1M transport examples build to object through
//!        Cranelift, proving object import generation for the new ABI.
//!
//! § BOUNDARY
//!   These tests stop at object emission to stay deterministic under cargo
//!   test. Runtime exe-link/run gates live in `scripts/dev/validate_renderer_transport.ps1`.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use cssl_ast::{SourceFile, SourceId, Surface};
use cssl_hir::lower_module;
use cssl_mir::{lower_fn_body, lower_module_signatures, LowerCtx, MirModule};
use csslc::cli::{Backend, BuildArgs, EmitMode};
use csslc::commands::{build, check};
use csslc::exit_code;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("..")
        .canonicalize()
        .expect("repo root must canonicalize")
}

fn repo_file(rel: &str) -> PathBuf {
    repo_root().join(rel)
}

fn assert_check_succeeds(rel: &str) {
    let path = repo_file(rel);
    let source = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("source '{rel}' read-error: {e}"));
    let code = check::run_with_source(&path, &source);
    let ok = ExitCode::from(exit_code::SUCCESS);
    assert_eq!(format!("{code:?}"), format!("{ok:?}"), "{rel} must check");
}

fn assert_object_build_succeeds(rel: &str) {
    let path = repo_file(rel);
    let source = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("source '{rel}' read-error: {e}"));
    let out = std::env::temp_dir().join(format!(
        "csslc_renderer_transport_{}.obj",
        rel.replace(['/', '\\'], "_")
    ));
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
    let ok = ExitCode::from(exit_code::SUCCESS);
    assert_eq!(
        format!("{code:?}"),
        format!("{ok:?}"),
        "{rel} must emit object"
    );
}

fn lower_repo_file_to_mir(path: &Path) -> MirModule {
    let source_text = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("source '{}' read-error: {e}", path.display()));
    let source = SourceFile::new(
        SourceId::first(),
        path.to_string_lossy().as_ref(),
        &source_text,
        Surface::RustHybrid,
    );
    let tokens = cssl_lex::lex(&source);
    let (cst, parse_diags) = cssl_parse::parse(&source, &tokens);
    assert_eq!(parse_diags.error_count(), 0, "parse errors: {parse_diags:?}");
    let (hir, interner, hir_diags) = lower_module(&source, &cst);
    assert_eq!(hir_diags.error_count(), 0, "HIR errors: {hir_diags:?}");
    let lower_ctx = LowerCtx::new(&interner);
    let mut mir = lower_module_signatures(&lower_ctx, &hir);
    for item in &hir.items {
        if let cssl_hir::HirItem::Fn(hir_fn) = item {
            let fn_name = interner.resolve(hir_fn.name);
            let mir_fn = mir
                .funcs
                .iter_mut()
                .find(|f| f.name == fn_name)
                .unwrap_or_else(|| panic!("MIR fn '{fn_name}' missing"));
            lower_fn_body(&interner, Some(&source), hir_fn, mir_fn);
        }
    }
    mir
}

fn mir_contains_op(mir: &MirModule, op_name: &str) -> bool {
    mir.funcs.iter().any(|f| {
        f.body
            .blocks
            .iter()
            .any(|b| b.ops.iter().any(|op| op.name == op_name))
    })
}

#[test]
fn renderer_transport_new_modules_check() {
    for rel in [
        "engine/conventions.cssl",
        "stdlib/gpu_transport.cssl",
        "stdlib/vec_mut.cssl",
        "engine/frame_arena.cssl",
        "engine/mesh.cssl",
        "engine/instance.cssl",
        "engine/profiler.cssl",
        "engine/frame_graph.cssl",
        "examples/hello_million_entities.cssl",
        "examples/gpu_transport_abi_smoke.cssl",
        "examples/gpu_transport_abi_exhaustive.cssl",
        "examples/million_entities_transport_smoke.cssl",
    ] {
        assert_check_succeeds(rel);
    }
}

#[test]
fn renderer_transport_exhaustive_gpu_ops_lower_to_mir() {
    let mir = lower_repo_file_to_mir(&repo_file("examples/gpu_transport_abi_exhaustive.cssl"));
    for op in [
        "cssl.gpu.buffer_create",
        "cssl.gpu.buffer_destroy",
        "cssl.gpu.buffer_map",
        "cssl.gpu.buffer_unmap",
        "cssl.gpu.buffer_upload",
        "cssl.gpu.cmd_buf_begin",
        "cssl.gpu.cmd_buf_end",
        "cssl.gpu.cmd_buf_bind_pipeline",
        "cssl.gpu.cmd_buf_bind_vbuf",
        "cssl.gpu.cmd_buf_bind_ibuf",
        "cssl.gpu.cmd_buf_bind_descriptor",
        "cssl.gpu.cmd_buf_push_constants",
        "cssl.gpu.cmd_buf_draw_indexed",
        "cssl.gpu.cmd_buf_draw_indirect",
        "cssl.gpu.cmd_buf_dispatch",
        "cssl.gpu.cmd_buf_submit_v2",
    ] {
        assert!(mir_contains_op(&mir, op), "expected MIR op {op}");
    }
}

#[test]
fn renderer_transport_examples_emit_objects() {
    for rel in [
        "examples/gpu_transport_abi_smoke.cssl",
        "examples/gpu_transport_abi_exhaustive.cssl",
        "examples/million_entities_transport_smoke.cssl",
        "examples/hello_million_entities.cssl",
    ] {
        assert_object_build_succeeds(rel);
    }
}
