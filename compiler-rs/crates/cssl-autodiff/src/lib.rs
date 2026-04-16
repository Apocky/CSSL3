//! CSSLv3 stage0 — source-to-source automatic differentiation on MIR.
//!
//! Authoritative design : `specs/05_AUTODIFF.csl` + `specs/17_JETS.csl`.
//!
//! § STATUS : T7 scaffold — post-monomorphization AD transform pending.
//! § OPS-COVERED : per `specs/05_AUTODIFF.csl` rules-table — `FAdd`, `FMul`, `Sqrt`,
//!                `Call`, `Load`/`Store`, `If`/`Loop`, with `@checkpoint` handling.
//! § KILLER-APP : `bwd_diff(sphere_sdf)(p).d_p` bit-exact vs analytic (T1-gate).

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]

/// Crate version, exposes `CARGO_PKG_VERSION`.
pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod scaffold_tests {
    #[test]
    fn scaffold_version_present() {
        assert!(!super::STAGE0_SCAFFOLD.is_empty());
    }
}
