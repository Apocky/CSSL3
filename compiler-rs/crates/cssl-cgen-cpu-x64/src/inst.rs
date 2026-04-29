//! X64 instruction skeleton + X64Func / X64FuncAllocated.
//!
//! § STATUS — S7-G2 surface
//!   This module defines the *canonical surface* the register allocator
//!   consumes. S7-G1 will own the per-MIR-op lowering tables that *produce*
//!   `X64Func` ; this slice declares the shape so G1 conforms.
//!
//! § DESIGN
//!   - `X64Inst` is intentionally small at G2 — `mov` / `add` / `sub` / `cmp`
//!     / `imul` / `xor` / `cdq` / `idiv` / `call` / `ret` / `jmp` / `jcc` /
//!     `push` / `pop` / `lea` / SSE moves + arith / spill-marker / reload-marker.
//!   - Each instruction tracks its `uses` (read operand vregs) + `defs` (written
//!     operand vregs) explicitly, so the interval-computation pass doesn't have
//!     to walk into operand-decoding logic.
//!   - `fixed_uses` / `fixed_defs` carry x86-64 hard constraints (e.g. `idiv`
//!     reads/writes `rax` + `rdx` ; `mul` writes `rax`+`rdx`). The allocator
//!     honors these constraints during interval coloring.
//!   - Spill / reload markers are first-class instructions ; the allocator
//!     inserts them after picking spill victims so the byte-emission pass
//!     (S7-G3) emits the actual `mov [rsp+disp], reg` / `mov reg, [rsp+disp]`.
//!
//! § VARIANT TABLE
//!   See [`X64InstKind`] for the enumerated variants.

use crate::reg::{RegBank, X64PReg, X64VReg};
use core::fmt;

/// Memory addressing mode for x64 operands. Stage-0 supports the simplest case :
/// `[base + disp]`. SIB (scale-index-base) addressing lands when array indexing
/// patterns surface in the lowering tables (S7-G1+).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemAddr {
    /// Base register vreg (typically rsp for stack ops).
    pub base: X64VReg,
    /// Constant displacement.
    pub disp: i32,
}

impl fmt::Display for MemAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.disp >= 0 {
            write!(f, "[{} + {}]", self.base, self.disp)
        } else {
            write!(f, "[{} - {}]", self.base, -self.disp)
        }
    }
}

/// Operand of an x64 instruction in vreg form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum X64Operand {
    /// Virtual register reference.
    Reg(X64VReg),
    /// Immediate 32-bit signed (will be sign-extended at emission).
    Imm32(i32),
    /// Immediate 64-bit signed (only legal for `mov reg, imm64`).
    Imm64(i64),
    /// Memory operand.
    Mem(MemAddr),
}

impl fmt::Display for X64Operand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Reg(v) => write!(f, "{v}"),
            Self::Imm32(i) => write!(f, "{i}"),
            Self::Imm64(i) => write!(f, "{i}"),
            Self::Mem(m) => write!(f, "{m}"),
        }
    }
}

/// The instruction-kind enum. S7-G2 covers a working subset ; new variants
/// land alongside their per-MIR-op lowering rule (S7-G1+).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum X64InstKind {
    /// `mov dst, src` — register-to-register, immediate-to-register, or
    /// memory-to-register / register-to-memory move.
    Mov { dst: X64Operand, src: X64Operand },
    /// `add dst, src` — two-address-form integer add (dst is both used + defined).
    Add { dst: X64VReg, src: X64Operand },
    /// `sub dst, src` — two-address-form integer subtract.
    Sub { dst: X64VReg, src: X64Operand },
    /// `imul dst, src` — signed multiply, two-address.
    Imul { dst: X64VReg, src: X64Operand },
    /// `cmp lhs, rhs` — flags-only ; defs none.
    Cmp { lhs: X64VReg, rhs: X64Operand },
    /// `xor dst, src` — bitwise xor / zeroing idiom.
    Xor { dst: X64VReg, src: X64Operand },
    /// `lea dst, [base + disp]` — load-effective-address (also used for
    /// computing addresses without touching memory).
    Lea { dst: X64VReg, addr: MemAddr },
    /// `push reg` — prologue/epilogue + argument-passing helper.
    Push { src: X64VReg },
    /// `pop reg` — prologue/epilogue + argument-passing helper.
    Pop { dst: X64VReg },
    /// `call target` — function call. Argument vregs live in fixed_uses ;
    /// return-value vregs in fixed_defs. The allocator treats Call as a
    /// caller-saved-register clobber point.
    Call {
        /// Target callable name (linker-resolved).
        target: String,
    },
    /// `ret` — function return.
    Ret,
    /// `jmp label` — unconditional branch to a labeled instruction in the same fn.
    Jmp { target: String },
    /// `jcc label` — conditional branch on flag-condition.
    Jcc { cond: Cond, target: String },
    /// Pseudo-instruction : labels the start of a basic block. Zero bytes emitted.
    Label { name: String },
    /// Pseudo-instruction : marks where a vreg should be spilled. The allocator
    /// inserts these after picking spill victims ; the byte-emission pass
    /// converts them into `mov [rsp+slot], vreg`.
    SpillMarker { vreg: X64VReg },
    /// Pseudo-instruction : marks where a previously-spilled vreg is reloaded.
    /// Converts to `mov vreg, [rsp+slot]`.
    ReloadMarker { vreg: X64VReg },
    /// SSE float `addsd / addss / addpd / addps`. Bank is XMM.
    Addf { dst: X64VReg, src: X64Operand },
    /// SSE float subtract.
    Subf { dst: X64VReg, src: X64Operand },
    /// SSE float multiply.
    Mulf { dst: X64VReg, src: X64Operand },
    /// SSE float divide.
    Divf { dst: X64VReg, src: X64Operand },
}

/// Branch condition (mirrors x64 condition codes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cond {
    /// Equal / zero.
    Eq,
    /// Not equal / not zero.
    Ne,
    /// Less than (signed).
    Lt,
    /// Less or equal (signed).
    Le,
    /// Greater than (signed).
    Gt,
    /// Greater or equal (signed).
    Ge,
}

impl fmt::Display for Cond {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Eq => "e",
            Self::Ne => "ne",
            Self::Lt => "l",
            Self::Le => "le",
            Self::Gt => "g",
            Self::Ge => "ge",
        })
    }
}

/// One x86-64 instruction in vreg form.
///
/// The `uses` / `defs` sets are explicit so the live-interval computation
/// doesn't reach into operand-decoding. `fixed_uses` / `fixed_defs` carry
/// hard ABI / instruction constraints (`idiv` requires rax+rdx ; calls clobber
/// the caller-saved set ; etc.).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct X64Inst {
    /// The mnemonic + operand shape.
    pub kind: X64InstKind,
    /// Vregs read by this instruction.
    pub uses: Vec<X64VReg>,
    /// Vregs written by this instruction.
    pub defs: Vec<X64VReg>,
    /// Hard preg uses (e.g. `idiv` reads rax + rdx).
    pub fixed_uses: Vec<X64PReg>,
    /// Hard preg defs (e.g. `idiv` writes rax + rdx ; calls write rax).
    pub fixed_defs: Vec<X64PReg>,
    /// Pregs clobbered for the duration of this instruction (e.g. caller-saved
    /// regs at a Call site). The allocator may not have a vreg in any clobbered
    /// preg across this instruction's program-point.
    pub clobbers: Vec<X64PReg>,
}

impl X64Inst {
    /// Constructor : `mov dst, src` with auto-derived uses/defs from operand shape.
    #[must_use]
    pub fn mov(dst: X64VReg, src: X64Operand) -> Self {
        let mut uses = Vec::new();
        if let X64Operand::Reg(v) = src {
            uses.push(v);
        }
        if let X64Operand::Mem(m) = src {
            uses.push(m.base);
        }
        Self {
            kind: X64InstKind::Mov {
                dst: X64Operand::Reg(dst),
                src,
            },
            uses,
            defs: vec![dst],
            fixed_uses: vec![],
            fixed_defs: vec![],
            clobbers: vec![],
        }
    }

    /// Constructor : two-address `add dst, src` (dst both used + defined).
    #[must_use]
    pub fn add(dst: X64VReg, src: X64Operand) -> Self {
        let mut uses = vec![dst];
        if let X64Operand::Reg(v) = src {
            uses.push(v);
        }
        Self {
            kind: X64InstKind::Add { dst, src },
            uses,
            defs: vec![dst],
            fixed_uses: vec![],
            fixed_defs: vec![],
            clobbers: vec![],
        }
    }

    /// Constructor : two-address `sub dst, src`.
    #[must_use]
    pub fn sub(dst: X64VReg, src: X64Operand) -> Self {
        let mut uses = vec![dst];
        if let X64Operand::Reg(v) = src {
            uses.push(v);
        }
        Self {
            kind: X64InstKind::Sub { dst, src },
            uses,
            defs: vec![dst],
            fixed_uses: vec![],
            fixed_defs: vec![],
            clobbers: vec![],
        }
    }

    /// Constructor : `cmp lhs, rhs` (no defs ; flags-only).
    #[must_use]
    pub fn cmp(lhs: X64VReg, rhs: X64Operand) -> Self {
        let mut uses = vec![lhs];
        if let X64Operand::Reg(v) = rhs {
            uses.push(v);
        }
        Self {
            kind: X64InstKind::Cmp { lhs, rhs },
            uses,
            defs: vec![],
            fixed_uses: vec![],
            fixed_defs: vec![],
            clobbers: vec![],
        }
    }

    /// Constructor : `ret`.
    #[must_use]
    pub fn ret() -> Self {
        Self {
            kind: X64InstKind::Ret,
            uses: vec![],
            defs: vec![],
            fixed_uses: vec![],
            fixed_defs: vec![],
            clobbers: vec![],
        }
    }

    /// Constructor : function call site with caller-saved clobbers materialized.
    /// `arg_vregs` are the vregs holding arguments (consumed by the call) ;
    /// `result_vreg` is the vreg the return value lands in (defined post-call).
    /// Caller-saved-preg clobbers should be added by the caller via
    /// [`Self::with_clobbers`] once an Abi is known.
    #[must_use]
    pub fn call(
        target: impl Into<String>,
        arg_vregs: Vec<X64VReg>,
        result_vreg: Option<X64VReg>,
    ) -> Self {
        let defs = result_vreg.map(|v| vec![v]).unwrap_or_default();
        Self {
            kind: X64InstKind::Call {
                target: target.into(),
            },
            uses: arg_vregs,
            defs,
            fixed_uses: vec![],
            fixed_defs: vec![],
            clobbers: vec![],
        }
    }

    /// Builder : add a clobber list (typically the ABI's caller-saved set at a Call).
    #[must_use]
    pub fn with_clobbers(mut self, clobbers: Vec<X64PReg>) -> Self {
        self.clobbers = clobbers;
        self
    }

    /// Builder : add fixed preg uses + defs (e.g. `idiv` rax+rdx).
    #[must_use]
    pub fn with_fixed(mut self, fixed_uses: Vec<X64PReg>, fixed_defs: Vec<X64PReg>) -> Self {
        self.fixed_uses = fixed_uses;
        self.fixed_defs = fixed_defs;
        self
    }

    /// Constructor : a label pseudo-instruction.
    #[must_use]
    pub fn label(name: impl Into<String>) -> Self {
        Self {
            kind: X64InstKind::Label { name: name.into() },
            uses: vec![],
            defs: vec![],
            fixed_uses: vec![],
            fixed_defs: vec![],
            clobbers: vec![],
        }
    }
}

impl fmt::Display for X64Inst {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            X64InstKind::Mov { dst, src } => write!(f, "mov {dst}, {src}"),
            X64InstKind::Add { dst, src } => write!(f, "add {dst}, {src}"),
            X64InstKind::Sub { dst, src } => write!(f, "sub {dst}, {src}"),
            X64InstKind::Imul { dst, src } => write!(f, "imul {dst}, {src}"),
            X64InstKind::Cmp { lhs, rhs } => write!(f, "cmp {lhs}, {rhs}"),
            X64InstKind::Xor { dst, src } => write!(f, "xor {dst}, {src}"),
            X64InstKind::Lea { dst, addr } => write!(f, "lea {dst}, {addr}"),
            X64InstKind::Push { src } => write!(f, "push {src}"),
            X64InstKind::Pop { dst } => write!(f, "pop {dst}"),
            X64InstKind::Call { target } => write!(f, "call {target}"),
            X64InstKind::Ret => f.write_str("ret"),
            X64InstKind::Jmp { target } => write!(f, "jmp {target}"),
            X64InstKind::Jcc { cond, target } => write!(f, "j{cond} {target}"),
            X64InstKind::Label { name } => write!(f, "{name}:"),
            X64InstKind::SpillMarker { vreg } => write!(f, "; SPILL {vreg}"),
            X64InstKind::ReloadMarker { vreg } => write!(f, "; RELOAD {vreg}"),
            X64InstKind::Addf { dst, src } => write!(f, "addsd {dst}, {src}"),
            X64InstKind::Subf { dst, src } => write!(f, "subsd {dst}, {src}"),
            X64InstKind::Mulf { dst, src } => write!(f, "mulsd {dst}, {src}"),
            X64InstKind::Divf { dst, src } => write!(f, "divsd {dst}, {src}"),
        }
    }
}

/// Function in vreg form — the input to the register allocator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct X64Func {
    /// Function name (linker-visible).
    pub name: String,
    /// Calling convention.
    pub abi: crate::reg::Abi,
    /// Linear instruction stream. Multiple basic blocks are encoded as
    /// instructions with `Label` pseudo-ops at block heads + `Jmp`/`Jcc`
    /// branches.
    pub insts: Vec<X64Inst>,
    /// Total number of vregs used in `insts`. The allocator uses this to
    /// pre-size internal tables.
    pub vreg_count: u32,
    /// Parameter vregs — these enter the function live (program-point 0).
    /// At ABI-lowering time they're moved out of the canonical-arg pregs into
    /// allocator-friendly vregs.
    pub param_vregs: Vec<X64VReg>,
    /// Result vregs — these must be live at every `Ret`.
    pub result_vregs: Vec<X64VReg>,
}

impl X64Func {
    /// Construct an empty function shell.
    #[must_use]
    pub fn new(name: impl Into<String>, abi: crate::reg::Abi) -> Self {
        Self {
            name: name.into(),
            abi,
            insts: vec![],
            vreg_count: 0,
            param_vregs: vec![],
            result_vregs: vec![],
        }
    }

    /// Append an instruction.
    pub fn push(&mut self, inst: X64Inst) {
        // Keep vreg_count up to date.
        for v in inst.uses.iter().chain(inst.defs.iter()) {
            if v.index + 1 > self.vreg_count {
                self.vreg_count = v.index + 1;
            }
        }
        self.insts.push(inst);
    }

    /// Number of instructions.
    #[must_use]
    pub fn len(&self) -> usize {
        self.insts.len()
    }

    /// Empty?
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.insts.is_empty()
    }
}

/// The register-allocated form. Each instruction is annotated with the
/// physical registers chosen for its vregs ; spill / reload markers carry
/// stack-slot offsets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct X64FuncAllocated {
    /// Function name.
    pub name: String,
    /// ABI.
    pub abi: crate::reg::Abi,
    /// Per-instruction allocation : the ith instruction's vreg→preg mapping.
    /// One [`AllocatedInst`] per original instruction (prologue/epilogue
    /// push/pop pairs are recorded separately on `callee_saved_used` +
    /// `frame_size`).
    pub allocated_insts: Vec<AllocatedInst>,
    /// Callee-saved pregs the function uses ; the prologue must `push` them
    /// + the epilogue must `pop` in reverse order.
    pub callee_saved_used: Vec<X64PReg>,
    /// Total spill-slot frame size, including 16-byte-alignment padding.
    /// Caller emits `sub rsp, frame_size` in the prologue + `add rsp, frame_size`
    /// in the epilogue.
    pub frame_size: u32,
    /// Per-vreg final assignment : either a preg or a spill-slot offset.
    pub assignment: Vec<VregAssignment>,
}

/// One instruction post-allocation. The `inst` field is preserved for
/// debugging / printing ; the `preg_for_vreg` slice is the actual byte-emission
/// input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AllocatedInst {
    /// The original instruction (vreg form).
    pub inst: X64Inst,
    /// Vreg → preg resolution at this program-point. Each entry is
    /// `(vreg, resolution)`. If the vreg is currently spilled, the resolution
    /// records the slot.
    pub resolutions: Vec<VregResolution>,
}

/// Final assignment of a vreg : either a preg, a stack spill-slot, or split
/// across multiple lifetimes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VregAssignment {
    /// Lives entirely in this preg.
    Preg(X64PReg),
    /// Lives entirely in this spill-slot.
    Spill(crate::spill::SpillSlot),
    /// Live-range was split. Each segment is either a preg or a slot.
    Split(Vec<VregSegment>),
}

/// One segment of a split live-range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VregSegment {
    /// Inclusive start program-point.
    pub start: usize,
    /// Exclusive end program-point.
    pub end: usize,
    /// Where the vreg lives in this segment.
    pub location: VregLocation,
}

/// Where a vreg lives (preg or spill-slot) in some segment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VregLocation {
    /// In a physical register.
    Preg(X64PReg),
    /// On the stack.
    Spill(crate::spill::SpillSlot),
}

/// Per-vreg resolution at a single program-point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VregResolution {
    /// The vreg.
    pub vreg: X64VReg,
    /// Where it lives at this point.
    pub location: VregLocation,
}

impl VregLocation {
    /// `true` iff in a register (vs spill-slot).
    #[must_use]
    pub const fn is_preg(self) -> bool {
        matches!(self, Self::Preg(_))
    }

    /// Returns the preg if currently in a register.
    #[must_use]
    pub const fn as_preg(self) -> Option<X64PReg> {
        match self {
            Self::Preg(p) => Some(p),
            Self::Spill(_) => None,
        }
    }

    /// Returns the bank of the location (preg's bank, or the slot's bank).
    #[must_use]
    pub const fn bank(self) -> RegBank {
        match self {
            Self::Preg(p) => p.bank(),
            Self::Spill(s) => s.bank,
        }
    }
}

#[cfg(test)]
mod inst_tests {
    use super::*;
    use crate::reg::{Abi, RegBank};

    #[test]
    fn mov_constructor_records_uses_and_defs() {
        let v0 = X64VReg::gp(0);
        let v1 = X64VReg::gp(1);
        let inst = X64Inst::mov(v0, X64Operand::Reg(v1));
        assert_eq!(inst.defs, vec![v0]);
        assert_eq!(inst.uses, vec![v1]);
    }

    #[test]
    fn add_two_address_records_dst_as_use_and_def() {
        let v0 = X64VReg::gp(0);
        let v1 = X64VReg::gp(1);
        let inst = X64Inst::add(v0, X64Operand::Reg(v1));
        assert_eq!(inst.defs, vec![v0]);
        assert!(inst.uses.contains(&v0)); // dst is used (read-modify-write)
        assert!(inst.uses.contains(&v1));
    }

    #[test]
    fn cmp_records_no_defs() {
        let v0 = X64VReg::gp(0);
        let v1 = X64VReg::gp(1);
        let inst = X64Inst::cmp(v0, X64Operand::Reg(v1));
        assert!(inst.defs.is_empty());
        assert_eq!(inst.uses.len(), 2);
    }

    #[test]
    fn x64func_push_updates_vreg_count() {
        let mut f = X64Func::new("fn0", Abi::SysVAmd64);
        let v3 = X64VReg::gp(3);
        let v5 = X64VReg::gp(5);
        f.push(X64Inst::mov(v3, X64Operand::Reg(v5)));
        assert_eq!(f.vreg_count, 6); // max index 5 + 1
    }

    #[test]
    fn vreg_location_bank_routes_correctly() {
        let preg_loc = VregLocation::Preg(X64PReg::Rax);
        assert_eq!(preg_loc.bank(), RegBank::Gp);
        let xmm_loc = VregLocation::Preg(X64PReg::Xmm5);
        assert_eq!(xmm_loc.bank(), RegBank::Xmm);
    }
}
