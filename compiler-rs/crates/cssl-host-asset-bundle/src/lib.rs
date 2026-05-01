// § cssl-host-asset-bundle
// I> asset-bundle container : manifest + materials + file-blobs + FNV-128 fingerprint
// I> companion to cssl-asset-fetcher (wave-7 wire-up) ; wraps a fetched 3D-asset
// I> alongside its license-record so downstream loaders can validate provenance
// I> in one shot. JSON-only on-disk format ; no LAB binary container in this slice.
//
// modules :
//   manifest : BundleManifest + MaterialEntry + FileEntry + FileKind
//   bundle   : AssetBundle (manifest + file_blobs in-memory) + FNV-1a-128 fingerprint
//   storage  : LabStore — directory-backed save / load / list of bundles
//
// scope-note : the fingerprint is a stdlib-only FNV-1a-128 (matching
// cssl-host-golden) ; it is a *perceptual fingerprint hash, not crypto*.
// Sufficient for cache-keys + bit-exact regression detection ; insufficient
// against an adversary. Wave-7+ will swap in BLAKE3 keyed-mode when wired.

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![doc = "Asset-bundle container : manifest + material + file-blobs + FNV-1a-128 fingerprint + JSON disk store."]

/// `AssetBundle` + `BundleErr` — in-memory bundle with finalize/validate/inspect.
pub mod bundle;
/// `BundleManifest` + `MaterialEntry` + `FileEntry` + `FileKind` — serializable manifest schema.
pub mod manifest;
/// `LabStore` — directory-backed save / load / list of bundles (JSON manifest + raw blob files).
pub mod storage;

pub use bundle::{AssetBundle, BundleErr};
pub use manifest::{BundleManifest, FileEntry, FileKind, MaterialEntry};
pub use storage::LabStore;

// re-export for downstream convenience — bundles always carry a license-record
pub use cssl_host_license_attribution::{AssetLicenseRecord, License};

/// Bundle-manifest schema version currently emitted by [`AssetBundle::finalize`].
///
/// Bumped when the on-disk JSON layout changes in a non-additive way.
/// Loaders compare the loaded manifest's `schema_version` against this constant
/// and reject incompatible versions ; additive fields use `serde(default)`.
pub const MANIFEST_SCHEMA_VERSION: u32 = 1;
