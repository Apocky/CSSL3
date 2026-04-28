//! § csslc — CSSLv3 stage-0 compiler CLI library
//! ════════════════════════════════════════════════
//!
//! Authoritative spec : `specs/01_BOOTSTRAP.csl § CLI-SUBCOMMANDS`
//!                    + `specs/14_BACKEND.csl § CLI-ENTRY`.
//!
//! § ROLE
//!   Entry-point library powering the `csslc` binary. Parses argv, routes to
//!   subcommand handlers, orchestrates the full stage-0 compiler pipeline.
//!
//! § DESIGN
//!   - `pub fn run(args: Vec<String>) -> ExitCode` is the testable entry.
//!     Tests pass synthesized argv vectors instead of spawning subprocesses.
//!   - Manual argv parsing (no `clap` workspace dep added at stage-0).
//!     Each subcommand has its own arg shape ; the parser is a small
//!     hand-rolled state machine in [`cli`].
//!   - Subcommand handlers live in [`commands`]. Each receives the parsed
//!     `BuildArgs` / `CheckArgs` / etc. and returns an [`ExitCode`].
//!   - Diagnostic rendering uses simple stderr formatting at stage-0 ;
//!     miette-style pretty rendering is deferred (still stable codes).
//!
//! § SUBCOMMANDS  (per slice spec S6-A2)
//!   - `build  <input>.cssl [-o <out>] [--target=<triple>] [--emit=<mode>]`
//!   - `check  <input>.cssl`
//!   - `fmt    <input>.cssl`
//!   - `test   [--update-golden]`
//!   - `emit-mlir <input>.cssl`
//!   - `verify <input>.cssl`
//!   - `version`
//!   - `help` (or `-h` / `--help`)
//!
//! § EXIT CODES
//!   - `0` : success
//!   - `1` : user-error (bad args, source compilation failure, file not found)
//!   - `2` : internal-error (panic in compiler, unexpected state)
//!
//! § STATUS  (S6-A2)
//!   `build` and `check` are fully orchestrated through the existing pipeline
//!   (lex / parse / HIR-lower / AD-legality / refinement / MIR-lower /
//!   monomorphize / drop-unspecialized / call-rewrite). Object-file emission
//!   (`--emit=object` / `--emit=exe`) writes a placeholder file at S6-A2 ;
//!   real `.o` emission lands in S6-A3 and linker invocation in S6-A4.
//!   `fmt` / `test` / `emit-mlir` / `verify` are skeletal — they return exit-0
//!   with explanatory messages so downstream tooling doesn't error on them.

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
#![allow(clippy::module_name_repetitions)]

use std::process::ExitCode;

pub mod cli;
pub mod commands;
pub mod diag;

/// Exit-code constants. Keep aligned with the documented user-error /
/// internal-error split in the module-level docs.
pub mod exit_code {
    /// Success. The command did exactly what it was asked.
    pub const SUCCESS: u8 = 0;
    /// User-error : bad args, source compilation failure, file not found.
    pub const USER_ERROR: u8 = 1;
    /// Internal-error : compiler panic or unexpected state.
    pub const INTERNAL_ERROR: u8 = 2;
}

/// Library entry-point. Pass argv (full vec including the program name as
/// `args[0]`). Returns an [`ExitCode`] that the binary forwards to the OS.
#[must_use]
pub fn run(args: Vec<String>) -> ExitCode {
    let parsed = match cli::parse(&args) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("csslc: {e}");
            eprintln!();
            eprintln!("{}", cli::usage());
            return ExitCode::from(exit_code::USER_ERROR);
        }
    };

    match parsed {
        cli::Command::Build(args) => commands::build::run(&args),
        cli::Command::Check(args) => commands::check::run(&args),
        cli::Command::Fmt(args) => commands::fmt::run(&args),
        cli::Command::Test(args) => commands::test_cmd::run(&args),
        cli::Command::EmitMlir(args) => commands::emit_mlir::run(&args),
        cli::Command::Verify(args) => commands::verify::run(&args),
        cli::Command::Version => commands::version::run(),
        cli::Command::Help => commands::help::run(),
    }
}

/// Crate version constant, exposed from `Cargo.toml` (`CARGO_PKG_VERSION`).
pub const STAGE0_VERSION: &str = env!("CARGO_PKG_VERSION");

/// PRIME-DIRECTIVE attestation, per `PRIME_DIRECTIVE.md § 11`.
pub const ATTESTATION: &str =
    "There was no hurt nor harm in the making of this, to anyone/anything/anybody.";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_constant_present() {
        assert!(!STAGE0_VERSION.is_empty());
    }

    #[test]
    fn attestation_constant_canonical() {
        assert_eq!(
            ATTESTATION,
            "There was no hurt nor harm in the making of this, to anyone/anything/anybody."
        );
    }

    #[test]
    fn run_with_no_args_returns_user_error() {
        let code = run(vec!["csslc".to_string()]);
        // ExitCode is opaque ; exercise via downcast-equivalent comparison
        let expected: ExitCode = ExitCode::from(exit_code::USER_ERROR);
        assert_eq!(format!("{code:?}"), format!("{expected:?}"));
    }

    #[test]
    fn run_with_version_subcommand_succeeds() {
        let code = run(vec!["csslc".to_string(), "version".to_string()]);
        let ok: ExitCode = ExitCode::from(exit_code::SUCCESS);
        assert_eq!(format!("{code:?}"), format!("{ok:?}"));
    }

    #[test]
    fn run_with_help_subcommand_succeeds() {
        let code = run(vec!["csslc".to_string(), "help".to_string()]);
        let ok: ExitCode = ExitCode::from(exit_code::SUCCESS);
        assert_eq!(format!("{code:?}"), format!("{ok:?}"));
    }

    #[test]
    fn run_with_dash_h_succeeds() {
        let code = run(vec!["csslc".to_string(), "-h".to_string()]);
        let ok: ExitCode = ExitCode::from(exit_code::SUCCESS);
        assert_eq!(format!("{code:?}"), format!("{ok:?}"));
    }

    #[test]
    fn run_with_unknown_subcommand_returns_user_error() {
        let code = run(vec!["csslc".to_string(), "frobnicate".to_string()]);
        let err: ExitCode = ExitCode::from(exit_code::USER_ERROR);
        assert_eq!(format!("{code:?}"), format!("{err:?}"));
    }
}
