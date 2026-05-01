// § cssl-host-license-attribution
// I> license-aware metadata + attribution-HUD + filter helpers
// I> wave-4 — LoA free-3D-asset ingest support
// I> spec-ref : specs/grand-vision/06_FREE_3D_INGEST.csl
//
// purpose :
//   t∞: tag every fetched asset (GLB · PNG · HDR · WAV) ⊗ License + author + source-url
//   t∞: enforce LoA-policy (CC0 ✓ · CC-BY-4.0 ✓ · CC-NC ✗ · proprietary ✗)
//   t∞: emit attribution-HUD strings for on-screen credit
//   t∞: filter registry by license-permissions
//
// modules :
//   license      : License-enum + permission-predicates + canonical-URLs
//   asset        : AssetLicenseRecord — per-asset metadata + attribution-text
//   registry     : LicenseRegistry — keyed map + reports + filters
//   loa_policy   : LoaLicensePolicy — project-level acceptance rules

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![doc = "License-aware metadata + attribution-HUD + filter helpers for LoA free-asset ingest."]

/// `AssetLicenseRecord` — per-asset metadata, attribution-text + HTML helpers.
pub mod asset;
/// `License` enum — recognized license kinds + permission predicates + canonical URLs.
pub mod license;
/// `LoaLicensePolicy` — project-level acceptance rules + decision evaluation.
pub mod loa_policy;
/// `LicenseRegistry` — keyed map of records + filters + multi-line / JSONL reports.
pub mod registry;

pub use asset::AssetLicenseRecord;
pub use license::License;
pub use loa_policy::{LoaLicensePolicy, PolicyDecision};
pub use registry::{LicenseRegistry, RegErr};
