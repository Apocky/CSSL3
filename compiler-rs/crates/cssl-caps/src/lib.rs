//! CSSLv3 stage0 — Pony-6 capability checker + Vale gen-refs.
//!
//! Authoritative design : `specs/12_CAPABILITIES.csl`.
//!
//! § STATUS : T5 scaffold — alias+deny matrix + gen-ref synthesis pending.
//! § CAPABILITY-SET : `iso | trn | ref | val | box | tag` per Pony-lineage.
//! § INTEROP : `Handle<T>` lowering `tag<T>` ≡ u64 24+40 packed (Vale gen-ref).

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
