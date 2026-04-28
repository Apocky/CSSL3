//! csslc binary entry — thin wrapper over [`csslc::run`].
//!
//! The library's `run` fn takes a `Vec<String>` so unit tests can invoke
//! it without spawning a subprocess. This binary collects `std::env::args`
//! and forwards.

#![forbid(unsafe_code)]

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    csslc::run(args)
}
