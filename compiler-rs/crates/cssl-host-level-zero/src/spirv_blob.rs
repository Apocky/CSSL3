//! Minimal compute-shader SPIR-V test fixture.
//!
//! § ROLE
//!   Until S6-D1 lands the real CSSLv3 SPIR-V emitter, the L0 host's smoke
//!   tests need *something* to feed `zeModuleCreate`. This module produces a
//!   hand-rolled, spec-conformant SPIR-V 1.0 binary containing :
//!     - one `OpEntryPoint GLCompute "cssl_e5_smoke_kernel"`
//!     - one workgroup-size `OpExecutionMode LocalSize 1 1 1`
//!     - a no-op `OpReturn` body
//!
//! § SPEC
//!   - SPIR-V 1.0 binary layout :
//!       u32 magic = 0x07230203
//!       u32 version (major<<16 | minor<<8) = 0x00010000
//!       u32 generator = 0
//!       u32 bound = N
//!       u32 schema = 0
//!       <instructions...>
//!   - Each instruction is `(wordcount<<16 | opcode) <words>`.
//!
//! § FUTURE
//!   When S6-D1 lands the body emitter, this module's only role becomes a
//!   regression-fixture for the loader machinery — D1 produces real kernels,
//!   this stays as a smoke-test floor.

/// Canonical entry-point name the test SPIR-V exposes.
pub const MINIMAL_COMPUTE_KERNEL_ENTRY: &str = "cssl_e5_smoke_kernel";

/// Build the minimal compute-kernel SPIR-V binary.
///
/// Returned blob is byte-aligned (`u32 << 4`-aligned ; SPIR-V words are 4 bytes).
/// SPIR-V validators accept this as a well-formed compute-shader-capable module.
#[must_use]
pub fn minimal_compute_kernel_blob() -> Vec<u8> {
    // ─ ID numbering (must be < bound) ─
    // %1 = void type
    // %2 = void()fn type
    // %3 = main function
    // %4 = entry block label
    let bound: u32 = 5;

    // SPIR-V opcodes (canonical encoding — see spec §3.32).
    const OP_CAPABILITY: u32 = 17;
    const OP_MEMORY_MODEL: u32 = 14;
    const OP_ENTRY_POINT: u32 = 15;
    const OP_EXECUTION_MODE: u32 = 16;
    const OP_TYPE_VOID: u32 = 19;
    const OP_TYPE_FUNCTION: u32 = 33;
    const OP_FUNCTION: u32 = 54;
    const OP_FUNCTION_END: u32 = 56;
    const OP_LABEL: u32 = 248;
    const OP_RETURN: u32 = 253;

    const CAP_SHADER: u32 = 1;
    const ADDRESSING_LOGICAL: u32 = 0;
    const MEMMODEL_GLSL450: u32 = 1;
    const EXEC_MODEL_GLCOMPUTE: u32 = 5;
    const EXEC_MODE_LOCAL_SIZE: u32 = 17;

    let mut words: Vec<u32> = Vec::with_capacity(64);

    // Header
    words.push(0x0723_0203); // magic
    words.push(0x0001_0000); // version 1.0
    words.push(0); // generator
    words.push(bound); // id bound
    words.push(0); // schema reserved

    // OpCapability Shader
    words.push(make_op(OP_CAPABILITY, 2));
    words.push(CAP_SHADER);

    // OpMemoryModel Logical GLSL450
    words.push(make_op(OP_MEMORY_MODEL, 3));
    words.push(ADDRESSING_LOGICAL);
    words.push(MEMMODEL_GLSL450);

    // OpEntryPoint GLCompute %3 "cssl_e5_smoke_kernel" (no interface ids)
    let name_words = encode_literal_string(MINIMAL_COMPUTE_KERNEL_ENTRY);
    let ep_word_count: u32 = (3 + name_words.len()) as u32;
    words.push(make_op(OP_ENTRY_POINT, ep_word_count));
    words.push(EXEC_MODEL_GLCOMPUTE);
    words.push(3); // entry-point function id
    words.extend_from_slice(&name_words);

    // OpExecutionMode %3 LocalSize 1 1 1
    words.push(make_op(OP_EXECUTION_MODE, 6));
    words.push(3); // entry-point id
    words.push(EXEC_MODE_LOCAL_SIZE);
    words.push(1);
    words.push(1);
    words.push(1);

    // %1 = OpTypeVoid
    words.push(make_op(OP_TYPE_VOID, 2));
    words.push(1);

    // %2 = OpTypeFunction %1 (void → ())
    words.push(make_op(OP_TYPE_FUNCTION, 3));
    words.push(2); // result id
    words.push(1); // return type = void

    // %3 = OpFunction %1 None %2
    words.push(make_op(OP_FUNCTION, 5));
    words.push(1); // result type = void
    words.push(3); // result id
    words.push(0); // function-control bits = none
    words.push(2); // function-type

    // %4 = OpLabel
    words.push(make_op(OP_LABEL, 2));
    words.push(4);

    // OpReturn
    words.push(make_op(OP_RETURN, 1));

    // OpFunctionEnd
    words.push(make_op(OP_FUNCTION_END, 1));

    // u32 → little-endian u8.
    let mut bytes = Vec::with_capacity(words.len() * 4);
    for w in words {
        bytes.extend_from_slice(&w.to_le_bytes());
    }
    bytes
}

/// Build a `(wordcount << 16) | opcode` SPIR-V instruction prefix.
const fn make_op(opcode: u32, wordcount: u32) -> u32 {
    (wordcount << 16) | (opcode & 0xFFFF)
}

/// Encode a string into SPIR-V "Literal String" words (NUL-terminated, padded
/// to a 4-byte boundary, packed little-endian).
fn encode_literal_string(s: &str) -> Vec<u32> {
    let mut bytes: Vec<u8> = s.as_bytes().to_vec();
    bytes.push(0); // NUL terminator
    while bytes.len() % 4 != 0 {
        bytes.push(0);
    }
    let mut words = Vec::with_capacity(bytes.len() / 4);
    for chunk in bytes.chunks_exact(4) {
        let mut buf = [0u8; 4];
        buf.copy_from_slice(chunk);
        words.push(u32::from_le_bytes(buf));
    }
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blob_starts_with_spirv_magic() {
        let blob = minimal_compute_kernel_blob();
        assert!(blob.len() >= 20, "blob too small to contain header");
        let magic = u32::from_le_bytes([blob[0], blob[1], blob[2], blob[3]]);
        assert_eq!(magic, 0x0723_0203, "SPIR-V magic mismatch");
    }

    #[test]
    fn blob_version_is_1_0() {
        let blob = minimal_compute_kernel_blob();
        let version = u32::from_le_bytes([blob[4], blob[5], blob[6], blob[7]]);
        assert_eq!(version, 0x0001_0000, "expected SPIR-V 1.0");
    }

    #[test]
    fn blob_size_is_word_aligned() {
        let blob = minimal_compute_kernel_blob();
        assert_eq!(
            blob.len() % 4,
            0,
            "blob length must be word-aligned (multiple of 4)"
        );
    }

    #[test]
    fn blob_contains_entry_point_name() {
        let blob = minimal_compute_kernel_blob();
        let name_bytes = MINIMAL_COMPUTE_KERNEL_ENTRY.as_bytes();
        let found = blob.windows(name_bytes.len()).any(|w| w == name_bytes);
        assert!(
            found,
            "blob should embed entry-point name '{MINIMAL_COMPUTE_KERNEL_ENTRY}'",
        );
    }

    #[test]
    fn blob_bound_is_at_least_5() {
        let blob = minimal_compute_kernel_blob();
        let bound = u32::from_le_bytes([blob[12], blob[13], blob[14], blob[15]]);
        assert!(bound >= 5, "bound must accommodate all referenced ids");
    }

    #[test]
    fn make_op_packs_wordcount_high_opcode_low() {
        let packed = make_op(15, 4); // OpEntryPoint, 4 words
        assert_eq!(packed, (4 << 16) | 15);
    }

    #[test]
    fn encode_literal_string_round_trips_short() {
        let words = encode_literal_string("foo");
        // "foo\0" = 4 bytes = 1 word
        assert_eq!(words.len(), 1);
        assert_eq!(words[0], 0x00_6f_6f_66); // little-endian "foo\0"
    }

    #[test]
    fn encode_literal_string_pads_to_word_boundary() {
        let words = encode_literal_string("a");
        // "a\0\0\0" = 1 word
        assert_eq!(words.len(), 1);
    }

    #[test]
    fn encode_literal_string_long_string_correct_words() {
        let words = encode_literal_string("hello"); // 5 bytes + NUL = 6 → pad to 8 → 2 words
        assert_eq!(words.len(), 2);
    }

    #[test]
    fn entry_point_constant_well_formed_identifier() {
        // Entry-point name must be a valid SPIR-V identifier (no embedded NUL).
        assert!(!MINIMAL_COMPUTE_KERNEL_ENTRY.contains('\0'));
        assert!(!MINIMAL_COMPUTE_KERNEL_ENTRY.is_empty());
    }
}
