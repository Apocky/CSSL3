//! CSSLv3 compiler CLI (stage0) — entry-point.
//!
//! Authoritative design : `specs/01_BOOTSTRAP.csl` + `specs/14_BACKEND.csl`.
//! Task roadmap        : `HANDOFF_SESSION_1.csl`.
//! Progress log        : `SESSION_1_HANDOFF.md` + `DECISIONS.md`.
//!
//! § STATUS : T1 workspace scaffold — subcommands pending (T2+).
//!   Future subcommands per `specs/01_BOOTSTRAP.csl` :
//!     build, check, fmt, test, bench, lint, doc,
//!     emit-mlir, emit-spirv, emit-c99, replay, bench --update-baseline,
//!     test --update-golden, verify (§§ 20), attest (R16 + §§ 22).

#![forbid(unsafe_code)]

fn main() {
    eprintln!(
        "csslc {} — CSSLv3 stage0 compiler (scaffold)",
        env!("CARGO_PKG_VERSION")
    );
    eprintln!("Status : T1 workspace scaffold — subcommands pending (T2+).");
    eprintln!("See DECISIONS.md and SESSION_1_HANDOFF.md at the repository root.");
    std::process::exit(0);
}
