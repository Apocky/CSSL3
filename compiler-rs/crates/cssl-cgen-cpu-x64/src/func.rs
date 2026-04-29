//! § func — `X64Func` boundary type between G1..G4 and G5.
//!
//! Defines the data the object-file emitter consumes. Every field here is
//! shaped so that the encoder (G1), regalloc (G2), isel (G3), and frame-
//! layout (G4) phases — none of which have landed yet on this branch — can
//! populate it incrementally without breaking G5's surface.
//!
//! § INVARIANTS
//!   - `name` is a NUL-free ASCII identifier (the linker rejects non-ASCII).
//!   - `bytes` is the fully-encoded function body (prologue + body + epilogue).
//!   - Each `relocs[i].offset` is a byte index into `bytes` where the relocation
//!     site begins (the start of the immediate field, not the start of the
//!     instruction).
//!   - `relocs[i].target_index` is an index into the per-module symbol table
//!     (built from the funcs slice + any `extern_imports`).
//!   - `is_export` controls symbol visibility: `true` ⇒ STB_GLOBAL on ELF /
//!     EXTERNAL on COFF / N_EXT on Mach-O ; `false` ⇒ STB_LOCAL / STATIC / 0.

use thiserror::Error;

// ───────────────────────────────────────────────────────────────────────
// § X64Func — one encoded function ready to be packed into an object file.
// ───────────────────────────────────────────────────────────────────────

/// One function's encoded machine-code + symbol metadata + relocations.
///
/// Produced by the encoder (G1) — extended in-place by G2..G4. The object
/// emitter (G5) takes a slice of these and serialises them into a `.o` /
/// `.obj` byte-stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct X64Func {
    /// Linker-visible name. ASCII identifier, NUL-free.
    pub name: String,
    /// Encoded x86-64 machine-code bytes. Length is the function's size.
    pub bytes: Vec<u8>,
    /// Relocation sites within `bytes`.
    pub relocs: Vec<X64Reloc>,
    /// True ⇒ the symbol is global (visible to the linker for cross-TU calls).
    /// False ⇒ local / module-private.
    pub is_export: bool,
}

impl X64Func {
    /// Construct a new function record. `name` must be non-empty.
    ///
    /// # Errors
    /// Returns [`X64FuncError::EmptyName`] if `name` is empty,
    /// [`X64FuncError::NonAsciiName`] if it contains non-ASCII or NUL bytes.
    pub fn new(
        name: impl Into<String>,
        bytes: Vec<u8>,
        relocs: Vec<X64Reloc>,
        is_export: bool,
    ) -> Result<Self, X64FuncError> {
        let name = name.into();
        if name.is_empty() {
            return Err(X64FuncError::EmptyName);
        }
        if !name.is_ascii() || name.bytes().any(|b| b == 0) {
            return Err(X64FuncError::NonAsciiName { name });
        }
        Ok(Self {
            name,
            bytes,
            relocs,
            is_export,
        })
    }

    /// Convenience : construct a leaf function (no relocations, no calls)
    /// that simply executes `bytes` and returns. Useful for the trivial
    /// `fn main() -> i32 { 42 }` end-to-end smoke (the encoded body is
    /// `B8 2A 00 00 00 C3` — `mov eax, 42 ; ret`).
    pub fn leaf_export(name: impl Into<String>, bytes: Vec<u8>) -> Result<Self, X64FuncError> {
        Self::new(name, bytes, Vec::new(), true)
    }
}

// ───────────────────────────────────────────────────────────────────────
// § X64Reloc — one relocation site in an X64Func body.
// ───────────────────────────────────────────────────────────────────────

/// A relocation site within an `X64Func`'s `bytes`.
///
/// `offset` is the byte index of the immediate field that needs to be
/// patched at link time. `target_index` indexes into the module-level
/// symbol table that the object emitter constructs from the funcs slice
/// plus any `extern_imports` passed alongside.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct X64Reloc {
    /// Byte offset within the parent `X64Func.bytes` where the relocation
    /// site begins.
    pub offset: u32,
    /// Index into the module-level symbol table (funcs + imports).
    pub target_index: u32,
    /// Relocation kind — selects the per-format encoding.
    pub kind: X64RelocKind,
    /// Addend applied to the symbol value (typically `-4` for near-call /
    /// PC-relative-32 to account for the PC at end-of-instruction).
    pub addend: i32,
}

/// Relocation kind, format-agnostic.
///
/// § PER-FORMAT MAPPING (per spec § OBJECT-FILE-WRITING)
///   | kind        | ELF                    | COFF                       | Mach-O                     |
///   | ----------- | ---------------------- | -------------------------- | -------------------------- |
///   | NearCall    | R_X86_64_PLT32 = 4     | IMAGE_REL_AMD64_REL32 = 4  | X86_64_RELOC_BRANCH = 2    |
///   | PcRel32     | R_X86_64_PC32  = 2     | IMAGE_REL_AMD64_REL32 = 4  | X86_64_RELOC_SIGNED = 1    |
///   | Abs64       | R_X86_64_64    = 1     | IMAGE_REL_AMD64_ADDR64 = 1 | X86_64_RELOC_UNSIGNED = 0  |
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum X64RelocKind {
    /// 32-bit near-call site (`E8 xx xx xx xx`). PIE-friendly via PLT
    /// indirection on ELF.
    NearCall,
    /// 32-bit PC-relative (e.g. `LEA rax, [rip+disp32]` data load).
    PcRel32,
    /// 64-bit absolute address (e.g. function-pointer table).
    Abs64,
}

impl X64RelocKind {
    /// ELF `r_type` field for this kind.
    #[must_use]
    pub const fn elf_r_type(self) -> u32 {
        match self {
            Self::NearCall => 4, // R_X86_64_PLT32
            Self::PcRel32 => 2,  // R_X86_64_PC32
            Self::Abs64 => 1,    // R_X86_64_64
        }
    }

    /// COFF `Type` field (IMAGE_REL_AMD64_*).
    #[must_use]
    pub const fn coff_type(self) -> u16 {
        match self {
            // Near-call and PC-rel both use REL32 on COFF (the linker
            // distinguishes them via the symbol type).
            Self::NearCall | Self::PcRel32 => 0x0004, // IMAGE_REL_AMD64_REL32
            Self::Abs64 => 0x0001,                    // IMAGE_REL_AMD64_ADDR64
        }
    }

    /// Mach-O `r_type` field (X86_64_RELOC_*).
    #[must_use]
    pub const fn macho_r_type(self) -> u8 {
        match self {
            Self::NearCall => 2, // X86_64_RELOC_BRANCH
            Self::PcRel32 => 1,  // X86_64_RELOC_SIGNED
            Self::Abs64 => 0,    // X86_64_RELOC_UNSIGNED
        }
    }

    /// True if this relocation is PC-relative (affects how the linker
    /// computes the patched value + sets the `r_pcrel` bit on Mach-O).
    #[must_use]
    pub const fn is_pc_relative(self) -> bool {
        matches!(self, Self::NearCall | Self::PcRel32)
    }
}

// ───────────────────────────────────────────────────────────────────────
// § X64Symbol — extern imports referenced by relocs.
// ───────────────────────────────────────────────────────────────────────

/// An external symbol the module's relocations refer to.
///
/// Typically these are cssl-rt entry points (`__cssl_alloc`, `__cssl_free`,
/// `__cssl_panic`, …). Listed once per module ; relocations refer by
/// index into the combined funcs+imports symbol table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct X64Symbol {
    /// Linker-visible name.
    pub name: String,
    /// What sort of external this is — affects whether the linker resolves
    /// it via the PLT, the GOT, or as a direct reference.
    pub kind: X64SymbolKind,
}

/// Kind of external symbol (informational ; affects relocation type
/// selection by the encoder, not by G5 itself).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum X64SymbolKind {
    /// External function (resolved at link time ; called via PLT on PIE).
    Function,
    /// External data symbol (e.g. a global variable).
    Data,
}

impl X64Symbol {
    /// Construct a new external function symbol.
    ///
    /// # Errors
    /// Returns [`X64FuncError`] if the name is empty or non-ASCII.
    pub fn new_function(name: impl Into<String>) -> Result<Self, X64FuncError> {
        let name = name.into();
        if name.is_empty() {
            return Err(X64FuncError::EmptyName);
        }
        if !name.is_ascii() || name.bytes().any(|b| b == 0) {
            return Err(X64FuncError::NonAsciiName { name });
        }
        Ok(Self {
            name,
            kind: X64SymbolKind::Function,
        })
    }

    /// Construct a new external data symbol.
    ///
    /// # Errors
    /// Returns [`X64FuncError`] if the name is empty or non-ASCII.
    pub fn new_data(name: impl Into<String>) -> Result<Self, X64FuncError> {
        let name = name.into();
        if name.is_empty() {
            return Err(X64FuncError::EmptyName);
        }
        if !name.is_ascii() || name.bytes().any(|b| b == 0) {
            return Err(X64FuncError::NonAsciiName { name });
        }
        Ok(Self {
            name,
            kind: X64SymbolKind::Data,
        })
    }
}

// ───────────────────────────────────────────────────────────────────────
// § X64FuncError — boundary-construction failure modes.
// ───────────────────────────────────────────────────────────────────────

#[derive(Debug, Error, PartialEq, Eq)]
pub enum X64FuncError {
    #[error("function/symbol name is empty")]
    EmptyName,
    #[error("function/symbol name `{name}` contains non-ASCII or NUL bytes")]
    NonAsciiName { name: String },
}

// ───────────────────────────────────────────────────────────────────────
// § tests
// ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::{X64Func, X64FuncError, X64Reloc, X64RelocKind, X64Symbol, X64SymbolKind};

    /// `mov eax, 42 ; ret` — the canonical end-to-end smoke body.
    const MAIN_42_BYTES: &[u8] = &[0xB8, 0x2A, 0x00, 0x00, 0x00, 0xC3];

    #[test]
    fn leaf_export_constructs() {
        let f = X64Func::leaf_export("main", MAIN_42_BYTES.to_vec()).unwrap();
        assert_eq!(f.name, "main");
        assert_eq!(f.bytes, MAIN_42_BYTES);
        assert!(f.relocs.is_empty());
        assert!(f.is_export);
    }

    #[test]
    fn empty_name_rejected() {
        let err = X64Func::new("", vec![0xC3], vec![], true).unwrap_err();
        assert_eq!(err, X64FuncError::EmptyName);
    }

    #[test]
    fn non_ascii_name_rejected() {
        let err = X64Func::new("föö", vec![0xC3], vec![], true).unwrap_err();
        assert!(matches!(err, X64FuncError::NonAsciiName { .. }));
    }

    #[test]
    fn nul_in_name_rejected() {
        let err = X64Func::new("foo\0bar", vec![0xC3], vec![], true).unwrap_err();
        assert!(matches!(err, X64FuncError::NonAsciiName { .. }));
    }

    #[test]
    fn reloc_kind_elf_mapping() {
        assert_eq!(X64RelocKind::NearCall.elf_r_type(), 4); // R_X86_64_PLT32
        assert_eq!(X64RelocKind::PcRel32.elf_r_type(), 2); // R_X86_64_PC32
        assert_eq!(X64RelocKind::Abs64.elf_r_type(), 1); // R_X86_64_64
    }

    #[test]
    fn reloc_kind_coff_mapping() {
        assert_eq!(X64RelocKind::NearCall.coff_type(), 0x0004); // REL32
        assert_eq!(X64RelocKind::PcRel32.coff_type(), 0x0004); // REL32
        assert_eq!(X64RelocKind::Abs64.coff_type(), 0x0001); // ADDR64
    }

    #[test]
    fn reloc_kind_macho_mapping() {
        assert_eq!(X64RelocKind::NearCall.macho_r_type(), 2); // BRANCH
        assert_eq!(X64RelocKind::PcRel32.macho_r_type(), 1); // SIGNED
        assert_eq!(X64RelocKind::Abs64.macho_r_type(), 0); // UNSIGNED
    }

    #[test]
    fn reloc_kind_pc_relative_predicate() {
        assert!(X64RelocKind::NearCall.is_pc_relative());
        assert!(X64RelocKind::PcRel32.is_pc_relative());
        assert!(!X64RelocKind::Abs64.is_pc_relative());
    }

    #[test]
    fn reloc_struct_round_trips() {
        let r = X64Reloc {
            offset: 1,
            target_index: 0,
            kind: X64RelocKind::NearCall,
            addend: -4,
        };
        let r2 = r;
        assert_eq!(r, r2);
    }

    #[test]
    fn symbol_function_constructs() {
        let s = X64Symbol::new_function("__cssl_alloc").unwrap();
        assert_eq!(s.name, "__cssl_alloc");
        assert_eq!(s.kind, X64SymbolKind::Function);
    }

    #[test]
    fn symbol_data_constructs() {
        let s = X64Symbol::new_data("__cssl_panic_msg").unwrap();
        assert_eq!(s.kind, X64SymbolKind::Data);
    }

    #[test]
    fn symbol_empty_name_rejected() {
        let err = X64Symbol::new_function("").unwrap_err();
        assert_eq!(err, X64FuncError::EmptyName);
    }
}
