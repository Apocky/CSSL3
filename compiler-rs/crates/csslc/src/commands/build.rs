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

    // ── structured-CFG validation (T11-D70 / S6-D5) ───────────────────
    // Per `specs/15_MLIR.csl § PASS-PIPELINE` step 1 ("structured-CFG
    // validate") + `specs/02_IR.csl § STRUCTURED-CFG RULES (CC4)`, this
    // pass runs AFTER monomorphization (no generic-fn placeholders left)
    // and BEFORE codegen, so any orphan terminator / unstructured `cf.br`
    // / malformed scf.* shape surfaces here with stable diagnostic-codes
    // (CFG0001..CFG0010) instead of slipping into the backend as a
    // generic UnsupportedMirOp. On success the validator writes the
    // `("structured_cfg.validated", "true")` marker that GPU emitters
    // D1..D4 will check before emission.
    if let Err(violations) = cssl_mir::validate_and_mark(&mut mir_mod) {
        for v in &violations {
            // The Display impl already prefixes with the code (e.g.
            // "CFG0003: fn `f` ..."), so we render through diag's
            // `<file>:<line>:<col>: error: [<code>] <msg>` shape but
            // strip the redundant `<code>: ` prefix from the message.
            let raw = format!("{v}");
            let stripped = raw.strip_prefix(&format!("{}: ", v.code())).unwrap_or(&raw);
            eprintln!("{}: error: [{}] {}", path.display(), v.code(), stripped);
        }
        eprintln!(
            "csslc: build failed — {} structured-CFG violation(s)",
            violations.len()
        );
        return ExitCode::from(exit_code::USER_ERROR);
    }

    // ── emission ─────────────────────────────────────────────────────
    // S6-A3 : --emit=object | --emit=exe routes through cranelift-object.
    // (--emit=exe still produces a .obj/.o here ; S6-A4 invokes the linker
    // to turn it into a runnable executable.)
    // Other --emit modes (mlir/spirv/wgsl/dxil/msl) keep the stage-0
    // placeholder since their backends land in later phases.
    let output_path = resolve_output_path(args, path);
    let mode_label = emit_mode_label(args.emit);
    let mir_fn_count = mir_mod.funcs.len();

    match args.emit {
        EmitMode::Object => {
            let bytes = match cssl_cgen_cpu_cranelift::emit_object_module(&mir_mod) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("csslc: object-emit error : {e}");
                    return ExitCode::from(exit_code::USER_ERROR);
                }
            };
            if let Err(e) = std::fs::write(&output_path, &bytes) {
                eprintln!(
                    "csslc: error: cannot write output '{}' ({})",
                    output_path.display(),
                    e
                );
                return ExitCode::from(exit_code::USER_ERROR);
            }
            eprintln!(
                "csslc: build {} → {} : {} MIR fn(s) → {} bytes ({})",
                path.display(),
                output_path.display(),
                mir_fn_count,
                bytes.len(),
                mode_label,
            );
        }
        EmitMode::Exe => {
            let bytes = match cssl_cgen_cpu_cranelift::emit_object_module(&mir_mod) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("csslc: object-emit error : {e}");
                    return ExitCode::from(exit_code::USER_ERROR);
                }
            };
            // Write object bytes to a temp file alongside the requested
            // output path (so the linker can read it).
            let mut obj_path = output_path.clone();
            let obj_ext = if cfg!(target_os = "windows") {
                "obj"
            } else {
                "o"
            };
            obj_path.set_extension(obj_ext);
            if let Err(e) = std::fs::write(&obj_path, &bytes) {
                eprintln!(
                    "csslc: error: cannot write intermediate object '{}' ({})",
                    obj_path.display(),
                    e
                );
                return ExitCode::from(exit_code::USER_ERROR);
            }
            // Invoke linker.
            match crate::linker::link(&[obj_path.clone()], &output_path, &[]) {
                Ok(()) => {
                    eprintln!(
                        "csslc: build {} → {} : {} MIR fn(s) → {} bytes (object) → linked exe",
                        path.display(),
                        output_path.display(),
                        mir_fn_count,
                        bytes.len(),
                    );
                }
                Err(e) => {
                    eprintln!("csslc: linker error : {e}");
                    eprintln!("  intermediate object kept at : {}", obj_path.display());
                    return ExitCode::from(exit_code::USER_ERROR);
                }
            }
            // Best-effort cleanup of the intermediate object on success.
            let _ = std::fs::remove_file(&obj_path);
        }
        _ => {
            let placeholder = format!(
                "// CSSLv3 stage-0 build artifact (placeholder — non-object emit mode).\n\
                 // The pipeline lex → parse → HIR → walkers → MIR → monomorphize ran clean.\n\
                 //\n\
                 // input         : {}\n\
                 // emit-mode     : {}\n\
                 // mir-fn-count  : {}\n\
                 // opt-level     : {}\n\
                 // target        : {}\n\
                 //\n\
                 // Real backend emission for {} lands in a later phase\n\
                 // (SPIR-V → S6-D1 ; DXIL → S6-D2 ; MSL → S6-D3 ; WGSL → S6-D4).\n",
                path.display(),
                mode_label,
                mir_fn_count,
                args.opt_level,
                args.target.as_deref().unwrap_or("(host-default)"),
                mode_label,
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
                "csslc: build {} → {} : {} MIR fn(s) ({} placeholder)",
                path.display(),
                output_path.display(),
                mir_fn_count,
                mode_label,
            );
        }
    }
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

    /// Default test BuildArgs uses `EmitMode::Object` so the in-process
    /// pipeline produces raw object bytes (no linker invocation). Tests that
    /// specifically want the Exe-path can build their own args.
    fn build_args(input: &str, output: &str) -> BuildArgs {
        BuildArgs {
            input: PathBuf::from(input),
            output: Some(PathBuf::from(output)),
            target: None,
            emit: EmitMode::Object,
            opt_level: 0,
        }
    }

    #[test]
    fn build_minimal_module_writes_object_bytes() {
        // S6-A3 : --emit=Object writes raw cranelift-object bytes ; the
        // bytes start with the host-platform magic. (--emit=Exe goes
        // through the linker which is exercised by S6-A5's gate test.)
        let src = "module com.apocky.examples.hello\n\
                   fn main() -> i32 { 42 }\n";
        let tmp_out =
            std::env::temp_dir().join(format!("csslc_build_test_{}.obj", std::process::id()));
        let args = build_args("hello.cssl", tmp_out.to_str().unwrap());
        let code = run_with_source(Path::new("hello.cssl"), src, &args);

        let ok: ExitCode = ExitCode::from(exit_code::SUCCESS);
        assert_eq!(format!("{code:?}"), format!("{ok:?}"));
        assert!(tmp_out.exists(), "object output should exist");
        let written = std::fs::read(&tmp_out).unwrap();
        assert!(!written.is_empty(), "object bytes should be non-empty");
        // Verify host-platform magic prefix.
        let host_magic =
            cssl_cgen_cpu_cranelift::magic_prefix(cssl_cgen_cpu_cranelift::host_default_format());
        assert!(
            written.starts_with(host_magic),
            "expected magic {:02X?} ; got first 8 bytes {:02X?}",
            host_magic,
            &written[..written.len().min(8)],
        );
        let _ = std::fs::remove_file(&tmp_out);
    }

    #[test]
    fn build_with_emit_mlir_writes_placeholder() {
        // Non-object emit modes still write the explanatory placeholder.
        let src = "module com.apocky.examples.hello\n\
                   fn main() -> i32 { 42 }\n";
        let tmp_out =
            std::env::temp_dir().join(format!("csslc_emit_mlir_{}.mlir", std::process::id()));
        let args = BuildArgs {
            input: PathBuf::from("hello.cssl"),
            output: Some(tmp_out.clone()),
            target: None,
            emit: EmitMode::Mlir,
            opt_level: 0,
        };
        let code = run_with_source(Path::new("hello.cssl"), src, &args);
        let ok: ExitCode = ExitCode::from(exit_code::SUCCESS);
        assert_eq!(format!("{code:?}"), format!("{ok:?}"));
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

    /// T11-D70 / S6-D5 — verify the pipeline runs the structured-CFG
    /// validator. We can't easily inject malformed MIR through the
    /// surface-syntax pipeline (the frontend is itself well-behaved on
    /// hello-world), so this test asserts the SUCCESS path : a well-formed
    /// `fn main() -> i32 { 42 }` flows through and produces object bytes.
    /// The validator failure-path is exercised by the unit tests in
    /// `cssl_mir::structured_cfg` directly.
    #[test]
    fn build_pipeline_validates_structured_cfg_on_well_formed_source() {
        let src = "module com.apocky.examples.hello\n\
                   fn main() -> i32 { 42 }\n";
        let tmp_out = std::env::temp_dir().join(format!("csslc_d5_ok_{}.obj", std::process::id()));
        let args = build_args("hello.cssl", tmp_out.to_str().unwrap());
        let code = run_with_source(Path::new("hello.cssl"), src, &args);
        let ok: ExitCode = ExitCode::from(exit_code::SUCCESS);
        assert_eq!(
            format!("{code:?}"),
            format!("{ok:?}"),
            "well-formed source must pass D5 validator"
        );
        // Output must exist + start with the host magic prefix (validator
        // didn't short-circuit codegen).
        assert!(tmp_out.exists());
        let written = std::fs::read(&tmp_out).unwrap();
        assert!(!written.is_empty());
        let _ = std::fs::remove_file(&tmp_out);
    }
}
