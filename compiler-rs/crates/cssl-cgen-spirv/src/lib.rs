//! CSSLv3 — from-scratch SPIR-V binary backend (zero external deps).
//!
//! § T11-D267 (W-G1) — `cssl-cgen-spirv` crate.
//!
//! § SPEC : `specs/07_CODEGEN.csl § GPU BACKEND` + `specs/14_BACKEND.csl
//! § OWNED SPIR-V EMITTER` + Khronos SPIR-V Specification (Unified)
//! Revision 1.5 § 2 (Binary Form) + § 3 (Binary Form Operands).
//!
//! § DESIGN
//!   The Khronos SPIR-V binary form is, at its lowest layer, a sequence of
//!   little-endian 32-bit words :
//!
//!   - header (5 words) : magic 0x07230203 + version + generator + bound + 0
//!   - instructions : `word0 = (word_count << 16) | opcode` ;
//!     `word1..N` = operands (result-type, result-id, …)
//!
//!   That's it. There are no implicit structures, no tagged unions, no
//!   alignment quirks. Any reader that can decode little-endian u32 streams
//!   can consume our output ; any tool that targets SPIR-V (Vulkan drivers,
//!   `spirv-val`, `spirv-cross`, `spirv-opt`) accepts what we emit.
//!
//!   We mirror the spec one-to-one :
//!     - `binary.rs` : header + instruction-stream emission.
//!     - `op.rs` : opcode table + per-Op operand layout.
//!     - `lower.rs` : `MirFunc` → `SpirvBinary` driver.
//!
//!   Three shader stages are supported in this slice :
//!     - `ExecutionModel::GLCompute` (compute shaders ; LocalSize x,y,z).
//!     - `ExecutionModel::Vertex` (vertex shaders ; gl_Position output).
//!     - `ExecutionModel::Fragment` (fragment shaders ; OriginUpperLeft).
//!
//!   Resource bindings supported :
//!     - Uniform buffers (`Uniform` storage class, `BufferBlock` decoration).
//!     - Push constants (`PushConstant` storage class).
//!     - Storage buffers (`StorageBuffer` storage class, RW).
//!     - Sampled images (`UniformConstant` storage class).
//!
//! § PROPRIETARY-EVERYTHING THESIS
//!   This crate is the canonical SPIR-V emit path the LoA-v13 GPU stack
//!   ships. The companion `cssl-cgen-gpu-spirv` crate (T11-D34) wraps
//!   `rspirv` and serves as a cross-validator in tests only ; LoA scenes
//!   compiled for production go through THIS path so that no external
//!   crate sits in the source-of-truth chain.
//!
//! § PRIME-DIRECTIVE
//!   This is a renderer-side codegen ; consent + IFC enforcement happens
//!   upstream (cssl-mir `BiometricEgressCheck` + `EnforcesSigmaAtCellTouches`
//!   + `IfcLoweringPass`). Ops reaching `lower::lower_function` are already
//!   consent-clean. No identity claims encoded.

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
// § Style allowances — opcode tables + spec-mirroring constants are
// large-match heavy + carry intentionally-redundant patterns to mirror
// the spec § 3 layout exactly.
#![allow(clippy::match_same_arms)]
#![allow(clippy::similar_names)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::struct_field_names)]
#![allow(clippy::unreadable_literal)]
#![allow(clippy::struct_excessive_bools)]
#![allow(dead_code)]

pub mod binary;
pub mod lower;
pub mod op;

pub use binary::{SpirvBinary, SPIRV_MAGIC, SPIRV_VERSION_1_5};
pub use lower::{lower_function, LowerError, ShaderStage, ShaderTarget};
pub use op::{Decoration, ExecutionModel, Op, StorageClass};

/// Crate-level error type — the wider lowering pipeline produces
/// [`LowerError`] ; binary-emission is infallible (writes to a `Vec<u32>`)
/// so it carries no error type of its own. A unified facade is exposed
/// here for callers who want a single error variant.
#[derive(Debug, thiserror::Error)]
pub enum SpirvCgenError {
    #[error("lowering failed : {0}")]
    Lower(#[from] LowerError),
}
