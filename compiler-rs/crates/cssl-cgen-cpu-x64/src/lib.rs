//! CSSLv3 stage1+ — Native x86-64 CPU codegen (foundation slice S7-G1).
//!
//! § SPEC
//!   - `specs/14_BACKEND.csl` § OWNED x86-64 BACKEND
//!   - `specs/07_CODEGEN.csl` § CPU BACKEND — stage1+
//!
//! § ROLE
//!   This crate is the **owned** x86-64 backend. It replaces (or augments) the
//!   `cssl-cgen-cpu-cranelift` stage-0 throwaway per `specs/14 § OWNED x86-64
//!   BACKEND`. CSSLv3 takes ownership of the full code-gen pipeline :
//!     1. instruction-selection tables (this slice : S7-G1)
//!     2. register-allocator                                 — sibling slice S7-G2
//!     3. ABI / calling-convention lowering                  — sibling slice S7-G3
//!     4. machine-code encoder                               — sibling slice S7-G4
//!     5. object-file emitter (ELF / COFF / Mach-O)          — sibling slice S7-G5
//!
//!   This slice is the FOUNDATION : MIR → virtual-register-based [`X64Func`].
//!   Sibling slices consume the [`X64Func`] surface as their input ; once all
//!   five land, the cranelift dependency drops.
//!
//! § THE D5 MARKER FANOUT-CONTRACT
//!   Like the D-axis GPU emitters (D1 SPIR-V / D2 DXIL / D3 MSL / D4 WGSL),
//!   this CPU backend consumes the `("structured_cfg.validated", "true")`
//!   module attribute set by [`cssl_mir::validate_and_mark`] (T11-D70 / S6-D5).
//!   [`select_function`] checks the marker via [`cssl_mir::has_structured_cfg_marker`]
//!   at entry and returns [`SelectError::StructuredCfgMarkerMissing`] when
//!   absent. This makes validator-bypass crashy-loud rather than silently
//!   producing ill-formed x86-64 from `cf.br` / `cf.cond_br` shapes.
//!
//!   Defense-in-depth : even if the marker is bypassed, the per-op selection
//!   table rejects `cf.br` / `cf.cond_br` / `cssl.unsupported(Break|Continue)`
//!   with stable codes [`SelectError::UnstructuredOp`] /
//!   [`SelectError::UnsupportedOp`].
//!
//! § THE MIR-OP COVERAGE TABLE (S7-G1 stage-0)
//!   Mirrors the cranelift JIT (`cssl-cgen-cpu-cranelift::jit`) at the same
//!   stabilization tier as D1..D4 :
//!
//!   | MIR op                        | X64Inst emission                              |
//!   | ----------------------------- | --------------------------------------------- |
//!   | `arith.constant` (i*/f*)      | `Mov(reg, Imm{i,f}*)`                         |
//!   | `arith.addi/subi/muli`        | `Mov(dst, lhs); Add/Sub/IMul(dst, rhs)`       |
//!   | `arith.sdivi`                 | sign-extend (`Cdq`/`Cqo`) → `Idiv`            |
//!   | `arith.udivi`                 | zero-upper → `Div`                            |
//!   | `arith.addf/subf/mulf/divf`   | `Mov; FpAdd/FpSub/FpMul/FpDiv (SSE2)`         |
//!   | `arith.negf`                  | `XorpsConstSignBit` (sign-bit flip)           |
//!   | `arith.cmpi/cmpf` + `select`  | `Cmp; Setcc → Movzx` for cmp, `Select`        |
//!   | `func.return`                 | `Ret { operands }`                            |
//!   | `func.call`                   | `Call { callee, args }`  (no ABI lower @ G1)  |
//!   | `scf.if`                      | synthetic blocks + `Jcc` + `Jmp` to merge     |
//!   | `scf.for/while/loop`          | header / body / exit triplet + back-edge      |
//!   | `scf.yield`                   | consumed by parent scf.if (Mov to merge-vreg) |
//!   | `memref.load/store`           | `Load { dst, addr } / Store { src, addr }`    |
//!   | `cssl.heap.alloc/dealloc/realloc` | `Call(__cssl_alloc/free/realloc)`         |
//!   | `cssl.closure.*`              | REJECT (Phase-H concern)                      |
//!   | `cf.br / cf.cond_br`          | REJECT (defense-in-depth)                     |
//!   | `cssl.unsupported(Break|Continue)` | REJECT                                   |
//!
//! § VIRTUAL-REGISTER MODEL
//!   The selector emits virtual registers ([`X64VReg`]) not physical ones —
//!   sibling slice S7-G2 (register-allocator) maps virtuals to physicals.
//!   Each vreg has a 32-bit ID + a width tag ([`X64Width`]) that determines
//!   which x86-64 register-class it lives in (general-purpose i8/i16/i32/i64
//!   GPRs, or SSE2 xmm0..xmm15 for f32/f64). vreg ID 0 is reserved as the
//!   null/sentinel ; legitimate vregs start at 1.
//!
//! § ABI DEFERRED
//!   Stage-0 emits abstract `Call { callee, args, results }` and `Ret { operands }`
//!   — sibling slice S7-G3 maps the abstract operands onto System-V (Linux/macOS)
//!   or MS-x64 (Windows) physical registers + stack-spill conventions. At G1
//!   the operand lists are width-tagged so G3 sees enough info to lay out the
//!   call frame.
//!
//! § FLOATING-POINT COMPARISON SEMANTICS
//!   Per the slice handoff landmines, x86-64 has both ordered (`comiss`/`comisd`)
//!   and unordered (`ucomiss`/`ucomisd`) compare instructions. The MIR
//!   `arith.cmpf` op carries a predicate attribute (`oeq`/`one`/`ole`/`oge`/etc)
//!   that explicitly distinguishes ordered vs unordered ; the selector picks
//!   `Ucomiss`/`Ucomisd` for the `o*` (ordered) predicates because they signal
//!   QNaN as ordered-not-equal (matching IEEE 754-2008 quiet semantics) and
//!   `Comiss`/`Comisd` for the `u*` predicates which signal QNaN.
//!
//! § INTEGER DIVISION DETAIL
//!   Per the slice handoff landmines, x86-64 `idiv r/m32` requires the dividend
//!   in `edx:eax` with `edx` sign-extended from `eax` (instruction `cdq`). The
//!   64-bit form uses `rdx:rax` with `cqo`. The selector emits an explicit
//!   [`X64Inst::Cdq`] / [`X64Inst::Cqo`] before each [`X64Inst::Idiv`] to
//!   satisfy the contract ; the register-allocator (G2) is responsible for
//!   pinning the dividend to `eax`/`rax` and the result to `eax`/`rax` (and
//!   the remainder, when wanted, to `edx`/`rdx`).
//!
//! § FUTURE WORK (sibling G-axis slices)
//!   - **G2** : graph-color + linear-scan hybrid register allocator. Consumes
//!     [`X64Func`] with virtual regs ; emits physical-reg-allocated form.
//!   - **G3** : System-V AMD64 + MS-x64 ABI lowering. Maps abstract `Call`
//!     args onto integer-class regs (rdi/rsi/rdx/rcx/r8/r9 SysV, rcx/rdx/r8/r9
//!     MS-x64) + xmm0..xmm7 for floats + stack-spill for excess.
//!   - **G4** : machine-code encoder. [`X64Inst`] → bytes (REX prefix
//!     handling + ModR/M + SIB + immediate encoding).
//!   - **G5** : object-file emitter. ELF / COFF / Mach-O writer + relocations
//!     for `Call` to extern symbols.

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::similar_names)]

pub mod display;
pub mod func;
pub mod inst;
pub mod select;
pub mod vreg;

pub use display::format_func;
pub use func::{X64Block, X64Func, X64Signature};
pub use inst::{
    BlockId, FpCmpKind, IntCmpKind, MemAddr, MemScale, X64Imm, X64Inst, X64SetCondCode, X64Term,
};
pub use select::{select_function, select_module, SelectError};
pub use vreg::{X64VReg, X64Width};

/// Crate version exposed for scaffold verification.
pub const STAGE1_FOUNDATION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod scaffold_tests {
    use super::STAGE1_FOUNDATION;

    #[test]
    fn scaffold_version_present() {
        assert!(!STAGE1_FOUNDATION.is_empty());
    }
}
