//! ABI tables : register classes + per-ABI argument-passing rules + alignment
//! + shadow-space + caller/callee-saved register sets.
//!
//! § SPEC : `specs/07_CODEGEN.csl § CPU BACKEND § ABI` and
//! `specs/14_BACKEND.csl § OWNED x86-64 BACKEND § ABI`.
//!
//! § AUTHORITATIVE TABLES
//!
//! ```text
//! ┌──────────────────────┬─────────────────────────────┬─────────────────────────────┐
//! │ aspect               │ System V AMD64              │ Microsoft x64 (MS-x64)      │
//! ├──────────────────────┼─────────────────────────────┼─────────────────────────────┤
//! │ int  arg-regs        │ rdi rsi rdx rcx r8 r9       │ rcx rdx r8 r9               │
//! │ float arg-regs       │ xmm0..xmm7                  │ xmm0..xmm3                  │
//! │ alias int↔float pos? │ NO  (independent counters)  │ YES (positional alias)      │
//! │ stack overflow dir   │ right-to-left, pushed       │ right-to-left, stored       │
//! │ return int           │ rax                         │ rax                         │
//! │ return float         │ xmm0                        │ xmm0                        │
//! │ caller-saved (int)   │ rax rcx rdx rsi rdi r8..r11 │ rax rcx rdx r8..r11         │
//! │ caller-saved (xmm)   │ xmm0..xmm15                 │ xmm0..xmm5                  │
//! │ callee-saved (int)   │ rbx rbp r12..r15            │ rbx rbp rdi rsi r12..r15    │
//! │ callee-saved (xmm)   │ NONE                        │ xmm6..xmm15                 │
//! │ shadow space         │ NONE                        │ 32 bytes (caller alloc)     │
//! │ stack align @ call   │ 16 bytes                    │ 16 bytes                    │
//! │ red-zone             │ 128 bytes below rsp         │ NONE                        │
//! │ stack-cleanup        │ caller cleans               │ caller cleans               │
//! │ struct return ≤ 16B  │ rax+rdx (deferred at G3)    │ rax    (deferred at G3)     │
//! │ struct return > 16B  │ hidden ptr arg (deferred)   │ hidden ptr arg (deferred)   │
//! │ variadic             │ al = #vector-regs used      │ first 4 reg + rest stack    │
//! │                      │   (REJECTED at G3)          │   (REJECTED at G3)          │
//! └──────────────────────┴─────────────────────────────┴─────────────────────────────┘
//! ```
//!
//! § REFERENCES
//!   - System V AMD64 ABI v1.0 (<https://gitlab.com/x86-psABIs/x86-64-ABI>)
//!   - Microsoft x64 calling convention
//!     (<https://learn.microsoft.com/en-us/cpp/build/x64-calling-convention>)

use core::fmt;
use thiserror::Error;

// ══════════════════════════════════════════════════════════════════════════
// § X64Abi — top-level discriminant
// ══════════════════════════════════════════════════════════════════════════

/// Calling-convention ABI for the bespoke x86-64 backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum X64Abi {
    /// System V AMD64 — Linux + macOS-Intel + BSD.
    SystemV,
    /// Microsoft x64 — Windows-MSVC + Windows-GNU.
    MicrosoftX64,
}

impl X64Abi {
    /// Short stable name (used in diagnostics + tests + serialized profiles).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SystemV => "sysv",
            Self::MicrosoftX64 => "ms-x64",
        }
    }

    /// Number of integer register-args before stack overflow.
    #[must_use]
    pub const fn int_arg_reg_count(self) -> usize {
        match self {
            Self::SystemV => 6,
            Self::MicrosoftX64 => 4,
        }
    }

    /// Number of float register-args before stack overflow.
    #[must_use]
    pub const fn float_arg_reg_count(self) -> usize {
        match self {
            Self::SystemV => 8,
            Self::MicrosoftX64 => 4,
        }
    }

    /// Whether this ABI's int + float arg counters share a single positional
    /// counter (MS-x64) or are independent (System V).
    ///
    /// MS-x64 example : a 3-arg fn `(i64, f64, i64)` places arg 0 in `rcx`,
    /// arg 1 in `xmm1` (NOT `xmm0` ! — arg 1 means the *second* slot), and
    /// arg 2 in `r8`. The xmm number tracks the positional slot.
    ///
    /// System V example : same `(i64, f64, i64)` places arg 0 in `rdi`,
    /// arg 1 in `xmm0` (independent counter), arg 2 in `rsi`.
    #[must_use]
    pub const fn shares_positional_arg_counter(self) -> bool {
        matches!(self, Self::MicrosoftX64)
    }

    /// Shadow space (in bytes) that the caller MUST allocate before each call.
    /// MS-x64 requires 32 bytes ; System V requires 0.
    #[must_use]
    pub const fn shadow_space_bytes(self) -> u32 {
        match self {
            Self::SystemV => 0,
            Self::MicrosoftX64 => MS_X64_SHADOW_SPACE,
        }
    }

    /// Stack alignment at call boundary (always 16 on x86-64 SysV + MS-x64).
    #[must_use]
    pub const fn call_boundary_alignment(self) -> u32 {
        CALL_BOUNDARY_ALIGNMENT
    }

    /// Whether this ABI permits the System-V red-zone (128 bytes below rsp).
    #[must_use]
    pub const fn allows_red_zone(self) -> bool {
        matches!(self, Self::SystemV)
    }

    /// Integer argument-register table (positional).
    #[must_use]
    pub const fn int_arg_regs(self) -> &'static [GpReg] {
        match self {
            Self::SystemV => SYSV_INT_ARG_REGS,
            Self::MicrosoftX64 => MS_X64_INT_ARG_REGS,
        }
    }

    /// Float argument-register table (positional).
    #[must_use]
    pub const fn float_arg_regs(self) -> &'static [XmmReg] {
        match self {
            Self::SystemV => SYSV_FLOAT_ARG_REGS,
            Self::MicrosoftX64 => MS_X64_FLOAT_ARG_REGS,
        }
    }

    /// Caller-saved integer registers.
    #[must_use]
    pub const fn caller_saved_gp(self) -> &'static [GpReg] {
        match self {
            Self::SystemV => SYSV_CALLER_SAVED_GP,
            Self::MicrosoftX64 => MS_X64_CALLER_SAVED_GP,
        }
    }

    /// Caller-saved XMM registers.
    #[must_use]
    pub const fn caller_saved_xmm(self) -> &'static [XmmReg] {
        match self {
            Self::SystemV => SYSV_CALLER_SAVED_XMM,
            Self::MicrosoftX64 => MS_X64_CALLER_SAVED_XMM,
        }
    }

    /// Callee-saved integer registers.
    #[must_use]
    pub const fn callee_saved_gp(self) -> &'static [GpReg] {
        match self {
            Self::SystemV => SYSV_CALLEE_SAVED_GP,
            Self::MicrosoftX64 => MS_X64_CALLEE_SAVED_GP,
        }
    }

    /// Callee-saved XMM registers.
    #[must_use]
    pub const fn callee_saved_xmm(self) -> &'static [XmmReg] {
        match self {
            Self::SystemV => SYSV_CALLEE_SAVED_XMM,
            Self::MicrosoftX64 => MS_X64_CALLEE_SAVED_XMM,
        }
    }

    /// Resolved at compile-time : the ABI for the host platform (Windows → MS-x64,
    /// otherwise System V).
    #[must_use]
    pub const fn host_default() -> Self {
        if cfg!(target_os = "windows") {
            Self::MicrosoftX64
        } else {
            Self::SystemV
        }
    }
}

impl fmt::Display for X64Abi {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Stack alignment at call boundary (16 bytes for both x86-64 ABIs).
pub const CALL_BOUNDARY_ALIGNMENT: u32 = 16;

/// MS-x64 shadow-space size (32 bytes : 4 register args × 8).
pub const MS_X64_SHADOW_SPACE: u32 = 32;

// ══════════════════════════════════════════════════════════════════════════
// § GpReg — general-purpose 64-bit register encoding
// ══════════════════════════════════════════════════════════════════════════

/// 64-bit general-purpose registers used by the x86-64 ABIs.
///
/// Encoded as the canonical Intel encoding (0..15) for forward-compat with
/// the eventual `emit.rs` ModR/M byte builder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
#[allow(clippy::upper_case_acronyms)]
pub enum GpReg {
    /// `rax` — accumulator. Encoded as 0. Return-value reg (int).
    Rax = 0,
    /// `rcx` — counter. Encoded as 1. MS-x64 arg-0 ; SysV arg-3.
    Rcx = 1,
    /// `rdx` — data. Encoded as 2. MS-x64 arg-1 ; SysV arg-2 ; sysv-struct-return-high.
    Rdx = 2,
    /// `rbx` — base. Encoded as 3. Callee-saved on both ABIs.
    Rbx = 3,
    /// `rsp` — stack pointer. Encoded as 4. NEVER an arg / return reg.
    Rsp = 4,
    /// `rbp` — base pointer / frame pointer. Encoded as 5. Callee-saved on both ABIs.
    Rbp = 5,
    /// `rsi` — source index. Encoded as 6. SysV arg-1 ; MS-x64 callee-saved.
    Rsi = 6,
    /// `rdi` — destination index. Encoded as 7. SysV arg-0 ; MS-x64 callee-saved.
    Rdi = 7,
    /// `r8` — encoded as 8. SysV arg-4 ; MS-x64 arg-2.
    R8 = 8,
    /// `r9` — encoded as 9. SysV arg-5 ; MS-x64 arg-3.
    R9 = 9,
    /// `r10` — encoded as 10. Caller-saved on both ABIs (often used as a static-chain).
    R10 = 10,
    /// `r11` — encoded as 11. Caller-saved on both ABIs (scratch).
    R11 = 11,
    /// `r12` — encoded as 12. Callee-saved on both ABIs.
    R12 = 12,
    /// `r13` — encoded as 13. Callee-saved on both ABIs.
    R13 = 13,
    /// `r14` — encoded as 14. Callee-saved on both ABIs.
    R14 = 14,
    /// `r15` — encoded as 15. Callee-saved on both ABIs.
    R15 = 15,
}

impl GpReg {
    /// Canonical lowercase Intel mnemonic.
    #[must_use]
    pub const fn name(self) -> &'static str {
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

    /// Numeric Intel encoding 0..=15 (REX.B / ModR/M-rm extension applies for 8..=15).
    #[must_use]
    pub const fn encoding(self) -> u8 {
        self as u8
    }
}

impl fmt::Display for GpReg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

// ══════════════════════════════════════════════════════════════════════════
// § XmmReg — 128-bit float / SIMD register encoding
// ══════════════════════════════════════════════════════════════════════════

/// 128-bit XMM registers (used for f32/f64 + SIMD).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
#[allow(clippy::upper_case_acronyms)]
pub enum XmmReg {
    /// `xmm0` — return-value reg (float).
    Xmm0 = 0,
    /// `xmm1`.
    Xmm1 = 1,
    /// `xmm2`.
    Xmm2 = 2,
    /// `xmm3` — last MS-x64 float arg-reg.
    Xmm3 = 3,
    /// `xmm4`.
    Xmm4 = 4,
    /// `xmm5` — last MS-x64 caller-saved xmm.
    Xmm5 = 5,
    /// `xmm6` — first MS-x64 callee-saved xmm.
    Xmm6 = 6,
    /// `xmm7` — last SysV float arg-reg.
    Xmm7 = 7,
    /// `xmm8`.
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

impl XmmReg {
    /// Canonical lowercase mnemonic (`xmm0`..`xmm15`).
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

    /// Numeric encoding 0..=15 (REX.B / VEX.vvvv extension applies for 8..=15).
    #[must_use]
    pub const fn encoding(self) -> u8 {
        self as u8
    }
}

impl fmt::Display for XmmReg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

// ══════════════════════════════════════════════════════════════════════════
// § ArgClass — abstract classification of a single argument
// ══════════════════════════════════════════════════════════════════════════

/// Abstract classification of an argument value as it flows through the ABI
/// lowering layer. The G3 surface handles scalar Int + Float ; aggregate
/// classes (Struct ≤ 16 bytes via SysV's classification algorithm, Vector for
/// SIMD-by-value) are stub-rejected with [`AbiError::StructReturnNotSupported`]
/// or kept structurally for the typechecker to refuse upstream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ArgClass {
    /// 8 / 16 / 32 / 64-bit signed or unsigned integer (or pointer).
    Int,
    /// 32 or 64-bit IEEE float (`f32` or `f64`).
    Float,
}

impl ArgClass {
    /// Short stable name (used in diagnostics + tests).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Int => "int",
            Self::Float => "float",
        }
    }
}

impl fmt::Display for ArgClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// ══════════════════════════════════════════════════════════════════════════
// § IntArgRegs / FloatArgRegs — per-ABI argument-register-list newtypes
// ══════════════════════════════════════════════════════════════════════════

/// Newtype wrapping the integer-argument-register list (ABI-dependent length).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IntArgRegs(pub &'static [GpReg]);

impl IntArgRegs {
    /// Number of register slots before stack overflow.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.0.len()
    }

    /// True when no register slots exist (never true for x86-64 ABIs).
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Get the i-th register slot (None when out-of-range → arg goes to stack).
    #[must_use]
    pub fn get(&self, idx: usize) -> Option<GpReg> {
        self.0.get(idx).copied()
    }
}

/// Newtype wrapping the float-argument-register list (ABI-dependent length).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FloatArgRegs(pub &'static [XmmReg]);

impl FloatArgRegs {
    /// Number of register slots before stack overflow.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.0.len()
    }

    /// True when no register slots exist (never true for x86-64 ABIs).
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Get the i-th register slot (None when out-of-range → arg goes to stack).
    #[must_use]
    pub fn get(&self, idx: usize) -> Option<XmmReg> {
        self.0.get(idx).copied()
    }
}

// ══════════════════════════════════════════════════════════════════════════
// § ReturnReg — return-value register classification
// ══════════════════════════════════════════════════════════════════════════

/// Where a single return value lives at function exit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReturnReg {
    /// Integer / pointer return : always `rax` on both ABIs (G3 scalar-only).
    Int(GpReg),
    /// Float return : always `xmm0` on both ABIs.
    Float(XmmReg),
    /// Void return — no register written.
    Void,
}

impl ReturnReg {
    /// Resolve the return register for a single scalar [`ArgClass`] under [`X64Abi`].
    ///
    /// Both ABIs use `rax` for int + `xmm0` for float ; this helper exists for
    /// future-proofing (when DarwinAmd64 or a non-MSVC variant deviates).
    #[must_use]
    pub const fn for_class(_abi: X64Abi, class: ArgClass) -> Self {
        match class {
            ArgClass::Int => Self::Int(GpReg::Rax),
            ArgClass::Float => Self::Float(XmmReg::Xmm0),
        }
    }
}

// ══════════════════════════════════════════════════════════════════════════
// § AbiError — diagnostic error variants raised by the lowering layer
// ══════════════════════════════════════════════════════════════════════════

/// Errors raised by the ABI-lowering helpers when the input is malformed or
/// hits a deferred ABI feature (variadic, large-struct return, etc).
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AbiError {
    /// Variadic functions are deferred at G3.
    /// SysV requires `al = #vector-regs-used` — not yet implemented.
    /// MS-x64 requires float args also dual-allocated to int regs.
    #[error("variadic calls not supported at G3 (S7-G4 deferred) ; ABI = {abi}, arity = {arity}")]
    VariadicNotSupported {
        /// ABI under which the call was attempted.
        abi: X64Abi,
        /// Number of args at the variadic call site (informational).
        arity: usize,
    },
    /// Large-struct return via hidden first-arg pointer is deferred at G3 ;
    /// scalar-return only is supported.
    #[error("struct return not supported at G3 (scalar-return only) ; ABI = {abi}, size = {size_bytes} bytes")]
    StructReturnNotSupported {
        /// ABI under which the return was attempted.
        abi: X64Abi,
        /// Reported struct-return size (informational).
        size_bytes: u32,
    },
    /// Stack alignment invariant violated by the caller (must be 16-aligned at
    /// the call instruction).
    #[error("stack alignment violation : rsp delta {rsp_delta} bytes is not 16-aligned at call boundary (ABI = {abi})")]
    StackAlignmentViolation {
        /// ABI under which the call was attempted.
        abi: X64Abi,
        /// Computed rsp delta that failed the alignment check.
        rsp_delta: i64,
    },
}

// ══════════════════════════════════════════════════════════════════════════
// § Internal tables — register lists per ABI
// ══════════════════════════════════════════════════════════════════════════

/// System V AMD64 — integer argument registers (positional).
const SYSV_INT_ARG_REGS: &[GpReg] = &[
    GpReg::Rdi,
    GpReg::Rsi,
    GpReg::Rdx,
    GpReg::Rcx,
    GpReg::R8,
    GpReg::R9,
];

/// System V AMD64 — float argument registers (positional).
const SYSV_FLOAT_ARG_REGS: &[XmmReg] = &[
    XmmReg::Xmm0,
    XmmReg::Xmm1,
    XmmReg::Xmm2,
    XmmReg::Xmm3,
    XmmReg::Xmm4,
    XmmReg::Xmm5,
    XmmReg::Xmm6,
    XmmReg::Xmm7,
];

/// System V AMD64 — caller-saved integer registers.
const SYSV_CALLER_SAVED_GP: &[GpReg] = &[
    GpReg::Rax,
    GpReg::Rcx,
    GpReg::Rdx,
    GpReg::Rsi,
    GpReg::Rdi,
    GpReg::R8,
    GpReg::R9,
    GpReg::R10,
    GpReg::R11,
];

/// System V AMD64 — caller-saved XMM registers (all 16).
const SYSV_CALLER_SAVED_XMM: &[XmmReg] = &[
    XmmReg::Xmm0,
    XmmReg::Xmm1,
    XmmReg::Xmm2,
    XmmReg::Xmm3,
    XmmReg::Xmm4,
    XmmReg::Xmm5,
    XmmReg::Xmm6,
    XmmReg::Xmm7,
    XmmReg::Xmm8,
    XmmReg::Xmm9,
    XmmReg::Xmm10,
    XmmReg::Xmm11,
    XmmReg::Xmm12,
    XmmReg::Xmm13,
    XmmReg::Xmm14,
    XmmReg::Xmm15,
];

/// System V AMD64 — callee-saved integer registers.
const SYSV_CALLEE_SAVED_GP: &[GpReg] = &[
    GpReg::Rbx,
    GpReg::Rbp,
    GpReg::R12,
    GpReg::R13,
    GpReg::R14,
    GpReg::R15,
];

/// System V AMD64 — callee-saved XMM registers (NONE on SysV).
const SYSV_CALLEE_SAVED_XMM: &[XmmReg] = &[];

/// MS-x64 — integer argument registers (positional).
const MS_X64_INT_ARG_REGS: &[GpReg] = &[GpReg::Rcx, GpReg::Rdx, GpReg::R8, GpReg::R9];

/// MS-x64 — float argument registers (positional ; aliases int regs).
const MS_X64_FLOAT_ARG_REGS: &[XmmReg] = &[XmmReg::Xmm0, XmmReg::Xmm1, XmmReg::Xmm2, XmmReg::Xmm3];

/// MS-x64 — caller-saved integer registers.
const MS_X64_CALLER_SAVED_GP: &[GpReg] = &[
    GpReg::Rax,
    GpReg::Rcx,
    GpReg::Rdx,
    GpReg::R8,
    GpReg::R9,
    GpReg::R10,
    GpReg::R11,
];

/// MS-x64 — caller-saved XMM registers.
const MS_X64_CALLER_SAVED_XMM: &[XmmReg] = &[
    XmmReg::Xmm0,
    XmmReg::Xmm1,
    XmmReg::Xmm2,
    XmmReg::Xmm3,
    XmmReg::Xmm4,
    XmmReg::Xmm5,
];

/// MS-x64 — callee-saved integer registers (note rdi + rsi are callee-saved here !).
const MS_X64_CALLEE_SAVED_GP: &[GpReg] = &[
    GpReg::Rbx,
    GpReg::Rbp,
    GpReg::Rdi,
    GpReg::Rsi,
    GpReg::R12,
    GpReg::R13,
    GpReg::R14,
    GpReg::R15,
];

/// MS-x64 — callee-saved XMM registers.
const MS_X64_CALLEE_SAVED_XMM: &[XmmReg] = &[
    XmmReg::Xmm6,
    XmmReg::Xmm7,
    XmmReg::Xmm8,
    XmmReg::Xmm9,
    XmmReg::Xmm10,
    XmmReg::Xmm11,
    XmmReg::Xmm12,
    XmmReg::Xmm13,
    XmmReg::Xmm14,
    XmmReg::Xmm15,
];

#[cfg(test)]
mod tests {
    use super::*;

    // ── X64Abi basics ──

    #[test]
    fn abi_short_names() {
        assert_eq!(X64Abi::SystemV.as_str(), "sysv");
        assert_eq!(X64Abi::MicrosoftX64.as_str(), "ms-x64");
    }

    #[test]
    fn abi_display() {
        assert_eq!(format!("{}", X64Abi::SystemV), "sysv");
        assert_eq!(format!("{}", X64Abi::MicrosoftX64), "ms-x64");
    }

    #[test]
    fn host_default_resolves_per_target() {
        let host = X64Abi::host_default();
        if cfg!(target_os = "windows") {
            assert_eq!(host, X64Abi::MicrosoftX64);
        } else {
            assert_eq!(host, X64Abi::SystemV);
        }
    }

    // ── int / float arg-reg-count counts ──

    #[test]
    fn sysv_has_six_int_arg_regs() {
        assert_eq!(X64Abi::SystemV.int_arg_reg_count(), 6);
        assert_eq!(X64Abi::SystemV.int_arg_regs().len(), 6);
    }

    #[test]
    fn ms_x64_has_four_int_arg_regs() {
        assert_eq!(X64Abi::MicrosoftX64.int_arg_reg_count(), 4);
        assert_eq!(X64Abi::MicrosoftX64.int_arg_regs().len(), 4);
    }

    #[test]
    fn sysv_has_eight_float_arg_regs() {
        assert_eq!(X64Abi::SystemV.float_arg_reg_count(), 8);
        assert_eq!(X64Abi::SystemV.float_arg_regs().len(), 8);
    }

    #[test]
    fn ms_x64_has_four_float_arg_regs() {
        assert_eq!(X64Abi::MicrosoftX64.float_arg_reg_count(), 4);
        assert_eq!(X64Abi::MicrosoftX64.float_arg_regs().len(), 4);
    }

    // ── arg-reg-table positional ordering (the load-bearing canonical order) ──

    #[test]
    fn sysv_int_arg_regs_canonical_order() {
        let regs = X64Abi::SystemV.int_arg_regs();
        assert_eq!(regs[0], GpReg::Rdi);
        assert_eq!(regs[1], GpReg::Rsi);
        assert_eq!(regs[2], GpReg::Rdx);
        assert_eq!(regs[3], GpReg::Rcx);
        assert_eq!(regs[4], GpReg::R8);
        assert_eq!(regs[5], GpReg::R9);
    }

    #[test]
    fn ms_x64_int_arg_regs_canonical_order() {
        let regs = X64Abi::MicrosoftX64.int_arg_regs();
        assert_eq!(regs[0], GpReg::Rcx);
        assert_eq!(regs[1], GpReg::Rdx);
        assert_eq!(regs[2], GpReg::R8);
        assert_eq!(regs[3], GpReg::R9);
    }

    #[test]
    fn sysv_float_arg_regs_xmm0_through_xmm7() {
        let regs = X64Abi::SystemV.float_arg_regs();
        for (i, &xmm) in regs.iter().enumerate() {
            assert_eq!(xmm.encoding() as usize, i);
        }
    }

    #[test]
    fn ms_x64_float_arg_regs_xmm0_through_xmm3() {
        let regs = X64Abi::MicrosoftX64.float_arg_regs();
        assert_eq!(regs.len(), 4);
        for (i, &xmm) in regs.iter().enumerate() {
            assert_eq!(xmm.encoding() as usize, i);
        }
    }

    // ── shadow space + alignment + red-zone ──

    #[test]
    fn ms_x64_requires_thirty_two_byte_shadow_space() {
        assert_eq!(X64Abi::MicrosoftX64.shadow_space_bytes(), 32);
        assert_eq!(MS_X64_SHADOW_SPACE, 32);
    }

    #[test]
    fn sysv_has_zero_shadow_space() {
        assert_eq!(X64Abi::SystemV.shadow_space_bytes(), 0);
    }

    #[test]
    fn both_abis_require_sixteen_byte_call_alignment() {
        assert_eq!(X64Abi::SystemV.call_boundary_alignment(), 16);
        assert_eq!(X64Abi::MicrosoftX64.call_boundary_alignment(), 16);
        assert_eq!(CALL_BOUNDARY_ALIGNMENT, 16);
    }

    #[test]
    fn only_sysv_allows_red_zone() {
        assert!(X64Abi::SystemV.allows_red_zone());
        assert!(!X64Abi::MicrosoftX64.allows_red_zone());
    }

    // ── shares-positional-counter rule (the MS-x64 alias landmine) ──

    #[test]
    fn ms_x64_shares_int_float_positional_counter() {
        assert!(X64Abi::MicrosoftX64.shares_positional_arg_counter());
    }

    #[test]
    fn sysv_int_float_counters_are_independent() {
        assert!(!X64Abi::SystemV.shares_positional_arg_counter());
    }

    // ── caller / callee-saved sets — disjoint within an ABI ──

    #[test]
    fn sysv_caller_and_callee_saved_gp_disjoint() {
        for r in X64Abi::SystemV.caller_saved_gp() {
            assert!(!X64Abi::SystemV.callee_saved_gp().contains(r));
        }
    }

    #[test]
    fn ms_x64_caller_and_callee_saved_gp_disjoint() {
        for r in X64Abi::MicrosoftX64.caller_saved_gp() {
            assert!(!X64Abi::MicrosoftX64.callee_saved_gp().contains(r));
        }
    }

    #[test]
    fn ms_x64_callee_saved_includes_rdi_and_rsi() {
        // The MS-x64 specific landmine — these are caller-saved on SysV but
        // CALLEE-saved on MS-x64.
        let cs = X64Abi::MicrosoftX64.callee_saved_gp();
        assert!(cs.contains(&GpReg::Rdi));
        assert!(cs.contains(&GpReg::Rsi));
    }

    #[test]
    fn sysv_callee_saved_excludes_rdi_and_rsi() {
        let cs = X64Abi::SystemV.callee_saved_gp();
        assert!(!cs.contains(&GpReg::Rdi));
        assert!(!cs.contains(&GpReg::Rsi));
    }

    #[test]
    fn ms_x64_callee_saved_xmm_six_through_fifteen() {
        let cs = X64Abi::MicrosoftX64.callee_saved_xmm();
        assert_eq!(cs.len(), 10);
        assert_eq!(cs[0], XmmReg::Xmm6);
        assert_eq!(cs[9], XmmReg::Xmm15);
    }

    #[test]
    fn sysv_has_no_callee_saved_xmm() {
        assert!(X64Abi::SystemV.callee_saved_xmm().is_empty());
    }

    // ── return reg ──

    #[test]
    fn return_reg_int_is_rax_on_both_abis() {
        for &abi in &[X64Abi::SystemV, X64Abi::MicrosoftX64] {
            match ReturnReg::for_class(abi, ArgClass::Int) {
                ReturnReg::Int(r) => assert_eq!(r, GpReg::Rax),
                other => panic!("expected Int(Rax), got {other:?}"),
            }
        }
    }

    #[test]
    fn return_reg_float_is_xmm0_on_both_abis() {
        for &abi in &[X64Abi::SystemV, X64Abi::MicrosoftX64] {
            match ReturnReg::for_class(abi, ArgClass::Float) {
                ReturnReg::Float(r) => assert_eq!(r, XmmReg::Xmm0),
                other => panic!("expected Float(Xmm0), got {other:?}"),
            }
        }
    }

    // ── GpReg / XmmReg encodings ──

    #[test]
    fn gp_reg_canonical_intel_encoding() {
        assert_eq!(GpReg::Rax.encoding(), 0);
        assert_eq!(GpReg::Rcx.encoding(), 1);
        assert_eq!(GpReg::Rdx.encoding(), 2);
        assert_eq!(GpReg::Rbx.encoding(), 3);
        assert_eq!(GpReg::Rsp.encoding(), 4);
        assert_eq!(GpReg::Rbp.encoding(), 5);
        assert_eq!(GpReg::Rsi.encoding(), 6);
        assert_eq!(GpReg::Rdi.encoding(), 7);
        assert_eq!(GpReg::R8.encoding(), 8);
        assert_eq!(GpReg::R15.encoding(), 15);
    }

    #[test]
    fn gp_reg_names_match_intel_mnemonics() {
        assert_eq!(GpReg::Rax.name(), "rax");
        assert_eq!(GpReg::R15.name(), "r15");
        assert_eq!(format!("{}", GpReg::Rdi), "rdi");
    }

    #[test]
    fn xmm_reg_encoding_zero_through_fifteen() {
        assert_eq!(XmmReg::Xmm0.encoding(), 0);
        assert_eq!(XmmReg::Xmm15.encoding(), 15);
    }

    #[test]
    fn xmm_reg_names_match_canonical_form() {
        assert_eq!(XmmReg::Xmm0.name(), "xmm0");
        assert_eq!(XmmReg::Xmm7.name(), "xmm7");
        assert_eq!(XmmReg::Xmm15.name(), "xmm15");
        assert_eq!(format!("{}", XmmReg::Xmm5), "xmm5");
    }

    // ── ArgClass + IntArgRegs / FloatArgRegs newtypes ──

    #[test]
    fn arg_class_short_names() {
        assert_eq!(ArgClass::Int.as_str(), "int");
        assert_eq!(ArgClass::Float.as_str(), "float");
    }

    #[test]
    fn int_arg_regs_get_in_range() {
        let r = IntArgRegs(X64Abi::SystemV.int_arg_regs());
        assert_eq!(r.get(0), Some(GpReg::Rdi));
        assert_eq!(r.get(5), Some(GpReg::R9));
        assert_eq!(r.get(6), None); // overflow goes to stack
        assert!(!r.is_empty());
        assert_eq!(r.len(), 6);
    }

    #[test]
    fn float_arg_regs_get_out_of_range_returns_none() {
        let r = FloatArgRegs(X64Abi::MicrosoftX64.float_arg_regs());
        assert_eq!(r.get(3), Some(XmmReg::Xmm3));
        assert_eq!(r.get(4), None);
        assert!(!r.is_empty());
        assert_eq!(r.len(), 4);
    }

    // ── AbiError ──

    #[test]
    fn abi_error_variadic_displays() {
        let e = AbiError::VariadicNotSupported {
            abi: X64Abi::MicrosoftX64,
            arity: 5,
        };
        let s = format!("{e}");
        assert!(s.contains("variadic"));
        assert!(s.contains("ms-x64"));
        assert!(s.contains("arity = 5"));
    }

    #[test]
    fn abi_error_struct_return_displays() {
        let e = AbiError::StructReturnNotSupported {
            abi: X64Abi::SystemV,
            size_bytes: 24,
        };
        let s = format!("{e}");
        assert!(s.contains("struct return"));
        assert!(s.contains("scalar-return only"));
    }

    #[test]
    fn abi_error_alignment_displays() {
        let e = AbiError::StackAlignmentViolation {
            abi: X64Abi::MicrosoftX64,
            rsp_delta: 24,
        };
        let s = format!("{e}");
        assert!(s.contains("alignment violation"));
        assert!(s.contains("16-aligned"));
    }
}
