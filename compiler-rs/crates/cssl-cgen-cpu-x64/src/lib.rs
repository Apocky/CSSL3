//! CSSLv3 stage-1 — owned x86-64 backend (ABI / calling-convention layer).
//!
//! § SPEC : `specs/07_CODEGEN.csl § CPU BACKEND` + `specs/14_BACKEND.csl § OWNED x86-64 BACKEND § ABI`.
//!
//! § SCOPE (S7-G3 / T11-D85)
//!   - [`abi`] — `X64Abi` enum + per-ABI register tables + alignment + shadow-space rules.
//!   - [`lower`] — `lower_call` / `lower_return` / `lower_prologue` / `lower_epilogue` lowering passes.
//!
//! § WHAT THIS CRATE DOES
//!
//! This is the layer between the abstract MIR `Call` / `Ret` opcodes and the
//! actual x86-64 register / stack passing convention. The owned backend
//! (per `specs/14_BACKEND.csl`) replaces the stage-0 Cranelift path with
//! in-house instruction selection ; this crate is the first piece — the
//! ABI lowering tables. Subsequent S7 slices plug instruction-selection,
//! register-allocation, and machine-code emission layers atop it.
//!
//! Two ABIs are supported, gated by [`X64Abi`] :
//!
//! ```text
//! ┌──────────────────────┬─────────────────────────────┬─────────────────────────────┐
//! │ aspect               │ System V AMD64              │ Microsoft x64 (MS-x64)      │
//! ├──────────────────────┼─────────────────────────────┼─────────────────────────────┤
//! │ targets              │ Linux + macOS-Intel + BSD   │ Windows-MSVC + Windows-GNU  │
//! │ int  arg-regs        │ rdi rsi rdx rcx r8 r9       │ rcx rdx r8 r9               │
//! │ float arg-regs       │ xmm0..xmm7                  │ xmm0..xmm3                  │
//! │ return int           │ rax                         │ rax                         │
//! │ return float         │ xmm0                        │ xmm0                        │
//! │ caller-saved (int)   │ rax rcx rdx rsi rdi r8..r11 │ rax rcx rdx r8..r11         │
//! │ caller-saved (xmm)   │ xmm0..xmm15                 │ xmm0..xmm5                  │
//! │ callee-saved (int)   │ rbx rbp r12..r15            │ rbx rbp rdi rsi r12..r15    │
//! │ callee-saved (xmm)   │ NONE                        │ xmm6..xmm15                 │
//! │ shadow space         │ NONE                        │ 32 bytes (caller alloc)     │
//! │ red-zone             │ 128 bytes below rsp         │ NONE                        │
//! │ stack align @ call   │ 16 bytes                    │ 16 bytes                    │
//! └──────────────────────┴─────────────────────────────┴─────────────────────────────┘
//! ```
//!
//! Both ABIs require **16-byte stack alignment at the call boundary** (the
//! 8-byte pushed return-address means callee sees `rsp % 16 == 8` at entry ;
//! prologue's `push rbp` re-aligns to 16).
//!
//! § DEFERRED
//!   - **Variadic args** (`...`) : on MS-x64 first 4 are reg ; rest stack.
//!     On System-V, `al` = number of vector args used in the call. G3 REJECTS
//!     variadic call lowering with [`AbiError::VariadicNotSupported`] ; lands
//!     once a variadic-aware MIR op is in flight.
//!   - **Large-struct return via hidden first-arg pointer** : G3 handles
//!     scalar-return only (i32/i64/f32/f64). Struct return rejected with
//!     [`AbiError::StructReturnNotSupported`]. Lands when struct-types in
//!     MIR signatures are stable.
//!   - **Red-zone optimization** : System-V allows 128-byte red-zone below
//!     `rsp` for leaf fns. G3 conservatively reserves stack always ; G2 / G4
//!     can flip the leaf-fn flag to skip prologue/epilogue when no calls + no
//!     stack-frame alloc are present.
//!   - **Frame-pointer omit** : G3 always emits `push rbp ; mov rbp, rsp`.
//!     A future opt slice can omit when `frame_size == 0 ∧ no_calls`.
//!
//! § PRIME-DIRECTIVE
//!   This crate emits ABI-table data and structural lowering decisions ; it
//!   does NOT execute, observe, or surveil. Standard `forbid(unsafe_code)`.

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
#![allow(clippy::module_name_repetitions)]

pub mod abi;
pub mod lower;

pub use abi::{
    AbiError, ArgClass, FloatArgRegs, GpReg, IntArgRegs, ReturnReg, X64Abi, XmmReg,
    CALL_BOUNDARY_ALIGNMENT, MS_X64_SHADOW_SPACE,
};
pub use lower::{
    lower_call, lower_epilogue, lower_prologue, lower_return, AbstractInsn, CallSiteLayout,
    CalleeSavedSlot, FunctionLayout, LoweredCall, LoweredEpilogue, LoweredPrologue, LoweredReturn,
    StackSlot,
};

/// Crate version exposed for scaffold verification.
pub const G3_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod scaffold_tests {
    use super::G3_SCAFFOLD;

    #[test]
    fn scaffold_version_present() {
        assert!(!G3_SCAFFOLD.is_empty());
    }
}
