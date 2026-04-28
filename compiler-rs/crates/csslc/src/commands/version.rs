//! § commands::version — `csslc version`.

use std::process::ExitCode;

use crate::exit_code;

/// Print the canonical version line and the toolchain anchor.
pub fn run() -> ExitCode {
    println!(
        "csslc {} — CSSLv3 stage-0 compiler\n\
         toolchain : rustc 1.85.0 (R16 anchor, T11-D20)\n\
         attestation : {}",
        crate::STAGE0_VERSION,
        crate::ATTESTATION,
    );
    ExitCode::from(exit_code::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_run_returns_success() {
        let code = run();
        let ok: ExitCode = ExitCode::from(exit_code::SUCCESS);
        assert_eq!(format!("{code:?}"), format!("{ok:?}"));
    }
}
