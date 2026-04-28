//! § commands — subcommand handlers for `csslc`.
//!
//! Each submodule implements one subcommand. The top-level dispatcher in
//! `crate::run` matches a typed [`crate::cli::Command`] and forwards to the
//! appropriate `run` fn.

pub mod build;
pub mod check;
pub mod emit_mlir;
pub mod fmt;
pub mod help;
pub mod test_cmd;
pub mod verify;
pub mod version;
