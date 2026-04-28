//! § commands::help — `csslc help` / `-h` / `--help`.

use std::process::ExitCode;

use crate::cli;
use crate::exit_code;

/// Print the usage text and exit successfully.
pub fn run() -> ExitCode {
    println!("{}", cli::usage());
    ExitCode::from(exit_code::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_run_returns_success() {
        let code = run();
        let ok: ExitCode = ExitCode::from(exit_code::SUCCESS);
        assert_eq!(format!("{code:?}"), format!("{ok:?}"));
    }
}
