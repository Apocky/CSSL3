//! CSSLv3 stage0 — from-scratch DXIL bytecode emitter.
//!
//! § SPEC : `specs/07_CODEGEN.csl` § GPU BACKEND — DXIL native path
//!         + `specs/14_BACKEND.csl` § OWNED EMITTER doctrine.
//!
//! § DISTINCTION FROM `cssl-cgen-gpu-dxil`
//!   `cssl-cgen-gpu-dxil` (T11-D73, S6-D2) emits HLSL text + shells out to
//!   `dxc.exe` to produce DXIL bytes — convenient when DXC is on PATH but
//!   pulls in an external toolchain dependency. This crate authors DXIL
//!   FROM-SCRATCH (DXBC framing + LLVM-3.7 bitcode) with **zero external
//!   dependencies** beyond the workspace stdlib + `cssl-mir`. That matches
//!   the LoA-v13 GPU-substrate-of-record requirement of an offline
//!   shader-compile pipeline that runs on a clean Windows host with no
//!   DirectXShaderCompiler install.
//!
//! § SCOPE (T11-D268 / W-G2)
//!   - [`container`] : DXBC container framing — magic `'DXBC'`, chunk
//!     table (DXIL / SHEX / ISG1 / OSG1 / RTS0 / RDAT), 4-byte alignment
//!     between parts, container-size header back-patch.
//!   - [`bitcode`]   : LLVM-3.7 bitstream emission — `BitWriter` primitives
//!     + module / type / function blocks + DXIL-inner header that pre-fixes
//!     the LLVM magic with a `'DXIL'` versioned header per
//!     `DxilBitcodeWriter.cpp`.
//!   - [`lower`]     : `MirModule` → DXIL container driver — entry-point
//!     resolution + part-ordering + `compute|vertex|pixel` stage classification.
//!
//! § DEFERRED (W-G2-α follow-up slice)
//!   - Real per-MirOp → LLVM-bitcode-instruction lowering (currently emits
//!     a `ret void` stub body).
//!   - DXIL-validator round-trip pass (DXIL.dll signing). The container
//!     emits a zero-hash header at stage-0 ; D3D12's debug layer accepts
//!     zero-hash containers when the
//!     `D3D12_FEATURE_DATA_SHADER_CACHE.SkipShaderHash` driver-policy is on.
//!   - Mesh / amplification / RT-library / work-graph stages — framing exists
//!     in [`lower::ShaderStage`] but body lowering is stage-0-stubbed.
//!   - Multi-entry-point library targets (`lib_6_x`).
//!   - Resource-binding emission via `RDAT` chunk.

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
// § Style allowances — the bit-stream + container modules are byte-exact
// machinery + favour explicit-shape over Option combinators for diagnostics.
#![allow(clippy::match_same_arms)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::redundant_field_names)]
#![allow(clippy::similar_names)]

pub mod bitcode;
pub mod container;
pub mod lower;

pub use bitcode::{
    emit_dxil_payload, emit_llvm_bitcode, payload_has_llvm_bitcode_magic,
    payload_starts_with_dxil_magic, BitWriter, BitcodeError, BlockId, ModuleConfig,
    DXIL_BITCODE_VERSION, DXIL_INNER_MAGIC, DXIL_KIND_DXIL, LLVM_BITCODE_MAGIC,
    LLVM_BITCODE_MAGIC_BC,
};
pub use container::{
    build_empty_isg1, build_empty_osg1, build_minimal_root_signature, build_shex_chunk, fourcc,
    part_tag, ContainerError, DxbcContainer, DxbcPart, ParsedDxbcHeader, CONTAINER_VERSION_MAJOR,
    CONTAINER_VERSION_MINOR, DXBC_HEADER_SIZE, DXBC_MAGIC, PART_HEADER_SIZE,
};
pub use lower::{
    lower_to_dxil, DxilArtifact, DxilLowerConfig, DxilLowerError, ShaderModel, ShaderStage,
};

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

// ─────────────────────────────────────────────────────────────────
// § INTEGRATION TESTS
//   These exercise the full MirModule → DxbcContainer pipeline + verify
//   byte-exact framing properties the D3D12 runtime + DXIL.dll validator
//   rely on. The `lower::tests` module covers the lower-driver per-stage
//   surface ; this cluster covers cross-module composition + magic-byte +
//   round-trip parsing.
// ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod integration_tests {
    use super::*;
    use cssl_mir::{IntWidth, MirFunc, MirModule, MirOp, MirType, ValueId};

    fn make_compute_module(entry: &str) -> MirModule {
        let mut m = MirModule::new();
        let mut f = MirFunc::new(entry, vec![], vec![]);
        f.push_op(
            MirOp::std("arith.constant")
                .with_result(ValueId(0), MirType::Int(IntWidth::I32))
                .with_attribute("value", "0"),
        );
        f.push_op(MirOp::std("func.return"));
        m.push_func(f);
        m
    }

    /// Test 1 : DXBC header parses cleanly out of the emitted container.
    #[test]
    fn dxbc_header_round_trips_through_parser() {
        let m = make_compute_module("main_cs");
        let cfg = DxilLowerConfig::compute_default("main_cs");
        let art = lower_to_dxil(&m, &cfg).unwrap();
        let header = ParsedDxbcHeader::parse(&art.container_bytes).unwrap();
        assert_eq!(header.magic, DXBC_MAGIC);
        assert_eq!(header.version_major, CONTAINER_VERSION_MAJOR);
        assert_eq!(header.version_minor, CONTAINER_VERSION_MINOR);
        assert_eq!(header.container_size as usize, art.container_bytes.len());
        // 5 parts : RTS0, ISG1, OSG1, SHEX, DXIL
        assert_eq!(header.part_count, 5);
    }

    /// Test 2 : DXIL chunk payload contains LLVM-bitcode magic.
    #[test]
    fn dxil_chunk_contains_llvm_bitcode_magic() {
        let m = make_compute_module("main_cs");
        let cfg = DxilLowerConfig::compute_default("main_cs");
        let art = lower_to_dxil(&m, &cfg).unwrap();
        // Find the DXIL part by scanning the container bytes for the 4cc.
        let bytes = &art.container_bytes;
        let mut dxil_off: Option<usize> = None;
        for i in DXBC_HEADER_SIZE..(bytes.len().saturating_sub(8)) {
            if &bytes[i..i + 4] == b"DXIL" {
                dxil_off = Some(i);
                break;
            }
        }
        let dxil_off = dxil_off.expect("DXIL part 4cc not found in container");
        // Part header is 8 bytes ; the payload follows. The DXIL-inner magic
        // sits at the start of the payload.
        let payload_start = dxil_off + 8;
        assert!(
            payload_starts_with_dxil_magic(&bytes[payload_start..]),
            "DXIL payload doesn't start with DXIL inner magic"
        );
        assert!(
            payload_has_llvm_bitcode_magic(&bytes[payload_start..]),
            "DXIL payload missing LLVM bitcode magic at offset 16"
        );
    }

    /// Test 3 : compute-shader profile string is well-formed.
    #[test]
    fn compute_shader_profile_string_is_canonical() {
        let m = make_compute_module("kernel_main");
        let cfg = DxilLowerConfig::compute_default("kernel_main");
        let art = lower_to_dxil(&m, &cfg).unwrap();
        assert_eq!(art.profile, "cs_6_6");
    }

    /// Test 4 : vertex shader at SM 6.0 produces the SM 6.0 profile string.
    #[test]
    fn vertex_shader_at_sm60_uses_correct_profile() {
        let m = make_compute_module("vs_main");
        let mut cfg = DxilLowerConfig::vertex_default("vs_main");
        cfg.shader_model = ShaderModel::SM_6_0;
        let art = lower_to_dxil(&m, &cfg).unwrap();
        assert_eq!(art.profile, "vs_6_0");
    }

    /// Test 5 : pixel shader includes both ISG1 + OSG1 for the IA + RT
    /// signature surface (PSO creation requires both even when empty).
    #[test]
    fn pixel_shader_emits_both_input_and_output_signatures() {
        let m = make_compute_module("ps_main");
        let cfg = DxilLowerConfig::pixel_default("ps_main");
        let art = lower_to_dxil(&m, &cfg).unwrap();
        let bytes = &art.container_bytes;
        let mut isg1_found = false;
        let mut osg1_found = false;
        for w in bytes.windows(4) {
            if w == b"ISG1" {
                isg1_found = true;
            }
            if w == b"OSG1" {
                osg1_found = true;
            }
        }
        assert!(isg1_found, "ISG1 not present in pixel-shader container");
        assert!(osg1_found, "OSG1 not present in pixel-shader container");
    }

    /// Test 6 : embedded root-signature blob has canonical version-1.1
    /// header bytes when located in the container.
    #[test]
    fn embedded_root_signature_has_canonical_version_token() {
        let m = make_compute_module("main_cs");
        let cfg = DxilLowerConfig::compute_default("main_cs");
        let art = lower_to_dxil(&m, &cfg).unwrap();
        let bytes = &art.container_bytes;
        // Find RTS0 4cc.
        let mut rts0_off: Option<usize> = None;
        for i in DXBC_HEADER_SIZE..(bytes.len().saturating_sub(8)) {
            if &bytes[i..i + 4] == b"RTS0" {
                rts0_off = Some(i);
                break;
            }
        }
        let rts0_off = rts0_off.expect("RTS0 part not found");
        // Part header is 8 bytes ; payload follows. First u32 is the version.
        let payload_start = rts0_off + 8;
        assert!(payload_start + 4 <= bytes.len());
        let mut buf = [0u8; 4];
        buf.copy_from_slice(&bytes[payload_start..payload_start + 4]);
        let version = u32::from_le_bytes(buf);
        assert_eq!(version, 2, "RTS0 should encode v1.1 (value 2)");
    }

    /// Test 7 : full pipeline byte-exactness — same MIR + same config
    /// produces byte-identical container (deterministic emission gate).
    #[test]
    fn lowering_is_deterministic_across_runs() {
        let m1 = make_compute_module("main_cs");
        let m2 = make_compute_module("main_cs");
        let cfg = DxilLowerConfig::compute_default("main_cs");
        let a1 = lower_to_dxil(&m1, &cfg).unwrap();
        let a2 = lower_to_dxil(&m2, &cfg).unwrap();
        assert_eq!(
            a1.container_bytes, a2.container_bytes,
            "DXIL emission is not deterministic across runs"
        );
    }
}
