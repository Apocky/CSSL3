//! § commands::emit_mlir — `csslc emit-mlir <input.cssl>`.
//!
//! Runs the frontend + MIR-lowering and dumps a textual MIR-module summary
//! to stdout. Stage-0 emits a structured-but-coarse summary (one line per
//! `MirFunc`) ; real MLIR dialect emission lands later when the
//! `cssl-mlir-bridge` FFI is wired.

use std::path::Path;
use std::process::ExitCode;

use crate::cli::EmitMlirArgs;
use crate::diag;
use crate::exit_code;

pub fn run(args: &EmitMlirArgs) -> ExitCode {
    let source = match std::fs::read_to_string(&args.input) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("csslc: {}", diag::fs_error(&args.input, &e));
            return ExitCode::from(exit_code::USER_ERROR);
        }
    };
    run_with_source(&args.input, &source)
}

pub fn run_with_source(path: &Path, source: &str) -> ExitCode {
    use cssl_ast::{SourceFile, SourceId, Surface};

    let file = SourceFile::new(
        SourceId::first(),
        path.display().to_string(),
        source,
        Surface::RustHybrid,
    );
    let tokens = cssl_lex::lex(&file);
    let (cst, parse_bag) = cssl_parse::parse(&file, &tokens);
    let parse_errors = diag::emit_diagnostics(path, &parse_bag);
    if parse_errors > 0 {
        return ExitCode::from(exit_code::USER_ERROR);
    }

    let (hir_mod, interner, lower_bag) = cssl_hir::lower_module(&file, &cst);
    let lower_errors = diag::emit_diagnostics(path, &lower_bag);
    if lower_errors > 0 {
        return ExitCode::from(exit_code::USER_ERROR);
    }

    let lower_ctx = cssl_mir::LowerCtx::new(&interner);
    let mut mir_mod = cssl_mir::MirModule::new();
    for item in &hir_mod.items {
        if let cssl_hir::HirItem::Fn(f) = item {
            let mut mf = cssl_mir::lower_function_signature(&lower_ctx, f);
            cssl_mir::lower_fn_body(&interner, Some(&file), f, &mut mf);
            mir_mod.push_func(mf);
        }
    }

    println!("// CSSLv3 stage-0 MIR dump for {}", path.display());
    println!("// (real MLIR-dialect emission deferred to the cssl-mlir-bridge slice)");
    for f in &mir_mod.funcs {
        println!(
            "// fn {} : {} block(s) — generic={}",
            f.name,
            f.body.blocks.len(),
            f.is_generic,
        );
    }
    ExitCode::from(exit_code::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emit_mlir_with_minimal_module_succeeds() {
        let src = "module com.apocky.examples.hello\n\
                   fn main() -> i32 { 42 }\n";
        let code = run_with_source(Path::new("hello.cssl"), src);
        let ok: ExitCode = ExitCode::from(exit_code::SUCCESS);
        assert_eq!(format!("{code:?}"), format!("{ok:?}"));
    }

    #[test]
    fn emit_mlir_with_missing_file_returns_user_error() {
        let args = EmitMlirArgs {
            input: std::path::PathBuf::from("/nonexistent/foo.cssl"),
        };
        let code = run(&args);
        let err: ExitCode = ExitCode::from(exit_code::USER_ERROR);
        assert_eq!(format!("{code:?}"), format!("{err:?}"));
    }
}
