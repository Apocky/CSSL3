//! CSSLv3 stage0 — Jet<T,N> higher-order automatic differentiation.
//!
//! Authoritative design : `specs/17_JETS.csl` + `specs/05_AUTODIFF.csl`.
//!
//! § STATUS : T7 scaffold — `Jet<T, N>` type + `cssl.jet.*` op lowering pending.
//! § ORDER-INF : lazy `Jet<T, ∞>` via co-inductive stream (future).
//! § STAGING   : `@staged`-per-N specialization — see `cssl-staging`.

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
