//! Physical + virtual register definitions for the x86-64 backend.
//!
//! § SPEC : `specs/07_CODEGEN.csl` § CPU BACKEND § ABI line + `specs/14_BACKEND.csl`.
//!
//! § REGISTER FILE
//!   * 16 general-purpose 64-bit pregs : rax / rbx / rcx / rdx / rsi / rdi /
//!     rbp / rsp / r8..r15. (rsp + rbp are reserved at S7-G2 — see
//!     [`X64PReg::is_reserved_for_user_alloc`] + crate-root `RESERVED REGISTER
//!     DISCIPLINE`.)
//!   * 16 SSE 128-bit pregs : xmm0..xmm15.
//!   * Encoding-numbers match the canonical x86-64 ModR/M encoding (rax = 0,
//!     r15 = 15, xmm0 = 0, xmm15 = 15).
//!
//! § VIRTUAL REGISTERS
//!   `X64VReg` is the input form to the allocator : an opaque numeric handle
//!   with an associated [`RegBank`] (GP for integers / pointers, XMM for
//!   floats / SIMD). Mapping vreg → preg is the allocator's job.
//!
//! § ABI METADATA
//!   Per-preg ABI flags (caller-saved / callee-saved) drive prologue/epilogue
//!   push/pop pair emission. The tables here cover the three stage-0 ABIs :
//!   SysV-AMD64 (Linux + BSD), Windows-x64 (Microsoft MS-x64), and
//!   Darwin-AMD64 (macOS-Intel ; SysV-equivalent for our purposes).

use core::fmt;

/// Calling-convention ABI. Mirrors the `Abi` enum in `cssl-cgen-cpu-cranelift::abi`
/// — re-declared here so this crate doesn't take a path-dep on the cranelift crate
/// (the two backends are siblings, not parent/child).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Abi {
    /// System V AMD64 (Linux / BSD).
    SysVAmd64,
    /// Microsoft x64 (Windows).
    WindowsX64,
    /// Darwin-AMD64 (macOS-Intel ; uses SysV with Apple-extensions).
    DarwinAmd64,
}

impl fmt::Display for Abi {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::SysVAmd64 => "sysv",
            Self::WindowsX64 => "win64",
            Self::DarwinAmd64 => "darwin",
        })
    }
}

/// Register-bank — picks the encoding family for a vreg.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RegBank {
    /// General-purpose 8/16/32/64-bit integer + pointer registers.
    Gp,
    /// SSE / AVX 128-bit (xmm) — float + SIMD scalars.
    Xmm,
}

impl fmt::Display for RegBank {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Gp => "gp",
            Self::Xmm => "xmm",
        })
    }
}

/// Per-preg role under a specific ABI. Drives prologue/epilogue codegen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RegRole {
    /// Caller-saved : the called fn may clobber freely ; the caller is responsible
    /// for saving across calls. Allocator preference for short-lived intervals.
    CallerSaved,
    /// Callee-saved : the called fn must preserve the value across its body.
    /// If the allocator uses one, the prologue emits a `push` and the epilogue
    /// a matching `pop` to restore.
    CalleeSaved,
    /// Reserved : never allocated to user vregs (rsp always ; rbp at S7-G2).
    Reserved,
}

impl fmt::Display for RegRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::CallerSaved => "caller-saved",
            Self::CalleeSaved => "callee-saved",
            Self::Reserved => "reserved",
        })
    }
}

/// The 16 + 16 physical registers visible to the x86-64 backend. The repr-u8
/// encoding numbers match the canonical ModR/M encoding for each register.
#[allow(clippy::upper_case_acronyms)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum X64PReg {
    // ─── General-purpose (16 × 64-bit) ───
    Rax = 0,
    Rcx = 1,
    Rdx = 2,
    Rbx = 3,
    /// Stack pointer — RESERVED across all ABIs.
    Rsp = 4,
    /// Frame pointer — RESERVED at S7-G2 (omit-frame-pointer is a future slice).
    Rbp = 5,
    Rsi = 6,
    Rdi = 7,
    R8 = 8,
    R9 = 9,
    R10 = 10,
    R11 = 11,
    R12 = 12,
    R13 = 13,
    R14 = 14,
    R15 = 15,

    // ─── SSE / AVX (16 × 128-bit) ───
    Xmm0 = 16,
    Xmm1 = 17,
    Xmm2 = 18,
    Xmm3 = 19,
    Xmm4 = 20,
    Xmm5 = 21,
    Xmm6 = 22,
    Xmm7 = 23,
    Xmm8 = 24,
    Xmm9 = 25,
    Xmm10 = 26,
    Xmm11 = 27,
    Xmm12 = 28,
    Xmm13 = 29,
    Xmm14 = 30,
    Xmm15 = 31,
}

impl X64PReg {
    /// Total count of physical regs the backend tracks (32).
    pub const COUNT: usize = 32;

    /// All 32 pregs in canonical order.
    pub const ALL: [Self; Self::COUNT] = [
        Self::Rax,
        Self::Rcx,
        Self::Rdx,
        Self::Rbx,
        Self::Rsp,
        Self::Rbp,
        Self::Rsi,
        Self::Rdi,
        Self::R8,
        Self::R9,
        Self::R10,
        Self::R11,
        Self::R12,
        Self::R13,
        Self::R14,
        Self::R15,
        Self::Xmm0,
        Self::Xmm1,
        Self::Xmm2,
        Self::Xmm3,
        Self::Xmm4,
        Self::Xmm5,
        Self::Xmm6,
        Self::Xmm7,
        Self::Xmm8,
        Self::Xmm9,
        Self::Xmm10,
        Self::Xmm11,
        Self::Xmm12,
        Self::Xmm13,
        Self::Xmm14,
        Self::Xmm15,
    ];

    /// Encoding number (0..=15 within bank).
    #[must_use]
    pub const fn encoding(self) -> u8 {
        match self.bank() {
            RegBank::Gp => self as u8,
            RegBank::Xmm => self as u8 - 16,
        }
    }

    /// The register-bank.
    #[must_use]
    pub const fn bank(self) -> RegBank {
        if (self as u8) < 16 {
            RegBank::Gp
        } else {
            RegBank::Xmm
        }
    }

    /// Short canonical name (e.g. "rax", "xmm0").
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

    /// Returns `true` iff this preg is reserved from user-vreg allocation at S7-G2.
    /// Currently rsp + rbp are reserved across all ABIs.
    #[must_use]
    pub const fn is_reserved_for_user_alloc(self) -> bool {
        matches!(self, Self::Rsp | Self::Rbp)
    }

    /// Per-ABI caller-saved / callee-saved / reserved role.
    ///
    /// References : SysV-AMD64 ABI specification §3.2.3 ; Microsoft x64 calling
    /// convention (learn.microsoft.com) ; Apple AMD64 follows SysV.
    #[must_use]
    pub fn role_under_abi(self, abi: Abi) -> RegRole {
        // Reserved are reserved everywhere.
        if self.is_reserved_for_user_alloc() {
            return RegRole::Reserved;
        }
        match abi {
            // SysV-AMD64 (Linux/BSD/Mac-Intel) :
            //   callee-saved GP : rbx, rbp, r12, r13, r14, r15
            //     (rbp is structurally Reserved above ; rest below)
            //   callee-saved XMM : NONE (all xmm0..xmm15 are caller-saved)
            //   caller-saved GP  : rax, rcx, rdx, rsi, rdi, r8, r9, r10, r11
            Abi::SysVAmd64 | Abi::DarwinAmd64 => match self {
                Self::Rbx | Self::R12 | Self::R13 | Self::R14 | Self::R15 => RegRole::CalleeSaved,
                _ => RegRole::CallerSaved,
            },
            // Microsoft x64 (Windows) :
            //   callee-saved GP  : rbx, rbp, rdi, rsi, r12, r13, r14, r15
            //     (rbp Reserved above)
            //   callee-saved XMM : xmm6..xmm15
            //   caller-saved GP  : rax, rcx, rdx, r8, r9, r10, r11
            //   caller-saved XMM : xmm0..xmm5
            Abi::WindowsX64 => match self {
                Self::Rbx
                | Self::Rsi
                | Self::Rdi
                | Self::R12
                | Self::R13
                | Self::R14
                | Self::R15 => RegRole::CalleeSaved,
                Self::Xmm6
                | Self::Xmm7
                | Self::Xmm8
                | Self::Xmm9
                | Self::Xmm10
                | Self::Xmm11
                | Self::Xmm12
                | Self::Xmm13
                | Self::Xmm14
                | Self::Xmm15 => RegRole::CalleeSaved,
                _ => RegRole::CallerSaved,
            },
        }
    }
}

impl fmt::Display for X64PReg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// A virtual register — input to the allocator. Opaque numeric handle plus a
/// register-bank tag so the allocator routes integer vregs to GP pregs and
/// float vregs to XMM pregs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct X64VReg {
    /// Numeric vreg index. Allocator-internal — assigned by the upstream
    /// virtual-instruction emitter (S7-G1).
    pub index: u32,
    /// Which preg-bank this vreg can be assigned to.
    pub bank: RegBank,
}

impl X64VReg {
    /// Construct a GP virtual register.
    #[must_use]
    pub const fn gp(index: u32) -> Self {
        Self {
            index,
            bank: RegBank::Gp,
        }
    }

    /// Construct an XMM virtual register.
    #[must_use]
    pub const fn xmm(index: u32) -> Self {
        Self {
            index,
            bank: RegBank::Xmm,
        }
    }
}

impl fmt::Display for X64VReg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "v{}.{}", self.index, self.bank)
    }
}
