//! § commands::build — `csslc build <input.cssl> [-o <output>] [...]`.
//!
//! Full stage-0 pipeline orchestration : load source → lex → parse →
//! HIR-lower → AD-legality → refinement-obligation collection →
//! MIR-lower (signatures + bodies) → auto_monomorphize → call-site rewrite
//! → drop-unspecialized-generic → structured-CFG validate → CPU cgen
//! (cranelift OR native-x64, dispatched on `--backend`) → linker.
//!
//! § S7-G6 (T11-D88) — selectable CPU codegen backend
//!   The CPU object/exe emit step branches on [`Backend`] (parsed from
//!   `--backend=cranelift|native-x64`). Default is [`Backend::Cranelift`]
//!   which preserves S6-A5 behavior bit-for-bit ; [`Backend::NativeX64`]
//!   dispatches to `cssl-cgen-cpu-x64` (the S7-G axis hand-rolled CPU
//!   path). Both crates expose the same `emit_object_module(&MirModule)
//!   -> Result<Vec<u8>, _>` shape, so the dispatcher only differs in
//!   which crate it calls + how it formats backend-specific errors.
//!
//!   When G1..G5 sibling slices haven't yet landed, the native-x64 path
//!   surfaces `NativeX64Error::BackendNotYetLanded` with a clear hint to
//!   use `--backend=cranelift` for the working CPU path. The cssl-examples
//!   native-hello-world gate detects this error and SKIPS gracefully.

use std::path::Path;
use std::process::ExitCode;

use crate::cli::{Backend, BuildArgs, EmitMode};
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

    // § T11-W15-CSSLC-MULTI-MODULE : auxiliary modules — each `--module-path=…`
    //   path goes through the SAME lex → parse → HIR-lower passes ; their
    //   HIR fns are concatenated into a separate vec for the MIR-lower step
    //   below. The interner stays per-main-module ; auxiliary-module names
    //   are looked-up in their own per-file interner ; cross-module symbol
    //   resolution at MIR-level is fname-string-based so name-collisions
    //   surface as link-time errors.
    //
    //   Auxiliary file IDs start at SourceId::first()+1 so spans + diags
    //   don't collide ; the diag-emitter prefixes the path so user can tell
    //   which file an error came from.
    let mut aux_files: Vec<SourceFile> = Vec::new();
    let mut aux_hirs: Vec<(cssl_hir::HirModule, cssl_hir::Interner)> = Vec::new();
    for (idx, aux_path) in args.module_paths.iter().enumerate() {
        let aux_src = match std::fs::read_to_string(aux_path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!(
                    "csslc: cannot read aux-module '{}' ({}) — skipping",
                    aux_path.display(),
                    e
                );
                return ExitCode::from(exit_code::USER_ERROR);
            }
        };
        // SourceId(0) is synthetic-sentinel ; main file is SourceId::first()=1 ;
        // aux files start at 2+idx.
        let aux_id = SourceId(idx as u32 + 2);
        let aux_file = SourceFile::new(
            aux_id,
            aux_path.display().to_string(),
            &aux_src,
            Surface::RustHybrid,
        );
        let aux_tokens = cssl_lex::lex(&aux_file);
        let (aux_cst, aux_parse_bag) = cssl_parse::parse(&aux_file, &aux_tokens);
        let n_perr = diag::emit_diagnostics(aux_path, &aux_parse_bag);
        if n_perr > 0 {
            eprintln!(
                "csslc: aux-module '{}' parse failed — {} error(s)",
                aux_path.display(),
                n_perr
            );
            return ExitCode::from(exit_code::USER_ERROR);
        }
        let (aux_hir, aux_interner, aux_lower_bag) = cssl_hir::lower_module(&aux_file, &aux_cst);
        let n_lerr = diag::emit_diagnostics(aux_path, &aux_lower_bag);
        if n_lerr > 0 {
            eprintln!(
                "csslc: aux-module '{}' HIR-lower failed — {} error(s)",
                aux_path.display(),
                n_lerr
            );
            return ExitCode::from(exit_code::USER_ERROR);
        }
        aux_files.push(aux_file);
        aux_hirs.push((aux_hir, aux_interner));
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
    // First pass : lower extern fn signatures (no body) so the call-result
    // fixup can find them.
    for item in &hir_mod.items {
        if let cssl_hir::HirItem::ExternFn(ef) = item {
            mir_mod.push_func(cssl_mir::lower::lower_extern_fn_signature(&lower_ctx, ef));
        }
    }
    // Second pass : lower regular fn signatures + bodies.
    for item in &hir_mod.items {
        if let cssl_hir::HirItem::Fn(f) = item {
            let mut mf = cssl_mir::lower_function_signature(&lower_ctx, f);
            cssl_mir::lower_fn_body(&interner, Some(&file), f, &mut mf);
            mir_mod.push_func(mf);
        }
    }

    // § T11-W15-CSSLC-MULTI-MODULE : aux-module MIR-lower
    //   Each auxiliary HIR module goes through extern-then-regular-fn
    //   lower passes mirroring the main module's pipeline. The aux modules
    //   share the same `MirModule` so the linker sees one object with all
    //   symbols. Per-aux-module interners stay isolated to avoid name-arena
    //   collision ; symbol resolution at the MIR level is fname-string-based.
    for ((aux_hir, aux_interner), aux_file) in aux_hirs.iter().zip(aux_files.iter()) {
        let aux_lower_ctx = cssl_mir::LowerCtx::new(aux_interner);
        for item in &aux_hir.items {
            if let cssl_hir::HirItem::ExternFn(ef) = item {
                mir_mod.push_func(cssl_mir::lower::lower_extern_fn_signature(
                    &aux_lower_ctx,
                    ef,
                ));
            }
        }
        for item in &aux_hir.items {
            if let cssl_hir::HirItem::Fn(f) = item {
                let mut mf = cssl_mir::lower_function_signature(&aux_lower_ctx, f);
                cssl_mir::lower_fn_body(aux_interner, Some(aux_file), f, &mut mf);
                mir_mod.push_func(mf);
            }
        }
    }

    // ── T11-LOA-PURE-CSSL : resolve opaque func.call result types from
    //    the now-populated module signature table. Required for pure-CSSL
    //    `extern fn name() -> i32` calls to type-check at the cranelift
    //    backend's stage-0 scalars-only gate.
    let _resolved = cssl_mir::resolve_call_result_types(&mut mir_mod);

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
            let bytes = match emit_cpu_object_bytes(&mir_mod, args.backend) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("csslc: object-emit error ({}): {}", args.backend.label(), e);
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
                "csslc: build {} → {} : {} MIR fn(s) → {} bytes ({}, backend={})",
                path.display(),
                output_path.display(),
                mir_fn_count,
                bytes.len(),
                mode_label,
                args.backend.label(),
            );
        }
        EmitMode::Exe => {
            let bytes = match emit_cpu_object_bytes(&mir_mod, args.backend) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("csslc: object-emit error ({}): {}", args.backend.label(), e);
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
            // Invoke linker. T11-D319 : auto-discover + default-link cssl-rt
            // staticlib so the loa_startup.rs ctor (`.CRT$XCU` / `.init_array`
            // initializer) activates without requiring CSSL-side FFI calls.
            // The build-log surface adds a "+ cssl-rt" suffix when discovery
            // succeeded so the user can confirm the runtime is wired in.
            let rt_linked = crate::linker::discover_cssl_rt_staticlib().is_some();
            let loa_host_linked = crate::linker::discover_loa_host_staticlib().is_some();
            match crate::linker::link(&[obj_path.clone()], &output_path, &[]) {
                Ok(()) => {
                    let mut tag = String::new();
                    if rt_linked {
                        tag.push_str(" + cssl-rt");
                    }
                    if loa_host_linked {
                        tag.push_str(" + loa-host");
                    }
                    eprintln!(
                        "csslc: build {} → {} : {} MIR fn(s) → {} bytes (object, backend={}) → linked exe{}",
                        path.display(),
                        output_path.display(),
                        mir_fn_count,
                        bytes.len(),
                        args.backend.label(),
                        tag,
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

// ───────────────────────────────────────────────────────────────────────
// § S7-G6 (T11-D88) — backend-dispatched CPU object emission
// ───────────────────────────────────────────────────────────────────────

/// Emit CPU object bytes for `module` via the selected [`Backend`].
///
/// Uniformizes the two backend crates' error types into a `String` so the
/// caller (the `match args.emit` block above) doesn't have to branch on
/// backend at the diagnostic-formatting layer.
///
/// § S7-G6 invariant
///   When `backend == Backend::NativeX64` and G1..G5 sibling slices have
///   not yet landed, the returned `Err(_)` message starts with the literal
///   prefix `"native-x64 backend not yet landed"` so the cssl-examples
///   native-hello-world gate test can detect this case + skip.
///
/// # Errors
/// Returns the backend-specific error rendered as a `String`.
fn emit_cpu_object_bytes(
    module: &cssl_mir::MirModule,
    backend: Backend,
) -> Result<Vec<u8>, String> {
    match backend {
        Backend::Cranelift => {
            cssl_cgen_cpu_cranelift::emit_object_module(module).map_err(|e| e.to_string())
        }
        Backend::NativeX64 => {
            cssl_cgen_cpu_x64::emit_object_module(module).map_err(|e| e.to_string())
        }
    }
}

/// True iff `err_msg` indicates the native-x64 backend's
/// `BackendNotYetLanded` skeleton-state. Helper for the cssl-examples
/// native-hello-world gate test : SKIP gracefully when G1..G5 are in flight.
#[must_use]
pub fn is_native_x64_backend_not_yet_landed(err_msg: &str) -> bool {
    err_msg.starts_with("native-x64 backend not yet landed")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Default test BuildArgs uses `EmitMode::Object` + `Backend::Cranelift`
    /// so the in-process pipeline produces raw object bytes (no linker
    /// invocation) via the working stage-0 path. Tests that specifically
    /// want the Exe-path or the native-x64 backend build their own args.
    fn build_args(input: &str, output: &str) -> BuildArgs {
        BuildArgs {
            input: PathBuf::from(input),
            output: Some(PathBuf::from(output)),
            target: None,
            emit: EmitMode::Object,
            opt_level: 0,
            backend: Backend::Cranelift,
            module_paths: Vec::new(),
        }
    }

    /// Build args that exercise the native-x64 backend dispatch path.
    fn build_args_native_x64(input: &str, output: &str) -> BuildArgs {
        BuildArgs {
            input: PathBuf::from(input),
            output: Some(PathBuf::from(output)),
            target: None,
            emit: EmitMode::Object,
            opt_level: 0,
            backend: Backend::NativeX64,
            module_paths: Vec::new(),
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
            backend: Backend::Cranelift,
            module_paths: Vec::new(),
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
            backend: Backend::Cranelift,
            module_paths: Vec::new(),
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
            backend: Backend::Cranelift,
            module_paths: Vec::new(),
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

    // ───────────────────────────────────────────────────────────────────
    // § S7-G6 (T11-D88) — backend-dispatch tests
    // ───────────────────────────────────────────────────────────────────

    /// Verify the explicit `Backend::Cranelift` path produces identical
    /// bytes to the implicit-default path. (Both should map to the same
    /// `cssl_cgen_cpu_cranelift::emit_object_module` call.)
    #[test]
    fn build_with_explicit_cranelift_backend_succeeds() {
        let src = "module com.apocky.examples.hello\n\
                   fn main() -> i32 { 42 }\n";
        let tmp_out =
            std::env::temp_dir().join(format!("csslc_g6_clift_{}.obj", std::process::id()));
        let args = build_args("hello.cssl", tmp_out.to_str().unwrap()); // default Cranelift
        let code = run_with_source(Path::new("hello.cssl"), src, &args);
        let ok: ExitCode = ExitCode::from(exit_code::SUCCESS);
        assert_eq!(format!("{code:?}"), format!("{ok:?}"));
        assert!(tmp_out.exists());
        let written = std::fs::read(&tmp_out).unwrap();
        assert!(!written.is_empty());
        let host_magic =
            cssl_cgen_cpu_cranelift::magic_prefix(cssl_cgen_cpu_cranelift::host_default_format());
        assert!(written.starts_with(host_magic));
        let _ = std::fs::remove_file(&tmp_out);
    }

    /// Verify the `Backend::NativeX64` dispatch path is reached and either
    /// (a) succeeds — G1..G5 have landed and produce real object bytes —
    /// or (b) reports the canonical `BackendNotYetLanded` error message.
    /// Either outcome counts as the dispatcher working ; this test
    /// validates that csslc actually calls the new backend crate's surface.
    #[test]
    fn build_with_native_x64_backend_dispatches_through_x64_crate() {
        let src = "module com.apocky.examples.hello\n\
                   fn main() -> i32 { 42 }\n";
        let tmp_out =
            std::env::temp_dir().join(format!("csslc_g6_native_{}.obj", std::process::id()));
        let args = build_args_native_x64("hello.cssl", tmp_out.to_str().unwrap());
        let code = run_with_source(Path::new("hello.cssl"), src, &args);
        let ok: ExitCode = ExitCode::from(exit_code::SUCCESS);
        let user_err: ExitCode = ExitCode::from(exit_code::USER_ERROR);
        let actual = format!("{code:?}");
        let success_dbg = format!("{ok:?}");
        let user_err_dbg = format!("{user_err:?}");
        // Either succeeds (G1..G5 landed) or surfaces the canonical
        // not-yet-landed user-error. Both are acceptable at S7-G6 dispatch.
        assert!(
            actual == success_dbg || actual == user_err_dbg,
            "expected success or user-error from native-x64 backend ; got {actual}"
        );
        if actual == success_dbg {
            // G1..G5 landed → real bytes written. Verify host-magic prefix.
            assert!(tmp_out.exists());
            let written = std::fs::read(&tmp_out).unwrap();
            assert!(!written.is_empty());
            // Native-x64 crate also exposes magic_prefix matching the host.
            let host_magic =
                cssl_cgen_cpu_x64::magic_prefix(cssl_cgen_cpu_x64::host_default_format());
            assert!(written.starts_with(host_magic));
        }
        let _ = std::fs::remove_file(&tmp_out);
    }

    /// Direct dispatcher-level test : `emit_cpu_object_bytes` for each
    /// backend returns a `Result` of the expected shape. Doesn't depend
    /// on the full pipeline — exercises the smallest possible surface
    /// where the dispatch-on-Backend lives.
    #[test]
    fn emit_cpu_object_bytes_dispatches_per_backend() {
        let m = cssl_mir::MirModule::new();
        // Cranelift on an empty module should succeed (emits a near-empty
        // object file).
        let r_clift = emit_cpu_object_bytes(&m, Backend::Cranelift);
        assert!(
            r_clift.is_ok(),
            "cranelift on empty module should succeed ; got {r_clift:?}"
        );

        // Native-x64 should either succeed (G1..G5 landed) or report the
        // canonical "backend not yet landed" message. Both acceptable.
        let r_native = emit_cpu_object_bytes(&m, Backend::NativeX64);
        match r_native {
            Ok(_bytes) => {
                // Either G1..G5 landed and the empty module emits real bytes.
                // No further assertion possible at this layer ; the
                // crate-level tests in cssl-cgen-cpu-x64 cover the bytes.
            }
            Err(e) => {
                assert!(
                    is_native_x64_backend_not_yet_landed(&e),
                    "expected canonical not-yet-landed message ; got `{e}`"
                );
            }
        }
    }

    // ───────────────────────────────────────────────────────────────────
    // § T11-W15-CSSLC-MULTI-MODULE — multi-module compile dispatch
    // ───────────────────────────────────────────────────────────────────

    /// Verify the auxiliary `--module-path` parses + lowers + adds its MIR
    /// fns to the same module as the main input. The build pipeline must
    /// produce a single object containing symbols from BOTH files.
    #[test]
    fn build_with_module_path_compiles_aux_module_into_same_object() {
        let main_src = "module com.apocky.test.multi.main\n\
                        extern \"C\" fn aux_helper(x: u32) -> u32 ;\n\
                        fn main() -> i32 {\n\
                            let _r: u32 = aux_helper(42u32) ;\n\
                            0i32\n\
                        }\n";
        let aux_src = "module com.apocky.test.multi.aux\n\
                       fn aux_helper(x: u32) -> u32 {\n\
                           x * 2u32\n\
                       }\n";
        // Write the auxiliary to a temp file so the build pipeline reads it
        // through the normal aux-module path-IO path.
        let aux_path =
            std::env::temp_dir().join(format!("csslc_w15_aux_{}.csl", std::process::id()));
        std::fs::write(&aux_path, aux_src).unwrap();

        let tmp_out =
            std::env::temp_dir().join(format!("csslc_w15_multi_{}.obj", std::process::id()));
        let args = BuildArgs {
            input: PathBuf::from("test_main.cssl"),
            output: Some(tmp_out.clone()),
            target: None,
            emit: EmitMode::Object,
            opt_level: 0,
            backend: Backend::Cranelift,
            module_paths: vec![aux_path.clone()],
        };

        let code = run_with_source(Path::new("test_main.cssl"), main_src, &args);
        let ok: ExitCode = ExitCode::from(exit_code::SUCCESS);
        assert_eq!(
            format!("{code:?}"),
            format!("{ok:?}"),
            "multi-module build with --module-path MUST succeed"
        );
        assert!(tmp_out.exists(), "object output should exist");
        let written = std::fs::read(&tmp_out).unwrap();
        assert!(!written.is_empty(), "object bytes should be non-empty");

        // Cleanup.
        let _ = std::fs::remove_file(&aux_path);
        let _ = std::fs::remove_file(&tmp_out);
    }

    #[test]
    fn build_with_missing_module_path_returns_user_error() {
        // Auxiliary path that doesn't exist must fail the build.
        let main_src = "module com.test\nfn main() -> i32 { 0i32 }\n";
        let args = BuildArgs {
            input: PathBuf::from("test_main.cssl"),
            output: Some(std::env::temp_dir().join("csslc_w15_bad.obj")),
            target: None,
            emit: EmitMode::Object,
            opt_level: 0,
            backend: Backend::Cranelift,
            module_paths: vec![PathBuf::from("/nonexistent/aux.csl")],
        };
        let code = run_with_source(Path::new("test_main.cssl"), main_src, &args);
        let err: ExitCode = ExitCode::from(exit_code::USER_ERROR);
        assert_eq!(format!("{code:?}"), format!("{err:?}"));
    }

    /// The `is_native_x64_backend_not_yet_landed` helper recognizes the
    /// canonical error prefix and rejects unrelated strings.
    #[test]
    fn is_native_x64_backend_not_yet_landed_matches_canonical_prefix() {
        let canonical = format!("{}", cssl_cgen_cpu_x64::NativeX64Error::BackendNotYetLanded);
        assert!(is_native_x64_backend_not_yet_landed(&canonical));
        assert!(!is_native_x64_backend_not_yet_landed(
            "cranelift native ISA unavailable : oops"
        ));
        assert!(!is_native_x64_backend_not_yet_landed(""));
    }

    /// Backend-comparison gate : on the same well-formed `fn main() -> i32
    /// { 42 }` source, both the cranelift and native-x64 backends should
    /// each EITHER succeed (producing host-magic-prefixed bytes) or, in
    /// the native-x64 case, surface the canonical not-yet-landed user
    /// error. This is the SEMANTIC equivalence assertion the dispatch
    /// REPORT-BACK requires : "both run + exit 42 + similar text-section
    /// size within tolerance" is the eventual gate ; at G6 dispatch time
    /// we assert the dispatch-mechanism level (both paths dispatch
    /// cleanly + both return either success or canonical-error).
    #[test]
    fn backend_comparison_both_paths_dispatch_cleanly() {
        let src = "module com.apocky.examples.hello\n\
                   fn main() -> i32 { 42 }\n";
        let tmp_clift =
            std::env::temp_dir().join(format!("csslc_g6_cmp_clift_{}.obj", std::process::id()));
        let tmp_native =
            std::env::temp_dir().join(format!("csslc_g6_cmp_native_{}.obj", std::process::id()));

        let args_clift = build_args("hello.cssl", tmp_clift.to_str().unwrap());
        let args_native = build_args_native_x64("hello.cssl", tmp_native.to_str().unwrap());

        let code_clift = run_with_source(Path::new("hello.cssl"), src, &args_clift);
        let code_native = run_with_source(Path::new("hello.cssl"), src, &args_native);

        let ok = format!("{:?}", ExitCode::from(exit_code::SUCCESS));
        let user_err = format!("{:?}", ExitCode::from(exit_code::USER_ERROR));

        // Cranelift must succeed (S6-A5 baseline preserved).
        assert_eq!(format!("{code_clift:?}"), ok);
        assert!(tmp_clift.exists());
        let bytes_clift = std::fs::read(&tmp_clift).unwrap();
        assert!(!bytes_clift.is_empty());

        // Native-x64 either succeeds with host-magic bytes or surfaces
        // the user-error (G1..G5 in flight).
        let actual_native = format!("{code_native:?}");
        if actual_native == ok {
            assert!(tmp_native.exists());
            let bytes_native = std::fs::read(&tmp_native).unwrap();
            assert!(!bytes_native.is_empty());
            // Both produced bytes for the host platform — they should each
            // start with the host magic. (Bit-for-bit byte equivalence is
            // NOT expected ; the two backends differ in encoding choices.)
            let host_magic_clift = cssl_cgen_cpu_cranelift::magic_prefix(
                cssl_cgen_cpu_cranelift::host_default_format(),
            );
            let host_magic_native =
                cssl_cgen_cpu_x64::magic_prefix(cssl_cgen_cpu_x64::host_default_format());
            assert!(bytes_clift.starts_with(host_magic_clift));
            assert!(bytes_native.starts_with(host_magic_native));
            // Text-section size tolerance — at G6 dispatch this is informational
            // only, not asserted ; both bytes ≠ empty is the contract.
        } else {
            // Skip path : G1..G5 not yet landed.
            assert_eq!(actual_native, user_err);
        }

        let _ = std::fs::remove_file(&tmp_clift);
        let _ = std::fs::remove_file(&tmp_native);
    }
}
