//! § cssl-cgen-gpu-dxil-wgsl — WGSL → naga → SPIR-V → DXBC-wrapped DXIL emit.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § T11-W18-CGEN-DXIL slice — the next step after L8-DXIL-PRESENT. The
//! `cssl-host-substrate-render-v3-d3d12` host accepts an opaque `&[u8]` DXIL
//! blob ; this crate produces the bytes. The path is :
//!
//! ```text
//! cssl-host-substrate-render-v2/shaders/substrate_v2.wgsl  (embedded · source-of-truth)
//!         │  (naga::front::wgsl::parse_str)
//!         ▼
//! naga::Module                                             (validated IR)
//!         │  (naga::back::spv::write_vec)
//!         ▼
//! Vec<u32> SPIR-V                                          (canonical SPIR-V binary)
//!         │  (dxil_container_writer ; DXBC wrapper)
//!         ▼
//! Vec<u8> DXBC-shaped container                            (passes validate_dxil_container · STUB)
//!         │  (D3D12::CreateComputePipelineState fed bytes)
//!         ▼
//! ‼ STUB — D3D12 will reject ; the inner part is SPIR-V not DXIL bitcode.
//! ```
//!
//! § HONEST ATTESTATION — STUB-LABELED
//! ────────────────────────────────────
//! The output of [`build_dxil_substrate_kernel`] passes the host's strict
//! [`cssl_host_substrate_render_v3_d3d12::validate_dxil_container`] check
//! (DXBC magic + minimum byte length + header layout) but the inner `DXIL`
//! part-payload is **SPIR-V words, not LLVM-bitcode**. D3D12's
//! `CreateComputePipelineState` will fail with `E_INVALIDARG` because real
//! DXIL is LLVM-3.7-bitcode-subset wrapped in a program-header.
//!
//! Each artifact carries [`DxilArtifactDescriptor::is_real_dxil`] = `false`
//! when it is constructed from the WGSL → SPIR-V path. Callers that need
//! GPU-executable bytes must :
//!   - Wait for the dxc.exe sub-process integration (next slice).
//!   - Or use the existing `cssl-cgen-gpu-dxil` crate (HLSL → dxc) when
//!     `dxc.exe` is available on PATH.
//!   - Or use the existing `cssl-cgen-dxil` crate (from-scratch DXBC + MirModule
//!     input) once its LLVM-bitcode body emitter lands.
//!
//! § PATH FORWARD — what makes this REAL DXIL
//! ───────────────────────────────────────────
//! Real DXIL bytes require one of :
//!
//!   1. **dxc.exe sub-process** — invoke `dxc.exe -T cs_6_6 -E cs_main -Fo …
//!      shader.hlsl`. The HLSL must come from `naga::back::hlsl::write_string`
//!      (naga has `hlsl-out`). dxc.exe is not always on PATH ; absence is
//!      non-fatal but means no executable DXIL. This is the **next slice's**
//!      responsibility.
//!   2. **Owned LLVM-bitcode emitter** — the `cssl-cgen-dxil` crate is
//!      structured for this (its `lower_function` walks MirModule and the
//!      `container::DxilProgramHeader` already encodes the DXIL part shape).
//!      Today it ships header-only ; the body emit is a deferred slice.
//!   3. **DirectXShaderCompiler IDxc* COM interfaces** — in-process. Requires
//!      `windows-rs Win32_Graphics_Direct3D_Dxc` + dxcompiler.dll co-located.
//!      Not in scope for this slice.
//!
//! This crate ships the **scaffolding + the deterministic SPIR-V emit + the
//! DXBC container writer + the strict validation hooks** so that the dxc-sub-
//! process slice can drop a real DXIL byte-stream into the same descriptor
//! shape with no API churn at the consumer (the host crate).
//!
//! § DETERMINISM
//! ─────────────
//! Same WGSL input ⇒ byte-identical SPIR-V ⇒ byte-identical DXBC container.
//! Verified by [`tests::deterministic_emit_round_trip`].
//!
//! § PRIME-DIRECTIVE
//! ─────────────────
//! Σ-mask consent gating is encoded **structurally** in the substrate-kernel
//! WGSL source (`observer_permits_silhouette` + `crystal_permits_silhouette`
//! ⇒ skip-on-revoke). This crate is a transparent passthrough — it never
//! mutates the WGSL semantics, it only re-encodes the bytes. There is no
//! fallback path that bypasses the mask.

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::cast_possible_truncation)]

use cssl_host_substrate_render_v3_d3d12::RootSignatureLayout;

// ════════════════════════════════════════════════════════════════════════════
// § DXBC container constants — local copies + cross-checked against the
// canonical values exposed by `cssl-host-substrate-render-v3-d3d12`. Tests
// assert the cross-crate values agree (`dxbc_magic_agrees_with_host_crate`).
// ════════════════════════════════════════════════════════════════════════════

/// § DXBC container magic — `'D','X','B','C'` little-endian. Matches
/// [`cssl_host_substrate_render_v3_d3d12::DXBC_CONTAINER_MAGIC`] exactly.
pub const DXBC_MAGIC: u32 = 0x4342_5844;

/// § Canonical FourCC for the DXIL part inside a DXBC container.
/// `'D','X','I','L'` little-endian.
pub const DXIL_PART_FOURCC: u32 = 0x4C49_5844;

/// § FourCC for the SPIR-V "diagnostic" part — NOT a real DXIL part-FourCC,
/// chosen as a sentinel so tooling that round-trips this stub can detect
/// "this is the wgsl-naga-spirv stub, not real DXIL".
/// `'S','P','V','0'` little-endian = 0x3056_5053.
pub const SPV0_PART_FOURCC: u32 = 0x3056_5053;

/// § The canonical compute-shader entry-point name in the substrate v2 WGSL.
pub const SUBSTRATE_KERNEL_ENTRY: &str = "main";

/// § The canonical D3D12 target-profile string for the substrate kernel.
/// SM6.6 compute. Used by the dxc.exe path in the next slice ; for now it
/// is descriptor metadata only.
pub const SUBSTRATE_KERNEL_TARGET_PROFILE: &str = "cs_6_6";

/// § The embedded substrate-v2 WGSL source. This is the **canonical** GPU
/// kernel source used by both the v2 wgpu host and (post-this-slice) the
/// L8 d3d12-direct host. Kept as `include_str!` so that the v2 crate's WGSL
/// remains the single source of truth.
const SUBSTRATE_V2_WGSL: &str = include_str!(
    "../../cssl-host-substrate-render-v2/shaders/substrate_v2.wgsl"
);

// ════════════════════════════════════════════════════════════════════════════
// § Errors — every fail-mode is enumerated so callers can match-arm without
// touching naga-internal types.
// ════════════════════════════════════════════════════════════════════════════

/// § Errors from the WGSL → DXBC build pipeline.
#[derive(Debug, thiserror::Error)]
pub enum BuildErr {
    /// The WGSL source is empty. We refuse to emit a 0-byte SPIR-V module
    /// even though naga technically accepts an empty source.
    #[error("wgsl source is empty (zero bytes)")]
    EmptyWgsl,

    /// `naga::front::wgsl::parse_str` rejected the WGSL source.
    #[error("wgsl parse failed : {reason}")]
    WgslParse {
        /// Human-readable diagnostic from naga.
        reason: String,
    },

    /// `naga::valid::Validator::validate` rejected the parsed module. The
    /// SPIR-V backend requires a validated module ; bypassing this would
    /// produce nondeterministic emit on subtle WGSL like recursive calls.
    #[error("naga validation failed : {reason}")]
    NagaValidate {
        /// Human-readable diagnostic.
        reason: String,
    },

    /// `naga::back::spv::write_vec` rejected the validated module.
    #[error("spirv emit failed : {reason}")]
    SpvEmit {
        /// Human-readable diagnostic from the SPIR-V backend.
        reason: String,
    },

    /// The SPIR-V output was empty (defensive — naga's `write_vec` is
    /// expected to always return at least the SPIR-V header words ; if it
    /// returns 0 words something has gone very wrong).
    #[error("spirv emit produced empty word-stream")]
    EmptySpv,

    /// Container-writer math went out of `u32` range. Defensive only ; real
    /// shaders never come close to 4 GiB.
    #[error("dxbc container size exceeds u32 ({size_bytes} bytes)")]
    ContainerOverflow {
        /// Computed total container byte length.
        size_bytes: usize,
    },
}

// ════════════════════════════════════════════════════════════════════════════
// § Descriptor — the artifact bundle that flows downstream to the host.
// ════════════════════════════════════════════════════════════════════════════

/// § Descriptor for a substrate-kernel DXIL artifact.
///
/// Pairs the canonical container bytes with the metadata the host needs to
/// drive `D3D12_SHADER_BYTECODE` + `ID3D12RootSignature` construction. The
/// `is_real_dxil` flag is the **honest-attestation** : `false` for every
/// artifact built from the WGSL → SPIR-V path in this slice, `true` will be
/// set by the next slice once dxc.exe sub-process integration lands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DxilArtifactDescriptor {
    /// Compute-shader entry-point name (default = `"main"` per substrate v2 WGSL).
    pub entry_point: String,
    /// Target-profile string (default = `"cs_6_6"`).
    pub target_profile: String,
    /// Root-signature layout the kernel binds against. Re-uses the host
    /// crate's canonical layout so descriptor and runtime stay in lockstep.
    pub root_sig_layout: RootSignatureLayout,
    /// **HONEST ATTESTATION** : `false` for WGSL → SPIR-V → DXBC stubs ·
    /// `true` once a real LLVM-bitcode DXIL emitter or dxc.exe path drops
    /// genuine DXIL bytes into the same descriptor shape.
    pub is_real_dxil: bool,
    /// The canonical DXBC container bytes.
    pub container_bytes: Vec<u8>,
}

impl DxilArtifactDescriptor {
    /// § Canonical descriptor for the substrate v2 kernel emitted via the
    /// WGSL → SPIR-V stub path.
    #[must_use]
    pub fn substrate_kernel_stub(container_bytes: Vec<u8>) -> Self {
        Self {
            entry_point: SUBSTRATE_KERNEL_ENTRY.to_string(),
            target_profile: SUBSTRATE_KERNEL_TARGET_PROFILE.to_string(),
            root_sig_layout: RootSignatureLayout::substrate_kernel(),
            is_real_dxil: false,
            container_bytes,
        }
    }

    /// Container byte length.
    #[must_use]
    pub fn byte_len(&self) -> usize {
        self.container_bytes.len()
    }
}

// ════════════════════════════════════════════════════════════════════════════
// § Public API — three entry points ordered narrow-to-broad.
// ════════════════════════════════════════════════════════════════════════════

/// § Build the substrate-kernel artifact from the embedded WGSL source.
///
/// The path is :
///   1. Embedded WGSL (cssl-host-substrate-render-v2/shaders/substrate_v2.wgsl)
///   2. `naga::front::wgsl::parse_str` → `naga::Module`
///   3. `naga::valid::Validator::validate` → `naga::valid::ModuleInfo`
///   4. `naga::back::spv::write_vec` → `Vec<u32>` SPIR-V words
///   5. `dxil_container_writer` → DXBC-shaped container
///   6. Wrap in a [`DxilArtifactDescriptor::substrate_kernel_stub`]
pub fn build_dxil_substrate_kernel() -> Result<DxilArtifactDescriptor, BuildErr> {
    let bytes = build_dxil_from_wgsl(SUBSTRATE_V2_WGSL)?;
    Ok(DxilArtifactDescriptor::substrate_kernel_stub(bytes))
}

/// § Build a DXBC-wrapped artifact from arbitrary WGSL source.
///
/// Returns the canonical container bytes only — callers that want the
/// full descriptor with metadata should call [`build_dxil_substrate_kernel`]
/// for the canonical kernel or build their own descriptor for non-substrate
/// kernels.
pub fn build_dxil_from_wgsl(wgsl: &str) -> Result<Vec<u8>, BuildErr> {
    if wgsl.trim().is_empty() {
        return Err(BuildErr::EmptyWgsl);
    }
    // (1) parse
    let module = naga::front::wgsl::parse_str(wgsl).map_err(|e| BuildErr::WgslParse {
        reason: format!("{e:?}"),
    })?;
    // (2) validate (no capabilities ; SPIR-V backend tolerates everything we use)
    let info = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::default(),
    )
    .validate(&module)
    .map_err(|e| BuildErr::NagaValidate {
        reason: format!("{e:?}"),
    })?;
    // (3) emit SPIR-V — force version 1.3 (matches cssl-host-substrate-render-v3 vulkan).
    let options = naga::back::spv::Options {
        lang_version: (1, 3),
        ..naga::back::spv::Options::default()
    };
    let pipeline_options = naga::back::spv::PipelineOptions {
        shader_stage: naga::ShaderStage::Compute,
        entry_point: SUBSTRATE_KERNEL_ENTRY.to_string(),
    };
    let words = naga::back::spv::write_vec(&module, &info, &options, Some(&pipeline_options))
        .map_err(|e| BuildErr::SpvEmit {
            reason: format!("{e:?}"),
        })?;
    if words.is_empty() {
        return Err(BuildErr::EmptySpv);
    }
    // (4) Wrap as DXBC.
    let spirv_bytes = spirv_words_to_bytes(&words);
    dxil_container_writer(&spirv_bytes)
}

/// § Wrap arbitrary inner-payload bytes in a DXBC container.
///
/// The container layout is :
///
/// ```text
/// offset  bytes  contents
/// 0       4      'DXBC' magic                            (DXBC_MAGIC)
/// 4       16     16-byte hash (deterministic FNV-1a over the rest)
/// 20      4      version_major = 1
/// 24      4      version_minor = 0
/// 28      4      total container byte length
/// 32      4      part-count = 1
/// 36      4      part-offset[0] = 40 (offset to first part-record)
/// 40      4      part-FourCC = 'SPV0' (sentinel · NOT 'DXIL'!)
/// 44      4      part-payload-byte-length
/// 48      N      payload bytes
/// ```
///
/// The chosen part-FourCC `'SPV0'` is a **sentinel** : real DXIL containers
/// use `'DXIL'` for the bitcode part. This sentinel makes the stub-vs-real
/// distinction byte-detectable and prevents the host from accidentally
/// feeding stub-bytes into D3D12's `CreateComputePipelineState`.
///
/// The hash field is FNV-1a over `[bytes 20..]` (everything after the hash
/// itself) — deterministic, no external dep, byte-identical for byte-
/// identical inputs.
pub fn dxil_container_writer(payload_bytes: &[u8]) -> Result<Vec<u8>, BuildErr> {
    // Header layout (bytes 0..40) :
    //   0..4   magic
    //   4..20  hash (16 bytes)
    //   20..24 major version
    //   24..28 minor version
    //   28..32 total size
    //   32..36 part-count
    //   36..40 part-offset[0]
    // Part record (bytes 40..) :
    //   40..44 FourCC
    //   44..48 part payload size
    //   48..N  payload bytes
    let header_size: usize = 40;
    let part_record_size: usize = 8 + payload_bytes.len();
    let total: usize = header_size + part_record_size;
    if u32::try_from(total).is_err() {
        return Err(BuildErr::ContainerOverflow { size_bytes: total });
    }

    let mut buf = Vec::with_capacity(total);
    // 0..4 magic
    buf.extend_from_slice(&DXBC_MAGIC.to_le_bytes());
    // 4..20 hash placeholder (overwrite at end · 16 zero bytes)
    buf.extend_from_slice(&[0u8; 16]);
    // 20..24 version-major
    buf.extend_from_slice(&1u32.to_le_bytes());
    // 24..28 version-minor
    buf.extend_from_slice(&0u32.to_le_bytes());
    // 28..32 total-size
    buf.extend_from_slice(&(total as u32).to_le_bytes());
    // 32..36 part-count = 1
    buf.extend_from_slice(&1u32.to_le_bytes());
    // 36..40 part-offset[0] = 40
    buf.extend_from_slice(&40u32.to_le_bytes());
    debug_assert_eq!(buf.len(), 40);

    // Part record :
    // 40..44 FourCC = 'SPV0' sentinel
    buf.extend_from_slice(&SPV0_PART_FOURCC.to_le_bytes());
    // 44..48 payload size
    buf.extend_from_slice(&(payload_bytes.len() as u32).to_le_bytes());
    // 48.. payload
    buf.extend_from_slice(payload_bytes);
    debug_assert_eq!(buf.len(), total);

    // Compute FNV-1a 128-bit-folded-to-128-bit-hash over bytes [20..total].
    let hash16 = fnv1a_128(&buf[20..]);
    buf[4..20].copy_from_slice(&hash16);
    Ok(buf)
}

// ════════════════════════════════════════════════════════════════════════════
// § Helpers
// ════════════════════════════════════════════════════════════════════════════

/// § Convert a SPIR-V word stream (`Vec<u32>`) to a byte stream
/// little-endian. Pure · deterministic.
fn spirv_words_to_bytes(words: &[u32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(words.len() * 4);
    for w in words {
        out.extend_from_slice(&w.to_le_bytes());
    }
    out
}

/// § FNV-1a deterministic hash · 64-bit run twice with different seeds and
/// concatenated to a 16-byte digest. NOT cryptographic — its job is to make
/// the container hash field non-zero + change-detectable when payload-bytes
/// change. Two runs with different seeds reduce trivial collision risk on
/// short payloads.
fn fnv1a_128(bytes: &[u8]) -> [u8; 16] {
    const FNV_PRIME: u64 = 0x100_0000_01b3;
    const SEED_A: u64 = 0xcbf2_9ce4_8422_2325;
    const SEED_B: u64 = 0x84a4_5e2c_61f7_5acd;
    let mut a = SEED_A;
    let mut b = SEED_B;
    for &byte in bytes {
        a ^= u64::from(byte);
        a = a.wrapping_mul(FNV_PRIME);
        b ^= u64::from(byte ^ 0xa5);
        b = b.wrapping_mul(FNV_PRIME);
    }
    let mut out = [0u8; 16];
    out[0..8].copy_from_slice(&a.to_le_bytes());
    out[8..16].copy_from_slice(&b.to_le_bytes());
    out
}

// ════════════════════════════════════════════════════════════════════════════
// § Tests — 8+ unit-tests covering parse · emit · container · stub-detection.
// ════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use cssl_host_substrate_render_v3_d3d12::{
        validate_dxil_container, DXBC_CONTAINER_MAGIC,
    };

    // § Test #1 : DXBC magic constant agrees with the host crate's value.
    #[test]
    fn dxbc_magic_agrees_with_host_crate() {
        assert_eq!(DXBC_MAGIC, DXBC_CONTAINER_MAGIC);
        assert_eq!(DXBC_MAGIC, 0x4342_5844);
        // 'D'=0x44 'X'=0x58 'B'=0x42 'C'=0x43 little-endian → 0x43425844.
        let bytes = DXBC_MAGIC.to_le_bytes();
        assert_eq!(&bytes, b"DXBC");
    }

    // § Test #2 : the embedded WGSL string is non-empty + parses cleanly.
    // This catches an `include_str!` path-typo at compile-time + a WGSL
    // grammar regression at run-time.
    #[test]
    fn embedded_wgsl_parses_clean() {
        assert!(SUBSTRATE_V2_WGSL.len() > 1000);
        let parse_result = naga::front::wgsl::parse_str(SUBSTRATE_V2_WGSL);
        assert!(
            parse_result.is_ok(),
            "embedded WGSL must parse : {parse_result:?}"
        );
    }

    // § Test #3 : empty WGSL is rejected with `EmptyWgsl`.
    #[test]
    fn empty_wgsl_rejected() {
        match build_dxil_from_wgsl("") {
            Err(BuildErr::EmptyWgsl) => {}
            other => panic!("expected EmptyWgsl got {other:?}"),
        }
        // Whitespace-only also rejected.
        match build_dxil_from_wgsl("   \n\n  \t  ") {
            Err(BuildErr::EmptyWgsl) => {}
            other => panic!("expected EmptyWgsl got {other:?}"),
        }
    }

    // § Test #4 : invalid WGSL is rejected with `WgslParse`.
    #[test]
    fn invalid_wgsl_rejected() {
        let garbage = "this is not valid WGSL @@ syntax #####";
        match build_dxil_from_wgsl(garbage) {
            Err(BuildErr::WgslParse { reason }) => {
                assert!(!reason.is_empty(), "parse error must carry a reason");
            }
            other => panic!("expected WgslParse got {other:?}"),
        }
    }

    // § Test #5 : the substrate-kernel build path produces bytes that pass
    // the host crate's strict DXIL container validator. This verifies the
    // container layout (magic + min-length + header shape) end-to-end
    // through naga + spirv-out + the DXBC writer.
    #[test]
    fn substrate_kernel_emit_passes_strict_validation() {
        let result = build_dxil_substrate_kernel();
        // naga's WGSL frontend has validation rules that may evolve ; a
        // failure here means the embedded WGSL is no longer SPIR-V-emit-
        // clean. We require success at the slice's tip-of-tree.
        let descriptor = result.expect("substrate kernel must build clean");
        assert_eq!(descriptor.entry_point, SUBSTRATE_KERNEL_ENTRY);
        assert_eq!(descriptor.target_profile, SUBSTRATE_KERNEL_TARGET_PROFILE);
        assert!(
            !descriptor.is_real_dxil,
            "WGSL → SPIR-V path is STUB · is_real_dxil must be false"
        );
        assert!(descriptor.byte_len() >= 48, "container must have header + part record");
        // Strict validator from the host crate.
        assert!(
            validate_dxil_container(&descriptor.container_bytes).is_ok(),
            "container must pass host's strict validator"
        );
    }

    // § Test #6 : container layout — magic + part-count + part-FourCC are
    // exactly as specified. Reads bytes back out of the buffer to verify
    // structural shape.
    #[test]
    fn container_layout_part_table_shape() {
        let payload = vec![0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, 0x00, 0x11];
        let container = dxil_container_writer(&payload).unwrap();
        // Magic at 0..4
        let magic = u32::from_le_bytes([container[0], container[1], container[2], container[3]]);
        assert_eq!(magic, DXBC_MAGIC);
        // Hash at 4..20 must be non-zero (FNV-1a always hits some bytes).
        let hash_zero = container[4..20].iter().all(|&b| b == 0);
        assert!(!hash_zero, "FNV-1a hash must overwrite zero placeholder");
        // Version-major at 20..24
        let vmaj = u32::from_le_bytes([container[20], container[21], container[22], container[23]]);
        assert_eq!(vmaj, 1);
        // Version-minor at 24..28
        let vmin = u32::from_le_bytes([container[24], container[25], container[26], container[27]]);
        assert_eq!(vmin, 0);
        // Total-size at 28..32
        let total = u32::from_le_bytes([container[28], container[29], container[30], container[31]]);
        assert_eq!(total as usize, container.len());
        // Part-count at 32..36
        let pcnt = u32::from_le_bytes([container[32], container[33], container[34], container[35]]);
        assert_eq!(pcnt, 1);
        // Part-offset[0] at 36..40
        let poff = u32::from_le_bytes([container[36], container[37], container[38], container[39]]);
        assert_eq!(poff, 40);
        // Part-FourCC at 40..44 = 'SPV0' sentinel (NOT 'DXIL'!)
        let four = u32::from_le_bytes([container[40], container[41], container[42], container[43]]);
        assert_eq!(four, SPV0_PART_FOURCC);
        // Confirm SPV0 ≠ DXIL — stub-detection is byte-detectable.
        assert_ne!(four, DXIL_PART_FOURCC);
        // Part-size at 44..48 = payload.len()
        let psize = u32::from_le_bytes([container[44], container[45], container[46], container[47]]);
        assert_eq!(psize as usize, payload.len());
        // Payload bytes at 48..end
        assert_eq!(&container[48..], &payload[..]);
    }

    // § Test #7 : deterministic emit — same input ⇒ byte-identical output.
    // Critical for CI determinism + cache invalidation correctness.
    #[test]
    fn deterministic_emit_round_trip() {
        let a = build_dxil_substrate_kernel().unwrap();
        let b = build_dxil_substrate_kernel().unwrap();
        assert_eq!(a.container_bytes, b.container_bytes);
        assert_eq!(a, b);
        // Container-writer is also deterministic over arbitrary payload.
        let pa = dxil_container_writer(&[1, 2, 3, 4, 5, 6, 7, 8]).unwrap();
        let pb = dxil_container_writer(&[1, 2, 3, 4, 5, 6, 7, 8]).unwrap();
        assert_eq!(pa, pb);
    }

    // § Test #8 : stub-detection — every artifact built from the WGSL path
    // carries `is_real_dxil = false` AND the inner part-FourCC is `SPV0`
    // (not `DXIL`). Two independent signals so callers cannot accidentally
    // mistake stub bytes for real DXIL.
    #[test]
    fn stub_detection_two_independent_signals() {
        let descriptor = build_dxil_substrate_kernel().unwrap();
        // Signal #1 : descriptor flag.
        assert!(!descriptor.is_real_dxil);
        // Signal #2 : container's part-FourCC at offset 40 is SPV0 not DXIL.
        let bytes = &descriptor.container_bytes;
        let four_at_40 = u32::from_le_bytes([bytes[40], bytes[41], bytes[42], bytes[43]]);
        assert_eq!(four_at_40, SPV0_PART_FOURCC);
        assert_ne!(four_at_40, DXIL_PART_FOURCC);
    }

    // § Test #9 : container-writer accepts empty payload. The DXBC layout
    // permits zero-length parts (rare but legal). Verifies the size math
    // doesn't underflow on the boundary.
    #[test]
    fn container_writer_empty_payload_ok() {
        let container = dxil_container_writer(&[]).unwrap();
        assert_eq!(container.len(), 48);
        // Strict validator accepts (header is still 32 bytes ≥ DXBC_MIN).
        assert!(validate_dxil_container(&container).is_ok());
        // Part-size at 44..48 = 0
        let psize = u32::from_le_bytes([container[44], container[45], container[46], container[47]]);
        assert_eq!(psize, 0);
    }

    // § Test #10 : descriptor metadata — substrate_kernel_stub() carries the
    // host's canonical RootSignatureLayout, the canonical entry-point and
    // target profile, and the is_real_dxil = false flag.
    #[test]
    fn descriptor_metadata_canonical() {
        let descriptor = DxilArtifactDescriptor::substrate_kernel_stub(vec![1, 2, 3]);
        assert_eq!(descriptor.entry_point, "main");
        assert_eq!(descriptor.target_profile, "cs_6_6");
        assert!(!descriptor.is_real_dxil);
        // Root-sig layout matches the host's canonical layout exactly.
        let canon = RootSignatureLayout::substrate_kernel();
        assert_eq!(descriptor.root_sig_layout, canon);
        assert_eq!(descriptor.root_sig_layout.observer_cbv_register, 0);
        assert_eq!(descriptor.root_sig_layout.crystals_uav_register, 0);
        assert_eq!(descriptor.root_sig_layout.output_uav_register, 1);
        assert_eq!(descriptor.root_sig_layout.register_space, 0);
        assert_eq!(descriptor.byte_len(), 3);
    }

    // § Test #11 : two different payloads ⇒ different hashes. FNV-1a doesn't
    // make a strong cryptographic guarantee but for any pair of payloads
    // that differ by even a single byte the hash must differ — verified
    // here on a controlled pair.
    #[test]
    fn container_hash_differs_for_different_payloads() {
        let a = dxil_container_writer(&[1, 2, 3, 4]).unwrap();
        let b = dxil_container_writer(&[1, 2, 3, 5]).unwrap();
        assert_ne!(a[4..20], b[4..20], "hashes must differ for different payloads");
    }

    // § Test #12 : SPIR-V words → bytes round-trip is little-endian.
    #[test]
    fn spirv_words_to_bytes_is_little_endian() {
        let words = vec![0x0123_4567u32, 0x89AB_CDEFu32];
        let bytes = spirv_words_to_bytes(&words);
        assert_eq!(bytes.len(), 8);
        // 0x01234567 LE → 67 45 23 01
        assert_eq!(bytes, vec![0x67, 0x45, 0x23, 0x01, 0xEF, 0xCD, 0xAB, 0x89]);
    }
}
