//! SPIR-V binary format — header + instruction-stream encoding.
//!
//! § REF : Khronos SPIR-V 1.5 § 2 (Binary Form).
//!
//! § HEADER (5 × u32 little-endian)
//!   word 0 : magic       = 0x07230203
//!   word 1 : version     = (major<<16) | (minor<<8)   — we emit 1.5 = 0x00010500
//!   word 2 : generator   = (vendor<<16) | tool        — 0 = unspecified (legal)
//!   word 3 : bound       = max(result-id) + 1         — patched at finalize
//!   word 4 : reserved    = 0
//!
//! § INSTRUCTION
//!   word 0 : (word_count << 16) | opcode_lo16
//!     where word_count includes the opcode-word itself, so
//!     word_count = 1 + operand_word_count.
//!   words 1..N : operand words (each operand is 1+ words ; literal-strings
//!                are nul-terminated UTF-8 packed 4-bytes/word, low-byte first).

use crate::op::Op;

/// Magic number identifying SPIR-V binaries (Khronos § 2.3).
pub const SPIRV_MAGIC: u32 = 0x07230203;

/// SPIR-V 1.5 version word (major=1, minor=5).
pub const SPIRV_VERSION_1_5: u32 = 0x00010500;

/// SPIR-V 1.0 version word — used by tests that target older Vulkan envs.
pub const SPIRV_VERSION_1_0: u32 = 0x00010000;

/// Generator magic — 0 = unspecified per Khronos § 2.3 word 2.
/// Khronos maintains a registry of vendor magics ; we leave ours
/// at 0 to advertise no specific generator (legal).
pub const SPIRV_GENERATOR: u32 = 0;

/// SPIR-V binary container — header words + instruction-stream words.
///
/// The header is patched at [`Self::finalize`] time so that the `bound`
/// word reflects the maximum result-id allocated during emission +1.
#[derive(Debug, Clone, Default)]
pub struct SpirvBinary {
    /// Version word (default = 1.5).
    pub version: u32,
    /// All non-header words : capabilities + extensions + memory-model +
    /// entry-points + execution-modes + debug-names + decorations + types
    /// + globals + functions, in spec § 2.4 (Logical Layout) order.
    /// Callers using [`SpirvBinary::push_op`] are responsible for ordering ;
    /// the [`crate::lower`] driver enforces the canonical layout.
    pub words: Vec<u32>,
    /// Highest result-id allocated so far. `bound = max_id + 1`.
    pub max_id: u32,
}

impl SpirvBinary {
    /// New empty binary targeting SPIR-V 1.5.
    #[must_use]
    pub fn new() -> Self {
        Self { version: SPIRV_VERSION_1_5, words: Vec::new(), max_id: 0 }
    }

    /// Allocate + return a fresh result-id (1-based per Khronos § 2.2).
    pub fn alloc_id(&mut self) -> u32 {
        self.max_id += 1;
        self.max_id
    }

    /// Note that an externally-generated id was used (e.g., from a stable
    /// id-allocator the lowering driver maintains). Adjusts `max_id` so
    /// the header's `bound` covers it.
    pub fn note_id(&mut self, id: u32) {
        if id > self.max_id {
            self.max_id = id;
        }
    }

    /// Append a raw operation. `operands` are the raw operand-words
    /// (NOT including the opcode/word-count header).
    ///
    /// SPIR-V instruction words are encoded as :
    ///   `word0 = (word_count << 16) | opcode_lo16`
    /// where `word_count = 1 + operands.len()`.
    pub fn push_op(&mut self, op: Op, operands: &[u32]) {
        let word_count = (1 + operands.len()) as u32;
        let header = (word_count << 16) | u32::from(op.opcode());
        self.words.push(header);
        self.words.extend_from_slice(operands);
    }

    /// Append an instruction whose operand list contains a literal UTF-8
    /// string. Per Khronos § 2.2.1, literal strings are nul-terminated +
    /// padded to 4-byte boundaries, packed low-byte-first into u32 words.
    pub fn push_op_with_string(&mut self, op: Op, prefix: &[u32], s: &str, suffix: &[u32]) {
        let str_words = encode_literal_string(s);
        let total_operands = prefix.len() + str_words.len() + suffix.len();
        let word_count = (1 + total_operands) as u32;
        let header = (word_count << 16) | u32::from(op.opcode());
        self.words.push(header);
        self.words.extend_from_slice(prefix);
        self.words.extend(str_words);
        self.words.extend_from_slice(suffix);
    }

    /// Finalize : assemble header + instructions into a single u32 stream.
    ///
    /// Header layout per Khronos § 2.3 :
    ///   [magic, version, generator, bound, 0, ...instructions]
    #[must_use]
    pub fn finalize(&self) -> Vec<u32> {
        let bound = self.max_id + 1;
        let mut out = Vec::with_capacity(5 + self.words.len());
        out.push(SPIRV_MAGIC);
        out.push(self.version);
        out.push(SPIRV_GENERATOR);
        out.push(bound);
        out.push(0);
        out.extend_from_slice(&self.words);
        out
    }

    /// Convenience : finalize + serialize to little-endian bytes.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let words = self.finalize();
        let mut bytes = Vec::with_capacity(words.len() * 4);
        for w in words {
            bytes.extend_from_slice(&w.to_le_bytes());
        }
        bytes
    }
}

/// Encode a UTF-8 string as Khronos § 2.2.1 LiteralString : nul-terminated +
/// 4-byte-padded, low-byte-first word packing.
///
/// Examples (from spec § 2.2.1) :
///   "abc"  → bytes `[a, b, c, 0]` → 1 word `0x00636261`
///   "ab"   → bytes `[a, b, 0, 0]` → 1 word `0x00006261`
///   "abcd" → bytes `[a, b, c, d, 0, 0, 0, 0]` → 2 words
#[must_use]
pub fn encode_literal_string(s: &str) -> Vec<u32> {
    let mut bytes: Vec<u8> = s.bytes().collect();
    // Always at least one nul terminator.
    bytes.push(0);
    // Pad to 4-byte boundary.
    while bytes.len() % 4 != 0 {
        bytes.push(0);
    }
    let mut words = Vec::with_capacity(bytes.len() / 4);
    for chunk in bytes.chunks_exact(4) {
        let w = u32::from(chunk[0])
            | (u32::from(chunk[1]) << 8)
            | (u32::from(chunk[2]) << 16)
            | (u32::from(chunk[3]) << 24);
        words.push(w);
    }
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_magic_version_bound() {
        let mut b = SpirvBinary::new();
        let _ = b.alloc_id();
        let _ = b.alloc_id();
        let _ = b.alloc_id();
        let words = b.finalize();
        assert_eq!(words[0], SPIRV_MAGIC);
        assert_eq!(words[1], SPIRV_VERSION_1_5);
        assert_eq!(words[2], SPIRV_GENERATOR);
        assert_eq!(words[3], 4, "bound = max_id+1 = 3+1 = 4");
        assert_eq!(words[4], 0, "reserved word must be 0");
    }

    #[test]
    fn literal_string_three_chars() {
        // "abc" → bytes [a, b, c, 0] = 1 word, 0x00636261
        let w = encode_literal_string("abc");
        assert_eq!(w.len(), 1);
        assert_eq!(w[0], 0x00636261);
    }

    #[test]
    fn literal_string_four_chars_needs_pad_word() {
        // "abcd" → bytes [a, b, c, d, 0, 0, 0, 0] = 2 words.
        let w = encode_literal_string("abcd");
        assert_eq!(w.len(), 2);
        assert_eq!(w[0], 0x64636261);
        assert_eq!(w[1], 0x00000000);
    }

    #[test]
    fn instruction_word_count_encoding() {
        let mut b = SpirvBinary::new();
        // OpMemoryModel takes 2 operands : (AddressingModel, MemoryModel).
        b.push_op(Op::MemoryModel, &[0, 1]);
        // word0 = (3 << 16) | 14 ; opcode 14 = OpMemoryModel.
        let header = b.words[0];
        let word_count = header >> 16;
        let opcode = header & 0xFFFF;
        assert_eq!(word_count, 3);
        assert_eq!(opcode, 14);
        assert_eq!(b.words[1], 0);
        assert_eq!(b.words[2], 1);
    }

    #[test]
    fn bytes_are_little_endian() {
        let mut b = SpirvBinary::new();
        let bytes = b.to_bytes();
        // First 4 bytes = magic 0x07230203 little-endian = [03, 02, 23, 07].
        assert_eq!(&bytes[0..4], &[0x03, 0x02, 0x23, 0x07]);
        // Next 4 bytes = version 1.5 = 0x00010500 LE = [00, 05, 01, 00].
        assert_eq!(&bytes[4..8], &[0x00, 0x05, 0x01, 0x00]);
        // Suppress unused-mut warning when no ops pushed.
        let _ = &mut b;
    }
}
