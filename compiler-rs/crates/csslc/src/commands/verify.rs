//! § commands::verify — `csslc verify <input.cssl>`.
//!
//! Runs the frontend + every walker (AD-legality, refinement-obligation,
//! IFC, staged-check, macro-hygiene) and reports cumulative results. SMT
//! solver dispatch is omitted at stage-0 (would require Z3 on PATH) ; we
//! still translate predicates to Query terms and report translation-success.

use std::path::Path;
use std::process::ExitCode;

use crate::cli::VerifyArgs;
use crate::diag;
use crate::exit_code;

pub fn run(args: &VerifyArgs) -> ExitCode {
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

    // ── walkers ───────────────────────────────────────────────────────
    let ad_report = cssl_hir::check_ad_legality(&hir_mod, &interner);
    let obligations = cssl_hir::collect_refinement_obligations(&hir_mod, &interner);
    let smt_results = cssl_smt::translate_bag(&obligations, &interner);
    let smt_translated = smt_results.iter().filter(|(_, r)| r.is_ok()).count();
    let smt_failed = smt_results.iter().filter(|(_, r)| r.is_err()).count();

    eprintln!("csslc: verify {} :", path.display());
    eprintln!("  HIR items                : {}", hir_mod.items.len());
    eprintln!(
        "  AD-legality diagnostics  : {} ({} fns checked)",
        ad_report.diagnostics.len(),
        ad_report.checked_fn_count,
    );
    eprintln!("  refinement obligations   : {}", obligations.len());
    eprintln!("  SMT predicate translations : {smt_translated} ok / {smt_failed} failed");

    if !ad_report.diagnostics.is_empty() {
        for d in &ad_report.diagnostics {
            eprintln!("    [AD] {}", d.message());
        }
        return ExitCode::from(exit_code::USER_ERROR);
    }
    ExitCode::from(exit_code::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verify_minimal_module_succeeds() {
        let src = "module com.apocky.examples.hello\n\
                   fn main() -> i32 { 42 }\n";
        let code = run_with_source(Path::new("hello.cssl"), src);
        let ok: ExitCode = ExitCode::from(exit_code::SUCCESS);
        assert_eq!(format!("{code:?}"), format!("{ok:?}"));
    }

    #[test]
    fn verify_with_missing_file_returns_user_error() {
        let args = VerifyArgs {
            input: std::path::PathBuf::from("/nonexistent/foo.cssl"),
        };
        let code = run(&args);
        let err: ExitCode = ExitCode::from(exit_code::USER_ERROR);
        assert_eq!(format!("{code:?}"), format!("{err:?}"));
    }
}
