//! CSSLv3 stage0 — orthogonal-persistence image + schema-migration + hot-reload.
//!
//! § SPEC : `specs/18_ORTHOPERSIST.csl` (R13 Pharo-lineage).
//!
//! § SCOPE (T11-phase-1 / this commit)
//!   - [`SchemaVersion`]     — monotonic u32 + BLAKE3 digest identifier.
//!   - [`SchemaMigration`]   — (from, to) version pair + migration-id label.
//!   - [`MigrationChain`]    — ordered list of migrations between any two versions.
//!   - [`ImageHeader`]       — persistence-image metadata block.
//!   - [`ImageRecord`]       — typed record stored in the image (key + schema-ver + bytes).
//!   - [`PersistenceImage`]  — in-memory representation of a persistence image.
//!   - [`PersistenceBackend`] trait — WAL / LMDB / in-memory dispatch.
//!   - [`InMemoryBackend`]   — stage-0 reference impl.
//!   - [`PersistError`]      — error enum.
//!
//! § T11-phase-2 DEFERRED
//!   - WAL-file backend (append-only log + snapshot checkpoints).
//!   - LMDB backend (alternative for large working-sets).
//!   - `@hot_reload_preserve` HIR attribute extraction + root-set discovery.
//!   - Schema-derivation from HIR-types (T11-phase-2 hooks into `cssl-hir`).
//!   - Live-object migration (apply migration-chain to in-flight image).
//!   - R16 attestation of image-provenance (BLAKE3 chain + Ed25519 signature).

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::module_name_repetitions)]

pub mod backend;
pub mod image;
pub mod migration;
pub mod schema;

pub use backend::{InMemoryBackend, PersistError, PersistenceBackend};
pub use image::{ImageHeader, ImageRecord, PersistenceImage};
pub use migration::{MigrationChain, SchemaMigration};
pub use schema::SchemaVersion;

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
