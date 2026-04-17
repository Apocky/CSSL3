//! CSSLv3 stage0 — Metal host submission scaffold.
//!
//! § SPEC : `specs/14_BACKEND.csl` § HOST-SUBMIT BACKENDS § Metal.
//!
//! § STRATEGY
//!   Phase-1 catalogs the Metal feature-set / GPU-family / argument-buffer-tier
//!   surface without pulling in the `metal` crate. The `metal` crate is
//!   Apple-platforms-only (macOS / iOS / tvOS / visionOS) ; phase-2 will cfg-gate
//!   the FFI path behind `#[cfg(target_os = "macos")]` etc.
//!
//! § SCOPE (T10-phase-1-hosts / this commit)
//!   - [`MetalFeatureSet`]  — MTLFeatureSet enumeration (Apple-Silicon-first).
//!   - [`GpuFamily`]        — Apple1..Apple9 / Mac1..Mac2 + common2..common3.
//!   - [`MtlDevice`]        — Metal-device record.
//!   - [`MetalHeapType`]    — private / shared / managed.

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::struct_excessive_bools)]

pub mod device;
pub mod feature_set;
pub mod heap;

pub use device::{GpuFamily, MtlDevice};
pub use feature_set::MetalFeatureSet;
pub use heap::{MetalHeapType, MetalResourceOptions};

/// Crate version exposed for scaffold verification.
pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod scaffold_tests {
    use super::STAGE0_SCAFFOLD;

    #[test]
    fn scaffold_version_present() {
        assert!(!STAGE0_SCAFFOLD.is_empty());
    }
}
