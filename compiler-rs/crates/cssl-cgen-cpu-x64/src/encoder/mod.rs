//! § encoder — machine-code byte encoder submodule (S7-G4 / T11-D86).
//!
//! § ROLE
//!   Consumes a post-regalloc [`X64Inst`] (a SIBLING type to the isel +
//!   regalloc surfaces until a future G7-pipeline slice unifies them) and
//!   emits canonical x86-64 byte sequences. REX prefix synthesis (Intel
//!   SDM Vol 2 §2.1) ; ModR/M + SIB packing (§2.2) ; short / long branch
//!   encoding ; SSE2 scalar prefix discipline (0x66 / 0xF2 / 0xF3).
//!
//! § SURFACE
//!   - [`reg`]    — `Gpr` (16 64-bit GP registers) + `Xmm` (16 SSE registers)
//!     + `OperandSize` (B8 / B16 / B32 / B64).
//!   - [`mem`]    — `MemOperand` addressing model + `Scale` (X1 / X2 / X4 / X8).
//!   - [`inst`]   — `X64Inst` post-regalloc instruction surface + `BranchTarget`
//!     + `Cond` (16 condition codes : E / NE / L / GE / LE / G / B / AE /
//!     BE / A / S / NS / O / NO / P / NP).
//!   - [`modrm`]  — `make_modrm` + `make_rex_optional` + `make_rex_forced` +
//!     `make_sib` + `emit_disp` + `lower_mem_operand` + `DispKind` + `MemEmission`.
//!   - [`encode`] — `encode_inst(&X64Inst) -> Vec<u8>` + `encode_into(&X64Inst, &mut Vec<u8>)`.
//!
//! § INDEPENDENT SURFACE
//!   Per T11-D86 (S7-G4 slice handoff), this submodule's `X64Inst` is
//!   intentionally a SIBLING-TYPE to the isel + regalloc surfaces. A
//!   future G7-pipeline slice will provide a thin `into_emit()` adapter
//!   from regalloc-output to encoder-input.
//!
//! § ATTESTATION
//!   There was no hurt nor harm in the making of this, to anyone /
//!   anything / anybody.

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

/// Encoder submodule version sentinel. Mirrors the G4 slice's
/// `X64_ENCODER_VERSION` const so tests inside the `tests` submodule
/// keep the same scaffold-version assertion shape.
pub const X64_ENCODER_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests;
