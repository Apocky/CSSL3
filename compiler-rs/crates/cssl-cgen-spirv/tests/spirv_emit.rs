//! Integration tests for `cssl-cgen-spirv` — verify byte-for-byte
//! correctness of the SPIR-V binary against Khronos § 2 + § 3.
//!
//! § COVERAGE
//!   1. `header_bytes_canonical`     — magic + version + bound + reserved.
//!   2. `type_table_dedup`           — repeated type lookups reuse ids.
//!   3. `entry_point_records_name`   — OpEntryPoint instr carries the entry name.
//!   4. `simple_compute_shader`      — full module emission for compute stage.
//!   5. `vertex_shader_emits_position`— gl_Position output declared.
//!   6. `fragment_shader_origin_upper_left` — Fragment ExecutionMode set.
//!   7. `compute_with_uniform_and_push_constant` — multi-binding emission.
//!   8. `storage_buffer_runtime_array_block`     — SSBO trailing array.

use cssl_cgen_spirv::binary::{SPIRV_MAGIC, SPIRV_VERSION_1_5};
use cssl_cgen_spirv::lower::{lower_function, ShaderTarget};
use cssl_cgen_spirv::op::{Capability, ExecutionModel, Op};
use cssl_mir::func::MirFunc;

/// Find the first instruction with the given opcode and return the slice
/// of words covering it (header + operands).
fn find_op<'a>(words: &'a [u32], opcode: u16) -> Option<&'a [u32]> {
    let mut i = 5; // skip header
    while i < words.len() {
        let header = words[i];
        let wc = (header >> 16) as usize;
        let oc = (header & 0xFFFF) as u16;
        if wc == 0 { return None; }
        if oc == opcode {
            return Some(&words[i..i + wc]);
        }
        i += wc;
    }
    None
}

fn count_op(words: &[u32], opcode: u16) -> usize {
    let mut i = 5usize;
    let mut n = 0usize;
    while i < words.len() {
        let header = words[i];
        let wc = (header >> 16) as usize;
        let oc = (header & 0xFFFF) as u16;
        if wc == 0 { break; }
        if oc == opcode { n += 1; }
        i += wc;
    }
    n
}

#[test]
fn test_1_header_bytes_canonical() {
    let f = MirFunc::new("main", vec![], vec![]);
    let bin = lower_function(&f, &ShaderTarget::compute("main", (1, 1, 1))).unwrap();
    let bytes = bin.to_bytes();
    // Magic 0x07230203 little-endian.
    assert_eq!(&bytes[0..4], &[0x03, 0x02, 0x23, 0x07]);
    let words = bin.finalize();
    assert_eq!(words[0], SPIRV_MAGIC);
    assert_eq!(words[1], SPIRV_VERSION_1_5);
    assert_eq!(words[2], 0, "generator unspecified");
    assert!(words[3] > 1, "bound must exceed initial id space");
    assert_eq!(words[4], 0, "reserved word zero");
}

#[test]
fn test_2_type_table_dedup() {
    // Two compute lowerings produce the same set of types ; emitting twice
    // through a single SpirvBinary via the type-cache shows id reuse.
    // (We exercise this indirectly by counting OpTypeVoid : exactly 1 even
    // though every fn-type references it.)
    let f = MirFunc::new("main", vec![], vec![]);
    let bin = lower_function(&f, &ShaderTarget::compute("main", (1, 1, 1))).unwrap();
    let words = bin.finalize();
    assert_eq!(count_op(&words, Op::TypeVoid.opcode()), 1, "void emitted once");
    assert_eq!(count_op(&words, Op::TypeFunction.opcode()), 1, "fn-type emitted once");
}

#[test]
fn test_3_entry_point_records_name() {
    let f = MirFunc::new("kernel_main", vec![], vec![]);
    let bin = lower_function(&f, &ShaderTarget::compute("kernel_main", (8, 8, 1))).unwrap();
    let words = bin.finalize();
    let entry = find_op(&words, Op::EntryPoint.opcode()).expect("OpEntryPoint present");
    // Operand 1 = exec model = GLCompute (5).
    assert_eq!(entry[1], ExecutionModel::GLCompute.as_u32());
    // Operand 3+ = the name as packed UTF-8 nul-terminated. Decode 1st word.
    let name_w0 = entry[3];
    let bytes: [u8; 4] = name_w0.to_le_bytes();
    assert_eq!(bytes[0], b'k');
    assert_eq!(bytes[1], b'e');
    assert_eq!(bytes[2], b'r');
    assert_eq!(bytes[3], b'n');
}

#[test]
fn test_4_simple_compute_shader() {
    let f = MirFunc::new("main", vec![], vec![]);
    let target = ShaderTarget::compute("main", (8, 8, 1));
    let bin = lower_function(&f, &target).unwrap();
    let words = bin.finalize();
    // First non-header op is OpCapability Shader.
    let cap = find_op(&words, Op::Capability.opcode()).expect("OpCapability");
    assert_eq!(cap[1], Capability::Shader.as_u32());
    // OpMemoryModel must be present.
    assert!(find_op(&words, Op::MemoryModel.opcode()).is_some());
    // OpExecutionMode LocalSize 8 8 1.
    let em = find_op(&words, Op::ExecutionMode.opcode()).expect("OpExecutionMode");
    assert_eq!(em[2], cssl_cgen_spirv::op::ExecutionMode::LocalSize as u32);
    assert_eq!(em[3], 8);
    assert_eq!(em[4], 8);
    assert_eq!(em[5], 1);
    // OpFunction + OpLabel + OpReturn + OpFunctionEnd present.
    assert!(find_op(&words, Op::Function.opcode()).is_some());
    assert!(find_op(&words, Op::Label.opcode()).is_some());
    assert!(find_op(&words, Op::Return.opcode()).is_some());
    assert!(find_op(&words, Op::FunctionEnd.opcode()).is_some());
}

#[test]
fn test_5_vertex_shader_emits_position() {
    let f = MirFunc::new("vsmain", vec![], vec![]);
    let target = ShaderTarget::vertex("vsmain");
    let bin = lower_function(&f, &target).unwrap();
    let words = bin.finalize();
    let entry = find_op(&words, Op::EntryPoint.opcode()).expect("OpEntryPoint");
    assert_eq!(entry[1], ExecutionModel::Vertex.as_u32());
    // Decoration Builtin Position = decoration 11, builtin 0.
    let mut found_position = false;
    let mut i = 5usize;
    while i < words.len() {
        let header = words[i];
        let wc = (header >> 16) as usize;
        let oc = (header & 0xFFFF) as u16;
        if wc == 0 { break; }
        if oc == Op::Decorate.opcode() && wc >= 4 {
            let deco = words[i + 2];
            if deco == cssl_cgen_spirv::op::Decoration::Builtin.as_u32() && words[i + 3] == 0 {
                found_position = true;
                break;
            }
        }
        i += wc;
    }
    assert!(found_position, "vertex shader must declare Builtin Position");
}

#[test]
fn test_6_fragment_shader_origin_upper_left() {
    let f = MirFunc::new("fsmain", vec![], vec![]);
    let target = ShaderTarget::fragment("fsmain");
    let bin = lower_function(&f, &target).unwrap();
    let words = bin.finalize();
    let entry = find_op(&words, Op::EntryPoint.opcode()).unwrap();
    assert_eq!(entry[1], ExecutionModel::Fragment.as_u32());
    let em = find_op(&words, Op::ExecutionMode.opcode()).expect("OpExecutionMode");
    assert_eq!(em[2], cssl_cgen_spirv::op::ExecutionMode::OriginUpperLeft as u32);
}

#[test]
fn test_7_compute_with_uniform_and_push_constant() {
    let f = MirFunc::new("main", vec![], vec![]);
    let target = ShaderTarget::compute("main", (1, 1, 1))
        .with_uniform()
        .with_push_constant();
    let bin = lower_function(&f, &target).unwrap();
    let words = bin.finalize();
    // 3 OpVariable expected : GlobalInvocationId + uniform + push_constant.
    assert_eq!(count_op(&words, Op::Variable.opcode()), 3);
    // Block decoration must appear at least twice (uniform + push-const).
    let mut block_count = 0usize;
    let mut i = 5usize;
    while i < words.len() {
        let header = words[i];
        let wc = (header >> 16) as usize;
        let oc = (header & 0xFFFF) as u16;
        if wc == 0 { break; }
        if oc == Op::Decorate.opcode() && wc >= 3
            && words[i + 2] == cssl_cgen_spirv::op::Decoration::Block.as_u32() {
            block_count += 1;
        }
        i += wc;
    }
    assert!(block_count >= 2, "uniform + push-const both decorated Block, got {block_count}");
}

#[test]
fn test_8_storage_buffer_runtime_array_block() {
    let f = MirFunc::new("main", vec![], vec![]);
    let target = ShaderTarget::compute("main", (16, 1, 1))
        .with_storage_buffer()
        .with_sampled_image();
    let bin = lower_function(&f, &target).unwrap();
    let words = bin.finalize();
    // OpTypeRuntimeArray must appear (the SSBO's trailing array shape).
    assert!(
        find_op(&words, Op::TypeRuntimeArray.opcode()).is_some(),
        "OpTypeRuntimeArray expected for storage-buffer"
    );
    // OpTypeSampledImage for the sampled-image binding.
    assert!(
        find_op(&words, Op::TypeSampledImage.opcode()).is_some(),
        "OpTypeSampledImage expected"
    );
}
