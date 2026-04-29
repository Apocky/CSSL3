//! CSSLv3 stage-1 — owned x86-64 backend (S7-G2 : register allocator).
//!
//! § SPEC : `specs/07_CODEGEN.csl` § CPU BACKEND § stage1+
//!          (owned x86-64 emitter + own regalloc graph-color + linear-scan hybrid).
//!
//! § ROLE — S7-G2 (this slice)
//!   This crate consumes the [`X64Func`] virtual-register form (defined in
//!   this slice as a *canonical surface* — S7-G1 will adopt or extend it as
//!   the per-op lowering tables land) and produces a physical-register-allocated
//!   form via **linear-scan register allocation** (LSRA).
//!
//!   The classic Poletto+Sarkar 1999 LSRA algorithm is used :
//!   intervals sorted by start-point, greedy assignment from a pool of free
//!   physical registers, spill-on-conflict via the further-future-use heuristic,
//!   and live-range splitting for spilled vregs that need a register again later.
//!
//! § SCOPE (S7-G2)
//!   - [`reg`]      — `X64PReg` (16 GP + 16 XMM physical regs) + `X64VReg` (virtual)
//!                    + ABI metadata (caller-saved / callee-saved per Abi).
//!   - [`inst`]     — `X64Inst` instruction skeleton (mov / add / cmp / call / ret /
//!                    spill / reload markers) + `X64Func` (vreg form) + `X64FuncAllocated`
//!                    (preg form, post-LSRA).
//!   - [`interval`] — `LiveInterval` { vreg, start, end, hints, kind } + per-fn
//!                    interval computation walking the linear instruction stream.
//!   - [`alloc`]    — `LinearScanAllocator` ; the LSRA driver. Sorts intervals by
//!                    start, walks them in order, maintains active set, frees expired
//!                    intervals, picks free pregs, spills further-future-use on
//!                    conflict, splits intervals that need a reg again post-spill.
//!   - [`spill`]    — `SpillSlots` ; on-stack frame layout. Every slot is 16-byte
//!                    aligned (SSE alignment) ; i32 / i64 over-allocate when needed.
//!                    Tracks total frame size for prologue / epilogue emission.
//!
//! § ALGORITHM CHOICE — linear-scan vs graph-coloring
//!   LSRA is chosen for stage-0 / stage-1 register allocation per `specs/07_CODEGEN.csl`
//!   § CPU BACKEND § stage1+ "graph-color + linear-scan hybrid per-fn". This slice
//!   lands the linear-scan half ; graph-coloring (Chaitin-Briggs) is deferred
//!   until a benchmark surfaces a workload where LSRA's spill heuristic is
//!   measurably worse. Reasons :
//!     * LSRA is O(n log n) on intervals ; graph-coloring is O(n²) on the
//!       interference graph in the general case.
//!     * LSRA is mechanically simple — fewer subtle bugs, easier to validate
//!       against the published reference.
//!     * LSRA's spill heuristic (further-future-use) is competitive with
//!       graph-coloring on straight-line code + structured loops, which is what
//!       CSSLv3 MIR produces post-D5 structured-CFG validation.
//!   The hybrid future-slice (S7-G3+) layers Chaitin-Briggs over LSRA-decided
//!   intervals : LSRA picks the *spill set* (which vregs lose their register)
//!   while graph-coloring decides *which preg* the surviving vregs get,
//!   improving register pressure on dense bodies.
//!
//! § RESERVED REGISTER DISCIPLINE
//!   At S7-G2 the following pregs are NEVER allocated to user vregs :
//!     * `Rsp` — stack pointer ; reserved across all ABIs.
//!     * `Rbp` — frame pointer ; kept reserved at S7-G2 ("omit-frame-pointer"
//!               mode is a future slice flag).
//!   The remaining 14 GP pregs (rax/rbx/rcx/rdx/rsi/rdi/r8..r15) and all 16 XMM
//!   pregs are available to the allocator. Per-ABI caller-saved / callee-saved
//!   metadata drives the prologue / epilogue push/pop generation.
//!
//! § DEFERRED (future slices)
//!   - Phi-node coalescing hints — the [`LiveInterval::hints`] field accepts
//!     coalesce-with-other-vreg hints but the LSRA driver currently honors
//!     them only as preference, not as constraint. Full move-coalescing lands
//!     when SSA φ-resolution lands upstream.
//!   - Chaitin-Briggs graph-coloring fallback for high-pressure bodies.
//!   - Two-address-form constraint propagation (e.g. `mul` on x64 has fixed
//!     `rax` / `rdx` constraints) — encoded in [`inst::X64Inst`] as `fixed_use`
//!     / `fixed_def` slots ; the allocator honors them but the constraint
//!     propagation pass that *introduces* them is in S7-G1.
//!   - Live-range hole-filling : current LSRA treats intervals as contiguous
//!     half-open `[start, end)` ranges. Real holes (gaps where a vreg is dead
//!     mid-fn) lead to slightly worse register pressure ; the SSA-based
//!     interval-with-holes form is a future-slice optimization.
//!   - Real x64 machine-code emission. S7-G2 produces `X64FuncAllocated` with
//!     pregs assigned + spill slots resolved ; the byte-emission pass
//!     (REX prefix + ModR/M + SIB + displacement + immediate) is S7-G3.

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::module_name_repetitions)]

pub mod alloc;
pub mod inst;
pub mod interval;
pub mod reg;
pub mod spill;

pub use alloc::{allocate, AllocError, AllocReport, LinearScanAllocator};
pub use inst::{X64Func, X64FuncAllocated, X64Inst, X64InstKind, X64Operand};
pub use interval::{compute_live_intervals, IntervalKind, LiveInterval, ProgramPoint};
pub use reg::{Abi, RegBank, RegRole, X64PReg, X64VReg};
pub use spill::{SpillSlot, SpillSlots};

/// Crate version exposed for scaffold verification (matches sibling cssl-cgen-cpu-cranelift convention).
pub const STAGE1_X64_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod scaffold_tests {
    use super::STAGE1_X64_SCAFFOLD;

    #[test]
    fn scaffold_version_present() {
        assert!(!STAGE1_X64_SCAFFOLD.is_empty());
    }
}

#[cfg(test)]
mod tests;
