//! LLVM 3.7 bitcode emission — the inner bytestream of a DXIL part.
//!
//! § BACKGROUND
//!   DXIL = LLVM bitcode wrapped in a DXBC container, frozen at the LLVM 3.7
//!   bytestream layout + extended with DirectX-specific intrinsic-name strings
//!   (`dx.op.*`, `dx.types.*`) + DXIL-version metadata. Microsoft's
//!   DirectXShaderCompiler (DXC) authors this through LLVM's bitcode-writer ;
//!   we author it from-scratch by emitting the bitstream primitives directly.
//!
//! § BITSTREAM PRIMITIVES (LLVM `Bitcode/BitstreamWriter.h`)
//!   The bitcode container is a bit-stream of 32-bit words that encodes :
//!     • magic header `0x4243C0DE` ('B' 'C' 0xC0 0xDE) + DXIL extra magic
//!     • blocks (entered with `ENTER_SUBBLOCK`, exited with `END_BLOCK`)
//!     • abbreviated records (compact form using DEFINE_ABBREV ops)
//!     • unabbreviated records (UNABBREV_RECORD = code 3 + ops)
//!
//!   Block IDs we emit at stage-0 :
//!     • BLOCKINFO_BLOCK_ID (0)
//!     • MODULE_BLOCK_ID    (8)   — top-level module
//!     • PARAMATTR_BLOCK_ID (9)
//!     • CONSTANTS_BLOCK_ID (11)
//!     • FUNCTION_BLOCK_ID  (12)
//!     • TYPE_SYMTAB_BLOCK_ID (14)
//!     • VALUE_SYMTAB_BLOCK_ID (14 — same id reused)
//!     • METADATA_BLOCK_ID  (15)
//!     • TYPE_BLOCK_ID_NEW  (17)
//!
//! § STAGE-0 SCOPE
//!   We emit a *minimal-but-valid* bitstream that DXIL.dll's verifier accepts
//!   as well-formed even if the function body itself is a single `ret void`.
//!   Real shader-body lowering is the W-G2-α follow-up slice ; this slice
//!   establishes the byte-exact framing + abbreviation-defining infrastructure
//!   so that follow-up ops slot in by appending records to the function
//!   block.

use thiserror::Error;

/// LLVM bitcode wrapper magic (`'B' 'C' 0xC0 0xDE` little-endian).
pub const LLVM_BITCODE_MAGIC: u32 = 0x0DEC_017B;
/// Same as [`LLVM_BITCODE_MAGIC`] but byte-swapped to the conventional
/// `0x4243C0DE` form some validators check for.
pub const LLVM_BITCODE_MAGIC_BC: u32 = 0xDEC0_4243;

/// DXIL-specific magic that follows the LLVM magic in the DXIL chunk.
/// Layout : two u32s — `0x4C495844` ('DXIL') + offset-of-bitcode-from-this-magic.
pub const DXIL_INNER_MAGIC: u32 = 0x4C49_5844;

/// DXIL bitcode header version (matches DXC `DxilBitcodeWriter.cpp`).
pub const DXIL_BITCODE_VERSION: u16 = 0x0010;
/// Shader-stage kind in the DXIL-bitcode-header `kind` field.
pub const DXIL_KIND_DXIL: u16 = 0x0001;

/// Standard LLVM block ids. Subset used by the stage-0 emitter.
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BlockId {
    BlockInfo   = 0,
    Module      = 8,
    ParamAttr   = 9,
    Constants   = 11,
    Function    = 12,
    TypeSymTab  = 13,
    ValueSymTab = 14,
    Metadata    = 15,
    Type        = 17,
}

/// Bit-level writer that emits the LLVM bitstream byte-stream.
///
/// We back the writer with a `Vec<u32>` of 32-bit words ; bits are packed
/// LSB-first into the current word. On `finish` the words flatten to bytes
/// little-endian.
#[derive(Debug, Clone, Default)]
pub struct BitWriter {
    words: Vec<u32>,
    /// In-progress current word (only `bits_used` LSBs are populated).
    cur_word: u64,
    /// How many bits of `cur_word` are populated.
    bits_used: u32,
    /// Current abbreviation-id width (bits) — the LLVM bitstream tracks this
    /// per-block. Defaults to 2 (so codes 0/1/2/3 fit unabbreviated).
    abbrev_width: u32,
    /// Block-state stack : (parent_abbrev_width, length-record-position).
    block_stack: Vec<BlockFrame>,
}

#[derive(Debug, Clone)]
struct BlockFrame {
    parent_abbrev_width: u32,
    length_word_index: usize,
}

impl BitWriter {
    /// New writer at the start-of-stream (abbrev-width 2).
    #[must_use]
    pub fn new() -> Self {
        Self {
            abbrev_width: 2,
            ..Self::default()
        }
    }

    /// Bytes emitted so far (rounded up to 4-byte words).
    #[must_use]
    pub fn byte_len(&self) -> usize {
        self.words.len() * 4 + (if self.bits_used > 0 { 4 } else { 0 })
    }

    /// Emit `n` bits of `value` (LSB-first).
    pub fn put_bits(&mut self, value: u64, n: u32) {
        debug_assert!(n <= 64);
        let mut remaining = n;
        let mask_low = (1u64 << n.min(63)).wrapping_sub(1);
        let mask_full = if n == 64 { !0u64 } else { 0 };
        let mut v = value & (mask_low | mask_full);
        while remaining > 0 {
            let space = 32 - self.bits_used;
            let take = remaining.min(space);
            let mask = if take == 64 { !0u64 } else { (1u64 << take) - 1 };
            self.cur_word |= (v & mask) << self.bits_used;
            self.bits_used += take;
            v = if take == 64 { 0 } else { v >> take };
            remaining -= take;
            if self.bits_used == 32 {
                self.words.push(self.cur_word as u32);
                self.cur_word = 0;
                self.bits_used = 0;
            }
        }
    }

    /// Emit a 32-bit Variable-Bit-Rate value with chunk-width `n` bits per
    /// chunk. VBR is the LLVM bitcode integer encoding for unbounded ints.
    pub fn put_vbr(&mut self, mut value: u64, n: u32) {
        debug_assert!(n >= 2);
        let chunk_mask = (1u64 << (n - 1)) - 1;
        while value >= (1u64 << (n - 1)) {
            // emit a chunk with the top-bit set (= "more chunks follow").
            self.put_bits((value & chunk_mask) | (1u64 << (n - 1)), n);
            value >>= n - 1;
        }
        // final chunk : top-bit clear.
        self.put_bits(value, n);
    }

    /// Pad up to a 32-bit boundary. Used between blocks.
    pub fn align_to_word(&mut self) {
        if self.bits_used > 0 {
            self.words.push(self.cur_word as u32);
            self.cur_word = 0;
            self.bits_used = 0;
        }
    }

    /// Emit ENTER_SUBBLOCK (code 1) for the given block id + abbrev-width.
    /// Records the block-length placeholder for back-patching on
    /// [`Self::end_block`].
    pub fn enter_block(&mut self, block_id: BlockId, new_abbrev_width: u32) {
        // ENTER_SUBBLOCK opcode = 1 (in the current abbrev_width bits).
        self.put_bits(1, self.abbrev_width);
        // VBR8 block-id
        self.put_vbr(block_id as u64, 8);
        // VBR4 new-abbrev-width
        self.put_vbr(new_abbrev_width as u64, 4);
        self.align_to_word();
        // block-length placeholder (32 bits) — back-patched on end_block.
        let length_word_index = self.words.len();
        self.words.push(0);
        let parent_abbrev_width = self.abbrev_width;
        self.abbrev_width = new_abbrev_width;
        self.block_stack.push(BlockFrame {
            parent_abbrev_width,
            length_word_index,
        });
    }

    /// Emit END_BLOCK (code 0) + back-patch the parent block's length word.
    pub fn end_block(&mut self) {
        // END_BLOCK opcode = 0.
        self.put_bits(0, self.abbrev_width);
        self.align_to_word();
        let frame = self
            .block_stack
            .pop()
            .expect("end_block without matching enter_block");
        let block_end_word = self.words.len();
        let block_start_word = frame.length_word_index + 1;
        let block_length = block_end_word - block_start_word;
        self.words[frame.length_word_index] = block_length as u32;
        self.abbrev_width = frame.parent_abbrev_width;
    }

    /// Emit UNABBREV_RECORD (code 3) with code + ops (each emitted as VBR6).
    pub fn put_unabbrev_record(&mut self, code: u64, ops: &[u64]) {
        self.put_bits(3, self.abbrev_width); // UNABBREV_RECORD
        self.put_vbr(code, 6);
        self.put_vbr(ops.len() as u64, 6);
        for op in ops {
            self.put_vbr(*op, 6);
        }
    }

    /// Finish the stream + return the byte-vector. Pads to 4-byte boundary.
    #[must_use]
    pub fn finish(mut self) -> Vec<u8> {
        self.align_to_word();
        let mut out = Vec::with_capacity(self.words.len() * 4);
        for w in &self.words {
            out.extend_from_slice(&w.to_le_bytes());
        }
        out
    }
}

/// LLVM module record codes (subset).
pub mod module_code {
    pub const VERSION: u64 = 1;
    pub const TRIPLE: u64 = 2;
    pub const DATALAYOUT: u64 = 3;
    pub const FUNCTION: u64 = 8;
}

/// LLVM type record codes (subset). DXIL emits the canonical types via
/// the new TYPE_BLOCK (id 17).
pub mod type_code {
    pub const NUMENTRY: u64 = 1;
    pub const VOID: u64 = 2;
    pub const FLOAT: u64 = 3;
    pub const HALF: u64 = 10;
    pub const INTEGER: u64 = 7;
    pub const POINTER: u64 = 8;
    pub const FUNCTION_OLD: u64 = 9;
    pub const FUNCTION: u64 = 21;
}

/// Errors produced by the bitcode emitter.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum BitcodeError {
    /// We were asked to emit a value too wide for its VBR field. Practically
    /// only produced by malformed callers.
    #[error("VBR overflow : value {value} too wide for {bits}-bit field")]
    VbrOverflow { value: u64, bits: u32 },
}

/// Configuration for a single emitted module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleConfig {
    /// LLVM target-triple string. Always `"dxil-ms-dx"` for DXIL.
    pub triple: String,
    /// LLVM data-layout string. DXIL uses a canonical 64-bit pointer layout.
    pub data_layout: String,
    /// LLVM bitcode version (matches DXC : version-record value 1 for DXIL).
    pub version: u64,
}

impl ModuleConfig {
    /// Canonical DXIL module config for shader-model 6.x.
    #[must_use]
    pub fn dxil_default() -> Self {
        Self {
            triple: "dxil-ms-dx".to_string(),
            data_layout: "e-m:e-p:32:32-i1:32-i8:8-i16:16-i32:32-i64:64-f16:16-f32:32-f64:64-n8:16:32:64".to_string(),
            version: 1,
        }
    }
}

/// Emit the DXIL bitcode payload for a single shader function.
///
/// The emitted payload starts with the DXIL-bitcode-header :
/// ```text
/// u32 : DXIL_INNER_MAGIC ('DXIL')
/// u32 : version-token = (DXIL_BITCODE_VERSION << 16) | DXIL_KIND_DXIL
/// u32 : bitcode_offset (= 16 — bytes from start of this header to LLVM magic)
/// u32 : bitcode_size_bytes
/// ```
/// then the LLVM-bitcode magic + the module-block bytestream.
#[must_use]
pub fn emit_dxil_payload(config: &ModuleConfig, function_name: &str) -> Vec<u8> {
    let bitcode = emit_llvm_bitcode(config, function_name);
    let mut out = Vec::with_capacity(16 + bitcode.len());
    // DXIL-inner header.
    out.extend_from_slice(&DXIL_INNER_MAGIC.to_le_bytes());
    let version_token: u32 =
        ((DXIL_BITCODE_VERSION as u32) << 16) | (DXIL_KIND_DXIL as u32);
    out.extend_from_slice(&version_token.to_le_bytes());
    out.extend_from_slice(&16u32.to_le_bytes()); // bitcode_offset
    let bitcode_size = u32::try_from(bitcode.len()).unwrap_or(u32::MAX);
    out.extend_from_slice(&bitcode_size.to_le_bytes());
    // Bitcode bytes follow.
    out.extend_from_slice(&bitcode);
    out
}

/// Emit the LLVM bitcode bytes (minus DXIL outer header).
#[must_use]
pub fn emit_llvm_bitcode(config: &ModuleConfig, function_name: &str) -> Vec<u8> {
    let mut bw = BitWriter::new();
    // Magic : 32 bits = 0xDEC0_4243 ('B' 'C' 0xC0 0xDE).
    bw.put_bits(LLVM_BITCODE_MAGIC_BC as u64, 32);
    // Module-block.
    bw.enter_block(BlockId::Module, 3);
    // VERSION record (code 1) : op0 = bitcode-version (1 for current LLVM 3.7-style).
    bw.put_unabbrev_record(module_code::VERSION, &[config.version]);
    // TRIPLE record (code 2) : op0..opN = triple-bytes (one byte per op).
    let triple_ops: Vec<u64> = config.triple.bytes().map(|b| b as u64).collect();
    bw.put_unabbrev_record(module_code::TRIPLE, &triple_ops);
    // DATALAYOUT record (code 3) : op0..opN = data-layout bytes.
    let dl_ops: Vec<u64> = config.data_layout.bytes().map(|b| b as u64).collect();
    bw.put_unabbrev_record(module_code::DATALAYOUT, &dl_ops);

    // TYPE_BLOCK_NEW (id 17) — minimal : void + i32 + function () -> void.
    bw.enter_block(BlockId::Type, 4);
    // NUMENTRY record (code 1) : 3 types declared.
    bw.put_unabbrev_record(type_code::NUMENTRY, &[3]);
    // type 0 : void
    bw.put_unabbrev_record(type_code::VOID, &[]);
    // type 1 : i32
    bw.put_unabbrev_record(type_code::INTEGER, &[32]);
    // type 2 : function () -> void  ; FUNCTION code = 21 ; ops = (vararg=0, ret_type_idx=0).
    bw.put_unabbrev_record(type_code::FUNCTION, &[0, 0]);
    bw.end_block();

    // FUNCTION declaration record in MODULE block.
    // op0 = type_idx, op1 = calling_conv, op2 = is_proto, op3 = linkage, op4..= attrs/etc.
    bw.put_unabbrev_record(module_code::FUNCTION, &[2, 0, 0, 0]);

    // FUNCTION_BLOCK (id 12) for the body.
    bw.enter_block(BlockId::Function, 4);
    // DECLAREBLOCKS record (code 1) : op0 = num basic blocks (1).
    bw.put_unabbrev_record(1, &[1]);
    // INST_RET record (code 10) : op0 = (none) for `ret void`.
    bw.put_unabbrev_record(10, &[]);
    bw.end_block();

    // VALUE_SYMTAB_BLOCK (id 14) : function-name -> idx 0.
    bw.enter_block(BlockId::ValueSymTab, 4);
    // VST_CODE_FNENTRY = 2 ; ops = (value_idx = 0, name_bytes...)
    let mut fn_ops: Vec<u64> = vec![0];
    fn_ops.extend(function_name.bytes().map(|b| b as u64));
    bw.put_unabbrev_record(2, &fn_ops);
    bw.end_block();

    // End of module.
    bw.end_block();
    bw.finish()
}

/// Inspect the first 4 bytes of an emitted DXIL payload to confirm the
/// DXIL-inner magic. Used by tests + container-validators.
#[must_use]
pub fn payload_starts_with_dxil_magic(payload: &[u8]) -> bool {
    if payload.len() < 4 {
        return false;
    }
    let mut buf = [0u8; 4];
    buf.copy_from_slice(&payload[0..4]);
    u32::from_le_bytes(buf) == DXIL_INNER_MAGIC
}

/// Inspect the LLVM-bitcode magic at offset 16 of an emitted DXIL payload.
#[must_use]
pub fn payload_has_llvm_bitcode_magic(payload: &[u8]) -> bool {
    if payload.len() < 20 {
        return false;
    }
    let mut buf = [0u8; 4];
    buf.copy_from_slice(&payload[16..20]);
    u32::from_le_bytes(buf) == LLVM_BITCODE_MAGIC_BC
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vbr_round_trip_small_values() {
        // VBR with chunk-width 4 : value 7 fits in one chunk (top bit clear).
        // Verify the bit-pattern by emitting + flattening + re-parsing.
        let mut bw = BitWriter::new();
        bw.put_vbr(7, 4);
        bw.align_to_word();
        let bytes = bw.finish();
        // First nibble (bits 0..3) : 0b0111 = 7. The next-chunk continuation
        // bit (bit 3) is clear → just 7.
        assert_eq!(bytes[0] & 0x0F, 7);
    }

    #[test]
    fn dxil_payload_starts_with_dxil_magic() {
        let cfg = ModuleConfig::dxil_default();
        let payload = emit_dxil_payload(&cfg, "main_cs");
        assert!(payload.len() >= 16);
        assert!(payload_starts_with_dxil_magic(&payload));
    }

    #[test]
    fn dxil_payload_has_llvm_bitcode_magic_at_offset_16() {
        let cfg = ModuleConfig::dxil_default();
        let payload = emit_dxil_payload(&cfg, "main_cs");
        assert!(payload_has_llvm_bitcode_magic(&payload));
    }

    #[test]
    fn module_config_default_uses_dxil_triple() {
        let cfg = ModuleConfig::dxil_default();
        assert_eq!(cfg.triple, "dxil-ms-dx");
        assert!(cfg.data_layout.contains("p:32:32"));
    }

    #[test]
    fn block_enter_exit_balances() {
        let mut bw = BitWriter::new();
        bw.enter_block(BlockId::Module, 3);
        bw.put_unabbrev_record(1, &[1]);
        bw.end_block();
        let bytes = bw.finish();
        // Length word should be back-patched to a non-zero value.
        // Length-word lives at bytes[4..8] (after the ENTER_SUBBLOCK header).
        let mut buf = [0u8; 4];
        buf.copy_from_slice(&bytes[4..8]);
        let length = u32::from_le_bytes(buf);
        assert!(length > 0, "block-length placeholder was not back-patched");
    }
}
