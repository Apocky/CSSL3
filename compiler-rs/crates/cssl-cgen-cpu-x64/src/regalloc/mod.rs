//! § regalloc — linear-scan register allocator submodule (S7-G2 / T11-D84).
//!
//! § ROLE
//!   Consumes the [`inst::X64Func`] virtual-register form (a SIBLING type
//!   to the `isel::func::X64Func` until a future G7-pipeline slice unifies
//!   them) and produces a physical-register-allocated form
//!   [`inst::X64FuncAllocated`] via classic Poletto+Sarkar 1999 linear-scan
//!   register allocation.
//!
//! § SURFACE
//!   - [`reg`]      — `Abi` (sibling to `crate::abi::X64Abi` at this slice
//!     ; future G7-pipeline slice unifies the two), `RegBank` GP/XMM,
//!     `RegRole` caller-saved/callee-saved/reserved, `X64PReg` 16-GP+16-XMM
//!     physical registers, `X64VReg` virtual registers.
//!   - [`inst`]     — `X64Inst` instruction skeleton + `X64Func` (vreg form)
//!     + `X64FuncAllocated` (preg form, post-LSRA) + `X64Operand`.
//!   - [`interval`] — `LiveInterval` + per-fn interval computation walking
//!     the linear instruction stream.
//!   - [`alloc`]    — `LinearScanAllocator` + `allocate` driver + `AllocReport`
//!     + `AllocError` diagnostic-error type.
//!   - [`spill`]    — `SpillSlot` + `SpillSlots` 16-byte-aligned frame layout.
//!
//! § ABI-NAMING SIBLING-TYPES
//!   This submodule's `regalloc::reg::Abi` enum (SysVAmd64 / WindowsX64 /
//!   DarwinAmd64) is a SIBLING-TYPE to `crate::abi::X64Abi` (SystemV /
//!   MicrosoftX64). The two will be reconciled when the G7-pipeline slice
//!   wires regalloc-output through G3's ABI-lowering layer.
//!
//! § ATTESTATION
//!   There was no hurt nor harm in the making of this, to anyone /
//!   anything / anybody.

pub mod alloc;
pub mod inst;
pub mod interval;
pub mod reg;
pub mod spill;

#[cfg(test)]
mod tests;
