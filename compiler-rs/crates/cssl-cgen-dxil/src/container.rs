//! DXBC container format — the wrapper around DXIL bitcode that the D3D12
//! runtime + DXIL.dll validator + `ID3D12Device::CreateGraphicsPipelineState`
//! consume.
//!
//! § FORMAT (Microsoft DirectXShaderCompiler `dxc/DxilContainer.h`)
//!
//! ```text
//! +--------------------------------------------------------+
//! | DxilContainerHeader (32 bytes)                         |
//! |  • magic       : u32  = 0x43425844 ('DXBC')            |
//! |  • hash        : [u8; 16]  (DXC MD5-equiv ; we emit 0  |
//! |                             at stage-0 ; D3D12 accepts |
//! |                             zero-hash containers when  |
//! |                             debug-layer ≥ "skip-hash")|
//! |  • container_version_major : u16 = 1                   |
//! |  • container_version_minor : u16 = 0                   |
//! |  • container_size_bytes    : u32  (total file size)    |
//! |  • part_count              : u32                       |
//! +--------------------------------------------------------+
//! | PartOffsetTable : [u32 ; part_count]                   |
//! |   each entry = byte-offset of part header from start   |
//! +--------------------------------------------------------+
//! | Part_0_Header (8 bytes) : magic_4cc + part_size        |
//! | Part_0_Payload (part_size bytes, 4-byte aligned)       |
//! | Part_1_Header ...                                      |
//! | ...                                                    |
//! +--------------------------------------------------------+
//! ```
//!
//! § PART 4CC CODES (subset used by stage-0)
//!   • `DXIL` (0x4C495844) — DXIL bitcode (the LLVM-3.7 bytestream from
//!     `bitcode.rs`).
//!   • `SHEX` (0x58454853) — shader-execution / DXBC-style exec metadata.
//!     We emit a minimal SHEX even on the DXIL path because some D3D12
//!     drivers consult the DXBC-version chunk for stage-classification.
//!   • `ISG1` (0x31475349) — input-signature (vertex / pixel / compute
//!     stages may declare zero inputs ; CS still emits an empty ISG1).
//!   • `OSG1` (0x3147534F) — output-signature.
//!   • `RTS0` (0x30535452) — root-signature (D3D12 binding layout).
//!   • `RDAT` (0x54414452) — runtime-data (resource binding, function
//!     properties for library targets).
//!
//! All multi-byte integers are little-endian on disk.

use thiserror::Error;

/// DXBC container magic — `'D' 'X' 'B' 'C'` little-endian.
pub const DXBC_MAGIC: u32 = 0x4342_5844;

/// Container header version we always emit. The D3D12 runtime accepts
/// `1.0` for every shader-target the LoA-v13 path uses.
pub const CONTAINER_VERSION_MAJOR: u16 = 1;
pub const CONTAINER_VERSION_MINOR: u16 = 0;

/// Length in bytes of the DXBC fixed header (before the part-offset table).
pub const DXBC_HEADER_SIZE: usize = 32;

/// Length in bytes of every part-header (4cc + part_size).
pub const PART_HEADER_SIZE: usize = 8;

/// Compute the four-character-code (little-endian u32) for a 4-ASCII-byte tag.
///
/// # Panics
/// Panics in const-context if `tag.len() != 4`.
#[must_use]
pub const fn fourcc(tag: &[u8; 4]) -> u32 {
    (tag[0] as u32) | ((tag[1] as u32) << 8) | ((tag[2] as u32) << 16) | ((tag[3] as u32) << 24)
}

/// Canonical DXBC part-tags used by the stage-0 emitter.
pub mod part_tag {
    use super::fourcc;

    /// `'D' 'X' 'I' 'L'` — DXIL LLVM-bitcode payload (the main shader body).
    pub const DXIL: u32 = fourcc(b"DXIL");
    /// `'S' 'H' 'E' 'X'` — shader-execution chunk (DXBC-format exec table).
    pub const SHEX: u32 = fourcc(b"SHEX");
    /// `'I' 'S' 'G' '1'` — version-1 input-signature element table.
    pub const ISG1: u32 = fourcc(b"ISG1");
    /// `'O' 'S' 'G' '1'` — version-1 output-signature element table.
    pub const OSG1: u32 = fourcc(b"OSG1");
    /// `'R' 'T' 'S' '0'` — root-signature serialized blob.
    pub const RTS0: u32 = fourcc(b"RTS0");
    /// `'R' 'D' 'A' 'T'` — runtime-data chunk (library / RT binding info).
    pub const RDAT: u32 = fourcc(b"RDAT");
    /// `'P' 'S' 'V' '0'` — pipeline-state-validation chunk (driver hints).
    pub const PSV0: u32 = fourcc(b"PSV0");
}

/// Errors produced while assembling a DXBC container.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ContainerError {
    /// A part payload exceeded the 4 GiB-1 size limit imposed by the u32
    /// part_size field. (Practically impossible for shader code, but we
    /// surface a real error rather than panicking on overflow.)
    #[error("DXBC part '{tag}' payload too large : {size} bytes (max 4 GiB-1)")]
    PartTooLarge {
        tag: String,
        size: usize,
    },
    /// The container-total size overflowed u32.
    #[error("DXBC container total size overflowed u32 : {size} bytes")]
    ContainerTooLarge {
        size: usize,
    },
}

/// One part within a DXBC container. The payload bytes are owned ; the
/// container builder will pad to 4-byte alignment between parts on `finish`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DxbcPart {
    /// Four-character code tag (use constants from [`part_tag`]).
    pub tag: u32,
    /// Raw little-endian payload bytes.
    pub payload: Vec<u8>,
}

impl DxbcPart {
    /// Build a part from a tag + payload bytes.
    #[must_use]
    pub fn new(tag: u32, payload: Vec<u8>) -> Self {
        Self { tag, payload }
    }

    /// Render the four-char tag back to a printable ASCII string for diagnostics.
    #[must_use]
    pub fn tag_string(&self) -> String {
        let bytes = self.tag.to_le_bytes();
        bytes.iter().map(|&b| b as char).collect()
    }
}

/// Builder for a DXBC container. Append parts via [`Self::push_part`], then
/// call [`Self::finish`] to serialize the bytes.
#[derive(Debug, Default, Clone)]
pub struct DxbcContainer {
    parts: Vec<DxbcPart>,
}

impl DxbcContainer {
    /// New empty container.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a part. Order matters for some D3D12 drivers ; the canonical
    /// order is : `RTS0`, `ISG1`, `OSG1`, `PSV0`, `RDAT`, `SHEX`, `DXIL`.
    pub fn push_part(&mut self, part: DxbcPart) {
        self.parts.push(part);
    }

    /// Number of parts currently appended.
    #[must_use]
    pub fn part_count(&self) -> usize {
        self.parts.len()
    }

    /// Read-only access to the parts in append order.
    #[must_use]
    pub fn parts(&self) -> &[DxbcPart] {
        &self.parts
    }

    /// Serialize the container into a single byte vector.
    ///
    /// # Errors
    /// Returns [`ContainerError::PartTooLarge`] if any individual part
    /// payload exceeds u32-max bytes ; [`ContainerError::ContainerTooLarge`]
    /// if the total container size overflows u32.
    pub fn finish(self) -> Result<Vec<u8>, ContainerError> {
        // 1) compute header + offset-table footprint.
        let part_count = self.parts.len();
        let header_size = DXBC_HEADER_SIZE;
        let offset_table_size = part_count
            .checked_mul(4)
            .ok_or(ContainerError::ContainerTooLarge { size: usize::MAX })?;
        let mut running_offset = header_size
            .checked_add(offset_table_size)
            .ok_or(ContainerError::ContainerTooLarge { size: usize::MAX })?;

        // 2) compute each part's offset (header sits @ running_offset, then payload, then 4B-pad).
        let mut part_offsets: Vec<u32> = Vec::with_capacity(part_count);
        for part in &self.parts {
            if part.payload.len() > u32::MAX as usize {
                return Err(ContainerError::PartTooLarge {
                    tag: part.tag_string(),
                    size: part.payload.len(),
                });
            }
            let off_u32 = u32::try_from(running_offset).map_err(|_| {
                ContainerError::ContainerTooLarge {
                    size: running_offset,
                }
            })?;
            part_offsets.push(off_u32);
            // header (8) + payload + 4B-pad
            let payload_padded = (part.payload.len() + 3) & !3;
            running_offset = running_offset
                .checked_add(PART_HEADER_SIZE + payload_padded)
                .ok_or(ContainerError::ContainerTooLarge {
                    size: usize::MAX,
                })?;
        }
        let total_size = running_offset;
        let total_size_u32 = u32::try_from(total_size)
            .map_err(|_| ContainerError::ContainerTooLarge { size: total_size })?;

        // 3) emit bytes.
        let mut out = Vec::with_capacity(total_size);
        // header
        out.extend_from_slice(&DXBC_MAGIC.to_le_bytes());
        // 16-byte hash : zero @ stage-0 (D3D12 debug-layer hash-skip mode accepts).
        out.extend_from_slice(&[0u8; 16]);
        out.extend_from_slice(&CONTAINER_VERSION_MAJOR.to_le_bytes());
        out.extend_from_slice(&CONTAINER_VERSION_MINOR.to_le_bytes());
        out.extend_from_slice(&total_size_u32.to_le_bytes());
        out.extend_from_slice(&u32::try_from(part_count).unwrap_or(u32::MAX).to_le_bytes());
        debug_assert_eq!(out.len(), DXBC_HEADER_SIZE);

        // offset-table
        for off in &part_offsets {
            out.extend_from_slice(&off.to_le_bytes());
        }

        // parts
        for part in &self.parts {
            out.extend_from_slice(&part.tag.to_le_bytes());
            let part_size_u32 =
                u32::try_from(part.payload.len()).expect("part-size pre-checked above");
            out.extend_from_slice(&part_size_u32.to_le_bytes());
            out.extend_from_slice(&part.payload);
            // 4-byte align
            let pad = (4 - (part.payload.len() & 3)) & 3;
            out.extend(std::iter::repeat(0u8).take(pad));
        }
        debug_assert_eq!(out.len(), total_size);
        Ok(out)
    }
}

/// Parse the DXBC header out of a finished container — used by tests + by
/// validator round-trip walkers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedDxbcHeader {
    pub magic: u32,
    pub hash: [u8; 16],
    pub version_major: u16,
    pub version_minor: u16,
    pub container_size: u32,
    pub part_count: u32,
}

impl ParsedDxbcHeader {
    /// Parse the 32-byte header out of `bytes`.
    ///
    /// # Errors
    /// Returns [`ContainerError::ContainerTooLarge`] (used as a generic
    /// "container is malformed" sentinel here) if `bytes` is shorter than
    /// the header or if the magic mismatches.
    pub fn parse(bytes: &[u8]) -> Result<Self, ContainerError> {
        if bytes.len() < DXBC_HEADER_SIZE {
            return Err(ContainerError::ContainerTooLarge { size: bytes.len() });
        }
        let mut buf4 = [0u8; 4];
        let mut buf2 = [0u8; 2];
        buf4.copy_from_slice(&bytes[0..4]);
        let magic = u32::from_le_bytes(buf4);
        if magic != DXBC_MAGIC {
            return Err(ContainerError::ContainerTooLarge { size: bytes.len() });
        }
        let mut hash = [0u8; 16];
        hash.copy_from_slice(&bytes[4..20]);
        buf2.copy_from_slice(&bytes[20..22]);
        let version_major = u16::from_le_bytes(buf2);
        buf2.copy_from_slice(&bytes[22..24]);
        let version_minor = u16::from_le_bytes(buf2);
        buf4.copy_from_slice(&bytes[24..28]);
        let container_size = u32::from_le_bytes(buf4);
        buf4.copy_from_slice(&bytes[28..32]);
        let part_count = u32::from_le_bytes(buf4);
        Ok(Self {
            magic,
            hash,
            version_major,
            version_minor,
            container_size,
            part_count,
        })
    }
}

/// Build a minimal SHEX shader-execution chunk for the given stage-tag +
/// shader-model. The format we emit is the legacy DXBC version-token + size
/// in DWORDs ; D3D12 drivers tolerate a near-empty SHEX as long as the
/// version-token classifies the stage correctly and a `RET` instruction
/// terminates the body.
///
/// § FORMAT (DXBC 5.x version-token, see `dxc/DxilCommon.h`)
/// ```text
/// DWORD 0 : version-token = (stage_class << 16) | (sm_major << 4) | sm_minor
/// DWORD 1 : length-in-dwords (must be ≥ 2)
/// DWORD 2 : opcode-token RET (0x00000010 + extension bits)
/// ```
#[must_use]
pub fn build_shex_chunk(stage_class: u16, sm_major: u8, sm_minor: u8) -> Vec<u8> {
    let version: u32 =
        ((stage_class as u32) << 16) | ((sm_major as u32 & 0xF) << 4) | (sm_minor as u32 & 0xF);
    let length_dwords: u32 = 3;
    // RET opcode = 0x10, instruction-length = 1 dword in the upper 7 bits @ 24..30.
    // Token layout : bits 0..10 opcode, bits 24..30 instruction-length-in-dwords.
    let ret_token: u32 = 0x0000_0010 | (1u32 << 24);
    let mut out = Vec::with_capacity(12);
    out.extend_from_slice(&version.to_le_bytes());
    out.extend_from_slice(&length_dwords.to_le_bytes());
    out.extend_from_slice(&ret_token.to_le_bytes());
    out
}

/// Build an empty `ISG1` (input-signature) chunk : zero elements + zero
/// reserved. D3D12 accepts this for compute shaders (no per-vertex inputs).
#[must_use]
pub fn build_empty_isg1() -> Vec<u8> {
    // ISG1 layout : u32 element_count + u32 reserved-key + (per-element table : empty).
    let mut out = Vec::with_capacity(8);
    out.extend_from_slice(&0u32.to_le_bytes()); // element_count
    out.extend_from_slice(&8u32.to_le_bytes()); // reserved : header-tail offset
    out
}

/// Build an empty `OSG1` (output-signature) chunk — same shape as ISG1.
#[must_use]
pub fn build_empty_osg1() -> Vec<u8> {
    build_empty_isg1()
}

/// Build a minimal serialized root-signature blob (RTS0 chunk).
///
/// § FORMAT (D3D12 root-signature v1.1 serialized form)
/// ```text
/// DWORD  : version (1 = v1.0 ; 2 = v1.1)
/// DWORD  : num_parameters
/// DWORD  : params_offset (always 24 = sizeof(this header))
/// DWORD  : num_static_samplers
/// DWORD  : samplers_offset
/// DWORD  : flags (D3D12_ROOT_SIGNATURE_FLAGS bitfield)
/// ```
///
/// We emit version 1.1 with zero parameters + zero samplers + the
/// "ALLOW_INPUT_ASSEMBLER_INPUT_LAYOUT" flag for VS/PS pipelines (or
/// `DENY_*` flags reset for CS ; caller controls via `flags`).
#[must_use]
pub fn build_minimal_root_signature(flags: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(24);
    out.extend_from_slice(&2u32.to_le_bytes()); // version 1.1
    out.extend_from_slice(&0u32.to_le_bytes()); // num_parameters
    out.extend_from_slice(&24u32.to_le_bytes()); // params_offset
    out.extend_from_slice(&0u32.to_le_bytes()); // num_static_samplers
    out.extend_from_slice(&24u32.to_le_bytes()); // samplers_offset
    out.extend_from_slice(&flags.to_le_bytes()); // flags
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dxbc_magic_is_canonical() {
        // Magic should serialize to ASCII 'D' 'X' 'B' 'C' little-endian.
        let bytes = DXBC_MAGIC.to_le_bytes();
        assert_eq!(&bytes, b"DXBC");
        assert_eq!(DXBC_MAGIC, 0x4342_5844);
    }

    #[test]
    fn fourcc_helper_matches_known_tags() {
        assert_eq!(part_tag::DXIL.to_le_bytes(), *b"DXIL");
        assert_eq!(part_tag::SHEX.to_le_bytes(), *b"SHEX");
        assert_eq!(part_tag::ISG1.to_le_bytes(), *b"ISG1");
        assert_eq!(part_tag::OSG1.to_le_bytes(), *b"OSG1");
        assert_eq!(part_tag::RTS0.to_le_bytes(), *b"RTS0");
        assert_eq!(part_tag::RDAT.to_le_bytes(), *b"RDAT");
    }

    #[test]
    fn empty_container_round_trips_header() {
        let bytes = DxbcContainer::new().finish().unwrap();
        assert_eq!(bytes.len(), DXBC_HEADER_SIZE);
        let header = ParsedDxbcHeader::parse(&bytes).unwrap();
        assert_eq!(header.magic, DXBC_MAGIC);
        assert_eq!(header.version_major, 1);
        assert_eq!(header.version_minor, 0);
        assert_eq!(header.container_size, 32);
        assert_eq!(header.part_count, 0);
    }

    #[test]
    fn single_part_container_layout() {
        let mut c = DxbcContainer::new();
        c.push_part(DxbcPart::new(part_tag::DXIL, vec![0xCAu8, 0xFE, 0xBA, 0xBE]));
        let bytes = c.finish().unwrap();
        // header(32) + offset_table(4) + part_header(8) + payload(4) = 48
        assert_eq!(bytes.len(), 48);
        let header = ParsedDxbcHeader::parse(&bytes).unwrap();
        assert_eq!(header.part_count, 1);
        assert_eq!(header.container_size, 48);
        // offset-table entry @ bytes[32..36] should be 32 + 4 = 36 (header + offset_table).
        let mut buf4 = [0u8; 4];
        buf4.copy_from_slice(&bytes[32..36]);
        let part_off = u32::from_le_bytes(buf4);
        assert_eq!(part_off, 36);
        // part header @ bytes[36..44] : tag 'DXIL' + size 4
        buf4.copy_from_slice(&bytes[36..40]);
        assert_eq!(u32::from_le_bytes(buf4), part_tag::DXIL);
        buf4.copy_from_slice(&bytes[40..44]);
        assert_eq!(u32::from_le_bytes(buf4), 4);
        // payload
        assert_eq!(&bytes[44..48], &[0xCAu8, 0xFE, 0xBA, 0xBE]);
    }

    #[test]
    fn part_padding_aligns_to_4_bytes() {
        let mut c = DxbcContainer::new();
        // 5-byte payload should be padded to 8.
        c.push_part(DxbcPart::new(part_tag::DXIL, vec![1u8, 2, 3, 4, 5]));
        c.push_part(DxbcPart::new(part_tag::SHEX, vec![0xAA]));
        let bytes = c.finish().unwrap();
        // header(32) + table(8) + part1(8 + 8) + part2(8 + 4) = 68
        assert_eq!(bytes.len(), 68);
    }

    #[test]
    fn shex_chunk_has_canonical_version_token() {
        // SM 6.0 compute = stage-class 0x4358 ('CX' inverted) — but stage_class
        // value is opaque to this helper ; we just verify the encoding shape.
        let chunk = build_shex_chunk(0x4358, 6, 0);
        assert_eq!(chunk.len(), 12);
        let mut buf = [0u8; 4];
        buf.copy_from_slice(&chunk[0..4]);
        let token = u32::from_le_bytes(buf);
        assert_eq!(token >> 16, 0x4358);
        assert_eq!((token >> 4) & 0xF, 6);
        assert_eq!(token & 0xF, 0);
        // length-in-dwords
        buf.copy_from_slice(&chunk[4..8]);
        assert_eq!(u32::from_le_bytes(buf), 3);
    }

    #[test]
    fn root_signature_blob_canonical_shape() {
        let blob = build_minimal_root_signature(0);
        assert_eq!(blob.len(), 24);
        let mut buf = [0u8; 4];
        buf.copy_from_slice(&blob[0..4]);
        assert_eq!(u32::from_le_bytes(buf), 2); // version 1.1
        buf.copy_from_slice(&blob[4..8]);
        assert_eq!(u32::from_le_bytes(buf), 0); // num_parameters
    }
}
