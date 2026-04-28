//! § commands::check — `csslc check <input.cssl>`.
//!
//! Frontend-only orchestration : load source → lex → parse → HIR-lower.
//! Reports any errors via `diag` and returns a non-zero exit code if the
//! source has errors. No emission.

use std::path::Path;
use std::process::ExitCode;

use crate::cli::CheckArgs;
use crate::diag;
use crate::exit_code;

pub fn run(args: &CheckArgs) -> ExitCode {
    let source = match std::fs::read_to_string(&args.input) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("csslc: {}", diag::fs_error(&args.input, &e));
            return ExitCode::from(exit_code::USER_ERROR);
        }
    };
    run_with_source(&args.input, &source)
}

/// Invoke the frontend pipeline on `(path, source)`. Splits out for
/// in-process tests that synthesize source without touching the file
/// system.
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
        eprintln!("csslc: check failed — {parse_errors} parse error(s)");
        return ExitCode::from(exit_code::USER_ERROR);
    }

    let (_hir_mod, _interner, lower_bag) = cssl_hir::lower_module(&file, &cst);
    let lower_errors = diag::emit_diagnostics(path, &lower_bag);
    if lower_errors > 0 {
        eprintln!("csslc: check failed — {lower_errors} HIR-lower error(s)");
        return ExitCode::from(exit_code::USER_ERROR);
    }

    eprintln!("csslc: check {} : OK", path.display());
    ExitCode::from(exit_code::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_with_empty_source_succeeds() {
        let code = run_with_source(Path::new("empty.cssl"), "");
        let ok: ExitCode = ExitCode::from(exit_code::SUCCESS);
        assert_eq!(format!("{code:?}"), format!("{ok:?}"));
    }

    #[test]
    fn check_with_minimal_module_succeeds() {
        let src = "module com.apocky.examples.hello\n\
                   fn main() -> i32 { 42 }\n";
        let code = run_with_source(Path::new("hello.cssl"), src);
        let ok: ExitCode = ExitCode::from(exit_code::SUCCESS);
        assert_eq!(format!("{code:?}"), format!("{ok:?}"));
    }

    #[test]
    fn check_with_missing_file_returns_user_error() {
        let args = CheckArgs {
            input: std::path::PathBuf::from("/nonexistent/foo.cssl"),
        };
        let code = run(&args);
        let err: ExitCode = ExitCode::from(exit_code::USER_ERROR);
        assert_eq!(format!("{code:?}"), format!("{err:?}"));
    }
}
