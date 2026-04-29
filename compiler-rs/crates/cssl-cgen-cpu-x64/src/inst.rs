//! § X64Inst — virtual-register-based x86-64 instructions.
//!
//! § DESIGN
//!   The selector emits [`X64Inst`] in virtual-register form. Each variant
//!   captures one canonical x86-64 instruction (or a small group of related
//!   instructions sharing the same operand shape). At G1 the goal is
//!   COVERAGE not OPTIMIZATION — every D5-validated MIR op-shape selects to
//!   exactly one `X64Inst` (or a small fixed sequence for ops like `cmp +
//!   setcc + movzx` or `cdq + idiv`).
//!
//! § OPERAND SHAPE — VREG-SOURCE / VREG-DESTINATION
//!   Most instructions take a destination [`X64VReg`] + one or more source
//!   [`X64VReg`]s. The selector enforces width-matching at construction time :
//!   for a typed binary op like `Add(dst, lhs, rhs)`, all three operands have
//!   the same width tag. The encoder (G4) reads the width to pick REX prefix
//!   bits + opcode variant.
//!
//! § ABSTRACT vs CONCRETE
//!   The instruction shapes here are mostly concrete x86-64 (`Add` ≡ `add`,
//!   `Mov` ≡ `mov`, `Jcc` ≡ `j<cc>`). A few ABI-touching variants are
//!   ABSTRACT at G1 — sibling slice S7-G3 lowers them to concrete forms :
//!     - `Call` : carries abstract (callee, args, results) ; G3 emits the
//!                System-V or MS-x64 reg/stack passing setup before the
//!                actual `call` instruction.
//!     - `Ret`  : carries abstract operands ; G3 emits the return-value
//!                setup (`mov rax, vreg` etc.) before the `ret` instruction.
//!
//! § FLOATING-POINT COMPARISON
//!   The MIR `arith.cmpf` predicate (`oeq`/`one`/`ole`/`uge`/etc) maps onto
//!   x86-64's `ucomiss`/`ucomisd` (ordered, quiet-NaN-not-equal) for `o*`
//!   predicates and `comiss`/`comisd` (signaling) for `u*`. The selector
//!   picks the variant per the predicate (see [`X64Inst::Ucomi`] and
//!   [`X64Inst::Comi`]) ; the encoder emits the matching opcode.
//!
//! § INTEGER DIVISION
//!   `idiv r/m{32,64}` requires the dividend in `edx:eax` / `rdx:rax` with
//!   the upper half sign-extended from the lower half. The selector emits
//!   `Cdq` / `Cqo` followed by `Idiv` ; the register-allocator (G2) is
//!   responsible for pinning the dividend to `eax`/`rax` so the sign-ext
//!   instruction operates on the right register.

use core::fmt;

use crate::vreg::{X64VReg, X64Width};

/// Identifier for a basic block within an [`crate::X64Func`].
///
/// Block 0 is conventionally the entry block ; later blocks are created by
/// the selector for synthetic control-flow scaffolding (`scf.if` then/else/
/// merge ; `scf.for/while/loop` header/body/exit).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct BlockId(pub u32);

impl BlockId {
    /// Entry block (id = 0).
    pub const ENTRY: Self = Self(0);
}

impl fmt::Display for BlockId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "bb{}", self.0)
    }
}

/// Immediate operand for `Mov` / arithmetic ops with constant rhs.
///
/// § DESIGN
///   The selector lifts MIR `arith.constant` results to immediates when the
///   constant flows directly into a single use. Otherwise it materializes
///   the constant into a vreg via `Mov(reg, Imm)` and references the vreg.
///   At G1 we always materialize — the lift-to-imm peephole is a future
///   optimization pass.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum X64Imm {
    /// 32-bit signed integer immediate.
    I32(i32),
    /// 64-bit signed integer immediate.
    I64(i64),
    /// 32-bit IEEE 754 float immediate (encoded as bit-pattern via mov-imm
    /// or rip-relative load — encoder decides).
    F32(u32),
    /// 64-bit IEEE 754 double immediate (encoded similarly).
    F64(u64),
    /// Boolean immediate (0 = false, 1 = true).
    Bool(bool),
}

impl X64Imm {
    /// Width of the immediate on x86-64.
    #[must_use]
    pub const fn width(self) -> X64Width {
        match self {
            Self::I32(_) => X64Width::I32,
            Self::I64(_) => X64Width::I64,
            Self::F32(_) => X64Width::F32,
            Self::F64(_) => X64Width::F64,
            Self::Bool(_) => X64Width::Bool,
        }
    }
}

impl Eq for X64Imm {}

impl fmt::Display for X64Imm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::I32(v) => write!(f, "{v}i32"),
            Self::I64(v) => write!(f, "{v}i64"),
            Self::F32(bits) => write!(f, "f32:0x{bits:08x}"),
            Self::F64(bits) => write!(f, "f64:0x{bits:016x}"),
            Self::Bool(b) => write!(f, "{b}"),
        }
    }
}

/// Index-scale for memory-addressing modes (`[base + index*scale + disp]`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemScale {
    /// Scale 1 (no shift).
    One,
    /// Scale 2.
    Two,
    /// Scale 4.
    Four,
    /// Scale 8.
    Eight,
}

impl MemScale {
    /// Numeric scale value (1, 2, 4, or 8).
    #[must_use]
    pub const fn value(self) -> u32 {
        match self {
            Self::One => 1,
            Self::Two => 2,
            Self::Four => 4,
            Self::Eight => 8,
        }
    }
}

impl fmt::Display for MemScale {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.value())
    }
}

/// Memory address operand : `[base + index*scale + displacement]`.
///
/// At G1 the selector emits the simplest form needed by the MIR shape — for
/// `memref.load %ptr` we emit `MemAddr { base: ptr, index: None, displacement: 0 }`.
/// For `memref.load %ptr, %offset` we emit `MemAddr { base: ptr,
/// index: Some(offset, MemScale::One), displacement: 0 }`. Future peephole
/// passes can fold `iadd %ptr, const` into a displacement, etc.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemAddr {
    /// Base register — always present, must be `Ptr` or `I64` width.
    pub base: X64VReg,
    /// Optional indexed component : `(index, scale)`.
    pub index: Option<(X64VReg, MemScale)>,
    /// Constant byte offset added to the address.
    pub displacement: i32,
}

impl MemAddr {
    /// Simple `[base]` address.
    #[must_use]
    pub const fn base(base: X64VReg) -> Self {
        Self {
            base,
            index: None,
            displacement: 0,
        }
    }

    /// `[base + index]` address (scale 1).
    #[must_use]
    pub const fn base_plus_index(base: X64VReg, index: X64VReg) -> Self {
        Self {
            base,
            index: Some((index, MemScale::One)),
            displacement: 0,
        }
    }
}

impl fmt::Display for MemAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[")?;
        write!(f, "{}", self.base)?;
        if let Some((idx, sc)) = &self.index {
            write!(f, " + {idx} * {sc}")?;
        }
        if self.displacement != 0 {
            if self.displacement >= 0 {
                write!(f, " + {}", self.displacement)?;
            } else {
                write!(f, " - {}", -self.displacement)?;
            }
        }
        f.write_str("]")
    }
}

/// Integer comparison kind for [`X64Inst::Cmp`] + [`X64Inst::Setcc`] /
/// [`X64Term::Jcc`] decisions. Maps directly to x86-64 condition codes :
/// `je / jne / jl / jle / jg / jge` (signed) and `jb / jbe / ja / jae`
/// (unsigned).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IntCmpKind {
    Eq,
    Ne,
    /// Signed less-than (`jl`).
    Slt,
    /// Signed less-than-or-equal (`jle`).
    Sle,
    /// Signed greater-than (`jg`).
    Sgt,
    /// Signed greater-than-or-equal (`jge`).
    Sge,
    /// Unsigned less-than (`jb`).
    Ult,
    /// Unsigned less-than-or-equal (`jbe`).
    Ule,
    /// Unsigned greater-than (`ja`).
    Ugt,
    /// Unsigned greater-than-or-equal (`jae`).
    Uge,
}

impl IntCmpKind {
    /// Canonical mnemonic for diagnostics (matches MIR predicate strings).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Eq => "eq",
            Self::Ne => "ne",
            Self::Slt => "slt",
            Self::Sle => "sle",
            Self::Sgt => "sgt",
            Self::Sge => "sge",
            Self::Ult => "ult",
            Self::Ule => "ule",
            Self::Ugt => "ugt",
            Self::Uge => "uge",
        }
    }

    /// x86-64 setcc / jcc mnemonic suffix (`e`, `ne`, `l`, `le`, `g`, `ge`,
    /// `b`, `be`, `a`, `ae`).
    #[must_use]
    pub const fn x86_suffix(self) -> &'static str {
        match self {
            Self::Eq => "e",
            Self::Ne => "ne",
            Self::Slt => "l",
            Self::Sle => "le",
            Self::Sgt => "g",
            Self::Sge => "ge",
            Self::Ult => "b",
            Self::Ule => "be",
            Self::Ugt => "a",
            Self::Uge => "ae",
        }
    }
}

impl fmt::Display for IntCmpKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Floating-point comparison kind. The `Ordered` flag distinguishes `Ucomiss`
/// (ordered : QNaN signaled as ordered-not-equal — the IEEE 754-2008 quiet
/// semantics) from `Comiss` (signaling : QNaN raises invalid-operation).
///
/// MIR predicates `o*` (`oeq` / `one` / `olt` / `ole` / `ogt` / `oge`) map
/// to ordered ; `u*` (`une` / `ult` / `ule` / `ugt` / `uge`) map to
/// unordered/signaling. The `Ordered` and `Unordered` predicates themselves
/// (just-NaN-or-not) round-trip through this enum's discriminant cleanly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FpCmpKind {
    /// `oeq` — ordered equal.
    Oeq,
    /// `one` — ordered not equal.
    One,
    /// `olt` — ordered less-than.
    Olt,
    /// `ole` — ordered less-than-or-equal.
    Ole,
    /// `ogt` — ordered greater-than.
    Ogt,
    /// `oge` — ordered greater-than-or-equal.
    Oge,
    /// `une` — unordered or not-equal (NaN-true).
    Une,
    /// `ult` — unordered or less-than.
    Ult,
    /// `ule` — unordered or less-than-or-equal.
    Ule,
    /// `ugt` — unordered or greater-than.
    Ugt,
    /// `uge` — unordered or greater-than-or-equal.
    Uge,
    /// `ord` — neither operand is NaN.
    Ord,
    /// `uno` — at least one operand is NaN.
    Uno,
}

impl FpCmpKind {
    /// `true` iff this predicate uses the ordered-compare variant
    /// (`Ucomiss`/`Ucomisd`). Per the slice handoff landmines, ordered =
    /// IEEE 754-2008 quiet semantics ; unordered = signaling.
    #[must_use]
    pub const fn is_ordered(self) -> bool {
        matches!(
            self,
            Self::Oeq | Self::One | Self::Olt | Self::Ole | Self::Ogt | Self::Oge | Self::Ord
        )
    }

    /// Canonical MIR predicate string (`"oeq"`, `"olt"`, `"uno"`, …).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Oeq => "oeq",
            Self::One => "one",
            Self::Olt => "olt",
            Self::Ole => "ole",
            Self::Ogt => "ogt",
            Self::Oge => "oge",
            Self::Une => "une",
            Self::Ult => "ult",
            Self::Ule => "ule",
            Self::Ugt => "ugt",
            Self::Uge => "uge",
            Self::Ord => "ord",
            Self::Uno => "uno",
        }
    }
}

impl fmt::Display for FpCmpKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Condition-code suffix for `Setcc` instructions. Drives the x86-64 `set<cc>`
/// instruction byte that materializes the flag bit into an 8-bit register.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum X64SetCondCode {
    /// Integer comparison setcc.
    Int(IntCmpKind),
    /// Float comparison — the flag bit comes from a prior `ucomiss`/`comiss`.
    /// FP comparison sets ZF/PF/CF in IEEE-flavored ways ; the condition code
    /// here is the resolved test (`e` after `ucomiss` for oeq, etc).
    Float(FpCmpKind),
}

impl fmt::Display for X64SetCondCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Int(k) => write!(f, "set{}", k.x86_suffix()),
            Self::Float(k) => write!(f, "set:{k}"),
        }
    }
}

/// Block terminator. Each [`crate::X64Block`] ends with exactly one
/// terminator.
#[derive(Debug, Clone, PartialEq)]
pub enum X64Term {
    /// Unconditional branch to `target`.
    Jmp { target: BlockId },
    /// Conditional branch : `if cond { jmp then } else { jmp else }`.
    /// `cond_kind` records the test that produced the cond flag (used by the
    /// encoder to pick the right `j<cc>` opcode).
    Jcc {
        cond_kind: X64SetCondCode,
        cond_vreg: X64VReg,
        then_block: BlockId,
        else_block: BlockId,
    },
    /// Return from function. `operands` are the source-level result vregs ;
    /// G3 lowers them onto the System-V / MS-x64 return-value registers.
    Ret { operands: Vec<X64VReg> },
    /// Block falls through to `next` without emitting a `jmp` (used for
    /// straight-line scaffolding before G2 lays out the real block order).
    /// The encoder treats this identically to `Jmp { target: next }` ; the
    /// distinction matters only for layout heuristics in G2.
    Fallthrough { next: BlockId },
    /// Reserved for unreachable terminator (e.g., after a panic). Stage-0
    /// MIR doesn't produce these but the variant is reserved to keep the
    /// type total.
    Unreachable,
}

impl fmt::Display for X64Term {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Jmp { target } => write!(f, "jmp {target}"),
            Self::Jcc {
                cond_kind,
                cond_vreg,
                then_block,
                else_block,
            } => write!(f, "jcc({cond_kind} {cond_vreg}) {then_block}, {else_block}"),
            Self::Ret { operands } => {
                write!(f, "ret")?;
                for (i, v) in operands.iter().enumerate() {
                    if i == 0 {
                        write!(f, " ")?;
                    } else {
                        write!(f, ", ")?;
                    }
                    write!(f, "{v}")?;
                }
                Ok(())
            }
            Self::Fallthrough { next } => write!(f, "fallthrough {next}"),
            Self::Unreachable => f.write_str("unreachable"),
        }
    }
}

/// A virtual-register-based x86-64 instruction.
///
/// § COVERAGE NOTE
///   The variants below cover the subset of x86-64 needed by the D5-validated
///   MIR op-set listed in `crate` doc-block. Future passes (peephole, addr-mode
///   folding) grow the variant set ; G2/G3/G4/G5 consume this surface as-is.
#[derive(Debug, Clone, PartialEq)]
pub enum X64Inst {
    // ─────────────────────────────────────────────────────────────────
    // § Move + immediate materialization.
    // ─────────────────────────────────────────────────────────────────
    /// `dst <- src` — register-to-register move.
    Mov { dst: X64VReg, src: X64VReg },
    /// `dst <- imm` — immediate materialization. Width must match `dst.width`.
    MovImm { dst: X64VReg, imm: X64Imm },
    // ─────────────────────────────────────────────────────────────────
    // § Integer arithmetic — destination-overlapping form (`add dst, src`
    //   on x86-64 is `dst <- dst + src`). The selector emits a `Mov dst, lhs`
    //   before the op so `dst <- lhs + rhs`. G2 may fold the move when
    //   register-coalescing applies.
    // ─────────────────────────────────────────────────────────────────
    /// `dst <- dst + src`.
    Add { dst: X64VReg, src: X64VReg },
    /// `dst <- dst - src`.
    Sub { dst: X64VReg, src: X64VReg },
    /// `dst <- dst * src` — signed integer multiply (`imul`). Single-operand
    /// `mul` form (full-width product into rdx:rax) is reserved for future
    /// 128-bit ops.
    IMul { dst: X64VReg, src: X64VReg },
    /// `cdq` — sign-extend `eax` into `edx:eax` for 32-bit signed division.
    /// Selector emits before [`Self::Idiv`] when the dividend is `i32`.
    Cdq,
    /// `cqo` — sign-extend `rax` into `rdx:rax` for 64-bit signed division.
    Cqo,
    /// `idiv divisor` — signed division. Result : quotient in `eax`/`rax`,
    /// remainder in `edx`/`rdx`. Selector materializes the result vreg from
    /// `eax`/`rax` after the divide. Width is the `divisor` vreg's width.
    Idiv { divisor: X64VReg },
    /// `div divisor` — unsigned division. Same shape as [`Self::Idiv`]
    /// but unsigned ; selector emits a `xor edx, edx` (or equivalent) instead
    /// of `cdq`/`cqo`.
    Div { divisor: X64VReg },
    /// `xor edx, edx` (or `xor rdx, rdx` for i64) — zeroes the upper-half
    /// register before unsigned division. Encoded via xor reg, reg of the
    /// matching width. The selector emits this immediately before
    /// [`Self::Div`].
    XorRdx { width: X64Width },
    /// `dst <- dst & src`.
    And { dst: X64VReg, src: X64VReg },
    /// `dst <- dst | src`.
    Or { dst: X64VReg, src: X64VReg },
    /// `dst <- dst ^ src`.
    Xor { dst: X64VReg, src: X64VReg },
    /// `dst <- dst << src` (shift count in `cl` per x86-64 SHL convention ;
    /// G2 pins `src` to `cl` when needed).
    Shl { dst: X64VReg, src: X64VReg },
    /// `dst <- dst >> src` (logical / unsigned).
    Shr { dst: X64VReg, src: X64VReg },
    /// `dst <- dst >> src` (arithmetic / signed).
    Sar { dst: X64VReg, src: X64VReg },
    /// `dst <- -dst` (two's-complement negation).
    Neg { dst: X64VReg },
    /// `dst <- ~dst` (bitwise complement).
    Not { dst: X64VReg },
    // ─────────────────────────────────────────────────────────────────
    // § SSE2 floating-point.
    // ─────────────────────────────────────────────────────────────────
    /// `addss xmm, xmm` (f32) or `addsd xmm, xmm` (f64) : `dst <- dst + src`.
    FpAdd { dst: X64VReg, src: X64VReg },
    /// `subss / subsd` : `dst <- dst - src`.
    FpSub { dst: X64VReg, src: X64VReg },
    /// `mulss / mulsd` : `dst <- dst * src`.
    FpMul { dst: X64VReg, src: X64VReg },
    /// `divss / divsd` : `dst <- dst / src`.
    FpDiv { dst: X64VReg, src: X64VReg },
    /// `xorps xmm, sign-bit-mask` : `dst <- -dst` for f32.
    /// On x86-64 SSE2, fp-negation is most efficiently a sign-bit XOR with the
    /// IEEE 754 sign-bit constant (`0x80000000` for f32, `0x8000000000000000`
    /// for f64). The encoder emits the rip-relative load of the constant.
    FpNeg { dst: X64VReg, width: X64Width },
    /// `ucomiss xmm, xmm` (f32) or `ucomisd xmm, xmm` (f64) — ordered
    /// floating-point compare. Sets ZF/PF/CF flags ; followed by [`Self::Setcc`]
    /// or [`X64Term::Jcc`] to materialize the boolean.
    Ucomi { lhs: X64VReg, rhs: X64VReg },
    /// `comiss / comisd` — signaling floating-point compare (used for `u*`
    /// predicates that want unordered → invalid-operation flag).
    Comi { lhs: X64VReg, rhs: X64VReg },
    // ─────────────────────────────────────────────────────────────────
    // § Comparisons + conditional materialization.
    // ─────────────────────────────────────────────────────────────────
    /// `cmp lhs, rhs` — integer compare. Sets ZF/SF/CF/OF.
    Cmp { lhs: X64VReg, rhs: X64VReg },
    /// `set<cc> dst` — materialize flag-bit into 8-bit register. The selector
    /// always follows up with a `Movzx` to widen to the result width if
    /// the MIR boolean is consumed as a wider type.
    Setcc {
        dst: X64VReg,
        cond_kind: X64SetCondCode,
    },
    /// `movzx dst, src` — zero-extend src into dst (unsigned widen). Used after
    /// `setcc` when the consumer expects a wider boolean than 1 byte.
    Movzx { dst: X64VReg, src: X64VReg },
    /// `movsx dst, src` — sign-extend (used for signed-integer widening).
    Movsx { dst: X64VReg, src: X64VReg },
    /// `cmov<cc> dst, src` — conditional move. Used to lower `arith.select`
    /// when the cond is a boolean already in flags ; otherwise the selector
    /// materializes via `cmp + setcc`.
    Cmov {
        dst: X64VReg,
        src: X64VReg,
        cond_kind: X64SetCondCode,
    },
    /// `arith.select` lowering with explicit cond vreg : the selector emits
    /// `Test(cond, cond)` then `cmovne dst, src_true ; mov dst, src_false`.
    /// At G1 we keep this as a high-level op so G2's coalescer can choose
    /// the best concrete shape.
    Select {
        dst: X64VReg,
        cond: X64VReg,
        if_true: X64VReg,
        if_false: X64VReg,
    },
    /// `test src, src` — sets ZF based on whether src is zero. Used before
    /// `Cmov` to materialize the cond flag from a boolean vreg.
    Test { src: X64VReg },
    // ─────────────────────────────────────────────────────────────────
    // § Memory operations (memref.load / memref.store).
    // ─────────────────────────────────────────────────────────────────
    /// `mov dst, [addr]` — load from memory. Width must match `dst.width`.
    Load { dst: X64VReg, addr: MemAddr },
    /// `mov [addr], src` — store to memory. Width must match `src.width`.
    Store { src: X64VReg, addr: MemAddr },
    /// `lea dst, [addr]` — load-effective-address. Used for pointer arithmetic
    /// without dereferencing.
    Lea { dst: X64VReg, addr: MemAddr },
    // ─────────────────────────────────────────────────────────────────
    // § Function calls — abstract at G1, lowered to System-V / MS-x64 at G3.
    // ─────────────────────────────────────────────────────────────────
    /// `call <callee>` — abstract call. Args + results are vregs ; G3 maps
    /// them onto the System-V or MS-x64 ABI.
    Call {
        callee: String,
        args: Vec<X64VReg>,
        results: Vec<X64VReg>,
    },
    // ─────────────────────────────────────────────────────────────────
    // § Stack frame primitives — used by G3's prologue/epilogue.
    // ─────────────────────────────────────────────────────────────────
    /// `push reg`. Reserved for G3 ; not emitted by the G1 selector but
    /// the variant is part of the public type so G3 can extend the inst stream.
    Push { src: X64VReg },
    /// `pop reg`. Reserved for G3.
    Pop { dst: X64VReg },
}

impl X64Inst {
    /// Result vreg of this instruction, if any. Used by display + sanity tests.
    /// For multi-result instructions ([`Self::Call`] with multiple returns,
    /// [`Self::Idiv`] producing both quotient + remainder), returns the first.
    #[must_use]
    pub fn def(&self) -> Option<X64VReg> {
        match self {
            Self::Mov { dst, .. }
            | Self::MovImm { dst, .. }
            | Self::Add { dst, .. }
            | Self::Sub { dst, .. }
            | Self::IMul { dst, .. }
            | Self::And { dst, .. }
            | Self::Or { dst, .. }
            | Self::Xor { dst, .. }
            | Self::Shl { dst, .. }
            | Self::Shr { dst, .. }
            | Self::Sar { dst, .. }
            | Self::Neg { dst }
            | Self::Not { dst }
            | Self::FpAdd { dst, .. }
            | Self::FpSub { dst, .. }
            | Self::FpMul { dst, .. }
            | Self::FpDiv { dst, .. }
            | Self::FpNeg { dst, .. }
            | Self::Setcc { dst, .. }
            | Self::Movzx { dst, .. }
            | Self::Movsx { dst, .. }
            | Self::Cmov { dst, .. }
            | Self::Select { dst, .. }
            | Self::Load { dst, .. }
            | Self::Lea { dst, .. }
            | Self::Pop { dst } => Some(*dst),
            Self::Cmp { .. }
            | Self::Ucomi { .. }
            | Self::Comi { .. }
            | Self::Test { .. }
            | Self::Store { .. }
            | Self::Push { .. }
            | Self::Cdq
            | Self::Cqo
            | Self::XorRdx { .. } => None,
            Self::Idiv { .. } | Self::Div { .. } => None, // result lives in eax/rax — recovered via Mov post-divide
            Self::Call { results, .. } => results.first().copied(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BlockId, FpCmpKind, IntCmpKind, MemAddr, MemScale, X64Imm, X64Inst, X64SetCondCode, X64Term,
    };
    use crate::vreg::{X64VReg, X64Width};

    // ─── BlockId ───────────────────────────────────────────────────────

    #[test]
    fn block_id_entry_is_zero() {
        assert_eq!(BlockId::ENTRY, BlockId(0));
    }

    #[test]
    fn block_id_display() {
        assert_eq!(BlockId(3).to_string(), "bb3");
    }

    // ─── X64Imm ────────────────────────────────────────────────────────

    #[test]
    fn imm_widths_canonical() {
        assert_eq!(X64Imm::I32(5).width(), X64Width::I32);
        assert_eq!(X64Imm::I64(5).width(), X64Width::I64);
        assert_eq!(X64Imm::F32(0).width(), X64Width::F32);
        assert_eq!(X64Imm::F64(0).width(), X64Width::F64);
        assert_eq!(X64Imm::Bool(true).width(), X64Width::Bool);
    }

    #[test]
    fn imm_display_distinguishes_widths() {
        assert_eq!(X64Imm::I32(42).to_string(), "42i32");
        assert_eq!(X64Imm::I64(42).to_string(), "42i64");
        assert_eq!(X64Imm::Bool(true).to_string(), "true");
    }

    // ─── MemAddr ───────────────────────────────────────────────────────

    #[test]
    fn mem_addr_base_only() {
        let a = MemAddr::base(X64VReg::new(1, X64Width::Ptr));
        assert_eq!(a.to_string(), "[v1:ptr]");
    }

    #[test]
    fn mem_addr_with_index_no_disp() {
        let a = MemAddr::base_plus_index(
            X64VReg::new(1, X64Width::Ptr),
            X64VReg::new(2, X64Width::I64),
        );
        assert_eq!(a.to_string(), "[v1:ptr + v2:i64 * 1]");
    }

    #[test]
    fn mem_addr_with_displacement() {
        let mut a = MemAddr::base(X64VReg::new(1, X64Width::Ptr));
        a.displacement = 16;
        assert_eq!(a.to_string(), "[v1:ptr + 16]");
        a.displacement = -8;
        assert_eq!(a.to_string(), "[v1:ptr - 8]");
    }

    // ─── IntCmpKind ────────────────────────────────────────────────────

    #[test]
    fn int_cmp_x86_suffixes_canonical() {
        assert_eq!(IntCmpKind::Eq.x86_suffix(), "e");
        assert_eq!(IntCmpKind::Ne.x86_suffix(), "ne");
        assert_eq!(IntCmpKind::Slt.x86_suffix(), "l");
        assert_eq!(IntCmpKind::Sge.x86_suffix(), "ge");
        assert_eq!(IntCmpKind::Ult.x86_suffix(), "b");
        assert_eq!(IntCmpKind::Uge.x86_suffix(), "ae");
    }

    // ─── FpCmpKind ─────────────────────────────────────────────────────

    #[test]
    fn fp_cmp_ordered_partition() {
        // o*/ord = ordered ; u*/uno = unordered.
        for k in [
            FpCmpKind::Oeq,
            FpCmpKind::One,
            FpCmpKind::Olt,
            FpCmpKind::Ole,
            FpCmpKind::Ogt,
            FpCmpKind::Oge,
            FpCmpKind::Ord,
        ] {
            assert!(k.is_ordered(), "{k:?} should be ordered");
        }
        for k in [
            FpCmpKind::Une,
            FpCmpKind::Ult,
            FpCmpKind::Ule,
            FpCmpKind::Ugt,
            FpCmpKind::Uge,
            FpCmpKind::Uno,
        ] {
            assert!(!k.is_ordered(), "{k:?} should be unordered");
        }
    }

    #[test]
    fn fp_cmp_predicates_match_mir_strings() {
        // ‼ Names match MIR `arith.cmpf {predicate=...}` attribute strings 1:1.
        assert_eq!(FpCmpKind::Oeq.as_str(), "oeq");
        assert_eq!(FpCmpKind::Ole.as_str(), "ole");
        assert_eq!(FpCmpKind::Une.as_str(), "une");
        assert_eq!(FpCmpKind::Ord.as_str(), "ord");
    }

    // ─── X64Term ───────────────────────────────────────────────────────

    #[test]
    fn term_jmp_display() {
        let t = X64Term::Jmp { target: BlockId(2) };
        assert_eq!(t.to_string(), "jmp bb2");
    }

    #[test]
    fn term_jcc_display() {
        let t = X64Term::Jcc {
            cond_kind: X64SetCondCode::Int(IntCmpKind::Slt),
            cond_vreg: X64VReg::new(3, X64Width::Bool),
            then_block: BlockId(2),
            else_block: BlockId(3),
        };
        let s = t.to_string();
        assert!(s.contains("setl"), "expected setl in {s}");
        assert!(s.contains("bb2"));
        assert!(s.contains("bb3"));
    }

    #[test]
    fn term_ret_display() {
        let t = X64Term::Ret {
            operands: vec![X64VReg::new(7, X64Width::I32)],
        };
        assert_eq!(t.to_string(), "ret v7:i32");
    }

    #[test]
    fn term_ret_void_display() {
        let t = X64Term::Ret { operands: vec![] };
        assert_eq!(t.to_string(), "ret");
    }

    #[test]
    fn term_fallthrough_display() {
        let t = X64Term::Fallthrough { next: BlockId(5) };
        assert_eq!(t.to_string(), "fallthrough bb5");
    }

    // ─── X64Inst.def() ─────────────────────────────────────────────────

    #[test]
    fn inst_def_for_arithmetic_ops() {
        let dst = X64VReg::new(1, X64Width::I32);
        let src = X64VReg::new(2, X64Width::I32);
        assert_eq!(X64Inst::Mov { dst, src }.def(), Some(dst));
        assert_eq!(X64Inst::Add { dst, src }.def(), Some(dst));
        assert_eq!(X64Inst::Sub { dst, src }.def(), Some(dst));
        assert_eq!(X64Inst::IMul { dst, src }.def(), Some(dst));
    }

    #[test]
    fn inst_def_for_void_ops_is_none() {
        let v = X64VReg::new(1, X64Width::I32);
        assert_eq!(X64Inst::Cmp { lhs: v, rhs: v }.def(), None);
        assert_eq!(X64Inst::Cdq.def(), None);
        assert_eq!(X64Inst::Cqo.def(), None);
        assert_eq!(X64Inst::Test { src: v }.def(), None);
    }

    #[test]
    fn inst_def_for_call_uses_first_result() {
        let r0 = X64VReg::new(10, X64Width::I32);
        let r1 = X64VReg::new(11, X64Width::I32);
        let inst = X64Inst::Call {
            callee: "foo".to_string(),
            args: vec![],
            results: vec![r0, r1],
        };
        assert_eq!(inst.def(), Some(r0));
    }

    #[test]
    fn inst_def_for_call_no_result_is_none() {
        let inst = X64Inst::Call {
            callee: "void_fn".to_string(),
            args: vec![],
            results: vec![],
        };
        assert_eq!(inst.def(), None);
    }

    #[test]
    fn mem_scale_values_canonical() {
        assert_eq!(MemScale::One.value(), 1);
        assert_eq!(MemScale::Two.value(), 2);
        assert_eq!(MemScale::Four.value(), 4);
        assert_eq!(MemScale::Eight.value(), 8);
    }
}
