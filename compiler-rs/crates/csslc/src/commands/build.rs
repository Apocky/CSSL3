//! § commands::build — `csslc build <input.cssl> [-o <output>] [...]`.
//!
//! Full stage-0 pipeline orchestration : load source → lex → parse →
//! HIR-lower → AD-legality → refinement-obligation collection →
//! MIR-lower (signatures + bodies) → auto_monomorphize → call-site rewrite
//! → drop-unspecialized-generic → cranelift-cgen.
//!
//! Real object-file emission lands in S6-A3 ; this slice writes a
//! diagnostic placeholder file at the requested `--output` path so the
//! "completes without error" success-gate is observable.

use std::path::Path;
use std::process::ExitCode;

use crate::cli::{BuildArgs, EmitMode};
use crate::diag;
use crate::exit_code;

pub fn run(args: &BuildArgs) -> ExitCode {
    let source = match std::fs::read_to_string(&args.input) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("csslc: {}", diag::fs_error(&args.input, &e));
            return ExitCode::from(exit_code::USER_ERROR);
        }
    };
    run_with_source(&args.input, &source, args)
}

/// Pipeline-orchestrated build. Splits out for in-process tests.
#[allow(clippy::too_many_lines)]
pub fn run_with_source(path: &Path, source: &str, args: &BuildArgs) -> ExitCode {
    use cssl_ast::{SourceFile, SourceId, Surface};

    let file = SourceFile::new(
        SourceId::first(),
        path.display().to_string(),
        source,
        Surface::RustHybrid,
    );

    // ── frontend ──────────────────────────────────────────────────────
    let tokens = cssl_lex::lex(&file);
    let (cst, parse_bag) = cssl_parse::parse(&file, &tokens);
    let parse_errors = diag::emit_diagnostics(path, &parse_bag);
    if parse_errors > 0 {
        eprintln!("csslc: build failed — {parse_errors} parse error(s)");
        return ExitCode::from(exit_code::USER_ERROR);
    }

    let (hir_mod, interner, lower_bag) = cssl_hir::lower_module(&file, &cst);
    let lower_errors = diag::emit_diagnostics(path, &lower_bag);
    if lower_errors > 0 {
        eprintln!("csslc: build failed — {lower_errors} HIR-lower error(s)");
        return ExitCode::from(exit_code::USER_ERROR);
    }

    // ── walkers (semantics) ───────────────────────────────────────────
    let ad_report = cssl_hir::check_ad_legality(&hir_mod, &interner);
    if !ad_report.diagnostics.is_empty() {
        for d in &ad_report.diagnostics {
            eprintln!("{}: error: [AD-LEGALITY] {}", path.display(), d.message(),);
        }
        eprintln!(
            "csslc: build failed — {} AD-legality error(s)",
            ad_report.diagnostics.len()
        );
        return ExitCode::from(exit_code::USER_ERROR);
    }

    // refinement-obligation collection (informational, does not block)
    let _obligations = cssl_hir::collect_refinement_obligations(&hir_mod, &interner);

    // ── MIR ───────────────────────────────────────────────────────────
    let lower_ctx = cssl_mir::LowerCtx::new(&interner);
    let mut mir_mod = cssl_mir::MirModule::new();
    for item in &hir_mod.items {
        if let cssl_hir::HirItem::Fn(f) = item {
            let mut mf = cssl_mir::lower_function_signature(&lower_ctx, f);
            cssl_mir::lower_fn_body(&interner, Some(&file), f, &mut mf);
            mir_mod.push_func(mf);
        }
    }

    // ── monomorphization quartet (D38..D50) ───────────────────────────
    let mono_report = cssl_mir::auto_monomorphize(&hir_mod, &interner, Some(&file));
    for spec in &mono_report.specializations {
        mir_mod.push_func(spec.clone());
    }
    cssl_mir::rewrite_generic_call_sites(&mut mir_mod, &mono_report.call_site_names);
    cssl_mir::drop_unspecialized_generic_fns(&mut mir_mod);

    // ── emission (placeholder per slice spec : real .o emit is S6-A3) ─
    let output_path = resolve_output_path(args, path);
    let mode_label = emit_mode_label(args.emit);
    let mir_fn_count = mir_mod.funcs.len();
    let placeholder = format!(
        "// CSSLv3 stage-0 build artifact (placeholder — S6-A2).\n\
         // The pipeline lex → parse → HIR → walkers → MIR → monomorphize ran clean.\n\
         //\n\
         // input         : {}\n\
         // emit-mode     : {}\n\
         // mir-fn-count  : {}\n\
         // opt-level     : {}\n\
         // target        : {}\n\
         //\n\
         // Real cranelift-object emission lands in S6-A3 ; linker invocation in S6-A4.\n\
         // Until then, this placeholder confirms the upstream pipeline composed without error.\n",
        path.display(),
        mode_label,
        mir_fn_count,
        args.opt_level,
        args.target.as_deref().unwrap_or("(host-default)"),
    );
    if let Err(e) = std::fs::write(&output_path, placeholder) {
        eprintln!(
            "csslc: error: cannot write output '{}' ({})",
            output_path.display(),
            e
        );
        return ExitCode::from(exit_code::USER_ERROR);
    }
    eprintln!(
        "csslc: build {} → {} : {} MIR fn(s) ({})",
        path.display(),
        output_path.display(),
        mir_fn_count,
        mode_label,
    );
    ExitCode::from(exit_code::SUCCESS)
}

fn resolve_output_path(args: &BuildArgs, input: &Path) -> std::path::PathBuf {
    if let Some(o) = &args.output {
        return o.clone();
    }
    // Default : <input-stem>.<ext> next to input.
    let stem = input.file_stem().and_then(|s| s.to_str()).unwrap_or("a");
    let ext = match args.emit {
        EmitMode::Mlir => "mlir",
        EmitMode::Spirv => "spv",
        EmitMode::Wgsl => "wgsl",
        EmitMode::Dxil => "dxil",
        EmitMode::Msl => "msl",
        EmitMode::Object => {
            if cfg!(target_os = "windows") {
                "obj"
            } else {
                "o"
            }
        }
        EmitMode::Exe => {
            if cfg!(target_os = "windows") {
                "exe"
            } else {
                "out"
            }
        }
    };
    let mut out = std::path::PathBuf::from(stem);
    out.set_extension(ext);
    out
}

const fn emit_mode_label(m: EmitMode) -> &'static str {
    match m {
        EmitMode::Mlir => "mlir",
        EmitMode::Spirv => "spirv",
        EmitMode::Wgsl => "wgsl",
        EmitMode::Dxil => "dxil",
        EmitMode::Msl => "msl",
        EmitMode::Object => "object",
        EmitMode::Exe => "exe",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn build_args(input: &str, output: &str) -> BuildArgs {
        BuildArgs {
            input: PathBuf::from(input),
            output: Some(PathBuf::from(output)),
            target: None,
            emit: EmitMode::Exe,
            opt_level: 0,
        }
    }

    #[test]
    fn build_minimal_module_writes_placeholder() {
        let src = "module com.apocky.examples.hello\n\
                   fn main() -> i32 { 42 }\n";
        let tmp_out =
            std::env::temp_dir().join(format!("csslc_build_test_{}.exe", std::process::id()));
        let args = build_args("hello.cssl", tmp_out.to_str().unwrap());
        let code = run_with_source(Path::new("hello.cssl"), src, &args);

        let ok: ExitCode = ExitCode::from(exit_code::SUCCESS);
        assert_eq!(format!("{code:?}"), format!("{ok:?}"));
        assert!(tmp_out.exists(), "placeholder output should exist");
        let written = std::fs::read_to_string(&tmp_out).unwrap();
        assert!(written.contains("CSSLv3 stage-0 build artifact"));
        assert!(written.contains("hello.cssl"));
        let _ = std::fs::remove_file(&tmp_out);
    }

    #[test]
    fn build_default_output_path_uses_input_stem() {
        // No --output passed ⇒ default to <stem>.<ext> in CWD ; we set
        // an explicit one in args here to keep the test hermetic.
        let args = BuildArgs {
            input: PathBuf::from("hello.cssl"),
            output: None,
            target: None,
            emit: EmitMode::Mlir,
            opt_level: 0,
        };
        let p = resolve_output_path(&args, &args.input);
        assert_eq!(p, PathBuf::from("hello.mlir"));
    }

    #[test]
    fn build_default_output_for_object_uses_platform_extension() {
        let args = BuildArgs {
            input: PathBuf::from("hello.cssl"),
            output: None,
            target: None,
            emit: EmitMode::Object,
            opt_level: 0,
        };
        let p = resolve_output_path(&args, &args.input);
        let ext = p.extension().unwrap().to_str().unwrap();
        assert!(matches!(ext, "obj" | "o"));
    }

    #[test]
    fn build_with_missing_file_returns_user_error() {
        let args = build_args("/nonexistent/foo.cssl", "/tmp/whatever.exe");
        let code = run(&args);
        let err: ExitCode = ExitCode::from(exit_code::USER_ERROR);
        assert_eq!(format!("{code:?}"), format!("{err:?}"));
    }

    #[test]
    fn emit_mode_label_canonical() {
        assert_eq!(emit_mode_label(EmitMode::Mlir), "mlir");
        assert_eq!(emit_mode_label(EmitMode::Exe), "exe");
        assert_eq!(emit_mode_label(EmitMode::Object), "object");
    }

    #[test]
    fn build_pipeline_runs_full_chain_on_empty_module() {
        let src = "module com.apocky.examples.empty\n";
        let tmp_out = std::env::temp_dir().join(format!("csslc_empty_{}.out", std::process::id()));
        let args = build_args("empty.cssl", tmp_out.to_str().unwrap());
        let code = run_with_source(Path::new("empty.cssl"), src, &args);
        let ok: ExitCode = ExitCode::from(exit_code::SUCCESS);
        assert_eq!(format!("{code:?}"), format!("{ok:?}"));
        let _ = std::fs::remove_file(&tmp_out);
    }
}
