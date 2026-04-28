//! § commands::test_cmd — `csslc test [--update-golden]` (stage-0 stub).
//!
//! Real implementation will discover `*.cssl` test files in `tests/`,
//! invoke them with the JIT, and compare against golden outputs. Stage-0
//! returns an explanatory message and exit-0 so downstream `make test`
//! style scripts don't fail.
//!
//! `--update-golden` is parsed (so workflows that pass it survive) but
//! has no effect at stage-0.

use std::process::ExitCode;

use crate::cli::TestArgs;
use crate::exit_code;

pub fn run(args: &TestArgs) -> ExitCode {
    if args.update_golden {
        eprintln!(
            "csslc: test : (stage-0 stub — `--update-golden` accepted but no goldens to update yet)"
        );
    } else {
        eprintln!(
            "csslc: test : (stage-0 stub — discovers no project tests yet ; \
             see `cargo test --workspace` for compiler-rs tests)"
        );
    }
    ExitCode::from(exit_code::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_with_update_golden_returns_success() {
        let args = TestArgs {
            update_golden: true,
        };
        let code = run(&args);
        let ok: ExitCode = ExitCode::from(exit_code::SUCCESS);
        assert_eq!(format!("{code:?}"), format!("{ok:?}"));
    }

    #[test]
    fn test_run_without_update_golden_returns_success() {
        let args = TestArgs {
            update_golden: false,
        };
        let code = run(&args);
        let ok: ExitCode = ExitCode::from(exit_code::SUCCESS);
        assert_eq!(format!("{code:?}"), format!("{ok:?}"));
    }
}
