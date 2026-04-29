//! Memory-operand model : `[base + index*scale + disp]` and RIP-relative.
//!
//! § SPEC : Intel SDM Vol 2 §2.2 (ModR/M + SIB + displacement).
//!
//! § DESIGN
//!   - [`MemOperand::Base`] : `[base + disp32_or_8]`. Base ∈ Gpr ; rsp/r12 forces SIB ;
//!     rbp/r13 with disp=0 forces mod=01 disp8=0 (avoiding the `[disp32]` / RIP-rel slot).
//!   - [`MemOperand::BaseIndex`] : `[base + index*scale + disp]`. Always uses SIB.
//!     index ≠ rsp (rsp index in SIB means "no index").
//!   - [`MemOperand::IndexOnly`] : `[disp32 + index*scale]` (mod=00 base=101 form).
//!   - [`MemOperand::RipRel`] : `[rip + disp32]` (mod=00 r/m=101 with REX.B=0).
//!
//! § INTEL-SDM-DETAIL
//! ```text
//!   ModR/M.r/m == 100 with mod ∈ {00,01,10} ⇒ SIB byte follows.
//!   ModR/M.r/m == 101 with mod == 00 ⇒ NOT [rbp] but [rip + disp32] (in 64-bit mode).
//!     → [rbp] (zero displacement) MUST be encoded as mod=01 r/m=101 disp8=0.
//!   SIB.base == 101 with mod == 00 ⇒ NO base register, only [disp32 + index*scale].
//!     → [rbp + index*scale] (zero displacement) MUST use mod=01 disp8=0.
//! ```

use crate::encoder::reg::Gpr;

/// SIB scale factor (index multiplier).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Scale {
    /// `*1` (no scaling).
    S1 = 0,
    /// `*2`.
    S2 = 1,
    /// `*4`.
    S4 = 2,
    /// `*8`.
    S8 = 3,
}

impl Scale {
    /// 2-bit field as it appears in SIB bits 7..=6.
    #[must_use]
    pub const fn bits(self) -> u8 {
        self as u8
    }
}

/// A memory addressing operand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemOperand {
    /// `[base + disp]` — base register + signed 32-bit displacement.
    ///
    /// Encoder picks shortest form : disp=0 (mod=00, except rbp/r13) ; |disp| ≤ 127 → disp8 ;
    /// otherwise disp32. rsp/r12 always emit a SIB byte.
    Base { base: Gpr, disp: i32 },
    /// `[base + index*scale + disp]` — base + scaled index + signed 32-bit displacement.
    ///
    /// `index` MUST NOT be `Gpr::Rsp` (SIB.index == 100 = "no index").
    BaseIndex {
        base: Gpr,
        index: Gpr,
        scale: Scale,
        disp: i32,
    },
    /// `[index*scale + disp32]` — no base, always disp32.
    ///
    /// Encoded as mod=00 SIB.base=101 SIB.index=index, with explicit disp32 trailer.
    IndexOnly { index: Gpr, scale: Scale, disp: i32 },
    /// `[rip + disp32]` — rip-relative addressing (mod=00, r/m=101).
    RipRel { disp: i32 },
}

impl MemOperand {
    /// Helper : `[base]` with zero displacement.
    #[must_use]
    pub const fn base(base: Gpr) -> Self {
        Self::Base { base, disp: 0 }
    }

    /// Helper : `[base + disp]`.
    #[must_use]
    pub const fn base_disp(base: Gpr, disp: i32) -> Self {
        Self::Base { base, disp }
    }

    /// Helper : `[rip + disp32]`.
    #[must_use]
    pub const fn rip_rel(disp: i32) -> Self {
        Self::RipRel { disp }
    }
}
