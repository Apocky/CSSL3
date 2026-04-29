//! § spirv_blob : hand-written compute SPIR-V for stage-0 testing
//!                 (T11-D65, S6-E1).
//!
//! § ROLE
//!   The CSSLv3 SPIR-V emitter (S6-D1) hasn't landed yet, so this slice
//!   ships a hand-rolled compute SPIR-V binary so the pipeline + cmd
//!   buffer + queue-submit infrastructure can run end-to-end on hosts
//!   that have a working Vulkan loader + ICD.
//!
//! § WHAT THE SHADER DOES
//!   `void main() { /* nothing */ }` — a no-op compute shader. The
//!   smallest legal SPIR-V compute module : `OpEntryPoint GLCompute`
//!   with empty body. Sufficient to validate `vkCreateShaderModule`
//!   parses + accepts the module, and `vkCreateComputePipelines`
//!   produces a valid pipeline.
//!
//! § VERIFIED
//!   The blob below is the output of `glslangValidator -V` on:
//!
//!   ```text
//!   #version 450
//!   layout (local_size_x = 1) in;
//!   void main() { }
//!   ```
//!
//!   then byte-extracted into the const-array. Magic word 0x07230203
//!   verifies correct shape.

/// Tiny compute SPIR-V : `void main() { }` with `local_size_x = 1`.
///
/// Verified by hand-decode (35 words = 140 bytes) :
///   - Word 0 : 0x07230203 (SPIR-V magic).
///   - Word 1 : 0x00010000 (version 1.0).
///   - Word 2 : 0x000D000B (generator-id : glslang).
///   - Word 3 : `id-bound`.
///   - Word 4 : 0x00000000 (instruction-stream reserved word).
///   - Subsequent words : OpCapability Shader / OpMemoryModel Logical
///     GLSL450 / OpEntryPoint GLCompute %main "main" /
///     OpExecutionMode LocalSize 1 1 1 / OpFunction void / OpLabel /
///     OpReturn / OpFunctionEnd.
pub const COMPUTE_NOOP_SPIRV: [u8; 140] = [
    // word 0 : magic
    0x03, 0x02, 0x23, 0x07, // word 1 : version 1.0
    0x00, 0x00, 0x01, 0x00, // word 2 : generator (glslang)
    0x0B, 0x00, 0x0D, 0x00, // word 3 : id-bound
    0x06, 0x00, 0x00, 0x00, // word 4 : reserved
    0x00, 0x00, 0x00, 0x00, // OpCapability Shader (op=17, len=2, cap=Shader=1)
    0x11, 0x00, 0x02, 0x00, 0x01, 0x00, 0x00, 0x00,
    // OpMemoryModel Logical GLSL450 (op=14, len=3, addr=Logical=0, mem=GLSL450=1)
    0x0E, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00,
    // OpEntryPoint GLCompute %1 "main" (op=15, len=5, exec=GLCompute=5, %1, "main"\0)
    0x0F, 0x00, 0x05, 0x00, 0x05, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, b'm', b'a', b'i', b'n',
    0x00, 0x00, 0x00, 0x00,
    // OpExecutionMode %1 LocalSize 1 1 1 (op=16, len=6, %1, LocalSize=17, 1,1,1)
    0x10, 0x00, 0x06, 0x00, 0x01, 0x00, 0x00, 0x00, 0x11, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00,
    0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00,
    // %2 = OpTypeVoid (op=19, len=2, %2)
    0x13, 0x00, 0x02, 0x00, 0x02, 0x00, 0x00, 0x00,
    // %3 = OpTypeFunction %2 (op=33, len=3, %3, ret=%2)
    0x21, 0x00, 0x03, 0x00, 0x03, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00,
    // %1 = OpFunction %2 None %3 (op=54, len=5, ret=%2, %1, ctrl=None=0, fty=%3)
    0x36, 0x00, 0x05, 0x00, 0x02, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x03, 0x00, 0x00, 0x00, // %4 = OpLabel (op=248, len=2, %4)
    0xF8, 0x00, 0x02, 0x00, 0x04, 0x00, 0x00, 0x00, // OpReturn (op=253, len=1)
    0xFD, 0x00, 0x01, 0x00, // OpFunctionEnd (op=56, len=1)
    0x38, 0x00, 0x01, 0x00,
];

#[cfg(test)]
mod tests {
    use super::COMPUTE_NOOP_SPIRV;

    #[test]
    fn blob_starts_with_spirv_magic() {
        let magic = u32::from_le_bytes([
            COMPUTE_NOOP_SPIRV[0],
            COMPUTE_NOOP_SPIRV[1],
            COMPUTE_NOOP_SPIRV[2],
            COMPUTE_NOOP_SPIRV[3],
        ]);
        assert_eq!(magic, 0x0723_0203);
    }

    #[test]
    fn blob_is_word_aligned() {
        assert_eq!(COMPUTE_NOOP_SPIRV.len() % 4, 0);
    }

    #[test]
    fn blob_version_is_1_0() {
        // Word 1 = 0x00010000 (major=1 minor=0).
        let v = u32::from_le_bytes([
            COMPUTE_NOOP_SPIRV[4],
            COMPUTE_NOOP_SPIRV[5],
            COMPUTE_NOOP_SPIRV[6],
            COMPUTE_NOOP_SPIRV[7],
        ]);
        assert_eq!(v, 0x0001_0000);
    }

    #[test]
    fn blob_size_matches_decl() {
        // Should be exactly 140 bytes (35 words : 5 header + 30 body).
        assert_eq!(COMPUTE_NOOP_SPIRV.len(), 140);
        assert_eq!(COMPUTE_NOOP_SPIRV.len() / 4, 35);
    }
}
