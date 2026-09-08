//! § commands::check — `csslc check <input.cssl>`.
//!
//! Frontend-only orchestration : load source → lex → parse → HIR-lower → type-check.
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

    let (hir_mod, interner, lower_bag) = cssl_hir::lower_module(&file, &cst);
    let lower_errors = diag::emit_diagnostics(path, &lower_bag);
    if lower_errors > 0 {
        eprintln!("csslc: check failed — {lower_errors} HIR-lower error(s)");
        return ExitCode::from(exit_code::USER_ERROR);
    }

    let (_type_map, type_diagnostics) = cssl_hir::check_module(&hir_mod, &interner);
    let type_errors = diag::emit_type_diagnostics(path, &file, &type_diagnostics);
    if type_errors > 0 {
        eprintln!("csslc: check failed — {type_errors} type-check error(s)");
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

    #[test]
    fn check_preserves_bool_and_integer_not_contracts() {
        let src = "fn bool_not(value: bool) -> bool { !value }\n\
                   fn int_bang(value: u32) -> u32 { !value }\n\
                   fn int_tilde(value: u32) -> u32 { ~value }\n";
        let code = run_with_source(Path::new("valid_not.cssl"), src);
        assert_eq!(
            format!("{code:?}"),
            format!("{:?}", ExitCode::from(exit_code::SUCCESS))
        );
    }

    #[test]
    fn check_rejects_noncanonical_bool_ingress_and_invalid_unary_domains() {
        let invalid = [
            "fn invalid(value: bool) -> bool { ~value }",
            "fn invalid() -> bool { 2 }",
            "fn invalid(value: u8) -> bool { value }",
            "fn invalid(value: u8) -> bool { value as bool }",
            "fn invalid(value: f32) -> bool { !value }",
            "fn invalid(value: String) -> bool { !value }",
        ];
        let expected = format!("{:?}", ExitCode::from(exit_code::USER_ERROR));
        for (index, src) in invalid.iter().enumerate() {
            let code = run_with_source(Path::new("invalid_type.cssl"), src);
            assert_eq!(
                format!("{code:?}"),
                expected,
                "case {index} accepted: {src}"
            );
        }
    }

    #[test]
    fn check_keeps_unknown_unqualified_local_names_fatal() {
        let src = "fn invalid() -> i32 { definitely_missing_local }";
        let code = run_with_source(Path::new("unknown_local.cssl"), src);
        assert_eq!(
            format!("{code:?}"),
            format!("{:?}", ExitCode::from(exit_code::USER_ERROR))
        );
    }

    #[test]
    fn opaque_external_surfaces_do_not_mask_local_bool_violations() {
        let invalid = [
            "use std::result::Result\n\
             use external::opaque\n\
             fn invalid(value: u8) -> bool {\n\
                 let a: Result<u8, u8> = external::qualified(value);\n\
                 let b = opaque(value);\n\
                 value\n\
             }",
            "use std::option::Option\n\
             fn invalid(value: bool) -> bool {\n\
                 let a: Option<u8> = external::qualified();\n\
                 ~value\n\
             }",
        ];
        let expected = format!("{:?}", ExitCode::from(exit_code::USER_ERROR));
        for src in invalid {
            let code = run_with_source(Path::new("opaque_with_local_error.cssl"), src);
            assert_eq!(
                format!("{code:?}"),
                expected,
                "local violation masked: {src}"
            );
        }
    }
}
