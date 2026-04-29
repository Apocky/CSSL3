//! CSSLv3 stage1+ — owned x86-64 instruction encoder.
//!
//! § SPEC : `specs/14_BACKEND.csl` § OWNED x86-64 BACKEND phase 4 (emit) +
//!          `specs/07_CODEGEN.csl` § CPU BACKEND — stage1+ (owned).
//!
//! § SCOPE (T11-D86 / S7-G4 — this commit)
//!   - [`reg::Gpr`]  / [`reg::Xmm`] / [`reg::OperandSize`] — register + width model.
//!   - [`mem::MemOperand`] / [`mem::Scale`] — memory addressing model.
//!   - [`inst::X64Inst`] — canonical post-regalloc instruction surface.
//!   - [`encode::encode_inst`] / [`encode::encode_into`] — `X64Inst` → bytes.
//!   - REX prefix synthesis (§§ Intel SDM Vol 2 §2.1) ; ModR/M + SIB packing (§ 2.2) ;
//!     short / long branch encoding ; SSE2 scalar prefix discipline (0x66 / 0xF2 / 0xF3).
//!
//! § COVERAGE
//!   integer  : Mov / Add / Sub / Mul / IMul / IDiv / Cmp / Lea / Push / Pop / Load / Store
//!   control  : Jmp / Jcc / Call / Ret
//!   SSE2     : Movss / Movsd / Addss / Addsd / Subss / Subsd / Mulss / Mulsd / Divss / Divsd /
//!              `UCOMIsd` / `CVTSI2sd` / `CVTSD2si`
//!
//! § INDEPENDENCE
//!   This crate carries its own X64Inst surface so that G4 can land independently of
//!   G1 (instruction-selection) / G2 (regalloc) / G3 (ABI-lower). When siblings land,
//!   they may :
//!     (a) consume the surface here as-is, or
//!     (b) define their own `X64Inst` in `cssl-cgen-cpu-isel` / `-regalloc` and provide
//!         a thin `into_emit()` that maps to this crate's enum.
//!
//! § TESTING
//!   Per-instruction byte-equality tests assert against known-good encodings cross-checked
//!   on godbolt + Intel SDM tables.
//!
//! § STAGE-2 DEFERRED
//!   - Memory-form arithmetic (e.g. `add [rax + 8], rcx`) — ALU-RM/MR.
//!   - Memory-form SSE2 (e.g. `movsd xmm0, [rdi]`).
//!   - Shifts / rotates / bit-test ops (Sal/Sar/Shl/Shr/Rol/Ror/Test/Bt/Bsf/Bsr).
//!   - AVX/AVX2 VEX prefix encoding (`Vmovss/Vfmadd231sd`/...).
//!   - AVX-512 EVEX prefix.
//!   - Atomics (`lock` prefix + cmpxchg / xadd).
//!   - Symbolic relocations (G5 — linker will own this).

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
#![allow(clippy::module_name_repetitions)]

pub mod encode;
pub mod inst;
pub mod mem;
pub mod modrm;
pub mod reg;

pub use encode::{encode_inst, encode_into};
pub use inst::{BranchTarget, Cond, X64Inst};
pub use mem::{MemOperand, Scale};
pub use modrm::{
    emit_disp, lower_mem_operand, make_modrm, make_rex_forced, make_rex_optional, make_sib,
    DispKind, MemEmission,
};
pub use reg::{Gpr, OperandSize, Xmm};

/// Crate version exposed for scaffold verification.
pub const X64_ENCODER_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests;
