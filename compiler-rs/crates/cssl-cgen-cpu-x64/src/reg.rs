//! x86-64 physical register encoding.
//!
//! § SPEC : Intel SDM Vol 2 — register encoding tables.
//!
//! § DESIGN
//!   - 16 general-purpose 64-bit regs : rax / rcx / rdx / rbx / rsp / rbp / rsi / rdi / r8..r15.
//!   - 16 SSE/SSE2 XMM regs : xmm0..xmm15.
//!   - 8-bit subregister discipline : low-byte rax/rcx/rdx/rbx are al/cl/dl/bl (legacy, no REX).
//!     Access to spl/bpl/sil/dil OR r8b..r15b requires REX prefix presence (even REX=0x40).
//!   - The encoder reads `Gpr` as a 4-bit field {0..15} and splits into the 3-bit ModR/M nibble
//!     plus the 1-bit REX-extension (B/X/R) when needed.
//!
//! § X64Inst-INDEPENDENCE
//!   This file does not depend on the (potentially-unlanded) sibling X64Inst surface.
//!   When G1 lands, callers may map their own register types into [`Gpr`] / [`Xmm`] via
//!   the public constructors / `from_index`.

use core::fmt;

/// 64-bit general-purpose register.
///
/// Encoding follows Intel SDM Vol 2 Table 3-1 (the "Reg" 4-bit field).
/// Bit-3 (REX.B / REX.R / REX.X depending on position) is set when the index ≥ 8.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Gpr {
    /// `rax` — accumulator (return-value-low SysV-AMD64).
    Rax = 0,
    /// `rcx` — counter (4th arg SysV-AMD64).
    Rcx = 1,
    /// `rdx` — data (3rd arg SysV-AMD64, return-value-high).
    Rdx = 2,
    /// `rbx` — base (callee-saved).
    Rbx = 3,
    /// `rsp` — stack pointer.
    Rsp = 4,
    /// `rbp` — base pointer (callee-saved).
    Rbp = 5,
    /// `rsi` — source index (2nd arg SysV-AMD64).
    Rsi = 6,
    /// `rdi` — destination index (1st arg SysV-AMD64).
    Rdi = 7,
    /// `r8` — extended (5th arg SysV-AMD64).
    R8 = 8,
    /// `r9` — extended (6th arg SysV-AMD64).
    R9 = 9,
    /// `r10` — extended (caller-saved).
    R10 = 10,
    /// `r11` — extended (caller-saved).
    R11 = 11,
    /// `r12` — extended (callee-saved).
    R12 = 12,
    /// `r13` — extended (callee-saved).
    R13 = 13,
    /// `r14` — extended (callee-saved).
    R14 = 14,
    /// `r15` — extended (callee-saved).
    R15 = 15,
}

impl Gpr {
    /// Construct from raw 4-bit index 0..=15.
    ///
    /// # Panics
    /// Panics if `idx > 15`.
    #[must_use]
    pub const fn from_index(idx: u8) -> Self {
        match idx {
            0 => Self::Rax,
            1 => Self::Rcx,
            2 => Self::Rdx,
            3 => Self::Rbx,
            4 => Self::Rsp,
            5 => Self::Rbp,
            6 => Self::Rsi,
            7 => Self::Rdi,
            8 => Self::R8,
            9 => Self::R9,
            10 => Self::R10,
            11 => Self::R11,
            12 => Self::R12,
            13 => Self::R13,
            14 => Self::R14,
            15 => Self::R15,
            _ => panic!("Gpr::from_index : idx > 15"),
        }
    }

    /// Raw 4-bit register index.
    #[must_use]
    pub const fn index(self) -> u8 {
        self as u8
    }

    /// Low 3 bits — the field that lands in ModR/M.reg or ModR/M.r/m or SIB.base/index.
    #[must_use]
    pub const fn rm_bits(self) -> u8 {
        (self as u8) & 0b0000_0111
    }

    /// Bit 3 — the REX-extension bit (B / X / R depending on operand position).
    #[must_use]
    pub const fn rex_bit(self) -> bool {
        ((self as u8) & 0b0000_1000) != 0
    }

    /// Whether this register requires an explicit SIB byte when used as memory-base
    /// (any ModR/M.r/m == 100 forces SIB).
    #[must_use]
    pub const fn forces_sib_as_base(self) -> bool {
        // r/m == 100 → rsp / r12 (after REX.B masking)
        matches!(self, Self::Rsp | Self::R12)
    }

    /// Whether this register's r/m field collides with the `[disp32]` / RIP-relative slot
    /// at mod==00 (r/m == 101 → rbp / r13). For `[reg]` addressing of rbp/r13 we MUST use
    /// mod==01 with disp8=0 instead.
    #[must_use]
    pub const fn collides_with_riprel(self) -> bool {
        matches!(self, Self::Rbp | Self::R13)
    }

    /// 64-bit canonical name string.
    #[must_use]
    pub const fn name64(self) -> &'static str {
        match self {
            Self::Rax => "rax",
            Self::Rcx => "rcx",
            Self::Rdx => "rdx",
            Self::Rbx => "rbx",
            Self::Rsp => "rsp",
            Self::Rbp => "rbp",
            Self::Rsi => "rsi",
            Self::Rdi => "rdi",
            Self::R8 => "r8",
            Self::R9 => "r9",
            Self::R10 => "r10",
            Self::R11 => "r11",
            Self::R12 => "r12",
            Self::R13 => "r13",
            Self::R14 => "r14",
            Self::R15 => "r15",
        }
    }
}

impl fmt::Display for Gpr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name64())
    }
}

/// 128-bit XMM SSE/SSE2 register.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Xmm {
    /// `xmm0` (1st FP arg SysV-AMD64).
    Xmm0 = 0,
    /// `xmm1`.
    Xmm1 = 1,
    /// `xmm2`.
    Xmm2 = 2,
    /// `xmm3`.
    Xmm3 = 3,
    /// `xmm4`.
    Xmm4 = 4,
    /// `xmm5`.
    Xmm5 = 5,
    /// `xmm6`.
    Xmm6 = 6,
    /// `xmm7`.
    Xmm7 = 7,
    /// `xmm8` — REX.R/B required.
    Xmm8 = 8,
    /// `xmm9`.
    Xmm9 = 9,
    /// `xmm10`.
    Xmm10 = 10,
    /// `xmm11`.
    Xmm11 = 11,
    /// `xmm12`.
    Xmm12 = 12,
    /// `xmm13`.
    Xmm13 = 13,
    /// `xmm14`.
    Xmm14 = 14,
    /// `xmm15`.
    Xmm15 = 15,
}

impl Xmm {
    /// Construct from raw 4-bit index 0..=15.
    ///
    /// # Panics
    /// Panics if `idx > 15`.
    #[must_use]
    pub const fn from_index(idx: u8) -> Self {
        match idx {
            0 => Self::Xmm0,
            1 => Self::Xmm1,
            2 => Self::Xmm2,
            3 => Self::Xmm3,
            4 => Self::Xmm4,
            5 => Self::Xmm5,
            6 => Self::Xmm6,
            7 => Self::Xmm7,
            8 => Self::Xmm8,
            9 => Self::Xmm9,
            10 => Self::Xmm10,
            11 => Self::Xmm11,
            12 => Self::Xmm12,
            13 => Self::Xmm13,
            14 => Self::Xmm14,
            15 => Self::Xmm15,
            _ => panic!("Xmm::from_index : idx > 15"),
        }
    }

    /// Raw 4-bit register index.
    #[must_use]
    pub const fn index(self) -> u8 {
        self as u8
    }

    /// Low 3 bits — ModR/M.reg or ModR/M.r/m position field.
    #[must_use]
    pub const fn rm_bits(self) -> u8 {
        (self as u8) & 0b0000_0111
    }

    /// Bit 3 — REX-extension bit (R / B / X depending on operand position).
    #[must_use]
    pub const fn rex_bit(self) -> bool {
        ((self as u8) & 0b0000_1000) != 0
    }

    /// Canonical name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Xmm0 => "xmm0",
            Self::Xmm1 => "xmm1",
            Self::Xmm2 => "xmm2",
            Self::Xmm3 => "xmm3",
            Self::Xmm4 => "xmm4",
            Self::Xmm5 => "xmm5",
            Self::Xmm6 => "xmm6",
            Self::Xmm7 => "xmm7",
            Self::Xmm8 => "xmm8",
            Self::Xmm9 => "xmm9",
            Self::Xmm10 => "xmm10",
            Self::Xmm11 => "xmm11",
            Self::Xmm12 => "xmm12",
            Self::Xmm13 => "xmm13",
            Self::Xmm14 => "xmm14",
            Self::Xmm15 => "xmm15",
        }
    }
}

impl fmt::Display for Xmm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Operand size for GPR-direct ops : 8 / 16 / 32 / 64-bit.
///
/// Drives REX.W (B64) and the 0x66 operand-size override prefix (B16) and 8-bit
/// opcode-form selection (B8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OperandSize {
    /// 8-bit (al/cl/dl/bl/spl/bpl/sil/dil/r8b..r15b).
    B8,
    /// 16-bit (ax/cx/.../r15w) — uses 0x66 prefix.
    B16,
    /// 32-bit (eax/ecx/.../r15d) — REX.W=0.
    B32,
    /// 64-bit (rax/rcx/.../r15) — REX.W=1.
    B64,
}

impl OperandSize {
    /// Whether REX.W must be set for this operand size.
    #[must_use]
    pub const fn rex_w(self) -> bool {
        matches!(self, Self::B64)
    }

    /// Whether the 0x66 operand-size override prefix must precede the opcode.
    #[must_use]
    pub const fn needs_op_size_prefix(self) -> bool {
        matches!(self, Self::B16)
    }
}
