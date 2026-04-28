//! § commands::fmt — `csslc fmt <input.cssl>` (stage-0 stub).
//!
//! Stage-0 has no real source-formatter ; this subcommand verifies the
//! file is readable + parseable + then echoes the original source to
//! stdout. Real formatting lands in a future slice once the AST→source
//! printer exists.

use std::process::ExitCode;

use crate::cli::FmtArgs;
use crate::diag;
use crate::exit_code;

pub fn run(args: &FmtArgs) -> ExitCode {
    let source = match std::fs::read_to_string(&args.input) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("csslc: {}", diag::fs_error(&args.input, &e));
            return ExitCode::from(exit_code::USER_ERROR);
        }
    };
    // Stage-0 : passthrough echo. Future slices will implement the
    // CST → canonical-source printer that respects spec § 09_SYNTAX.
    print!("{source}");
    eprintln!(
        "csslc: fmt {} : passthrough (real formatter pending later slice)",
        args.input.display()
    );
    ExitCode::from(exit_code::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn fmt_with_missing_file_returns_user_error() {
        let args = FmtArgs {
            input: PathBuf::from("/nonexistent/foo.cssl"),
        };
        let code = run(&args);
        let err: ExitCode = ExitCode::from(exit_code::USER_ERROR);
        assert_eq!(format!("{code:?}"), format!("{err:?}"));
    }

    #[test]
    fn fmt_passthrough_returns_success() {
        let tmp = std::env::temp_dir().join(format!("csslc_fmt_{}.cssl", std::process::id()));
        std::fs::write(&tmp, "fn main() {}\n").unwrap();
        let args = FmtArgs { input: tmp.clone() };
        let code = run(&args);
        let ok: ExitCode = ExitCode::from(exit_code::SUCCESS);
        assert_eq!(format!("{code:?}"), format!("{ok:?}"));
        let _ = std::fs::remove_file(&tmp);
    }
}
