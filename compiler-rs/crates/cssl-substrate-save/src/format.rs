//! § cssl-substrate-save — binary save-file format (S8-H5, T11-D93).
//!
//! § FORMAT  (little-endian binary, deterministic field-ordering)
//!
//! ```text
//! offset       len   field
//! ─────────────────────────────────────────────────────────────
//! 0            8     magic           = b"CSSLSAVE"
//! 8            4     version         = u32 LE (currently 1)
//! 12           8     omega_len       = u64 LE
//! 20           ω     omega_blob      = ω bytes
//! 20+ω         8     log_len         = u64 LE
//! 28+ω         λ     log_blob        = λ bytes
//! 28+ω+λ      32     attestation     = 32 bytes BLAKE3
//!                                       over (magic ‖ version ‖ omega_len ‖
//!                                              omega_blob ‖ log_len ‖ log_blob)
//! 60+ω+λ       8     trailer_offset  = u64 LE
//!                                       = 28 + ω + λ (start-of-attestation)
//! ─────────────────────────────────────────────────────────────
//! total file size = 68 + ω + λ
//! ```
//!
//! ## Ω-tensor blob shape
//!
//! ```text
//! n_tensors   : u32 LE
//! repeat n_tensors times :
//!   name_len  : u32 LE
//!   name      : `name_len` UTF-8 bytes
//!   type_tag  : u8
//!   rank      : u32 LE
//!   shape     : `rank` × u32 LE
//!   strides   : `rank` × u32 LE
//!   ifc_label : u32 LE     (cell-0's IFC-label ; rank-0 only carries one)
//!   data_len  : u64 LE
//!   data      : `data_len` bytes (cell-0's bytes ; H1 will multi-cell-extend)
//! frame       : u64 LE
//! ```
//!
//! Tensors emit in `(name)`-sorted order — the [`crate::OmegaScheduler`]
//! invariant. HashMap iteration is FORBIDDEN per the slice landmines.
//!
//! ## Replay-log blob shape
//!
//! ```text
//! n_events    : u64 LE
//! repeat n_events times :
//!   frame     : u64 LE
//!   kind      : u8       (1 = Sim, 2 = Render, 3 = Audio, 4 = Save, 5 = Net)
//!   payload_len : u32 LE
//!   payload   : `payload_len` bytes
//! ```
//!
//! Events emit in `(frame, kind, payload)` lexicographic order — the
//! [`crate::ReplayLog::sorted_events`] discipline.
//!
//! ## Attestation
//!
//! BLAKE3 hash over `magic ‖ version_le ‖ omega_len_le ‖ omega_blob ‖ log_len_le ‖ log_blob`.
//!
//! Computed AFTER serialization completes, then appended at the END of the
//! file with an 8-byte trailer-offset. This shape supports streaming-read
//! verification : a reader can seek to `file_size − 40`, read the trailer-
//! offset + attestation, then re-hash the prefix to verify.
//!
//! ## Forward-compatibility
//!
//! - Version field reserved for migration — see `LoadError::UnsupportedVersion`.
//! - The trailing 32-byte slot after `attestation` is reserved for an
//!   Ed25519 signature (DEFERRED ; D-1 in the crate-root doc-block).
//!   Stage-0 readers ignore any bytes between `attestation` and
//!   `trailer_offset+40` ; that's how the signature would slot in without
//!   a format-version bump.
//!
//! ## PRIME-DIRECTIVE alignment
//!
//! - **Magic + version + attestation** : the fixed magic-prefix prevents
//!   an attacker from feeding a save-shaped non-CSSLSAVE file ; the
//!   version field forces a major-version-bump for any breaking change ;
//!   the attestation HARD-FAILS on tamper.
//! - **Deterministic field-ordering** : two Substrate runs producing the
//!   same logical state MUST serialize byte-identically. Any divergence
//!   is a determinism bug per `specs/30_SUBSTRATE.csl § R-10`.
//! - **No HashMap iteration** : enforced by walking `Vec<(String, OmegaTensor)>`
//!   in sorted-order via [`crate::OmegaScheduler::insert_tensor`].

use cssl_telemetry::ContentHash;

use crate::error::LoadError;
use crate::omega::{OmegaCell, OmegaScheduler, OmegaTensor, ReplayEvent, ReplayKind, ReplayLog};

// ───────────────────────────────────────────────────────────────────────
// § Magic + version constants.
// ───────────────────────────────────────────────────────────────────────

/// Magic header — first 8 bytes of every CSSLSAVE file.
pub const FORMAT_MAGIC: &[u8; 8] = b"CSSLSAVE";

/// Current format version. Migration is deferred to a later slice ;
/// loading a different version returns [`LoadError::UnsupportedVersion`].
pub const FORMAT_VERSION: u32 = 1;

// ───────────────────────────────────────────────────────────────────────
// § Type tags — STABLE from S8-H5 forward.
// ───────────────────────────────────────────────────────────────────────

/// Type tag for `u8` cells.
pub const OMEGA_TYPE_TAG_U8: u8 = 1;
/// Type tag for `i32` cells.
pub const OMEGA_TYPE_TAG_I32: u8 = 2;
/// Type tag for `i64` cells.
pub const OMEGA_TYPE_TAG_I64: u8 = 3;
/// Type tag for `f32` cells.
pub const OMEGA_TYPE_TAG_F32: u8 = 4;
/// Type tag for `f64` cells.
pub const OMEGA_TYPE_TAG_F64: u8 = 5;

/// Set of recognized type-tags ; readers reject anything outside this set.
pub const OMEGA_TYPE_TAGS: [u8; 5] = [
    OMEGA_TYPE_TAG_U8,
    OMEGA_TYPE_TAG_I32,
    OMEGA_TYPE_TAG_I64,
    OMEGA_TYPE_TAG_F32,
    OMEGA_TYPE_TAG_F64,
];

/// 32-byte BLAKE3 attestation hash.
pub type AttestationHash = ContentHash;

/// Header overhead in bytes : 8 magic + 4 version + 8 omega_len + 8 log_len + 32 attestation + 8 trailer_offset.
pub const HEADER_OVERHEAD: u64 = 8 + 4 + 8 + 8 + 32 + 8;

/// Minimum viable file size : header overhead with empty omega + empty log.
/// In bytes : 68 (no Ω-tensor body, no replay-log body).
pub const MIN_FILE_SIZE: u64 = HEADER_OVERHEAD;

// ───────────────────────────────────────────────────────────────────────
// § SaveFile — in-memory representation of a complete save.
// ───────────────────────────────────────────────────────────────────────

/// Versioned save-file payload.
///
/// Holds the canonical Ω-tensor + replay-log + their attestation. Construct
/// via [`SaveFile::from_scheduler`] ; serialize via [`SaveFile::to_bytes`] ;
/// parse from bytes via [`SaveFile::from_bytes`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveFile {
    /// Format version (always [`FORMAT_VERSION`] at write-time).
    pub version: u32,
    /// Snapshot of the Ω-tensor portion of the scheduler.
    pub omega: Vec<(String, OmegaTensor)>,
    /// Frame counter at save-time.
    pub frame: u64,
    /// Replay-log captured up to `frame`.
    pub replay_log: ReplayLog,
    /// BLAKE3 hash over the serialized payload (magic + version + lengths + blobs).
    pub attestation: AttestationHash,
}

impl SaveFile {
    /// Build a [`SaveFile`] from an [`OmegaScheduler`]. Computes the
    /// attestation hash over the canonical serialization.
    #[must_use]
    pub fn from_scheduler(scheduler: &OmegaScheduler) -> Self {
        let mut sf = Self {
            version: FORMAT_VERSION,
            omega: scheduler.snapshot_tensors(),
            frame: scheduler.frame,
            replay_log: ReplayLog {
                events: scheduler.replay_log.sorted_events(),
            },
            attestation: ContentHash::zero(),
        };
        sf.recompute_attestation();
        sf
    }

    /// Reconstruct an [`OmegaScheduler`] from this save-file. The returned
    /// scheduler holds the Ω-tensor + frame + replay-log byte-identically
    /// to what was saved.
    #[must_use]
    pub fn into_scheduler(self) -> OmegaScheduler {
        OmegaScheduler {
            tensors: self.omega,
            frame: self.frame,
            replay_log: self.replay_log,
        }
    }

    /// Take a snapshot view of the Ω-tensor portion (the assertion target
    /// for `replay(save).snapshot() == save.snapshot()`).
    #[must_use]
    pub fn snapshot_omega(&self) -> Vec<(String, OmegaTensor)> {
        self.omega.clone()
    }

    /// Recompute the attestation hash over the current omega + replay-log
    /// fields. Called by [`Self::from_scheduler`] ; rarely called by
    /// external code unless mutating the save in-place (which we
    /// discourage — produce a new save instead).
    pub fn recompute_attestation(&mut self) {
        let omega_blob = serialize_omega(&self.omega, self.frame);
        let log_blob = serialize_replay_log(&self.replay_log);
        self.attestation = compute_attestation(self.version, &omega_blob, &log_blob);
    }

    /// Verify the stored attestation matches a fresh BLAKE3 over the
    /// payload. Returns `Ok(())` on match, [`LoadError::AttestationMismatch`]
    /// on mismatch.
    ///
    /// # Errors
    /// [`LoadError::AttestationMismatch`] if the stored attestation does
    /// not match a fresh hash.
    pub fn verify_attestation(&self) -> Result<(), LoadError> {
        let omega_blob = serialize_omega(&self.omega, self.frame);
        let log_blob = serialize_replay_log(&self.replay_log);
        let fresh = compute_attestation(self.version, &omega_blob, &log_blob);
        if fresh == self.attestation {
            Ok(())
        } else {
            Err(LoadError::AttestationMismatch)
        }
    }

    /// Serialize this save-file to bytes per the canonical format above.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let omega_blob = serialize_omega(&self.omega, self.frame);
        let log_blob = serialize_replay_log(&self.replay_log);
        let omega_len = omega_blob.len() as u64;
        let log_len = log_blob.len() as u64;
        let body_len = 8 + 4 + 8 + omega_len + 8 + log_len;
        let trailer_offset = body_len; // start-of-attestation

        let mut out = Vec::with_capacity(body_len as usize + 32 + 8);
        out.extend_from_slice(FORMAT_MAGIC);
        out.extend_from_slice(&self.version.to_le_bytes());
        out.extend_from_slice(&omega_len.to_le_bytes());
        out.extend_from_slice(&omega_blob);
        out.extend_from_slice(&log_len.to_le_bytes());
        out.extend_from_slice(&log_blob);
        out.extend_from_slice(&self.attestation.0);
        out.extend_from_slice(&trailer_offset.to_le_bytes());
        out
    }

    /// Parse + validate a save-file from bytes.
    ///
    /// # Errors
    /// Returns the corresponding [`LoadError`] variant on any of :
    /// truncation, magic-mismatch, version-mismatch, length-overflow,
    /// trailer-offset-mismatch, attestation-mismatch, or malformed
    /// inner blobs.
    pub fn from_bytes(buf: &[u8]) -> Result<Self, LoadError> {
        let total = buf.len() as u64;
        if total < MIN_FILE_SIZE {
            return Err(LoadError::Truncated(total, MIN_FILE_SIZE));
        }

        // Magic.
        if &buf[0..8] != FORMAT_MAGIC {
            return Err(LoadError::BadMagic);
        }

        // Version.
        let version = u32_le(&buf[8..12]);
        if version != FORMAT_VERSION {
            return Err(LoadError::UnsupportedVersion {
                got: version,
                expected: FORMAT_VERSION,
            });
        }

        // Ω-tensor blob.
        let omega_len = u64_le(&buf[12..20]);
        let after_omega_len_off = 20u64;
        let remaining_after_omega_header = total
            .checked_sub(after_omega_len_off)
            .ok_or(LoadError::Truncated(total, MIN_FILE_SIZE))?;
        if omega_len > remaining_after_omega_header {
            return Err(LoadError::OmegaBlobOverflow {
                claimed: omega_len,
                remaining: remaining_after_omega_header,
            });
        }
        let omega_end = after_omega_len_off + omega_len;
        let omega_blob = &buf[after_omega_len_off as usize..omega_end as usize];

        // Replay-log blob.
        if omega_end + 8 > total {
            return Err(LoadError::Truncated(total, omega_end + 8));
        }
        let log_len = u64_le(&buf[omega_end as usize..omega_end as usize + 8]);
        let after_log_len_off = omega_end + 8;
        let remaining_after_log_header = total
            .checked_sub(after_log_len_off)
            .ok_or(LoadError::Truncated(total, MIN_FILE_SIZE))?;
        if log_len > remaining_after_log_header {
            return Err(LoadError::ReplayBlobOverflow {
                claimed: log_len,
                remaining: remaining_after_log_header,
            });
        }
        let log_end = after_log_len_off + log_len;
        let log_blob = &buf[after_log_len_off as usize..log_end as usize];

        // Attestation.
        if log_end + 32 + 8 > total {
            return Err(LoadError::Truncated(total, log_end + 32 + 8));
        }
        let mut attestation = [0u8; 32];
        attestation.copy_from_slice(&buf[log_end as usize..(log_end + 32) as usize]);
        let stored_attestation = ContentHash(attestation);

        // Trailer-offset.
        let trailer_off_byte = (log_end + 32) as usize;
        let stored_trailer_offset = u64_le(&buf[trailer_off_byte..trailer_off_byte + 8]);
        let expected_trailer_offset = log_end;
        if stored_trailer_offset != expected_trailer_offset {
            return Err(LoadError::TrailerOffsetMismatch {
                got: stored_trailer_offset,
                expected: expected_trailer_offset,
            });
        }

        // Verify attestation BEFORE parsing the inner blobs — fail fast.
        let fresh_attestation = compute_attestation(version, omega_blob, log_blob);
        if fresh_attestation != stored_attestation {
            return Err(LoadError::AttestationMismatch);
        }

        // Parse inner blobs only after the cryptographic gate has passed.
        let (omega, frame) = parse_omega(omega_blob)?;
        let replay_log = parse_replay_log(log_blob)?;

        Ok(Self {
            version,
            omega,
            frame,
            replay_log,
            attestation: stored_attestation,
        })
    }
}

// ───────────────────────────────────────────────────────────────────────
// § Ω-tensor blob serialization.
// ───────────────────────────────────────────────────────────────────────

fn serialize_omega(tensors: &[(String, OmegaTensor)], frame: u64) -> Vec<u8> {
    // Pre-size estimate : 4 bytes per tensor name-len + names + per-tensor metadata.
    let mut out = Vec::with_capacity(64 + tensors.len() * 64);
    out.extend_from_slice(&(tensors.len() as u32).to_le_bytes());
    for (name, t) in tensors {
        out.extend_from_slice(&(name.len() as u32).to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        // Stage-0 single-cell tensors emit cell-0's type_tag + ifc_label + data.
        // H1 multi-cell tensors will lift this to a per-cell loop.
        let cell0 = t.cells.first();
        let type_tag = cell0.map_or(OMEGA_TYPE_TAG_U8, |c| c.type_tag);
        out.push(type_tag);
        out.extend_from_slice(&t.rank.to_le_bytes());
        for &dim in &t.shape {
            out.extend_from_slice(&dim.to_le_bytes());
        }
        for &stride in &t.strides {
            out.extend_from_slice(&stride.to_le_bytes());
        }
        let ifc_label = cell0.map_or(0u32, |c| c.ifc_label);
        out.extend_from_slice(&ifc_label.to_le_bytes());
        let data = cell0.map_or(&[][..], |c| c.data.as_slice());
        out.extend_from_slice(&(data.len() as u64).to_le_bytes());
        out.extend_from_slice(data);
    }
    out.extend_from_slice(&frame.to_le_bytes());
    out
}

fn parse_omega(buf: &[u8]) -> Result<(Vec<(String, OmegaTensor)>, u64), LoadError> {
    let mut cursor = 0usize;
    if buf.len() < 4 {
        return Err(LoadError::Truncated(buf.len() as u64, 4));
    }
    let n = u32_le(&buf[cursor..cursor + 4]) as usize;
    cursor += 4;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        // name
        if buf.len() < cursor + 4 {
            return Err(LoadError::Truncated(buf.len() as u64, (cursor + 4) as u64));
        }
        let name_len = u32_le(&buf[cursor..cursor + 4]) as usize;
        cursor += 4;
        if buf.len() < cursor + name_len {
            return Err(LoadError::Truncated(
                buf.len() as u64,
                (cursor + name_len) as u64,
            ));
        }
        let name = std::str::from_utf8(&buf[cursor..cursor + name_len])
            .map_err(|_| LoadError::OmegaDataUnderflow {
                claimed_bytes: name_len as u64,
                actual_bytes: 0,
            })?
            .to_string();
        cursor += name_len;
        // type_tag
        if buf.len() < cursor + 1 {
            return Err(LoadError::Truncated(buf.len() as u64, (cursor + 1) as u64));
        }
        let type_tag = buf[cursor];
        cursor += 1;
        if !OMEGA_TYPE_TAGS.contains(&type_tag) {
            return Err(LoadError::UnknownTypeTag(type_tag));
        }
        // rank
        if buf.len() < cursor + 4 {
            return Err(LoadError::Truncated(buf.len() as u64, (cursor + 4) as u64));
        }
        let rank = u32_le(&buf[cursor..cursor + 4]);
        cursor += 4;
        // shape (rank u32) + strides (rank u32)
        let dim_bytes = (rank as usize) * 4;
        let need = cursor + dim_bytes * 2;
        if buf.len() < need {
            return Err(LoadError::Truncated(buf.len() as u64, need as u64));
        }
        let mut shape = Vec::with_capacity(rank as usize);
        for _ in 0..rank as usize {
            shape.push(u32_le(&buf[cursor..cursor + 4]));
            cursor += 4;
        }
        let mut strides = Vec::with_capacity(rank as usize);
        for _ in 0..rank as usize {
            strides.push(u32_le(&buf[cursor..cursor + 4]));
            cursor += 4;
        }
        // shape/rank consistency check (defense-in-depth ; rank is the source of truth)
        if shape.len() as u32 != rank {
            return Err(LoadError::RankShapeMismatch {
                claimed_rank: rank,
                actual_dims: shape.len() as u32,
            });
        }
        // ifc_label
        if buf.len() < cursor + 4 {
            return Err(LoadError::Truncated(buf.len() as u64, (cursor + 4) as u64));
        }
        let ifc_label = u32_le(&buf[cursor..cursor + 4]);
        cursor += 4;
        // data
        if buf.len() < cursor + 8 {
            return Err(LoadError::Truncated(buf.len() as u64, (cursor + 8) as u64));
        }
        let data_len = u64_le(&buf[cursor..cursor + 8]);
        cursor += 8;
        let need = cursor + data_len as usize;
        if buf.len() < need {
            return Err(LoadError::OmegaDataUnderflow {
                claimed_bytes: data_len,
                actual_bytes: (buf.len() - cursor) as u64,
            });
        }
        let data = buf[cursor..cursor + data_len as usize].to_vec();
        cursor += data_len as usize;

        let cell = OmegaCell::with_label(type_tag, data, ifc_label);
        let tensor = OmegaTensor {
            rank,
            shape,
            strides,
            cells: vec![cell],
        };
        out.push((name, tensor));
    }

    // frame
    if buf.len() < cursor + 8 {
        return Err(LoadError::Truncated(buf.len() as u64, (cursor + 8) as u64));
    }
    let frame = u64_le(&buf[cursor..cursor + 8]);

    Ok((out, frame))
}

// ───────────────────────────────────────────────────────────────────────
// § Replay-log blob serialization.
// ───────────────────────────────────────────────────────────────────────

fn serialize_replay_log(log: &ReplayLog) -> Vec<u8> {
    let events = log.sorted_events();
    let mut out = Vec::with_capacity(8 + events.len() * 16);
    out.extend_from_slice(&(events.len() as u64).to_le_bytes());
    for e in &events {
        out.extend_from_slice(&e.frame.to_le_bytes());
        out.push(e.kind as u8);
        out.extend_from_slice(&(e.payload.len() as u32).to_le_bytes());
        out.extend_from_slice(&e.payload);
    }
    out
}

fn parse_replay_log(buf: &[u8]) -> Result<ReplayLog, LoadError> {
    if buf.is_empty() {
        return Ok(ReplayLog::new());
    }
    if buf.len() < 8 {
        return Err(LoadError::Truncated(buf.len() as u64, 8));
    }
    let n = u64_le(&buf[0..8]) as usize;
    let mut cursor = 8usize;
    let mut events = Vec::with_capacity(n);
    for _ in 0..n {
        if buf.len() < cursor + 8 + 1 + 4 {
            return Err(LoadError::Truncated(
                buf.len() as u64,
                (cursor + 8 + 1 + 4) as u64,
            ));
        }
        let frame = u64_le(&buf[cursor..cursor + 8]);
        cursor += 8;
        let kind_byte = buf[cursor];
        cursor += 1;
        let kind = ReplayKind::from_byte(kind_byte).ok_or(LoadError::UnknownEventTag(kind_byte))?;
        let payload_len = u32_le(&buf[cursor..cursor + 4]) as usize;
        cursor += 4;
        if buf.len() < cursor + payload_len {
            return Err(LoadError::Truncated(
                buf.len() as u64,
                (cursor + payload_len) as u64,
            ));
        }
        let payload = buf[cursor..cursor + payload_len].to_vec();
        cursor += payload_len;
        events.push(ReplayEvent::new(frame, kind, payload));
    }
    Ok(ReplayLog { events })
}

// ───────────────────────────────────────────────────────────────────────
// § Attestation hash.
// ───────────────────────────────────────────────────────────────────────

fn compute_attestation(version: u32, omega_blob: &[u8], log_blob: &[u8]) -> AttestationHash {
    // BLAKE3 over (magic || version_le || omega_len_le || omega_blob || log_len_le || log_blob).
    //
    // We reuse `cssl_telemetry::ContentHash::hash` (which wraps `blake3::hash`)
    // rather than depend on `blake3` directly. The attestation input is
    // assembled into a single byte-vector ; for the save-files we expect at
    // stage-0 (kilobytes-to-megabytes) the one-shot hash is fast enough.
    // A future slice can switch to streaming `blake3::Hasher::update` if
    // huge saves become a thing.
    let mut input =
        Vec::with_capacity(FORMAT_MAGIC.len() + 4 + 8 + omega_blob.len() + 8 + log_blob.len());
    input.extend_from_slice(FORMAT_MAGIC);
    input.extend_from_slice(&version.to_le_bytes());
    input.extend_from_slice(&(omega_blob.len() as u64).to_le_bytes());
    input.extend_from_slice(omega_blob);
    input.extend_from_slice(&(log_blob.len() as u64).to_le_bytes());
    input.extend_from_slice(log_blob);
    ContentHash::hash(&input)
}

// ───────────────────────────────────────────────────────────────────────
// § Helper readers.
// ───────────────────────────────────────────────────────────────────────

fn u32_le(b: &[u8]) -> u32 {
    let mut a = [0u8; 4];
    a.copy_from_slice(&b[..4]);
    u32::from_le_bytes(a)
}

fn u64_le(b: &[u8]) -> u64 {
    let mut a = [0u8; 8];
    a.copy_from_slice(&b[..8]);
    u64::from_le_bytes(a)
}

// ───────────────────────────────────────────────────────────────────────
// § Tests — deterministic-format, round-trip, attestation-mismatch.
// ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_scheduler() -> OmegaScheduler {
        let mut s = OmegaScheduler::new();
        s.insert_tensor(
            "alpha",
            OmegaTensor::scalar(OmegaCell::with_label(
                OMEGA_TYPE_TAG_I32,
                42i32.to_le_bytes().to_vec(),
                7,
            )),
        );
        s.insert_tensor(
            "beta",
            OmegaTensor::scalar(OmegaCell::new(
                OMEGA_TYPE_TAG_F64,
                std::f64::consts::PI.to_le_bytes().to_vec(),
            )),
        );
        s.frame = 5;
        s.replay_log
            .append(ReplayEvent::new(0, ReplayKind::Sim, vec![1, 2, 3]));
        s.replay_log
            .append(ReplayEvent::new(1, ReplayKind::Render, vec![4, 5]));
        s
    }

    #[test]
    fn save_file_round_trip_byte_exact() {
        let s = make_scheduler();
        let sf = SaveFile::from_scheduler(&s);
        let bytes = sf.to_bytes();
        let parsed = SaveFile::from_bytes(&bytes).expect("parse must succeed");
        assert_eq!(parsed, sf);
    }

    #[test]
    fn save_file_to_bytes_starts_with_magic_and_version() {
        let s = make_scheduler();
        let sf = SaveFile::from_scheduler(&s);
        let bytes = sf.to_bytes();
        assert_eq!(&bytes[0..8], FORMAT_MAGIC);
        assert_eq!(u32_le(&bytes[8..12]), FORMAT_VERSION);
    }

    #[test]
    fn save_file_parses_back_to_identical_scheduler() {
        let s = make_scheduler();
        let sf = SaveFile::from_scheduler(&s);
        let bytes = sf.to_bytes();
        let parsed = SaveFile::from_bytes(&bytes).expect("parse must succeed");
        let s2 = parsed.into_scheduler();
        assert_eq!(s, s2);
    }

    #[test]
    fn save_file_serialization_is_deterministic() {
        // Two schedulers produced from the same insert-sequence must
        // produce byte-identical save-files.
        let s1 = make_scheduler();
        let s2 = make_scheduler();
        let b1 = SaveFile::from_scheduler(&s1).to_bytes();
        let b2 = SaveFile::from_scheduler(&s2).to_bytes();
        assert_eq!(b1, b2);
    }

    #[test]
    fn save_file_serialization_independent_of_insert_order() {
        // Insert in (zebra, alpha, mango) AND (alpha, mango, zebra) — the
        // OmegaScheduler::insert_tensor invariant sorts ; the save-file
        // serializes byte-identically.
        let mut s1 = OmegaScheduler::new();
        s1.insert_tensor("zebra", OmegaTensor::default());
        s1.insert_tensor("alpha", OmegaTensor::default());
        s1.insert_tensor("mango", OmegaTensor::default());
        let mut s2 = OmegaScheduler::new();
        s2.insert_tensor("alpha", OmegaTensor::default());
        s2.insert_tensor("mango", OmegaTensor::default());
        s2.insert_tensor("zebra", OmegaTensor::default());
        let b1 = SaveFile::from_scheduler(&s1).to_bytes();
        let b2 = SaveFile::from_scheduler(&s2).to_bytes();
        assert_eq!(b1, b2);
    }

    #[test]
    fn ifc_label_survives_save_load() {
        let mut s = OmegaScheduler::new();
        s.insert_tensor(
            "secret",
            OmegaTensor::scalar(OmegaCell::with_label(
                OMEGA_TYPE_TAG_I64,
                {
                    #[allow(clippy::cast_possible_wrap)]
                    let v = 0xDEAD_BEEF_FACE_CAFE_u64 as i64;
                    v.to_le_bytes().to_vec()
                },
                0xCAFE_BABE,
            )),
        );
        let sf = SaveFile::from_scheduler(&s);
        let bytes = sf.to_bytes();
        let parsed = SaveFile::from_bytes(&bytes).expect("must parse");
        let cell = &parsed.omega[0].1.cells[0];
        assert_eq!(cell.ifc_label, 0xCAFE_BABE);
    }

    #[test]
    fn bad_magic_is_rejected() {
        let mut bytes = SaveFile::from_scheduler(&make_scheduler()).to_bytes();
        bytes[0] = b'X';
        let err = SaveFile::from_bytes(&bytes).unwrap_err();
        assert_eq!(err, LoadError::BadMagic);
    }

    #[test]
    fn unsupported_version_is_rejected() {
        let mut bytes = SaveFile::from_scheduler(&make_scheduler()).to_bytes();
        // Bump the version byte.
        bytes[8] = 99;
        let err = SaveFile::from_bytes(&bytes).unwrap_err();
        assert_eq!(
            err,
            LoadError::UnsupportedVersion {
                got: 99,
                expected: FORMAT_VERSION,
            }
        );
    }

    #[test]
    fn truncated_file_is_rejected() {
        let bytes = SaveFile::from_scheduler(&make_scheduler()).to_bytes();
        // Lop off the trailer-offset.
        let truncated = &bytes[..bytes.len() - 4];
        let err = SaveFile::from_bytes(truncated).unwrap_err();
        match err {
            LoadError::Truncated(actual, _) => {
                assert_eq!(actual, truncated.len() as u64);
            }
            other => panic!("expected Truncated, got {other:?}"),
        }
    }

    #[test]
    fn attestation_tamper_is_rejected() {
        let mut bytes = SaveFile::from_scheduler(&make_scheduler()).to_bytes();
        // Flip a bit in the Ω-tensor blob (somewhere in the body, after the lengths).
        let body_start = 20;
        bytes[body_start + 4] ^= 0xFF;
        let err = SaveFile::from_bytes(&bytes).unwrap_err();
        assert_eq!(err, LoadError::AttestationMismatch);
    }

    #[test]
    fn attestation_mismatch_in_replay_log_is_rejected() {
        let s = make_scheduler();
        let sf = SaveFile::from_scheduler(&s);
        // Flip a bit in the replay-log body. Locate the log_len header :
        // 8 magic + 4 version + 8 omega_len = offset 20 ; omega_blob follows ;
        // the log_len is at 20 + omega_len. We compute it precisely by
        // re-deriving omega_len from the file rather than guessing.
        let mut tampered = sf.to_bytes();
        let omega_len = u64_le(&tampered[12..20]) as usize;
        let log_body_start = 20 + omega_len + 8;
        if log_body_start < tampered.len() - 32 - 8 {
            // Only flip if there is replay-log body to corrupt.
            tampered[log_body_start] ^= 0xAA;
            let err = SaveFile::from_bytes(&tampered).unwrap_err();
            assert_eq!(err, LoadError::AttestationMismatch);
        }
    }

    #[test]
    fn attestation_recompute_is_idempotent() {
        let s = make_scheduler();
        let sf1 = SaveFile::from_scheduler(&s);
        let mut sf2 = sf1.clone();
        sf2.recompute_attestation();
        assert_eq!(sf1.attestation, sf2.attestation);
    }

    #[test]
    fn verify_attestation_passes_on_well_formed_save() {
        let s = make_scheduler();
        let sf = SaveFile::from_scheduler(&s);
        sf.verify_attestation().expect("must verify");
    }

    #[test]
    fn verify_attestation_rejects_post_construct_mutation() {
        let s = make_scheduler();
        let mut sf = SaveFile::from_scheduler(&s);
        // Mutate a cell post-construct without recomputing attestation.
        sf.omega[0].1.cells[0].data[0] ^= 0x01;
        let err = sf.verify_attestation().unwrap_err();
        assert_eq!(err, LoadError::AttestationMismatch);
    }

    #[test]
    fn unknown_type_tag_in_blob_is_rejected() {
        // Hand-craft a blob with type-tag 99.
        let mut omega_blob = Vec::new();
        omega_blob.extend_from_slice(&1u32.to_le_bytes()); // 1 tensor
        omega_blob.extend_from_slice(&1u32.to_le_bytes()); // name_len = 1
        omega_blob.push(b'x');
        omega_blob.push(99); // unknown type-tag
        omega_blob.extend_from_slice(&0u32.to_le_bytes()); // rank
        omega_blob.extend_from_slice(&0u32.to_le_bytes()); // ifc_label
        omega_blob.extend_from_slice(&0u64.to_le_bytes()); // data_len
        omega_blob.extend_from_slice(&0u64.to_le_bytes()); // frame
        let err = parse_omega(&omega_blob).unwrap_err();
        assert_eq!(err, LoadError::UnknownTypeTag(99));
    }

    #[test]
    fn unknown_event_tag_in_replay_blob_is_rejected() {
        let mut log_blob = Vec::new();
        log_blob.extend_from_slice(&1u64.to_le_bytes()); // 1 event
        log_blob.extend_from_slice(&0u64.to_le_bytes()); // frame = 0
        log_blob.push(99); // unknown kind
        log_blob.extend_from_slice(&0u32.to_le_bytes()); // payload_len = 0
        let err = parse_replay_log(&log_blob).unwrap_err();
        assert_eq!(err, LoadError::UnknownEventTag(99));
    }

    #[test]
    fn empty_scheduler_round_trips() {
        let s = OmegaScheduler::new();
        let sf = SaveFile::from_scheduler(&s);
        let bytes = sf.to_bytes();
        let parsed = SaveFile::from_bytes(&bytes).expect("must parse");
        assert_eq!(parsed.omega.len(), 0);
        assert_eq!(parsed.frame, 0);
        assert!(parsed.replay_log.is_empty());
    }

    #[test]
    fn large_replay_log_round_trips() {
        let mut s = OmegaScheduler::new();
        for i in 0..100u64 {
            s.replay_log.append(ReplayEvent::new(
                i,
                ReplayKind::Sim,
                (i as u32).to_le_bytes().to_vec(),
            ));
        }
        let sf = SaveFile::from_scheduler(&s);
        let bytes = sf.to_bytes();
        let parsed = SaveFile::from_bytes(&bytes).expect("must parse");
        assert_eq!(parsed.replay_log.events.len(), 100);
        for (i, e) in parsed.replay_log.events.iter().enumerate() {
            assert_eq!(e.frame, i as u64);
            assert_eq!(e.kind, ReplayKind::Sim);
        }
    }

    #[test]
    fn snapshot_omega_is_independent_of_save_file_after_clone() {
        let s = make_scheduler();
        let sf = SaveFile::from_scheduler(&s);
        let snap = sf.snapshot_omega();
        // Snapshot is a clone, not an alias.
        assert_eq!(snap, sf.omega);
        assert!(!std::ptr::eq(snap.as_ptr(), sf.omega.as_ptr()));
    }

    #[test]
    fn header_overhead_constant_matches_layout() {
        // 8 magic + 4 version + 8 omega_len + 8 log_len + 32 attestation + 8 trailer_offset = 68
        assert_eq!(HEADER_OVERHEAD, 68);
        assert_eq!(MIN_FILE_SIZE, 68);
    }

    #[test]
    fn file_size_matches_header_plus_blobs() {
        let s = make_scheduler();
        let sf = SaveFile::from_scheduler(&s);
        let bytes = sf.to_bytes();
        let omega_len = u64_le(&bytes[12..20]);
        let log_off = 20 + omega_len as usize;
        let log_len = u64_le(&bytes[log_off..log_off + 8]);
        let expected = HEADER_OVERHEAD + omega_len + log_len;
        assert_eq!(bytes.len() as u64, expected);
    }

    #[test]
    fn trailer_offset_matches_attestation_start() {
        let s = make_scheduler();
        let bytes = SaveFile::from_scheduler(&s).to_bytes();
        let omega_len = u64_le(&bytes[12..20]);
        let log_off = 20 + omega_len as usize;
        let log_len = u64_le(&bytes[log_off..log_off + 8]);
        let expected_trailer_off = log_off as u64 + 8 + log_len;
        let actual_trailer_off = u64_le(&bytes[bytes.len() - 8..]);
        assert_eq!(actual_trailer_off, expected_trailer_off);
    }
}
