//! § X64VReg — virtual register identifier + width tag.
//!
//! § DESIGN
//!   The selector emits virtual-register-based instructions. Each vreg has :
//!     - 32-bit ID (`u32`)            — monotonic per [`crate::X64Func`].
//!     - [`X64Width`] width tag       — drives register-class selection at G2.
//!
//!   ID 0 is reserved as the null/sentinel. Legitimate vregs start at 1 and
//!   increase monotonically as the selector creates them. The register-
//!   allocator (sibling slice G2) is responsible for mapping each vreg to a
//!   physical register / spill-slot ; G1 just emits virtual-form code.
//!
//! § WIDTH-TAG DISCIPLINE
//!   The width tag is the canonical type information for the vreg. It records
//!   bit-width AND register-class kind (general-purpose vs SSE float vs ptr).
//!   At G1 the selector reads MIR types and emits the corresponding width ;
//!   the encoder (G4) reads the width to pick the correct REX prefix bits +
//!   instruction opcode variant.

use core::fmt;

/// Virtual register width (drives reg-class + encoding).
///
/// § NOTES
///   - `I8` / `I16` / `I32` / `I64` live in general-purpose registers
///     (rax/rbx/rcx/rdx/rsi/rdi/r8..r15 on x86-64).
///   - `F32` / `F64` live in SSE2 xmm registers (xmm0..xmm15).
///   - `Ptr` is a 64-bit GPR but distinguished from `I64` so the encoder /
///     ABI lowering can emit pointer-typed relocations correctly. Stage-0
///     pointer width is 64 bits (host x86-64).
///   - `Bool` is a single-bit logical value but stored in an 8-bit GPR slot
///     per x86-64 boolean ABI (matches cranelift's `i1 → i8` lowering).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum X64Width {
    /// 1-byte general-purpose (i8). Stored in `al`/`bl`/etc. slot.
    I8,
    /// 2-byte general-purpose (i16). Stored in `ax`/`bx`/etc. slot.
    I16,
    /// 4-byte general-purpose (i32). Stored in `eax`/`ebx`/etc. slot.
    I32,
    /// 8-byte general-purpose (i64). Stored in `rax`/`rbx`/etc. slot.
    I64,
    /// 4-byte SSE float (f32). Stored in `xmm{0..15}` low 32 bits.
    F32,
    /// 8-byte SSE float (f64). Stored in `xmm{0..15}` low 64 bits.
    F64,
    /// Boolean — 8-bit GPR slot per x86-64 boolean-ABI convention.
    Bool,
    /// Pointer — 8-bit-aligned 64-bit GPR slot. Distinguished from `I64` so
    /// the encoder emits pointer-typed relocations correctly.
    Ptr,
}

impl X64Width {
    /// Byte-width on x86-64 host.
    #[must_use]
    pub const fn byte_size(self) -> u32 {
        match self {
            Self::I8 | Self::Bool => 1,
            Self::I16 => 2,
            Self::I32 | Self::F32 => 4,
            Self::I64 | Self::F64 | Self::Ptr => 8,
        }
    }

    /// `true` iff this width occupies a general-purpose register (vs SSE).
    #[must_use]
    pub const fn is_gpr(self) -> bool {
        matches!(
            self,
            Self::I8 | Self::I16 | Self::I32 | Self::I64 | Self::Bool | Self::Ptr
        )
    }

    /// `true` iff this width occupies an SSE (xmm) register.
    #[must_use]
    pub const fn is_sse(self) -> bool {
        matches!(self, Self::F32 | Self::F64)
    }

    /// Canonical short-form for diagnostics (`"i32"`, `"f64"`, `"ptr"`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::I8 => "i8",
            Self::I16 => "i16",
            Self::I32 => "i32",
            Self::I64 => "i64",
            Self::F32 => "f32",
            Self::F64 => "f64",
            Self::Bool => "bool",
            Self::Ptr => "ptr",
        }
    }
}

impl fmt::Display for X64Width {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Virtual register identifier. The 32-bit `id` is monotonic per
/// [`crate::X64Func`] ; the [`X64Width`] drives reg-class + encoding.
///
/// § INVARIANT — id 0 is reserved as null/sentinel. Legitimate vregs start at 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct X64VReg {
    /// 32-bit monotonic id within owning [`crate::X64Func`].
    pub id: u32,
    /// Width tag — drives reg-class + encoding decisions.
    pub width: X64Width,
}

impl X64VReg {
    /// New vreg.
    #[must_use]
    pub const fn new(id: u32, width: X64Width) -> Self {
        Self { id, width }
    }

    /// Sentinel "null" vreg : `id = 0`. Never produced by [`crate::select_function`] ;
    /// reserved as the canonical "no register" marker for diagnostics.
    #[must_use]
    pub const fn null() -> Self {
        Self {
            id: 0,
            width: X64Width::I32,
        }
    }

    /// `true` iff this is the sentinel null vreg.
    #[must_use]
    pub const fn is_null(self) -> bool {
        self.id == 0
    }
}

impl fmt::Display for X64VReg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "v{}:{}", self.id, self.width)
    }
}

#[cfg(test)]
mod tests {
    use super::{X64VReg, X64Width};

    #[test]
    fn width_byte_sizes_canonical() {
        assert_eq!(X64Width::I8.byte_size(), 1);
        assert_eq!(X64Width::I16.byte_size(), 2);
        assert_eq!(X64Width::I32.byte_size(), 4);
        assert_eq!(X64Width::I64.byte_size(), 8);
        assert_eq!(X64Width::F32.byte_size(), 4);
        assert_eq!(X64Width::F64.byte_size(), 8);
        assert_eq!(X64Width::Bool.byte_size(), 1);
        assert_eq!(X64Width::Ptr.byte_size(), 8);
    }

    #[test]
    fn width_class_partitions_gpr_vs_sse() {
        assert!(X64Width::I32.is_gpr());
        assert!(X64Width::I64.is_gpr());
        assert!(X64Width::Bool.is_gpr());
        assert!(X64Width::Ptr.is_gpr());
        assert!(!X64Width::F32.is_gpr());
        assert!(!X64Width::F64.is_gpr());

        assert!(X64Width::F32.is_sse());
        assert!(X64Width::F64.is_sse());
        assert!(!X64Width::I32.is_sse());
        assert!(!X64Width::Bool.is_sse());
    }

    #[test]
    fn width_display_canonical() {
        assert_eq!(X64Width::I32.to_string(), "i32");
        assert_eq!(X64Width::F64.to_string(), "f64");
        assert_eq!(X64Width::Ptr.to_string(), "ptr");
    }

    #[test]
    fn vreg_null_is_id_zero() {
        let n = X64VReg::null();
        assert_eq!(n.id, 0);
        assert!(n.is_null());
    }

    #[test]
    fn vreg_legitimate_starts_at_one() {
        // ‼ Convention : selector starts allocating at id 1 ; id 0 is the sentinel.
        let v = X64VReg::new(1, X64Width::I32);
        assert!(!v.is_null());
        assert_eq!(v.id, 1);
        assert_eq!(v.width, X64Width::I32);
    }

    #[test]
    fn vreg_display_form() {
        let v = X64VReg::new(7, X64Width::F32);
        assert_eq!(v.to_string(), "v7:f32");
    }

    #[test]
    fn vreg_equality_distinguishes_id_and_width() {
        // Two vregs with same id but different width are NOT equal — the
        // width tag is part of the identity. (In practice the selector
        // never reuses an id with a different width, but the type forbids
        // it structurally.)
        let a = X64VReg::new(3, X64Width::I32);
        let b = X64VReg::new(3, X64Width::I64);
        let c = X64VReg::new(3, X64Width::I32);
        assert_ne!(a, b);
        assert_eq!(a, c);
    }
}
