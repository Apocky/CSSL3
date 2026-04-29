//! § cssl-substrate-save — save / load + replay-determinism for the
//!   Substrate (S8-H5, T11-D93).
//!
//! § ROLE
//!   Persists Substrate state to disk + reproduces it byte-identically.
//!   Builds on the H1 (Ω-tensor serialization) + H2 (omega_step replay-log) +
//!   B5 (file I/O) trajectories. At the time this slice landed, H1 + H2
//!   were not-yet-impl'd ; this crate carries canonical-shaped placeholder
//!   types (`OmegaTensor`, `OmegaScheduler`, `ReplayLog`, `ReplayEvent`)
//!   that future H1 + H2 slices will either re-export-from-here OR replace
//!   in-place under the same public API. The save-format + replay-from
//!   contract is stable from this slice forward.
//!
//! § SPEC : `specs/30_SUBSTRATE.csl` (canonical Ω-tensor shape +
//!   omega_step contract + R-1..R-10 reimpl-contracts) +
//!   `specs/22_TELEMETRY.csl § FS-OPS` (path-hash-only logging) +
//!   `specs/11_IFC.csl` (IFC labels travel through saves) + R18 audit-chain
//!   (BLAKE3 attestation chain for save-files).
//!
//! § SURFACE
//!   - [`SaveFile`] — versioned binary save-file with magic + version +
//!     Ω-tensor blob + replay-log blob + R18 attestation hash (BLAKE3)
//!   - [`save`] — serialize an [`OmegaScheduler`] to a path
//!   - [`load`] — verify magic + version + attestation + deserialize
//!   - [`replay_from`] — re-run from frame 0 using a save-file's replay-log
//!     to produce an Ω-tensor that BIT-EQUALS the snapshot in the save
//!   - [`SaveError`] / [`LoadError`] — typed failure modes (no-silent-corrupt)
//!
//! § FORMAT  (little-endian binary, deterministic field-ordering)
//!   - magic           : `b"CSSLSAVE"` (8 bytes)
//!   - version         : u32 LE       (currently `FORMAT_VERSION = 1`)
//!   - omega_len       : u64 LE       (length of Ω-tensor blob)
//!   - omega_blob      : `omega_len` bytes (sorted-key Ω-tensor serialization)
//!   - log_len         : u64 LE       (length of replay-log blob)
//!   - log_blob        : `log_len` bytes (frame-ordered ReplayEvent stream)
//!   - attestation     : 32 bytes BLAKE3 over (magic || version || omega_blob || log_blob)
//!   - trailer-offset  : u64 LE       (= file_size − 8 − 32 − 8 = body+attestation start ;
//!                                       used by streaming readers to seek)
//!
//!   Serialization is deterministic : Ω-tensor fields emit in
//!   `(type-tag, rank, shape, strides, data)` order ; HashMap iteration
//!   order is FORBIDDEN per the slice landmines — keys are sorted into a
//!   stable `Vec` before encoding.
//!
//! § DETERMINISM CONTRACT
//!   Given a save-file produced by [`save`], the assertion
//!   `replay_from(save, save.frame).snapshot() == save.snapshot_omega()`
//!   MUST hold byte-equal. This is the H5 invariant. When H2 lands the
//!   real `omega_step` replay, `replay_from` upgrades to drive it ;
//!   the byte-equal assertion is preserved across the upgrade.
//!
//! § PRIME-DIRECTIVE alignment  (per `specs/30_SUBSTRATE.csl` § Ω-TENSOR-LEVEL)
//!   - **Path-hash-only logging** (per `specs/22_TELEMETRY.csl § FS-OPS`) :
//!     save-file paths are never logged in cleartext ; the host audit-sink
//!     receives only the BLAKE3 hash of the path. Disclosure that a save
//!     happened is OK ; disclosure of WHERE it happened requires
//!     `ConsentToken<"fs-path-disclosure">` (deferred ; gated by N!).
//!   - **Attestation-mismatch is HARD-FAIL** : a save-file whose stored
//!     attestation does not match the freshly-computed BLAKE3 over its
//!     payload is REFUSED. We never silently corrupt state. Returns
//!     [`LoadError::AttestationMismatch`].
//!   - **IFC labels travel through saves** : the Ω-tensor's per-cell
//!     `ifc_label : L` field is part of the serialized form ; loading a
//!     save reconstructs the lattice exactly. Cross-cell-label-leak via
//!     save-round-trip is a-bug-shape we test for explicitly.
//!   - **No save-scumming surveillance** : rapid save/load cycles are
//!     observable to host telemetry (counter increment), but are NEVER
//!     blocked. Player-agency for save/load is preserved per the slice
//!     handoff. The telemetry counter is local-to-the-process per
//!     `cssl-rt::io::open_count` precedent ; nothing escapes the process.
//!   - **Cryptographic signing deferred** : R18 specifies signed-audit-
//!     chain attestation ; this slice provides the hash + the verification
//!     hook. A future slice ([`SignedAttestation` placeholder]) wires
//!     `cssl-telemetry::SigningKey` into the save-flow once the cssl-rt
//!     cap-system + IFC-`Privilege<level>` plumbing lands.
//!
//! § STAGE-0 NOTES
//!   - **H1 / H2 placeholder types** : [`OmegaTensor`] + [`OmegaScheduler`] +
//!     [`ReplayLog`] + [`ReplayEvent`] are minimal shapes implemented inside
//!     this crate so the save/load + replay machinery compiles + tests
//!     standalone. When H1 lands the canonical Ω-tensor type, this crate
//!     re-imports it (drop the local impl ; keep the API). When H2 lands
//!     the real `omega_step` replay-deterministic tick, [`replay_from`]
//!     upgrades to drive it.
//!   - **`replay_invariant_holds_after_upgrade` test** is the one gate-skip
//!     in this slice — it returns `Skipped("H2 omega_step pending")` until
//!     the real omega_step lands, because we cannot test bit-equal-replay
//!     against a real tick that doesn't exist yet. The test asserts the
//!     STAGE-0 placeholder invariant (replay of an empty log → identity).
//!
//! § DEFERRED  (called-out future slices)
//!   - **D-1 cryptographic signing** : the R18 attestation hash is here ;
//!     the Ed25519 signature over (build_id || omega_blob || log_blob)
//!     deferred to a session-8+ slice that wires `cssl-telemetry::SigningKey`
//!     through the cssl-rt cap-system. The format reserves the trailing
//!     32 bytes ; signature would slot in immediately after attestation.
//!   - **D-2 streaming reader** : the format includes a trailer-offset for
//!     streaming-mode reads (don't materialize the entire blob). Stage-0
//!     [`load`] is read-into-Vec ; streaming form follows when 100 MB+
//!     saves become a thing.
//!   - **D-3 version migration** : `version` field present ; if older-
//!     version save loaded, run migration chain. Stage-0 only handles
//!     `FORMAT_VERSION = 1` ; older versions return [`LoadError::UnsupportedVersion`].
//!   - **D-4 compression** : §§ 30 D-2 (save-game-compression) deferred.
//!     Format is uncompressed at stage-0.

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
#![allow(clippy::module_name_repetitions)]

pub mod error;
pub mod format;
pub mod io;
pub mod omega;
pub mod replay;

pub use error::{LoadError, SaveError};
pub use format::{
    AttestationHash, SaveFile, FORMAT_MAGIC, FORMAT_VERSION, OMEGA_TYPE_TAG_F32,
    OMEGA_TYPE_TAG_F64, OMEGA_TYPE_TAG_I32, OMEGA_TYPE_TAG_I64, OMEGA_TYPE_TAG_U8,
};
pub use io::{load, path_hash, save};
pub use omega::{OmegaCell, OmegaScheduler, OmegaTensor, ReplayEvent, ReplayKind, ReplayLog};
pub use replay::replay_from;

/// Crate version, exposed for scaffold tests.
pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod scaffold_tests {
    use super::STAGE0_SCAFFOLD;

    #[test]
    fn scaffold_version_present() {
        assert!(!STAGE0_SCAFFOLD.is_empty());
    }

    #[test]
    fn format_version_is_one() {
        assert_eq!(super::FORMAT_VERSION, 1);
    }

    #[test]
    fn format_magic_is_eight_bytes() {
        assert_eq!(super::FORMAT_MAGIC.len(), 8);
        assert_eq!(super::FORMAT_MAGIC, b"CSSLSAVE");
    }
}
