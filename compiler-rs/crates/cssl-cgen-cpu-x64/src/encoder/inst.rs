//! `X64Inst` — canonical post-regalloc instruction surface.
//!
//! § SPEC : `specs/14_BACKEND.csl` § OWNED x86-64 BACKEND phase 4 (emit).
//!
//! § DESIGN
//!   This is the surface G1 builds (instruction-selection) + G2 (regalloc) + G3 (ABI-lower)
//!   feed into G4 (this crate) for byte-emission. Until siblings land, the surface is
//!   defined here in a forward-compatible shape : every variant carries the operands the
//!   encoder needs (sources fully-physical, sizes explicit, branch-targets either known-
//!   relative or symbolic).
//!
//! § COVERAGE matches the slice plan :
//!   integer  : Mov / Add / Sub / Mul / IMul / IDiv / Cmp / Lea / Push / Pop / Load / Store
//!   control  : Jmp / Jcc / Call / Ret
//!   SSE2     : Movss / Movsd / Addss / Addsd / Subss / Subsd / Mulss / Mulsd / Divss / Divsd /
//!              `UComiss` / `UComisd` / `Comiss` / `Comisd` / `Sqrtss` / `Sqrtsd` /
//!              `Cvtsi2ss` / `Cvtsi2sd` / `Cvtss2si` / `Cvtsd2si` / `Xorps` /
//!              `MovssMem` (load/store) / `MovsdMem` (load/store) /
//!              `MovqXmmFromGp` / `MovqGpFromXmm` (G11 / T11-D102 SSE2 float path)
//!
//! § INVARIANTS
//!   - Source/dest are physical regs (post-regalloc) ; G4 does not allocate.
//!   - `OperandSize` carries the width — emitter dispatches on it for opcode/prefix selection.
//!   - Branches emit a placeholder + caller-supplied target ; the linker / G5 step patches
//!     into final relative form. For local-relative emission (testing) the relative offset
//!     is provided directly.

use crate::encoder::mem::MemOperand;
use crate::encoder::reg::{Gpr, OperandSize, Xmm};

/// Conditional-branch condition code (low-nibble of `Jcc` opcode).
///
/// Matches Intel SDM Vol 2 §B.1 (condition-code Cc field).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Cond {
    /// Equal / zero.
    E = 0x4,
    /// Not equal / not zero.
    Ne = 0x5,
    /// Below (unsigned <).
    B = 0x2,
    /// Above-or-equal (unsigned ≥).
    Ae = 0x3,
    /// Below-or-equal (unsigned ≤).
    Be = 0x6,
    /// Above (unsigned >).
    A = 0x7,
    /// Less (signed <).
    L = 0xC,
    /// Greater-or-equal (signed ≥).
    Ge = 0xD,
    /// Less-or-equal (signed ≤).
    Le = 0xE,
    /// Greater (signed >).
    G = 0xF,
    /// Sign (signed-bit set).
    S = 0x8,
    /// Not-sign.
    Ns = 0x9,
    /// Parity-even.
    P = 0xA,
    /// Parity-odd.
    Np = 0xB,
    /// Overflow.
    O = 0x0,
    /// No-overflow.
    No = 0x1,
}

impl Cond {
    /// 4-bit condition code field.
    #[must_use]
    pub const fn code(self) -> u8 {
        self as u8
    }
}

/// Branch target — either a known relative offset (for testing / local jumps), or a
/// symbol reference to be resolved at link/relocation time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BranchTarget {
    /// Known signed relative offset measured FROM end-of-instruction.
    /// 8-bit form is selected when |offset| ≤ 127 ; otherwise 32-bit.
    Rel(i32),
    /// Force long (32-bit) form even if offset fits 8-bit (used for forward-refs).
    Rel32(i32),
}

/// Canonical post-regalloc x86-64 instruction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum X64Inst {
    // ─── integer moves ───────────────────────────────────────────────
    /// `mov dst, src` — register-to-register move.
    MovRR {
        size: OperandSize,
        dst: Gpr,
        src: Gpr,
    },
    /// `mov dst, imm` — immediate-to-register.
    /// 8/16/32-bit forms : C7 /0 ; 64-bit form : B8+rd 64-bit imm (movabs).
    MovRI {
        size: OperandSize,
        dst: Gpr,
        imm: i64,
    },
    /// `mov dst, [mem]` — load. Width drives prefix/opcode.
    Load {
        size: OperandSize,
        dst: Gpr,
        src: MemOperand,
    },
    /// `mov [mem], src` — store.
    Store {
        size: OperandSize,
        dst: MemOperand,
        src: Gpr,
    },
    /// `lea dst, [mem]` — load-effective-address. Always 64-bit.
    Lea {
        size: OperandSize,
        dst: Gpr,
        src: MemOperand,
    },
    // ─── integer arithmetic ──────────────────────────────────────────
    /// `add dst, src` — register/register.
    AddRR {
        size: OperandSize,
        dst: Gpr,
        src: Gpr,
    },
    /// `add dst, imm32` (sign-extended for 64-bit).
    AddRI {
        size: OperandSize,
        dst: Gpr,
        imm: i32,
    },
    /// `sub dst, src`.
    SubRR {
        size: OperandSize,
        dst: Gpr,
        src: Gpr,
    },
    /// `sub dst, imm32`.
    SubRI {
        size: OperandSize,
        dst: Gpr,
        imm: i32,
    },
    /// `mul src` — unsigned multiply ; result in rdx:rax.
    Mul { size: OperandSize, src: Gpr },
    /// `imul src` — signed multiply (one-operand) ; result in rdx:rax.
    ImulR { size: OperandSize, src: Gpr },
    /// `imul dst, src` — signed multiply (two-operand, dst = dst * src).
    ImulRR {
        size: OperandSize,
        dst: Gpr,
        src: Gpr,
    },
    /// `idiv src` — signed divide rdx:rax by src ; quotient → rax, remainder → rdx.
    IDiv { size: OperandSize, src: Gpr },
    /// `cmp dst, src` — compare (sets flags).
    CmpRR {
        size: OperandSize,
        dst: Gpr,
        src: Gpr,
    },
    /// `cmp dst, imm32`.
    CmpRI {
        size: OperandSize,
        dst: Gpr,
        imm: i32,
    },
    // ─── stack / control ─────────────────────────────────────────────
    /// `push reg` — 64-bit-default push (no REX.W needed on 64-bit mode).
    Push { src: Gpr },
    /// `pop reg`.
    Pop { dst: Gpr },
    /// `ret` — near return.
    Ret,
    /// `call rel32` — near-call relative.
    CallRel { target: BranchTarget },
    /// `jmp rel32 / rel8` — unconditional near jump.
    Jmp { target: BranchTarget },
    /// `jcc rel32 / rel8` — conditional near jump.
    Jcc { cond: Cond, target: BranchTarget },
    // ─── SSE2 scalar FP ──────────────────────────────────────────────
    /// `movss xmm, xmm` — scalar single move (xmm-xmm form, F3 0F 10).
    MovssRR { dst: Xmm, src: Xmm },
    /// `movsd xmm, xmm` — scalar double move (F2 0F 10).
    MovsdRR { dst: Xmm, src: Xmm },
    /// `addss xmm, xmm` (F3 0F 58).
    AddssRR { dst: Xmm, src: Xmm },
    /// `addsd xmm, xmm` (F2 0F 58).
    AddsdRR { dst: Xmm, src: Xmm },
    /// `subss xmm, xmm` (F3 0F 5C).
    SubssRR { dst: Xmm, src: Xmm },
    /// `subsd xmm, xmm` (F2 0F 5C).
    SubsdRR { dst: Xmm, src: Xmm },
    /// `mulss xmm, xmm` (F3 0F 59).
    MulssRR { dst: Xmm, src: Xmm },
    /// `mulsd xmm, xmm` (F2 0F 59).
    MulsdRR { dst: Xmm, src: Xmm },
    /// `divss xmm, xmm` (F3 0F 5E).
    DivssRR { dst: Xmm, src: Xmm },
    /// `divsd xmm, xmm` (F2 0F 5E).
    DivsdRR { dst: Xmm, src: Xmm },
    /// `ucomisd xmm, xmm` — unordered compare scalar double (66 0F 2E).
    UComisdRR { dst: Xmm, src: Xmm },
    /// `cvtsi2sd xmm, gpr` — convert signed-int to scalar-double.
    /// 32-bit src : F2 0F 2A /r ; 64-bit src : F2 REX.W 0F 2A /r.
    CvtSi2sdRR {
        size: OperandSize,
        dst: Xmm,
        src: Gpr,
    },
    /// `cvtsd2si gpr, xmm` — convert scalar-double to signed-int.
    /// 32-bit dst : F2 0F 2D /r ; 64-bit dst : F2 REX.W 0F 2D /r.
    CvtSd2siRR {
        size: OperandSize,
        dst: Gpr,
        src: Xmm,
    },
    // ─── SSE2 G11 (T11-D102) extension : single-precision compares /
    //    sqrt / single-precision conversions / xorps for FpNeg /
    //    movss/movsd memory forms / movd/movq XMM↔GPR transfer ──────────
    /// `ucomiss xmm, xmm` — unordered ordered-compare scalar single (0F 2E /r).
    UComissRR { dst: Xmm, src: Xmm },
    /// `comiss xmm, xmm` — signaling ordered-compare scalar single (0F 2F /r).
    /// Sets ZF/PF/CF (signaling on QNaN).
    ComissRR { dst: Xmm, src: Xmm },
    /// `comisd xmm, xmm` — signaling ordered-compare scalar double (66 0F 2F /r).
    ComisdRR { dst: Xmm, src: Xmm },
    /// `sqrtss xmm, xmm` (F3 0F 51 /r) — scalar-single square root.
    SqrtssRR { dst: Xmm, src: Xmm },
    /// `sqrtsd xmm, xmm` (F2 0F 51 /r) — scalar-double square root.
    SqrtsdRR { dst: Xmm, src: Xmm },
    /// `cvtsi2ss xmm, gpr` — convert signed-int to scalar-single.
    /// 32-bit src : F3 0F 2A /r ; 64-bit src : F3 REX.W 0F 2A /r.
    CvtSi2ssRR {
        size: OperandSize,
        dst: Xmm,
        src: Gpr,
    },
    /// `cvtss2si gpr, xmm` — convert scalar-single to signed-int.
    /// 32-bit dst : F3 0F 2D /r ; 64-bit dst : F3 REX.W 0F 2D /r.
    CvtSs2siRR {
        size: OperandSize,
        dst: Gpr,
        src: Xmm,
    },
    /// `xorps xmm, xmm` — packed-single bitwise-XOR (0F 57 /r).
    ///
    /// Used by G11 (T11-D102) for f32 sign-bit flip (FpNeg) — the opcode is
    /// shared between f32 and f64 sign-flip ; emitter picks the same opcode
    /// either way.
    ///
    /// § INVARIANT  (T11-D112 / S7-G10) : `xorps r, r` is also the canonical
    /// idiom for materializing the IEEE 754 zero bit-pattern in an XMM
    /// register WITHOUT a rip-relative load from a constant pool. This
    /// is the only float-imm path G10 supports ; non-zero float
    /// constants require a rodata section + rip-relative load that is
    /// reserved for a follow-up slice.
    XorpsRR { dst: Xmm, src: Xmm },
    /// `xorpd xmm, xmm` (66 0F 57 /r) — bitwise XOR for f64 sign-bit flip.
    XorpdRR { dst: Xmm, src: Xmm },
    /// `movss xmm, [mem]` (F3 0F 10 /r) — load scalar-single from memory.
    MovssLoad { dst: Xmm, src: MemOperand },
    /// `movss [mem], xmm` (F3 0F 11 /r) — store scalar-single to memory.
    MovssStore { dst: MemOperand, src: Xmm },
    /// `movsd xmm, [mem]` (F2 0F 10 /r) — load scalar-double from memory.
    MovsdLoad { dst: Xmm, src: MemOperand },
    /// `movsd [mem], xmm` (F2 0F 11 /r) — store scalar-double to memory.
    MovsdStore { dst: MemOperand, src: Xmm },
    /// `movd xmm, gpr32` (66 0F 6E /r) — bit-pattern transfer 32-bit GPR → XMM.
    /// Used to materialize an f32 constant via the integer-pattern path.
    MovdXmmFromGp { dst: Xmm, src: Gpr },
    /// `movd gpr32, xmm` (66 0F 7E /r) — bit-pattern transfer XMM → 32-bit GPR.
    MovdGpFromXmm { dst: Gpr, src: Xmm },
    /// `movq xmm, gpr64` (66 REX.W 0F 6E /r) — bit-pattern transfer 64-bit
    /// GPR → XMM. Used to materialize an f64 constant via the integer-
    /// pattern path : `mov rax, <f64 bits as i64> ; movq xmm0, rax`.
    MovqXmmFromGp { dst: Xmm, src: Gpr },
    /// `movq gpr64, xmm` (66 REX.W 0F 7E /r) — bit-pattern transfer XMM →
    /// 64-bit GPR.
    MovqGpFromXmm { dst: Gpr, src: Xmm },
}
