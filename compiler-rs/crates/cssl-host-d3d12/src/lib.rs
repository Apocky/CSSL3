//! CSSLv3 stage0 — D3D12 host submission scaffold.
//!
//! § SPEC : `specs/14_BACKEND.csl` § HOST-SUBMIT BACKENDS § D3D12.
//!
//! § STRATEGY
//!   Phase-1 catalogs the D3D12 device / adapter / feature-level surface. No
//!   `windows-rs` FFI yet (requires MSVC toolchain per T1-D7). Phase-2 wires real
//!   `ID3D12Device` / `ID3D12CommandQueue` / `IDXGIAdapter4` via the windows crate.
//!
//! § SCOPE (T10-phase-1-hosts / this commit)
//!   - [`FeatureLevel`]     — D3D_FEATURE_LEVEL_12_0..12_2.
//!   - [`DxgiAdapter`]      — adapter identification record.
//!   - [`D3d12FeatureOptions`] — subset of `D3D12_FEATURE_DATA_D3D12_OPTIONS*` fields.
//!   - [`CommandListType`]  — direct / compute / copy / bundle / videos.
//!   - [`DescriptorHeapType`] — cbv-srv-uav / sampler / rtv / dsv.
//!   - [`HeapType`]         — default / upload / readback / custom.

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::struct_excessive_bools)]

pub mod adapter;
pub mod features;
pub mod heap;

pub use adapter::{DxgiAdapter, FeatureLevel};
pub use features::{D3d12FeatureOptions, WaveMatrixTier};
pub use heap::{CommandListType, DescriptorHeapType, HeapType};

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
